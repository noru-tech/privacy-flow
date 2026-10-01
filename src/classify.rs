//! Field-name classification and the Fideslang vocabulary.
//!
//! Classification is an exact lookup in privacy-datamap's table
//! (`vendor/classification/classification.json`), never an inference: a name is personal data
//! of a category only when the table, a catalogue `field` source, the project's config or an
//! ingested data map says so. Names the table marks `maybe_pii` become sources of category
//! `unknown` that need review.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const CLASSIFICATION: &str = include_str!("../vendor/classification/classification.json");
const DATA_CATEGORIES: &str = include_str!("../vendor/taxonomy/data_categories.json");
const SPECIAL: &str = include_str!("../vendor/taxonomy/special_categories.json");

/// The category of data whose kind the analysis cannot name (request bodies, `maybe_pii`).
pub const UNKNOWN: &str = "unknown";

#[derive(Deserialize)]
struct Table {
    exact: BTreeMap<String, String>,
    maybe_pii: Vec<String>,
    operational: Vec<String>,
}

#[derive(Deserialize)]
struct TaxonomyEntry {
    fides_key: String,
}

#[derive(Deserialize)]
struct Special {
    fides_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub category: String,
    pub needs_review: bool,
    /// Where the classification came from: `classification-table`, a catalogue or config id,
    /// or `datamap:<file>`.
    pub by: String,
}

pub struct Classifier {
    exact: HashMap<String, String>,
    maybe: HashSet<String>,
    operational: HashSet<String>,
    special: Vec<String>,
    taxonomy: BTreeSet<String>,
    /// Extra names (catalogue `field` sources, config, data map): normalized name → (category, by).
    extra: BTreeMap<String, Vec<(String, String)>>,
    /// Names a project's config declares not personal.
    suppressed: HashSet<String>,
}

/// The table's own normalization: lowercase, every run of non-alphanumerics collapsed to `_`,
/// trimmed of leading and trailing `_`.
pub fn table_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('_');
            }
            pending = false;
            out.extend(c.to_lowercase());
        } else {
            pending = true;
        }
    }
    out
}

/// `firstName` → `first_name`, `IPAddress` → `ip_address`.
pub fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let prev_upper = i > 0 && chars[i - 1].is_uppercase();
            if i > 0 && (prev_lower || (prev_upper && next_lower)) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Candidate keys for a name, in lookup order.
pub fn keys(name: &str) -> Vec<String> {
    let a = table_key(name);
    let b = table_key(&snake(name));
    if a == b { vec![a] } else { vec![a, b] }
}

impl Classifier {
    pub fn new() -> Result<Classifier> {
        let table: Table =
            serde_json::from_str(CLASSIFICATION).context("vendored classification table")?;
        let taxonomy: Vec<TaxonomyEntry> =
            serde_json::from_str(DATA_CATEGORIES).context("vendored taxonomy")?;
        let special: Special =
            serde_json::from_str(SPECIAL).context("vendored special categories")?;
        Ok(Classifier {
            exact: table.exact.into_iter().collect(),
            maybe: table.maybe_pii.into_iter().collect(),
            operational: table.operational.into_iter().collect(),
            special: special.fides_keys,
            taxonomy: taxonomy.into_iter().map(|e| e.fides_key).collect(),
            extra: BTreeMap::new(),
            suppressed: HashSet::new(),
        })
    }

    /// Add a field name with a category, from a catalogue or config `field` source or a data map.
    pub fn add(&mut self, name: &str, category: &str, by: &str) -> Result<()> {
        self.check_category(category)
            .with_context(|| format!("field {name:?} from {by}"))?;
        for k in keys(name) {
            let e = self.extra.entry(k).or_default();
            let item = (category.to_string(), by.to_string());
            if !e.contains(&item) {
                e.push(item);
                e.sort();
            }
        }
        Ok(())
    }

    /// Declare a name not personal (config `not_personal`): the table no longer classifies it.
    pub fn not_personal(&mut self, name: &str) {
        for k in keys(name) {
            self.suppressed.insert(k);
        }
    }

    pub fn check_category(&self, category: &str) -> Result<()> {
        if category == UNKNOWN || self.taxonomy.contains(category) {
            Ok(())
        } else {
            bail!(
                "{category:?} is not a Fideslang data category in the vendored taxonomy (or `unknown`)"
            )
        }
    }

    /// Every category a field name carries. Extra sources come first and are all returned; the
    /// table answers only when nothing else does.
    pub fn classify(&self, name: &str) -> Vec<Classified> {
        let ks = keys(name);
        let mut out = Vec::new();
        for k in &ks {
            if let Some(list) = self.extra.get(k) {
                for (category, by) in list {
                    let c = Classified {
                        category: category.clone(),
                        needs_review: false,
                        by: by.clone(),
                    };
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
        if ks
            .iter()
            .any(|k| self.operational.contains(k) || self.suppressed.contains(k))
        {
            return out;
        }
        for k in &ks {
            if let Some(category) = self.exact.get(k) {
                return vec![Classified {
                    category: category.clone(),
                    needs_review: false,
                    by: "classification-table".into(),
                }];
            }
        }
        for k in &ks {
            if self.maybe.contains(k) {
                return vec![Classified {
                    category: UNKNOWN.into(),
                    needs_review: true,
                    by: "classification-table:maybe_pii".into(),
                }];
            }
        }
        out
    }

    pub fn is_special(&self, category: &str) -> bool {
        self.special
            .iter()
            .any(|k| category == k || category.starts_with(&format!("{k}.")))
    }

    pub fn is_credential(category: &str) -> bool {
        ["user.authorization", "system.authentication"]
            .iter()
            .any(|k| category == *k || category.starts_with(&format!("{k}.")))
    }

    pub fn is_taxonomy_key(&self, category: &str) -> bool {
        self.taxonomy.contains(category)
    }
}

/// True when `prefix` covers `category`: equal, a dotted ancestor, or `*`.
pub fn covers(prefix: &str, category: &str) -> bool {
    prefix == "*" || category == prefix || category.starts_with(&format!("{prefix}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_the_table() {
        assert_eq!(table_key("E-Mail"), "e_mail");
        assert_eq!(table_key("__email__"), "email");
        assert_eq!(snake("firstName"), "first_name");
        assert_eq!(snake("IPAddress"), "ip_address");
        assert_eq!(keys("emailAddress"), vec!["emailaddress", "email_address"]);
    }

    #[test]
    fn classifies_exact_names_only() {
        let c = Classifier::new().unwrap();
        assert_eq!(c.classify("email")[0].category, "user.contact.email");
        assert_eq!(c.classify("firstName")[0].category, "user.name.first");
        assert!(c.classify("id").is_empty());
        assert!(c.classify("updated_at").is_empty());
        let maybe = c.classify("address");
        assert_eq!(maybe[0].category, UNKNOWN);
        assert!(maybe[0].needs_review);
    }

    #[test]
    fn special_and_credentials() {
        let c = Classifier::new().unwrap();
        assert!(c.is_special("user.health_and_medical.genetic"));
        assert!(!c.is_special("user.contact.email"));
        assert!(Classifier::is_credential("user.authorization.password"));
        assert!(covers("user", "user.contact.email"));
        assert!(!covers("user.contact", "user.contacts"));
    }
}
