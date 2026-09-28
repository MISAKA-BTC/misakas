//! **A preview of spec 04b §8's per-position cost of a TIR program** — the numbers the network's
//! `palw_tir_v1` ceilings bound, computed by the lowerer so `palw-class check-architecture` can say
//! which ceiling a program would exceed before `tir_admit_v1` (the normative computation, tir/core)
//! exists. Everything is at the worst case `H = min(window, max_context)`.
//!
//! * **MACs**: `E(out) · K` per `MatMul`, summed over every occurrence of one position.
//! * **State bytes**: `Fixed` states at their size, `Hist` states at `min(window, max_context)` rows,
//!   per instance (one per layer for a per-layer state).
//! * **Peak live bytes**: nodes in index order; an output is live from its node to its last
//!   consumer, or to the end of the occurrence if it is a root (committed, a state write or append,
//!   a carry-out, the logits); the peak over the position's occurrences.

use misaka_palw_tir::program::{Ref, StateKind};
use misaka_palw_tir::{Dim, Prim, TirProgramV1};
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ProgramCostV1 {
    /// Nodes of one position: `pre`, every layer's block, `post`.
    pub unrolled_nodes: u64,
    pub macs: u128,
    pub elementwise: u128,
    pub transcendental: u128,
    pub state_bytes: u128,
    pub peak_live_bytes: u128,
    /// The `H` the numbers are at, per block (`None` for a block without a window).
    pub h: Vec<Option<u32>>,
}

fn elems(shape: &[Dim], h: u32) -> u128 {
    shape
        .iter()
        .map(|d| match d {
            Dim::Fixed(n) => *n as u128,
            Dim::H => h as u128,
        })
        .product()
}

/// An operand's shape inside block `b`.
fn operand_shape(p: &TirProgramV1, b: &misaka_palw_tir::Block, r: Ref) -> Vec<Dim> {
    let fixed = |s: &[u32]| s.iter().map(|x| Dim::Fixed(*x)).collect();
    match r {
        Ref::Node(j) => b.nodes[j as usize].out.shape.clone(),
        Ref::Param(j) => fixed(&p.params[j as usize].shape),
        Ref::CarryIn(c) => b.carry_in[c as usize].shape.clone(),
        Ref::Const(j) => fixed(&p.consts[j as usize].shape),
        Ref::State(j) => fixed(&p.states[j as usize].shape),
        Ref::Input(_) => Vec::new(),
    }
}

/// The cost of one position at `max_context`.
pub fn program_cost_v1(p: &TirProgramV1, max_context: u32) -> ProgramCostV1 {
    // A block's window: the window of the Hist states it appends to.
    let windows: Vec<Option<u32>> = p
        .blocks
        .iter()
        .map(|b| {
            b.nodes.iter().find_map(|n| match n.prim {
                Prim::HistAppend { state } => match p.states[state as usize].kind {
                    StateKind::Hist { window } => Some(window),
                    StateKind::Fixed { .. } => None,
                },
                _ => None,
            })
        })
        .collect();
    let hs: Vec<Option<u32>> = windows.iter().map(|w| w.map(|w| w.min(max_context).max(1))).collect();
    let mut per_block = Vec::with_capacity(p.blocks.len());
    for (bi, b) in p.blocks.iter().enumerate() {
        let h = hs[bi].unwrap_or(1);
        let (mut macs, mut ew, mut tr) = (0u128, 0u128, 0u128);
        for n in &b.nodes {
            let e = elems(&n.out.shape, h);
            match &n.prim {
                Prim::MatMul => {
                    let k = match operand_shape(p, b, n.inputs[0]).last() {
                        Some(Dim::Fixed(k)) => *k as u128,
                        Some(Dim::H) => h as u128,
                        None => 1,
                    };
                    macs += e * k;
                }
                Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => tr += e,
                Prim::TopK { k, .. } => ew += elems(&operand_shape(p, b, n.inputs[0]), h) * *k as u128,
                Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => ew += elems(&operand_shape(p, b, n.inputs[0]), h),
                _ => ew += e,
            }
        }
        // Peak live bytes of one occurrence.
        let len = b.nodes.len();
        let mut last = vec![0usize; len];
        for (i, n) in b.nodes.iter().enumerate() {
            last[i] = i;
            for r in &n.inputs {
                if let Ref::Node(j) = r {
                    last[*j as usize] = last[*j as usize].max(i);
                }
            }
        }
        for (i, n) in b.nodes.iter().enumerate() {
            let root =
                n.commit || matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. }) || b.carry_out.contains(&(i as u16));
            if root {
                last[i] = len;
            }
        }
        let bytes: Vec<u128> = b.nodes.iter().map(|n| elems(&n.out.shape, h) * n.out.dtype.width() as u128).collect();
        let mut delta = vec![0i128; len + 2];
        for i in 0..len {
            delta[i] += bytes[i] as i128;
            delta[last[i] + 1] -= bytes[i] as i128;
        }
        let (mut live, mut peak) = (0i128, 0i128);
        for d in delta.iter().take(len + 1) {
            live += d;
            peak = peak.max(live);
        }
        per_block.push((b.nodes.len() as u64, macs, ew, tr, peak as u128));
    }
    let mut out = ProgramCostV1 { h: hs, ..Default::default() };
    let occ: Vec<u8> =
        std::iter::once(p.schedule.pre).chain(p.schedule.layers.iter().copied()).chain(std::iter::once(p.schedule.post)).collect();
    for b in occ {
        let (n, m, e, t, peak) = per_block[b as usize];
        out.unrolled_nodes += n;
        out.macs += m;
        out.elementwise += e;
        out.transcendental += t;
        out.peak_live_bytes = out.peak_live_bytes.max(peak);
    }
    let layers = p.schedule.layers.len() as u128;
    for s in &p.states {
        let row: u128 = s.shape.iter().map(|x| *x as u128).product::<u128>() * s.dtype.width() as u128;
        let one = match s.kind {
            StateKind::Fixed { .. } => row,
            StateKind::Hist { window } => row * window.min(max_context) as u128,
        };
        out.state_bytes += if s.per_layer { one * layers } else { one };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dense_program_costs_what_its_projections_say() {
        let cfg =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hf/llama/config.json")).expect("fixture");
        let spec = crate::parse_config_str(&cfg).expect("spec");
        let hl = crate::hl::build_program(&spec).expect("hl");
        let lw = crate::lower::lower(&hl, &crate::lower::LowerOpts::default()).expect("lowered");
        let c = program_cost_v1(&lw.program, 64);
        // Tiny llama: d 32, heads 4 × 8, kv 2 × 8, ff 64, vocab 64, 2 layers. Projections per layer:
        // q 32·32, k 16·32, v 16·32, o 32·32, gate/up 64·32 each, down 32·64; attention over
        // H = 64: scores 4·8·64 and values 4·64·8; the (untied) head 64·32. Every projection input
        // is split (norm rows, the context: 8 outlier columns; the GLU hidden: 16), which adds
        // `out · k`, the head's included.
        let per_layer = 32 * 32 * 2 + 16 * 32 * 2 + 64 * 32 * 3 + 2 * 4 * 8 * 64;
        let split = (32 + 16 + 16 + 32 + 64 + 64) * 8 + 32 * 16;
        assert_eq!(c.macs, (2 * (per_layer + split) + 64 * 32 + 64 * 8) as u128);
        // K and V histories: 2 layers × 2 × 64 rows × 16 i16.
        assert_eq!(c.state_bytes, 2 * 2 * 64 * 16 * 2);
        assert!(c.peak_live_bytes > 0 && c.unrolled_nodes > 100);
    }
}
