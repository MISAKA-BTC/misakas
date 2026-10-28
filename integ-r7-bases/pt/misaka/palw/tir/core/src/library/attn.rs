//! **Attention.**
//!
//! [`BlockBuilder::attention`] is grouped-query attention over a block's history window — MHA
//! (`kv_heads = heads`), GQA, MQA (`kv_heads = 1`); a sliding window is the block's `Hist` window
//! (`H = min(pos + 1, W)`, exactly HF's `kv_idx > q_idx − W`), with optional score soft-capping,
//! ALiBi and a sink. The softmax is the two-pass form over `H`, which is what makes the court's
//! history dissection (spec 04b §10.3) apply.
//!
//! [`BlockBuilder::mla_absorbed`] is DeepSeek's multi-head latent attention in the absorbed form:
//! the history row is the latent `c_kv` (plus the shared rotary key), and the per-head up
//! projections are folded into the query and the output — two chained contractions with a
//! narrowing between them.
//!
//! The query, keys and values are codes; the logits and probabilities are Q24; the history-wide
//! products are exact `i64` sums (`H · 2^25 · 2^15 < 2^63` for every legal window).

use crate::builder::BlockBuilder;
use crate::library::Narrowing;
use crate::program::Ref;
use crate::types::{DType, Dim};

/// What [`BlockBuilder::attention`] needs besides its three operands.
#[derive(Clone, Copy, Debug)]
pub struct AttnCfg {
    pub heads: u32,
    pub kv_heads: u32,
    pub head_dim: u32,
    /// `q·k` (an exact `i64` dot of codes) to Q24 logits — the `1/√d` scale (or Gemma's
    /// `query_pre_attn_scalar`) and both code scales are its multiplier.
    pub score: Narrowing,
    /// Soft-capping `cap·tanh(s/cap)` of the Q24 logits (Gemma 2), `cap` a Q24 `i32` at their scale.
    pub softcap: Option<Ref>,
    /// ALiBi slopes `[heads]` (Q24 at the logits' scale).
    pub alibi: Option<Ref>,
    /// A sink logit per head `[heads]` (gpt-oss), Q24 at the logits' scale.
    pub sink: Option<Ref>,
    /// The softmax's widening shift (`softmax_shifted`'s `up`).
    pub up_bits: u32,
    /// `P·V` (Q24 probabilities against value codes, exact `i64`) back to codes.
    pub value: Narrowing,
}

/// What [`BlockBuilder::mla_absorbed`] needs besides its operands.
#[derive(Clone, Copy, Debug)]
pub struct MlaCfg {
    pub heads: u32,
    /// `W_kbᵀ·q_nope` (`i64`) to codes: the absorbed query, one per head, in the latent's space.
    pub q_latent: Narrowing,
    /// The latent part of the score, `q̃·c` (`i64`), to Q24 logits.
    pub score_latent: Narrowing,
    /// The rotary part of the score, `q_rope·k_rope` (`i64`), to Q24 logits.
    pub score_rope: Narrowing,
    pub up_bits: u32,
    /// `Σ p·c` (`i64`) to latent codes.
    pub ctx: Narrowing,
    /// `W_vb·ctx` (`i64`) to output codes.
    pub out: Narrowing,
}

