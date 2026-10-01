//! A plain-text summary for terminals and CI logs.

use std::fmt::Write;

use crate::report::Document;

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    let commit = doc
        .subject
        .commit
        .as_deref()
        .map(|c| &c[..c.len().min(12)])
        .unwrap_or("no commit");
    let langs: Vec<String> = doc
        .subject
        .languages
        .iter()
        .map(|l| format!("{} {} files", l.language, l.files))
        .collect();
    let _ = writeln!(
        out,
        "privacy-flow {} · {} · {} · {}",
        doc.tool.version,
        commit,
        if langs.is_empty() {
            "no supported files".to_string()
        } else {
            langs.join(", ")
        },
        match doc.subject.enumerated_by {
            crate::files::Method::Git => "git ls-files",
            crate::files::Method::Walk => "directory walk",
            crate::files::Method::GitRevision => "git revision",
        }
    );
    if let Some(d) = &doc.diff {
        let _ = writeln!(
            out,
            "diff {}..{}: {} introduced, {} changed, {} removed",
            short(&d.base),
            short(&d.head),
            d.introduced,
            d.changed,
            d.removed
        );
    }
    out.push('\n');
    if doc.findings.is_empty() {
        out.push_str("No findings.\n");
    } else {
        let width = doc
            .findings
            .iter()
            .map(|f| f.rule_id.len())
            .max()
            .unwrap_or(5);
        for f in &doc.findings {
            let loc = f
                .location
                .as_ref()
                .map(|l| format!("{}:{}:{}", l.path, l.line, l.column))
                .unwrap_or_else(|| "-".into());
            let mut tags = Vec::new();
            if f.needs_review {
                tags.push("needs review");
            }
            if f.heuristic {
                tags.push("heuristic");
            }
            if let Some(d) = &f.disposition {
                tags.push(match d.status {
                    crate::report::DispositionStatus::Open => "open",
                    crate::report::DispositionStatus::Accepted => "accepted",
                    crate::report::DispositionStatus::Remediated => "remediated",
                    crate::report::DispositionStatus::FalsePositive => "false positive",
                });
            }
            let tags = if tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", tags.join(", "))
            };
            let _ = writeln!(
                out,
                "{:<width$}  {:<7}  {}  {}",
                f.rule_id,
                f.severity.as_str(),
                f.id,
                loc
            );
            let _ = writeln!(out, "{:<width$}           {}{}", "", f.message, tags);
        }
    }
    out.push('\n');
    let counts: Vec<String> = doc
        .summary
        .findings
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    let _ = writeln!(
        out,
        "{} flow(s) from {} source(s) to {} sink(s); findings: {}",
        doc.summary.flows,
        doc.sources.len(),
        doc.sinks.len(),
        if counts.is_empty() {
            "none".to_string()
        } else {
            counts.join(", ")
        }
    );
    if !doc.summary.processors.is_empty() {
        let _ = writeln!(
            out,
            "processors reached: {}",
            doc.summary.processors.join(", ")
        );
    }
    if doc.coverage.complete {
        out.push_str("coverage: complete\n");
    } else {
        let _ = writeln!(
            out,
            "coverage: INCOMPLETE — {} gap(s); a clean result cannot be claimed",
            doc.coverage.gaps.len()
        );
        for g in doc.coverage.gaps.iter().take(20) {
            let loc = g.locations.first().map(|l| match l.line {
                Some(line) => format!("{}:{line}", l.path),
                None => l.path.clone(),
            });
            let more = if g.locations.len() > 1 {
                format!(" (+{} more)", g.locations.len() - 1)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  {} {}: {} — {}{}",
                g.id,
                g.kind,
                g.detail,
                loc.unwrap_or_default(),
                more
            );
        }
        if doc.coverage.gaps.len() > 20 {
            let _ = writeln!(out, "  … {} more", doc.coverage.gaps.len() - 20);
        }
    }
    out
}

fn short(s: &str) -> &str {
    &s[..s.len().min(12)]
}
