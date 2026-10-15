//! The repository's systems and the edges between them and their processors, for
//! `piiflow datamap` (ADR 0018). `scan` and `diff` never run this: their output is unchanged.

pub mod catalogue;
pub mod clients;
pub mod model;
pub mod names;
pub mod prisma;
pub mod stores;
pub mod systems;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalogue::SinkClass;
use crate::report::{self, Document};
use model::{AttributionRec, EdgeRec, Endpoint, NoteRec, Topology};

pub struct Inputs<'a> {
    pub program: &'a crate::program::Program,
    pub facts: &'a crate::facts::Facts,
    pub resolver: &'a crate::resolve::Resolver,
    /// Every listed file the configuration does not exclude.
    pub files: &'a [String],
    pub config: &'a crate::config::Config,
    pub catalogue: &'a crate::catalogue::Catalogue,
    pub topo: &'a catalogue::TopologyCatalogue,
    pub classifier: &'a crate::classify::Classifier,
    /// The files [`wants`] selects: (path, text).
    pub texts: &'a [(String, String)],
}

/// Whether a listed file is one `build` reads besides the analysed sources: docker-compose
/// files and Prisma schemas.
pub fn wants(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    ((name.starts_with("docker-compose") || name.starts_with("compose"))
        && (name.ends_with(".yml") || name.ends_with(".yaml")))
        || name.ends_with(".prisma")
}

/// Detect the systems, bind requests to client registrations, attribute every flow, and
/// derive the edges into `doc.topology`.
pub fn build(doc: &mut Document, inp: &Inputs) {
    let mut sys = systems::detect(&systems::Inputs {
        program: inp.program,
        resolver: inp.resolver,
        files: inp.files,
        config: inp.config,
        catalogue: inp.catalogue,
        topo: inp.topo,
    });
    let defs = crate::values::Defs::build(inp.program);
    let names = names::collect(
        &names::Inputs {
            program: inp.program,
            defs: &defs,
            resolver: inp.resolver,
            topo: inp.topo,
            compose: inp.texts,
        },
        &mut sys.recs,
    );
    let closures: Vec<(String, &[bool])> = sys
        .recs
        .iter()
        .zip(&sys.closure)
        .map(|(r, c)| (r.key.clone(), c.as_slice()))
        .collect();
    let bindings = clients::bind(
        &clients::Inputs {
            program: inp.program,
            facts: inp.facts,
            defs: &defs,
            catalogue: inp.catalogue,
            topo: inp.topo,
            names: &names,
            systems: &closures,
        },
        &doc.sinks,
    );
    drop(closures);
    user_input(inp, &mut sys);
    let with_own: Vec<(String, &[bool], &[bool])> = sys
        .recs
        .iter()
        .zip(&sys.closure)
        .zip(&sys.own)
        .map(|((r, c), o)| (r.key.clone(), c.as_slice(), o.as_slice()))
        .collect();
    let stores = stores::datasets(&stores::Inputs {
        texts: inp.texts,
        program: inp.program,
        defs: &defs,
        classifier: inp.classifier,
        topo: inp.topo,
        resolver: inp.resolver,
        systems: &with_own,
    });
    drop(with_own);
    for r in &mut sys.recs {
        if let Some(keys) = stores.links.get(&r.key) {
            r.datasets = keys.iter().cloned().collect();
        }
    }
    let path_index: BTreeMap<&str, usize> = inp
        .program
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();
    let mut attribution = Vec::new();
    let mut notes = sys.notes;
    notes.extend(names.notes);
    notes.extend(stores.notes);
    notes.extend(bindings.notes);
    for fl in &doc.flows {
        let files: BTreeSet<usize> = fl
            .path
            .iter()
            .filter_map(|h| path_index.get(h.path.as_str()).copied())
            .collect();
        let systems: Vec<String> = sys
            .recs
            .iter()
            .zip(&sys.closure)
            .filter(|(_, inside)| files.iter().all(|f| inside[*f]))
            .map(|(r, _)| r.key.clone())
            .collect();
        if systems.is_empty() {
            let mut paths: Vec<String> = fl.path.iter().map(|h| h.path.clone()).collect();
            paths.dedup();
            notes.push(NoteRec {
                kind: "unattributed_flow".into(),
                detail: format!(
                    "flow {} crosses the code of more than one system, or of none",
                    fl.id
                ),
                paths,
            });
            continue;
        }
        attribution.push(AttributionRec {
            flow: fl.id.clone(),
            systems,
        });
    }
    notes.sort();
    notes.dedup();
    doc.topology = Some(Topology {
        catalogue: inp.topo.reference.clone(),
        mode: sys.mode,
        systems: sys.recs,
        datasets: stores.datasets,
        attribution,
        bindings: bindings.recs,
        edges: Vec::new(),
        declarations: Vec::new(),
        notes,
    });
    derive(doc);
}

