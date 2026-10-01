//! Shared helpers for the integration tests: running the analysis in-process over a fixture
//! directory, and reading the expectations written next to the code.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use privacy_flow::analyze::{self, EngineKind, Run, Tree};
use privacy_flow::files;

pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every fixture directory: `tests/fixtures/<language>/<name>`.
pub fn fixtures() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for lang in std::fs::read_dir(fixtures_root()).expect("fixtures dir") {
        let lang = lang.unwrap().path();
        if !lang.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&lang).unwrap() {
            let f = f.unwrap().path();
            if f.join("fixture.json").is_file() {
                out.push(f);
            }
        }
    }
    out.sort();
    out
}

pub fn name(dir: &Path) -> String {
    dir.strip_prefix(fixtures_root())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn run(dir: &Path, engine: EngineKind) -> Run {
    let tree = Tree::WorkTree(dir);
    let loaded = analyze::load_config(&tree, None).expect("config");
    let listing = files::walk(dir).expect("walk");
    analyze::analyze(&tree, &listing, &loaded, engine).expect("analysis")
}

pub struct Expect {
    /// (rule, path, line)
    pub findings: BTreeSet<(String, String, u32)>,
    /// (path, line) of sinks a flow must reach (classes no rule covers).
    pub flows: BTreeSet<(String, u32)>,
    pub complete: bool,
    pub gaps: Vec<String>,
}

/// Read `expect: PF001 PF002` and `expect-flow` markers from every file, and fixture.json.
pub fn expectations(dir: &Path) -> Expect {
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("fixture.json")).unwrap()).unwrap();
    let mut findings = BTreeSet::new();
    let mut flows = BTreeSet::new();
    let listing = files::walk(dir).unwrap();
    for rel in &listing.files {
        if rel == "fixture.json" {
            continue;
        }
        let text = std::fs::read_to_string(dir.join(rel)).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let n = i as u32 + 1;
            if let Some(idx) = line.find("expect: ") {
                for rule in line[idx + "expect: ".len()..].split_whitespace() {
                    if rule.starts_with("PF") {
                        findings.insert((rule.to_string(), rel.clone(), n));
                    }
                }
            }
            if line.contains("expect-flow") {
                flows.insert((rel.clone(), n));
            }
        }
    }
    Expect {
        findings,
        flows,
        complete: meta["complete"].as_bool().unwrap_or(true),
        gaps: meta["gaps"]
            .as_array()
            .map(|a| a.iter().map(|g| g.as_str().unwrap().to_string()).collect())
            .unwrap_or_default(),
    }
}
