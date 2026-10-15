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
    (
        "dispatchers.yml",
        include_str!("../catalogue/dispatchers.yml"),
    ),
    (
        "presentation.yml",
        include_str!("../catalogue/presentation.yml"),
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
    /// Storage on the user's device by a native app (.NET MAUI `SecureStorage`, `Preferences`).
    DeviceStorage,
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
            SinkClass::DeviceStorage => "device_storage",
            SinkClass::ThirdParty => "third_party",
        }
    }

    /// Data leaves the system's own boundary: a vendor or another host.
    pub fn is_external(self) -> bool {
        !matches!(
            self,
            SinkClass::Log | SinkClass::BrowserStorage | SinkClass::DeviceStorage
        )
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
    /// When the URL argument was built by a call (`new HttpRequestMessage(method, url)`), the
    /// URL is that call's positional argument at this index.
    #[serde(default)]
    pub built_arg: Option<usize>,
    /// Calls the URL passes through unchanged (`new Uri(url)`): their first argument is the URL.
    #[serde(default)]
    pub wrappers: Vec<String>,
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
    /// Calls to the project's own functions carrying one of these attributes (decorator API
    /// paths): a method whose body a source generator writes (`[LoggerMessage]`), an interface
    /// method a library implements (Refit's `[Post]`).
    #[serde(default)]
    pub decorated: Vec<String>,
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
    /// Globs over the annotation's type name, and for C# over the declared type's API path.
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
    /// Parameters whose declared type is a project class extending one of these (API path
    /// globs) are not request input (a `DbContext` subclass).
    #[serde(default)]
    pub skip_bases: Vec<String>,
    /// Parameters typed with one of the project's interfaces are injected services.
    #[serde(default)]
    pub skip_interfaces: bool,
    /// Parameters with one of these attributes (API path globs) are not request input.
    #[serde(default)]
    pub skip_attrs: Vec<String>,
    /// Parameters with one of these attributes are a bag of request inputs: each property of
    /// their declared type is one (`[AsParameters]`).
    #[serde(default)]
    pub expand_attrs: Vec<String>,
    /// Parameters declared with these types (`Guid`, `Int32`) are request input only when their
    /// name classifies them: `whole` does not apply.
    #[serde(default)]
    pub classify_only_types: Vec<String>,
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
    /// Per-attribute overrides (`[FromBody]`), keyed by an API path glob of the attribute; they
    /// win over `by_type` and the skip rules.
    #[serde(default)]
    pub by_attr: BTreeMap<String, ParamRule>,
}

/// Which methods of a matched class are handlers.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberFilter {
    /// Modifiers a method must have (`public`, `override`).
    #[serde(default)]
    pub require: Vec<String>,
    /// Modifiers (or `constructor`) that exclude a method.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Attributes (API path globs) that exclude a method (`[NonAction]`).
    #[serde(default)]
    pub exclude_decorators: Vec<String>,
}

/// A `read` call whose category its literal argument decides (`FindFirst("email")`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgCategory {
    /// The positional argument holding the name.
    pub index: usize,
    /// Names with their categories, compared after the last `/` or `.` (claim type URIs and
    /// `ClaimTypes.Email` alike).
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    /// Classify other names with the classification table.
    #[serde(default)]
    pub classify: bool,
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
    /// Properties with one of `decorators` (attributes) that the framework sets from request
    /// input (Blazor's `[SupplyParameterFromForm]`, MVC's `[BindProperty]`).
    Property,
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
    /// Handlers: methods of the project's classes that extend one of these (API path globs;
    /// MVC controllers).
    #[serde(default)]
    pub bases: Vec<String>,
    /// Handlers: methods of the project's classes with one of these attributes.
    #[serde(default)]
    pub class_decorators: Vec<String>,
    /// Which methods of a class matched by `bases` or `class_decorators` (or of any class, with
    /// `has_param_type`) are handlers.
    #[serde(default)]
    pub members: Option<MemberFilter>,
    /// Handlers: methods with a parameter declared with one of these types (API path globs),
    /// such as a gRPC service's overrides, which take a `ServerCallContext`.
    #[serde(default)]
    pub has_param_type: Vec<String>,
    /// `read` calls: the category is decided by a literal argument.
    #[serde(default)]
    pub arg_category: Option<ArgCategory>,
    #[serde(default)]
    pub citation: Option<String>,
}

