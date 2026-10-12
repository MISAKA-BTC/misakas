//! **RFC 8785 — JSON Canonicalization Scheme (JCS), over `serde_json::Value`.**
//!
//! One byte string per JSON value, reproducible by any implementation of the RFC:
//!
//! * no insignificant whitespace;
//! * object members sorted by their names as sequences of UTF-16 code units (RFC 8785 §3.2.3) —
//!   NOT by code point and NOT by UTF-8 bytes, which is the one place the three orders disagree
//!   (a supplementary-plane character such as U+1F602 sorts BEFORE U+FB33 under UTF-16 because its
//!   surrogates are `D83D DE02`);
//! * strings escaped as §3.2.2.2 says: `\"`, `\\`, the five short escapes `\b \t \n \f \r`,
//!   `\u00xx` (lowercase hex) for the remaining control characters below U+0020, and every other
//!   character — U+007F, U+2028, `/`, the lot — raw UTF-8;
//! * numbers in ECMAScript `Number::toString` form (§3.2.2.3): the shortest digit string that
//!   round-trips, laid out as the ES algorithm lays it out (`1e+21`, `0.000001`,
//!   `9.999999999999997e-7`, `-0` as `0`), with NaN and the infinities refused because JSON has no
//!   spelling for them.
//!
//! **Why this is written here and not reached through `serde_json::to_vec`.** serde_json's
//! compact writer sorts nothing (its map is a `BTreeMap` today and an `IndexMap` under the
//! `preserve_order` feature, which any crate in a build may switch on), prints `1e21` as
//! `1000000000000000000000`, and prints `1.0` as `1.0`. All three are fine for a wire format and
//! wrong for a byte string two trees must agree on before hashing it.
//!
//! **Integers beyond 2^53.** RFC 8785 canonicalizes I-JSON, whose numbers are IEEE 754 doubles.
//! serde_json parses `18446744073709551615` exactly as a `u64`; this module renders it the way an
//! ECMAScript engine that parsed the same text would — as the nearest double,
//! `18446744073709552000` — because that is the canonical form the RFC defines, and a constraint
//! that needed the exact integer would need a schema keyword the subset does not carry anyway.

use std::fmt::Write as _;

use serde_json::Value;

/// The canonical bytes of `value`, or the reason there are none (a non-finite number).
pub fn to_rfc8785(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    write_value(&mut out, value)?;
    Ok(out)
}

fn write_value(out: &mut Vec<u8>, value: &Value) -> Result<(), String> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            let f = n.as_f64().ok_or_else(|| format!("the number {n} has no IEEE double value"))?;
            if !f.is_finite() {
                return Err(format!("the number {n} is not finite; JSON has no spelling for it (RFC 8785 §3.2.2.3)"));
            }
            out.extend_from_slice(es_number_to_string(f).as_bytes());
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_value(out, item)?;
            }
            out.push(b']');
        }
        Value::Object(members) => {
            // Sorted by UTF-16 code units, whatever order the map holds them in.
            let mut keys: Vec<&String> = members.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push(b'{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_string(out, key);
                out.push(b':');
                write_value(out, &members[key.as_str()])?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

/// RFC 8785 §3.2.2.2.
fn write_string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{8}' => out.extend_from_slice(b"\\b"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\u{c}' => out.extend_from_slice(b"\\f"),
            '\r' => out.extend_from_slice(b"\\r"),
            c if (c as u32) < 0x20 => {
                let mut escaped = String::with_capacity(6);
                write!(escaped, "\\u{:04x}", c as u32).expect("writing to a String cannot fail");
                out.extend_from_slice(escaped.as_bytes());
            }
            c => out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes()),
        }
    }
    out.push(b'"');
}

