//! `.privacy-flow.yml`: project configuration, validated against `schemas/config.schema.json`.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::catalogue::{Extensions, PropagatorDef, SanitiserDef, SinkDef, SourceDef};
use crate::classify::Classifier;
use crate::glob::Glob;

pub const FILE: &str = ".privacy-flow.yml";
pub const SCHEMA: &str = include_str!("../schemas/config.schema.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Medium,
    High,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Medium => "medium",
            Severity::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Severity> {
        match s {
            "info" => Some(Severity::Info),
            "warning" => Some(Severity::Warning),
            "medium" => Some(Severity::Medium),
            "high" => Some(Severity::High),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SystemDecl {
    pub fides_key: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldDecl {
    pub name: String,
    pub category: String,
    pub citation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NotPersonalDecl {
    pub name: String,
    pub citation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProcessorDecl {
    pub name: String,
    #[serde(default)]
    pub fides_key: Option<String>,
    #[serde(default)]
    pub sinks: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    pub citation: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RulePolicy {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub severity: Option<Severity>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecl {
    #[serde(default)]
    pub fail_on: Option<Severity>,
    #[serde(default)]
    pub rules: BTreeMap<String, RulePolicy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DatamapSetting {
    Off(bool),
    Paths(Vec<String>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub system: Option<SystemDecl>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "yes")]
    pub default_excludes: bool,
    #[serde(default)]
    pub datamap: Option<DatamapSetting>,
    #[serde(default)]
    pub max_call_depth: Option<u32>,
    #[serde(default)]
    pub fields: Vec<FieldDecl>,
    #[serde(default)]
    pub not_personal: Vec<NotPersonalDecl>,
    #[serde(default)]
    pub sources: Vec<SourceDef>,
    #[serde(default)]
    pub sinks: Vec<SinkDef>,
    #[serde(default)]
    pub sanitisers: Vec<SanitiserDef>,
    #[serde(default)]
    pub propagators: Vec<PropagatorDef>,
    #[serde(default)]
    pub processors: Vec<ProcessorDecl>,
    #[serde(default)]
    pub policy: PolicyDecl,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            system: None,
            exclude: Vec::new(),
            default_excludes: true,
            datamap: None,
            max_call_depth: None,
            fields: Vec::new(),
            not_personal: Vec::new(),
            sources: Vec::new(),
            sinks: Vec::new(),
            sanitisers: Vec::new(),
            propagators: Vec::new(),
            processors: Vec::new(),
            policy: PolicyDecl::default(),
        }
    }
}

/// Validate a document against an embedded JSON Schema, with every error listed.
pub fn validate_schema(schema_text: &str, doc: &serde_json::Value, what: &str) -> Result<()> {
    let schema: serde_json::Value =
        serde_json::from_str(schema_text).expect("embedded schema is JSON");
    let validator = jsonschema::validator_for(&schema).expect("embedded schema compiles");
    let errors: Vec<String> = validator
        .iter_errors(doc)
        .map(|e| {
            let at = e.instance_path().to_string();
            format!(
                "{}: {}",
                if at.is_empty() { "/".to_string() } else { at },
                e
            )
        })
        .collect();
    if !errors.is_empty() {
        bail!(
            "{what} does not conform to its schema:\n  {}",
            errors.join("\n  ")
        );
    }
    Ok(())
}

impl Config {
    pub fn parse(text: &str, path: &str) -> Result<Config> {
        let value: serde_json::Value =
            serde_saphyr::from_str(text).with_context(|| format!("{path}: not valid YAML"))?;
        validate_schema(SCHEMA, &value, path)?;
        let config: Config = serde_json::from_value(value)
            .with_context(|| format!("{path}: does not match the configuration format"))?;
        for g in &config.exclude {
            Glob::path(g).with_context(|| format!("{path}: exclude pattern {g:?}"))?;
        }
        Ok(config)
    }

    pub fn extensions(&self) -> Extensions {
        Extensions {
            sinks: self.sinks.clone(),
            sources: self.sources.clone(),
            sanitisers: self.sanitisers.clone(),
            propagators: self.propagators.clone(),
        }
    }

    /// Check categories against the vendored taxonomy and register declared fields.
    pub fn apply_fields(&self, classifier: &mut Classifier) -> Result<()> {
        for f in &self.fields {
            classifier.add(&f.name, &f.category, &format!("config:{}", f.name))?;
        }
        for n in &self.not_personal {
            classifier.not_personal(&n.name);
        }
        for s in &self.sources {
            if let Some(c) = &s.category {
                classifier
                    .check_category(c)
                    .with_context(|| format!("source {}", s.id))?;
            }
        }
        for s in &self.sanitisers {
            for r in &s.removes {
                if r != "*" {
                    classifier
                        .check_category(r)
                        .with_context(|| format!("sanitiser {}", s.id))?;
                }
            }
        }
        if let Some(r) = self.policy.rules.get("PFC01")
            && !r.enabled
        {
            bail!(
                "policy.rules.PFC01 cannot be disabled: a coverage gap can never be turned into a clean result (record a disposition on the gap's finding instead)"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates() {
        let c = Config::parse(
            "version: 1\nprocessors:\n  - name: PostHog\n    citation: DPA 2026-03\npolicy:\n  fail_on: high\n  rules:\n    PF001: { severity: high }\n",
            "x",
        )
        .unwrap();
        assert_eq!(c.processors[0].name, "PostHog");
        assert_eq!(c.policy.fail_on, Some(Severity::High));
        assert!(Config::parse("version: 2\n", "x").is_err());
        assert!(Config::parse("version: 1\nprocessors: [{ name: X }]\n", "x").is_err());
        assert!(Config::parse("version: 1\nbogus: true\n", "x").is_err());
    }

    #[test]
    fn pfc01_cannot_be_disabled() {
        let c = Config::parse(
            "version: 1\npolicy:\n  rules:\n    PFC01: { enabled: false }\n",
            "x",
        )
        .unwrap();
        let mut cl = Classifier::new().unwrap();
        assert!(c.apply_fields(&mut cl).is_err());
    }
}
