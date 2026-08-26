//! Known-Answer Tests for the ADR-0040 primitives — the class's arithmetic in a form that is not
//! Rust.
//!
//! # Why the differential is not enough
//!
//! ADR-0040 registers a class only after "two independent implementations agree". Two exist:
//! `misaka-palw-base0-ref2`'s structural re-derivation and the vendored gemmlowp. Both agree
//! through `misaka-palw-base0-ref2/tests/differential.rs`, and that test compares **Rust to
//! Rust**. So the standard the ADR sets can currently be met only by someone who reads this
//! repository's Rust, and a third party who does that is not independent of it — the misreading
//! risk the second implementation exists to catch is exactly the risk of reading someone else's
//! code for the answer.
//!
//! A KAT set closes that. The vectors are `(op, arguments, output)` triples and nothing else: an
//! implementer in any language runs them, and a disagreement names the op and the input rather
//! than producing a diverging inference nobody can localise.
//!
//! # Why the vectors are enumerated, not sampled
//!
//! There is no RNG here. Every vector comes from one of three sources, each with a reason:
//!
//! * **Boundary tables** — type limits, powers of two and their neighbours, both signs. Both
//!   defects the second implementation found were negative-input-only and both survived tests
//!   that used positive values.
//! * **Exhaustive small windows** — the regions where the algorithms change behaviour: `IntExp`'s
//!   range-reduction buckets, `IntRsqrt`'s seed basin, the small `v` where `IntRecip`'s `r * r`
//!   overflowed `i64` for every input in `1..=511`.
//! * **Named regressions** — the specific inputs on which a real defect was found. They are
//!   listed in [`REGRESSIONS`] with what each one caught.
//!
//! A seeded RNG would produce a reproducible set too, but "seed `0x5EED_0006`" is not a reason,
//! and a third party cannot tell a deliberately-chosen vector from an accidental one.
//!
//! # The digest is what makes this an artifact
//!
//! [`digest`] hashes a canonical binary encoding of every group in order — not the JSON, whose
//! whitespace is a formatting decision. [`KAT_DIGEST`] pins it. Changing any vector or any
//! output changes the digest and fails the test that pins it, which is the point: the KAT set is
//! a frozen statement about the class, so it must not be possible to edit an answer to match a
//! new implementation.
//!
//! Emit the file with:
//!
//! ```text
//! cargo run --release -p misaka-palw-base0 --bin base0-kat > palw-base0-kat-v1.json
//! ```

use kaspa_consensus_core::palw_base0 as spec;
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The KAT set's own version, independent of the crate's. It changes when the vectors change,
/// which under ADR-0040 can only happen alongside a class id change.
pub const KAT_VERSION: u32 = 1;

/// One vector: the op's arguments in declaration order, then its output. Widened to `i64` so a
/// single container holds every op — the JSON carries the real widths in `arg_types`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KatVector {
    pub args: Vec<i64>,
    pub out: i64,
}

/// Every vector for one op, plus the metadata an implementer needs to call it.
#[derive(Clone, Debug)]
pub struct KatGroup {
    pub op: &'static str,
    pub arg_names: &'static [&'static str],
    pub arg_types: &'static [&'static str],
    pub out_type: &'static str,
    pub vectors: Vec<KatVector>,
}

/// Inputs on which a defect was actually found, and what each one caught. Carried as vectors so
/// that a third-party implementation making the same mistake fails on the same input rather than
/// on a random one.
pub const REGRESSIONS: &[(&str, &str)] = &[
    ("SRDHM(-1, 2^30) = 0", "half-UP, not half-away-from-zero: half-away gives -1 (ADR-0040 C1/C2)"),
    ("RoundingShiftRight(-64, 1) = -32", "the shift form `(x + 2^(s-1)) >> s` is wrong on every negative input"),
    ("RoundingShiftRight64 near i64's ends", "an earlier form panicked on overflow under `overflow-checks = true`"),
    ("IntRecip(v) for v in 1..=511", "`r * r` overflowed i64 on the whole small-v range; a random sweep hits it with p ~ 1e-4"),
    ("SRDHM(i32::MIN, i32::MIN) = i32::MAX", "the single saturating case; 2^63 is one past i64"),
];

