//! Allocation sites for plain objects.
//!
//! A `(variable, token)` node keeps one level of fields apart: `o = { a: { email } }` stores the
//! inner object into `(o, a)`, and reading `o.a` collapses it to the whole value, so `o.a.id`
//! carries the email address. Here every literal (`{...}`, `[...]`, a Python dict) is an
//! allocation site, and every named field of a site that is written gets a variable of its own,
//! as `this.f` does for class instances (ADR 0005). A flow-insensitive points-to analysis
//! (inclusion constraints over copies, field writes and field reads) finds which sites each
//! variable may hold; a read `o.k` of a variable that holds only known sites becomes a copy from
//! those sites' `k` variables, which keeps the value's own fields apart at any depth. Writes
//! through any alias reach the site's field variable, so `o2 = o; o2.a = x; log(o.a)` is seen.
//!
//! Everything this cannot see stays as it was: a variable that may hold a value from anywhere
//! else (a parameter, a call result, an import, a seed, a value built by string concatenation, a
//! catalogue flow) is unknown, and a read of an unknown variable keeps the `Load` edge. A site
//! written with a computed key (`o[k] = x`) is dirty, and reads of it keep the `Load` edge too.
//! Field writes keep their `Store` edge, so the whole object (`log(o)`) still carries every field.
//! Both engines read the resulting edges; neither knows about sites.
//!
//! **Across calls** (ADR 0009): a call that reaches only local functions gets, at its call site,
//! a clone of each site its callee returns, keyed by (call site, original literal), so a
//! wrapper's `return { data: { email, id } }` read as `r.data.id` in the caller keeps the inner
//! object's fields apart. A clone's field variables are filled through the call boundary, not by
//! edges: each callee-side field variable of a returned site is an extra **return slot** of the
//! callee, with an exit at each call site to the clone's variable. The engines apply slots as
//! they apply the return value, through summaries, so a helper called with an email address and
//! with an ID does not mix them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use super::{Edge, EdgeKind, Facts};
use crate::ir::VarKind;
use crate::program::{GCallee, GKind, GVar, Program, Sym, VarId};

/// A named field read whose edge waits for the points-to result.
pub(super) struct PendingLoad {
    pub obj: VarId,
    pub dst: VarId,
    pub field: Sym,
    pub stmt: u32,
}

/// The points-to element "a value from somewhere the analysis does not model".
const UNKNOWN: u32 = u32::MAX;
/// The field a computed-key write goes to: a site with it is dirty.
const DIRTY: Sym = Sym::MAX;
/// A variable that may hold more sites than this is treated as unknown, which bounds the
/// number of copies one read can produce.
const MAX_SITES: usize = 32;

/// How sites seen on one side of a call are seen on the other.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Mapping {
    /// Out of call site `c`'s callee into its caller: a site the callee owns is the call
    /// site's clone of it.
    Ret(u32),
    /// Into parameter `j` of function `f` at call site `c`, at a path of fields below it: any
    /// site is the formal site for (`f`, `j`, path).
    Arg {
        c: u32,
        f: u32,
        j: u32,
        path: Vec<Sym>,
    },
}

impl Mapping {
    /// The mapping for field `k` of what this maps.
    fn child(&self, k: Sym) -> Mapping {
        match self {
            Mapping::Ret(c) => Mapping::Ret(*c),
            Mapping::Arg { c, f, j, path } => {
                let mut path = path.clone();
                path.push(k);
                Mapping::Arg {
                    c: *c,
                    f: *f,
                    j: *j,
                    path,
                }
            }
        }
    }
}

/// A site that is not a literal.
#[derive(Clone, Debug)]
enum Extra {
    /// Call site `c`'s clone of a site its callee returns.
    Clone { c: u32, base: u32 },
    /// What a parameter of `f` holds, at a path of fields below it, across its call sites
    /// (keyed by function, parameter and path in `formal_id`).
    Formal { f: u32 },
}

