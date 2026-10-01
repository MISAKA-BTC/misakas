//! **Per-position cost estimates of an HL program** (for `palw-tir-check`; RFC-0002 §5.3 will
//! derive exact costs from the expanded TIR in Gate 2). Everything is counted per position:
//! history-dependent work as a coefficient of the visible history length `H`.

use super::*;
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CostReport {
    /// MACs that do not depend on the history length (projections, experts, recurrences).
    pub macs_fixed: u64,
    /// MACs per visible history row, summed over layers, for layers without a window.
    pub macs_per_hist_row: u64,
    /// MACs per visible history row for windowed layers, as `(window, per-row MACs)`.
    pub windowed: Vec<(usize, u64)>,
    /// Elementwise ops (activations, norms, adds, rope).
    pub elementwise: u64,
    /// Transcendental evaluations (exp, tanh, erf, sigmoid, softplus, rsqrt) excluding softmax.
    pub transcendental: u64,
    /// Elements of all `Fixed` states over all layers.
    pub fixed_state_elems: u64,
    /// Elements appended to `Hist` states per position, over all layers.
    pub hist_elems_per_pos: u64,
    /// Param elements (per-layer params counted once per bound layer).
    pub param_elems: u64,
    /// Active expert fraction note: routed experts counted at top-k only.
    pub routed_expert_macs: u64,
}

impl CostReport {
    /// Total MACs at visible history `h` (windowed layers see `min(h, window)`).
    pub fn macs_at(&self, h: usize) -> u64 {
        let mut m = self.macs_fixed + self.macs_per_hist_row * h as u64;
        for (w, c) in &self.windowed {
            m += c * h.min(*w) as u64;
        }
        m
    }
}

fn numel(s: &[usize]) -> u64 {
    s.iter().product::<usize>() as u64
}

