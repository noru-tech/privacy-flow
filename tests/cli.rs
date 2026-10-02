//! The command line: exit codes, outputs, dispositions, explain, diff and validation.
//! Principle 4 lives here: a result with coverage gaps can never exit as clean.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command as Std;

use assert_cmd::Command;
use predicates::prelude::*;

fn piiflow() -> Command {
    Command::cargo_bin("piiflow").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    common::fixtures_root().join(name)
}

/// Copy a fixture into a temporary directory (so outputs never land in the repository).
fn copy_fixture(name: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixture(name), tmp.path());
    tmp
}

fn copy_dir(from: &Path, to: &Path) {
    for e in walkdir::WalkDir::new(from) {
        let e = e.unwrap();
        let rel = e.path().strip_prefix(from).unwrap();
        let dst = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dst).unwrap();
        } else {
            std::fs::copy(e.path(), &dst).unwrap();
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Std::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

#[test]
fn scan_writes_the_default_document_and_exits_0_when_complete() {
    let tmp = copy_fixture("ts/pf001-fail");
    piiflow()
        .arg("scan")
        .arg(tmp.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("wrote"));
    let doc = tmp.path().join(".privacy-flow/flows.json");
    assert!(doc.is_file());
    piiflow().arg("validate").arg(&doc).assert().code(0);
}

#[test]
fn principle_4_gaps_exit_4_from_scan_and_check_even_with_no_findings() {
    let tmp = copy_fixture("ts/unsupported-framework");
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .assert()
        .code(4);
    let doc = tmp.path().join(".privacy-flow/flows.json");
    // `check` may not report the scan clean even at the most permissive threshold.
    piiflow()
        .args(["check", "--fail-on", "high"])
        .arg(&doc)
        .assert()
        .code(4)
        .stdout(predicate::str::contains("INCOMPLETE"));
}

#[test]
fn check_fails_on_the_threshold_and_honours_dispositions_by_date() {
    let tmp = copy_fixture("ts/pf002-fail");
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .assert()
        .code(0);
    let path = tmp.path().join(".privacy-flow/flows.json");
    piiflow()
        .arg("check")
        .arg(&path)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("PF002"));
    piiflow()
        .args(["check", "--fail-on", "high"])
        .arg(&path)
        .assert()
        .code(1);

    // Record a disposition on the finding, as a human would: only the disposition changes.
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    doc["findings"][0]["disposition"] = serde_json::json!({
        "status": "accepted", "owner": "privacy@example.com", "decided_at": "2026-09-01",
        "expires_at": "2026-09-30", "rationale": "DPA in signature; tracked in LEG-91", "remediated_at": null
    });
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    piiflow().arg("validate").arg(&path).assert().code(0);
    // A non-open disposition needs an explicit date: the machine clock is never read.
    piiflow().arg("check").arg(&path).assert().code(2);
    piiflow()
        .args(["check", "--as-of", "2026-09-15"])
        .arg(&path)
        .assert()
        .code(0);
    piiflow()
        .args(["check", "--as-of", "2026-10-01"])
        .arg(&path)
        .assert()
        .code(1);
    piiflow()
        .args(["check", "--as-of", "2026-31-01"])
        .arg(&path)
        .assert()
        .code(2);

    // A re-scan carries the disposition forward by finding ID.
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .assert()
        .code(0);
    let again: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(again["findings"][0]["disposition"]["status"], "accepted");
}

#[test]
fn validate_rejects_a_tampered_document_with_exit_3() {
    let tmp = copy_fixture("ts/pf001-fail");
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .assert()
        .code(0);
    let path = tmp.path().join(".privacy-flow/flows.json");
    let text = std::fs::read_to_string(&path).unwrap();
    // Removing a finding is caught by re-derivation; editing a fact is caught by the digest.
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    doc["findings"].as_array_mut().unwrap().remove(0);
    std::fs::write(&path, doc.to_string()).unwrap();
    piiflow()
        .arg("validate")
        .arg(&path)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("digest"));
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    doc["flows"][0]["category"] = serde_json::json!("user.name");
    std::fs::write(&path, doc.to_string()).unwrap();
    piiflow().arg("validate").arg(&path).assert().code(3);
    std::fs::write(&path, "{}").unwrap();
    piiflow().arg("validate").arg(&path).assert().code(3);
}

