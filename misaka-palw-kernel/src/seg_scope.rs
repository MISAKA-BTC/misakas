//! **K2-TIR-v4/v5 under DA16b's court scope** (`crate::scope`; `docs/design/palw/k2-real-scale.md` §4.2) — option (c), the Lead's
//! 2026-10-10 decision: a SMALL model-dependent value is never served; its court is a whole-value recompute.
//!
//! DA16b's clear budget is 0 for every LM (an elementwise model relation is revealed by one position), so every model-dependent value
//! (`crate::scope::court_model_dependent_mask_v1`, Level 2, which contains Level 1's model bytes) is owed only as commitment structure.
//! A value is **withheld** when it is model-dependent AND its whole-value court fits [`SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1`]:
//!
//! * a demanded position serves a withheld value as its node opening alone (`ChunkBodyV1::Withheld`): its commitment and path, never an
//!   element, a leaf or a leaf hash;
//! * a verifier holds the model (ADR-0177 D7) and re-executes: its own value of a withheld node authenticates iff its commitment is the
//!   producer's, and a different commitment is a fault;
//! * the court (`SegFaultV1::WholeValue`) recomputes EVERY element of the value from the filed operands — the filer's own earlier values
//!   (authenticated whole against their commitments, or by leaves), param leaves from the filer's own copy against the registered v3
//!   commitments — and convicts iff the recomputed commitment is not the committed one, or the semantics refuse an element.
//!
//! A model-dependent value whose whole-value court does NOT fit stays served in the clear: DA16b's option (a), flat 64-element hiding
//! tiles, is its planned remedy and is not built. [`seg_scope_report_v1`] reports this gap. The report is not yet connected to the
//! ledger's reward gate; a false report is a remaining implementation gap, not an enforced refusal of rewards.
//!
//! The decision is a pure function of the program (its shapes at the worst `H`), so the ledger's DA layout, the gate's price and every
//! verifier agree on it without any declaration.

use misaka_palw_tir::program::{INPUT_TOKEN, Ref, TirProgramV1};
use misaka_palw_tir::{DType, Prim};

use crate::merkle::AXIS_ROW;
use crate::merkle3::LayoutV3;
use crate::seg::PROMPT_TILE_IDS_V1;

/// **The most a whole-value court of a withheld value may file**, in bytes (below the node's 1,583,616-byte carrier, with room for the
/// filing header). A model-dependent value whose court is larger is not withheld.
pub const SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1: u64 = 1 << 20;
/// A commitment-only response must also have an executable court under the v4/v5 descriptor's work ceiling.
pub const SEG_WHOLE_VALUE_COURT_MAX_WORK_V1: u64 = 1 << 36;

/// How many of the program's params are weights: all of them, but a K2-TIR-v5 encoder's trailing job inputs (ids, count).
pub fn seg_model_params_v1(program: &TirProgramV1) -> usize {
    crate::seg_encoder::encoder_binding_v1(program).map(|b| b.first_input as usize).unwrap_or(program.params.len())
}

/// A whole tensor as a filing carries it: dtype, shape, the elements at the dtype's width.
pub fn whole_tensor_bytes_v1(shape: &[usize], dtype: DType) -> u64 {
    1 + (4 + 8 * shape.len() as u64) + (4 + LayoutV3::of(shape).len * dtype.width() as u64)
}

/// The bytes of EVERY row leaf of a tensor (each at the tree's full depth): what opening all of it by leaves files.
fn all_leaves_bytes(shape: &[usize], dtype: DType) -> u64 {
    let l = LayoutV3::of(shape);
    let leaves = l.leaves(AXIS_ROW);
    l.len * dtype.width() as u64 + leaves * ((crate::merkle::depth(leaves) + 1) * 64 + 8 * shape.len() as u64 + 32)
}

/// **The price of a whole-value court of `(occurrence s, node n)`** (`(bytes, work)`): the filing's fixed fields and the output's node
/// opening (no leaf: its elements are recomputed), and per operand an upper bound of what a filer opens — a committed operand whole or
/// by every leaf (whichever is larger), a param by every leaf, a token or job ids by their tile. A filer opens only what the elements
/// read, so every filing is within it.
pub fn whole_value_court_cost_v1(program: &TirProgramV1, s: usize, n: usize, node_count: u64) -> (u64, u64) {
    whole_value_court_cost_in_v1(program, s, n, node_count, true)
}

