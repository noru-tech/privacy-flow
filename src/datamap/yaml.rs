//! A data map as a tree, written as block YAML in a fixed shape: every string double-quoted,
//! keys in the order the tree has them, piiflow's own comments as `#:` lines above the element
//! they describe.

use crate::output::yaml::q;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Str(String),
    Bool(bool),
    /// A number, as written.
    Num(String),
    Null,
    List(Vec<Node>),
    Map(Vec<(String, Node)>),
    /// `#:` comment lines, then the node (a list item or a map value).
    Noted(Vec<String>, Box<Node>),
}

impl Node {
    pub fn str(s: impl Into<String>) -> Node {
        Node::Str(s.into())
    }

    pub fn strs<I: IntoIterator<Item = S>, S: Into<String>>(items: I) -> Node {
        Node::List(items.into_iter().map(|s| Node::Str(s.into())).collect())
    }

    pub fn noted(notes: Vec<String>, node: Node) -> Node {
        if notes.is_empty() {
            node
        } else {
            Node::Noted(notes, Box::new(node))
        }
    }

    /// The node under its notes.
    pub fn bare(&self) -> &Node {
        match self {
            Node::Noted(_, n) => n.bare(),
            n => n,
        }
    }

    /// The node under its notes, mutably.
    pub fn bare_mut(&mut self) -> &mut Node {
        match self {
            Node::Noted(_, n) => n.bare_mut(),
            n => n,
        }
    }

    fn notes(&self) -> &[String] {
        match self {
            Node::Noted(n, _) => n,
            _ => &[],
        }
    }
}

/// Render a top-level map, after `header` lines (each written as `# line`).
pub fn render(header: &[String], root: &Node) -> String {
    let mut out = String::new();
    for h in header {
        if h.is_empty() {
            out.push_str("#\n");
        } else {
            out.push_str("# ");
            out.push_str(h);
            out.push('\n');
        }
    }
    if let Node::Map(entries) = root.bare() {
        write_map(&mut out, entries, 0);
    }
    out
}

fn pad(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

fn write_notes(out: &mut String, notes: &[String], indent: usize) {
    for n in notes {
        pad(out, indent);
        out.push_str("#: ");
        out.push_str(n);
        out.push('\n');
    }
}

fn scalar(n: &Node) -> Option<String> {
    match n {
        Node::Str(s) => Some(q(s)),
        Node::Bool(b) => Some(b.to_string()),
        Node::Num(n) => Some(n.clone()),
        Node::Null => Some("null".into()),
        Node::List(items) if items.is_empty() => Some("[]".into()),
        Node::Map(entries) if entries.is_empty() => Some("{}".into()),
        _ => None,
    }
}

fn write_map(out: &mut String, entries: &[(String, Node)], indent: usize) {
    for (k, v) in entries {
        write_notes(out, v.notes(), indent);
        let v = v.bare();
        pad(out, indent);
        out.push_str(k);
        out.push(':');
        match scalar(v) {
            Some(s) => {
                out.push(' ');
                out.push_str(&s);
                out.push('\n');
            }
            None => {
                out.push('\n');
                match v {
                    Node::List(items) => write_list(out, items, indent + 2),
                    Node::Map(m) => write_map(out, m, indent + 2),
                    _ => {}
                }
            }
        }
    }
}

fn write_list(out: &mut String, items: &[Node], indent: usize) {
    for item in items {
        write_notes(out, item.notes(), indent);
        let item = item.bare();
        pad(out, indent);
        out.push('-');
        match (scalar(item), item) {
            (Some(s), _) => {
                out.push(' ');
                out.push_str(&s);
                out.push('\n');
            }
            (None, Node::Map(entries)) => {
                // The first entry shares the dash's line; the rest align under it.
                let mut first = String::new();
                write_map(&mut first, &entries[..1], indent + 2);
                out.push(' ');
                out.push_str(first.trim_start());
                write_map(out, &entries[1..], indent + 2);
            }
            (None, Node::List(inner)) => {
                out.push('\n');
                write_list(out, inner, indent + 2);
            }
            _ => out.push('\n'),
        }
    }
}

/// Parse YAML (or JSON) into a tree, keeping the order of every map's keys. Anchors and aliases
/// are expanded; comments are not kept.
pub fn parse(text: &str) -> anyhow::Result<Node> {
    Ok(serde_saphyr::from_str::<Node>(text)?)
}

impl<'de> serde::Deserialize<'de> for Node {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        d.deserialize_any(NodeVisitor)
    }
}