/// A call that hands its argument to the project's own handlers through a library: MediatR's
/// `Send` reaches every `IRequestHandler<TRequest, _>.Handle` for the request's type, and every
/// pipeline behavior's.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatcherDef {
    pub id: String,
    pub language: String,
    #[serde(default)]
    pub description: Option<String>,
    /// API paths of the dispatching calls.
    pub calls: Vec<String>,
    /// The positional argument dispatched.
    #[serde(default)]
    pub arg: usize,
    pub targets: Vec<DispatchTarget>,
    #[serde(default)]
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchTarget {
    /// Classes implementing (or extending) a type matching this API path glob.
    pub implements: String,
    /// Their methods with this name receive the argument.
    pub method: String,
    /// As this parameter.
    #[serde(default)]
    pub param: usize,
    /// Only when the argument's type is (or extends) the interface's type argument at this
    /// index; an open type parameter accepts any.
    #[serde(default)]
    pub type_arg: Option<usize>,
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
    /// Arguments flow to the result, the receiver does not: a call on a service client returns
    /// the service's data, not the client's configuration (`stripe.subscriptions.retrieve(id)`
    /// does not carry the key the client was built with).
    ArgsOnly,
    /// Arguments flow into the receiver (`xs.push(x)`), and to the result.
    ArgsToReceiver,
    /// Like `args_to_receiver`, and the result is the receiver itself, so a chained call adds
    /// to it too (`sb.Append(a).Append(b)`).
    Fluent,
    /// The receiver's elements flow into the first parameter of a callback argument, and the
    /// callback's return value flows to the result (`xs.map(x => ...)`).
    ReceiverToCallback,
    /// The return value of each callback argument flows to the result, and the other arguments
    /// do too (`Task.Run(() => ...)`, a tool function handed to a model).
    CallbackResult,
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
    /// Base types and attributes (API path globs) whose use marks a class as the framework's,
    /// where imports alone do not show it (C# global and implicit usings).
    #[serde(default)]
    pub types: Vec<String>,
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
    dispatchers: Vec<DispatcherDef>,
    #[serde(default)]
    non_propagating_fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    value_methods: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    accessor_fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    metadata_types: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    contextual: Vec<ContextualDef>,
    #[serde(default)]
    entity_identifiers: Vec<EntityIdentifiersDef>,
    #[serde(default)]
    presentation: Vec<PresentationDef>,
}

/// Calls that only present data in the app's user interface (`catalogue/presentation.yml`): a
/// coverage gap at one is low impact.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationDef {
    pub id: String,
    pub language: String,
    pub calls: Vec<String>,
    pub why: String,
    pub citation: String,
}

/// A name the classification table classifies whose category holds only in context
/// (`catalogue/classification.yml`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextualDef {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// With a category, the entry adds its names (the table does not have them), in context
    /// when it gives one, everywhere when it does not.
    #[serde(default)]
    pub category: Option<String>,
    pub names: Vec<String>,
    /// Globs over the table key of the object the name is read from.
    #[serde(default)]
    pub objects: Vec<String>,
    /// Names whose presence beside it (fields of the same type, parameters of the same
    /// function) is context enough.
    #[serde(default)]
    pub siblings: Vec<String>,
}

/// Identifiers of things, not people (`catalogue/classification.yml`): a name made of entity
/// words and an identifier suffix (`item_id`, `catalog_item_id`, `BrandIds`) is not personal data.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityIdentifiersDef {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Words naming a thing (`item`, `brand`) or a compound of them (`order_line`).
    pub entities: Vec<String>,
    /// The last word of an identifier's name (`id`, `ids`).
    pub suffixes: Vec<String>,
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
    pub decorated: Vec<Glob>,
}

