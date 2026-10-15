//! A privacy-datamap manifest (`.noru/privacy-datamap.yml`), read-only: the decisions a person
//! signed there win over what the code proposes and over edits of the data map.
//!
//! A collection or declaration with an `interpretation` and no `needs_review: true` is a
//! decision. A decided collection's field categories (and `non_personal_fields`) replace the
//! generated ones of the collection of the same name; a decided declaration replaces the
//! proposal of its system with the same name or data use, or joins the system. Datasets and
//! systems only the manifest has are included. Bookkeeping keys are dropped.

use crate::datamap::yaml::Node;

/// Keys privacy-datamap keeps for itself.
const BOOKKEEPING: &[&str] = &[
    "refs",
    "interpretation",
    "needs_review",
    "non_personal_fields",
    "structure_digest",
];

pub const DEFAULT_PATH: &str = ".noru/privacy-datamap.yml";

fn get<'a>(n: &'a Node, key: &str) -> Option<&'a Node> {
    match n.bare() {
        Node::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v.bare()),
        _ => None,
    }
}

fn get_mut<'a>(n: &'a mut Node, key: &str) -> Option<&'a mut Node> {
    match n {
        Node::Noted(_, inner) => get_mut(inner, key),
        Node::Map(m) => m.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn str_of(n: Option<&Node>) -> Option<&str> {
    match n? {
        Node::Str(s) => Some(s),
        _ => None,
    }
}

fn items(n: Option<&Node>) -> Vec<&Node> {
    match n {
        Some(Node::List(l)) => l.iter().collect(),
        _ => Vec::new(),
    }
}

fn items_mut(n: Option<&mut Node>) -> Vec<&mut Node> {
    match n {
        Some(Node::Noted(_, inner)) => items_mut(Some(inner)),
        Some(Node::List(l)) => l.iter_mut().collect(),
        _ => Vec::new(),
    }
}

fn strings(n: Option<&Node>) -> Vec<String> {
    items(n)
        .into_iter()
        .filter_map(|x| str_of(Some(x.bare())).map(str::to_string))
        .collect()
}

/// A decision: interpreted, and not waiting for review.
fn decided(n: &Node) -> bool {
    get(n, "interpretation").is_some() && !matches!(get(n, "needs_review"), Some(Node::Bool(true)))
}

/// `expires_at` before `as_of` (ISO dates compare as strings).
fn expired(n: &Node, as_of: Option<&str>) -> Option<String> {
    let at = str_of(get(get(n, "interpretation")?, "expires_at"))?;
    let as_of = as_of?;
    (at < as_of).then(|| at.to_string())
}

/// The node without privacy-datamap's bookkeeping, recursively.
fn strip(n: &Node) -> Node {
    match n.bare() {
        Node::Map(m) => Node::Map(
            m.iter()
                .filter(|(k, _)| !BOOKKEEPING.contains(&k.as_str()))
                .filter_map(|(k, v)| {
                    if k == "meta" {
                        // `meta.noru` is privacy-datamap's.
                        let kept = match v.bare() {
                            Node::Map(mm) => Node::Map(
                                mm.iter().filter(|(kk, _)| kk != "noru").cloned().collect(),
                            ),
                            other => other.clone(),
                        };
                        if matches!(&kept, Node::Map(x) if x.is_empty()) {
                            return None;
                        }
                        return Some((k.clone(), kept));
                    }
                    Some((k.clone(), strip(v)))
                })
                .collect(),
        ),
        Node::List(l) => Node::List(l.iter().map(strip).collect()),
        other => other.clone(),
    }
}

/// privacy-datamap's key for a system root: `apps/api` → `apps_api`, the root `repository`.
pub fn key_for_root(root: &str) -> String {
    if root.is_empty() {
        return "repository".into();
    }
    root.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Apply the manifest's decisions to a data map tree. `roots` maps privacy-datamap's root keys
/// to system keys. Returns notes for the report.
pub fn apply(
    tree: &mut Node,
    manifest: &Node,
    roots: &[(String, String)],
    as_of: Option<&str>,
) -> Vec<String> {
    let mut notes = Vec::new();
    let Node::Map(root) = tree.bare_mut() else {
        return notes;
    };
    if !root.iter().any(|(k, _)| k == "dataset") {
        root.insert(0, ("dataset".into(), Node::List(Vec::new())));
    }
    if !root.iter().any(|(k, _)| k == "system") {
        root.push(("system".into(), Node::List(Vec::new())));
    }

    // Collections.
    for mds in items(get(manifest, "dataset")) {
        let mkey = str_of(get(mds, "fides_key")).unwrap_or("").to_string();
        let mut matched_any = false;
        for mc in items(get(mds, "collections")) {
            let name = str_of(get(mc, "name")).unwrap_or("").to_string();
            if !decided(mc) {
                continue;
            }
            if let Some(at) = expired(mc, as_of) {
                notes.push(format!(
                    "manifest decision on collection {name} expired {at}; still applied, review it in privacy-datamap"
                ));
            }
            let non_personal = strings(get(mc, "non_personal_fields"));
            let datasets = root
                .iter_mut()
                .find(|(k, _)| k == "dataset")
                .map(|(_, v)| v);
            for ds in items_mut(datasets) {
                for c in items_mut(get_mut(ds, "collections")) {
                    if !str_of(get(c, "name")).is_some_and(|n| n.eq_ignore_ascii_case(&name)) {
                        continue;
                    }
                    matched_any = true;
                    let Some(fields) = get_mut(c, "fields") else {
                        continue;
                    };
                    decide_fields(fields, get(mc, "fields"), &non_personal);
                    notes.push(format!(
                        "collection {name}: categories as decided in the privacy-datamap manifest"
                    ));
                }
            }
        }
        if !matched_any {
            // A dataset only the manifest knows (its store is not an ORM piiflow reads).
            let datasets = root
                .iter_mut()
                .find(|(k, _)| k == "dataset")
                .map(|(_, v)| v);
            if let Some(Node::List(l)) = datasets.map(Node::bare_mut)
                && !l
                    .iter()
                    .any(|d| str_of(get(d, "fides_key")) == Some(mkey.as_str()))
            {
                l.push(Node::noted(
                    vec!["from the privacy-datamap manifest".into()],
                    strip(mds),
                ));
                notes.push(format!("dataset {mkey}: from the privacy-datamap manifest"));
            }
        }
    }

    // Declarations.
    for msys in items(get(manifest, "system")) {
        let mkey = str_of(get(msys, "fides_key")).unwrap_or("").to_string();
        let key = roots
            .iter()
            .find(|(r, _)| *r == mkey)
            .map(|(_, k)| k.clone())
            .unwrap_or_else(|| mkey.clone());
        let systems = root.iter_mut().find(|(k, _)| k == "system").map(|(_, v)| v);
        let Some(Node::List(systems)) = systems.map(Node::bare_mut) else {
            continue;
        };
        let Some(sys) = systems
            .iter_mut()
            .find(|s| str_of(get(s, "fides_key")) == Some(key.as_str()))
        else {
            systems.push(Node::noted(
                vec!["from the privacy-datamap manifest".into()],
                strip(msys),
            ));
            notes.push(format!("system {mkey}: from the privacy-datamap manifest"));
            continue;
        };
        for md in items(get(msys, "privacy_declarations")) {
            if !decided(md) {
                continue;
            }
            let name = str_of(get(md, "name")).unwrap_or("").to_string();
            let data_use = str_of(get(md, "data_use")).unwrap_or("").to_string();
            if let Some(at) = expired(md, as_of) {
                notes.push(format!(
                    "manifest decision {key}: {name} expired {at}; still applied, review it in privacy-datamap"
                ));
            }
            let decided_md = strip(md);
            let mut replaced: Option<String> = None;
            let decls = get_mut(sys, "privacy_declarations");
            if let Some(Node::List(list)) = decls.map(Node::bare_mut) {
                if let Some(d) = list.iter_mut().find(|d| {
                    str_of(get(d, "name")) == Some(name.as_str())
                        || str_of(get(d, "data_use")) == Some(data_use.as_str())
                }) {
                    replaced = str_of(get(d, "name")).map(str::to_string);
                    if let (Node::Map(dm), Node::Map(src)) = (d.bare_mut(), &decided_md) {
                        for (k, v) in src {
                            match dm.iter_mut().find(|(kk, _)| kk == k) {
                                Some(slot) => slot.1 = v.clone(),
                                None => dm.push((k.clone(), v.clone())),
                            }
                        }
                    }
                } else {
                    list.push(Node::noted(
                        vec!["decided in the privacy-datamap manifest".into()],
                        decided_md,
                    ));
                }
            }
            // Decided: no longer under review.
            if let Some(meta) = get_mut(sys, "meta")
                && let Some(pf) = get_mut(meta, "privacy_flow")
                && let Some(Node::List(review)) = get_mut(pf, "needs_review").map(Node::bare_mut)
            {
                review.retain(|n| {
                    let s = str_of(Some(n.bare()));
                    s != Some(name.as_str()) && s != replaced.as_deref()
                });
            }
            notes.push(format!(
                "{key}: {name} as decided in the privacy-datamap manifest"
            ));
        }
    }
    notes
}

/// A decided collection's fields: its categories replace the generated ones; non-personal
/// fields have none; fields only the manifest has are added.
fn decide_fields(fields: &mut Node, decided: Option<&Node>, non_personal: &[String]) {
    let Node::List(list) = fields.bare_mut() else {
        return;
    };
    for f in list.iter_mut() {
        let name = str_of(get(f, "name")).unwrap_or("").to_string();
        let Node::Map(fm) = f.bare_mut() else {
            continue;
        };
        if non_personal.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            fm.retain(|(k, _)| k != "data_categories");
            continue;
        }
        let Some(df) = items(decided)
            .into_iter()
            .find(|d| str_of(get(d, "name")).is_some_and(|n| n.eq_ignore_ascii_case(&name)))
        else {
            continue;
        };
        let cats = strings(get(df, "data_categories"));
        fm.retain(|(k, _)| k != "data_categories");
        if !cats.is_empty() {
            let at = fm
                .iter()
                .position(|(k, _)| k == "fields")
                .unwrap_or(fm.len());
            fm.insert(at, ("data_categories".into(), Node::strs(cats)));
        }
    }
    for df in items(decided) {
        let name = str_of(get(df, "name")).unwrap_or("");
        if !list
            .iter()
            .any(|f| str_of(get(f, "name")).is_some_and(|n| n.eq_ignore_ascii_case(name)))
        {
            list.push(strip(df));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply;
    use crate::datamap::yaml::{parse, render};

    #[test]
    fn decisions_win_and_bookkeeping_goes() {
        let mut tree = parse("dataset:\n  - fides_key: shop_db\n    collections:\n      - name: members\n        fields:\n          - name: email_address\n            data_categories: [user.contact.email]\n          - name: phone\n            data_categories: [user.contact.phone_number]\n          - name: nickname\nsystem:\n  - fides_key: shop_api\n    meta:\n      privacy_flow:\n        needs_review: [Essential for Service]\n    privacy_declarations:\n      - name: Essential for Service\n        data_use: essential.service\n        data_categories: [user.contact.email]\n").unwrap();
        let manifest = parse("dataset:\n  - fides_key: app_postgres\n    collections:\n      - name: members\n        interpretation: { owner: Dana, decided_at: \"2026-08-20\", expires_at: \"2026-09-01\" }\n        non_personal_fields: [phone]\n        fields:\n          - name: nickname\n            data_categories: [user.name]\n            refs: [\"x.sql:3\"]\n  - fides_key: audit\n    collections:\n      - name: events\n        fields: []\nsystem:\n  - fides_key: apps_api\n    privacy_declarations:\n      - name: Operate the account\n        data_use: essential.service\n        data_subjects: [customer]\n        data_categories: [user.contact.email]\n        interpretation: { owner: Dana }\n").unwrap();
        let notes = apply(
            &mut tree,
            &manifest,
            &[("apps_api".into(), "shop_api".into())],
            Some("2026-10-15"),
        );
        let text = render(&[], &tree);
        assert!(!text.contains("phone_number"), "{text}");
        assert!(text.contains("\"user.name\""), "{text}");
        assert!(text.contains("fides_key: \"audit\""), "{text}");
        assert!(text.contains("Operate the account"), "{text}");
        assert!(!text.contains("needs_review: [\"Essential"), "{text}");
        assert!(!text.contains("refs"), "{text}");
        assert!(!text.contains("interpretation"), "{text}");
        assert!(notes.iter().any(|n| n.contains("expired")), "{notes:?}");
    }
}
