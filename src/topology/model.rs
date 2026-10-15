//! The topology section of a flow-facts document: the systems of a repository, which of them
//! each flow belongs to, and the edges the data map is written from. Only `piiflow datamap`
//! writes it.

use serde::{Deserialize, Serialize};

use crate::catalogue::SinkClass;
use crate::report::CatalogueRef;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    pub catalogue: CatalogueRef,
    /// `detected`, `config`, `mixed` (declared and detected systems) or `repository` (no
    /// deployable found: the repository is one system).
    pub mode: String,
    pub systems: Vec<SystemRec>,
    /// FACT: the systems each flow belongs to, by flow ID.
    pub attribution: Vec<AttributionRec>,
    /// DERIVED from the flows, sinks, processors and attribution.
    pub edges: Vec<EdgeRec>,
    pub notes: Vec<NoteRec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SystemRec {
    pub key: String,
    pub name: String,
    /// `service`, `worker`, `application`, `function`, `web_client` or `repository`.
    pub kind: String,
    pub language: Option<String>,
    /// The root directory, `""` for the repository root.
    pub root: String,
    /// The project file or manifest that makes it a system, if any.
    pub manifest: Option<String>,
    /// Declared under `systems:` in `.privacy-flow.yml`.
    pub declared: bool,
    /// Files of its own, and with the libraries it references.
    pub files: u32,
    pub closure_files: u32,
    /// Directories of the library units it references.
    pub libraries: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// The catalogue entry or config key that matched.
    pub rule: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AttributionRec {
    pub flow: String,
    pub systems: Vec<String>,
}

/// Where an edge goes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Endpoint {
    /// Another system of the repository.
    System { key: String },
    /// A processor: named by the catalogue, a declaration or a literal host.
    Processor { name: String, fides_key: String },
    /// A destination the code does not name, by sink class and the file that sends.
    Unresolved { class: SinkClass, path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdgeRec {
    pub from: String,
    pub to: Endpoint,
    pub class: SinkClass,
    pub categories: Vec<String>,
    pub flows: Vec<String>,
    pub sinks: Vec<String>,
    pub declared: bool,
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct NoteRec {
    /// `unattributed_flow`, `ignored_system_config`, …
    pub kind: String,
    pub detail: String,
    pub paths: Vec<String>,
}