pub struct Source {
    pub def: SourceDef,
    pub origin: Origin,
    pub registrations: Vec<Glob>,
    pub decorators: Vec<Glob>,
    pub files: Vec<Glob>,
    pub reads: Vec<Glob>,
    pub calls: Vec<Glob>,
    pub bases: Vec<Glob>,
    pub class_decorators: Vec<Glob>,
    pub has_param_type: Vec<Glob>,
}

pub struct Dispatcher {
    pub def: DispatcherDef,
    pub origin: Origin,
    pub calls: Vec<Glob>,
    /// One per target: the `implements` glob.
    pub implements: Vec<Glob>,
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
    pub dispatchers: Vec<Dispatcher>,
    pub non_propagating_fields: BTreeMap<String, BTreeSet<String>>,
    /// Methods of plain values (strings), per language: calling one does not make the receiver
    /// a record.
    pub value_methods: BTreeMap<String, BTreeSet<String>>,
    /// Fields, per language, that read what an object holds rather than an attribute of it
    /// (`claim.Value`): a read of one is not classified by its name.
    pub accessor_fields: BTreeMap<String, BTreeSet<String>>,
    /// Types (API path globs), per language, whose values describe code rather than data
    /// (`System.Type`): their fields are not personal data.
    pub metadata_types: BTreeMap<String, Vec<String>>,
    pub contextual: Vec<ContextualDef>,
    pub entity_identifiers: Vec<EntityIdentifiersDef>,
    pub presentation: Vec<PresentationDef>,
}

