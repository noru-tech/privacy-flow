//! One program from many files: renumbering, import resolution and provenance.
//!
//! Files are assembled in sorted path order, so every identifier is the same however many
//! threads lowered them. Imports resolve to scanned files (relative paths, `tsconfig.json`
//! path aliases, Python packages) or to external modules.
//!
//! **Provenance** answers "what is this value?" for callee resolution: an API path into an
//! external module (`openai:().chat.completions.create`), a local function, a class or an
//! instance of one, or a module. It is a flow-insensitive fixpoint over copies, field reads and
//! writes, calls and returns, with bounded path length and set size so that it terminates.
//! Provenance decides which function a call reaches; it never carries personal data.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ir::*;

pub type VarId = u32;
pub type FuncId = u32;
pub type ClassId = u32;
pub type FileId = u32;
pub type Sym = u32;

/// Longest API path kept, in segments; longer paths stop growing.
const MAX_PATH_SEGMENTS: usize = 16;
/// Most API paths kept per variable (the smallest are kept): paths can grow without bound
/// through loops and chains, so they are capped.
const MAX_API_PROVS: usize = 16;
/// Most provenance values of any kind per variable. Functions, classes and modules are finite
/// and every one matters (a component's render callbacks from all of its call sites), so this
/// is only a safety bound.
const MAX_PROVS: usize = 4096;
/// Bounds on a class hierarchy walk: ancestors of one class, and subclasses a virtual call
/// dispatches to.
const MAX_LINEAGE: usize = 32;
const MAX_DESCENDANTS: usize = 256;

#[derive(Default, Debug)]
pub struct Interner {
    strings: Vec<String>,
    index: HashMap<String, Sym>,
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.strings.len() as Sym;
        self.strings.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }

    pub fn get(&self, s: &str) -> Option<Sym> {
        self.index.get(s).copied()
    }

    pub fn str(&self, i: Sym) -> &str {
        &self.strings[i as usize]
    }
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub path: String,
    pub lang: Lang,
    pub lines: u32,
    pub module_func: FuncId,
    pub notes: Vec<LowerNote>,
    /// Python `from m import *` targets (names visible in this module).
    pub star: Vec<Target>,
    pub reexports: Vec<Target>,
}

