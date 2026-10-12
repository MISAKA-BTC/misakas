//! **The scoring library** (RFC-0004 §7.3, work item A7) — the stages an evaluation pipeline scores a
//! subject with: [`exact_match_v1`], [`ref_logprob_v1`] and [`ref_loglik_sum_v1`] (RefLogLik, two
//! stages), [`judge_v1`] and [`pairwise_v1`].
//!
//! Each is an ordinary version-2 program (spec 04b §15) of plain primitives — nothing here is known
//! to the court, and a program built by these functions and the same program written out by hand are
//! the same bytes. Each reads only external inputs (no weights: every param is lifted into an input)
//! and all but RefLogLik's first stage run as ONE position (`TripRule::Fixed { n: 1 }`) and commit
//! their score as their `Final` output —
//! an `i32` tensor, so the score is a committed leaf a court adjudicates like any other and the
//! pipeline's output digest (`score_root`, RFC-0003 §I.3) commits it.
//!
//! | kind | inputs (binding) | score |
//! | --- | --- | --- |
//! | ExactMatch | the generated ids (`Generated` or `FinalizedOutput`) and their count, the key's ids and their count (`Key`), the opening and closing delimiter ids (job scalars; `−1` for none) | `[1]`: 1 when the span after the first opening delimiter, up to the first closing one at or after it, equals the key; else 0 |
//! | RefLogLik | stage 1 (`TokenCount` over the reference, `Generated`): the decode stage's consumed rows (`StageRows`) and the subject's logit scale (a job scalar, Q24 nats per logit unit), one position per reference id; stage 2: stage 1's rows and count | `[2]`: `Σ_r log p(ref_r)` in Q24 nats, exact, as `(hi, lo)` with `sum = hi · 2^31 + lo`, `lo ∈ [0, 2^31)` |
//! | Judge | a judge stage's scalar (`StageFinal`) | `[1]`: the judge's scalar, clamped to the policy's `[lo, hi]` |
//! | Pairwise | a pairwise judge's preference of `A` over `B` (`StageFinal`), the order R drew (a job scalar: 0 when `A` is the candidate) and the margin (a job scalar) | `[1]`: +1 when the candidate is preferred beyond the margin, −1 when the parent is, 0 otherwise |
//!
//! **RefLogLik's arithmetic** is the library's shifted softmax in log form, per row `r`: `m = max_j
//! x_j`; `z_j = clamp((x_j − m) · scale, i32::MIN, 0)` (Q24 nats); `S = Σ_j IntExp(z_j)`;
//! `log p(ref_r) = clamp(z_{ref_r} − IntLn(S), i32::MIN, 0)`; the rows summed in `i64`, exactly. It is
//! two stages so that no cone reads more than one logits row (the decode door's own bound): stage 1's
//! leaf at position `p` reads row `p` and nothing else of the edge, stage 2's reads `R` values.
//! [`ref_loglik_reference_v1`] is the same arithmetic in plain Rust — what a second implementation
//! checks the golden vectors against.

use crate::arith::{int_exp, int_ln};
use crate::builder::ProgramBuilder;
use crate::error::{TirErrorKind, TirResult, err};
use crate::prim::{Cmp, Rounding};
use crate::program::{HISTORY_BOUND_V1_SMALL, Ref};
use crate::program_v2::{InputSource, OutputDecl, TirProgramV2};
use crate::types::{DType, Dim, TensorType};

/// The library's version: a scoring stage names the kind and this version in the policy.
pub const SCORING_LIBRARY_VERSION_V1: u16 = 1;

/// The scoring kinds, as a policy names them. Tags: `ExactMatch 0`, `RefLogLik 1`, `Judge 2`,
/// `Pairwise 3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ScoringKindV1 {
    ExactMatch,
    RefLogLik,
    Judge,
    Pairwise,
}

fn carry_of(pb: &ProgramBuilder, block: u8) -> Vec<TensorType> {
    let b = &pb.blocks[block as usize];
    b.carry_out.iter().map(|n| b.nodes[*n as usize].out.clone()).collect()
}

