//! Merging a new generation into the data map a person keeps (ADR 0019).
//!
//! Three trees: the base is what piiflow generated last time (the lock), ours is the file as it
//! is now, theirs is what the code shows now. Elements of lists are matched by identity
//! (`fides_key`; `name`; a DataFlow's type and key). A value nobody touched follows the code; a
//! value a person changed stays; an element a person deleted stays deleted; an element the code
//! no longer shows goes, unless a person changed it; anything a person added stays. Lists of
//! keys (`data_categories`, …) merge as sets.

use std::collections::{BTreeMap, BTreeSet};

use crate::datamap::yaml::Node;

#[derive(Debug, Default)]
pub struct Report {
    /// Values a person changed that the code would now set otherwise.
    pub conflicts: Vec<String>,
    /// Elements a person deleted that the code still shows.
    pub kept_deleted: Vec<String>,
    /// Elements the code no longer shows, kept because a person changed them.
    pub kept_unobserved: Vec<String>,
    /// Elements the code no longer shows, removed.
    pub removed: Vec<String>,
    /// Declarations a person had accepted whose evidence changed: back under review.
    pub re_review: Vec<String>,
    /// References removed because what they named is gone.
    pub dangling: Vec<String>,
}

impl Report {
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut add = |what: &str, list: &[String]| {
            for x in list {
                out.push(format!("{what}: {x}"));
            }
        };
        add("kept your change", &self.conflicts);
        add("kept deleted", &self.kept_deleted);
        add(
            "no longer in the code, kept (changed by you)",
            &self.kept_unobserved,
        );
        add("no longer in the code, removed", &self.removed);
        add("evidence changed, back under review", &self.re_review);
        add("removed a reference to what is gone", &self.dangling);
        out
    }
}

/// Lists whose items are matched by identity, by the key that holds them.
fn identity(list_key: &str, item: &Node) -> Option<String> {
    let Node::Map(m) = item.bare() else {
        return None;
    };
    let get = |k: &str| {
        m.iter().find_map(|(kk, v)| match v.bare() {
            Node::Str(s) if kk == k => Some(s.clone()),
            _ => None,
        })
    };
    match list_key {
        "dataset" | "system" => get("fides_key"),
        "collections" | "fields" | "privacy_declarations" => get("name"),
        // Incomplete declarations have no name to tell them apart: the list is one value.
        "proposals" => None,
        "egress" | "ingress" => Some(format!(
            "{}:{}",
            get("type").unwrap_or_default(),
            get("fides_key")?
        )),
        _ => get("fides_key").or_else(|| get("name")),
    }
}

/// Equal, notes aside.
pub fn same(a: &Node, b: &Node) -> bool {
    match (a.bare(), b.bare()) {
        (Node::Map(x), Node::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, w)| same(v, w))
                })
        }
        (Node::List(x), Node::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, w)| same(v, w))
        }
        (x, y) => x == y,
    }
}

fn get<'a>(n: Option<&'a Node>, key: &str) -> Option<&'a Node> {
    match n?.bare() {
        Node::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn notes_of(n: &Node) -> Vec<String> {
    match n {
        Node::Noted(notes, _) => notes.clone(),
        _ => Vec::new(),
    }
}

fn is_scalar_list(items: &[&Node]) -> bool {
    items
        .iter()
        .all(|i| matches!(i.bare(), Node::Str(_) | Node::Num(_) | Node::Bool(_)))
}

struct Merger<'r> {
    report: &'r mut Report,
}

