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
//! - **C# projects:** every `.csproj` applies to the files below its directory (the nearest
//!   wins), with the `Directory.Build.props` above it: its SDK and `ImplicitUsings` give the
//!   namespaces every file sees, as do its `<Using>` items. Files a project compiles from
//!   elsewhere (`<Compile Include="..\Shared\x.cs">`) belong to it too.

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

/// A `<Using>` item of a C# project.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsingItem {
    pub name: String,
    pub alias: Option<String>,
    pub is_static: bool,
    pub remove: bool,
}

/// A C# project: what its files see without a `using` of their own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CsProject {
    /// The `.csproj` path; its directory is the project's.
    pub path: String,
    /// `Microsoft.NET.Sdk.Web`, without a version.
    pub sdk: String,
    pub implicit_usings: bool,
    pub usings: Vec<UsingItem>,
    pub root_namespace: Option<String>,
}

impl CsProject {
    /// The namespaces every file of the project imports: the SDK's implicit ones (when
    /// enabled) and the `<Using>` items, minus the removed ones. Aliases and static usings are
    /// in [`CsProject::usings`].
    pub fn namespaces(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if self.implicit_usings {
            out.extend(implicit_usings(&self.sdk).iter().map(|s| s.to_string()));
        }
        for u in &self.usings {
            if !u.remove && u.alias.is_none() && !u.is_static && !out.contains(&u.name) {
                out.push(u.name.clone());
            }
        }
        out.retain(|n| !self.usings.iter().any(|u| u.remove && &u.name == n));
        out
    }
}

