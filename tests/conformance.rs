//! The conformance corpus, in process: every required vector's flows are exactly its
//! expected.jsonl, and every known-limitation vector still fails (when one starts passing, its
//! status and KNOWN-LIMITATIONS.md should change).

use std::collections::BTreeSet;
use std::path::Path;

use privacy_flow::analyze::{self, EngineKind, Tree};
use privacy_flow::files;
use privacy_flow::output::{self, Format};

type Flow = (String, u64, String, u64, String);

fn flows(text: &str) -> BTreeSet<Flow> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            (
                v["source"]["path"].as_str().unwrap().to_string(),
                v["source"]["line"].as_u64().unwrap(),
                v["sink"]["path"].as_str().unwrap().to_string(),
                v["sink"]["line"].as_u64().unwrap(),
                v["category"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/vectors");
    let mut vectors: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    vectors.sort();
    let mut failures = Vec::new();
    let mut required = 0;
    for v in vectors {
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(v.join("vector.json")).unwrap()).unwrap();
        let expected = flows(&std::fs::read_to_string(v.join("expected.jsonl")).unwrap());
        let tree = Tree::WorkTree(&v);
        let loaded = analyze::load_config(&tree, None).unwrap();
        let listing = files::walk(&v).unwrap();
        let doc = analyze::analyze(&tree, &listing, &loaded, EngineKind::Worklist)
            .unwrap()
            .document;
        let got = flows(&output::render(&doc, Format::Facts).unwrap());
        let name = v.file_name().unwrap().to_string_lossy().to_string();
        match meta["status"].as_str() {
            Some("known-limitation") => {
                if got == expected {
                    failures.push(format!(
                        "{name}: known-limitation vector now passes; promote it to required"
                    ));
                }
            }
            _ => {
                required += 1;
                if got != expected {
                    failures.push(format!(
                        "{name}: missing {:?}, unexpected {:?}",
                        expected.difference(&got).collect::<Vec<_>>(),
                        got.difference(&expected).collect::<Vec<_>>()
                    ));
                }
            }
        }
    }
    assert!(required >= 30, "the corpus lost vectors");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
