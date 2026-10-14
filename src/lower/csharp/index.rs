//! The project-wide declaration index, and C# name resolution against it.
//!
//! A simple name resolves the way the C# compiler looks it up, minus what only assembly
//! metadata could tell: nested types of the enclosing types, then each enclosing namespace from
//! the innermost out (its types, its sub-namespaces, then the aliases and `using` namespaces
//! declared there), and at the outermost level the file's own usings, the project's `global
//! using`s and `<Using>` items, and the SDK's implicit usings.
//!
//! The project's own types are known completely. An external type is known only by name: a
//! `using` namespace qualifies it when the catalogue spells a type there, or names the
//! namespace with a wildcard after it (`Microsoft.Extensions.Logging.**.LogInformation`). Where
//! several `using`s could qualify it, every candidate is kept; the compiler would reject a real
//! ambiguity, so at most one is right, and the catalogue matches that one.

use std::collections::{BTreeMap, BTreeSet};

use super::scan::{FileScan, Member, MemberKind, TypeKind, TypeText, Using};
use crate::resolve::Resolver;

pub type TypeId = usize;

#[derive(Clone, Debug)]
pub enum BaseRef {
    Project(TypeId),
    /// An external type, with its candidate qualified names.
    External(Vec<String>),
}

#[derive(Clone, Debug)]
pub struct TypeDef {
    /// `Shop.Api.OrdersApi`, `Shop.Api.OrdersApi.Inner` for a nested type.
    pub fqn: String,
    pub name: String,
    pub arity: usize,
    pub kind: TypeKind,
    pub project: String,
    /// (file, index into the file's scanned types), in path order: the first is canonical.
    pub parts: Vec<(String, usize)>,
    pub partial: bool,
    pub is_static: bool,
    pub type_params: Vec<String>,
    pub outer: Option<TypeId>,
    pub bases: Vec<BaseRef>,
    /// Own members (of every part) and those of its source ancestors, nearest first.
    pub all_members: BTreeMap<String, Member>,
    /// Members the source does not show may exist: it extends a type outside the project (or a
    /// part is generated).
    pub open: bool,
}

impl TypeDef {
    pub fn is_interface(&self) -> bool {
        self.kind == TypeKind::Interface
    }

    /// The file whose declaration stands for the type.
    pub fn canonical_file(&self) -> &str {
        &self.parts[0].0
    }