/// Most variables hold one site or none, so sets are sorted vectors and the constraint lists
/// other than copies are sparse.
struct Solver {
    /// Sorted; `UNKNOWN`, the largest element, sorts last.
    pts: Vec<Vec<u32>>,
    copy_out: Vec<Vec<u32>>,
    /// The copies added while solving, which all touch a field node; statement copies are
    /// distinct edges already.
    dynamic: HashSet<(u32, u32)>,
    /// Target node → (source node, field) of field writes into it.
    stores_into: HashMap<u32, Vec<(u32, Sym)>>,
    /// Object node → (destination node, field) of field reads from it.
    loads_from: HashMap<u32, Vec<(u32, Sym)>>,
    /// (site, field) → node, for nodes past the program's variables.
    fields: BTreeMap<(u32, Sym), u32>,
    queue: VecDeque<u32>,
    queued: Vec<bool>,
    /// A node that holds only `UNKNOWN`: the source of computed-key writes.
    unknown_node: u32,
    /// Sites are statement indices of literals; extra sites are numbered from here.
    extra0: u32,
    extras: Vec<Extra>,
    clone_id: HashMap<(u32, u32), u32>,
    formal_id: HashMap<(u32, u32, Vec<Sym>), u32>,
    /// Clones made per call site, against `MAX_SITES`.
    clones_at: HashMap<u32, usize>,
    /// The function owning each site: the literal's, a clone's caller, a formal site's
    /// function.
    site_owner: Vec<u32>,
    /// Facts call sites: (callee, the function containing the call).
    call_sites: Vec<(u32, u32)>,
    mappings: Vec<Mapping>,
    mapping_id: HashMap<Mapping, u32>,
    /// Source node → (target node, mapping): the target holds the source's sites as seen on
    /// the other side of a call.
    mapped: HashMap<u32, Vec<(u32, u32)>>,
    /// (site, the site standing for it across a call, mapping).
    mirrors: BTreeSet<(u32, u32, u32)>,
    mirrors_of: HashMap<u32, Vec<(u32, u32)>>,
    fields_of_site: HashMap<u32, Vec<Sym>>,
    /// Site → its rest node: what spreads into a literal (`{ ...x }`, each spread copied into
    /// it), or a clone's or formal site's rest, filled through the call. A field read of the
    /// site also reads that field of the rest.
    rests: HashMap<u32, Vec<u32>>,
    /// (rest, the rest standing for it across a call, mapping): exits or bindings.
    rest_mirrors: BTreeSet<(u32, u32, u32)>,
    /// Field reads made of each site, so a rest the site gets later is read too.
    site_loads: HashMap<u32, Vec<(u32, Sym)>>,
    loads_done: HashSet<(u32, u32, Sym)>,
}

/// How deep below a parameter formal sites go; deeper is unknown.
const MAX_FORMAL_DEPTH: usize = 3;

impl Solver {
    fn new(nvars: usize, extra0: u32, site_owner: Vec<u32>, call_sites: Vec<(u32, u32)>) -> Self {
        let mut s = Solver {
            pts: vec![Vec::new(); nvars],
            copy_out: vec![Vec::new(); nvars],
            dynamic: HashSet::new(),
            stores_into: HashMap::new(),
            loads_from: HashMap::new(),
            fields: BTreeMap::new(),
            queue: VecDeque::new(),
            queued: vec![false; nvars],
            unknown_node: 0,
            extra0,
            extras: Vec::new(),
            clone_id: HashMap::new(),
            formal_id: HashMap::new(),
            clones_at: HashMap::new(),
            site_owner,
            call_sites,
            mappings: Vec::new(),
            mapping_id: HashMap::new(),
            mapped: HashMap::new(),
            mirrors: BTreeSet::new(),
            mirrors_of: HashMap::new(),
            fields_of_site: HashMap::new(),
            rests: HashMap::new(),
            rest_mirrors: BTreeSet::new(),
            site_loads: HashMap::new(),
            loads_done: HashSet::new(),
        };
        s.unknown_node = s.push_node();
        s.add(s.unknown_node, UNKNOWN);
        s
    }

    fn push_node(&mut self) -> u32 {
        let n = self.pts.len() as u32;
        self.pts.push(Vec::new());
        self.copy_out.push(Vec::new());
        self.queued.push(false);
        n
    }

    fn mapping(&mut self, m: Mapping) -> u32 {
        if let Some(&id) = self.mapping_id.get(&m) {
            return id;
        }
        let id = self.mappings.len() as u32;
        self.mappings.push(m.clone());
        self.mapping_id.insert(m, id);
        id
    }

