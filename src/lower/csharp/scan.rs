//! The declaration scan: what one C# file declares, before anything is lowered.
//!
//! C# resolves a name against the whole project: the types of every file in its namespace,
//! the `using` directives in scope, and the members a class inherits from a base declared in
//! another file. So every file is scanned first (in parallel), the scans are merged into a
//! [`super::index::CsIndex`], and only then is each file lowered against it.
//!
//! Namespace scopes and types are keyed by the byte offset where they start, so the lowering
//! finds the scan's record for the node it is looking at without walking in step with it.

use tree_sitter::{Node, Parser};

use crate::lower::named_children;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Using {
    /// `using A.B;`
    Namespace(String),
    /// `using static A.B.C;`: the static members of a type.
    Static(String),
    /// `using L = A.B.C;`
    Alias(String, String),
}

/// A namespace declaration (or the compilation unit, scope 0, named `""`).
#[derive(Clone, Debug, Default)]
pub struct NsScope {
    /// The full namespace name (`eShop.Ordering.API`).
    pub name: String,
    pub parent: Option<usize>,
    pub usings: Vec<Using>,
    /// Byte offset of the declaration (0 for the compilation unit).
    pub start: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Class,
    Struct,
    Record,
    Interface,
    Enum,
    Delegate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberKind {
    Field,
    Property,
    Method,
    Event,
    /// A parameter of a primary constructor: in scope in every member, like a field.
    PrimaryParam,
    Nested,
    EnumValue,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub kind: MemberKind,
    pub is_static: bool,
    /// An extension method (`static T M(this X x)`).
    pub is_extension: bool,
}

/// A type as written in a base list: `Basket.BasketBase`, `IRequestHandler<Cmd, bool>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeText {
    pub segs: Vec<String>,
    /// Type arguments of the last segment.
    pub arity: usize,
    /// `global::` prefix.
    pub global: bool,
}

#[derive(Clone, Debug)]
pub struct TypeScan {
    pub name: String,
    pub arity: usize,
    pub kind: TypeKind,
    /// The namespace scope it is declared in.
    pub scope: usize,
    /// The enclosing type in this file, for a nested type.
    pub outer: Option<usize>,
    pub partial: bool,
    pub is_static: bool,
    pub type_params: Vec<String>,
    pub bases: Vec<TypeText>,
    pub members: Vec<Member>,
    pub start: usize,
}

#[derive(Clone, Debug, Default)]
pub struct FileScan {
    pub path: String,
    pub scopes: Vec<NsScope>,
    pub types: Vec<TypeScan>,
    /// `global using` directives: they apply to every file of the project.
    pub global_usings: Vec<Using>,
}

pub fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("grammar version matches tree-sitter");
    parser
}

pub fn scan(path: &str, src: &str) -> FileScan {
    let mut out = FileScan {
        path: path.to_string(),
        scopes: vec![NsScope::default()],
        ..FileScan::default()
    };
    let Some(tree) = parser().parse(src, None) else {
        return out;
    };
    let mut s = Scanner { src, out: &mut out };
    s.unit(tree.root_node());
    out
}

struct Scanner<'a> {
    src: &'a str,
    out: &'a mut FileScan,
}

/// Whether a node has an anonymous child token with this text (`global`, `static`, `this`).
pub fn has_token(node: Node, src: &str, token: &str) -> bool {
    let mut c = node.walk();
    node.children(&mut c)
        .any(|ch| !ch.is_named() && &src[ch.byte_range()] == token)
}

pub fn modifiers(node: Node, src: &str) -> Vec<String> {
    named_children(node)
        .into_iter()
        .filter(|c| c.kind() == "modifier")
        .map(|c| src[c.byte_range()].trim().to_string())
        .collect()
}

/// The dotted text of a name (`A.B.C`, `global::A.B`), without type arguments.
pub fn name_text(node: Node, src: &str) -> String {
    type_text(node, src)
        .map(|t| t.segs.join("."))
        .unwrap_or_else(|| src[node.byte_range()].to_string())
}

