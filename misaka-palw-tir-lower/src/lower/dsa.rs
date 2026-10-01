//! **DeepSeek sparse attention** (`ATTN_TOKEN_INDEXER_V1`): the token indexer's selection, as a mask over the history
//! window, in the 25 primitives. No new primitive and no new court kernel.
//!
//! # What is selected
//!
//! The indexer scores every token `t` of the window, `s_t = Σ_h w_h · ReLU(q_h · k_t)` (the positive factors of the
//! model's formula, `dim^-½` and `heads^-½`, move no rank and are dropped), and the attention softmax runs over the
//! `topk` best tokens. **Ties go to the lowest index of the window** — the IR's `TopK` rule (04b §6.6), and the only
//! pinned choice: `torch.topk` leaves its tie order unspecified, and ReLU makes exact-zero scores common. When the
//! window holds at most `topk` tokens every token is selected (the layer is dense MLA).
//!
//! # Why not `TopK`
//!
//! `TopK` needs a `Fixed` axis (04b §6.6) and a gather along `H` would break "dissectability is structural" (§10.3):
//! the window is `H` long. So the selection is the mask `κ_t ≥ τ'` of an exact threshold found **by counting**:
//!
//! ```text
//!   s_t   the score, narrowed to SB bits (a calibrated scale; ReLU zeros stay exact zeros)
//!   κ_t = (s_t + 2^SB) · 2^b + (2^b − 1 − t)      distinct: the low b bits break ties toward the lowest t
//!   τ'  = the k-th largest κ  (0 when the window holds fewer than k tokens)
//!   vis_t = [κ_t ≥ τ']                              exactly min(k, H) tokens
//! ```
//!
//! `τ'` is found by a 16-ary radix search of `B/4` passes (`B = SB + 1 + b`, a multiple of four): pass `j` counts the
//! `κ ≥ prefix + (i + 1)·step_j` for the 15 candidates `i`, the digit is the number of candidates with at least `k`
//! tokens at or above them, and the prefix grows by `digit · step_j`. Each count is one `ReduceSum` over `H` of a
//! `Compare`; a pass reads the previous pass's total, so the passes chain in one cone of `B/4` reductions over `H`
//! (04b §9.5.1 allows 16). `τ'` is a **commit point** (one lane); everything downstream compares against that one
//! value, so the masked logits cost the court only a recomputation of `κ` over the tile's history rows.
//!
//! The selection is masked-dense: it saves no arithmetic over dense MLA, it only makes the function exact.

use super::*;
use tir::Cmp;

/// The bits of the narrowed score a ranking keeps (before the rounding of `SB` up to a multiple of four below): 2^16
/// levels resolve scores to one part in 65,000 of the largest — as fine as the 15-bit codes the scores come from, and
/// finer than the bf16 the models are trained in (a token within that of the k-th place is a coin flip in any
/// implementation); each four bits more cost a radix pass, about eight nodes of a block that has 512.
const SCORE_BITS: u32 = 16;

/// The low lanes of the committed threshold: `τ' = hi · 2^LOW_BITS + lo`, `hi` and `lo` each inside `i32`.
const LOW_BITS: u32 = 28;

/// What the selection reads: this position's indexer query and head weights (`i16` codes) and the window of the
/// indexer's keys (`i16`, `[H, dim]`, this position's row last).
pub(super) struct IndexInputs {
    pub q: Val,
    pub w: Val,
    pub keys: (tir::Ref, ScaleKey),
    pub heads: usize,
    pub dim: usize,
    pub topk: usize,
    /// The window bound of the history the keys live in (`H ≤ window`).
    pub window: u32,
}

/// `⌈log2 x⌉` for `x ≥ 1`, in integers (a program must not depend on a libm).
fn ceil_log2(x: u64) -> u32 {
    if x <= 1 { 0 } else { u64::BITS - (x - 1).leading_zeros() }
}