/// `pre` computes the score, `post` commits it clamped to `[lo, hi]`; every param is lifted into an
/// input with its declared interval, in declaration order. A `Final` output.
fn finish(pb: ProgramBuilder, pre: u8, lo: i64, hi: i64, inputs: &[(i64, i64)], name: &str) -> TirResult<TirProgramV2> {
    finish_as(pb, pre, lo, hi, inputs, name, false)
}

/// [`finish`], with a `Rows` output when `rows` (one value per position).
fn finish_as(
    mut pb: ProgramBuilder,
    pre: u8,
    lo: i64,
    hi: i64,
    inputs: &[(i64, i64)],
    name: &str,
    rows: bool,
) -> TirResult<TirProgramV2> {
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block(&format!("{name}.post"), carry);
        let o = b.clamp(Ref::CarryIn(0), lo, hi, DType::I32);
        b.commit(o);
        let Ref::Node(n) = o else { unreachable!("a clamp is a node") };
        (b.finish(&[]), n)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let lifted: Vec<(u16, InputSource)> =
        inputs.iter().enumerate().map(|(j, (lo, hi))| (j as u16, InputSource::External { lo: *lo, hi: *hi })).collect();
    let output = if rows { OutputDecl::Rows { node: out } } else { OutputDecl::Final { node: out } };
    TirProgramV2::from_v1_lifting_params(&v1, &lifted, output)
}

// ---------------------------------------------------------------------------------------------
// ExactMatch
// ---------------------------------------------------------------------------------------------

/// ExactMatch's static shape: the generated ids' pad length `G`, the key's `K`, and the token bound
/// the ids lie below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactMatchShapeV1 {
    pub gen_len: u32,
    pub key_len: u32,
    pub token_bound: u32,
}