    fn field(&mut self, site: u32, k: Sym) -> u32 {
        if let Some(&n) = self.fields.get(&(site, k)) {
            return n;
        }
        let n = self.push_node();
        self.fields.insert((site, k), n);
        self.fields_of_site.entry(site).or_default().push(k);
        // Every site standing for this one across a call gets the field too.
        for (other, m) in self.mirrors_of.get(&site).cloned().unwrap_or_default() {
            let to = self.field(other, k);
            let child = self.mappings[m as usize].child(k);
            let cm = self.mapping(child);
            self.add_mapped(n, to, cm);
        }
        n
    }

    fn owner(&self, site: u32) -> u32 {
        self.site_owner[site as usize]
    }

    fn extra(&self, site: u32) -> Option<&Extra> {
        site.checked_sub(self.extra0)
            .map(|i| &self.extras[i as usize])
    }

    fn base(&self, site: u32) -> u32 {
        match self.extra(site) {
            Some(Extra::Clone { base, .. }) => *base,
            _ => site,
        }
    }

    fn push_extra(&mut self, e: Extra, owner: u32) -> u32 {
        let id = self.extra0 + self.extras.len() as u32;
        self.extras.push(e);
        self.site_owner.push(owner);
        id
    }

    /// What `x` on one side of a call is on the other, under mapping `m`.
    fn map(&mut self, m: u32, x: u32) -> u32 {
        if x == UNKNOWN {
            return UNKNOWN;
        }
        let target = match self.mappings[m as usize].clone() {
            Mapping::Ret(c) => {
                let (callee, caller) = self.call_sites[c as usize];
                if self.owner(x) != callee {
                    return x;
                }
                let key = (c, self.base(x));
                match self.clone_id.get(&key) {
                    Some(&id) => id,
                    None => {
                        let n = self.clones_at.entry(c).or_default();
                        if *n >= MAX_SITES {
                            return UNKNOWN;
                        }
                        *n += 1;
                        let id = self.push_extra(Extra::Clone { c, base: key.1 }, caller);
                        self.clone_id.insert(key, id);
                        id
                    }
                }
            }
            Mapping::Arg { f, j, path, .. } => {
                if path.len() > MAX_FORMAL_DEPTH {
                    return UNKNOWN;
                }
                let key = (f, j, path.clone());
                match self.formal_id.get(&key) {
                    Some(&id) => id,
                    None => {
                        let id = self.push_extra(Extra::Formal { f }, f);
                        self.formal_id.insert(key, id);
                        id
                    }
                }
            }
        };
        if self.mirrors.insert((x, target, m)) {
            self.mirrors_of.entry(x).or_default().push((target, m));
            if let Some(&rest) = self.rests.get(&x).and_then(|r| r.first()) {
                self.mirror_rest(rest, target, m);
            }
            for k in self.fields_of_site.get(&x).cloned().unwrap_or_default() {
                let from = self.field(x, k);
                let to = self.field(target, k);
                let child = self.mappings[m as usize].child(k);
                let cm = self.mapping(child);
                self.add_mapped(from, to, cm);
            }
        }
        target
    }

    /// The rest of a site stands, across a call, for the rest of the site it maps to. A
    /// clone's rest holds the callee rest's sites as seen from the caller; a formal site's rest
    /// is unknown, so reads through it keep their `Load` edge.
    fn mirror_rest(&mut self, rest: u32, target: u32, m: u32) {
        let tr = self.rest_of(target);
        if !self.rest_mirrors.insert((rest, tr, m)) {
            return;
        }
        match self.mappings[m as usize] {
            Mapping::Ret(_) => self.add_mapped(rest, tr, m),
            Mapping::Arg { .. } => self.add(tr, UNKNOWN),
        }
    }

    /// A site's rest node, made on first use; reads already made of the site read it too.
    fn rest_of(&mut self, site: u32) -> u32 {
        if let Some(&r) = self.rests.get(&site).and_then(|r| r.first()) {
            return r;
        }
        let r = self.push_node();
        self.rests.insert(site, vec![r]);
        for (dst, k) in self.site_loads.get(&site).cloned().unwrap_or_default() {
            self.add_load(r, dst, k);
        }
        // Sites standing for this one get a rest standing for it.
        for (other, m) in self.mirrors_of.get(&site).cloned().unwrap_or_default() {
            self.mirror_rest(r, other, m);
        }
        r
    }