fn whole_value_court_cost_in_v1(program: &TirProgramV1, s: usize, n: usize, node_count: u64, flat_gather: bool) -> (u64, u64) {
    let occurrences = program.occurrences();
    let block = occurrences[s].0 as usize;
    let node = &program.blocks[block].nodes[n];
    let h = crate::plan::worst_h(program, block);
    let encoder = crate::seg_encoder::encoder_binding_v1(program).ok();
    let node_bytes = crate::element::node_opening_bytes_v1(node_count);
    let shell = 5u64;
    let mut bytes = 64 + node_bytes;
    let operand = |shape: Vec<usize>, dtype: DType, committed: bool| {
        if committed {
            node_bytes + whole_tensor_bytes_v1(&shape, dtype).max(all_leaves_bytes(&shape, dtype)) + 1
        } else {
            8 + all_leaves_bytes(&shape, dtype)
        }
    };
    for r in &node.inputs {
        let t = crate::plan::ref_type(program, block, r);
        bytes += match r {
            Ref::Node(_) | Ref::CarryIn(_) | Ref::State(_) => operand(t.resolve(h), t.dtype, true),
            Ref::Param(j) => match encoder {
                Some(b) if *j == b.first_input => shell + crate::element::prompt_tile_bytes_v1(b.l as u64, 0),
                Some(b) if *j == b.first_input + 1 => shell,
                _ => operand(t.resolve(h), t.dtype, false),
            },
            Ref::Input(j) if *j == INPUT_TOKEN => shell + crate::element::prompt_tile_bytes_v1(PROMPT_TILE_IDS_V1 as u64, 0) + 64 * 20,
            _ => shell,
        };
    }
    if matches!(node.prim, Prim::HistAppend { .. }) {
        bytes += operand(node.out.resolve(h), node.out.dtype, true);
    }
    let layout = LayoutV3::of(&node.out.resolve(h));
    let first_shape = || crate::plan::ref_type(program, block, &node.inputs[0]).resolve(h);
    let flat = flat_gather
        && matches!(node.prim, Prim::Gather { axis: 0, batch_dims: 0 })
        && crate::element::flat_gather_v1(
            node,
            &node.inputs.iter().map(|r| crate::plan::ref_type(program, block, r).resolve(h)).collect::<Vec<_>>(),
            &node.out.resolve(h),
        );
    // Whole-value verification invokes the element evaluator once per output. A matrix product reads its full contraction
    // for EACH output; TopK sorts its reduction line for each selected index. Counting the input tensors once underprices both.
    let element_work = match node.prim {
        // Two direct authenticated reads and an index bounds check, without per-element
        // stride/coordinate construction. Wire authentication and both output hashes retain
        // their separate byte prices below. The unit is abstract bounded work, not CPU time.
        Prim::Gather { .. } if flat => 16,
        Prim::MatMul => first_shape().last().copied().unwrap_or(1) as u64 * 4,
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => first_shape()[axis as usize] as u64 * 2,
        Prim::TopK { axis, .. } => {
            let n = first_shape()[axis as usize] as u64;
            n.saturating_mul(crate::merkle::depth(n).saturating_add(2))
        }
        _ => node.inputs.len().max(1) as u64 * 16,
    };
    let output_hash_bytes = layout
        .len
        .saturating_mul(node.out.dtype.width() as u64 * 2)
        .saturating_add(layout.leaves(AXIS_ROW).saturating_add(layout.leaves(crate::merkle::AXIS_COL)).saturating_mul(256));
    let work = layout
        .len
        .saturating_mul(element_work)
        .saturating_mul(64)
        .saturating_add(bytes.saturating_mul(256))
        .saturating_add(output_hash_bytes.saturating_mul(64));
    (bytes, work)
}

/// **`[occurrence][node]`: the withheld values** — model-dependent (DA16b Level 2) and of a whole-value court that fits
/// [`SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1`].
pub fn seg_withheld_mask_v1(program: &TirProgramV1) -> Vec<Vec<bool>> {
    let masked = crate::scope::court_model_dependent_mask_v1(program, seg_model_params_v1(program));
    let node_count: u64 = masked.iter().map(|o| o.len() as u64).sum();
    masked
        .iter()
        .enumerate()
        .map(|(s, occ)| {
            occ.iter()
                .enumerate()
                .map(|(n, m)| {
                    if !m {
                        return false;
                    }
                    // Keep the established DA selection conservative and unchanged: lowering a
                    // court's execution price must not silently withhold additional values.
                    let (bytes, work) = whole_value_court_cost_in_v1(program, s, n, node_count, false);
                    bytes <= SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1 && work <= SEG_WHOLE_VALUE_COURT_MAX_WORK_V1
                })
                .collect()
        })
        .collect()
}