#[derive(Clone, Debug)]
pub struct GVar {
    pub name: String,
    pub kind: VarKind,
    pub func: FuncId,
    pub file: FileId,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct GParam {
    pub var: VarId,
    pub name: String,
    pub rest: bool,
    pub kwrest: bool,
    pub annot: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GFunc {
    pub file: FileId,
    pub name: String,
    pub params: Vec<GParam>,
    pub ret: VarId,
    pub parent: Option<FuncId>,
    pub class: Option<ClassId>,
    pub bound_self: bool,
    pub ret_annot: Option<String>,
    pub decorators: Vec<VarId>,
    pub is_property: bool,
    pub pos: Pos,
    pub is_module: bool,
}

#[derive(Clone, Debug)]
pub struct GClass {
    pub file: FileId,
    pub name: String,
    pub this: VarId,
    pub ctor_this: Option<VarId>,
    pub methods: Vec<(String, FuncId)>,
    /// The base-class expressions as written; their provenance gives the bases.
    pub bases: Vec<VarId>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct GType {
    pub file: FileId,
    pub name: String,
    pub fields: Vec<(String, Pos)>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub enum GPart {
    Lit(String),
    Var(VarId),
}

#[derive(Clone, Debug)]
pub enum GArgKind {
    Positional,
    Keyword(String),
    Spread,
    KwSpread,
}

#[derive(Clone, Debug)]
pub struct GArg {
    pub var: VarId,
    pub kind: GArgKind,
}

#[derive(Clone, Debug)]
pub enum GCallee {
    Value(VarId),
    Method { recv: VarId, name: Sym },
    Dynamic,
}

#[derive(Clone, Debug)]
pub enum GKind {
    Copy {
        dst: VarId,
        src: VarId,
    },
    Lit {
        dst: VarId,
        value: Option<String>,
    },
    Load {
        dst: VarId,
        obj: VarId,
        field: Option<Sym>,
    },
    Store {
        obj: VarId,
        field: Option<Sym>,
        src: VarId,
    },
    Concat {
        dst: VarId,
        parts: Vec<GPart>,
    },
    Call {
        dst: VarId,
        callee: GCallee,
        args: Vec<GArg>,
        is_new: bool,
    },
    Return {
        src: VarId,
    },
    FuncRef {
        dst: VarId,
        func: FuncId,
    },
    ClassRef {
        dst: VarId,
        class: ClassId,
    },
    Global {
        dst: VarId,
        name: String,
    },
    Import {
        import: u32,
    },
    TypeRef {
        dst: VarId,
        ty: VarId,
    },
    Super {
        dst: VarId,
        class: ClassId,
    },
    Elements {
        dst: VarId,
        coll: VarId,
    },
    ThisLoad {
        dst: VarId,
        obj: VarId,
        field: Sym,
        var: VarId,
    },
    ThisStore {
        obj: VarId,
        field: Sym,
        src: VarId,
        var: VarId,
    },
}

#[derive(Clone, Debug)]
pub struct GStmt {
    pub file: FileId,
    pub func: FuncId,
    pub pos: Pos,
    pub text: String,
    pub kind: GKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Local(FileId),
    /// A Python package directory without an `__init__.py` among the scanned files.
    Package(String),
    External(String),
    Unresolved(String),
}

#[derive(Clone, Debug)]
pub struct GImport {
    pub file: FileId,
    pub var: VarId,
    pub module: String,
    pub kind: ImportKind,
    pub pos: Pos,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prov {
    /// An API path into an external module or a global.
    Api(Sym),
    Func(FuncId),
    Class(ClassId),
    Instance(ClassId),
    Module(FileId),
    Package(Sym),
    /// Reached through an import that names a file that was not found.
    Unresolved(Sym),
}

/// How a call was resolved.
#[derive(Clone, Debug, Default)]
pub struct CallTargets {
    /// Local functions, with whether the first parameter is the bound instance.
    pub funcs: Vec<(FuncId, bool)>,
    /// Classes instantiated by this call (`new C()`, Python `C()`).
    pub classes: Vec<ClassId>,
    pub apis: Vec<Sym>,
    pub unresolved: Vec<Sym>,
    /// The callee has no provenance at all (a parameter, a computed member).
    pub dynamic: bool,
    /// A method call on a receiver with no provenance: a plain value such as a string or an
    /// array, or an object the analysis cannot see the origin of.
    pub plain_method: Option<(VarId, Sym)>,
    /// A method called on `this`/`self` that this many subclasses override: the class's own
    /// (or inherited) method is followed, the overrides are not.
    pub overridden: usize,
}

pub use crate::resolve::Resolver;

pub struct Program {
    pub files: Vec<FileInfo>,
    pub vars: Vec<GVar>,
    pub funcs: Vec<GFunc>,
    pub classes: Vec<GClass>,
    pub stmts: Vec<GStmt>,
    pub imports: Vec<GImport>,
    pub types: Vec<GType>,
    pub annots: Vec<(VarId, String)>,
    pub syms: Interner,
    pub exports: Vec<BTreeMap<String, VarId>>,
    pub prov: Vec<Vec<Prov>>,
    /// Field provenance: `obj.k` holds these (object literals, `this.client = new X()`).
    pub fprov: HashMap<VarId, BTreeMap<Sym, Vec<Prov>>>,
    /// Call statement index → targets.
    pub calls: HashMap<u32, CallTargets>,
    file_index: BTreeMap<String, FileId>,
    /// Memo for [`Program::api_member`] and [`Program::api_call`]: the fixpoint asks for the
    /// same extensions every round.
    path_cache: HashMap<(Sym, String), Option<Sym>>,
    call_cache: HashMap<Sym, Option<Sym>>,
    /// The variables put into containers reached through an API path: a read sees their
    /// provenance and their fields'.
    path_containers: HashMap<Sym, BTreeSet<VarId>>,
    /// `this.f` read into a temporary → `f`'s own variable: a container written through
    /// `this.f.set(...)` is the field's, so a later `this.f.get(...)` sees it.
    this_home: HashMap<VarId, VarId>,
    /// The variables put into a container, keyed by the container's variable (or the field's
    /// variable for `this.f`): a read sees their provenance and their fields', as
    /// `path_containers` does for containers reached through an API path.
    var_containers: HashMap<VarId, BTreeSet<VarId>>,
    /// Direct local subclasses of each class, from the provenance of their base expressions.
    pub subclasses: Vec<Vec<ClassId>>,
    /// The variable of each class's own `this.f`, by field.
    class_fields: Vec<BTreeMap<Sym, VarId>>,
    /// Every class's `this` and constructor `this`.
    this_vars: HashSet<VarId>,
    /// (ancestor's `this.f`, a subclass's `this.f`): what a base class stores in a field, its
    /// subclasses read (a base constructor sets `self.channel`, a subclass method reads it).
    /// Not the other way, or every subclass would see what its siblings store.
    pub field_links: Vec<(VarId, VarId)>,
}

fn add_prov(set: &mut Vec<Prov>, p: Prov) -> bool {
    let is_path = |x: &Prov| matches!(x, Prov::Api(_) | Prov::Unresolved(_));
    match set.binary_search(&p) {
        Ok(_) => false,
        Err(i) => {
            if set.len() >= MAX_PROVS {
                return false;
            }
            if is_path(&p) {
                let paths = set.iter().filter(|x| is_path(x)).count();
                if paths >= MAX_API_PROVS {
                    // Keep the smallest paths: drop the largest one if `p` sorts before it.
                    let Some(last) = set.iter().rposition(is_path) else {
                        return false;
                    };
                    if p > set[last] {
                        return false;
                    }
                    set.remove(last);
                    let i = set.binary_search(&p).unwrap_err();
                    set.insert(i, p);
                    return true;
                }
            }
            set.insert(i, p);
            true
        }
    }
}

fn union_into(dst: &mut Vec<Prov>, src: &[Prov]) -> bool {
    let mut changed = false;
    for p in src {
        changed |= add_prov(dst, p.clone());
    }
    changed
}

fn normalize_path(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        normalize_path(rel)
    } else {
        normalize_path(&format!("{dir}/{rel}"))
    }
}

const JS_EXTS: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];

impl Program {
    pub fn build(mut files: Vec<FileIr>, resolver: &Resolver) -> Program {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let file_index: BTreeMap<String, FileId> = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.clone(), i as FileId))
            .collect();
        let mut p = Program {
            files: Vec::new(),
            vars: Vec::new(),
            funcs: Vec::new(),
            classes: Vec::new(),
            stmts: Vec::new(),
            imports: Vec::new(),
            types: Vec::new(),
            annots: Vec::new(),
            syms: Interner::default(),
            exports: Vec::new(),
            prov: Vec::new(),
            fprov: HashMap::new(),
            calls: HashMap::new(),
            file_index,
            path_cache: HashMap::new(),
            call_cache: HashMap::new(),
            path_containers: HashMap::new(),
            this_home: HashMap::new(),
            var_containers: HashMap::new(),
            subclasses: Vec::new(),
            class_fields: Vec::new(),
            this_vars: HashSet::new(),
            field_links: Vec::new(),
        };
        let mut pending_reexports: Vec<(FileId, Vec<String>)> = Vec::new();
        for (fi, f) in files.into_iter().enumerate() {
            let fid = fi as FileId;
            let vo = p.vars.len() as u32;
            let fo = p.funcs.len() as u32;
            let co = p.classes.len() as u32;
            let io = p.imports.len() as u32;
            for v in &f.vars {
                p.vars.push(GVar {
                    name: v.name.clone(),
                    kind: v.kind,
                    func: v.func + fo,
                    file: fid,
                    pos: v.pos,
                });
            }
            for func in &f.funcs {
                p.funcs.push(GFunc {
                    file: fid,
                    name: func.name.clone(),
                    params: func
                        .params
                        .iter()
                        .map(|q| GParam {
                            var: q.var + vo,
                            name: q.name.clone(),
                            rest: q.rest,
                            kwrest: q.kwrest,
                            annot: q.annot.clone(),
                        })
                        .collect(),
                    ret: func.ret + vo,
                    parent: func.parent.map(|x| x + fo),
                    class: func.class.map(|x| x + co),
                    bound_self: func.bound_self,
                    ret_annot: func.ret_annot.clone(),
                    decorators: func.decorators.iter().map(|d| d + vo).collect(),
                    is_property: func.is_property,
                    pos: func.pos,
                    is_module: func.is_module,
                });
            }
            for c in &f.classes {
                p.classes.push(GClass {
                    file: fid,
                    name: c.name.clone(),
                    this: c.this + vo,
                    ctor_this: c.ctor_this.map(|t| t + vo),
                    methods: c.methods.iter().map(|(n, m)| (n.clone(), m + fo)).collect(),
                    bases: c.bases.iter().map(|b| b + vo).collect(),
                    pos: c.pos,
                });
            }
            for s in &f.stmts {
                let kind = match &s.kind {
                    StmtKind::Copy { dst, src } => GKind::Copy {
                        dst: dst + vo,
                        src: src + vo,
                    },
                    StmtKind::Lit { dst, value } => GKind::Lit {
                        dst: dst + vo,
                        value: value.clone(),
                    },
                    StmtKind::Load { dst, obj, field } => GKind::Load {
                        dst: dst + vo,
                        obj: obj + vo,
                        field: field.as_deref().map(|x| p.syms.intern(x)),
                    },
                    StmtKind::Store { obj, field, src } => GKind::Store {
                        obj: obj + vo,
                        field: field.as_deref().map(|x| p.syms.intern(x)),
                        src: src + vo,
                    },
                    StmtKind::Concat { dst, parts } => GKind::Concat {
                        dst: dst + vo,
                        parts: parts
                            .iter()
                            .map(|q| match q {
                                Part::Lit(s) => GPart::Lit(s.clone()),
                                Part::Var(v) => GPart::Var(v + vo),
                            })
                            .collect(),
                    },
                    StmtKind::Call {
                        dst,
                        callee,
                        args,
                        is_new,
                    } => GKind::Call {
                        dst: dst + vo,
                        callee: match callee {
                            Callee::Value(v) => GCallee::Value(v + vo),
                            Callee::Method { recv, name } => GCallee::Method {
                                recv: recv + vo,
                                name: p.syms.intern(name),
                            },
                            Callee::Dynamic => GCallee::Dynamic,
                        },
                        args: args
                            .iter()
                            .map(|a| GArg {
                                var: a.var + vo,
                                kind: match &a.kind {
                                    ArgKind::Positional => GArgKind::Positional,
                                    ArgKind::Keyword(k) => GArgKind::Keyword(k.clone()),
                                    ArgKind::Spread => GArgKind::Spread,
                                    ArgKind::KwSpread => GArgKind::KwSpread,
                                },
                            })
                            .collect(),
                        is_new: *is_new,
                    },
                    StmtKind::Return { src } => GKind::Return { src: src + vo },
                    StmtKind::FuncRef { dst, func } => GKind::FuncRef {
                        dst: dst + vo,
                        func: func + fo,
                    },
                    StmtKind::ClassRef { dst, class } => GKind::ClassRef {
                        dst: dst + vo,
                        class: class + co,
                    },
                    StmtKind::Global { dst, name } => GKind::Global {
                        dst: dst + vo,
                        name: name.clone(),
                    },
                    StmtKind::Import { import } => GKind::Import {
                        import: import + io,
                    },
                    StmtKind::TypeRef { dst, ty } => GKind::TypeRef {
                        dst: dst + vo,
                        ty: ty + vo,
                    },
                    StmtKind::Super { dst, class } => GKind::Super {
                        dst: dst + vo,
                        class: class + co,
                    },
                    StmtKind::Elements { dst, coll } => GKind::Elements {
                        dst: dst + vo,
                        coll: coll + vo,
                    },
                    StmtKind::ThisLoad {
                        dst,
                        obj,
                        field,
                        var,
                    } => GKind::ThisLoad {
                        dst: dst + vo,
                        obj: obj + vo,
                        field: p.syms.intern(field),
                        var: var + vo,
                    },
                    StmtKind::ThisStore {
                        obj,
                        field,
                        src,
                        var,
                    } => GKind::ThisStore {
                        obj: obj + vo,
                        field: p.syms.intern(field),
                        src: src + vo,
                        var: var + vo,
                    },
                };
                p.stmts.push(GStmt {
                    file: fid,
                    func: s.func + fo,
                    pos: s.pos,
                    text: s.text.clone(),
                    kind,
                });
            }
            for imp in &f.imports {
                p.imports.push(GImport {
                    file: fid,
                    var: imp.var + vo,
                    module: imp.module.clone(),
                    kind: imp.kind.clone(),
                    pos: imp.pos,
                    target: Target::Unresolved(String::new()),
                });
            }
            for t in &f.types {
                p.types.push(GType {
                    file: fid,
                    name: t.name.clone(),
                    fields: t.fields.clone(),
                    pos: t.pos,
                });
            }
            for (v, a) in &f.annots {
                p.annots.push((v + vo, a.clone()));
            }
            let mut exports = BTreeMap::new();
            for e in &f.exports {
                // The first export of a name wins, matching module evaluation order.
                exports.entry(e.name.clone()).or_insert(e.var + vo);
            }
            p.exports.push(exports);
            pending_reexports.push((fid, f.reexports.clone()));
            p.files.push(FileInfo {
                path: f.path.clone(),
                lang: f.lang,
                lines: f.lines,
                module_func: fo,
                notes: f.notes.clone(),
                star: Vec::new(),
                reexports: Vec::new(),
            });
        }
        // Resolve imports and re-exports.
        for i in 0..p.imports.len() {
            let (file, module) = (p.imports[i].file, p.imports[i].module.clone());
            p.imports[i].target = p.resolve(file, &module, resolver);
        }
        for (fid, specs) in pending_reexports {
            let targets: Vec<Target> = specs.iter().map(|s| p.resolve(fid, s, resolver)).collect();
            if p.files[fid as usize].lang == Lang::Python {
                p.files[fid as usize].star = targets;
            } else {
                p.files[fid as usize].reexports = targets;
            }
        }
        p.prov = vec![Vec::new(); p.vars.len()];
        p.solve();
        p
    }

    pub fn file_id(&self, path: &str) -> Option<FileId> {
        self.file_index.get(path).copied()
    }

    // ------------------------------------------------------------------ import resolution

    fn exists(&self, path: &str) -> Option<FileId> {
        self.file_index.get(path).copied()
    }

    fn js_candidates(&self, p: &str) -> Option<FileId> {
        if let Some(f) = self.exists(p) {
            return Some(f);
        }
        for (from, tos) in [
            (".js", &[".ts", ".tsx"][..]),
            (".mjs", &[".mts"][..]),
            (".cjs", &[".cts"][..]),
            (".jsx", &[".tsx"][..]),
        ] {
            if let Some(stem) = p.strip_suffix(from) {
                for to in tos {
                    if let Some(f) = self.exists(&format!("{stem}{to}")) {
                        return Some(f);
                    }
                }
            }
        }
        for ext in JS_EXTS {
            if let Some(f) = self.exists(&format!("{p}{ext}")) {
                return Some(f);
            }
        }
        for ext in JS_EXTS {
            if let Some(f) = self.exists(&join(p, &format!("index{ext}"))) {
                return Some(f);
            }
        }
        None
    }

    fn resolve(&self, from: FileId, spec: &str, resolver: &Resolver) -> Target {
        let file = &self.files[from as usize];
        match file.lang {
            Lang::Python => self.resolve_py(&file.path, spec),
            _ => {
                let dir = dir_of(&file.path);
                if spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".."
                {
                    return match self.js_candidates(&join(dir, spec)) {
                        Some(f) => Target::Local(f),
                        None => Target::Unresolved(spec.to_string()),
                    };
                }
                if let Some(aliases) = resolver.tsconfig_for(&file.path) {
                    for (pattern, targets) in &aliases.paths {
                        let captured = match pattern.split_once('*') {
                            Some((pre, post)) => spec
                                .strip_prefix(pre)
                                .and_then(|r| r.strip_suffix(post))
                                .map(|s| s.to_string()),
                            None => (spec == pattern).then(String::new),
                        };
                        if let Some(c) = captured {
                            for t in targets {
                                let candidate = normalize_path(&t.replacen('*', &c, 1));
                                if let Some(f) = self.js_candidates(&candidate) {
                                    return Target::Local(f);
                                }
                            }
                            // An alias that names no scanned file may still be a workspace
                            // package or an external module.
                        }
                    }
                    if let Some(base) = &aliases.base_url
                        && let Some(f) = self.js_candidates(&join(base, spec))
                    {
                        return Target::Local(f);
                    }
                }
                if let Some(candidates) = resolver.package_candidates(spec) {
                    for c in candidates {
                        if let Some(f) = self.js_candidates(&c) {
                            return Target::Local(f);
                        }
                    }
                    return Target::Unresolved(spec.to_string());
                }
                Target::External(spec.strip_prefix("node:").unwrap_or(spec).to_string())
            }
        }
    }

    fn resolve_py(&self, from: &str, module: &str) -> Target {
        let dots = module.chars().take_while(|&c| c == '.').count();
        if dots > 0 {
            let mut base = dir_of(from).to_string();
            for _ in 1..dots {
                base = dir_of(&base).to_string();
            }
            let rest = &module[dots..];
            let path = if rest.is_empty() {
                base.clone()
            } else {
                join(&base, &rest.replace('.', "/"))
            };
            if let Some(f) = self.exists(&format!("{path}.py")) {
                return Target::Local(f);
            }
            if let Some(f) = self.exists(&join(&path, "__init__.py")) {
                return Target::Local(f);
            }
            if self.is_package_dir(&path) {
                return Target::Package(path);
            }
            return Target::Unresolved(module.to_string());
        }
        let rel = module.replace('.', "/");
        let mut best: Option<(usize, &str, FileId)> = None;
        for (path, &id) in &self.file_index {
            let stem = if let Some(s) = path.strip_suffix("/__init__.py") {
                s
            } else if let Some(s) = path.strip_suffix(".py") {
                s
            } else {
                continue;
            };
            if stem == rel || stem.ends_with(&format!("/{rel}")) {
                let key = (stem.len(), path.as_str());
                if best.is_none_or(|(l, p, _)| key < (l, p)) {
                    best = Some((key.0, key.1, id));
                }
            }
        }
        if let Some((_, _, id)) = best {
            return Target::Local(id);
        }
        // A namespace package: a directory of scanned Python files with no `__init__.py`.
        let mut dirs: Vec<&str> = Vec::new();
        for path in self.file_index.keys() {
            if !path.ends_with(".py") {
                continue;
            }
            let d = dir_of(path);
            if d == rel || d.ends_with(&format!("/{rel}")) {
                dirs.push(d);
            }
        }
        dirs.sort_by_key(|d| (d.len(), *d));
        if let Some(d) = dirs.first() {
            return Target::Package(d.to_string());
        }
        Target::External(module.to_string())
    }

    fn is_package_dir(&self, dir: &str) -> bool {
        let prefix = format!("{dir}/");
        self.file_index
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(p, _)| p.starts_with(&prefix) && p.ends_with(".py"))
    }

