//! A data map as a tree, written as block YAML in a fixed shape: every string double-quoted,
//! keys in the order the tree has them, piiflow's own comments as `#:` lines above the element
//! they describe.

use crate::output::yaml::q;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Str(String),
    Bool(bool),
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

#[cfg(test)]
mod tests {
    use super::{Node, render};

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