/// What DA16b's court scope makes of a K2-TIR-v4/v5 program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SegScopeReportV1 {
    /// Model-dependent values (Level 2) a position commits.
    pub masked: u64,
    /// Of those, withheld (option (c): never served; whole-value courts).
    pub withheld: u64,
    /// Of those, still served in the clear: they need option (a) (flat 64-element hiding tiles), which is not built.
    pub clear_masked: u64,
}

impl SegScopeReportV1 {
    /// Whether no model-dependent value is served in the clear. This report does not itself enforce reward eligibility.
    pub fn court_scope_complete(&self) -> bool {
        self.clear_masked == 0
    }
}

/// **The court-scope report of a program** (see [`SegScopeReportV1`]).
pub fn seg_scope_report_v1(program: &TirProgramV1) -> SegScopeReportV1 {
    let masked = crate::scope::court_model_dependent_mask_v1(program, seg_model_params_v1(program));
    let withheld = seg_withheld_mask_v1(program);
    let mut r = SegScopeReportV1::default();
    for (m, w) in masked.iter().flatten().zip(withheld.iter().flatten()) {
        r.masked += *m as u64;
        r.withheld += *w as u64;
        r.clear_masked += (*m && !*w) as u64;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN};
    use misaka_palw_tir::{Dim, TensorType};

    fn lookup_program(len: u32) -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(len, HISTORY_BOUND_V1_SMALL);
        let embedding = pb.param("embedding", DType::I16, &[len, 1], false);
        let table = pb.param("lookup", DType::I16, &[63], false);
        let indices = pb.param("indices", DType::I32, &[len], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embedding, Ref::Input(INPUT_TOKEN), 0, 0);
            let x = b.cast(x, DType::I32);
            b.finish(&[x])
        };
        let post = {
            let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
            let x = b.broadcast(Ref::CarryIn(0), &[Dim::Fixed(len)]);
            let x = b.add(x, indices, DType::I32);
            let x = b.clamp(x, 0, 62, DType::I32);
            let x = b.gather(table, x, 0, 0);
            let x = b.cast(x, DType::I32);
            b.commit(x);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        pb.finish(pre, vec![], post, logits)
    }

    #[test]
    fn flat_lookup_price_changes_only_element_work_and_keeps_conservative_disclosure() {
        let mut programs = vec![
            misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program,
            misaka_palw_tir_sketch::fixture::wide128_v1(7).program,
            misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, 4).program,
        ];
        // Both sides of the one-MiB disclosure bound; large declarations require no tensor allocation.
        programs.extend([64, 151_936, 190_000, 300_000, 20_000_000].map(lookup_program));
        let mut specialized = 0;
        for p in programs {
            let model = crate::scope::court_model_dependent_mask_v1(&p, seg_model_params_v1(&p));
            let mask = seg_withheld_mask_v1(&p);
            let count = model.iter().map(|o| o.len() as u64).sum();
            for (s, (b, _)) in p.occurrences().iter().enumerate() {
                for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
                    let old = whole_value_court_cost_in_v1(&p, s, n, count, false);
                    let new = whole_value_court_cost_v1(&p, s, n, count);
                    assert_eq!(old.0, new.0, "authenticated byte bound changed");
                    assert_eq!(
                        mask[s][n],
                        model[s][n] && old.0 <= SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1 && old.1 <= SEG_WHOLE_VALUE_COURT_MAX_WORK_V1
                    );
                    let h = crate::plan::worst_h(&p, *b as usize);
                    let inputs = node.inputs.iter().map(|r| crate::plan::ref_type(&p, *b as usize, r).resolve(h)).collect::<Vec<_>>();
                    let out = node.out.resolve(h);
                    if crate::element::flat_gather_v1(node, &inputs, &out) {
                        specialized += 1;
                        assert_eq!(
                            old.1 - new.1,
                            LayoutV3::of(&out).len * 16 * 64,
                            "wire authentication and hashes retain their entire price"
                        );
                    } else {
                        assert_eq!(old, new, "unrelated primitive price changed");
                    }
                }
            }
        }
        assert!(specialized >= 5);
    }
}