impl Merger<'_> {
    /// Merge one element. `key` is the map key or list key it sits under.
    fn node(
        &mut self,
        path: &str,
        key: &str,
        b: Option<&Node>,
        o: Option<&Node>,
        t: Option<&Node>,
    ) -> Option<Node> {
        match (b, o, t) {
            (_, None, None) => None,
            (None, None, Some(t)) => Some(t.clone()),
            (Some(_), None, Some(_)) => {
                self.report.kept_deleted.push(path.to_string());
                None
            }
            (None, Some(o), None) => Some(o.clone()),
            (Some(b), Some(o), None) => {
                if same(o, b) {
                    self.report.removed.push(path.to_string());
                    None
                } else {
                    self.report.kept_unobserved.push(path.to_string());
                    Some(o.clone())
                }
            }
            (b, Some(o), Some(t)) => Some(self.present(path, key, b, o, t)),
        }
    }

    fn present(&mut self, path: &str, key: &str, b: Option<&Node>, o: &Node, t: &Node) -> Node {
        let notes = notes_of(t);
        let merged = match (o.bare(), t.bare()) {
            (Node::Map(om), Node::Map(tm)) => {
                let mut keys: Vec<&str> = tm.iter().map(|(k, _)| k.as_str()).collect();
                for (k, _) in om {
                    if !keys.contains(&k.as_str()) {
                        keys.push(k);
                    }
                }
                let mut out = Vec::new();
                for k in keys {
                    let sub = format!("{path}.{k}");
                    let bv = get(b, k);
                    let ov = om.iter().find(|(kk, _)| kk == k).map(|(_, v)| v);
                    let tv = tm.iter().find(|(kk, _)| kk == k).map(|(_, v)| v);
                    if let Some(v) = self.node(&sub, k, bv, ov, tv) {
                        out.push((k.to_string(), v));
                    }
                }
                Node::Map(out)
            }
            (Node::List(ol), Node::List(tl)) => self.list(path, key, b, ol, tl),
            (ob, tb) => {
                if b.is_some_and(|b| same(b, o)) {
                    // Untouched: the code's value.
                    return t.clone();
                }
                if same(ob, tb) || b.is_none_or(|b| same(b, t)) {
                    return o.clone();
                }
                self.report.conflicts.push(path.to_string());
                let mut n = notes_of(o);
                n.push(format!("piiflow proposes {}", short(tb)));
                return Node::noted(n, o.bare().clone());
            }
        };
        Node::noted(notes, merged)
    }

    fn list(&mut self, path: &str, key: &str, b: Option<&Node>, ol: &[Node], tl: &[Node]) -> Node {
        let bl: Vec<&Node> = match b.map(Node::bare) {
            Some(Node::List(x)) => x.iter().collect(),
            _ => Vec::new(),
        };
        let all: Vec<&Node> = ol.iter().chain(tl).chain(bl.iter().copied()).collect();
        if is_scalar_list(&all) {
            // A set: theirs' additions and removals since the base, on ours.
            let bset: Vec<&Node> = bl.clone();
            let has = |list: &[&Node], x: &Node| list.iter().any(|y| same(y, x));
            let tref: Vec<&Node> = tl.iter().collect();
            let mut out: Vec<Node> = Vec::new();
            for x in ol {
                let removed_by_code = b.is_some() && has(&bset, x) && !has(&tref, x);
                if !removed_by_code && !out.iter().any(|y| same(y, x)) {
                    out.push(x.clone());
                }
            }
            for x in tl {
                let added_by_code = b.is_none_or(|_| !has(&bset, x));
                // Without a base, the file is the person's: the code adds nothing.
                if b.is_some() && added_by_code && !out.iter().any(|y| same(y, x)) {
                    out.push(x.clone());
                }
            }
            return Node::List(out);
        }
        // Items with an identity.
        let ids =
            |l: &[&Node]| -> Option<Vec<String>> { l.iter().map(|i| identity(key, i)).collect() };
        let olr: Vec<&Node> = ol.iter().collect();
        let tlr: Vec<&Node> = tl.iter().collect();
        let (Some(oids), Some(tids), Some(bids)) = (ids(&olr), ids(&tlr), ids(&bl)) else {
            // No identity: the list is one value.
            let o = Node::List(ol.to_vec());
            let t = Node::List(tl.to_vec());
            if b.is_some_and(|b| same(b, &o)) {
                return t;
            }
            return o;
        };
        let mut order: Vec<String> = tids.clone();
        for id in &oids {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        let renames = if key == "privacy_declarations" {
            renamed(&bl, &bids, &olr, &oids, &tids)
        } else {
            BTreeMap::new()
        };
        let mut out = Vec::new();
        for id in order {
            let find = |l: &[&Node], ids: &[String], id: &str| -> Option<Node> {
                ids.iter().position(|x| x == id).map(|i| l[i].clone())
            };
            let bi = find(&bl, &bids, &id);
            let mut oi = find(&olr, &oids, &id);
            let ti = find(&tlr, &tids, &id);
            // A declaration renamed by a person: theirs under its new name.
            if oi.is_none()
                && let Some(new) = renames.get(&id)
            {
                oi = find(&olr, &oids, new);
                if let Some(merged) = self.node(
                    &format!("{path}[{new}]"),
                    key,
                    bi.as_ref(),
                    oi.as_ref(),
                    ti.as_ref(),
                ) {
                    out.push(rename(merged, new));
                }
                continue;
            }
            if renames.values().any(|n| *n == id) {
                continue;
            }
            if let Some(m) = self.node(
                &format!("{path}[{id}]"),
                key,
                bi.as_ref(),
                oi.as_ref(),
                ti.as_ref(),
            ) {
                out.push(m);
            }
        }
        Node::List(out)
    }
}

