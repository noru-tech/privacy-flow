//! The shared intermediate representation every language lowers into.
//!
//! The IR is deliberately small: variables, and statements that move values between them
//! (copies, field reads and writes, string building, calls, returns), plus the declarations the
//! resolver needs (functions, classes, imports, exports, type declarations). Everything
//! language-specific stays in [`crate::lower`]; nothing downstream looks at a syntax tree.
//!
//! Identifiers in a [`FileIr`] are local to that file. [`crate::program`] renumbers them into
//! one program in sorted file order, which is what makes the result independent of how many
//! threads lowered the files.

use serde::Serialize;

/// A file-local variable index.
pub type Var = u32;
/// A file-local function index.
pub type FuncIdx = u32;
/// A file-local class index.
pub type ClassIdx = u32;

/// A position in a source file: 1-based line and 1-based column in Unicode code points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    Javascript,
    Typescript,
    Python,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::Javascript => "javascript",
            Lang::Typescript => "typescript",
            Lang::Python => "python",
        }
    }

    /// The catalogue language: TypeScript and JavaScript share one catalogue.
    pub fn family(self) -> &'static str {
        match self {
            Lang::Javascript | Lang::Typescript => "javascript",
            Lang::Python => "python",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    /// A named local binding (or a module-level binding when `func` is the module function).
    Local,
    /// A formal parameter.
    Param,
    /// A compiler temporary.
    Temp,
    /// The return slot of a function.
    Ret,
    /// The shared `this`/`self` of a class: one per class, flow-insensitive across methods.
    This,
}

