//! The Datalog engine: the same reachability as [`super::worklist`], written as an `ascent`
//! program over the same facts. It computes which seeds reach which hits (and where the depth
//! bound stops a witness); it does not reconstruct paths. The test suite runs it as an oracle
//! against the worklist engine on every fixture.
//!
//! Relation glossary: `fr` is summary reachability from a formal parameter (function, class,
//! formal, start token, variable, token, depth); `sum` is a summary entry (function, class,
//! formal, start token, kind, a, b, depth) where kind 0 is a return slot (a = token, b = the
//! slot), 1 a hit
//! (a = hit) and 2 a shared variable (a = variable, b = token); `sumgap` is a depth-bound gap
//! a summary carries (a call site); `tr` is reachability from a seed. The depth-bound rules
//! use negation over reachability, so they sit in a second stratum. `slot(g, v)` is a return slot
//! of `g` (its return value, or a field variable of a site it returns) and `exit(v, s, to)` is
//! where slot `v` lands at call site `s` (ADR 0009).

use std::collections::BTreeSet;

use ascent::ascent;

use super::{Options, Target};
use crate::facts::*;
use crate::program::Program;

const NONE: u32 = u32::MAX;

ascent! {
    struct Dl;

    relation edge_copy(u32, u32);
    relation edge_collapse(u32, u32);
    relation edge_load(u32, u32, u32);
    relation edge_store(u32, u32, u32);
    relation edge_san(u32, u32, u32);
    relation covers(u32, u32);
    relation loads(u32, u32);
    relation formal(u32, u32, u32);
    relation slot(u32, u32);
    relation exit(u32, u32, u32);
    relation binding(u32, u32, u32);
    relation site(u32, u32, u32);
    relation hitarg(u32, u32);
    relation shared(u32);
    relation stops(u32, u32);
    relation token_seed(u32, u32);
    relation summ_func(u32);
    relation class(u32);
    relation seed(u32, u32, u32, u32);
    relation maxd(u32);

    relation fr(u32, u32, u32, u32, u32, u32, u32);
    relation sum(u32, u32, u32, u32, u32, u32, u32, u32);
    relation fr_app(u32, u32, u32, u32, u32, u32, u32, u32, u32);

    relation tr(u32, u32, u32, u32, u32);
    relation tr_app(u32, u32, u32, u32, u32, u32, u32);
    relation reach(u32, u32);
    relation reach_gap(u32, u32);
    relation sumgap(u32, u32, u32, u32, u32);
    relation fr_reached(u32, u32, u32, u32, u32, u32);
    relation sum_hit(u32, u32, u32, u32, u32);
    relation sum_glob(u32, u32, u32, u32, u32, u32);
    relation tr_reached(u32, u32, u32);

    // ---------------------------------------------------------------- summaries
    // A shared variable stops a summary, except the summary owner's own return slots.
    stops(g, v) <-- summ_func(g), shared(v), !slot(g, v);

    fr(g, c, j, t0, v, t0, 0) <-- summ_func(g), class(c), formal(g, j, v), token_seed(g, t0);

    fr(g, c, j, t0, b, t, d) <-- fr(g, c, j, t0, a, t, d), !stops(g, a), edge_copy(a, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, _t, d), !stops(g, a), edge_collapse(a, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, t, d), !stops(g, a), edge_load(a, k, b), if *t == TOP || t == k;
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, t, d), if *t == PHI, !stops(g, a), edge_load(a, k, b), !loads(g, k);
    fr(g, c, j, t0, b, k, d) <-- fr(g, c, j, t0, a, _t, d), !stops(g, a), edge_store(a, k, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, _t, d), !stops(g, a), edge_san(a, s, b), !covers(c, s);

    sum(g, c, j, t0, 0, t, v, d) <-- fr(g, c, j, t0, v, t, d), slot(g, v);
    sum(g, c, j, t0, 1, h, 0, d) <-- fr(g, c, j, t0, v, _t, d), !stops(g, v), hitarg(v, h);
    sum(g, c, j, t0, 2, v, t, d) <-- fr(g, c, j, t0, v, t, d), stops(g, v);

    // Nested call sites, by caller token: TOP; a named field the callee reads; a named field it
    // does not (through the callee's PHI entry, substituting the field back); and PHI.
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == TOP, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        sum(h, c, jj, &TOP, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 >= 2, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        loads(h, t1), sum(h, c, jj, t1, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a2, b2, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 >= 2, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        !loads(h, t1), sum(h, c, jj, &PHI, kind, a, b, dh),
        let a2 = if *kind == 0 && *a == PHI { *t1 } else { *a },
        let b2 = if *kind == 2 && *b == PHI { *t1 } else { *b };
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == PHI, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        sum(h, c, jj, &PHI, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == PHI, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        loads(h, tk), !loads(g, tk), sum(h, c, jj, tk, kind, a, b, dh);

    fr(g, c, j, t0, to, a, raw + 2) <-- fr_app(g, c, j, t0, s, kind, a, b, raw), if *kind == 0, maxd(m), if raw + 2 <= *m, exit(b, s, to);
    sum(g, c, j, t0, 1, a, 0, raw + 1) <-- fr_app(g, c, j, t0, _s, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 <= *m;
    sum(g, c, j, t0, 2, a, b, raw + 1) <-- fr_app(g, c, j, t0, _s, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 <= *m;

    // ---------------------------------------------------------------- seeds
    tr(s, c, v, t, 0) <-- seed(s, c, v, t);
    tr(s, c, b, t, d) <-- tr(s, c, a, t, d), edge_copy(a, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, _t, d), edge_collapse(a, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, t, d), edge_load(a, k, b), if *t == TOP || t == k;
    tr(s, c, b, k, d) <-- tr(s, c, a, _t, d), edge_store(a, k, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, _t, d), edge_san(a, san, b), !covers(c, san);
    reach(s, h) <-- tr(s, _c, v, _t, _d), hitarg(v, h);

    tr(s, c, to, t, d + 1) <-- tr(s, c, v, t, d), exit(v, _st, to), maxd(m), if d + 1 <= *m;

    tr_app(s, c, st, kind, a, b, d + dh) <--
        tr(s, c, v, t1, d), if *t1 == TOP, binding(v, st, jj), site(st, h, _dst), sum(h, c, jj, &TOP, kind, a, b, dh);
    tr_app(s, c, st, kind, a, b, d + dh) <--
        tr(s, c, v, t1, d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), loads(h, t1), sum(h, c, jj, t1, kind, a, b, dh);
    tr_app(s, c, st, kind, a2, b2, d + dh) <--
        tr(s, c, v, t1, d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), !loads(h, t1),
        sum(h, c, jj, &PHI, kind, a, b, dh),
        let a2 = if *kind == 0 && *a == PHI { *t1 } else { *a },
        let b2 = if *kind == 2 && *b == PHI { *t1 } else { *b };

    tr(s, c, to, a, raw + 2) <-- tr_app(s, c, st, kind, a, b, raw), if *kind == 0, maxd(m), if raw + 2 <= *m, exit(b, st, to);
    reach(s, a) <-- tr_app(s, _c, _st, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 <= *m;
    tr(s, c, a, b, raw + 1) <-- tr_app(s, c, _st, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 <= *m;

    // ---------------------------------------------------------------- depth-bound gaps
    // A second stratum: the bound is a gap only where it cut a witness to something not
    // reached within the bound some other way (normative; see engine/mod.rs).
    fr_reached(g, c, j, t0, v, t) <-- fr(g, c, j, t0, v, t, _d);
    sum_hit(g, c, j, t0, h) <-- sum(g, c, j, t0, k, h, _b, _d), if *k == 1;
    sum_glob(g, c, j, t0, v, t) <-- sum(g, c, j, t0, k, v, t, _d), if *k == 2;
    tr_reached(s, v, t) <-- tr(s, _c, v, t, _d);

    sumgap(g, c, j, t0, s) <-- fr_app(g, c, j, t0, s, kind, a, b, raw), if *kind == 0, maxd(m), if raw + 2 > *m,
        exit(b, s, to), !fr_reached(g, c, j, t0, to, a);
    sumgap(g, c, j, t0, s) <-- fr_app(g, c, j, t0, s, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 > *m,
        !sum_hit(g, c, j, t0, a);
    sumgap(g, c, j, t0, s) <-- fr_app(g, c, j, t0, s, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 > *m,
        !sum_glob(g, c, j, t0, a, b);
    // Gaps a callee's summary carries, by the same key matching as the other entries.
    sumgap(g, c, j, t0, s2) <-- fr(g, c, j, t0, v, t1, _d), if *t1 == TOP, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        sumgap(h, c, jj, &TOP, s2);
    sumgap(g, c, j, t0, s2) <-- fr(g, c, j, t0, v, t1, _d), if *t1 >= 2, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        loads(h, t1), sumgap(h, c, jj, t1, s2);
    sumgap(g, c, j, t0, s2) <-- fr(g, c, j, t0, v, t1, _d), if *t1 >= 2, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        !loads(h, t1), sumgap(h, c, jj, &PHI, s2);
    sumgap(g, c, j, t0, s2) <-- fr(g, c, j, t0, v, t1, _d), if *t1 == PHI, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        sumgap(h, c, jj, &PHI, s2);
    sumgap(g, c, j, t0, s2) <-- fr(g, c, j, t0, v, t1, _d), if *t1 == PHI, !stops(g, v), binding(v, s, jj), site(s, h, _dst),
        loads(h, tk), !loads(g, tk), sumgap(h, c, jj, tk, s2);

    reach_gap(s, st) <-- tr_app(s, _c, st, kind, a, b, raw), if *kind == 0, maxd(m), if raw + 2 > *m,
        exit(b, st, to), !tr_reached(s, to, a);
    reach_gap(s, st) <-- tr_app(s, _c, st, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 > *m, !reach(s, a);
    reach_gap(s, st) <-- tr_app(s, _c, st, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 > *m, !tr_reached(s, a, b);
    reach_gap(s, st) <-- tr(s, _c, v, t, d), exit(v, st, to), maxd(m), if d + 1 > *m, !tr_reached(s, to, t);
    reach_gap(s, s2) <-- tr(s, c, v, t1, _d), if *t1 == TOP, binding(v, st, jj), site(st, h, _dst), sumgap(h, c, jj, &TOP, s2);
    reach_gap(s, s2) <-- tr(s, c, v, t1, _d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), loads(h, t1), sumgap(h, c, jj, t1, s2);
    reach_gap(s, s2) <-- tr(s, c, v, t1, _d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), !loads(h, t1), sumgap(h, c, jj, &PHI, s2);
}

/// Every (seed, target) pair the program reaches.
pub fn run(f: &Facts, p: &Program, opts: Options) -> BTreeSet<(u32, Target)> {
    let mut out = BTreeSet::new();
    if f.seeds.is_empty() {
        return out;
    }
    let (sigs, class_of) = category_classes(&f.seeds, &f.sanitiser_removes);
    let mut dl = Dl::default();
    for (from, edges) in f.out.iter().enumerate() {
        let a = from as u32;
        for e in edges {
            match e.kind {
                EdgeKind::Copy => dl.edge_copy.push((a, e.to)),
                EdgeKind::Collapse => dl.edge_collapse.push((a, e.to)),
                EdgeKind::Load(k) => dl.edge_load.push((a, named(k), e.to)),
                EdgeKind::Store(k) => dl.edge_store.push((a, named(k), e.to)),
                EdgeKind::Sanitize(s) => dl.edge_san.push((a, s, e.to)),
            }
        }
    }
    for (c, sig) in sigs.iter().enumerate() {
        dl.class.push((c as u32,));
        for (s, &covered) in sig.iter().enumerate() {
            if covered {
                dl.covers.push((c as u32, s as u32));
            }
        }
    }
    for (g, fields) in f.load_fields.iter().enumerate() {
        for &k in fields {
            dl.loads.push((g as u32, named(k)));
        }
    }
    for (g, formals) in f.formals.iter().enumerate() {
        for (j, &v) in formals.iter().enumerate() {
            dl.formal.push((g as u32, j as u32, v));
        }
    }
    for (v, g) in f.slot_of.iter().enumerate() {
        if let Some(g) = g {
            dl.slot.push((*g, v as u32));
        }
    }
    for (v, es) in f.exits.iter().enumerate() {
        for &(st, to) in es {
            dl.exit.push((v as u32, st, to));
        }
    }
    for (g, _func) in p.funcs.iter().enumerate() {
        if !f.callers[g].is_empty() {
            dl.summ_func.push((g as u32,));
            dl.token_seed.push((g as u32, TOP));
            dl.token_seed.push((g as u32, PHI));
            for &k in &f.load_fields[g] {
                dl.token_seed.push((g as u32, named(k)));
            }
        }
    }
    for (v, bs) in f.bindings.iter().enumerate() {
        for b in bs {
            dl.binding.push((v as u32, b.site, b.formal));
        }
    }
    for (i, s) in f.sites.iter().enumerate() {
        dl.site.push((i as u32, s.func, s.dst));
    }
    for (v, hs) in f.hit_args.iter().enumerate() {
        for &h in hs {
            dl.hitarg.push((v as u32, h));
        }
    }
    for (v, &s) in f.shared.iter().enumerate() {
        if s {
            dl.shared.push((v as u32,));
        }
    }
    for (i, s) in f.seeds.iter().enumerate() {
        dl.seed
            .push((i as u32, class_of[&s.category] as u32, s.var, s.token));
    }
    dl.maxd.push((opts.max_depth,));
    dl.run();
    for (s, h) in dl.reach {
        out.insert((s, Target::Hit(h)));
    }
    for (s, st) in dl.reach_gap {
        if st != NONE {
            out.insert((s, Target::DepthBound(st)));
        }
    }
    out
}
