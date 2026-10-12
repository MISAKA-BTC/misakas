//! **Legacy conformance, first evidence: the frozen BASE-0 KAT set, reproduced through PALW-TIR.**
//!
//! `misaka_palw_base0::kat` enumerates 10,000+ vectors for the nine ADR-0040 primitives and pins
//! their BLAKE2b-256 digest (`KAT_DIGEST`). This test regenerates the same argument sets (the
//! generators below are a transcription of `kat.rs`'s tables), computes every output through IR
//! PROGRAMS — the builder's composite templates, validated, run by the reference evaluator — and requires
//! the identical digest. So `RoundingShiftRight`, `SRDHM`, `Requantize(WithZero)`, `Rescale` and
//! `IntRecip`, none of which is a primitive of PALW-TIR v1, are byte-identical as IR segments, and
//! `IntExp`/`IntRsqrt` as primitives, on the whole published set.
//!
//! No dependency on `misaka-palw-base0`: the digest string is the only thing shared.

mod common;

use std::collections::BTreeSet;

use common::{arg, eval_graph};
use misaka_palw_tir::{DType, Ref, Rounding};

/// `misaka_palw_base0::kat::KAT_DIGEST` at `rcore/int-6` 08481e720.
const KAT_DIGEST: &str = "d136224b269a15c60198ec82af99482f49521245848c9ffba0df814141d28fd3";
const KAT_VERSION: u64 = 1;
const LN2_Q: i32 = 11_629_080;
const Z_MAX: i32 = 31;

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

