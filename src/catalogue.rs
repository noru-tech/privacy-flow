//! The rule catalogue: sources, sinks, sanitisers, propagators and frameworks, as data.
//!
//! The default catalogue is the YAML under `catalogue/`, compiled into the binary. A project's
//! `.privacy-flow.yml` adds entries with the same shape (each with a citation). Adding an SDK
//! is a change to these files, not to Rust: the analysis only ever asks the catalogue what an
//! API path, method name or local function is.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::glob::Glob;

pub const FILES: &[(&str, &str)] = &[
    (
        "classification.yml",
        include_str!("../catalogue/classification.yml"),
    ),
    (
        "frameworks.yml",
        include_str!("../catalogue/frameworks.yml"),
    ),
    (
        "propagators.yml",
        include_str!("../catalogue/propagators.yml"),
    ),
    (
        "sanitisers.yml",
        include_str!("../catalogue/sanitisers.yml"),
    ),
    ("sinks.yml", include_str!("../catalogue/sinks.yml")),
    ("sources.yml", include_str!("../catalogue/sources.yml")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SinkClass {
    Log,
    ErrorTracking,
    Analytics,
    Messaging,
    Llm,
    Http,
    BrowserStorage,
    /// Other third-party SDKs: payments, CRM and support, cloud storage, search, queues.
    ThirdParty,
}

impl SinkClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SinkClass::Log => "log",
            SinkClass::ErrorTracking => "error_tracking",
            SinkClass::Analytics => "analytics",
            SinkClass::Messaging => "messaging",
            SinkClass::Llm => "llm",
            SinkClass::Http => "http",
            SinkClass::BrowserStorage => "browser_storage",
            SinkClass::ThirdParty => "third_party",
        }
    }

    /// Data leaves the system's own boundary: a vendor or another host.
    pub fn is_external(self) -> bool {
        !matches!(self, SinkClass::Log | SinkClass::BrowserStorage)
    }
}

/// Which arguments of a sink call carry data out: `all` (the default) or positional indices.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ArgSel {
    All(AllArgs),
    Indices(Vec<usize>),
}