/// The mask `[1, H]` (`i8`) of the tokens the indexer keeps: `1` exactly where the token is among the `min(topk, H)`
/// best (ties to the lowest index of the window).
pub(super) fn token_index_mask(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, ix: &IndexInputs, site: &str) -> Result<tir::Ref> {
    let (hi, di) = (ix.heads as u32, ix.dim as u32);
    if ix.q.dt != DType::I16 || ix.w.dt != DType::I16 || ix.q.len != ix.heads * ix.dim || ix.w.len != ix.heads {
        return Err(LowerError::eval(format!("internal: `{site}`'s indexer reads {:?}/{:?} rows of {}/{}", ix.q.dt, ix.w.dt, ix.q.len, ix.w.len)));
    }
    // Scores: dots [hi, H] exact in i64, ReLU, the head-weighted sum [1, H].
    let qm = b.reshape_fixed(ix.q.r, &[hi, di]);
    let kt = b.transpose(ix.keys.0, &[1, 0]);
    let dots = b.matmul(qm, kt, DType::I64);
    let mut relu = b.clamp(dots, 0, 1 << 62, DType::I64);
    // Headroom of the weighted sum in i64: `dim · 32767²` per dot, `32767` per weight, `heads` terms. Past 62 bits the
    // dots are shifted down first (the ranking needs no more resolution than its codes had).
    let dot_bits = ceil_log2(32767u64 * 32767 * ix.dim as u64);
    let need = dot_bits + 15 + ceil_log2(ix.heads as u64);
    let pre = need.saturating_sub(62);
    if pre > 0 {
        relu = b.shr(relu, pre, Rounding::Floor, DType::I64);
    }
    let wrow = b.reshape_fixed(ix.w.r, &[1, hi]);
    let score = b.matmul(wrow, relu, DType::I64);
    // The narrowing to SB bits: the real score is `score · sq · sk · sw · 2^pre`, calibrated at its own site.
    let bidx = ceil_log2(ix.window.max(2) as u64);
    let passes = (SCORE_BITS + 1 + bidx).div_ceil(4);
    let total_bits = 4 * passes;
    let sb = total_bits - 1 - bidx; // ≥ SCORE_BITS
    if total_bits > 56 {
        return Err(LowerError::not_lowerable(format!("{site}: a token indexer over a window of 2^{bidx} positions needs {total_bits}-bit keys")));
    }
    let score_key = ScaleKey::site(vec![format!("{site}.idx_score")], false).times(1.0 / (1u64 << (sb - 15)) as f64);
    let (kq, kk, kw, ks) = (ix.q.key.clone(), ix.keys.1.clone(), ix.w.key.clone(), score_key);
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &format!("{site}.idx_score"),
        1,
        Arc::new(move |c| Ok(vec![c.scale(&kq)? * c.scale(&kk)? * c.scale(&kw)? * (1u64 << pre) as f64 / c.scale(&ks)?])),
    )?;
    let sc = narrow(b, score, m, s, None, DType::I32);
    let lim = (1i64 << sb) - 1;
    let sc = b.clamp(sc, -lim, lim, DType::I32);
    let h = b.shape(sc)[1];
    Ok(top_k_mask(b, sc, h, bidx, sb, ix.topk))
}

