//! The `piiflow` command line.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser, Subcommand};

use crate::Exit;
use crate::analyze::{self, EngineKind, Tree};
use crate::catalogue::Catalogue;
use crate::classify::Classifier;
use crate::config::{self, Severity};
use crate::files;
use crate::output::{self, Format};
use crate::report::{self, Document};
use crate::rules::RULES;

pub const DEFAULT_OUTPUT: &str = ".privacy-flow/flows.json";
pub const FLOWS_SCHEMA: &str = include_str!("../schemas/flows.schema.json");

#[derive(Parser)]
#[command(
    name = "piiflow",
    version,
    about = "Find where personal data goes: logs, third-party SDKs, LLM providers and outbound HTTP.",
    long_about = "Find where personal data goes: logs, third-party SDKs, LLM providers and outbound HTTP.\n\nDeterministic and offline: no network access, no telemetry, no model. Every finding carries the full chain of file:line:column hops from source to sink, and anything the analysis cannot resolve is reported as a coverage gap, never as \"no flow\"."
)]
pub struct Cli {
    /// Silence status lines on stderr.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Worker threads (output is identical for every value).
    #[arg(long, global = true, value_name = "N")]
    pub threads: Option<usize>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Analyse a directory and write findings and flow facts (default .privacy-flow/flows.json).
    Scan(ScanArgs),
    /// Report only flows introduced or changed between two revisions (BASE..HEAD).
    Diff(DiffArgs),
    /// Print a finding's (or flow's) hop chain with the source line of each hop.
    Explain(ExplainArgs),
    /// Enforce policy and dispositions on a flow-facts document; exit 1 on failure.
    Check(CheckArgs),
    /// Re-check a flow-facts document (or in-toto Statement) against its schema, digest and rules.
    Validate(ValidateArgs),
    /// List the loaded rules and catalogue: every source, sink, sanitiser and propagator.
    Rules(RulesArgs),
    /// Show detected languages, frameworks, configuration and coverage before a scan.
    Doctor(DoctorArgs),
    /// Print shell completions.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Write man pages to a directory.
    Manpage {
        #[arg(default_value = "man")]
        dir: PathBuf,
    },
}

#[derive(clap::Args)]
pub struct ScanArgs {
    /// Directory to analyse.
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Output file; the format is inferred from the name unless --format is given.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Output format (stdout when --output is not given).
    #[arg(short, long, value_enum)]
    pub format: Option<Format>,
    /// Configuration file (default: .privacy-flow.yml in the directory).
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// A previous flow-facts document to carry dispositions from (default: the output file, or
    /// .privacy-flow/flows.json).
    #[arg(long)]
    pub dispositions: Option<PathBuf>,
    /// Flow engine. `datalog` also runs the Datalog engine and fails if the two disagree.
    #[arg(long, value_enum, default_value = "worklist", hide = true)]
    pub engine: EngineKind,
    /// Print phase timings to stderr.
    #[arg(long)]
    pub timings: bool,
}

#[derive(clap::Args)]
pub struct DiffArgs {
    /// Revision range: BASE..HEAD (two commits) or BASE.. (BASE against the working tree).
    pub range: String,
    /// Directory to analyse (inside the repository).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(short, long, value_enum)]
    pub format: Option<Format>,
    /// A flow-facts document to carry dispositions from.
    #[arg(long)]
    pub dispositions: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct ExplainArgs {
    /// A finding ID (pf-…), flow ID (flow-…) or gap ID (gap-…).
    pub id: String,
    /// Flow-facts document (default: .privacy-flow/flows.json under --root).
    #[arg(short, long)]
    pub input: Option<PathBuf>,
    /// Directory the document's paths are relative to.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

#[derive(clap::Args)]
pub struct CheckArgs {
    /// Flow-facts document or in-toto Statement.
    pub file: PathBuf,
    /// Date to evaluate dispositions on (YYYY-MM-DD); required when any disposition is not open.
    #[arg(long, value_name = "DATE")]
    pub as_of: Option<String>,
    /// Override the document's policy threshold.
    #[arg(long, value_name = "SEVERITY")]
    pub fail_on: Option<String>,
}

#[derive(clap::Args)]
pub struct ValidateArgs {
    pub file: PathBuf,
}

#[derive(clap::Args)]
pub struct RulesArgs {
    #[arg(short, long, value_enum, default_value = "table")]
    pub format: RulesFormat,
    /// Include a project's configuration entries.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum RulesFormat {
    Table,
    Json,
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// A failure with the exit code it maps to.
pub struct Failure {
    pub exit: Exit,
    pub error: anyhow::Error,
}

fn usage(e: impl Into<anyhow::Error>) -> Failure {
    Failure {
        exit: Exit::Usage,
        error: e.into(),
    }
}

fn invalid(e: impl Into<anyhow::Error>) -> Failure {
    Failure {
        exit: Exit::InvalidInput,
        error: e.into(),
    }
}

type Outcome = std::result::Result<Exit, Failure>;

pub fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { Exit::Usage as u8 } else { 0 };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    if let Some(n) = cli.threads {
        if n == 0 {
            eprintln!("error: --threads must be at least 1");
            return ExitCode::from(Exit::Usage as u8);
        }
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global();
    }
    let quiet = cli.quiet;
    let result = match cli.command {
        Command::Scan(a) => scan(a, quiet),
        Command::Diff(a) => diff(a, quiet),
        Command::Explain(a) => explain(a),
        Command::Check(a) => check(a, quiet),
        Command::Validate(a) => validate_cmd(a, quiet),
        Command::Rules(a) => rules(a),
        Command::Doctor(a) => doctor(a),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "piiflow",
                &mut std::io::stdout(),
            );
            Ok(Exit::Ok)
        }
        Command::Manpage { dir } => manpage(&dir),
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(f) => {
            eprintln!("error: {:#}", f.error);
            ExitCode::from(f.exit as u8)
        }
    }
}

