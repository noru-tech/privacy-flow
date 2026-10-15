//! Requests sent through a registered client (`catalogue/topology/clients.yml`): the base
//! address the registration sets, and so the system or processor they reach.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalogue::{Catalogue, SinkClass};
use crate::facts::{Facts, HitKind};
use crate::glob::api_matches_any;
use crate::program::{ClassId, GArgKind, GCallee, GKind, Program, Prov, VarId};
use crate::report::{SinkRec, fides_slug};
use crate::topology::catalogue::{ClientDef, ClientKind, TopologyCatalogue};
use crate::topology::model::{BindingRec, Endpoint, NoteRec};
use crate::topology::names::{Names, authority_of, is_local};
use crate::values::{Defs, api_paths};

pub struct Inputs<'a> {
    pub program: &'a Program,
    pub facts: &'a Facts,
    pub defs: &'a Defs,
    pub catalogue: &'a Catalogue,
    pub topo: &'a TopologyCatalogue,
    pub names: &'a Names,
    /// System keys with their closures (per program file).
    pub systems: &'a [(String, &'a [bool])],
}

/// API paths a call statement resolves to.
fn call_paths(p: &Program, stmt: u32) -> Vec<String> {
    let mut out: Vec<String> = p
        .calls
        .get(&stmt)
        .map(|t| t.apis.iter().map(|s| p.syms.str(*s).to_string()).collect())
        .unwrap_or_default();
    if let GKind::Call {
        callee: GCallee::Value(v),
        ..
    } = &p.stmts[stmt as usize].kind
    {
        out.extend(api_paths(p, *v));
    }
    out
}

/// The literal strings a value may be: through copies, and through the arguments of the
/// wrapper constructors (`new Uri("…")`, a target-typed `new("…")`).
fn literals(
    p: &Program,
    defs: &Defs,
    var: VarId,
    wrappers: &[String],
    depth: u32,
    out: &mut BTreeSet<String>,
) {
    if depth > 8 {
        return;
    }
    for &d in defs.of(var) {
        match &p.stmts[d as usize].kind {
            GKind::Lit { value: Some(v), .. } => {
                out.insert(v.clone());
            }
            GKind::Copy { src, .. } => literals(p, defs, *src, wrappers, depth + 1, out),
            GKind::Call { args, is_new, .. } => {
                let wrapped = call_paths(p, d).iter().any(|a| {
                    api_matches_any(wrappers, a) || api_matches_any(wrappers, &format!("{a}()"))
                });
                if (wrapped || (*is_new && call_paths(p, d).is_empty()))
                    && let Some(a) = args.first()
                {
                    literals(p, defs, a.var, wrappers, depth + 1, out);
                }
            }
            _ => {}
        }
    }
}

/// The values stored into `field` of `obj` (or of what it was copied from).
fn stored(p: &Program, defs: &Defs, obj: VarId, fields: &[String]) -> Vec<VarId> {
    let objs = [obj, defs.copy_origin(p, obj)];
    p.stmts
        .iter()
        .filter_map(|s| match &s.kind {
            GKind::Store {
                obj: o,
                field: Some(f),
                src,
            } if objs.contains(o) && fields.iter().any(|x| x == p.syms.str(*f)) => Some(*src),
            _ => None,
        })
        .collect()
}

/// The base addresses a registering call sets.
fn bases(p: &Program, defs: &Defs, def: &ClientDef, stmt: u32) -> BTreeSet<String> {
    let GKind::Call { args, .. } = &p.stmts[stmt as usize].kind else {
        return BTreeSet::new();
    };
    let c = &def.configure;
    let mut srcs: Vec<VarId> = Vec::new();
    if let Some(k) = &c.keyword {
        srcs.extend(
            args.iter()
                .filter(|a| matches!(&a.kind, GArgKind::Keyword(n) if n == k))
                .map(|a| a.var),
        );
    }
    if let Some(i) = c.arg {
        let positional: Vec<VarId> = args
            .iter()
            .filter(|a| matches!(a.kind, GArgKind::Positional))
            .map(|a| a.var)
            .collect();
        if let Some(&v) = positional.get(i) {
            if c.fields.is_empty() {
                srcs.push(v);
            } else {
                srcs.extend(stored(p, defs, v, &c.fields));
            }
        }
    }
    if !c.lambda_fields.is_empty() {
        for a in args {
            for pr in &p.prov[a.var as usize] {
                let Prov::Func(f) = pr else { continue };
                let Some(param) = p.funcs[*f as usize].params.first() else {
                    continue;
                };
                srcs.extend(stored(p, defs, param.var, &c.lambda_fields));
            }
        }
    }
    let mut out = BTreeSet::new();
    for s in srcs {
        literals(p, defs, s, &def.wrappers, 0, &mut out);
    }
    out
}

/// The calls a receiver comes from, through copies and the field it is held in.
fn origins(p: &Program, defs: &Defs, var: VarId, depth: u32, out: &mut BTreeSet<u32>) {
    if depth > 8 {
        return;
    }
    for &d in defs.of(var) {
        match &p.stmts[d as usize].kind {
            GKind::Copy { src, .. } => origins(p, defs, *src, depth + 1, out),
            GKind::Call { .. } => {
                out.insert(d);
            }
            GKind::ThisLoad { var: field_var, .. } => {
                origins(p, defs, *field_var, depth + 1, out);
            }
            _ => {}
        }
    }
    // A field variable is defined by the stores into it.
    for s in &p.stmts {
        if let GKind::ThisStore { src, var: v, .. } = &s.kind
            && *v == var
        {
            origins(p, defs, *src, depth + 1, out);
        }
    }
}