struct NodeVisitor;

impl<'de> serde::de::Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a YAML value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
        Ok(Node::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
        Ok(Node::Num(v.to_string()))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
        Ok(Node::Num(v.to_string()))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Node, E> {
        Ok(Node::Num(v.to_string()))
    }
    fn visit_str<E>(self, v: &str) -> Result<Node, E> {
        Ok(Node::Str(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Node, E> {
        Ok(Node::Str(v))
    }
    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }
    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }
    fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        serde::Deserialize::deserialize(d)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element::<Node>()? {
            out.push(v);
        }
        Ok(Node::List(out))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut out: Vec<(String, Node)> = Vec::new();
        while let Some(k) = map.next_key::<Node>()? {
            let key = match k {
                Node::Str(s) | Node::Num(s) => s,
                Node::Bool(b) => b.to_string(),
                Node::Null => "null".into(),
                _ => return Err(serde::de::Error::custom("a map key must be a scalar")),
            };
            let v = map.next_value::<Node>()?;
            match out.iter_mut().find(|(k2, _)| *k2 == key) {
                Some(slot) => slot.1 = v,
                None => out.push((key, v)),
            }
        }
        Ok(Node::Map(out))
    }
}

/// The tree as JSON (notes dropped), for the lock.
pub fn to_json(n: &Node) -> serde_json::Value {
    use serde_json::Value;
    match n.bare() {
        Node::Str(s) => Value::String(s.clone()),
        Node::Bool(b) => Value::Bool(*b),
        Node::Num(s) => s
            .parse::<serde_json::Number>()
            .map(Value::Number)
            .unwrap_or_else(|_| Value::String(s.clone())),
        Node::Null => Value::Null,
        Node::List(items) => Value::Array(items.iter().map(to_json).collect()),
        Node::Map(entries) => Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
        Node::Noted(_, inner) => to_json(inner),
    }
}

/// A tree from JSON (the lock); keys come back sorted, which the merge does not depend on.
pub fn from_json(v: &serde_json::Value) -> Node {
    use serde_json::Value;
    match v {
        Value::String(s) => Node::Str(s.clone()),
        Value::Bool(b) => Node::Bool(*b),
        Value::Number(n) => Node::Num(n.to_string()),
        Value::Null => Node::Null,
        Value::Array(a) => Node::List(a.iter().map(from_json).collect()),
        Value::Object(m) => Node::Map(m.iter().map(|(k, v)| (k.clone(), from_json(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Node, parse, render};

    #[test]
    fn parse_keeps_order_and_round_trips() {
        let text =
            "system:\n  - fides_key: \"web\"\n    zeta: 1\n    alpha: null\n    flag: true\n";
        let n = parse(text).unwrap();
        assert_eq!(render(&[], &n), text);
    }

    #[test]
    fn block_yaml() {
        let root = Node::Map(vec![(
            "system".into(),
            Node::List(vec![Node::noted(
                vec!["a service".into()],
                Node::Map(vec![
                    ("fides_key".into(), Node::str("web")),
                    ("egress".into(), Node::List(vec![])),
                    (
                        "meta".into(),
                        Node::Map(vec![("flag".into(), Node::Bool(true))]),
                    ),
                    ("data_categories".into(), Node::strs(["user.name"])),
                ]),
            )]),
        )]);
        assert_eq!(
            render(&["head".into()], &root),
            "# head\nsystem:\n  #: a service\n  - fides_key: \"web\"\n    egress: []\n    meta:\n      flag: true\n    data_categories:\n      - \"user.name\"\n"
        );
    }
}
