//! The repository's systems and the edges between them and their processors, for
//! `piiflow datamap` (ADR 0018). `scan` and `diff` never run this: their output is unchanged.

pub mod catalogue;
pub mod model;
pub mod systems;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalogue::SinkClass;
use crate::report::{self, Document};
use model::{AttributionRec, EdgeRec, Endpoint, NoteRec, Topology};

/// Detect the systems, attribute every flow, and derive the edges into `doc.topology`.
pub fn build(doc: &mut Document, inp: &systems::Inputs) {
    let sys = systems::detect(inp);
    let path_index: BTreeMap<&str, usize> = inp
        .program
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();
    let mut attribution = Vec::new();
    let mut notes = sys.notes;
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
        attribution,
        edges: Vec::new(),
        notes,
    });
    derive(doc);
}

/// Recompute the edges from the flows, sinks, processors and attribution.
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
            let e = acc.entry((s.clone(), to.clone(), snk.class)).or_default();
            e.0.insert(fl.category.clone());
            e.1.insert(fl.id.clone());
            e.2.insert(snk.id.clone());
            if let Some(d) = decl {
                e.3.get_or_insert_with(|| d.citation.clone());
                e.4 = true;
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
    doc.topology = Some(topo);
}