/// Type limits, powers of two and their neighbours, both signs — the values most likely to
/// separate two implementations.
fn boundary_i32() -> Vec<i32> {
    let mut v = vec![0, 1, -1, 2, -2, 3, -3, i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for bit in 0..31u32 {
        let p = 1i32 << bit;
        v.extend_from_slice(&[p, p - 1, p + 1, -p, -p - 1, -p + 1]);
    }
    v
}

fn boundary_i64() -> Vec<i64> {
    let mut v = vec![0, 1, -1, 2, -2, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1];
    for bit in 0..62u32 {
        let p = 1i64 << bit;
        v.extend_from_slice(&[p, p - 1, p + 1, -p, -p - 1, -p + 1]);
    }
    v
}

/// A smaller `i32` table for the three-argument ops, where the full cross product would be
/// hundreds of thousands of vectors without testing anything the two-argument ops do not.
fn compact_i32() -> Vec<i32> {
    let mut v = vec![0, 1, -1, 2, -2, 127, -128, 255, -256, i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for bit in [7u32, 14, 20, 23, 29, 30] {
        let p = 1i32 << bit;
        v.extend_from_slice(&[p, p - 1, -p, -p + 1]);
    }
    v
}

/// Arguments are collected into a `BTreeSet` before evaluation, so the vector order is the
/// lexicographic order of the argument tuples and does not depend on the order the generators
/// ran. The digest is therefore a property of the *set*, not of this function's control flow.
fn build<F: Fn(&[i64]) -> i64>(
    op: &'static str,
    arg_names: &'static [&'static str],
    arg_types: &'static [&'static str],
    out_type: &'static str,
    args: BTreeSet<Vec<i64>>,
    apply: F,
) -> KatGroup {
    let vectors = args.into_iter().map(|a| KatVector { out: apply(&a), args: a }).collect();
    KatGroup { op, arg_names, arg_types, out_type, vectors }
}

/// The nine primitives ADR-0040 defines, in catalog order.
pub fn groups() -> Vec<KatGroup> {
    vec![
        rounding_shift_right(),
        rounding_shift_right_64(),
        srdhm(),
        requantize(),
        requantize_with_zero(),
        rescale(),
        int_exp(),
        int_rsqrt(),
        int_recip(),
    ]
}

fn rounding_shift_right() -> KatGroup {
    let mut args = BTreeSet::new();
    // Exhaustive across the small shifts on a window that contains the negative half-cases.
    for x in -64..=64i64 {
        for s in 0..=4i64 {
            args.insert(vec![x, s]);
        }
    }
    for &x in boundary_i32().iter() {
        for s in [0i64, 1, 2, 15, 16, 30, 31] {
            args.insert(vec![x as i64, s]);
        }
    }
    build("RoundingShiftRight", &["x", "s"], &["i32", "u8"], "i32", args, |a| {
        spec::rounding_shift_right(a[0] as i32, a[1] as u8) as i64
    })
}

fn rounding_shift_right_64() -> KatGroup {
    let mut args = BTreeSet::new();
    for x in -64..=64i64 {
        for s in 0..=4i64 {
            args.insert(vec![x, s]);
        }
    }
    for &x in boundary_i64().iter() {
        // 62 is `RESCALE_MAX_SHIFT`, the largest shift the class ever asks for.
        for s in [0i64, 1, 2, 31, 32, 33, 61, 62] {
            args.insert(vec![x, s]);
        }
    }
    build("RoundingShiftRight64", &["x", "s"], &["i64", "u8"], "i64", args, |a| spec::rounding_shift_right_64(a[0], a[1] as u8))
}

