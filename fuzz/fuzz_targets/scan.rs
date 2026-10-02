//! The whole pipeline on an in-memory tree: lowering, resolution across files, facts, the
//! worklist engine and the report. It never panics, and the same input gives the same document
//! digest twice (the determinism principle).
//!
//! The input is split on NUL bytes into at most eight files. The first byte of each chunk picks
//! its name, including `.privacy-flow.yml`, so a configuration can take part.

#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;
use privacy_flow::analyze::{self, EngineKind, Tree};
use privacy_flow::files::{Listing, Method};

const NAMES: [&str; 8] = [
    "src/index.ts",
    "src/users.ts",
    "src/view.tsx",
    "lib/util.js",
    "app/main.py",
    "app/models.py",
    "package.json",
    ".privacy-flow.yml",
];

fn scan(tree: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    let tree_ref = Tree::Revision(tree);
    let loaded = analyze::load_config(&tree_ref, None).ok()?;
    let listing = Listing {
        method: Method::GitRevision,
        files: tree.keys().cloned().collect(),
        commit: None,
        dirty: None,
    };
    let run = analyze::analyze(&tree_ref, &listing, &loaded, EngineKind::Worklist).ok()?;
    Some(run.document.digest)
}

fuzz_target!(|data: &[u8]| {
    let mut tree = BTreeMap::new();
    for chunk in data.split(|&b| b == 0).take(NAMES.len()) {
        let Some((&pick, body)) = chunk.split_first() else {
            continue;
        };
        tree.insert(
            NAMES[usize::from(pick) % NAMES.len()].to_string(),
            body.to_vec(),
        );
    }
    if tree.is_empty() {
        return;
    }
    let first = scan(&tree);
    assert_eq!(
        first,
        scan(&tree),
        "the same tree gave two different documents"
    );
});
