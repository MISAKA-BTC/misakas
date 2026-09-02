//! ADR-0078 Decision 2: a grammar's canonicalizer is "a pure function (whitespace, key order,
//! number form, nothing semantic)". This is the JSON half of that function, shared by every
//! JSON-shaped grammar: parse, refuse what has no canonical form, and re-emit with sorted keys,
//! no whitespace, and integers only.
//!
//! What is refused, by name: any non-integer number (`1.0`, `1e3`, a value outside `i64`/`u64`)
//! — a float has no integer transformer downstream and no canonical text, so it is not a
//! parse error but a *grammar* refusal (ADR-0078 X4: a parse failure yields no object);
//! duplicate keys (serde_json keeps the last, which is a semantic choice this layer must not
//! make, so a duplicate is rejected before the parser can choose); and lone surrogates (invalid
//! UTF-8 never reaches us because the input is `&[u8]` validated as UTF-8 first).
//!
//! The writer follows RFC 8785 for strings (escape `"`, `\\`, and control characters; `\b`
//! `\f` `\n` `\r` `\t` short forms; everything else `\u00XX`; non-ASCII emitted raw as UTF-8)
//! and emits integers in their shortest decimal form. Booleans and `null` are literal.

use crate::DeriveError;
use serde_json::Value;
use std::collections::BTreeMap;

/// A canonical JSON tree: integers only, keys sorted by their UTF-8 bytes (the order
/// `BTreeMap<String, _>` gives), no floats anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonValue {
    Null,
    Bool(bool),
    /// Every integer the input can carry, widened; a `u64` above `i64::MAX` keeps its value.
    Int(i128),
    Str(String),
    Arr(Vec<CanonValue>),
    Obj(BTreeMap<String, CanonValue>),
}

impl CanonValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            CanonValue::Int(v) => i64::try_from(*v).ok(),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            CanonValue::Int(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            CanonValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[CanonValue]> {
        match self {
            CanonValue::Arr(a) => Some(a.as_slice()),
            _ => None,
        }
    }
    pub fn as_obj(&self) -> Option<&BTreeMap<String, CanonValue>> {
        match self {
            CanonValue::Obj(o) => Some(o),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            CanonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// `obj[key]`, or `None` when this is not an object or has no such key.
    pub fn get(&self, key: &str) -> Option<&CanonValue> {
        self.as_obj().and_then(|o| o.get(key))
    }
}

/// Parse `input` (UTF-8 JSON) into a canonical tree, refusing what has no canonical form.
pub fn parse_canonical(input: &[u8]) -> Result<CanonValue, DeriveError> {
    let text = std::str::from_utf8(input).map_err(|_| DeriveError::Grammar("input is not UTF-8".into()))?;
    reject_duplicate_keys(text)?;
    let v: Value = serde_json::from_str(text).map_err(|e| DeriveError::Grammar(format!("json: {e}")))?;
    convert(&v)
}

fn convert(v: &Value) -> Result<CanonValue, DeriveError> {
    Ok(match v {
        Value::Null => CanonValue::Null,
        Value::Bool(b) => CanonValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CanonValue::Int(i as i128)
            } else if let Some(u) = n.as_u64() {
                CanonValue::Int(u as i128)
            } else {
                return Err(DeriveError::Grammar(format!("non-integer number {n} has no canonical form")));
            }
        }
        Value::String(s) => CanonValue::Str(s.clone()),
        Value::Array(a) => CanonValue::Arr(a.iter().map(convert).collect::<Result<_, _>>()?),
        Value::Object(o) => {
            let mut m = BTreeMap::new();
            for (k, val) in o {
                m.insert(k.clone(), convert(val)?);
            }
            CanonValue::Obj(m)
        }
    })
}