/// The SDK's implicit `global using` namespaces, from the table in the .NET SDK documentation
/// ("Implicit using directives", <https://learn.microsoft.com/dotnet/core/project-sdk/overview>).
/// The Razor, Blazor WebAssembly and Aspire SDKs build on `Microsoft.NET.Sdk` and add none.
pub fn implicit_usings(sdk: &str) -> Vec<&'static str> {
    const BASE: &[&str] = &[
        "System",
        "System.Collections.Generic",
        "System.IO",
        "System.Linq",
        "System.Net.Http",
        "System.Threading",
        "System.Threading.Tasks",
    ];
    const EXTENSIONS: &[&str] = &[
        "Microsoft.Extensions.Configuration",
        "Microsoft.Extensions.DependencyInjection",
        "Microsoft.Extensions.Hosting",
        "Microsoft.Extensions.Logging",
    ];
    let mut out: Vec<&'static str> = BASE.to_vec();
    match sdk {
        "Microsoft.NET.Sdk.Web" => {
            out.extend([
                "System.Net.Http.Json",
                "Microsoft.AspNetCore.Builder",
                "Microsoft.AspNetCore.Hosting",
                "Microsoft.AspNetCore.Http",
                "Microsoft.AspNetCore.Routing",
            ]);
            out.extend(EXTENSIONS);
        }
        "Microsoft.NET.Sdk.Worker" => out.extend(EXTENSIONS),
        _ => {}
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Resolver {
    /// tsconfig directory (root-relative, `""` for the root) → aliases.
    pub tsconfigs: BTreeMap<String, Aliases>,
    pub packages: BTreeMap<String, Package>,
    /// C# project directory → project.
    pub csprojs: BTreeMap<String, CsProject>,
    /// Files a C# project compiles from outside its directory → that project's directory.
    pub cs_links: BTreeMap<String, String>,
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

/// The files a resolver needs: every `package.json`, `tsconfig*.json` and `jsconfig.json`, and
/// every `.csproj` and `Directory.Build.props`.
pub fn wants(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "package.json"
        || is_config(path)
        || name.ends_with(".csproj")
        || name == "Directory.Build.props"
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
        r.load_csprojs(&texts);
        r
    }

    fn load_csprojs(&mut self, texts: &BTreeMap<&str, &str>) {
        for (path, text) in texts {
            if !path.ends_with(".csproj") {
                continue;
            }
            let dir = dir_of(path).to_string();
            // Two projects in one directory: the first in path order applies to its files.
            if self.csprojs.contains_key(&dir) {
                continue;
            }
            // `Directory.Build.props` files above the project are imported before it, nearest
            // last; the project's own settings override theirs.
            let mut props = Vec::new();
            let mut d = dir.clone();
            loop {
                let candidate = join(&d, "Directory.Build.props");
                if let Some(t) = texts.get(candidate.as_str()) {
                    props.push(*t);
                }
                if d.is_empty() {
                    break;
                }
                d = dir_of(&d).to_string();
            }
            let mut project = CsProject {
                path: path.to_string(),
                sdk: "Microsoft.NET.Sdk".into(),
                ..CsProject::default()
            };
            for t in props.iter().rev().chain(std::iter::once(text)) {
                let t = strip_xml_comments(t);
                if let Some(sdk) = project_sdk(&t) {
                    project.sdk = sdk;
                }
                if let Some(v) = xml_property(&t, "ImplicitUsings") {
                    project.implicit_usings = matches!(v.as_str(), "enable" | "true");
                }
                if let Some(v) = xml_property(&t, "RootNamespace") {
                    project.root_namespace = Some(v);
                }
                project.usings.extend(using_items(&t));
                for inc in compile_includes(&t) {
                    let target = join(&dir, &inc.replace('\\', "/"));
                    self.cs_links.entry(target).or_insert_with(|| dir.clone());
                }
            }
            self.csprojs.insert(dir, project);
        }
    }

    /// The C# project a file belongs to: the nearest `.csproj` above it, or the project that
    /// compiles it from elsewhere.
    pub fn csproj_for(&self, file: &str) -> Option<&CsProject> {
        let mut dir = dir_of(file);
        loop {
            if let Some(p) = self.csprojs.get(dir) {
                return Some(p);
            }
            if dir.is_empty() {
                break;
            }
            dir = dir_of(dir);
        }
        self.cs_links
            .get(file)
            .and_then(|d| self.csprojs.get(d.as_str()))
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

fn strip_xml_comments(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find("<!--") {
        out.push_str(&rest[..i]);
        match rest[i..].find("-->") {
            Some(j) => rest = &rest[i + j + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The value of an XML attribute in a tag's text: `Include="X"` → `X`.
fn xml_attr(tag: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r#"\b{name}\s*=\s*"([^"]*)""#)).ok()?;
    re.captures(tag).map(|c| c[1].trim().to_string())
}

/// `<Project Sdk="Microsoft.NET.Sdk.Web">`, `<Sdk Name="..."/>`, without a version: the first
/// SDK named, which is the one whose implicit usings apply.
fn project_sdk(t: &str) -> Option<String> {
    let re = regex::Regex::new(r"<(Project|Sdk)\b[^>]*>").ok()?;
    for c in re.captures_iter(t) {
        let tag = c.get(0)?.as_str();
        let attr = if &c[1] == "Project" { "Sdk" } else { "Name" };
        if let Some(v) = xml_attr(tag, attr) {
            let first = v.split(';').next().unwrap_or(&v);
            return Some(first.split('/').next().unwrap_or(first).trim().to_string());
        }
    }
    None
}

/// The last value of an MSBuild property: `<ImplicitUsings>enable</ImplicitUsings>`.
fn xml_property(t: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r"<{name}(?:\s[^>]*)?>([^<]*)</{name}>")).ok()?;
    re.captures_iter(t).last().map(|c| c[1].trim().to_string())
}

fn using_items(t: &str) -> Vec<UsingItem> {
    let Ok(re) = regex::Regex::new(r"<Using\b[^>]*>") else {
        return Vec::new();
    };
    re.find_iter(t)
        .filter_map(|m| {
            let tag = m.as_str();
            let (name, remove) = match (xml_attr(tag, "Include"), xml_attr(tag, "Remove")) {
                (Some(n), _) => (n, false),
                (None, Some(n)) => (n, true),
                _ => return None,
            };
            Some(UsingItem {
                name,
                alias: xml_attr(tag, "Alias"),
                is_static: xml_attr(tag, "Static").is_some_and(|v| v == "true"),
                remove,
            })
        })
        .collect()
}

fn compile_includes(t: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(r"<Compile\b[^>]*>") else {
        return Vec::new();
    };
    re.find_iter(t)
        .filter_map(|m| xml_attr(m.as_str(), "Include"))
        .filter(|p| !p.contains(['*', '$']))
        .collect()
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

    #[test]
    fn csharp_projects() {
        let r = Resolver::from_files(&files(&[
            (
                "Directory.Build.props",
                "<Project><PropertyGroup><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>",
            ),
            (
                "src/Api/Api.csproj",
                r#"<Project Sdk="Microsoft.NET.Sdk.Web/9.0.0">
                  <!-- <Using Include="Commented.Out" /> -->
                  <PropertyGroup><RootNamespace>Shop.Api</RootNamespace></PropertyGroup>
                  <ItemGroup>
                    <Using Include="MediatR" />
                    <Using Include="System.IO" Remove="System.IO" />
                    <Using Remove="System.Threading" />
                    <Using Include="Serilog.Log" Alias="Log" />
                    <Compile Include="..\Shared\Activity.cs" Link="Activity.cs" />
                  </ItemGroup>
                </Project>"#,
            ),
            (
                "src/Lib/Lib.csproj",
                r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><ImplicitUsings>disable</ImplicitUsings></PropertyGroup></Project>"#,
            ),
        ]));
        let api = r.csproj_for("src/Api/Endpoints/Orders.cs").unwrap();
        assert_eq!(api.sdk, "Microsoft.NET.Sdk.Web");
        assert_eq!(api.root_namespace.as_deref(), Some("Shop.Api"));
        let ns = api.namespaces();
        assert!(ns.contains(&"Microsoft.Extensions.Logging".to_string()));
        assert!(ns.contains(&"MediatR".to_string()));
        assert!(!ns.contains(&"System.Threading".to_string()));
        assert!(!ns.contains(&"Commented.Out".to_string()));
        assert!(!ns.contains(&"Serilog.Log".to_string()));
        assert!(
            r.csproj_for("src/Lib/X.cs")
                .unwrap()
                .namespaces()
                .is_empty()
        );
        assert_eq!(
            r.csproj_for("src/Shared/Activity.cs")
                .map(|p| p.path.as_str()),
            Some("src/Api/Api.csproj")
        );
        assert!(r.csproj_for("tools/x.cs").is_none());
    }
}