fn status(quiet: bool, msg: impl AsRef<str>) {
    if !quiet {
        eprintln!("{}", msg.as_ref());
    }
}

fn write_output(path: Option<&Path>, bytes: &str) -> Result<()> {
    match path {
        Some(p) => {
            if let Some(parent) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::write(p, bytes).with_context(|| format!("writing {}", p.display()))
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(bytes.as_bytes())?;
            if !bytes.ends_with('\n') {
                out.write_all(b"\n")?;
            }
            Ok(())
        }
    }
}

/// Read a flow-facts document from JSON or an in-toto Statement, without validating it.
pub fn read_document(path: &Path) -> Result<Document> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    let value = if value.get("_type").is_some() {
        output::intoto::unwrap(&value)?
    } else {
        value
    };
    config::validate_schema(FLOWS_SCHEMA, &value, &path.display().to_string())?;
    serde_json::from_value(value)
        .with_context(|| format!("{} is not a privacy-flow document", path.display()))
}

/// Full validation: schema, digest, derived findings, and dispositions.
pub fn validate_document(doc: &Document) -> Result<()> {
    let digest = report::digest_of(doc)?;
    if digest != doc.digest {
        return Err(anyhow!(
            "digest mismatch: the document says {} but its content hashes to {digest}",
            doc.digest
        ));
    }
    let classifier = Classifier::new()?;
    let mut again = doc.clone();
    report::derive(&mut again, &classifier)?;
    let strip = |d: &Document| {
        let mut d = d.clone();
        for f in &mut d.findings {
            f.disposition = None;
        }
        (d.findings, d.egress, d.summary)
    };
    let (derived, _, _) = strip(&again);
    let (findings, egress, summary) = strip(doc);
    if doc.diff.is_some() {
        // A diff keeps the findings that are new or changed: each must be one the facts imply.
        if let Some(f) = findings.iter().find(|f| !derived.contains(f)) {
            return Err(anyhow!(
                "finding {} does not follow from the document's flows, coverage and policy",
                f.id
            ));
        }
    } else if (derived, again.egress.clone(), again.summary.clone()) != (findings, egress, summary)
    {
        return Err(anyhow!(
            "findings, egress or summary do not follow from the document's flows, coverage and policy"
        ));
    }
    for f in &doc.findings {
        if !report::known_rule(&f.rule_id) {
            return Err(anyhow!("finding {} names unknown rule {}", f.id, f.rule_id));
        }
        if let Some(d) = &f.disposition {
            let errs = report::check_disposition(d);
            if !errs.is_empty() {
                return Err(anyhow!(
                    "finding {}: invalid disposition: {}",
                    f.id,
                    errs.join("; ")
                ));
            }
        }
    }
    Ok(())
}

fn previous_dispositions(
    explicit: Option<&Path>,
    output: Option<&Path>,
    root: &Path,
) -> Option<Document> {
    let candidates: Vec<PathBuf> = match explicit {
        Some(p) => vec![p.to_path_buf()],
        None => output
            .map(Path::to_path_buf)
            .into_iter()
            .chain(std::iter::once(root.join(DEFAULT_OUTPUT)))
            .collect(),
    };
    candidates.into_iter().find_map(|p| {
        if p.is_file() {
            read_document(&p).ok()
        } else {
            None
        }
    })
}

