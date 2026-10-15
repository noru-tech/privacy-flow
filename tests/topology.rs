//! `piiflow datamap` on the fixtures marked `"topology": true`: the topology section and the
//! generated data map match committed goldens (`UPDATE_GOLDENS=1 cargo test --test topology`),
//! are the same with 1 and 8 threads, re-derive to themselves, and validate against the schema.

mod common;

use privacy_flow::canonical;

fn with_threads<T: Send>(n: usize, f: impl FnOnce() -> T + Send) -> T {
    rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build()
        .unwrap()
        .install(f)
}

#[test]
fn topology_and_datamap_match_goldens() {
    let goldens = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    let update = std::env::var_os("UPDATE_GOLDENS").is_some();
    let dirs = common::topology_fixtures();
    assert!(!dirs.is_empty(), "no topology fixtures");
    let mut failures = Vec::new();
    for dir in dirs {
        let name = common::name(&dir).replace('/', "__");
        let doc = with_threads(1, || common::datamap_doc(&dir));
        let many = with_threads(8, || common::datamap_doc(&dir));
        assert_eq!(doc, many, "{name}: 1 and 8 threads differ");

        let topo = doc.topology.clone().expect("topology");
        let mut again = doc.clone();
        privacy_flow::topology::derive(&mut again);
        assert_eq!(
            again.topology.as_ref(),
            Some(&topo),
            "{name}: edges do not re-derive"
        );
        let value = serde_json::to_value(&doc).unwrap();
        privacy_flow::config::validate_schema(
            include_str!("../schemas/flows.schema.json"),
            &value,
            &name,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));

        let outputs = [
            (
                format!("{name}.topology.json"),
                canonical::jcs_bytes(&topo).unwrap(),
            ),
            (
                format!("{name}.datamap.yml"),
                privacy_flow::datamap::render_generated(&doc),
            ),
        ];
        for (file, bytes) in outputs {
            let path = goldens.join(&file);
            if update {
                std::fs::write(&path, &bytes).unwrap();
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(want) if want == bytes => {}
                Ok(_) => failures.push(format!("{file} differs")),
                Err(_) => failures.push(format!("{file} is missing")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "golden mismatches (UPDATE_GOLDENS=1 to accept):\n{}",
        failures.join("\n")
    );
}
