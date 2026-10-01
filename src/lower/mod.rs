//! Lowering: one module per language turns a tree-sitter syntax tree into [`FileIr`].
//!
//! [`Builder`] holds what every language needs: the scope stack, variable and statement
//! creation, positions in code points, and citation text. The language modules decide what a
//! construct means; the builder only records it.

pub mod js;
pub mod python;

use std::collections::HashMap;

use tree_sitter::Node;

use crate::ir::*;

/// Lower one file. Never fails: a file that does not parse is lowered as far as the tree goes
/// and carries a `ParseError` note, which the coverage section reports.
pub fn lower_file(path: &str, lang: Lang, source: &str) -> FileIr {
    match lang {
        Lang::Javascript | Lang::Typescript => js::lower(path, lang, source),
        Lang::Python => python::lower(path, source),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Function,
    Block,
    /// A Python class body: its names are not visible to the methods inside it.
    Class,
}

pub struct Scope {
    pub kind: ScopeKind,
    pub func: FuncIdx,
    pub names: HashMap<String, Var>,
}

pub struct Builder<'s> {
    pub ir: FileIr,
    pub src: &'s str,
    line_starts: Vec<usize>,
    pub scopes: Vec<Scope>,
    pub func: FuncIdx,
    pub class_stack: Vec<ClassIdx>,
    globals: HashMap<(FuncIdx, String), Var>,
    /// One variable per (class, field) for `this.field` in methods.
    field_vars: HashMap<(ClassIdx, String), Var>,
    /// The constructor being lowered and its own `this`.
    pub ctor_this: Option<(ClassIdx, Var)>,
}

const TEXT_MAX: usize = 80;

impl<'s> Builder<'s> {
    pub fn new(path: &str, lang: Lang, src: &'s str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        let mut ir = FileIr::new(path, lang);
        ir.lines = if src.is_empty() {
            0
        } else {
            line_starts.len() as u32 - u32::from(src.ends_with('\n'))
        };
        let mut b = Builder {
            ir,
            src,
            line_starts,
            scopes: Vec::new(),
            func: 0,
            class_stack: Vec::new(),
            globals: HashMap::new(),
            field_vars: HashMap::new(),
            ctor_this: None,
        };
        // Function 0 is the module: it owns top-level statements and module-level bindings.
        let ret = b.raw_var("<return>", VarKind::Ret, 0, Pos { line: 1, column: 1 });
        b.ir.funcs.push(FuncIr {
            name: "<module>".into(),
            params: Vec::new(),
            ret,
            parent: None,
            class: None,
            bound_self: false,
            ret_annot: None,
            decorators: Vec::new(),
            pos: Pos { line: 1, column: 1 },
            is_module: true,
        });
        b.scopes.push(Scope {
            kind: ScopeKind::Function,
            func: 0,
            names: HashMap::new(),
        });
        b
    }

    pub fn finish(self) -> FileIr {
        self.ir
    }

    pub fn pos(&self, node: Node) -> Pos {
        self.pos_at(node.start_byte())
    }

    pub fn pos_at(&self, byte: usize) -> Pos {
        let line = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.line_starts[line];
        let column = self
            .src
            .get(start..byte)
            .map_or(byte - start, |s| s.chars().count());
        Pos {
            line: line as u32 + 1,
            column: column as u32 + 1,
        }
    }