impl Default for ArgSel {
    fn default() -> Self {
        ArgSel::All(AllArgs::All)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AllArgs {
    All,
}

impl ArgSel {
    pub fn is_all(&self) -> bool {
        matches!(self, ArgSel::All(_))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSpec {
    /// Positional argument holding the URL.
    #[serde(default)]
    pub arg: Option<usize>,
    /// Keyword argument holding the URL (Python).
    #[serde(default)]
    pub keyword: Option<String>,
    /// When the argument is an object literal, the field holding the URL.
    #[serde(default)]
    pub field: Option<String>,
    /// A relative URL resolves against a client's configured base URL, not the application's
    /// own origin.
    #[serde(default)]
    pub base_url: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkDef {
    pub id: String,
    pub language: String,
    pub class: SinkClass,
    #[serde(default)]
    pub processor: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// API paths of calls that are this sink.
    #[serde(default)]
    pub calls: Vec<String>,
    /// API paths of property writes that are this sink (`document.cookie`).
    #[serde(default)]
    pub assigns: Vec<String>,
    /// Heuristic: method calls on a receiver with one of these names whose provenance is
    /// unknown (`logger.info(x)` where `logger` was injected).
    #[serde(default)]
    pub receivers: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub args: ArgSel,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// The receiver carries data out too: `send()` on a message object built from the data.
    #[serde(default)]
    pub receiver: bool,
    #[serde(default)]
    pub host: Option<HostSpec>,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamRule {
    #[serde(default)]
    pub whole: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub methods: BTreeMap<String, String>,
    /// A parameter annotated with a type the analysis knows is narrowed to that type's
    /// classified fields instead of `whole`.
    #[serde(default)]
    pub typed: bool,
    /// Parameters annotated with these types are not request input (injected dependencies).
    #[serde(default)]
    pub skip_types: Vec<String>,
    /// Parameters with these names are not request input.
    #[serde(default)]
    pub skip_names: Vec<String>,
    /// Classify a parameter by its own name before falling back to `whole`.
    #[serde(default)]
    pub by_name: bool,
    /// Give the parameter this API-path provenance (the response object of a handler).
    #[serde(default)]
    pub provenance: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamSpec {
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(flatten)]
    pub rule: ParamRule,
    /// Per-annotation overrides (`Request` in FastAPI).
    #[serde(default)]
    pub by_type: BTreeMap<String, ParamRule>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Functions registered as request handlers: passed to a registration call, decorated,
    /// or defined in matching files.
    Handler,
    /// Exported functions with given names in matching files (Next.js route handlers).
    RouteExport,
    /// Reads and calls of API paths that return request input (Flask's `request`).
    Read,
    /// Field names that are personal data (extends the classification table).
    Field,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDef {
    pub id: String,
    pub language: String,
    pub kind: SourceKind,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub registrations: Vec<String>,
    #[serde(default)]
    pub decorators: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub calls: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SanitiserDef {
    pub id: String,
    pub language: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub calls: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    /// Local functions: `path/to/file.ts:functionName` (globs allowed in both halves).
    #[serde(default)]
    pub functions: Vec<String>,
    /// Category prefixes removed; `*` removes everything, including `unknown`.
    pub removes: Vec<String>,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    /// Arguments (and the receiver, for methods) flow to the result.
    ArgsToResult,
    /// Arguments flow into the receiver (`xs.push(x)`), and to the result.
    ArgsToReceiver,
    /// The receiver's elements flow into the first parameter of a callback argument, and the
    /// callback's return value flows to the result (`xs.map(x => ...)`).
    ReceiverToCallback,
    /// Nothing flows.
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropagatorDef {
    pub id: String,
    pub language: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub calls: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    /// Any method call on a receiver with one of these names whose origin the analysis cannot
    /// see (an unannotated `db_session` parameter).
    #[serde(default)]
    pub receivers: Vec<String>,
    pub flow: Flow,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameworkDef {
    pub name: String,
    pub language: String,
    pub modules: Vec<String>,
    pub supported: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogueFile {
    version: String,
    #[serde(default)]
    sinks: Vec<SinkDef>,
    #[serde(default)]
    sources: Vec<SourceDef>,
    #[serde(default)]
    sanitisers: Vec<SanitiserDef>,
    #[serde(default)]
    propagators: Vec<PropagatorDef>,
    #[serde(default)]
    frameworks: Vec<FrameworkDef>,
    #[serde(default)]
    non_propagating_fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    contextual: Vec<ContextualDef>,
}

/// A name the classification table classifies whose category holds only in context
/// (`catalogue/classification.yml`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextualDef {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    pub names: Vec<String>,
    /// Globs over the table key of the object the name is read from.
    #[serde(default)]
    pub objects: Vec<String>,
    /// Names whose presence beside it (fields of the same type, parameters of the same
    /// function) is context enough.
    #[serde(default)]
    pub siblings: Vec<String>,
}

/// Where a catalogue entry came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Default,
    Config,
}

pub struct Sink {
    pub def: SinkDef,
    pub origin: Origin,
    pub calls: Vec<Glob>,
    pub assigns: Vec<Glob>,
}

pub struct Source {
    pub def: SourceDef,
    pub origin: Origin,
    pub registrations: Vec<Glob>,
    pub decorators: Vec<Glob>,
    pub files: Vec<Glob>,
    pub reads: Vec<Glob>,
    pub calls: Vec<Glob>,
}

pub struct Sanitiser {
    pub def: SanitiserDef,
    pub origin: Origin,
    pub calls: Vec<Glob>,
    /// (file glob, function glob)
    pub functions: Vec<(Glob, Glob)>,
}

pub struct Propagator {
    pub def: PropagatorDef,
    pub origin: Origin,
    pub calls: Vec<Glob>,
}

pub struct Catalogue {
    pub version: String,
    pub digest: String,
    pub sinks: Vec<Sink>,
    pub sources: Vec<Source>,
    pub sanitisers: Vec<Sanitiser>,
    pub propagators: Vec<Propagator>,
    pub frameworks: Vec<FrameworkDef>,
    pub non_propagating_fields: BTreeMap<String, BTreeSet<String>>,
    pub contextual: Vec<ContextualDef>,
}

/// Entries a project adds in `.privacy-flow.yml`.
#[derive(Clone, Debug, Default)]
pub struct Extensions {
    pub sinks: Vec<SinkDef>,
    pub sources: Vec<SourceDef>,
    pub sanitisers: Vec<SanitiserDef>,
    pub propagators: Vec<PropagatorDef>,
}

fn compile(patterns: &[String], what: &str, id: &str) -> Result<Vec<Glob>> {
    patterns
        .iter()
        .map(|p| Glob::api(p).with_context(|| format!("{what} pattern {p:?} in {id}")))
        .collect()
}

fn compile_paths(patterns: &[String], id: &str) -> Result<Vec<Glob>> {
    patterns
        .iter()
        .map(|p| Glob::path(p).with_context(|| format!("file pattern {p:?} in {id}")))
        .collect()
}

const LANGUAGES: &[&str] = &["javascript", "python"];

impl Catalogue {
    /// The built-in catalogue only.
    pub fn builtin() -> Result<Catalogue> {
        Catalogue::load(&Extensions::default())
    }

    pub fn load(ext: &Extensions) -> Result<Catalogue> {
        let mut version: Option<String> = None;
        let mut merged = CatalogueFile::default();
        let mut hasher_input = String::new();
        for (name, text) in FILES {
            let file: CatalogueFile =
                serde_saphyr::from_str(text).with_context(|| format!("catalogue/{name}"))?;
            match &version {
                None => version = Some(file.version.clone()),
                Some(v) if *v != file.version => bail!(
                    "catalogue/{name} has version {} but others have {v}",
                    file.version
                ),
                _ => {}
            }
            hasher_input.push_str(name);
            hasher_input.push('\n');
            hasher_input.push_str(text);
            merged.sinks.extend(file.sinks);
            merged.sources.extend(file.sources);
            merged.sanitisers.extend(file.sanitisers);
            merged.propagators.extend(file.propagators);
            merged.frameworks.extend(file.frameworks);
            merged.contextual.extend(file.contextual);
            for (lang, fields) in file.non_propagating_fields {
                merged
                    .non_propagating_fields
                    .entry(lang)
                    .or_default()
                    .extend(fields);
            }
        }
        let mut ids = BTreeSet::new();
        let mut check_id = |id: &str, lang: &str| -> Result<()> {
            if !ids.insert(id.to_string()) {
                bail!("duplicate catalogue id {id:?}");
            }
            if !LANGUAGES.contains(&lang) {
                bail!(
                    "catalogue entry {id:?}: unknown language {lang:?} (expected javascript or python)"
                );
            }
            Ok(())
        };
        let mut sinks = Vec::new();
        for (def, origin) in merged
            .sinks
            .into_iter()
            .map(|d| (d, Origin::Default))
            .chain(ext.sinks.iter().cloned().map(|d| (d, Origin::Config)))
        {
            check_id(&def.id, &def.language)?;
            let calls = compile(&def.calls, "call", &def.id)?;
            let assigns = compile(&def.assigns, "assign", &def.id)?;
            sinks.push(Sink {
                def,
                origin,
                calls,
                assigns,
            });
        }
        let mut sources = Vec::new();
        for (def, origin) in merged
            .sources
            .into_iter()
            .map(|d| (d, Origin::Default))
            .chain(ext.sources.iter().cloned().map(|d| (d, Origin::Config)))
        {
            check_id(&def.id, &def.language)?;
            sources.push(Source {
                registrations: compile(&def.registrations, "registration", &def.id)?,
                decorators: compile(&def.decorators, "decorator", &def.id)?,
                files: compile_paths(&def.files, &def.id)?,
                reads: compile(&def.reads, "read", &def.id)?,
                calls: compile(&def.calls, "call", &def.id)?,
                def,
                origin,
            });
        }
        let mut sanitisers = Vec::new();
        for (def, origin) in merged
            .sanitisers
            .into_iter()
            .map(|d| (d, Origin::Default))
            .chain(ext.sanitisers.iter().cloned().map(|d| (d, Origin::Config)))
        {
            check_id(&def.id, &def.language)?;
            let mut functions = Vec::new();
            for f in &def.functions {
                let Some((file, func)) = f.rsplit_once(':') else {
                    bail!(
                        "sanitiser {}: function {f:?} must be `path/to/file:functionName`",
                        def.id
                    );
                };
                functions.push((Glob::path(file)?, Glob::api(func)?));
            }
            sanitisers.push(Sanitiser {
                calls: compile(&def.calls, "call", &def.id)?,
                functions,
                def,
                origin,
            });
        }
        let mut propagators = Vec::new();
        for (def, origin) in merged
            .propagators
            .into_iter()
            .map(|d| (d, Origin::Default))
            .chain(ext.propagators.iter().cloned().map(|d| (d, Origin::Config)))
        {
            check_id(&def.id, &def.language)?;
            propagators.push(Propagator {
                calls: compile(&def.calls, "call", &def.id)?,
                def,
                origin,
            });
        }
        let non_propagating_fields = merged
            .non_propagating_fields
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        Ok(Catalogue {
            version: version.unwrap_or_default(),
            digest: crate::canonical::sha256(hasher_input.as_bytes()),
            sinks,
            sources,
            sanitisers,
            propagators,
            frameworks: merged.frameworks,
            non_propagating_fields,
            contextual: merged.contextual,
        })
    }

    pub fn non_propagating(&self, lang: &str, field: &str) -> bool {
        self.non_propagating_fields
            .get(lang)
            .is_some_and(|s| s.contains(field))
    }
}
