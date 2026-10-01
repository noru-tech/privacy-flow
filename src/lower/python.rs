//! Python lowering (tree-sitter-python).
//!
//! Python scoping is per function: every name a function assigns is local to it unless the
//! function declares it `global` or `nonlocal`, so each function body is pre-scanned for its
//! targets before it is lowered. Class bodies get their own scope, invisible to the methods
//! inside them. A method's first parameter is bound to the class's shared instance variable,
//! so `self.email = email` in one method and `log(self.email)` in another meet.

use std::collections::HashSet;

use tree_sitter::{Node, Parser};

use super::{Builder, named_children, unquote};
use crate::ir::*;

pub fn lower(path: &str, src: &str) -> FileIr {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("grammar version matches tree-sitter");
    let mut py = Py {
        b: Builder::new(path, Lang::Python, src),
        globals: vec![HashSet::new()],
    };
    match parser.parse(src, None) {
        Some(tree) => {
            let root = tree.root_node();
            py.b.record_parse_errors(root);
            let mut names = Vec::new();
            let mut globals = HashSet::new();
            collect_targets(root, src, &mut names, &mut globals);
            for (n, node) in names {
                let pos = py.b.pos(node);
                py.b.declare(&n, pos, true);
            }
            for c in named_children(root) {
                py.stmt(c);
            }
            // Every module-level binding is importable.
            let mut exported: Vec<(String, Var)> = py.b.scopes[0]
                .names
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            exported.sort();
            for (name, var) in exported {
                py.b.ir.exports.push(ExportIr { name, var });
            }
        }
        None => py.b.ir.notes.push(LowerNote {
            kind: NoteKind::ParseError,
            pos: Pos { line: 1, column: 1 },
            detail: "parser returned no tree".into(),
        }),
    }
    py.b.finish()
}

struct Py<'s> {
    b: Builder<'s>,
    /// Names declared `global` in each enclosing function (innermost last).
    globals: Vec<HashSet<String>>,
}