/// **ExactMatch** (RFC-0004 §7.3). Inputs, in order: `em.gen` `idx [G]`, `em.gen_count` `idx []`,
/// `em.key` `idx [K]`, `em.key_count` `idx []`, `em.open` `i32 []` and `em.close` `i32 []` (a
/// delimiter id, or −1 for none: the span starts at 0, or runs to the count). Output `i32 [1]` ∈ {0, 1}.
pub fn exact_match_v1(shape: ExactMatchShapeV1) -> TirResult<TirProgramV2> {
    let ExactMatchShapeV1 { gen_len: g, key_len: k, token_bound } = shape;
    if g == 0 || k == 0 || token_bound == 0 {
        return err(TirErrorKind::NormalForm, "an exact match reads at least one generated id and one key id");
    }
    let tb = token_bound as i64 - 1;
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let generated = pb.param("em.gen", DType::Idx, &[g], false);
    let gen_n = pb.param("em.gen_count", DType::Idx, &[], false);
    let key = pb.param("em.key", DType::Idx, &[k], false);
    let key_n = pb.param("em.key_count", DType::Idx, &[], false);
    let open = pb.param("em.open", DType::I32, &[], false);
    let close = pb.param("em.close", DType::I32, &[], false);
    let pre = {
        let mut b = pb.block("em.pre", vec![]);
        let (zero8, one8) = (b.c(DType::I8, 0), b.c(DType::I8, 1));
        let and = |b: &mut crate::builder::BlockBuilder<'_>, x: Ref, y: Ref| b.select(x, y, zero8, DType::I8);
        let or = |b: &mut crate::builder::BlockBuilder<'_>, x: Ref, y: Ref| b.select(x, one8, y, DType::I8);
        let (zero, one) = (b.c(DType::I32, 0), b.c(DType::I32, 1));
        let gi = b.cast(generated, DType::I32);
        let pos = b.iota(DType::I32, &[Dim::Fixed(g)], 0, 0, 1);
        let n = b.cast(gen_n, DType::I32);
        let valid = b.compare(pos, n, Cmp::Lt);
        let gconst = b.c(DType::I32, g as i128);
        // `G − i` at a hit, 0 elsewhere: the maximum names the first hit, and 0 names none.
        let rev = b.sub(gconst, pos, DType::I32);
        // The first opening delimiter; the span starts after it (at 0 when there is none to find).
        let is_open = b.compare(gi, open, Cmp::Eq);
        let open_hit = and(&mut b, valid, is_open);
        let score_o = b.select(open_hit, rev, zero, DType::I32);
        let best_o = b.reduce_max(score_o, 0);
        let open_at = b.sub(gconst, best_o, DType::I32);
        let no_open = b.compare(open, zero, Cmp::Lt);
        let after_open = b.add(open_at, one, DType::I32);
        let start = b.select(no_open, zero, after_open, DType::I32);
        let found_o = b.compare(best_o, zero, Cmp::Gt);
        let found_open = or(&mut b, no_open, found_o);
        // The first closing delimiter at or after the start; the span ends at it (at the count when
        // there is none to find).
        let is_close = b.compare(gi, close, Cmp::Eq);
        let from_start = b.compare(pos, start, Cmp::Ge);
        let close_hit = and(&mut b, valid, is_close);
        let close_hit = and(&mut b, close_hit, from_start);
        let score_c = b.select(close_hit, rev, zero, DType::I32);
        let best_c = b.reduce_max(score_c, 0);
        let close_at = b.sub(gconst, best_c, DType::I32);
        let no_close = b.compare(close, zero, Cmp::Lt);
        let end = b.select(no_close, n, close_at, DType::I32);
        let found_c = b.compare(best_c, zero, Cmp::Gt);
        let found_close = or(&mut b, no_close, found_c);
        // The span's length is the key's, and its ids are the key's.
        let span = b.sub(end, start, DType::I32);
        let kn = b.cast(key_n, DType::I32);
        let len_ok = b.compare(span, kn, Cmp::Eq);
        let j = b.iota(DType::I32, &[Dim::Fixed(k)], 0, 0, 1);
        let at = b.add(start, j, DType::I32);
        let at = b.clamp(at, 0, g as i64 - 1, DType::Idx);
        let span_ids = b.gather(gi, at, 0, 0);
        let ki = b.cast(key, DType::I32);
        let same = b.compare(span_ids, ki, Cmp::Eq);
        let past = b.compare(j, kn, Cmp::Ge);
        let ok = or(&mut b, same, past);
        let ok = b.cast(ok, DType::I32);
        let n_ok = b.reduce_sum(ok, 0, DType::I32);
        let kc = b.c(DType::I32, k as i128);
        let all_ok = b.compare(n_ok, kc, Cmp::Eq);
        let pass = and(&mut b, found_open, found_close);
        let pass = and(&mut b, pass, len_ok);
        let pass = and(&mut b, pass, all_ok);
        let out = b.cast(pass, DType::I32);
        b.finish(&[out])
    };
    finish(pb, pre, 0, 1, &[(0, tb), (0, g as i64), (0, tb), (0, k as i64), (-1, tb), (-1, tb)], "em")
}

/// **ExactMatch in plain Rust** over the unpadded ids: `open`/`close` a delimiter id, or −1 for none.
pub fn exact_match_reference_v1(generated: &[u32], key: &[u32], open: i64, close: i64) -> bool {
    let start = if open < 0 {
        0
    } else {
        match generated.iter().position(|t| *t as i64 == open) {
            Some(i) => i + 1,
            None => return false,
        }
    };
    let end = if close < 0 {
        generated.len()
    } else {
        match generated[start.min(generated.len())..].iter().position(|t| *t as i64 == close) {
            Some(i) => start + i,
            None => return false,
        }
    };
    start <= end && generated[start..end] == *key
}

// ---------------------------------------------------------------------------------------------
// RefLogLik: two stages, so no cone reads more than one logits row
// ---------------------------------------------------------------------------------------------