fn summary_line(doc: &Document) -> String {
    let counts: Vec<String> = doc
        .summary
        .findings
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    let cov = if doc.coverage.complete {
        "coverage complete".to_string()
    } else {
        format!("coverage INCOMPLETE ({} gap(s))", doc.coverage.gaps.len())
    };
    format!(
        "{} flow(s), {} finding(s){}; {cov}",
        doc.flows.len(),
        doc.findings.len(),
        if counts.is_empty() {
            String::new()
        } else {
            format!(" ({})", counts.join(", "))
        }
    )
}

fn emit(
    doc: &Document,
    output: Option<PathBuf>,
    format: Option<Format>,
    default_path: PathBuf,
    quiet: bool,
) -> std::result::Result<(), Failure> {
    let (path, format) = match (output, format) {
        (Some(o), f) => {
            let f = f.or_else(|| Format::infer(&o)).unwrap_or(Format::Json);
            (Some(o), f)
        }
        (None, Some(f)) => (None, f),
        (None, None) => (Some(default_path), Format::Json),
    };
    let bytes = output::render(doc, format).map_err(usage)?;
    write_output(path.as_deref(), &bytes).map_err(invalid)?;
    if let Some(p) = &path {
        status(
            quiet,
            format!("wrote {}: {}", p.display(), summary_line(doc)),
        );
    }
    Ok(())
}

// ------------------------------------------------------------------------------- scan

fn scan(a: ScanArgs, quiet: bool) -> Outcome {
    if !a.path.is_dir() {
        return Err(usage(anyhow!("{} is not a directory", a.path.display())));
    }
    let root = a.path.as_path();
    let tree = Tree::WorkTree(root);
    let loaded = analyze::load_config(&tree, a.config.as_deref()).map_err(invalid)?;
    let listing = files::list(root).map_err(invalid)?;
    let run = analyze::analyze(&tree, &listing, &loaded, a.engine).map_err(invalid)?;
    let mut doc = run.document;
    if let Some(prev) = previous_dispositions(a.dispositions.as_deref(), a.output.as_deref(), root)
    {
        let dropped = report::carry_dispositions(&mut doc, &prev);
        for id in dropped {
            status(
                quiet,
                format!("note: disposition for {id} dropped: the finding no longer exists"),
            );
        }
    }
    if a.timings {
        let t = &run.timings;
        eprintln!(
            "timings: {} files, {} lines; lower {} ms, program {} ms, facts {} ms, engine {} ms, report {} ms",
            t.files, t.lines, t.lower_ms, t.program_ms, t.facts_ms, t.engine_ms, t.report_ms
        );
    }
    emit(&doc, a.output, a.format, root.join(DEFAULT_OUTPUT), quiet)?;
    Ok(if doc.coverage.complete {
        Exit::Ok
    } else {
        Exit::CoverageIncomplete
    })
}

// ------------------------------------------------------------------------------- diff