    fn add_mapped(&mut self, from: u32, to: u32, m: u32) {
        let list = self.mapped.entry(from).or_default();
        if list.contains(&(to, m)) {
            return;
        }
        list.push((to, m));
        for x in self.pts[from as usize].clone() {
            let y = self.map(m, x);
            self.add(to, y);
        }
    }

    fn enqueue(&mut self, n: u32) {
        if !self.queued[n as usize] {
            self.queued[n as usize] = true;
            self.queue.push_back(n);
        }
    }

    fn unknown(&self, n: u32) -> bool {
        self.pts[n as usize].last() == Some(&UNKNOWN)
    }

    /// Add one element; a set past the bound becomes `{UNKNOWN}` and stays so.
    fn add(&mut self, n: u32, x: u32) {
        let set = &mut self.pts[n as usize];
        if set.len() == 1 && set[0] == UNKNOWN {
            return;
        }
        match set.binary_search(&x) {
            Ok(_) => return,
            Err(i) => set.insert(i, x),
        }
        if set.len() > MAX_SITES {
            *set = vec![UNKNOWN];
        }
        self.enqueue(n);
    }

    /// A copy added while solving: deduplicated, and propagated at once.
    fn add_copy(&mut self, from: u32, to: u32) {
        if from == to || !self.dynamic.insert((from, to)) {
            return;
        }
        self.copy_out[from as usize].push(to);
        for x in self.pts[from as usize].clone() {
            self.add(to, x);
        }
    }

    fn sites(&self, n: u32) -> Vec<u32> {
        self.pts[n as usize]
            .iter()
            .copied()
            .filter(|&x| x != UNKNOWN)
            .collect()
    }

    fn solve(&mut self) {
        while let Some(n) = self.queue.pop_front() {
            self.queued[n as usize] = false;
            let xs = self.pts[n as usize].clone();
            for to in self.copy_out[n as usize].clone() {
                for &x in &xs {
                    self.add(to, x);
                }
            }
            for (to, m) in self.mapped.get(&n).cloned().unwrap_or_default() {
                for &x in &xs {
                    let y = self.map(m, x);
                    self.add(to, y);
                }
            }
            let sites = self.sites(n);
            let stores = self.stores_into.get(&n).cloned().unwrap_or_default();
            for (src, k) in stores {
                for &s in &sites {
                    let f = self.field(s, k);
                    self.add_copy(src, f);
                }
            }
            let loads = self.loads_from.get(&n).cloned().unwrap_or_default();
            for (dst, k) in loads {
                self.apply_load(n, dst, k);
            }
        }
    }

    /// `dst = n.k`: the field of each site, and the same read of each site's rest.
    fn apply_load(&mut self, n: u32, dst: u32, k: Sym) {
        // A field of an unknown value is unknown.
        if self.unknown(n) {
            self.add(dst, UNKNOWN);
        }
        for s in self.sites(n) {
            let reads = self.site_loads.entry(s).or_default();
            if !reads.contains(&(dst, k)) {
                reads.push((dst, k));
            }
            let f = self.field(s, k);
            self.add_copy(f, dst);
            let d = self.field(s, DIRTY);
            self.add_copy(d, dst);
            for r in self.rests.get(&s).cloned().unwrap_or_default() {
                self.add_load(r, dst, k);
            }
        }
    }

    fn add_load(&mut self, obj: u32, dst: u32, k: Sym) {
        if !self.loads_done.insert((obj, dst, k)) {
            return;
        }
        self.loads_from.entry(obj).or_default().push((dst, k));
        self.apply_load(obj, dst, k);
    }
}

/// A callee-side return slot and where it exits at one call site.
pub(super) struct SlotExit {
    pub slot: VarId,
    pub func: u32,
    pub site: u32,
    pub to: VarId,
}

/// Whether a call reaches local functions and nothing else, so its result is what they return.
fn local_only(p: &Program, stmt: u32) -> bool {
    p.calls.get(&stmt).is_some_and(|t| {
        !t.funcs.is_empty()
            && t.classes.is_empty()
            && t.apis.is_empty()
            && t.unresolved.is_empty()
            && !t.dynamic
            && t.plain_method.is_none()
    })
}

