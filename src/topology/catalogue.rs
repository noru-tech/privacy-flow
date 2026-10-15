//! `catalogue/topology/`: what the data map needs to know about deployables, clients, stores and
//! purposes. Versioned and digested apart from the flow catalogue, so `scan` output does not
//! depend on it.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::report::CatalogueRef;

pub const FILES: &[(&str, &str)] = &[(
    "systems.yml",
    include_str!("../../catalogue/topology/systems.yml"),
)];

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: String,
    #[serde(default)]
    deployables: Vec<Deployable>,
    #[serde(default)]
    never: Vec<Deployable>,
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
}

impl TopologyCatalogue {
    pub fn builtin() -> Result<TopologyCatalogue> {
        let mut version: Option<String> = None;
        let mut hasher_input = String::new();
        let mut deployables = Vec::new();
        let mut never = Vec::new();
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
        }
        for d in deployables.iter().chain(&never) {
            if d.citation.trim().len() < 3 {
                bail!("catalogue/topology: {} needs a citation", d.id);
            }
        }
        Ok(TopologyCatalogue {
            reference: CatalogueRef {
                version: version.unwrap_or_default(),
                digest: crate::canonical::sha256(hasher_input.as_bytes()),
            },
            deployables,
            never,
        })
    }
}