/// RefLogLik's shape: `R` rows as the decode stage's edge carries them (at least its `max_trip`),
/// each of the subject's logits node shape `row`, its last axis the vocabulary `V`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefLogLikShapeV1 {
    pub rows: u32,
    pub row: Vec<u32>,
}

/// The largest logit scale a RefLogLik stage takes (Q24 nats per logit unit, `< 2^31`).
pub const REF_LOGLIK_MAX_SCALE_V1: i64 = i32::MAX as i64;

/// **RefLogLik, stage 1: each reference id's log-probability** (RFC-0004 §7.3) — a `TokenCount`
/// stage over the reference (`TokenSource::Generated`), one position per id: position `p` reads the
/// decode stage's consumed row `p` (its cone reads that row alone: a `Gather` reads its index first)
/// and scores its token, `z_{ref_p} − IntLn(Σ_j IntExp(z_j))` with `z = clamp((x − max x) · scale,
/// i32::MIN, 0)`, clamped at `i32::MIN` (−128 nats). Inputs: `rl.rows` `i32 [R] ++ row` (`StageRows`
/// over the decode stage), `rl.scale` `i32 []` (a job scalar); the token is the reference id; the
/// `Fixed` state `rl.pos` counts the rows read. Output `Rows` `i32 [1]`.
pub fn ref_logprob_v1(shape: &RefLogLikShapeV1) -> TirResult<TirProgramV2> {
    let r = shape.rows;
    let v: u32 = shape.row.iter().product();
    if r == 0 || v == 0 || shape.row.last().copied() != Some(v) {
        return err(TirErrorKind::NormalForm, "a reference log-probability reads at least one row, and a row is its vocabulary");
    }
    let mut rows_shape = vec![r];
    rows_shape.extend_from_slice(&shape.row);
    let mut pb = ProgramBuilder::new(v, HISTORY_BOUND_V1_SMALL);
    let rows = pb.param("rl.rows", DType::I32, &rows_shape, false);
    let scale = pb.param("rl.scale", DType::I32, &[], false);
    let cursor = pb.fixed_state("rl.pos", DType::I32, &[1], 0, r as i64, false);
    let pre = {
        let mut b = pb.block("rl.pre", vec![]);
        let x = b.reshape_fixed(rows, &[r, v]);
        let at = b.clamp(Ref::State(cursor), 0, r as i64 - 1, DType::Idx);
        let row = b.gather(x, at, 0, 0);
        let row = b.cast(row, DType::I64);
        let m = b.reduce_max(row, 1);
        let d = b.sub(row, m, DType::I64);
        let scale = b.cast(scale, DType::I64);
        let w = b.mul(d, scale, DType::I128);
        let z = b.clamp(w, i32::MIN as i64, 0, DType::I32);
        let e = b.int_exp(z);
        let sum = b.reduce_sum(e, 1, DType::I64);
        let ln = b.int_ln(sum);
        let ln = b.reshape_fixed(ln, &[1]);
        let zr = b.gather(z, Ref::Input(0), 1, 0);
        let lp = b.sub(zr, ln, DType::I64);
        let lp = b.clamp(lp, i32::MIN as i64, 0, DType::I32);
        let one = b.c(DType::I32, 1);
        let next = b.add(Ref::State(cursor), one, DType::I32);
        let next = b.clamp(next, 0, r as i64, DType::I32);
        b.state_write(cursor, next);
        b.finish(&[lp])
    };
    finish_as(pb, pre, i32::MIN as i64, 0, &[(i32::MIN as i64, i32::MAX as i64), (0, REF_LOGLIK_MAX_SCALE_V1)], "rl", true)
}

