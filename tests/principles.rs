//! Tests that fail if a principle is broken (principles 2, 3, 5 and 6; principle 1 is
//! `determinism.rs`, principle 4 is `cli.rs` and `fixtures.rs`).

use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Crates that open network connections or talk to a model provider. None may be a dependency
/// of the binary, directly or transitively.
const FORBIDDEN_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "surf",
    "attohttpc",
    "minreq",
    "h2",
    "quinn",
    "tokio-tungstenite",
    "tungstenite",
    "async-openai",
    "openai",
    "anthropic",
    "anthropic-sdk",
    "ollama-rs",
    "llm",
    "genai",
    "rig-core",
    "sentry",
    "opentelemetry",
    "posthog-rs",
    "segment",
    "self_update",
    "update-informer",
];

#[test]
fn principle_2_and_3_no_network_or_model_crates() {
    let lock = std::fs::read_to_string(root().join("Cargo.lock")).unwrap();
    for line in lock.lines() {
        if let Some(name) = line.strip_prefix("name = ") {
            let name = name.trim_matches('"');
            assert!(
                !FORBIDDEN_CRATES.contains(&name),
                "Cargo.lock contains {name}, which can reach the network or a model"
            );
        }
    }
}

#[test]
fn principle_2_and_3_no_sockets_in_the_source() {
    // The only process the binary starts is `git`, for file enumeration and `diff`.
    let mut stack = vec![root().join("src")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for needle in [
                "std::net",
                "TcpStream",
                "UdpSocket",
                "tokio::net",
                "ToSocketAddrs",
            ] {
                assert!(!text.contains(needle), "{} uses {needle}", p.display());
            }
            for (i, line) in text.lines().enumerate() {
                if line.contains("Command::new(") {
                    assert!(
                        line.contains("Command::new(\"git\")"),
                        "{}:{} starts a process other than git",
                        p.display(),
                        i + 1
                    );
                }
            }
        }
    }
}

#[test]
fn principle_5_every_flow_hop_is_cited() {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("schemas/flows.schema.json")).unwrap(),
    )
    .unwrap();
    let hop = &schema["properties"]["flows"]["items"]["properties"]["path"];
    assert_eq!(
        hop["minItems"], 2,
        "a flow path must have at least a source and a sink"
    );
    let required = hop["items"]["required"].as_array().unwrap();
    for field in ["path", "line", "column", "kind"] {
        assert!(
            required.iter().any(|r| r == field),
            "a hop must carry {field}"
        );
    }
}

#[test]
fn principle_6_rules_are_data() {
    // The catalogue loads from YAML, and no sink, source or SDK name is hard-coded in the
    // analysis: the facts builder only asks the catalogue.
    let cat = privacy_flow::catalogue::Catalogue::builtin().unwrap();
    assert!(cat.sinks.len() > 40 && cat.sources.len() > 5 && !cat.sanitisers.is_empty());
    let facts = std::fs::read_to_string(root().join("src/facts.rs")).unwrap();
    for sdk in [
        "openai",
        "posthog",
        "sentry",
        "segment",
        "anthropic",
        "pino",
        "winston",
        "logging.",
    ] {
        assert!(
            !facts.to_lowercase().contains(&format!("\"{sdk}")),
            "src/facts.rs mentions {sdk}; it belongs in catalogue/"
        );
    }
}

#[test]
fn vendored_data_is_unmodified() {
    use sha2::{Digest, Sha256};
    let pins = [
        (
            "vendor/taxonomy/data_categories.json",
            "f1be7bbd1d19b8f2ed0f777156ec844e8a5a7144563f3afae7548d5c32c24af1",
        ),
        (
            "vendor/taxonomy/data_subjects.json",
            "18b9758032e5c6264fd8842aa71630e67b81c253d8064860e0a6e09113fd477c",
        ),
        (
            "vendor/taxonomy/data_uses.json",
            "557d57cca1eac975ca45dda7a344541ec6c59c13e70e83192d140760e0a8e7bb",
        ),
        (
            "vendor/taxonomy/special_categories.json",
            "ac5316dbbaf7605f26a3e4750df97f2e341572ebe8ea419a39557dc40c534e71",
        ),
        (
            "vendor/classification/classification.json",
            "4f1caa4a3e8d076d9ba9e6f6b9d067a9c5888d85f7af4ef76f19c33815650333",
        ),
    ];
    for (path, want) in pins {
        let bytes = std::fs::read(root().join(path)).unwrap();
        let got: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            got, want,
            "{path} differs from the vendored snapshot (see vendor/SOURCE.md)"
        );
    }
}

#[test]
fn every_rule_has_a_page_and_every_catalogue_category_is_a_taxonomy_key() {
    for r in privacy_flow::rules::RULES {
        let page = root().join(format!("docs/rules/{}.md", r.id));
        assert!(page.is_file(), "missing {}", page.display());
    }
    let classifier = privacy_flow::classify::Classifier::new().unwrap();
    let cat = privacy_flow::catalogue::Catalogue::builtin().unwrap();
    for s in &cat.sources {
        let mut cats: Vec<&String> = s.def.category.iter().collect();
        for p in &s.def.params {
            cats.extend(p.rule.whole.iter());
            cats.extend(p.rule.fields.values());
            cats.extend(p.rule.methods.values());
            for r in p.by_type.values() {
                cats.extend(r.fields.values());
                cats.extend(r.methods.values());
            }
        }
        for c in cats {
            classifier
                .check_category(c)
                .unwrap_or_else(|e| panic!("source {}: {e}", s.def.id));
        }
    }
    for s in &cat.sanitisers {
        for r in &s.def.removes {
            if r != "*" {
                classifier
                    .check_category(r)
                    .unwrap_or_else(|e| panic!("sanitiser {}: {e}", s.def.id));
            }
        }
    }
}