    /// A Python submodule `name` of a package directory.
    fn submodule(&self, dir: &str, name: &str) -> Option<Prov> {
        let path = join(dir, name);
        if let Some(f) = self.exists(&format!("{path}.py")) {
            return Some(Prov::Module(f));
        }
        if let Some(f) = self.exists(&join(&path, "__init__.py")) {
            return Some(Prov::Module(f));
        }
        None
    }

    /// The variable a module exports under `name`, following re-exports.
    pub fn export_var(&self, file: FileId, name: &str) -> Option<VarId> {
        let mut seen = BTreeSet::new();
        self.export_var_inner(file, name, &mut seen)
    }

    fn export_var_inner(
        &self,
        file: FileId,
        name: &str,
        seen: &mut BTreeSet<FileId>,
    ) -> Option<VarId> {
        if !seen.insert(file) {
            return None;
        }
        if let Some(&v) = self.exports[file as usize].get(name) {
            return Some(v);
        }
        for t in &self.files[file as usize].reexports {
            if let Target::Local(f) = t
                && let Some(v) = self.export_var_inner(*f, name, seen)
            {
                return Some(v);
            }
        }
        for t in &self.files[file as usize].star {
            if let Target::Local(f) = t
                && let Some(v) = self.export_var_inner(*f, name, seen)
            {
                return Some(v);
            }
        }
        None
    }

