//! The host names systems are reached by: the names an orchestrator gives the projects it runs
//! (`catalogue/topology/systems.yml` `orchestrators`), docker-compose services by their build
//! context, and `systems: hosts:` in `.privacy-flow.yml`.

use std::collections::{BTreeMap, BTreeSet};

use crate::glob::api_matches_any;
use crate::program::{GKind, Program};
use crate::resolve::{Resolver, normalize};
use crate::topology::catalogue::TopologyCatalogue;
use crate::topology::model::{NoteRec, SystemRec};
use crate::values::{Defs, api_paths};

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn join(dir: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    if dir.is_empty() {
        normalize(&rel)
    } else {
        normalize(&format!("{dir}/{rel}"))
    }
}

/// `https+http://Orders-Api:8080/x` → `orders-api`.
pub fn authority_of(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = match url.find("://") {
        Some(i)
            if url[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
                && url[..i].starts_with(|c: char| c.is_ascii_alphabetic()) =>
        {
            &url[i + 3..]
        }
        _ => url.strip_prefix("//")?,
    };
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// A host only this machine or its network reaches.
pub fn is_local(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") || host == "[::1]" {
        return true;
    }
    let octets: Vec<u8> = host.split('.').filter_map(|o| o.parse().ok()).collect();
    if octets.len() == 4 && host.split('.').count() == 4 {
        return matches!(
            octets.as_slice(),
            [127, ..] | [10, ..] | [192, 168, ..] | [169, 254, ..] | [0, ..]
        ) || (octets[0] == 172 && (16..=31).contains(&octets[1]));
    }
    false
}

pub struct Names {
    /// Host name (lowercase) → system key.
    pub by_host: BTreeMap<String, String>,
    pub notes: Vec<NoteRec>,
}

pub struct Inputs<'a> {
    pub program: &'a Program,
    pub defs: &'a Defs,
    pub resolver: &'a Resolver,
    pub topo: &'a TopologyCatalogue,
    /// docker-compose files: (path, text).
    pub compose: &'a [(String, String)],
}