/// A finding's identity across revisions: the rule and the flow's (source, sink) anchors, or
/// the gap.
fn diff_doc(base: &Document, head: &Document) -> Document {
    let base_findings: BTreeMap<&str, &report::FindingRec> =
        base.findings.iter().map(|f| (f.id.as_str(), f)).collect();
    let base_flows: BTreeMap<(&str, &str), Vec<&str>> = {
        let mut m: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
        for f in &base.flows {
            m.entry((f.source.as_str(), f.sink.as_str()))
                .or_default()
                .push(f.category.as_str());
        }
        m
    };
    let base_gaps: BTreeMap<&str, &report::GapRec> = base
        .coverage
        .gaps
        .iter()
        .map(|g| (g.id.as_str(), g))
        .collect();
    let mut out = head.clone();
    let flows_by_id: BTreeMap<String, report::FlowRec> = head
        .flows
        .iter()
        .map(|f| (f.id.clone(), f.clone()))
        .collect();
    let mut introduced = 0u32;
    let mut changed = 0u32;
    let mut keep_findings = Vec::new();
    for f in &head.findings {
        if base_findings.contains_key(f.id.as_str()) {
            // Same rule and same flow (or gap): changed only if a gap gained locations.
            if let Some(g) = f.gap.as_deref() {
                let hg = head.coverage.gaps.iter().find(|x| x.id == g);
                let bg = base_gaps.get(g);
                if let (Some(hg), Some(bg)) = (hg, bg)
                    && hg.locations.iter().any(|l| !bg.locations.contains(l))
                {
                    changed += 1;
                    keep_findings.push(f.clone());
                }
            }
            continue;
        }
        let is_changed = f
            .flow
            .as_deref()
            .and_then(|id| flows_by_id.get(id))
            .is_some_and(|fl| base_flows.contains_key(&(fl.source.as_str(), fl.sink.as_str())));
        if is_changed {
            changed += 1;
        } else {
            introduced += 1;
        }
        keep_findings.push(f.clone());
    }
    let head_ids: std::collections::BTreeSet<&str> =
        head.findings.iter().map(|f| f.id.as_str()).collect();
    let removed = base
        .findings
        .iter()
        .filter(|f| !head_ids.contains(f.id.as_str()))
        .count() as u32;
    // Keep the flows, gaps, sources and sinks the kept findings refer to.
    let flow_ids: std::collections::BTreeSet<String> = keep_findings
        .iter()
        .filter_map(|f| f.flow.clone())
        .collect();
    let gap_ids: std::collections::BTreeSet<String> =
        keep_findings.iter().filter_map(|f| f.gap.clone()).collect();
    out.flows.retain(|f| flow_ids.contains(&f.id));
    out.coverage.gaps.retain(|g| gap_ids.contains(&g.id));
    let src: std::collections::BTreeSet<&str> =
        out.flows.iter().map(|f| f.source.as_str()).collect();
    let snk: std::collections::BTreeSet<&str> = out.flows.iter().map(|f| f.sink.as_str()).collect();
    let src: std::collections::BTreeSet<String> = src.into_iter().map(str::to_string).collect();
    let snk: std::collections::BTreeSet<String> = snk.into_iter().map(str::to_string).collect();
    out.sources.retain(|s| src.contains(&s.id));
    out.sinks.retain(|s| snk.contains(&s.id));
    out.findings = keep_findings;
    out.diff = Some(report::DiffInfo {
        base: base.subject.commit.clone().unwrap_or_default(),
        head: head
            .subject
            .commit
            .clone()
            .unwrap_or_else(|| "working tree".into()),
        introduced,
        changed,
        removed,
    });
    out
}

fn diff(a: DiffArgs, quiet: bool) -> Outcome {
    let (base_rev, head_rev) = match a.range.split_once("..") {
        Some((b, h)) if !b.is_empty() && !h.contains("..") => {
            (b.to_string(), (!h.is_empty()).then(|| h.to_string()))
        }
        _ => {
            return Err(usage(anyhow!(
                "expected BASE..HEAD or BASE.., got {:?}",
                a.range
            )));
        }
    };
    let root = a.path.as_path();
    if !files::in_work_tree(root) {
        return Err(usage(anyhow!(
            "diff needs a Git work tree; {} is not inside one",
            root.display()
        )));
    }
    let (base_listing, base_files) = files::read_revision(root, &base_rev).map_err(usage)?;
    let base_map: BTreeMap<String, Vec<u8>> = base_files.into_iter().collect();
    let base_tree = Tree::Revision(&base_map);
    let base_loaded = analyze::load_config(&base_tree, None).map_err(invalid)?;
    let base = analyze::analyze(
        &base_tree,
        &base_listing,
        &base_loaded,
        EngineKind::Worklist,
    )
    .map_err(invalid)?
    .document;
    let head = match &head_rev {
        Some(h) => {
            let (listing, files_) = files::read_revision(root, h).map_err(usage)?;
            let map: BTreeMap<String, Vec<u8>> = files_.into_iter().collect();
            let tree = Tree::Revision(&map);
            let loaded = analyze::load_config(&tree, None).map_err(invalid)?;
            analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist)
                .map_err(invalid)?
                .document
        }
        None => {
            let tree = Tree::WorkTree(root);
            let loaded = analyze::load_config(&tree, None).map_err(invalid)?;
            let listing = files::list(root).map_err(invalid)?;
            analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist)
                .map_err(invalid)?
                .document
        }
    };
    let mut doc = diff_doc(&base, &head);
    if let Some(prev) = previous_dispositions(a.dispositions.as_deref(), None, root) {
        report::carry_dispositions(&mut doc, &prev);
    }
    report::derive_summary_only(&mut doc);
    report::seal(&mut doc).map_err(invalid)?;
    let default = root.join(".privacy-flow/diff.json");
    emit(&doc, a.output, a.format, default, quiet)?;
    Ok(if doc.coverage.gaps.is_empty() {
        Exit::Ok
    } else {
        Exit::CoverageIncomplete
    })
}

// ------------------------------------------------------------------------------- explain

fn read_lines(root: &Path, path: &str) -> Option<Vec<String>> {
    let text = std::fs::read(root.join(path)).ok()?;
    Some(
        String::from_utf8_lossy(&text)
            .lines()
            .map(str::to_string)
            .collect(),
    )
}

