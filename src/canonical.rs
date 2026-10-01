//! Canonical serialization: every JSON document `piiflow` writes is the RFC 8785 (JSON
//! Canonicalization Scheme) serialization of its value, and every digest is SHA-256 over
//! exactly those bytes. Any JCS implementation reproduces them.

use anyhow::Result;
use sha2::{Digest, Sha256};

/// The RFC 8785 serialization of `value`.
pub fn jcs_bytes<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json_canonicalizer::to_string(value)?)
}

/// `sha256:<hex>` over [`jcs_bytes`].
pub fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(sha256(jcs_bytes(value)?.as_bytes()))
}

/// `sha256:<hex>` over raw bytes.
pub fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity("sha256:".len() + 2 * digest.len());
    out.push_str("sha256:");
    for b in digest {
        write!(out, "{b:02x}").expect("writing to a String cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rfc8785_ordering_and_numbers() {
        let v = json!({"b": 1, "a": [true, null, "é"], "c": 1.0e2});
        assert_eq!(
            jcs_bytes(&v).unwrap(),
            r#"{"a":[true,null,"é"],"b":1,"c":100}"#
        );
    }

    #[test]
    fn digest_is_over_jcs_bytes() {
        let v = json!({"x": 1});
        assert_eq!(digest(&v).unwrap(), sha256(br#"{"x":1}"#));
    }
}
