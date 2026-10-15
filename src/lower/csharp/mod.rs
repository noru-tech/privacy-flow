//! C# lowering (tree-sitter-c-sharp).
//!
//! C# names are resolved here, against the project-wide [`index::CsIndex`] (ADR 0016): a bare
//! `_logger` is a field of the enclosing class or of a base class declared in another file, a
//! bare `Order` a type of the namespace or of a `using`, a bare `Console` a type outside the
//! project. Project types become imports of the file that declares them (by qualified name),
//! external types imports of their qualified name, so their API paths are dotted
//! (`Microsoft.Extensions.Logging.ILogger.LogInformation`) with generic arguments stripped.
//!
//! - **Members:** fields and auto-properties are fields of the class's instance; a property with
//!   a getter body is a property function. A primary constructor (and a record's positional
//!   parameters) stores each parameter into a field of the same name, and field initializers run
//!   inside it.
//! - **Attributes** are constructions of the attribute type (`[FromBody]` is
//!   `new Microsoft.AspNetCore.Mvc.FromBodyAttribute()`), kept as decorators of the method, the
//!   class, or the parameter.
//! - **Types** a value is declared with become type references (provenance only) and, reduced to
//!   a name, annotations; `Task<T>`, `T?` and `ActionResult<T>` are `T`, a collection is its
//!   element type for the annotation and nothing for provenance.
//! - `await`, casts, `!` and `.ConfigureAwait(...)` are the value itself; `out` and `ref`
//!   arguments receive the call's result.

pub mod index;
pub mod scan;

use std::collections::HashMap;

use tree_sitter::Node;

use self::index::{CsIndex, Res, TypeId, looks_like_interface};
use self::scan::{has_token, modifiers, name_text, predefined, type_text};
use super::razor::{Generated, RazorCtx, generate};
use super::{Builder, named_children};
use crate::ir::*;

/// Whether a path is a Razor component, lowered through its generated C#.
pub fn is_razor(path: &str) -> bool {
    path.ends_with(".razor")
}

/// Scan one C# file's declarations (a `.razor` file's generated C#).
pub fn scan_file(path: &str, src: &str, razor: &RazorCtx) -> scan::FileScan {
    if is_razor(path) {
        let g = generate(path, src, razor);
        return scan::scan(path, &g.text);
    }
    scan::scan(path, src)
}

/// Lower one C# file on its own (no other file's declarations are known).
pub fn lower_alone(path: &str, src: &str) -> FileIr {
    let razor = RazorCtx::default();
    let ix = CsIndex::build(
        vec![scan_file(path, src, &razor)],
        &crate::resolve::Resolver::default(),
        Default::default(),
        Default::default(),
    );
    lower(path, src, &ix)
}

pub fn lower(path: &str, src: &str, ix: &CsIndex) -> FileIr {
    if is_razor(path) {
        let ctx = ix.razor_ctx(path);
        let g = generate(path, src, &ctx);
        // Development aid: `PIIFLOW_DEBUG_RAZOR=<path part>` prints the generated C#.
        if let Some(want) = std::env::var_os("PIIFLOW_DEBUG_RAZOR")
            && path.contains(&*want.to_string_lossy())
        {
            eprintln!("// {path}\n{}", g.text);
        }
        return lower_text(path, &g.text, ix, Some((src, &g)));
    }
    lower_text(path, src, ix, None)
}

fn lower_text<'s>(
    path: &str,
    src: &'s str,
    ix: &CsIndex,
    origin: Option<(&'s str, &'s Generated)>,
) -> FileIr {
    let mut parser = scan::parser();
    let mut b = Builder::new(path, Lang::CSharp, src);
    if let Some((orig, g)) = origin {
        b.set_origin(orig, g);
    }
    let mut cs = Cs {
        b,
        ix,
        path: path.to_string(),
        scope: 0,
        frames: Vec::new(),
        imports: HashMap::new(),
        cond: Vec::new(),
        method_type_params: Vec::new(),
        rethrow: Vec::new(),
        ref_params: Vec::new(),
    };
    match parser.parse(src, None) {
        Some(tree) => {
            let root = tree.root_node();
            cs.b.record_parse_errors(root);
            cs.unit(root);
        }
        None => cs.b.ir.notes.push(LowerNote {
            kind: NoteKind::ParseError,
            pos: Pos { line: 1, column: 1 },
            detail: "parser returned no tree".into(),
        }),
    }
    cs.b.finish()
}

