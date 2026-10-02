//! Output formats. Every machine format is written as exact, reproducible bytes: JSON as
//! RFC 8785, the Fides fragment and the table by deterministic hand-written renderers.

pub mod fides;
pub mod intoto;
pub mod review;
pub mod sarif;
pub mod table;

use std::path::Path;

use anyhow::Result;

use crate::report::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Table,
    Json,
    Sarif,
    Fides,
    #[value(name = "in-toto")]
    InToto,
    /// One canonical JSON line per flow: source, sink, category, sink class and processor. The
    /// conformance corpus's external-runner contract reads this.
    Facts,
    /// A Markdown review queue: findings that need a human decision (maybe-personal names,
    /// heuristic sinks) and open coverage gaps, each with what would resolve it.
    Review,
}

impl Format {
    /// The format an output file name implies.
    pub fn infer(path: &Path) -> Option<Format> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name.ends_with(".intoto.json") {
            Some(Format::InToto)
        } else if name.ends_with(".sarif") || name.ends_with(".sarif.json") {
            Some(Format::Sarif)
        } else if name.ends_with(".json") {
            Some(Format::Json)
        } else if name.ends_with(".yml") || name.ends_with(".yaml") {
            Some(Format::Fides)
        } else if name.ends_with(".jsonl") {
            Some(Format::Facts)
        } else if name.ends_with(".md") {
            Some(Format::Review)
        } else if name.ends_with(".txt") {
            Some(Format::Table)
        } else {
            None
        }
    }
}

pub fn render(doc: &Document, format: Format) -> Result<String> {
    Ok(match format {
        Format::Table => table::render(doc),
        Format::Json => crate::canonical::jcs_bytes(doc)?,
        Format::Sarif => crate::canonical::jcs_bytes(&sarif::render(doc))?,
        Format::Fides => fides::render(doc),
        Format::InToto => crate::canonical::jcs_bytes(&intoto::render(doc)?)?,
        Format::Facts => facts_lines(doc)?,
        Format::Review => review::render(doc),
    })
}

/// `{"category","class","processor","sink":{"path","line","column"},"source":{...}}` per flow,
/// sorted, one RFC 8785 line each.
pub fn facts_lines(doc: &Document) -> Result<String> {
    let sources: std::collections::BTreeMap<&str, &crate::report::SourceRec> =
        doc.sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let sinks: std::collections::BTreeMap<&str, &crate::report::SinkRec> =
        doc.sinks.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut lines = Vec::new();
    for f in &doc.flows {
        let (Some(src), Some(snk)) = (sources.get(f.source.as_str()), sinks.get(f.sink.as_str()))
        else {
            continue;
        };
        let v = serde_json::json!({
            "source": { "path": src.location.path, "line": src.location.line, "column": src.location.column },
            "sink": { "path": snk.location.path, "line": snk.location.line, "column": snk.location.column },
            "category": f.category,
            "class": snk.class.as_str(),
            "processor": snk.processor,
        });
        lines.push(crate::canonical::jcs_bytes(&v)?);
    }
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}
