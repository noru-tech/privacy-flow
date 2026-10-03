//! The pipeline: enumerate, read, lower, assemble, build facts, run an engine, report.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use rayon::prelude::*;

use crate::catalogue::{Catalogue, SourceKind};
use crate::classify::Classifier;
use crate::config::{self, Config, DatamapSetting};
use crate::datamap::{self, Datamap};
use crate::engine::{self, Options};
use crate::facts;
use crate::files::{self, Excludes, Listing};
use crate::lower;
use crate::program::Program;
use crate::report::{self, Analysis, ConfigRef, Document, LanguageCount, Subject};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum EngineKind {
    Worklist,
    Datalog,
}

/// Where file contents come from.
pub enum Tree<'a> {
    WorkTree(&'a Path),
    /// A revision's files, already read.
    Revision(&'a BTreeMap<String, Vec<u8>>),
}

impl Tree<'_> {
    fn get(&self, rel: &str) -> Option<Vec<u8>> {
        match self {
            Tree::WorkTree(root) => {
                let p = root.join(rel);
                p.is_file().then(|| std::fs::read(p).ok()).flatten()
            }
            Tree::Revision(files) => files.get(rel).cloned(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Timings {
    pub files: usize,
    pub lines: u64,
    pub lower_ms: u128,
    pub program_ms: u128,
    pub facts_ms: u128,
    pub engine_ms: u128,
    pub report_ms: u128,
}

pub struct Loaded {
    pub config: Config,
    pub config_ref: ConfigRef,
}

/// Read the configuration: an explicit path, or `.privacy-flow.yml` in the tree.
pub fn load_config(tree: &Tree, explicit: Option<&Path>) -> Result<Loaded> {
    let (path, text) = match explicit {
        Some(p) => (
            Some(p.to_string_lossy().replace('\\', "/")),
            Some(std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?),
        ),
        None => match tree.get(config::FILE) {
            Some(bytes) => (
                Some(config::FILE.to_string()),
                Some(String::from_utf8(bytes).context(".privacy-flow.yml is not UTF-8")?),
            ),
            None => (None, None),
        },
    };
    match (path, text) {
        (Some(path), Some(text)) => {
            let config = Config::parse(&text, &path)?;
            Ok(Loaded {
                config,
                config_ref: ConfigRef {
                    digest: Some(crate::canonical::sha256(text.as_bytes())),
                    path: Some(path),
                },
            })
        }
        _ => Ok(Loaded {
            config: Config::default(),
            config_ref: ConfigRef {
                path: None,
                digest: None,
            },
        }),
    }
}

pub fn classifier_for(config: &Config, catalogue: &Catalogue) -> Result<Classifier> {
    let mut c = Classifier::new()?;
    for def in &catalogue.contextual {
        c.add_contextual(def)?;
    }
    for src in &catalogue.sources {
        if src.def.kind == SourceKind::Field {
            for (name, cat) in &src.def.fields {
                c.add(name, cat, &src.def.id)?;
            }
        }
    }
    config.apply_fields(&mut c)?;
    Ok(c)
}

pub fn load_datamaps(tree: &Tree, config: &Config, listing: &Listing) -> Result<Vec<Datamap>> {
    let paths: Vec<String> = match &config.datamap {
        Some(DatamapSetting::Off(false)) => Vec::new(),
        Some(DatamapSetting::Off(true)) | None => datamap::DEFAULT_PATHS
            .iter()
            .filter(|p| listing.files.iter().any(|f| f == *p) || tree.get(p).is_some())
            .map(|p| p.to_string())
            .collect(),
        Some(DatamapSetting::Paths(ps)) => ps.clone(),
    };
    let mut out = Vec::new();
    for p in paths {
        let bytes = tree
            .get(&p)
            .with_context(|| format!("data map {p} not found"))?;
        let text = String::from_utf8(bytes).with_context(|| format!("{p} is not UTF-8"))?;
        out.push(Datamap::parse(&p, &text)?);
    }
    Ok(out)
}

pub struct Run {
    pub document: Document,
    pub timings: Timings,
    pub program: Program,
    pub facts: facts::Facts,
    pub reaches: Vec<engine::Reach>,
}

pub fn analyze(
    tree: &Tree,
    listing: &Listing,
    loaded: &Loaded,
    engine_kind: EngineKind,
) -> Result<Run> {
    let config = &loaded.config;
    let excludes = Excludes::new(&config.exclude, config.default_excludes)?;
    let mut supported: Vec<(String, crate::ir::Lang)> = Vec::new();
    let mut unsupported: Vec<(String, String)> = Vec::new();
    for f in &listing.files {
        if excludes.excluded(f) {
            continue;
        }
        if let Some(lang) = files::language_of(f) {
            supported.push((f.clone(), lang));
        } else if let Some(lang) = files::unsupported_language_of(f) {
            unsupported.push((lang.to_string(), f.clone()));
        }
    }
    let catalogue = Catalogue::load(&config.extensions())?;
    let mut classifier = classifier_for(config, &catalogue)?;
    let datamaps = load_datamaps(tree, config, listing)?;
    for dm in &datamaps {
        for (field, cat) in dm.all_fields() {
            if classifier.check_category(&cat).is_ok() {
                classifier.add(&field, &cat, &format!("datamap:{}", dm.path))?;
            }
        }
    }
    let resolver_inputs: Vec<(String, String)> = listing
        .files
        .iter()
        .filter(|f| crate::resolve::wants(f) && !f.split('/').any(|seg| seg == "node_modules"))
        .filter_map(|f| {
            tree.get(f)
                .map(|b| (f.clone(), String::from_utf8_lossy(&b).into_owned()))
        })
        .collect();
    let resolver = crate::resolve::Resolver::from_files(&resolver_inputs);

    let t0 = Instant::now();
    // Read and lower in parallel; results are collected in sorted path order.
    let lowered: Vec<Result<(crate::ir::FileIr, String), (String, String)>> = supported
        .par_iter()
        .map(|(path, lang)| {
            let bytes = tree
                .get(path)
                .ok_or_else(|| (path.clone(), "could not be read".to_string()))?;
            if bytes.len() > files::MAX_FILE_BYTES {
                return Err((
                    path.clone(),
                    format!(
                        "larger than {} MiB; not analysed",
                        files::MAX_FILE_BYTES / (1024 * 1024)
                    ),
                ));
            }
            let digest = crate::canonical::sha256(&bytes);
            let text = String::from_utf8_lossy(&bytes);
            Ok((lower::lower_file(path, *lang, &text), digest))
        })
        .collect();
    let mut irs = Vec::new();
    let mut digests = BTreeMap::new();
    let mut skipped = Vec::new();
    for r in lowered {
        match r {
            Ok((ir, d)) => {
                digests.insert(ir.path.clone(), d);
                irs.push(ir);
            }
            Err(e) => skipped.push(e),
        }
    }
    let mut langs: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut lines = 0u64;
    for ir in &irs {
        let e = langs.entry(ir.lang.name().to_string()).or_default();
        e.0 += 1;
        e.1 += ir.lines;
        lines += ir.lines as u64;
    }
    let lower_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let mut program = Program::build(irs, &resolver);
    let program_ms = t1.elapsed().as_millis();
    if std::env::var_os("PIIFLOW_DEBUG_IR").is_some() {
        program.dump(&mut std::io::stderr());
    }

    let t2 = Instant::now();
    let inputs = facts::Inputs {
        catalogue: &catalogue,
        classifier: &classifier,
        datamaps: &datamaps,
        unsupported_files: &unsupported,
    };
    let mut facts = facts::build(&mut program, &inputs);
    for (path, why) in skipped {
        facts.static_gaps.push(facts::StaticGap {
            kind: facts::GapKind::ParseError,
            detail: format!("not analysed: {why}"),
            locations: vec![(path, None)],
        });
    }
    let facts_ms = t2.elapsed().as_millis();

    let max_depth = config
        .max_call_depth
        .unwrap_or(crate::engine::DEFAULT_MAX_DEPTH);
    let opts = Options { max_depth };
    let t3 = Instant::now();
    let reaches = match engine_kind {
        EngineKind::Worklist => engine::worklist::run(&facts, &program, opts),
        EngineKind::Datalog => {
            // The Datalog engine computes reachability only; witness paths always come from the
            // worklist engine, and the two must agree.
            let paths = engine::worklist::run(&facts, &program, opts);
            let dl = engine::datalog::run(&facts, &program, opts);
            let wl: std::collections::BTreeSet<(u32, engine::Target)> =
                paths.iter().map(|r| (r.seed, r.target)).collect();
            if wl != dl {
                bail!(
                    "internal error: the Datalog and worklist engines disagree ({} vs {} reach facts)",
                    dl.len(),
                    wl.len()
                );
            }
            paths
        }
    };
    let engine_ms = t3.elapsed().as_millis();

    let t4 = Instant::now();
    let subject = Subject {
        enumerated_by: listing.method,
        commit: listing.commit.clone(),
        dirty: listing.dirty,
        files: program.files.len() as u32,
        languages: langs
            .into_iter()
            .map(|(language, (files, lines))| LanguageCount {
                language,
                files,
                lines,
            })
            .collect(),
    };
    let analysis = Analysis {
        engine: match engine_kind {
            EngineKind::Worklist => "worklist".into(),
            EngineKind::Datalog => "datalog".into(),
        },
        max_call_depth: max_depth,
        datamaps: datamaps.iter().map(|d| d.path.clone()).collect(),
    };
    let ctx = report::Context {
        catalogue: &catalogue,
        classifier: &classifier,
        config,
        config_ref: loaded.config_ref.clone(),
        subject,
        analysis,
        file_digests: &digests,
    };
    let document = report::build(&program, &facts, &reaches, &ctx)?;
    let report_ms = t4.elapsed().as_millis();
    Ok(Run {
        document,
        timings: Timings {
            files: program.files.len(),
            lines,
            lower_ms,
            program_ms,
            facts_ms,
            engine_ms,
            report_ms,
        },
        program,
        facts,
        reaches,
    })
}
