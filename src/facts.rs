//! Flow facts: the graph the engines run on.
//!
//! The program's statements become edges between `(variable, field token)` nodes; the
//! catalogue turns calls into sink hits, sanitiser edges, propagation, or coverage gaps; the
//! classification table, type declarations, framework models and config turn reads and
//! parameters into seeds. Nothing here decides whether data reaches anything: that is the
//! engines' job, and both engines read exactly these facts.

mod heap;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::catalogue::{ArgSel, Catalogue, Flow, ParamRule, SinkClass, SourceKind};
use crate::classify::{Classifier, UNKNOWN};
use crate::datamap::Datamap;
use crate::ir::{ImportKind, NoteKind, Pos, VarKind};
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
    /// (formal var, formal name, annotation, rule)
    params: Vec<(VarId, String, Option<String>, ParamRule)>,
    provenance: Option<(VarId, String)>,
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
        for i in 0..self.p.stmts.len() {
            let stmt = i as u32;
            let s = self.p.stmts[i].clone();
            let lang = self.lang(s.file);
            match &s.kind {
                GKind::Copy { dst, src } => self.edge(*src, *dst, EdgeKind::Copy, stmt),
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

    fn field_read(&mut self, stmt: u32, obj: VarId, dst: VarId, name: &str) {
        // `models.User`, `Mailer.send`: a member of code, not of data. An instance (`this`) is
        // data, even though it carries its class as provenance.
        let instance = matches!(self.p.vars[obj as usize].kind, VarKind::This)
            || self.p.classes.iter().any(|c| c.ctor_this == Some(obj));
        if !instance
            && self.p.prov[obj as usize]
                .iter()
                .any(|p| matches!(p, Prov::Module(_) | Prov::Package(_) | Prov::Class(_)))
        {
            return;
        }
        for c in self.inp.classifier.classify(name) {
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
        for &(f, bound) in &t.funcs {
            if let Some(si) = self.local_sanitiser(f) {
                for &a in &Self::arg_vars(args) {
                    self.edge(a, dst, EdgeKind::Sanitize(si), stmt);
                }
                continue;
            }
            let site = self.f.sites.len() as u32;
            self.f.sites.push(Site { stmt, dst, func: f });
            self.f.callers[f as usize].push(site);
            self.bind_args(site, f, bound, args);
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
        let local = !t.funcs.is_empty() || !t.classes.is_empty();
        let mut explained = (!t.funcs.is_empty() && !t.open_world) || !t.classes.is_empty();
        let mut unknown = Vec::new();
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
                let category = src.def.category.clone().unwrap_or_else(|| UNKNOWN.into());
                let seed = self.stmt_seed(
                    stmt,
                    dst,
                    TOP,
                    category,
                    false,
                    SeedKind::Request,
                    src.def.id.clone(),
                    path.to_string(),
                );
                self.seed(seed);
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
            Flow::ArgsToResult => {
                for v in vars {
                    self.edge(v, dst, EdgeKind::Collapse, stmt);
                }
            }
            Flow::ArgsToReceiver => {
                if let Some(r) = recv {
                    for a in args {
                        self.edge(a.var, r, EdgeKind::Copy, stmt);
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
        let module = match lang {
            "python" => path.split('.').next().unwrap_or(path),
            _ => match path.split_once(':') {
                Some((m, _)) => m,
                None => return false,
            },
        };
        let sep = if lang == "python" { "." } else { "/" };
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
        if let Some(field) = &spec.field
            && self.literal_prefix(v, 0).is_none()
        {
            // An options object: `axios({ url: '...' })`.
            let sym = self.p.syms.get(field);
            let store = self.p.stmts.iter().find_map(|s| match &s.kind {
                GKind::Store {
                    obj,
                    field: Some(k),
                    src,
                } if *obj == v && Some(*k) == sym => Some(*src),
                _ => None,
            });
            match store {
                Some(src) => v = src,
                None => return Host::Dynamic,
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
        found.sort();
        found.dedup();
        let mut handlers = Vec::new();
        for (si, f) in found {
            let src = &cat.sources[si];
            let func = &self.p.funcs[f as usize];
            let skip = usize::from(func.bound_self);
            let formals: Vec<&GParam> = func.params.iter().skip(skip).collect();
            let mut params = Vec::new();
            let mut provenance = None;
            for spec in &src.def.params {
                let selected: Vec<&GParam> = if spec.all {
                    formals.clone()
                } else if let Some(n) = &spec.name {
                    formals.iter().copied().filter(|q| &q.name == n).collect()
                } else if let Some(i) = spec.index {
                    formals.get(i).copied().into_iter().collect()
                } else {
                    Vec::new()
                };
                for q in selected {
                    let rule = q
                        .annot
                        .as_ref()
                        .and_then(|a| spec.by_type.get(a))
                        .cloned()
                        .unwrap_or_else(|| spec.rule.clone());
                    if q.annot
                        .as_ref()
                        .is_some_and(|a| rule.skip_types.contains(a))
                        || rule.skip_names.contains(&q.name)
                    {
                        continue;
                    }
                    if let Some(pv) = &rule.provenance {
                        provenance = Some((q.var, pv.clone()));
                    }
                    params.push((q.var, q.name.clone(), q.annot.clone(), rule));
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
            for (var, name, annot, rule) in &h.params {
                let mut narrowed = false;
                if rule.typed
                    && let Some(a) = annot
                {
                    narrowed = self.typed_seeds(*var, a, SeedKind::Request, &src_id);
                }
                if rule.by_name && !narrowed {
                    let cs = self.inp.classifier.classify(name);
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
                if let (Some(whole), false) = (&rule.whole, narrowed) {
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
                for (fname, fpos) in &t.fields {
                    for c in self.inp.classifier.classify(fname) {
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

    fn param_name_seeds(&mut self) {
        for fi in 0..self.p.funcs.len() {
            let formals = self.f.formals[fi].clone();
            for (i, var) in formals.into_iter().enumerate() {
                let v = &self.p.vars[var as usize];
                if v.kind != VarKind::Param || v.name.is_empty() {
                    continue;
                }
                let name = v.name.clone();
                for c in self.inp.classifier.classify(&name) {
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
            let lang = self.p.files[imp.file as usize].lang.family();
            let mut candidates = vec![m.clone()];
            if let (ImportKind::Named(n), "python") = (&imp.kind, lang) {
                candidates.push(format!("{m}.{n}"));
            }
            let sep = if lang == "python" { "." } else { "/" };
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
fn is_runtime_global(path: &str, lang: &str) -> bool {
    lang == "javascript" && !path.contains(':')
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
    if lang == "python" {
        return m.split('.').next().unwrap_or(m).to_string();
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
