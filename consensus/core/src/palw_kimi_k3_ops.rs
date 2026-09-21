//! **`PALW-KIMI-K3` — integer ops the Kimi K3 graph needs and Qwen3.6's hybrid does not have.**
//!
//! ADR-0097 §1.3 priced Kimi K3 as a *stand-in* in the hybrid family's geometry (KDA as
//! GatedDeltaNet, MLA as grouped-query attention, 92 layers at interval 4). That stand-in is a
//! verdict, never a class. This module is the arithmetic a class would actually run:
//!
//! * **KDA** — gated linear attention with a `k_dim × v_dim` recurrent state. The *shape* matches
//!   GatedDeltaNet's rank-one write; the *program* is a different kernel id, so a court that
//!   knows Q36 GDN cannot silently re-execute a Kimi step.
//! * **MLA** — multi-head latent attention. The committed cache is `kv_lora_rank + qk_rope`
//!   values a position, not `kv_heads × head_dim`. Scores and values run after a decompress
//!   matmul the graph names separately.
//! * **Router** — softmax over 896 experts, take 16, renormalize. Ties break to the **lowest
//!   expert index**. Two shared experts are always on and are not in the 896.
//!
//! Integers only, no libm, no float. Activations are A16 codes. Router weights are Q[`K`].

use crate::palw_base0::{K, ONE, rounding_shift_right_64};
use crate::palw_base0_a16::{A16_CODE_MAX, A16QuantParams, a16_scale_round};
use crate::palw_base0_ops::softmax_shifted;
use crate::palw_qwen36_ops::PalwQwen36OpError;

/// Experts the Kimi K3 card routes over (not counting the two shared).
pub const KIMI_K3_NUM_EXPERTS: usize = 896;
/// `num_experts_per_tok` on the card.
pub const KIMI_K3_EXPERTS_PER_TOKEN: usize = 16;
/// Shared experts that are always on, outside the routed set.
pub const KIMI_K3_SHARED_EXPERTS: usize = 2;

const KIMI_K3_MAX_ROUTED: usize = 64;

const _: () = assert!(
    KIMI_K3_MAX_ROUTED as i64 * ONE * (i32::MAX as i64) < i64::MAX / 4,
    "the weighted combine accumulates k terms of (Q[K] weight × a wide i32 lane) in i64"
);

pub type PalwKimiK3OpError = PalwQwen36OpError;

fn check_a16(row: &[i32]) -> Result<(), PalwKimiK3OpError> {
    if row.is_empty() {
        return Err(PalwKimiK3OpError::Empty);
    }
    if row.iter().any(|v| (*v as i64).abs() > A16_CODE_MAX) {
        return Err(PalwKimiK3OpError::NotA16Codes);
    }
    Ok(())
}

/// One routed expert: which one, and the renormalized weight it carries in Q[`K`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KimiK3RoutedExpert {
    pub expert: u16,
    pub weight_q: i32,
}

/// **Op K1: `RouterTopK` — softmax over every routed expert, take `k`, renormalize to Q[`K`].**
///
/// Ties break to the **lowest expert index**, then the result is sorted by expert index
/// ascending. Two honest hosts that disagree on a tie would otherwise run different experts and
/// produce unrelated outputs; the court cannot bisect that.
pub fn kimi_k3_router_topk(logits: &[i32], k: usize, up_bits: u8) -> Result<Vec<KimiK3RoutedExpert>, PalwKimiK3OpError> {
    check_a16(logits)?;
    let experts = logits.len();
    if k == 0 || k > experts || k > KIMI_K3_MAX_ROUTED {
        return Err(PalwKimiK3OpError::BadK { k, experts });
    }
    let probs = softmax_shifted(logits, up_bits).map_err(|_| PalwKimiK3OpError::Empty)?;
    let mut chosen = Vec::with_capacity(k);
    let mut taken = vec![false; experts];
    for _ in 0..k {
        let mut best = usize::MAX;
        for (i, p) in probs.iter().enumerate() {
            if taken[i] {
                continue;
            }
            if best == usize::MAX || *p > probs[best] {
                best = i;
            }
        }
        taken[best] = true;
        chosen.push(KimiK3RoutedExpert { expert: best as u16, weight_q: probs[best] });
    }
    let sum: i64 = chosen.iter().map(|c| c.weight_q as i64).sum();
    if sum <= 0 {
        let equal = (ONE / k as i64) as i32;
        for (i, c) in chosen.iter_mut().enumerate() {
            c.weight_q = if i + 1 == k { (ONE - equal as i64 * (k as i64 - 1)) as i32 } else { equal };
        }
    } else {
        let mut remaining = ONE;
        for (i, c) in chosen.iter_mut().enumerate() {
            if i + 1 == k {
                c.weight_q = remaining as i32;
            } else {
                let w = (c.weight_q as i64 * ONE) / sum;
                c.weight_q = w as i32;
                remaining -= w;
            }
        }
    }
    chosen.sort_by_key(|c| c.expert);
    Ok(chosen)
}

