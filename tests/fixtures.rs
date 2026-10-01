//! Every fixture: the findings are exactly the `expect:` markers in its files, coverage is as
//! its fixture.json says, the document validates, and the Datalog engine agrees with the
//! worklist engine on every (source, sink) pair.

mod common;

use std::collections::BTreeSet;

use privacy_flow::analyze::EngineKind;
use privacy_flow::engine::{self, Options};

#[test]
fn every_fixture_matches_its_markers() {
    let mut failures = Vec::new();
    for dir in common::fixtures() {
        let name = common::name(&dir);
        let expect = common::expectations(&dir);
        let run = common::run(&dir, EngineKind::Worklist);
        let doc = &run.document;
        let actual: BTreeSet<(String, String, u32)> = doc
            .findings
            .iter()
            .filter_map(|f| {
                f.location
                    .as_ref()
                    .map(|l| (f.rule_id.clone(), l.path.clone(), l.line))
            })
            .collect();
        if actual != expect.findings {
            let missing: Vec<_> = expect.findings.difference(&actual).collect();
            let extra: Vec<_> = actual.difference(&expect.findings).collect();
            failures.push(format!(
                "{name}: findings differ\n    missing: {missing:?}\n    unexpected: {extra:?}"
            ));
        }
        if doc.coverage.complete != expect.complete {
            let gaps: Vec<_> = doc
                .coverage
                .gaps
                .iter()
                .map(|g| format!("{} {}", g.kind, g.detail))
                .collect();
            failures.push(format!(
                "{name}: coverage complete is {} (gaps: {gaps:?})",
                doc.coverage.complete
            ));
        }
        for kind in &expect.gaps {
            if !doc.coverage.gaps.iter().any(|g| &g.kind == kind) {
                failures.push(format!("{name}: expected a {kind} gap"));
            }
        }
        let sinks: BTreeSet<(String, u32)> = doc
            .flows
            .iter()
            .filter_map(|f| f.path.last().map(|h| (h.path.clone(), h.line)))
            .collect();
        for want in &expect.flows {
            if !sinks.contains(want) {
                failures.push(format!("{name}: expected a flow to {}:{}", want.0, want.1));
            }
        }
        // Every finding that names a flow carries a complete chain from a source to a sink.
        for fl in &doc.flows {
            if fl.path.first().map(|h| h.kind.as_str()) != Some("source")
                || fl.path.last().map(|h| h.kind.as_str()) != Some("sink")
            {
                failures.push(format!(
                    "{name}: flow {} is not a source-to-sink chain",
                    fl.id
                ));
            }
        }
        if let Err(e) = privacy_flow::cli::validate_document(doc) {
            failures.push(format!("{name}: document does not validate: {e:#}"));
        }
        // The Datalog oracle.
        let opts = Options {
            max_depth: doc.analysis.max_call_depth,
        };
        let dl = engine::datalog::run(&run.facts, &run.program, opts);
        let wl: BTreeSet<_> = run.reaches.iter().map(|r| (r.seed, r.target)).collect();
        if dl != wl {
            failures.push(format!(
                "{name}: engines disagree\n    datalog only: {:?}\n    worklist only: {:?}",
                dl.difference(&wl).collect::<Vec<_>>(),
                wl.difference(&dl).collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} problem(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_rule_has_failing_and_passing_fixtures_in_both_languages() {
    let names: BTreeSet<String> = common::fixtures().iter().map(|d| common::name(d)).collect();
    for lang in ["ts", "py"] {
        for rule in [
            "pf001", "pf002", "pf003", "pf004", "pf005", "pf006", "pfc01",
        ] {
            for kind in ["fail", "pass"] {
                let n = format!("{lang}/{rule}-{kind}");
                assert!(names.contains(&n), "missing fixture {n}");
            }
        }
    }
}

#[test]
fn failing_fixtures_fail_their_rule_and_passing_fixtures_do_not() {
    for dir in common::fixtures() {
        let name = common::name(&dir);
        let Some((_, case)) = name.split_once('/') else {
            continue;
        };
        let Some((rule, kind)) = case.split_once('-') else {
            continue;
        };
        if !rule.starts_with("pf") || !matches!(kind, "fail" | "pass") {
            continue;
        }
        let rule = rule.to_uppercase();
        let doc = common::run(&dir, EngineKind::Worklist).document;
        let fired = doc.findings.iter().any(|f| f.rule_id == rule);
        match kind {
            "fail" => assert!(fired, "{name}: {rule} did not fire"),
            _ => assert!(!fired, "{name}: {rule} fired"),
        }
    }
}