    /// The key every part of the type shares (merged into one class by the program).
    pub fn key(&self) -> String {
        if self.arity == 0 {
            format!("{}|{}", self.project, self.fqn)
        } else {
            format!("{}|{}`{}", self.project, self.fqn, self.arity)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Res {
    Project(TypeId),
    Namespace(String),
    /// Candidate qualified names of an external type, or of a member path into one.
    External(Vec<String>),
    Unknown,
}

#[derive(Clone, Debug, Default)]
struct ProjectUsings {
    namespaces: Vec<String>,
    statics: Vec<String>,
    aliases: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct FileEntry {
    project: String,
    scan: FileScan,
    /// Declaration start byte → scope index / type id.
    scope_at: BTreeMap<usize, usize>,
    type_at: BTreeMap<usize, TypeId>,
}

#[derive(Clone, Debug, Default)]
pub struct CsIndex {
    files: BTreeMap<String, FileEntry>,
    pub defs: Vec<TypeDef>,
    /// fqn (without arity) → types, in (project, path) order.
    by_fqn: BTreeMap<String, Vec<TypeId>>,
    /// Project namespaces and every prefix of one.
    namespaces: BTreeSet<String>,
    /// Literal prefixes of the catalogue's C# patterns: names it spells.
    known: BTreeSet<String>,
    /// Namespaces the catalogue names with a wildcard after them: every type under them is one
    /// it may know.
    open: BTreeSet<String>,
    projects: BTreeMap<String, ProjectUsings>,
    /// The context each Razor component was generated with.
    razor: BTreeMap<String, crate::lower::razor::RazorCtx>,
}

/// Expand `{a,b}` alternations (nested ones too).
fn expand(p: &str) -> Vec<String> {
    let Some(open) = p.find('{') else {
        return vec![p.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    for (i, c) in p[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![p.to_string()];
    };
    let inner = &p[open + 1..close];
    let mut alts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                alts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&inner[start..]);
    let mut out = Vec::new();
    for a in alts {
        for rest in expand(&format!("{}{}{}", &p[..open], a, &p[close + 1..])) {
            out.push(rest);
            if out.len() > 4096 {
                return out;
            }
        }
    }
    out
}

/// What the catalogue spells: (known names, open namespaces) from its C# API patterns.
pub fn known_from_patterns<'a>(
    patterns: impl IntoIterator<Item = &'a str>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut known = BTreeSet::new();
    let mut open = BTreeSet::new();
    for p in patterns {
        for e in expand(p) {
            let cut = e.find(['*', '<']).unwrap_or(e.len());
            let lit = e[..cut].replace("()", "");
            if lit.is_empty() {
                continue;
            }
            match lit.strip_suffix('.') {
                Some(ns) if cut < e.len() => {
                    if !ns.is_empty() {
                        open.insert(ns.to_string());
                    }
                }
                _ => {
                    // `System.String**`: a name followed directly by a wildcard is still a name.
                    let name = lit.trim_end_matches('.');
                    if !name.is_empty() {
                        known.insert(name.to_string());
                    }
                }
            }
        }
    }
    (known, open)
}

impl CsIndex {
    pub fn build(
        scans: Vec<FileScan>,
        resolver: &Resolver,
        known: BTreeSet<String>,
        open: BTreeSet<String>,
    ) -> CsIndex {
        let mut ix = CsIndex {
            known,
            open,
            ..CsIndex::default()
        };
        let mut scans = scans;
        scans.sort_by(|a, b| a.path.cmp(&b.path));
        for s in scans {
            let project = resolver
                .csproj_for(&s.path)
                .map(|p| p.path.clone())
                .unwrap_or_default();
            let scope_at = s
                .scopes
                .iter()
                .enumerate()
                .map(|(i, sc)| (sc.start, i))
                .collect();
            let entry = ix.projects.entry(project.clone()).or_default();
            for u in &s.global_usings {
                match u {
                    Using::Namespace(n) => push_unique(&mut entry.namespaces, n),
                    Using::Static(n) => push_unique(&mut entry.statics, n),
                    Using::Alias(a, t) => entry.aliases.push((a.clone(), t.clone())),
                }
            }
            ix.files.insert(
                s.path.clone(),
                FileEntry {
                    project,
                    scan: s,
                    scope_at,
                    type_at: BTreeMap::new(),
                },
            );
        }
        // Project-file usings, after the global usings of the code.
        for (key, entry) in ix.projects.iter_mut() {
            if let Some(p) = resolver.csprojs.values().find(|p| &p.path == key) {
                for n in p.namespaces() {
                    push_unique(&mut entry.namespaces, &n);
                }
                for u in &p.usings {
                    if u.remove {
                        continue;
                    }
                    if let Some(a) = &u.alias {
                        entry.aliases.push((a.clone(), u.name.clone()));
                    } else if u.is_static {
                        push_unique(&mut entry.statics, &u.name);
                    }
                }
            }
        }
        // Types, merging the parts of partial types.
        let paths: Vec<String> = ix.files.keys().cloned().collect();
        let mut by_key: BTreeMap<String, TypeId> = BTreeMap::new();
        for path in &paths {
            let n = ix.files[path].scan.types.len();
            let mut ids: Vec<TypeId> = Vec::with_capacity(n);
            for i in 0..n {
                let (fqn, t, project) = {
                    let f = &ix.files[path];
                    let t = f.scan.types[i].clone();
                    let fqn = match t.outer {
                        Some(o) => format!("{}.{}", ix.defs[ids[o]].fqn, t.name),
                        None => {
                            let ns = &f.scan.scopes[t.scope].name;
                            if ns.is_empty() {
                                t.name.clone()
                            } else {
                                format!("{ns}.{}", t.name)
                            }
                        }
                    };
                    (fqn, t, f.project.clone())
                };
                let key = format!("{project}|{fqn}`{}", t.arity);
                let existing = if t.partial {
                    by_key.get(&key).copied()
                } else {
                    None
                };
                let id = match existing {
                    Some(id) => {
                        ix.defs[id].parts.push((path.clone(), i));
                        id
                    }
                    None => {
                        let id = ix.defs.len();
                        ix.defs.push(TypeDef {
                            fqn: fqn.clone(),
                            name: t.name.clone(),
                            arity: t.arity,
                            kind: t.kind,
                            project,
                            parts: vec![(path.clone(), i)],
                            partial: t.partial,
                            is_static: t.is_static,
                            type_params: t.type_params.clone(),
                            outer: t.outer.map(|o| ids[o]),
                            bases: Vec::new(),
                            all_members: BTreeMap::new(),
                            open: t.partial,
                        });
                        if t.partial {
                            by_key.insert(key, id);
                        }
                        ix.by_fqn.entry(fqn.clone()).or_default().push(id);
                        id
                    }
                };
                ids.push(id);
                let start = t.start;
                ix.files
                    .get_mut(path)
                    .expect("file")
                    .type_at
                    .insert(start, id);
            }
            let f = &ix.files[path];
            for sc in &f.scan.scopes {
                let mut ns = sc.name.as_str();
                while !ns.is_empty() {
                    ix.namespaces.insert(ns.to_string());
                    ns = ns.rsplit_once('.').map_or("", |(a, _)| a);
                }
            }
        }
        // Bases, resolved where each part declares them.
        for id in 0..ix.defs.len() {
            let mut bases = Vec::new();
            for (path, i) in ix.defs[id].parts.clone() {
                let t = ix.files[&path].scan.types[i].clone();
                let enclosing: Vec<TypeId> = ix.defs[id].outer.into_iter().collect();
                for b in &t.bases {
                    let r = ix.resolve_type(&path, t.scope, &enclosing, b);
                    match r {
                        Res::Project(p) if p != id => bases.push(BaseRef::Project(p)),
                        Res::External(c) => bases.push(BaseRef::External(c)),
                        Res::Unknown | Res::Namespace(_) => {
                            bases.push(BaseRef::External(vec![b.segs.join(".")]))
                        }
                        _ => {}
                    }
                }
            }
            ix.defs[id].bases = bases;
        }
        // Members along the source lineage; open when a base is outside the project.
        for id in 0..ix.defs.len() {
            let mut all: BTreeMap<String, Member> = BTreeMap::new();
            let mut open = ix.defs[id].open;
            let mut seen = BTreeSet::new();
            let mut queue = vec![id];
            while let Some(t) = queue.first().copied() {
                queue.remove(0);
                if !seen.insert(t) || seen.len() > 32 {
                    continue;
                }
                for (path, i) in &ix.defs[t].parts {
                    for m in &ix.files[path].scan.types[*i].members {
                        all.entry(m.name.clone()).or_insert_with(|| m.clone());
                    }
                }
                for b in &ix.defs[t].bases {
                    match b {
                        BaseRef::Project(p) => queue.push(*p),
                        BaseRef::External(c) => {
                            if !c.iter().all(|n| looks_like_interface(n)) {
                                open = true;
                            }
                        }
                    }
                }
            }
            ix.defs[id].all_members = all;
            ix.defs[id].open = open;
        }
        ix
    }

    /// Record the context a Razor component's C# was generated with, for its lowering.
    pub fn set_razor_ctx(&mut self, path: &str, ctx: crate::lower::razor::RazorCtx) {
        self.razor.insert(path.to_string(), ctx);
    }

    pub fn razor_ctx(&self, path: &str) -> crate::lower::razor::RazorCtx {
        self.razor.get(path).cloned().unwrap_or_default()
    }

    pub fn file_project(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(|f| f.project.as_str())
    }

    pub fn scope_at(&self, path: &str, start: usize) -> Option<usize> {
        self.files.get(path)?.scope_at.get(&start).copied()
    }

    pub fn type_at(&self, path: &str, start: usize) -> Option<TypeId> {
        self.files.get(path)?.type_at.get(&start).copied()
    }

    pub fn def(&self, id: TypeId) -> &TypeDef {
        &self.defs[id]
    }

    pub fn member(&self, id: TypeId, name: &str) -> Option<&Member> {
        self.defs[id].all_members.get(name)
    }

    /// The source ancestors of a type, nearest first (itself included).
    pub fn lineage(&self, id: TypeId) -> Vec<TypeId> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() && out.len() < 32 {
            for b in &self.defs[out[i]].bases {
                if let BaseRef::Project(p) = b
                    && !out.contains(p)
                {
                    out.push(*p);
                }
            }
            i += 1;
        }
        out
    }

    /// How well the catalogue knows the external name `fqn`: `None` when it does not; 3 when
    /// it spells the name (or a member of it); 2 when the name's namespace is one it covers
    /// with a wildcard; 1 when an enclosing namespace is.
    fn plausible(&self, fqn: &str) -> Option<u8> {
        if self.known.contains(fqn) {
            return Some(3);
        }
        let dotted = format!("{fqn}.");
        if self
            .known
            .range(dotted.clone()..)
            .next()
            .is_some_and(|k| k.starts_with(&dotted))
        {
            return Some(3);
        }
        let mut ns = fqn;
        let mut first = true;
        while let Some((a, _)) = ns.rsplit_once('.') {
            if self.open.contains(a) {
                return Some(if first { 2 } else { 1 });
            }
            first = false;
            ns = a;
        }
        None
    }

    /// Whether the catalogue spells the external name `fqn` (or a member of it).
    pub fn is_spelled(&self, fqn: &str) -> bool {
        self.plausible(fqn) == Some(3)
    }

    /// Whether a namespace is the project's, the base class library's (`System.*`), or one the
    /// catalogue names or covers.
    fn known_namespace(&self, ns: &str) -> bool {
        if self.namespaces.contains(ns)
            || self.external_namespace(ns)
            || ns == "System"
            || ns.starts_with("System.")
        {
            return true;
        }
        let mut n = ns;
        loop {
            if self.open.contains(n) {
                return true;
            }
            match n.rsplit_once('.') {
                Some((a, _)) => n = a,
                None => return false,
            }
        }
    }

    /// Whether `name` is an external namespace root the catalogue names (`System`).
    fn external_namespace(&self, ns: &str) -> bool {
        let dotted = format!("{ns}.");
        self.open.contains(ns)
            || self
                .known
                .range(dotted.clone()..)
                .next()
                .is_some_and(|k| k.starts_with(&dotted))
            || self
                .open
                .range(dotted.clone()..)
                .next()
                .is_some_and(|k| k.starts_with(&dotted))
    }

    /// A project type `fqn` with this arity, preferring the given project.
    fn project_type(&self, fqn: &str, arity: Option<usize>, project: &str) -> Option<TypeId> {
        let ids = self.by_fqn.get(fqn)?;
        let fits = |id: &&TypeId| arity.is_none_or(|a| self.defs[**id].arity == a);
        ids.iter()
            .filter(fits)
            .find(|id| self.defs[**id].project == project)
            .or_else(|| ids.iter().find(fits))
            .or_else(|| ids.iter().find(|id| self.defs[**id].project == project))
            .or_else(|| ids.first())
            .copied()
    }

    /// Nested types of `t` and of its source ancestors.
    fn nested(&self, t: TypeId, name: &str, arity: Option<usize>, project: &str) -> Option<TypeId> {
        for k in self.lineage(t) {
            let fqn = format!("{}.{}", self.defs[k].fqn, name);
            if let Some(id) = self.project_type(&fqn, arity, project) {
                return Some(id);
            }
        }
        None
    }

    /// The usings that apply at the compilation-unit level of a file: its own, then the
    /// project's.
    fn unit_usings(&self, path: &str) -> (Vec<String>, Vec<(String, String)>, Vec<String>) {
        let mut namespaces = Vec::new();
        let mut aliases = Vec::new();
        let mut statics = Vec::new();
        let Some(f) = self.files.get(path) else {
            return (namespaces, aliases, statics);
        };
        for u in &f.scan.scopes[0].usings {
            match u {
                Using::Namespace(n) => push_unique(&mut namespaces, n),
                Using::Alias(a, t) => aliases.push((a.clone(), t.clone())),
                Using::Static(n) => push_unique(&mut statics, n),
            }
        }
        if let Some(p) = self.projects.get(&f.project) {
            for n in &p.namespaces {
                push_unique(&mut namespaces, n);
            }
            aliases.extend(p.aliases.iter().cloned());
            for n in &p.statics {
                push_unique(&mut statics, n);
            }
        }
        (namespaces, aliases, statics)
    }

    /// The `using static` types in scope at a namespace scope of a file.
    pub fn static_usings(&self, path: &str, scope: usize) -> Vec<String> {
        let mut out = Vec::new();
        let Some(f) = self.files.get(path) else {
            return out;
        };
        let mut s = Some(scope);
        while let Some(i) = s {
            if i == 0 {
                break;
            }
            for u in &f.scan.scopes[i].usings {
                if let Using::Static(n) = u {
                    push_unique(&mut out, n);
                }
            }
            s = f.scan.scopes[i].parent;
        }
        out.extend(self.unit_usings(path).2);
        out
    }

    /// Resolve the first segment of a name.
    pub fn resolve_simple(
        &self,
        path: &str,
        scope: usize,
        enclosing: &[TypeId],
        name: &str,
        arity: Option<usize>,
    ) -> Res {
        let Some(f) = self.files.get(path) else {
            return Res::Unknown;
        };
        let project = f.project.as_str();
        for &t in enclosing.iter().rev() {
            if let Some(id) = self.nested(t, name, arity, project) {
                return Res::Project(id);
            }
            if self.defs[t].name == name && arity.is_none_or(|a| a == self.defs[t].arity) {
                return Res::Project(t);
            }
        }
        // Namespace levels, innermost first; each declaration's own usings apply at its level.
        // A name only a catalogue wildcard covers is a guess: an outer level may still know it.
        let mut guess: Option<Res> = None;
        let mut s = Some(scope).filter(|&i| i > 0);
        while let Some(i) = s {
            let sc = &f.scan.scopes[i];
            let parent_name = sc.parent.map_or("", |p| f.scan.scopes[p].name.as_str());
            // `namespace A.B.C` nests three levels; the ones its parent does not cover are its.
            let mut level = sc.name.as_str();
            let mut first = true;
            loop {
                let fqn = format!("{level}.{name}");
                if let Some(id) = self.project_type(&fqn, arity, project) {
                    return Res::Project(id);
                }
                if self.namespaces.contains(&fqn) {
                    return Res::Namespace(fqn);
                }
                if first {
                    let usings: Vec<&Using> = sc.usings.iter().collect();
                    match self.via_usings(&usings, name, arity, project) {
                        Some((r, true)) => return r,
                        Some((r, false)) => {
                            guess.get_or_insert(r);
                        }
                        None => {}
                    }
                }
                first = false;
                match level.rsplit_once('.') {
                    Some((a, _)) if a.len() > parent_name.len() => level = a,
                    _ => break,
                }
            }
            s = sc.parent.filter(|&p| p > 0);
        }
        // The compilation unit: global types and namespaces, then every using in scope.
        if let Some(id) = self.project_type(name, arity, project) {
            return Res::Project(id);
        }
        if self.namespaces.contains(name) || self.external_namespace(name) {
            return Res::Namespace(name.to_string());
        }
        let (namespaces, aliases, _) = self.unit_usings(path);
        let usings: Vec<Using> = aliases
            .into_iter()
            .map(|(a, t)| Using::Alias(a, t))
            .chain(namespaces.into_iter().map(Using::Namespace))
            .collect();
        let refs: Vec<&Using> = usings.iter().collect();
        match self.via_usings(&refs, name, arity, project) {
            Some((r, true)) => r,
            Some((r, false)) => guess.unwrap_or(r),
            None => guess.unwrap_or(Res::Unknown),
        }
    }

    /// A name through a level's `using` directives, and whether the answer is definite (a
    /// project type, an alias, a name the catalogue spells) rather than a wildcard's guess.
    fn via_usings(
        &self,
        usings: &[&Using],
        name: &str,
        arity: Option<usize>,
        project: &str,
    ) -> Option<(Res, bool)> {
        for u in usings {
            if let Using::Alias(a, t) = u
                && a == name
            {
                return Some((self.resolve_absolute(t, project), true));
            }
        }
        let mut externals: Vec<(u8, String)> = Vec::new();
        for u in usings {
            if let Using::Namespace(ns) = u {
                let fqn = format!("{ns}.{name}");
                if let Some(id) = self.project_type(&fqn, arity, project) {
                    return Some((Res::Project(id), true));
                }
                if let Some(score) = self.plausible(&fqn)
                    && !externals.iter().any(|(_, x)| *x == fqn)
                {
                    externals.push((score, fqn));
                }
            }
        }
        // A name the catalogue spells beats one a wildcard covers. Several spelled ones are all
        // kept (the compiler rejects a real ambiguity, so one is right and the catalogue
        // matches it); of wildcard ones only the first in using order is (the file's own
        // usings, then the project's, then the SDK's implicit ones), since a wildcard says
        // nothing of which namespace a name the catalogue does not know is in.
        let best = externals.iter().map(|(s, _)| *s).max()?;
        let mut out: Vec<String> = externals
            .into_iter()
            .filter(|(s, _)| *s == best)
            .map(|(_, n)| n)
            .collect();
        if best < 3 {
            // A `using` of a namespace neither the project nor the catalogue knows may be where
            // the name is: then a wildcard's guess would hide a call nothing explains.
            let unknown_using = usings.iter().any(|u| match u {
                Using::Namespace(ns) => !self.known_namespace(ns),
                _ => false,
            });
            if unknown_using {
                return None;
            }
            out.truncate(1);
        }
        Some((Res::External(out), best == 3))
    }

    /// A fully qualified name (an alias target, `global::A.B`).
    pub fn resolve_absolute(&self, dotted: &str, project: &str) -> Res {
        let segs: Vec<&str> = dotted.split('.').collect();
        let mut ns = String::new();
        for (i, s) in segs.iter().enumerate() {
            let fqn = if ns.is_empty() {
                s.to_string()
            } else {
                format!("{ns}.{s}")
            };
            if let Some(id) = self.project_type(&fqn, None, project) {
                let mut cur = id;
                for rest in &segs[i + 1..] {
                    match self.nested(cur, rest, None, project) {
                        Some(n) => cur = n,
                        None => return Res::External(vec![dotted.to_string()]),
                    }
                }
                return Res::Project(cur);
            }
            ns = fqn;
        }
        if self.namespaces.contains(dotted) {
            return Res::Namespace(dotted.to_string());
        }
        Res::External(vec![dotted.to_string()])
    }

    /// Resolve a dotted name as far as it names types and namespaces: the number of segments
    /// consumed and what they name. The rest are member accesses.
    pub fn resolve_path(
        &self,
        path: &str,
        scope: usize,
        enclosing: &[TypeId],
        segs: &[&str],
        last_arity: Option<usize>,
    ) -> (usize, Res) {
        let project = self.file_project(path).unwrap_or("").to_string();
        let arity_at = |i: usize| {
            if i + 1 == segs.len() {
                last_arity
            } else {
                None
            }
        };
        let mut res = self.resolve_simple(path, scope, enclosing, segs[0], arity_at(0));
        let mut used = 1;
        while used < segs.len() {
            let s = segs[used];
            let next = match &res {
                Res::Namespace(ns) => {
                    let fqn = format!("{ns}.{s}");
                    if let Some(id) = self.project_type(&fqn, arity_at(used), &project) {
                        Res::Project(id)
                    } else if self.namespaces.contains(&fqn) || self.external_namespace(&fqn) {
                        Res::Namespace(fqn)
                    } else if self.plausible(&fqn).is_some()
                        || !self.namespaces.contains(ns.as_str())
                    {
                        // An external namespace's type, known to the catalogue or not.
                        Res::External(vec![fqn])
                    } else {
                        break;
                    }
                }
                Res::Project(t) => match self.nested(*t, s, arity_at(used), &project) {
                    Some(n) => Res::Project(n),
                    None => break,
                },
                _ => break,
            };
            res = next;
            used += 1;
        }
        (used, res)
    }

    /// Resolve a type as written (a base list entry).
    pub fn resolve_type(
        &self,
        path: &str,
        scope: usize,
        enclosing: &[TypeId],
        t: &TypeText,
    ) -> Res {
        if t.global {
            let project = self.file_project(path).unwrap_or("").to_string();
            return self.resolve_absolute(&t.segs.join("."), &project);
        }
        let segs: Vec<&str> = t.segs.iter().map(String::as_str).collect();
        let (used, res) = self.resolve_path(path, scope, enclosing, &segs, Some(t.arity));
        if used == segs.len() {
            return res;
        }
        match res {
            Res::External(c) => Res::External(
                c.into_iter()
                    .map(|p| format!("{p}.{}", segs[used..].join(".")))
                    .collect(),
            ),
            _ => Res::Unknown,
        }
    }

    /// Every project type's member that is an extension method, by name.
    pub fn is_type_param(&self, enclosing: &[TypeId], name: &str) -> bool {
        enclosing
            .iter()
            .any(|&t| self.defs[t].type_params.iter().any(|p| p == name))
    }

    /// Whether a member of `t` is static (for `Type.Member` reads).
    pub fn member_kind(&self, t: TypeId, name: &str) -> Option<MemberKind> {
        self.member(t, name).map(|m| m.kind)
    }
}

fn push_unique(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

/// An external type named like an interface (`IRequestHandler`, `IDisposable`): it adds no
/// members a class must inherit.
pub fn looks_like_interface(fqn: &str) -> bool {
    let last = fqn.rsplit('.').next().unwrap_or(fqn);
    let mut cs = last.chars();
    matches!((cs.next(), cs.next()), (Some('I'), Some(c)) if c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::super::scan::scan;
    use super::*;
    use crate::resolve::Resolver;

    fn index(files: &[(&str, &str)], resolver: &[(&str, &str)], patterns: &[&str]) -> CsIndex {
        let scans = files.iter().map(|(p, s)| scan(p, s)).collect();
        let r = Resolver::from_files(
            &resolver
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect::<Vec<_>>(),
        );
        let (known, open) = known_from_patterns(patterns.iter().copied());
        CsIndex::build(scans, &r, known, open)
    }

    #[test]
    fn known_names_from_patterns() {
        let (known, open) = known_from_patterns([
            "Microsoft.Extensions.Logging.**.{LogInformation,LogError}",
            "System.Console.{Write,WriteLine}",
            "System.{String,Convert}{,()}**",
            "MediatR.{IMediator,ISender}{,()}.Send",
            "**.AsIChatClient().GetResponseAsync",
        ]);
        assert!(open.contains("Microsoft.Extensions.Logging"));
        assert!(known.contains("System.Console.Write"));
        assert!(known.contains("System.String"));
        assert!(known.contains("MediatR.IMediator.Send"));
        assert!(!open.contains("System"));
    }

    #[test]
    fn resolves_like_the_compiler() {
        let ix = index(
            &[
                (
                    "src/Api/Orders.cs",
                    r#"namespace Shop.Api.Orders;
using Shop.Domain;
public class OrdersApi : ControllerBase {
    public class Line { }
}
"#,
                ),
                (
                    "src/Api/GlobalUsings.cs",
                    "global using MediatR;\nglobal using Shop.Api.Orders;\n",
                ),
                (
                    "src/Domain/Order.cs",
                    "namespace Shop.Domain { public partial class Order { public string Email; } }",
                ),
                (
                    "src/Domain/Order.Part.cs",
                    "namespace Shop.Domain { public partial class Order : Base { public string Name; } public class Base { public int Id; } }",
                ),
            ],
            &[
                (
                    "src/Api/Api.csproj",
                    r#"<Project Sdk="Microsoft.NET.Sdk.Web"><PropertyGroup><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>"#,
                ),
                (
                    "src/Domain/Domain.csproj",
                    r#"<Project Sdk="Microsoft.NET.Sdk"></Project>"#,
                ),
            ],
            &[
                "Microsoft.Extensions.Logging.ILogger**.LogInformation",
                "Microsoft.Extensions.{Configuration,DependencyInjection,Hosting,Logging}.**",
                "MediatR.{IMediator}{,()}.Send",
                "Microsoft.AspNetCore.**.Request",
                "System.Console.WriteLine",
            ],
        );
        let api = "src/Api/Orders.cs";
        let scope = 1;
        // A project type through a `using` in the namespace declaration.
        let order = match ix.resolve_simple(api, scope, &[], "Order", None) {
            Res::Project(id) => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(ix.def(order).fqn, "Shop.Domain.Order");
        assert_eq!(ix.def(order).parts.len(), 2);
        assert!(ix.member(order, "Email").is_some());
        assert!(ix.member(order, "Name").is_some());
        assert!(
            ix.member(order, "Id").is_some(),
            "inherited from a source base"
        );
        // External types through implicit and global usings.
        assert_eq!(
            ix.resolve_simple(api, scope, &[], "ILogger", Some(1)),
            Res::External(vec!["Microsoft.Extensions.Logging.ILogger".into()])
        );
        assert_eq!(
            ix.resolve_simple(api, scope, &[], "IMediator", None),
            Res::External(vec!["MediatR.IMediator".into()])
        );
        // A qualified name: its type part is external; how far the catalogue spells it into
        // members does not change the path.
        let segs = ["System", "Console", "WriteLine"];
        let (used, res) = ix.resolve_path(api, scope, &[], &segs, None);
        let Res::External(c) = res else { panic!() };
        let full: Vec<String> = c
            .iter()
            .map(|p| {
                [p.as_str()]
                    .into_iter()
                    .chain(segs[used..].iter().copied())
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .collect();
        assert_eq!(full, vec!["System.Console.WriteLine".to_string()]);
        // Under a namespace the catalogue covers with a wildcard, any name may be its (the first
        // such using wins); outside every known namespace a name is unknown.
        assert!(matches!(
            ix.resolve_simple(api, scope, &[], "Nope", None),
            Res::External(c) if c.len() == 1
        ));
        assert_eq!(
            ix.resolve_simple("src/Domain/Order.cs", 1, &[], "Nope", None),
            Res::Unknown
        );
        // A nested type of an enclosing type, and the type's own name.
        let api_t = ix.type_at(api, ix.files[api].scan.types[0].start).unwrap();
        assert!(matches!(
            ix.resolve_simple(api, scope, &[api_t], "Line", None),
            Res::Project(_)
        ));
        assert!(ix.def(api_t).open, "extends an external base");
        assert!(!ix.def(order).open || ix.def(order).partial);
    }
}