#[derive(Clone, Debug)]
pub struct VarInfo {
    pub name: String,
    pub kind: VarKind,
    /// Declaring function.
    pub func: FuncIdx,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub var: Var,
    pub name: String,
    /// `*args` (Python) or `...rest` (JS): collects every remaining positional argument.
    pub rest: bool,
    /// `**kwargs`: collects every unmatched keyword argument.
    pub kwrest: bool,
    pub annot: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FuncIr {
    /// Display name: `send`, `Mailer.send`, `<anonymous>`, `<module>`.
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Var,
    pub parent: Option<FuncIdx>,
    pub class: Option<ClassIdx>,
    /// Python method whose first parameter is the instance (`self`); call arguments start at 1.
    pub bound_self: bool,
    /// Return type annotation, reduced to a type name (`Promise<User>` → `User`).
    pub ret_annot: Option<String>,
    /// Decorator expressions (Python), lowered into variables of the enclosing function.
    pub decorators: Vec<Var>,
    /// A property: reading the member (`obj.name`, without a call) runs it and gives its result
    /// (Python `@property`, JavaScript `get name()`).
    pub is_property: bool,
    pub pos: Pos,
    /// True for the synthetic function holding a file's top-level statements.
    pub is_module: bool,
}

#[derive(Clone, Debug)]
pub struct ClassIr {
    pub name: String,
    /// The instance as methods see it: shared by every instance (flow-insensitive).
    pub this: Var,
    /// The instance inside the constructor, which the constructor returns: each `new C(...)`
    /// gets the constructor's summary, so instances do not share what they were built with.
    pub ctor_this: Option<Var>,
    /// (method name, function) in declaration order.
    pub methods: Vec<(String, FuncIdx)>,
    /// The base classes as written (`extends Base`, `class C(Base, Mixin)`), lowered into
    /// variables of the enclosing function, in order.
    pub bases: Vec<Var>,
    /// The TypeScript interfaces a class implements (or an interface extends): it is one of
    /// their subtypes, so a call on a value typed with one reaches it. Members are never looked
    /// up in them.
    pub implements: Vec<Var>,
    /// A TypeScript interface: a type with no members of its own.
    pub is_interface: bool,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<(String, Pos)>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportKind {
    /// `import x from 'm'` — the module's default export.
    Default,
    /// `import * as x from 'm'`, `const x = require('m')`, Python `import m` — the module object.
    Namespace,
    /// `import { a } from 'm'`, `from m import a`.
    Named(String),
}

#[derive(Clone, Debug)]
pub struct ImportIr {
    pub var: Var,
    /// The module specifier exactly as written (`./mailer`, `openai`, `.models`, `app.models`).
    pub module: String,
    pub kind: ImportKind,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct ExportIr {
    /// `default` for the default export; `*` re-exports are in `reexports`.
    pub name: String,
    pub var: Var,
}

#[derive(Clone, Debug)]
pub enum Part {
    Lit(String),
    Var(Var),
}

#[derive(Clone, Debug)]
pub enum ArgKind {
    Positional,
    Keyword(String),
    /// `...xs` / `*xs`.
    Spread,
    /// `**kw`.
    KwSpread,
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub var: Var,
    pub kind: ArgKind,
}

#[derive(Clone, Debug)]
pub enum Callee {
    /// Call a value: `f(x)`, `new C(x)`.
    Value(Var),
    /// Call a method: `recv.name(x)`.
    Method { recv: Var, name: String },
    /// A computed callee (`obj[k](x)`): never resolved.
    Dynamic,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `dst = src`: every field of `src` flows to the same field of `dst`.
    Copy {
        dst: Var,
        src: Var,
    },
    /// `dst = <literal>`; `value` is the text of a string literal.
    Lit {
        dst: Var,
        value: Option<String>,
    },
    /// `dst = obj.field` (`field` is `None` for a computed key).
    Load {
        dst: Var,
        obj: Var,
        field: Option<String>,
    },
    /// `obj.field = src` (`field` is `None` for a computed key).
    Store {
        obj: Var,
        field: Option<String>,
        src: Var,
    },
    /// `dst = a + "lit" + b`, template literals, f-strings: everything collapses into `dst`.
    Concat {
        dst: Var,
        parts: Vec<Part>,
    },
    Call {
        dst: Var,
        callee: Callee,
        args: Vec<Arg>,
        is_new: bool,
    },
    Return {
        src: Var,
    },
    FuncRef {
        dst: Var,
        func: FuncIdx,
    },
    ClassRef {
        dst: Var,
        class: ClassIdx,
    },
    /// A free identifier that no scope declares (`console`, `fetch`, `print`).
    Global {
        dst: Var,
        name: String,
    },
    Import {
        import: u32,
    },
    /// `dst = this.field` in a method: the field's own variable `var` (one per class and field)
    /// is the value read, so `this.config.url` keeps `config`'s fields apart. `obj` is the
    /// class's shared instance, for provenance and citations.
    ThisLoad {
        dst: Var,
        obj: Var,
        field: String,
        var: Var,
    },
    /// `this.field = src`: the value goes into the field's own variable `var`.
    ThisStore {
        obj: Var,
        field: String,
        src: Var,
        var: Var,
    },
    /// `dst` is declared with the type `ty` names (a TypeScript or Python annotation). Only
    /// provenance reads it: a parameter typed `Logger` from `pino` is a pino logger, one typed
    /// with a local class is an instance of it. No data flows.
    TypeRef {
        dst: Var,
        ty: Var,
    },
    /// `dst` is `super` in a method of `class`: the base classes, for calling their methods and
    /// constructors (`super.send(x)`, `super().__init__(x)`, `super(x)`). Only provenance reads it.
    Super {
        dst: Var,
        class: ClassIdx,
    },
    /// `dst` is bound to each element of `coll` (`for (const h of handlers)`, `for h in hs`).
    /// Only provenance reads it: the element is what the container holds, not the container.
    /// Data needs no statement of its own; a container's data is its elements'.
    Elements {
        dst: Var,
        coll: Var,
    },
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub func: FuncIdx,
    pub pos: Pos,
    /// The source text of the expression, on one line and truncated, for citations.
    pub text: String,
    pub kind: StmtKind,
}

/// Something the lowering could not model, recorded for the coverage section.
#[derive(Clone, Debug)]
pub struct LowerNote {
    pub kind: NoteKind,
    pub pos: Pos,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteKind {
    ParseError,
    /// `'use server'` and other constructs that make a function remotely callable in a way no
    /// supported framework model covers.
    UnsupportedConstruct,
}

#[derive(Clone, Debug)]
pub struct FileIr {
    pub path: String,
    pub lang: Lang,
    pub lines: u32,
    pub vars: Vec<VarInfo>,
    pub funcs: Vec<FuncIr>,
    pub classes: Vec<ClassIr>,
    pub stmts: Vec<Stmt>,
    pub imports: Vec<ImportIr>,
    pub exports: Vec<ExportIr>,
    /// `export * from 'm'` specifiers.
    pub reexports: Vec<String>,
    pub types: Vec<TypeDecl>,
    /// Variables with a type annotation, reduced to a type name.
    pub annots: Vec<(Var, String)>,
    pub notes: Vec<LowerNote>,
}

impl FileIr {
    pub fn new(path: &str, lang: Lang) -> Self {
        FileIr {
            path: path.to_string(),
            lang,
            lines: 0,
            vars: Vec::new(),
            funcs: Vec::new(),
            classes: Vec::new(),
            stmts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            reexports: Vec::new(),
            types: Vec::new(),
            annots: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Reduce a type annotation to the name of the type it carries: `Promise<User[]>` → `User`,
/// `Optional[User]` → `User`, `User | null` → `User`. Returns `None` for primitives.
pub fn reduce_type_name(text: &str) -> Option<String> {
    const WRAPPERS: &[&str] = &[
        "Promise",
        "Array",
        "ReadonlyArray",
        "Partial",
        "Required",
        "Readonly",
        "Optional",
        "List",
        "list",
        "Sequence",
        "Iterable",
        "Awaitable",
        "Mapped",
        "Union",
        "Set",
        "set",
        "tuple",
        "Tuple",
        "NonNullable",
    ];
    const PRIMITIVES: &[&str] = &[
        "string",
        "number",
        "boolean",
        "any",
        "unknown",
        "void",
        "null",
        "undefined",
        "never",
        "object",
        "bigint",
        "symbol",
        "str",
        "int",
        "float",
        "bool",
        "bytes",
        "None",
        "dict",
        "Dict",
        "Any",
        "Record",
        "Object",
        "String",
        "Number",
        "Boolean",
        "Date",
        "datetime",
    ];
    let mut t = text.trim().trim_start_matches(':').trim();
    // Strip unions with null/undefined/None: take the first non-null alternative.
    let alternatives: Vec<&str> = split_top_level(t, '|');
    if alternatives.len() > 1 {
        for alt in alternatives {
            let alt = alt.trim();
            if !matches!(alt, "null" | "undefined" | "None") {
                return reduce_type_name(alt);
            }
        }
        return None;
    }
    loop {
        t = t.trim();
        if let Some(stripped) = t.strip_suffix("[]") {
            t = stripped;
            continue;
        }
        let open = t.find(['<', '[']);
        if let Some(i) = open {
            let head = t[..i].trim();
            let head_last = head.rsplit('.').next().unwrap_or(head);
            if WRAPPERS.contains(&head_last) {
                let inner = &t[i + 1..t.len().saturating_sub(1)];
                let first = split_top_level(inner, ',');
                if head_last == "Union" || head_last == "Optional" {
                    for alt in first {
                        let alt = alt.trim();
                        if alt != "None" {
                            return reduce_type_name(alt);
                        }
                    }
                    return None;
                }
                t = first.first().copied().unwrap_or("");
                continue;
            }
            t = head;
        }
        break;
    }
    let name = t
        .rsplit('.')
        .next()
        .unwrap_or(t)
        .trim()
        .trim_matches(['"', '\'']);
    if name.is_empty()
        || PRIMITIVES.contains(&name)
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        return None;
    }
    Some(name.to_string())
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::reduce_type_name;

    #[test]
    fn reduces_wrapped_types() {
        assert_eq!(reduce_type_name("Promise<User[]>").as_deref(), Some("User"));
        assert_eq!(reduce_type_name(": User | null").as_deref(), Some("User"));
        assert_eq!(
            reduce_type_name("Optional[models.User]").as_deref(),
            Some("User")
        );
        assert_eq!(
            reduce_type_name("list[Customer]").as_deref(),
            Some("Customer")
        );
        assert_eq!(reduce_type_name("string"), None);
        assert_eq!(reduce_type_name("Record<string, any>"), None);
        assert_eq!(reduce_type_name("{ a: string }"), None);
    }
}