/// **ECMAScript `Number::toString(x)` for a finite double** (ECMA-262 §6.1.6.1.20, which RFC 8785
/// §3.2.2.3 adopts).
///
/// The ES rule picks the shortest digit string that round-trips and, when several strings of
/// that length do, the one CLOSEST to the value — and on an exact tie, the one whose last digit
/// is even. Rust's `{:e}` gives the shortest length, but not that tie rule: `1424953923781206.25`
/// (RFC 8785's `0x43143ff3c1cb0959`) is exactly halfway between `…206.2` and `…206.3`, and `{:e}`
/// rounds it up where ES rounds it to even. Rust's fixed-precision formatting DOES round half to
/// even, so the digits are taken from a second render at exactly the shortest length, kept only
/// if it still round-trips (at a power-of-two boundary the rounding interval is lopsided and the
/// nearest string can fall outside it; the shortest render is then the correct one). What the
/// rest of this function adds is the LAYOUT: where the decimal point goes, when the exponent form
/// is used (below 10^-6 and at 10^21 and above), and how the exponent is spelled (`e+21`, `e-7`,
/// never `e21`). The RFC's own vectors pin every branch below.
pub fn es_number_to_string(x: f64) -> String {
    debug_assert!(x.is_finite());
    if x == 0.0 {
        return "0".to_string(); // covers -0: ES prints it as "0"
    }
    let magnitude = x.abs();
    let (digits, exponent) = shortest_closest_digits(magnitude);
    let k = digits.len() as i32;
    let n = exponent + 1;
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(&digits);
        for _ in 0..(n - k) {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        let e = n - 1;
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if e >= 0 { '+' } else { '-' });
        out.push_str(&e.abs().to_string());
    }
    out
}

/// Split a `d.ddde±N` rendering into its digit string (no point, no trailing zeros) and the
/// exponent of its first digit.
fn split_scientific(rendered: &str) -> (String, i32) {
    let (mantissa, exponent) = rendered.split_once('e').expect("`{:e}` always carries an exponent");
    let exponent: i32 = exponent.parse().expect("`{:e}` writes a decimal exponent");
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    (digits, exponent)
}

