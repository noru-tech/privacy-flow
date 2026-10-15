//! The lock: what `piiflow datamap` generated last, the base of the next merge. Committed beside
//! the data map; Fides reads only YAML, so it ignores the file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::datamap::yaml::{self, Node};

pub const FORMAT: &str = "0.1";

/// `.fides/datamap.yml` → `.fides/datamap.lock.json`.
pub fn path_for(map: &Path) -> PathBuf {
    let stem = map
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "datamap".into());
    map.with_file_name(format!("{stem}.lock.json"))
}

/// The generated tree a lock holds.
pub fn parse(text: &str) -> Result<Node> {
    let v: serde_json::Value = serde_json::from_str(text).context("not JSON")?;
    crate::config::validate_schema(
        include_str!("../../schemas/datamap-lock.schema.json"),
        &v,
        "lock",
    )?;
    if v["format"] != FORMAT {
        bail!("lock format {} is not {FORMAT}", v["format"]);
    }
    Ok(yaml::from_json(&v["generated"]))
}

/// The lock for a generated tree, as RFC 8785 JSON.
pub fn render(generated: &Node) -> Result<String> {
    let v = serde_json::json!({
        "format": FORMAT,
        "generated": yaml::to_json(generated),
    });
    let mut s = crate::canonical::jcs_bytes(&v)?;
    s.push('\n');
    Ok(s)
}
