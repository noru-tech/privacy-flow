//! Datasets from ORM models (`catalogue/topology/stores.yml`): the collections and fields a
//! repository's database contexts and models declare, classified by name, and the systems that
//! use them.

use std::collections::{BTreeMap, BTreeSet};

use crate::classify::{Classifier, Place};
use crate::glob::api_matches_any;
use crate::output::yaml::snake;
use crate::program::{ClassId, FuncId, GArg, GArgKind, GCallee, GKind, Program, Prov, VarId};
use crate::topology::catalogue::{StoreDef, StoreKind, TopologyCatalogue};
use crate::topology::model::{CollectionRec, DatasetRec, FieldRec, NoteRec};
use crate::values::{Defs, api_paths};

pub struct Inputs<'a> {
    pub program: &'a Program,
    pub defs: &'a Defs,
    pub classifier: &'a Classifier,
    pub topo: &'a TopologyCatalogue,
    pub resolver: &'a crate::resolve::Resolver,
    /// (system key, closure, own files), per program file.
    pub systems: &'a [(String, &'a [bool], &'a [bool])],
}

pub struct Stores {
    pub datasets: Vec<DatasetRec>,
    /// System key → dataset keys.
    pub links: BTreeMap<String, BTreeSet<String>>,
    pub notes: Vec<NoteRec>,
}

/// The classes a value names.
fn classes_of(p: &Program, v: VarId) -> Vec<ClassId> {
    p.prov[v as usize]
        .iter()
        .filter_map(|pr| match pr {
            Prov::Class(c) | Prov::Instance(c) => Some(*c),
            _ => None,
        })
        .collect()
}

/// API paths of the external types a class and its local ancestors extend or implement.
fn ancestor_paths(p: &Program, c: ClassId) -> Vec<String> {
    let mut out = Vec::new();
    for k in p.lineage(c) {
        let class = &p.classes[k as usize];
        for v in class.bases.iter().chain(&class.implements) {
            out.extend(api_paths(p, *v));
        }
    }
    out
}

fn loc(p: &Program, file: u32, line: u32) -> String {
    format!("{}:{}", p.files[file as usize].path, line)
}

/// A fluent call: (method, arguments, statement).
type FluentCall = (String, Vec<GArg>, u32);

/// A field of an entity before classification: (column, member, location, owned class).
type Row = (String, Option<String>, Option<String>, Option<ClassId>);

/// The literal a value is, whole.
fn literal(p: &Program, defs: &Defs, v: VarId) -> Option<String> {
    defs.literal_prefix(p, v)
        .and_then(|(s, whole)| whole.then_some(s))
}

struct Ctx<'a> {
    p: &'a Program,
    defs: &'a Defs,
    /// Statements of each function.
    by_func: BTreeMap<FuncId, Vec<u32>>,
}