/// The classes a registering call's type argument names.
fn type_arg_classes(p: &Program, stmt: u32) -> BTreeSet<ClassId> {
    let st = &p.stmts[stmt as usize];
    let GKind::Call { dst, .. } = &st.kind else {
        return BTreeSet::new();
    };
    let mut out: BTreeSet<ClassId> = p.prov[*dst as usize]
        .iter()
        .filter_map(|pr| match pr {
            Prov::Instance(c) | Prov::Class(c) => Some(*c),
            _ => None,
        })
        .collect();
    for t in &p.stmts {
        if let GKind::TypeRef { dst: d, ty } = &t.kind
            && d == dst
            && t.file == st.file
        {
            out.extend(p.prov[*ty as usize].iter().filter_map(|pr| match pr {
                Prov::Instance(c) | Prov::Class(c) => Some(*c),
                _ => None,
            }));
        }
    }
    out
}

pub struct Bindings {
    pub recs: Vec<BindingRec>,
    pub notes: Vec<NoteRec>,
}

pub fn bind(inp: &Inputs, sinks: &[SinkRec]) -> Bindings {
    let p = inp.program;
    let defs = inp.defs;
    // Registrations: (client entry, statement, file).
    let mut regs: Vec<(&ClientDef, u32)> = Vec::new();
    for (i, st) in p.stmts.iter().enumerate() {
        if !matches!(st.kind, GKind::Call { .. }) {
            continue;
        }
        let lang = p.files[st.file as usize].lang.family();
        let paths = call_paths(p, i as u32);
        for c in &inp.topo.clients {
            if c.language == lang && paths.iter().any(|a| api_matches_any(&c.calls, a)) {
                regs.push((c, i as u32));
            }
        }
    }
    // The statements of each sink record, by place and catalogue entry.
    let mut sink_stmts: BTreeMap<(String, u32, u32, String), Vec<u32>> = BTreeMap::new();
    for h in &inp.facts.hits {
        let HitKind::Sink { sink, .. } = &h.kind else {
            continue;
        };
        let st = &p.stmts[h.stmt as usize];
        sink_stmts
            .entry((
                p.files[st.file as usize].path.clone(),
                st.pos.line,
                st.pos.column,
                inp.catalogue.sinks[*sink as usize].def.id.clone(),
            ))
            .or_default()
            .push(h.stmt);
    }

    let mut recs: BTreeSet<BindingRec> = BTreeSet::new();
    let mut notes: BTreeSet<NoteRec> = BTreeSet::new();
    for snk in sinks {
        if snk.class != SinkClass::Http || snk.host.as_ref().is_none_or(|h| h.kind != "dynamic") {
            continue;
        }
        let key = (
            snk.location.path.clone(),
            snk.location.line,
            snk.location.column,
            snk.catalogue_id.clone(),
        );
        for &stmt in sink_stmts.get(&key).into_iter().flatten() {
            let st = &p.stmts[stmt as usize];
            let GKind::Call {
                callee: GCallee::Method { recv, .. },
                ..
            } = &st.kind
            else {
                continue;
            };
            let mut from: BTreeSet<u32> = BTreeSet::new();
            origins(p, defs, *recv, 0, &mut from);
            let class = p.funcs[st.func as usize].class.or_else(|| {
                let mut f = p.funcs[st.func as usize].parent;
                while let Some(fi) = f {
                    if let Some(c) = p.funcs[fi as usize].class {
                        return Some(c);
                    }
                    f = p.funcs[fi as usize].parent;
                }
                None
            });
            let recv_apis: Vec<String> = api_paths(p, *recv).collect();
            for (def, reg) in &regs {
                let applies = match def.client {
                    ClientKind::Result => from.contains(reg),
                    ClientKind::TypeArg => {
                        recv_apis.iter().any(|a| api_matches_any(&def.injects, a))
                            && class.is_some_and(|c| type_arg_classes(p, *reg).contains(&c))
                    }
                };
                if !applies {
                    continue;
                }
                let reg_st = &p.stmts[*reg as usize];
                let reg_file = reg_st.file as usize;
                let registration = format!("{}:{}", p.files[reg_file].path, reg_st.pos.line);
                for base in bases(p, defs, def, *reg) {
                    let Some(host) = authority_of(&base) else {
                        continue;
                    };
                    let to = if let Some(k) = inp.names.by_host.get(&host) {
                        Endpoint::System { key: k.clone() }
                    } else if is_local(&host) || !host.contains('.') {
                        notes.insert(NoteRec {
                            kind: "unknown_host".into(),
                            detail: format!(
                                "{base} names no system of the repository; declare it under systems: hosts:"
                            ),
                            paths: vec![registration.clone()],
                        });
                        continue;
                    } else {
                        Endpoint::Processor {
                            name: host.clone(),
                            fides_key: fides_slug(&host),
                        }
                    };
                    for (system, closure) in inp.systems {
                        if !closure[reg_file] || !closure[st.file as usize] {
                            continue;
                        }
                        if matches!(&to, Endpoint::System { key } if key == system) {
                            continue;
                        }
                        recs.insert(BindingRec {
                            sink: snk.id.clone(),
                            system: system.clone(),
                            client: def.id.clone(),
                            registration: registration.clone(),
                            base: base.clone(),
                            to: to.clone(),
                        });
                    }
                }
            }
        }
    }
    Bindings {
        recs: recs.into_iter().collect(),
        notes: notes.into_iter().collect(),
    }
}
