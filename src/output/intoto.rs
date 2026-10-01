//! An unsigned in-toto Statement v1 whose subject is the scanned commit and whose predicate is
//! the flow-facts document. Sign it with the DSSE signer you already use (cosign, GitHub
//! artifact attestations); `piiflow validate` checks a Statement's subject against its
//! predicate and re-validates the predicate.

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::report::Document;

pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
pub const PREDICATE_TYPE: &str = "https://noru.tech/spec/privacy-flow/flows/v0.1";
pub const SUBJECT_NAME: &str = "git:scanned-commit";

pub fn render(doc: &Document) -> Result<Value> {
    let Some(commit) = &doc.subject.commit else {
        bail!("an in-toto Statement needs a commit as its subject; scan inside a Git work tree");
    };
    Ok(json!({
        "_type": STATEMENT_TYPE,
        "subject": [{ "name": SUBJECT_NAME, "digest": { "gitCommit": commit } }],
        "predicateType": PREDICATE_TYPE,
        "predicate": doc
    }))
}

/// The predicate of a Statement, after checking the envelope fields and the subject.
pub fn unwrap(v: &Value) -> Result<Value> {
    if v.get("_type").and_then(Value::as_str) != Some(STATEMENT_TYPE) {
        bail!("not an in-toto Statement v1");
    }
    if v.get("predicateType").and_then(Value::as_str) != Some(PREDICATE_TYPE) {
        bail!("predicateType is not {PREDICATE_TYPE}");
    }
    let predicate = v.get("predicate").cloned().unwrap_or(Value::Null);
    let commit = predicate.pointer("/subject/commit").and_then(Value::as_str);
    let expected = commit.map(|c| json!([{ "name": SUBJECT_NAME, "digest": { "gitCommit": c } }]));
    if expected.as_ref() != v.get("subject") {
        bail!("the Statement's subject is not the predicate's scanned commit");
    }
    Ok(predicate)
}
