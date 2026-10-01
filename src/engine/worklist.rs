//! The worklist engine: function summaries by a dependency-driven worklist, then a shortest
//! path search from every seed. Deterministic by construction: every map is ordered, ties in
//! the priority queue break on `(dist, depth, var, token)`, and seeds run in parallel but are
//! collected in seed order.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, VecDeque};
use std::sync::Arc;

use rayon::prelude::*;

use super::{Options, Reach, Step, Target};
use crate::facts::*;
use crate::program::{FuncId, Program, VarId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum STarget {
    Ret(Token),
    Hit(u32),
    DepthBound(u32),
    Global(VarId, Token),
}

#[derive(Clone, Debug)]
struct Entry {
    target: STarget,
    depth: u32,
    path: Arc<Vec<Step>>,
}

/// (formal index, token) → entries.
type Summary = BTreeMap<(u32, Token), Vec<Entry>>;

type Key = (VarId, Token, u32);

/// What a summary contains, for detecting that a recomputation changed nothing.
type Signature = BTreeSet<((u32, Token), STarget, u32, u32)>;

enum Via {
    Start,
    Step(Key, Step),
    Splice {
        prev: Key,
        site: u32,
        formal: u32,
        path: Arc<Vec<Step>>,
        exit: bool,
    },
}

/// How a target was reached: at a state, plus a tail of steps after it.
#[derive(Clone)]
enum Tail {
    None,
    Steps(Vec<Step>),
    Splice {
        site: u32,
        formal: u32,
        path: Arc<Vec<Step>>,
    },
}

impl Tail {
    fn len(&self) -> u32 {
        match self {
            Tail::None => 0,
            Tail::Steps(s) => s.len() as u32,
            Tail::Splice { path, .. } => 1 + path.len() as u32,
        }
    }
}

/// What a witness cut by the depth bound was about to reach.
#[derive(Clone, Copy)]
enum Cut {
    State(VarId, Token),
    Hit(u32),
    Global(VarId, Token),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Summary(FuncId),
    Seed,
}

struct Ctx<'a> {
    f: &'a Facts,
    opts: Options,
    sigs: Vec<Vec<bool>>,
    ret_of: Vec<Option<FuncId>>,
}

pub fn run(f: &Facts, p: &Program, opts: Options) -> Vec<Reach> {
    if f.seeds.is_empty() {
        return Vec::new();
    }
    let (sigs, class_of) = category_classes(&f.seeds, &f.sanitiser_removes);
    let mut ret_of = vec![None; p.vars.len()];
    for (i, func) in p.funcs.iter().enumerate() {
        ret_of[func.ret as usize] = Some(i as FuncId);
    }
    let ctx = Ctx {
        f,
        opts,
        sigs,
        ret_of,
    };
    let summaries = ctx.summaries(p);
    let per_seed: Vec<Vec<Reach>> = f
        .seeds
        .par_iter()
        .enumerate()
        .map(|(i, s)| {
            let class = class_of[&s.category];
            let found = ctx.search(
                &summaries,
                class,
                (s.var, s.token),
                Mode::Seed,
                &mut BTreeSet::new(),
            );
            // One witness per target: the shortest, then the shallowest.
            let mut best: BTreeMap<Target, (u32, u32, Vec<Step>)> = BTreeMap::new();
            for (t, depth, dist, path) in found {
                let target = match t {
                    STarget::Hit(h) => Target::Hit(h),
                    STarget::DepthBound(s) => Target::DepthBound(s),
                    _ => continue,
                };
                let better = best
                    .get(&target)
                    .is_none_or(|(d0, dep0, _)| (dist, depth) < (*d0, *dep0));
                if better {
                    best.insert(target, (dist, depth, path));
                }
            }
            best.into_iter()
                .map(|(target, (_, depth, path))| Reach {
                    seed: i as u32,
                    target,
                    depth,
                    path,
                })
                .collect()
        })
        .collect();
    per_seed.into_iter().flatten().collect()
}

impl<'a> Ctx<'a> {
    fn loads(&self, g: FuncId, k: u32) -> bool {
        self.f.load_fields[g as usize].binary_search(&k).is_ok()
    }