pub fn estimate(p: &HlProgram) -> CostReport {
    let mut r = CostReport::default();
    let mut per_block: Vec<CostReport> = Vec::new();
    for b in &p.blocks {
        let mut c = CostReport::default();
        for n in &b.nodes {
            let out: u64 = n.outs.first().map(|s| numel(s)).unwrap_or(0);
            match &n.op {
                Op::Linear { lora, .. } => {
                    let inp = match n.inputs[1] {
                        Ref::Param(pi) => p.params[pi as usize].shape.get(1).copied().unwrap_or(0) as u64,
                        _ => 0,
                    };
                    c.macs_fixed += out * inp;
                    if let Some(l) = lora {
                        c.macs_fixed += l.rank as u64 * (inp + out);
                    }
                }
                Op::Attention { heads, head_dim, v_head_dim, window, .. } => {
                    let per = (*heads * (*head_dim + *v_head_dim)) as u64;
                    match window {
                        Some(w) => c.windowed.push((*w, per)),
                        None => c.macs_per_hist_row += per,
                    }
                    c.transcendental += *heads as u64; // per row, approximated below by softmax exps
                }
                Op::MlaAttention { heads, nope, rope, v_dim, kv_lora, indexer, .. } => {
                    let (h, r_) = (*heads as u64, *kv_lora as u64);
                    c.macs_fixed += h * (*nope as u64 * r_ + *v_dim as u64 * r_);
                    c.macs_per_hist_row += h * (2 * r_ + *rope as u64);
                    if let Some(ix) = indexer {
                        // The scorer: one dot per (index head, history row), then the head-weighted sum; the top-k
                        // threshold's radix passes are elementwise over the row (about 16 compares and adds a pass).
                        c.macs_per_hist_row += (ix.heads * ix.dim + ix.heads) as u64;
                        c.elementwise += 0;
                    }
                }
                Op::GatedDelta { v_heads, dk, dv, .. } => c.macs_fixed += 4 * (*v_heads * *dk * *dv) as u64,
                Op::SelectiveScan { inner, state } => c.macs_fixed += 3 * (*inner * *state) as u64,
                Op::Ssd { heads, head_dim, state, .. } => c.macs_fixed += 3 * (*heads * *head_dim * *state) as u64,
                Op::CausalConv1d { channels, kernel, .. } => c.macs_fixed += (*channels * *kernel) as u64,
                Op::Wkv4 => c.elementwise += 12 * out,
                Op::Wkv6 { heads, head_size } => c.macs_fixed += 3 * (*heads * *head_size * *head_size) as u64,
                Op::Wkv7 { heads, head_size } => c.macs_fixed += 4 * (*heads * *head_size * *head_size) as u64,
                Op::MoeExperts { top_k, .. } => {
                    if let (Ref::Param(g), Ref::Param(d)) = (n.inputs[3], n.inputs[5]) {
                        let gs = &p.params[g as usize].shape;
                        let ds = &p.params[d as usize].shape;
                        let m = *top_k as u64 * (2 * numel(&gs[1..]) + numel(&ds[1..]));
                        c.macs_fixed += m;
                        c.routed_expert_macs += m;
                    }
                }
                Op::Act(_) | Op::Xielu | Op::Softcap { .. } | Op::DecayExpNegExp | Op::ClampedSwiGlu { .. } => {
                    c.elementwise += out;
                    c.transcendental += out;
                }
                Op::Norm { groups, .. } | Op::L2Norm { groups, .. } | Op::GatedRmsNorm { groups, .. } => {
                    c.elementwise += 3 * out;
                    c.transcendental += *groups as u64;
                }
                Op::Rope { heads, rotary_dim, .. } | Op::RopeAtBlock { heads, rotary_dim, .. } => c.elementwise += 3 * (*heads * *rotary_dim) as u64,
                Op::StreamMean { .. } | Op::StreamOuter { .. } | Op::GroupDot { .. } | Op::GroupRepeat { .. } | Op::GatherRows { .. } | Op::BlockMean { .. } => c.elementwise += out.max(1),
                // the hash: a few dozen elementwise ops over 63-bit vectors per order
                Op::NgramIds { ple, .. } => c.elementwise += 63 * 6 * ple.ngram_size as u64,
                // block scores: one dot per (head, block), the keys written back
                Op::BlockSelect { heads, dim, blocks, .. } => {
                    c.macs_fixed += (*heads * *dim * *blocks) as u64;
                    c.elementwise += *blocks as u64;
                }
                Op::BlockWrite { blocks, .. } => c.elementwise += *blocks as u64,
                Op::Route { experts, .. } => {
                    c.elementwise += 2 * *experts as u64;
                    c.transcendental += *experts as u64;
                }
                Op::Embedding
                | Op::PosEmbedding { .. }
                | Op::Slice { .. }
                | Op::Concat
                | Op::Zeros
                | Op::HistAppend
                | Op::TokenShift => {}
                Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Scale { .. }
                | Op::Clamp { .. }
                | Op::Lerp
                | Op::PosScale { .. }
                | Op::ScaleParam => c.elementwise += out,
            }
        }
        per_block.push(c);
    }
    let mut add = |c: &CostReport| {
        r.macs_fixed += c.macs_fixed;
        r.macs_per_hist_row += c.macs_per_hist_row;
        r.windowed.extend(c.windowed.iter().cloned());
        r.elementwise += c.elementwise;
        r.transcendental += c.transcendental;
        r.routed_expert_macs += c.routed_expert_macs;
    };
    add(&per_block[p.pre]);
    add(&per_block[p.post]);
    for k in &p.schedule {
        add(&per_block[*k as usize]);
    }
    // Merge windows.
    let mut w: std::collections::BTreeMap<usize, u64> = Default::default();
    for (a, b) in &r.windowed {
        *w.entry(*a).or_default() += b;
    }
    r.windowed = w.into_iter().collect();
    for (si, s) in p.states.iter().enumerate() {
        let layers = p.schedule.iter().filter(|k| p.block_states(**k as usize).contains(&(si as u32))).count() as u64;
        match s.kind {
            StateKind::Fixed => r.fixed_state_elems += layers * numel(&s.shape),
            StateKind::Hist { .. } => r.hist_elems_per_pos += layers * numel(&s.shape),
        }
    }
    for (pi, d) in p.params.iter().enumerate() {
        let n = numel(&d.shape);
        if d.per_layer {
            let layers = p.schedule.iter().filter(|k| p.block_params(**k as usize).contains(&(pi as u32))).count() as u64;
            r.param_elems += layers * n;
        } else {
            r.param_elems += n;
        }
    }
    r
}