/// The mask `[1, H]` (`i8`) of the `min(topk, H)` best of the narrowed scores `sc` (`[1, H]`, `i32`, each inside
/// `±(2^sb − 1)`): ties to the lowest index; `h` is the window's dimension (`H`; a `Fixed` one in the unit tests) and
/// `sb + 1 + bidx` a multiple of four with `H ≤ 2^bidx`.
pub(super) fn top_k_mask(b: &mut BlockBuilder<'_>, sc: tir::Ref, h: Dim, bidx: u32, sb: u32, topk: usize) -> tir::Ref {
    let total_bits = sb + 1 + bidx;
    debug_assert!(total_bits % 4 == 0, "the radix search takes four bits a pass");
    let passes = total_bits / 4;
    // κ_t = (s_t + 2^sb) · 2^b + (2^b − 1 − t) ∈ [0, 2^total_bits), distinct per t.
    let off = b.c(DType::I64, 1i128 << sb);
    let s_off = b.add(sc, off, DType::I64);
    let scale_hi = b.c(DType::I64, 1i128 << bidx);
    let hi_part = b.mul(s_off, scale_hi, DType::I64);
    let t = b.iota(DType::I64, &[Dim::Fixed(1), h], 1, 0, 1);
    let top = b.c(DType::I64, (1i128 << bidx) - 1);
    let low = b.sub(top, t, DType::I64);
    let kappa = b.add(hi_part, low, DType::I64);
    // The k-th largest κ by a 16-ary radix search; each pass is one reduction over H.
    let kc = b.c(DType::I64, topk as i128);
    let mut prefix = b.c(DType::I64, 0);
    for j in 0..passes {
        let step = 1i64 << (total_bits - 4 * (j + 1));
        let cand_off = b.iota(DType::I64, &[Dim::Fixed(15), Dim::Fixed(1)], 0, step, step); // (i + 1)·step
        let cand = b.add(prefix, cand_off, DType::I64);
        let ge = b.compare(kappa, cand, Cmp::Ge); // [15, H]
        let counts = b.reduce_sum(ge, 1, DType::I64); // [15, 1]
        let enough = b.compare(counts, kc, Cmp::Ge);
        // The candidates grow with i and the counts shrink, so the candidates with enough tokens at or above them
        // are a prefix of them: the largest of those (or the prefix itself when there is none) is the next prefix,
        // `prefix + digit · step`.
        let kept = b.select(enough, cand, prefix, DType::I64);
        prefix = b.reduce_max(kept, 0); // [1, 1]
    }
    // The threshold is what the court commits to — a commit point is at most 32 bits wide (PALW-TIR-5), so it is two
    // lanes, `τ' = hi · 2^28 + lo` — and every masked row compares against it.
    let hi = b.shr(prefix, LOW_BITS, Rounding::Floor, DType::I64);
    let hi = b.clamp(hi, 0, i32::MAX as i64, DType::I32);
    let radix = b.c(DType::I64, 1i128 << LOW_BITS);
    let hi_back = b.mul(hi, radix, DType::I64);
    let lo = b.sub(prefix, hi_back, DType::I64);
    let lo = b.clamp(lo, 0, (1i64 << LOW_BITS) - 1, DType::I32);
    b.commit(hi);
    b.commit(lo);
    let tau_hi = b.mul(hi, radix, DType::I64);
    let tau = b.add(tau_hi, lo, DType::I64);
    b.compare(kappa, tau, Cmp::Ge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Interpreter, MapParams, Tensor};

    /// A 64-bit LCG, so no test depends on an RNG crate's stream.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            self.0 ^ (self.0 >> 29)
        }
    }

    /// The selection by sorting: the `min(k, n)` largest, ties to the lowest index.
    fn reference(scores: &[i64], k: usize) -> Vec<i128> {
        let mut order: Vec<usize> = (0..scores.len()).collect();
        order.sort_by(|a, b| scores[*b].cmp(&scores[*a]).then(a.cmp(b)));
        let mut keep = vec![0i128; scores.len()];
        for i in order.into_iter().take(k) {
            keep[i] = 1;
        }
        keep
    }

    /// The program: scores `[1, n]` (`i32`, a param) in, the mask out, over a window of `n` rows.
    fn mask_program(n: usize, bidx: u32, sb: u32, k: usize) -> misaka_palw_tir::TirProgramV1 {
        let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
        let sp = pb.param("scores", DType::I32, &[1, n as u32], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let lim = (1i64 << sb) - 1;
            let sc = b.clamp(sp, -lim, lim, DType::I32); // the range analysis needs the bound a lowering's narrowing states
            let vis = top_k_mask(&mut b, sc, Dim::Fixed(n as u32), bidx, sb, k);
            b.finish(&[vis])
        };
        let carry = pb.blocks[pre as usize].nodes.last().expect("a node").out.clone();
        let post = {
            let mut b = pb.block("post", vec![carry.clone()]);
            let l = b.reshape(tir::Ref::CarryIn(0), &carry.shape);
            b.commit(l);
            b.finish(&[])
        };
        pb.finish(pre, vec![], post, 0)
    }

    /// **The radix selection is the sort's**, on every window up to 300 tokens, for `k` from 1 to past the window, over
    /// scores full of ties (a few distinct values, many zeros), negatives, extremes and the all-equal vector — and the
    /// program passes the range analysis (no `i64` can overflow, on any scores).
    #[test]
    fn the_radix_selection_equals_the_sort_with_ties_to_the_lowest_index() {
        let mut rng = Lcg(0xD5A);
        for (n, k) in [(1usize, 1usize), (2, 1), (5, 4), (8, 8), (17, 4), (64, 5), (100, 16), (300, 64), (300, 1), (300, 299), (40, 100)] {
            let bidx = ceil_log2(n.max(2) as u64);
            let passes = (SCORE_BITS + 1 + bidx).div_ceil(4);
            let sb = 4 * passes - 1 - bidx;
            let program = mask_program(n, bidx, sb, k);
            tir::interval::analyze_ranges(&program).unwrap_or_else(|e| panic!("n {n} k {k}: the range analysis refuses the selection: {e}"));
            let interp = Interpreter::new(&program).expect("valid");
            let lim = (1i64 << sb) - 1;
            for case in 0..60 {
                let scores: Vec<i64> = (0..n)
                    .map(|_| match case % 6 {
                        0 => 0,
                        1 => (rng.next() % 3) as i64 - 1,
                        2 => [0, 0, 0, 5, -5][(rng.next() % 5) as usize],
                        3 => if rng.next() % 2 == 0 { lim } else { -lim },
                        4 => (rng.next() % (2 * lim as u64 + 1)) as i64 - lim,
                        _ => ((rng.next() % 41) as i64 - 20) * 1000,
                    })
                    .collect();
                let mut params = MapParams::default();
                params.tensors.insert((0, None), Tensor::new(DType::I32, vec![1, n], scores.iter().map(|x| *x as i128).collect()).expect("scores"));
                let out = interp.run(&params, &[0]).expect("the program runs");
                assert_eq!(out[0].logits.data, reference(&scores, k), "n {n} k {k} scores {scores:?}");
            }
        }
    }
}