fn srdhm() -> KatGroup {
    let mut args = BTreeSet::new();
    // The second operand is a *scale* in practice, so the table is the values a scale takes:
    // unity (2^30), its neighbours, the ends, and the exact-half constructors.
    let scales: [i32; 14] =
        [0, 1, -1, 2, -2, 1 << 29, 1 << 30, (1 << 30) + 1, (1 << 30) - 1, -(1 << 30), i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for &a in boundary_i32().iter() {
        for &b in scales.iter() {
            args.insert(vec![a as i64, b as i64]);
        }
    }
    // The negative exact-half products, where half-up and half-away-from-zero disagree. Freely
    // constructible, not statistically rare — which is why they are enumerated.
    for a in -8..=8i64 {
        args.insert(vec![a, 1 << 30]);
        args.insert(vec![a, -(1 << 30)]);
    }
    build("SRDHM", &["a", "b"], &["i32", "i32"], "i32", args, |a| spec::srdhm(a[0] as i32, a[1] as i32) as i64)
}

const REQUANTIZE_MULTIPLIERS: [i32; 6] = [1, -1, 1 << 29, 1 << 30, i32::MAX, i32::MIN];

fn requantize() -> KatGroup {
    let mut args = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &mult in REQUANTIZE_MULTIPLIERS.iter() {
            for shift in [0i64, 1, 7, 15, 30, 31] {
                args.insert(vec![acc as i64, mult as i64, shift]);
            }
        }
    }
    build("Requantize", &["acc", "multiplier", "shift"], &["i32", "i32", "u8"], "i8", args, |a| {
        spec::requantize(a[0] as i32, a[1] as i32, a[2] as u8) as i64
    })
}

fn requantize_with_zero() -> KatGroup {
    let mut args = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &mult in [1i32 << 30, i32::MAX, -(1 << 30)].iter() {
            for shift in [0i64, 7, 31] {
                // The zero point is added BEFORE the clamp (ADR-0040 G2), so the vectors must
                // include the values that saturate in each direction *because of* the zero.
                for zero in [-129i64, -128, -1, 0, 1, 127, 128] {
                    args.insert(vec![acc as i64, mult as i64, shift, zero]);
                }
            }
        }
    }
    build("RequantizeWithZero", &["acc", "multiplier", "shift", "zero"], &["i32", "i32", "u8", "i32"], "i8", args, |a| {
        spec::requantize_with_zero(a[0] as i32, a[1] as i32, a[2] as u8, a[3] as i32) as i64
    })
}

fn rescale() -> KatGroup {
    let mut args = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &mult in REQUANTIZE_MULTIPLIERS.iter() {
            // Unlike `Requantize`, the gain here may exceed 1: shift 31 is unity and anything
            // below it amplifies. Both sides of 31 are covered because that is the whole point of
            // Decision H.
            for shift in [0i64, 1, 15, 30, 31, 32, 47, 62] {
                args.insert(vec![acc as i64, mult as i64, shift]);
            }
        }
    }
    build("Rescale", &["acc", "multiplier", "shift"], &["i32", "i32", "u8"], "i32", args, |a| {
        spec::rescale_q(a[0] as i32, a[1] as i32, a[2] as u8) as i64
    })
}

fn int_exp() -> KatGroup {
    let mut args = BTreeSet::new();
    // Near zero, where Poly2 does the work: exhaustive.
    for x in -2048..=64i64 {
        args.insert(vec![x]);
    }
    // Every range-reduction bucket and both sides of each boundary, out past the cutoff.
    for z in 0..=(spec::Z_MAX + 2) {
        for delta in -3..=3i32 {
            args.insert(vec![(-(z.saturating_mul(spec::LN2_Q)).saturating_add(delta)) as i64]);
        }
    }
    for &x in boundary_i32().iter() {
        args.insert(vec![x as i64]);
    }
    build("IntExp", &["x"], &["i32"], "i32", args, |a| spec::int_exp(a[0] as i32) as i64)
}

