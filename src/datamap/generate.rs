//! The data map the code shows, before any merge: one Fides system per system of the
//! repository, one per processor and one per destination the code does not name, with the
//! egress and ingress between them.

use std::collections::{BTreeMap, BTreeSet};

use crate::classify::UNKNOWN;
use crate::datamap::yaml::Node;
use crate::report::Document;
use crate::topology::model::{DatasetRec, EdgeRec, Endpoint, FieldRec, Topology};

/// What each receiving key is, for its own system entry.
enum Receiver {
    System,
    Processor {
        name: String,
    },
    Unresolved {
        class: String,
        sites: BTreeSet<(String, u32)>,
    },
}

/// One DataFlow: its key, categories, and the comment lines saying where it comes from.
#[derive(Default)]
struct Flow {
    categories: BTreeSet<String>,
    unknown: bool,
    labels: BTreeSet<String>,
}

impl Flow {
    fn add(&mut self, edge: &EdgeRec) {
        for c in &edge.categories {
            if c == UNKNOWN {
                self.unknown = true;
            } else {
                self.categories.insert(c.clone());
            }
        }
    }

    fn node(self, key: &str) -> Node {
        self.node_of(key, "system")
    }

    fn node_of(self, key: &str, kind: &str) -> Node {
        let mut notes: Vec<String> = self.labels.into_iter().collect();
        if self.unknown {
            notes.push(
                "also data of unknown category (request input not narrowed by a type): review"
                    .into(),
            );
        }
        let mut m = vec![
            ("fides_key".to_string(), Node::str(key)),
            ("type".to_string(), Node::str(kind)),
        ];
        if !self.categories.is_empty() {
            m.push(("data_categories".into(), Node::strs(self.categories)));
        }
        Node::noted(notes, Node::Map(m))
    }
}

/// What marks a data map as one `piiflow datamap` maintains (its first header line).
pub const MARKER: &str = "written by `piiflow datamap`";

/// The comment at the top of a generated data map.
pub const HEADER: &[&str] = &[
    "A Fides data map (Fideslang 3.1.3) of this repository, written by `piiflow datamap`.",
    "Systems, datasets, egress and ingress come from the code; lines starting with \"#:\" say where.",
    "Keys starting with unresolved_ are destinations the code does not name: replace each with",
    "the receiving system's key, or declare it in .privacy-flow.yml and run again.",
    "Privacy declarations are PROPOSALS: their data use and data subjects are suggested from the",
    "evidence, and each system lists them under meta.privacy_flow.needs_review until a person",
    "decides. No legal basis is proposed.",
];

const SHOWN: usize = 5;

/// How a sink class reads in a comment or a name.
fn class_label(class: &str) -> &str {
    match class {
        "http" => "HTTP",
        "llm" => "LLM",
        "error_tracking" => "error-tracking",
        "browser_storage" => "browser-storage",
        "device_storage" => "device-storage",
        "third_party" => "third-party",
        c => c,
    }
}

fn sites(list: &BTreeSet<(String, u32)>) -> String {
    let mut s: Vec<String> = list
        .iter()
        .take(SHOWN)
        .map(|(p, l)| format!("{p}:{l}"))
        .collect();
    if list.len() > SHOWN {
        s.push(format!("and {} more", list.len() - SHOWN));
    }
    s.join(", ")
}