impl Ctx<'_> {
    fn name(&self, s: crate::program::Sym) -> &str {
        self.p.syms.str(s)
    }

    /// `o => o.Address`: the member a lambda argument returns.
    fn lambda_member(&self, arg: VarId) -> Option<String> {
        for pr in &self.p.prov[arg as usize] {
            let Prov::Func(f) = pr else { continue };
            let param = self.p.funcs[*f as usize].params.first()?.var;
            for &s in self.by_func.get(f).into_iter().flatten() {
                let GKind::Return { src } = &self.p.stmts[s as usize].kind else {
                    continue;
                };
                for &d in self.defs.of(*src) {
                    if let GKind::Load {
                        obj,
                        field: Some(k),
                        ..
                    } = &self.p.stmts[d as usize].kind
                        && self.defs.copy_origin(self.p, *obj) == param
                    {
                        return Some(self.name(*k).to_string());
                    }
                }
            }
        }
        None
    }

    /// A string argument, or the member a lambda argument names.
    fn member_arg(&self, args: &[GArg]) -> Option<String> {
        let a = args
            .iter()
            .find(|a| matches!(a.kind, GArgKind::Positional))?;
        literal(self.p, self.defs, a.var).or_else(|| self.lambda_member(a.var))
    }

    /// The chain of method calls a call ends: the receiver it starts from, and each call's
    /// (method, arguments, statement), outermost last.
    fn chain(&self, stmt: u32) -> Option<(VarId, Vec<FluentCall>)> {
        let mut out = Vec::new();
        let mut s = stmt;
        loop {
            let GKind::Call {
                callee: GCallee::Method { recv, name },
                args,
                ..
            } = &self.p.stmts[s as usize].kind
            else {
                return None;
            };
            out.push((self.name(*name).to_string(), args.clone(), s));
            let defs = self.defs.of(*recv);
            match defs {
                [d] if matches!(
                    self.p.stmts[*d as usize].kind,
                    GKind::Call {
                        callee: GCallee::Method { .. },
                        ..
                    }
                ) =>
                {
                    s = *d;
                }
                _ => {
                    out.reverse();
                    return Some((self.defs.copy_origin(self.p, *recv), out));
                }
            }
        }
    }

    /// The class a generic call's type argument names (`Entity<Order>()`).
    fn type_arg_class(&self, stmt: u32) -> Option<ClassId> {
        let st = &self.p.stmts[stmt as usize];
        let GKind::Call { dst, .. } = &st.kind else {
            return None;
        };
        self.by_func
            .get(&st.func)?
            .iter()
            .find_map(|&t| match &self.p.stmts[t as usize].kind {
                GKind::TypeRef { dst: d, ty } if d == dst => {
                    classes_of(self.p, *ty).first().copied()
                }
                _ => None,
            })
    }
}

/// What a fluent configuration says about one entity.
#[derive(Default)]
struct EntityConf {
    table: Option<String>,
    columns: BTreeMap<String, String>,
    mapped: BTreeSet<String>,
    shadow: BTreeSet<String>,
    ignored: BTreeSet<String>,
    owned: BTreeSet<String>,
}

impl EntityConf {
    fn apply(&mut self, ctx: &Ctx, d: &StoreDef, calls: &[FluentCall]) {
        let f = &d.fluent;
        let Some((first, args, _)) = calls.first() else {
            return;
        };
        let rename = calls[1..]
            .iter()
            .find(|(m, _, _)| f.column.contains(m))
            .and_then(|(_, a, _)| a.first())
            .and_then(|a| literal(ctx.p, ctx.defs, a.var));
        if f.table.contains(first) {
            if let Some(t) = args.first().and_then(|a| literal(ctx.p, ctx.defs, a.var)) {
                self.table = Some(t);
            }
        } else if f.property.contains(first) {
            let Some(a) = args.first() else { return };
            if let Some(name) = literal(ctx.p, ctx.defs, a.var) {
                // `Property("_cardNumber")`, `Property<int>("BuyerId")`: a mapped field or a
                // shadow property, by name.
                self.mapped.insert(name.clone());
                self.shadow.insert(name.clone());
                if let Some(r) = rename {
                    self.columns.insert(name, r);
                }
            } else if let Some(m) = ctx.lambda_member(a.var) {
                self.mapped.insert(m.clone());
                if let Some(r) = rename {
                    self.columns.insert(m, r);
                }
            }
        } else if f.ignore.contains(first) {
            if let Some(m) = ctx.member_arg(args) {
                self.ignored.insert(m);
            }
        } else if f.owned.contains(first)
            && let Some(m) = ctx.member_arg(args)
        {
            self.owned.insert(m);
        }
    }
}

