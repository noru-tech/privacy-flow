//! Output formats. Every machine format is written as exact, reproducible bytes: JSON as
//! RFC 8785, the Fides fragment and the table by deterministic hand-written renderers.

pub mod fides;
pub mod intoto;
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
    })
}
