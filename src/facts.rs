//! Flow facts: the graph the engines run on.
//!
//! The program's statements become edges between `(variable, field token)` nodes; the
//! catalogue turns calls into sink hits, sanitiser edges, propagation, or coverage gaps; the
//! classification table, type declarations, framework models and config turn reads and
//! parameters into seeds. Nothing here decides whether data reaches anything: that is the
//! engines' job, and both engines read exactly these facts.

mod heap;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::catalogue::{ArgSel, Catalogue, Flow, ParamRule, SinkClass, SourceKind};
use crate::classify::{Classifier, Place, UNKNOWN};
use crate::datamap::Datamap;
use crate::glob::api_matches_any;
use crate::ir::{ImportKind, Lang, NoteKind, Pos, VarKind};
use crate::program::*;

/// A field token: [`TOP`] is the whole value, [`PHI`] is "some field the summary owner does not
/// name" (summaries only), and `named(sym)` is one field.
pub type Token = u32;
pub const TOP: Token = 0;
pub const PHI: Token = 1;

pub fn named(sym: Sym) -> Token {
    sym + 2
}

pub fn token_sym(t: Token) -> Option<Sym> {
    (t >= 2).then(|| t - 2)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// Every field of the source flows to the same field of the target.
    Copy,
    /// Every field of the source flows to the whole target.
    Collapse,
    /// Field `k` (or the whole) of the source flows to the whole target.
    Load(Sym),
    /// Every field of the source flows to field `k` of the target.
    Store(Sym),
    /// Like `Collapse`, except for categories the sanitiser removes.
    Sanitize(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub to: VarId,
    pub kind: EdgeKind,
    pub stmt: u32,
}

/// An actual argument bound to a formal parameter at a call site.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub site: u32,
    pub formal: u32,
}

/// A call to a local function.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub stmt: u32,
    pub dst: VarId,
    pub func: FuncId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Host {
    /// The sink is not outbound HTTP.
    None,
    Literal(String),
    /// A relative URL: the application's own origin.
    Relative,
    /// The host is computed at run time.
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GapKind {
    UnsupportedLanguage,
    ParseError,
    UnsupportedFramework,
    UnsupportedConstruct,
    UnresolvedCallee,
    UnresolvedMethod,
    DynamicCall,
    UnresolvedImport,
    DepthBound,
}

impl GapKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GapKind::UnsupportedLanguage => "unsupported_language",
            GapKind::ParseError => "parse_error",
            GapKind::UnsupportedFramework => "unsupported_framework",
            GapKind::UnsupportedConstruct => "unsupported_construct",
            GapKind::UnresolvedCallee => "unresolved_callee",
            GapKind::UnresolvedMethod => "unresolved_method",
            GapKind::DynamicCall => "dynamic_call",
            GapKind::UnresolvedImport => "unresolved_import",
            GapKind::DepthBound => "depth_bound",
        }
    }
}

#[derive(Clone, Debug)]
pub enum HitKind {
    Sink {
        sink: u32,
        heuristic: bool,
        host: Host,
    },
    /// Personal data entering this call is a coverage gap, not a flow.
    Gap { kind: GapKind, detail: String },
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub stmt: u32,
    pub kind: HitKind,
    pub args: Vec<VarId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SeedKind {
    /// A read of a classified field: `user.email`.
    FieldRead,
    /// A parameter whose own name is classified: `function send(email)`.
    ParamName,
    /// A variable annotated with a type that declares classified fields.
    Typed,
    /// Request input in a supported framework.
    Request,
    /// A catalogue or config `read` source.
    Api,
}

impl SeedKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SeedKind::FieldRead => "field_read",
            SeedKind::ParamName => "parameter_name",
            SeedKind::Typed => "typed_object",
            SeedKind::Request => "request_input",
            SeedKind::Api => "api",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Seed {
    pub var: VarId,
    pub token: Token,
    pub category: String,
    pub needs_review: bool,
    pub kind: SeedKind,
    pub file: FileId,
    pub pos: Pos,
    pub text: String,
    /// What classified it: `classification-table`, a catalogue/config id, `datamap:<file>`.
    pub by: String,
    /// The field or parameter name the classification rests on.
    pub name: String,
    /// For a parameter-name seed, the function and formal index.
    pub param: Option<(FuncId, u32)>,
    /// For a typed seed, where the field is declared.
    pub declared_at: Option<(FileId, Pos)>,
    /// The statement that creates the seed (a field read, a request-method call), if any.
    pub stmt: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct StaticGap {
    pub kind: GapKind,
    pub detail: String,
    /// (path, position) — position is `None` for whole-file gaps.
    pub locations: Vec<(String, Option<Pos>)>,
}

pub struct Facts {
    pub out: Vec<Vec<Edge>>,
    pub bindings: Vec<Vec<Binding>>,
    pub sites: Vec<Site>,
    pub callers: Vec<Vec<u32>>,
    pub hits: Vec<Hit>,
    pub hit_args: Vec<Vec<u32>>,
    pub seeds: Vec<Seed>,
    pub shared: Vec<bool>,
    /// Fields each function (with its nested functions) reads by name, sorted.
    pub load_fields: Vec<Vec<Sym>>,
    /// Formal parameters a call site can bind, per function (the bound `self` excluded).
    pub formals: Vec<Vec<VarId>>,
    pub sanitiser_removes: Vec<Vec<String>>,
    pub static_gaps: Vec<StaticGap>,
    /// Handlers recognised per source id, for `doctor` and coverage notes.
    pub handlers: BTreeMap<String, usize>,
    /// The function each return slot belongs to: its return value, and the field variables of
    /// the sites it returns (ADR 0009).
    pub slot_of: Vec<Option<FuncId>>,
    /// Per return slot: (call site, the caller's variable it exits to). A function's return
    /// value exits to each call's result, in the order of its callers.
    pub exits: Vec<Vec<(u32, VarId)>>,
}

pub struct Inputs<'a> {
    pub catalogue: &'a Catalogue,
    pub classifier: &'a Classifier,
    pub datamaps: &'a [Datamap],
    /// Files of languages the analysis does not parse: (language, path).
    pub unsupported_files: &'a [(String, String)],
}

struct Builder<'a, 'p> {
    p: &'p mut Program,
    inp: &'a Inputs<'a>,
    f: Facts,
    defs: Vec<Vec<u32>>,
    seed_keys: BTreeSet<(VarId, Token, String)>,
    types_by_name: BTreeMap<String, Vec<usize>>,
    /// Each class's `this` and constructor `this`, to its class.
    this_class: HashMap<VarId, usize>,
    /// Variables used as records or classes: their named fields are read, or methods other than
    /// a string's are called on them. A value named like a personal attribute (`email`) that is
    /// one of these is not that attribute (Ghost's `email` is an Email model).
    records: HashSet<VarId>,
    /// Plain copies between variables (`const email = data.email`), by source.
    copies: HashMap<VarId, Vec<VarId>>,
    /// Named field reads, resolved against allocation sites once every edge exists.
    loads: Vec<heap::PendingLoad>,
    /// (function, formal) pairs some call binds other than one argument to one parameter
    /// (rest parameters, `...xs`, `**kw`): they do not get formal sites.
    inexact: BTreeSet<(FuncId, u32)>,
}

pub fn build(p: &mut Program, inp: &Inputs) -> Facts {
    let nvars = p.vars.len();
    let nfuncs = p.funcs.len();
    let mut types_by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, t) in p.types.iter().enumerate() {
        types_by_name.entry(t.name.clone()).or_default().push(i);
    }
    let mut this_class: HashMap<VarId, usize> = HashMap::new();
    for (i, c) in p.classes.iter().enumerate() {
        this_class.insert(c.this, i);
        for &t in c.ctor_this.iter().chain(&c.more_ctor_this) {
            this_class.insert(t, i);
        }
    }
    let mut b = Builder {
        p,
        inp,
        f: Facts {
            out: vec![Vec::new(); nvars],
            bindings: vec![Vec::new(); nvars],
            sites: Vec::new(),
            callers: vec![Vec::new(); nfuncs],
            hits: Vec::new(),
            hit_args: vec![Vec::new(); nvars],
            seeds: Vec::new(),
            shared: Vec::new(),
            load_fields: vec![Vec::new(); nfuncs],
            formals: Vec::new(),
            sanitiser_removes: inp
                .catalogue
                .sanitisers
                .iter()
                .map(|s| s.def.removes.clone())
                .collect(),
            static_gaps: Vec::new(),
            handlers: BTreeMap::new(),
            slot_of: Vec::new(),
            exits: Vec::new(),
        },
        defs: vec![Vec::new(); nvars],
        seed_keys: BTreeSet::new(),
        types_by_name,
        this_class,
        records: HashSet::new(),
        copies: HashMap::new(),
        loads: Vec::new(),
        inexact: BTreeSet::new(),
    };
    let handlers = b.find_handlers();
    if handlers.iter().any(|h| h.provenance.is_some()) {
        for h in &handlers {
            if let Some((var, path)) = &h.provenance {
                let s = b.p.syms.intern(path);
                b.p.seed_prov(*var, Prov::Api(s));
            }
        }
        b.p.solve();
    }
    b.f.shared = (0..nvars as u32).map(|v| b.p.is_shared(v)).collect();
    b.f.formals =
        b.p.funcs
            .iter()
            .map(|func| {
                let skip = usize::from(func.bound_self);
                func.params.iter().skip(skip).map(|q| q.var).collect()
            })
            .collect();
    b.index_defs();
    b.load_fields();
    b.statements();
    b.handler_seeds(&handlers);
    b.property_seeds();
    b.annotation_seeds();
    b.param_name_seeds();
    b.static_gaps();
    let loads = std::mem::take(&mut b.loads);
    let inexact = std::mem::take(&mut b.inexact);
    let slot_exits = heap::resolve(b.p, &mut b.f, &b.defs, loads, &inexact);
    let nvars = b.p.vars.len();
    b.f.slot_of = vec![None; nvars];
    b.f.exits = vec![Vec::new(); nvars];
    for (g, func) in b.p.funcs.iter().enumerate() {
        b.f.slot_of[func.ret as usize] = Some(g as FuncId);
        b.f.exits[func.ret as usize] = b.f.callers[g]
            .iter()
            .map(|&site| (site, b.f.sites[site as usize].dst))
            .collect();
    }
    for e in slot_exits {
        b.f.slot_of[e.slot as usize] = Some(e.func);
        b.f.exits[e.slot as usize].push((e.site, e.to));
    }
    // What a function throws exits at each call site to that call's catch parameter, or, outside
    // a `try`, to the caller's own throw slot.
    let mut catch_of: HashMap<u32, VarId> = HashMap::new();
    for st in &b.p.stmts {
        if let GKind::Echo { dst, call, .. } = st.kind {
            catch_of.insert(call, dst);
        }
    }
    for (g, func) in b.p.funcs.iter().enumerate() {
        b.f.slot_of[func.throw as usize] = Some(g as FuncId);
        b.f.exits[func.throw as usize] = b.f.callers[g]
            .iter()
            .map(|&site| {
                let stmt = b.f.sites[site as usize].stmt;
                let to = catch_of
                    .get(&stmt)
                    .copied()
                    .unwrap_or_else(|| b.p.funcs[b.p.stmts[stmt as usize].func as usize].throw);
                (site, to)
            })
            .collect();
    }
    for (i, h) in b.f.hits.iter().enumerate() {
        for &a in &h.args {
            if !b.f.hit_args[a as usize].contains(&(i as u32)) {
                b.f.hit_args[a as usize].push(i as u32);
            }
        }
    }
    b.f
}

