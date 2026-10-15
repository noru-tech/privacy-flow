//! SARIF 2.1.0, one result per finding: a code flow per flow of its exposure, with every hop from
//! source to sink, and its sources as related locations.

use serde_json::{Value, json};

use crate::config::Severity;
use crate::report::{DispositionStatus, Document, Hop, Loc};
use crate::rules::{RULES, help_uri};

fn level(s: Severity) -> &'static str {
    match s {
        Severity::High => "error",
        Severity::Medium | Severity::Warning => "warning",
        Severity::Info => "note",
    }
}

fn physical(path: &str, line: Option<u32>, column: Option<u32>) -> Value {
    let mut loc = json!({ "artifactLocation": { "uri": path, "uriBaseId": "%SRCROOT%" } });
    if let Some(l) = line {
        let mut region = json!({ "startLine": l });
        if let Some(c) = column {
            region["startColumn"] = json!(c);
        }
        loc["region"] = region;
    }
    loc
}

fn hop_location(h: &Hop) -> Value {
    let msg = match &h.note {
        Some(n) => format!("{}: {} — {}", h.kind, h.text, n),
        None => format!("{}: {}", h.kind, h.text),
    };
    json!({
        "location": {
            "physicalLocation": physical(&h.path, Some(h.line), Some(h.column)),
            "message": { "text": msg }
        }
    })
}

pub fn render(doc: &Document) -> Value {
    let rules: Vec<Value> = RULES
        .iter()
        .map(|r| {
            let s = doc
                .policy
                .rules
                .get(r.id)
                .map(|s| s.severity)
                .unwrap_or(r.severity);
            json!({
                "id": r.id,
                "name": r.name,
                "shortDescription": { "text": r.title },
                "helpUri": help_uri(r.id),
                "defaultConfiguration": { "level": level(s) },
                "properties": { "tags": ["privacy"], "maps_to": r.maps_to, "severity": s.as_str() }
            })
        })
        .collect();
    let flows: std::collections::BTreeMap<&str, &crate::report::FlowRec> =
        doc.flows.iter().map(|f| (f.id.as_str(), f)).collect();
    let gaps: std::collections::BTreeMap<&str, &crate::report::GapRec> = doc
        .coverage
        .gaps
        .iter()
        .map(|g| (g.id.as_str(), g))
        .collect();
    let results: Vec<Value> = doc
        .findings
        .iter()
        .map(|f| {
            let rule_index = RULES.iter().position(|r| r.id == f.rule_id).unwrap_or(0);
            let mut r = json!({
                "ruleId": f.rule_id,
                "ruleIndex": rule_index,
                "level": level(f.severity),
                "message": { "text": f.message },
                "partialFingerprints": { "privacyFlowFindingId/v1": f.id },
                "properties": {
                    "severity": f.severity.as_str(),
                    "categories": f.categories,
                    "processor": f.processor,
                    "needs_review": f.needs_review,
                    "heuristic": f.heuristic
                }
            });
            let mut locations = Vec::new();
            let evidence: Vec<&crate::report::FlowRec> =
                f.flows.iter().filter_map(|id| flows.get(id.as_str()).copied()).collect();
            if let Some(flow) = evidence.first() {
                if let Some(last) = flow.path.last() {
                    locations.push(json!({ "physicalLocation": physical(&last.path, Some(last.line), Some(last.column)) }));
                }
                r["codeFlows"] = json!(evidence.iter().map(|fl| json!({
                    "message": { "text": format!("{} ({})", fl.id, fl.category) },
                    "threadFlows": [{ "locations": fl.path.iter().map(hop_location).collect::<Vec<_>>() }]
                })).collect::<Vec<_>>());
                // Each source once, in the order its first flow cites it.
                let mut seen = std::collections::BTreeSet::new();
                let related: Vec<Value> = evidence
                    .iter()
                    .filter_map(|fl| fl.path.first())
                    .filter(|h| seen.insert((h.path.as_str(), h.line, h.column)))
                    .enumerate()
                    .map(|(i, h)| json!({
                        "id": i,
                        "physicalLocation": physical(&h.path, Some(h.line), Some(h.column)),
                        "message": { "text": format!("source: {}", h.text) }
                    }))
                    .collect();
                r["relatedLocations"] = json!(related);
                r["properties"]["flows"] = json!(f.flows);
            } else if let Some(g) = f.gap.as_deref().and_then(|id| gaps.get(id)) {
                if let Some(l) = g.locations.first() {
                    locations.push(json!({ "physicalLocation": physical(&l.path, l.line, l.column) }));
                }
                r["properties"]["gap"] = json!(g.id);
                r["properties"]["gap_kind"] = json!(g.kind);
                r["properties"]["gap_impact"] = json!(g.impact);
            } else if let Some(Loc { path, line, column }) = &f.location {
                locations.push(json!({ "physicalLocation": physical(path, Some(*line), Some(*column)) }));
            }
            if !locations.is_empty() {
                r["locations"] = json!(locations);
            }
            if let Some(d) = &f.disposition
                && d.status != DispositionStatus::Open {
                    r["suppressions"] = json!([{
                        "kind": "external",
                        "status": "accepted",
                        "justification": d.rationale.clone().unwrap_or_default(),
                        "properties": { "disposition": d }
                    }]);
                }
            r
        })
        .collect();
    let mut run = json!({
        "tool": {
            "driver": {
                "name": "privacy-flow",
                "version": doc.tool.version,
                "semanticVersion": doc.tool.version,
                "informationUri": "https://github.com/noru-tech/privacy-flow",
                "rules": rules
            }
        },
        "columnKind": "unicodeCodePoints",
        "results": results,
        "properties": {
            "coverage_complete": doc.coverage.complete,
            "catalogue_version": doc.catalogue.version,
            "document_digest": doc.digest
        }
    });
    if let Some(c) = &doc.subject.commit {
        run["properties"]["commit"] = json!(c);
    }
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [run]
    })
}