/// serde_json silently keeps the LAST of two equal keys. Canonicalization must not choose, so
/// a duplicate key inside any one object is refused here with a scan that tracks nesting and
/// string state — enough to find `{"a":1,"a":2}` at any depth without a second parser.
fn reject_duplicate_keys(text: &str) -> Result<(), DeriveError> {
    #[derive(Default)]
    struct Frame {
        is_object: bool,
        keys: Vec<String>,
        expect_key: bool,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'"' => {
                // read a string literal
                let start = i + 1;
                let mut j = start;
                let mut escaped = false;
                while j < bytes.len() {
                    let d = bytes[j];
                    if escaped {
                        escaped = false;
                    } else if d == b'\\' {
                        escaped = true;
                    } else if d == b'"' {
                        break;
                    }
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(DeriveError::Grammar("unterminated string".into()));
                }
                let raw = &text[start..j];
                if let Some(top) = stack.last_mut() {
                    if top.is_object && top.expect_key {
                        // The raw (still-escaped) key text is compared: two keys that differ only
                        // in escaping (`"a"` vs `"a"`) are decoded by serde before they
                        // collide, so decode here too for the comparison.
                        let decoded: String = serde_json::from_str(&format!("\"{raw}\"")).unwrap_or_else(|_| raw.to_string());
                        if top.keys.contains(&decoded) {
                            return Err(DeriveError::Grammar(format!("duplicate key {decoded:?}")));
                        }
                        top.keys.push(decoded);
                        top.expect_key = false;
                    }
                }
                i = j + 1;
                continue;
            }
            b'{' => stack.push(Frame { is_object: true, keys: Vec::new(), expect_key: true }),
            b'[' => stack.push(Frame { is_object: false, keys: Vec::new(), expect_key: false }),
            b'}' | b']' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    if top.is_object {
                        top.expect_key = true;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Emit the canonical bytes of `v`.
pub fn write_canonical(v: &CanonValue) -> Vec<u8> {
    let mut out = Vec::new();
    write_into(v, &mut out);
    out
}

fn write_into(v: &CanonValue, out: &mut Vec<u8>) {
    match v {
        CanonValue::Null => out.extend_from_slice(b"null"),
        CanonValue::Bool(true) => out.extend_from_slice(b"true"),
        CanonValue::Bool(false) => out.extend_from_slice(b"false"),
        CanonValue::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        CanonValue::Str(s) => write_string(s, out),
        CanonValue::Arr(a) => {
            out.push(b'[');
            for (n, item) in a.iter().enumerate() {
                if n > 0 {
                    out.push(b',');
                }
                write_into(item, out);
            }
            out.push(b']');
        }
        CanonValue::Obj(o) => {
            out.push(b'{');
            for (n, (k, val)) in o.iter().enumerate() {
                if n > 0 {
                    out.push(b',');
                }
                write_string(k, out);
                out.push(b':');
                write_into(val, out);
            }
            out.push(b'}');
        }
    }
}

/// RFC 8785 §3.2.2.2 string serialization.
pub fn write_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{8}' => out.extend_from_slice(b"\\b"),
            '\u{c}' => out.extend_from_slice(b"\\f"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            c if (c as u32) < 0x20 => out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes()),
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// Parse and re-emit: the whole canonicalizer for a JSON grammar that imposes no schema.
pub fn canonicalize_json(input: &[u8]) -> Result<Vec<u8>, DeriveError> {
    Ok(write_canonical(&parse_canonical(input)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_keys_strips_whitespace_and_keeps_integers() {
        let out = canonicalize_json(br#" { "b" : [1, 2 ,3], "a": {"z":null,"y":true}, "c": -7 } "#).unwrap();
        assert_eq!(out, br#"{"a":{"y":true,"z":null},"b":[1,2,3],"c":-7}"#);
    }

    #[test]
    fn refuses_floats_and_exponents() {
        assert!(canonicalize_json(br#"{"a":1.0}"#).is_err());
        assert!(canonicalize_json(br#"{"a":1e3}"#).is_err());
        assert!(canonicalize_json(br#"[0.5]"#).is_err());
    }

    #[test]
    fn keeps_u64_above_i64_max() {
        let out = canonicalize_json(b"[18446744073709551615]").unwrap();
        assert_eq!(out, b"[18446744073709551615]");
    }

    #[test]
    fn refuses_duplicate_keys_at_any_depth() {
        assert!(canonicalize_json(br#"{"a":1,"a":2}"#).is_err());
        assert!(canonicalize_json(br#"{"x":[{"k":1,"k":1}]}"#).is_err());
        assert!(canonicalize_json(br#"{"a":{"b":1},"c":{"b":2}}"#).is_ok());
        assert!(canonicalize_json(br#"{"a":"b","b":"a"}"#).is_ok());
        assert!(canonicalize_json(br#"{"a":1,"a":2}"#).is_err());
    }

    #[test]
    fn strings_follow_rfc8785() {
        let out = canonicalize_json("[\"q\\\"b\\\\\\u0001\\n日本\"]".as_bytes()).unwrap();
        assert_eq!(out, "[\"q\\\"b\\\\\\u0001\\n日本\"]".as_bytes());
    }

    #[test]
    fn idempotent() {
        let once = canonicalize_json(br#"{"b":1,"a":[{"d":2,"c":3}]}"#).unwrap();
        let twice = canonicalize_json(&once).unwrap();
        assert_eq!(once, twice);
    }
}
