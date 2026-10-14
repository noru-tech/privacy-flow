# ADR 0016: C#: a lowering that resolves names against the whole project

Status: accepted, 2026-10-14.

## Context

C# was reported as an unsupported language. It is the third language, and the first whose
names a single file cannot resolve:

- **Namespaces, not modules.** A type is visible to every file of its namespace and to every file
  with a `using` of it. `using` directives may be global (`global using`, in any file of the
  project), come from the project file (`<Using>`), or be implicit (the SDK adds `System`,
  `Microsoft.Extensions.Logging` and others when `ImplicitUsings` is on).
- **Implicit `this`.** A bare `_logger` or `Orders` is a member of the enclosing class, of a
  base class declared in another file, or of another part of a `partial` class; or it is a type.
  Which one decides whether the IR reads a field or imports a type, so it has to be known while
  lowering.
- **No assembly metadata.** The types of NuGet packages and the base class library are not in
  the scan. A type outside the project is known only by name.

ADR 0001 promised that a language is one lowering. The JavaScript and Python lowerings see one
file; this one cannot.

## Decision

**Two passes.** Every C# file (and every Razor component's generated C#) is first scanned, in
parallel, for its declarations: namespace scopes, `using` directives, types with their kinds,
arities, base lists and member names. The scans are merged in path order into a `CsIndex`, with
the project data from `.csproj` and `Directory.Build.props` (`src/resolve.rs`): the SDK and
`ImplicitUsings`, `<Using>` items, `<Compile Include>` links. Then each file is lowered, in
parallel, against the index. The lowering resolves every name; the program resolves only file
paths, so `ImportKind`, `Target` and `FileIr` are unchanged.

**Resolution** follows the compiler's order: nested types of the enclosing types; each enclosing
namespace from the innermost out, with its types, its sub-namespaces, and the aliases and
`using`s declared at that level; at the compilation unit, the file's usings, the project's global
and `<Using>` usings, and the SDK's implicit ones. A member of the enclosing types (including
those inherited from source base classes) is read through the instance (`ThisLoad`).

**External types are qualified by the catalogue.** For a name no project type explains, each
`using` namespace `U` gives a candidate `U.Name`:

1. a name the catalogue spells (a literal prefix of one of its C# patterns, such as
   `Microsoft.Extensions.Logging.ILogger`) is definite; several spelled candidates are all kept,
   since the compiler rejects a real ambiguity and the catalogue matches the right one;
2. otherwise a namespace the catalogue covers with a wildcard (`Microsoft.EntityFrameworkCore.**`)
   gives a guess, the first in using order, and only when every `using` in scope is a namespace
   the project, the base class library (`System.*`) or the catalogue knows: a `using Acme.Mail;`
   the catalogue knows nothing of may be where the name is, and a guess would hide the call;
3. a guess at an inner namespace level waits for the outer levels, which may spell the name;
4. otherwise the name stays bare (`Mailer.Deliver`), and a call on it is a coverage gap.

API paths are dotted and namespace-qualified with generic arguments stripped
(`Microsoft.Extensions.Logging.ILogger.LogInformation`); a constructed value is `T()`; an
extension method is a member of the type it extends.

**Lowering.** Types are hoisted and exported under their qualified names. Primary constructors and
records store their parameters into fields; field and property initializers run in the
constructor; auto-properties are fields and getters with bodies are property functions;
constructor overloads each have their own instance (`more_ctor_this`). Attributes are
constructions of the attribute type, kept on the method, the class, the parameter
(`Param.attrs`) or the member (`field_attrs`). Declared types become type references
(`Task<T>`, `T?` and `ActionResult<T>` are `T`; collections name nothing for provenance and their
element for typed seeds; primitives nothing). `await`, casts, `!` and `.ConfigureAwait()` are the
value; `typeof` and `GetType()` are `System.Type`; `out` and `ref` arguments receive the call's
result. A method call on a field writes the receiver back to the field, so `Messages.Add(m)` is
seen by a later read of `Messages`. LINQ query syntax lowers to elements and stores. Both branches
of `#if` are lowered.

**The program** merges the parts of a `partial` class into one, dispatches extension methods
(the most specific overload by the receiver's type; called through their class they bind
normally), considers every overload of a name, and treats classes declared in source as closed
(`LangTraits::closed_local_classes`): a method none of them defines (`ToString`) is a method of a
plain value, not a dynamic call. A typed parameter's declared type is its provenance; a function
handed to a library does not also get the callback path.

**Language branches** outside the lowerings are a table, `LangTraits` in `src/ir.rs`, instead of
`== Lang::Python` tests: dotted paths, the module separator, the global prefix, default exports,
packages, run-time globals, closed classes, authoritative declared types.

**Razor** (`.razor`) has no maintained tree-sitter grammar. `src/lower/razor.rs` translates a
component into the C# the Razor compiler would make of it, as far as data goes, with a byte map
back: directives become usings, attributes and injected properties; `@code` is copied; markup
becomes a render method in which `@expr` is evaluated, control flow keeps its C#, and component
tags are object initializers of the child component; `@bind` assigns the bound value from
`EventCallbackFactoryBinderExtensions.CreateBinder()`, which is what the compiler generates and
what the catalogue names as a source. Positions and citations are the `.razor` file's. A
component's namespace is the project's root namespace and its folders, and its usings include
every `_Imports.razor` above it.

## Alternatives

- **Late binding in `program.rs`.** Lower names as lookups and decide member or type once the
  class lineage is known. Provenance only grows, so a name read as a type in an early round
  could not become a member later; and the lineage that decides it is itself provenance.
- **A namespace-to-files `Target`.** Fits imports, not implicit `this`, which is most of the
  problem.
- **Roslyn.** Exact, but a .NET runtime and a restore of every package at scan time; the
  analysis would no longer be a single static binary with no network access.
- **Qualify external names by every `using`.** Every candidate in every implicit namespace:
  path sets overflowed the per-variable cap on eShop and wrong candidates matched propagators.

## Measurements

dotnet/eShop at `dc7ea499`: 516 C# and Razor files, 20,605 lines, after the default exclusions (tests, migrations, build output), on an Apple M3 Pro:

| | |
| --- | --- |
| Time | 0.6 s with one thread, 0.4 s with eight |
| Findings | PF001 134, PF003 12, PF006 41, PFC01 27 |
| Coverage gaps | 27: two files the grammar cannot parse (`#if` inside an expression and a base list), the `.cshtml` views, and MAUI client APIs |

The 100k-line synthetic C# service (`benches/synthetic.py --lang cs`) scans in 2.0 s and 644
MiB, against 1.2 s and 488 MiB for TypeScript; the lowering parses each file twice.

The findings `tests/smoke/eshop.expected.json` requires include the order request logged whole
four lines below the comment saying it must not be (`OrdersApi.cs:134`), the MediatR pipeline
behaviors that log every command with its card number (ADR 0017), the Duende events with user
names, the chatbot input reaching the model, and the checkout form reaching the ordering API.

## Consequences

- TypeScript and Python output is unchanged; the catalogue digest changes.
- Fixtures `cs/*`, conformance vectors `cs-*`, the eShop smoke test
  (`.github/scripts/smoke_eshop.py`), and fuzz inputs for C#, Razor and project files.
- **Not covered** (KNOWN-LIMITATIONS.md): types only assembly metadata would explain (a
  wildcard guess may name the wrong namespace; return types of external calls are unknown);
  source-generated code other than `[LoggerMessage]` and gRPC messages; `.cshtml` views and Razor
  Pages handlers; getters followed for data; flow-insensitivity, as everywhere.