/// Generated declarations a person renamed: same evidence as in the base, under a name the base
/// does not have. Exactly one candidate, or none.
fn renamed(
    bl: &[&Node],
    bids: &[String],
    ol: &[&Node],
    oids: &[String],
    tids: &[String],
) -> BTreeMap<String, String> {
    let evidence = |n: &Node| -> Vec<String> {
        ["egress", "ingress", "dataset_references"]
            .iter()
            .map(|k| format!("{:?}", get(Some(n), k).map(super::yaml::to_json)))
            .collect()
    };
    let mut out = BTreeMap::new();
    for (i, id) in bids.iter().enumerate() {
        if oids.contains(id) || !tids.contains(id) {
            continue;
        }
        let want = evidence(bl[i]);
        let candidates: Vec<&String> = oids
            .iter()
            .enumerate()
            .filter(|(j, oid)| !bids.contains(oid) && evidence(ol[*j]) == want)
            .map(|(_, oid)| oid)
            .collect();
        if let [one] = candidates.as_slice() {
            out.insert(id.clone(), (*one).clone());
        }
    }
    out
}

fn rename(n: Node, name: &str) -> Node {
    match n {
        Node::Noted(notes, inner) => Node::Noted(notes, Box::new(rename(*inner, name))),
        Node::Map(m) => Node::Map(
            m.into_iter()
                .map(|(k, v)| {
                    if k == "name" {
                        (k, Node::str(name))
                    } else {
                        (k, v)
                    }
                })
                .collect(),
        ),
        other => other,
    }
}

fn short(n: &Node) -> String {
    match n {
        Node::Str(s) => format!("\"{s}\""),
        Node::List(items) => format!(
            "[{}]",
            items.iter().map(short).collect::<Vec<_>>().join(", ")
        ),
        Node::Map(_) => "a different value".into(),
        Node::Bool(b) => b.to_string(),
        Node::Num(s) => s.clone(),
        Node::Null => "null".into(),
        Node::Noted(_, inner) => short(inner),
    }
}

/// Merge `theirs` into `ours` over `base`; without `ours` the result is `theirs`.
pub fn merge(base: Option<&Node>, ours: Option<&Node>, theirs: &Node) -> (Node, Report) {
    let mut report = Report::default();
    let Some(ours) = ours else {
        return (theirs.clone(), report);
    };
    let ours = &review_lists(base, ours);
    let (theirs, back) = accept(base, ours, theirs, &mut report);
    let mut m = Merger {
        report: &mut report,
    };
    let mut merged = m
        .node("", "", base, Some(ours), Some(&theirs))
        .unwrap_or_else(|| Node::Map(Vec::new()));
    // Declarations whose evidence changed go back under review, whatever the set merge kept.
    if let Node::Map(r) = merged.bare_mut()
        && let Some((_, systems)) = r.iter_mut().find(|(k, _)| k == "system")
        && let Node::List(systems) = systems.bare_mut()
    {
        for sys in systems.iter_mut() {
            let key = identity("system", sys).unwrap_or_default();
            let names: Vec<String> = back
                .iter()
                .filter(|(k, _)| *k == key)
                .map(|(_, n)| n.clone())
                .collect();
            if names.is_empty() {
                continue;
            }
            let mut all: Vec<String> = needs_review(sys).into_iter().collect();
            all.extend(names);
            all.sort();
            all.dedup();
            if let Node::Map(sm) = sys.bare_mut() {
                set_needs_review(sm, all);
            }
        }
    }
    let merged = tidy(merged, &mut report);
    (merged, report)
}

fn system_list(n: Option<&Node>) -> Vec<&Node> {
    match get(n, "system").map(Node::bare) {
        Some(Node::List(l)) => l.iter().collect(),
        _ => Vec::new(),
    }
}