pub fn datasets(inp: &Inputs) -> Stores {
    let p = inp.program;
    let mut by_func: BTreeMap<FuncId, Vec<u32>> = BTreeMap::new();
    for (i, s) in p.stmts.iter().enumerate() {
        by_func.entry(s.func).or_default().push(i as u32);
    }
    let ctx = Ctx {
        p,
        defs: inp.defs,
        by_func,
    };
    let mut out = Stores {
        datasets: Vec::new(),
        links: BTreeMap::new(),
        notes: Vec::new(),
    };
    for d in &inp.topo.stores {
        match d.kind {
            StoreKind::EntityFramework => entity_framework(inp, &ctx, d, &mut out),
            StoreKind::Django => django(inp, &ctx, d, &mut out),
        }
    }
    out.datasets.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Classify a column (then its member) in its collection.
fn classify(
    c: &Classifier,
    entity: &str,
    siblings: &[String],
    column: &str,
    member: Option<&str>,
) -> (Vec<String>, bool, Option<String>) {
    let place = Place {
        object: Some(entity),
        siblings,
    };
    let mut hits = c.classify_in(column, &place);
    if hits.is_empty()
        && let Some(m) = member
    {
        hits = c.classify_in(m.trim_start_matches('_'), &place);
    }
    let definite: Vec<&crate::classify::Classified> =
        hits.iter().filter(|h| !h.needs_review).collect();
    if !definite.is_empty() {
        let mut cats: Vec<String> = definite.iter().map(|h| h.category.clone()).collect();
        cats.sort();
        cats.dedup();
        return (cats, false, definite.first().map(|h| h.by.clone()));
    }
    let review = !hits.is_empty();
    (Vec::new(), review, hits.first().map(|h| h.by.clone()))
}

fn field(
    inp: &Inputs,
    entity: &str,
    siblings: &[String],
    column: &str,
    member: Option<&str>,
    location: Option<String>,
) -> FieldRec {
    let (categories, needs_review, by) = classify(inp.classifier, entity, siblings, column, member);
    FieldRec {
        name: column.to_string(),
        member: member.filter(|m| *m != column).map(str::to_string),
        location,
        categories,
        needs_review,
        classified_by: by,
        fields: Vec::new(),
    }
}

// ------------------------------------------------------------------ Entity Framework

fn entity_framework(inp: &Inputs, ctx: &Ctx, d: &StoreDef, out: &mut Stores) {
    let p = inp.program;
    let lang = |c: ClassId| p.files[p.classes[c as usize].file as usize].lang.family();
    let contexts: Vec<ClassId> = (0..p.classes.len() as ClassId)
        .filter(|&c| lang(c) == d.language && !p.classes[c as usize].is_interface)
        .filter(|&c| {
            ancestor_paths(p, c)
                .iter()
                .any(|a| api_matches_any(&d.contexts, a))
        })
        .collect();
    if contexts.is_empty() {
        return;
    }
    // Configuration classes: entity → configuring class.
    let mut configs: Vec<(ClassId, ClassId)> = Vec::new();
    for c in 0..p.classes.len() as ClassId {
        let class = &p.classes[c as usize];
        // An external interface may be lowered as a base: its kind is not known.
        let supers = class
            .implements
            .iter()
            .zip(&class.implements_args)
            .chain(class.bases.iter().zip(&class.base_args));
        for (v, args) in supers {
            if api_paths(p, *v).any(|a| api_matches_any(&d.configurations, &a))
                && let Some(e) = args
                    .first()
                    .and_then(|a| classes_of(p, *a).first().copied())
            {
                configs.push((e, c));
            }
        }
    }
    // Registrations: context → (statement, database literal).
    let mut regs: Vec<(ClassId, u32, Option<String>)> = Vec::new();
    for (i, st) in p.stmts.iter().enumerate() {
        let GKind::Call { args, .. } = &st.kind else {
            continue;
        };
        let paths: Vec<String> = p
            .calls
            .get(&(i as u32))
            .map(|t| t.apis.iter().map(|s| p.syms.str(*s).to_string()).collect())
            .unwrap_or_default();
        if !paths.iter().any(|a| api_matches_any(&d.registrations, a)) {
            continue;
        }
        let Some(c) = ctx.type_arg_class(i as u32) else {
            continue;
        };
        let db = args
            .iter()
            .find(|a| matches!(a.kind, GArgKind::Positional))
            .and_then(|a| literal(p, ctx.defs, a.var))
            .or_else(|| database_in_lambdas(p, ctx, d, args));
        regs.push((c, i as u32, db));
    }

    for &c in &contexts {
        let class = &p.classes[c as usize];
        let mut entities: BTreeMap<ClassId, (String, EntityConf)> = BTreeMap::new();
        // `DbSet<T>` members.
        for k in p.lineage(c) {
            for m in &p.classes[k as usize].members {
                let Some(ty) = &m.ty else { continue };
                let is_set = ty
                    .head
                    .is_some_and(|h| api_paths(p, h).any(|a| api_matches_any(&d.sets, &a)));
                if !is_set {
                    continue;
                }
                if let Some(e) = ty
                    .args
                    .first()
                    .and_then(|a| classes_of(p, *a).first().copied())
                {
                    entities
                        .entry(e)
                        .or_insert_with(|| (m.name.clone(), EntityConf::default()));
                }
            }
        }
        // `OnModelCreating` and the local methods it calls (to depth 3).
        let mut model_funcs: Vec<FuncId> = class
            .methods
            .iter()
            .filter(|(n, _)| d.model_method.as_deref() == Some(n.as_str()))
            .map(|(_, f)| *f)
            .collect();
        let mut i = 0;
        let mut depth = vec![0u32; model_funcs.len()];
        while i < model_funcs.len() {
            let f = model_funcs[i];
            if depth[i] < 3 {
                for &s in ctx.by_func.get(&f).into_iter().flatten() {
                    if let Some(t) = p.calls.get(&s) {
                        for (g, _) in &t.funcs {
                            if !model_funcs.contains(g) {
                                model_funcs.push(*g);
                                depth.push(depth[i] + 1);
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        // Lambdas inside them too.
        let mut all_funcs: BTreeSet<FuncId> = model_funcs.iter().copied().collect();
        let mut grew = true;
        while grew {
            grew = false;
            for (fi, f) in p.funcs.iter().enumerate() {
                if let Some(parent) = f.parent
                    && all_funcs.contains(&parent)
                    && all_funcs.insert(fi as FuncId)
                {
                    grew = true;
                }
            }
        }
        let roots: BTreeSet<VarId> = all_funcs
            .iter()
            .flat_map(|f| p.funcs[*f as usize].params.iter().map(|q| q.var))
            .collect();
        let mut applied: BTreeSet<ClassId> = BTreeSet::new();
        let mut apply_all = false;
        let mut schema = None;
        // Lambda parameters configuring one entity: `Entity<T>(b => …)`.
        let mut lambda_entity: BTreeMap<VarId, ClassId> = BTreeMap::new();
        for f in &all_funcs {
            for &s in ctx.by_func.get(f).into_iter().flatten() {
                let Some((root, calls)) = ctx.chain(s) else {
                    continue;
                };
                if !roots.contains(&root) {
                    continue;
                }
                let (m, args, first) = &calls[0];
                if d.fluent.schema.contains(m) {
                    schema = args.first().and_then(|a| literal(p, ctx.defs, a.var));
                } else if d.fluent.apply.contains(m) {
                    for a in args {
                        applied.extend(classes_of(p, a.var));
                    }
                } else if d.fluent.apply_all.contains(m) {
                    apply_all = true;
                } else if d.fluent.entity.contains(m)
                    && let Some(e) = ctx.type_arg_class(*first)
                {
                    let entry = entities.entry(e).or_insert_with(|| {
                        (p.classes[e as usize].name.clone(), EntityConf::default())
                    });
                    entry.1.apply(ctx, d, &calls[1..]);
                    for a in args {
                        for pr in &p.prov[a.var as usize] {
                            if let Prov::Func(lf) = pr
                                && let Some(q) = p.funcs[*lf as usize].params.first()
                            {
                                lambda_entity.insert(q.var, e);
                            }
                        }
                    }
                }
            }
        }
        // Chains on an `Entity<T>(b => …)` lambda's parameter.
        for f in &all_funcs {
            for &s in ctx.by_func.get(f).into_iter().flatten() {
                if let Some((root, calls)) = ctx.chain(s)
                    && let Some(&e) = lambda_entity.get(&root)
                    && let Some(entry) = entities.get_mut(&e)
                {
                    entry.1.apply(ctx, d, &calls);
                }
            }
        }
        // Configuration classes the context applies.
        let project_of = |c: ClassId| {
            inp.resolver
                .csproj_for(&p.files[p.classes[c as usize].file as usize].path)
                .map(|x| x.path.clone())
        };
        let ctx_project = project_of(c);
        for &(e, cfg) in &configs {
            let same_project = ctx_project.is_some() && project_of(cfg) == ctx_project;
            if !(applied.contains(&cfg) || (apply_all && same_project)) {
                continue;
            }
            let entry = entities
                .entry(e)
                .or_insert_with(|| (p.classes[e as usize].name.clone(), EntityConf::default()));
            for (name, f) in &p.classes[cfg as usize].methods {
                if name != "Configure" {
                    continue;
                }
                let Some(builder) = p.funcs[*f as usize].params.first().map(|q| q.var) else {
                    continue;
                };
                let mut funcs: BTreeSet<FuncId> = BTreeSet::from([*f]);
                for (fi, g) in p.funcs.iter().enumerate() {
                    if g.parent == Some(*f) {
                        funcs.insert(fi as FuncId);
                    }
                }
                for g in &funcs {
                    for &s in ctx.by_func.get(g).into_iter().flatten() {
                        if let Some((root, calls)) = ctx.chain(s)
                            && root == builder
                        {
                            entry.1.apply(ctx, d, &calls);
                        }
                    }
                }
            }
        }

        let entity_set: BTreeSet<ClassId> = entities.keys().copied().collect();
        let mut collections: Vec<CollectionRec> = Vec::new();
        for (e, (set_name, conf)) in &entities {
            let ec = &p.classes[*e as usize];
            let table = conf
                .table
                .clone()
                .or_else(|| class_attr_literal(p, ctx.defs, *e, &d.table_attributes))
                .unwrap_or_else(|| set_name.clone());
            let fields = entity_fields(inp, ctx, d, *e, conf, &entity_set, 0);
            collections.push(CollectionRec {
                name: table,
                entity: ec.qualified_name.clone().unwrap_or_else(|| ec.name.clone()),
                location: loc(p, ec.file, ec.pos.line),
                fields,
            });
        }
        // Tables a base context brings.
        for bc in &d.base_collections {
            for k in p.lineage(c) {
                let kc = &p.classes[k as usize];
                for (bi, b) in kc.bases.iter().enumerate() {
                    let Some(base_path) = api_paths(p, *b)
                        .find(|a| api_matches_any(std::slice::from_ref(&bc.context), a))
                    else {
                        continue;
                    };
                    for col in &bc.collections {
                        let entity = col
                            .entity_type_arg
                            .and_then(|i| kc.base_args.get(bi).and_then(|a| a.get(i)))
                            .and_then(|v| classes_of(p, *v).first().copied());
                        let mut names: Vec<String> = col.fields.clone();
                        let mut fields: Vec<FieldRec> = Vec::new();
                        let own = entity.map(|e| {
                            entity_fields(inp, ctx, d, e, &EntityConf::default(), &entity_set, 0)
                        });
                        if let Some(own) = &own {
                            names.extend(own.iter().map(|f| f.name.clone()));
                        }
                        for n in &col.fields {
                            fields.push(field(inp, &col.name, &names, n, None, None));
                        }
                        for f in own.into_iter().flatten() {
                            if !fields.iter().any(|x| x.name == f.name) {
                                fields.push(f);
                            }
                        }
                        fields.sort_by(|a, b| a.name.cmp(&b.name));
                        if !collections.iter().any(|x| x.name == col.name) {
                            collections.push(CollectionRec {
                                name: col.name.clone(),
                                entity: entity
                                    .map(|e| {
                                        let ec = &p.classes[e as usize];
                                        ec.qualified_name.clone().unwrap_or_else(|| ec.name.clone())
                                    })
                                    .unwrap_or_else(|| base_path.clone()),
                                location: loc(p, kc.file, kc.pos.line),
                                fields,
                            });
                        }
                    }
                }
            }
        }
        collections.sort_by(|a, b| a.name.cmp(&b.name));
        collections.dedup_by(|a, b| a.name == b.name);
        let key = snake(&class.name);
        let ctx_file = class.file as usize;
        let mut database = None;
        let mut linked = false;
        for (rc, stmt, db) in &regs {
            if *rc != c {
                continue;
            }
            database = database.or_else(|| db.clone());
            let file = p.stmts[*stmt as usize].file as usize;
            for (sys, closure, _) in inp.systems {
                if closure[file] {
                    out.links
                        .entry(sys.clone())
                        .or_default()
                        .insert(key.clone());
                    linked = true;
                }
            }
        }
        if !linked {
            // No registration: the systems whose own code holds or reaches the context.
            for (sys, closure, own) in inp.systems {
                if own[ctx_file] || (closure[ctx_file] && own.iter().any(|x| *x)) {
                    out.links
                        .entry(sys.clone())
                        .or_default()
                        .insert(key.clone());
                }
            }
        }
        out.datasets.push(DatasetRec {
            key,
            name: class.name.clone(),
            store: d.id.clone(),
            location: loc(p, class.file, class.pos.line),
            database,
            schema,
            collections,
        });
    }
}

/// `options => options.UseNpgsql(config.GetConnectionString("orderingdb"))`: the literal of a
/// `database_calls` call inside a lambda argument.
fn database_in_lambdas(p: &Program, ctx: &Ctx, d: &StoreDef, args: &[GArg]) -> Option<String> {
    for a in args {
        for pr in &p.prov[a.var as usize] {
            let Prov::Func(f) = pr else { continue };
            for &s in ctx.by_func.get(f).into_iter().flatten() {
                let GKind::Call { args: inner, .. } = &p.stmts[s as usize].kind else {
                    continue;
                };
                let hit = p.calls.get(&s).is_some_and(|t| {
                    t.apis
                        .iter()
                        .any(|x| api_matches_any(&d.database_calls, p.syms.str(*x)))
                });
                if hit && let Some(v) = inner.first().and_then(|x| literal(p, ctx.defs, x.var)) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The first string argument of a class attribute matching `attrs` (`[Table("orders")]`).
fn class_attr_literal(p: &Program, defs: &Defs, c: ClassId, attrs: &[String]) -> Option<String> {
    p.classes[c as usize].decorators.iter().find_map(|a| {
        api_paths(p, *a)
            .any(|x| api_matches_any(attrs, &x))
            .then(|| attr_literal(p, defs, *a))
            .flatten()
    })
}

/// An attribute's first string argument: attributes are constructions of the attribute type.
fn attr_literal(p: &Program, defs: &Defs, a: VarId) -> Option<String> {
    let (args, _) = defs.defining_call(p, a)?;
    let first = args.first()?;
    literal(p, defs, first.var)
}

fn member_attrs(p: &Program, c: ClassId, name: &str) -> Vec<VarId> {
    p.classes[c as usize]
        .field_attrs
        .iter()
        .filter(|(n, _)| n == name)
        .flat_map(|(_, a)| a.iter().copied())
        .collect()
}

/// The fields of an entity: its mapped members and those of its local ancestors, owned types
/// nested, navigations left out.
fn entity_fields(
    inp: &Inputs,
    ctx: &Ctx,
    d: &StoreDef,
    e: ClassId,
    conf: &EntityConf,
    entities: &BTreeSet<ClassId>,
    depth: u32,
) -> Vec<FieldRec> {
    let p = inp.program;
    let entity_name = p.classes[e as usize].name.clone();
    let mut rows: Vec<Row> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for k in p.lineage(e) {
        let kc = &p.classes[k as usize];
        for m in &kc.members {
            if m.is_static
                || m.kind == crate::ir::MemberKind::Computed
                || conf.ignored.contains(&m.name)
                || !seen.insert(m.name.clone())
            {
                continue;
            }
            // Private fields are mapped only when the configuration names them.
            if m.kind == crate::ir::MemberKind::Field && !conf.mapped.contains(&m.name) {
                continue;
            }
            let attrs = member_attrs(p, k, &m.name);
            let attr_paths: Vec<String> = attrs.iter().flat_map(|a| api_paths(p, *a)).collect();
            if attr_paths.iter().any(|a| api_matches_any(&d.not_mapped, a)) {
                continue;
            }
            let ty_classes: Vec<ClassId> =
                m.ty.as_ref()
                    .map(|t| {
                        t.head
                            .into_iter()
                            .chain(t.args.iter().copied())
                            .flat_map(|v| classes_of(p, v))
                            .collect()
                    })
                    .unwrap_or_default();
            // A navigation to another entity is a relation, not a field.
            if ty_classes.iter().any(|c| entities.contains(c)) {
                continue;
            }
            let owned_class = ty_classes.iter().copied().find(|c| {
                conf.owned.contains(&m.name)
                    || p.classes[*c as usize]
                        .decorators
                        .iter()
                        .any(|a| api_paths(p, *a).any(|x| api_matches_any(&d.owned_attributes, &x)))
            });
            if owned_class.is_none() && conf.owned.contains(&m.name) {
                continue;
            }
            let column = conf
                .columns
                .get(&m.name)
                .cloned()
                .or_else(|| {
                    attrs.iter().find_map(|a| {
                        api_paths(p, *a)
                            .any(|x| api_matches_any(&d.column_attributes, &x))
                            .then(|| attr_literal(p, ctx.defs, *a))
                            .flatten()
                    })
                })
                .unwrap_or_else(|| m.name.clone());
            rows.push((
                column,
                Some(m.name.clone()),
                Some(loc(p, kc.file, m.pos.line)),
                owned_class,
            ));
        }
    }
    for s in &conf.shadow {
        if !seen.contains(s) {
            let column = conf.columns.get(s).cloned().unwrap_or_else(|| s.clone());
            rows.push((column, None, None, None));
        }
    }
    let siblings: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let mut out: Vec<FieldRec> = rows
        .into_iter()
        .map(|(column, member, location, owned)| {
            let mut f = field(
                inp,
                &entity_name,
                &siblings,
                &column,
                member.as_deref(),
                location,
            );
            if let Some(o) = owned.filter(|_| depth < 2) {
                f.fields =
                    entity_fields(inp, ctx, d, o, &EntityConf::default(), entities, depth + 1);
                f.categories.clear();
                f.needs_review = false;
                f.classified_by = None;
            }
            f
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

// ------------------------------------------------------------------ Django

fn django(inp: &Inputs, ctx: &Ctx, d: &StoreDef, out: &mut Stores) {
    let p = inp.program;
    let is_model = |c: ClassId| {
        p.files[p.classes[c as usize].file as usize].lang.family() == d.language
            && ancestor_paths(p, c)
                .iter()
                .any(|a| api_matches_any(&d.models, a))
    };
    // The `Meta` class nested in a model: its literal attributes.
    let meta = |c: ClassId, attr: &str| -> Option<String> {
        let name = d.meta_class.as_deref()?;
        let mc = p
            .classes
            .iter()
            .find(|mc| mc.outer == Some(c) && mc.name == name)?;
        let m = mc.members.iter().find(|m| m.name == attr)?;
        let v = m.value?;
        // `abstract = True`: a constant is a literal with no string value; its text says it.
        literal(p, ctx.defs, v).or_else(|| match ctx.defs.of(v) {
            [d] => Some(p.stmts[*d as usize].text.clone()),
            _ => None,
        })
    };
    let mut by_system: BTreeMap<String, Vec<CollectionRec>> = BTreeMap::new();
    let mut first_loc: BTreeMap<String, String> = BTreeMap::new();
    for c in 0..p.classes.len() as ClassId {
        if !is_model(c) {
            continue;
        }
        if meta(c, "abstract").is_some_and(|v| v == "True" || v == "true") {
            continue;
        }
        let class = &p.classes[c as usize];
        let file = class.file as usize;
        let app = dir_of(&p.files[file].path)
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        let table = meta(c, "db_table").unwrap_or_else(|| {
            if app.is_empty() {
                class.name.to_lowercase()
            } else {
                format!("{app}_{}", class.name.to_lowercase())
            }
        });
        let mut rows: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for k in p.lineage(c) {
            let kc = &p.classes[k as usize];
            for m in &kc.members {
                let Some(v) = m.value else { continue };
                if !seen.insert(m.name.clone()) {
                    continue;
                }
                let paths: Vec<String> = api_paths(p, v).collect();
                let column_kw = d.column_keyword.as_deref().and_then(|kw| {
                    ctx.defs.defining_call(p, v).and_then(|(args, _)| {
                        args.iter()
                            .find(|a| matches!(&a.kind, GArgKind::Keyword(n) if n == kw))
                            .and_then(|a| literal(p, ctx.defs, a.var))
                    })
                });
                let at = Some(loc(p, kc.file, m.pos.line));
                if paths.iter().any(|a| api_matches_any(&d.relations, a)) {
                    let col = column_kw.unwrap_or_else(|| format!("{}_id", m.name));
                    rows.push((col, Some(m.name.clone()), at));
                } else if paths.iter().any(|a| api_matches_any(&d.columns, a))
                    && !paths.iter().any(|a| api_matches_any(&d.not_columns, a))
                {
                    let col = column_kw.unwrap_or_else(|| m.name.clone());
                    rows.push((col, Some(m.name.clone()), at));
                }
            }
        }
        let ancestors = ancestor_paths(p, c);
        for base in &d.inherited {
            if ancestors
                .iter()
                .any(|a| api_matches_any(std::slice::from_ref(&base.base), a))
            {
                for f in &base.fields {
                    if seen.insert(f.clone()) {
                        rows.push((f.clone(), None, None));
                    }
                }
            }
        }
        let siblings: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
        let mut fields: Vec<FieldRec> = rows
            .into_iter()
            .map(|(col, member, at)| {
                field(inp, &class.name, &siblings, &col, member.as_deref(), at)
            })
            .collect();
        fields.sort_by(|a, b| a.name.cmp(&b.name));
        let owner = inp
            .systems
            .iter()
            .find(|(_, _, own)| own[file])
            .or_else(|| inp.systems.iter().find(|(_, closure, _)| closure[file]))
            .map(|(k, _, _)| k.clone())
            .unwrap_or_else(|| "models".to_string());
        first_loc
            .entry(owner.clone())
            .or_insert_with(|| loc(p, class.file, class.pos.line));
        by_system.entry(owner).or_default().push(CollectionRec {
            name: table,
            entity: class.name.clone(),
            location: loc(p, class.file, class.pos.line),
            fields,
        });
    }
    for (system, mut collections) in by_system {
        collections.sort_by(|a, b| a.name.cmp(&b.name));
        let key = format!("{system}_db");
        if inp.systems.iter().any(|(k, _, _)| *k == system) {
            out.links
                .entry(system.clone())
                .or_default()
                .insert(key.clone());
        }
        out.datasets.push(DatasetRec {
            key,
            name: format!("{system} database"),
            store: d.id.clone(),
            location: first_loc.remove(&system).unwrap_or_default(),
            database: None,
            schema: None,
            collections,
        });
    }
}
