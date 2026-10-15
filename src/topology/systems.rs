//! The systems of a repository: one per deployable unit (`catalogue/topology/systems.yml`) or
//! per `systems:` declaration, else the repository as one. Each file belongs to the nearest unit
//! above it; a system's closure is its own files and those of the libraries it references or
//! imports, never another system's own files.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalogue::Catalogue;
use crate::config::Config;
use crate::glob::{Glob, api_matches_any};
use crate::output::yaml::snake;
use crate::program::{GKind, Program, Target};
use crate::resolve::Resolver;
use crate::topology::catalogue::{Deployable, TopologyCatalogue};
use crate::topology::model::{Evidence, NoteRec, SystemRec};

pub struct Inputs<'a> {
    pub program: &'a Program,
    pub resolver: &'a Resolver,
    /// Every listed file that is not excluded (marker files are not all analysed).
    pub files: &'a [String],
    pub config: &'a Config,
    pub catalogue: &'a Catalogue,
    pub topo: &'a TopologyCatalogue,
}

pub struct Systems {
    pub recs: Vec<SystemRec>,
    /// Per system, per program file: in its closure.
    pub closure: Vec<Vec<bool>>,
    /// Per system, per program file: its own.
    pub own: Vec<Vec<bool>>,
    pub mode: String,
    pub notes: Vec<NoteRec>,
}

/// A project, package or marked directory.
#[derive(Clone, Debug, Default)]
struct Unit {
    dir: String,
    /// Language family of the files it owns; `None` for a marker of any language.
    lang: Option<&'static str>,
    manifest: Option<String>,
    identity: String,
    /// (rule, kind) when deployable.
    deployable: Option<(String, String)>,
    never: bool,
    /// Directories of the units it references (C# `ProjectReference`s).
    refs: Vec<String>,
}

