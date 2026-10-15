//! Fides data maps: reading existing ones to seed classification (`ingest`), and writing the
//! repository's full data map for `piiflow datamap` (`generate`, rendered by `yaml`).

pub mod generate;
pub mod ingest;
pub mod yaml;

pub use ingest::*;

use anyhow::Result;

use crate::analyze::Run;
use crate::config::Config;
use crate::report::{self, Document};
use crate::topology::catalogue::TopologyCatalogue;

/// The flow-facts document of a run, with the repository's topology, sealed.
pub fn document(
    tree: &crate::analyze::Tree,
    run: Run,
    config: &Config,
    topo: &TopologyCatalogue,
) -> Result<Document> {
    let compose: Vec<(String, String)> = run
        .files
        .iter()
        .filter(|f| crate::topology::wants(f))
        .filter_map(|f| tree.read_text(f).map(|t| (f.clone(), t)))
        .collect();
    let mut doc = run.document;
    crate::topology::build(
        &mut doc,
        &crate::topology::Inputs {
            program: &run.program,
            facts: &run.facts,
            resolver: &run.resolver,
            files: &run.files,
            config,
            catalogue: &run.catalogue,
            topo,
            compose: &compose,
        },
    );
    report::seal(&mut doc)?;
    Ok(doc)
}

/// The data map the code shows, as YAML, before any merge.
pub fn render_generated(doc: &Document) -> String {
    let header: Vec<String> = generate::HEADER.iter().map(|s| s.to_string()).collect();
    yaml::render(&header, &generate::generate(doc))
}