/// A type reference as segments, stripped of type arguments, nullability and arrays.
pub fn type_text(node: Node, src: &str) -> Option<TypeText> {
    match node.kind() {
        "identifier" => Some(TypeText {
            segs: vec![src[node.byte_range()].to_string()],
            arity: 0,
            global: false,
        }),
        "generic_name" => {
            let name = named_children(node).into_iter().next()?;
            let arity = named_children(node)
                .into_iter()
                .find(|c| c.kind() == "type_argument_list")
                .map_or(0, |l| named_children(l).len().max(1));
            Some(TypeText {
                segs: vec![src[name.byte_range()].to_string()],
                arity,
                global: false,
            })
        }
        "qualified_name" => {
            let q = node.child_by_field_name("qualifier")?;
            let n = node.child_by_field_name("name")?;
            let mut left = type_text(q, src)?;
            let right = type_text(n, src)?;
            left.segs.extend(right.segs);
            left.arity = right.arity;
            Some(left)
        }
        "alias_qualified_name" => {
            // `global::A.B`: the alias is `global`.
            let n = node.child_by_field_name("name")?;
            let mut t = type_text(n, src)?;
            t.global = true;
            Some(t)
        }
        "nullable_type" | "array_type" | "ref_type" | "scoped_type" | "pointer_type" => {
            type_text(node.child_by_field_name("type")?, src)
        }
        "primary_constructor_base_type" => type_text(node.child_by_field_name("type")?, src),
        "predefined_type" => Some(TypeText {
            segs: vec![predefined(&src[node.byte_range()]).to_string()],
            arity: 0,
            global: false,
        }),
        _ => None,
    }
}

/// The type a C# keyword names.
pub fn predefined(keyword: &str) -> &str {
    match keyword {
        "string" => "System.String",
        "object" => "System.Object",
        "bool" => "System.Boolean",
        "int" => "System.Int32",
        "long" => "System.Int64",
        "short" => "System.Int16",
        "byte" => "System.Byte",
        "char" => "System.Char",
        "decimal" => "System.Decimal",
        "double" => "System.Double",
        "float" => "System.Single",
        "uint" => "System.UInt32",
        "ulong" => "System.UInt64",
        "ushort" => "System.UInt16",
        "sbyte" => "System.SByte",
        "nint" => "System.IntPtr",
        "nuint" => "System.UIntPtr",
        "void" => "System.Void",
        "dynamic" => "System.Object",
        other => other,
    }
}