/// **RefLogLik, stage 2: the sum** — one position reading stage 1's rows (`StageRows`, `rl.lp` `i32
/// [R, 1]`) and their count (`StageRowCount`, `rl.count` `idx []`): `Σ_{r < count} lp_r`, exact in
/// `i64`, committed as `(hi, lo)` with `sum = hi · 2^31 + lo`, `lo ∈ [0, 2^31)`. Output `i32 [2]`.
pub fn ref_loglik_sum_v1(rows: u32) -> TirResult<TirProgramV2> {
    if rows == 0 {
        return err(TirErrorKind::NormalForm, "a reference log-likelihood sums at least one row");
    }
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let lp = pb.param("rl.lp", DType::I32, &[rows, 1], false);
    let count = pb.param("rl.count", DType::Idx, &[], false);
    let pre = {
        let mut b = pb.block("rl.sum", vec![]);
        let x = b.reshape_fixed(lp, &[rows]);
        let x = b.cast(x, DType::I64);
        let pos = b.iota(DType::Idx, &[Dim::Fixed(rows)], 0, 0, 1);
        let kept = b.compare(pos, count, Cmp::Lt);
        let zero = b.c(DType::I64, 0);
        let x = b.select(kept, x, zero, DType::I64);
        let total = b.reduce_sum(x, 0, DType::I64);
        let hi = b.shr(total, 31, Rounding::Floor, DType::I64);
        let two31 = b.c(DType::I64, 1i128 << 31);
        let hi_part = b.mul(hi, two31, DType::I64);
        let lo = b.sub(total, hi_part, DType::I64);
        let hi = b.clamp(hi, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let lo = b.clamp(lo, 0, i32::MAX as i64, DType::I32);
        let out = b.concat(&[hi, lo], 0);
        b.finish(&[out])
    };
    finish(pb, pre, i32::MIN as i64, i32::MAX as i64, &[(i32::MIN as i64, 0), (0, rows as i64)], "rl.sum")
}

/// **One reference id's log-probability in plain Rust** (stage 1's value at a position).
pub fn ref_logprob_reference_v1(row: &[i32], id: u32, scale: i64) -> i32 {
    let m = row.iter().copied().max().unwrap_or(0) as i128;
    let z: Vec<i128> = row.iter().map(|x| ((*x as i128 - m) * scale as i128).clamp(i32::MIN as i128, 0)).collect();
    let s: i128 = z.iter().map(|z| int_exp(*z)).sum();
    (z[id as usize] - int_ln(s)).clamp(i32::MIN as i128, 0) as i32
}

/// **RefLogLik in plain Rust**: `rows[r]` the logits row `ref[r]` was scored against (the decode
/// stage's consumed rows), `scale` Q24 nats per logit unit. Returns the exact Q24 sum.
pub fn ref_loglik_reference_v1(rows: &[Vec<i32>], refs: &[u32], scale: i64) -> i64 {
    rows.iter().zip(refs).map(|(row, id)| ref_logprob_reference_v1(row, *id, scale) as i64).sum()
}

/// A RefLogLik score's `(hi, lo)` as the exact sum.
pub fn ref_loglik_join_v1(hi: i32, lo: i32) -> i64 {
    (hi as i64) * (1i64 << 31) + lo as i64
}

// ---------------------------------------------------------------------------------------------
// Judge and Pairwise
// ---------------------------------------------------------------------------------------------

/// **Judge** (RFC-0004 §7.3): input `jd.score` `i32 [1]` — a registered judge class's scalar output
/// (`StageFinal` of its stage); output `i32 [1]`, the scalar clamped to the policy's `[lo, hi]`.
pub fn judge_v1(lo: i32, hi: i32) -> TirResult<TirProgramV2> {
    if lo > hi {
        return err(TirErrorKind::NormalForm, "a judge's range is lo ≤ hi");
    }
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let score = pb.param("jd.score", DType::I32, &[1], false);
    let pre = {
        let mut b = pb.block("jd.pre", vec![]);
        let s = b.clamp(score, lo as i64, hi as i64, DType::I32);
        b.finish(&[s])
    };
    finish(pb, pre, lo as i64, hi as i64, &[(i32::MIN as i64, i32::MAX as i64)], "jd")
}

/// **Pairwise** (RFC-0004 §7.3). Inputs: `pw.pref` `i32 [1]` — a pairwise judge's preference of `A`
/// over `B` (`StageFinal`); `pw.order` `i32 []` — R's order (0: `A` is the candidate, 1: `A` is the
/// parent); `pw.margin` `i32 []` — a preference within it is a tie. Output `i32 [1]` ∈ {−1, 0, 1}, the
/// candidate's outcome.
pub fn pairwise_v1() -> TirResult<TirProgramV2> {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let pref = pb.param("pw.pref", DType::I32, &[1], false);
    let order = pb.param("pw.order", DType::I32, &[], false);
    let margin = pb.param("pw.margin", DType::I32, &[], false);
    let pre = {
        let mut b = pb.block("pw.pre", vec![]);
        let zero = b.c(DType::I64, 0);
        let neg_margin = b.sub(zero, margin, DType::I64);
        let above = b.compare(pref, margin, Cmp::Gt);
        let below = b.compare(pref, neg_margin, Cmp::Lt);
        let above = b.cast(above, DType::I32);
        let below = b.cast(below, DType::I32);
        let v = b.sub(above, below, DType::I32);
        let one = b.c(DType::I32, 1);
        let swapped = b.compare(order, one, Cmp::Eq);
        let zero32 = b.c(DType::I32, 0);
        let nv = b.sub(zero32, v, DType::I32);
        let out = b.select(swapped, nv, v, DType::I32);
        b.finish(&[out])
    };
    finish(pb, pre, -1, 1, &[(i32::MIN as i64, i32::MAX as i64), (0, 1), (0, i32::MAX as i64)], "pw")
}

/// **Pairwise in plain Rust.**
pub fn pairwise_reference_v1(pref: i32, order: i32, margin: i32) -> i32 {
    let v = (pref as i64 > margin as i64) as i32 - ((pref as i64) < -(margin as i64)) as i32;
    if order == 1 { -v } else { v }
}

// ---------------------------------------------------------------------------------------------
// The scoring set's identity
// ---------------------------------------------------------------------------------------------

/// The key the scoring set's id is hashed under (the caller's: this crate never hashes).
pub const PALW_IMPROVE_SCORING_SET_DOMAIN_V1: &[u8] = b"misaka-palw/improve/scoring-set/v1";

/// **The library at its reference shapes**, in the order [`scoring_set_descriptor_v1`] lists them:
/// each program's name and canonical bytes. The shapes are pins, not limits — a policy instantiates
/// each kind at its own shape; the descriptor says which builders the network runs.
pub fn scoring_reference_programs_v1() -> Vec<(&'static str, TirProgramV2)> {
    let em = exact_match_v1(ExactMatchShapeV1 { gen_len: 8, key_len: 4, token_bound: 16 }).expect("the reference shape builds");
    let lp = ref_logprob_v1(&RefLogLikShapeV1 { rows: 4, row: vec![1, 8] }).expect("the reference shape builds");
    let sum = ref_loglik_sum_v1(4).expect("the reference shape builds");
    let jd = judge_v1(-1000, 1000).expect("the reference range builds");
    let pw = pairwise_v1().expect("builds");
    vec![("exact-match", em), ("ref-logprob", lp), ("ref-loglik-sum", sum), ("judge", jd), ("pairwise", pw)]
}

/// **The scoring set's descriptor** — the canonical bytes RFC-0004's `scoring_set_id` hashes, keyed
/// [`PALW_IMPROVE_SCORING_SET_DOMAIN_V1`] by the caller (this crate is a leaf and never hashes):
/// `le16(SCORING_LIBRARY_VERSION_V1)` and, for each reference program in order, `le32(|name|) ‖ name ‖
/// le32(|bytes|) ‖ bytes`. Two builds whose scoring stages differ in any byte differ here.
pub fn scoring_set_descriptor_v1() -> Vec<u8> {
    let mut out = SCORING_LIBRARY_VERSION_V1.to_le_bytes().to_vec();
    for (name, program) in scoring_reference_programs_v1() {
        let bytes = program.encode();
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
    out
}