/// The generated data map of a document that has a topology.
pub fn generate(doc: &Document) -> Node {
    let Some(topo) = &doc.topology else {
        return Node::Map(vec![("system".into(), Node::List(Vec::new()))]);
    };
    let sink_at: BTreeMap<&str, (String, u32)> = doc
        .sinks
        .iter()
        .map(|s| (s.id.as_str(), (s.location.path.clone(), s.location.line)))
        .collect();
    let unresolved = unresolved_keys(topo);
    let key_of = |to: &Endpoint| -> String {
        match to {
            Endpoint::System { key } => key.clone(),
            Endpoint::Processor { fides_key, .. } => fides_key.clone(),
            Endpoint::Unresolved { class, path } => {
                unresolved[&(class.as_str().to_string(), path.clone())].clone()
            }
        }
    };

    let mut receivers: BTreeMap<String, Receiver> = BTreeMap::new();
    let mut egress: BTreeMap<String, BTreeMap<String, Flow>> = BTreeMap::new();
    let mut ingress: BTreeMap<String, BTreeMap<String, Flow>> = BTreeMap::new();
    for s in &topo.systems {
        receivers.insert(s.key.clone(), Receiver::System);
    }
    for e in &topo.edges {
        let to = key_of(&e.to);
        let at: BTreeSet<(String, u32)> = e
            .sinks
            .iter()
            .filter_map(|s| sink_at.get(s.as_str()).cloned())
            .collect();
        let label = match &e.to {
            Endpoint::Processor { name, .. } => {
                receivers
                    .entry(to.clone())
                    .or_insert_with(|| Receiver::Processor { name: name.clone() });
                format!(
                    "{name} via {}{}: {}",
                    e.class.as_str(),
                    if e.declared { "" } else { " (undeclared)" },
                    sites(&at)
                )
            }
            Endpoint::System { key } => {
                format!(
                    "to {key} over {}: {}",
                    class_label(e.class.as_str()),
                    sites(&at)
                )
            }
            Endpoint::Unresolved { class, .. } => {
                let r = receivers
                    .entry(to.clone())
                    .or_insert_with(|| Receiver::Unresolved {
                        class: class.as_str().to_string(),
                        sites: BTreeSet::new(),
                    });
                if let Receiver::Unresolved { sites, .. } = r {
                    sites.extend(at.iter().cloned());
                }
                format!(
                    "unresolved {} destination, sent from {}",
                    class_label(class.as_str()),
                    sites(&at)
                )
            }
        };
        let out = egress
            .entry(e.from.clone())
            .or_default()
            .entry(to.clone())
            .or_default();
        out.add(e);
        out.labels.insert(label);
        let inn = ingress
            .entry(to)
            .or_default()
            .entry(e.from.clone())
            .or_default();
        inn.add(e);
    }

    let systems_by_key: BTreeMap<&str, &crate::topology::model::SystemRec> =
        topo.systems.iter().map(|s| (s.key.as_str(), s)).collect();
    let mut items = Vec::new();
    for (key, r) in &receivers {
        let mut m: Vec<(String, Node)> = vec![("fides_key".into(), Node::str(key))];
        let mut notes = Vec::new();
        match r {
            Receiver::System => {
                let s = systems_by_key[key.as_str()];
                m.push(("name".into(), Node::str(&s.name)));
                m.push(("system_type".into(), Node::str("Service")));
                let review: Vec<String> = topo
                    .declarations
                    .iter()
                    .filter(|d| d.system == *key && d.needs_review)
                    .map(|d| d.name.clone())
                    .collect();
                if !review.is_empty() {
                    m.push((
                        "meta".into(),
                        Node::Map(vec![(
                            "privacy_flow".into(),
                            Node::Map(vec![("needs_review".into(), Node::strs(review))]),
                        )]),
                    ));
                }
                if !s.datasets.is_empty() {
                    m.push((
                        "dataset_references".into(),
                        Node::strs(s.datasets.iter().cloned()),
                    ));
                }
                let root = if s.root.is_empty() {
                    "the repository root".to_string()
                } else {
                    s.root.clone()
                };
                let why: Vec<String> = s
                    .evidence
                    .iter()
                    .filter(|e| e.rule != "alias")
                    .map(|e| format!("{}: {}", e.rule, e.path))
                    .collect();
                notes.push(if s.declared && s.kind != "repository" {
                    format!(
                        "declared under systems: in .privacy-flow.yml ({})",
                        s.evidence
                            .iter()
                            .map(|e| e.path.clone())
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                } else if s.kind == "repository" {
                    "the repository as one system: no deployable service was detected".to_string()
                } else {
                    format!("{} rooted at {root} ({})", s.kind, why.join("; "))
                });
                let named: Vec<String> = s
                    .evidence
                    .iter()
                    .filter(|e| e.rule == "alias")
                    .map(|e| e.path.clone())
                    .collect();
                if !s.aliases.is_empty() {
                    notes.push(format!(
                        "reached as {}{}",
                        s.aliases.join(", "),
                        if named.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", named.join(", "))
                        }
                    ));
                }
            }
            Receiver::Processor { name } => {
                m.push(("name".into(), Node::str(name)));
                m.push(("system_type".into(), Node::str("Third Party")));
                notes.push(format!("third-party processor {name}"));
            }
            Receiver::Unresolved { class, sites: at } => {
                m.push((
                    "name".into(),
                    Node::str(format!("Unresolved {} destination", class_label(class))),
                ));
                m.push(("system_type".into(), Node::str("Unresolved destination")));
                m.push((
                    "meta".into(),
                    Node::Map(vec![(
                        "privacy_flow".into(),
                        Node::Map(vec![("unresolved".into(), Node::Bool(true))]),
                    )]),
                ));
                notes.push(format!(
                    "{} destination the code does not name, sent from {}",
                    class_label(class),
                    sites(at)
                ));
            }
        }
        if let Some(out) = egress.remove(key) {
            m.push((
                "egress".into(),
                Node::List(out.into_iter().map(|(k, f)| f.node(&k)).collect()),
            ));
        }
        let mut inn: Vec<Node> = ingress
            .remove(key)
            .map(|inn| inn.into_iter().map(|(k, f)| f.node(&k)).collect())
            .unwrap_or_default();
        if let (Receiver::System, Some(s)) = (r, systems_by_key.get(key.as_str()))
            && s.user_input.sources > 0
        {
            let mut f = Flow::default();
            for c in &s.user_input.categories {
                if c == UNKNOWN {
                    f.unknown = true;
                } else {
                    f.categories.insert(c.clone());
                }
            }
            f.labels.insert(format!(
                "request input to its handlers: {}{}",
                s.user_input.sites.join(", "),
                if s.user_input.sources as usize > s.user_input.sites.len() {
                    " and more"
                } else {
                    ""
                }
            ));
            inn.push(f.node_of("user", "user"));
        }
        if !inn.is_empty() {
            m.push(("ingress".into(), Node::List(inn)));
        }
        let decls: Vec<Node> = topo
            .declarations
            .iter()
            .filter(|d| d.system == *key)
            .map(|d| declaration(d, &key_of))
            .collect();
        m.push(("privacy_declarations".into(), Node::List(decls)));
        items.push(Node::noted(notes, Node::Map(m)));
    }
    let mut root = Vec::new();
    if !topo.datasets.is_empty() {
        root.push((
            "dataset".to_string(),
            Node::List(topo.datasets.iter().map(dataset).collect()),
        ));
    }
    root.push(("system".into(), Node::List(items)));
    Node::Map(root)
}

