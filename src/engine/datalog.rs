//! The Datalog engine: the same reachability as [`super::worklist`], written as an `ascent`
//! program over the same facts. It computes which seeds reach which hits (and where the depth
//! bound stops a witness); it does not reconstruct paths. The test suite runs it as an oracle
//! against the worklist engine on every fixture.
//!
//! Relation glossary: `fr` is summary reachability from a formal parameter (function, class,
//! formal, start token, variable, token, depth); `sum` is a summary entry (function, class,
//! formal, start token, kind, a, b, depth) where kind 0 is a return (a = token), 1 a hit
//! (a = hit), 2 a shared variable (a = variable, b = token) and 3 a depth-bound gap (a = call
//! site); `tr` is reachability from a seed.

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
    relation ret(u32, u32);
    relation binding(u32, u32, u32);
    relation site(u32, u32, u32);
    relation hitarg(u32, u32);
    relation shared(u32);
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

    // ---------------------------------------------------------------- summaries
    fr(g, c, j, t0, v, t0, 0) <-- summ_func(g), class(c), formal(g, j, v), token_seed(g, t0);

    fr(g, c, j, t0, b, t, d) <-- fr(g, c, j, t0, a, t, d), !shared(a), edge_copy(a, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, _t, d), !shared(a), edge_collapse(a, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, t, d), !shared(a), edge_load(a, k, b), if *t == TOP || t == k;
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, t, d), if *t == PHI, !shared(a), edge_load(a, k, b), !loads(g, k);
    fr(g, c, j, t0, b, k, d) <-- fr(g, c, j, t0, a, _t, d), !shared(a), edge_store(a, k, b);
    fr(g, c, j, t0, b, TOP, d) <-- fr(g, c, j, t0, a, _t, d), !shared(a), edge_san(a, s, b), !covers(c, s);

    sum(g, c, j, t0, 0, t, 0, d) <-- fr(g, c, j, t0, v, t, d), ret(g, v);
    sum(g, c, j, t0, 1, h, 0, d) <-- fr(g, c, j, t0, v, _t, d), !shared(v), hitarg(v, h);
    sum(g, c, j, t0, 2, v, t, d) <-- fr(g, c, j, t0, v, t, d), shared(v);

    // Nested call sites, by caller token: TOP; a named field the callee reads; a named field it
    // does not (through the callee's PHI entry, substituting the field back); and PHI.
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == TOP, !shared(v), binding(v, s, jj), site(s, h, _dst),
        sum(h, c, jj, &TOP, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 >= 2, !shared(v), binding(v, s, jj), site(s, h, _dst),
        loads(h, t1), sum(h, c, jj, t1, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a2, b2, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 >= 2, !shared(v), binding(v, s, jj), site(s, h, _dst),
        !loads(h, t1), sum(h, c, jj, &PHI, kind, a, b, dh),
        let a2 = if *kind == 0 && *a == PHI { *t1 } else { *a },
        let b2 = if *kind == 2 && *b == PHI { *t1 } else { *b };
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == PHI, !shared(v), binding(v, s, jj), site(s, h, _dst),
        sum(h, c, jj, &PHI, kind, a, b, dh);
    fr_app(g, c, j, t0, s, kind, a, b, d + dh) <--
        fr(g, c, j, t0, v, t1, d), if *t1 == PHI, !shared(v), binding(v, s, jj), site(s, h, _dst),
        loads(h, tk), !loads(g, tk), sum(h, c, jj, tk, kind, a, b, dh);

    fr(g, c, j, t0, dst, a, raw + 2) <-- fr_app(g, c, j, t0, s, kind, a, _b, raw), if *kind == 0, maxd(m), if raw + 2 <= *m, site(s, _h, dst);
    sum(g, c, j, t0, 3, s, 0, 0) <-- fr_app(g, c, j, t0, s, kind, _a, _b, raw), if *kind == 0, maxd(m), if raw + 2 > *m;
    sum(g, c, j, t0, 1, a, 0, raw + 1) <-- fr_app(g, c, j, t0, _s, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 <= *m;
    sum(g, c, j, t0, 3, s, 0, 0) <-- fr_app(g, c, j, t0, s, kind, _a, _b, raw), if *kind == 1 || *kind == 2, maxd(m), if raw + 1 > *m;
    sum(g, c, j, t0, 3, a, 0, 0) <-- fr_app(g, c, j, t0, _s, kind, a, _b, _raw), if *kind == 3;
    sum(g, c, j, t0, 2, a, b, raw + 1) <-- fr_app(g, c, j, t0, _s, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 <= *m;

    // ---------------------------------------------------------------- seeds
    tr(s, c, v, t, 0) <-- seed(s, c, v, t);
    tr(s, c, b, t, d) <-- tr(s, c, a, t, d), edge_copy(a, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, _t, d), edge_collapse(a, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, t, d), edge_load(a, k, b), if *t == TOP || t == k;
    tr(s, c, b, k, d) <-- tr(s, c, a, _t, d), edge_store(a, k, b);
    tr(s, c, b, TOP, d) <-- tr(s, c, a, _t, d), edge_san(a, san, b), !covers(c, san);
    reach(s, h) <-- tr(s, _c, v, _t, _d), hitarg(v, h);

    tr(s, c, dst, t, d + 1) <-- tr(s, c, v, t, d), ret(g, v), site(_st, g, dst), maxd(m), if d + 1 <= *m;
    reach_gap(s, st) <-- tr(s, _c, v, _t, d), ret(g, v), site(st, g, _dst), maxd(m), if d + 1 > *m;

    tr_app(s, c, st, kind, a, b, d + dh) <--
        tr(s, c, v, t1, d), if *t1 == TOP, binding(v, st, jj), site(st, h, _dst), sum(h, c, jj, &TOP, kind, a, b, dh);
    tr_app(s, c, st, kind, a, b, d + dh) <--
        tr(s, c, v, t1, d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), loads(h, t1), sum(h, c, jj, t1, kind, a, b, dh);
    tr_app(s, c, st, kind, a2, b2, d + dh) <--
        tr(s, c, v, t1, d), if *t1 >= 2, binding(v, st, jj), site(st, h, _dst), !loads(h, t1),
        sum(h, c, jj, &PHI, kind, a, b, dh),
        let a2 = if *kind == 0 && *a == PHI { *t1 } else { *a },
        let b2 = if *kind == 2 && *b == PHI { *t1 } else { *b };

    tr(s, c, dst, a, raw + 2) <-- tr_app(s, c, st, kind, a, _b, raw), if *kind == 0, maxd(m), if raw + 2 <= *m, site(st, _h, dst);
    reach_gap(s, st) <-- tr_app(s, _c, st, kind, _a, _b, raw), if *kind == 0, maxd(m), if raw + 2 > *m;
    reach(s, a) <-- tr_app(s, _c, _st, kind, a, _b, raw), if *kind == 1, maxd(m), if raw + 1 <= *m;
    reach_gap(s, st) <-- tr_app(s, _c, st, kind, _a, _b, raw), if *kind == 1 || *kind == 2, maxd(m), if raw + 1 > *m;
    reach_gap(s, a) <-- tr_app(s, _c, _st, kind, a, _b, _raw), if *kind == 3;
    tr(s, c, a, b, raw + 1) <-- tr_app(s, c, _st, kind, a, b, raw), if *kind == 2, maxd(m), if raw + 1 <= *m;
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
    for (g, func) in p.funcs.iter().enumerate() {
        dl.ret.push((g as u32, func.ret));
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
