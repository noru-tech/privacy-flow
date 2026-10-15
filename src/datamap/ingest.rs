//! Ingest an existing data map so that column classifications made there flow into the
//! analysis.
//!
//! Two shapes are read, both with the Fides dataset structure (`collections` → `fields`, nested
//! `fields`, `data_categories`):
//!
//! - a Fides data map, `.fides/datamap.yml` (top-level `dataset:` list), which is what
//!   privacy-datamap renders once its manifest validates;
//! - privacy-datamap's derived facts, `.noru/.cache/privacy-datamap.derived.json` (top-level
//!   `datasets:` list).
//!
//! A field's categories apply to reads of that field name anywhere, and to typed objects whose
//! type name matches the collection name (`User` ↔ `users`).

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub const DEFAULT_PATHS: &[&str] = &[
    ".fides/datamap.yml",
    ".noru/.cache/privacy-datamap.derived.json",
];

#[derive(Clone, Debug, Default)]
pub struct Datamap {
    /// Path relative to the scan root, for citations.
    pub path: String,
    /// collection name → (field name → categories)
    pub collections: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

impl Datamap {
    pub fn parse(path: &str, text: &str) -> Result<Datamap> {
        let value: Value = if path.ends_with(".json") {
            serde_json::from_str(text).with_context(|| format!("{path}: not JSON"))?
        } else {
            serde_saphyr::from_str(text).with_context(|| format!("{path}: not YAML"))?
        };
        let datasets = value
            .get("dataset")
            .or_else(|| value.get("datasets"))
            .and_then(Value::as_array)
            .with_context(|| format!("{path}: no `dataset` or `datasets` list"))?;
        let mut dm = Datamap {
            path: path.to_string(),
            collections: BTreeMap::new(),
        };
        for ds in datasets {
            let Some(collections) = ds.get("collections").and_then(Value::as_array) else {
                continue;
            };
            for c in collections {
                let Some(name) = c.get("name").and_then(Value::as_str) else {
                    bail!("{path}: a collection has no name");
                };
                let entry = dm.collections.entry(name.to_string()).or_default();
                if let Some(fields) = c.get("fields").and_then(Value::as_array) {
                    collect_fields(fields, entry);
                }
            }
        }
        Ok(dm)
    }

    /// Every (field, category) pair.
    pub fn all_fields(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for fields in self.collections.values() {
            for (f, cats) in fields {
                for c in cats {
                    out.push((f.clone(), c.clone()));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The (field, category) pairs of the collection a type name corresponds to.
    pub fn fields_of(&self, type_name: &str) -> Vec<(String, String)> {
        let want = normalize_collection(type_name);
        let mut out = Vec::new();
        for (name, fields) in &self.collections {
            if normalize_collection(name) != want {
                continue;
            }
            for (f, cats) in fields {
                for c in cats {
                    out.push((f.clone(), c.clone()));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

fn collect_fields(fields: &[Value], out: &mut BTreeMap<String, Vec<String>>) {
    for f in fields {
        let Some(name) = f.get("name").and_then(Value::as_str) else {
            continue;
        };
        let cats: Vec<String> = f
            .get("data_categories")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if !cats.is_empty() {
            let e = out.entry(name.to_string()).or_default();
            e.extend(cats);
            e.sort();
            e.dedup();
        }
        if let Some(nested) = f.get("fields").and_then(Value::as_array) {
            collect_fields(nested, out);
        }
    }
}

/// `Users`, `users`, `user`, `User` → `user`; `customer_orders` → `customerorder`.
fn normalize_collection(name: &str) -> String {
    let mut s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    if s.ends_with("ies") {
        s.truncate(s.len() - 3);
        s.push('y');
    } else if s.ends_with('s') && !s.ends_with("ss") {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fides_and_derived() {
        let yml = "dataset:\n  - fides_key: app\n    collections:\n      - name: users\n        fields:\n          - name: email\n            data_categories: [user.contact.email]\n          - name: address\n            fields:\n              - name: city\n                data_categories: [user.contact.address.city]\n";
        let dm = Datamap::parse(".fides/datamap.yml", yml).unwrap();
        assert_eq!(
            dm.fields_of("User"),
            vec![
                ("city".to_string(), "user.contact.address.city".to_string()),
                ("email".to_string(), "user.contact.email".to_string()),
            ]
        );
        let json = r#"{"datasets":[{"fides_key":"x","collections":[{"name":"Customer","fields":[{"name":"phone","data_categories":["user.contact.phone_number"]}]}]}]}"#;
        let dm = Datamap::parse("d.json", json).unwrap();
        assert_eq!(dm.fields_of("customers").len(), 1);
        assert!(Datamap::parse("x.yml", "a: 1").is_err());
    }
}