impl BlockBuilder<'_> {
    /// **Grouped-query attention** of one position: `q:code[heads·d]` against the key and value
    /// windows `k`, `v:code[H, kv_heads·d]` (the values of the block's two `HistAppend`s). Query
    /// head `h` reads kv head `h / (heads / kv_heads)` — HF's `repeat_kv`. Returns `code[heads·d]`.
    ///
    /// `scores = N_score(q·Kᵀ)` → [soft-cap] → [+ ALiBi] → two-pass softmax over `H` [with the
    /// sink in the denominator] → `N_value(P·V)`.
    pub fn attention(&mut self, q: Ref, k: Ref, v: Ref, cfg: &AttnCfg) -> Ref {
        let (heads, kv, d) = (cfg.heads, cfg.kv_heads, cfg.head_dim);
        let g = heads / kv;
        let q3 = self.reshape_fixed(q, &[kv, g, d]);
        let k3 = self.reshape(k, &[Dim::H, Dim::Fixed(kv), Dim::Fixed(d)]);
        let kt = self.transpose(k3, &[1, 2, 0]);
        let s = self.matmul(q3, kt, DType::I64);
        let mut logits = self.narrow_wide(s, &cfg.score);
        if let Some(cap) = cfg.softcap {
            logits = self.softcap_q24(logits, cap);
        }
        if let Some(slopes) = cfg.alibi {
            let sl = self.reshape_fixed(slopes, &[kv, g, 1]);
            let biased = self.alibi(logits, sl);
            logits = self.clamp(biased, i32::MIN as i64, i32::MAX as i64, DType::I32);
        }
        let p = match cfg.sink {
            Some(sink) => {
                let sk = self.reshape_fixed(sink, &[kv, g, 1]);
                self.softmax_with_sink(logits, sk, cfg.up_bits)
            }
            None => self.softmax_shifted(logits, cfg.up_bits),
        };
        let v3 = self.reshape(v, &[Dim::H, Dim::Fixed(kv), Dim::Fixed(d)]);
        let vt = self.transpose(v3, &[1, 0, 2]);
        let o = self.matmul(p, vt, DType::I64);
        let o = self.narrow_codes(o, &cfg.value);
        self.reshape_fixed(o, &[heads * d])
    }

    /// **Multi-head latent attention, absorbed** (DeepSeek-V2/V3, MiniCPM3, Kimi's MLA layers).
    ///
    /// * `q_nope:code[heads, dn]`, `q_rope:code[heads, dr]` (rotated by the caller);
    /// * `c_kv:code[H, r]` — the window of latent rows (`kv_a_proj`'s latent, normed), and
    ///   `k_rope:code[H, dr]` — the window of the ONE rotary key all heads share;
    /// * `w_kb:w8[heads, dn, r]` and `w_vb:w8[heads, r, dv]` — `kv_b_proj` split per head, the
    ///   value half stored transposed.
    ///
    /// `q̃_h = N(W_kb,hᵀ·q_nope,h)` (contraction 1, over `dn`), `logits = N(q̃·cᵀ) + N(q_rope·k_ropeᵀ)`,
    /// two-pass softmax over `H`, `ctx = N(Σ p·c)` (over `H`), `o_h = N(W_vb,hᵀ·ctx_h)` (contraction
    /// 2, over `r`). Returns `code[heads·dv]`. Absorption moves rounding points relative to HF's
    /// expanded cache (a fidelity matter, corpus §4); it costs no primitive.
    #[allow(clippy::too_many_arguments)]
    pub fn mla_absorbed(&mut self, q_nope: Ref, q_rope: Ref, c_kv: Ref, k_rope: Ref, w_kb: Ref, w_vb: Ref, cfg: &MlaCfg) -> Ref {
        let h = cfg.heads;
        let qn = self.shape(q_nope);
        let Dim::Fixed(dn) = qn[1] else { panic!("static") };
        let qr = self.shape(q_rope);
        let Dim::Fixed(dr) = qr[1] else { panic!("static") };
        let vb = self.shape(w_vb);
        let Dim::Fixed(dv) = vb[2] else { panic!("static") };
        // Contraction 1: q̃ = W_kbᵀ q_nope, per head, into the latent space.
        let qn3 = self.reshape_fixed(q_nope, &[h, 1, dn]);
        let qt = self.matmul(qn3, w_kb, DType::I64);
        let qt = self.narrow_codes(qt, &cfg.q_latent);
        // Scores over the window: the latent part and the shared rotary part, each to Q24.
        let ct = self.transpose(c_kv, &[1, 0]);
        let s1 = self.matmul(qt, ct, DType::I64);
        let l1 = self.narrow_wide(s1, &cfg.score_latent);
        let qr3 = self.reshape_fixed(q_rope, &[h, 1, dr]);
        let kt = self.transpose(k_rope, &[1, 0]);
        let s2 = self.matmul(qr3, kt, DType::I64);
        let l2 = self.narrow_wide(s2, &cfg.score_rope);
        let l = self.add(l1, l2, DType::I64);
        let l = self.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let p = self.softmax_shifted(l, cfg.up_bits);
        // Σ p·c over the window, in the latent space.
        let ctx = self.matmul(p, c_kv, DType::I64);
        let ctx = self.narrow_codes(ctx, &cfg.ctx);
        // Contraction 2: o = W_vbᵀ ctx, per head.
        let o = self.matmul(ctx, w_vb, DType::I64);
        let o = self.narrow_codes(o, &cfg.out);
        self.reshape_fixed(o, &[h * dv])
    }
}
