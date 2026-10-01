//! **What a check costs, counted on a program's shapes** (RFC-0007 Part II, §II.9; the measurement
//! tool's tables).
//!
//! Everything here is a function of the program, the analysis and the history length — no weights
//! and no execution — so it runs on the real classes' programs lowered from their `config.json`
//! alone, at published sizes no test could materialise. Per position, at history length `h`:
//!
//! * a **recompute** costs every `MatMul`'s multiply-adds and reads every weight the position touches
//!   (a routed weight: only the gathered experts' slices);
//! * an **algebraic check** costs each served `MatMul`'s check (`crate::geom`: `|out| + rows·K`,
//!   plus the operand's sketch for a fresh vector), the exact recompute of every other node the check
//!   needs (elementwise work), the `MatMul`s its policy recomputes, and the served values' bytes.
//!
//! Byte counts are given at the served values' declared width (`i64` = 8 bytes) and packed to the
//! width of the node's proven interval (`⌈log2(span + 1)⌉` bits; for a program lowered without
//! weights the interval assumes type-worst-case weights, so the packed figure is an upper bound).

use misaka_palw_tir::Prim;
use misaka_palw_tir::program::Ref;
use misaka_palw_tir_exec::TirPlan;

use crate::analysis::{TirCheckPolicyV1, TirMatMulKindV1, TirSketchAnalysisV1, TirWeightSourceV1};
use crate::field::tir_sketch_moduli_for_span_v1;
use crate::geom::TirCheckGeomV1;

/// One position's costs at one history length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TirPositionCostV1 {
    /// Multiply-adds a recompute spends on weight products, and on activation × activation ones.
    pub recompute_weight_macs: u64,
    pub recompute_act_macs: u64,
    /// Weight bytes a recompute reads (gathered experts only).
    pub weight_bytes_read: u64,
    /// Field multiply-adds of the checks: against sketches, and with fresh vectors.
    pub check_weight_terms: u64,
    pub check_fresh_terms: u64,
    /// Multiply-adds of the activation × activation products the policy recomputes.
    pub exact_act_macs: u64,
    /// Elements every other needed node recomputes (norms, narrowings, softmax …).
    pub exact_elements: u64,
    /// Served elements: weight products, activation × activation products.
    pub served_weight_elements: u64,
    pub served_act_elements: u64,
    /// Served bytes at declared width, and packed to the proven interval.
    pub served_bytes: u64,
    pub served_packed_bits: u64,
    /// Committed lanes (4 bytes each) — what a dense capture serves today.
    pub committed_lanes: u64,
}

impl TirPositionCostV1 {
    pub fn add(&mut self, o: &Self) {
        self.recompute_weight_macs += o.recompute_weight_macs;
        self.recompute_act_macs += o.recompute_act_macs;
        self.weight_bytes_read += o.weight_bytes_read;
        self.check_weight_terms += o.check_weight_terms;
        self.check_fresh_terms += o.check_fresh_terms;
        self.exact_act_macs += o.exact_act_macs;
        self.exact_elements += o.exact_elements;
        self.served_weight_elements += o.served_weight_elements;
        self.served_act_elements += o.served_act_elements;
        self.served_bytes += o.served_bytes;
        self.served_packed_bits += o.served_packed_bits;
        self.committed_lanes += o.committed_lanes;
    }
}

/// A class's one-off figures: what the sketches hold against what they stand for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TirClassCostV1 {
    /// Weight `MatMul` sites (per occurrence) and how many need two or three moduli.
    pub weight_sites: u64,
    pub wide_sites: u64,
    /// Field entries the sketch store holds (sketches and site vectors, every modulus), 8 bytes each.
    pub sketch_entries: u64,
    /// Elements and bytes of the weights the sketches stand for.
    pub sketched_weight_elements: u64,
    pub sketched_weight_bytes: u64,
    /// Every param instance's bytes; the ones the checker must hold; the ones only sketches need.
    pub param_bytes: u64,
    pub held_param_bytes: u64,
    pub sketched_param_bytes: u64,
}

fn bits_of(span: u128) -> u64 {
    (128 - span.leading_zeros()) as u64
}

fn elements_of(plan: &TirPlan, block: u8, r: Ref, h: usize) -> u64 {
    let p = &plan.program;
    let b = &p.blocks[block as usize];
    match r {
        Ref::Node(j) => b.nodes[j as usize].out.elements_at(h as u64),
        Ref::CarryIn(k) => b.carry_in[k as usize].elements_at(h as u64),
        Ref::Param(j) => p.params[j as usize].shape.iter().map(|d| *d as u64).product(),
        Ref::Const(j) => p.consts[j as usize].shape.iter().map(|d| *d as u64).product(),
        Ref::State(j) => p.states[j as usize].shape.iter().map(|d| *d as u64).product(),
        Ref::Input(_) => 1,
    }
}

