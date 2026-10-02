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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use super::{Edge, EdgeKind, Facts};
use crate::ir::VarKind;
use crate::program::{GKind, GVar, Program, Sym, VarId};

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
}

impl Solver {
    fn new(nvars: usize) -> Self {
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

    fn field(&mut self, site: u32, k: Sym) -> u32 {
        if let Some(&n) = self.fields.get(&(site, k)) {
            return n;
        }
        let n = self.push_node();
        self.fields.insert((site, k), n);
        n
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
            let sites = self.sites(n);
            let stores = self.stores_into.get(&n).cloned().unwrap_or_default();
            for (src, k) in stores {
                for &s in &sites {
                    let f = self.field(s, k);
                    self.add_copy(src, f);
                }
            }
            let unknown = self.unknown(n);
            let loads = self.loads_from.get(&n).cloned().unwrap_or_default();
            for (dst, k) in loads {
                // A field of an unknown value is unknown.
                if unknown {
                    self.add(dst, UNKNOWN);
                }
                for &s in &sites {
                    let f = self.field(s, k);
                    self.add_copy(f, dst);
                    let d = self.field(s, DIRTY);
                    self.add_copy(d, dst);
                }
            }
        }
    }
}

/// Resolve the pending field reads into edges, adding a variable per written field of each
/// allocation site.
pub(super) fn resolve(p: &mut Program, f: &mut Facts, defs: &[Vec<u32>], loads: Vec<PendingLoad>) {
    let nvars = p.vars.len();
    let mut s = Solver::new(nvars);

    // Unknown by construction: anything but a local or temporary, and anything with a
    // definition other than a literal, a copy or a named field read.
    for (v, ds) in defs.iter().enumerate().take(nvars) {
        let kind = p.vars[v].kind;
        let modelled = matches!(kind, VarKind::Local | VarKind::Temp)
            && !ds.is_empty()
            && ds.iter().all(|&d| {
                matches!(
                    p.stmts[d as usize].kind,
                    GKind::Lit { .. } | GKind::Copy { .. } | GKind::Load { field: Some(_), .. }
                )
            });
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
    // Field writes, with their statements, for the edges to emit afterwards.
    let mut stores: Vec<(VarId, VarId, Sym, u32)> = Vec::new();
    for (from, edges) in f.out.iter().enumerate() {
        let from = from as u32;
        for e in edges {
            match (e.kind, &p.stmts[e.stmt as usize].kind) {
                (EdgeKind::Copy, GKind::Copy { .. }) => s.copy_out[from as usize].push(e.to),
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
    for l in &loads {
        s.loads_from
            .entry(l.obj)
            .or_default()
            .push((l.dst, l.field));
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
    for l in &loads {
        let sites = s.sites(l.obj);
        let dirty = sites
            .iter()
            .any(|&site| s.fields.get(&(site, DIRTY)).is_some_and(|&d| s.unknown(d)));
        if s.unknown(l.obj) || sites.is_empty() || dirty {
            new_edges.insert((l.obj, l.stmt, l.dst), EdgeKind::Load(l.field));
        }
        if s.unknown(l.obj) {
            continue;
        }
        for site in sites {
            if written.contains(&(site, l.field)) {
                field_edges.push(((site, l.field), l.dst, l.stmt, true));
            }
        }
    }
    let mut var_of: BTreeMap<(u32, Sym), VarId> = BTreeMap::new();
    for &(site, k) in &written {
        let st = &p.stmts[site as usize];
        let v = p.vars.len() as VarId;
        p.vars.push(GVar {
            name: format!("{{}}.{}", p.syms.str(k)),
            kind: VarKind::Temp,
            func: st.func,
            file: st.file,
            pos: st.pos,
        });
        p.prov.push(Vec::new());
        var_of.insert((site, k), v);
    }
    let mut funcs: BTreeMap<VarId, BTreeSet<u32>> = BTreeMap::new();
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
    for (&(site, _), &v) in &var_of {
        let owner = p.stmts[site as usize].func;
        // Shared when the site's function is a module, or the field is used from another
        // function: summaries stop at it, and the search continues from it.
        let shared = p.funcs[owner as usize].is_module
            || funcs
                .get(&v)
                .is_some_and(|fs| fs.iter().any(|&g| g != owner));
        debug_assert_eq!(f.shared.len(), v as usize);
        f.shared.push(shared);
    }
    for ((from, stmt, to), kind) in new_edges {
        f.out[from as usize].push(Edge { to, kind, stmt });
    }
}