/// What each system's request handlers receive: the seeds in handler functions (and request
/// input) within its closure.
fn user_input(inp: &Inputs, sys: &mut systems::Systems) {
    let p = inp.program;
    let handler = |mut f: crate::program::FuncId| -> bool {
        loop {
            if inp.facts.handler_funcs.binary_search(&f).is_ok() {
                return true;
            }
            match p.funcs[f as usize].parent {
                Some(parent) => f = parent,
                None => return false,
            }
        }
    };
    for (rec, closure) in sys.recs.iter_mut().zip(&sys.closure) {
        let mut categories: BTreeSet<String> = BTreeSet::new();
        let mut sites: BTreeSet<(String, u32)> = BTreeSet::new();
        let mut sources = 0u32;
        for seed in &inp.facts.seeds {
            if !closure[seed.file as usize] || seed.needs_review {
                continue;
            }
            let request = seed.kind == crate::facts::SeedKind::Request
                || handler(p.vars[seed.var as usize].func);
            if !request {
                continue;
            }
            sources += 1;
            categories.insert(seed.category.clone());
            sites.insert((p.files[seed.file as usize].path.clone(), seed.pos.line));
        }
        rec.user_input = model::UserInput {
            categories: categories.into_iter().collect(),
            sites: sites
                .into_iter()
                .take(5)
                .map(|(p, l)| format!("{p}:{l}"))
                .collect(),
            sources,
        };
    }
}

/// Recompute the edges from the flows, sinks, processors, attribution and bindings.
pub fn derive(doc: &mut Document) {
    let Some(mut topo) = doc.topology.take() else {
        return;
    };
    let sinks: BTreeMap<&str, &report::SinkRec> =
        doc.sinks.iter().map(|s| (s.id.as_str(), s)).collect();
    let flows: BTreeMap<&str, &report::FlowRec> =
        doc.flows.iter().map(|f| (f.id.as_str(), f)).collect();
    type Acc = (
        BTreeSet<String>,
        BTreeSet<String>,
        BTreeSet<String>,
        Option<String>,
        bool,
    );
    let mut acc: BTreeMap<(String, Endpoint, SinkClass), Acc> = BTreeMap::new();
    let mut bound: BTreeMap<(&str, &str), Vec<&Endpoint>> = BTreeMap::new();
    for b in &topo.bindings {
        bound
            .entry((b.sink.as_str(), b.system.as_str()))
            .or_default()
            .push(&b.to);
    }
    for a in &topo.attribution {
        let Some(fl) = flows.get(a.flow.as_str()) else {
            continue;
        };
        let Some(snk) = sinks.get(fl.sink.as_str()) else {
            continue;
        };
        if !snk.class.is_external() || report::is_first_party(snk) {
            continue;
        }
        let decl = report::processor_declared(&doc.processors, snk);
        let to = match decl
            .map(|d| d.name.clone())
            .or_else(|| report::processor_of(snk))
        {
            Some(name) => Endpoint::Processor {
                fides_key: decl
                    .and_then(|d| d.fides_key.clone())
                    .unwrap_or_else(|| report::fides_slug(&name)),
                name,
            },
            None => Endpoint::Unresolved {
                class: snk.class,
                path: snk.location.path.clone(),
            },
        };
        for s in &a.systems {
            let targets: Vec<&Endpoint> = match bound.get(&(snk.id.as_str(), s.as_str())) {
                Some(t) => t.clone(),
                None => vec![&to],
            };
            for t in targets {
                let e = acc.entry((s.clone(), t.clone(), snk.class)).or_default();
                e.0.insert(fl.category.clone());
                e.1.insert(fl.id.clone());
                e.2.insert(snk.id.clone());
                if let Some(d) = decl
                    && *t == to
                {
                    e.3.get_or_insert_with(|| d.citation.clone());
                    e.4 = true;
                }
            }
        }
    }
    topo.edges = acc
        .into_iter()
        .map(
            |((from, to, class), (categories, flows, sinks, citation, declared))| EdgeRec {
                from,
                to,
                class,
                categories: categories.into_iter().collect(),
                flows: flows.into_iter().collect(),
                sinks: sinks.into_iter().collect(),
                declared,
                citation,
            },
        )
        .collect();
    topo.declarations = declarations(&topo, catalogue::TopologyCatalogue::shared());
    doc.topology = Some(topo);
}