/// **One position's costs** at history length `h` (`H = min(h, W)` per block), with `post` run
/// or not.
pub fn tir_position_cost_v1(
    plan: &TirPlan,
    analysis: &TirSketchAnalysisV1,
    policy: &TirCheckPolicyV1,
    h: usize,
    run_post: bool,
) -> TirPositionCostV1 {
    let p = &plan.program;
    let mut c = TirPositionCostV1::default();
    let post_occ = plan.occurrences.len() - 1;
    for (occ, &(block, _)) in plan.occurrences.iter().enumerate() {
        if occ == post_occ && !run_post {
            break;
        }
        let bp = &plan.blocks[block as usize];
        let hb = bp.window.map(|w| h.min(w as usize)).unwrap_or(1).max(1);
        let needed = analysis.checker_needed(p, block, hb, policy);
        let b = &p.blocks[block as usize];
        for (ni, node) in b.nodes.iter().enumerate() {
            let np = &bp.nodes[ni];
            let out_elems = node.out.elements_at(hb as u64);
            if node.commit {
                c.committed_lanes += out_elems;
            }
            let served = analysis.witnessed(p, block, ni as u16, hb, policy);
            if node.prim == Prim::MatMul {
                let site = analysis.site(block, ni as u16).expect("every MatMul has a site");
                let k = np.in_types[0].resolve(hb).last().copied().unwrap_or(1) as u64;
                let macs = out_elems * k;
                let span = np.facts.out.hi.abs_diff(np.facts.out.lo);
                let mods = tir_sketch_moduli_for_span_v1(span).len() as u64;
                match site.kind {
                    TirMatMulKindV1::Weight { side, source } => {
                        c.recompute_weight_macs += macs;
                        let routed_rank = match source {
                            TirWeightSourceV1::Routed { idx_rank, .. } => idx_rank as usize,
                            TirWeightSourceV1::Static(_) => 0,
                        };
                        let g = TirCheckGeomV1::new(side, &np.in_types[0], &np.in_types[1], hb, routed_rank);
                        let w_width = match source.data() {
                            Ref::Param(j) => p.params[j as usize].dtype.width() as u64,
                            _ => 1,
                        };
                        let w_elems = match source {
                            TirWeightSourceV1::Routed { idx, .. } => elements_of(plan, block, idx, hb) * g.body_len() as u64,
                            TirWeightSourceV1::Static(r) => elements_of(plan, block, r, hb),
                        };
                        c.weight_bytes_read += w_elems * w_width;
                        c.check_weight_terms += g.check_terms() * mods;
                        c.served_weight_elements += out_elems;
                        c.served_bytes += out_elems * node.out.dtype.width() as u64;
                        c.served_packed_bits += out_elems * bits_of(span);
                    }
                    TirMatMulKindV1::ActAct | TirMatMulKindV1::Exact => {
                        c.recompute_act_macs += macs;
                        if served {
                            let g = TirCheckGeomV1::new(crate::analysis::TirSideV1::Right, &np.in_types[0], &np.in_types[1], hb, 0);
                            c.check_fresh_terms += (g.check_terms() + g.body_len() as u64) * mods;
                            c.served_act_elements += out_elems;
                            c.served_bytes += out_elems * node.out.dtype.width() as u64;
                            c.served_packed_bits += out_elems * bits_of(span);
                        } else if needed[ni] {
                            c.exact_act_macs += macs;
                        }
                    }
                }
            } else if needed[ni] {
                c.exact_elements += out_elems;
            }
        }
    }
    c
}

/// **A class's one-off figures** (sketch store against the weights), with `h_max` the longest
/// history the held-param analysis considers.
pub fn tir_class_cost_v1(plan: &TirPlan, analysis: &TirSketchAnalysisV1, policy: &TirCheckPolicyV1, h_max: usize) -> TirClassCostV1 {
    let p = &plan.program;
    let mut c = TirClassCostV1::default();
    for &(block, _) in &plan.occurrences {
        let bp = &plan.blocks[block as usize];
        for site in &analysis.blocks[block as usize].matmuls {
            let TirMatMulKindV1::Weight { side, source } = site.kind else { continue };
            let np = &bp.nodes[site.node as usize];
            let span = np.facts.out.hi.abs_diff(np.facts.out.lo);
            let mods = tir_sketch_moduli_for_span_v1(span).len() as u64;
            let routed_rank = match source {
                TirWeightSourceV1::Routed { idx_rank, .. } => idx_rank as usize,
                TirWeightSourceV1::Static(_) => 0,
            };
            let g = TirCheckGeomV1::new(side, &np.in_types[0], &np.in_types[1], 1, routed_rank);
            let experts = match source.data() {
                Ref::Param(j) if routed_rank > 0 => p.params[j as usize].shape[0] as u64,
                _ => 1,
            };
            let w_elems = experts * g.body_len() as u64;
            let w_width = match source.data() {
                Ref::Param(j) => p.params[j as usize].dtype.width() as u64,
                _ => 1,
            };
            c.weight_sites += 1;
            c.wide_sites += (mods > 1) as u64;
            c.sketch_entries += mods * (g.v_len() as u64 + experts * g.s_len() as u64);
            c.sketched_weight_elements += w_elems;
            c.sketched_weight_bytes += w_elems * w_width;
        }
    }
    let bytes_of = |j: u16| -> u64 {
        let d = &p.params[j as usize];
        d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
    };
    let instances = |j: u16| plan.param_instances.iter().filter(|(pj, _)| *pj == j).count() as u64;
    c.param_bytes = plan.param_instances.iter().map(|(j, _)| bytes_of(*j)).sum();
    c.held_param_bytes = analysis.held_params(p, h_max, policy).iter().map(|j| bytes_of(*j) * instances(*j)).sum();
    c.sketched_param_bytes = analysis.sketched_params(p, h_max, policy).iter().map(|j| bytes_of(*j) * instances(*j)).sum();
    c
}