    pub fn text(&self, node: Node) -> &'s str {
        self.src.get(node.byte_range()).unwrap_or("")
    }

    /// Citation text: the node's source on one line, whitespace collapsed, at most 80 chars.
    pub fn cite(&self, node: Node) -> String {
        let raw = self.text(node);
        let mut out = String::with_capacity(raw.len().min(TEXT_MAX + 3));
        let mut last_space = false;
        for c in raw.chars() {
            if c.is_whitespace() {
                if !last_space && !out.is_empty() {
                    out.push(' ');
                }
                last_space = true;
            } else {
                out.push(c);
                last_space = false;
            }
            if out.chars().count() >= TEXT_MAX {
                out.push('…');
                break;
            }
        }
        out.trim_end().to_string()
    }

    fn raw_var(&mut self, name: &str, kind: VarKind, func: FuncIdx, pos: Pos) -> Var {
        let id = self.ir.vars.len() as Var;
        self.ir.vars.push(VarInfo {
            name: name.to_string(),
            kind,
            func,
            pos,
        });
        id
    }

    pub fn temp(&mut self, pos: Pos) -> Var {
        self.raw_var("", VarKind::Temp, self.func, pos)
    }

    /// Declare `name` in the innermost scope (or the innermost function scope when
    /// `function_scoped`), returning the existing variable if it is already declared there.
    pub fn declare(&mut self, name: &str, pos: Pos, function_scoped: bool) -> Var {
        let idx = if function_scoped {
            self.scopes
                .iter()
                .rposition(|s| s.kind == ScopeKind::Function)
                .unwrap_or(0)
        } else {
            self.scopes.len() - 1
        };
        if let Some(&v) = self.scopes[idx].names.get(name) {
            return v;
        }
        let func = self.scopes[idx].func;
        let v = self.raw_var(name, VarKind::Local, func, pos);
        self.scopes[idx].names.insert(name.to_string(), v);
        v
    }

    /// Declare `name` at module level (an assignment to an undeclared name).
    pub fn declare_module(&mut self, name: &str, pos: Pos) -> Var {
        if let Some(&v) = self.scopes[0].names.get(name) {
            return v;
        }
        let v = self.raw_var(name, VarKind::Local, 0, pos);
        self.scopes[0].names.insert(name.to_string(), v);
        v
    }

    pub fn declare_param(&mut self, name: &str, pos: Pos) -> Var {
        let v = self.raw_var(name, VarKind::Param, self.func, pos);
        if !name.is_empty() {
            self.scopes
                .last_mut()
                .expect("scope")
                .names
                .insert(name.to_string(), v);
        }
        v
    }

    /// Resolve a name through the scope stack. Class scopes are visible only from directly
    /// inside them (Python semantics; JS never pushes class scopes).
    pub fn lookup(&self, name: &str) -> Option<Var> {
        let top = self.scopes.len() - 1;
        for (i, scope) in self.scopes.iter().enumerate().rev() {
            if scope.kind == ScopeKind::Class && i != top {
                continue;
            }
            if let Some(&v) = scope.names.get(name) {
                return Some(v);
            }
        }
        None
    }

    /// A free identifier: one `Global` statement per function and name.
    pub fn global(&mut self, name: &str, node: Node) -> Var {
        if let Some(&v) = self.globals.get(&(self.func, name.to_string())) {
            return v;
        }
        let pos = self.pos(node);
        let dst = self.temp(pos);
        self.emit(
            node,
            StmtKind::Global {
                dst,
                name: name.to_string(),
            },
        );
        self.globals.insert((self.func, name.to_string()), dst);
        dst
    }

    pub fn resolve(&mut self, name: &str, node: Node) -> Var {
        match self.lookup(name) {
            Some(v) => v,
            None => self.global(name, node),
        }
    }

    pub fn emit(&mut self, node: Node, kind: StmtKind) {
        let pos = self.pos(node);
        let text = self.cite(node);
        self.ir.stmts.push(Stmt {
            func: self.func,
            pos,
            text,
            kind,
        });
    }

    pub fn emit_at(&mut self, pos: Pos, text: String, kind: StmtKind) {
        self.ir.stmts.push(Stmt {
            func: self.func,
            pos,
            text,
            kind,
        });
    }

    pub fn lit(&mut self, node: Node, value: Option<String>) -> Var {
        let dst = self.temp(self.pos(node));
        self.emit(node, StmtKind::Lit { dst, value });
        dst
    }

    pub fn copy(&mut self, node: Node, dst: Var, src: Var) {
        if dst != src {
            self.emit(node, StmtKind::Copy { dst, src });
        }
    }

    pub fn load(&mut self, node: Node, obj: Var, field: Option<String>) -> Var {
        let dst = self.temp(self.pos(node));
        self.emit(node, StmtKind::Load { dst, obj, field });
        dst
    }

    /// A value that may be any of `vars` (`a || b`, `c ? a : b`).
    pub fn union(&mut self, node: Node, vars: &[Var]) -> Var {
        let dst = self.temp(self.pos(node));
        for &v in vars {
            self.copy(node, dst, v);
        }
        dst
    }

    pub fn concat(&mut self, node: Node, parts: Vec<Part>) -> Var {
        let dst = self.temp(self.pos(node));
        self.emit(node, StmtKind::Concat { dst, parts });
        dst
    }

    /// Start a function; returns its index. The caller lowers parameters and body, then calls
    /// [`Builder::end_function`].
    pub fn begin_function(&mut self, name: String, node: Node, class: Option<ClassIdx>) -> FuncIdx {
        let idx = self.ir.funcs.len() as FuncIdx;
        let pos = self.pos(node);
        let ret = self.raw_var("<return>", VarKind::Ret, idx, pos);
        self.ir.funcs.push(FuncIr {
            name,
            params: Vec::new(),
            ret,
            parent: Some(self.func),
            class,
            bound_self: false,
            ret_annot: None,
            decorators: Vec::new(),
            pos,
            is_module: false,
        });
        self.func = idx;
        self.scopes.push(Scope {
            kind: ScopeKind::Function,
            func: idx,
            names: HashMap::new(),
        });
        idx
    }

    pub fn end_function(&mut self, idx: FuncIdx) {
        self.scopes.pop();
        self.func = self.ir.funcs[idx as usize].parent.unwrap_or(0);
    }

    pub fn push_block(&mut self) {
        let func = self.func;
        self.scopes.push(Scope {
            kind: ScopeKind::Block,
            func,
            names: HashMap::new(),
        });
    }

    pub fn push_class_scope(&mut self) {
        let func = self.func;
        self.scopes.push(Scope {
            kind: ScopeKind::Class,
            func,
            names: HashMap::new(),
        });
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn ret_var(&self) -> Var {
        self.ir.funcs[self.func as usize].ret
    }

    pub fn new_class(&mut self, name: &str, node: Node) -> ClassIdx {
        let idx = self.ir.classes.len() as ClassIdx;
        let pos = self.pos(node);
        let this = self.raw_var("this", VarKind::This, self.func, pos);
        self.ir.classes.push(ClassIr {
            name: name.to_string(),
            this,
            ctor_this: None,
            methods: Vec::new(),
            pos,
        });
        // `this` is an instance of the class, so `this.method()` resolves.
        self.instance_of(this, idx, pos);
        idx
    }

    /// Give `var` the provenance "an instance of `class`".
    fn instance_of(&mut self, var: Var, class: ClassIdx, pos: Pos) {
        let c = self.raw_var("", VarKind::Temp, self.func, pos);
        self.emit_at(pos, "this".into(), StmtKind::ClassRef { dst: c, class });
        self.emit_at(pos, "this".into(), StmtKind::TypeRef { dst: var, ty: c });
    }

    /// The class whose shared instance `v` is, if it is one.
    pub fn class_of_this(&self, v: Var) -> Option<ClassIdx> {
        self.ir
            .classes
            .iter()
            .position(|c| c.this == v)
            .map(|i| i as ClassIdx)
    }

    /// The variable holding `this.<field>` for a class.
    pub fn field_var(&mut self, class: ClassIdx, field: &str) -> Var {
        if let Some(&v) = self.field_vars.get(&(class, field.to_string())) {
            return v;
        }
        let this = self.ir.classes[class as usize].this;
        let (func, pos) = (
            self.ir.vars[this as usize].func,
            self.ir.vars[this as usize].pos,
        );
        let v = self.raw_var(&format!("this.{field}"), VarKind::This, func, pos);
        self.field_vars.insert((class, field.to_string()), v);
        v
    }

    pub fn this_load(&mut self, node: Node, class: ClassIdx, field: &str) -> Var {
        let var = self.field_var(class, field);
        let obj = self.ir.classes[class as usize].this;
        let dst = self.temp(self.pos(node));
        self.emit(
            node,
            StmtKind::ThisLoad {
                dst,
                obj,
                field: field.to_string(),
                var,
            },
        );
        dst
    }

    pub fn this_store(&mut self, node: Node, class: ClassIdx, field: &str, src: Var) {
        let var = self.field_var(class, field);
        let obj = self.ir.classes[class as usize].this;
        self.emit(
            node,
            StmtKind::ThisStore {
                obj,
                field: field.to_string(),
                src,
                var,
            },
        );
    }

    /// A write to `obj.field`, where `obj` may be an instance: in a method it goes to the
    /// field's variable, in the constructor to the constructor's own `this` and the field's
    /// variable, and otherwise it is a plain field write.
    pub fn store_member(&mut self, node: Node, obj: Var, field: Option<String>, src: Var) {
        if let Some(f) = &field {
            if let Some(c) = self.class_of_this(obj) {
                self.this_store(node, c, f, src);
                return;
            }
            if let Some((c, t)) = self.ctor_this
                && t == obj
            {
                self.emit(
                    node,
                    StmtKind::Store {
                        obj: t,
                        field: field.clone(),
                        src,
                    },
                );
                self.this_store(node, c, f, src);
                return;
            }
        }
        self.emit(node, StmtKind::Store { obj, field, src });
    }

    /// A read of `obj.field`, through the field's variable when `obj` is a method's `this`.
    pub fn load_member(&mut self, node: Node, obj: Var, field: Option<String>) -> Var {
        if let (Some(f), Some(c)) = (&field, self.class_of_this(obj)) {
            return self.this_load(node, c, f);
        }
        self.load(node, obj, field)
    }

    /// At the end of a class: the whole instance holds every field (`log(this)`).
    pub fn finish_class(&mut self, class: ClassIdx) {
        let this = self.ir.classes[class as usize].this;
        let pos = self.ir.classes[class as usize].pos;
        let mut fields: Vec<(String, Var)> = self
            .field_vars
            .iter()
            .filter(|((c, _), _)| *c == class)
            .map(|((_, f), v)| (f.clone(), *v))
            .collect();
        fields.sort();
        for (f, v) in fields {
            self.emit_at(
                pos,
                format!("this.{f}"),
                StmtKind::Store {
                    obj: this,
                    field: Some(f),
                    src: v,
                },
            );
        }
    }

    /// Start lowering a constructor: its `this` is its own local, an instance of the class.
    pub fn begin_ctor(&mut self, class: ClassIdx, pos: Pos) -> Var {
        let t = self.raw_var("this", VarKind::Local, self.func, pos);
        self.instance_of(t, class, pos);
        self.ir.classes[class as usize].ctor_this = Some(t);
        self.ctor_this = Some((class, t));
        t
    }

    /// End a constructor: it returns its `this`.
    pub fn end_ctor(&mut self, pos: Pos) {
        if let Some((_, t)) = self.ctor_this.take() {
            self.emit_at(pos, "this".into(), StmtKind::Return { src: t });
        }
    }

    pub fn note(&mut self, kind: NoteKind, node: Node, detail: String) {
        let pos = self.pos(node);
        self.ir.notes.push(LowerNote { kind, pos, detail });
    }

    /// Record parse errors: one note per ERROR or MISSING node, outermost only.
    pub fn record_parse_errors(&mut self, root: Node) {
        if !root.has_error() {
            return;
        }
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if n.is_error() || n.is_missing() {
                let detail = if n.is_missing() {
                    format!("missing {}", n.kind())
                } else {
                    "syntax error".to_string()
                };
                self.note(NoteKind::ParseError, n, detail);
                continue;
            }
            if n.has_error() {
                let mut cursor = n.walk();
                let children: Vec<Node> = n.children(&mut cursor).collect();
                for c in children.into_iter().rev() {
                    stack.push(c);
                }
            }
        }
        self.ir.notes.sort_by_key(|n| n.pos);
    }
}

/// Unquote a string literal's text: strips one layer of matching quotes (and Python prefixes)
/// without interpreting escapes, which is enough for hosts, keys and module specifiers.
pub fn unquote(raw: &str) -> String {
    let prefix = raw.find(['"', '\'', '`']).unwrap_or(0);
    let s = if prefix <= 2 && raw[..prefix].chars().all(|c| "rRbBuUfF".contains(c)) {
        &raw[prefix..]
    } else {
        raw
    };
    for q in ["\"\"\"", "'''", "\"", "'", "`"] {
        if s.len() >= 2 * q.len() && s.starts_with(q) && s.ends_with(q) {
            return s[q.len()..s.len() - q.len()].to_string();
        }
    }
    s.to_string()
}

pub fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| !c.is_extra())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::unquote;

    #[test]
    fn unquotes_only_string_literals() {
        assert_eq!(unquote("'a'"), "a");
        assert_eq!(unquote("f\"x{y}\""), "x{y}");
        assert_eq!(unquote("rb'z'"), "z");
        assert_eq!(unquote("format"), "format");
        assert_eq!(unquote("`t`"), "t");
    }
}
