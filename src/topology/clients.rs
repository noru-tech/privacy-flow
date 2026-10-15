//! Requests sent through a registered client (`catalogue/topology/clients.yml`): the base
//! address the registration sets, and so the system or processor they reach.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalogue::{Catalogue, SinkClass};
use crate::facts::{Facts, HitKind};
use crate::glob::api_matches_any;
use crate::program::{ClassId, GArgKind, GCallee, GKind, Program, Prov, VarId};
use crate::report::{SinkRec, fides_slug};
use crate::topology::catalogue::{ClientDef, ClientKind, ConfigDef, TopologyCatalogue};
use crate::topology::model::{BindingRec, Endpoint, NoteRec};
use crate::topology::names::{Names, authority_of, config_key, is_local};
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

/// A base address as the code sets it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Base {
    Literal(String),
    /// A configuration key, as written (`Billing:Url`, `services:billing:https:0`).
    Config(String),
}

impl Base {
    fn text(&self) -> String {
        match self {
            Base::Literal(s) => s.clone(),
            Base::Config(k) => format!("config:{k}"),
        }
    }
}

/// The literal string a value is, when it is one.
fn literal(p: &Program, defs: &Defs, var: VarId) -> Option<String> {
    defs.literal_prefix(p, var)
        .and_then(|(s, whole)| whole.then_some(s))
}

/// What a value may be: literal strings through copies and the arguments of the wrapper
/// constructors (`new Uri("…")`, a target-typed `new("…")`), and configuration reads.
fn values(
    p: &Program,
    defs: &Defs,
    var: VarId,
    wrappers: &[String],
    config: &[&ConfigDef],
    depth: u32,
    out: &mut BTreeSet<Base>,
) {
    if depth > 8 {
        return;
    }
    for &d in defs.of(var) {
        match &p.stmts[d as usize].kind {
            GKind::Lit { value: Some(v), .. } => {
                out.insert(Base::Literal(v.clone()));
            }
            GKind::Copy { src, .. } => values(p, defs, *src, wrappers, config, depth + 1, out),
            // `configuration["Billing:Url"]`
            GKind::Load {
                obj,
                field: Some(f),
                ..
            } if api_paths(p, *obj)
                .any(|a| config.iter().any(|c| api_matches_any(&c.objects, &a))) =>
            {
                out.insert(Base::Config(p.syms.str(*f).to_string()));
            }
            GKind::Call { args, is_new, .. } => {
                let paths = call_paths(p, d);
                let first = args
                    .iter()
                    .find(|a| matches!(a.kind, GArgKind::Positional))
                    .and_then(|a| literal(p, defs, a.var));
                if let Some(k) = &first {
                    if config
                        .iter()
                        .any(|c| paths.iter().any(|a| api_matches_any(&c.calls, a)))
                    {
                        out.insert(Base::Config(k.clone()));
                        continue;
                    }
                    if config.iter().any(|c| {
                        paths
                            .iter()
                            .any(|a| api_matches_any(&c.connection_strings, a))
                    }) {
                        out.insert(Base::Config(format!("ConnectionStrings:{k}")));
                        continue;
                    }
                }
                let wrapped = paths.iter().any(|a| {
                    api_matches_any(wrappers, a) || api_matches_any(wrappers, &format!("{a}()"))
                });
                if (wrapped || (*is_new && paths.is_empty()))
                    && let Some(a) = args.first()
                {
                    values(p, defs, a.var, wrappers, config, depth + 1, out);
                }
            }
            _ => {}
        }
    }
}

/// Statements by what they write, built once.
struct Index {
    /// Object → (field, value) stores.
    stores: BTreeMap<VarId, Vec<(crate::program::Sym, VarId)>>,
    /// Field variable → values stored into it through `this`.
    this_stores: BTreeMap<VarId, Vec<VarId>>,
    /// Declared variable → type values (`TypeRef`).
    type_refs: BTreeMap<VarId, Vec<VarId>>,
    /// Receiver → the method calls made on it.
    by_recv: BTreeMap<VarId, Vec<u32>>,
    /// Source → the variables copied from it.
    copies: BTreeMap<VarId, Vec<VarId>>,
}

