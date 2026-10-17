//! One definition per identity. Elements of a list that share one (a dataset's or a system's
//! `fides_key`, a collection's or a field's name, a DataFlow's type and key, a declaration's name)
//! are folded into the first, so that nothing either had is lost.
//!
//! Maps fold key by key. Where both have a value and they differ, the first one's stays and the
//! other is reported (a note of the run, not of the file, so that a second run writes the same
//! bytes). Sets are their union. Elements of lists with an
//! identity fold in turn; other lists join without repeats. A declaration that one definition
//! accepted (left out of `needs_review`) stays accepted. Two declarations with one name and
//! different data uses are both kept: the later one is renamed after its data use and goes under
//! review.

use std::collections::BTreeSet;

use crate::datamap::merge::{identity, rename, same, set_needs_review, short};
use crate::datamap::yaml::Node;

/// Lists whose elements have an identity.
const KEYED: &[&str] = &[
    "dataset",
    "system",
    "collections",
    "fields",
    "privacy_declarations",
    "egress",
    "ingress",
];

/// Fold the elements of `root` that share an identity. What was folded, as notes.
pub fn consolidate(root: &mut Node) -> Vec<String> {
    let mut notes = Vec::new();
    walk(root, "", "", &mut notes);
    let mut seen = BTreeSet::new();
    notes.retain(|n| seen.insert(n.clone()));
    notes
}

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

fn notes_of(n: &Node) -> Vec<String> {
    match n {
        Node::Noted(notes, _) => notes.clone(),
        _ => Vec::new(),
    }
}

/// `n` is the element at `path`; `key` is the map key or list key it sits under.
fn walk(n: &mut Node, path: &str, key: &str, notes: &mut Vec<String>) {
    match n.bare_mut() {
        Node::Map(m) => {
            if key == "system" {
                distinct_declarations(m, path, notes);
            }
            for (k, v) in m.iter_mut() {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk(v, &sub, k, notes);
            }
        }
        Node::List(l) => {
            if KEYED.contains(&key) {
                fold_list(l, path, key, notes);
            }
            for item in l.iter_mut() {
                let sub = match identity(key, item) {
                    Some(id) => format!("{path}[{id}]"),
                    None => path.to_string(),
                };
                walk(item, &sub, key, notes);
            }
        }
        _ => {}
    }
}

fn fold_list(l: &mut Vec<Node>, path: &str, key: &str, notes: &mut Vec<String>) {
    let mut out: Vec<Node> = Vec::new();
    let mut ids: Vec<Option<String>> = Vec::new();
    for item in std::mem::take(l) {
        let id = identity(key, &item);
        match id
            .as_ref()
            .and_then(|id| ids.iter().position(|x| x.as_ref() == Some(id)))
        {
            Some(i) => {
                let p = format!("{path}[{}]", id.unwrap_or_default());
                notes.push(format!("{p}: defined more than once; merged into one"));
                let first = std::mem::replace(&mut out[i], Node::Null);
                out[i] = fold(first, &item, &p, key, notes);
            }
            None => {
                ids.push(id);
                out.push(item);
            }
        }
    }
    *l = out;
}

/// `b` folded into `a`, the element at `path` under `key`.
fn fold(a: Node, b: &Node, path: &str, key: &str, notes: &mut Vec<String>) -> Node {
    let mut ns = notes_of(&a);
    for n in notes_of(b) {
        if !ns.contains(&n) {
            ns.push(n);
        }
    }
    let review = (key == "system").then(|| review_after(&a, b));
    let merged = match (a.bare(), b.bare()) {
        (Node::Map(am), Node::Map(bm)) => {
            let mut out = am.clone();
            for (k, bv) in bm {
                match out.iter_mut().find(|(kk, _)| kk == k) {
                    Some(slot) => {
                        let av = std::mem::replace(&mut slot.1, Node::Null);
                        slot.1 = fold(av, bv, &format!("{path}.{k}"), k, notes);
                    }
                    None => out.push((k.clone(), bv.clone())),
                }
            }
            if let Some(names) = review {
                set_needs_review(&mut out, names);
            }
            Node::Map(out)
        }
        // Elements with an identity fold when the walk reaches the joined list.
        (Node::List(al), Node::List(bl)) if KEYED.contains(&key) => {
            Node::List(al.iter().chain(bl).cloned().collect())
        }
        (Node::List(al), Node::List(bl)) => {
            let mut out = al.clone();
            for x in bl {
                if !out.iter().any(|y| same(y, x)) {
                    out.push(x.clone());
                }
            }
            Node::List(out)
        }
        (x, y) if same(x, y) => x.clone(),
        (x, y) => {
            notes.push(format!(
                "{path}: kept {}; another definition has {}",
                short(x),
                short(y)
            ));
            x.clone()
        }
    };
    Node::noted(ns, merged)
}