fn needs_review(system: &Node) -> BTreeSet<String> {
    let meta = get(Some(system), "meta");
    let pf = get(meta, "privacy_flow");
    match get(pf, "needs_review").map(Node::bare) {
        Some(Node::List(l)) => l
            .iter()
            .filter_map(|x| match x.bare() {
                Node::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// A system whose review list a person emptied away (deleting `meta` with it) has an empty
/// list, not a deleted one: later proposals must still reach it.
fn review_lists(base: Option<&Node>, ours: &Node) -> Node {
    let mut ours = ours.clone();
    let bsys = system_list(base);
    let Node::Map(root) = ours.bare_mut() else {
        return ours;
    };
    let Some((_, systems)) = root.iter_mut().find(|(k, _)| k == "system") else {
        return ours;
    };
    let Node::List(systems) = systems.bare_mut() else {
        return ours;
    };
    for sys in systems.iter_mut() {
        let key = identity("system", sys);
        let had = bsys
            .iter()
            .any(|b| identity("system", b) == key && !needs_review(b).is_empty());
        let has = get(get(get(Some(sys), "meta"), "privacy_flow"), "needs_review").is_some();
        if had
            && !has
            && let Node::Map(sm) = sys.bare_mut()
        {
            let pf = Node::Map(vec![("needs_review".into(), Node::List(Vec::new()))]);
            match sm.iter_mut().find(|(k, _)| k == "meta") {
                Some((_, meta)) => {
                    if let Node::Map(mm) = meta.bare_mut() {
                        match mm.iter_mut().find(|(k, _)| k == "privacy_flow") {
                            Some((_, p)) => {
                                if let Node::Map(pm) = p.bare_mut() {
                                    pm.push(("needs_review".into(), Node::List(Vec::new())));
                                }
                            }
                            None => mm.push(("privacy_flow".into(), pf)),
                        }
                    }
                }
                None => sm.push(("meta".into(), Node::Map(vec![("privacy_flow".into(), pf)]))),
            }
        }
    }
    ours
}

/// A declaration a person accepted (removed from `needs_review`) is theirs: its data use and
/// data subjects stay as they are. If its evidence changed since, it goes back under review.
fn accept(
    base: Option<&Node>,
    ours: &Node,
    theirs: &Node,
    report: &mut Report,
) -> (Node, Vec<(String, String)>) {
    let mut back: Vec<(String, String)> = Vec::new();
    let mut theirs = theirs.clone();
    let bsys = system_list(base);
    let osys = system_list(Some(ours));
    let Node::Map(root) = theirs.bare_mut() else {
        return (theirs, back);
    };
    let Some((_, systems)) = root.iter_mut().find(|(k, _)| k == "system") else {
        return (theirs, back);
    };
    let Node::List(systems) = systems.bare_mut() else {
        return (theirs, back);
    };
    for sys in systems.iter_mut() {
        let Some(key) = identity("system", sys) else {
            continue;
        };
        let find = |l: &[&Node]| {
            l.iter()
                .find(|s| identity("system", s).as_deref() == Some(&key))
                .map(|s| (*s).clone())
        };
        let (Some(b), Some(o)) = (find(&bsys), find(&osys)) else {
            continue;
        };
        let accepted: BTreeSet<String> = needs_review(&b)
            .difference(&needs_review(&o))
            .cloned()
            .collect();
        let decls = |n: &Node| -> Vec<Node> {
            match get(Some(n), "privacy_declarations").map(Node::bare) {
                Some(Node::List(l)) => l.clone(),
                _ => Vec::new(),
            }
        };
        let (bd, od) = (decls(&b), decls(&o));
        let Node::Map(sm) = sys.bare_mut() else {
            continue;
        };
        let mut re_review: Vec<String> = Vec::new();
        if let Some((_, list)) = sm.iter_mut().find(|(k, _)| k == "privacy_declarations")
            && let Node::List(list) = list.bare_mut()
        {
            for d in list.iter_mut() {
                let Some(name) = identity("privacy_declarations", d) else {
                    continue;
                };
                if !accepted.contains(&name) {
                    continue;
                }
                let bdec = bd
                    .iter()
                    .find(|x| identity("privacy_declarations", x).as_deref() == Some(&name));
                let odec = od
                    .iter()
                    .find(|x| identity("privacy_declarations", x).as_deref() == Some(&name));
                let Node::Map(dm) = d.bare_mut() else {
                    continue;
                };
                for owned in ["data_use", "data_subjects"] {
                    if let Some(ov) = get(odec, owned)
                        && let Some(slot) = dm.iter_mut().find(|(k, _)| k == owned)
                    {
                        slot.1 = ov.clone();
                    }
                }
                let changed = ["data_categories", "egress", "ingress", "dataset_references"]
                    .iter()
                    .any(|k| {
                        let tv = dm.iter().find(|(kk, _)| kk == k).map(|(_, v)| v);
                        match (get(bdec, k), tv) {
                            (Some(x), Some(y)) => !same(x, y),
                            (None, None) => false,
                            _ => true,
                        }
                    });
                if changed {
                    re_review.push(name.clone());
                    back.push((key.clone(), name.clone()));
                    report.re_review.push(format!("{key}: {name}"));
                }
            }
        }
        // Back under review: in theirs' list, and (as base had it) not removed by the merge.
        if !re_review.is_empty() {
            let mut names: Vec<String> = needs_review(&o).into_iter().collect();
            names.extend(re_review);
            names.sort();
            names.dedup();
            set_needs_review(sm, names);
        } else {
            // Keep the person's acceptances: theirs lists what base listed minus accepted.
            let mut names: Vec<String> = needs_review(&Node::Map(sm.clone()))
                .difference(&accepted)
                .cloned()
                .collect();
            names.sort();
            set_needs_review(sm, names);
        }
    }
    (theirs, back)
}

/// A system's `meta.privacy_flow`, made if it is not there.
pub(crate) fn privacy_flow_mut(sm: &mut Vec<(String, Node)>) -> Option<&mut Vec<(String, Node)>> {
    let mi = match sm.iter().position(|(k, _)| k == "meta") {
        Some(i) => i,
        None => {
            let at = sm
                .iter()
                .position(|(k, _)| k == "system_type")
                .map_or(sm.len(), |i| i + 1);
            sm.insert(at, ("meta".into(), Node::Map(Vec::new())));
            at
        }
    };
    let Node::Map(mm) = sm[mi].1.bare_mut() else {
        return None;
    };
    let pi = match mm.iter().position(|(k, _)| k == "privacy_flow") {
        Some(i) => i,
        None => {
            mm.push(("privacy_flow".into(), Node::Map(Vec::new())));
            mm.len() - 1
        }
    };
    match mm[pi].1.bare_mut() {
        Node::Map(pm) => Some(pm),
        _ => None,
    }
}

pub(crate) fn set_needs_review(sm: &mut Vec<(String, Node)>, names: Vec<String>) {
    let list = Node::strs(names.clone());
    let pf = Node::Map(vec![("needs_review".into(), list)]);
    match sm.iter_mut().find(|(k, _)| k == "meta") {
        Some((_, meta)) => {
            if let Node::Map(mm) = meta.bare_mut() {
                match mm.iter_mut().find(|(k, _)| k == "privacy_flow") {
                    Some((_, p)) => {
                        // In place: the keys keep their order across runs.
                        if let Node::Map(pm) = p.bare_mut() {
                            match pm.iter().position(|(k, _)| k == "needs_review") {
                                Some(i) if names.is_empty() => {
                                    pm.remove(i);
                                }
                                Some(i) => pm[i].1 = Node::strs(names),
                                None if !names.is_empty() => {
                                    pm.push(("needs_review".into(), Node::strs(names)))
                                }
                                None => {}
                            }
                        }
                    }
                    None if !names.is_empty() => mm.push(("privacy_flow".into(), pf)),
                    None => {}
                }
            }
        }
        None if !names.is_empty() => {
            let at = sm
                .iter()
                .position(|(k, _)| k == "system_type")
                .map_or(sm.len(), |i| i + 1);
            sm.insert(
                at,
                ("meta".into(), Node::Map(vec![("privacy_flow".into(), pf)])),
            );
        }
        None => {}
    }
}

/// After a merge: declarations name only the system's own DataFlows, and placeholder systems
/// nothing reaches any more go.
fn tidy(mut root: Node, report: &mut Report) -> Node {
    let Node::Map(r) = root.bare_mut() else {
        return root;
    };
    let Some((_, systems)) = r.iter_mut().find(|(k, _)| k == "system") else {
        return root;
    };
    let Node::List(systems) = systems.bare_mut() else {
        return root;
    };
    let flow_keys = |sys: &Node, dir: &str| -> BTreeSet<String> {
        match get(Some(sys), dir).map(Node::bare) {
            Some(Node::List(l)) => l
                .iter()
                .filter_map(|f| get(Some(f), "fides_key"))
                .filter_map(|k| match k.bare() {
                    Node::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => BTreeSet::new(),
        }
    };
    let referenced: BTreeSet<String> = systems
        .iter()
        .flat_map(|s| {
            flow_keys(s, "egress")
                .into_iter()
                .chain(flow_keys(s, "ingress"))
        })
        .collect();
    systems.retain(|s| {
        let placeholder = matches!(
            get(get(get(Some(s), "meta"), "privacy_flow"), "unresolved").map(Node::bare),
            Some(Node::Bool(true))
        );
        let key = identity("system", s).unwrap_or_default();
        let keep = !placeholder || referenced.contains(&key);
        if !keep {
            report
                .removed
                .push(format!("system[{key}] (nothing reaches it)"));
        }
        keep
    });
    for sys in systems.iter_mut() {
        // An empty review list says nothing.
        if let Node::Map(sm) = sys.bare_mut()
            && let Some((_, meta)) = sm.iter_mut().find(|(k, _)| k == "meta")
            && let Node::Map(mm) = meta.bare_mut()
            && let Some((_, pf)) = mm.iter_mut().find(|(k, _)| k == "privacy_flow")
            && let Node::Map(pm) = pf.bare_mut()
        {
            pm.retain(|(k, v)| {
                !(k == "needs_review" && matches!(v.bare(), Node::List(l) if l.is_empty()))
            });
        }
        let key = identity("system", sys).unwrap_or_default();
        let out_keys = flow_keys(sys, "egress");
        let in_keys = flow_keys(sys, "ingress");
        let Node::Map(sm) = sys.bare_mut() else {
            continue;
        };
        let Some((_, decls)) = sm.iter_mut().find(|(k, _)| k == "privacy_declarations") else {
            continue;
        };
        let Node::List(decls) = decls.bare_mut() else {
            continue;
        };
        for d in decls.iter_mut() {
            let name = identity("privacy_declarations", d).unwrap_or_default();
            let Node::Map(dm) = d.bare_mut() else {
                continue;
            };
            for (dir, keys) in [("egress", &out_keys), ("ingress", &in_keys)] {
                if let Some(pos) = dm.iter().position(|(k, _)| k == dir)
                    && let Node::List(l) = dm[pos].1.bare_mut()
                {
                    l.retain(|x| {
                        let ok = matches!(x.bare(), Node::Str(s) if keys.contains(s));
                        if !ok {
                            report
                                .dangling
                                .push(format!("{key}: {name}.{dir} {}", short(x)));
                        }
                        ok
                    });
                    if l.is_empty() {
                        dm.remove(pos);
                    }
                }
            }
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::merge;
    use crate::datamap::yaml::{Node, parse, render};

    fn y(s: &str) -> Node {
        parse(s).unwrap()
    }

    #[test]
    fn edits_survive_and_untouched_values_follow_the_code() {
        let base = y(
            "system:\n  - fides_key: web\n    system_type: Service\n    egress:\n      - fides_key: a\n        type: system\n        data_categories: [user.name]\n      - fides_key: b\n        type: system\n    privacy_declarations: []\n",
        );
        // A person changed the system type, deleted egress b, added a legal field.
        let ours = y(
            "system:\n  - fides_key: web\n    system_type: Web service\n    egress:\n      - fides_key: a\n        type: system\n        data_categories: [user.name]\n    dpo: privacy@example.com\n    privacy_declarations: []\n",
        );
        // The code now also sends an email to a, and still to b.
        let theirs = y(
            "system:\n  - fides_key: web\n    system_type: Service\n    egress:\n      - fides_key: a\n        type: system\n        data_categories: [user.name, user.contact.email]\n      - fides_key: b\n        type: system\n    privacy_declarations: []\n",
        );
        let (m, r) = merge(Some(&base), Some(&ours), &theirs);
        let text = render(&[], &m);
        assert!(text.contains("system_type: \"Web service\""), "{text}");
        assert!(text.contains("\"user.contact.email\""), "{text}");
        assert!(!text.contains("fides_key: \"b\""), "{text}");
        assert!(text.contains("dpo: \"privacy@example.com\""), "{text}");
        assert_eq!(r.kept_deleted, [".system[web].egress[system:b]"]);
        // Nothing changed: merging again is a fixpoint.
        let (again, _) = merge(Some(&theirs), Some(&m), &theirs);
        assert_eq!(render(&[], &again), text);
    }

    #[test]
    fn untouched_elements_the_code_dropped_go_and_changed_ones_stay() {
        let base = y(
            "dataset:\n  - fides_key: d\n    collections:\n      - name: users\n        fields:\n          - name: email\n            data_categories: [user.contact.email]\n          - name: phone\n",
        );
        let ours = y(
            "dataset:\n  - fides_key: d\n    collections:\n      - name: users\n        fields:\n          - name: email\n            data_categories: [user.contact.email]\n          - name: phone\n            data_categories: [user.contact.phone_number]\n",
        );
        let theirs = y(
            "dataset:\n  - fides_key: d\n    collections:\n      - name: users\n        fields: []\n",
        );
        let (m, r) = merge(Some(&base), Some(&ours), &theirs);
        let text = render(&[], &m);
        assert!(!text.contains("email"), "{text}");
        assert!(text.contains("phone"), "{text}");
        assert_eq!(r.removed.len(), 1);
        assert_eq!(r.kept_unobserved.len(), 1);
    }

    #[test]
    fn accepted_declarations_keep_their_use_and_return_when_evidence_grows() {
        let base = y(
            "system:\n  - fides_key: web\n    meta:\n      privacy_flow:\n        needs_review: [Analytics for Reporting]\n    egress:\n      - fides_key: posthog\n        type: system\n    privacy_declarations:\n      - name: Analytics for Reporting\n        data_use: analytics.reporting\n        data_categories: [user.name]\n        egress: [posthog]\n",
        );
        // Accepted, with the data use changed by the person.
        let ours = y(
            "system:\n  - fides_key: web\n    meta:\n      privacy_flow: {}\n    egress:\n      - fides_key: posthog\n        type: system\n    privacy_declarations:\n      - name: Analytics for Reporting\n        data_use: analytics.reporting.ad_performance\n        data_categories: [user.name]\n        egress: [posthog]\n",
        );
        let same = base.clone();
        let (m, r) = merge(Some(&base), Some(&ours), &same);
        let text = render(&[], &m);
        assert!(
            text.contains("analytics.reporting.ad_performance"),
            "{text}"
        );
        assert!(!text.contains("needs_review"), "{text}");
        assert!(r.re_review.is_empty());
        let grown = y(
            "system:\n  - fides_key: web\n    meta:\n      privacy_flow:\n        needs_review: [Analytics for Reporting]\n    egress:\n      - fides_key: posthog\n        type: system\n    privacy_declarations:\n      - name: Analytics for Reporting\n        data_use: analytics.reporting\n        data_categories: [user.name, user.contact.email]\n        egress: [posthog]\n",
        );
        let (m, r) = merge(Some(&base), Some(&ours), &grown);
        let text = render(&[], &m);
        assert!(
            text.contains("analytics.reporting.ad_performance"),
            "{text}"
        );
        assert!(text.contains("needs_review"), "{text}");
        assert_eq!(r.re_review.len(), 1);
    }

    #[test]
    fn declarations_drop_references_to_deleted_flows() {
        let base = y(
            "system:\n  - fides_key: web\n    egress:\n      - fides_key: unresolved_http_x\n        type: system\n    privacy_declarations:\n      - name: Essential for Service\n        data_use: essential.service\n        data_categories: []\n        egress: [unresolved_http_x]\n  - fides_key: unresolved_http_x\n    system_type: Unresolved destination\n    meta:\n      privacy_flow:\n        unresolved: true\n    privacy_declarations: []\n",
        );
        // The person replaced the placeholder with the real system.
        let ours = y(
            "system:\n  - fides_key: web\n    egress:\n      - fides_key: orders_api\n        type: system\n    privacy_declarations:\n      - name: Essential for Service\n        data_use: essential.service\n        data_categories: []\n        egress: [unresolved_http_x]\n  - fides_key: unresolved_http_x\n    system_type: Unresolved destination\n    meta:\n      privacy_flow:\n        unresolved: true\n    privacy_declarations: []\n",
        );
        let (m, r) = merge(Some(&base), Some(&ours), &base);
        let text = render(&[], &m);
        assert!(!text.contains("unresolved_http_x"), "{text}");
        assert!(text.contains("orders_api"), "{text}");
        assert!(!r.dangling.is_empty());
    }
}