/// Collect the names a function body (or module) binds, without entering nested scopes.
fn collect_targets<'t>(
    node: Node<'t>,
    src: &str,
    out: &mut Vec<(String, Node<'t>)>,
    globals: &mut HashSet<String>,
) {
    for c in named_children(node) {
        match c.kind() {
            "function_definition" | "class_definition" => {
                if let Some(n) = c.child_by_field_name("name") {
                    out.push((src[n.byte_range()].to_string(), n));
                }
            }
            "decorated_definition" => {
                if let Some(d) = c.child_by_field_name("definition")
                    && let Some(n) = d.child_by_field_name("name")
                {
                    out.push((src[n.byte_range()].to_string(), n));
                }
            }
            "global_statement" | "nonlocal_statement" => {
                for n in named_children(c) {
                    globals.insert(src[n.byte_range()].to_string());
                }
            }
            "assignment" | "augmented_assignment" => {
                if let Some(l) = c.child_by_field_name("left") {
                    target_names(l, src, out);
                }
                if let Some(r) = c.child_by_field_name("right")
                    && r.kind() == "assignment"
                {
                    collect_targets_expr(r, src, out);
                }
            }
            "for_statement" => {
                if let Some(l) = c.child_by_field_name("left") {
                    target_names(l, src, out);
                }
                collect_targets(c, src, out, globals);
            }
            "with_statement" | "with_clause" | "with_item" => {
                collect_targets(c, src, out, globals);
            }
            "as_pattern" => {
                if let Some(a) = c.child_by_field_name("alias") {
                    target_names(a, src, out);
                }
            }
            "except_clause" => {
                for e in named_children(c) {
                    if e.kind() == "as_pattern"
                        && let Some(a) = e.child_by_field_name("alias")
                    {
                        target_names(a, src, out);
                    }
                }
                collect_targets(c, src, out, globals);
            }
            "import_statement" | "import_from_statement" => {
                for n in named_children(c) {
                    match n.kind() {
                        "aliased_import" => {
                            if let Some(a) = n.child_by_field_name("alias") {
                                out.push((src[a.byte_range()].to_string(), a));
                            }
                        }
                        "dotted_name" if c.child_by_field_name("module_name") != Some(n) => {
                            let first = if c.kind() == "import_statement" {
                                n.named_child(0)
                            } else {
                                n.named_child((n.named_child_count() as u32).saturating_sub(1))
                            };
                            if let Some(f) = first {
                                out.push((src[f.byte_range()].to_string(), f));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "lambda"
            | "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => {}
            "named_expression" => {
                if let Some(n) = c.child_by_field_name("name") {
                    out.push((src[n.byte_range()].to_string(), n));
                }
            }
            "block"
            | "if_statement"
            | "elif_clause"
            | "else_clause"
            | "while_statement"
            | "try_statement"
            | "finally_clause"
            | "match_statement"
            | "case_clause"
            | "expression_statement" => collect_targets(c, src, out, globals),
            _ => {}
        }
    }
}

fn collect_targets_expr<'t>(node: Node<'t>, src: &str, out: &mut Vec<(String, Node<'t>)>) {
    if let Some(l) = node.child_by_field_name("left") {
        target_names(l, src, out);
    }
    if let Some(r) = node.child_by_field_name("right")
        && r.kind() == "assignment"
    {
        collect_targets_expr(r, src, out);
    }
}

fn target_names<'t>(node: Node<'t>, src: &str, out: &mut Vec<(String, Node<'t>)>) {
    match node.kind() {
        "identifier" => out.push((src[node.byte_range()].to_string(), node)),
        "pattern_list"
        | "tuple_pattern"
        | "list_pattern"
        | "as_pattern_target"
        | "list_splat_pattern"
        | "parenthesized_expression"
        | "tuple"
        | "list" => {
            for c in named_children(node) {
                target_names(c, src, out);
            }
        }
        _ => {}
    }
}

impl<'s> Py<'s> {
    fn text(&self, n: Node) -> &'s str {
        self.b.text(n)
    }

    fn in_class_body(&self) -> bool {
        self.b
            .scopes
            .last()
            .is_some_and(|s| s.kind == super::ScopeKind::Class)
    }

    // ---------------------------------------------------------------- statements

    fn stmts(&mut self, node: Node) {
        for c in named_children(node) {
            self.stmt(c);
        }
    }

    fn stmt(&mut self, node: Node) {
        match node.kind() {
            "expression_statement" => {
                for c in named_children(node) {
                    self.expr(c);
                }
            }
            "return_statement" => {
                if let Some(e) = node.named_child(0) {
                    let v = self.expr(e);
                    self.b.emit(node, StmtKind::Return { src: v });
                }
            }
            "if_statement" | "elif_clause" | "while_statement" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                if let Some(c) = node
                    .child_by_field_name("consequence")
                    .or_else(|| node.child_by_field_name("body"))
                {
                    self.stmts(c);
                }
                for c in named_children(node) {
                    if matches!(c.kind(), "elif_clause" | "else_clause") {
                        self.stmt(c);
                    }
                }
                if let Some(a) = node.child_by_field_name("alternative")
                    && !matches!(a.kind(), "elif_clause" | "else_clause")
                {
                    self.stmt(a);
                }
            }
            "else_clause" | "finally_clause" | "block" => {
                for c in named_children(node) {
                    if c.kind() == "block" {
                        self.stmts(c);
                    } else {
                        self.stmt(c);
                    }
                }
            }
            "for_statement" => {
                let right = node.child_by_field_name("right").map(|r| self.expr(r));
                if let (Some(l), Some(r)) = (node.child_by_field_name("left"), right) {
                    self.assign_to(l, r);
                }
                if let Some(b) = node.child_by_field_name("body") {
                    self.stmts(b);
                }
                if let Some(a) = node.child_by_field_name("alternative") {
                    self.stmt(a);
                }
            }
            "try_statement" => {
                for c in named_children(node) {
                    match c.kind() {
                        "block" => self.stmts(c),
                        "except_clause" | "except_group_clause" => {
                            for e in named_children(c) {
                                match e.kind() {
                                    "block" => self.stmts(e),
                                    "as_pattern" => {
                                        if let Some(a) = e.child_by_field_name("alias") {
                                            let v = self.b.lit(e, None);
                                            self.assign_to(a, v);
                                        }
                                    }
                                    _ => {
                                        self.expr(e);
                                    }
                                }
                            }
                        }
                        _ => self.stmt(c),
                    }
                }
            }
            "with_statement" => {
                for c in named_children(node) {
                    match c.kind() {
                        "with_clause" => {
                            for item in named_children(c) {
                                if let Some(value) = item.child_by_field_name("value") {
                                    if value.kind() == "as_pattern" {
                                        let src = value.named_child(0).map(|e| self.expr(e));
                                        if let (Some(src), Some(alias)) =
                                            (src, value.child_by_field_name("alias"))
                                        {
                                            self.assign_to(alias, src);
                                        }
                                    } else {
                                        self.expr(value);
                                    }
                                }
                            }
                        }
                        "block" => self.stmts(c),
                        _ => {}
                    }
                }
            }
            "match_statement" => {
                if let Some(s) = node.child_by_field_name("subject") {
                    self.expr(s);
                }
                for c in named_children(node) {
                    if c.kind() == "block" {
                        for case in named_children(c) {
                            if let Some(b) = case.child_by_field_name("consequence") {
                                self.stmts(b);
                            }
                        }
                    }
                }
            }
            "function_definition" => {
                self.def(node, Vec::new());
            }
            "decorated_definition" => {
                let mut decorators = Vec::new();
                for d in named_children(node) {
                    if d.kind() == "decorator"
                        && let Some(e) = d.named_child(0)
                    {
                        decorators.push((self.expr(e), self.text(e).to_string()));
                    }
                }
                if let Some(def) = node.child_by_field_name("definition") {
                    match def.kind() {
                        "function_definition" => {
                            self.def(def, decorators);
                        }
                        "class_definition" => self.class(def),
                        _ => {}
                    }
                }
            }
            "class_definition" => self.class(node),
            "import_statement" => self.import(node),
            "import_from_statement" => self.import_from(node),
            "future_import_statement"
            | "global_statement"
            | "nonlocal_statement"
            | "pass_statement"
            | "break_statement"
            | "continue_statement"
            | "comment" => {}
            "raise_statement" | "assert_statement" | "delete_statement" | "print_statement"
            | "exec_statement" => {
                for c in named_children(node) {
                    self.expr(c);
                }
            }
            _ => {
                self.expr(node);
            }
        }
    }

    // ---------------------------------------------------------------- assignment

    fn assignment(&mut self, node: Node) -> Var {
        let left = node.child_by_field_name("left");
        let annot = node
            .child_by_field_name("type")
            .and_then(|t| reduce_type_name(self.text(t)));
        let in_class = self.in_class_body();
        let v = match node.child_by_field_name("right") {
            Some(r) => self.expr(r),
            None => {
                // `email: str` in a class body declares a field; elsewhere it binds nothing.
                if in_class && let Some(l) = left.filter(|l| l.kind() == "identifier") {
                    self.class_field(l);
                    if let (Some(t), Some(&c)) =
                        (node.child_by_field_name("type"), self.b.class_stack.last())
                    {
                        let this = self.b.ir.classes[c as usize].this;
                        let v = self.b.temp(self.b.pos(node));
                        self.type_ref(v, t);
                        let name = self.text(l).to_string();
                        self.b.store_member(node, this, Some(name), v);
                    }
                }
                return self.b.lit(node, None);
            }
        };
        if let Some(l) = left {
            if in_class && l.kind() == "identifier" {
                self.class_field(l);
                // Class attributes are read back through instances: `self.x`.
                if let Some(&c) = self.b.class_stack.last() {
                    let this = self.b.ir.classes[c as usize].this;
                    let name = self.text(l).to_string();
                    self.b.store_member(node, this, Some(name), v);
                }
            }
            self.assign_to(l, v);
            if let (Some(a), "identifier") = (annot, l.kind())
                && let Some(var) = self.b.lookup(self.text(l))
            {
                self.b.ir.annots.push((var, a));
            }
            if let (Some(t), "identifier") = (node.child_by_field_name("type"), l.kind())
                && let Some(var) = self.b.lookup(self.text(l))
            {
                self.type_ref(var, t);
            }
        }
        v
    }

    fn class_field(&mut self, name: Node) {
        let n = self.text(name);
        if n.starts_with('_') || matches!(n, "objects" | "Meta" | "model_config" | "Config") {
            return;
        }
        let Some(&c) = self.b.class_stack.last() else {
            return;
        };
        let class_name = self.b.ir.classes[c as usize].name.clone();
        let pos = self.b.pos(name);
        if let Some(t) = self
            .b
            .ir
            .types
            .iter_mut()
            .rev()
            .find(|t| t.name == class_name)
            && !t.fields.iter().any(|(f, _)| f == n)
        {
            t.fields.push((n.to_string(), pos));
        }
    }

    fn assign_to(&mut self, left: Node, v: Var) {
        match left.kind() {
            "identifier" => {
                let name = self.text(left);
                let pos = self.b.pos(left);
                let is_global = self.globals.last().is_some_and(|g| g.contains(name));
                let var = if is_global {
                    self.b.declare_module(name, pos)
                } else {
                    match self.b.lookup(name) {
                        Some(var) => var,
                        None => {
                            let function_scoped = !self.in_class_body();
                            self.b.declare(name, pos, function_scoped)
                        }
                    }
                };
                self.b.copy(left, var, v);
            }
            "attribute" => {
                let Some(obj) = left.child_by_field_name("object") else {
                    return;
                };
                let o = self.expr(obj);
                let field = left
                    .child_by_field_name("attribute")
                    .map(|a| self.text(a).to_string());
                self.b.store_member(left, o, field, v);
            }
            "subscript" => {
                let Some(obj) = left.child_by_field_name("value") else {
                    return;
                };
                let o = self.expr(obj);
                match left.child_by_field_name("subscript") {
                    Some(s) if s.kind() == "string" && !has_interpolation(s) => {
                        let field = Some(string_content(s, self.b.src));
                        self.b.emit(
                            left,
                            StmtKind::Store {
                                obj: o,
                                field,
                                src: v,
                            },
                        );
                    }
                    Some(s) if s.kind() == "integer" => self.b.copy(left, o, v),
                    Some(s) => {
                        self.expr(s);
                        self.b.emit(
                            left,
                            StmtKind::Store {
                                obj: o,
                                field: None,
                                src: v,
                            },
                        );
                    }
                    None => {}
                }
            }
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "as_pattern_target"
            | "tuple"
            | "list"
            | "parenthesized_expression" => {
                for c in named_children(left) {
                    self.assign_to(c, v);
                }
            }
            "list_splat_pattern" | "list_splat" => {
                if let Some(c) = left.named_child(0) {
                    self.assign_to(c, v);
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- expressions

    fn expr(&mut self, node: Node) -> Var {
        match node.kind() {
            "identifier" => {
                let name = self.text(node);
                self.b.resolve(name, node)
            }
            "attribute" => {
                let obj = match node.child_by_field_name("object") {
                    Some(o) => self.expr(o),
                    None => return self.b.lit(node, None),
                };
                let field = node
                    .child_by_field_name("attribute")
                    .map(|a| self.text(a).to_string());
                self.b.load_member(node, obj, field)
            }
            "subscript" => {
                let obj = match node.child_by_field_name("value") {
                    Some(o) => self.expr(o),
                    None => return self.b.lit(node, None),
                };
                match node.child_by_field_name("subscript") {
                    Some(s) if s.kind() == "string" && !has_interpolation(s) => {
                        let f = string_content(s, self.b.src);
                        self.b.load(node, obj, Some(f))
                    }
                    Some(s) if matches!(s.kind(), "integer" | "slice" | "unary_operator") => {
                        self.b.union(node, &[obj])
                    }
                    Some(s) => {
                        self.expr(s);
                        self.b.load(node, obj, None)
                    }
                    None => self.b.load(node, obj, None),
                }
            }
            "call" => self.call(node),
            "assignment" => self.assignment(node),
            "augmented_assignment" => {
                let left = node.child_by_field_name("left");
                let l = left.map(|l| self.expr(l));
                let r = node.child_by_field_name("right").map(|r| self.expr(r));
                let parts = l.into_iter().chain(r).map(Part::Var).collect();
                let v = self.b.concat(node, parts);
                if let Some(l) = left {
                    self.assign_to(l, v);
                }
                v
            }
            "named_expression" => {
                let v = match node.child_by_field_name("value") {
                    Some(e) => self.expr(e),
                    None => self.b.lit(node, None),
                };
                if let Some(n) = node.child_by_field_name("name") {
                    self.assign_to(n, v);
                }
                v
            }
            "string" => self.string(node),
            "concatenated_string" => {
                let mut parts = Vec::new();
                for c in named_children(node) {
                    let v = self.string(c);
                    parts.push(Part::Var(v));
                }
                self.b.concat(node, parts)
            }
            "integer" | "float" | "true" | "false" | "none" | "ellipsis" | "comment" => {
                self.b.lit(node, None)
            }
            "binary_operator" => {
                let op = node
                    .child_by_field_name("operator")
                    .map(|o| self.text(o))
                    .unwrap_or("");
                let l = node.child_by_field_name("left").map(|l| self.expr(l));
                let r = node.child_by_field_name("right").map(|r| self.expr(r));
                match op {
                    "+" | "%" => {
                        let parts = l.into_iter().chain(r).map(Part::Var).collect();
                        self.b.concat(node, parts)
                    }
                    _ => self.b.lit(node, None),
                }
            }
            "boolean_operator" => {
                let vars: Vec<Var> = ["left", "right"]
                    .iter()
                    .filter_map(|f| node.child_by_field_name(f))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|c| self.expr(c))
                    .collect();
                self.b.union(node, &vars)
            }
            "not_operator" | "comparison_operator" | "unary_operator" => {
                for c in named_children(node) {
                    self.expr(c);
                }
                self.b.lit(node, None)
            }
            "conditional_expression" => {
                let children = named_children(node);
                let mut vars = Vec::new();
                for (i, c) in children.into_iter().enumerate() {
                    let v = self.expr(c);
                    if i != 1 {
                        vars.push(v);
                    }
                }
                self.b.union(node, &vars)
            }
            "list" | "tuple" | "set" | "expression_list" | "pattern_list" => {
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::Lit { dst, value: None });
                for c in named_children(node) {
                    let v = self.expr(c);
                    self.b.copy(c, dst, v);
                }
                dst
            }
            "dictionary" => {
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::Lit { dst, value: None });
                for c in named_children(node) {
                    match c.kind() {
                        "pair" => {
                            let key = c.child_by_field_name("key");
                            let field = match key {
                                Some(k) if k.kind() == "string" && !has_interpolation(k) => {
                                    Some(string_content(k, self.b.src))
                                }
                                Some(k) => {
                                    self.expr(k);
                                    None
                                }
                                None => None,
                            };
                            if let Some(value) = c.child_by_field_name("value") {
                                let v = self.expr(value);
                                self.b.emit(
                                    c,
                                    StmtKind::Store {
                                        obj: dst,
                                        field,
                                        src: v,
                                    },
                                );
                            }
                        }
                        "dictionary_splat" => {
                            if let Some(inner) = c.named_child(0) {
                                let v = self.expr(inner);
                                self.b.copy(c, dst, v);
                            }
                        }
                        _ => {}
                    }
                }
                dst
            }
            "list_comprehension"
            | "set_comprehension"
            | "generator_expression"
            | "dictionary_comprehension" => {
                self.b.push_block();
                for c in named_children(node) {
                    match c.kind() {
                        "for_in_clause" => {
                            let r = c.child_by_field_name("right").map(|r| self.expr(r));
                            if let (Some(l), Some(r)) = (c.child_by_field_name("left"), r) {
                                self.comprehension_bind(l, r);
                            }
                        }
                        "if_clause" => {
                            for e in named_children(c) {
                                self.expr(e);
                            }
                        }
                        _ => {}
                    }
                }
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::Lit { dst, value: None });
                if let Some(body) = node.child_by_field_name("body") {
                    let v = if body.kind() == "pair" {
                        let vs: Vec<Var> = named_children(body)
                            .into_iter()
                            .map(|e| self.expr(e))
                            .collect();
                        self.b.union(body, &vs)
                    } else {
                        self.expr(body)
                    };
                    self.b.copy(body, dst, v);
                }
                self.b.pop_scope();
                dst
            }
            "lambda" => {
                let f = self.b.begin_function("<lambda>".into(), node, None);
                self.globals.push(HashSet::new());
                if let Some(params) = node.child_by_field_name("parameters") {
                    for p in named_children(params) {
                        self.param(p, false);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    let v = self.expr(body);
                    self.b.emit(body, StmtKind::Return { src: v });
                }
                self.globals.pop();
                self.b.end_function(f);
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::FuncRef { dst, func: f });
                dst
            }
            "await"
            | "parenthesized_expression"
            | "list_splat"
            | "dictionary_splat"
            | "keyword_argument"
            | "type_conversion"
            | "yield" => {
                let mut last = None;
                for c in named_children(node) {
                    last = Some(self.expr(c));
                }
                last.unwrap_or_else(|| self.b.lit(node, None))
            }
            _ => {
                let mut vars = Vec::new();
                for c in named_children(node) {
                    vars.push(self.expr(c));
                }
                self.b.union(node, &vars)
            }
        }
    }

    fn comprehension_bind(&mut self, left: Node, src: Var) {
        match left.kind() {
            "identifier" => {
                let pos = self.b.pos(left);
                let var = self.b.declare(self.text(left), pos, false);
                self.b.copy(left, var, src);
            }
            _ => {
                for c in named_children(left) {
                    self.comprehension_bind(c, src);
                }
            }
        }
    }

    fn string(&mut self, node: Node) -> Var {
        if node.kind() != "string" {
            return self.expr(node);
        }
        if !has_interpolation(node) {
            let value = string_content(node, self.b.src);
            return self.b.lit(node, Some(value));
        }
        let mut parts = Vec::new();
        for c in named_children(node) {
            match c.kind() {
                "string_content" | "escape_sequence" => {
                    parts.push(Part::Lit(self.text(c).to_string()))
                }
                "interpolation" => {
                    if let Some(e) = c.child_by_field_name("expression") {
                        let v = self.expr(e);
                        parts.push(Part::Var(v));
                    }
                }
                _ => {}
            }
        }
        self.b.concat(node, parts)
    }

    fn call(&mut self, node: Node) -> Var {
        let func = node.child_by_field_name("function");
        let args_node = node.child_by_field_name("arguments");
        // `getattr(obj, "email")` is a field read.
        if let (Some(f), Some(a)) = (func, args_node)
            && f.kind() == "identifier"
            && self.text(f) == "getattr"
            && self.b.lookup("getattr").is_none()
        {
            let items = named_children(a);
            if items.len() >= 2 && items[1].kind() == "string" && !has_interpolation(items[1]) {
                let obj = self.expr(items[0]);
                let key = string_content(items[1], self.b.src);
                return self.b.load(node, obj, Some(key));
            }
        }
        let (callee, method) = match func {
            Some(f) if f.kind() == "attribute" => {
                let recv = f.child_by_field_name("object").map(|o| self.expr(o));
                let name = f
                    .child_by_field_name("attribute")
                    .map(|a| self.text(a).to_string())
                    .unwrap_or_default();
                match recv {
                    Some(recv) => (
                        Callee::Method {
                            recv,
                            name: name.clone(),
                        },
                        Some((recv, name)),
                    ),
                    None => (Callee::Dynamic, None),
                }
            }
            Some(f) => (Callee::Value(self.expr(f)), None),
            None => (Callee::Dynamic, None),
        };
        let mut args = Vec::new();
        let mut first_literal = None;
        if let Some(a) = args_node {
            if a.kind() == "generator_expression" {
                let v = self.expr(a);
                args.push(Arg {
                    var: v,
                    kind: ArgKind::Positional,
                });
            } else {
                for (i, c) in named_children(a).into_iter().enumerate() {
                    match c.kind() {
                        "keyword_argument" => {
                            let name = c
                                .child_by_field_name("name")
                                .map(|n| self.text(n).to_string())
                                .unwrap_or_default();
                            if let Some(value) = c.child_by_field_name("value") {
                                let v = self.expr(value);
                                args.push(Arg {
                                    var: v,
                                    kind: ArgKind::Keyword(name),
                                });
                            }
                        }
                        "list_splat" => {
                            if let Some(inner) = c.named_child(0) {
                                let v = self.expr(inner);
                                args.push(Arg {
                                    var: v,
                                    kind: ArgKind::Spread,
                                });
                            }
                        }
                        "dictionary_splat" => {
                            if let Some(inner) = c.named_child(0) {
                                let v = self.expr(inner);
                                args.push(Arg {
                                    var: v,
                                    kind: ArgKind::KwSpread,
                                });
                            }
                        }
                        "comment" => {}
                        _ => {
                            if i == 0 && c.kind() == "string" && !has_interpolation(c) {
                                first_literal = Some(string_content(c, self.b.src));
                            }
                            let v = self.expr(c);
                            args.push(Arg {
                                var: v,
                                kind: ArgKind::Positional,
                            });
                        }
                    }
                }
            }
        }
        let dst = self.b.temp(self.b.pos(node));
        self.b.emit(
            node,
            StmtKind::Call {
                dst,
                callee,
                args,
                is_new: false,
            },
        );
        // `request.form.get("email")`, `data.get("email")`: a named entry read.
        if let (Some((recv, name)), Some(key)) = (method, first_literal)
            && matches!(name.as_str(), "get" | "getlist" | "pop" | "setdefault")
        {
            self.b.emit(
                node,
                StmtKind::Load {
                    dst,
                    obj: recv,
                    field: Some(key),
                },
            );
        }
        dst
    }

    // ---------------------------------------------------------------- definitions

    fn def(&mut self, node: Node, decorators: Vec<(Var, String)>) -> FuncIdx {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        let class = if self.in_class_body() {
            self.b.class_stack.last().copied()
        } else {
            None
        };
        let is_static = decorators.iter().any(|(_, t)| t == "staticmethod");
        let display = match class {
            Some(c) => format!("{}.{}", self.b.ir.classes[c as usize].name, name),
            None => name.clone(),
        };
        // The function's own name binds in the enclosing scope.
        let pos = self.b.pos(node);
        let binding = if class.is_some() {
            None
        } else {
            Some(match self.b.lookup(&name) {
                Some(v) => v,
                None => self.b.declare(&name, pos, true),
            })
        };
        let f = self.b.begin_function(display, node, class);
        self.b.ir.funcs[f as usize].decorators = decorators.iter().map(|(v, _)| *v).collect();
        let outer_ctor = self.b.ctor_this.take();
        let is_ctor = name == "__init__" && !is_static;
        if let (true, Some(c)) = (is_ctor, class) {
            let pos = self.b.pos(node);
            self.b.begin_ctor(c, pos);
        }
        // Pre-scan the body for locals and `global` declarations.
        let mut names = Vec::new();
        let mut globals = HashSet::new();
        if let Some(body) = node.child_by_field_name("body") {
            collect_targets(body, self.b.src, &mut names, &mut globals);
        }
        self.globals.push(globals.clone());
        if let Some(params) = node.child_by_field_name("parameters") {
            let bound_self = class.is_some() && !is_static;
            for (i, p) in named_children(params).into_iter().enumerate() {
                self.param(p, bound_self && i == 0);
            }
            if bound_self && !self.b.ir.funcs[f as usize].params.is_empty() {
                self.b.ir.funcs[f as usize].bound_self = true;
            }
        }
        for (n, nnode) in names {
            if !globals.contains(&n)
                && self
                    .b
                    .scopes
                    .last()
                    .is_some_and(|s| !s.names.contains_key(&n))
            {
                let p = self.b.pos(nnode);
                self.b.declare(&n, p, true);
            }
        }
        if let Some(rt) = node.child_by_field_name("return_type") {
            self.b.ir.funcs[f as usize].ret_annot = reduce_type_name(self.text(rt));
            let ret = self.b.ret_var();
            self.type_ref(ret, rt);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.stmts(body);
        }
        self.globals.pop();
        if is_ctor && class.is_some() {
            let pos = self.b.pos(node);
            self.b.end_ctor(pos);
        }
        self.b.ctor_this = outer_ctor;
        self.b.end_function(f);
        match (class, binding) {
            (Some(c), _) => {
                self.b.ir.classes[c as usize].methods.push((name, f));
            }
            (None, Some(var)) => {
                self.b.emit(node, StmtKind::FuncRef { dst: var, func: f });
            }
            _ => {}
        }
        f
    }

    fn param(&mut self, p: Node, is_self: bool) {
        let (name_node, annot, default, rest, kwrest) = match p.kind() {
            "identifier" => (Some(p), None, None, false, false),
            "typed_parameter" => {
                let inner = named_children(p).into_iter().find(|c| c.kind() != "type");
                let (n, rest, kwrest) = match inner {
                    Some(i) if i.kind() == "list_splat_pattern" => (i.named_child(0), true, false),
                    Some(i) if i.kind() == "dictionary_splat_pattern" => {
                        (i.named_child(0), false, true)
                    }
                    other => (other, false, false),
                };
                (
                    n,
                    p.child_by_field_name("type")
                        .and_then(|t| reduce_type_name(self.text(t))),
                    None,
                    rest,
                    kwrest,
                )
            }
            "default_parameter" => (
                p.child_by_field_name("name"),
                None,
                p.child_by_field_name("value"),
                false,
                false,
            ),
            "typed_default_parameter" => (
                p.child_by_field_name("name"),
                p.child_by_field_name("type")
                    .and_then(|t| reduce_type_name(self.text(t))),
                p.child_by_field_name("value"),
                false,
                false,
            ),
            "list_splat_pattern" => (p.named_child(0), None, None, true, false),
            "dictionary_splat_pattern" => (p.named_child(0), None, None, false, true),
            _ => return,
        };
        let Some(name_node) = name_node.filter(|n| n.kind() == "identifier") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let var = if is_self {
            let c = *self.b.class_stack.last().expect("method inside a class");
            // In `__init__`, `self` is the constructor's own instance.
            let this = match self.b.ctor_this {
                Some((_, t)) => t,
                None => self.b.ir.classes[c as usize].this,
            };
            self.b
                .scopes
                .last_mut()
                .expect("scope")
                .names
                .insert(name.clone(), this);
            this
        } else {
            let pos = self.b.pos(name_node);
            self.b.declare_param(&name, pos)
        };
        let f = self.b.func as usize;
        self.b.ir.funcs[f].params.push(Param {
            var,
            name,
            rest,
            kwrest,
            annot: annot.clone(),
        });
        if let (Some(a), false) = (annot, is_self) {
            self.b.ir.annots.push((var, a));
        }
        if let (Some(t), false) = (p.child_by_field_name("type"), is_self) {
            self.type_ref(var, t);
        }
        if let Some(d) = default {
            let dv = self.expr(d);
            self.b.copy(d, var, dv);
        }
    }

    /// Record that `var` is declared with the type an annotation names.
    fn type_ref(&mut self, var: Var, annotation: Node) {
        if let Some(ty) = self.type_expr(annotation) {
            self.b.emit(annotation, StmtKind::TypeRef { dst: var, ty });
        }
    }

    /// The value an annotation names, if it names one in scope (`Session`, `logging.Logger`,
    /// `Optional[Mailer]` → `Mailer`, `"Mailer"`). Unknown names create nothing.
    fn type_expr(&mut self, node: Node) -> Option<Var> {
        const WRAPPERS: &[&str] = &[
            "Optional",
            "Annotated",
            "List",
            "list",
            "Sequence",
            "Iterable",
            "Iterator",
            "Awaitable",
            "Type",
            "type",
            "Union",
            "Final",
            "ClassVar",
        ];
        match node.kind() {
            "type" => {
                let inner = named_children(node).into_iter().next()?;
                self.type_expr(inner)
            }
            "identifier" => self.b.lookup(self.text(node)),
            "attribute" => {
                let base = self.type_expr(node.child_by_field_name("object")?)?;
                let field = self
                    .text(node.child_by_field_name("attribute")?)
                    .to_string();
                Some(self.b.load(node, base, Some(field)))
            }
            "generic_type" | "subscript" => {
                let parts = named_children(node);
                let head = node
                    .child_by_field_name("value")
                    .or_else(|| parts.first().copied())?;
                let last = self.text(head).rsplit('.').next().unwrap_or("").to_string();
                if WRAPPERS.contains(&last.as_str()) {
                    let params = parts.iter().find(|c| c.kind() == "type_parameter").copied();
                    let first = match params {
                        Some(tp) => named_children(tp)
                            .into_iter()
                            .find(|c| self.text(*c) != "None")?,
                        None => node.child_by_field_name("subscript")?,
                    };
                    self.type_expr(first)
                } else {
                    self.type_expr(head)
                }
            }
            "union_type" | "binary_operator" => {
                let src = self.b.src;
                named_children(node)
                    .into_iter()
                    .filter(|c| &src[c.byte_range()] != "None")
                    .find_map(|c| self.type_expr(c))
            }
            "string" => {
                let name = string_content(node, self.b.src);
                let first = name.split(['.', '[']).next().unwrap_or("").to_string();
                self.b.lookup(&first)
            }
            _ => None,
        }
    }

    fn class(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        if let Some(sup) = node.child_by_field_name("superclasses") {
            for c in named_children(sup) {
                self.expr(c);
            }
        }
        let c = self.b.new_class(&name, node);
        let pos = self.b.pos(node);
        let var = match self.b.lookup(&name) {
            Some(v) => v,
            None => self.b.declare(&name, pos, true),
        };
        self.b.emit(node, StmtKind::ClassRef { dst: var, class: c });
        self.b.ir.types.push(TypeDecl {
            name: name.clone(),
            fields: Vec::new(),
            pos,
        });
        self.b.class_stack.push(c);
        self.b.push_class_scope();
        if let Some(body) = node.child_by_field_name("body") {
            self.stmts(body);
        }
        self.b.pop_scope();
        self.b.finish_class(c);
        self.b.class_stack.pop();
        if self
            .b
            .ir
            .types
            .last()
            .is_some_and(|t| t.name == name && t.fields.is_empty())
        {
            self.b.ir.types.pop();
        }
    }

    // ---------------------------------------------------------------- imports

    fn push_import(&mut self, node: Node, var: Var, module: String, kind: ImportKind) {
        let pos = self.b.pos(node);
        let idx = self.b.ir.imports.len() as u32;
        self.b.ir.imports.push(ImportIr {
            var,
            module,
            kind,
            pos,
        });
        self.b.emit(node, StmtKind::Import { import: idx });
    }

    fn bind_name(&mut self, name: Node) -> Var {
        let n = self.text(name);
        let pos = self.b.pos(name);
        let is_global = self.globals.last().is_some_and(|g| g.contains(n));
        if is_global {
            return self.b.declare_module(n, pos);
        }
        match self.b.lookup(n) {
            Some(v) => v,
            None => self.b.declare(n, pos, true),
        }
    }

    fn import(&mut self, node: Node) {
        for n in named_children(node) {
            match n.kind() {
                "dotted_name" => {
                    // `import a.b.c` binds `a` to the top-level package.
                    let Some(first) = n.named_child(0) else {
                        continue;
                    };
                    let module = self.text(first).to_string();
                    let var = self.bind_name(first);
                    self.push_import(n, var, module, ImportKind::Namespace);
                }
                "aliased_import" => {
                    let (Some(name), Some(alias)) = (
                        n.child_by_field_name("name"),
                        n.child_by_field_name("alias"),
                    ) else {
                        continue;
                    };
                    let module = self.text(name).to_string();
                    let var = self.bind_name(alias);
                    self.push_import(n, var, module, ImportKind::Namespace);
                }
                _ => {}
            }
        }
    }

    fn import_from(&mut self, node: Node) {
        let Some(m) = node.child_by_field_name("module_name") else {
            return;
        };
        let module = self.text(m).replace(char::is_whitespace, "");
        for n in named_children(node) {
            if n == m {
                continue;
            }
            match n.kind() {
                "dotted_name" => {
                    let imported = self.text(n).to_string();
                    let Some(last) =
                        n.named_child((n.named_child_count() as u32).saturating_sub(1))
                    else {
                        continue;
                    };
                    let var = self.bind_name(last);
                    self.push_import(n, var, module.clone(), ImportKind::Named(imported));
                }
                "aliased_import" => {
                    let (Some(name), Some(alias)) = (
                        n.child_by_field_name("name"),
                        n.child_by_field_name("alias"),
                    ) else {
                        continue;
                    };
                    let imported = self.text(name).to_string();
                    let var = self.bind_name(alias);
                    self.push_import(n, var, module.clone(), ImportKind::Named(imported));
                }
                "wildcard_import" => self.b.ir.reexports.push(module.clone()),
                _ => {}
            }
        }
    }
}

fn has_interpolation(s: Node) -> bool {
    named_children(s)
        .iter()
        .any(|c| c.kind() == "interpolation")
}

fn string_content(s: Node, src: &str) -> String {
    let parts: Vec<&str> = named_children(s)
        .into_iter()
        .filter(|c| c.kind() == "string_content")
        .map(|c| &src[c.byte_range()])
        .collect();
    if parts.is_empty() && !named_children(s).iter().any(|c| c.kind() == "string_start") {
        return unquote(&src[s.byte_range()]);
    }
    parts.concat()
}