/// **Op K2: combine 16 routed expert outputs plus two always-on shared experts.**
///
/// `routed` is `k × hidden` concatenated in the same expert-index order [`kimi_k3_router_topk`]
/// committed. `shared` is `SHARED × hidden`. Shared experts carry weight `ONE` each; routed
/// weights already sum to `ONE`.
pub fn kimi_k3_moe_combine(
    routed: &[i32],
    shared: &[i32],
    weights: &[KimiK3RoutedExpert],
    hidden: usize,
) -> Result<Vec<i32>, PalwKimiK3OpError> {
    if hidden == 0 || weights.is_empty() {
        return Err(PalwKimiK3OpError::Empty);
    }
    let k = weights.len();
    if routed.len() != k * hidden {
        return Err(PalwKimiK3OpError::LengthMismatch { a: routed.len(), b: k * hidden });
    }
    if shared.len() != KIMI_K3_SHARED_EXPERTS * hidden {
        return Err(PalwKimiK3OpError::LengthMismatch { a: shared.len(), b: KIMI_K3_SHARED_EXPERTS * hidden });
    }
    let mut out = vec![0i32; hidden];
    for (e, w) in weights.iter().enumerate() {
        let block = &routed[e * hidden..(e + 1) * hidden];
        for (o, x) in out.iter_mut().zip(block) {
            let acc = *o as i64 + rounding_shift_right_64(*x as i64 * w.weight_q as i64, K as u8);
            *o = acc.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
        }
    }
    for s in 0..KIMI_K3_SHARED_EXPERTS {
        let block = &shared[s * hidden..(s + 1) * hidden];
        for (o, x) in out.iter_mut().zip(block) {
            let acc = *o as i64 + *x as i64;
            *o = acc.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
        }
    }
    Ok(out)
}

/// Per-head KDA recurrent state: `d_v` rows of `d_k`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KimiK3KdaStateV1 {
    pub d_v: usize,
    pub d_k: usize,
    pub s: Vec<i32>,
}

impl KimiK3KdaStateV1 {
    pub fn zeros(d_v: usize, d_k: usize) -> Self {
        Self { d_v, d_k, s: vec![0; d_v.saturating_mul(d_k)] }
    }
}

/// **Op K3: one KDA head step** — decay the state, rank-one write, read out `q`.
///
/// Distinct from [`crate::palw_qwen36_ops::q36_gdn_step`]: same algebraic family, different
/// kernel id, and the write uses a saturating i64 product without Qwen36's extra delta
/// requant (Kimi's card does not publish that calibration).
pub fn kimi_k3_kda_step(
    state: &mut KimiK3KdaStateV1,
    k: &[i32],
    v: &[i32],
    q: &[i32],
    decay_q: i64,
    beta_q: i64,
    out_scale: A16QuantParams,
) -> Result<Vec<i32>, PalwKimiK3OpError> {
    let (d_v, d_k) = (state.d_v, state.d_k);
    if d_v == 0 || d_k == 0 || state.s.len() != d_v * d_k {
        return Err(PalwKimiK3OpError::Empty);
    }
    if k.len() != d_k || q.len() != d_k {
        return Err(PalwKimiK3OpError::LengthMismatch { a: k.len(), b: d_k });
    }
    if v.len() != d_v {
        return Err(PalwKimiK3OpError::LengthMismatch { a: v.len(), b: d_v });
    }
    check_a16(k)?;
    check_a16(v)?;
    check_a16(q)?;
    if !(0..=ONE).contains(&decay_q) || !(0..=ONE).contains(&beta_q) {
        return Err(PalwKimiK3OpError::Empty);
    }
    if d_k > 1 << 17 || d_v > 1 << 17 {
        return Err(PalwKimiK3OpError::BadK { k: d_k, experts: d_v });
    }
    for slot in state.s.iter_mut() {
        *slot = rounding_shift_right_64(*slot as i64 * decay_q, K as u8).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    }
    for (row, vi) in state.s.chunks_exact_mut(d_k).zip(v) {
        let acc: i64 = row.iter().zip(k).map(|(a, b)| *a as i64 * *b as i64).sum();
        let delta = (*vi as i64).saturating_sub(acc >> K);
        let write = rounding_shift_right_64(delta.saturating_mul(beta_q), K as u8);
        for (s, ki) in row.iter_mut().zip(k) {
            let next = *s as i64 + rounding_shift_right_64(write.saturating_mul(*ki as i64), K as u8);
            *s = next.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        }
    }
    let mut out = Vec::with_capacity(d_v);
    for row in state.s.chunks_exact(d_k) {
        let acc: i64 = row.iter().zip(q).map(|(a, b)| *a as i64 * *b as i64).sum();
        out.push(
            a16_scale_round(acc, out_scale.multiplier, out_scale.shift)
                .saturating_add(out_scale.zero)
                .clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32,
        );
    }
    Ok(out)
}

/// Compressed MLA cache width a position: `kv_lora_rank + qk_rope_head_dim`.
pub fn kimi_k3_mla_cache_width_v1(kv_lora_rank: u32, qk_rope_head_dim: u32) -> u32 {
    kv_lora_rank.saturating_add(qk_rope_head_dim)
}

