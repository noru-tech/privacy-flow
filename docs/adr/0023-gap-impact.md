# ADR 0023: Coverage gaps are ranked by impact; device storage is a sink

Status: accepted, 2026-10-15. Extends ADR 0004.

## Context

ADR 0004 makes every blind spot a gap and every gap a PFC01 finding, all at one severity. A
tester ran 0.4.0 on dotnet/eShop and asked for the gaps that could interrupt a real flow of
personal data to come first, not to be treated like every other unresolved call.

The 27 gaps on eShop mix several kinds:

- **Refresh token:** `OidcClient.RefreshTokenAsync` sends one to the identity provider.
- **Device storage:** `SecureStorage.SetAsync` and `Preferences.Set` store a token and settings
  on the phone.
- **User interface:** `Shell.Current.GoToAsync`, `DisplayAlert`, `BindableObject.SetValue` and
  animations only show something on screen.
- **Unanalysed files:** 18 Razor views and two files that did not parse.

A data gap is recorded only where data reached it (ADR 0004), and it lists the sources that did.
What it lacked was a reading of those sources and of the call.

## Decision

**Every gap has an `impact` and an `impact_reason`.**

| Impact | When |
| --- | --- |
| `high` | personal data (a source whose category is known and needs no review) reaches the call's arguments |
| `medium` | only values whose category needs review reach it; or the gap exists whatever the data (parse error, unsupported language, framework or construct), so what it hides is not known |
| `low` | the call only presents data in the app's own interface (`catalogue/presentation.yml`) |

- **Order.** `coverage.gaps` lists gaps by impact, then kind and detail. So do the text output,
  and the review checklist. SARIF results carry `gap_impact`.
- **Findings.** A PFC01 message names the impact. A low-impact gap is reported at `info` at most,
  as a finding that needs review is. It is still a gap: coverage stays incomplete, and `check`
  still asks for a disposition.
- **The presentation catalogue** lists API path globs per language, with what the call does and a
  citation. It starts with .NET MAUI: dialogs, Shell navigation, bindable properties, animations.
  Navigation counts as presentation because the parameters stay in the app. A page that sends
  them on does so through calls the analysis sees from that page's own input.

**Device storage is a sink, not a gap.**

- **The sinks.** MAUI's `SecureStorage.SetAsync` and `Preferences.Set` (and Xamarin.Essentials')
  are sinks of a new class, `device_storage`. Only the stored value (argument 1) counts.
- **What they report.** Like `browser_storage`, the class is not external. The flows are
  reported, no rule fires, and the data map draws no edge for them.
- **Bare names.** MAUI projects import `Microsoft.Maui.Storage` implicitly, which the SDK table
  in `src/resolve.rs` does not list. So the catalogue also spells the bare type names.

## Alternatives

- **Drop presentation calls from the gaps.**
  - Against: the catalogue would claim to know the call. A navigation that hands data to another
    page is a place the analysis stops, and saying so is the point of ADR 0004. Ranking keeps it
    visible.
- **Score by the number of sources.**
  - Against: 59 sources reach `ObservableCollection.Add`, two reach a navigation. The count says
    how much code feeds the call, not whether the data can leave.
- **Treat receivers as well as arguments.**
  - Against: a gap is recorded where data reaches the arguments (ADR 0004). Recording receivers
    as well would add gaps, against what the tester asked for.
- **Add MAUI's implicit namespaces to `src/resolve.rs`.**
  - For: the right fix for the bare names.
  - Against, for now: the qualification rule (ADR 0016) guesses only when every namespace in
    scope is known. Eighteen more namespaces would change how much of the client app is
    qualified, and so every gap detail there.
  - Kept for later.

## Measurements

eShop at `dc7ea499`:

| | Before | After |
| --- | --- | --- |
| Gaps (PFC01) | 27 | 25 |
| `high` | — | 16 (OIDC calls, the request provider through DI, observable collections, `SetProperty`, exception messages) |
| `medium` | — | 4 (two parse errors, Razor views, one collection reached only by values that need review) |
| `low` | — | 5 (navigation, dialog, bindable property, two animation calls), at `info` |
| Flows | 214 | 227 (13 to device storage) |

Measured on 0.4.0. With ADRs 0020 and 0021 the gaps and their impacts are the same, and flows
go from 208 to 221.

## Consequences

- `tests/fixtures/cs/gap-impact` has a gap of each impact and two device-storage sinks.
  `fixture.json` can state `gap_impacts`.
- The eShop smoke test checks three impacts, and that `SecureStorage.SetAsync` and
  `Preferences.Set` are no longer gaps.
- The flow-facts schema gains `impact` and `impact_reason` on gaps, and `device_storage` as a sink
  class (configuration too).
- **Not covered:**
  - `high` does not mean the call sends data anywhere, only that personal data reaches it;
  - presentation entries exist for MAUI only; other UI frameworks rank as their data says.