fn declaration_names(system: &Node) -> BTreeSet<String> {
    match get(system, "privacy_declarations") {
        Some(Node::List(l)) => l
            .iter()
            .filter_map(|d| identity("privacy_declarations", d))
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn review_of(system: &Node) -> BTreeSet<String> {
    let review = get(system, "meta")
        .and_then(|m| get(m, "privacy_flow"))
        .and_then(|p| get(p, "needs_review"));
    match review {
        Some(Node::List(l)) => l
            .iter()
            .filter_map(|x| str_of(Some(x.bare())).map(str::to_string))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// What stays under review when two definitions of a system fold: what either lists, unless a
/// definition that has the declaration accepted it.
fn review_after(a: &Node, b: &Node) -> Vec<String> {
    let accepted =
        |s: &Node, n: &String| declaration_names(s).contains(n) && !review_of(s).contains(n);
    review_of(a)
        .union(&review_of(b))
        .filter(|n| !accepted(a, n) && !accepted(b, n))
        .cloned()
        .collect()
}

/// Declarations of one system with one name and different data uses: the later ones renamed
/// after their data use, and under review.
fn distinct_declarations(sm: &mut Vec<(String, Node)>, path: &str, notes: &mut Vec<String>) {
    let Some((_, decls)) = sm.iter_mut().find(|(k, _)| k == "privacy_declarations") else {
        return;
    };
    let Node::List(decls) = decls.bare_mut() else {
        return;
    };
    let mut seen: Vec<(String, Option<String>)> = Vec::new();
    let mut renamed: Vec<String> = Vec::new();
    for d in decls.iter_mut() {
        let Some(name) = identity("privacy_declarations", d) else {
            continue;
        };
        let used = str_of(get(d, "data_use"))
            .filter(|u| !u.is_empty())
            .map(str::to_string);
        let clash = |seen: &[(String, Option<String>)], n: &str| {
            seen.iter()
                .any(|(sn, su)| sn == n && matches!((su, &used), (Some(x), Some(y)) if x != y))
        };
        let mut new = name.clone();
        while clash(&seen, &new) {
            new = format!("{new} ({})", used.as_deref().unwrap_or_default());
        }
        if new != name {
            notes.push(format!(
                "{path}: two declarations are named {name:?} with different data uses; the one for {} is now {new:?}, under review",
                used.as_deref().unwrap_or_default()
            ));
            *d = rename(std::mem::replace(d, Node::Null), &new);
            renamed.push(new.clone());
        }
        seen.push((new, used));
    }
    if !renamed.is_empty() {
        let mut names = review_of(&Node::Map(sm.clone()));
        names.extend(renamed);
        set_needs_review(sm, names.into_iter().collect());
    }
}

#[cfg(test)]
mod tests {
    use super::consolidate;
    use crate::datamap::validate::check;
    use crate::datamap::yaml::{parse, render};

    #[test]
    fn duplicate_systems_fold_into_one_and_keep_what_each_had() {
        let mut tree = parse(concat!(
            "system:\n",
            "  - fides_key: web\n",
            "    name: Storefront\n",
            "    system_type: Service\n",
            "    meta:\n      privacy_flow:\n        needs_review: [Send mail]\n",
            "    egress:\n",
            "      - fides_key: crm\n        type: system\n        data_categories: [user.name]\n",
            "    privacy_declarations:\n",
            "      - name: Send mail\n        data_use: marketing.communications\n        data_categories: [user.contact.email]\n        egress: [crm]\n",
            "      - name: Operate the account\n        data_use: essential.service\n        data_categories: [user.name]\n",
            "  - fides_key: web\n",
            "    name: Web app\n",
            "    system_type: Service\n",
            "    meta:\n      privacy_flow:\n        needs_review: [Operate the account, Analytics]\n",
            "    dataset_references: [shop_db]\n",
            "    egress:\n",
            "      - fides_key: crm\n        type: system\n        data_categories: [user.contact.email]\n",
            "      - fides_key: metrics\n        type: system\n",
            "    privacy_declarations:\n",
            "      - name: Send mail\n        data_use: marketing.communications\n        data_categories: [user.name]\n        egress: [crm]\n",
            "      - name: Operate the account\n        data_use: essential.service\n        data_categories: [user.name]\n",
            "      - name: Analytics\n        data_use: analytics.reporting\n        data_categories: [user.device]\n        egress: [metrics]\n",
            "      - name: Operate the account\n        data_use: essential.service.authentication\n        data_categories: [user.authorization]\n",
        ))
        .unwrap();
        let notes = consolidate(&mut tree);
        assert!(check(&tree).is_empty(), "{:?}", check(&tree));
        let text = render(&[], &tree);
        assert_eq!(text.matches("fides_key: \"web\"").count(), 1, "{text}");
        // The first definition's name stays; the other is reported.
        assert!(text.contains("name: \"Storefront\""), "{text}");
        assert!(
            notes
                .iter()
                .any(|n| n.contains("system[web].name") && n.contains("Web app")),
            "{notes:?}"
        );
        // Union of references, flows and their categories.
        assert!(text.contains("\"shop_db\""), "{text}");
        assert!(text.contains("fides_key: \"metrics\""), "{text}");
        assert_eq!(text.matches("fides_key: \"crm\"").count(), 1, "{text}");
        assert!(text.contains("\"user.contact.email\""), "{text}");
        // Every declaration, once; one name with two uses is kept as two.
        assert_eq!(text.matches("name: \"Send mail\"").count(), 1, "{text}");
        assert_eq!(
            text.matches("name: \"Operate the account\"").count(),
            1,
            "{text}"
        );
        assert!(text.contains("name: \"Analytics\""), "{text}");
        assert!(
            text.contains("name: \"Operate the account (essential.service.authentication)\""),
            "{text}"
        );
        // Accepted in either definition that has it: stays accepted. Analytics only the second
        // has, under review there: still under review. The renamed one: under review.
        let review = &text[text.find("needs_review:").unwrap()..text.find("egress:").unwrap()];
        assert!(!review.contains("\"Send mail\""), "{text}");
        assert!(!review.contains("\"Operate the account\""), "{text}");
        assert!(review.contains("\"Analytics\""), "{text}");
        assert!(
            review.contains("essential.service.authentication"),
            "{text}"
        );
        // A fixpoint: nothing left to fold.
        assert!(consolidate(&mut tree).is_empty());
        assert_eq!(render(&[], &tree), text);
    }

    #[test]
    fn duplicate_datasets_collections_and_fields_fold() {
        let mut tree = parse(concat!(
            "dataset:\n",
            "  - fides_key: shop_db\n    collections:\n",
            "      - name: users\n        fields:\n",
            "          - name: email\n            data_categories: [user.contact.email]\n",
            "  - fides_key: shop_db\n    description: Orders and users\n    collections:\n",
            "      - name: users\n        fields:\n",
            "          - name: email\n            data_categories: [user.contact]\n",
            "          - name: phone\n            data_categories: [user.contact.phone_number]\n",
            "      - name: orders\n        fields: []\n",
        ))
        .unwrap();
        consolidate(&mut tree);
        assert!(check(&tree).is_empty(), "{:?}", check(&tree));
        let text = render(&[], &tree);
        assert_eq!(text.matches("fides_key: \"shop_db\"").count(), 1, "{text}");
        assert_eq!(text.matches("name: \"users\"").count(), 1, "{text}");
        assert_eq!(text.matches("name: \"email\"").count(), 1, "{text}");
        for want in [
            "Orders and users",
            "\"user.contact\"",
            "\"user.contact.email\"",
            "phone",
            "orders",
        ] {
            assert!(text.contains(want), "{want}: {text}");
        }
    }
}