/// A proposed privacy declaration, with what it rests on.
fn declaration(
    d: &crate::topology::model::DeclarationRec,
    key_of: &dyn Fn(&Endpoint) -> String,
) -> Node {
    let mut m: Vec<(String, Node)> = vec![
        ("name".into(), Node::str(&d.name)),
        ("data_use".into(), Node::str(&d.data_use)),
        (
            "data_subjects".into(),
            Node::strs(d.data_subjects.iter().cloned()),
        ),
        (
            "data_categories".into(),
            Node::strs(d.data_categories.iter().cloned()),
        ),
    ];
    if !d.datasets.is_empty() {
        m.push((
            "dataset_references".into(),
            Node::strs(d.datasets.iter().cloned()),
        ));
    }
    let egress: BTreeSet<String> = d.egress.iter().map(key_of).collect();
    if !egress.is_empty() {
        m.push(("egress".into(), Node::strs(egress)));
    }
    if !d.ingress.is_empty() {
        m.push(("ingress".into(), Node::strs(d.ingress.iter().cloned())));
    }
    let mut notes = vec![format!(
        "PROPOSED {} with data subjects {}, from the code: review it",
        d.data_use,
        d.data_subjects.join(", ")
    )];
    notes.extend(d.basis.iter().cloned());
    Node::noted(notes, Node::Map(m))
}