/// (field, category, needs review, classified by, declared at)
type TypedField = (String, String, bool, String, Option<(FileId, Pos)>);

/// A recognised request handler and what to seed on it.
struct Handler {
    source: usize,
    func: FuncId,
    params: Vec<HandlerParam>,
    provenance: Option<(VarId, String)>,
}

/// A parameter of a handler that is request input, or one property of it (`[AsParameters]`).
struct HandlerParam {
    var: VarId,
    name: String,
    annot: Option<String>,
    rule: ParamRule,
    /// The property of the parameter that is the input, when not the whole parameter.
    field: Option<String>,
}

impl<'a, 'p> Builder<'a, 'p> {
    fn lang(&self, file: FileId) -> &'static str {
        self.p.files[file as usize].lang.family()
    }

    fn index_defs(&mut self) {
        for (i, s) in self.p.stmts.iter().enumerate() {
            let dst = match &s.kind {
                GKind::Copy { dst, .. }
                | GKind::Lit { dst, .. }
                | GKind::Load { dst, .. }
                | GKind::Concat { dst, .. }
                | GKind::Call { dst, .. }
                | GKind::ThisLoad { dst, .. }
                | GKind::FuncRef { dst, .. }
                | GKind::ClassRef { dst, .. }
                | GKind::Global { dst, .. } => *dst,
                _ => continue,
            };
            self.defs[dst as usize].push(i as u32);
        }
    }

    fn load_fields(&mut self) {
        let mut sets: Vec<BTreeSet<Sym>> = vec![BTreeSet::new(); self.p.funcs.len()];
        for s in &self.p.stmts {
            if let GKind::Load { field: Some(k), .. } = s.kind {
                let mut f = Some(s.func);
                while let Some(fi) = f {
                    sets[fi as usize].insert(k);
                    f = self.p.funcs[fi as usize].parent;
                }
            }
        }
        self.f.load_fields = sets.into_iter().map(|s| s.into_iter().collect()).collect();
    }

    fn edge(&mut self, from: VarId, to: VarId, kind: EdgeKind, stmt: u32) {
        if from == to && matches!(kind, EdgeKind::Copy) {
            return;
        }
        self.f.out[from as usize].push(Edge { to, kind, stmt });
    }

    fn hit(&mut self, stmt: u32, kind: HitKind, args: Vec<VarId>) {
        if args.is_empty() {
            return;
        }
        self.f.hits.push(Hit { stmt, kind, args });
    }

    fn seed(&mut self, seed: Seed) {
        let key = (seed.var, seed.token, seed.category.clone());
        if self.seed_keys.insert(key) {
            self.f.seeds.push(seed);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stmt_seed(
        &self,
        stmt: u32,
        var: VarId,
        token: Token,
        category: String,
        needs_review: bool,
        kind: SeedKind,
        by: String,
        name: String,
    ) -> Seed {
        let s = &self.p.stmts[stmt as usize];
        Seed {
            var,
            token,
            category,
            needs_review,
            kind,
            file: s.file,
            pos: s.pos,
            text: s.text.clone(),
            by,
            name,
            param: None,
            declared_at: None,
            stmt: Some(stmt),
        }
    }

    // ------------------------------------------------------------------ statements

    fn statements(&mut self) {
        self.field_links();
        self.records = self.find_records();
        // Copies, and arguments bound to the parameters of local functions (a model class passed
        // to a helper that calls `Model.findOne` is a record there).
        let mut copies: HashMap<VarId, Vec<VarId>> = HashMap::new();
        for (i, st) in self.p.stmts.iter().enumerate() {
            match &st.kind {
                GKind::Copy { dst, src } => copies.entry(*src).or_default().push(*dst),
                GKind::Call { args, callee, .. } => {
                    let Some(t) = self.p.calls.get(&(i as u32)) else {
                        continue;
                    };
                    for &(f, bound) in &t.funcs {
                        let params = &self.p.funcs[f as usize].params;
                        let mut pos = usize::from(bound);
                        let with_recv = receiver_args(callee, args, t.receiver_first.contains(&f));
                        for a in &with_recv {
                            if matches!(a.kind, GArgKind::Positional) {
                                if let Some(q) = params.get(pos).filter(|q| !q.rest) {
                                    copies.entry(a.var).or_default().push(q.var);
                                }
                                pos += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.copies = copies;
        for i in 0..self.p.stmts.len() {
            let stmt = i as u32;
            let s = self.p.stmts[i].clone();
            let lang = self.lang(s.file);
            match &s.kind {
                GKind::Copy { dst, src } | GKind::Throw { dst, src } => {
                    self.edge(*src, *dst, EdgeKind::Copy, stmt)
                }
                GKind::Echo { dst, srcs, call } => {
                    if self.echoes(*call, lang) {
                        for &v in srcs {
                            self.edge(v, *dst, EdgeKind::Collapse, stmt);
                        }
                    }
                    // A callback handed to a library (`prisma.$transaction(async tx => ...)`)
                    // throws through the library call.
                    if self.p.calls.get(call).is_some_and(|t| t.funcs.is_empty()) {
                        let callbacks: Vec<VarId> = srcs
                            .iter()
                            .flat_map(|&v| self.p.prov[v as usize].iter())
                            .filter_map(|p| match p {
                                Prov::Func(f) => Some(self.p.funcs[*f as usize].throw),
                                _ => None,
                            })
                            .collect();
                        for t in callbacks {
                            self.edge(t, *dst, EdgeKind::Copy, stmt);
                        }
                    }
                }
                GKind::Load {
                    dst,
                    obj,
                    field: Some(k),
                } => {
                    let name = self.p.syms.str(*k).to_string();
                    if !self.inp.catalogue.non_propagating(lang, &name) {
                        self.loads.push(heap::PendingLoad {
                            obj: *obj,
                            dst: *dst,
                            field: *k,
                            stmt,
                        });
                    }
                    self.field_read(stmt, *obj, *dst, &name);
                    self.read_source(stmt, *dst, lang);
                }
                GKind::Load {
                    dst,
                    obj,
                    field: None,
                } => self.edge(*obj, *dst, EdgeKind::Collapse, stmt),
                GKind::Store { obj, field, src } => {
                    match field {
                        Some(k) => self.edge(*src, *obj, EdgeKind::Store(*k), stmt),
                        None => self.edge(*src, *obj, EdgeKind::Collapse, stmt),
                    }
                    if let Some(k) = field {
                        self.assign_sink(stmt, *obj, *k, *src, lang);
                    }
                }
                GKind::ThisLoad {
                    dst,
                    obj,
                    field,
                    var,
                } => {
                    let name = self.p.syms.str(*field).to_string();
                    self.edge(*var, *dst, EdgeKind::Copy, stmt);
                    self.field_read(stmt, *obj, *dst, &name);
                    self.read_source(stmt, *dst, lang);
                }
                GKind::ThisStore { src, var, .. } => self.edge(*src, *var, EdgeKind::Copy, stmt),
                GKind::Concat { dst, parts } => {
                    for part in parts {
                        if let GPart::Var(v) = part {
                            self.edge(*v, *dst, EdgeKind::Collapse, stmt);
                        }
                    }
                }
                GKind::Return { src } => {
                    let ret = self.p.funcs[s.func as usize].ret;
                    self.edge(*src, ret, EdgeKind::Copy, stmt);
                }
                GKind::Call {
                    dst, callee, args, ..
                } => self.call(stmt, *dst, callee, args, lang),
                _ => {}
            }
        }
    }

    /// What a class stores in a field flows to the same field of its subclasses, cited where
    /// the subclass reads (or else writes) the field.
    fn field_links(&mut self) {
        let mut site: HashMap<VarId, u32> = HashMap::new();
        for (i, s) in self.p.stmts.iter().enumerate() {
            match s.kind {
                GKind::ThisLoad { var, .. } => {
                    site.insert(var, i as u32);
                }
                GKind::ThisStore { var, .. } => {
                    site.entry(var).or_insert(i as u32);
                }
                _ => {}
            }
        }
        for (from, to) in self.p.field_links.clone() {
            if let Some(&stmt) = site.get(&to) {
                self.edge(from, to, EdgeKind::Copy, stmt);
            }
        }
    }

    /// Whether a call's error may carry its arguments: a call into a library (an API path that
    /// is not a language built-in), or one the analysis cannot resolve. The project's own
    /// functions throw what they throw, through their throw slots.
    fn echoes(&self, call: u32, lang: &str) -> bool {
        let Some(t) = self.p.calls.get(&call) else {
            return false;
        };
        let library = t.apis.iter().any(|&s| {
            let path = self.p.syms.str(s);
            !(is_runtime_global(path, lang) || path.starts_with("builtins."))
        });
        library
            || !t.unresolved.is_empty()
            || (t.funcs.is_empty() && t.classes.is_empty() && t.dynamic)
    }

    fn field_read(&mut self, stmt: u32, obj: VarId, dst: VarId, name: &str) {
        // `models.User`, `Mailer.send`: a member of code, not of data. An instance (`this`) is
        // data, even though it carries its class as provenance.
        let class = self.this_class.get(&obj).copied();
        let instance = matches!(self.p.vars[obj as usize].kind, VarKind::This) || class.is_some();
        if !instance
            && self.p.prov[obj as usize]
                .iter()
                .any(|p| matches!(p, Prov::Module(_) | Prov::Package(_) | Prov::Class(_)))
        {
            return;
        }
        // What the field is read from: the receiver's name, or for `this.f` the class and its
        // other fields.
        let (object, siblings) = match class {
            Some(c) => {
                let name = self.p.classes[c].name.clone();
                let fields = self.type_fields(&name);
                (Some(name), fields)
            }
            None => (
                receiver_ident(&self.p.stmts[stmt as usize].text)
                    .or_else(|| Some(self.p.vars[obj as usize].name.clone())),
                Vec::new(),
            ),
        };
        let place = Place {
            object: object.as_deref(),
            siblings: &siblings,
        };
        // `data.email` used as a record (`email.id`, `email.get(...)`) is not an address.
        if self.read_as_record(dst) {
            return;
        }
        // `claim.Value`, `task.Result`: an accessor of what the object holds, not an attribute.
        let lang = self.lang(self.p.stmts[stmt as usize].file);
        if self.inp.catalogue.accessor_field(lang, name) {
            return;
        }
        // `type.Name`: metadata of code.
        if let Some(globs) = self.inp.catalogue.metadata_types.get(lang)
            && self
                .api_paths_of(&[obj])
                .iter()
                .any(|p| api_matches_any(globs, p.trim_end_matches("()")))
        {
            return;
        }
        for c in self.inp.classifier.classify_in(name, &place) {
            let seed = self.stmt_seed(
                stmt,
                dst,
                TOP,
                c.category,
                c.needs_review,
                SeedKind::FieldRead,
                c.by,
                name.to_string(),
            );
            self.seed(seed);
        }
    }

    fn read_source(&mut self, stmt: u32, dst: VarId, lang: &str) {
        let paths: Vec<String> = self.p.prov[dst as usize]
            .iter()
            .filter_map(|p| match p {
                Prov::Api(s) => Some(self.p.syms.str(*s).to_string()),
                _ => None,
            })
            .collect();
        for path in paths {
            for src in &self.inp.catalogue.sources {
                if src.def.kind != SourceKind::Read || src.def.language != lang {
                    continue;
                }
                if src.reads.iter().any(|g| g.is_match(&path)) {
                    let category = src.def.category.clone().unwrap_or_else(|| UNKNOWN.into());
                    let seed = self.stmt_seed(
                        stmt,
                        dst,
                        TOP,
                        category,
                        false,
                        SeedKind::Request,
                        src.def.id.clone(),
                        path.clone(),
                    );
                    self.seed(seed);
                }
            }
        }
    }

    fn assign_sink(&mut self, stmt: u32, obj: VarId, k: Sym, src: VarId, lang: &str) {
        let field = self.p.syms.str(k).to_string();
        let bases: Vec<Sym> = self.p.prov[obj as usize]
            .iter()
            .filter_map(|p| match p {
                Prov::Api(s) => Some(*s),
                _ => None,
            })
            .collect();
        for base in bases {
            let Some(path) = self.p.api_member(base, &field) else {
                continue;
            };
            let path = self.p.syms.str(path).to_string();
            for (si, sink) in self.inp.catalogue.sinks.iter().enumerate() {
                if sink.def.language == lang && sink.assigns.iter().any(|g| g.is_match(&path)) {
                    self.hit(
                        stmt,
                        HitKind::Sink {
                            sink: si as u32,
                            heuristic: false,
                            host: Host::None,
                        },
                        vec![src],
                    );
                }
            }
        }
    }

    fn arg_vars(args: &[GArg]) -> Vec<VarId> {
        args.iter().map(|a| a.var).collect()
    }

    fn select_args(sel: &ArgSel, keywords: &[String], args: &[GArg]) -> Vec<VarId> {
        if sel.is_all() {
            return Self::arg_vars(args);
        }
        let ArgSel::Indices(idx) = sel else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut pos = 0usize;
        for a in args {
            match &a.kind {
                GArgKind::Positional => {
                    if idx.contains(&pos) {
                        out.push(a.var);
                    }
                    pos += 1;
                }
                GArgKind::Spread => out.push(a.var),
                GArgKind::Keyword(k) => {
                    if keywords.contains(k) {
                        out.push(a.var);
                    }
                }
                GArgKind::KwSpread => out.push(a.var),
            }
        }
        out
    }

    fn call(&mut self, stmt: u32, dst: VarId, callee: &GCallee, args: &[GArg], lang: &str) {
        let Some(t) = self.p.calls.get(&stmt).cloned() else {
            return;
        };
        let recv = match callee {
            GCallee::Method { recv, .. } => Some(*recv),
            _ => None,
        };
        let mut all: Vec<VarId> = Self::arg_vars(args);
        if let Some(r) = recv {
            all.push(r);
        }
        let mut propagated = false;
        let propagate = |b: &mut Self, propagated: &mut bool| {
            if !*propagated {
                for &a in &all {
                    b.edge(a, dst, EdgeKind::Collapse, stmt);
                }
                *propagated = true;
            }
        };
        // A class with a constructor: the instance is what its constructor returns (through the
        // constructor's summary, per call site). Without one, it is the class's shared `this`
        // (field initializers).
        for &c in &t.classes {
            let class = &self.p.classes[c as usize];
            if class.ctor_this.is_none() {
                let this = class.this;
                self.edge(this, dst, EdgeKind::Copy, stmt);
            }
        }
        // Functions with no body in source (a generated `[LoggerMessage]` method): a sink when
        // the catalogue names their attribute, otherwise a call that reaches nothing visible.
        let mut stubs: Vec<String> = Vec::new();
        let mut reached = 0usize;
        for &(f, bound) in &t.funcs {
            let fargs = receiver_args(callee, args, t.receiver_first.contains(&f));
            if self.decorated_sink(stmt, f, &fargs, lang) {
                reached += 1;
                continue;
            }
            if self.p.funcs[f as usize].is_stub {
                stubs.push(self.p.funcs[f as usize].name.clone());
                continue;
            }
            reached += 1;
            if let Some(si) = self.local_sanitiser(f) {
                for &a in &Self::arg_vars(&fargs) {
                    self.edge(a, dst, EdgeKind::Sanitize(si), stmt);
                }
                continue;
            }
            let site = self.f.sites.len() as u32;
            self.f.sites.push(Site { stmt, dst, func: f });
            self.f.callers[f as usize].push(site);
            self.bind_args(site, f, bound, &fargs);
            self.annotated_result(stmt, dst, f);
        }
        let apis: Vec<String> = t
            .apis
            .iter()
            .map(|s| self.p.syms.str(*s).to_string())
            .collect();
        // A call is explained when it reaches a local function, a class, or an API path the
        // catalogue knows. Only an unexplained call is a coverage gap; the other possible
        // targets of an explained call are not (`remember()` returns both a cache entry and
        // the Prisma client it holds).
        // A receiver typed with an interface is open: the local implementations it reaches do not
        // explain a target in an external module (an adapter loaded at run time, `require()()`).
        // Paths through built-in containers (`Map().get()`) are where the value was kept, not
        // what it is, and still are.
        if self.dispatch(stmt, dst, &apis, &t, args, lang) {
            reached += 1;
        }
        let local = reached > 0 || !t.classes.is_empty();
        let mut explained = (reached > 0 && !t.open_world) || !t.classes.is_empty();
        let mut unknown = stubs;
        for path in &apis {
            if self.api_call(stmt, dst, path, args, recv, lang)
                || self.name_heuristic_sink(stmt, path, args, lang)
                || (t.open_world && local && is_container_path(path, lang))
            {
                explained = true;
            } else if let Some(r) = recv.filter(|_| is_runtime_global(path, lang)) {
                // A method on a run-time global the catalogue does not name (`globalThis.cache`,
                // a declared global): treated like a method on a plain value.
                let name = path.rsplit('.').next().unwrap_or(path).to_string();
                if self.data_method(stmt, dst, r, &name, args, lang) {
                    explained = true;
                } else {
                    unknown.push(path.clone());
                }
            } else {
                unknown.push(path.clone());
            }
        }
        // Calls into an unsupported framework are covered by that framework's own gap.
        let before = unknown.len();
        unknown.retain(|p| !self.in_unsupported_framework(p, lang));
        if unknown.len() < before && unknown.is_empty() && t.unresolved.is_empty() && !t.dynamic {
            propagate(self, &mut propagated);
            explained = true;
        }
        if !explained {
            let gaps = unknown
                .into_iter()
                .map(|p| (GapKind::UnresolvedCallee, p))
                .chain(
                    t.unresolved
                        .iter()
                        .map(|s| (GapKind::UnresolvedImport, self.p.syms.str(*s).to_string())),
                )
                .chain(t.dynamic.then(|| {
                    (
                        GapKind::DynamicCall,
                        callee_text(&self.p.stmts[stmt as usize].text),
                    )
                }));
            for (kind, detail) in gaps.collect::<Vec<_>>() {
                // Assume the call propagates, and report personal data reaching it as a gap.
                propagate(self, &mut propagated);
                self.hit(stmt, HitKind::Gap { kind, detail }, Self::arg_vars(args));
            }
        }
        if let Some((r, name)) = t.plain_method {
            let name = self.p.syms.str(name).to_string();
            self.plain_method(stmt, dst, r, &name, args, lang);
        }
        if t.overridden > 0 {
            // `this.run(x)` in a base class: the overrides in subclasses are not followed.
            let detail = format!(
                "{} (overridden in {} subclass{})",
                callee_text(&self.p.stmts[stmt as usize].text),
                t.overridden,
                if t.overridden == 1 { "" } else { "es" }
            );
            self.hit(
                stmt,
                HitKind::Gap {
                    kind: GapKind::DynamicCall,
                    detail,
                },
                Self::arg_vars(args),
            );
        }
    }

    /// A dispatching call (`mediator.Send(command)`): the argument reaches the handlers of its
    /// type, as a call to each would pass it. Returns true when a dispatcher matched.
    fn dispatch(
        &mut self,
        stmt: u32,
        dst: VarId,
        apis: &[String],
        t: &CallTargets,
        args: &[GArg],
        lang: &str,
    ) -> bool {
        let cat = self.inp.catalogue;
        let mut names: Vec<String> = apis.to_vec();
        // A project's own bus: its local method, by the class's qualified name.
        for &(f, _) in &t.funcs {
            let func = &self.p.funcs[f as usize];
            if let Some(c) = func.class
                && let Some(q) = &self.p.classes[c as usize].qualified_name
            {
                let short = func.name.rsplit('.').next().unwrap_or(&func.name);
                names.push(format!("{q}.{short}"));
            }
        }
        let mut matched = false;
        for d in &cat.dispatchers {
            if d.def.language != lang
                || !names.iter().any(|n| d.calls.iter().any(|g| g.is_match(n)))
            {
                continue;
            }
            let Some(arg) = args
                .iter()
                .filter(|a| matches!(a.kind, GArgKind::Positional))
                .nth(d.def.arg)
            else {
                continue;
            };
            matched = true;
            let arg_classes: Vec<ClassId> = self.p.prov[arg.var as usize]
                .iter()
                .filter_map(|p| match p {
                    Prov::Instance(c) => Some(*c),
                    _ => None,
                })
                .collect();
            let mut targets: BTreeSet<(FuncId, usize, bool)> = BTreeSet::new();
            for (ti, target) in d.def.targets.iter().enumerate() {
                let glob = &d.implements[ti];
                for c in 0..self.p.classes.len() as ClassId {
                    let class = &self.p.classes[c as usize];
                    if class.is_interface || self.lang(class.file) != lang {
                        continue;
                    }
                    // The base (or interface) that matches, and its type arguments.
                    let mut type_args: Option<Vec<VarId>> = None;
                    for k in self.p.lineage(c) {
                        let kc = &self.p.classes[k as usize];
                        let none: Vec<VarId> = Vec::new();
                        let pairs =
                            kc.bases
                                .iter()
                                .enumerate()
                                .map(|(i, b)| (b, kc.base_args.get(i).unwrap_or(&none)))
                                .chain(
                                    kc.implements.iter().enumerate().map(|(i, b)| {
                                        (b, kc.implements_args.get(i).unwrap_or(&none))
                                    }),
                                );
                        for (b, targs) in pairs {
                            let hit = self.p.prov[*b as usize].iter().any(|p| match p {
                                Prov::Api(s) => {
                                    let path = self.p.syms.str(*s);
                                    glob.is_match(path.strip_suffix("()").unwrap_or(path))
                                }
                                Prov::Class(x) => self.p.classes[*x as usize]
                                    .qualified_name
                                    .as_ref()
                                    .is_some_and(|q| glob.is_match(q)),
                                _ => false,
                            });
                            if hit {
                                type_args = Some(targs.clone());
                                break;
                            }
                        }
                        if type_args.is_some() {
                            break;
                        }
                    }
                    let Some(targs) = type_args else { continue };
                    let (fits, exact) = match target.type_arg.and_then(|i| targs.get(i)) {
                        Some(&ta) => self.dispatch_fits(ta, &arg_classes),
                        None => (true, false),
                    };
                    if !fits {
                        continue;
                    }
                    for k in self.p.lineage(c) {
                        let ms: Vec<FuncId> = self.p.classes[k as usize]
                            .methods
                            .iter()
                            .filter(|(n, _)| *n == target.method)
                            .map(|(_, f)| *f)
                            .collect();
                        if !ms.is_empty() {
                            for m in ms {
                                targets.insert((m, target.param, exact));
                            }
                            break;
                        }
                    }
                }
            }
            if std::env::var_os("PIIFLOW_DEBUG_IR").is_some() {
                let names: Vec<&str> = targets
                    .iter()
                    .map(|(f, _, _)| self.p.funcs[*f as usize].name.as_str())
                    .collect();
                eprintln!("dispatch #{stmt} {} -> {names:?}", d.def.id);
            }
            for (f, param, exact) in targets {
                if self.p.funcs[f as usize].is_stub {
                    continue;
                }
                let site = self.f.sites.len() as u32;
                self.f.sites.push(Site { stmt, dst, func: f });
                self.f.callers[f as usize].push(site);
                // One argument to one parameter, as a direct call would bind it, whether the
                // type filter could decide the handler or not.
                let _ = exact;
                if param < self.f.formals[f as usize].len() {
                    self.f.bindings[arg.var as usize].push(Binding {
                        site,
                        formal: param as u32,
                    });
                }
            }
        }
        matched
    }

    /// Whether a dispatched value whose classes are `arg_classes` fits a handler's type argument
    /// `ta`, and whether the fit is exact. A type parameter (no provenance) takes anything; a
    /// value of unknown class reaches every handler, inexactly.
    fn dispatch_fits(&self, ta: VarId, arg_classes: &[ClassId]) -> (bool, bool) {
        let tprov = &self.p.prov[ta as usize];
        if tprov.is_empty() {
            return (true, false);
        }
        if arg_classes.is_empty() {
            return (true, false);
        }
        let fits = tprov.iter().any(|p| match p {
            Prov::Class(k) => arg_classes.iter().any(|&a| self.p.lineage(a).contains(k)),
            Prov::Api(s) => {
                let want = self.p.syms.str(*s).rsplit('.').next().unwrap_or("");
                arg_classes
                    .iter()
                    .any(|&a| self.p.classes[a as usize].name == want)
            }
            _ => false,
        });
        (fits, true)
    }

    /// A call to a project function whose attribute the catalogue names as a sink.
    fn decorated_sink(&mut self, stmt: u32, f: FuncId, args: &[GArg], lang: &str) -> bool {
        let decorators = self.p.funcs[f as usize].decorators.clone();
        if decorators.is_empty() {
            return false;
        }
        let paths: Vec<String> = decorators
            .iter()
            .flat_map(|d| self.p.prov[*d as usize].iter())
            .filter_map(|p| match p {
                Prov::Api(s) => Some(self.p.syms.str(*s).to_string()),
                _ => None,
            })
            .collect();
        let cat = self.inp.catalogue;
        for (si, sink) in cat.sinks.iter().enumerate() {
            if sink.def.language != lang
                || !sink
                    .decorated
                    .iter()
                    .any(|g| paths.iter().any(|p| g.is_match(p)))
            {
                continue;
            }
            let selected = Self::select_args(&sink.def.args, &sink.def.keywords, args);
            let host = match sink.def.class {
                SinkClass::Http => Host::Dynamic,
                _ => Host::None,
            };
            self.hit(
                stmt,
                HitKind::Sink {
                    sink: si as u32,
                    heuristic: false,
                    host,
                },
                selected,
            );
            return true;
        }
        false
    }

    fn local_sanitiser(&self, f: FuncId) -> Option<u32> {
        let func = &self.p.funcs[f as usize];
        let path = &self.p.files[func.file as usize].path;
        let lang = self.lang(func.file);
        let short = func.name.rsplit('.').next().unwrap_or(&func.name);
        self.inp
            .catalogue
            .sanitisers
            .iter()
            .position(|s| {
                s.def.language == lang
                    && s.functions.iter().any(|(file, name)| {
                        file.is_match(path) && (name.is_match(&func.name) || name.is_match(short))
                    })
            })
            .map(|i| i as u32)
    }

    fn bind_args(&mut self, site: u32, f: FuncId, bound: bool, args: &[GArg]) {
        let params = self.p.funcs[f as usize].params.clone();
        let offset = usize::from(bound && self.p.funcs[f as usize].bound_self);
        let formals: Vec<&GParam> = params.iter().skip(offset).collect();
        let formal_index = |var: VarId, b: &Self| {
            b.f.formals[f as usize]
                .iter()
                .position(|&x| x == var)
                .map(|i| i as u32)
        };
        let mut pos = 0usize;
        let mut bound_formals: BTreeSet<usize> = BTreeSet::new();
        let mut binds: Vec<(VarId, u32)> = Vec::new();
        let mut inexact: Vec<u32> = Vec::new();
        for a in args {
            match &a.kind {
                GArgKind::Positional => {
                    let target = formals.get(pos).copied().filter(|q| !q.kwrest);
                    match target {
                        Some(q) if !q.rest => {
                            bound_formals.insert(pos);
                            if let Some(fi) = formal_index(q.var, self) {
                                binds.push((a.var, fi));
                            }
                            pos += 1;
                        }
                        _ => {
                            if let Some(q) = formals.iter().find(|q| q.rest)
                                && let Some(fi) = formal_index(q.var, self)
                            {
                                binds.push((a.var, fi));
                                inexact.push(fi);
                            }
                        }
                    }
                }
                GArgKind::Keyword(k) => {
                    let target = formals
                        .iter()
                        .find(|q| q.name == *k && !q.rest && !q.kwrest)
                        .or_else(|| formals.iter().find(|q| q.kwrest));
                    if let Some(q) = target
                        && let Some(fi) = formal_index(q.var, self)
                    {
                        binds.push((a.var, fi));
                        if q.kwrest {
                            inexact.push(fi);
                        }
                    }
                }
                GArgKind::Spread => {
                    for (i, q) in formals.iter().enumerate().skip(pos) {
                        if !q.kwrest
                            && !bound_formals.contains(&i)
                            && let Some(fi) = formal_index(q.var, self)
                        {
                            binds.push((a.var, fi));
                            inexact.push(fi);
                        }
                    }
                }
                GArgKind::KwSpread => {
                    for q in &formals {
                        if !q.rest
                            && let Some(fi) = formal_index(q.var, self)
                        {
                            binds.push((a.var, fi));
                            inexact.push(fi);
                        }
                    }
                }
            }
        }
        binds.sort();
        binds.dedup();
        for fi in inexact {
            self.inexact.insert((f, fi));
        }
        for (var, formal) in binds {
            self.f.bindings[var as usize].push(Binding { site, formal });
        }
    }

    /// Returns true when the catalogue knows this API path.
    fn api_call(
        &mut self,
        stmt: u32,
        dst: VarId,
        path: &str,
        args: &[GArg],
        recv: Option<VarId>,
        lang: &str,
    ) -> bool {
        let cat = self.inp.catalogue;
        for (si, sink) in cat.sinks.iter().enumerate() {
            if sink.def.language != lang || !sink.calls.iter().any(|g| g.is_match(path)) {
                continue;
            }
            let mut selected = Self::select_args(&sink.def.args, &sink.def.keywords, args);
            if sink.def.receiver {
                selected.extend(recv);
            }
            let host = match (&sink.def.class, &sink.def.host) {
                (SinkClass::Http, Some(h)) => self.host(h, args),
                (SinkClass::Http, None) => Host::Dynamic,
                _ => Host::None,
            };
            self.hit(
                stmt,
                HitKind::Sink {
                    sink: si as u32,
                    heuristic: false,
                    host,
                },
                selected,
            );
            return true;
        }
        for src in &cat.sources {
            if src.def.kind == SourceKind::Read
                && src.def.language == lang
                && src.calls.iter().any(|g| g.is_match(path))
            {
                let mut categories = Vec::new();
                let mut name = path.to_string();
                if let Some(ac) = &src.def.arg_category
                    && let Some(arg) = args
                        .iter()
                        .filter(|a| matches!(a.kind, GArgKind::Positional))
                        .nth(ac.index)
                    && let Some(key) = self.name_of(arg.var)
                {
                    // `FindFirst("sub")`, `FindFirstValue(ClaimTypes.Email)`: the claim type
                    // says what the value is.
                    let short = key.rsplit(['/', '.']).next().unwrap_or(&key).to_string();
                    let found = ac
                        .names
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(&short))
                        .map(|(_, c)| c.clone());
                    match found {
                        Some(c) => categories.push((c, false, src.def.id.clone())),
                        None if ac.classify => {
                            let place = Place {
                                object: None,
                                siblings: &[],
                            };
                            for c in self.inp.classifier.classify_in(&short, &place) {
                                categories.push((
                                    c.category,
                                    c.needs_review,
                                    format!("{}+{}", src.def.id, c.by),
                                ));
                            }
                        }
                        None => {}
                    }
                    name = format!("{path}({short})");
                }
                if categories.is_empty() {
                    let category = src.def.category.clone().unwrap_or_else(|| UNKNOWN.into());
                    categories.push((category, false, src.def.id.clone()));
                }
                for (category, needs_review, by) in categories {
                    let seed = self.stmt_seed(
                        stmt,
                        dst,
                        TOP,
                        category,
                        needs_review,
                        SeedKind::Request,
                        by,
                        name.clone(),
                    );
                    self.seed(seed);
                }
                return true;
            }
        }
        for (i, s) in cat.sanitisers.iter().enumerate() {
            if s.def.language == lang && s.calls.iter().any(|g| g.is_match(path)) {
                let mut vars = Self::arg_vars(args);
                vars.extend(recv);
                for v in vars {
                    self.edge(v, dst, EdgeKind::Sanitize(i as u32), stmt);
                }
                return true;
            }
        }
        for prop in &cat.propagators {
            if prop.def.language == lang && prop.calls.iter().any(|g| g.is_match(path)) {
                self.apply_flow(stmt, dst, prop.def.flow, recv, args);
                return true;
            }
        }
        false
    }

    fn apply_flow(
        &mut self,
        stmt: u32,
        dst: VarId,
        flow: Flow,
        recv: Option<VarId>,
        args: &[GArg],
    ) {
        let mut vars = Self::arg_vars(args);
        vars.extend(recv);
        match flow {
            Flow::ArgsOnly => {
                for a in args {
                    self.edge(a.var, dst, EdgeKind::Collapse, stmt);
                }
            }
            Flow::ArgsToResult => {
                for v in vars {
                    self.edge(v, dst, EdgeKind::Collapse, stmt);
                }
            }
            Flow::ArgsToReceiver | Flow::Fluent => {
                if let Some(r) = recv {
                    for a in args {
                        self.edge(a.var, r, EdgeKind::Copy, stmt);
                    }
                    if flow == Flow::Fluent {
                        self.edge(dst, r, EdgeKind::Copy, stmt);
                    }
                }
                for v in vars {
                    self.edge(v, dst, EdgeKind::Collapse, stmt);
                }
            }
            Flow::ReceiverToCallback => {
                if let Some(r) = recv {
                    self.edge(r, dst, EdgeKind::Copy, stmt);
                    for a in args {
                        let funcs: Vec<FuncId> = self.p.prov[a.var as usize]
                            .iter()
                            .filter_map(|p| match p {
                                Prov::Func(f) => Some(*f),
                                _ => None,
                            })
                            .collect();
                        for f in funcs {
                            if let Some(&formal) = self.f.formals[f as usize].first() {
                                self.edge(r, formal, EdgeKind::Copy, stmt);
                            }
                            let ret = self.p.funcs[f as usize].ret;
                            self.edge(ret, dst, EdgeKind::Copy, stmt);
                        }
                    }
                }
            }
            Flow::CallbackResult => {
                for a in args {
                    let funcs: Vec<FuncId> = self.p.prov[a.var as usize]
                        .iter()
                        .filter_map(|p| match p {
                            Prov::Func(f) => Some(*f),
                            _ => None,
                        })
                        .collect();
                    if funcs.is_empty() {
                        self.edge(a.var, dst, EdgeKind::Collapse, stmt);
                    }
                    for f in funcs {
                        let ret = self.p.funcs[f as usize].ret;
                        self.edge(ret, dst, EdgeKind::Copy, stmt);
                    }
                }
            }
            Flow::None => {}
        }
    }

    fn plain_method(
        &mut self,
        stmt: u32,
        dst: VarId,
        recv: VarId,
        name: &str,
        args: &[GArg],
        lang: &str,
    ) {
        let cat = self.inp.catalogue;
        let receiver = receiver_name(
            &self.p.stmts[stmt as usize].text,
            name,
            &self.p.vars[recv as usize].name,
        );
        for (si, sink) in cat.sinks.iter().enumerate() {
            if sink.def.language == lang
                && !sink.def.receivers.is_empty()
                && sink.def.receivers.iter().any(|r| r == &receiver)
                && sink.def.methods.iter().any(|m| m == name)
            {
                let selected = Self::select_args(&sink.def.args, &sink.def.keywords, args);
                self.hit(
                    stmt,
                    HitKind::Sink {
                        sink: si as u32,
                        heuristic: true,
                        host: Host::None,
                    },
                    selected,
                );
                return;
            }
        }
        if self.data_method(stmt, dst, recv, name, args, lang) {
            return;
        }
        for prop in &cat.propagators {
            if prop.def.language == lang && prop.def.receivers.iter().any(|r| r == &receiver) {
                self.apply_flow(stmt, dst, prop.def.flow, Some(recv), args);
                return;
            }
        }
        // An unknown method on a receiver with no known origin: the receiver's own data flows
        // to the result, and arguments handed to it are a coverage gap.
        for a in args {
            self.edge(a.var, dst, EdgeKind::Collapse, stmt);
        }
        self.edge(recv, dst, EdgeKind::Collapse, stmt);
        let detail = if receiver.is_empty() {
            format!("<value>.{name}")
        } else {
            format!("{receiver}.{name}")
        };
        self.hit(
            stmt,
            HitKind::Gap {
                kind: GapKind::UnresolvedMethod,
                detail,
            },
            Self::arg_vars(args),
        );
    }

    /// The receiver-name heuristic for an API path the catalogue does not know: `ctx.logger.info`
    /// handed to a handler by an unmodelled framework is still a logger.
    fn name_heuristic_sink(&mut self, stmt: u32, path: &str, args: &[GArg], lang: &str) -> bool {
        let mut segs = path.rsplit(['.', ':']);
        let (Some(method), Some(receiver)) = (segs.next(), segs.next()) else {
            return false;
        };
        let receiver = receiver.trim_end_matches("()");
        let cat = self.inp.catalogue;
        for (si, sink) in cat.sinks.iter().enumerate() {
            if sink.def.language == lang
                && sink.def.receivers.iter().any(|r| r == receiver)
                && sink.def.methods.iter().any(|m| m == method)
            {
                let selected = Self::select_args(&sink.def.args, &sink.def.keywords, args);
                self.hit(
                    stmt,
                    HitKind::Sink {
                        sink: si as u32,
                        heuristic: true,
                        host: Host::None,
                    },
                    selected,
                );
                return true;
            }
        }
        false
    }

    fn in_unsupported_framework(&self, path: &str, lang: &str) -> bool {
        let traits = Lang::family_traits(lang);
        let module = if traits.dotted_api_paths {
            if traits.packages {
                path.split('.').next().unwrap_or(path)
            } else {
                // A C# path is a namespace-qualified type and its members: any prefix may be the
                // framework's namespace.
                path
            }
        } else {
            match path.split_once(':') {
                Some((m, _)) => m,
                None => return false,
            }
        };
        let sep = traits.module_sep.to_string();
        self.inp.catalogue.frameworks.iter().any(|fw| {
            !fw.supported
                && fw.language == lang
                && fw.modules.iter().any(|m| {
                    module == m
                        || module.starts_with(&format!("{m}{sep}"))
                        || m.starts_with(&format!("{module}{sep}"))
                })
        })
    }

    /// A method the catalogue lists by name for plain values (sanitisers like `includes`,
    /// propagators like `toLowerCase`, `push`, `map`). Returns true when it applied one.
    fn data_method(
        &mut self,
        stmt: u32,
        dst: VarId,
        recv: VarId,
        name: &str,
        args: &[GArg],
        lang: &str,
    ) -> bool {
        let cat = self.inp.catalogue;
        for (i, s) in cat.sanitisers.iter().enumerate() {
            if s.def.language == lang && s.def.methods.iter().any(|m| m == name) {
                let mut vars = Self::arg_vars(args);
                vars.push(recv);
                for v in vars {
                    self.edge(v, dst, EdgeKind::Sanitize(i as u32), stmt);
                }
                return true;
            }
        }
        for prop in &cat.propagators {
            if prop.def.language == lang && prop.def.methods.iter().any(|m| m == name) {
                self.apply_flow(stmt, dst, prop.def.flow, Some(recv), args);
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------ string constants

    /// The name a value spells: a string literal, or the last segment of the API path of a
    /// constant read (`ClaimTypes.Email`).
    fn name_of(&self, var: VarId) -> Option<String> {
        if let Some((s, true)) = self.literal_prefix(var, 0) {
            return Some(s);
        }
        self.p.prov[var as usize].iter().find_map(|p| match p {
            Prov::Api(s) => {
                let path = self.p.syms.str(*s);
                (!path.ends_with(')')).then(|| path.rsplit('.').next().unwrap_or(path).to_string())
            }
            _ => None,
        })
    }

    /// What a variable was copied from, through single plain copies.
    fn copy_origin(&self, var: VarId, depth: u32) -> VarId {
        let defs = &self.defs[var as usize];
        match (defs.as_slice(), depth < 8) {
            ([d], true) => match &self.p.stmts[*d as usize].kind {
                GKind::Copy { src, .. } => self.copy_origin(*src, depth + 1),
                _ => var,
            },
            _ => var,
        }
    }

    /// The call that defines a variable, through plain copies: (its arguments, its API paths).
    fn defining_call(&self, var: VarId, depth: u32) -> Option<(Vec<GArg>, Vec<String>)> {
        if depth > 8 {
            return None;
        }
        let defs = &self.defs[var as usize];
        if defs.len() != 1 {
            return None;
        }
        let stmt = defs[0];
        match &self.p.stmts[stmt as usize].kind {
            GKind::Copy { src, .. } => self.defining_call(*src, depth + 1),
            GKind::Call { args, callee, .. } => {
                let mut paths: Vec<String> = Vec::new();
                if let Some(t) = self.p.calls.get(&stmt) {
                    paths.extend(t.apis.iter().map(|s| self.p.syms.str(*s).to_string()));
                }
                if let GCallee::Value(v) = callee {
                    paths.extend(self.api_paths_of(&[*v]));
                }
                Some((args.clone(), paths))
            }
            _ => None,
        }
    }

    /// The literal text a variable is known to start with, and whether that is all of it.
    fn literal_prefix(&self, var: VarId, depth: u32) -> Option<(String, bool)> {
        if depth > 8 {
            return None;
        }
        let defs = &self.defs[var as usize];
        if defs.len() != 1 {
            return None;
        }
        match &self.p.stmts[defs[0] as usize].kind {
            GKind::Lit { value: Some(v), .. } => Some((v.clone(), true)),
            GKind::Copy { src, .. } => self.literal_prefix(*src, depth + 1),
            GKind::Concat { parts, .. } => {
                let mut out = String::new();
                for (i, part) in parts.iter().enumerate() {
                    match part {
                        GPart::Lit(s) => out.push_str(s),
                        GPart::Var(v) => match self.literal_prefix(*v, depth + 1) {
                            Some((s, true)) => out.push_str(&s),
                            Some((s, false)) => {
                                out.push_str(&s);
                                return Some((out, false));
                            }
                            None => {
                                return if i == 0 && out.is_empty() {
                                    None
                                } else {
                                    Some((out, false))
                                };
                            }
                        },
                    }
                }
                Some((out, true))
            }
            _ => None,
        }
    }

    fn host(&self, spec: &crate::catalogue::HostSpec, args: &[GArg]) -> Host {
        let mut var = None;
        if let Some(k) = &spec.keyword {
            var = args
                .iter()
                .find(|a| matches!(&a.kind, GArgKind::Keyword(n) if n == k))
                .map(|a| a.var);
        }
        if var.is_none()
            && let Some(i) = spec.arg
        {
            var = args
                .iter()
                .filter(|a| matches!(a.kind, GArgKind::Positional))
                .nth(i)
                .map(|a| a.var);
        }
        let Some(mut v) = var else {
            return Host::Dynamic;
        };
        // `SendAsync(new HttpRequestMessage(HttpMethod.Post, url))`: the URL the request was
        // built with.
        if let Some(i) = spec.built_arg
            && self.literal_prefix(v, 0).is_none()
            && let Some((args, _)) = self.defining_call(v, 0)
            && let Some(a) = args
                .iter()
                .filter(|a| matches!(a.kind, GArgKind::Positional))
                .nth(i)
        {
            v = a.var;
        }
        if let Some(field) = &spec.field
            && self.literal_prefix(v, 0).is_none()
        {
            // An options object: `axios({ url: '...' })`, `new HttpRequestMessage { RequestUri =
            // ... }` held in a variable.
            let sym = self.p.syms.get(field);
            let objs = [v, self.copy_origin(v, 0)];
            let store = self.p.stmts.iter().find_map(|s| match &s.kind {
                GKind::Store {
                    obj,
                    field: Some(k),
                    src,
                } if objs.contains(obj) && Some(*k) == sym => Some(*src),
                _ => None,
            });
            match store {
                Some(src) => v = src,
                None if spec.built_arg.is_some() => {}
                None => return Host::Dynamic,
            }
        }
        // `new Uri("https://...")`: the URL is the wrapper's argument.
        for _ in 0..4 {
            if self.literal_prefix(v, 0).is_some() || spec.wrappers.is_empty() {
                break;
            }
            match self.defining_call(v, 0) {
                Some((args, paths)) if paths.iter().any(|p| api_matches_any(&spec.wrappers, p)) => {
                    match args.iter().find(|a| matches!(a.kind, GArgKind::Positional)) {
                        Some(a) => v = a.var,
                        None => break,
                    }
                }
                _ => break,
            }
        }
        match self.literal_prefix(v, 0) {
            Some((prefix, complete)) => match host_of(&prefix, complete) {
                Host::Relative if spec.base_url => Host::Dynamic,
                h => h,
            },
            None => Host::Dynamic,
        }
    }

    // ------------------------------------------------------------------ handlers and seeds

    fn find_handlers(&mut self) -> Vec<Handler> {
        let cat = self.inp.catalogue;
        let mut found: Vec<(usize, FuncId)> = Vec::new();
        for (si, src) in cat.sources.iter().enumerate() {
            match src.def.kind {
                SourceKind::Handler => {
                    if !src.registrations.is_empty() {
                        for (stmt, t) in &self.p.calls {
                            let file = self.p.stmts[*stmt as usize].file;
                            if self.lang(file) != src.def.language {
                                continue;
                            }
                            if !t.apis.iter().any(|a| {
                                src.registrations
                                    .iter()
                                    .any(|g| g.is_match(self.p.syms.str(*a)))
                            }) {
                                continue;
                            }
                            let GKind::Call { args, .. } = &self.p.stmts[*stmt as usize].kind
                            else {
                                continue;
                            };
                            for a in args {
                                let mut provs = self.p.prov[a.var as usize].clone();
                                // `fastify.route({ handler })`
                                if let Some(sym) = self.p.syms.get("handler")
                                    && let Some(ps) = self.p.fget(a.var, sym)
                                {
                                    provs.extend(ps.iter().cloned());
                                }
                                for prov in provs {
                                    if let Prov::Func(f) = prov {
                                        found.push((si, f));
                                    }
                                }
                            }
                        }
                    }
                    if !src.decorators.is_empty() {
                        for (fi, func) in self.p.funcs.iter().enumerate() {
                            if self.lang(func.file) != src.def.language {
                                continue;
                            }
                            let hit = func.decorators.iter().any(|d| {
                                self.p.prov[*d as usize].iter().any(|p| match p {
                                    Prov::Api(s) => src
                                        .decorators
                                        .iter()
                                        .any(|g| g.is_match(self.p.syms.str(*s))),
                                    _ => false,
                                })
                            });
                            if hit {
                                found.push((si, fi as FuncId));
                            }
                        }
                    }
                    if src.registrations.is_empty()
                        && src.decorators.is_empty()
                        && !src.files.is_empty()
                    {
                        for (fi, func) in self.p.funcs.iter().enumerate() {
                            if func.is_module || self.lang(func.file) != src.def.language {
                                continue;
                            }
                            let path = &self.p.files[func.file as usize].path;
                            if src.files.iter().any(|g| g.is_match(path)) {
                                found.push((si, fi as FuncId));
                            }
                        }
                    }
                }
                SourceKind::RouteExport => {
                    for (file, info) in self.p.files.iter().enumerate() {
                        if info.lang.family() != src.def.language
                            || !src.files.iter().any(|g| g.is_match(&info.path))
                        {
                            continue;
                        }
                        for name in &src.def.exports {
                            if let Some(v) = self.p.export_var(file as FileId, name) {
                                for prov in &self.p.prov[v as usize] {
                                    if let Prov::Func(f) = prov {
                                        found.push((si, *f));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Methods of classes: by base type or class attribute (controllers), or by a parameter's
        // type (gRPC services).
        for (si, src) in cat.sources.iter().enumerate() {
            if src.def.kind != SourceKind::Handler {
                continue;
            }
            if !src.bases.is_empty() || !src.class_decorators.is_empty() {
                for c in 0..self.p.classes.len() as ClassId {
                    let class = &self.p.classes[c as usize];
                    if self.lang(class.file) != src.def.language || class.is_interface {
                        continue;
                    }
                    let by_base = !src.bases.is_empty() && {
                        let paths = self.ancestor_paths(c);
                        paths
                            .iter()
                            .any(|p| src.bases.iter().any(|g| g.is_match(p)))
                    };
                    let by_attr = !src.class_decorators.is_empty() && {
                        let paths = self.api_paths_of(&class.decorators);
                        paths
                            .iter()
                            .any(|p| src.class_decorators.iter().any(|g| g.is_match(p)))
                    };
                    if !(by_base || by_attr) {
                        continue;
                    }
                    let methods: Vec<FuncId> = class.methods.iter().map(|(_, f)| *f).collect();
                    for f in methods {
                        if self.member_selected(f, src.def.members.as_ref()) {
                            found.push((si, f));
                        }
                    }
                }
            }
            if !src.has_param_type.is_empty() {
                for f in 0..self.p.funcs.len() as FuncId {
                    let func = &self.p.funcs[f as usize];
                    if func.is_module || self.lang(func.file) != src.def.language {
                        continue;
                    }
                    let hit = func.params.iter().any(|q| {
                        self.api_paths_of(&[q.var])
                            .iter()
                            .any(|p| src.has_param_type.iter().any(|g| g.is_match(p)))
                    });
                    if hit && self.member_selected(f, src.def.members.as_ref()) {
                        found.push((si, f));
                    }
                }
            }
        }
        found.sort();
        found.dedup();
        let annots: HashMap<VarId, String> = self.p.annots.iter().cloned().collect();
        let mut handlers = Vec::new();
        for (si, f) in found {
            let src = &cat.sources[si];
            let func = &self.p.funcs[f as usize];
            let skip = usize::from(func.bound_self);
            let formals: Vec<GParam> = func.params.iter().skip(skip).cloned().collect();
            let mut params = Vec::new();
            let mut provenance = None;
            for spec in &src.def.params {
                let selected: Vec<&GParam> = if spec.all {
                    formals.iter().collect()
                } else if let Some(n) = &spec.name {
                    formals.iter().filter(|q| &q.name == n).collect()
                } else if let Some(i) = spec.index {
                    formals.get(i).into_iter().collect()
                } else {
                    Vec::new()
                };
                for q in selected {
                    let attr_paths = self.api_paths_of(&q.attrs);
                    let by_attr = spec
                        .by_attr
                        .iter()
                        .find(|(g, _)| {
                            attr_paths
                                .iter()
                                .any(|p| api_matches_any(std::slice::from_ref(*g), p))
                        })
                        .map(|(_, r)| r.clone());
                    let rule = by_attr.clone().unwrap_or_else(|| {
                        q.annot
                            .as_ref()
                            .and_then(|a| spec.by_type.get(a))
                            .cloned()
                            .unwrap_or_else(|| spec.rule.clone())
                    });
                    if by_attr.is_none() {
                        if self.param_skipped(
                            q.var,
                            q.annot.as_deref(),
                            &q.name,
                            &rule,
                            &attr_paths,
                        ) {
                            continue;
                        }
                        // `[AsParameters] OrderServices services`: each property is bound on
                        // its own, by the same rule.
                        if attr_paths
                            .iter()
                            .any(|p| api_matches_any(&rule.expand_attrs, p))
                        {
                            for (field, fvar) in self.declared_fields(q.var) {
                                let fannot = annots.get(&fvar).cloned();
                                if self.param_skipped(fvar, fannot.as_deref(), &field, &rule, &[]) {
                                    continue;
                                }
                                params.push(HandlerParam {
                                    var: q.var,
                                    name: format!("{}.{field}", q.name),
                                    annot: fannot,
                                    rule: rule.clone(),
                                    field: Some(field),
                                });
                            }
                            continue;
                        }
                    }
                    if let Some(pv) = &rule.provenance {
                        provenance = Some((q.var, pv.clone()));
                    }
                    params.push(HandlerParam {
                        var: q.var,
                        name: q.name.clone(),
                        annot: q.annot.clone(),
                        rule,
                        field: None,
                    });
                }
            }
            // A handler that selects nothing (Django: no `request` parameter) is not one.
            if params.is_empty() {
                continue;
            }
            *self.f.handlers.entry(src.def.id.clone()).or_default() += 1;
            handlers.push(Handler {
                source: si,
                func: f,
                params,
                provenance,
            });
        }
        handlers
    }

    #[allow(clippy::too_many_arguments)]
    fn param_seed(
        &self,
        var: VarId,
        token: Token,
        category: String,
        needs_review: bool,
        kind: SeedKind,
        by: String,
        name: String,
    ) -> Seed {
        let v = &self.p.vars[var as usize];
        Seed {
            var,
            token,
            category,
            needs_review,
            kind,
            file: v.file,
            pos: v.pos,
            text: name.clone(),
            by,
            name,
            param: None,
            declared_at: None,
            stmt: None,
        }
    }

    fn handler_seeds(&mut self, handlers: &[Handler]) {
        for h in handlers {
            let src_id = self.inp.catalogue.sources[h.source].def.id.clone();
            for hp in &h.params {
                let (var, name, annot, rule) = (&hp.var, &hp.name, &hp.annot, &hp.rule);
                if let Some(field) = &hp.field {
                    // One property of the parameter: classified by its name, or `whole`.
                    let place = Place {
                        object: None,
                        siblings: &[],
                    };
                    let mut cs = Vec::new();
                    if rule.by_name {
                        cs = self.inp.classifier.classify_in(field, &place);
                    }
                    let sym = self.p.syms.intern(field);
                    let mut cats: Vec<(String, bool, String)> = cs
                        .into_iter()
                        .map(|c| (c.category, c.needs_review, format!("{src_id}+{}", c.by)))
                        .collect();
                    if cats.is_empty()
                        && let Some(w) = &rule.whole
                    {
                        cats.push((w.clone(), false, src_id.clone()));
                    }
                    for (category, needs_review, by) in cats {
                        let s = self.param_seed(
                            *var,
                            named(sym),
                            category,
                            needs_review,
                            SeedKind::Request,
                            by,
                            name.clone(),
                        );
                        self.seed(s);
                    }
                    continue;
                }
                let mut narrowed = false;
                if rule.typed
                    && let Some(a) = annot
                {
                    narrowed = self.typed_seeds(*var, a, SeedKind::Request, &src_id);
                }
                if rule.by_name && !narrowed {
                    let others: Vec<String> = h.params.iter().map(|p| p.name.clone()).collect();
                    let place = Place {
                        object: None,
                        siblings: &others,
                    };
                    let cs = self.inp.classifier.classify_in(name, &place);
                    for c in &cs {
                        let s = self.param_seed(
                            *var,
                            TOP,
                            c.category.clone(),
                            c.needs_review,
                            SeedKind::Request,
                            format!("{src_id}+{}", c.by),
                            name.clone(),
                        );
                        self.seed(s);
                    }
                    narrowed |= !cs.is_empty();
                }
                // A number or a GUID is request input only when its name says what it is.
                let scalar = annot
                    .as_ref()
                    .is_some_and(|a| api_matches_any(&rule.classify_only_types, a));
                if let (Some(whole), false, false) = (&rule.whole, narrowed, scalar) {
                    let s = self.param_seed(
                        *var,
                        TOP,
                        whole.clone(),
                        false,
                        SeedKind::Request,
                        src_id.clone(),
                        name.clone(),
                    );
                    self.seed(s);
                }
                for (field, category) in &rule.fields {
                    let sym = self.p.syms.intern(field);
                    let s = self.param_seed(
                        *var,
                        named(sym),
                        category.clone(),
                        false,
                        SeedKind::Request,
                        src_id.clone(),
                        format!("{name}.{field}"),
                    );
                    self.seed(s);
                }
                if !rule.methods.is_empty() {
                    for (i, st) in self.p.stmts.iter().enumerate() {
                        let GKind::Call {
                            dst,
                            callee: GCallee::Method { recv, name: m },
                            ..
                        } = &st.kind
                        else {
                            continue;
                        };
                        if recv != var {
                            continue;
                        }
                        let mname = self.p.syms.str(*m).to_string();
                        if let Some(category) = rule.methods.get(&mname) {
                            let seed = self.stmt_seed(
                                i as u32,
                                *dst,
                                TOP,
                                category.clone(),
                                false,
                                SeedKind::Request,
                                src_id.clone(),
                                format!("{name}.{mname}()"),
                            );
                            if self
                                .seed_keys
                                .insert((seed.var, seed.token, seed.category.clone()))
                            {
                                self.f.seeds.push(seed);
                            }
                        }
                    }
                }
            }
            let _ = h.func;
        }
    }

    /// The API paths a variable's provenance holds (calls' `()` kept).
    fn api_paths_of(&self, vars: &[VarId]) -> Vec<String> {
        vars.iter()
            .flat_map(|v| self.p.prov[*v as usize].iter())
            .filter_map(|p| match p {
                Prov::Api(s) => Some(self.p.syms.str(*s).to_string()),
                _ => None,
            })
            .collect()
    }

    /// API paths of the external types a class extends or implements, through its local
    /// ancestors.
    fn ancestor_paths(&self, c: ClassId) -> Vec<String> {
        let mut out = Vec::new();
        for k in self.p.lineage(c) {
            let class = &self.p.classes[k as usize];
            out.extend(self.api_paths_of(&class.bases));
            out.extend(self.api_paths_of(&class.implements));
        }
        out
    }

    /// Whether a method of a matched class is a handler under the source's member filter.
    fn member_selected(&self, f: FuncId, filter: Option<&crate::catalogue::MemberFilter>) -> bool {
        let func = &self.p.funcs[f as usize];
        if func.is_property || func.is_stub {
            return false;
        }
        let Some(m) = filter else {
            return !func.is_ctor;
        };
        if m.require.iter().any(|r| !func.modifiers.contains(r)) {
            return false;
        }
        if m.exclude
            .iter()
            .any(|x| func.modifiers.contains(x) || (x == "constructor" && func.is_ctor))
        {
            return false;
        }
        let paths = self.api_paths_of(&func.decorators);
        !paths
            .iter()
            .any(|p| api_matches_any(&m.exclude_decorators, p))
    }

    /// Whether a handler parameter is not request input: an injected service, the request
    /// context, a cancellation token.
    fn param_skipped(
        &self,
        var: VarId,
        annot: Option<&str>,
        name: &str,
        rule: &ParamRule,
        attr_paths: &[String],
    ) -> bool {
        if rule.skip_names.iter().any(|n| n == name) {
            return true;
        }
        if annot.is_some_and(|a| api_matches_any(&rule.skip_types, a)) {
            return true;
        }
        if attr_paths
            .iter()
            .any(|p| api_matches_any(&rule.skip_attrs, p))
        {
            return true;
        }
        for p in &self.p.prov[var as usize] {
            match p {
                Prov::Api(s) => {
                    let path = self.p.syms.str(*s);
                    let path = path.strip_suffix("()").unwrap_or(path);
                    if api_matches_any(&rule.skip_types, path) {
                        return true;
                    }
                }
                Prov::Instance(c) => {
                    if rule.skip_interfaces && self.p.classes[*c as usize].is_interface {
                        return true;
                    }
                    if !rule.skip_bases.is_empty()
                        && self
                            .ancestor_paths(*c)
                            .iter()
                            .any(|p| api_matches_any(&rule.skip_bases, p))
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// The fields of the class a variable is declared with, and each one's variable.
    fn declared_fields(&self, var: VarId) -> Vec<(String, VarId)> {
        let mut out: BTreeMap<String, VarId> = BTreeMap::new();
        for p in &self.p.prov[var as usize] {
            let Prov::Instance(c) = p else { continue };
            let this = self.p.classes[*c as usize].this;
            for st in &self.p.stmts {
                if let GKind::ThisStore {
                    obj, field, var, ..
                } = st.kind
                    && obj == this
                {
                    out.entry(self.p.syms.str(field).to_string()).or_insert(var);
                }
            }
        }
        out.into_iter().collect()
    }

    /// Properties a framework sets from request input (`[SupplyParameterFromForm]`): their field
    /// variables are seeded.
    fn property_seeds(&mut self) {
        let cat = self.inp.catalogue;
        let annots: HashMap<VarId, String> = self.p.annots.iter().cloned().collect();
        for src in &cat.sources {
            if src.def.kind != SourceKind::Property {
                continue;
            }
            let rule = src
                .def
                .params
                .first()
                .map(|p| p.rule.clone())
                .unwrap_or_default();
            for c in 0..self.p.classes.len() as ClassId {
                let class = self.p.classes[c as usize].clone();
                if self.lang(class.file) != src.def.language || class.field_attrs.is_empty() {
                    continue;
                }
                if !src.bases.is_empty()
                    && !self
                        .ancestor_paths(c)
                        .iter()
                        .any(|p| src.bases.iter().any(|g| g.is_match(p)))
                {
                    continue;
                }
                if !src.class_decorators.is_empty()
                    && !self
                        .api_paths_of(&class.decorators)
                        .iter()
                        .any(|p| src.class_decorators.iter().any(|g| g.is_match(p)))
                {
                    continue;
                }
                for (field, attrs) in &class.field_attrs {
                    let paths = self.api_paths_of(attrs);
                    if !paths
                        .iter()
                        .any(|p| src.decorators.iter().any(|g| g.is_match(p)))
                    {
                        continue;
                    }
                    let Some(fsym) = self.p.syms.get(field) else {
                        continue;
                    };
                    let fvars: Vec<VarId> = self
                        .p
                        .stmts
                        .iter()
                        .filter_map(|st| match st.kind {
                            GKind::ThisStore {
                                obj, field, var, ..
                            }
                            | GKind::ThisLoad {
                                obj, field, var, ..
                            } if obj == class.this && field == fsym => Some(var),
                            _ => None,
                        })
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    // Cite the declaration: the first write of the field in the class's file.
                    let decl = self.p.stmts.iter().position(|st| {
                        matches!(st.kind, GKind::ThisStore { obj, field, .. }
                            if obj == class.this && field == fsym)
                    });
                    for fv in fvars {
                        *self.f.handlers.entry(src.def.id.clone()).or_default() += 1;
                        let before = self.f.seeds.len();
                        let mut narrowed = false;
                        if rule.typed
                            && let Some(a) = annots.get(&fv).cloned()
                        {
                            narrowed = self.typed_seeds(fv, &a, SeedKind::Request, &src.def.id);
                        }
                        if !narrowed && let Some(w) = &rule.whole {
                            let s = self.param_seed(
                                fv,
                                TOP,
                                w.clone(),
                                false,
                                SeedKind::Request,
                                src.def.id.clone(),
                                field.clone(),
                            );
                            self.seed(s);
                        }
                        if let Some(d) = decl {
                            let st = &self.p.stmts[d];
                            let (file, pos, text) = (st.file, st.pos, st.text.clone());
                            for seed in &mut self.f.seeds[before..] {
                                seed.file = file;
                                seed.pos = pos;
                                seed.text = text.clone();
                            }
                        }
                    }
                }
            }
        }
    }

    /// The declared fields of the types named `name`.
    fn type_fields(&self, name: &str) -> Vec<String> {
        self.types_by_name
            .get(name)
            .into_iter()
            .flatten()
            .flat_map(|&i| self.p.types[i].fields.iter().map(|f| f.0.clone()))
            .collect()
    }

    /// Seed the classified fields of a type on `var`. Returns true when the type is known.
    fn typed_seeds(
        &mut self,
        var: VarId,
        type_name: &str,
        kind: SeedKind,
        by_prefix: &str,
    ) -> bool {
        let mut known = false;
        let mut fields: Vec<TypedField> = Vec::new();
        if let Some(idx) = self.types_by_name.get(type_name).cloned() {
            for i in idx {
                known = true;
                let t = &self.p.types[i];
                let names: Vec<String> = t.fields.iter().map(|f| f.0.clone()).collect();
                let place = Place {
                    object: Some(&t.name),
                    siblings: &names,
                };
                for (fname, fpos) in &t.fields {
                    for c in self.inp.classifier.classify_in(fname, &place) {
                        fields.push((
                            fname.clone(),
                            c.category,
                            c.needs_review,
                            c.by,
                            Some((t.file, *fpos)),
                        ));
                    }
                }
            }
        }
        for dm in self.inp.datamaps {
            for (fname, category) in dm.fields_of(type_name) {
                known = true;
                fields.push((fname, category, false, format!("datamap:{}", dm.path), None));
            }
        }
        fields.sort();
        fields.dedup();
        for (fname, category, needs_review, by, declared_at) in fields {
            let sym = self.p.syms.intern(&fname);
            let by = if by_prefix.is_empty() {
                by
            } else {
                format!("{by_prefix}+{by}")
            };
            let mut s = self.param_seed(
                var,
                named(sym),
                category,
                needs_review,
                kind,
                by,
                format!("{type_name}.{fname}"),
            );
            let vname = &self.p.vars[var as usize].name;
            if !vname.is_empty() {
                s.text = format!("{vname}.{fname} ({vname}: {type_name})");
            }
            s.declared_at = declared_at;
            self.seed(s);
        }
        known
    }

    fn annotation_seeds(&mut self) {
        let annots = self.p.annots.clone();
        for (var, type_name) in annots {
            self.typed_seeds(var, &type_name, SeedKind::Typed, "");
        }
    }

    fn annotated_result(&mut self, stmt: u32, dst: VarId, f: FuncId) {
        let Some(t) = self.p.funcs[f as usize].ret_annot.clone() else {
            return;
        };
        let before = self.f.seeds.len();
        self.typed_seeds(dst, &t, SeedKind::Typed, "");
        // Cite the call, not the temporary.
        let s = &self.p.stmts[stmt as usize];
        let (file, pos, text) = (s.file, s.pos, s.text.clone());
        for seed in &mut self.f.seeds[before..] {
            seed.file = file;
            seed.pos = pos;
            seed.text = text.clone();
        }
    }

    fn find_records(&self) -> HashSet<VarId> {
        let mut records = HashSet::new();
        for (i, st) in self.p.stmts.iter().enumerate() {
            // `email.Masked()` calls the project's extension method with `email`: it is used as
            // what it is, not as a record.
            if self
                .p
                .calls
                .get(&(i as u32))
                .is_some_and(|t| !t.receiver_first.is_empty())
            {
                continue;
            }
            match &st.kind {
                GKind::Load {
                    obj,
                    field: Some(k),
                    ..
                } => {
                    let lang = self.lang(st.file);
                    if !self
                        .inp
                        .catalogue
                        .non_propagating(lang, self.p.syms.str(*k))
                    {
                        records.insert(*obj);
                    }
                }
                GKind::Call {
                    callee: GCallee::Method { recv, name },
                    ..
                } if !self
                    .inp
                    .catalogue
                    .value_method(self.lang(st.file), self.p.syms.str(*name)) =>
                {
                    records.insert(*recv);
                }
                _ => {}
            }
        }
        records
    }

    /// Whether a value read into `dst` is used as a record: `dst` itself, or a variable it is
    /// copied into (`const email = data.email`), a step or two on.
    fn read_as_record(&self, dst: VarId) -> bool {
        let mut frontier = vec![dst];
        for _ in 0..3 {
            if frontier.iter().any(|v| self.records.contains(v)) {
                return true;
            }
            frontier = frontier
                .iter()
                .flat_map(|v| self.copies.get(v).into_iter().flatten().copied())
                .collect();
            if frontier.is_empty() {
                return false;
            }
        }
        frontier.iter().any(|v| self.records.contains(v))
    }

    fn param_name_seeds(&mut self) {
        let records = self.records.clone();
        for fi in 0..self.p.funcs.len() {
            let formals = self.f.formals[fi].clone();
            for (i, var) in formals.into_iter().enumerate() {
                let v = &self.p.vars[var as usize];
                if v.kind != VarKind::Param || v.name.is_empty() || records.contains(&var) {
                    continue;
                }
                let name = v.name.clone();
                let others: Vec<String> = self.p.funcs[fi]
                    .params
                    .iter()
                    .map(|q| q.name.clone())
                    .collect();
                let place = Place {
                    object: None,
                    siblings: &others,
                };
                for c in self.inp.classifier.classify_in(&name, &place) {
                    let mut s = self.param_seed(
                        var,
                        TOP,
                        c.category,
                        c.needs_review,
                        SeedKind::ParamName,
                        c.by,
                        name.clone(),
                    );
                    s.param = Some((fi as FuncId, i as u32));
                    self.seed(s);
                }
            }
        }
    }

    // ------------------------------------------------------------------ static gaps

    fn static_gaps(&mut self) {
        let mut by_lang: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (lang, path) in self.inp.unsupported_files {
            by_lang.entry(lang.clone()).or_default().push(path.clone());
        }
        for (lang, mut paths) in by_lang {
            paths.sort();
            self.f.static_gaps.push(StaticGap {
                kind: GapKind::UnsupportedLanguage,
                detail: format!("{lang}: {} file(s) not analysed", paths.len()),
                locations: paths.into_iter().map(|p| (p, None)).collect(),
            });
        }
        for info in &self.p.files {
            let errors: Vec<_> = info
                .notes
                .iter()
                .filter(|n| n.kind == NoteKind::ParseError)
                .collect();
            if let Some(first) = errors.first() {
                self.f.static_gaps.push(StaticGap {
                    kind: GapKind::ParseError,
                    detail: format!(
                        "{} syntax error(s); the file was analysed only as far as it parsed",
                        errors.len()
                    ),
                    locations: vec![(info.path.clone(), Some(first.pos))],
                });
            }
            for n in info
                .notes
                .iter()
                .filter(|n| n.kind == NoteKind::UnsupportedConstruct)
            {
                self.f.static_gaps.push(StaticGap {
                    kind: GapKind::UnsupportedConstruct,
                    detail: n.detail.clone(),
                    locations: vec![(info.path.clone(), Some(n.pos))],
                });
            }
        }
        // Frameworks whose request input is not modelled.
        let mut by_framework: BTreeMap<String, Vec<(String, Option<Pos>)>> = BTreeMap::new();
        for imp in &self.p.imports {
            let Target::External(m) = &imp.target else {
                continue;
            };
            let file_lang = self.p.files[imp.file as usize].lang;
            let lang = file_lang.family();
            let traits = file_lang.traits();
            let mut candidates = vec![m.clone()];
            if let (ImportKind::Named(n), true) = (&imp.kind, traits.dotted_api_paths) {
                candidates.push(format!("{m}.{n}"));
            }
            let sep = traits.module_sep.to_string();
            for fw in &self.inp.catalogue.frameworks {
                if fw.supported || fw.language != lang {
                    continue;
                }
                let matches = fw.modules.iter().any(|x| {
                    candidates
                        .iter()
                        .any(|c| c == x || c.starts_with(&format!("{x}{sep}")))
                });
                if matches {
                    by_framework
                        .entry(fw.name.clone())
                        .or_default()
                        .push((self.p.files[imp.file as usize].path.clone(), Some(imp.pos)));
                }
            }
        }
        // C# imports are global and implicit: a class that extends a framework's type (or carries
        // its attribute) uses it.
        for c in 0..self.p.classes.len() as ClassId {
            let class = &self.p.classes[c as usize];
            let lang = self.lang(class.file);
            let mut paths = self.ancestor_paths(c);
            paths.extend(self.api_paths_of(&class.decorators));
            let methods: Vec<VarId> = class
                .methods
                .iter()
                .flat_map(|(_, f)| self.p.funcs[*f as usize].decorators.iter().copied())
                .collect();
            paths.extend(self.api_paths_of(&methods));
            for fw in &self.inp.catalogue.frameworks {
                if fw.supported || fw.language != lang || fw.types.is_empty() {
                    continue;
                }
                if paths.iter().any(|p| api_matches_any(&fw.types, p)) {
                    by_framework.entry(fw.name.clone()).or_default().push((
                        self.p.files[class.file as usize].path.clone(),
                        Some(class.pos),
                    ));
                }
            }
        }
        for (name, mut locs) in by_framework {
            locs.sort();
            locs.dedup();
            self.f.static_gaps.push(StaticGap {
                kind: GapKind::UnsupportedFramework,
                detail: format!(
                    "{name}: request input is not modelled, so handler parameters are not sources, and calls into it are not analysed"
                ),
                locations: locs,
            });
        }
    }
}

/// An API path rooted at a JavaScript global the catalogue does not name (a declared global,
/// `globalThis.cache`): no module, so it is a run-time object of the program's own.
/// The name of what a field read reads from, from its text: `address` in `address.state`,
/// `billing_address` in `order.billing_address["state"]`, `tax_breakdown` in
/// `order.tax_breakdown[0]["state"]`.
fn receiver_ident(text: &str) -> Option<String> {
    let mut t = &text[..text.rfind(['.', '['])?];
    while let Some(rest) = t.strip_suffix(']') {
        t = &rest[..rest.rfind('[')?];
    }
    // The trailing identifier, by characters: the one before it may be multi-byte (`…`).
    let ident: String = t
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    (!ident.is_empty()).then_some(ident)
}

/// A call's arguments as a function receives them: an extension method reached as a method
/// (`x.Mask()`) takes the receiver first.
fn receiver_args(callee: &GCallee, args: &[GArg], receiver_first: bool) -> Vec<GArg> {
    match callee {
        GCallee::Method { recv, .. } if receiver_first => std::iter::once(GArg {
            var: *recv,
            kind: GArgKind::Positional,
        })
        .chain(args.iter().cloned())
        .collect(),
        _ => args.to_vec(),
    }
}

fn is_runtime_global(path: &str, lang: &str) -> bool {
    Lang::family_traits(lang).bare_paths_are_runtime_globals && !path.contains(':')
}

/// A path through a built-in container (`Map().get()`, `Array.from()`): the value was kept
/// there, which says nothing of what it is.
fn is_container_path(path: &str, lang: &str) -> bool {
    const CONTAINERS: &[&str] = &[
        "Map", "Set", "WeakMap", "WeakSet", "Array", "Object", "Promise",
    ];
    is_runtime_global(path, lang)
        && CONTAINERS.contains(&path.split(['.', '(']).next().unwrap_or(path))
}

/// The module a specifier belongs to: `@scope/pkg/sub` → `@scope/pkg`, `pkg/sub` → `pkg`,
/// Python `a.b.c` → `a`.
pub fn top_module(m: &str, lang: &str) -> String {
    let traits = Lang::family_traits(lang);
    if traits.packages {
        return m.split('.').next().unwrap_or(m).to_string();
    }
    if traits.dotted_api_paths {
        // A C# namespace is not a package: its first two segments name the vendor and product
        // (`Microsoft.Extensions`, `Serilog.Sinks`).
        let mut parts = m.split('.');
        return match (parts.next(), parts.next()) {
            (Some(a), Some(b)) => format!("{a}.{b}"),
            _ => m.to_string(),
        };
    }
    let mut parts = m.split('/');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) if a.starts_with('@') => format!("{a}/{b}"),
        (Some(a), _) => a.to_string(),
        _ => m.to_string(),
    }
}

/// The host of a URL literal, when the literal is known far enough to contain it.
pub fn host_of(prefix: &str, complete: bool) -> Host {
    let rest = if let Some(r) = prefix
        .strip_prefix("https://")
        .or_else(|| prefix.strip_prefix("http://"))
        .or_else(|| prefix.strip_prefix("//"))
    {
        r
    } else if prefix.starts_with('/') {
        return Host::Relative;
    } else if complete && !prefix.contains("://") && !prefix.is_empty() {
        // `fetch("api/users")`: relative to the page.
        return Host::Relative;
    } else {
        return Host::Dynamic;
    };
    let end = rest.find(['/', '?', '#', ':']);
    match end {
        Some(i) if i > 0 => Host::Literal(rest[..i].to_ascii_lowercase()),
        None if complete && !rest.is_empty() => Host::Literal(rest.to_ascii_lowercase()),
        _ => Host::Dynamic,
    }
}

/// The text of a call's callee, for dynamic-call gap details.
fn callee_text(text: &str) -> String {
    let t = text.split('(').next().unwrap_or(text).trim();
    if t.is_empty() {
        "<call>".into()
    } else {
        t.to_string()
    }
}

/// The last identifier before `.method(` in the call text: `this.logger.info(x)` → `logger`.
fn receiver_name(text: &str, method: &str, var_name: &str) -> String {
    if !var_name.is_empty() {
        return var_name.to_string();
    }
    let needle = format!(".{method}");
    let Some(i) = text.find(&needle) else {
        return String::new();
    };
    let before = &text[..i];
    let ident: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    ident
}

/// Category classes: categories that every sanitiser treats alike share summaries.
pub fn category_classes(
    seeds: &[Seed],
    removes: &[Vec<String>],
) -> (Vec<Vec<bool>>, HashMap<String, usize>) {
    let mut sigs: Vec<Vec<bool>> = Vec::new();
    let mut map = HashMap::new();
    let mut cats: Vec<&str> = seeds.iter().map(|s| s.category.as_str()).collect();
    cats.sort();
    cats.dedup();
    for c in cats {
        let sig: Vec<bool> = removes
            .iter()
            .map(|r| r.iter().any(|p| crate::classify::covers(p, c)))
            .collect();
        let idx = match sigs.iter().position(|s| *s == sig) {
            Some(i) => i,
            None => {
                sigs.push(sig);
                sigs.len() - 1
            }
        };
        map.insert(c.to_string(), idx);
    }
    (sigs, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://api.posthog.com/capture", true),
            Host::Literal("api.posthog.com".into())
        );
        assert_eq!(
            host_of("https://API.example.com", true),
            Host::Literal("api.example.com".into())
        );
        assert_eq!(host_of("https://api.exa", false), Host::Dynamic);
        assert_eq!(host_of("/api/users", true), Host::Relative);
        assert_eq!(host_of("", false), Host::Dynamic);
    }

    #[test]
    fn receiver_idents() {
        assert_eq!(receiver_ident("address.state").as_deref(), Some("address"));
        assert_eq!(
            receiver_ident("order.tax_breakdown[0][\"state\"]").as_deref(),
            Some("tax_breakdown")
        );
        // A multi-byte character before the identifier (truncated text ends in `…`).
        assert_eq!(receiver_ident("…café.state").as_deref(), Some("café"));
        assert_eq!(receiver_ident("x…é.id").as_deref(), Some("é"));
        assert_eq!(receiver_ident("…[0].id"), None);
        assert_eq!(receiver_ident("state"), None);
    }

    #[test]
    fn receivers() {
        assert_eq!(
            receiver_name("this.logger.info(user)", "info", ""),
            "logger"
        );
        assert_eq!(receiver_name("log.warn(x)", "warn", "log"), "log");
    }

    #[test]
    fn modules() {
        assert_eq!(top_module("@sentry/node/x", "javascript"), "@sentry/node");
        assert_eq!(top_module("koa", "javascript"), "koa");
        assert_eq!(top_module("aiohttp.web", "python"), "aiohttp");
    }
}
