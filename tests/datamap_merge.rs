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

#[test]
fn privacy_datamap_decisions_win() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".noru")).unwrap();
    std::fs::write(
        d.join(".noru/privacy-datamap.yml"),
        "version: 0.1.0\npiece: privacy-datamap\ndataset:\n  - fides_key: app_postgres\n    collections:\n      - name: members\n        interpretation: { owner: Dana Okafor, decided_at: \"2026-08-20\", expires_at: \"2027-02-20\" }\n        non_personal_fields: [phone]\n        fields:\n          - name: email_address\n            data_categories: [user.contact.email]\n            refs: [\"db/0001.sql:4\"]\nsystem:\n  - fides_key: apps_api\n    privacy_declarations:\n      - name: Operate the account\n        data_use: essential.service\n        data_subjects: [customer]\n        data_categories: [user.contact.email]\n        interpretation: { owner: Dana Okafor, decided_at: \"2026-08-20\" }\n",
    )
    .unwrap();
    assert_eq!(datamap(d, &[]), 0);
    let text = map(d);
    assert!(!text.contains("user.contact.phone_number"), "{text}");
    let api = system(&text, "shop_api");
    assert!(api.contains("name: \"Operate the account\""), "{api}");
    assert!(
        !text.contains("refs:") && !text.contains("interpretation"),
        "{text}"
    );
    // An edit of a decided value does not stick: the manifest decides.
    edit(
        &d.join(".fides/datamap.yml"),
        "        data_use: \"essential.service\"\n        data_subjects:\n          - \"customer\"\n        data_categories:\n          - \"user.contact.email\"\n      ",
        "        data_use: \"marketing.communications\"\n        data_subjects:\n          - \"customer\"\n        data_categories:\n          - \"user.contact.email\"\n      ",
    );
    assert_eq!(datamap(d, &[]), 0);
    assert!(!system(&map(d), "shop_api").contains("marketing.communications"));
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn incomplete_declarations_are_proposals_not_declarations() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".noru")).unwrap();
    std::fs::write(
        d.join(".noru/privacy-datamap.yml"),
        "version: 0.1.0\npiece: privacy-datamap\nsystem:\n  - fides_key: crm\n    name: CRM\n    system_type: Service\n    privacy_declarations:\n      - name: \"\"\n        data_use: \"\"\n        data_categories: [user.financial.credit_card]\n      - name: Sync contacts\n        data_use: essential.service\n        data_categories: [user.contact.email]\n        needs_review: true\n",
    )
    .unwrap();
    assert_eq!(datamap(d, &[]), 0);
    let crm = system(&map(d), "crm").to_string();
    let (meta, decls) = crm.split_once("privacy_declarations:").unwrap();
    assert!(
        meta.contains("proposals:") && meta.contains("user.financial.credit_card"),
        "{crm}"
    );
    assert!(!decls.contains("name: \"\""), "{crm}");
    // A manifest declaration nobody decided stays under review.
    assert!(decls.contains("Sync contacts"), "{crm}");
    assert!(
        meta.contains("needs_review:\n          - \"Sync contacts\""),
        "{crm}"
    );
    assert_eq!(datamap(d, &["--check"]), 0);

    // A data use a person emptied: a proposal, kept across runs.
    edit(
        &d.join(".fides/datamap.yml"),
        "data_use: \"analytics.reporting\"",
        "data_use: \"\"",
    );
    assert_eq!(datamap(d, &[]), 0);
    let api = system(&map(d), "shop_api").to_string();
    let (meta, decls) = api.split_once("privacy_declarations:").unwrap();
    assert!(meta.contains("INCOMPLETE: no data use"), "{api}");
    assert!(!decls.contains("Analytics for Reporting"), "{api}");
    let text = map(d);
    assert_eq!(datamap(d, &[]), 0);
    assert_eq!(map(d), text);
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn a_map_that_would_not_be_valid_fideslang_is_not_written() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    assert_eq!(datamap(d, &[]), 0);
    let m = d.join(".fides/datamap.yml");
    edit(&m, "    system_type: \"Service\"\n", "");
    let edited = map(d);
    let out = Command::new(env!("CARGO_BIN_EXE_piiflow"))
        .args(["datamap", "--walk"])
        .arg(d)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("would not be valid Fideslang") && err.contains("system_type is required"),
        "{err}"
    );
    assert_eq!(map(d), edited);
}