/// Group each system's evidence by the data use proposed for it.
fn declarations(topo: &Topology, cat: &catalogue::TopologyCatalogue) -> Vec<model::DeclarationRec> {
    use model::DeclarationRec;
    let pur = &cat.purposes;
    #[derive(Default)]
    struct Acc {
        basis: BTreeSet<String>,
        categories: BTreeSet<String>,
        egress: BTreeSet<Endpoint>,
        ingress: BTreeSet<String>,
        datasets: BTreeSet<String>,
    }
    let mut groups: BTreeMap<(String, String), Acc> = BTreeMap::new();
    let mut add = |system: &str, p: &catalogue::Purpose, basis: String| -> (String, String) {
        let key = (system.to_string(), p.data_use.clone());
        groups
            .entry(key.clone())
            .or_default()
            .basis
            .insert(format!("{basis}: {}", p.why));
        key
    };
    let categories = |cats: &[String]| -> Vec<String> {
        cats.iter()
            .filter(|c| c.as_str() != crate::classify::UNKNOWN)
            .cloned()
            .collect()
    };
    // (group, categories, egress endpoint, ingress key, dataset key) to add.
    type Planned = (
        (String, String),
        Vec<String>,
        Option<Endpoint>,
        Option<String>,
        Option<String>,
    );
    let mut planned: Vec<Planned> = Vec::new();
    for e in &topo.edges {
        let (p, basis) = match &e.to {
            Endpoint::Processor { name, .. } => match pur.by_processor.get(name) {
                Some(p) => (p, format!("purposes.yml#by_processor.{name}")),
                None => match pur.by_class.get(e.class.as_str()) {
                    Some(p) => (p, format!("purposes.yml#by_class.{}", e.class.as_str())),
                    None => continue,
                },
            },
            Endpoint::System { .. } => match pur.evidence.get("internal") {
                Some(p) => (p, "purposes.yml#evidence.internal".to_string()),
                None => continue,
            },
            Endpoint::Unresolved { .. } => match pur.evidence.get("unresolved") {
                Some(p) => (p, "purposes.yml#evidence.unresolved".to_string()),
                None => continue,
            },
        };
        let key = add(&e.from, p, basis);
        planned.push((
            key,
            categories(&e.categories),
            Some(e.to.clone()),
            None,
            None,
        ));
        // The receiving system of an internal edge receives it for the same reason.
        if let Endpoint::System { key: to } = &e.to {
            let k = add(to, p, "purposes.yml#evidence.internal".to_string());
            planned.push((
                k,
                categories(&e.categories),
                None,
                Some(e.from.clone()),
                None,
            ));
        }
    }
    for s in &topo.systems {
        if s.user_input.sources > 0
            && let Some(p) = pur.evidence.get("user_ingress")
        {
            let k = add(&s.key, p, "purposes.yml#evidence.user_ingress".to_string());
            planned.push((
                k,
                categories(&s.user_input.categories),
                None,
                Some("user".into()),
                None,
            ));
        }
        if let Some(p) = pur.evidence.get("datasets") {
            for ds in &s.datasets {
                let k = add(&s.key, p, "purposes.yml#evidence.datasets".to_string());
                let cats: Vec<String> = topo
                    .datasets
                    .iter()
                    .filter(|d| &d.key == ds)
                    .flat_map(|d| d.collections.iter())
                    .flat_map(|c| field_categories(&c.fields))
                    .collect();
                planned.push((k, cats, None, None, Some(ds.clone())));
            }
        }
    }
    for (key, cats, egress, ingress, dataset) in planned {
        let acc = groups.entry(key).or_default();
        acc.categories.extend(cats);
        acc.egress.extend(egress);
        acc.ingress.extend(ingress);
        acc.datasets.extend(dataset);
    }
    groups
        .into_iter()
        .map(|((system, data_use), acc)| DeclarationRec {
            system,
            name: pur
                .use_names
                .get(&data_use)
                .cloned()
                .unwrap_or_else(|| data_use.clone()),
            data_use,
            basis: acc.basis.into_iter().collect(),
            data_subjects: pur.default_data_subjects.clone(),
            data_categories: acc.categories.into_iter().collect(),
            egress: acc.egress.into_iter().collect(),
            ingress: acc.ingress.into_iter().collect(),
            datasets: acc.datasets.into_iter().collect(),
            needs_review: true,
        })
        .collect()
}

fn field_categories(fields: &[model::FieldRec]) -> Vec<String> {
    fields
        .iter()
        .flat_map(|f| {
            f.categories
                .iter()
                .cloned()
                .chain(field_categories(&f.fields))
        })
        .collect()
}