/// The ES digit string for a positive finite double: shortest, then closest, then even.
fn shortest_closest_digits(magnitude: f64) -> (String, i32) {
    let shortest = format!("{magnitude:e}");
    let (digits, exponent) = split_scientific(&shortest);
    let k = digits.len();
    // The same length, rendered by the exact (round-half-even) path.
    let nearest = format!("{magnitude:.*e}", k - 1);
    if nearest.parse::<f64>() == Ok(magnitude) {
        let (nearest_digits, nearest_exponent) = split_scientific(&nearest);
        if nearest_digits.len() <= k {
            return (nearest_digits, nearest_exponent);
        }
    }
    (digits, exponent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(text: &str) -> String {
        let value: Value = serde_json::from_str(text).unwrap();
        String::from_utf8(to_rfc8785(&value).unwrap()).unwrap()
    }

    /// **RFC 8785 §3.2.3's worked example**, byte for byte: numbers to ES form, escapes reduced
    /// to the canonical ones, members sorted, whitespace gone.
    #[test]
    fn the_rfcs_own_example_canonicalizes_to_the_rfcs_own_answer() {
        let input = r#"{
          "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
          "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
          "literals": [null, true, false]
        }"#;
        assert_eq!(
            canon(input),
            "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"€$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}"
        );
    }

    /// **RFC 8785 §3.2.3's sorting example**: names sort as UTF-16 code units, so the emoji
    /// (surrogates `D83D DE02`) lands before U+FB33 — the order code points and UTF-8 bytes both
    /// get wrong. The non-ASCII names come out raw, exactly as the RFC's note says they must.
    #[test]
    fn member_names_sort_by_utf16_code_units() {
        let input = r#"{
          "\u20ac": "Euro Sign",
          "\r": "Carriage Return",
          "\u000a": "Newline",
          "1": "One",
          "\u0080": "Control\u007f",
          "\ud83d\ude02": "Smiley",
          "\u00f6": "Latin Small Letter O With Diaeresis",
          "\ufb33": "Hebrew Letter Dalet With Dagesh",
          "</script>": "Browser Challenge"
        }"#;
        let out = canon(input);
        let reparsed: Value = serde_json::from_str(&out).unwrap();
        let keys: Vec<&String> = reparsed.as_object().unwrap().keys().collect();
        // serde_json's map re-sorts by UTF-8 bytes on parse, so the ORDER is read off the bytes.
        let order: Vec<usize> = ["\n", "\r", "1", "</script>", "\u{80}", "ö", "€", "😂", "\u{fb33}"]
            .iter()
            .map(|k| {
                out.find(&format!("{}:", String::from_utf8(to_rfc8785(&Value::String(k.to_string())).unwrap()).unwrap())).unwrap()
            })
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "the members appear in UTF-16 order: {out}");
        assert_eq!(keys.len(), 9);
        assert!(out.contains("\"\u{80}\":\"Control\u{7f}\""), "U+0080 and U+007F are raw, not escaped: {out}");
        assert!(
            out.starts_with("{\"\\n\":\"Newline\",\"\\r\":\"Carriage Return\",\"1\":\"One\",\"</script>\":\"Browser Challenge\",")
        );
    }

    /// **RFC 8785 Appendix B's number vectors** — the doubles by their bit patterns, the strings
    /// by the RFC. Each one pins a branch of the ES layout: subnormal, the largest double, the
    /// 2^53 boundary, the 10^21 switch to exponent form, the 10^-6 switch, and the shortest-digit
    /// choice on three neighbours of a third.
    #[test]
    fn the_rfcs_number_vectors_render_in_ecmascript_form() {
        let vectors: [(u64, &str); 17] = [
            (0x0000000000000000, "0"),
            (0x8000000000000000, "0"),
            (0x0000000000000001, "5e-324"),
            (0x7fefffffffffffff, "1.7976931348623157e+308"),
            (0x4340000000000000, "9007199254740992"),
            (0xc340000000000000, "-9007199254740992"),
            (0x4430000000000000, "295147905179352830000"),
            (0x44b52d02c7e14af6, "1e+23"),
            (0x444b1ae4d6e2ef4f, "999999999999999900000"),
            (0x444b1ae4d6e2ef50, "1e+21"),
            (0x3eb0c6f7a0b5ed8c, "9.999999999999997e-7"),
            (0x3eb0c6f7a0b5ed8d, "0.000001"),
            (0x41b3de4355555553, "333333333.3333332"),
            (0x41b3de4355555554, "333333333.33333325"),
            (0x41b3de4355555555, "333333333.3333333"),
            (0xbecbf647612f3696, "-0.0000033333333333333333"),
            (0x43143ff3c1cb0959, "1424953923781206.2"),
        ];
        for (bits, expected) in vectors {
            let x = f64::from_bits(bits);
            assert_eq!(es_number_to_string(x), expected, "bits {bits:#018x}");
            // And the layout round-trips: parsing the string gives the same double back.
            assert_eq!(expected.parse::<f64>().unwrap().to_bits(), if bits == 0x8000000000000000 { 0 } else { bits }, "{expected}");
        }
        // Integers that serde_json holds exactly render as ES would render the parsed double.
        assert_eq!(canon("[1, -1, 1.0, 10, 1e2, 123456789012345680000]"), "[1,-1,1,10,100,123456789012345680000]");
        assert_eq!(canon("18446744073709551615"), "18446744073709552000", "a u64 past 2^53 is the nearest double, as I-JSON reads it");
    }

    /// Strings: the five short escapes, `\u00xx` in lowercase for the rest of C0, and nothing
    /// else escaped — not `/`, not DEL, not U+2028.
    #[test]
    fn strings_use_exactly_the_rfcs_escapes() {
        assert_eq!(canon(r#""a\u0008b\tc\nd\u000ce\rf""#), "\"a\\bb\\tc\\nd\\fe\\rf\"");
        assert_eq!(canon(r#""\u0001\u001F""#), "\"\\u0001\\u001f\"");
        assert_eq!(canon(r#""\/\u007f\u2028""#), "\"/\u{7f}\u{2028}\"");
        assert_eq!(canon(r#""quote\" back\\slash""#), "\"quote\\\" back\\\\slash\"");
    }

    /// Whitespace and nesting: nothing between tokens, arrays in order, objects sorted at every
    /// depth, and the empty forms.
    #[test]
    fn whitespace_is_gone_and_nesting_is_sorted_at_every_depth() {
        assert_eq!(
            canon(r#"{ "b" : [ { "y" : 1 , "x" : [ ] } , { } ] , "a" : { "d" : null , "c" : true } }"#),
            r#"{"a":{"c":true,"d":null},"b":[{"x":[],"y":1},{}]}"#
        );
        assert_eq!(canon("[]"), "[]");
        assert_eq!(canon("{}"), "{}");
        assert_eq!(canon(r#""""#), "\"\"");
        // Idempotent: canonical bytes canonicalize to themselves.
        let once = canon(r#"{"z":[1.50,{"b":2,"a":"é"}],"a":-0.0}"#);
        assert_eq!(canon(&once), once);
        assert_eq!(once, r#"{"a":0,"z":[1.5,{"a":"é","b":2}]}"#);
    }
}
