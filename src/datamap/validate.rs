//! The Fideslang 3.1.3 rules a merged data map must keep, checked offline: required fields,
//! DataFlow types, and declarations that name only their system's own DataFlows. CI checks the
//! goldens with fideslang itself.

use std::collections::BTreeSet;

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

/// What fideslang would reject, as messages.
pub fn check(root: &Node) -> Vec<String> {
    let mut out = Vec::new();
    for ds in list(get(root, "dataset")) {
        let key = str_of(get(ds, "fides_key")).unwrap_or("");
        if !is_key(key) {
            out.push(format!("dataset {key:?}: fides_key is not a Fides key"));
        }
        if !matches!(get(ds, "collections"), Some(Node::List(_))) {
            out.push(format!("dataset {key}: collections is required"));
        }
        for c in list(get(ds, "collections")) {
            let name = str_of(get(c, "name")).unwrap_or("");
            if name.is_empty() {
                out.push(format!("dataset {key}: a collection has no name"));
            }
            if !matches!(get(c, "fields"), Some(Node::List(_))) {
                out.push(format!("dataset {key}.{name}: fields is required"));
            }
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