impl Scanner<'_> {
    fn text(&self, n: Node) -> &str {
        &self.src[n.byte_range()]
    }

    fn unit(&mut self, root: Node) {
        let mut scope = 0;
        for c in named_children(root) {
            match c.kind() {
                "using_directive" => {
                    if let Some(u) = self.using(c) {
                        if has_token(c, self.src, "global") {
                            self.out.global_usings.push(u);
                        } else {
                            self.out.scopes[scope].usings.push(u);
                        }
                    }
                }
                "file_scoped_namespace_declaration" => {
                    let name = c
                        .child_by_field_name("name")
                        .map(|n| name_text(n, self.src))
                        .unwrap_or_default();
                    self.out.scopes.push(NsScope {
                        name,
                        parent: Some(0),
                        usings: Vec::new(),
                        start: c.start_byte(),
                    });
                    scope = self.out.scopes.len() - 1;
                }
                _ => self.decl(c, scope, None),
            }
        }
    }

    fn using(&self, node: Node) -> Option<Using> {
        let named = named_children(node);
        if let Some(alias) = node.child_by_field_name("name") {
            let target = named.iter().find(|c| **c != alias)?;
            return Some(Using::Alias(
                self.text(alias).to_string(),
                name_text(*target, self.src),
            ));
        }
        let target = named.into_iter().next()?;
        let name = name_text(target, self.src);
        if has_token(node, self.src, "static") {
            Some(Using::Static(name))
        } else {
            Some(Using::Namespace(name))
        }
    }

    fn decl(&mut self, node: Node, scope: usize, outer: Option<usize>) {
        match node.kind() {
            "namespace_declaration" => {
                let local = node
                    .child_by_field_name("name")
                    .map(|n| name_text(n, self.src))
                    .unwrap_or_default();
                let parent = &self.out.scopes[scope].name;
                let name = if parent.is_empty() {
                    local
                } else {
                    format!("{parent}.{local}")
                };
                self.out.scopes.push(NsScope {
                    name,
                    parent: Some(scope),
                    usings: Vec::new(),
                    start: node.start_byte(),
                });
                let inner = self.out.scopes.len() - 1;
                if let Some(body) = node.child_by_field_name("body") {
                    for c in named_children(body) {
                        if c.kind() == "using_directive" {
                            if let Some(u) = self.using(c) {
                                self.out.scopes[inner].usings.push(u);
                            }
                        } else {
                            self.decl(c, inner, None);
                        }
                    }
                }
            }
            "class_declaration"
            | "struct_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "delegate_declaration" => self.type_decl(node, scope, outer),
            "preproc_if" | "preproc_elif" | "preproc_else" => {
                for c in named_children(node) {
                    self.decl(c, scope, outer);
                }
            }
            _ => {}
        }
    }

    fn type_decl(&mut self, node: Node, scope: usize, outer: Option<usize>) {
        let kind = match node.kind() {
            "class_declaration" => TypeKind::Class,
            "struct_declaration" => TypeKind::Struct,
            "record_declaration" => TypeKind::Record,
            "interface_declaration" => TypeKind::Interface,
            "enum_declaration" => TypeKind::Enum,
            _ => TypeKind::Delegate,
        };
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        let mods = modifiers(node, self.src);
        let type_params: Vec<String> = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "type_parameter_list")
            .map(|l| {
                named_children(l)
                    .into_iter()
                    .filter_map(|p| p.child_by_field_name("name"))
                    .map(|n| self.text(n).to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut bases = Vec::new();
        if let Some(bl) = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "base_list")
        {
            for b in named_children(bl) {
                if let Some(t) = type_text(b, self.src) {
                    bases.push(t);
                }
            }
        }
        let idx = self.out.types.len();
        self.out.types.push(TypeScan {
            name: name.clone(),
            arity: type_params.len(),
            kind,
            scope,
            outer,
            partial: mods.iter().any(|m| m == "partial"),
            is_static: mods.iter().any(|m| m == "static"),
            type_params,
            bases,
            members: Vec::new(),
            start: node.start_byte(),
        });
        if let Some(o) = outer {
            self.out.types[o].members.push(Member {
                name,
                kind: MemberKind::Nested,
                is_static: true,
                is_extension: false,
            });
        }
        let mut members = Vec::new();
        // A primary constructor's parameters (and a record's positional properties).
        if let Some(pl) = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "parameter_list")
        {
            for p in named_children(pl) {
                if let Some(n) = p.child_by_field_name("name") {
                    members.push(Member {
                        name: self.text(n).to_string(),
                        kind: if kind == TypeKind::Record {
                            MemberKind::Property
                        } else {
                            MemberKind::PrimaryParam
                        },
                        is_static: false,
                        is_extension: false,
                    });
                }
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.members(body, idx, scope, &mut members);
        }
        self.out.types[idx].members.extend(members);
    }

    fn members(&mut self, body: Node, idx: usize, scope: usize, out: &mut Vec<Member>) {
        for m in named_children(body) {
            let is_static = modifiers(m, self.src)
                .iter()
                .any(|x| x == "static" || x == "const");
            match m.kind() {
                "field_declaration" | "event_field_declaration" => {
                    for vd in named_children(m) {
                        if vd.kind() != "variable_declaration" {
                            continue;
                        }
                        for d in named_children(vd) {
                            if d.kind() == "variable_declarator"
                                && let Some(n) = d.child_by_field_name("name")
                            {
                                out.push(Member {
                                    name: self.text(n).to_string(),
                                    kind: if m.kind() == "field_declaration" {
                                        MemberKind::Field
                                    } else {
                                        MemberKind::Event
                                    },
                                    is_static,
                                    is_extension: false,
                                });
                            }
                        }
                    }
                }
                "property_declaration" | "event_declaration" => {
                    if let Some(n) = m.child_by_field_name("name") {
                        out.push(Member {
                            name: self.text(n).to_string(),
                            kind: if m.kind() == "property_declaration" {
                                MemberKind::Property
                            } else {
                                MemberKind::Event
                            },
                            is_static,
                            is_extension: false,
                        });
                    }
                }
                "method_declaration" => {
                    if let Some(n) = m.child_by_field_name("name") {
                        let is_extension = m
                            .child_by_field_name("parameters")
                            .and_then(|pl| named_children(pl).into_iter().next())
                            .is_some_and(|p| modifiers(p, self.src).iter().any(|x| x == "this"));
                        out.push(Member {
                            name: self.text(n).to_string(),
                            kind: MemberKind::Method,
                            is_static,
                            is_extension,
                        });
                    }
                }
                "enum_member_declaration" => {
                    if let Some(n) = m.child_by_field_name("name") {
                        out.push(Member {
                            name: self.text(n).to_string(),
                            kind: MemberKind::EnumValue,
                            is_static: true,
                            is_extension: false,
                        });
                    }
                }
                "class_declaration"
                | "struct_declaration"
                | "record_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "delegate_declaration" => self.type_decl(m, scope, Some(idx)),
                "preproc_if" | "preproc_elif" | "preproc_else" => {
                    self.members(m, idx, scope, out);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_namespaces_types_and_members() {
        let src = r#"
global using Shop.Shared;
using L = Microsoft.Extensions.Logging.ILogger;
namespace Shop.Api;
using static System.Math;

public partial class OrdersApi<T>(ILogger<OrdersApi<T>> logger) : ControllerBase, IFoo<int>
{
    private readonly ILogger _log = logger;
    public string Email { get; set; }
    public static string Mask(this string s) => s;
    public class Inner { }
}
public record Order(string Email, int Id);
"#;
        let f = scan("a.cs", src);
        assert_eq!(
            f.global_usings,
            vec![Using::Namespace("Shop.Shared".into())]
        );
        assert_eq!(f.scopes.len(), 2);
        assert_eq!(f.scopes[1].name, "Shop.Api");
        assert_eq!(
            f.scopes[1].usings,
            vec![Using::Static("System.Math".into())]
        );
        assert!(
            matches!(&f.scopes[0].usings[0], Using::Alias(a, t) if a == "L" && t == "Microsoft.Extensions.Logging.ILogger")
        );
        let api = &f.types[0];
        assert_eq!(
            (api.name.as_str(), api.arity, api.partial),
            ("OrdersApi", 1, true)
        );
        assert_eq!(api.scope, 1);
        assert_eq!(api.bases[0].segs, vec!["ControllerBase"]);
        assert_eq!(api.bases[1].arity, 1);
        let names: Vec<&str> = api.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Inner", "logger", "_log", "Email", "Mask"]);
        assert!(
            api.members
                .iter()
                .any(|m| m.name == "Mask" && m.is_extension)
        );
        assert_eq!(f.types[1].name, "Inner");
        assert_eq!(f.types[1].outer, Some(0));
        assert_eq!(f.types[2].kind, TypeKind::Record);
        assert!(
            f.types[2]
                .members
                .iter()
                .any(|m| m.name == "Email" && m.kind == MemberKind::Property)
        );
    }
}