impl Index {
    fn build(p: &Program) -> Index {
        let mut ix = Index {
            stores: BTreeMap::new(),
            this_stores: BTreeMap::new(),
            type_refs: BTreeMap::new(),
            by_recv: BTreeMap::new(),
            copies: BTreeMap::new(),
        };
        for (i, s) in p.stmts.iter().enumerate() {
            match &s.kind {
                GKind::Call {
                    callee: GCallee::Method { recv, .. },
                    ..
                } => ix.by_recv.entry(*recv).or_default().push(i as u32),
                GKind::Copy { dst, src } => ix.copies.entry(*src).or_default().push(*dst),
                GKind::Store {
                    obj,
                    field: Some(f),
                    src,
                } => ix.stores.entry(*obj).or_default().push((*f, *src)),
                GKind::ThisStore { src, var, .. } => {
                    ix.this_stores.entry(*var).or_default().push(*src)
                }
                GKind::TypeRef { dst, ty } => ix.type_refs.entry(*dst).or_default().push(*ty),
                _ => {}
            }
        }
        ix
    }
}

/// The values stored into `field` of `obj` (or of what it was copied from).
fn stored(p: &Program, defs: &Defs, ix: &Index, obj: VarId, fields: &[String]) -> Vec<VarId> {
    let mut objs = vec![obj, defs.copy_origin(p, obj)];
    objs.dedup();
    objs.iter()
        .flat_map(|o| ix.stores.get(o).into_iter().flatten())
        .filter(|(f, _)| fields.iter().any(|x| x == p.syms.str(*f)))
        .map(|(_, src)| *src)
        .collect()
}

