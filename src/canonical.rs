//! Canonical serialization: every JSON document `piiflow` writes is the RFC 8785 (JSON
//! Canonicalization Scheme) serialization of its value, and every digest is SHA-256 over
//! exactly those bytes. Any JCS implementation reproduces them.

use anyhow::Result;
use sha2::{Digest, Sha256};

/// The RFC 8785 serialization of `value`.
///
/// Every document `piiflow` writes has only integer numbers and ASCII object keys. For such a
/// value RFC 8785 is exactly: object members sorted by key, no whitespace, and strings escaped
/// as ECMAScript's `JSON.stringify` does (which serde_json's string escaping matches). That is
/// what [`write_simple`] does, about twenty times faster than a general canonicalizer on a large
/// document. Any value outside that case (a float, a non-ASCII key) goes through the general
/// implementation. A test compares the two.
pub fn jcs_bytes<T: serde::Serialize>(value: &T) -> Result<String> {
    let v = serde_json::to_value(value)?;
    if is_simple(&v) {
        let mut out = String::new();
        write_simple(&v, &mut out)?;
        Ok(out)
    } else {
        Ok(serde_json_canonicalizer::to_string(&v)?)
    }
}

fn is_simple(v: &serde_json::Value) -> bool {
    use serde_json::Value;
    match v {
        Value::Number(n) => n.is_i64() || n.is_u64(),
        Value::Array(a) => a.iter().all(is_simple),
        Value::Object(m) => m.iter().all(|(k, v)| k.is_ascii() && is_simple(v)),
        _ => true,
    }
}

fn write_simple(v: &serde_json::Value, out: &mut String) -> Result<()> {
    use serde_json::Value;
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s)?),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_simple(x, out)?;
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k)?);
                out.push(':');
                write_simple(&m[k], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
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
    fn fast_path_matches_the_general_canonicalizer() {
        let v = json!({
            "z": [1, -2, 9007199254740991u64, "é\u{1F600}\n\t\"\\\u{1}\u{7f}\u{2028}", null, true],
            "a": {"b": {}, "a": [], "_": "x"},
            "M": "Zürich",
            "": 0
        });
        assert!(is_simple(&v));
        let mut fast = String::new();
        write_simple(&v, &mut fast).unwrap();
        assert_eq!(fast, serde_json_canonicalizer::to_string(&v).unwrap());
        // A float or a non-ASCII key takes the general path.
        assert!(!is_simple(&json!({"x": 1.5})));
        assert!(!is_simple(&json!({"é": 1})));
        assert_eq!(
            jcs_bytes(&json!({"x": 1.5, "é": 1})).unwrap(),
            serde_json_canonicalizer::to_string(&json!({"x": 1.5, "é": 1})).unwrap()
        );
    }

    #[test]
    fn digest_is_over_jcs_bytes() {
        let v = json!({"x": 1});
        assert_eq!(digest(&v).unwrap(), sha256(br#"{"x":1}"#));
    }
}
