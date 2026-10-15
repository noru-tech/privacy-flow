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
    /// What the systems store, from their ORM models.
    pub datasets: Vec<DatasetRec>,
    /// FACT: the systems each flow belongs to, by flow ID.
    pub attribution: Vec<AttributionRec>,
    /// FACT: requests bound to a client registration, per system: where they go.
    pub bindings: Vec<BindingRec>,
    /// DERIVED from the flows, sinks, processors and attribution.
    pub edges: Vec<EdgeRec>,
    /// DERIVED: proposed privacy declarations, one per (system, proposed data use). Proposals,
    /// never decisions: every one needs review.
    pub declarations: Vec<DeclarationRec>,
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
    /// Host names other systems reach it by, from orchestrators and `systems:`.
    pub aliases: Vec<String>,
    /// What its request handlers receive from users.
    pub user_input: UserInput,
    /// The datasets it uses, by key.
    pub datasets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DatasetRec {
    pub key: String,
    pub name: String,
    /// The catalogue entry that read it.
    pub store: String,
    /// `path:line` of the context or the first model.
    pub location: String,
    /// The database a registration names (`"orderingdb"`).
    pub database: Option<String>,
    pub schema: Option<String>,
    pub collections: Vec<CollectionRec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollectionRec {
    pub name: String,
    /// The entity or model class.
    pub entity: String,
    pub location: String,
    pub fields: Vec<FieldRec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldRec {
    /// The column.
    pub name: String,
    /// The member it maps, when it is not the column name.
    pub member: Option<String>,
    pub location: Option<String>,
    pub categories: Vec<String>,
    /// Classified only as maybe personal: no category until reviewed.
    pub needs_review: bool,
    pub classified_by: Option<String>,
    /// Owned or embedded fields.
    pub fields: Vec<FieldRec>,
}

/// The data a system's request handlers receive: categories, and where (first five sites).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserInput {
    pub categories: Vec<String>,
    pub sites: Vec<String>,
    pub sources: u32,
}

/// A request sent through a registered client, in one system.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct BindingRec {
    pub sink: String,
    pub system: String,
    /// The catalogue entry of the registration.
    pub client: String,
    /// `path:line` of the registering call.
    pub registration: String,
    /// The base address it sets.
    pub base: String,
    pub to: Endpoint,
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

/// A proposed privacy declaration: a data use for a group of a system's evidence.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclarationRec {
    pub system: String,
    /// The data use's name in the taxonomy (`Analytics for Reporting`).
    pub name: String,
    pub data_use: String,
    /// Where the proposal comes from (`purposes.yml#by_class.analytics`) and why.
    pub basis: Vec<String>,
    pub data_subjects: Vec<String>,
    pub data_categories: Vec<String>,
    pub egress: Vec<Endpoint>,
    /// System keys, or `user`.
    pub ingress: Vec<String>,
    pub datasets: Vec<String>,
    /// Always true: a person decides.
    pub needs_review: bool,
}
