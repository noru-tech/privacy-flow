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
pub fn document(run: Run, config: &Config, topo: &TopologyCatalogue) -> Result<Document> {
    let mut doc = run.document;
    crate::topology::build(
        &mut doc,
        &crate::topology::systems::Inputs {
            program: &run.program,
            resolver: &run.resolver,
            files: &run.files,
            config,
            catalogue: &run.catalogue,
            topo,
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