    fn apply(&self, kind: EdgeKind, t: Token, class: usize, mode: Mode) -> Option<Token> {
        match kind {
            EdgeKind::Copy => Some(t),
            EdgeKind::Collapse => Some(TOP),
            EdgeKind::Load(k) => {
                if t == TOP || t == named(k) {
                    Some(TOP)
                } else if t == PHI {
                    match mode {
                        Mode::Summary(g) if !self.loads(g, k) => Some(TOP),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            EdgeKind::Store(k) => Some(named(k)),
            EdgeKind::Sanitize(s) => {
                if self.sigs[class][s as usize] {
                    None
                } else {
                    Some(TOP)
                }
            }
        }
    }

    /// The summary entries of `h` that apply to token `t` arriving at formal `formal`, with the
    /// substitution for `PHI` in their results.
    fn lookup<'s>(
        &self,
        summary: &'s Summary,
        h: FuncId,
        formal: u32,
        t: Token,
        mode: Mode,
    ) -> Vec<(&'s Entry, Option<Token>)> {
        let mut out = Vec::new();
        let mut push = |key: (u32, Token), subst: Option<Token>| {
            if let Some(es) = summary.get(&key) {
                for e in es {
                    out.push((e, subst));
                }
            }
        };
        if t == TOP {
            push((formal, TOP), None);
        } else if t == PHI {
            push((formal, PHI), None);
            if let Mode::Summary(g) = mode {
                for &k in &self.f.load_fields[h as usize] {
                    if !self.loads(g, k) {
                        push((formal, named(k)), None);
                    }
                }
            }
        } else {
            let k = token_sym(t).expect("named token");
            if self.loads(h, k) {
                push((formal, t), None);
            } else {
                push((formal, PHI), Some(t));
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn search(
        &self,
        summaries: &[Vec<Summary>],
        class: usize,
        start: (VarId, Token),
        mode: Mode,
        consulted: &mut BTreeSet<FuncId>,
    ) -> Vec<(STarget, u32, u32, Vec<Step>)> {
        let f = self.f;
        let max = self.opts.max_depth;
        let mut best: HashMap<Key, u32> = HashMap::new();
        let mut via: HashMap<Key, Via> = HashMap::new();
        let mut heap: BinaryHeap<Reverse<(u32, u32, VarId, Token)>> = BinaryHeap::new();
        // (target, depth) → (dist, state, tail)
        let mut found: BTreeMap<(STarget, u32), (u32, Key, Tail)> = BTreeMap::new();
        // Where the depth bound stopped a witness, and what that witness was going to reach.
        let mut cuts: Vec<(Cut, u32, u32, u32, Key, Tail)> = Vec::new();
        let start_key = (start.0, start.1, 0);
        best.insert(start_key, 0);
        via.insert(start_key, Via::Start);
        heap.push(Reverse((0, 0, start.0, start.1)));

        let record = |found: &mut BTreeMap<(STarget, u32), (u32, Key, Tail)>,
                      t: STarget,
                      depth: u32,
                      dist: u32,
                      at: Key,
                      tail: Tail| {
            let total = dist + tail.len();
            let slot = found.entry((t, depth));
            match slot {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert((total, at, tail));
                }
                std::collections::btree_map::Entry::Occupied(mut o) => {
                    if total < o.get().0 {
                        o.insert((total, at, tail));
                    }
                }
            }
        };

        while let Some(Reverse((d, depth, v, t))) = heap.pop() {
            let key = (v, t, depth);
            if best.get(&key).is_some_and(|&b| b < d) {
                continue;
            }
            let relax = |heap: &mut BinaryHeap<Reverse<(u32, u32, VarId, Token)>>,
                         best: &mut HashMap<Key, u32>,
                         via: &mut HashMap<Key, Via>,
                         to: Key,
                         nd: u32,
                         how: Via| {
                if best.get(&to).is_none_or(|&b| nd < b) {
                    best.insert(to, nd);
                    via.insert(to, how);
                    heap.push(Reverse((nd, to.2, to.0, to.1)));
                }
            };
            if let Mode::Summary(owner) = mode {
                if f.shared[v as usize] && (v, t) != start {
                    record(&mut found, STarget::Global(v, t), depth, d, key, Tail::None);
                    continue;
                }
                if self.ret_of[v as usize] == Some(owner) {
                    record(&mut found, STarget::Ret(t), depth, d, key, Tail::None);
                }
            }
            for &h in &f.hit_args[v as usize] {
                record(&mut found, STarget::Hit(h), depth, d, key, Tail::None);
            }
            for e in &f.out[v as usize] {
                if let Some(t2) = self.apply(e.kind, t, class, mode) {
                    relax(
                        &mut heap,
                        &mut best,
                        &mut via,
                        (e.to, t2, depth),
                        d + 1,
                        Via::Step(key, Step::Edge(e.stmt)),
                    );
                }
            }
            for b in &f.bindings[v as usize] {
                let site = f.sites[b.site as usize];
                let h = site.func;
                consulted.insert(h);
                let summary = &summaries[h as usize][class];
                for (entry, subst) in self.lookup(summary, h, b.formal, t, mode) {
                    let plen = entry.path.len() as u32;
                    let sub = |t3: Token| if t3 == PHI { subst.unwrap_or(PHI) } else { t3 };
                    let splice = || Tail::Splice {
                        site: b.site,
                        formal: b.formal,
                        path: entry.path.clone(),
                    };
                    let enter_only = || {
                        Tail::Steps(vec![Step::Enter {
                            site: b.site,
                            formal: b.formal,
                        }])
                    };
                    match entry.target {
                        STarget::Ret(t3) => {
                            let nd = depth + entry.depth + 2;
                            if nd > max {
                                cuts.push((
                                    Cut::State(site.dst, sub(t3)),
                                    b.site,
                                    depth,
                                    d,
                                    key,
                                    enter_only(),
                                ));
                                continue;
                            }
                            relax(
                                &mut heap,
                                &mut best,
                                &mut via,
                                (site.dst, sub(t3), nd),
                                d + 2 + plen,
                                Via::Splice {
                                    prev: key,
                                    site: b.site,
                                    formal: b.formal,
                                    path: entry.path.clone(),
                                    exit: true,
                                },
                            );
                        }
                        STarget::Hit(hh) => {
                            let nd = depth + entry.depth + 1;
                            if nd > max {
                                cuts.push((Cut::Hit(hh), b.site, depth, d, key, enter_only()));
                            } else {
                                record(&mut found, STarget::Hit(hh), nd, d, key, splice());
                            }
                        }
                        STarget::DepthBound(s2) => {
                            let nd = (depth + entry.depth + 1).min(max);
                            record(&mut found, STarget::DepthBound(s2), nd, d, key, splice());
                        }
                        STarget::Global(gv, t3) => {
                            let nd = depth + entry.depth + 1;
                            if nd > max {
                                let cut = match mode {
                                    Mode::Summary(_) => Cut::Global(gv, sub(t3)),
                                    Mode::Seed => Cut::State(gv, sub(t3)),
                                };
                                cuts.push((cut, b.site, depth, d, key, enter_only()));
                            } else if let Mode::Summary(_) = mode {
                                record(
                                    &mut found,
                                    STarget::Global(gv, sub(t3)),
                                    nd,
                                    d,
                                    key,
                                    splice(),
                                );
                            } else {
                                relax(
                                    &mut heap,
                                    &mut best,
                                    &mut via,
                                    (gv, sub(t3), nd),
                                    d + 1 + plen,
                                    Via::Splice {
                                        prev: key,
                                        site: b.site,
                                        formal: b.formal,
                                        path: entry.path.clone(),
                                        exit: false,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            if mode == Mode::Seed
                && let Some(g) = self.ret_of[v as usize]
            {
                for &s in &f.callers[g as usize] {
                    let nd = depth + 1;
                    let dst = f.sites[s as usize].dst;
                    if nd > max {
                        cuts.push((
                            Cut::State(dst, t),
                            s,
                            depth,
                            d,
                            key,
                            Tail::Steps(vec![Step::Exit { site: s }]),
                        ));
                        continue;
                    }
                    relax(
                        &mut heap,
                        &mut best,
                        &mut via,
                        (dst, t, nd),
                        d + 1,
                        Via::Step(key, Step::Exit { site: s }),
                    );
                }
            }
        }
        // The bound is reported only where it cut a witness to something not reached within
        // it some other way: a recursive call that would go round again, or a longer path to a
        // sink already found, is not a gap.
        let reached_states: std::collections::HashSet<(VarId, Token)> =
            best.keys().map(|(v, t, _)| (*v, *t)).collect();
        for (cut, site, depth, d, at, tail) in cuts {
            let reached = match cut {
                Cut::State(v, t) => reached_states.contains(&(v, t)),
                Cut::Hit(h) => found.keys().any(|(t, _)| *t == STarget::Hit(h)),
                Cut::Global(v, t) => found.keys().any(|(x, _)| *x == STarget::Global(v, t)),
            };
            if !reached {
                record(&mut found, STarget::DepthBound(site), depth, d, at, tail);
            }
        }
        found
            .into_iter()
            .map(|((target, depth), (dist, at, tail))| {
                let mut path = reconstruct(&via, at);
                match tail {
                    Tail::None => {}
                    Tail::Steps(s) => path.extend(s),
                    Tail::Splice {
                        site,
                        formal,
                        path: inner,
                    } => {
                        path.push(Step::Enter { site, formal });
                        path.extend(inner.iter().copied());
                    }
                }
                (target, depth, dist, path)
            })
            .collect()
    }

    fn summaries(&self, p: &Program) -> Vec<Vec<Summary>> {
        let f = self.f;
        let nclasses = self.sigs.len().max(1);
        let nfuncs = f.callers.len();
        let mut out: Vec<Vec<Summary>> = vec![vec![Summary::new(); nclasses]; nfuncs];
        let needed: Vec<FuncId> = (0..nfuncs as FuncId)
            .filter(|&g| !f.callers[g as usize].is_empty())
            .collect();
        if needed.is_empty() {
            return out;
        }
        // Bottom-up start order: callees before callers (DFS post-order over call sites).
        let mut calls_from: Vec<BTreeSet<FuncId>> = vec![BTreeSet::new(); nfuncs];
        for site in &f.sites {
            let mut caller = Some(p.stmts[site.stmt as usize].func);
            while let Some(c) = caller {
                calls_from[c as usize].insert(site.func);
                caller = p.funcs[c as usize].parent;
            }
        }
        let mut order = Vec::new();
        let mut seen = vec![false; nfuncs];
        for &g in &needed {
            post_order(g, &calls_from, &mut seen, &mut order);
        }
        let mut queue: VecDeque<FuncId> = order
            .into_iter()
            .filter(|g| !f.callers[*g as usize].is_empty())
            .collect();
        let mut queued: BTreeSet<FuncId> = queue.iter().copied().collect();
        let mut dependents: Vec<BTreeSet<FuncId>> = vec![BTreeSet::new(); nfuncs];
        let mut signatures: Vec<Signature> = vec![BTreeSet::new(); nfuncs];
        while let Some(g) = queue.pop_front() {
            queued.remove(&g);
            let mut consulted = BTreeSet::new();
            let mut new = vec![Summary::new(); nclasses];
            let mut tokens = vec![TOP, PHI];
            tokens.extend(f.load_fields[g as usize].iter().map(|&k| named(k)));
            for (class, slot) in new.iter_mut().enumerate() {
                for (j, &formal) in f.formals[g as usize].iter().enumerate() {
                    for &tok in &tokens {
                        let found = self.search(
                            &out,
                            class,
                            (formal, tok),
                            Mode::Summary(g),
                            &mut consulted,
                        );
                        let mut entries: Vec<Entry> = Vec::new();
                        let mut by_target: BTreeMap<STarget, Vec<(u32, u32, Vec<Step>)>> =
                            BTreeMap::new();
                        for (t, depth, dist, path) in found {
                            by_target.entry(t).or_default().push((depth, dist, path));
                        }
                        for (t, mut cands) in by_target {
                            cands.sort();
                            let mut kept: Vec<(u32, u32)> = Vec::new();
                            for (depth, dist, path) in cands {
                                if kept.iter().any(|&(d0, s0)| d0 <= depth && s0 <= dist) {
                                    continue;
                                }
                                kept.push((depth, dist));
                                entries.push(Entry {
                                    target: t,
                                    depth,
                                    path: Arc::new(path),
                                });
                            }
                        }
                        if !entries.is_empty() {
                            slot.insert((j as u32, tok), entries);
                        }
                    }
                }
            }
            for &h in &consulted {
                dependents[h as usize].insert(g);
            }
            let sig: BTreeSet<_> = new
                .iter()
                .enumerate()
                .flat_map(|(c, s)| {
                    s.iter().flat_map(move |(k, es)| {
                        es.iter().map(move |e| {
                            (*k, e.target, e.depth * 64 + c as u32, e.path.len() as u32)
                        })
                    })
                })
                .collect();
            if sig != signatures[g as usize] {
                signatures[g as usize] = sig;
                out[g as usize] = new;
                for &d in &dependents[g as usize] {
                    if queued.insert(d) {
                        queue.push_back(d);
                    }
                }
            }
        }
        out
    }
}

fn post_order(
    g: FuncId,
    calls_from: &[BTreeSet<FuncId>],
    seen: &mut [bool],
    out: &mut Vec<FuncId>,
) {
    // Iterative DFS, so deep call chains cannot overflow the stack.
    let mut stack: Vec<(FuncId, Vec<FuncId>)> = Vec::new();
    if seen[g as usize] {
        return;
    }
    seen[g as usize] = true;
    stack.push((g, calls_from[g as usize].iter().rev().copied().collect()));
    while let Some((node, children)) = stack.last_mut() {
        if let Some(c) = children.pop() {
            if !seen[c as usize] {
                seen[c as usize] = true;
                let next: Vec<FuncId> = calls_from[c as usize].iter().rev().copied().collect();
                stack.push((c, next));
            }
        } else {
            out.push(*node);
            stack.pop();
        }
    }
}

fn reconstruct(via: &HashMap<Key, Via>, mut at: Key) -> Vec<Step> {
    let mut rev: Vec<Step> = Vec::new();
    loop {
        match via.get(&at) {
            None | Some(Via::Start) => break,
            Some(Via::Step(prev, step)) => {
                rev.push(*step);
                at = *prev;
            }
            Some(Via::Splice {
                prev,
                site,
                formal,
                path,
                exit,
            }) => {
                if *exit {
                    rev.push(Step::Exit { site: *site });
                }
                for s in path.iter().rev() {
                    rev.push(*s);
                }
                rev.push(Step::Enter {
                    site: *site,
                    formal: *formal,
                });
                at = *prev;
            }
        }
    }
    rev.reverse();
    rev
}