/// A type being lowered.
struct Frame {
    id: Option<TypeId>,
    class: ClassIdx,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum ImportKey {
    Namespace,
    Named(String),
}

struct Cs<'s, 'i> {
    b: Builder<'s>,
    ix: &'i CsIndex,
    path: String,
    /// The namespace scope (an index into the file's scan) being lowered.
    scope: usize,
    frames: Vec<Frame>,
    imports: HashMap<(FuncIdx, String, ImportKey), Var>,
    /// Receivers of the `?.` accesses being lowered, innermost last.
    cond: Vec<Var>,
    /// Type parameters of the generic methods being lowered.
    method_type_params: Vec<Vec<String>>,
    /// The catch variables of the `catch` clauses being lowered (for `throw;`).
    rethrow: Vec<Var>,
    /// The `out` and `ref` parameters of the functions being lowered.
    ref_params: Vec<Vec<Var>>,
}

/// Generic types whose value is their first type argument.
const WRAPPERS: &[&str] = &[
    "Task",
    "ValueTask",
    "Nullable",
    "ActionResult",
    "Ok",
    "Created",
    "CreatedAtRoute",
    "CreatedAtAction",
    "Accepted",
    "AcceptedAtRoute",
    "Results",
    "Lazy",
    "IOptions",
    "IOptionsSnapshot",
    "IOptionsMonitor",
];

/// Generic collections: a value of one holds its type argument's values.
const COLLECTIONS: &[&str] = &[
    "List",
    "IList",
    "IReadOnlyList",
    "IEnumerable",
    "IAsyncEnumerable",
    "ICollection",
    "IReadOnlyCollection",
    "Collection",
    "ObservableCollection",
    "HashSet",
    "ISet",
    "IReadOnlySet",
    "SortedSet",
    "IQueryable",
    "IOrderedEnumerable",
    "IOrderedQueryable",
    "Queue",
    "Stack",
    "LinkedList",
    "ImmutableList",
    "ImmutableArray",
    "IImmutableList",
    "ConcurrentBag",
    "ConcurrentQueue",
    "BlockingCollection",
    "Span",
    "ReadOnlySpan",
    "Memory",
    "ReadOnlyMemory",
    "PagedList",
    "PaginatedItems",
];

const TYPE_DECLS: &[&str] = &[
    "class_declaration",
    "struct_declaration",
    "record_declaration",
    "interface_declaration",
    "enum_declaration",
    "delegate_declaration",
];

/// The string value of a C# string literal node, without interpreting escapes.
pub fn string_value(node: Node, src: &str) -> Option<String> {
    let text = &src[node.byte_range()];
    match node.kind() {
        "string_literal" => {
            let mut out = String::new();
            for c in named_children(node) {
                if matches!(c.kind(), "string_literal_content" | "escape_sequence") {
                    out.push_str(&src[c.byte_range()]);
                }
            }
            Some(out)
        }
        "verbatim_string_literal" => {
            let t = text.trim_start_matches('@').trim_start_matches('"');
            let t = t.strip_suffix('"').unwrap_or(t);
            Some(t.replace("\"\"", "\""))
        }
        "raw_string_literal" => Some(
            named_children(node)
                .into_iter()
                .filter(|c| c.kind() == "raw_string_content")
                .map(|c| &src[c.byte_range()])
                .collect(),
        ),
        _ => None,
    }
}

impl<'s> Cs<'s, '_> {
    fn text(&self, n: Node) -> &'s str {
        self.b.text(n)
    }

    // ---------------------------------------------------------------- context

    fn class(&self) -> Option<ClassIdx> {
        self.frames.last().map(|f| f.class)
    }

    fn type_ids(&self) -> Vec<TypeId> {
        self.frames.iter().filter_map(|f| f.id).collect()
    }

    /// The instance as the code being lowered sees it: the constructor's own, or the class's
    /// shared one.
    fn this(&self) -> Option<Var> {
        let c = self.class()?;
        match self.b.ctor_this {
            Some((k, t)) if k == c => Some(t),
            _ => Some(self.b.ir.classes[c as usize].this),
        }
    }

    fn is_type_param(&self, name: &str) -> bool {
        self.method_type_params
            .iter()
            .any(|ps| ps.iter().any(|p| p == name))
            || self.ix.is_type_param(&self.type_ids(), name)
    }

    /// The innermost enclosing type that has a member `name`.
    fn member_frame(&self, name: &str) -> Option<usize> {
        self.frames.iter().rposition(|f| {
            f.id.is_some_and(|id| {
                self.ix
                    .member(id, name)
                    .is_some_and(|m| m.kind != scan::MemberKind::Nested)
            })
        })
    }

    /// Whether the innermost type may have members its source does not show.
    fn open_class(&self) -> bool {
        self.frames
            .last()
            .and_then(|f| f.id)
            .is_some_and(|id| self.ix.def(id).open)
    }

    // ---------------------------------------------------------------- imports

    fn import(&mut self, node: Node, module: String, key: ImportKey) -> Var {
        let k = (self.b.func, module.clone(), key.clone());
        if let Some(&v) = self.imports.get(&k) {
            return v;
        }
        let pos = self.b.pos(node);
        let var = self.b.temp(pos);
        let idx = self.b.ir.imports.len() as u32;
        self.b.ir.imports.push(ImportIr {
            var,
            module,
            kind: match key.clone() {
                ImportKey::Namespace => ImportKind::Namespace,
                ImportKey::Named(n) => ImportKind::Named(n),
            },
            pos,
        });
        self.b.emit(node, StmtKind::Import { import: idx });
        self.imports.insert(k, var);
        var
    }

    fn project_type(&mut self, node: Node, id: TypeId) -> Var {
        let def = self.ix.def(id);
        let (file, name) = (def.canonical_file().to_string(), export_name(def));
        self.import(node, file, ImportKey::Named(name))
    }

    fn resolved(&mut self, node: Node, res: Res, fallback: &str) -> Var {
        match res {
            Res::Project(id) => self.project_type(node, id),
            Res::Namespace(ns) => self.import(node, ns, ImportKey::Namespace),
            Res::External(cands) => {
                let vars: Vec<Var> = cands
                    .into_iter()
                    .map(|c| self.import(node, c, ImportKey::Namespace))
                    .collect();
                if vars.len() == 1 {
                    vars[0]
                } else {
                    self.b.union(node, &vars)
                }
            }
            Res::Unknown => self.b.global(fallback, node),
        }
    }

    // ---------------------------------------------------------------- declarations

    fn unit(&mut self, root: Node) {
        for c in named_children(root) {
            match c.kind() {
                "file_scoped_namespace_declaration" => {
                    if let Some(s) = self.ix.scope_at(&self.path, c.start_byte()) {
                        self.scope = s;
                    }
                }
                "global_statement" => {
                    for s in named_children(c) {
                        self.stmt(s);
                    }
                }
                _ => self.decl(c),
            }
        }
    }

    fn decl(&mut self, node: Node) {
        match node.kind() {
            "namespace_declaration" => {
                let outer = self.scope;
                if let Some(s) = self.ix.scope_at(&self.path, node.start_byte()) {
                    self.scope = s;
                }
                if let Some(body) = node.child_by_field_name("body") {
                    for c in named_children(body) {
                        self.decl(c);
                    }
                }
                self.scope = outer;
            }
            k if TYPE_DECLS.contains(&k) => self.type_decl(node),
            "preproc_if" | "preproc_elif" | "preproc_else" => {
                for c in named_children(node) {
                    self.decl(c);
                }
            }
            "global_statement" => {
                for s in named_children(node) {
                    self.stmt(s);
                }
            }
            _ => {}
        }
    }

    /// Lower attribute lists into constructions of the attribute types, in the current function.
    fn attributes(&mut self, node: Node) -> Vec<Var> {
        let mut out = Vec::new();
        for list in named_children(node) {
            if list.kind() != "attribute_list" {
                continue;
            }
            // `[return: X]`, `[assembly: X]` are not the declaration's.
            if named_children(list)
                .iter()
                .any(|c| c.kind() == "attribute_target_specifier")
            {
                continue;
            }
            for a in named_children(list) {
                if a.kind() != "attribute" {
                    continue;
                }
                let Some(name) = a.child_by_field_name("name") else {
                    continue;
                };
                let callee = self.attribute_type(name);
                let mut args = Vec::new();
                if let Some(al) = named_children(a)
                    .into_iter()
                    .find(|c| c.kind() == "attribute_argument_list")
                {
                    for arg in named_children(al) {
                        let named = arg.child_by_field_name("name").map(|n| self.text(n));
                        let value = named_children(arg)
                            .into_iter()
                            .rfind(|c| Some(*c) != arg.child_by_field_name("name"));
                        if let Some(v) = value {
                            let var = self.expr(v);
                            args.push(Arg {
                                var,
                                kind: match named {
                                    Some(n) => ArgKind::Keyword(n.to_string()),
                                    None => ArgKind::Positional,
                                },
                            });
                        }
                    }
                }
                let dst = self.b.temp(self.b.pos(a));
                self.b.emit(
                    a,
                    StmtKind::Call {
                        dst,
                        callee: Callee::Value(callee),
                        args,
                        is_new: true,
                    },
                );
                out.push(dst);
            }
        }
        out
    }

    /// `[FromBody]` names `FromBodyAttribute` when there is one.
    fn attribute_type(&mut self, name: Node) -> Var {
        let Some(t) = type_text(name, self.b.src) else {
            return self.b.lit(name, None);
        };
        let mut long = t.clone();
        if let Some(last) = long.segs.last_mut()
            && !last.ends_with("Attribute")
        {
            last.push_str("Attribute");
        }
        let enclosing = self.type_ids();
        let res = self
            .ix
            .resolve_type(&self.path, self.scope, &enclosing, &long);
        let res = match res {
            Res::Unknown => self.ix.resolve_type(&self.path, self.scope, &enclosing, &t),
            r => r,
        };
        let fallback = long.segs.join(".");
        self.resolved(name, res, &fallback)
    }

    fn type_decl(&mut self, node: Node) {
        let id = self.ix.type_at(&self.path, node.start_byte());
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        let attrs = self.attributes(node);
        let outer = self.class();
        let c = self.b.new_class(&name, node);
        self.b.ir.classes[c as usize].outer = outer;
        let pos = self.b.pos(node);
        if let Some(id) = id {
            let def = self.ix.def(id);
            let class = &mut self.b.ir.classes[c as usize];
            class.qualified_name = Some(def.fqn.clone());
            class.type_params = def.type_params.clone();
            class.is_interface = def.is_interface();
            if def.partial {
                class.partial_key = Some(def.key());
            }
            let export = export_name(def);
            let var = self.b.declare_module(&format!("<type {export}>"), pos);
            self.b.emit(node, StmtKind::ClassRef { dst: var, class: c });
            self.b.ir.exports.push(ExportIr { name: export, var });
        }
        self.b.ir.classes[c as usize].decorators = attrs;
        // The type's own type parameters are in scope in its base list.
        self.frames.push(Frame { id, class: c });
        // Base types: classes first, then interfaces, each with its type arguments.
        if let Some(bl) = named_children(node)
            .into_iter()
            .find(|k| k.kind() == "base_list")
        {
            let mut enclosing: Vec<TypeId> = self.type_ids();
            // A base list does not see the type's own members or nested types.
            enclosing.pop();
            for bnode in named_children(bl) {
                let tnode = match bnode.kind() {
                    "primary_constructor_base_type" => bnode.child_by_field_name("type"),
                    _ => Some(bnode),
                };
                let Some(tnode) = tnode else { continue };
                let Some(tt) = type_text(tnode, self.b.src) else {
                    continue;
                };
                let res = self
                    .ix
                    .resolve_type(&self.path, self.scope, &enclosing, &tt);
                let is_interface = match &res {
                    Res::Project(p) => self.ix.def(*p).is_interface(),
                    Res::External(cands) => cands.iter().all(|c| looks_like_interface(c)),
                    _ => looks_like_interface(&tt.segs.join(".")),
                };
                let var = self.resolved(tnode, res, &tt.segs.join("."));
                let args = self.type_args(tnode);
                let class = &mut self.b.ir.classes[c as usize];
                if is_interface {
                    class.implements.push(var);
                    class.implements_args.push(args);
                } else {
                    class.bases.push(var);
                    class.base_args.push(args);
                }
            }
        }
        let type_index = self.b.ir.types.len();
        self.b.ir.types.push(TypeDecl {
            name: name.clone(),
            fields: Vec::new(),
            pos,
        });
        let body = node.child_by_field_name("body");
        let primary = named_children(node)
            .into_iter()
            .find(|k| k.kind() == "parameter_list");
        let is_record = node.kind() == "record_declaration";
        if let Some(pl) = primary {
            self.primary_ctor(node, pl, body, is_record, type_index);
        }
        if let Some(body) = body {
            self.members(body, c, type_index, primary.is_some());
        }
        self.b.finish_class(c);
        self.frames.pop();
        if self.b.ir.types[type_index].fields.is_empty() {
            self.b.ir.types.remove(type_index);
        }
    }

    /// The type arguments of a generic type, as type values (a type parameter has none).
    fn type_args(&mut self, tnode: Node) -> Vec<Var> {
        let g = match tnode.kind() {
            "generic_name" => Some(tnode),
            "qualified_name" => tnode
                .child_by_field_name("name")
                .filter(|n| n.kind() == "generic_name"),
            _ => None,
        };
        let Some(g) = g else { return Vec::new() };
        let Some(list) = named_children(g)
            .into_iter()
            .find(|c| c.kind() == "type_argument_list")
        else {
            return Vec::new();
        };
        named_children(list)
            .into_iter()
            .map(|a| match self.type_value(a, true) {
                Some(v) => v,
                None => self.b.lit(a, None),
            })
            .collect()
    }

    /// A primary constructor (or a record's positional parameters): each parameter is stored
    /// into a field of the same name, and the class's field and property initializers run here.
    fn primary_ctor(
        &mut self,
        node: Node,
        params: Node,
        body: Option<Node>,
        is_record: bool,
        type_index: usize,
    ) {
        let Some(c) = self.class() else { return };
        let name = self.b.ir.classes[c as usize].name.clone();
        let f = self
            .b
            .begin_function(format!("{name}.{name}"), node, Some(c));
        self.b.ir.funcs[f as usize].is_ctor = true;
        self.b.ir.funcs[f as usize].modifiers = modifiers(node, self.b.src);
        let outer_ctor = self.b.ctor_this.take();
        let pos = self.b.pos(node);
        let this = self.begin_ctor(c, pos);
        self.ref_params.push(Vec::new());
        let vars = self.params(params);
        for (pnode, var, pname) in vars {
            self.b.store_member(pnode, this, Some(pname.clone()), var);
            if is_record {
                let p = self.b.pos(pnode);
                self.b.ir.types[type_index].fields.push((pname.clone(), p));
                let ty = pnode.child_by_field_name("type").map(|t| self.type_expr(t));
                self.b.ir.classes[c as usize].members.push(MemberIr {
                    name: pname,
                    pos: p,
                    kind: MemberKind::Property,
                    is_static: false,
                    ty,
                    value: None,
                });
            }
        }
        // `record R(string A) : Base(A)`: the base constructor runs with these arguments.
        if let Some(bl) = named_children(node)
            .into_iter()
            .find(|k| k.kind() == "base_list")
            && let Some(pb) = named_children(bl)
                .into_iter()
                .find(|k| k.kind() == "primary_constructor_base_type")
            && let Some(al) = named_children(pb)
                .into_iter()
                .find(|k| k.kind() == "argument_list")
        {
            let sup = self.b.temp(self.b.pos(pb));
            self.b.emit(pb, StmtKind::Super { dst: sup, class: c });
            self.invoke(pb, Callee::Value(sup), Some(al), true);
        }
        if let Some(body) = body {
            self.initializers(body, this, type_index);
        }
        self.finish_function_refs();
        self.b.end_ctor(pos);
        self.b.ctor_this = outer_ctor;
        self.b.end_function(f);
        self.b.ir.classes[c as usize]
            .methods
            .push((".ctor".to_string(), f));
    }

    fn begin_ctor(&mut self, c: ClassIdx, pos: Pos) -> Var {
        let previous = self.b.ir.classes[c as usize].ctor_this;
        let t = self.b.begin_ctor(c, pos);
        if let Some(p) = previous {
            // An overload: the first constructor stays `ctor_this`.
            let class = &mut self.b.ir.classes[c as usize];
            class.ctor_this = Some(p);
            class.more_ctor_this.push(t);
        }
        t
    }

    /// Field and property initializers of a class body, with `this` the instance being built.
    fn initializers(&mut self, body: Node, this: Var, type_index: usize) {
        for m in named_children(body) {
            match m.kind() {
                "field_declaration" | "event_field_declaration" => {
                    self.field(m, this, type_index);
                }
                "property_declaration" => {
                    if let Some(value) = m.child_by_field_name("value")
                        && value.kind() != "arrow_expression_clause"
                        && let Some(n) = m.child_by_field_name("name")
                    {
                        let key = self.text(n).to_string();
                        let v = self.expr(value);
                        self.b.store_member(m, this, Some(key), v);
                    }
                }
                "preproc_if" | "preproc_elif" | "preproc_else" => {
                    self.initializers(m, this, type_index);
                }
                _ => {}
            }
        }
    }

    /// A field declaration: its declared type and initializer go into the field.
    fn field(&mut self, m: Node, this: Var, type_index: usize) {
        let Some(vd) = named_children(m)
            .into_iter()
            .find(|c| c.kind() == "variable_declaration")
        else {
            return;
        };
        let ty = vd.child_by_field_name("type");
        for d in named_children(vd) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            let Some(n) = d.child_by_field_name("name") else {
                continue;
            };
            let key = self.text(n).to_string();
            let p = self.b.pos(d);
            if !self.b.ir.types[type_index]
                .fields
                .iter()
                .any(|(f, _)| *f == key)
            {
                self.b.ir.types[type_index].fields.push((key.clone(), p));
            }
            if let Some(t) = ty {
                self.typed_member(d, this, &key, t);
            }
            if let Some(value) = declarator_value(d) {
                let v = self.expr_typed(value, ty);
                self.b.store_member(d, this, Some(key), v);
            }
        }
    }

    /// `ILogger _logger;` says what `this._logger` is.
    fn typed_member(&mut self, node: Node, this: Var, key: &str, ty: Node) {
        let v = self.b.temp(self.b.pos(node));
        let typed = self.type_ref(v, ty);
        if typed {
            self.b.store_member(node, this, Some(key.to_string()), v);
        }
        if let Some(a) = self.annotation(ty)
            && let Some(c) = self.class()
        {
            let fv = self.b.field_var(c, key);
            self.b.ir.annots.push((fv, a));
        }
    }

    /// Attributes of a field or property, kept by member name.
    fn member_attributes(&mut self, m: Node, c: ClassIdx, key: &str) {
        let attrs = self.attributes(m);
        if !attrs.is_empty() {
            self.b.ir.classes[c as usize]
                .field_attrs
                .push((key.to_string(), attrs));
            // The declaration writes the field (with no value), so it can be cited.
            let this = self.b.ir.classes[c as usize].this;
            let v = self.b.lit(m, None);
            self.b.store_member(m, this, Some(key.to_string()), v);
        }
    }

    fn members(&mut self, body: Node, c: ClassIdx, type_index: usize, has_primary: bool) {
        let this = self.b.ir.classes[c as usize].this;
        for m in named_children(body) {
            if m.kind() == "field_declaration"
                && let Some(vd) = named_children(m)
                    .into_iter()
                    .find(|k| k.kind() == "variable_declaration")
            {
                let is_static = modifiers(m, self.b.src)
                    .iter()
                    .any(|x| x == "static" || x == "const");
                let ty = vd.child_by_field_name("type");
                for d in named_children(vd) {
                    if d.kind() == "variable_declarator"
                        && let Some(n) = d.child_by_field_name("name")
                    {
                        let key = self.text(n).to_string();
                        self.member_attributes(m, c, &key);
                        let ty = ty.map(|t| self.type_expr(t));
                        let pos = self.b.pos(d);
                        self.b.ir.classes[c as usize].members.push(MemberIr {
                            name: key,
                            pos,
                            kind: MemberKind::Field,
                            is_static,
                            ty,
                            value: None,
                        });
                    }
                }
            }
            match m.kind() {
                "field_declaration" | "event_field_declaration" => {
                    if !has_primary {
                        self.field(m, this, type_index);
                    } else {
                        // Initializers ran in the primary constructor; the declared types
                        // still say what the fields are.
                        self.field_types(m, this, type_index);
                    }
                }
                "property_declaration" => self.property(m, c, this, type_index, has_primary),
                "method_declaration" => self.method(m, c),
                "constructor_declaration" => self.ctor(m, c),
                "destructor_declaration"
                | "operator_declaration"
                | "conversion_operator_declaration" => {
                    let f = self.function(m, "op".to_string(), Some(c));
                    let _ = f;
                }
                "indexer_declaration" | "event_declaration" => self.accessors(m, c, "this[]"),
                k if TYPE_DECLS.contains(&k) => self.type_decl(m),
                "preproc_if" | "preproc_elif" | "preproc_else" => {
                    self.members(m, c, type_index, has_primary)
                }
                "enum_member_declaration_list" => {}
                _ => {}
            }
        }
    }

    fn field_types(&mut self, m: Node, this: Var, type_index: usize) {
        let Some(vd) = named_children(m)
            .into_iter()
            .find(|c| c.kind() == "variable_declaration")
        else {
            return;
        };
        let ty = vd.child_by_field_name("type");
        for d in named_children(vd) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            let Some(n) = d.child_by_field_name("name") else {
                continue;
            };
            let key = self.text(n).to_string();
            let p = self.b.pos(d);
            if !self.b.ir.types[type_index]
                .fields
                .iter()
                .any(|(f, _)| *f == key)
            {
                self.b.ir.types[type_index].fields.push((key.clone(), p));
            }
            if let Some(t) = ty {
                self.typed_member(d, this, &key, t);
            }
        }
    }

    fn property(&mut self, m: Node, c: ClassIdx, this: Var, type_index: usize, has_primary: bool) {
        let Some(n) = m.child_by_field_name("name") else {
            return;
        };
        let key = self.text(n).to_string();
        self.member_attributes(m, c, &key);
        let p = self.b.pos(m);
        let computed = m
            .child_by_field_name("value")
            .is_some_and(|v| v.kind() == "arrow_expression_clause");
        let ty = m.child_by_field_name("type").map(|t| self.type_expr(t));
        self.b.ir.classes[c as usize].members.push(MemberIr {
            name: key.clone(),
            pos: p,
            kind: if computed {
                MemberKind::Computed
            } else {
                MemberKind::Property
            },
            is_static: modifiers(m, self.b.src).iter().any(|x| x == "static"),
            ty,
            value: None,
        });
        if !self.b.ir.types[type_index]
            .fields
            .iter()
            .any(|(f, _)| *f == key)
        {
            self.b.ir.types[type_index].fields.push((key.clone(), p));
        }
        let ty = m.child_by_field_name("type");
        if let Some(t) = ty {
            self.typed_member(m, this, &key, t);
        }
        match m.child_by_field_name("value") {
            // `string Full => $"{First} {Last}";`
            Some(v) if v.kind() == "arrow_expression_clause" => {
                let f = self.b.begin_function(
                    format!("{}.{}", self.b.ir.classes[c as usize].name, key),
                    m,
                    Some(c),
                );
                self.b.ir.funcs[f as usize].is_property = true;
                if let Some(e) = named_children(v).into_iter().next() {
                    let r = self.expr(e);
                    self.b.emit(e, StmtKind::Return { src: r });
                }
                self.ret_type(f, ty);
                self.finish_function_refs();
                self.b.end_function(f);
                self.b.ir.classes[c as usize].methods.push((key.clone(), f));
            }
            Some(v) if !has_primary => {
                let val = self.expr_typed(v, ty);
                self.b.store_member(m, this, Some(key.clone()), val);
            }
            _ => {}
        }
        if let Some(acc) = m.child_by_field_name("accessors") {
            for a in named_children(acc) {
                if a.kind() != "accessor_declaration" {
                    continue;
                }
                let body = a.child_by_field_name("body");
                let Some(body) = body else { continue };
                let is_get = has_token(a, self.b.src, "get");
                let display = format!(
                    "{}.{}{}",
                    self.b.ir.classes[c as usize].name,
                    if is_get { "" } else { "set_" },
                    key
                );
                let f = self.b.begin_function(display, a, Some(c));
                if is_get {
                    self.b.ir.funcs[f as usize].is_property = true;
                } else {
                    // `value` is what is assigned: the property's own field.
                    let v = self.b.this_load(a, c, &key);
                    let pos = self.b.pos(a);
                    let value = self.b.declare("value", pos, false);
                    self.b.copy(a, value, v);
                }
                self.body(body);
                if is_get {
                    self.ret_type(f, ty);
                }
                self.finish_function_refs();
                self.b.end_function(f);
                let mname = if is_get {
                    key.clone()
                } else {
                    format!("set_{key}")
                };
                self.b.ir.classes[c as usize].methods.push((mname, f));
            }
        }
    }

    /// Indexer and event accessors: their bodies are lowered, as methods no call reaches by name.
    fn accessors(&mut self, m: Node, c: ClassIdx, name: &str) {
        let Some(acc) = m.child_by_field_name("accessors") else {
            return;
        };
        for a in named_children(acc) {
            if let Some(body) = a.child_by_field_name("body") {
                let f = self.b.begin_function(
                    format!("{}.{name}", self.b.ir.classes[c as usize].name),
                    a,
                    Some(c),
                );
                if let Some(pl) = m.child_by_field_name("parameters") {
                    self.params(pl);
                }
                let pos = self.b.pos(a);
                self.b.declare("value", pos, false);
                self.body(body);
                self.finish_function_refs();
                self.b.end_function(f);
            }
        }
    }

    fn method(&mut self, m: Node, c: ClassIdx) {
        let Some(n) = m.child_by_field_name("name") else {
            return;
        };
        let name = self.text(n).to_string();
        let mods = modifiers(m, self.b.src);
        let has_body = m.child_by_field_name("body").is_some();
        let in_interface = self.b.ir.classes[c as usize].is_interface;
        // Abstract and interface methods: calls reach the implementations.
        if !has_body
            && (in_interface || mods.iter().any(|x| x == "abstract"))
            && !mods.iter().any(|x| x == "partial")
        {
            return;
        }
        let f = self.function(m, name.clone(), Some(c));
        self.b.ir.funcs[f as usize].is_stub = !has_body;
        self.b.ir.classes[c as usize].methods.push((name, f));
    }

    fn ctor(&mut self, m: Node, c: ClassIdx) {
        let mods = modifiers(m, self.b.src);
        if mods.iter().any(|x| x == "static") {
            let f = self.function(m, ".cctor".to_string(), Some(c));
            let _ = f;
            return;
        }
        let f = self.function(m, ".ctor".to_string(), Some(c));
        self.b.ir.classes[c as usize]
            .methods
            .push((".ctor".to_string(), f));
    }

    /// A method, constructor or local function: attributes in the enclosing function, then the
    /// function with its parameters and body.
    fn function(&mut self, node: Node, name: String, class: Option<ClassIdx>) -> FuncIdx {
        let attrs = self.attributes(node);
        let param_list = node.child_by_field_name("parameters").or_else(|| {
            named_children(node)
                .into_iter()
                .find(|c| c.kind() == "parameter_list")
        });
        let param_attrs: Vec<Vec<Var>> = param_list
            .map(|pl| {
                named_children(pl)
                    .into_iter()
                    .filter(|p| p.kind() == "parameter")
                    .map(|p| self.attributes(p))
                    .collect()
            })
            .unwrap_or_default();
        let is_ctor = node.kind() == "constructor_declaration" && name == ".ctor";
        let display = match class {
            Some(c) => {
                let cname = &self.b.ir.classes[c as usize].name;
                if is_ctor {
                    format!("{cname}.{cname}")
                } else {
                    format!("{cname}.{name}")
                }
            }
            None => name,
        };
        let f = self.b.begin_function(display, node, class);
        let mods = modifiers(node, self.b.src);
        self.b.ir.funcs[f as usize].decorators = attrs;
        self.b.ir.funcs[f as usize].is_ctor = is_ctor;
        self.b.ir.funcs[f as usize].modifiers = mods;
        let outer_ctor = self.b.ctor_this.take();
        let pos = self.b.pos(node);
        if let (true, Some(c)) = (is_ctor, class) {
            self.begin_ctor(c, pos);
        }
        let tps: Vec<String> = node
            .child_by_field_name("type_parameters")
            .map(|l| {
                named_children(l)
                    .into_iter()
                    .filter_map(|p| p.child_by_field_name("name"))
                    .map(|n| self.text(n).to_string())
                    .collect()
            })
            .unwrap_or_default();
        self.method_type_params.push(tps);
        self.ref_params.push(Vec::new());
        if let Some(pl) = param_list {
            self.params(pl);
            let params = &mut self.b.ir.funcs[f as usize].params;
            for (q, a) in params.iter_mut().zip(param_attrs) {
                q.attrs = a;
            }
            if named_children(pl)
                .first()
                .is_some_and(|p| modifiers(*p, self.b.src).iter().any(|x| x == "this"))
            {
                self.b.ir.funcs[f as usize].is_extension = true;
            }
        }
        // `: base(...)` / `: this(...)`.
        if let Some(init) = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "constructor_initializer")
            && let Some(c) = class
        {
            let args = named_children(init)
                .into_iter()
                .find(|k| k.kind() == "argument_list");
            if has_token(init, self.b.src, "base") {
                let sup = self.b.temp(self.b.pos(init));
                self.b.emit(init, StmtKind::Super { dst: sup, class: c });
                self.invoke(init, Callee::Value(sup), args, true);
            } else {
                let own = self.b.temp(self.b.pos(init));
                self.b.emit(init, StmtKind::ClassRef { dst: own, class: c });
                self.invoke(init, Callee::Value(own), args, true);
            }
        }
        self.ret_type(
            f,
            node.child_by_field_name("returns")
                .or_else(|| node.child_by_field_name("type")),
        );
        if let Some(body) = node.child_by_field_name("body") {
            self.body(body);
        }
        self.finish_function_refs();
        self.method_type_params.pop();
        if is_ctor {
            self.b.end_ctor(pos);
        }
        self.b.ctor_this = outer_ctor;
        self.b.end_function(f);
        f
    }

    /// A body: a block, or `=> expression`.
    fn body(&mut self, body: Node) {
        match body.kind() {
            "arrow_expression_clause" => {
                if let Some(e) = named_children(body).into_iter().next() {
                    let v = self.expr(e);
                    self.b.emit(e, StmtKind::Return { src: v });
                }
            }
            "block" => self.block_items(body),
            _ => {
                let v = self.expr(body);
                self.b.emit(body, StmtKind::Return { src: v });
            }
        }
    }

    /// At the end of a function: `out` and `ref` parameters leave through the return value.
    fn finish_function_refs(&mut self) {
        if let Some(refs) = self.ref_params.pop() {
            let pos = self.b.ir.funcs[self.b.func as usize].pos;
            for r in refs {
                self.b
                    .emit_at(pos, "out".into(), StmtKind::Return { src: r });
            }
        }
    }

    fn ret_type(&mut self, f: FuncIdx, ty: Option<Node>) {
        let Some(t) = ty else { return };
        self.b.ir.funcs[f as usize].ret_annot = self.annotation(t);
        let ret = self.b.ir.funcs[f as usize].ret;
        self.type_ref(ret, t);
    }

    /// Declare a parameter list's parameters in the current function.
    fn params<'t>(&mut self, pl: Node<'t>) -> Vec<(Node<'t>, Var, String)> {
        let mut out = Vec::new();
        for p in named_children(pl) {
            let (name_node, ty, default) = match p.kind() {
                "parameter" => (
                    p.child_by_field_name("name"),
                    p.child_by_field_name("type"),
                    named_children(p).into_iter().find(|c| {
                        Some(*c) != p.child_by_field_name("name")
                            && Some(*c) != p.child_by_field_name("type")
                            && !matches!(c.kind(), "attribute_list" | "modifier")
                    }),
                ),
                "implicit_parameter" | "identifier" => (Some(p), None, None),
                _ => continue,
            };
            let Some(name_node) = name_node else { continue };
            let name = self.text(name_node).to_string();
            let mods = modifiers(p, self.b.src);
            let pos = self.b.pos(name_node);
            let var = self.b.declare_param(&name, pos);
            let annot = ty.and_then(|t| self.annotation(t));
            let f = self.b.func as usize;
            self.b.ir.funcs[f].params.push(Param {
                var,
                attrs: Vec::new(),
                name: name.clone(),
                rest: mods.iter().any(|m| m == "params"),
                kwrest: false,
                annot: annot.clone(),
            });
            if let Some(a) = annot {
                self.b.ir.annots.push((var, a));
            }
            if let Some(t) = ty {
                self.type_ref(var, t);
            }
            if mods.iter().any(|m| m == "out" || m == "ref")
                && let Some(r) = self.ref_params.last_mut()
            {
                r.push(var);
            }
            if let Some(d) = default
                && !matches!(d.kind(), "default_expression")
            {
                let dv = self.expr(d);
                self.b.copy(d, var, dv);
            }
            out.push((p, var, name));
        }
        out
    }

    // ---------------------------------------------------------------- types

    /// Record that `var` is declared with the type `ty` names. True when it names one.
    fn type_ref(&mut self, var: Var, ty: Node) -> bool {
        match self.type_value(ty, false) {
            Some(t) => {
                self.b.emit(ty, StmtKind::TypeRef { dst: var, ty: t });
                true
            }
            None => false,
        }
    }

    /// The value a type names, for provenance: a project class, or an external type's path.
    /// Wrappers are their argument; collections, primitives and type parameters name nothing.
    /// With `exact`, a generic type is itself (a base list's `IRequestHandler<T, R>`).
    fn type_value(&mut self, ty: Node, exact: bool) -> Option<Var> {
        match ty.kind() {
            "nullable_type" | "ref_type" | "scoped_type" => {
                self.type_value(ty.child_by_field_name("type")?, exact)
            }
            "array_type"
            | "tuple_type"
            | "pointer_type"
            | "function_pointer_type"
            | "implicit_type" => None,
            "predefined_type" => None,
            "identifier" | "qualified_name" | "generic_name" | "alias_qualified_name" => {
                let tt = type_text(ty, self.b.src)?;
                let last = tt.segs.last()?.clone();
                if !exact && tt.arity > 0 {
                    if WRAPPERS.contains(&last.as_str()) {
                        let g = if ty.kind() == "qualified_name" {
                            ty.child_by_field_name("name")?
                        } else {
                            ty
                        };
                        let list = named_children(g)
                            .into_iter()
                            .find(|c| c.kind() == "type_argument_list")?;
                        let first = named_children(list).into_iter().next()?;
                        return self.type_value(first, false);
                    }
                    if COLLECTIONS.contains(&last.as_str()) {
                        return None;
                    }
                }
                if tt.segs.len() == 1 && self.is_type_param(&last) {
                    return None;
                }
                let enclosing = self.type_ids();
                let res = self
                    .ix
                    .resolve_type(&self.path, self.scope, &enclosing, &tt);
                let fallback = tt.segs.join(".");
                Some(self.resolved(ty, res, &fallback))
            }
            _ => None,
        }
    }

    /// A declared type for the members side table: its head exactly (`DbSet` of `DbSet<T>`) and
    /// its type arguments.
    fn type_expr(&mut self, t: Node) -> TypeExpr {
        let inner = match t.kind() {
            "nullable_type" | "ref_type" | "scoped_type" => {
                t.child_by_field_name("type").unwrap_or(t)
            }
            _ => t,
        };
        TypeExpr {
            text: self.text(t).to_string(),
            head: self.type_value(inner, true),
            args: self.type_args(inner),
        }
    }

    /// A type reduced to the name of the type it carries, for typed seeds.
    fn annotation(&self, ty: Node) -> Option<String> {
        match ty.kind() {
            "nullable_type" | "ref_type" | "scoped_type" | "array_type" | "pointer_type" => {
                self.annotation(ty.child_by_field_name("type")?)
            }
            // `int` is `Int32`: no type declares fields under these names, but they say what a
            // parameter can hold.
            "predefined_type" => {
                let full = predefined(self.text(ty));
                Some(full.rsplit('.').next().unwrap_or(full).to_string())
            }
            "identifier" | "qualified_name" | "alias_qualified_name" => {
                let t = type_text(ty, self.b.src)?;
                let last = t.segs.last()?;
                (!self.is_type_param(last)).then(|| last.clone())
            }
            "generic_name" => {
                let t = type_text(ty, self.b.src)?;
                let last = t.segs.last()?.clone();
                if WRAPPERS.contains(&last.as_str()) || COLLECTIONS.contains(&last.as_str()) {
                    let list = named_children(ty)
                        .into_iter()
                        .find(|c| c.kind() == "type_argument_list")?;
                    let first = named_children(list).into_iter().next()?;
                    return self.annotation(first);
                }
                Some(last)
            }
            _ => None,
        }
    }

    // ---------------------------------------------------------------- statements

    fn block_items(&mut self, block: Node) {
        self.b.push_block();
        // Local functions are visible throughout their block.
        for c in named_children(block) {
            if c.kind() == "local_function_statement"
                && let Some(n) = c.child_by_field_name("name")
            {
                let pos = self.b.pos(n);
                self.b.declare(self.text(n), pos, false);
            }
        }
        for c in named_children(block) {
            self.stmt(c);
        }
        self.b.pop_scope();
    }

    fn stmt(&mut self, node: Node) {
        match node.kind() {
            "block" => self.block_items(node),
            "expression_statement" => {
                for c in named_children(node) {
                    self.expr(c);
                }
            }
            "local_declaration_statement" => {
                for c in named_children(node) {
                    if c.kind() == "variable_declaration" {
                        self.var_decl(c);
                    }
                }
            }
            "return_statement" => {
                if let Some(e) = node.named_child(0) {
                    let v = self.expr(e);
                    self.b.emit(node, StmtKind::Return { src: v });
                }
            }
            "yield_statement" => {
                if let Some(e) = node.named_child(0) {
                    let v = self.expr(e);
                    self.b.emit(node, StmtKind::Return { src: v });
                }
            }
            "throw_statement" => match node.named_child(0) {
                Some(e) => {
                    let v = self.expr(e);
                    self.b.throw(node, v);
                }
                None => {
                    if let Some(&v) = self.rethrow.last() {
                        self.b.throw(node, v);
                    }
                }
            },
            "if_statement" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                if let Some(c) = node.child_by_field_name("consequence") {
                    self.scoped_stmt(c);
                }
                if let Some(a) = node.child_by_field_name("alternative") {
                    self.scoped_stmt(a);
                }
            }
            "while_statement" | "do_statement" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                if let Some(b) = node.child_by_field_name("body") {
                    self.scoped_stmt(b);
                }
            }
            "for_statement" => {
                self.b.push_block();
                for c in named_children(node) {
                    match c.kind() {
                        "variable_declaration" => self.var_decl(c),
                        k if k.ends_with("_statement") || k == "block" => self.stmt(c),
                        _ => {
                            self.expr(c);
                        }
                    }
                }
                self.b.pop_scope();
            }
            "foreach_statement" => self.foreach(node),
            "try_statement" => self.try_stmt(node),
            "switch_statement" => self.switch_stmt(node),
            "using_statement" | "lock_statement" | "fixed_statement" | "checked_statement"
            | "unsafe_statement" | "labeled_statement" => {
                self.b.push_block();
                for c in named_children(node) {
                    match c.kind() {
                        "variable_declaration" => self.var_decl(c),
                        "identifier" if node.kind() == "labeled_statement" => {}
                        k if k.ends_with("_statement") || k == "block" => self.stmt(c),
                        _ => {
                            self.expr(c);
                        }
                    }
                }
                self.b.pop_scope();
            }
            "local_function_statement" => {
                let Some(n) = node.child_by_field_name("name") else {
                    return;
                };
                let name = self.text(n).to_string();
                let f = self.function(node, name.clone(), None);
                let var = match self.b.lookup(&name) {
                    Some(v) => v,
                    None => {
                        let pos = self.b.pos(n);
                        self.b.declare(&name, pos, false)
                    }
                };
                self.b.emit(node, StmtKind::FuncRef { dst: var, func: f });
            }
            "preproc_if" | "preproc_elif" | "preproc_else" => {
                // Both branches of `#if` are code that may run.
                let condition = node.child_by_field_name("condition");
                for c in named_children(node) {
                    if Some(c) == condition {
                        continue;
                    }
                    if TYPE_DECLS.contains(&c.kind()) {
                        self.type_decl(c);
                    } else {
                        self.stmt(c);
                    }
                }
            }
            "global_statement" => {
                for c in named_children(node) {
                    self.stmt(c);
                }
            }
            "break_statement" | "continue_statement" | "goto_statement" | "empty_statement"
            | "comment" | "preproc_region" | "preproc_endregion" | "preproc_pragma"
            | "preproc_nullable" | "preproc_define" | "preproc_undef" | "preproc_line"
            | "preproc_warning" | "preproc_error" => {}
            k if TYPE_DECLS.contains(&k) => self.type_decl(node),
            _ => {
                self.expr(node);
            }
        }
    }

    fn scoped_stmt(&mut self, node: Node) {
        self.b.push_block();
        self.stmt(node);
        self.b.pop_scope();
    }

    fn var_decl(&mut self, vd: Node) {
        let ty = vd
            .child_by_field_name("type")
            .filter(|t| t.kind() != "implicit_type");
        for d in named_children(vd) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            let value = declarator_value(d).map(|v| self.expr_typed(v, ty));
            if let Some(n) = d.child_by_field_name("name") {
                let name = self.text(n).to_string();
                let pos = self.b.pos(n);
                let var = self.b.declare(&name, pos, false);
                if let Some(v) = value {
                    self.b.copy(d, var, v);
                }
                if let Some(t) = ty {
                    self.type_ref(var, t);
                    if let Some(a) = self.annotation(t) {
                        self.b.ir.annots.push((var, a));
                    }
                }
            } else if let Some(tp) = named_children(d)
                .into_iter()
                .find(|c| c.kind() == "tuple_pattern")
            {
                // `var (a, b) = t;`: each name holds what the tuple holds.
                let names = self.pattern_names(tp);
                for (name, n) in names {
                    let pos = self.b.pos(n);
                    let var = self.b.declare(&name, pos, false);
                    if let Some(v) = value {
                        self.b.copy(n, var, v);
                    }
                }
            }
        }
    }

    fn pattern_names<'t>(&self, node: Node<'t>) -> Vec<(String, Node<'t>)> {
        let mut out = Vec::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            match n.kind() {
                "identifier" => out.push((self.text(n).to_string(), n)),
                "tuple_pattern" | "parenthesized_variable_designation" => {
                    for c in named_children(n).into_iter().rev() {
                        stack.push(c);
                    }
                }
                _ => {
                    if let Some(name) = n.child_by_field_name("name") {
                        stack.push(name);
                    }
                }
            }
        }
        out
    }

    fn foreach(&mut self, node: Node) {
        let coll = node.child_by_field_name("right").map(|r| self.expr(r));
        self.b.push_block();
        let ty = node
            .child_by_field_name("type")
            .filter(|t| t.kind() != "implicit_type");
        if let (Some(left), Some(coll)) = (node.child_by_field_name("left"), coll) {
            for (name, n) in self.pattern_names(left) {
                let pos = self.b.pos(n);
                let var = self.b.declare(&name, pos, false);
                self.b.copy(n, var, coll);
                self.b.emit(n, StmtKind::Elements { dst: var, coll });
                if let Some(t) = ty {
                    self.type_ref(var, t);
                    if let Some(a) = self.annotation(t) {
                        self.b.ir.annots.push((var, a));
                    }
                }
            }
        }
        if let Some(b) = node.child_by_field_name("body") {
            self.stmt(b);
        }
        self.b.pop_scope();
    }

    fn try_stmt(&mut self, node: Node) {
        let has_catch = named_children(node)
            .iter()
            .any(|c| c.kind() == "catch_clause");
        let catch = has_catch.then(|| self.b.temp(self.b.pos(node)));
        for c in named_children(node) {
            match c.kind() {
                "block" => {
                    if let Some(v) = catch {
                        self.b.catches.push(v);
                    }
                    self.block_items(c);
                    if catch.is_some() {
                        self.b.catches.pop();
                    }
                }
                "catch_clause" => {
                    let Some(cv) = catch else { continue };
                    self.b.push_block();
                    for k in named_children(c) {
                        match k.kind() {
                            "catch_declaration" => {
                                if let Some(n) = k.child_by_field_name("name") {
                                    let pos = self.b.pos(n);
                                    let var = self.b.declare(self.text(n), pos, false);
                                    self.b.copy(n, var, cv);
                                    if let Some(t) = k.child_by_field_name("type") {
                                        self.type_ref(var, t);
                                    }
                                }
                            }
                            "catch_filter_clause" => {
                                for e in named_children(k) {
                                    self.expr(e);
                                }
                            }
                            "block" => {
                                self.rethrow.push(cv);
                                self.block_items(k);
                                self.rethrow.pop();
                            }
                            _ => {}
                        }
                    }
                    self.b.pop_scope();
                }
                "finally_clause" => {
                    for k in named_children(c) {
                        self.stmt(k);
                    }
                }
                _ => {}
            }
        }
    }

    fn switch_stmt(&mut self, node: Node) {
        let subject = node.child_by_field_name("value").map(|v| self.expr(v));
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        for section in named_children(body) {
            if section.kind() != "switch_section" {
                continue;
            }
            self.b.push_block();
            for c in named_children(section) {
                if c.kind().ends_with("_statement") || c.kind() == "block" {
                    self.stmt(c);
                } else if c.kind() == "when_clause" {
                    for e in named_children(c) {
                        self.expr(e);
                    }
                } else if let Some(s) = subject {
                    self.pattern(c, s);
                }
            }
            self.b.pop_scope();
        }
    }

    /// Bind the names a pattern declares to (parts of) `subject`.
    fn pattern(&mut self, p: Node, subject: Var) {
        match p.kind() {
            "declaration_pattern" | "var_pattern" => {
                if let Some(n) = p.child_by_field_name("name").or_else(|| {
                    named_children(p)
                        .into_iter()
                        .find(|c| c.kind() == "identifier")
                }) {
                    let pos = self.b.pos(n);
                    let var = self.b.declare(self.text(n), pos, false);
                    self.b.copy(n, var, subject);
                    if let Some(t) = p
                        .child_by_field_name("type")
                        .filter(|t| t.kind() != "implicit_type")
                    {
                        self.type_ref(var, t);
                        if let Some(a) = self.annotation(t) {
                            self.b.ir.annots.push((var, a));
                        }
                    }
                }
            }
            "recursive_pattern" => {
                for c in named_children(p) {
                    match c.kind() {
                        "property_pattern_clause" => {
                            for sp in named_children(c) {
                                if sp.kind() != "subpattern" {
                                    continue;
                                }
                                let parts = named_children(sp);
                                if parts.len() == 2 && parts[0].kind() == "identifier" {
                                    let field = self.text(parts[0]).to_string();
                                    let v = self.b.load(sp, subject, Some(field));
                                    self.pattern(parts[1], v);
                                } else if let Some(last) = parts.last() {
                                    self.pattern(*last, subject);
                                }
                            }
                        }
                        "positional_pattern_clause" => {
                            for sp in named_children(c) {
                                for k in named_children(sp) {
                                    self.pattern(k, subject);
                                }
                            }
                        }
                        "identifier" if Some(c) == p.child_by_field_name("name") => {
                            let pos = self.b.pos(c);
                            let var = self.b.declare(self.text(c), pos, false);
                            self.b.copy(c, var, subject);
                        }
                        _ => {}
                    }
                }
            }
            "and_pattern"
            | "or_pattern"
            | "parenthesized_pattern"
            | "negated_pattern"
            | "list_pattern"
            | "subpattern"
            | "tuple_pattern" => {
                for c in named_children(p) {
                    self.pattern(c, subject);
                }
            }
            "constant_pattern" | "relational_pattern" | "type_pattern" | "discard" => {}
            _ => {}
        }
    }

    // ---------------------------------------------------------------- expressions

    /// An expression whose value is assigned to something declared with type `ty`: a
    /// target-typed `new(...)` constructs that type.
    fn expr_typed(&mut self, node: Node, ty: Option<Node>) -> Var {
        if node.kind() == "implicit_object_creation_expression"
            && let Some(t) = ty
        {
            return self.creation(node, Some(t));
        }
        self.expr(node)
    }

    fn expr(&mut self, node: Node) -> Var {
        match node.kind() {
            "identifier" => self.ident(node),
            "this" => match self.this() {
                Some(t) => t,
                None => self.b.lit(node, None),
            },
            "base" => self.super_value(node),
            "generic_name" | "qualified_name" | "alias_qualified_name" | "predefined_type" => {
                match self.type_value(node, true) {
                    Some(v) => v,
                    None => match node.kind() {
                        "predefined_type" => {
                            let name = predefined(self.text(node)).to_string();
                            self.import(node, name, ImportKey::Namespace)
                        }
                        _ => self.b.lit(node, None),
                    },
                }
            }
            "member_access_expression" => self.member_access(node),
            "member_binding_expression" => {
                let recv = match self.cond.last() {
                    Some(&r) => r,
                    None => return self.b.lit(node, None),
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| simple_name(n, self.b.src));
                self.b.load_member(node, recv, name)
            }
            "element_binding_expression" => {
                let recv = match self.cond.last() {
                    Some(&r) => r,
                    None => return self.b.lit(node, None),
                };
                self.index(node, recv, named_children(node))
            }
            "conditional_access_expression" => {
                let recv = match node.child_by_field_name("condition") {
                    Some(c) => self.expr(c),
                    None => return self.b.lit(node, None),
                };
                self.cond.push(recv);
                let rest = named_children(node)
                    .into_iter()
                    .find(|c| Some(*c) != node.child_by_field_name("condition"));
                let v = match rest {
                    Some(r) => self.expr(r),
                    None => recv,
                };
                self.cond.pop();
                v
            }
            "element_access_expression" => {
                let obj = match node.child_by_field_name("expression") {
                    Some(o) => self.expr(o),
                    None => return self.b.lit(node, None),
                };
                let args = node
                    .child_by_field_name("subscript")
                    .map(named_children)
                    .unwrap_or_default();
                self.index(node, obj, args)
            }
            "invocation_expression" => self.invocation(node),
            "object_creation_expression" => self.creation(node, None),
            "implicit_object_creation_expression" => self.creation(node, None),
            "anonymous_object_creation_expression" => {
                let dst = self.b.lit(node, None);
                for c in named_children(node) {
                    match c.kind() {
                        "assignment_expression" => {
                            let key = c
                                .child_by_field_name("left")
                                .map(|l| self.text(l).to_string());
                            if let Some(r) = c.child_by_field_name("right") {
                                let v = self.expr(r);
                                self.b.emit(
                                    c,
                                    StmtKind::Store {
                                        obj: dst,
                                        field: key,
                                        src: v,
                                    },
                                );
                            }
                        }
                        _ => {
                            // `new { user.Email }` is a member named `Email`.
                            let key = Some(last_identifier(c, self.b.src));
                            let v = self.expr(c);
                            self.b.emit(
                                c,
                                StmtKind::Store {
                                    obj: dst,
                                    field: key,
                                    src: v,
                                },
                            );
                        }
                    }
                }
                dst
            }
            "with_expression" => {
                let children = named_children(node);
                let src = match children.first() {
                    Some(s) => self.expr(*s),
                    None => return self.b.lit(node, None),
                };
                let dst = self.b.temp(self.b.pos(node));
                self.b.copy(node, dst, src);
                if let Some(init) = children.iter().find(|c| c.kind() == "with_initializer") {
                    self.member_initializers(*init, dst);
                }
                dst
            }
            "array_creation_expression"
            | "implicit_array_creation_expression"
            | "stackalloc_expression"
            | "implicit_stackalloc_expression"
            | "initializer_expression"
            | "collection_expression" => {
                let dst = self.b.lit(node, None);
                self.elements_into(node, dst);
                dst
            }
            "tuple_expression" => {
                let dst = self.b.lit(node, None);
                for (i, a) in named_children(node).into_iter().enumerate() {
                    let name = a
                        .child_by_field_name("name")
                        .map(|n| self.text(n).to_string())
                        .unwrap_or_else(|| format!("Item{}", i + 1));
                    if let Some(e) = named_children(a)
                        .into_iter()
                        .rfind(|c| Some(*c) != a.child_by_field_name("name"))
                    {
                        let v = self.expr(e);
                        self.b.emit(
                            a,
                            StmtKind::Store {
                                obj: dst,
                                field: Some(name),
                                src: v,
                            },
                        );
                        self.b.copy(a, dst, v);
                    }
                }
                dst
            }
            "string_literal" | "verbatim_string_literal" | "raw_string_literal" => {
                let v = string_value(node, self.b.src);
                self.b.lit(node, v)
            }
            "interpolated_string_expression" => self.interpolated(node),
            "typeof_expression" => self.import(node, "System.Type".into(), ImportKey::Namespace),
            "character_literal"
            | "integer_literal"
            | "real_literal"
            | "boolean_literal"
            | "null_literal"
            | "default_expression"
            | "sizeof_expression"
            | "makeref_expression"
            | "reftype_expression"
            | "refvalue_expression"
            | "discard"
            | "comment" => self.b.lit(node, None),
            "binary_expression" => self.binary(node),
            "assignment_expression" => self.assignment(node),
            "conditional_expression" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                let vs: Vec<Var> = ["consequence", "alternative"]
                    .iter()
                    .filter_map(|f| node.child_by_field_name(f))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|c| self.expr(c))
                    .collect();
                self.b.union(node, &vs)
            }
            "switch_expression" => {
                let children = named_children(node);
                let subject = match children.first() {
                    Some(s) => self.expr(*s),
                    None => return self.b.lit(node, None),
                };
                let mut vs = Vec::new();
                for arm in children.iter().skip(1) {
                    if arm.kind() != "switch_expression_arm" {
                        continue;
                    }
                    self.b.push_block();
                    let parts = named_children(*arm);
                    for (i, p) in parts.iter().enumerate() {
                        if i + 1 == parts.len() {
                            vs.push(self.expr(*p));
                        } else if p.kind() == "when_clause" {
                            for e in named_children(*p) {
                                self.expr(e);
                            }
                        } else {
                            self.pattern(*p, subject);
                        }
                    }
                    self.b.pop_scope();
                }
                self.b.union(node, &vs)
            }
            "is_pattern_expression" => {
                let subject = node.child_by_field_name("expression").map(|e| self.expr(e));
                if let (Some(s), Some(p)) = (subject, node.child_by_field_name("pattern")) {
                    // The names a pattern declares are visible after it (`if (x is User u)`).
                    self.pattern(p, s);
                }
                self.b.lit(node, None)
            }
            "is_expression" => {
                if let Some(l) = node.child_by_field_name("left") {
                    self.expr(l);
                }
                self.b.lit(node, None)
            }
            "as_expression" => match node.child_by_field_name("left") {
                Some(l) => self.expr(l),
                None => self.b.lit(node, None),
            },
            "cast_expression" => match node.child_by_field_name("value") {
                Some(v) => self.expr(v),
                None => self.b.lit(node, None),
            },
            "postfix_unary_expression" => {
                let inner = named_children(node).into_iter().next();
                let op_is_bang = self.text(node).trim_end().ends_with('!');
                match inner {
                    Some(i) if op_is_bang => self.expr(i),
                    Some(i) => {
                        self.expr(i);
                        self.b.lit(node, None)
                    }
                    None => self.b.lit(node, None),
                }
            }
            "prefix_unary_expression" => {
                for c in named_children(node) {
                    self.expr(c);
                }
                self.b.lit(node, None)
            }
            "await_expression"
            | "parenthesized_expression"
            | "checked_expression"
            | "ref_expression"
            | "range_expression" => {
                let vs: Vec<Var> = named_children(node)
                    .into_iter()
                    .map(|c| self.expr(c))
                    .collect();
                match vs.as_slice() {
                    [one] => *one,
                    _ => self.b.union(node, &vs),
                }
            }
            "lambda_expression" | "anonymous_method_expression" => self.lambda(node),
            "throw_expression" => {
                if let Some(e) = node.named_child(0) {
                    let v = self.expr(e);
                    self.b.throw(node, v);
                }
                self.b.lit(node, None)
            }
            "declaration_expression" => {
                let Some(n) = node.child_by_field_name("name") else {
                    return self.b.lit(node, None);
                };
                let pos = self.b.pos(n);
                let var = self.b.declare(self.text(n), pos, false);
                if let Some(t) = node
                    .child_by_field_name("type")
                    .filter(|t| t.kind() != "implicit_type")
                {
                    self.type_ref(var, t);
                }
                var
            }
            "query_expression" => self.query(node),
            "argument" => match named_children(node)
                .into_iter()
                .rfind(|c| Some(*c) != node.child_by_field_name("name"))
            {
                Some(e) => self.expr(e),
                None => self.b.lit(node, None),
            },
            _ => {
                let vs: Vec<Var> = named_children(node)
                    .into_iter()
                    .map(|c| self.expr(c))
                    .collect();
                self.b.union(node, &vs)
            }
        }
    }

    fn super_value(&mut self, node: Node) -> Var {
        match self.class() {
            Some(class) => {
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::Super { dst, class });
                dst
            }
            None => self.b.lit(node, None),
        }
    }

    /// A bare identifier, resolved as the compiler would (see the module documentation).
    fn ident(&mut self, node: Node) -> Var {
        let name = self.text(node);
        if let Some(v) = self.b.lookup(name) {
            return v;
        }
        if self.is_type_param(name) {
            return self.b.lit(node, None);
        }
        if let Some(fi) = self.member_frame(name) {
            return self.member_of_frame(node, fi, name);
        }
        let enclosing = self.type_ids();
        let res = self
            .ix
            .resolve_simple(&self.path, self.scope, &enclosing, name, None);
        // In a class that extends a type outside the project, a name no declaration in scope
        // explains is most likely one of that type's members (`Request` in a controller).
        let inherited = self.open_class()
            && match &res {
                Res::Unknown => true,
                Res::External(c) => !c.iter().any(|x| self.ix.is_spelled(x)),
                _ => false,
            };
        if inherited && let Some(this) = self.this() {
            return self.b.load_member(node, this, Some(name.to_string()));
        }
        if res == Res::Unknown {
            for st in self.ix.static_usings(&self.path, self.scope) {
                let full = format!("{st}.{name}");
                if self.ix.is_spelled(&full) {
                    return self.import(node, full, ImportKey::Namespace);
                }
            }
        }
        self.resolved(node, res, name)
    }

    /// The field of the current instance an expression reads, if it reads one: `_items`,
    /// `Messages`, `this.Messages`.
    fn field_of_this(&self, e: Node) -> Option<String> {
        let name = match e.kind() {
            "identifier" => {
                let n = self.text(e);
                if self.b.lookup(n).is_some() {
                    return None;
                }
                n
            }
            "member_access_expression"
                if e.child_by_field_name("expression").is_none()
                    && has_token(e, self.b.src, "this") =>
            {
                self.text(e.child_by_field_name("name")?)
            }
            _ => return None,
        };
        let fi = self.member_frame(name)?;
        if fi + 1 != self.frames.len() {
            return None;
        }
        let id = self.frames[fi].id?;
        match self.ix.member(id, name)?.kind {
            scan::MemberKind::Field
            | scan::MemberKind::Property
            | scan::MemberKind::PrimaryParam => Some(name.to_string()),
            _ => None,
        }
    }

    /// A member of an enclosing type, read through its instance.
    fn member_of_frame(&mut self, node: Node, fi: usize, name: &str) -> Var {
        let c = self.frames[fi].class;
        let this = if fi + 1 == self.frames.len() {
            self.this()
        } else {
            Some(self.b.ir.classes[c as usize].this)
        };
        match this {
            Some(t) => self.b.load_member(node, t, Some(name.to_string())),
            None => self.b.lit(node, None),
        }
    }

    /// The identifiers of a dotted chain (`A.B.C`), if the expression is one.
    fn chain<'t>(&self, node: Node<'t>, out: &mut Vec<(String, Node<'t>)>) -> bool {
        match node.kind() {
            "identifier" => {
                out.push((self.text(node).to_string(), node));
                true
            }
            "member_access_expression" => {
                let (Some(e), Some(n)) = (
                    node.child_by_field_name("expression"),
                    node.child_by_field_name("name"),
                ) else {
                    return false;
                };
                if n.kind() != "identifier" || !self.chain(e, out) {
                    return false;
                }
                out.push((self.text(n).to_string(), n));
                true
            }
            _ => false,
        }
    }

    fn member_access(&mut self, node: Node) -> Var {
        let name = node
            .child_by_field_name("name")
            .map(|n| simple_name(n, self.b.src));
        // `typeof(T).Name`, `x.GetType().FullName`: metadata of a type, not data.
        if let Some(e) = node.child_by_field_name("expression")
            && is_type_metadata(e, self.b.src)
        {
            self.expr(e);
            return self.b.lit(node, None);
        }
        // `A.B.C` where `A` is not a value in scope: a namespace or type path.
        let mut segs = Vec::new();
        if self.chain(node, &mut segs) {
            let head = segs[0].0.as_str();
            let value_head = self.b.lookup(head).is_some()
                || self.is_type_param(head)
                || self.member_frame(head).is_some();
            if !value_head {
                let names: Vec<&str> = segs.iter().map(|(s, _)| s.as_str()).collect();
                let enclosing = self.type_ids();
                let (used, res) = self
                    .ix
                    .resolve_path(&self.path, self.scope, &enclosing, &names, None);
                if !matches!(res, Res::Unknown) && used > 1 {
                    let prefix_node = segs[used - 1].1;
                    let anchor = if used == segs.len() {
                        node
                    } else {
                        prefix_node
                    };
                    let mut v = self.resolved(anchor, res, &names[..used].join("."));
                    for (s, _) in &segs[used..] {
                        v = self.b.load_member(node, v, Some(s.clone()));
                    }
                    return v;
                }
            }
        }
        let obj = match node.child_by_field_name("expression") {
            Some(e) => self.expr(e),
            None => {
                // `this.X` / `base.X`: the token is anonymous.
                if has_token(node, self.b.src, "base") {
                    self.super_value(node)
                } else {
                    match self.this() {
                        Some(t) => t,
                        None => return self.b.lit(node, None),
                    }
                }
            }
        };
        self.b.load_member(node, obj, name)
    }

    fn index(&mut self, node: Node, obj: Var, args: Vec<Node>) -> Var {
        let args: Vec<Node> = args
            .into_iter()
            .map(|a| {
                if a.kind() == "argument" {
                    named_children(a).into_iter().next_back().unwrap_or(a)
                } else {
                    a
                }
            })
            .collect();
        match args.as_slice() {
            [a] if matches!(a.kind(), "string_literal" | "verbatim_string_literal") => {
                let key = string_value(*a, self.b.src);
                self.b.load(node, obj, key)
            }
            // A range or an index into a sequence: part of the same value.
            [a] if matches!(a.kind(), "range_expression" | "integer_literal")
                || a.kind() == "prefix_unary_expression" =>
            {
                self.expr(*a);
                self.b.union(node, &[obj])
            }
            _ => {
                for a in args {
                    self.expr(a);
                }
                self.b.load(node, obj, None)
            }
        }
    }

    fn interpolated(&mut self, node: Node) -> Var {
        let mut parts = Vec::new();
        for c in named_children(node) {
            match c.kind() {
                "string_content" | "escape_sequence" => {
                    parts.push(Part::Lit(self.text(c).to_string()))
                }
                "interpolation" => {
                    for e in named_children(c) {
                        if matches!(
                            e.kind(),
                            "interpolation_brace"
                                | "interpolation_alignment_clause"
                                | "interpolation_format_clause"
                        ) {
                            continue;
                        }
                        let v = self.expr(e);
                        parts.push(Part::Var(v));
                    }
                }
                _ => {}
            }
        }
        if parts.iter().all(|p| matches!(p, Part::Lit(_))) {
            let s: String = parts
                .into_iter()
                .map(|p| match p {
                    Part::Lit(s) => s,
                    Part::Var(_) => String::new(),
                })
                .collect();
            return self.b.lit(node, Some(s));
        }
        self.b.concat(node, parts)
    }

    fn binary(&mut self, node: Node) -> Var {
        let op = node
            .child_by_field_name("operator")
            .map(|o| self.text(o))
            .unwrap_or("");
        let l = node.child_by_field_name("left").map(|l| self.expr(l));
        let r = node.child_by_field_name("right").map(|r| self.expr(r));
        match op {
            "+" => {
                let parts = l.into_iter().chain(r).map(Part::Var).collect();
                self.b.concat(node, parts)
            }
            "??" => {
                let vs: Vec<Var> = l.into_iter().chain(r).collect();
                self.b.union(node, &vs)
            }
            _ => self.b.lit(node, None),
        }
    }

    fn assignment(&mut self, node: Node) -> Var {
        let op = node
            .child_by_field_name("operator")
            .map(|o| self.text(o).to_string())
            .unwrap_or_else(|| "=".into());
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");
        let r = match right {
            Some(r) => self.expr(r),
            None => return self.b.lit(node, None),
        };
        let v = match op.as_str() {
            "=" => r,
            "+=" | "??=" => {
                let l = left.map(|l| self.expr(l));
                let vs: Vec<Var> = l.into_iter().chain([r]).collect();
                if op == "+=" {
                    self.b.concat(node, vs.into_iter().map(Part::Var).collect())
                } else {
                    self.b.union(node, &vs)
                }
            }
            _ => self.b.lit(node, None),
        };
        if let Some(l) = left {
            self.assign_to(l, v);
        }
        v
    }

    fn assign_to(&mut self, left: Node, v: Var) {
        match left.kind() {
            "identifier" => {
                let name = self.text(left);
                if let Some(var) = self.b.lookup(name) {
                    self.b.copy(left, var, v);
                    return;
                }
                // A member of the class (or of a base outside the project); otherwise a
                // discard (`_ = ...`).
                if (self.member_frame(name).is_some() || self.open_class())
                    && let Some(this) = self.this()
                {
                    self.b.store_member(left, this, Some(name.to_string()), v);
                }
            }
            "member_access_expression" => {
                let field = left
                    .child_by_field_name("name")
                    .map(|n| simple_name(n, self.b.src));
                let obj = match left.child_by_field_name("expression") {
                    Some(e) => self.expr(e),
                    None => match self.this() {
                        Some(t) => t,
                        None => return,
                    },
                };
                self.b.store_member(left, obj, field, v);
            }
            "member_binding_expression" => {
                if let Some(&recv) = self.cond.last() {
                    let field = left
                        .child_by_field_name("name")
                        .map(|n| simple_name(n, self.b.src));
                    self.b.store_member(left, recv, field, v);
                }
            }
            "element_access_expression" => {
                let Some(o) = left.child_by_field_name("expression") else {
                    return;
                };
                let obj = self.expr(o);
                let args: Vec<Node> = left
                    .child_by_field_name("subscript")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| named_children(a).into_iter().next_back().unwrap_or(a))
                    .collect();
                let field = match args.as_slice() {
                    [a] if matches!(a.kind(), "string_literal" | "verbatim_string_literal") => {
                        string_value(*a, self.b.src)
                    }
                    _ => {
                        for a in args {
                            self.expr(a);
                        }
                        None
                    }
                };
                self.b.emit(left, StmtKind::Store { obj, field, src: v });
            }
            "tuple_expression" | "declaration_expression" | "parenthesized_expression" => {
                for c in named_children(left) {
                    let target = if c.kind() == "argument" {
                        named_children(c).into_iter().next_back().unwrap_or(c)
                    } else {
                        c
                    };
                    self.assign_to(target, v);
                }
                if left.kind() == "declaration_expression" {
                    let var = self.expr(left);
                    self.b.copy(left, var, v);
                }
            }
            "conditional_access_expression" => {
                let recv = left.child_by_field_name("condition").map(|c| self.expr(c));
                if let Some(r) = recv {
                    self.cond.push(r);
                    for c in named_children(left) {
                        if Some(c) != left.child_by_field_name("condition") {
                            self.assign_to(c, v);
                        }
                    }
                    self.cond.pop();
                }
            }
            _ => {}
        }
    }

    /// `new T(args) { A = x }`, `new(args)`, `new List<T> { a, b }`.
    fn creation(&mut self, node: Node, target: Option<Node>) -> Var {
        let ty = node.child_by_field_name("type").or(target);
        let args = node.child_by_field_name("arguments").or_else(|| {
            named_children(node)
                .into_iter()
                .find(|c| c.kind() == "argument_list")
        });
        let callee = ty.and_then(|t| self.type_value(t, true));
        let dst = match callee {
            Some(cv) => self.invoke(node, Callee::Value(cv), args, true),
            None => {
                let dst = self.b.lit(node, None);
                if let Some(a) = args {
                    for arg in named_children(a) {
                        let v = self.expr(arg);
                        self.b.copy(arg, dst, v);
                    }
                }
                dst
            }
        };
        if let Some(init) = node.child_by_field_name("initializer").or_else(|| {
            named_children(node)
                .into_iter()
                .find(|c| c.kind() == "initializer_expression")
        }) {
            self.member_initializers(init, dst);
        }
        dst
    }

    /// `{ A = x, ["k"] = y, z }`: named members, indexed entries, collection elements.
    fn member_initializers(&mut self, init: Node, dst: Var) {
        let children = named_children(init);
        let mut i = 0;
        while i < children.len() {
            let c = children[i];
            match c.kind() {
                "assignment_expression" => {
                    let left = c.child_by_field_name("left");
                    let Some(r) = c.child_by_field_name("right") else {
                        i += 1;
                        continue;
                    };
                    let field = match left {
                        Some(l) if l.kind() == "identifier" => Some(self.text(l).to_string()),
                        Some(l) => {
                            let keys: Vec<Node> = named_children(l)
                                .into_iter()
                                .flat_map(|a| {
                                    if a.kind() == "argument" {
                                        named_children(a)
                                    } else {
                                        vec![a]
                                    }
                                })
                                .collect();
                            match keys.as_slice() {
                                [k] if k.kind() == "string_literal" => string_value(*k, self.b.src),
                                _ => None,
                            }
                        }
                        None => None,
                    };
                    let v = if r.kind() == "initializer_expression" {
                        // `Address = { City = x }`: a nested initializer.
                        let nested = self.b.load(c, dst, field.clone());
                        self.member_initializers(r, nested);
                        nested
                    } else {
                        self.expr(r)
                    };
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field,
                            src: v,
                        },
                    );
                }
                // `with { Name = "y" }` lists name, value pairs.
                "identifier" if init.kind() == "with_initializer" && i + 1 < children.len() => {
                    let field = Some(self.text(c).to_string());
                    let v = self.expr(children[i + 1]);
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field,
                            src: v,
                        },
                    );
                    i += 1;
                }
                _ => {
                    let v = self.expr(c);
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field: None,
                            src: v,
                        },
                    );
                }
            }
            i += 1;
        }
    }

    fn elements_into(&mut self, node: Node, dst: Var) {
        for c in named_children(node) {
            match c.kind() {
                "initializer_expression" => self.elements_into(c, dst),
                "collection_element" => {
                    for e in named_children(c) {
                        let inner = match e.kind() {
                            "expression_element" | "spread_element" => {
                                named_children(e).into_iter().next()
                            }
                            _ => Some(e),
                        };
                        if let Some(x) = inner {
                            let v = self.expr(x);
                            if e.kind() == "spread_element" {
                                self.b.copy(e, dst, v);
                            } else {
                                self.b.emit(
                                    e,
                                    StmtKind::Store {
                                        obj: dst,
                                        field: None,
                                        src: v,
                                    },
                                );
                            }
                        }
                    }
                }
                "array_type" | "array_rank_specifier" | "predefined_type" | "identifier"
                    if node.kind() != "initializer_expression" =>
                {
                    // The array's type and sizes.
                    if c.kind() == "array_rank_specifier" {
                        for e in named_children(c) {
                            self.expr(e);
                        }
                    }
                }
                _ => {
                    let v = self.expr(c);
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field: None,
                            src: v,
                        },
                    );
                }
            }
        }
    }

    fn lambda(&mut self, node: Node) -> Var {
        let attrs = self.attributes(node);
        let param_attrs: Vec<Vec<Var>> = node
            .child_by_field_name("parameters")
            .filter(|p| p.kind() == "parameter_list")
            .map(|pl| {
                named_children(pl)
                    .into_iter()
                    .filter(|p| p.kind() == "parameter")
                    .map(|p| self.attributes(p))
                    .collect()
            })
            .unwrap_or_default();
        let f = self.b.begin_function("<lambda>".into(), node, None);
        self.b.ir.funcs[f as usize].decorators = attrs;
        self.ref_params.push(Vec::new());
        match node.child_by_field_name("parameters") {
            Some(p) if p.kind() == "parameter_list" => {
                self.params(p);
                for (q, a) in self.b.ir.funcs[f as usize]
                    .params
                    .iter_mut()
                    .zip(param_attrs)
                {
                    q.attrs = a;
                }
            }
            Some(p) if p.kind() == "implicit_parameter" => {
                let name = self.text(p).to_string();
                let pos = self.b.pos(p);
                let var = self.b.declare_param(&name, pos);
                self.b.ir.funcs[f as usize].params.push(Param {
                    var,
                    attrs: Vec::new(),
                    name,
                    rest: false,
                    kwrest: false,
                    annot: None,
                });
            }
            _ => {
                // `delegate (int x) { ... }`
                if let Some(pl) = named_children(node)
                    .into_iter()
                    .find(|c| c.kind() == "parameter_list")
                {
                    self.params(pl);
                }
            }
        }
        let body = node.child_by_field_name("body").or_else(|| {
            named_children(node)
                .into_iter()
                .find(|c| c.kind() == "block")
        });
        if let Some(b) = body {
            self.body(b);
        }
        self.finish_function_refs();
        self.b.end_function(f);
        let dst = self.b.temp(self.b.pos(node));
        self.b.emit(node, StmtKind::FuncRef { dst, func: f });
        dst
    }

    /// LINQ query syntax: each range variable is an element of its source; `select` and
    /// `group` put their value into the result's elements.
    fn query(&mut self, node: Node) -> Var {
        self.b.push_block();
        let dst = self.b.lit(node, None);
        let mut stack = named_children(node);
        stack.reverse();
        while let Some(c) = stack.pop() {
            match c.kind() {
                "from_clause" | "join_clause" => {
                    let name = c.child_by_field_name("name").or_else(|| {
                        named_children(c)
                            .into_iter()
                            .find(|k| k.kind() == "identifier")
                    });
                    let exprs: Vec<Node> = named_children(c)
                        .into_iter()
                        .filter(|k| Some(*k) != name && !k.kind().ends_with("_type"))
                        .collect();
                    let mut source = None;
                    for (i, e) in exprs.iter().enumerate() {
                        let v = self.expr(*e);
                        if i == 0 {
                            source = Some(v);
                        }
                    }
                    if let (Some(n), Some(s)) = (name, source) {
                        let pos = self.b.pos(n);
                        let var = self.b.declare(self.text(n), pos, false);
                        self.b.copy(n, var, s);
                        self.b.emit(n, StmtKind::Elements { dst: var, coll: s });
                    }
                }
                "let_clause" => {
                    let parts = named_children(c);
                    if let [n, e, ..] = parts.as_slice() {
                        let v = self.expr(*e);
                        let pos = self.b.pos(*n);
                        let var = self.b.declare(self.text(*n), pos, false);
                        self.b.copy(*n, var, v);
                    }
                }
                "select_clause" | "group_clause" => {
                    for e in named_children(c) {
                        let v = self.expr(e);
                        self.b.emit(
                            c,
                            StmtKind::Store {
                                obj: dst,
                                field: None,
                                src: v,
                            },
                        );
                    }
                }
                "query_continuation" => {
                    for k in named_children(c).into_iter().rev() {
                        stack.push(k);
                    }
                }
                _ => {
                    for e in named_children(c) {
                        self.expr(e);
                    }
                }
            }
        }
        self.b.pop_scope();
        dst
    }

    // ---------------------------------------------------------------- calls

    fn invocation(&mut self, node: Node) -> Var {
        let args = node.child_by_field_name("arguments");
        let Some(func) = node.child_by_field_name("function") else {
            return self.b.lit(node, None);
        };
        match func.kind() {
            // `nameof(x)` is the name.
            "identifier" if self.text(func) == "nameof" && self.b.lookup("nameof").is_none() => {
                let name = args
                    .and_then(|a| named_children(a).into_iter().next())
                    .map(|a| last_identifier(a, self.b.src));
                self.b.lit(node, name)
            }
            "identifier" | "generic_name" => {
                let name = simple_name(func, self.b.src);
                if let Some(v) = self.b.lookup(&name) {
                    return self.invoke(node, Callee::Value(v), args, false);
                }
                // A method of an enclosing type (or inherited from one outside the project).
                let member = self.member_frame(&name);
                if member.is_some() || (self.open_class() && self.class().is_some()) {
                    let fi = member.unwrap_or(self.frames.len() - 1);
                    let recv = if fi + 1 == self.frames.len() {
                        self.this()
                    } else {
                        Some(self.b.ir.classes[self.frames[fi].class as usize].this)
                    };
                    if let Some(recv) = recv {
                        let dst = self.invoke(node, Callee::Method { recv, name }, args, false);
                        self.generic_result(func, dst);
                        return dst;
                    }
                }
                for st in self.ix.static_usings(&self.path, self.scope) {
                    let full = format!("{st}.{name}");
                    if self.ix.is_spelled(&full) {
                        let v = self.import(func, full, ImportKey::Namespace);
                        return self.invoke(node, Callee::Value(v), args, false);
                    }
                }
                let v = self.expr(func);
                self.invoke(node, Callee::Value(v), args, false)
            }
            "member_access_expression" | "member_binding_expression" => {
                let name = func
                    .child_by_field_name("name")
                    .map(|n| simple_name(n, self.b.src))
                    .unwrap_or_default();
                // `x.GetType()`: a `System.Type`, which holds none of `x`'s data.
                if is_type_metadata(node, self.b.src) {
                    if let Some(e) = func.child_by_field_name("expression") {
                        self.expr(e);
                    }
                    return self.import(node, "System.Type".into(), ImportKey::Namespace);
                }
                // `x.ConfigureAwait(false)` is `x`.
                if name == "ConfigureAwait"
                    && func.kind() == "member_access_expression"
                    && let Some(e) = func.child_by_field_name("expression")
                {
                    return self.expr(e);
                }
                let recv = if func.kind() == "member_binding_expression" {
                    match self.cond.last() {
                        Some(&r) => r,
                        None => return self.b.lit(node, None),
                    }
                } else {
                    match func.child_by_field_name("expression") {
                        Some(e) => self.expr(e),
                        None => {
                            if has_token(func, self.b.src, "base") {
                                self.super_value(func)
                            } else {
                                match self.this() {
                                    Some(t) => t,
                                    None => return self.b.lit(node, None),
                                }
                            }
                        }
                    }
                };
                let dst = self.invoke(node, Callee::Method { recv, name }, args, false);
                if let Some(n) = func.child_by_field_name("name") {
                    self.generic_result(n, dst);
                }
                // `Messages.Add(m)`, `_items.Add(x)`: a call may change the object a field
                // holds, and later reads of the field see it.
                if func.kind() == "member_access_expression"
                    && let Some(field) = func
                        .child_by_field_name("expression")
                        .and_then(|e| self.field_of_this(e))
                    && let Some(this) = self.this()
                {
                    self.b.store_member(func, this, Some(field), recv);
                }
                dst
            }
            // `message?.Trim()`: a method call on the condition, when it is not null.
            "conditional_access_expression"
                if named_children(func)
                    .last()
                    .is_some_and(|c| c.kind() == "member_binding_expression") =>
            {
                let Some(cond) = func.child_by_field_name("condition") else {
                    return self.b.lit(node, None);
                };
                let recv = self.expr(cond);
                let binding = *named_children(func).last().expect("binding");
                let name = binding
                    .child_by_field_name("name")
                    .map(|n| simple_name(n, self.b.src))
                    .unwrap_or_default();
                self.invoke(node, Callee::Method { recv, name }, args, false)
            }
            _ => {
                let v = self.expr(func);
                self.invoke(node, Callee::Value(v), args, false)
            }
        }
    }

    /// `GetRequiredService<OrderService>()`, `Deserialize<Order>(json)`: a call with one type
    /// argument that names a type returns that type.
    fn generic_result(&mut self, name: Node, dst: Var) {
        if name.kind() != "generic_name" {
            return;
        }
        let Some(list) = named_children(name)
            .into_iter()
            .find(|c| c.kind() == "type_argument_list")
        else {
            return;
        };
        let targs = named_children(list);
        if let [t] = targs.as_slice()
            && let Some(tv) = self.type_value(*t, false)
        {
            self.b.emit(name, StmtKind::TypeRef { dst, ty: tv });
        }
    }

    /// Emit a call with the arguments of an argument list.
    fn invoke(&mut self, node: Node, callee: Callee, args: Option<Node>, is_new: bool) -> Var {
        let mut out = Vec::new();
        let mut outs = Vec::new();
        if let Some(a) = args {
            for arg in named_children(a) {
                if arg.kind() != "argument" {
                    continue;
                }
                let name = arg.child_by_field_name("name");
                let Some(e) = named_children(arg).into_iter().rfind(|c| Some(*c) != name) else {
                    continue;
                };
                let by_ref = has_token(arg, self.b.src, "out") || has_token(arg, self.b.src, "ref");
                let v = self.expr(e);
                if by_ref {
                    outs.push((e, v));
                }
                out.push(Arg {
                    var: v,
                    kind: match name {
                        Some(n) => ArgKind::Keyword(self.text(n).to_string()),
                        None => ArgKind::Positional,
                    },
                });
            }
        }
        let dst = self.b.temp(self.b.pos(node));
        let argv: Vec<Var> = out.iter().map(|a| a.var).collect();
        self.b.emit(
            node,
            StmtKind::Call {
                dst,
                callee,
                args: out,
                is_new,
            },
        );
        self.b.echo(node, &argv);
        for (e, v) in outs {
            // `TryGetValue(key, out var user)`: what the call produces.
            self.b.copy(e, v, dst);
            if e.kind() != "declaration_expression" {
                self.assign_to(e, dst);
            }
        }
        dst
    }
}

