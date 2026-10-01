//! Module resolution data for JavaScript and TypeScript: `tsconfig.json` path aliases and
//! workspace packages.
//!
//! A monorepo imports its own packages by name (`@acme/db`) and its apps use path aliases
//! (`~/lib/x`, `@/components/y`). Both resolve to scanned files here, so the analysis follows
//! calls across packages instead of treating them as unknown external modules.
//!
//! - **tsconfig:** every `tsconfig.json` / `jsconfig.json` among the scanned files applies to
//!   the files below its directory; the nearest one wins. `extends` is followed for relative
//!   paths and workspace packages (not for npm packages, which are not scanned).
//! - **workspaces:** every `package.json` with a `name` makes that name resolve to its
//!   directory; `exports` (`"."` and `"./*"` patterns, string targets), `module`, `main` and
//!   `types` name the entry point, falling back to `index`.

use std::collections::BTreeMap;

use crate::files::strip_jsonc;

/// `compilerOptions.paths` and `baseUrl`, with targets relative to the scan root.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aliases {
    pub base_url: Option<String>,
    /// (pattern with at most one `*`, targets with at most one `*`), longest pattern first.
    pub paths: Vec<(String, Vec<String>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Package {
    pub dir: String,
    /// Entry files for the bare package name, in preference order (root-relative, no extension
    /// resolution applied yet).
    pub entries: Vec<String>,
    /// `exports` subpath patterns: (`./*` → `./src/*.ts`), root-relative targets.
    pub subpaths: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct Resolver {
    /// tsconfig directory (root-relative, `""` for the root) → aliases.
    pub tsconfigs: BTreeMap<String, Aliases>,
    pub packages: BTreeMap<String, Package>,
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

pub fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        normalize(rel)
    } else {
        normalize(&format!("{dir}/{rel}"))
    }
}

fn is_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    (name.starts_with("tsconfig") && name.ends_with(".json")) || name == "jsconfig.json"
}

/// The files a resolver needs: every `package.json`, `tsconfig*.json` and `jsconfig.json`.
pub fn wants(path: &str) -> bool {
    path.rsplit('/').next() == Some("package.json") || is_config(path)
}

impl Resolver {
    /// Build from `(path, contents)` of the files [`wants`] selects.
    pub fn from_files(files: &[(String, String)]) -> Resolver {
        let texts: BTreeMap<&str, &str> = files
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        let mut r = Resolver::default();
        for (path, text) in &texts {
            if path.rsplit('/').next() != Some("package.json") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            let Some(name) = v.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let dir = dir_of(path).to_string();
            let mut entries = Vec::new();
            let mut subpaths = Vec::new();
            match v.get("exports") {
                Some(serde_json::Value::String(s)) => entries.push(join(&dir, s)),
                Some(serde_json::Value::Object(map)) => {
                    for (k, target) in map {
                        let t = export_target(target);
                        let Some(t) = t else { continue };
                        if k == "." {
                            entries.push(join(&dir, &t));
                        } else if k.starts_with("./") {
                            subpaths.push((k.clone(), join(&dir, &t)));
                        }
                    }
                }
                _ => {}
            }
            for key in ["types", "module", "main"] {
                if let Some(s) = v.get(key).and_then(|m| m.as_str()) {
                    entries.push(join(&dir, s));
                }
            }
            entries.push(join(&dir, "index"));
            entries.push(join(&dir, "src/index"));
            // The first package to claim a name wins, in path order.
            r.packages.entry(name.to_string()).or_insert(Package {
                dir,
                entries,
                subpaths,
            });
        }
        for path in texts.keys() {
            if !is_config(path) {
                continue;
            }
            let name = path.rsplit('/').next().unwrap_or(path);
            // Only the canonical names apply to a directory; tsconfig.build.json and friends
            // are reachable through `extends`.
            if name != "tsconfig.json" && name != "jsconfig.json" {
                continue;
            }
            let dir = dir_of(path).to_string();
            if r.tsconfigs.contains_key(&dir) {
                continue;
            }
            let aliases = r.load_tsconfig(path, &texts, 0);
            r.tsconfigs.insert(dir, aliases);
        }
        r
    }