/// The base addresses a registering call sets, in its own arguments or in the calls chained on
/// its result (`AddHttpClient("billing").ConfigureHttpClient(c => …)`).
fn bases(
    p: &Program,
    defs: &Defs,
    ix: &Index,
    def: &ClientDef,
    config: &[&ConfigDef],
    stmt: u32,
) -> BTreeSet<Base> {
    let GKind::Call { args, dst, .. } = &p.stmts[stmt as usize].kind else {
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
                srcs.extend(stored(p, defs, ix, v, &c.fields));
            }
        }
    }
    if !c.lambda_fields.is_empty() {
        // The registration's own lambdas, and those of the calls chained on its result.
        let mut lambda_args: Vec<VarId> = args.iter().map(|a| a.var).collect();
        let mut frontier = vec![*dst];
        for _ in 0..4 {
            let mut next = Vec::new();
            for v in frontier {
                let mut recvs = vec![v];
                recvs.extend(ix.copies.get(&v).into_iter().flatten().copied());
                for r in recvs {
                    for &call in ix.by_recv.get(&r).into_iter().flatten() {
                        let GKind::Call { dst, args, .. } = &p.stmts[call as usize].kind else {
                            continue;
                        };
                        if call_paths(p, call)
                            .iter()
                            .any(|a| api_matches_any(&def.chained, a))
                        {
                            lambda_args.extend(args.iter().map(|a| a.var));
                        }
                        next.push(*dst);
                    }
                }
            }
            frontier = next;
        }
        for a in lambda_args {
            for pr in &p.prov[a as usize] {
                let Prov::Func(f) = pr else { continue };
                // `c => c.BaseAddress = …`, `(sp, c) => c.BaseAddress = …`
                for param in &p.funcs[*f as usize].params {
                    srcs.extend(stored(p, defs, ix, param.var, &c.lambda_fields));
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    for s in srcs {
        values(p, defs, s, &def.wrappers, config, 0, &mut out);
    }
    out
}

/// The calls a receiver comes from, through copies and the field it is held in.
fn origins(p: &Program, defs: &Defs, ix: &Index, var: VarId, depth: u32, out: &mut BTreeSet<u32>) {
    if depth > 8 {
        return;
    }
    for &d in defs.of(var) {
        match &p.stmts[d as usize].kind {
            GKind::Copy { src, .. } => origins(p, defs, ix, *src, depth + 1, out),
            GKind::Call { .. } => {
                out.insert(d);
            }
            GKind::ThisLoad { var: field_var, .. } => {
                origins(p, defs, ix, *field_var, depth + 1, out);
            }
            _ => {}
        }
    }
    // A field variable is defined by the stores into it.
    for src in ix.this_stores.get(&var).into_iter().flatten() {
        origins(p, defs, ix, *src, depth + 1, out);
    }
}

/// The classes a registering call's type argument names.
fn type_arg_classes(p: &Program, ix: &Index, stmt: u32) -> BTreeSet<ClassId> {
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
    for ty in ix.type_refs.get(dst).into_iter().flatten() {
        out.extend(p.prov[*ty as usize].iter().filter_map(|pr| match pr {
            Prov::Instance(c) | Prov::Class(c) => Some(*c),
            _ => None,
        }));
    }
    out
}

pub struct Bindings {
    pub recs: Vec<BindingRec>,
    /// Sink → every endpoint its requests are bound to, in any system, including the system
    /// that sends them (which `recs` leaves out: it is no edge).
    pub resolved: BTreeMap<String, BTreeSet<(Endpoint, String)>>,
    pub notes: Vec<NoteRec>,
}

/// Where a base address goes, from the system that sends through it.
fn resolve(
    inp: &Inputs,
    config: &[&ConfigDef],
    base: &Base,
    system: &str,
    registration: &str,
    notes: &mut BTreeSet<NoteRec>,
) -> Option<Endpoint> {
    match base {
        Base::Literal(url) => {
            let host = authority_of(url)?;
            if let Some(k) = inp.names.by_host.get(&host) {
                return Some(Endpoint::System { key: k.clone() });
            }
            if is_local(&host) || !host.contains('.') {
                notes.insert(NoteRec {
                    kind: "unknown_host".into(),
                    detail: format!(
                        "{url} names no system of the repository; declare it under systems: hosts:"
                    ),
                    paths: vec![registration.to_string()],
                });
                return None;
            }
            Some(Endpoint::Processor {
                name: host.clone(),
                fides_key: fides_slug(&host),
            })
        }
        Base::Config(key) => {
            let k = config_key(key);
            let segs: Vec<&str> = k.split(':').collect();
            let section = |s: &str| {
                config.iter().any(|c| {
                    c.services_section
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(s))
                })
            };
            // Service discovery (`services:billing:https:0`) and connection strings name the
            // resource; any other key is what an orchestrator sets in this system's environment.
            let named = (segs.len() >= 2 && (section(segs[0]) || segs[0] == "connectionstrings"))
                .then(|| inp.names.by_host.get(segs[1]))
                .flatten();
            match named.or_else(|| inp.names.env.get(&(system.to_string(), k.clone()))) {
                Some(t) => Some(Endpoint::System { key: t.clone() }),
                None => {
                    notes.insert(NoteRec {
                        kind: "unresolved_configuration".into(),
                        detail: format!(
                            "the base address is read from configuration ({key}), which no orchestrator sets to a system of the repository"
                        ),
                        paths: vec![registration.to_string()],
                    });
                    None
                }
            }
        }
    }
}

pub fn bind(inp: &Inputs, sinks: &[SinkRec]) -> Bindings {
    let p = inp.program;
    let defs = inp.defs;
    let ix = Index::build(p);
    // Registrations: (client entry, statement, registered classes, client name, base addresses).
    type Reg<'a> = (
        &'a ClientDef,
        u32,
        BTreeSet<ClassId>,
        Option<String>,
        BTreeSet<Base>,
    );
    let mut regs: Vec<Reg> = Vec::new();
    for (i, st) in p.stmts.iter().enumerate() {
        let GKind::Call { args, .. } = &st.kind else {
            continue;
        };
        let lang = p.files[st.file as usize].lang.family();
        let paths = call_paths(p, i as u32);
        let config: Vec<&ConfigDef> = inp
            .topo
            .configuration
            .iter()
            .filter(|c| c.language == lang)
            .collect();
        for c in &inp.topo.clients {
            if c.language != lang || !paths.iter().any(|a| api_matches_any(&c.calls, a)) {
                continue;
            }
            let (classes, name) = match c.client {
                ClientKind::TypeArg => (type_arg_classes(p, &ix, i as u32), None),
                ClientKind::Result => (BTreeSet::new(), None),
                ClientKind::Named => {
                    let positional: Vec<&crate::program::GArg> = args
                        .iter()
                        .filter(|a| matches!(a.kind, GArgKind::Positional))
                        .collect();
                    let Some(name) = c
                        .name_arg
                        .and_then(|k| positional.get(k))
                        .and_then(|a| literal(p, defs, a.var))
                    else {
                        continue;
                    };
                    (BTreeSet::new(), Some(name))
                }
            };
            if c.client == ClientKind::TypeArg && classes.is_empty() {
                continue;
            }
            let found = bases(p, defs, &ix, c, &config, i as u32);
            regs.push((c, i as u32, classes, name, found));
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
    let mut resolved: BTreeMap<String, BTreeSet<(Endpoint, String)>> = BTreeMap::new();
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
            origins(p, defs, &ix, *recv, 0, &mut from);
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
            let config: Vec<&ConfigDef> = inp
                .topo
                .configuration
                .iter()
                .filter(|c| c.language == p.files[st.file as usize].lang.family())
                .collect();
            for (def, reg, classes, name, found) in &regs {
                let applies = match def.client {
                    ClientKind::Result => from.contains(reg),
                    ClientKind::TypeArg => {
                        recv_apis.iter().any(|a| api_matches_any(&def.injects, a))
                            && class.is_some_and(|c| classes.contains(&c))
                    }
                    // `factory.CreateClient("billing")`
                    ClientKind::Named => from.iter().any(|&c| {
                        let GKind::Call { args, .. } = &p.stmts[c as usize].kind else {
                            return false;
                        };
                        call_paths(p, c)
                            .iter()
                            .any(|a| api_matches_any(&def.creates, a))
                            && args
                                .iter()
                                .find(|a| matches!(a.kind, GArgKind::Positional))
                                .and_then(|a| literal(p, defs, a.var))
                                .as_ref()
                                == name.as_ref()
                    }),
                };
                if !applies {
                    continue;
                }
                let reg_st = &p.stmts[*reg as usize];
                let reg_file = reg_st.file as usize;
                let registration = format!("{}:{}", p.files[reg_file].path, reg_st.pos.line);
                for (system, closure) in inp.systems {
                    if !closure[reg_file] || !closure[st.file as usize] {
                        continue;
                    }
                    for base in found {
                        let Some(to) =
                            resolve(inp, &config, base, system, &registration, &mut notes)
                        else {
                            continue;
                        };
                        resolved
                            .entry(snk.id.clone())
                            .or_default()
                            .insert((to.clone(), registration.clone()));
                        if matches!(&to, Endpoint::System { key } if key == system) {
                            continue;
                        }
                        recs.insert(BindingRec {
                            sink: snk.id.clone(),
                            system: system.clone(),
                            client: def.id.clone(),
                            registration: registration.clone(),
                            base: base.text(),
                            to,
                        });
                    }
                }
            }
        }
    }
    Bindings {
        recs: recs.into_iter().collect(),
        resolved,
        notes: notes.into_iter().collect(),
    }
}
