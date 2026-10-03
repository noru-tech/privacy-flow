//! JavaScript and TypeScript lowering (tree-sitter-javascript, tree-sitter-typescript).
//!
//! TypeScript and TSX use the TypeScript grammars; `.js`, `.jsx`, `.mjs` and `.cjs` use the
//! JavaScript grammar, which parses JSX. Both share node kinds for everything the IR cares
//! about; the TypeScript additions (annotations, interfaces, parameter properties, `as`) are
//! handled where they appear.

use tree_sitter::{Node, Parser};

use super::{Builder, named_children, unquote};
use crate::ir::*;

pub fn lower(path: &str, lang: Lang, src: &str) -> FileIr {
    let mut parser = Parser::new();
    let language: tree_sitter::Language = match lang {
        Lang::Typescript if path.ends_with(".tsx") => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => tree_sitter_javascript::LANGUAGE.into(),
    };
    parser
        .set_language(&language)
        .expect("grammar version matches tree-sitter");
    let mut js = Js {
        b: Builder::new(path, lang, src),
    };
    match parser.parse(src, None) {
        Some(tree) => {
            let root = tree.root_node();
            js.b.record_parse_errors(root);
            js.hoist(root);
            for child in named_children(root) {
                js.stmt(child);
            }
        }
        None => {
            js.b.ir.notes.push(LowerNote {
                kind: NoteKind::ParseError,
                pos: Pos { line: 1, column: 1 },
                detail: "parser returned no tree".into(),
            });
        }
    }
    js.b.finish()
}

struct Js<'s> {
    b: Builder<'s>,
}

fn is_type_node(kind: &str) -> bool {
    kind.ends_with("type")
        || kind.ends_with("_type_annotation")
        || matches!(
            kind,
            "type_annotation"
                | "type_arguments"
                | "type_parameters"
                | "type_identifier"
                | "predefined_type"
                | "asserts_annotation"
                | "type_predicate_annotation"
                | "omitting_type_annotation"
                | "opting_type_annotation"
        )
}