/// `p.catch(f)` and `p.finally(f)` on a JavaScript promise hold what `p` holds: the receiver,
/// for a call that is one of them.
fn promise_passthrough(p: &Program, stmt: u32) -> Option<VarId> {
    let st = &p.stmts[stmt as usize];
    if p.files[st.file as usize].lang.family() != "javascript" {
        return None;
    }
    match &st.kind {
        GKind::Call {
            callee: GCallee::Method { recv, name },
            ..
        } if matches!(p.syms.str(*name), "catch" | "finally") => Some(*recv),
        _ => None,
    }
}

/// Resolve the pending field reads into edges, adding a variable per written field of each
/// allocation site and of each call site's clones, and return the clones' slot exits.
pub(super) fn resolve(
    p: &mut Program,
    f: &mut Facts,
    defs: &[Vec<u32>],
    loads: Vec<PendingLoad>,
    inexact: &BTreeSet<(u32, u32)>,
) -> Vec<SlotExit> {
    let nvars = p.vars.len();
    let clone0 = p.stmts.len() as u32;
    let site_owner: Vec<u32> = p.stmts.iter().map(|st| st.func).collect();
    let call_sites: Vec<(u32, u32)> = f
        .sites
        .iter()
        .map(|site| (site.func, p.stmts[site.stmt as usize].func))
        .collect();
    let mut s = Solver::new(nvars, clone0, site_owner, call_sites);

    // Parameters every binding of which passes one argument to it, by position or keyword:
    // they hold what their arguments hold, as formal sites.
    let mut param_binds: BTreeMap<VarId, Vec<(VarId, u32, u32, u32)>> = BTreeMap::new();
    for (a, bs) in f.bindings.iter().enumerate() {
        for b in bs {
            let callee = f.sites[b.site as usize].func;
            let formal = f.formals[callee as usize][b.formal as usize];
            param_binds
                .entry(formal)
                .or_default()
                .push((a as VarId, b.site, callee, b.formal));
        }
    }
    let exact = |v: VarId, binds: &[(VarId, u32, u32, u32)]| {
        p.vars[v as usize].kind == VarKind::Param
            && defs[v as usize].is_empty()
            && binds.iter().all(|&(_, _, g, j)| !inexact.contains(&(g, j)))
    };

    // Unknown by construction: anything but a local, a temporary or a return slot, and
    // anything with a definition other than a literal, a copy, a named field read or a call
    // that reaches only local functions.
    for (v, ds) in defs.iter().enumerate().take(nvars) {
        let kind = p.vars[v].kind;
        let modelled = match kind {
            VarKind::Ret => ds.is_empty(),
            VarKind::Param => param_binds
                .get(&(v as VarId))
                .is_some_and(|b| exact(v as VarId, b)),
            VarKind::Local | VarKind::Temp => {
                !ds.is_empty()
                    && ds.iter().all(|&d| match p.stmts[d as usize].kind {
                        GKind::Lit { .. }
                        | GKind::Copy { .. }
                        | GKind::Load { field: Some(_), .. } => true,
                        GKind::Call { .. } => {
                            local_only(p, d) || promise_passthrough(p, d).is_some()
                        }
                        _ => false,
                    })
            }
            _ => false,
        };
        if !modelled {
            s.add(v as u32, UNKNOWN);
        }
    }
    for seed in &f.seeds {
        s.add(seed.var, UNKNOWN);
    }
    for (i, st) in p.stmts.iter().enumerate() {
        if let GKind::Lit { dst, .. } = st.kind {
            s.add(dst, i as u32);
        }
    }
    // A literal's own temporary: defined by the literal and by what spreads into it.
    let mut literal_of: HashMap<VarId, u32> = HashMap::new();
    for (i, st) in p.stmts.iter().enumerate() {
        if let GKind::Lit { dst, .. } = st.kind {
            let ds = &defs[dst as usize];
            let literals = ds
                .iter()
                .filter(|&&d| matches!(p.stmts[d as usize].kind, GKind::Lit { .. }))
                .count();
            if p.vars[dst as usize].kind == VarKind::Temp
                && literals == 1
                && ds.iter().all(|&d| {
                    matches!(
                        p.stmts[d as usize].kind,
                        GKind::Lit { .. } | GKind::Copy { .. }
                    )
                })
            {
                literal_of.insert(dst, i as u32);
            }
        }
    }
    // Field writes, with their statements, for the edges to emit afterwards.
    let mut stores: Vec<(VarId, VarId, Sym, u32)> = Vec::new();
    // (literal, what spreads into it, the spread's statement)
    let mut spreads: Vec<(u32, VarId, u32)> = Vec::new();
    for (from, edges) in f.out.iter().enumerate() {
        let from = from as u32;
        for e in edges {
            match (e.kind, &p.stmts[e.stmt as usize].kind) {
                (EdgeKind::Copy, GKind::Copy { .. }) if literal_of.contains_key(&e.to) => {
                    spreads.push((literal_of[&e.to], from, e.stmt));
                }
                (EdgeKind::Copy, GKind::Copy { .. } | GKind::Return { .. }) => {
                    s.copy_out[from as usize].push(e.to)
                }
                // The promise's value passes through `catch`/`finally`; the callback, a function
                // value, carries nothing.
                (EdgeKind::Collapse | EdgeKind::Copy, GKind::Call { .. })
                    if promise_passthrough(p, e.stmt).is_some() =>
                {
                    if promise_passthrough(p, e.stmt) == Some(from) {
                        s.copy_out[from as usize].push(e.to);
                    } else if !defs[from as usize]
                        .iter()
                        .all(|&d| matches!(p.stmts[d as usize].kind, GKind::FuncRef { .. }))
                        || defs[from as usize].is_empty()
                    {
                        s.add(e.to, UNKNOWN);
                    }
                }
                (EdgeKind::Store(k), GKind::Store { .. }) => {
                    s.stores_into.entry(e.to).or_default().push((from, k));
                    stores.push((from, e.to, k, e.stmt));
                }
                (EdgeKind::Collapse, GKind::Store { field: None, .. }) => {
                    let u = s.unknown_node;
                    s.stores_into.entry(e.to).or_default().push((u, DIRTY));
                }
                _ => s.add(e.to, UNKNOWN),
            }
        }
    }
    // A literal's rest is a node of its own, owned by the literal's function, that every spread
    // copies into: the spreads stay cited, and the rest can be a return slot.
    spreads.sort_unstable();
    for &(site, from, _) in &spreads {
        let r = s.rest_of(site);
        s.copy_out[from as usize].push(r);
    }
    for l in &loads {
        s.loads_from
            .entry(l.obj)
            .or_default()
            .push((l.dst, l.field));
    }
    // A call that reaches only local functions holds what they return, as seen from the call
    // site.
    for c in 0..f.sites.len() {
        let site = f.sites[c];
        if local_only(p, site.stmt) {
            let ret = p.funcs[site.func as usize].ret;
            let m = s.mapping(Mapping::Ret(c as u32));
            s.add_mapped(ret, site.dst, m);
        }
    }
    // A parameter holds, at each call site, its argument's sites as formal sites.
    for (&formal, binds) in &param_binds {
        if !exact(formal, binds) {
            continue;
        }
        for &(a, c, g, j) in binds {
            let m = s.mapping(Mapping::Arg {
                c,
                f: g,
                j,
                path: Vec::new(),
            });
            s.add_mapped(a, formal, m);
        }
    }
    for n in 0..nvars as u32 {
        s.enqueue(n);
    }
    s.solve();
    // A variable written to that holds no site keeps what is written in its own field tokens,
    // which only a `Load` edge reads: make it unknown, so every variable it flows into keeps
    // its `Load` edges too.
    loop {
        let mut empty: Vec<u32> = s
            .stores_into
            .keys()
            .copied()
            .filter(|&n| (n as usize) < nvars && s.pts[n as usize].is_empty())
            .collect();
        empty.sort_unstable();
        if empty.is_empty() {
            break;
        }
        for n in empty {
            s.add(n, UNKNOWN);
        }
        s.solve();
    }

    // Edges. Field variables exist only for fields something is written to.
    let mut new_edges: BTreeMap<(u32, u32, u32), EdgeKind> = BTreeMap::new();
    let mut written: BTreeSet<(u32, Sym)> = BTreeSet::new();
    // ((site, field), other end, statement, whether the field variable is the source)
    let mut field_edges: Vec<((u32, Sym), VarId, u32, bool)> = Vec::new();
    for &(src, obj, k, stmt) in &stores {
        for site in s.sites(obj) {
            written.insert((site, k));
            field_edges.push(((site, k), src, stmt, false));
        }
    }
    // A clone's field exists where the field it stands for does, through any number of calls.
    loop {
        let mut grew = false;
        for &(x, clone, _) in &s.mirrors {
            for &k in s
                .fields_of_site
                .get(&x)
                .map(|v| v.as_slice())
                .unwrap_or(&[])
            {
                if k != DIRTY && written.contains(&(x, k)) && written.insert((clone, k)) {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    // Where a site is: its literal, a clone's call, a formal site's function; formal sites'
    // variables are parameters.
    let site_at = |site: u32| -> (crate::program::FileId, crate::ir::Pos, VarKind) {
        match s.extra(site) {
            Some(Extra::Clone { c, .. }) => {
                let st = &p.stmts[f.sites[*c as usize].stmt as usize];
                (st.file, st.pos, VarKind::Temp)
            }
            Some(Extra::Formal { f: g, .. }) => {
                let func = &p.funcs[*g as usize];
                (func.file, func.pos, VarKind::Param)
            }
            None => {
                let st = &p.stmts[site as usize];
                (st.file, st.pos, VarKind::Temp)
            }
        }
    };
    let mut var_of: BTreeMap<(u32, Sym), VarId> = BTreeMap::new();
    for &(site, k) in &written {
        let (file, pos, kind) = site_at(site);
        let v = p.vars.len() as VarId;
        p.vars.push(GVar {
            name: format!("{{}}.{}", p.syms.str(k)),
            kind,
            func: s.owner(site),
            file,
            pos,
        });
        p.prov.push(Vec::new());
        var_of.insert((site, k), v);
    }
    // Rest variables: a literal's belongs to its function, a clone's to the caller.
    let mut rest_var: HashMap<u32, VarId> = HashMap::new();
    let mut rest_owner: Vec<(VarId, u32)> = Vec::new();
    let mut all_rests: Vec<(u32, u32)> = s.rests.iter().map(|(site, r)| (*site, r[0])).collect();
    all_rests.sort_unstable();
    for (clone, node) in all_rests {
        let (file, pos, kind) = site_at(clone);
        let v = p.vars.len() as VarId;
        p.vars.push(GVar {
            name: "{...}".to_string(),
            kind,
            func: s.owner(clone),
            file,
            pos,
        });
        p.prov.push(Vec::new());
        rest_var.insert(node, v);
        rest_owner.push((v, s.owner(clone)));
    }
    let var_of_node = |n: u32| -> VarId { rest_var.get(&n).copied().unwrap_or(n) };
    let mut funcs: BTreeMap<VarId, BTreeSet<u32>> = BTreeMap::new();
    // Each spread copies into its literal's rest, citing the spread.
    for &(site, from, stmt) in &spreads {
        let r = var_of_node(s.rests[&site][0]);
        new_edges.insert((from, stmt, r), EdgeKind::Copy);
        funcs
            .entry(r)
            .or_default()
            .insert(p.stmts[stmt as usize].func);
    }
    let node_of_var: HashMap<VarId, u32> = rest_var.iter().map(|(&n, &v)| (v, n)).collect();

    // Field reads, following each site's rests as further reads of the same field.
    let mut queue: VecDeque<(VarId, VarId, Sym, u32)> = loads
        .iter()
        .map(|l| (l.obj, l.dst, l.field, l.stmt))
        .collect();
    let mut seen: HashSet<(VarId, VarId, Sym, u32)> = HashSet::new();
    while let Some((obj, dst, k, stmt)) = queue.pop_front() {
        if !seen.insert((obj, dst, k, stmt)) {
            continue;
        }
        let n = node_of_var.get(&obj).copied().unwrap_or(obj);
        let sites = s.sites(n);
        let dirty = sites
            .iter()
            .any(|&site| s.fields.get(&(site, DIRTY)).is_some_and(|&d| s.unknown(d)));
        if s.unknown(n) || sites.is_empty() || dirty {
            new_edges.insert((obj, stmt, dst), EdgeKind::Load(k));
            if obj >= nvars as VarId {
                funcs
                    .entry(obj)
                    .or_default()
                    .insert(p.stmts[stmt as usize].func);
            }
        }
        if s.unknown(n) {
            continue;
        }
        for site in sites {
            if written.contains(&(site, k)) {
                field_edges.push(((site, k), dst, stmt, true));
            }
            for &r in s.rests.get(&site).map(|v| v.as_slice()).unwrap_or(&[]) {
                queue.push_back((var_of_node(r), dst, k, stmt));
            }
        }
    }
    for &(key, other, stmt, from_field) in &field_edges {
        let fv = var_of[&key];
        let (from, to) = if from_field { (fv, other) } else { (other, fv) };
        new_edges.insert((from, stmt, to), EdgeKind::Copy);
        funcs
            .entry(fv)
            .or_default()
            .insert(p.stmts[stmt as usize].func);
    }

    let added = p.vars.len() - nvars;
    f.out.extend((0..added).map(|_| Vec::new()));
    f.bindings.extend((0..added).map(|_| Vec::new()));
    f.hit_args.extend((0..added).map(|_| Vec::new()));
    // Shared when the owner is a module, or the variable is used from a function that is
    // neither its owner nor nested in it: summaries stop at it, and the search continues from
    // it. A closure's use is not sharing, as for the locals it captures.
    let within = |mut g: u32, owner: u32| loop {
        if g == owner {
            return true;
        }
        match p.funcs[g as usize].parent {
            Some(parent) => g = parent,
            None => return false,
        }
    };
    let shared = |v: VarId, owner: u32| {
        p.funcs[owner as usize].is_module
            || funcs
                .get(&v)
                .is_some_and(|fs| fs.iter().any(|&g| !within(g, owner)))
    };
    for (&(site, _), &v) in &var_of {
        debug_assert_eq!(f.shared.len(), v as usize);
        f.shared.push(shared(v, s.owner(site)));
    }
    for &(v, owner) in &rest_owner {
        debug_assert_eq!(f.shared.len(), v as usize);
        f.shared.push(shared(v, owner));
    }
    for ((from, stmt, to), kind) in new_edges {
        f.out[from as usize].push(Edge { to, kind, stmt });
    }
    // Out of a call, each field variable of a site the callee returns exits, at that call
    // site, to the field variable of the call site's clone, and its rest to the clone's rest.
    // Into a call, each field variable of an argument's site is bound, at that call site, to
    // the field variable of the parameter's formal site, which is a further formal parameter.
    let mut exits = Vec::new();
    let mut binds: BTreeSet<(VarId, u32, VarId)> = BTreeSet::new();
    let mut pairs: Vec<(VarId, VarId, u32)> = Vec::new();
    for &(x, target, m) in &s.mirrors {
        for (&(site, k), &from) in var_of.range((x, 0)..=(x, Sym::MAX)) {
            debug_assert_eq!(site, x);
            if let Some(&to) = var_of.get(&(target, k)) {
                pairs.push((from, to, m));
            }
        }
    }
    for &(rest, tr, m) in &s.rest_mirrors {
        pairs.push((var_of_node(rest), var_of_node(tr), m));
    }
    for (from, to, m) in pairs {
        match &s.mappings[m as usize] {
            Mapping::Ret(c) => exits.push(SlotExit {
                slot: from,
                func: s.call_sites[*c as usize].0,
                site: *c,
                to,
            }),
            Mapping::Arg { c, .. } => {
                binds.insert((from, *c, to));
            }
        }
    }
    let mut new_formals: BTreeSet<(u32, VarId)> = BTreeSet::new();
    for &(_, _, to) in &binds {
        new_formals.insert((p.vars[to as usize].func, to));
    }
    for (g, v) in new_formals {
        f.formals[g as usize].push(v);
    }
    for (from, c, to) in binds {
        let g = s.call_sites[c as usize].0;
        let formal = f.formals[g as usize]
            .iter()
            .position(|&x| x == to)
            .expect("formal") as u32;
        f.bindings[from as usize].push(super::Binding { site: c, formal });
    }
    exits
}
