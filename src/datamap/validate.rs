//! The Fideslang 3.1.3 rules a merged data map must keep, checked offline: required fields,
//! DataFlow types, declarations that name only their system's own DataFlows, and keys that are
//! unique (a `fides_key` within its resource type, a name within its collection or dataset). CI
//! checks the goldens with fideslang itself.
//!
//! A declaration without a name or a data use is not filled in: it is moved out of
//! `privacy_declarations` into the system's `meta.privacy_flow.proposals`, for a person.

use std::collections::BTreeSet;

use crate::datamap::merge::{privacy_flow_mut, same};
use crate::datamap::yaml::Node;

fn get<'a>(n: &'a Node, key: &str) -> Option<&'a Node> {
    match n.bare() {
        Node::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v.bare()),
        _ => None,
    }
}

fn str_of(n: Option<&Node>) -> Option<&str> {
    match n? {
        Node::Str(s) => Some(s),
        _ => None,
    }
}

fn list(n: Option<&Node>) -> &[Node] {
    match n {
        Some(Node::List(l)) => l,
        _ => &[],
    }
}

fn is_key(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.<>-".contains(c))
}

/// Each value of `of` that more than one element of `items` has, once, in order.
fn repeated<'a>(items: &'a [Node], of: &str) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for k in items.iter().filter_map(|i| str_of(get(i, of))) {
        if !seen.insert(k) && !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

/// Field names repeated within one collection or one parent field, at any depth.
fn repeated_fields(fields: &[Node], at: &str, out: &mut Vec<String>) {
    for name in repeated(fields, "name") {
        out.push(format!("{at}: field {name} is defined more than once"));
    }
    for f in fields {
        let name = str_of(get(f, "name")).unwrap_or("");
        repeated_fields(list(get(f, "fields")), &format!("{at}.{name}"), out);
    }
}

/// What fideslang would reject, as messages.
pub fn check(root: &Node) -> Vec<String> {
    let mut out = Vec::new();
    for (kind, items) in [
        ("dataset", list(get(root, "dataset"))),
        ("system", list(get(root, "system"))),
    ] {
        for key in repeated(items, "fides_key") {
            out.push(format!(
                "{kind} {key}: defined more than once; each fides_key must be unique"
            ));
        }
    }
    for ds in list(get(root, "dataset")) {
        let key = str_of(get(ds, "fides_key")).unwrap_or("");
        if !is_key(key) {
            out.push(format!("dataset {key:?}: fides_key is not a Fides key"));
        }
        if !matches!(get(ds, "collections"), Some(Node::List(_))) {
            out.push(format!("dataset {key}: collections is required"));
        }
        for name in repeated(list(get(ds, "collections")), "name") {
            out.push(format!(
                "dataset {key}: collection {name} is defined more than once"
            ));
        }
        for c in list(get(ds, "collections")) {
            let name = str_of(get(c, "name")).unwrap_or("");
            if name.is_empty() {
                out.push(format!("dataset {key}: a collection has no name"));
            }
            if !matches!(get(c, "fields"), Some(Node::List(_))) {
                out.push(format!("dataset {key}.{name}: fields is required"));
            }
            repeated_fields(
                list(get(c, "fields")),
                &format!("dataset {key}.{name}"),
                &mut out,
            );
        }
    }
    for sys in list(get(root, "system")) {
        let key = str_of(get(sys, "fides_key")).unwrap_or("");
        if !is_key(key) {
            out.push(format!("system {key:?}: fides_key is not a Fides key"));
        }
        if str_of(get(sys, "system_type")).is_none() {
            out.push(format!("system {key}: system_type is required"));
        }
        if !matches!(get(sys, "privacy_declarations"), Some(Node::List(_))) {
            out.push(format!("system {key}: privacy_declarations is required"));
        }
        let mut flows: [BTreeSet<String>; 2] = [BTreeSet::new(), BTreeSet::new()];
        for (i, dir) in ["egress", "ingress"].iter().enumerate() {
            for f in list(get(sys, dir)) {
                let fk = str_of(get(f, "fides_key")).unwrap_or("");
                let ty = str_of(get(f, "type")).unwrap_or("");
                if !matches!(ty, "system" | "dataset" | "user") {
                    out.push(format!(
                        "system {key}: {dir} {fk}: type must be system, dataset or user"
                    ));
                }
                if (fk == "user") != (ty == "user") {
                    out.push(format!(
                        "system {key}: {dir} {fk}: the key user goes with the type user"
                    ));
                }
                flows[i].insert(fk.to_string());
            }
        }
        for d in list(get(sys, "privacy_declarations")) {
            let name = str_of(get(d, "name")).unwrap_or("");
            if name.trim().is_empty() {
                out.push(format!("system {key}: a declaration has no name"));
            }
            if !str_of(get(d, "data_use")).is_some_and(is_key) {
                out.push(format!(
                    "system {key}: declaration {name:?}: data_use is required"
                ));
            }
            if !matches!(get(d, "data_categories"), Some(Node::List(_))) {
                out.push(format!(
                    "system {key}: declaration {name:?}: data_categories is required"
                ));
            }
            for (i, dir) in ["egress", "ingress"].iter().enumerate() {
                let Some(n) = get(d, dir) else { continue };
                let names = list(Some(n));
                if names.is_empty() {
                    out.push(format!(
                        "system {key}: declaration {name:?}: {dir} must not be empty"
                    ));
                }
                for x in names {
                    let x = str_of(Some(x)).unwrap_or("");
                    if !flows[i].contains(x) {
                        out.push(format!(
                            "system {key}: declaration {name:?}: {dir} names {x}, which is not one of the system's {dir}"
                        ));
                    }
                }
            }
        }
    }
    out
}

/// What a declaration lacks to be one: a name and a data use. None when it has both.
fn lacks(d: &Node) -> Option<&'static str> {
    let named = str_of(get(d, "name")).is_some_and(|s| !s.trim().is_empty());
    let used = str_of(get(d, "data_use")).is_some_and(is_key);
    match (named, used) {
        (true, true) => None,
        (false, true) => Some("no name"),
        (true, false) => Some("no data use"),
        (false, false) => Some("no name and no data use"),
    }
}