#[test]
fn explain_prints_every_hop_with_its_source_line() {
    let tmp = copy_fixture("ts/cross-file-summary");
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .assert()
        .code(0);
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".privacy-flow/flows.json")).unwrap(),
    )
    .unwrap();
    let id = doc["findings"][0]["id"].as_str().unwrap().to_string();
    let hops = doc["flows"][0]["path"].as_array().unwrap().len();
    let out = piiflow()
        .args(["explain", &id, "--root"])
        .arg(tmp.path())
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("src/lib/format.ts"),
        "the chain crosses into the callee's file:\n{out}"
    );
    assert!(
        out.contains("return `${name}: ${phone_number}`;"),
        "source lines are shown:\n{out}"
    );
    assert_eq!(
        out.matches(" | ").count(),
        hops,
        "one source line per hop:\n{out}"
    );
    piiflow()
        .args(["explain", "pf-0000000000000000", "--root"])
        .arg(tmp.path())
        .assert()
        .code(2);
}

#[test]
fn every_format_renders_and_infers_from_the_file_name() {
    let tmp = copy_fixture("py/pf004-fail");
    for (name, needle) in [
        ("out.sarif", "\"version\":\"2.1.0\""),
        ("out.json", "\"schema_version\":\"0.1\""),
        ("out.yml", "egress:"),
    ] {
        let path = tmp.path().join(name);
        piiflow()
            .args(["scan", "-q"])
            .arg(tmp.path())
            .arg("-o")
            .arg(&path)
            .assert()
            .code(0);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(needle), "{name}: {text}");
    }
    piiflow()
        .args(["scan", "-q", "-f", "table"])
        .arg(tmp.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("PF004"));
    // An in-toto Statement needs a commit to name as its subject.
    piiflow()
        .args(["scan", "-q", "-f", "in-toto"])
        .arg(tmp.path())
        .assert()
        .code(2);
}

#[test]
fn in_toto_statement_names_the_commit_and_validates() {
    let tmp = copy_fixture("py/pf002-fail");
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "init"]);
    let out = tmp.path().join("flows.intoto.json");
    piiflow()
        .args(["scan", "-q"])
        .arg(tmp.path())
        .arg("-o")
        .arg(&out)
        .assert()
        .code(0);
    let st: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(st["_type"], "https://in-toto.io/Statement/v1");
    assert_eq!(
        st["subject"][0]["digest"]["gitCommit"],
        st["predicate"]["subject"]["commit"]
    );
    assert_eq!(st["predicate"]["subject"]["enumerated_by"], "git");
    piiflow().arg("validate").arg(&out).assert().code(0);
    piiflow().args(["check"]).arg(&out).assert().code(1);
}

#[test]
fn diff_reports_only_introduced_findings() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/a.ts"),
        "export function f(u: { email: string }) {\n  console.log(u.email);\n}\n",
    )
    .unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    std::fs::write(
        dir.join("src/b.ts"),
        "import { PostHog } from 'posthog-node';\nconst ph = new PostHog('k');\nexport function g(u: { email: string }) {\n  ph.capture({ distinctId: 'x', event: 'e', properties: { email: u.email } });\n}\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "head"]);
    let out = dir.join("diff.json");
    piiflow()
        .args(["diff", "-q", "HEAD~1..HEAD"])
        .arg(dir)
        .arg("-o")
        .arg(&out)
        .assert()
        .code(0);
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let rules: Vec<&str> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule_id"].as_str().unwrap())
        .collect();
    assert_eq!(rules, vec!["PF002"], "only the new flow is reported: {doc}");
    assert_eq!(doc["diff"]["introduced"], 1);
    piiflow().arg("validate").arg(&out).assert().code(0);
    // Against the working tree, and a bad range.
    piiflow()
        .args(["diff", "-q", "HEAD~1.."])
        .arg(dir)
        .arg("-f")
        .arg("table")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("PF002"));
    piiflow()
        .args(["diff", "-q", "nope..HEAD"])
        .arg(dir)
        .assert()
        .code(2);
    piiflow()
        .args(["diff", "-q", "HEAD"])
        .arg(dir)
        .assert()
        .code(2);
}