/// Entries a project adds in `.privacy-flow.yml`.
#[derive(Clone, Debug, Default)]
pub struct Extensions {
    pub sinks: Vec<SinkDef>,
    pub sources: Vec<SourceDef>,
    pub sanitisers: Vec<SanitiserDef>,
    pub propagators: Vec<PropagatorDef>,
    pub dispatchers: Vec<DispatcherDef>,
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

const LANGUAGES: &[&str] = &["javascript", "python", "csharp"];

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
            merged.dispatchers.extend(file.dispatchers);
            merged.contextual.extend(file.contextual);
            merged.entity_identifiers.extend(file.entity_identifiers);
            merged.presentation.extend(file.presentation);
            for (lang, fields) in file.non_propagating_fields {
                merged
                    .non_propagating_fields
                    .entry(lang)
                    .or_default()
                    .extend(fields);
            }
            for (lang, methods) in file.value_methods {
                merged
                    .value_methods
                    .entry(lang)
                    .or_default()
                    .extend(methods);
            }
            for (lang, types) in file.metadata_types {
                merged.metadata_types.entry(lang).or_default().extend(types);
            }
            for (lang, fields) in file.accessor_fields {
                merged
                    .accessor_fields
                    .entry(lang)
                    .or_default()
                    .extend(fields);
            }
        }
        // `scan` binds requests to client registrations and names systems with these (ADR 0022),
        // so its output depends on them too.
        for (name, text) in crate::topology::catalogue::FILES
            .iter()
            .filter(|(n, _)| ["clients.yml", "systems.yml"].contains(n))
        {
            hasher_input.push_str("topology/");
            hasher_input.push_str(name);
            hasher_input.push('\n');
            hasher_input.push_str(text);
        }
        let mut ids = BTreeSet::new();
        let mut check_id = |id: &str, lang: &str| -> Result<()> {
            if !ids.insert(id.to_string()) {
                bail!("duplicate catalogue id {id:?}");
            }
            if !LANGUAGES.contains(&lang) {
                bail!(
                    "catalogue entry {id:?}: unknown language {lang:?} (expected javascript, python or csharp)"
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
            let decorated = compile(&def.decorated, "decorated", &def.id)?;
            sinks.push(Sink {
                def,
                origin,
                calls,
                assigns,
                decorated,
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
                bases: compile(&def.bases, "base", &def.id)?,
                class_decorators: compile(&def.class_decorators, "class decorator", &def.id)?,
                has_param_type: compile(&def.has_param_type, "parameter type", &def.id)?,
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
        let mut dispatchers = Vec::new();
        for (def, origin) in merged
            .dispatchers
            .into_iter()
            .map(|d| (d, Origin::Default))
            .chain(ext.dispatchers.iter().cloned().map(|d| (d, Origin::Config)))
        {
            check_id(&def.id, &def.language)?;
            let implements = def
                .targets
                .iter()
                .map(|t| {
                    Glob::api(&t.implements)
                        .with_context(|| format!("implements pattern in {}", def.id))
                })
                .collect::<Result<Vec<_>>>()?;
            dispatchers.push(Dispatcher {
                calls: compile(&def.calls, "call", &def.id)?,
                implements,
                def,
                origin,
            });
        }
        for def in &merged.presentation {
            check_id(&def.id, &def.language)?;
            compile(&def.calls, "call", &def.id)?;
            if def.citation.trim().len() < 3 {
                bail!("catalogue entry {:?} needs a citation", def.id);
            }
        }
        let non_propagating_fields = merged
            .non_propagating_fields
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        let value_methods = merged
            .value_methods
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        let accessor_fields = merged
            .accessor_fields
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
            dispatchers,
            non_propagating_fields,
            value_methods,
            accessor_fields,
            metadata_types: merged.metadata_types,
            contextual: merged.contextual,
            entity_identifiers: merged.entity_identifiers,
            presentation: merged.presentation,
        })
    }

    /// Every API pattern the catalogue holds for a language, with each framework module as a
    /// namespace pattern: what a language that resolves names against the catalogue (C#) may
    /// qualify a name with.
    pub fn api_patterns(&self, lang: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in self.sinks.iter().filter(|s| s.def.language == lang) {
            out.extend(
                s.def
                    .calls
                    .iter()
                    .chain(&s.def.assigns)
                    .chain(&s.def.decorated)
                    .cloned(),
            );
            if let Some(h) = &s.def.host {
                out.extend(h.wrappers.iter().cloned());
            }
        }
        for s in self.sources.iter().filter(|s| s.def.language == lang) {
            out.extend(
                s.def
                    .registrations
                    .iter()
                    .chain(&s.def.decorators)
                    .chain(&s.def.reads)
                    .chain(&s.def.calls)
                    .chain(&s.def.bases)
                    .chain(&s.def.class_decorators)
                    .chain(&s.def.has_param_type)
                    .cloned(),
            );
            if let Some(m) = &s.def.members {
                out.extend(m.exclude_decorators.iter().cloned());
            }
            for spec in &s.def.params {
                for rule in std::iter::once(&spec.rule)
                    .chain(spec.by_type.values())
                    .chain(spec.by_attr.values())
                {
                    out.extend(
                        rule.skip_types
                            .iter()
                            .chain(&rule.skip_bases)
                            .chain(&rule.skip_attrs)
                            .chain(&rule.expand_attrs)
                            .cloned(),
                    );
                }
                out.extend(spec.by_attr.keys().cloned());
            }
        }
        for s in self.sanitisers.iter().filter(|s| s.def.language == lang) {
            out.extend(s.def.calls.iter().cloned());
        }
        for p in self.propagators.iter().filter(|p| p.def.language == lang) {
            out.extend(p.def.calls.iter().cloned());
        }
        for d in self.dispatchers.iter().filter(|d| d.def.language == lang) {
            out.extend(d.def.calls.iter().cloned());
            out.extend(d.def.targets.iter().map(|t| t.implements.clone()));
        }
        for f in self.frameworks.iter().filter(|f| f.language == lang) {
            out.extend(f.modules.iter().map(|m| format!("{m}.**")));
            out.extend(f.types.iter().cloned());
        }
        out
    }

    pub fn accessor_field(&self, lang: &str, field: &str) -> bool {
        self.accessor_fields
            .get(lang)
            .is_some_and(|s| s.contains(field))
    }

    pub fn value_method(&self, lang: &str, method: &str) -> bool {
        self.value_methods
            .get(lang)
            .is_some_and(|s| s.contains(method))
    }

    pub fn non_propagating(&self, lang: &str, field: &str) -> bool {
        self.non_propagating_fields
            .get(lang)
            .is_some_and(|s| s.contains(field))
    }
}