type UnitKey = (String, Option<&'static str>);

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn base_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `a/b/c` → `a/b`, `a/b`, …, `""`: the directory and its ancestors, nearest first.
fn ancestors(dir: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(dir);
    std::iter::from_fn(move || {
        let d = next?;
        next = (!d.is_empty()).then(|| dir_of(d));
        Some(d)
    })
}

fn project_matches(d: &Deployable, p: &crate::resolve::CsProject) -> bool {
    let Some(m) = &d.project else { return false };
    (m.sdk.is_empty() || m.sdk.iter().any(|s| s == &p.sdk))
        && (m.output_type.is_empty()
            || p.output_type
                .as_ref()
                .is_some_and(|o| m.output_type.iter().any(|t| t.eq_ignore_ascii_case(o))))
        && (m.package_refs.is_empty()
            || p.package_refs
                .iter()
                .any(|r| api_matches_any(&m.package_refs, r)))
        && m.properties.iter().all(|(k, v)| {
            p.properties
                .get(k)
                .is_some_and(|x| x.eq_ignore_ascii_case(v))
        })
}

fn package_matches(
    d: &Deployable,
    m: &crate::resolve::Manifest,
    framework_modules: &BTreeSet<String>,
) -> bool {
    let Some(pm) = &d.package else { return false };
    let deps =
        pm.dependencies.is_empty() || m.dependencies.iter().any(|x| pm.dependencies.contains(x));
    let fw = !pm.framework_dependency
        || m.dependencies.iter().any(|x| {
            framework_modules
                .iter()
                .any(|f| f == x || f.starts_with(&format!("{x}/")))
        });
    let scripts = pm.scripts.is_empty() || m.scripts.iter().any(|s| pm.scripts.contains(s));
    deps && fw && scripts
}

fn lang_ok(d: &Deployable, lang: &str) -> bool {
    d.language.as_deref().is_none_or(|l| l == lang)
}

fn units(inp: &Inputs) -> BTreeMap<UnitKey, Unit> {
    let topo = inp.topo;
    let mut units: BTreeMap<UnitKey, Unit> = BTreeMap::new();
    for (dir, p) in &inp.resolver.csprojs {
        let mut u = Unit {
            dir: dir.clone(),
            lang: Some("csharp"),
            manifest: Some(p.path.clone()),
            identity: base_name(&p.path).trim_end_matches(".csproj").to_string(),
            refs: p
                .project_refs
                .iter()
                .map(|r| dir_of(r).to_string())
                .collect(),
            ..Unit::default()
        };
        u.never = topo
            .never
            .iter()
            .any(|d| lang_ok(d, "csharp") && project_matches(d, p));
        u.deployable = topo
            .deployables
            .iter()
            .find(|d| lang_ok(d, "csharp") && project_matches(d, p))
            .map(|d| {
                (
                    d.id.clone(),
                    d.kind.clone().unwrap_or_else(|| "service".into()),
                )
            });
        units.insert((dir.clone(), Some("csharp")), u);
    }
    let framework_modules: BTreeSet<String> = inp
        .catalogue
        .frameworks
        .iter()
        .filter(|f| f.supported && f.language == "javascript")
        .flat_map(|f| f.modules.iter().cloned())
        .collect();
    for (dir, m) in &inp.resolver.manifests {
        let identity = m.name.clone().unwrap_or_else(|| base_name(dir).to_string());
        let deployable = topo
            .deployables
            .iter()
            .find(|d| lang_ok(d, "javascript") && package_matches(d, m, &framework_modules))
            .map(|d| {
                (
                    d.id.clone(),
                    d.kind.clone().unwrap_or_else(|| "service".into()),
                )
            });
        units.insert(
            (dir.clone(), Some("javascript")),
            Unit {
                dir: dir.clone(),
                lang: Some("javascript"),
                manifest: Some(m.path.clone()),
                identity,
                deployable,
                ..Unit::default()
            },
        );
    }
    for (dir, p) in &inp.resolver.pyprojects {
        units.insert(
            (dir.clone(), Some("python")),
            Unit {
                dir: dir.clone(),
                lang: Some("python"),
                manifest: Some(p.path.clone()),
                identity: p.name.clone().unwrap_or_else(|| base_name(dir).to_string()),
                ..Unit::default()
            },
        );
    }
    // Marker files and module-level application objects make their directory a root.
    let listed: BTreeSet<&str> = inp.files.iter().map(String::as_str).collect();
    let mut marked: Vec<(String, Option<&'static str>, &Deployable, String)> = Vec::new();
    for d in &topo.deployables {
        let lang = d.language.as_deref().map(family);
        for f in inp.files {
            if d.marker_files.iter().any(|m| m == base_name(f)) {
                marked.push((dir_of(f).to_string(), lang, d, f.clone()));
            }
        }
        if d.module_values.is_empty() {
            continue;
        }
        let p = inp.program;
        for s in &p.stmts {
            let file = &p.files[s.file as usize];
            if s.func != file.module_func || lang.is_some_and(|l| l != file.lang.family()) {
                continue;
            }
            let GKind::Call { dst, .. } = &s.kind else {
                continue;
            };
            if !crate::values::api_paths(p, *dst).any(|a| api_matches_any(&d.module_values, &a)) {
                continue;
            }
            let root = ancestors(dir_of(&file.path))
                .find(|dir| {
                    d.root_markers.iter().any(|m| {
                        let candidate = if dir.is_empty() {
                            m.clone()
                        } else {
                            format!("{dir}/{m}")
                        };
                        listed.contains(candidate.as_str())
                    })
                })
                .unwrap_or_else(|| dir_of(&file.path));
            marked.push((root.to_string(), lang, d, file.path.clone()));
        }
    }
    marked.sort_by(|a, b| (&a.0, &a.3).cmp(&(&b.0, &b.3)));
    for (dir, lang, d, evidence) in marked {
        let kind = d.kind.clone().unwrap_or_else(|| "service".into());
        // A marker of any language makes every unit in its directory deployable.
        let mut hit = false;
        for ((udir, _), u) in units.iter_mut() {
            if *udir == dir && (lang.is_none() || u.lang == lang) {
                hit = true;
                if u.deployable.is_none() && !u.never {
                    u.deployable = Some((d.id.clone(), kind.clone()));
                    u.manifest.get_or_insert(evidence.clone());
                }
            }
        }
        if !hit {
            units.entry((dir.clone(), lang)).or_insert_with(|| Unit {
                dir: dir.clone(),
                lang,
                manifest: Some(evidence.clone()),
                identity: if dir.is_empty() {
                    "repository".into()
                } else {
                    base_name(&dir).to_string()
                },
                deployable: Some((d.id.clone(), kind.clone())),
                ..Unit::default()
            });
        }
    }
    for u in units.values_mut() {
        if u.never {
            u.deployable = None;
        }
    }
    units
}

fn family(lang: &str) -> &'static str {
    match lang {
        "csharp" => "csharp",
        "python" => "python",
        _ => "javascript",
    }
}

/// The unit a file belongs to: the nearest above it, preferring one of its own language (or a
/// marker of any language) in the same directory. A script under a C# web project's `wwwroot`
/// is that project's.
fn owner_unit(units: &BTreeMap<UnitKey, Unit>, path: &str, lang: &'static str) -> Option<UnitKey> {
    for dir in ancestors(dir_of(path)) {
        for key in [(dir.to_string(), Some(lang)), (dir.to_string(), None)] {
            if units.contains_key(&key) {
                return Some(key);
            }
        }
        if let Some((key, _)) = units
            .range((dir.to_string(), None)..)
            .take_while(|((d, _), _)| d == dir)
            .next()
        {
            return Some(key.clone());
        }
    }
    None
}

/// Keys from bases, made distinct by prefixing the directories of the clashing ones,
/// innermost first; `fixed` keys (declared) are taken as they are.
fn unique_keys(items: &[(String, String, bool)]) -> Vec<String> {
    let mut depth = vec![0usize; items.len()];
    let key = |i: usize, d: usize| -> String {
        let (base, dir, fixed) = &items[i];
        if *fixed || d == 0 {
            return base.clone();
        }
        let segs: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        let from = segs.len().saturating_sub(d);
        let mut parts: Vec<String> = segs[from..].iter().map(|s| snake(s)).collect();
        parts.push(base.clone());
        parts.retain(|p| !p.is_empty());
        parts.join("_")
    };
    loop {
        let keys: Vec<String> = (0..items.len()).map(|i| key(i, depth[i])).collect();
        let mut changed = false;
        for i in 0..items.len() {
            let clash = keys
                .iter()
                .enumerate()
                .any(|(j, k)| j != i && *k == keys[i]);
            let room = items[i].1.split('/').filter(|s| !s.is_empty()).count();
            if clash && !items[i].2 && depth[i] < room {
                depth[i] += 1;
                changed = true;
            }
        }
        if !changed {
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();
            return keys
                .into_iter()
                .map(|k| {
                    let n = seen.entry(k.clone()).or_default();
                    *n += 1;
                    if *n == 1 { k } else { format!("{k}_{n}") }
                })
                .collect();
        }
    }
}

pub fn detect(inp: &Inputs) -> Systems {
    let p = inp.program;
    let nfiles = p.files.len();
    let units = if inp.config.system_detection {
        units(inp)
    } else {
        BTreeMap::new()
    };
    // Owning unit of each analysed file, and the units that compile it from elsewhere.
    let unit_of: Vec<Option<UnitKey>> = p
        .files
        .iter()
        .map(|f| owner_unit(&units, &f.path, f.lang.family()))
        .collect();

    // Declared systems claim files first.
    let mut notes: Vec<NoteRec> = Vec::new();
    let decls = &inp.config.systems;
    let globs: Vec<Vec<Glob>> = decls
        .iter()
        .map(|d| d.paths.iter().filter_map(|g| Glob::path(g).ok()).collect())
        .collect();
    let claimed: Vec<Option<usize>> = p
        .files
        .iter()
        .map(|f| {
            globs
                .iter()
                .position(|gs| gs.iter().any(|g| g.is_match(&f.path)))
        })
        .collect();

    struct Draft {
        base: String,
        dir: String,
        fixed: bool,
        rec: SystemRec,
        own: Vec<bool>,
        unit: Option<UnitKey>,
    }
    let mut drafts: Vec<Draft> = Vec::new();
    for (i, d) in decls.iter().enumerate() {
        let own: Vec<bool> = claimed.iter().map(|c| *c == Some(i)).collect();
        drafts.push(Draft {
            base: d.fides_key.clone(),
            dir: String::new(),
            fixed: true,
            rec: SystemRec {
                key: d.fides_key.clone(),
                name: d.name.clone().unwrap_or_else(|| d.fides_key.clone()),
                kind: "service".into(),
                language: None,
                root: String::new(),
                manifest: None,
                declared: true,
                files: 0,
                closure_files: 0,
                libraries: Vec::new(),
                evidence: vec![Evidence {
                    rule: "config:systems".into(),
                    path: d.paths.join(", "),
                }],
                aliases: d.hosts.clone(),
                user_input: Default::default(),
                datasets: Vec::new(),
            },
            own,
            unit: None,
        });
    }
    for (key, u) in &units {
        let Some((rule, kind)) = &u.deployable else {
            continue;
        };
        let own: Vec<bool> = (0..nfiles)
            .map(|f| unit_of[f].as_ref() == Some(key) && claimed[f].is_none())
            .collect();
        let owned_any = (0..nfiles).any(|f| unit_of[f].as_ref() == Some(key));
        if !owned_any {
            // Nothing of it is analysed (excluded, or no supported files).
            continue;
        }
        if !own.iter().any(|x| *x) {
            notes.push(NoteRec {
                kind: "claimed_by_config".into(),
                detail: format!("{} is declared under systems:", u.identity),
                paths: u.manifest.iter().cloned().collect(),
            });
            continue;
        }
        drafts.push(Draft {
            base: snake(&u.identity),
            dir: dir_of(&u.dir).to_string(),
            fixed: false,
            rec: SystemRec {
                key: String::new(),
                name: u.identity.clone(),
                kind: kind.clone(),
                language: u.lang.map(str::to_string),
                root: u.dir.clone(),
                manifest: u.manifest.clone(),
                declared: false,
                files: 0,
                closure_files: 0,
                libraries: Vec::new(),
                evidence: vec![Evidence {
                    rule: rule.clone(),
                    path: u.manifest.clone().unwrap_or_else(|| u.dir.clone()),
                }],
                aliases: Vec::new(),
                user_input: Default::default(),
                datasets: Vec::new(),
            },
            own,
            unit: Some(key.clone()),
        });
    }

    let mode;
    if drafts.is_empty() {
        mode = "repository".to_string();
        let (key, name) = match &inp.config.system {
            Some(s) => (
                s.fides_key.clone(),
                s.name.clone().unwrap_or_else(|| s.fides_key.clone()),
            ),
            None => ("this_system".to_string(), "repository".to_string()),
        };
        drafts.push(Draft {
            base: key.clone(),
            dir: String::new(),
            fixed: true,
            rec: SystemRec {
                key,
                name,
                kind: "repository".into(),
                language: None,
                root: String::new(),
                manifest: None,
                declared: inp.config.system.is_some(),
                files: 0,
                closure_files: 0,
                libraries: Vec::new(),
                evidence: Vec::new(),
                aliases: Vec::new(),
                user_input: Default::default(),
                datasets: Vec::new(),
            },
            own: vec![true; nfiles],
            unit: None,
        });
    } else {
        let declared = drafts.iter().any(|d| d.rec.declared);
        let detected = drafts.iter().any(|d| !d.rec.declared);
        mode = match (declared, detected) {
            (true, true) => "mixed",
            (true, false) => "config",
            _ => "detected",
        }
        .to_string();
        // `system:` names the only system when there is one.
        if let (Some(s), [only]) = (&inp.config.system, drafts.as_mut_slice())
            && !only.rec.declared
        {
            only.base = s.fides_key.clone();
            only.fixed = true;
            if let Some(n) = &s.name {
                only.rec.name = n.clone();
            }
        } else if inp.config.system.is_some() && drafts.len() > 1 {
            notes.push(NoteRec {
                kind: "system_ignored".into(),
                detail: "system: names one system, but the repository has several; declare them under systems:".into(),
                paths: Vec::new(),
            });
        }
    }
    let items: Vec<(String, String, bool)> = drafts
        .iter()
        .map(|d| (d.base.clone(), d.dir.clone(), d.fixed))
        .collect();
    for (d, k) in drafts.iter_mut().zip(unique_keys(&items)) {
        d.rec.key = if k.is_empty() { "system".into() } else { k };
    }
    drafts.sort_by(|a, b| a.rec.key.cmp(&b.rec.key));

    // Owner system of each file: the system whose own files include it.
    let mut owner_sys: Vec<Option<usize>> = vec![None; nfiles];
    for (s, d) in drafts.iter().enumerate() {
        for (owner, own) in owner_sys.iter_mut().zip(&d.own) {
            if *own && owner.is_none() {
                *owner = Some(s);
            }
        }
    }
    // Local import edges, by importing file.
    let mut imports: Vec<Vec<usize>> = vec![Vec::new(); nfiles];
    for imp in &p.imports {
        if let Target::Local(t) = imp.target {
            imports[imp.file as usize].push(t as usize);
        }
    }
    let path_index: BTreeMap<&str, usize> = p
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();

    let mut closure: Vec<Vec<bool>> = Vec::new();
    for (s, d) in drafts.iter_mut().enumerate() {
        let mut inside = d.own.clone();
        let mut libs: BTreeSet<String> = BTreeSet::new();
        // Referenced library units (C#), transitively.
        if let Some(start) = &d.unit {
            let mut stack: Vec<String> = units[start].refs.clone();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            while let Some(dir) = stack.pop() {
                if !seen.insert(dir.clone()) {
                    continue;
                }
                let key = (dir.clone(), start.1);
                let Some(u) = units.get(&key) else { continue };
                if u.deployable.is_some() {
                    continue;
                }
                libs.insert(dir.clone());
                stack.extend(u.refs.iter().cloned());
                for f in 0..nfiles {
                    if unit_of[f].as_ref() == Some(&key) && owner_sys[f].is_none() {
                        inside[f] = true;
                    }
                }
            }
            // Files compiled in from elsewhere (`<Compile Include>`).
            let dirs: BTreeSet<&str> = libs
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(start.0.as_str()))
                .collect();
            for (file, projects) in &inp.resolver.cs_links_all {
                if projects.iter().any(|x| dirs.contains(x.as_str()))
                    && let Some(&f) = path_index.get(file.as_str())
                {
                    inside[f] = true;
                }
            }
        }
        // Local imports, never into another system's own files.
        let mut stack: Vec<usize> = (0..nfiles).filter(|f| inside[*f]).collect();
        while let Some(f) = stack.pop() {
            for &t in &imports[f] {
                if inside[t] || owner_sys[t].is_some_and(|o| o != s) {
                    continue;
                }
                inside[t] = true;
                stack.push(t);
            }
        }
        for f in 0..nfiles {
            if inside[f]
                && !d.own[f]
                && let Some(k) = &unit_of[f]
            {
                libs.insert(k.0.clone());
            }
        }
        d.rec.files = d.own.iter().filter(|x| **x).count() as u32;
        d.rec.closure_files = inside.iter().filter(|x| **x).count() as u32;
        d.rec.libraries = libs.into_iter().collect();
        closure.push(inside);
    }
    notes.sort();
    let own: Vec<Vec<bool>> = drafts.iter().map(|d| d.own.clone()).collect();
    Systems {
        recs: drafts.into_iter().map(|d| d.rec).collect(),
        closure,
        own,
        mode,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::unique_keys;

    #[test]
    fn keys_are_distinct() {
        let items = vec![
            (
                "catalog_service".to_string(),
                "src/HybridApp".to_string(),
                false,
            ),
            (
                "catalog_service".to_string(),
                "src/WebApp".to_string(),
                false,
            ),
            ("orders".to_string(), "".to_string(), true),
            ("ordering_api".to_string(), "src".to_string(), false),
        ];
        assert_eq!(
            unique_keys(&items),
            [
                "hybrid_app_catalog_service",
                "web_app_catalog_service",
                "orders",
                "ordering_api"
            ]
        );
    }
}