/// The name a type is exported under: its qualified name, with its arity when generic.
pub fn export_name(def: &index::TypeDef) -> String {
    if def.arity == 0 {
        def.fqn.clone()
    } else {
        format!("{}`{}", def.fqn, def.arity)
    }
}

/// A declarator's initial value (the child after its name).
fn declarator_value(d: Node) -> Option<Node> {
    let name = d.child_by_field_name("name");
    named_children(d).into_iter().rfind(|c| {
        Some(*c) != name && c.kind() != "tuple_pattern" && c.kind() != "bracketed_argument_list"
    })
}

/// `typeof(T)` and `x.GetType()`: a `System.Type`.
fn is_type_metadata(e: Node, src: &str) -> bool {
    match e.kind() {
        "typeof_expression" => true,
        "invocation_expression" => {
            let Some(f) = e.child_by_field_name("function") else {
                return false;
            };
            let name = match f.kind() {
                "member_access_expression" => f.child_by_field_name("name"),
                "identifier" => Some(f),
                _ => None,
            };
            name.is_some_and(|n| &src[n.byte_range()] == "GetType")
                && e.child_by_field_name("arguments")
                    .is_some_and(|a| named_children(a).is_empty())
        }
        _ => false,
    }
}

/// `Foo` for `Foo` and `Foo<T>`.
fn simple_name(n: Node, src: &str) -> String {
    match n.kind() {
        "generic_name" => named_children(n)
            .into_iter()
            .next()
            .map(|i| src[i.byte_range()].to_string())
            .unwrap_or_default(),
        _ => name_text(n, src),
    }
}

/// The last identifier of an expression: `Email` in `user.Email`.
fn last_identifier(n: Node, src: &str) -> String {
    match n.kind() {
        "member_access_expression" => n
            .child_by_field_name("name")
            .map(|x| simple_name(x, src))
            .unwrap_or_default(),
        "identifier" => src[n.byte_range()].to_string(),
        _ => named_children(n)
            .into_iter()
            .next_back()
            .map(|c| last_identifier(c, src))
            .unwrap_or_default(),
    }
}