impl<'s> Js<'s> {
    fn text(&self, n: Node) -> &'s str {
        self.b.text(n)
    }

    // ---------------------------------------------------------------- declarations and hoisting

    /// Pre-declare the names a block binds, so uses before the declaration resolve to it.
    fn hoist(&mut self, block: Node) {
        for c in named_children(block) {
            self.hoist_one(c);
        }
    }

    fn hoist_one(&mut self, c: Node) {
        match c.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration" => {
                if let Some(name) = c.child_by_field_name("name") {
                    let pos = self.b.pos(name);
                    self.b.declare(self.text(name), pos, false);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let function_scoped = c.kind() == "variable_declaration";
                for d in named_children(c) {
                    if let Some(name) = d.child_by_field_name("name") {
                        let mut names = Vec::new();
                        pattern_names(name, self.b.src, &mut names);
                        for (n, node) in names {
                            let pos = self.b.pos(node);
                            self.b.declare(&n, pos, function_scoped);
                        }
                    }
                }
            }
            "export_statement" => {
                if let Some(d) = c.child_by_field_name("declaration") {
                    self.hoist_one(d);
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- statements

    fn block(&mut self, node: Node) {
        self.b.push_block();
        self.hoist(node);
        for c in named_children(node) {
            self.stmt(c);
        }
        self.b.pop_scope();
    }

    fn stmt(&mut self, node: Node) {
        match node.kind() {
            "expression_statement" => {
                if let Some(e) = node.named_child(0) {
                    if e.kind() == "string" && unquote(self.text(e)) == "use server" {
                        self.b.note(
                            NoteKind::UnsupportedConstruct,
                            e,
                            "'use server': Next.js server actions are callable from the client, and their arguments are not modelled as request input".into(),
                        );
                    }
                    self.expr(e);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let function_scoped = node.kind() == "variable_declaration";
                for d in named_children(node) {
                    if d.kind() == "variable_declarator" {
                        self.declarator(d, function_scoped);
                    }
                }
            }
            "function_declaration" | "generator_function_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| self.text(n).to_string());
                let f = self.function(node, name.clone(), None);
                let pos = self.b.pos(node);
                let var = self
                    .b
                    .declare(name.as_deref().unwrap_or("<anonymous>"), pos, false);
                self.b.emit(node, StmtKind::FuncRef { dst: var, func: f });
            }
            "class_declaration" | "abstract_class_declaration" => {
                self.class(node, true);
            }
            "interface_declaration" => self.interface(node),
            "type_alias_declaration" => self.type_alias(node),
            "export_statement" => self.export(node),
            "import_statement" => self.import(node),
            "return_statement" => {
                if let Some(e) = node.named_child(0) {
                    let v = self.expr(e);
                    self.b.emit(node, StmtKind::Return { src: v });
                }
            }
            "statement_block" => self.block(node),
            "if_statement" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                if let Some(c) = node.child_by_field_name("consequence") {
                    self.stmt(c);
                }
                if let Some(a) = node.child_by_field_name("alternative") {
                    for c in named_children(a) {
                        self.stmt(c);
                    }
                }
            }
            "for_statement" => {
                self.b.push_block();
                for field in ["initializer", "condition", "increment"] {
                    if let Some(c) = node.child_by_field_name(field) {
                        self.stmt_or_expr(c);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.stmt(body);
                }
                self.b.pop_scope();
            }
            "for_in_statement" => {
                self.b.push_block();
                let right = node.child_by_field_name("right").map(|r| self.expr(r));
                let mut is_of = false;
                let mut declares = false;
                let mut cursor = node.walk();
                for c in node.children(&mut cursor) {
                    match self.text(c) {
                        "of" => is_of = true,
                        "const" | "let" | "var" => declares = true,
                        _ => {}
                    }
                }
                if let Some(left) = node.child_by_field_name("left") {
                    // `for (x of xs)` binds elements; `for (k in o)` binds key names, which
                    // are not the object's values.
                    let src = match (is_of, right) {
                        (true, Some(r)) => r,
                        _ => self.b.lit(left, None),
                    };
                    if declares {
                        self.bind(left, src, Some(false));
                    } else {
                        self.assign_to(left, src);
                    }
                    if is_of {
                        self.elements_of(left, src);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.stmt(body);
                }
                self.b.pop_scope();
            }
            "while_statement" | "do_statement" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.stmt(body);
                }
            }
            "try_statement" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.stmt(body);
                }
                if let Some(h) = node.child_by_field_name("handler") {
                    self.b.push_block();
                    if let Some(p) = h.child_by_field_name("parameter") {
                        let v = self.b.lit(p, None);
                        self.bind(p, v, Some(false));
                    }
                    if let Some(body) = h.child_by_field_name("body") {
                        self.stmt(body);
                    }
                    self.b.pop_scope();
                }
                if let Some(f) = node.child_by_field_name("finalizer") {
                    for c in named_children(f) {
                        self.stmt(c);
                    }
                }
            }
            "switch_statement" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.expr(v);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.b.push_block();
                    for case in named_children(body) {
                        if let Some(v) = case.child_by_field_name("value") {
                            self.expr(v);
                        }
                        for (i, c) in named_children(case).into_iter().enumerate() {
                            if i == 0 && case.kind() == "switch_case" {
                                continue; // the case value, lowered above
                            }
                            self.stmt(c);
                        }
                    }
                    self.b.pop_scope();
                }
            }
            "labeled_statement" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.stmt(body);
                }
            }
            "throw_statement" => {
                if let Some(e) = node.named_child(0) {
                    self.expr(e);
                }
            }
            "empty_statement" | "break_statement" | "continue_statement" | "debugger_statement"
            | "comment" | "hash_bang_line" | "enum_declaration" | "import_alias" => {}
            "ambient_declaration" | "module" | "internal_module" | "expression_statement_list" => {
                for c in named_children(node) {
                    self.stmt(c);
                }
            }
            k if is_type_node(k) => {}
            _ => {
                self.expr(node);
            }
        }
    }

    fn stmt_or_expr(&mut self, node: Node) {
        match node.kind() {
            "lexical_declaration"
            | "variable_declaration"
            | "expression_statement"
            | "empty_statement" => self.stmt(node),
            _ => {
                self.expr(node);
            }
        }
    }

    fn declarator(&mut self, d: Node, function_scoped: bool) {
        let Some(name) = d.child_by_field_name("name") else {
            return;
        };
        let hint = (name.kind() == "identifier").then(|| self.text(name).to_string());
        let value = match d.child_by_field_name("value") {
            Some(v) => self.expr_named(v, hint.clone()),
            None => self.b.lit(d, None),
        };
        let annot = d
            .child_by_field_name("type")
            .and_then(|t| reduce_type_name(self.text(t)));
        let bound = self.bind(name, value, Some(function_scoped));
        if let (Some(t), Some(v)) = (d.child_by_field_name("type"), bound) {
            self.type_ref(v, t);
        }
        if let (Some(a), Some(v)) = (annot, bound) {
            self.b.ir.annots.push((v, a));
        }
    }

    // ---------------------------------------------------------------- patterns

    /// Bind a pattern to `src`. `declare` is `Some(function_scoped)` for declarations and
    /// `None` for assignments. Returns the variable for a plain identifier pattern.
    fn bind(&mut self, pattern: Node, src: Var, declare: Option<bool>) -> Option<Var> {
        match pattern.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                let name = self.text(pattern);
                let pos = self.b.pos(pattern);
                let var = match declare {
                    Some(fs) => self.b.declare(name, pos, fs),
                    None => match self.b.lookup(name) {
                        Some(v) => v,
                        None => self.b.declare(name, pos, true),
                    },
                };
                self.b.copy(pattern, var, src);
                Some(var)
            }
            "object_pattern" => {
                for c in named_children(pattern) {
                    match c.kind() {
                        "shorthand_property_identifier_pattern" => {
                            let v = self.b.load(c, src, Some(self.text(c).to_string()));
                            self.bind(c, v, declare);
                        }
                        "pair_pattern" => {
                            let key = c.child_by_field_name("key").and_then(|k| self.key_name(k));
                            let v = self.b.load(c, src, key);
                            if let Some(value) = c.child_by_field_name("value") {
                                self.bind(value, v, declare);
                            }
                        }
                        "object_assignment_pattern" => {
                            if let Some(left) = c.child_by_field_name("left") {
                                let key = match left.kind() {
                                    "shorthand_property_identifier_pattern" | "identifier" => {
                                        Some(self.text(left).to_string())
                                    }
                                    _ => None,
                                };
                                let v = self.b.load(c, src, key);
                                let d = match c.child_by_field_name("right") {
                                    Some(r) => self.expr(r),
                                    None => v,
                                };
                                let u = self.b.union(c, &[v, d]);
                                self.bind(left, u, declare);
                            }
                        }
                        "rest_pattern" => {
                            if let Some(inner) = c.named_child(0) {
                                self.bind(inner, src, declare);
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            "array_pattern" => {
                for c in named_children(pattern) {
                    self.bind(c, src, declare);
                }
                None
            }
            "assignment_pattern" => {
                let d = match pattern.child_by_field_name("right") {
                    Some(r) => self.expr(r),
                    None => src,
                };
                let u = self.b.union(pattern, &[src, d]);
                pattern
                    .child_by_field_name("left")
                    .and_then(|l| self.bind(l, u, declare))
            }
            "rest_pattern" => pattern
                .named_child(0)
                .and_then(|inner| self.bind(inner, src, declare)),
            _ => {
                self.assign_to(pattern, src);
                None
            }
        }
    }

    fn key_name(&mut self, key: Node) -> Option<String> {
        match key.kind() {
            "property_identifier" | "identifier" | "private_property_identifier" => {
                Some(self.text(key).trim_start_matches('#').to_string())
            }
            "string" => Some(unquote(self.text(key))),
            "number" => Some(self.text(key).to_string()),
            "computed_property_name" => {
                let inner = key.named_child(0)?;
                if inner.kind() == "string" {
                    Some(unquote(self.text(inner)))
                } else {
                    self.expr(inner);
                    None
                }
            }
            _ => None,
        }
    }

    fn assign_to(&mut self, left: Node, v: Var) {
        match left.kind() {
            "identifier" => {
                let name = self.text(left);
                let pos = self.b.pos(left);
                let var = match self.b.lookup(name) {
                    Some(var) => var,
                    // An undeclared assignment creates a global; model it as module-level.
                    None => self.b.declare_module(name, pos),
                };
                self.b.copy(left, var, v);
            }
            "member_expression" => {
                let obj = left.child_by_field_name("object");
                let prop = left.child_by_field_name("property");
                let obj_text = obj.map(|o| self.text(o)).unwrap_or("");
                let prop_name = prop.map(|p| self.text(p).trim_start_matches('#').to_string());
                // CommonJS exports.
                if self.b.lookup("module").is_none() && self.b.lookup("exports").is_none() {
                    if obj_text == "module" && prop_name.as_deref() == Some("exports") {
                        self.b.ir.exports.push(ExportIr {
                            name: "default".into(),
                            var: v,
                        });
                        return;
                    }
                    if (obj_text == "module.exports" || obj_text == "exports")
                        && let Some(name) = prop_name
                    {
                        self.b.ir.exports.push(ExportIr { name, var: v });
                        return;
                    }
                }
                let Some(obj) = obj else { return };
                let o = self.expr(obj);
                self.b.store_member(left, o, prop_name, v);
            }
            "subscript_expression" => {
                let Some(obj) = left.child_by_field_name("object") else {
                    return;
                };
                let o = self.expr(obj);
                match left.child_by_field_name("index") {
                    Some(i) if i.kind() == "string" => {
                        let field = Some(unquote(self.text(i)));
                        self.b.emit(
                            left,
                            StmtKind::Store {
                                obj: o,
                                field,
                                src: v,
                            },
                        );
                    }
                    Some(i) if i.kind() == "number" => self.b.copy(left, o, v),
                    Some(i) => {
                        self.expr(i);
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
            "object_pattern" | "array_pattern" => {
                self.bind(left, v, None);
            }
            "parenthesized_expression" | "non_null_expression" | "as_expression" => {
                if let Some(inner) = left.named_child(0) {
                    self.assign_to(inner, v);
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- expressions

    fn expr_named(&mut self, node: Node, hint: Option<String>) -> Var {
        match node.kind() {
            "arrow_function" | "function_expression" | "function" | "generator_function" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| self.text(n).to_string())
                    .or(hint);
                let f = self.function(node, name, None);
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::FuncRef { dst, func: f });
                dst
            }
            "class" => self.class(node, false),
            _ => self.expr(node),
        }
    }

    fn expr(&mut self, node: Node) -> Var {
        match node.kind() {
            "identifier" | "shorthand_property_identifier" => {
                let name = self.text(node);
                if name == "undefined" {
                    return self.b.lit(node, None);
                }
                self.b.resolve(name, node)
            }
            "this" => match (self.b.ctor_this, self.b.class_stack.last()) {
                (Some((_, t)), _) => t,
                (None, Some(&c)) => self.b.ir.classes[c as usize].this,
                _ => self.b.lit(node, None),
            },
            "super" => match self.b.class_stack.last() {
                Some(&class) => {
                    let dst = self.b.temp(self.b.pos(node));
                    self.b.emit(node, StmtKind::Super { dst, class });
                    dst
                }
                None => self.b.lit(node, None),
            },
            "member_expression" => {
                let obj = match node.child_by_field_name("object") {
                    Some(o) => self.expr(o),
                    None => return self.b.lit(node, None),
                };
                let field = node
                    .child_by_field_name("property")
                    .map(|p| self.text(p).trim_start_matches('#').to_string());
                self.b.load_member(node, obj, field)
            }
            "subscript_expression" => {
                let obj = match node.child_by_field_name("object") {
                    Some(o) => self.expr(o),
                    None => return self.b.lit(node, None),
                };
                match node.child_by_field_name("index") {
                    Some(i) if i.kind() == "string" => {
                        let f = unquote(self.text(i));
                        self.b.load(node, obj, Some(f))
                    }
                    // `xs[0]`: an element of the array is the array's content.
                    Some(i) if i.kind() == "number" => self.b.union(node, &[obj]),
                    Some(i) => {
                        self.expr(i);
                        self.b.load(node, obj, None)
                    }
                    None => self.b.load(node, obj, None),
                }
            }
            "call_expression" => self.call(node),
            "new_expression" => {
                let callee = match node.child_by_field_name("constructor") {
                    Some(c) => Callee::Value(self.expr(c)),
                    None => Callee::Dynamic,
                };
                let args = match node.child_by_field_name("arguments") {
                    Some(a) => self.args(a),
                    None => Vec::new(),
                };
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(
                    node,
                    StmtKind::Call {
                        dst,
                        callee,
                        args,
                        is_new: true,
                    },
                );
                dst
            }
            "await_expression" | "spread_element" | "yield_expression" => match node.named_child(0)
            {
                Some(inner) => self.expr(inner),
                None => self.b.lit(node, None),
            },
            "parenthesized_expression" => {
                let mut last = None;
                for c in named_children(node) {
                    if !is_type_node(c.kind()) {
                        last = Some(self.expr(c));
                    }
                }
                last.unwrap_or_else(|| self.b.lit(node, None))
            }
            "sequence_expression" => {
                let mut last = None;
                for c in named_children(node) {
                    last = Some(self.expr(c));
                }
                last.unwrap_or_else(|| self.b.lit(node, None))
            }
            "as_expression" | "satisfies_expression" | "non_null_expression" => {
                match named_children(node)
                    .into_iter()
                    .find(|c| !is_type_node(c.kind()))
                {
                    Some(inner) => self.expr(inner),
                    None => self.b.lit(node, None),
                }
            }
            "type_assertion" => {
                match named_children(node)
                    .into_iter()
                    .rev()
                    .find(|c| !is_type_node(c.kind()))
                {
                    Some(inner) => self.expr(inner),
                    None => self.b.lit(node, None),
                }
            }
            "template_string" => self.template(node),
            "string" => {
                let value = unquote(self.text(node));
                self.b.lit(node, Some(value))
            }
            "number" | "true" | "false" | "null" | "regex" | "undefined" => self.b.lit(node, None),
            "binary_expression" => {
                let op = node
                    .child_by_field_name("operator")
                    .map(|o| self.text(o))
                    .unwrap_or("");
                let l = node.child_by_field_name("left").map(|l| self.expr(l));
                let r = node.child_by_field_name("right").map(|r| self.expr(r));
                let vars: Vec<Var> = l.into_iter().chain(r).collect();
                match op {
                    "+" => {
                        let parts = vars.into_iter().map(Part::Var).collect();
                        self.b.concat(node, parts)
                    }
                    "||" | "??" | "&&" => self.b.union(node, &vars),
                    _ => self.b.lit(node, None),
                }
            }
            "unary_expression" | "update_expression" => {
                for c in named_children(node) {
                    self.expr(c);
                }
                self.b.lit(node, None)
            }
            "ternary_expression" => {
                if let Some(c) = node.child_by_field_name("condition") {
                    self.expr(c);
                }
                let mut vars = Vec::new();
                for f in ["consequence", "alternative"] {
                    if let Some(c) = node.child_by_field_name(f) {
                        vars.push(self.expr(c));
                    }
                }
                self.b.union(node, &vars)
            }
            "assignment_expression" => {
                let left = node.child_by_field_name("left");
                let hint = left
                    .filter(|l| l.kind() == "identifier")
                    .map(|l| self.text(l).to_string());
                let v = match node.child_by_field_name("right") {
                    Some(r) => self.expr_named(r, hint),
                    None => self.b.lit(node, None),
                };
                if let Some(l) = left {
                    self.assign_to(l, v);
                }
                v
            }
            "augmented_assignment_expression" => {
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
            "object" => self.object(node),
            "array" => {
                let dst = self.b.temp(self.b.pos(node));
                self.b.emit(node, StmtKind::Lit { dst, value: None });
                for c in named_children(node) {
                    let v = self.expr(c);
                    self.b.copy(c, dst, v);
                }
                dst
            }
            "arrow_function"
            | "function_expression"
            | "function"
            | "generator_function"
            | "class" => self.expr_named(node, None),
            "jsx_element" | "jsx_self_closing_element" if self.jsx_component(node).is_some() => {
                self.jsx_call(node)
            }
            "jsx_element"
            | "jsx_self_closing_element"
            | "jsx_fragment"
            | "jsx_opening_element"
            | "jsx_attribute"
            | "jsx_expression" => {
                let mut vars = Vec::new();
                for c in named_children(node) {
                    match c.kind() {
                        "jsx_expression"
                        | "jsx_element"
                        | "jsx_self_closing_element"
                        | "jsx_attribute"
                        | "jsx_opening_element"
                        | "jsx_fragment" => vars.push(self.expr(c)),
                        "jsx_text"
                        | "jsx_closing_element"
                        | "property_identifier"
                        | "identifier"
                        | "member_expression"
                        | "nested_identifier"
                        | "jsx_namespace_name"
                            if node.kind() != "jsx_expression" => {}
                        k if is_type_node(k) => {}
                        _ => vars.push(self.expr(c)),
                    }
                }
                self.b.union(node, &vars)
            }
            k if is_type_node(k) => self.b.lit(node, None),
            "comment" => self.b.lit(node, None),
            _ => {
                let mut vars = Vec::new();
                for c in named_children(node) {
                    if !is_type_node(c.kind()) {
                        vars.push(self.expr(c));
                    }
                }
                self.b.union(node, &vars)
            }
        }
    }

    /// Record that `var` is declared with the type an annotation names.
    fn type_ref(&mut self, var: Var, annotation: Node) {
        if let Some(ty) = self.type_expr(annotation) {
            self.b.emit(annotation, StmtKind::TypeRef { dst: var, ty });
        }
    }

    /// The value a type annotation names, if it names one in scope: an imported or local
    /// class or namespace (`Logger`, `Prisma.TransactionClient`, `Promise<Mailer>` → `Mailer`).
    /// Unknown names create nothing: a type is never a global.
    fn type_expr(&mut self, node: Node) -> Option<Var> {
        const WRAPPERS: &[&str] = &[
            "Promise",
            "Array",
            "ReadonlyArray",
            "Readonly",
            "Partial",
            "Required",
            "NonNullable",
            "Awaited",
        ];
        match node.kind() {
            "type_annotation"
            | "parenthesized_type"
            | "readonly_type"
            | "opting_type_annotation" => {
                let inner = named_children(node).into_iter().next()?;
                self.type_expr(inner)
            }
            "type_identifier" | "identifier" => self.b.lookup(self.text(node)),
            "nested_type_identifier" | "nested_identifier" | "member_expression" => {
                let module = node
                    .child_by_field_name("module")
                    .or_else(|| node.child_by_field_name("object"))?;
                let name = node
                    .child_by_field_name("name")
                    .or_else(|| node.child_by_field_name("property"))?;
                let base = self.type_expr(module)?;
                let field = self.text(name).to_string();
                Some(self.b.load(node, base, Some(field)))
            }
            "generic_type" => {
                let name = node.child_by_field_name("name")?;
                let last = self.text(name).rsplit('.').next().unwrap_or("");
                if WRAPPERS.contains(&last) {
                    let args = node.child_by_field_name("type_arguments")?;
                    let first = named_children(args).into_iter().next()?;
                    self.type_expr(first)
                } else {
                    self.type_expr(name)
                }
            }
            "union_type" => named_children(node)
                .into_iter()
                .filter(|c| c.kind() != "literal_type")
                .find_map(|c| self.type_expr(c)),
            "array_type" => {
                let inner = named_children(node).into_iter().next()?;
                self.type_expr(inner)
            }
            _ => None,
        }
    }

    /// The name node of a JSX element that renders a component (`<UserCard>`, `<Ui.Button>`),
    /// as opposed to a DOM element (`<div>`).
    fn jsx_component<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        let open = if node.kind() == "jsx_element" {
            node.child_by_field_name("open_tag")?
        } else {
            node
        };
        let name = open.child_by_field_name("name")?;
        let text = self.text(name);
        let is_component = match name.kind() {
            "identifier" => text.chars().next().is_some_and(|c| c.is_uppercase()),
            "member_expression" | "nested_identifier" => true,
            _ => false,
        };
        is_component.then_some(name)
    }

    /// `<Comp a={x} {...rest}>child</Comp>` is `Comp({ a: x, ...rest, children: child })`.
    fn jsx_call(&mut self, node: Node) -> Var {
        let name = self.jsx_component(node).expect("checked by the caller");
        let callee = self.expr(name);
        let open = if node.kind() == "jsx_element" {
            node.child_by_field_name("open_tag").unwrap_or(node)
        } else {
            node
        };
        let props = self.b.temp(self.b.pos(node));
        self.b.emit(
            open,
            StmtKind::Lit {
                dst: props,
                value: None,
            },
        );
        for attr in named_children(open) {
            match attr.kind() {
                "jsx_attribute" => {
                    let parts = named_children(attr);
                    let Some(key) = parts.first() else { continue };
                    let field = Some(self.text(*key).to_string());
                    let value = match parts.get(1) {
                        Some(v) if v.kind() == "jsx_expression" => match v.named_child(0) {
                            Some(e) => self.expr_named(e, field.clone()),
                            None => continue,
                        },
                        Some(v) => self.expr(*v),
                        None => self.b.lit(attr, None),
                    };
                    self.b.emit(
                        attr,
                        StmtKind::Store {
                            obj: props,
                            field,
                            src: value,
                        },
                    );
                }
                "jsx_expression" => {
                    // `{...rest}`
                    if let Some(inner) = attr.named_child(0) {
                        let v = self.expr(inner);
                        self.b.copy(attr, props, v);
                    }
                }
                _ => {}
            }
        }
        if node.kind() == "jsx_element" {
            let mut children = Vec::new();
            for c in named_children(node) {
                match c.kind() {
                    "jsx_expression"
                    | "jsx_element"
                    | "jsx_self_closing_element"
                    | "jsx_fragment" => children.push(self.expr(c)),
                    _ => {}
                }
            }
            if !children.is_empty() {
                let v = self.b.union(node, &children);
                self.b.emit(
                    node,
                    StmtKind::Store {
                        obj: props,
                        field: Some("children".into()),
                        src: v,
                    },
                );
            }
        }
        let dst = self.b.temp(self.b.pos(node));
        self.b.emit(
            open,
            StmtKind::Call {
                dst,
                callee: Callee::Value(callee),
                args: vec![Arg {
                    var: props,
                    kind: ArgKind::Positional,
                }],
                is_new: false,
            },
        );
        dst
    }

    fn template(&mut self, node: Node) -> Var {
        let mut parts = Vec::new();
        for c in named_children(node) {
            match c.kind() {
                "string_fragment" | "escape_sequence" => {
                    parts.push(Part::Lit(self.text(c).to_string()))
                }
                "template_substitution" => {
                    let mut last = None;
                    for e in named_children(c) {
                        last = Some(self.expr(e));
                    }
                    if let Some(v) = last {
                        parts.push(Part::Var(v));
                    }
                }
                _ => {}
            }
        }
        if parts.iter().all(|p| matches!(p, Part::Lit(_))) {
            let s: String = parts
                .iter()
                .map(|p| match p {
                    Part::Lit(s) => s.as_str(),
                    Part::Var(_) => "",
                })
                .collect();
            return self.b.lit(node, Some(s));
        }
        self.b.concat(node, parts)
    }

    fn object(&mut self, node: Node) -> Var {
        let dst = self.b.temp(self.b.pos(node));
        self.b.emit(node, StmtKind::Lit { dst, value: None });
        for c in named_children(node) {
            match c.kind() {
                "pair" => {
                    let key = c.child_by_field_name("key").and_then(|k| self.key_name(k));
                    if let Some(value) = c.child_by_field_name("value") {
                        let v = self.expr_named(value, key.clone());
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
                "shorthand_property_identifier" => {
                    let name = self.text(c).to_string();
                    let v = self.expr(c);
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field: Some(name),
                            src: v,
                        },
                    );
                }
                "spread_element" => {
                    if let Some(inner) = c.named_child(0) {
                        let v = self.expr(inner);
                        self.b.copy(c, dst, v);
                    }
                }
                "method_definition" => {
                    let key = c.child_by_field_name("name").and_then(|k| self.key_name(k));
                    let f = self.function(c, key.clone(), None);
                    let v = self.b.temp(self.b.pos(c));
                    self.b.emit(c, StmtKind::FuncRef { dst: v, func: f });
                    self.b.emit(
                        c,
                        StmtKind::Store {
                            obj: dst,
                            field: key,
                            src: v,
                        },
                    );
                }
                _ => {}
            }
        }
        dst
    }

    fn args(&mut self, args: Node) -> Vec<Arg> {
        if args.kind() == "template_string" {
            let v = self.template(args);
            return vec![Arg {
                var: v,
                kind: ArgKind::Positional,
            }];
        }
        let mut out = Vec::new();
        for c in named_children(args) {
            if c.kind() == "spread_element" {
                let v = match c.named_child(0) {
                    Some(inner) => self.expr(inner),
                    None => continue,
                };
                out.push(Arg {
                    var: v,
                    kind: ArgKind::Spread,
                });
            } else if !is_type_node(c.kind()) {
                let v = self.expr(c);
                out.push(Arg {
                    var: v,
                    kind: ArgKind::Positional,
                });
            }
        }
        out
    }

    fn call(&mut self, node: Node) -> Var {
        let func = node.child_by_field_name("function");
        let args_node = node.child_by_field_name("arguments");
        // `require('m')` and `import('m')` with a literal specifier are imports.
        if let (Some(f), Some(a)) = (func, args_node) {
            let is_require = f.kind() == "identifier"
                && self.text(f) == "require"
                && self.b.lookup("require").is_none();
            if is_require || f.kind() == "import" {
                let first = named_children(a).into_iter().next();
                if let Some(s) = first.filter(|s| s.kind() == "string") {
                    let module = unquote(self.text(s));
                    let pos = self.b.pos(node);
                    let var = self.b.temp(pos);
                    self.push_import(node, var, module, ImportKind::Namespace);
                    return var;
                }
            }
        }
        let (callee, method) = match func {
            Some(f) if f.kind() == "member_expression" => {
                let recv = f.child_by_field_name("object").map(|o| self.expr(o));
                let name = f
                    .child_by_field_name("property")
                    .map(|p| self.text(p).trim_start_matches('#').to_string())
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
            Some(f) if f.kind() == "subscript_expression" => {
                let recv = f.child_by_field_name("object").map(|o| self.expr(o));
                match (recv, f.child_by_field_name("index")) {
                    (Some(recv), Some(i)) if i.kind() == "string" => {
                        let name = unquote(self.text(i));
                        (
                            Callee::Method {
                                recv,
                                name: name.clone(),
                            },
                            Some((recv, name)),
                        )
                    }
                    (_, Some(i)) => {
                        self.expr(i);
                        (Callee::Dynamic, None)
                    }
                    _ => (Callee::Dynamic, None),
                }
            }
            Some(f) => (Callee::Value(self.expr(f)), None),
            None => (Callee::Dynamic, None),
        };
        let args = match args_node {
            Some(a) => self.args(a),
            None => Vec::new(),
        };
        let dst = self.b.temp(self.b.pos(node));
        // `x.get('email')` reads a named entry (FormData, URLSearchParams, Map, Headers).
        let literal_key = match (&method, args.first()) {
            (Some((_, name)), Some(_)) if name == "get" || name == "getAll" => args_node
                .and_then(|a| named_children(a).into_iter().next())
                .filter(|s| s.kind() == "string")
                .map(|s| unquote(self.text(s))),
            _ => None,
        };
        self.b.emit(
            node,
            StmtKind::Call {
                dst,
                callee,
                args,
                is_new: false,
            },
        );
        if let (Some((recv, _)), Some(key)) = (method, literal_key) {
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

    // ---------------------------------------------------------------- functions and classes

    fn function(&mut self, node: Node, name: Option<String>, class: Option<ClassIdx>) -> FuncIdx {
        let display = match (&class, &name) {
            (Some(c), Some(n)) => format!("{}.{}", self.b.ir.classes[*c as usize].name, n),
            (_, Some(n)) => n.clone(),
            _ => "<anonymous>".into(),
        };
        let is_ctor = class.is_some() && name.as_deref() == Some("constructor");
        let f = self.b.begin_function(display, node, class);
        let outer_ctor = self.b.ctor_this.take();
        if let (true, Some(c)) = (is_ctor, class) {
            let pos = self.b.pos(node);
            self.b.begin_ctor(c, pos);
        } else if node.kind() == "arrow_function" {
            // Arrow functions see the enclosing `this`, including a constructor's.
            self.b.ctor_this = outer_ctor;
        }
        if let Some(params) = node.child_by_field_name("parameters") {
            for p in named_children(params) {
                self.param(p, is_ctor);
            }
        } else if let Some(p) = node.child_by_field_name("parameter") {
            // `x => ...`
            self.param(p, false);
        }
        if let Some(rt) = node.child_by_field_name("return_type") {
            let ret = self.b.ret_var();
            self.type_ref(ret, rt);
            self.b.ir.funcs[f as usize].ret_annot = reduce_type_name(self.text(rt));
        }
        if let Some(body) = node.child_by_field_name("body") {
            if body.kind() == "statement_block" {
                self.hoist(body);
                for c in named_children(body) {
                    self.stmt(c);
                }
            } else {
                let v = self.expr(body);
                self.b.emit(body, StmtKind::Return { src: v });
            }
        }
        if is_ctor {
            let pos = self.b.pos(node);
            self.b.end_ctor(pos);
        }
        self.b.ctor_this = outer_ctor;
        self.b.end_function(f);
        f
    }

    fn param(&mut self, p: Node, is_ctor: bool) {
        let (pattern, annot, default, modifier) = match p.kind() {
            "required_parameter" | "optional_parameter" => {
                let modifier = named_children(p).into_iter().any(|c| {
                    c.kind() == "accessibility_modifier" || c.kind() == "override_modifier"
                }) || {
                    let mut cursor = p.walk();
                    p.children(&mut cursor).any(|c| c.kind() == "readonly")
                };
                (
                    p.child_by_field_name("pattern"),
                    p.child_by_field_name("type")
                        .and_then(|t| reduce_type_name(self.text(t))),
                    p.child_by_field_name("value"),
                    modifier,
                )
            }
            "assignment_pattern" => (
                p.child_by_field_name("left"),
                None,
                p.child_by_field_name("right"),
                false,
            ),
            "comment" => return,
            k if is_type_node(k) => return,
            _ => (Some(p), None, None, false),
        };
        let Some(pattern) = pattern else { return };
        let (name, rest, inner) = match pattern.kind() {
            "identifier" => (self.text(pattern).to_string(), false, None),
            "rest_pattern" => match pattern.named_child(0) {
                Some(i) if i.kind() == "identifier" => (self.text(i).to_string(), true, None),
                Some(i) => (String::new(), true, Some(i)),
                None => (String::new(), true, None),
            },
            "this" => return,
            _ => (String::new(), false, Some(pattern)),
        };
        let pos = self.b.pos(pattern);
        let var = self.b.declare_param(&name, pos);
        if let Some(t) = p
            .child_by_field_name("type")
            .filter(|_| matches!(p.kind(), "required_parameter" | "optional_parameter"))
        {
            self.type_ref(var, t);
        }
        let f = self.b.func as usize;
        self.b.ir.funcs[f].params.push(Param {
            var,
            name: name.clone(),
            rest,
            kwrest: false,
            annot: annot.clone(),
        });
        if let Some(a) = &annot {
            self.b.ir.annots.push((var, a.clone()));
        }
        if let Some(d) = default {
            let dv = self.expr(d);
            self.b.copy(d, var, dv);
        }
        if let Some(inner) = inner {
            self.bind(inner, var, Some(true));
        }
        // TypeScript parameter properties: `constructor(private mailer: Mailer)`.
        if is_ctor
            && modifier
            && !name.is_empty()
            && let Some(&c) = self.b.class_stack.last()
        {
            let this = match self.b.ctor_this {
                Some((_, t)) => t,
                None => self.b.ir.classes[c as usize].this,
            };
            self.b.store_member(p, this, Some(name.clone()), var);
            let class_name = self.b.ir.classes[c as usize].name.clone();
            if let Some(t) = self
                .b
                .ir
                .types
                .iter_mut()
                .rev()
                .find(|t| t.name == class_name)
            {
                t.fields.push((name, pos));
            }
        }
    }

    /// Lower a class. Declarations bind the class name in the current scope; expressions return
    /// a fresh variable holding the class.
    fn class(&mut self, node: Node, declaration: bool) -> Var {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.text(n).to_string());
        let display = name.clone().unwrap_or_else(|| "<class>".into());
        let c = self.b.new_class(&display, node);
        let pos = self.b.pos(node);
        let var = match (&name, declaration) {
            (Some(n), true) => self.b.declare(n, pos, false),
            _ => self.b.temp(pos),
        };
        self.b.emit(node, StmtKind::ClassRef { dst: var, class: c });
        // `extends Base` (TypeScript wraps it in an `extends_clause`; `implements` names types,
        // not values, and is not a base).
        for h in named_children(node) {
            if h.kind() != "class_heritage" {
                continue;
            }
            for e in named_children(h) {
                let value = match e.kind() {
                    "extends_clause" => e.child_by_field_name("value").or_else(|| e.named_child(0)),
                    "implements_clause" => None,
                    _ => Some(e),
                };
                if let Some(v) = value {
                    let b = self.expr(v);
                    self.b.ir.classes[c as usize].bases.push(b);
                }
            }
        }
        let type_index = self.b.ir.types.len();
        self.b.ir.types.push(TypeDecl {
            name: display.clone(),
            fields: Vec::new(),
            pos,
        });
        self.b.class_stack.push(c);
        let this = self.b.ir.classes[c as usize].this;
        if let Some(body) = node.child_by_field_name("body") {
            for m in named_children(body) {
                match m.kind() {
                    "method_definition" | "abstract_method_signature" => {
                        let mname = m.child_by_field_name("name").and_then(|k| self.key_name(k));
                        if m.kind() == "method_definition" {
                            let f = self.function(m, mname.clone(), Some(c));
                            let mut cur = m.walk();
                            if m.children(&mut cur).any(|k| k.kind() == "get") {
                                self.b.ir.funcs[f as usize].is_property = true;
                            }
                            if let Some(mname) = mname {
                                self.b.ir.classes[c as usize].methods.push((mname, f));
                            }
                        }
                    }
                    "public_field_definition" | "field_definition" => {
                        let key = m
                            .child_by_field_name("name")
                            .or_else(|| m.child_by_field_name("property"))
                            .and_then(|k| self.key_name(k));
                        if let Some(k) = &key {
                            let p = self.b.pos(m);
                            self.b.ir.types[type_index].fields.push((k.clone(), p));
                        }
                        if let Some(value) = m.child_by_field_name("value") {
                            let v = self.expr_named(value, key.clone());
                            self.b.store_member(m, this, key.clone(), v);
                        }
                        // `private readonly logger: Logger;` says what `this.logger` is.
                        if let Some(t) = m.child_by_field_name("type") {
                            let v = self.b.temp(self.b.pos(m));
                            self.type_ref(v, t);
                            self.b.store_member(m, this, key, v);
                        }
                    }
                    "class_static_block" => {
                        for s in named_children(m) {
                            self.stmt(s);
                        }
                    }
                    _ => {}
                }
            }
        }
        self.b.finish_class(c);
        self.b.class_stack.pop();
        var
    }

    fn interface(&mut self, node: Node) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let mut fields = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            self.collect_property_signatures(body, &mut fields);
        }
        let pos = self.b.pos(node);
        self.b.ir.types.push(TypeDecl {
            name: self.text(name).to_string(),
            fields,
            pos,
        });
    }

    fn type_alias(&mut self, node: Node) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let mut fields = Vec::new();
        if let Some(value) = node.child_by_field_name("value") {
            self.collect_property_signatures(value, &mut fields);
        }
        if fields.is_empty() {
            return;
        }
        let pos = self.b.pos(node);
        self.b.ir.types.push(TypeDecl {
            name: self.text(name).to_string(),
            fields,
            pos,
        });
    }

    fn collect_property_signatures(&mut self, node: Node, out: &mut Vec<(String, Pos)>) {
        for c in named_children(node) {
            match c.kind() {
                "property_signature" => {
                    if let Some(n) = c.child_by_field_name("name").and_then(|k| self.key_name(k)) {
                        out.push((n, self.b.pos(c)));
                    }
                }
                "object_type" | "interface_body" | "intersection_type" | "parenthesized_type" => {
                    self.collect_property_signatures(c, out)
                }
                _ => {}
            }
        }
    }

    // ---------------------------------------------------------------- modules

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

    fn import(&mut self, node: Node) {
        let Some(source) = node.child_by_field_name("source") else {
            return;
        };
        let module = unquote(self.text(source));
        // `import type { X } from 'm'` binds nothing at run time, but it names what annotated
        // values are; it is bound like any import, and only type references read it.
        for clause in named_children(node) {
            if clause.kind() != "import_clause" {
                continue;
            }
            for c in named_children(clause) {
                match c.kind() {
                    "identifier" => {
                        let pos = self.b.pos(c);
                        let var = self.b.declare(self.text(c), pos, false);
                        self.push_import(c, var, module.clone(), ImportKind::Default);
                    }
                    "namespace_import" => {
                        if let Some(id) = c.named_child(0) {
                            let pos = self.b.pos(id);
                            let var = self.b.declare(self.text(id), pos, false);
                            self.push_import(c, var, module.clone(), ImportKind::Namespace);
                        }
                    }
                    "named_imports" => {
                        for spec in named_children(c) {
                            if spec.kind() != "import_specifier" {
                                continue;
                            }
                            let Some(name) = spec.child_by_field_name("name") else {
                                continue;
                            };
                            let imported = unquote(self.text(name));
                            let local = spec.child_by_field_name("alias").unwrap_or(name);
                            let pos = self.b.pos(local);
                            let var = self.b.declare(&unquote(self.text(local)), pos, false);
                            let kind = if imported == "default" {
                                ImportKind::Default
                            } else {
                                ImportKind::Named(imported)
                            };
                            self.push_import(spec, var, module.clone(), kind);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn export(&mut self, node: Node) {
        let source = node
            .child_by_field_name("source")
            .map(|s| unquote(self.text(s)));
        if let Some(decl) = node.child_by_field_name("declaration") {
            let is_default = {
                let mut cursor = node.walk();
                node.children(&mut cursor).any(|c| c.kind() == "default")
            };
            self.stmt(decl);
            let mut names = Vec::new();
            match decl.kind() {
                "function_declaration"
                | "generator_function_declaration"
                | "class_declaration"
                | "abstract_class_declaration" => {
                    if let Some(n) = decl.child_by_field_name("name") {
                        names.push(self.text(n).to_string());
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    for d in named_children(decl) {
                        if let Some(n) = d.child_by_field_name("name") {
                            let mut found = Vec::new();
                            pattern_names(n, self.b.src, &mut found);
                            names.extend(found.into_iter().map(|(n, _)| n));
                        }
                    }
                }
                _ => {}
            }
            for n in names {
                if let Some(var) = self.b.lookup(&n) {
                    let name = if is_default { "default".to_string() } else { n };
                    self.b.ir.exports.push(ExportIr { name, var });
                }
            }
            return;
        }
        if let Some(value) = node.child_by_field_name("value") {
            let v = self.expr_named(value, Some("default".into()));
            self.b.ir.exports.push(ExportIr {
                name: "default".into(),
                var: v,
            });
            return;
        }
        let mut handled = false;
        for c in named_children(node) {
            match c.kind() {
                "export_clause" => {
                    handled = true;
                    for spec in named_children(c) {
                        if spec.kind() != "export_specifier" {
                            continue;
                        }
                        let Some(name) = spec.child_by_field_name("name") else {
                            continue;
                        };
                        let local = unquote(self.text(name));
                        let exported = spec
                            .child_by_field_name("alias")
                            .map(|a| unquote(self.text(a)))
                            .unwrap_or_else(|| local.clone());
                        let var = match &source {
                            Some(m) => {
                                let var = self.b.temp(self.b.pos(spec));
                                let kind = if local == "default" {
                                    ImportKind::Default
                                } else {
                                    ImportKind::Named(local)
                                };
                                self.push_import(spec, var, m.clone(), kind);
                                var
                            }
                            None => self.b.resolve(&local, name),
                        };
                        self.b.ir.exports.push(ExportIr {
                            name: exported,
                            var,
                        });
                    }
                }
                "namespace_export" => {
                    handled = true;
                    if let (Some(m), Some(id)) = (&source, c.named_child(0)) {
                        let var = self.b.temp(self.b.pos(c));
                        self.push_import(c, var, m.clone(), ImportKind::Namespace);
                        let name = unquote(self.text(id));
                        self.b.ir.exports.push(ExportIr { name, var });
                    }
                }
                _ => {}
            }
        }
        if !handled && let Some(m) = source {
            self.b.ir.reexports.push(m);
        }
    }
}

impl Js<'_> {
    /// For provenance: each name `left` binds is an element of `coll`.
    fn elements_of(&mut self, left: Node, coll: Var) {
        let mut names = Vec::new();
        pattern_names(left, self.b.src, &mut names);
        for (name, n) in names {
            if let Some(dst) = self.b.lookup(&name) {
                self.b.emit(n, StmtKind::Elements { dst, coll });
            }
        }
    }
}

/// The identifiers a binding pattern declares, in source order.
pub fn pattern_names<'t>(node: Node<'t>, src: &str, out: &mut Vec<(String, Node<'t>)>) {
    match node.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            out.push((src[node.byte_range()].to_string(), node));
        }
        "object_pattern" | "array_pattern" => {
            for c in named_children(node) {
                pattern_names(c, src, out);
            }
        }
        "pair_pattern" => {
            if let Some(v) = node.child_by_field_name("value") {
                pattern_names(v, src, out);
            }
        }
        "object_assignment_pattern" | "assignment_pattern" => {
            if let Some(l) = node.child_by_field_name("left") {
                pattern_names(l, src, out);
            }
        }
        "rest_pattern" => {
            if let Some(i) = node.named_child(0) {
                pattern_names(i, src, out);
            }
        }
        _ => {}
    }
}