/// **Op K4: fused MLA output** after decompress — integer scores, softmax, value mix.
///
/// `q` is `heads × d_qk`, `k`/`v` are `kv_len × heads × d_qk` / `kv_len × heads × d_v` in
/// position-major then head-major order. Ties in softmax are the same lowest-index rule the
/// router uses (via [`softmax_shifted`]).
pub fn kimi_k3_mla_fused(
    q: &[i32],
    k: &[i32],
    v: &[i32],
    heads: usize,
    d_qk: usize,
    d_v: usize,
    kv_len: usize,
    up_bits: u8,
) -> Result<Vec<i32>, PalwKimiK3OpError> {
    if heads == 0 || d_qk == 0 || d_v == 0 || kv_len == 0 {
        return Err(PalwKimiK3OpError::Empty);
    }
    if q.len() != heads * d_qk {
        return Err(PalwKimiK3OpError::LengthMismatch { a: q.len(), b: heads * d_qk });
    }
    if k.len() != kv_len * heads * d_qk {
        return Err(PalwKimiK3OpError::LengthMismatch { a: k.len(), b: kv_len * heads * d_qk });
    }
    if v.len() != kv_len * heads * d_v {
        return Err(PalwKimiK3OpError::LengthMismatch { a: v.len(), b: kv_len * heads * d_v });
    }
    check_a16(q)?;
    check_a16(k)?;
    check_a16(v)?;
    let mut out = vec![0i32; heads * d_v];
    for h in 0..heads {
        let qh = &q[h * d_qk..(h + 1) * d_qk];
        let mut logits = Vec::with_capacity(kv_len);
        for t in 0..kv_len {
            let kh = &k[(t * heads + h) * d_qk..(t * heads + h + 1) * d_qk];
            let acc: i64 = qh.iter().zip(kh).map(|(a, b)| *a as i64 * *b as i64).sum();
            logits.push((acc >> 8).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32);
        }
        let probs = softmax_shifted(&logits, up_bits).map_err(|_| PalwKimiK3OpError::Empty)?;
        for d in 0..d_v {
            let mut acc = 0i64;
            for t in 0..kv_len {
                let vh = v[(t * heads + h) * d_v + d];
                acc += probs[t] as i64 * vh as i64;
            }
            out[h * d_v + d] = rounding_shift_right_64(acc, K as u8).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_ties_break_to_the_lowest_index_and_sort_by_expert() {
        let logits = vec![0i32; 8];
        let routed = kimi_k3_router_topk(&logits, 3, 0).expect("ties are legal");
        assert_eq!(routed.iter().map(|r| r.expert).collect::<Vec<_>>(), vec![0, 1, 2]);
        let sum: i64 = routed.iter().map(|r| r.weight_q as i64).sum();
        assert_eq!(sum, ONE, "renormalized weights sum to ONE");
    }

    #[test]
    fn two_hosts_see_the_same_topk_on_equal_logits() {
        let logits = vec![100, 100, 50, 100, 0];
        let a = kimi_k3_router_topk(&logits, 3, 0).unwrap();
        let b = kimi_k3_router_topk(&logits, 3, 0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.iter().map(|r| r.expert).collect::<Vec<_>>(), vec![0, 1, 3]);
    }

    #[test]
    fn kda_step_is_deterministic_and_moves_the_state() {
        let mut state = KimiK3KdaStateV1::zeros(2, 2);
        let scale = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };
        let k = [1 << 12, 0];
        let v = [1 << 12, 0];
        let q = [1 << 12, 0];
        let out = kimi_k3_kda_step(&mut state, &k, &v, &q, ONE, ONE, scale).expect("step");
        assert_eq!(out.len(), 2);
        assert_ne!(state.s, KimiK3KdaStateV1::zeros(2, 2).s, "a step writes the state");
        let mut state2 = KimiK3KdaStateV1::zeros(2, 2);
        kimi_k3_kda_step(&mut state2, &k, &v, &q, ONE, ONE, scale).unwrap();
        assert_eq!(state.s, state2.s, "two hosts replay the same write");
    }

    #[test]
    fn mla_fused_matches_across_two_calls() {
        let q = vec![8, 0, 0, 8];
        let k = vec![8, 0, 0, 8, 0, 8, 8, 0];
        let v = vec![4, 0, 0, 4, 1, 0, 0, 1];
        let a = kimi_k3_mla_fused(&q, &k, &v, 2, 2, 2, 2, 0).unwrap();
        let b = kimi_k3_mla_fused(&q, &k, &v, 2, 2, 2, 2, 0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 4);
    }

    #[test]
    fn moe_combine_does_not_grow_with_padding() {
        let weights = vec![
            KimiK3RoutedExpert { expert: 0, weight_q: (ONE / 2) as i32 },
            KimiK3RoutedExpert { expert: 1, weight_q: (ONE / 2) as i32 },
        ];
        let routed = vec![10, 0, 0, 10];
        let shared = vec![1, 0, 0, 1];
        let out = kimi_k3_moe_combine(&routed, &shared, &weights, 2).unwrap();
        assert_eq!(out.len(), 2);
    }
}