    // ------------------------------------------------------------------ API paths

    fn is_python(&self, file: FileId) -> bool {
        self.files[file as usize].lang == Lang::Python
    }

    /// Extend an API path by a member: `m:` + `a` → `m:a`, `m:a` + `b` → `m:a.b`.
    pub fn api_member(&mut self, base: Sym, member: &str) -> Option<Sym> {
        let key = (base, member.to_string());
        if let Some(r) = self.path_cache.get(&key) {
            return *r;
        }
        let r = self.api_member_uncached(base, member);
        self.path_cache.insert(key, r);
        r
    }

    fn api_member_uncached(&mut self, base: Sym, member: &str) -> Option<Sym> {
        let s = self.syms.str(base);
        if s.matches('.').count() + 1 >= MAX_PATH_SEGMENTS {
            return None;
        }
        let out = if s.ends_with(':') {
            format!("{s}{member}")
        } else if matches!(s, "window" | "globalThis" | "self" | "global") {
            member.to_string()
        } else {
            format!("{s}.{member}")
        };
        Some(self.syms.intern(&out))
    }

    pub fn api_call(&mut self, base: Sym) -> Option<Sym> {
        if let Some(r) = self.call_cache.get(&base) {
            return *r;
        }
        let r = self.api_call_uncached(base);
        self.call_cache.insert(base, r);
        r
    }

    fn api_call_uncached(&mut self, base: Sym) -> Option<Sym> {
        let s = self.syms.str(base);
        if s.matches("()").count() >= MAX_PATH_SEGMENTS / 2 {
            return None;
        }
        let out = format!("{s}()");
        Some(self.syms.intern(&out))
    }

    fn import_prov(&mut self, imp: &GImport) -> Vec<Prov> {
        let py = self.is_python(imp.file);
        match (&imp.target, &imp.kind) {
            (Target::Local(f), ImportKind::Namespace) => vec![Prov::Module(*f)],
            (Target::Local(f), ImportKind::Default) => match self.export_var(*f, "default") {
                Some(v) => self.prov[v as usize].clone(),
                None => vec![Prov::Module(*f)],
            },
            (Target::Local(f), ImportKind::Named(n)) => self.member_of_module(*f, n),
            (Target::Package(d), ImportKind::Named(n)) => {
                let d = d.clone();
                self.submodule(&d, n).into_iter().collect()
            }
            (Target::Package(d), _) => {
                let s = self.syms.intern(d);
                vec![Prov::Package(s)]
            }
            (Target::External(m), ImportKind::Named(n)) => {
                let s = if py {
                    format!("{m}.{n}")
                } else {
                    format!("{m}:{n}")
                };
                vec![Prov::Api(self.syms.intern(&s))]
            }
            (Target::External(m), _) => {
                let s = if py { m.clone() } else { format!("{m}:") };
                vec![Prov::Api(self.syms.intern(&s))]
            }
            (Target::Unresolved(m), kind) => {
                let s = match kind {
                    ImportKind::Named(n) => format!("{m}:{n}"),
                    _ => m.clone(),
                };
                vec![Prov::Unresolved(self.syms.intern(&s))]
            }
        }
    }

    fn member_of_module(&mut self, f: FileId, name: &str) -> Vec<Prov> {
        if let Some(v) = self.export_var(f, name) {
            return self.prov[v as usize].clone();
        }
        if let Some(d) = self.export_var(f, "default")
            && let Some(sym) = self.syms.get(name)
            && let Some(ps) = self.fget(d, sym)
        {
            return ps.clone();
        }
        if self.is_python(f) {
            let path = self.files[f as usize].path.clone();
            if path.ends_with("__init__.py")
                && let Some(p) = self.submodule(dir_of(&path), name)
            {
                return vec![p];
            }
        }
        Vec::new()
    }