fn compact_i32() -> Vec<i32> {
    let mut v = vec![0, 1, -1, 2, -2, 127, -128, 255, -256, i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for bit in [7u32, 14, 20, 23, 29, 30] {
        let p = 1i32 << bit;
        v.extend_from_slice(&[p, p - 1, -p, -p + 1]);
    }
    v
}

const REQUANTIZE_MULTIPLIERS: [i32; 6] = [1, -1, 1 << 29, 1 << 30, i32::MAX, i32::MIN];

struct Group {
    op: &'static str,
    args: Vec<Vec<i64>>,
}

fn groups() -> Vec<Group> {
    let mut out = Vec::new();
    let mut a = BTreeSet::new();
    for x in -64..=64i64 {
        for s in 0..=4i64 {
            a.insert(vec![x, s]);
        }
    }
    for &x in boundary_i32().iter() {
        for s in [0i64, 1, 2, 15, 16, 30, 31] {
            a.insert(vec![x as i64, s]);
        }
    }
    out.push(Group { op: "RoundingShiftRight", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    for x in -64..=64i64 {
        for s in 0..=4i64 {
            a.insert(vec![x, s]);
        }
    }
    for &x in boundary_i64().iter() {
        for s in [0i64, 1, 2, 31, 32, 33, 61, 62] {
            a.insert(vec![x, s]);
        }
    }
    out.push(Group { op: "RoundingShiftRight64", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    let scales: [i32; 14] =
        [0, 1, -1, 2, -2, 1 << 29, 1 << 30, (1 << 30) + 1, (1 << 30) - 1, -(1 << 30), i32::MAX, i32::MIN, i32::MAX - 1, i32::MIN + 1];
    for &x in boundary_i32().iter() {
        for &b in scales.iter() {
            a.insert(vec![x as i64, b as i64]);
        }
    }
    for x in -8..=8i64 {
        a.insert(vec![x, 1 << 30]);
        a.insert(vec![x, -(1 << 30)]);
    }
    out.push(Group { op: "SRDHM", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &m in REQUANTIZE_MULTIPLIERS.iter() {
            for s in [0i64, 1, 7, 15, 30, 31] {
                a.insert(vec![acc as i64, m as i64, s]);
            }
        }
    }
    out.push(Group { op: "Requantize", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &m in [1i32 << 30, i32::MAX, -(1 << 30)].iter() {
            for s in [0i64, 7, 31] {
                for z in [-129i64, -128, -1, 0, 1, 127, 128] {
                    a.insert(vec![acc as i64, m as i64, s, z]);
                }
            }
        }
    }
    out.push(Group { op: "RequantizeWithZero", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    for &acc in compact_i32().iter() {
        for &m in REQUANTIZE_MULTIPLIERS.iter() {
            for s in [0i64, 1, 15, 30, 31, 32, 47, 62] {
                a.insert(vec![acc as i64, m as i64, s]);
            }
        }
    }
    out.push(Group { op: "Rescale", args: a.into_iter().collect() });

    let mut a = BTreeSet::new();
    for x in -2048..=64i64 {
        a.insert(vec![x]);
    }
    for z in 0..=(Z_MAX + 2) {
        for delta in -3..=3i32 {
            a.insert(vec![(-(z.saturating_mul(LN2_Q)).saturating_add(delta)) as i64]);
        }
    }
    for &x in boundary_i32().iter() {
        a.insert(vec![x as i64]);
    }
    out.push(Group { op: "IntExp", args: a.into_iter().collect() });

    for op in ["IntRsqrt", "IntRecip"] {
        let mut a = BTreeSet::new();
        for v in 0..=1024i64 {
            a.insert(vec![v]);
        }
        for bit in 0..62u32 {
            let p = 1i64 << bit;
            for v in [p - 1, p, p + 1] {
                a.insert(vec![v]);
            }
        }
        for v in [-1i64, -1000, i64::MIN, i64::MAX] {
            a.insert(vec![v]);
        }
        out.push(Group { op, args: a.into_iter().collect() });
    }
    out
}

fn col(args: &[Vec<i64>], i: usize) -> Vec<i128> {
    args.iter().map(|a| a[i] as i128).collect()
}

/// The group's outputs, computed by an IR graph.
fn outputs(g: &Group) -> Vec<i64> {
    let n = g.args.len();
    let t = match g.op {
        // RSR(x, s), s ≤ 31: Div(x, 2^s, half away from zero).
        "RoundingShiftRight" => eval_graph(&[arg("x", DType::I32, col(&g.args, 0)), arg("s", DType::I8, col(&g.args, 1))], |b, r| {
            let d = b.pow2_of(r[1]);
            b.div(r[0], d, Rounding::HalfAwayFromZero, DType::I32)
        }),
        "RoundingShiftRight64" => {
            eval_graph(&[arg("x", DType::I64, col(&g.args, 0)), arg("s", DType::I8, col(&g.args, 1))], |b, r| {
                let d = b.pow2_of(r[1]);
                b.div(r[0], d, Rounding::HalfAwayFromZero, DType::I64)
            })
        }
        // SRDHM(a, b) = sat32(HalfUp(a·b / 2^31)).
        "SRDHM" => eval_graph(&[arg("a", DType::I32, col(&g.args, 0)), arg("b", DType::I32, col(&g.args, 1))], |b, r| {
            let p = b.mul(r[0], r[1], DType::I64);
            let h = b.shr(p, 31, Rounding::HalfUp, DType::I64);
            b.clamp(h, i32::MIN as i64, i32::MAX as i64, DType::I32)
        }),
        "Requantize" | "RequantizeWithZero" => {
            let mut args = vec![
                arg("acc", DType::I32, col(&g.args, 0)),
                arg("m", DType::I32, col(&g.args, 1)),
                arg("s", DType::I8, col(&g.args, 2)),
            ];
            let with_zero = g.op == "RequantizeWithZero";
            args.push(arg("z", DType::I32, if with_zero { col(&g.args, 3) } else { vec![0; n] }));
            eval_graph(&args, |b, r| {
                // BASE-0 op 2 with a per-vector shift: RSR clamps its shift at 31.
                let p = b.mul(r[0], r[1], DType::I64);
                let h = b.shr(p, 31, Rounding::HalfUp, DType::I64);
                let srdhm = b.clamp(h, i32::MIN as i64, i32::MAX as i64, DType::I32);
                let s = b.clamp(r[2], 0, 31, DType::I8);
                let d = b.pow2_of(s);
                let rsr = b.div(srdhm, d, Rounding::HalfAwayFromZero, DType::I32);
                let t = b.add(rsr, r[3], DType::I64);
                b.clamp(t, -128, 127, DType::I8)
            })
        }
        "Rescale" => eval_graph(
            &[arg("acc", DType::I32, col(&g.args, 0)), arg("m", DType::I32, col(&g.args, 1)), arg("s", DType::I8, col(&g.args, 2))],
            |b, r| {
                let p = b.mul(r[0], r[1], DType::I64);
                let d = b.pow2_of(r[2]);
                let q = b.div(p, d, Rounding::HalfAwayFromZero, DType::I64);
                b.clamp(q, i32::MIN as i64, i32::MAX as i64, DType::I32)
            },
        ),
        "IntExp" => eval_graph(&[arg("x", DType::I32, col(&g.args, 0))], |b, r| b.int_exp(r[0])),
        "IntRsqrt" => eval_graph(&[arg("v", DType::I64, col(&g.args, 0))], |b, r| b.int_rsqrt(r[0])),
        "IntRecip" => eval_graph(&[arg("v", DType::I64, col(&g.args, 0))], |b, r| b.int_recip(r[0])),
        other => panic!("{other}"),
    }
    .unwrap_or_else(|e| panic!("{}: {e}", g.op));
    let _: Ref = Ref::Input(0);
    assert_eq!(t.data.len(), n);
    t.data.iter().map(|v| i64::try_from(*v).expect("every KAT output fits i64")).collect()
}

#[test]
fn the_base0_kat_digest_is_reproduced_through_ir_segments() {
    let groups = groups();
    assert_eq!(groups.len(), 9);
    let mut hasher = blake2b_simd::Params::new().hash_length(32).to_state();
    hasher.update(&KAT_VERSION.to_le_bytes());
    let mut total = 0usize;
    for g in &groups {
        let outs = outputs(g);
        hasher.update(g.op.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(&(g.args.len() as u64).to_le_bytes());
        for (a, o) in g.args.iter().zip(&outs) {
            for v in a.iter().chain(std::iter::once(o)) {
                hasher.update(&v.to_le_bytes());
            }
        }
        total += g.args.len();
    }
    let hex: String = hasher.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    assert!(total > 10_000, "only {total} vectors");
    assert_eq!(hex, KAT_DIGEST, "the BASE-0 KAT set is not reproduced by the IR segments");
}

/// The named regressions of `kat.rs`, through the same graphs.
#[test]
fn the_named_regressions_hold_in_ir() {
    let g = |op: &'static str, args: Vec<Vec<i64>>| outputs(&Group { op, args });
    assert_eq!(g("SRDHM", vec![vec![-1, 1 << 30]]), vec![0], "half UP, not half away");
    assert_eq!(g("SRDHM", vec![vec![i32::MIN as i64, i32::MIN as i64]]), vec![i32::MAX as i64]);
    assert_eq!(g("RoundingShiftRight", vec![vec![-64, 1], vec![-63, 1]]), vec![-32, -32]);
    assert_eq!(g("IntRsqrt", vec![vec![0], vec![i64::MIN]]), vec![0, 0]);
    let recips = g("IntRecip", (1..=511).map(|v| vec![v]).collect());
    assert!(recips.iter().all(|r| *r > 0), "IntRecip on 1..=511, where r·r overflowed i64 before");
}