    fn load_tsconfig(&self, path: &str, texts: &BTreeMap<&str, &str>, depth: u32) -> Aliases {
        let Some(text) = texts.get(path) else {
            return Aliases::default();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(text)) else {
            return Aliases::default();
        };
        let dir = dir_of(path);
        // Start from what this config extends; its own options override.
        let mut out = Aliases::default();
        if depth < 8 {
            let parents: Vec<String> = match v.get("extends") {
                Some(serde_json::Value::String(s)) => vec![s.clone()],
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            };
            for parent in parents {
                let resolved = if parent.starts_with('.') {
                    let p = join(dir, &parent);
                    if p.ends_with(".json") {
                        p
                    } else {
                        format!("{p}.json")
                    }
                } else {
                    match self.package_file(&parent) {
                        Some(p) => p,
                        None => continue,
                    }
                };
                let inherited = self.load_tsconfig(&resolved, texts, depth + 1);
                if inherited.base_url.is_some() {
                    out.base_url = inherited.base_url;
                }
                if !inherited.paths.is_empty() {
                    out.paths = inherited.paths;
                }
            }
        }
        let opts = v.get("compilerOptions");
        let base = opts
            .and_then(|o| o.get("baseUrl"))
            .and_then(|b| b.as_str())
            .map(|b| join(dir, b));
        if let Some(b) = &base {
            out.base_url = Some(b.clone());
        }
        if let Some(paths) = opts
            .and_then(|o| o.get("paths"))
            .and_then(|p| p.as_object())
        {
            // Path targets are relative to baseUrl, or to the config's own directory.
            let from = base.clone().unwrap_or_else(|| dir.to_string());
            let mut entries: Vec<(String, Vec<String>)> = paths
                .iter()
                .map(|(k, v)| {
                    let targets = v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|t| t.as_str())
                                .map(|t| join(&from, t))
                                .collect()
                        })
                        .unwrap_or_default();
                    (k.clone(), targets)
                })
                .collect();
            entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
            out.paths = entries;
        }
        out
    }

    /// `@scope/pkg/sub/file.json` → the root-relative path inside a workspace package.
    fn package_file(&self, spec: &str) -> Option<String> {
        let (name, sub) = split_package(spec);
        let pkg = self.packages.get(name)?;
        Some(join(
            &pkg.dir,
            if sub.is_empty() { "tsconfig.json" } else { sub },
        ))
    }

    /// The aliases that apply to a file: the nearest tsconfig above it.
    pub fn tsconfig_for(&self, file: &str) -> Option<&Aliases> {
        let mut dir = dir_of(file);
        loop {
            if let Some(a) = self.tsconfigs.get(dir) {
                return Some(a);
            }
            if dir.is_empty() {
                return None;
            }
            dir = dir_of(dir);
        }
    }

    /// Candidate root-relative paths (before extension resolution) for a workspace import.
    pub fn package_candidates(&self, spec: &str) -> Option<Vec<String>> {
        let (name, sub) = split_package(spec);
        let pkg = self.packages.get(name)?;
        if sub.is_empty() {
            return Some(pkg.entries.clone());
        }
        let wanted = format!("./{sub}");
        let mut out = Vec::new();
        for (pattern, target) in &pkg.subpaths {
            if let Some((pre, post)) = pattern.split_once('*') {
                if let Some(mid) = wanted.strip_prefix(pre).and_then(|r| r.strip_suffix(post)) {
                    out.push(target.replacen('*', mid, 1));
                }
            } else if *pattern == wanted {
                out.push(target.clone());
            }
        }
        out.push(join(&pkg.dir, sub));
        out.push(join(&pkg.dir, &format!("src/{sub}")));
        Some(out)
    }
}

fn split_package(spec: &str) -> (&str, &str) {
    let mut idx = spec.find('/');
    if spec.starts_with('@') {
        idx = idx.and_then(|i| spec[i + 1..].find('/').map(|j| i + 1 + j));
    }
    match idx {
        Some(i) => (&spec[..i], &spec[i + 1..]),
        None => (spec, ""),
    }
}

/// The string target of an `exports` entry: a string, or the first of `types`, `import`,
/// `default`, `require` in a conditions object.
fn export_target(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(m) => ["types", "import", "default", "require", "node"]
            .iter()
            .find_map(|k| m.get(*k).and_then(export_target)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn nested_tsconfig_with_extends_and_workspaces() {
        let r = Resolver::from_files(&files(&[
            (
                "package.json",
                r#"{"name":"root","workspaces":["apps/*","packages/*"]}"#,
            ),
            (
                "packages/db/package.json",
                r#"{"name":"@acme/db","main":"./index.ts"}"#,
            ),
            (
                "packages/ui/package.json",
                r#"{"name":"@acme/ui","exports":{".":"./src/index.ts","./*":"./src/*.tsx"}}"#,
            ),
            (
                "packages/tsconfig/package.json",
                r#"{"name":"@acme/tsconfig"}"#,
            ),
            (
                "packages/tsconfig/base.json",
                r##"{"compilerOptions":{"baseUrl":".","paths":{"#base/*":["./x/*"]}}}"##,
            ),
            (
                "apps/web/tsconfig.json",
                r#"{ "extends": "@acme/tsconfig/base.json", // inherits
               "compilerOptions": { "baseUrl": ".", "paths": { "~/*": ["./app/*"], } } }"#,
            ),
        ]));
        let a = r.tsconfig_for("apps/web/app/routes/x.tsx").unwrap();
        assert_eq!(
            a.paths,
            vec![("~/*".to_string(), vec!["apps/web/app/*".to_string()])]
        );
        assert_eq!(a.base_url.as_deref(), Some("apps/web"));
        assert!(r.tsconfig_for("packages/db/index.ts").is_none());
        assert_eq!(
            r.package_candidates("@acme/db").unwrap()[0],
            "packages/db/index.ts"
        );
        assert_eq!(
            r.package_candidates("@acme/ui/button").unwrap()[0],
            "packages/ui/src/button.tsx"
        );
        assert_eq!(split_package("@a/b/c/d"), ("@a/b", "c/d"));
        assert_eq!(split_package("lodash/fp"), ("lodash", "fp"));
    }
}
