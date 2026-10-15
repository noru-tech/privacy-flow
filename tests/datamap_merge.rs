//! `piiflow datamap` on a copy of a fixture: re-runs merge into the map a person keeps, and
//! `--check` tells when it is out of date (exit 5).

mod common;

use std::path::Path;
use std::process::Command;

fn copy_fixture(name: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let from = common::fixtures_root().join(name);
    for e in walkdir::WalkDir::new(&from) {
        let e = e.unwrap();
        let dst = tmp.path().join(e.path().strip_prefix(&from).unwrap());
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dst).unwrap();
        } else {
            std::fs::copy(e.path(), &dst).unwrap();
        }
    }
    tmp
}

/// Run `piiflow -q datamap --walk <dir> <args>`; the exit code.
fn datamap(dir: &Path, args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_piiflow"))
        .args(["-q", "datamap", "--walk"])
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

fn map(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(".fides/datamap.yml")).unwrap()
}

fn edit(path: &Path, from: &str, to: &str) {
    let s = std::fs::read_to_string(path).unwrap();
    assert!(
        s.contains(from),
        "{} does not contain {from:?}",
        path.display()
    );
    std::fs::write(path, s.replacen(from, to, 1)).unwrap();
}

/// The system entry of `key` in a rendered map.
fn system<'a>(text: &'a str, key: &str) -> &'a str {
    let start = text
        .find(&format!("\n  - fides_key: \"{key}\""))
        .unwrap_or_else(|| panic!("no system {key}"));
    let rest = &text[start + 5..];
    let end = rest.find("\n  - fides_key: ").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn a_second_run_changes_nothing_and_check_passes() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    let first = map(d);
    let lock = std::fs::read_to_string(d.join(".fides/datamap.lock.json")).unwrap();
    assert_eq!(datamap(d, &["--check"]), 0);
    assert_eq!(datamap(d, &[]), 0);
    assert_eq!(map(d), first);
    assert_eq!(
        std::fs::read_to_string(d.join(".fides/datamap.lock.json")).unwrap(),
        lock
    );
}

#[test]
fn check_fails_without_a_map_and_writes_nothing() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &["--check"]), 5);
    assert!(!d.join(".fides").exists());
}

#[test]
fn edits_survive_and_the_code_still_updates_the_rest() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    let m = d.join(".fides/datamap.yml");
    // A person adds a field Fides knows, and deletes the PostHog egress of the API.
    edit(
        &m,
        "    name: \"@shop/api\"\n",
        "    name: \"@shop/api\"\n    dpo: \"privacy@example.com\"\n",
    );
    let text = map(d);
    let api = system(&text, "shop_api");
    let posthog = api.find("      - fides_key: \"posthog\"").unwrap();
    let cut_end = api[posthog..].find("    ingress:").unwrap() + posthog;
    let removed = &api[posthog..cut_end];
    std::fs::write(&m, text.replacen(removed, "", 1)).unwrap();
    assert_eq!(datamap(d, &["--check"]), 5);
    // The code changes: the web app reports errors to Sentry.
    edit(
        &d.join("apps/web/app/api/subscribe/route.ts"),
        "import axios from 'axios';",
        "import axios from 'axios';\nimport * as Sentry from '@sentry/nextjs';",
    );
    edit(
        &d.join("apps/web/app/api/subscribe/route.ts"),
        "  await api.post(",
        "  Sentry.setUser({ email: body.email }); // expect: PF002\n  await api.post(",
    );
    assert_eq!(datamap(d, &[]), 0);
    let text = map(d);
    let api = system(&text, "shop_api");
    assert!(api.contains("dpo: \"privacy@example.com\""), "{api}");
    assert!(!api.contains("      - fides_key: \"posthog\""), "{api}");
    assert!(
        system(&text, "shop_web").contains("fides_key: \"sentry\""),
        "{text}"
    );
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn what_the_code_no_longer_shows_goes() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    assert!(map(d).contains("\"posthog\""));
    edit(
        &d.join("packages/shared/src/index.ts"),
        "  posthog.capture({ distinctId: email, event: 'signup' }); // expect: PF002\n",
        "",
    );
    assert_eq!(datamap(d, &[]), 0);
    assert!(!map(d).contains("posthog"), "{}", map(d));
}

#[test]
fn without_a_lock_the_file_is_kept_and_check_fails() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    std::fs::remove_file(d.join(".fides/datamap.lock.json")).unwrap();
    edit(
        &d.join(".fides/datamap.yml"),
        "    name: \"@shop/web\"\n",
        "    name: \"Storefront\"\n",
    );
    assert_eq!(datamap(d, &["--check"]), 5);
    assert_eq!(datamap(d, &[]), 0);
    assert!(map(d).contains("name: \"Storefront\""));
    assert!(d.join(".fides/datamap.lock.json").is_file());
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn comment_lines_and_line_endings_are_not_drift() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    let m = d.join(".fides/datamap.yml");
    let text = map(d)
        .replacen("#: ", "#: (moved) ", 1)
        .replace('\n', "\r\n");
    std::fs::write(&m, text).unwrap();
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn an_unreadable_map_is_invalid_input_and_nothing_is_written() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".fides")).unwrap();
    std::fs::write(d.join(".fides/datamap.yml"), "system: [\n").unwrap();
    assert_eq!(datamap(d, &[]), 3);
    assert_eq!(map(d), "system: [\n");
    assert!(!d.join(".fides/datamap.lock.json").exists());
}