fn int_rsqrt() -> KatGroup {
    let mut args = BTreeSet::new();
    for v in 0..=1024i64 {
        args.insert(vec![v]);
    }
    for bit in 0..62u32 {
        let p = 1i64 << bit;
        for v in [p - 1, p, p + 1] {
            args.insert(vec![v]);
        }
    }
    // Non-positive inputs are DEFINED as 0 rather than left to diverge, so they are vectors.
    for v in [-1i64, -1000, i64::MIN, i64::MAX] {
        args.insert(vec![v]);
    }
    build("IntRsqrt", &["v"], &["i64"], "i64", args, |a| spec::int_rsqrt(a[0]))
}

fn int_recip() -> KatGroup {
    let mut args = BTreeSet::new();
    // Exhaustive over the small `v` where an earlier form overflowed on every input.
    for v in 0..=1024i64 {
        args.insert(vec![v]);
    }
    for bit in 0..62u32 {
        let p = 1i64 << bit;
        for v in [p - 1, p, p + 1] {
            args.insert(vec![v]);
        }
    }
    for v in [-1i64, -1000, i64::MIN, i64::MAX] {
        args.insert(vec![v]);
    }
    build("IntRecip", &["v"], &["i64"], "i64", args, |a| spec::int_recip(a[0]))
}