fn explain(a: ExplainArgs) -> Outcome {
    let input = a
        .input
        .clone()
        .unwrap_or_else(|| a.root.join(DEFAULT_OUTPUT));
    let doc = read_document(&input).map_err(invalid)?;
    let finding = doc.findings.iter().find(|f| f.id == a.id);
    let flow_id = match finding {
        Some(f) => f.flow.clone(),
        None if a.id.starts_with("flow-") => Some(a.id.clone()),
        None => None,
    };
    let gap_id = match finding {
        Some(f) => f.gap.clone(),
        None if a.id.starts_with("gap-") => Some(a.id.clone()),
        None => None,
    };
    if finding.is_none() && flow_id.is_none() && gap_id.is_none() {
        return Err(usage(anyhow!(
            "no finding, flow or gap {} in {}",
            a.id,
            input.display()
        )));
    }
    let digests: BTreeMap<&str, &str> = doc
        .cited_files
        .iter()
        .map(|c| (c.path.as_str(), c.sha256.as_str()))
        .collect();
    let mut cache: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    let mut changed: BTreeMap<String, bool> = BTreeMap::new();
    let mut out = String::new();
    use std::fmt::Write;
    let mut line_of = |path: &str, line: u32| -> Option<String> {
        let lines = cache
            .entry(path.to_string())
            .or_insert_with(|| read_lines(&a.root, path));
        if !changed.contains_key(path) {
            let now = std::fs::read(a.root.join(path))
                .ok()
                .map(|b| crate::canonical::sha256(&b));
            let same = match (now.as_deref(), digests.get(path)) {
                (Some(n), Some(d)) => n == *d,
                _ => false,
            };
            changed.insert(path.to_string(), !same);
        }
        lines
            .as_ref()
            .and_then(|l| l.get(line.saturating_sub(1) as usize).cloned())
    };
    if let Some(f) = finding {
        let _ = writeln!(
            out,
            "{} {} {}: {}",
            f.id,
            f.rule_id,
            f.severity.as_str(),
            f.message
        );
        if let Some(d) = &f.disposition {
            let _ = writeln!(
                out,
                "disposition: {:?} by {} on {}",
                d.status,
                d.owner.as_deref().unwrap_or("?"),
                d.decided_at.as_deref().unwrap_or("?")
            );
        }
        if let Some(r) = crate::rules::rule(&f.rule_id) {
            let _ = writeln!(out, "rule: {} — {}", r.title, crate::rules::help_uri(r.id));
        }
    }
    if let Some(fid) = &flow_id {
        let Some(flow) = doc.flows.iter().find(|x| &x.id == fid) else {
            return Err(invalid(anyhow!("flow {fid} is not in the document")));
        };
        let _ = writeln!(
            out,
            "{} ({}, call depth {}):",
            flow.id, flow.category, flow.call_depth
        );
        for (i, h) in flow.path.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {:>2}. {:<7} {}:{}:{}",
                i + 1,
                h.kind,
                h.path,
                h.line,
                h.column
            );
            if let Some(n) = &h.note {
                let _ = writeln!(out, "      {n}");
            }
            match line_of(&h.path, h.line) {
                Some(src) => {
                    let _ = writeln!(out, "      {:>5} | {}", h.line, src);
                }
                None => {
                    let _ = writeln!(
                        out,
                        "      {:>5} | {}   (file not readable; text from the scan)",
                        h.line, h.text
                    );
                }
            }
        }
    }
    if let Some(gid) = &gap_id {
        let Some(g) = doc.coverage.gaps.iter().find(|x| &x.id == gid) else {
            return Err(invalid(anyhow!("gap {gid} is not in the document")));
        };
        let _ = writeln!(out, "{} {}: {}", g.id, g.kind, g.detail);
        for l in &g.locations {
            match l.line {
                Some(line) => {
                    let _ = writeln!(out, "  {}:{}:{}", l.path, line, l.column.unwrap_or(1));
                    if let Some(src) = line_of(&l.path, line) {
                        let _ = writeln!(out, "      {:>5} | {}", line, src);
                    }
                }
                None => {
                    let _ = writeln!(out, "  {}", l.path);
                }
            }
        }
        if !g.sources.is_empty() {
            let _ = writeln!(
                out,
                "  reached by {} source(s): {}",
                g.sources.len(),
                g.sources.join(", ")
            );
        }
    }
    let stale: Vec<&String> = changed
        .iter()
        .filter(|(_, c)| **c)
        .map(|(p, _)| p)
        .collect();
    if !stale.is_empty() {
        let _ = writeln!(
            out,
            "note: {} changed since the scan (or is not in it); line text may not match: {}",
            if stale.len() == 1 {
                "this file has"
            } else {
                "these files have"
            },
            stale
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    write_output(None, &out).map_err(invalid)?;
    Ok(Exit::Ok)
}

// ------------------------------------------------------------------------------- check

fn check(a: CheckArgs, quiet: bool) -> Outcome {
    let doc = read_document(&a.file).map_err(invalid)?;
    validate_document(&doc).map_err(invalid)?;
    if let Some(d) = &a.as_of
        && !report::is_date(d)
    {
        return Err(usage(anyhow!(
            "--as-of must be a calendar date (YYYY-MM-DD), got {d:?}"
        )));
    }
    let needs_date = doc.findings.iter().any(|f| {
        f.disposition
            .as_ref()
            .is_some_and(|d| d.status != report::DispositionStatus::Open)
    });
    if needs_date && a.as_of.is_none() {
        return Err(usage(anyhow!(
            "the document records dispositions; pass --as-of YYYY-MM-DD (the machine clock is never read)"
        )));
    }
    let fail_on = match &a.fail_on {
        Some(s) => Severity::parse(s)
            .ok_or_else(|| usage(anyhow!("--fail-on must be info, warning, medium or high")))?,
        None => doc.policy.fail_on,
    };
    let as_of = a.as_of.clone().unwrap_or_default();
    let active = |f: &&report::FindingRec| {
        !f.disposition
            .as_ref()
            .is_some_and(|d| report::suppressed(d, &as_of))
    };
    let blocking: Vec<&report::FindingRec> = doc
        .findings
        .iter()
        .filter(|f| f.rule_id == "PFC01")
        .filter(active)
        .collect();
    let failing: Vec<&report::FindingRec> = doc
        .findings
        .iter()
        .filter(|f| f.rule_id != "PFC01" && f.severity >= fail_on)
        .filter(active)
        .collect();
    let mut out = String::new();
    use std::fmt::Write;
    for f in failing.iter().chain(blocking.iter()) {
        let loc = f
            .location
            .as_ref()
            .map(|l| format!("{}:{}:{}", l.path, l.line, l.column))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{}  {:<7}  {}  {}\n    {}",
            f.rule_id,
            f.severity.as_str(),
            f.id,
            loc,
            f.message
        );
    }
    let verdict = if !blocking.is_empty() {
        format!(
            "INCOMPLETE: {} coverage gap(s) without a disposition; a clean result cannot be claimed",
            blocking.len()
        )
    } else if !failing.is_empty() {
        format!(
            "FAIL: {} finding(s) at or above {}",
            failing.len(),
            fail_on.as_str()
        )
    } else {
        format!(
            "PASS: no finding at or above {} without a disposition",
            fail_on.as_str()
        )
    };
    let _ = writeln!(out, "{verdict}");
    write_output(None, &out).map_err(invalid)?;
    let _ = quiet;
    Ok(if !blocking.is_empty() {
        Exit::CoverageIncomplete
    } else if !failing.is_empty() {
        Exit::PolicyFailed
    } else {
        Exit::Ok
    })
}