/// Collect aliases into the systems (`aliases`) and return the host table.
pub fn collect(inp: &Inputs, systems: &mut [SystemRec]) -> Names {
    let p = inp.program;
    let mut found: Vec<(String, String, String)> = Vec::new(); // (host, system key, evidence)
    let mut notes = Vec::new();
    let by_root: BTreeMap<String, String> = systems
        .iter()
        .filter(|s| !s.declared)
        .map(|s| (s.root.clone(), s.key.clone()))
        .collect();
    let by_manifest: BTreeMap<String, String> = systems
        .iter()
        .filter_map(|s| s.manifest.clone().map(|m| (m, s.key.clone())))
        .collect();
    // A project in the generated `Projects` namespace: the project file's name with dots and
    // dashes as underscores.
    let by_generated: BTreeMap<String, String> = systems
        .iter()
        .filter_map(|s| {
            let m = s.manifest.as_deref()?;
            let stem = m.rsplit('/').next()?.strip_suffix(".csproj")?;
            Some((stem.replace(['.', '-'], "_"), s.key.clone()))
        })
        .collect();

    for (i, st) in p.stmts.iter().enumerate() {
        let GKind::Call { dst, args, .. } = &st.kind else {
            continue;
        };
        let lang = p.files[st.file as usize].lang.family();
        let paths: Vec<String> = p
            .calls
            .get(&(i as u32))
            .map(|t| t.apis.iter().map(|s| p.syms.str(*s).to_string()).collect())
            .unwrap_or_default();
        for o in &inp.topo.orchestrators {
            if o.language != lang || !paths.iter().any(|a| api_matches_any(&o.calls, a)) {
                continue;
            }
            let Some(name) = args
                .get(o.name_arg)
                .and_then(|a| inp.defs.literal_prefix(p, a.var))
                .and_then(|(s, whole)| whole.then_some(s))
            else {
                continue;
            };
            let file = &p.files[st.file as usize].path;
            let base = inp
                .resolver
                .csproj_for(file)
                .map(|c| dir_of(&c.path).to_string())
                .unwrap_or_else(|| dir_of(file).to_string());
            let mut target: Option<String> = None;
            if let Some(path) = o
                .path_arg
                .and_then(|k| args.get(k))
                .and_then(|a| inp.defs.literal_prefix(p, a.var))
                .and_then(|(s, whole)| whole.then_some(s))
            {
                let path = join(&base, &path);
                target = by_manifest
                    .get(&path)
                    .or_else(|| by_root.get(&path))
                    .or_else(|| by_root.get(dir_of(&path)))
                    .cloned();
            }
            if target.is_none()
                && let Some(ns) = &o.type_arg_namespace
            {
                // `AddProject<Projects.Orders_Api>`: a type reference on the call's result.
                target = p.stmts.iter().find_map(|t| match &t.kind {
                    GKind::TypeRef { dst: d, ty } if d == dst && t.file == st.file => {
                        api_paths(p, *ty).find_map(|a| {
                            let segs: Vec<&str> = a.split('.').collect();
                            let n = segs.len();
                            (n >= 2 && segs[n - 2] == ns)
                                .then(|| by_generated.get(segs[n - 1]).cloned())
                                .flatten()
                        })
                    }
                    _ => None,
                });
            }
            match target {
                Some(key) => found.push((
                    name.to_ascii_lowercase(),
                    key,
                    format!("{}:{}", file, st.pos.line),
                )),
                None => notes.push(NoteRec {
                    kind: "orchestrated_resource_unresolved".into(),
                    detail: format!(
                        "{} names {name}, which is not a system of the repository",
                        o.id
                    ),
                    paths: vec![format!("{}:{}", file, st.pos.line)],
                }),
            }
        }
    }

    for (path, text) in inp.compose {
        let Ok(v) = serde_saphyr::from_str::<serde_json::Value>(text) else {
            notes.push(NoteRec {
                kind: "compose_unreadable".into(),
                detail: "not YAML".into(),
                paths: vec![path.clone()],
            });
            continue;
        };
        let Some(services) = v.get("services").and_then(|s| s.as_object()) else {
            continue;
        };
        for (name, svc) in services {
            let context = match svc.get("build") {
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(b) => b
                    .get("context")
                    .and_then(|c| c.as_str())
                    .map(str::to_string),
                None => None,
            };
            let Some(context) = context else { continue };
            let dir = join(dir_of(path), &context);
            let Some(key) = by_root.get(&dir).cloned() else {
                continue;
            };
            let mut hosts: BTreeSet<String> = BTreeSet::new();
            hosts.insert(name.clone());
            for k in ["hostname", "container_name"] {
                if let Some(h) = svc.get(k).and_then(|h| h.as_str()) {
                    hosts.insert(h.to_string());
                }
            }
            if let Some(nets) = svc.get("networks").and_then(|n| n.as_object()) {
                for n in nets.values() {
                    for a in n
                        .get("aliases")
                        .and_then(|a| a.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let Some(a) = a.as_str() {
                            hosts.insert(a.to_string());
                        }
                    }
                }
            }
            for h in hosts {
                found.push((h.to_ascii_lowercase(), key.clone(), path.clone()));
            }
        }
    }
    for s in systems.iter() {
        for h in &s.aliases {
            found.push((
                h.to_ascii_lowercase(),
                s.key.clone(),
                "config:systems".into(),
            ));
        }
    }

    // A name two systems claim names neither.
    let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (h, k, _) in &found {
        claims.entry(h.clone()).or_default().insert(k.clone());
    }
    let mut by_host = BTreeMap::new();
    for (h, keys) in claims {
        if keys.len() == 1 {
            by_host.insert(h, keys.into_iter().next().expect("one"));
        } else {
            notes.push(NoteRec {
                kind: "ambiguous_host".into(),
                detail: format!(
                    "{h} names {}",
                    keys.into_iter().collect::<Vec<_>>().join(" and ")
                ),
                paths: Vec::new(),
            });
        }
    }
    for s in systems.iter_mut() {
        let mut aliases: BTreeSet<String> =
            s.aliases.iter().map(|a| a.to_ascii_lowercase()).collect();
        let mut evidence: BTreeSet<String> = BTreeSet::new();
        for (h, k, e) in &found {
            if *k == s.key && by_host.get(h) == Some(k) {
                aliases.insert(h.clone());
                if e != "config:systems" {
                    evidence.insert(e.clone());
                }
            }
        }
        s.aliases = aliases.into_iter().collect();
        for e in evidence {
            s.evidence.push(crate::topology::model::Evidence {
                rule: "alias".into(),
                path: e,
            });
        }
        s.evidence.sort();
        s.evidence.dedup();
    }
    Names { by_host, notes }
}

#[cfg(test)]
mod tests {
    use super::{authority_of, is_local};

    #[test]
    fn authorities() {
        assert_eq!(
            authority_of("https+http://Orders-Api:8080/x").as_deref(),
            Some("orders-api")
        );
        assert_eq!(
            authority_of("http://user@api.example.com/").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(authority_of("/api/orders"), None);
        assert_eq!(
            authority_of("//cdn.example.com/a").as_deref(),
            Some("cdn.example.com")
        );
        assert!(is_local("localhost") && is_local("10.1.2.3") && is_local("172.20.0.1"));
        assert!(!is_local("orders-api") && !is_local("8.8.8.8"));
    }
}