/// Moves each declaration without a name or a data use into its system's
/// `meta.privacy_flow.proposals`, and out of `needs_review`. What moved, as notes.
pub fn quarantine(root: &mut Node) -> Vec<String> {
    let mut notes = Vec::new();
    let Node::Map(r) = root.bare_mut() else {
        return notes;
    };
    let Some((_, systems)) = r.iter_mut().find(|(k, _)| k == "system") else {
        return notes;
    };
    let Node::List(systems) = systems.bare_mut() else {
        return notes;
    };
    for sys in systems.iter_mut() {
        let key = str_of(get(sys, "fides_key")).unwrap_or("").to_string();
        let had = get(sys, "meta")
            .and_then(|m| get(m, "privacy_flow"))
            .and_then(|p| get(p, "proposals"))
            .is_some();
        let Node::Map(sm) = sys.bare_mut() else {
            continue;
        };
        let mut moved = Vec::new();
        if let Some((_, decls)) = sm.iter_mut().find(|(k, _)| k == "privacy_declarations")
            && let Node::List(decls) = decls.bare_mut()
        {
            let kept;
            (kept, moved) = std::mem::take(decls)
                .into_iter()
                .partition(|d| lacks(d).is_none());
            *decls = kept;
        }
        if moved.is_empty() && !had {
            continue;
        }
        let names: Vec<String> = moved
            .iter()
            .filter_map(|d| str_of(get(d, "name")).map(str::to_string))
            .collect();
        let Some(pf) = privacy_flow_mut(sm) else {
            continue;
        };
        if let Some((_, Node::List(review))) = pf.iter_mut().find(|(k, _)| k == "needs_review") {
            review
                .retain(|n| !str_of(Some(n.bare())).is_some_and(|s| names.iter().any(|x| x == s)));
        }
        pf.retain(|(k, v)| {
            k != "needs_review" || !matches!(v.bare(), Node::List(l) if l.is_empty())
        });
        if !pf.iter().any(|(k, _)| k == "proposals") {
            pf.push(("proposals".into(), Node::List(Vec::new())));
        }
        let Some((_, Node::List(proposals))) = pf.iter_mut().find(|(k, _)| k == "proposals") else {
            continue;
        };
        for d in moved {
            let why = lacks(&d).unwrap_or_default();
            let name = str_of(get(&d, "name")).unwrap_or("");
            notes.push(format!(
                "system {key}: declaration {name:?} has {why}; moved to meta.privacy_flow.proposals for review"
            ));
            if !proposals.iter().any(|p| same(p, &d)) {
                proposals.push(d.bare().clone());
            }
        }
        // Each run says what each proposal lacks: a person's file has no `#:` notes to read back.
        for p in proposals.iter_mut() {
            let note = match lacks(p) {
                Some(why) => format!(
                    "INCOMPLETE: {why}, so not a Fideslang declaration; complete it and move it to privacy_declarations"
                ),
                None => "complete: move it to privacy_declarations".to_string(),
            };
            *p = Node::noted(vec![note], p.bare().clone());
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::{check, quarantine};
    use crate::datamap::yaml::{parse, render};

    const MAP: &str = "system:\n  - fides_key: crm\n    system_type: Service\n    meta:\n      privacy_flow:\n        needs_review: [Sync contacts, Send mail]\n    privacy_declarations:\n      - name: Sync contacts\n        data_use: essential.service\n        data_categories: [user.contact.email]\n      - name: Send mail\n        data_use: \"\"\n        data_categories: [user.contact.email]\n      - name: \"\"\n        data_use: \"\"\n        data_categories: [user.name]\n";

    #[test]
    fn a_declaration_needs_a_name_and_a_data_use() {
        let problems = check(&parse(MAP).unwrap());
        assert!(
            problems.iter().any(|p| p.contains("has no name")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("data_use is required")),
            "{problems:?}"
        );
    }

    #[test]
    fn keys_and_names_are_unique() {
        let problems = check(&parse("dataset:\n  - fides_key: db\n    collections:\n      - name: users\n        fields:\n          - name: email\n          - name: email\n      - name: users\n        fields: []\n  - fides_key: db\n    collections: []\nsystem:\n  - fides_key: web\n    system_type: Service\n    privacy_declarations: []\n  - fides_key: web\n    system_type: Service\n    privacy_declarations: []\n  - fides_key: web\n    system_type: Service\n    privacy_declarations: []\n").unwrap());
        for want in [
            "dataset db: defined more than once",
            "system web: defined more than once",
            "dataset db: collection users is defined more than once",
            "dataset db.users: field email is defined more than once",
        ] {
            assert_eq!(
                problems.iter().filter(|p| p.starts_with(want)).count(),
                1,
                "{want}: {problems:?}"
            );
        }
    }

    #[test]
    fn incomplete_declarations_become_proposals_and_leave_review() {
        let mut tree = parse(MAP).unwrap();
        let notes = quarantine(&mut tree);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(check(&tree).is_empty(), "{:?}", check(&tree));
        let text = render(&[], &tree);
        let (meta, decls) = text.split_once("privacy_declarations:").unwrap();
        assert!(
            decls.contains("Sync contacts") && !decls.contains("Send mail"),
            "{text}"
        );
        assert!(
            meta.contains("proposals:") && meta.contains("Send mail"),
            "{text}"
        );
        assert!(meta.contains("INCOMPLETE: no data use"), "{text}");
        assert!(
            meta.contains("INCOMPLETE: no name and no data use"),
            "{text}"
        );
        let review = &meta[meta.find("needs_review:").unwrap()..meta.find("proposals:").unwrap()];
        assert!(
            review.contains("Sync contacts") && !review.contains("Send mail"),
            "{text}"
        );
        // Idempotent: nothing left to move, nothing added twice.
        assert!(quarantine(&mut tree).is_empty());
        assert_eq!(render(&[], &tree), text);
    }
}