#[test]
fn usage_errors_exit_2_and_invalid_config_exits_3() {
    piiflow()
        .args(["scan", "/definitely/not/here"])
        .assert()
        .code(2);
    piiflow()
        .args(["scan", "--format", "nope"])
        .assert()
        .code(2);
    piiflow().arg("frobnicate").assert().code(2);
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".privacy-flow.yml"),
        "version: 1\nprocessors:\n  - name: X\n",
    )
    .unwrap();
    piiflow()
        .arg("scan")
        .arg(tmp.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("citation"));
    std::fs::write(
        tmp.path().join(".privacy-flow.yml"),
        "version: 1\nfields:\n  - { name: x, category: user.not_a_category, citation: abc }\n",
    )
    .unwrap();
    piiflow()
        .arg("scan")
        .arg(tmp.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("taxonomy"));
}

#[test]
fn rules_doctor_completions_and_manpages() {
    piiflow()
        .arg("rules")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("PF003").and(predicate::str::contains("js.openai")));
    let out = piiflow()
        .args(["rules", "-f", "json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(v["sinks"].as_array().unwrap().len() > 40);
    let fw = copy_fixture("ts/unsupported-framework");
    piiflow()
        .arg("doctor")
        .arg(fw.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Koa"));
    piiflow()
        .args(["completions", "bash"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("piiflow"));
    let tmp = tempfile::tempdir().unwrap();
    piiflow().arg("manpage").arg(tmp.path()).assert().code(0);
    assert!(tmp.path().join("piiflow-scan.1").is_file());
}

#[test]
fn threads_flag_does_not_change_output() {
    let tmp = copy_fixture("ts/cross-file-summary");
    let a = tmp.path().join("a.json");
    let b = tmp.path().join("b.json");
    piiflow()
        .args(["--threads", "1", "scan", "-q"])
        .arg(tmp.path())
        .arg("-o")
        .arg(&a)
        .assert()
        .code(0);
    piiflow()
        .args(["--threads", "8", "scan", "-q"])
        .arg(tmp.path())
        .arg("-o")
        .arg(&b)
        .assert()
        .code(0);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    piiflow()
        .args(["--threads", "0", "scan"])
        .arg(tmp.path())
        .assert()
        .code(2);
}

#[test]
fn datalog_engine_agrees_through_the_cli() {
    let tmp = copy_fixture("ts/class-instance");
    piiflow()
        .args(["scan", "-q", "--engine", "datalog", "-f", "json"])
        .arg(tmp.path())
        .assert()
        .code(0);
}

/// Acceptance criterion: `explain` shows a complete cited chain for every finding. Every finding
/// of every fixture explains with exit 0; a flow finding prints one source line per hop, and a
/// coverage gap prints at least one cited location.
#[test]
fn explain_works_for_every_finding_of_every_fixture() {
    let mut explained = 0;
    for dir in common::fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let doc_path = tmp.path().join("flows.json");
        let status = Std::new(env!("CARGO_BIN_EXE_piiflow"))
            .args(["scan", "-q", "--walk", "-o"])
            .arg(&doc_path)
            .arg(&dir)
            .status()
            .unwrap();
        assert!(
            matches!(status.code(), Some(0 | 1 | 4)),
            "{}: scan failed",
            dir.display()
        );
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&doc_path).unwrap()).unwrap();
        let flows: std::collections::BTreeMap<&str, usize> = doc["flows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["id"].as_str().unwrap(),
                    f["path"].as_array().unwrap().len(),
                )
            })
            .collect();
        for finding in doc["findings"].as_array().unwrap() {
            let id = finding["id"].as_str().unwrap();
            let out = Std::new(env!("CARGO_BIN_EXE_piiflow"))
                .args(["explain", "-q", id, "-i"])
                .arg(&doc_path)
                .arg("--root")
                .arg(&dir)
                .output()
                .unwrap();
            let text = String::from_utf8(out.stdout).unwrap();
            assert_eq!(
                out.status.code(),
                Some(0),
                "{}: explain {id} failed:\n{}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            // Source lines are printed as `<indent><line number> | <code>`.
            let shown = text
                .lines()
                .filter(|l| {
                    l.trim_start()
                        .split_once(" | ")
                        .is_some_and(|(n, _)| n.parse::<u32>().is_ok())
                })
                .count();
            match finding["flow"].as_str() {
                Some(flow) => assert_eq!(
                    shown,
                    flows[flow],
                    "{}: {id} must show one source line per hop:\n{text}",
                    dir.display()
                ),
                None => assert!(
                    shown >= 1,
                    "{}: {id} must cite at least one location:\n{text}",
                    dir.display()
                ),
            }
            explained += 1;
        }
    }
    assert!(explained >= 41, "only {explained} findings explained");
}