    /// The provenance of `obj.name` for every provenance of `obj`.
    fn member(&mut self, obj: VarId, name: Sym) -> Vec<Prov> {
        let mut out = Vec::new();
        if let Some(ps) = self.fget(obj, name) {
            out.extend(ps.iter().cloned());
        }
        let field = self.syms.str(name).to_string();
        for p in self.prov[obj as usize].clone() {
            match p {
                Prov::Api(s) => {
                    if let Some(m) = self.api_member(s, &field) {
                        out.push(Prov::Api(m));
                    }
                }
                Prov::Unresolved(s) => {
                    if let Some(m) = self.api_member(s, &field) {
                        out.push(Prov::Unresolved(m));
                    }
                }
                Prov::Module(f) => out.extend(self.member_of_module(f, &field)),
                Prov::Package(d) => {
                    let d = self.syms.str(d).to_string();
                    out.extend(self.submodule(&d, &field));
                }
                Prov::Class(c) | Prov::Instance(c) => {
                    let instance = matches!(p, Prov::Instance(_));
                    let on_this = self.is_this(obj);
                    out.extend(self.class_member(c, instance && !on_this, name));
                }
                Prov::Func(f) => {
                    if matches!(field.as_str(), "bind" | "call" | "apply") {
                        out.push(Prov::Func(f));
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Whether `v` is a class's own instance as its methods (or its constructor) see it: `this`,
    /// Python's `self`.
    fn is_this(&self, v: VarId) -> bool {
        self.this_vars.contains(&v)
    }

    /// `name` on a class (`C.name`, `super.name`, `this.name`) or on an instance of it held
    /// elsewhere (`obj.name`): the method it defines or inherits, a field of its instances, or a
    /// member of an external base class. On an instance held elsewhere the call is virtual: a
    /// subclass's override may run instead (`transport.notify()` on a value typed `Transport`).
    /// On `this` it is not: a base class method's one summary serves every subclass, so
    /// dispatching `this.run()` to every override would hand each subclass's data to all the
    /// others.
    fn class_member(&mut self, c: ClassId, instance: bool, name: Sym) -> Vec<Prov> {
        let field = self.syms.str(name).to_string();
        let mut out = Vec::new();
        let lineage = self.lineage(c);
        let mut methods: Vec<FuncId> = self.find_method(&lineage, &field).into_iter().collect();
        let mut classes = lineage.clone();
        if instance {
            for d in self.descendants(c) {
                if let Some(m) = self.own_method(d, &field) {
                    methods.push(m);
                }
                classes.push(d);
            }
        }
        for m in methods {
            if self.funcs[m as usize].is_property {
                // Reading a property runs it: the value is what it returns.
                let ret = self.funcs[m as usize].ret;
                out.extend(self.prov[ret as usize].iter().cloned());
            } else {
                out.push(Prov::Func(m));
            }
        }
        for k in classes {
            if let Some(ps) = self.fget(self.classes[k as usize].this, name) {
                out.extend(ps.iter().cloned());
            }
        }
        if out.is_empty() {
            for s in self.external_bases(&lineage) {
                let base = if instance { self.api_call(s) } else { Some(s) };
                if let Some(m) = base.and_then(|b| self.api_member(b, &field)) {
                    out.push(Prov::Api(m));
                }
            }
        }
        out
    }

    fn own_method(&self, c: ClassId, name: &str) -> Option<FuncId> {
        self.classes[c as usize]
            .methods
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| *f)
    }

    /// The first class of a lineage that defines `name`.
    fn find_method(&self, lineage: &[ClassId], name: &str) -> Option<FuncId> {
        lineage.iter().find_map(|&k| self.own_method(k, name))
    }

    /// The local classes `c`'s base expressions name, in order.
    fn local_bases(&self, c: ClassId) -> Vec<ClassId> {
        let mut out = Vec::new();
        for &b in &self.classes[c as usize].bases {
            for p in &self.prov[b as usize] {
                if let Prov::Class(x) = p
                    && *x != c
                    && !out.contains(x)
                {
                    out.push(*x);
                }
            }
        }
        out
    }

    /// `c` and its local ancestors, nearest first (breadth-first, a stand-in for the method
    /// resolution order).
    pub fn lineage(&self, c: ClassId) -> Vec<ClassId> {
        let mut out = vec![c];
        let mut i = 0;
        while i < out.len() && out.len() < MAX_LINEAGE {
            for b in self.local_bases(out[i]) {
                if !out.contains(&b) {
                    out.push(b);
                }
            }
            i += 1;
        }
        out
    }

    /// Every local class that inherits from `c`, directly or not.
    pub fn descendants(&self, c: ClassId) -> Vec<ClassId> {
        let mut out: Vec<ClassId> = Vec::new();
        let mut stack = vec![c];
        while let Some(k) = stack.pop() {
            for &d in self.subclasses.get(k as usize).into_iter().flatten() {
                if d != c && !out.contains(&d) && out.len() < MAX_DESCENDANTS {
                    out.push(d);
                    stack.push(d);
                }
            }
        }
        out.sort();
        out
    }

    /// API paths of the external classes a lineage extends (`class Analytics extends PostHog`).
    fn external_bases(&self, lineage: &[ClassId]) -> Vec<Sym> {
        let mut out = Vec::new();
        for &k in lineage {
            for &b in &self.classes[k as usize].bases {
                for p in &self.prov[b as usize] {
                    if let Prov::Api(s) = p
                        && !out.contains(s)
                    {
                        out.push(*s);
                    }
                }
            }
        }
        out
    }

    /// Recompute subclasses and linked fields from the bases' provenance. True if either changed.
    fn refresh_hierarchy(&mut self) -> bool {
        if self.class_fields.len() != self.classes.len() {
            let this_of: HashMap<VarId, ClassId> = self
                .classes
                .iter()
                .enumerate()
                .map(|(i, c)| (c.this, i as ClassId))
                .collect();
            let mut fields = vec![BTreeMap::new(); self.classes.len()];
            for s in &self.stmts {
                let (GKind::ThisLoad {
                    obj, field, var, ..
                }
                | GKind::ThisStore {
                    obj, field, var, ..
                }) = s.kind
                else {
                    continue;
                };
                if let Some(&c) = this_of.get(&obj) {
                    fields[c as usize].entry(field).or_insert(var);
                }
            }
            self.class_fields = fields;
            self.this_vars = self
                .classes
                .iter()
                .flat_map(|c| [Some(c.this), c.ctor_this])
                .flatten()
                .collect();
        }
        let mut subclasses = vec![Vec::new(); self.classes.len()];
        let mut links = Vec::new();
        for c in 0..self.classes.len() as ClassId {
            for b in self.local_bases(c) {
                subclasses[b as usize].push(c);
            }
            let lineage = self.lineage(c);
            for &a in &lineage[1..] {
                for (f, &v) in &self.class_fields[c as usize] {
                    if let Some(&w) = self.class_fields[a as usize].get(f) {
                        links.push((w, v));
                    }
                }
            }
        }
        links.sort();
        links.dedup();
        let changed = subclasses != self.subclasses || links != self.field_links;
        self.subclasses = subclasses;
        self.field_links = links;
        changed
    }

    /// A subclass's field sees what its ancestor stores in the same field.
    fn link_fields(&mut self) -> bool {
        let mut changed = false;
        for (x, y) in self.field_links.clone() {
            let ps = self.prov[x as usize].clone();
            changed |= self.set(y, &ps);
            for (k, fps) in self.fields_of(x) {
                changed |= self.fadd(y, k, &fps);
            }
        }
        changed
    }

    fn callee_provs(&mut self, callee: &GCallee) -> Vec<Prov> {
        match callee {
            GCallee::Value(v) => self.prov[*v as usize].clone(),
            GCallee::Method { recv, name } => self.member(*recv, *name),
            GCallee::Dynamic => Vec::new(),
        }
    }

    /// The constructor `new C()` runs: its own, or the nearest ancestor's.
    fn ctor(&self, c: ClassId) -> Option<FuncId> {
        self.lineage(c).into_iter().find_map(|k| {
            self.own_method(k, "constructor")
                .or_else(|| self.own_method(k, "__init__"))
        })
    }

    // ------------------------------------------------------------------ provenance fixpoint

    /// Give a variable a provenance (a framework's response object) and re-solve.
    pub fn seed_prov(&mut self, var: VarId, p: Prov) {
        add_prov(&mut self.prov[var as usize], p);
    }

    pub fn solve(&mut self) {
        if self.this_home.is_empty() {
            for s in &self.stmts {
                if let GKind::ThisLoad { dst, var, .. } = s.kind {
                    self.this_home.insert(dst, var);
                }
            }
        }
        let mut rounds = 0;
        loop {
            rounds += 1;
            let mut changed = self.refresh_hierarchy();
            for i in 0..self.stmts.len() {
                changed |= self.step(i);
            }
            changed |= self.link_fields();
            if !changed || rounds > 64 {
                if std::env::var_os("PIIFLOW_DEBUG_IR").is_some() {
                    eprintln!("provenance: {rounds} round(s), converged: {}", !changed);
                }
                break;
            }
        }
        self.resolve_calls();
    }

    /// Functions passed to an external API get parameters that are "what that API hands its
    /// callback": `prisma.$transaction(tx => ...)` gives `tx` the path
    /// `@prisma/client:PrismaClient().$transaction.<cb0>`, so calls on it resolve against the
    /// catalogue like any other call into that module.
    fn callback_params(&mut self, api: Sym, args: &[GArg]) -> bool {
        let mut changed = false;
        for a in args {
            // A function argument, or a function in a field of an options or props object
            // (`<Controller render={...} />`, `fastify.route({ handler })`).
            let mut funcs: Vec<(String, FuncId)> = Vec::new();
            for p in &self.prov[a.var as usize] {
                if let Prov::Func(f) = p {
                    funcs.push(("cb".to_string(), *f));
                }
            }
            for (k, ps) in self.fields_of(a.var) {
                for p in ps {
                    if let Prov::Func(f) = p {
                        funcs.push((self.syms.str(k).to_string(), f));
                    }
                }
            }
            for (label, f) in funcs {
                let params: Vec<VarId> = self.funcs[f as usize]
                    .params
                    .iter()
                    .map(|q| q.var)
                    .collect();
                for (j, var) in params.into_iter().enumerate() {
                    if let Some(path) = self.api_member(api, &format!("<{label}{j}>")) {
                        changed |= add_prov(&mut self.prov[var as usize], Prov::Api(path));
                    }
                }
            }
        }
        changed
    }

    /// A local call's arguments give its parameters their provenance (context-insensitive), so
    /// a callback passed in by the caller resolves inside the callee: `onChange(v)` in a
    /// component reaches the handler the parent passed.
    fn bind_formals(&mut self, f: FuncId, bound: bool, args: &[GArg]) -> bool {
        let params: Vec<GParam> = self.funcs[f as usize]
            .params
            .iter()
            .skip(usize::from(bound))
            .cloned()
            .collect();
        let mut changed = false;
        let mut pos = 0usize;
        for a in args {
            let targets: Vec<VarId> = match &a.kind {
                GArgKind::Positional => {
                    let t = match params.get(pos) {
                        Some(q) if !q.rest && !q.kwrest => vec![q.var],
                        _ => params.iter().filter(|q| q.rest).map(|q| q.var).collect(),
                    };
                    pos += 1;
                    t
                }
                GArgKind::Keyword(k) => params
                    .iter()
                    .filter(|q| &q.name == k || q.kwrest)
                    .map(|q| q.var)
                    .take(1)
                    .collect(),
                GArgKind::Spread | GArgKind::KwSpread => params.iter().map(|q| q.var).collect(),
            };
            for t in targets {
                let ps = self.prov[a.var as usize].clone();
                changed |= union_into(&mut self.prov[t as usize], &ps);
                for (k, fps) in self.fields_of(a.var) {
                    changed |= self.fadd(t, k, &fps);
                }
            }
        }
        changed
    }

    pub fn fget(&self, v: VarId, k: Sym) -> Option<&Vec<Prov>> {
        self.fprov.get(&v).and_then(|m| m.get(&k))
    }

    fn fadd(&mut self, v: VarId, k: Sym, ps: &[Prov]) -> bool {
        if ps.is_empty() {
            return false;
        }
        union_into(self.fprov.entry(v).or_default().entry(k).or_default(), ps)
    }

    /// What a container holds: the provenance put into it through `set`/`push` (on the
    /// variable, or on every variable reaching it through the same API path), with the held
    /// values' fields added to `dst`.
    fn elements(&mut self, coll: VarId, dst: VarId) -> (Vec<Prov>, bool) {
        let elem = self.syms.intern("[]");
        let mut out: Vec<Prov> = self.fget(coll, elem).cloned().unwrap_or_default();
        let mut changed = false;
        let home = self.this_home.get(&coll).copied().unwrap_or(coll);
        let held: Vec<VarId> = self
            .var_containers
            .get(&home)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        for v in held {
            out.extend(self.prov[v as usize].iter().cloned());
            for (k, fps) in self.fields_of(v) {
                changed |= self.fadd(dst, k, &fps);
            }
        }
        let paths: Vec<Sym> = self.prov[coll as usize]
            .iter()
            .filter_map(|p| match p {
                Prov::Api(s) if !self.syms.str(*s).ends_with(')') => Some(*s),
                _ => None,
            })
            .collect();
        for p in paths {
            let vars: Vec<VarId> = self
                .path_containers
                .get(&p)
                .map(|v| v.iter().copied().collect())
                .unwrap_or_default();
            for v in vars {
                out.extend(self.prov[v as usize].iter().cloned());
                for (k, fps) in self.fields_of(v) {
                    changed |= self.fadd(dst, k, &fps);
                }
            }
        }
        (out, changed)
    }

    fn fields_of(&self, v: VarId) -> Vec<(Sym, Vec<Prov>)> {
        match self.fprov.get(&v) {
            Some(m) => m.iter().map(|(k, ps)| (*k, ps.clone())).collect(),
            None => Vec::new(),
        }
    }

    fn set(&mut self, var: VarId, ps: &[Prov]) -> bool {
        union_into(&mut self.prov[var as usize], ps)
    }

    fn step(&mut self, i: usize) -> bool {
        // This runs for every statement in every round: take the statement out rather than
        // clone it, and put it back.
        let kind = std::mem::replace(&mut self.stmts[i].kind, GKind::Return { src: 0 });
        let changed = self.step_kind(i, &kind);
        self.stmts[i].kind = kind;
        changed
    }

    fn step_kind(&mut self, i: usize, kind: &GKind) -> bool {
        let file = self.stmts[i].file;
        match *kind {
            GKind::Copy { dst, src } => {
                let ps = self.prov[src as usize].clone();
                let mut changed = self.set(dst, &ps);
                for (k, ps) in self.fields_of(src) {
                    changed |= self.fadd(dst, k, &ps);
                }
                changed
            }
            GKind::Load {
                dst,
                obj,
                field: Some(k),
            } => {
                let ps = self.member(obj, k);
                self.set(dst, &ps)
            }
            GKind::Store {
                obj,
                field: Some(k),
                src,
            } => {
                let ps = self.prov[src as usize].clone();
                if ps.is_empty() {
                    return false;
                }
                self.fadd(obj, k, &ps)
            }
            // Computed keys: `{ [lang]: i18n }` and `instances[lang]` meet in the container's
            // elements; a computed read also sees every named field.
            GKind::Store {
                obj,
                field: None,
                src,
            } => {
                let ps = self.prov[src as usize].clone();
                let elem = self.syms.intern("[]");
                self.fadd(obj, elem, &ps)
            }
            GKind::Load {
                dst,
                obj,
                field: None,
            } => {
                let mut ps: Vec<Prov> = Vec::new();
                for (_, fps) in self.fields_of(obj) {
                    ps.extend(fps);
                }
                self.set(dst, &ps)
            }
            GKind::Call {
                dst,
                ref callee,
                ref args,
                is_new,
            } => {
                let mut out = Vec::new();
                let mut changed = false;
                for p in self.callee_provs(callee) {
                    match p {
                        Prov::Api(s) => {
                            if let Some(c) = self.api_call(s) {
                                out.push(Prov::Api(c));
                            }
                            changed |= self.callback_params(s, args);
                        }
                        Prov::Unresolved(s) => {
                            if let Some(c) = self.api_call(s) {
                                out.push(Prov::Unresolved(c));
                            }
                        }
                        Prov::Func(f) => {
                            let ret = self.funcs[f as usize].ret;
                            out.extend(self.prov[ret as usize].iter().cloned());
                            // `return { client: x }`: the result carries the object's fields.
                            for (k, ps) in self.fields_of(ret) {
                                changed |= self.fadd(dst, k, &ps);
                            }
                            let bound = self.funcs[f as usize].bound_self
                                && self.funcs[f as usize].class.is_some();
                            changed |= self.bind_formals(f, bound, args);
                        }
                        Prov::Class(c) => {
                            out.push(Prov::Instance(c));
                            if let Some(ctor) = self.ctor(c) {
                                let bound = self.funcs[ctor as usize].bound_self;
                                changed |= self.bind_formals(ctor, bound, args);
                            }
                        }
                        _ => {}
                    }
                }
                let _ = is_new;
                // A method on a plain value with a callback (`xs.reduce(f, {})`, `xs.map(f)`):
                // the result is what the callback returns.
                if let GCallee::Method { recv, .. } = callee
                    && self.prov[*recv as usize].is_empty()
                {
                    let funcs: Vec<FuncId> = args
                        .iter()
                        .flat_map(|a| self.prov[a.var as usize].iter())
                        .filter_map(|p| match p {
                            Prov::Func(f) => Some(*f),
                            _ => None,
                        })
                        .collect();
                    for f in funcs {
                        let ret = self.funcs[f as usize].ret;
                        out.extend(self.prov[ret as usize].iter().cloned());
                        for (k, ps) in self.fields_of(ret) {
                            changed |= self.fadd(dst, k, &ps);
                        }
                    }
                }
                // Containers: what goes in through `set`/`push` comes out of `get`/`pop`
                // (`globalThis.cache.set(k, new Client())` … `cache.get(k).send()`).
                let dst_ref = &dst;
                if let GCallee::Method { recv, name } = callee {
                    let elem = self.syms.intern("[]");
                    // A container reached through an API path (`globalThis.cache`) is the same
                    // container at every access, whatever temporary holds it. A path ending in
                    // a call (`Map()`, `client.cache()`) is a new value at each call, not one
                    // location: keying on it would make every `new Map()` one container.
                    let paths: Vec<Sym> = self.prov[*recv as usize]
                        .iter()
                        .filter_map(|p| match p {
                            Prov::Api(s) if !self.syms.str(*s).ends_with(')') => Some(*s),
                            _ => None,
                        })
                        .collect();
                    match self.syms.str(*name) {
                        "set" | "add" | "push" | "unshift" | "append" | "setdefault" => {
                            if let Some(last) = args.last() {
                                let ps = self.prov[last.var as usize].clone();
                                changed |= self.fadd(*recv, elem, &ps);
                                let home = self.this_home.get(recv).copied().unwrap_or(*recv);
                                if home != *recv {
                                    changed |= self.fadd(home, elem, &ps);
                                }
                                changed |= self
                                    .var_containers
                                    .entry(home)
                                    .or_default()
                                    .insert(last.var);
                                for p in paths {
                                    changed |=
                                        self.path_containers.entry(p).or_default().insert(last.var);
                                }
                            }
                        }
                        "get" | "pop" | "shift" | "at" | "find" | "first" | "last"
                        | "getOrThrow" => {
                            let (ps, c) = self.elements(*recv, *dst_ref);
                            out.extend(ps);
                            changed |= c;
                        }
                        // A view of the same elements: iterating it iterates the container.
                        "values" | "entries" => {
                            if let Some(ps) = self.fget(*recv, elem).cloned() {
                                changed |= self.fadd(*dst_ref, elem, &ps);
                            }
                            let (ps, c) = self.elements(*recv, *dst_ref);
                            changed |= c;
                            changed |= self.fadd(*dst_ref, elem, &ps);
                        }
                        _ => {}
                    }
                }
                changed | self.set(dst, &out)
            }
            GKind::Return { src } => {
                let ret = self.funcs[self.stmts[i].func as usize].ret;
                let ps = self.prov[src as usize].clone();
                let mut changed = self.set(ret, &ps);
                for (k, fps) in self.fields_of(src) {
                    changed |= self.fadd(ret, k, &fps);
                }
                changed
            }
            GKind::ThisLoad {
                dst,
                obj,
                field,
                var,
            } => {
                let mut ps = self.member(obj, field);
                ps.extend(self.prov[var as usize].iter().cloned());
                let mut changed = self.set(dst, &ps);
                for (k, fps) in self.fields_of(var) {
                    changed |= self.fadd(dst, k, &fps);
                }
                changed
            }
            GKind::ThisStore {
                obj,
                field,
                src,
                var,
            } => {
                let ps = self.prov[src as usize].clone();
                let mut changed = self.set(var, &ps);
                for (k, fps) in self.fields_of(src) {
                    changed |= self.fadd(var, k, &fps);
                }
                changed | self.fadd(obj, field, &ps)
            }
            GKind::Elements { dst, coll } => {
                let (ps, changed) = self.elements(coll, dst);
                changed | self.set(dst, &ps)
            }
            GKind::TypeRef { dst, ty } => {
                let ps: Vec<Prov> = self.prov[ty as usize]
                    .iter()
                    .filter_map(|p| match p {
                        Prov::Api(s) => Some(Prov::Api(*s)),
                        Prov::Class(c) => Some(Prov::Instance(*c)),
                        _ => None,
                    })
                    .collect();
                self.set(dst, &ps)
            }
            GKind::Super { dst, class } => {
                let mut ps: Vec<Prov> = self
                    .local_bases(class)
                    .into_iter()
                    .map(Prov::Class)
                    .collect();
                for s in self.external_bases(&[class]) {
                    if let Some(i) = self.api_call(s) {
                        ps.push(Prov::Api(i));
                    }
                }
                self.set(dst, &ps)
            }
            GKind::FuncRef { dst, func } => self.set(dst, &[Prov::Func(func)]),
            GKind::ClassRef { dst, class } => self.set(dst, &[Prov::Class(class)]),
            GKind::Global { dst, ref name } => {
                let ps = self.global_prov(file, name);
                self.set(dst, &ps)
            }
            GKind::Import { import } => {
                let imp = self.imports[import as usize].clone();
                let ps = self.import_prov(&imp);
                let mut changed = self.set(imp.var, &ps);
                // A default import of an object literal carries its fields.
                if let (Target::Local(f), ImportKind::Default) = (&imp.target, &imp.kind)
                    && let Some(v) = self.export_var(*f, "default")
                {
                    for (k, ps) in self.fields_of(v) {
                        changed |= self.fadd(imp.var, k, &ps);
                    }
                }
                changed
            }
            _ => false,
        }
    }

    fn global_prov(&mut self, file: FileId, name: &str) -> Vec<Prov> {
        if self.is_python(file) {
            for t in self.files[file as usize].star.clone() {
                match t {
                    Target::Local(f) => {
                        if let Some(v) = self.export_var(f, name) {
                            return self.prov[v as usize].clone();
                        }
                    }
                    Target::External(m) => {
                        let _ = m;
                    }
                    _ => {}
                }
            }
            vec![Prov::Api(self.syms.intern(&format!("builtins.{name}")))]
        } else {
            vec![Prov::Api(self.syms.intern(name))]
        }
    }

    fn resolve_calls(&mut self) {
        let mut calls = HashMap::new();
        for i in 0..self.stmts.len() {
            let GKind::Call { callee, is_new, .. } = self.stmts[i].kind.clone() else {
                continue;
            };
            let mut t = CallTargets::default();
            let provs = self.callee_provs(&callee);
            let recv_provs = match &callee {
                GCallee::Method { recv, .. } => self.prov[*recv as usize].clone(),
                _ => Vec::new(),
            };
            for p in &provs {
                match p {
                    Prov::Api(s) => t.apis.push(*s),
                    Prov::Unresolved(s) => t.unresolved.push(*s),
                    Prov::Func(f) => {
                        let func = &self.funcs[*f as usize];
                        // A method reached through an instance or class is called bound; a
                        // function stored in a field and called is not.
                        let bound = func.bound_self && func.class.is_some();
                        t.funcs.push((*f, bound));
                    }
                    Prov::Class(c) => {
                        t.classes.push(*c);
                        if let Some(ctor) = self.ctor(*c) {
                            let bound = self.funcs[ctor as usize].bound_self;
                            t.funcs.push((ctor, bound));
                        }
                    }
                    _ => {}
                }
            }
            let _ = is_new;
            if let GCallee::Method { recv, name } = &callee
                && self.is_this(*recv)
            {
                let name = self.syms.str(*name).to_string();
                let mut overrides = BTreeSet::new();
                for p in &recv_provs {
                    if let Prov::Instance(c) = p {
                        for d in self.descendants(*c) {
                            if self.own_method(d, &name).is_some() {
                                overrides.insert(d);
                            }
                        }
                    }
                }
                t.overridden = overrides.len();
            }
            if provs.is_empty() {
                match &callee {
                    GCallee::Method { recv, name } if recv_provs.is_empty() => {
                        t.plain_method = Some((*recv, *name));
                    }
                    _ => t.dynamic = true,
                }
            }
            t.funcs.sort();
            t.funcs.dedup();
            t.classes.sort();
            t.classes.dedup();
            t.apis
                .sort_by(|a, b| self.syms.str(*a).cmp(self.syms.str(*b)));
            t.apis.dedup();
            t.unresolved
                .sort_by(|a, b| self.syms.str(*a).cmp(self.syms.str(*b)));
            t.unresolved.dedup();
            calls.insert(i as u32, t);
        }
        self.calls = calls;
    }

    /// Variables shared across functions: module-level bindings and class instances.
    pub fn is_shared(&self, v: VarId) -> bool {
        let var = &self.vars[v as usize];
        match var.kind {
            VarKind::This => true,
            VarKind::Local => self.funcs[var.func as usize].is_module,
            _ => false,
        }
    }

    pub fn api_str(&self, s: Sym) -> &str {
        self.syms.str(s)
    }

    /// Development aid (`PIIFLOW_DEBUG_IR=1`): every statement with the provenance of what it
    /// defines, and every import's resolution.
    pub fn dump(&self, w: &mut dyn std::io::Write) {
        for imp in &self.imports {
            let _ = writeln!(
                w,
                "import {}:{} {:?} {:?} -> {:?} (v{})",
                self.files[imp.file as usize].path,
                imp.pos.line,
                imp.module,
                imp.kind,
                imp.target,
                imp.var
            );
        }
        for (i, s) in self.stmts.iter().enumerate() {
            let _ = writeln!(
                w,
                "#{i} {}:{} [{}] {:?} | {}",
                self.files[s.file as usize].path,
                s.pos.line,
                self.funcs[s.func as usize].name,
                s.kind,
                s.text
            );
            let dst = match &s.kind {
                GKind::Copy { dst, .. }
                | GKind::Lit { dst, .. }
                | GKind::Load { dst, .. }
                | GKind::Concat { dst, .. }
                | GKind::Call { dst, .. }
                | GKind::ThisLoad { dst, .. }
                | GKind::FuncRef { dst, .. }
                | GKind::ClassRef { dst, .. }
                | GKind::Global { dst, .. } => Some(*dst),
                _ => None,
            };
            if let Some(d) = dst
                && !self.prov[d as usize].is_empty()
            {
                let ps: Vec<String> = self.prov[d as usize]
                    .iter()
                    .map(|p| match p {
                        Prov::Api(s) => format!("api:{}", self.syms.str(*s)),
                        Prov::Unresolved(s) => format!("unresolved:{}", self.syms.str(*s)),
                        other => format!("{other:?}"),
                    })
                    .collect();
                let _ = writeln!(w, "     v{d} = {}", ps.join(", "));
            }
            if let Some(t) = self.calls.get(&(i as u32)) {
                let _ = writeln!(
                    w,
                    "     call -> funcs {:?} apis {:?} dynamic {} plain {:?}",
                    t.funcs,
                    t.apis.iter().map(|a| self.syms.str(*a)).collect::<Vec<_>>(),
                    t.dynamic,
                    t.plain_method
                );
            }
        }
    }
}