fn validate_cmd(a: ValidateArgs, quiet: bool) -> Outcome {
    let doc = read_document(&a.file).map_err(invalid)?;
    validate_document(&doc).map_err(invalid)?;
    status(
        quiet,
        format!(
            "{}: valid ({}; {})",
            a.file.display(),
            doc.digest,
            summary_line(&doc)
        ),
    );
    Ok(Exit::Ok)
}

// ------------------------------------------------------------------------------- rules

#[derive(serde::Serialize)]
struct RulesDoc<'a> {
    catalogue_version: &'a str,
    catalogue_digest: &'a str,
    rules: Vec<serde_json::Value>,
    sources: Vec<serde_json::Value>,
    sinks: Vec<serde_json::Value>,
    sanitisers: Vec<serde_json::Value>,
    propagators: Vec<serde_json::Value>,
    frameworks: Vec<serde_json::Value>,
}

fn rules(a: RulesArgs) -> Outcome {
    let cfg = match &a.config {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .with_context(|| format!("reading {}", p.display()))
                .map_err(invalid)?;
            config::Config::parse(&text, &p.display().to_string()).map_err(invalid)?
        }
        None => config::Config::default(),
    };
    let cat = Catalogue::load(&cfg.extensions()).map_err(invalid)?;
    let tag = |o: &crate::catalogue::Origin| match o {
        crate::catalogue::Origin::Default => "default",
        crate::catalogue::Origin::Config => "config",
    };
    let doc = RulesDoc {
        catalogue_version: &cat.version,
        catalogue_digest: &cat.digest,
        rules: RULES
            .iter()
            .map(|r| serde_json::json!({"id": r.id, "name": r.name, "title": r.title, "severity": r.severity.as_str(), "maps_to": r.maps_to, "help": crate::rules::help_uri(r.id)}))
            .collect(),
        sources: cat.sources.iter().map(|s| serde_json::json!({"origin": tag(&s.origin), "definition": s.def})).collect(),
        sinks: cat.sinks.iter().map(|s| serde_json::json!({"origin": tag(&s.origin), "definition": s.def})).collect(),
        sanitisers: cat.sanitisers.iter().map(|s| serde_json::json!({"origin": tag(&s.origin), "definition": s.def})).collect(),
        propagators: cat.propagators.iter().map(|s| serde_json::json!({"origin": tag(&s.origin), "definition": s.def})).collect(),
        frameworks: cat.frameworks.iter().map(|f| serde_json::json!(f)).collect(),
    };
    let text = match a.format {
        RulesFormat::Json => crate::canonical::jcs_bytes(&doc).map_err(invalid)?,
        RulesFormat::Table => {
            use std::fmt::Write;
            let mut out = String::new();
            let _ = writeln!(out, "catalogue {} ({})\n", cat.version, cat.digest);
            out.push_str("RULES\n");
            for r in RULES {
                let _ = writeln!(out, "  {:<6} {:<8} {}", r.id, r.severity.as_str(), r.title);
            }
            out.push_str("\nSINKS\n");
            for s in &cat.sinks {
                let proc = s.def.processor.as_deref().unwrap_or("-");
                let pats: Vec<&str> = s
                    .def
                    .calls
                    .iter()
                    .chain(&s.def.assigns)
                    .map(String::as_str)
                    .collect();
                let pats = if pats.is_empty() {
                    format!(
                        "receivers {} · methods {}",
                        s.def.receivers.join("|"),
                        s.def.methods.join("|")
                    )
                } else {
                    pats.join("  ")
                };
                let _ = writeln!(
                    out,
                    "  {:<26} {:<16} {:<14} [{}] {}",
                    s.def.id,
                    s.def.class.as_str(),
                    proc,
                    tag(&s.origin),
                    pats
                );
            }
            out.push_str("\nSOURCES\n");
            for s in &cat.sources {
                let what: Vec<&str> = s
                    .def
                    .registrations
                    .iter()
                    .chain(&s.def.decorators)
                    .chain(&s.def.files)
                    .chain(&s.def.reads)
                    .chain(&s.def.calls)
                    .map(String::as_str)
                    .collect();
                let _ = writeln!(
                    out,
                    "  {:<26} {:<13} {:<10} [{}] {}",
                    s.def.id,
                    format!("{:?}", s.def.kind).to_lowercase(),
                    s.def.framework.as_deref().unwrap_or("-"),
                    tag(&s.origin),
                    what.join("  ")
                );
            }
            out.push_str("  (plus every field name in the classification table: vendor/classification/classification.json)\n");
            out.push_str("\nSANITISERS\n");
            for s in &cat.sanitisers {
                let what: Vec<&str> = s
                    .def
                    .calls
                    .iter()
                    .chain(&s.def.methods)
                    .chain(&s.def.functions)
                    .map(String::as_str)
                    .collect();
                let _ = writeln!(
                    out,
                    "  {:<26} removes {:<28} [{}] {}",
                    s.def.id,
                    s.def.removes.join(","),
                    tag(&s.origin),
                    what.join("  ")
                );
            }
            out.push_str("\nPROPAGATORS\n");
            for s in &cat.propagators {
                let n = s.def.calls.len() + s.def.methods.len();
                let _ = writeln!(
                    out,
                    "  {:<26} {:<20} [{}] {} pattern(s)",
                    s.def.id,
                    format!("{:?}", s.def.flow),
                    tag(&s.origin),
                    n
                );
            }
            out.push_str("\nFRAMEWORKS\n");
            for f in &cat.frameworks {
                let _ = writeln!(
                    out,
                    "  {:<18} {:<10} {:<13} {}",
                    f.name,
                    f.language,
                    if f.supported {
                        "supported"
                    } else {
                        "NOT supported"
                    },
                    f.modules.join(", ")
                );
            }
            out
        }
    };
    write_output(None, &text).map_err(invalid)?;
    Ok(Exit::Ok)
}