/// blake2b-256 over a canonical binary encoding: for each group in order, the op name, a `0x00`,
/// then every argument and output as little-endian `i64`.
///
/// The JSON is NOT hashed. Whitespace, key order and number formatting are presentation
/// decisions, and a digest that moved when they did would make the artifact hostage to its
/// serialiser.
pub fn digest(groups: &[KatGroup]) -> [u8; 32] {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).to_state();
    hasher.update(&(KAT_VERSION as u64).to_le_bytes());
    for group in groups {
        hasher.update(group.op.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(&(group.vectors.len() as u64).to_le_bytes());
        for vector in &group.vectors {
            for value in vector.args.iter().chain(std::iter::once(&vector.out)) {
                hasher.update(&value.to_le_bytes());
            }
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// The digest of the frozen set. A vector that changes value changes this, and the test that pins
/// it fails — which is the property that keeps an answer from being edited to match a new
/// implementation.
pub const KAT_DIGEST: &str = "d136224b269a15c60198ec82af99482f49521245848c9ffba0df814141d28fd3";

/// The exported form: one vector per line, so a diff between two releases names the vectors that
/// moved rather than reflowing the file.
pub fn to_json(groups: &[KatGroup]) -> String {
    let digest_hex = faster_hex::hex_string(&digest(groups));
    let mut out = String::new();
    out.push_str("{\n");
    let _ = writeln!(out, "  \"spec\": \"ADR-0040\",");
    let _ = writeln!(out, "  \"class\": \"PALW-BASE-0\",");
    let _ = writeln!(out, "  \"kat_version\": {KAT_VERSION},");
    let _ = writeln!(out, "  \"digest_blake2b256\": \"{digest_hex}\",");
    let _ = writeln!(
        out,
        "  \"digest_input\": \"per group: op name, 0x00, vector count as LE u64, then every argument and output as LE i64\","
    );
    out.push_str("  \"regressions\": [\n");
    for (index, (case, caught)) in REGRESSIONS.iter().enumerate() {
        let comma = if index + 1 == REGRESSIONS.len() { "" } else { "," };
        let _ = writeln!(out, "    {{ \"case\": \"{case}\", \"caught\": \"{caught}\" }}{comma}");
    }
    out.push_str("  ],\n");
    out.push_str("  \"groups\": [\n");
    for (index, group) in groups.iter().enumerate() {
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      \"op\": \"{}\",", group.op);
        let _ = writeln!(out, "      \"args\": [{}],", quoted(group.arg_names));
        let _ = writeln!(out, "      \"arg_types\": [{}],", quoted(group.arg_types));
        let _ = writeln!(out, "      \"out_type\": \"{}\",", group.out_type);
        let _ = writeln!(out, "      \"count\": {},", group.vectors.len());
        let _ = writeln!(out, "      \"vectors\": [");
        for (position, vector) in group.vectors.iter().enumerate() {
            let comma = if position + 1 == group.vectors.len() { "" } else { "," };
            let values: Vec<String> = vector.args.iter().chain(std::iter::once(&vector.out)).map(|v| v.to_string()).collect();
            let _ = writeln!(out, "        [{}]{comma}", values.join(", "));
        }
        let _ = writeln!(out, "      ]");
        let _ = writeln!(out, "    }}{}", if index + 1 == groups.len() { "" } else { "," });
    }
    out.push_str("  ]\n}\n");
    out
}

fn quoted(items: &[&str]) -> String {
    items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin. If this fails, either a primitive changed — which under ADR-0040 is a class id
    /// change and never a silent one — or a vector was edited, which is the thing the digest
    /// exists to prevent.
    #[test]
    fn the_kat_set_is_frozen() {
        let digest_hex = faster_hex::hex_string(&digest(&groups()));
        assert_eq!(digest_hex, KAT_DIGEST, "the KAT set moved; a primitive changing value is a class id change, not a test update");
    }

    /// Generation must not depend on iteration order, allocation order, or the order the
    /// generators happen to run in — otherwise the digest is a property of this machine.
    #[test]
    fn generation_is_deterministic() {
        assert_eq!(digest(&groups()), digest(&groups()));
        for (a, b) in groups().iter().zip(groups().iter()) {
            assert_eq!(a.vectors, b.vectors, "{} is not reproducible", a.op);
        }
    }

    /// A KAT set that covers only the easy half proves nothing about the half where the defects
    /// were. Both signs, and the specific regressions, must be present.
    #[test]
    fn every_group_covers_both_signs_and_is_not_trivial() {
        for group in groups() {
            assert!(group.vectors.len() >= 64, "{} has only {} vectors", group.op, group.vectors.len());
            let has_negative_input = group.vectors.iter().any(|v| v.args.iter().any(|&a| a < 0));
            assert!(has_negative_input, "{} has no negative input, and every defect found so far was negative-only", group.op);
            let distinct: BTreeSet<i64> = group.vectors.iter().map(|v| v.out).collect();
            assert!(distinct.len() > 1, "{} maps every vector to one output, so it tests nothing", group.op);
        }
    }

    /// The named regressions are vectors, not prose: each input below must appear in the set with
    /// the value the defect got wrong.
    #[test]
    fn the_named_regressions_are_in_the_set() {
        let all = groups();
        let find = |op: &str, args: &[i64]| -> i64 {
            all.iter()
                .find(|g| g.op == op)
                .unwrap_or_else(|| panic!("group {op}"))
                .vectors
                .iter()
                .find(|v| v.args == args)
                .unwrap_or_else(|| panic!("{op}{args:?} must be a vector"))
                .out
        };
        // Half-UP, not half-away: half-away would give -1.
        assert_eq!(find("SRDHM", &[-1, 1 << 30]), 0);
        // The shift form gives -32 too, but `(x + 2^(s-1)) >> s` gives -31 for x = -63.
        assert_eq!(find("RoundingShiftRight", &[-64, 1]), -32);
        assert_eq!(find("RoundingShiftRight", &[-63, 1]), -32);
        // The single saturating case.
        assert_eq!(find("SRDHM", &[i32::MIN as i64, i32::MIN as i64]), i32::MAX as i64);
        // Small `v`, where `r * r` overflowed i64 for every input in 1..=511.
        for v in [1i64, 2, 511] {
            let _ = find("IntRecip", &[v]);
        }
        // Defined, not divergent, on the non-positive domain.
        assert_eq!(find("IntRsqrt", &[0]), 0);
        assert_eq!(find("IntRsqrt", &[i64::MIN]), 0);
    }

    /// The JSON must round-trip its own digest: an implementer reads the file, recomputes, and
    /// gets the value printed in it.
    #[test]
    fn the_exported_digest_matches_the_exported_vectors() {
        let groups = groups();
        let json = to_json(&groups);
        let expected = faster_hex::hex_string(&digest(&groups));
        assert!(json.contains(&format!("\"digest_blake2b256\": \"{expected}\"")));
        let total: usize = groups.iter().map(|g| g.vectors.len()).sum();
        assert!(json.lines().count() > total, "every vector is its own line");
    }
}
