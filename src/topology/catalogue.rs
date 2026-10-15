//! `catalogue/topology/`: what the data map needs to know about deployables, clients, stores and
//! purposes. Versioned and digested apart from the flow catalogue, so `scan` output does not
//! depend on it.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::report::CatalogueRef;

pub const FILES: &[(&str, &str)] = &[
    (
        "clients.yml",
        include_str!("../../catalogue/topology/clients.yml"),
    ),
    (
        "stores.yml",
        include_str!("../../catalogue/topology/stores.yml"),
    ),
    (
        "systems.yml",
        include_str!("../../catalogue/topology/systems.yml"),
    ),
];

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: String,
    #[serde(default)]
    deployables: Vec<Deployable>,
    #[serde(default)]
    never: Vec<Deployable>,
    #[serde(default)]
    orchestrators: Vec<Orchestrator>,
    #[serde(default)]
    clients: Vec<ClientDef>,
    #[serde(default)]
    stores: Vec<StoreDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    EntityFramework,
    Django,
}

/// How ORM models of one kind say what is stored (`catalogue/topology/stores.yml`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreDef {
    pub id: String,
    pub language: String,
    pub kind: StoreKind,
    // entity_framework
    #[serde(default)]
    pub contexts: Vec<String>,
    #[serde(default)]
    pub sets: Vec<String>,
    #[serde(default)]
    pub configurations: Vec<String>,
    #[serde(default)]
    pub not_mapped: Vec<String>,
    #[serde(default)]
    pub table_attributes: Vec<String>,
    #[serde(default)]
    pub column_attributes: Vec<String>,
    #[serde(default)]
    pub owned_attributes: Vec<String>,
    #[serde(default)]
    pub model_method: Option<String>,
    #[serde(default)]
    pub fluent: Fluent,
    #[serde(default)]
    pub registrations: Vec<String>,
    /// Calls inside a registration's lambda that name the database (`GetConnectionString`).
    #[serde(default)]
    pub database_calls: Vec<String>,
    #[serde(default)]
    pub base_collections: Vec<BaseCollections>,
    // django
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub meta_class: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub not_columns: Vec<String>,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(default)]
    pub column_keyword: Option<String>,
    #[serde(default)]
    pub inherited: Vec<Inherited>,
    pub citation: String,
}

/// Method names of a fluent configuration API.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fluent {
    #[serde(default)]
    pub entity: Vec<String>,
    #[serde(default)]
    pub table: Vec<String>,
    #[serde(default)]
    pub column: Vec<String>,
    #[serde(default)]
    pub property: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub owned: Vec<String>,
    #[serde(default)]
    pub schema: Vec<String>,
    #[serde(default)]
    pub apply: Vec<String>,
    #[serde(default)]
    pub apply_all: Vec<String>,
}

/// Tables a context base class brings.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseCollections {
    pub context: String,
    pub collections: Vec<BaseCollection>,
    pub citation: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseCollection {
    pub name: String,
    /// The base's type argument that is the entity, whose own members add to `fields`.
    #[serde(default)]
    pub entity_type_arg: Option<usize>,
    pub fields: Vec<String>,
}

/// Fields a library base class gives its subclasses.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inherited {
    pub base: String,
    pub fields: Vec<String>,
}

/// A call that adds a project to an application model under a host name.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orchestrator {
    pub id: String,
    pub language: String,
    pub calls: Vec<String>,
    pub name_arg: usize,
    #[serde(default)]
    pub path_arg: Option<usize>,
    #[serde(default)]
    pub type_arg_namespace: Option<String>,
    pub citation: String,
}

/// A client registration: where a client's base address is set.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientDef {
    pub id: String,
    pub language: String,
    pub calls: Vec<String>,
    pub client: ClientKind,
    #[serde(default)]
    pub injects: Vec<String>,
    pub configure: Configure,
    #[serde(default)]
    pub wrappers: Vec<String>,
    pub citation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    TypeArg,
    Result,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configure {
    #[serde(default)]
    pub lambda_fields: Vec<String>,
    #[serde(default)]
    pub arg: Option<usize>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub keyword: Option<String>,
}

/// A kind of unit that is a deployable service, or (under `never`) one that is not.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployable {
    pub id: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub project: Option<ProjectMatch>,
    #[serde(default)]
    pub package: Option<PackageMatch>,
    #[serde(default)]
    pub marker_files: Vec<String>,
    #[serde(default)]
    pub module_values: Vec<String>,
    #[serde(default)]
    pub root_markers: Vec<String>,
    pub citation: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMatch {
    #[serde(default)]
    pub sdk: Vec<String>,
    #[serde(default)]
    pub output_type: Vec<String>,
    #[serde(default)]
    pub package_refs: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMatch {
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub framework_dependency: bool,
    #[serde(default)]
    pub scripts: Vec<String>,
}

pub struct TopologyCatalogue {
    pub reference: CatalogueRef,
    pub deployables: Vec<Deployable>,
    pub never: Vec<Deployable>,
    pub orchestrators: Vec<Orchestrator>,
    pub clients: Vec<ClientDef>,
    pub stores: Vec<StoreDef>,
}

impl TopologyCatalogue {
    pub fn builtin() -> Result<TopologyCatalogue> {
        let mut version: Option<String> = None;
        let mut hasher_input = String::new();
        let mut deployables = Vec::new();
        let mut never = Vec::new();
        let mut orchestrators = Vec::new();
        let mut clients = Vec::new();
        let mut stores = Vec::new();
        for (name, text) in FILES {
            let file: File = serde_saphyr::from_str(text)
                .with_context(|| format!("catalogue/topology/{name}"))?;
            match &version {
                None => version = Some(file.version.clone()),
                Some(v) if *v != file.version => bail!(
                    "catalogue/topology/{name} has version {} but others have {v}",
                    file.version
                ),
                _ => {}
            }
            hasher_input.push_str(name);
            hasher_input.push('\n');
            hasher_input.push_str(text);
            deployables.extend(file.deployables);
            never.extend(file.never);
            orchestrators.extend(file.orchestrators);
            clients.extend(file.clients);
            stores.extend(file.stores);
        }
        let citations = deployables
            .iter()
            .chain(&never)
            .map(|d| (&d.id, &d.citation))
            .chain(orchestrators.iter().map(|o| (&o.id, &o.citation)))
            .chain(clients.iter().map(|c| (&c.id, &c.citation)))
            .chain(stores.iter().map(|c| (&c.id, &c.citation)));
        for (id, citation) in citations {
            if citation.trim().len() < 3 {
                bail!("catalogue/topology: {id} needs a citation");
            }
        }
        Ok(TopologyCatalogue {
            reference: CatalogueRef {
                version: version.unwrap_or_default(),
                digest: crate::canonical::sha256(hasher_input.as_bytes()),
            },
            deployables,
            never,
            orchestrators,
            clients,
            stores,
        })
    }
}