// ------------------------------------------------------------------------------- doctor

fn doctor(a: DoctorArgs) -> Outcome {
    use std::fmt::Write;
    let root = a.path.as_path();
    if !root.is_dir() {
        return Err(usage(anyhow!("{} is not a directory", root.display())));
    }
    let tree = Tree::WorkTree(root);
    let mut out = String::new();
    let listing = files::list(root).map_err(invalid)?;
    let _ = writeln!(
        out,
        "files: {} enumerated by {}{}",
        listing.files.len(),
        match listing.method {
            files::Method::Git => "git ls-files",
            _ => "directory walk (not a Git work tree)",
        },
        listing
            .commit
            .as_deref()
            .map(|c| format!(" at {}", &c[..12.min(c.len())]))
            .unwrap_or_default()
    );
    let loaded = match analyze::load_config(&tree, a.config.as_deref()) {
        Ok(l) => {
            let _ = writeln!(
                out,
                "config: {}",
                match &l.config_ref.path {
                    Some(p) => format!(
                        "{p} (valid; {} processor(s), {} extra source(s), {} extra sink(s), {} sanitiser(s))",
                        l.config.processors.len(),
                        l.config.sources.len() + l.config.fields.len(),
                        l.config.sinks.len(),
                        l.config.sanitisers.len()
                    ),
                    None => "none (.privacy-flow.yml not found; defaults apply)".into(),
                }
            );
            l
        }
        Err(e) => {
            let _ = writeln!(out, "config: INVALID — {e:#}");
            write_output(None, &out).map_err(invalid)?;
            return Ok(Exit::InvalidInput);
        }
    };
    let run = analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist).map_err(invalid)?;
    let doc = &run.document;
    out.push_str("languages:\n");
    for l in &doc.subject.languages {
        let _ = writeln!(
            out,
            "  {:<11} {:>5} files {:>8} lines",
            l.language, l.files, l.lines
        );
    }
    if doc.subject.languages.is_empty() {
        out.push_str("  (no TypeScript, JavaScript or Python files after exclusions)\n");
    }
    let cat = Catalogue::load(&loaded.config.extensions()).map_err(invalid)?;
    out.push_str("frameworks (from imports):\n");
    let mut any = false;
    for fw in &cat.frameworks {
        let hit = run.program.imports.iter().any(|imp| match &imp.target {
            crate::program::Target::External(m) => fw.modules.iter().any(|x| {
                m == x || m.starts_with(&format!("{x}/")) || m.starts_with(&format!("{x}."))
            }),
            _ => false,
        });
        if hit {
            any = true;
            let _ = writeln!(
                out,
                "  {:<18} {}",
                fw.name,
                if fw.supported {
                    "supported"
                } else {
                    "NOT supported: reported as a coverage gap"
                }
            );
        }
    }
    if !any {
        out.push_str("  none detected\n");
    }
    if !run.facts.handlers.is_empty() {
        out.push_str("request handlers recognised:\n");
        for (id, n) in &run.facts.handlers {
            let _ = writeln!(out, "  {id}: {n}");
        }
    }
    let _ = writeln!(
        out,
        "data maps: {}",
        if doc.analysis.datamaps.is_empty() {
            "none".to_string()
        } else {
            doc.analysis.datamaps.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "catalogue: {} ({} sinks, {} sources, {} sanitisers, {} propagators)",
        cat.version,
        cat.sinks.len(),
        cat.sources.len(),
        cat.sanitisers.len(),
        cat.propagators.len()
    );
    let _ = writeln!(
        out,
        "analysis: {} sources and {} sinks found; max call depth {}",
        doc.summary.sources, doc.summary.sinks, doc.analysis.max_call_depth
    );
    if doc.coverage.complete {
        out.push_str("coverage: complete\n");
    } else {
        let _ = writeln!(
            out,
            "coverage: {} gap(s) a scan will report:",
            doc.coverage.gaps.len()
        );
        for g in &doc.coverage.gaps {
            let _ = writeln!(
                out,
                "  {:<22} {} ({} location(s))",
                g.kind,
                g.detail,
                g.locations.len()
            );
        }
    }
    write_output(None, &out).map_err(invalid)?;
    Ok(Exit::Ok)
}

fn manpage(dir: &Path) -> Outcome {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating {}", dir.display()))
        .map_err(invalid)?;
    let cmd = Cli::command();
    let mut pages = vec![(String::from("piiflow"), cmd.clone())];
    for sub in cmd.get_subcommands() {
        pages.push((format!("piiflow-{}", sub.get_name()), sub.clone()));
    }
    for (name, c) in pages {
        let mut buf = Vec::new();
        clap_mangen::Man::new(c.name(name.clone().leak() as &'static str))
            .render(&mut buf)
            .map_err(|e| invalid(anyhow!(e)))?;
        std::fs::write(dir.join(format!("{name}.1")), buf).map_err(|e| invalid(anyhow!(e)))?;
    }
    Ok(Exit::Ok)
}