const MANIFEST_API: &str = "version: 0.1.0\npiece: privacy-datamap\nsystem:\n  - fides_key: apps_api\n    name: API\n    system_type: Service\n    privacy_declarations:\n      - name: Operate the account\n        data_use: essential.service\n        data_subjects: [customer]\n        data_categories: [user.contact.email]\n        interpretation: { owner: Dana Okafor, decided_at: \"2026-08-20\" }\n";

/// How many elements of `key` (systems or datasets) a rendered map defines.
fn definitions(text: &str, key: &str) -> usize {
    let line = format!("  - fides_key: \"{key}\"");
    text.lines().filter(|l| *l == line).count()
}

#[test]
fn a_system_the_manifest_and_the_code_both_have_is_defined_once() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".noru")).unwrap();
    std::fs::write(d.join(".noru/privacy-datamap.yml"), MANIFEST_API).unwrap();
    assert_eq!(datamap(d, &[]), 0);
    assert_eq!(definitions(&map(d), "shop_api"), 1, "{}", map(d));
    assert_eq!(definitions(&map(d), "apps_api"), 0, "{}", map(d));
    // A person adopts the manifest's key for the system the code found.
    let m = d.join(".fides/datamap.yml");
    std::fs::write(&m, map(d).replace("\"shop_api\"", "\"apps_api\"")).unwrap();
    assert_eq!(datamap(d, &[]), 0);
    let text = map(d);
    assert_eq!(definitions(&text, "apps_api"), 1, "{text}");
    let api = system(&text, "apps_api");
    // The code's findings and the manifest's decision, in one definition.
    assert!(api.contains("- \"shop_db\""), "{api}");
    assert!(api.contains("fides_key: \"posthog\""), "{api}");
    assert_eq!(
        api.matches("name: \"Operate the account\"").count(),
        1,
        "{api}"
    );
    // Repeated runs: the same map, up to date.
    assert_eq!(datamap(d, &[]), 0);
    assert_eq!(map(d), text);
    assert_eq!(datamap(d, &["--check"]), 0);
}

#[test]
fn a_map_with_a_system_defined_twice_is_merged_into_one_without_losing_either() {
    let tmp = copy_fixture("ts/multi-service");
    let d = tmp.path();
    std::fs::create_dir_all(d.join(".noru")).unwrap();
    std::fs::write(d.join(".noru/privacy-datamap.yml"), MANIFEST_API).unwrap();
    assert_eq!(datamap(d, &[]), 0);
    let m = d.join(".fides/datamap.yml");
    std::fs::write(&m, map(d).replace("\"shop_api\"", "\"apps_api\"")).unwrap();
    // A second definition of the system, as earlier versions wrote from the manifest, with a
    // description and a category of its PostHog egress the first does not have.
    edit(
        &m,
        "\nsystem:\n",
        concat!(
            "\nsystem:\n",
            "  - fides_key: \"apps_api\"\n",
            "    name: \"API\"\n",
            "    description: \"Accounts and orders\"\n",
            "    system_type: \"Service\"\n",
            "    dataset_references:\n      - \"shop_db\"\n",
            "    egress:\n",
            "      - fides_key: \"posthog\"\n        type: \"system\"\n",
            "        data_categories:\n          - \"user.device.ip_address\"\n",
            "    privacy_declarations:\n",
            "      - name: \"Operate the account\"\n        data_use: \"essential.service\"\n",
            "        data_categories:\n          - \"user.contact.email\"\n",
        ),
    );
    assert_eq!(definitions(&map(d), "apps_api"), 2);
    assert_eq!(datamap(d, &["--check"]), 5);
    assert_eq!(datamap(d, &[]), 0);
    let text = map(d);
    assert_eq!(definitions(&text, "apps_api"), 1, "{text}");
    let api = system(&text, "apps_api");
    assert!(
        api.contains("description: \"Accounts and orders\""),
        "{api}"
    );
    let egress = &api[api.find("    egress:").unwrap()..api.find("    ingress:").unwrap()];
    assert_eq!(egress.matches("fides_key: \"posthog\"").count(), 1, "{api}");
    assert!(
        egress.contains("\"user.contact.email\"") && egress.contains("\"user.device.ip_address\""),
        "{api}"
    );
    assert!(api.contains("name: \"Analytics for Reporting\""), "{api}");
    assert_eq!(
        api.matches("name: \"Operate the account\"").count(),
        1,
        "{api}"
    );
    assert_eq!(datamap(d, &[]), 0);
    assert_eq!(map(d), text);
    assert_eq!(datamap(d, &["--check"]), 0);
}