/// A Fides dataset: collections and fields, classified fields cited.
fn dataset(d: &DatasetRec) -> Node {
    let mut review: Vec<String> = Vec::new();
    for c in &d.collections {
        collect_review(&c.name, &c.fields, &mut review);
    }
    let mut m: Vec<(String, Node)> = vec![
        ("fides_key".into(), Node::str(&d.key)),
        ("name".into(), Node::str(&d.name)),
    ];
    if !review.is_empty() {
        m.push((
            "meta".into(),
            Node::Map(vec![(
                "privacy_flow".into(),
                Node::Map(vec![("needs_review".into(), Node::strs(review))]),
            )]),
        ));
    }
    m.push((
        "collections".into(),
        Node::List(
            d.collections
                .iter()
                .map(|c| {
                    Node::noted(
                        vec![format!("{} ({})", c.location, c.entity)],
                        Node::Map(vec![
                            ("name".into(), Node::str(&c.name)),
                            ("fields".into(), fields(&c.fields)),
                        ]),
                    )
                })
                .collect(),
        ),
    ));
    let mut why = vec![format!("{} at {}", d.store, d.location)];
    if let Some(db) = &d.database {
        why.push(format!("database {db}"));
    }
    if let Some(sc) = &d.schema {
        why.push(format!("schema {sc}"));
    }
    Node::noted(vec![why.join("; ")], Node::Map(m))
}

fn fields(list: &[FieldRec]) -> Node {
    Node::List(
        list.iter()
            .map(|f| {
                let mut m: Vec<(String, Node)> = vec![("name".into(), Node::str(&f.name))];
                if !f.categories.is_empty() {
                    m.push((
                        "data_categories".into(),
                        Node::strs(f.categories.iter().cloned()),
                    ));
                }
                if !f.fields.is_empty() {
                    m.push(("fields".into(), fields(&f.fields)));
                }
                let mut notes = Vec::new();
                if !f.categories.is_empty() || f.needs_review {
                    let mut n = f.location.clone().unwrap_or_default();
                    if let Some(member) = &f.member {
                        n = format!("{n} ({member})");
                    }
                    if let Some(by) = &f.classified_by {
                        n = format!(
                            "{}{}{}",
                            n,
                            if n.is_empty() { "" } else { "; " },
                            if f.needs_review {
                                format!("maybe personal by {by}: review")
                            } else {
                                format!("classified by {by}")
                            }
                        );
                    }
                    notes.push(n.trim().to_string());
                }
                Node::noted(notes, Node::Map(m))
            })
            .collect(),
    )
}

fn collect_review(prefix: &str, list: &[FieldRec], out: &mut Vec<String>) {
    for f in list {
        let path = format!("{prefix}.{}", f.name);
        if f.needs_review {
            out.push(path.clone());
        }
        collect_review(&path, &f.fields, out);
    }
}

/// `unresolved_<class>_<file>` for every unresolved endpoint, distinct across the map.
fn unresolved_keys(topo: &Topology) -> BTreeMap<(String, String), String> {
    let groups: BTreeSet<(String, String)> = topo
        .edges
        .iter()
        .filter_map(|e| match &e.to {
            Endpoint::Unresolved { class, path } => {
                Some((class.as_str().to_string(), path.clone()))
            }
            _ => None,
        })
        .collect();
    let keys =
        crate::output::fides::unresolved_keys(groups.iter().map(|(c, p)| (c.as_str(), p.as_str())));
    keys.into_iter()
        .map(|((c, p), k)| ((c.to_string(), p.to_string()), k))
        .collect()
}
