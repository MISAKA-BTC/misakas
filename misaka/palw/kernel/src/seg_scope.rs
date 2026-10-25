//! **K2-TIR-v4/v5 under DA16b's court scope** (`crate::scope`; `docs/design/palw/k2-real-scale.md` §4.2) — every model-dependent value
//! (DA16b Level 2, which contains Level 1's model bytes) is owed only as commitment structure, never in the clear. A committed value has
//! one of three modes ([`SegValueModeV1`]), a pure function of the program, so the ledger's DA layout, the gate's price, the courts and
//! every verifier agree on it without any declaration:
//!
//! * **Clear** — model-independent: served in the clear, judged by the element court (v3 commitment).
//! * **Withheld** — option (c), the Lead's 2026-10-10 decision: model-dependent AND its whole-value court fits
//!   [`SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1`] / [`SEG_WHOLE_VALUE_COURT_MAX_WORK_V1`]. Served as its node opening alone
//!   (`ChunkBodyV1::Withheld`); the court (`SegFaultV1::WholeValue`) recomputes EVERY element from the filed operands and compares the
//!   v3 commitment.
//! * **Hiding** — option (a) (lane G14-S, 2026-10-11): model-dependent and NOT withheld (every 9B / BGE activation). Committed under
//!   flat 64-element hiding tiles ([`crate::hide`]), served as its leaf hashes (`ChunkBodyV1::Hidden`), judged by the hiding-leaf court
//!   (`SegFaultV1::HidingLeaf`): ONE tile recomputed from operands the filer supplies from its own re-execution, its hash compared with
//!   the producer's authenticated leaf hash. A hiding court whose price exceeds [`SEG_HIDING_COURT_MAX_BYTES_V1`] /
//!   [`SEG_HIDING_COURT_MAX_WORK_V1`] at the class's context is **unbounded**: the class's court scope is incomplete.
//!
//! [`seg_scope_report_v2`] is the GATE (not a report): a class whose `court_scope_complete()` is false takes no claim
//! (`KernelLedgerV1::commit_segmented_claim` refuses it by name, `COURT_SCOPE_INCOMPLETE`), so it is never reward-bearing.
//!
//! A verifier holds the model (ADR-0177 D7) and re-executes: its own value of a withheld or hiding node authenticates iff its commitment
//! is the producer's, and the first one whose commitment differs is the fault.

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
/// **The most a hiding-leaf court may file** (option (a)), in bytes: one filing in ONE kernel-route carrier (1,583,616 B) with the
/// segmented filing header (4,096 B) and the object's envelope (≤ 6,656 B) — a court past it would need chunked filing carriage
/// (tag 113, lane G14-B), which no hiding court relies on today. A hiding value whose court is larger is **unbounded**: its class's
/// court scope is incomplete (never reward-bearing).
pub const SEG_HIDING_COURT_MAX_BYTES_V1: u64 = 1_572_864;
/// The hiding-leaf court's work ceiling (the whole-value court's).
pub const SEG_HIDING_COURT_MAX_WORK_V1: u64 = SEG_WHOLE_VALUE_COURT_MAX_WORK_V1;

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

/// The bytes of EVERY hiding tile of a tensor, each at the hiding tree's full depth: what opening all of a hiding operand files.
pub fn all_hiding_tiles_bytes_v1(shape: &[usize], dtype: DType) -> u64 {
    let len = LayoutV3::of(shape).len;
    let tiles = crate::hide::hide_tiles_v1(len);
    4 + len * dtype.width() as u64 + tiles * (8 + 4 + 4 + 64 * crate::merkle::depth(tiles.max(1)))
}

/// **The price of a whole-value court of `(occurrence s, node n)`** (`(bytes, work)`): the filing's fixed fields and the output's node
/// opening (no leaf: its elements are recomputed), and per operand an upper bound of what a filer opens — a committed operand whole or
/// by every leaf (whichever is larger), a param by every leaf, a token or job ids by their tile. A filer opens only what the elements
/// read, so every filing is within it.
pub fn whole_value_court_cost_v1(program: &TirProgramV1, s: usize, n: usize, node_count: u64) -> (u64, u64) {
    whole_value_court_cost_in_v1(program, s, n, node_count, true)
}

fn whole_value_court_cost_in_v1(program: &TirProgramV1, s: usize, n: usize, node_count: u64, flat_gather: bool) -> (u64, u64) {
    whole_value_court_cost_in(program, s, n, node_count, flat_gather, false)
}

/// Candidate trace v2 only: a Gather of a registered param opens at most the
/// selected items' row leaves, rather than every item in that param. The other
/// operand, output hashes and general element evaluator keep their full price.
/// This does not change the v4/v5 disclosure mask, plan or reward contract.
pub fn model_vector_whole_court_cost_v2(program: &TirProgramV1, s: usize, n: usize, node_count: u64) -> (u64, u64) {
    whole_value_court_cost_in(program, s, n, node_count, true, true)
}

fn gather_param_leaves_bytes(shape: &[usize], dtype: DType, indices: u64) -> u64 {
    let l = LayoutV3::of(shape);
    let all = l.leaves(AXIS_ROW);
    let (leaves, elements) = if shape.len() == 1 {
        let leaves = all.min(indices);
        (leaves, l.len.min(leaves.saturating_mul(crate::merkle3::TILE_V3)))
    } else {
        let items = (shape[0] as u64).min(indices);
        let rows = l.rows / shape[0] as u64;
        (items.saturating_mul(rows).saturating_mul(l.row_tiles), items.saturating_mul(rows).saturating_mul(l.row_len))
    };
    elements
        .saturating_mul(dtype.width() as u64)
        .saturating_add(leaves.saturating_mul((crate::merkle::depth(all) + 1) * 64 + 8 * shape.len() as u64 + 32))
}

fn whole_value_court_cost_in(
    program: &TirProgramV1,
    s: usize,
    n: usize,
    node_count: u64,
    flat_gather: bool,
    partial_gather: bool,
) -> (u64, u64) {
    let occurrences = program.occurrences();
    let block = occurrences[s].0 as usize;
    let node = &program.blocks[block].nodes[n];
    let h = crate::plan::worst_h(program, block);
    let encoder = crate::seg_encoder::encoder_binding_v1(program).ok();
    let node_bytes = crate::element::node_opening_bytes_v1(node_count);
    // A `WholeOperandV1` shell: its node `Option` tag, the leaf and hidden vectors' lengths and its `whole` tag.
    let shell = 10u64;
    let mut bytes = 64 + node_bytes;
    // A committed operand's source may be clear (v3 leaves), withheld (whole) or hiding (hiding tiles): the largest of the three, so
    // the price never depends on which occurrence feeds the operand (and the modes are never priced in a circle). Plus the operand's shell.
    // The candidate trace v2 profile (`partial_gather`, RFC-0002) has no hiding values (`crate::element`'s candidate scope: withheld
    // or clear, every commitment v3), so a committed operand there is opened whole or by v3 leaves only; the hiding-tile term is the
    // frozen v4/v5 profile's (G14-S), where any operand may be hiding.
    let hiding_operands = !partial_gather;
    let operand = |shape: Vec<usize>, dtype: DType, committed: bool| {
        if committed {
            node_bytes
                + whole_tensor_bytes_v1(&shape, dtype).max(all_leaves_bytes(&shape, dtype)).max(if hiding_operands {
                    all_hiding_tiles_bytes_v1(&shape, dtype)
                } else {
                    0
                })
                + shell
        } else {
            shell + 2 + all_leaves_bytes(&shape, dtype)
        }
    };
    for (i, r) in node.inputs.iter().enumerate() {
        let t = crate::plan::ref_type(program, block, r);
        bytes += match r {
            Ref::Node(_) | Ref::CarryIn(_) | Ref::State(_) => operand(t.resolve(h), t.dtype, true),
            Ref::Param(j) => match encoder {
                Some(b) if *j == b.first_input => shell + crate::element::prompt_tile_bytes_v1(b.l as u64, 0),
                Some(b) if *j == b.first_input + 1 => shell,
                _ if partial_gather && i == 0 && matches!(node.prim, Prim::Gather { axis: 0, batch_dims: 0 }) => {
                    let indices = crate::plan::ref_type(program, block, &node.inputs[1]).resolve(h);
                    8 + gather_param_leaves_bytes(&t.resolve(h), t.dtype, LayoutV3::of(&indices).len)
                }
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

/// Commitment-only values for non-reward candidate traces. In addition to the
/// frozen mask, this admits the bounded Gather dependency court above. Values
/// outside these bounds are still a scope gap, never an eligibility certificate.
pub fn model_vector_withheld_mask_v2(program: &TirProgramV1) -> Vec<Vec<bool>> {
    let masked = crate::scope::court_model_dependent_mask_v1(program, seg_model_params_v1(program));
    let node_count = masked.iter().map(|o| o.len() as u64).sum();
    let old = seg_withheld_mask_v1(program);
    masked
        .iter()
        .enumerate()
        .map(|(s, occ)| {
            occ.iter()
                .enumerate()
                .map(|(n, m)| {
                    let (bytes, work) = model_vector_whole_court_cost_v2(program, s, n, node_count);
                    old[s][n] || (*m && bytes <= SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1 && work <= SEG_WHOLE_VALUE_COURT_MAX_WORK_V1)
                })
                .collect()
        })
        .collect()
}

/// Additional opening/work envelope which the candidate admission must reserve.
pub fn model_vector_whole_court_bounds_v2(program: &TirProgramV1) -> (u64, u64) {
    let mask = model_vector_withheld_mask_v2(program);
    let count = mask.iter().map(|o| o.len() as u64).sum();
    let mut bounds = (0u64, 0u64);
    for (s, occ) in mask.iter().enumerate() {
        for (n, withheld) in occ.iter().enumerate() {
            if *withheld {
                let (bytes, work) = model_vector_whole_court_cost_v2(program, s, n, count);
                bounds = (bounds.0.max(bytes), bounds.1.max(work));
            }
        }
    }
    bounds
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

/// **How a committed value is owed** (see the module note): a pure function of the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SegValueModeV1 {
    /// Model-independent: served in the clear, v3 commitment.
    Clear,
    /// Option (c): model-dependent with a bounded whole-value court; never served, v3 commitment.
    Withheld,
    /// Option (a): model-dependent, not withheld; flat 64-element hiding tiles, owed as leaf hashes.
    Hiding,
}

/// **`[occurrence][node]`: every committed value's mode.**
pub fn seg_value_modes_v1(program: &TirProgramV1) -> Vec<Vec<SegValueModeV1>> {
    let masked = crate::scope::court_model_dependent_mask_v1(program, seg_model_params_v1(program));
    let withheld = seg_withheld_mask_v1(program);
    masked
        .iter()
        .zip(&withheld)
        .map(|(m, w)| {
            m.iter()
                .zip(w)
                .map(|(m, w)| match (m, w) {
                    (false, _) => SegValueModeV1::Clear,
                    (true, true) => SegValueModeV1::Withheld,
                    (true, false) => SegValueModeV1::Hiding,
                })
                .collect()
        })
        .collect()
}

/// **`[occurrence][node]`: the hiding values** (option (a)).
pub fn seg_hiding_mask_v1(program: &TirProgramV1) -> Vec<Vec<bool>> {
    seg_value_modes_v1(program).into_iter().map(|o| o.into_iter().map(|m| m == SegValueModeV1::Hiding).collect()).collect()
}

/// **The commitment of one committed value under its mode**: a hiding value's is `C_hide` ([`crate::hide`]), every other one's v3.
pub fn seg_value_commitment_v1(mode: SegValueModeV1, t: &misaka_palw_tir::Tensor) -> crate::hash::Digest {
    match mode {
        SegValueModeV1::Hiding => crate::hide::tensor_commitment_hide_v1(t),
        SegValueModeV1::Clear | SegValueModeV1::Withheld => crate::merkle3::tensor_commitment_v3(t),
    }
}

/// **Every node commitment of one position's values** under `modes` (`[occurrence][node]`; a value past `modes` is committed v3).
pub fn seg_position_commitments_v1(
    modes: &[Vec<SegValueModeV1>],
    values: &[Vec<misaka_palw_tir::Tensor>],
) -> Vec<Vec<crate::hash::Digest>> {
    values
        .iter()
        .enumerate()
        .map(|(s, occ)| {
            occ.iter()
                .enumerate()
                .map(|(n, t)| {
                    let mode = modes.get(s).and_then(|o| o.get(n)).copied().unwrap_or(SegValueModeV1::Clear);
                    seg_value_commitment_v1(mode, t)
                })
                .collect()
        })
        .collect()
}

/// What DA16b's court scope makes of a K2-TIR-v4/v5 class — **the gate**: [`Self::court_scope_complete`] decides whether the class may
/// take (reward-bearing) claims.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SegScopeReportV1 {
    /// Model-dependent values (Level 2) a position commits, counted per `(occurrence, node)`.
    pub masked: u64,
    /// Of those, withheld (option (c): never served; whole-value courts).
    pub withheld: u64,
    /// Of those, hiding (option (a): leaf hashes served; hiding-leaf courts).
    pub hiding: u64,
    /// Of those, served in the clear. Always 0 since option (a): kept so a report can never claim completeness while one is.
    pub clear_masked: u64,
    /// Hiding values whose hiding-leaf court exceeds [`SEG_HIDING_COURT_MAX_BYTES_V1`] or [`SEG_HIDING_COURT_MAX_WORK_V1`] at the
    /// class's context: a lie in one cannot be convicted inside one carrier.
    pub unbounded: u64,
    /// The first unbounded value `(occurrence, node, bytes, work)`, for the refusal's name.
    pub first_unbounded: Option<(u16, u16, u64, u64)>,
    /// Masked values shorter than one full hiding tile at `H = 1`: their commitment (one leaf) hashes fewer than 64 elements. A
    /// residual, not a gap: the commitment is published whatever the court is.
    pub short: u64,
}

impl SegScopeReportV1 {
    /// **The gate**: no model-dependent value is ever served in the clear, and every one has a bounded court (whole-value or
    /// hiding-leaf). A class for which this is false takes no claim (`COURT_SCOPE_INCOMPLETE`).
    pub fn court_scope_complete(&self) -> bool {
        self.clear_masked == 0 && self.unbounded == 0
    }

    /// The refusal's text when incomplete.
    pub fn refusal(&self) -> Option<String> {
        if self.court_scope_complete() {
            return None;
        }
        Some(match self.first_unbounded {
            Some((s, n, b, w)) => format!(
                "COURT_SCOPE_INCOMPLETE: {} of {} model-dependent values have no bounded court (first: occurrence {s} node {n}, a \
                 hiding-leaf court of {b} B / work {w} past {SEG_HIDING_COURT_MAX_BYTES_V1} B / {SEG_HIDING_COURT_MAX_WORK_V1}); the \
                 class is not reward-bearing",
                self.unbounded, self.masked
            ),
            None => format!("COURT_SCOPE_INCOMPLETE: {} model-dependent values are served in the clear", self.clear_masked),
        })
    }
}

/// **The court-scope report of a program at its own worst history** (see [`seg_scope_report_v2`]).
pub fn seg_scope_report_v1(program: &TirProgramV1) -> SegScopeReportV1 {
    seg_scope_report_v2(program, u32::MAX)
}

/// **The court-scope gate of a class**: its program's value modes, and every hiding value's court priced at the class's context
/// (`min(window, max_positions)`, the history a claim of the class can reach), once per `(block, node)`.
pub fn seg_scope_report_v2(program: &TirProgramV1, max_positions: u32) -> SegScopeReportV1 {
    let modes = seg_value_modes_v1(program);
    let encoder = crate::seg_encoder::encoder_binding_v1(program).ok();
    let node_count: u64 = modes.iter().map(|o| o.len() as u64).sum();
    let occurrences = program.occurrences();
    type PriceKey = (u8, u16, Vec<Option<SegValueModeV1>>);
    let mut priced: std::collections::BTreeMap<PriceKey, (u64, u64)> = Default::default();
    let mut r = SegScopeReportV1::default();
    for (s, occ) in modes.iter().enumerate() {
        let block = occurrences[s].0;
        for (n, mode) in occ.iter().enumerate() {
            if *mode == SegValueModeV1::Clear {
                continue;
            }
            r.masked += 1;
            let node = &program.blocks[block as usize].nodes[n];
            if LayoutV3::of(&node.out.resolve(1)).len < crate::hide::HIDE_TILE_V1 {
                r.short += 1;
            }
            match mode {
                SegValueModeV1::Withheld => r.withheld += 1,
                SegValueModeV1::Hiding => {
                    r.hiding += 1;
                    let key = (block, n as u16, crate::element::operand_source_modes_v1(program, &modes, s, n));
                    let (bytes, work) = *priced.entry(key).or_insert_with(|| {
                        crate::element::hiding_leaf_court_cost_v1(program, s, n, node_count, max_positions, encoder.as_ref(), &modes)
                    });
                    if bytes > SEG_HIDING_COURT_MAX_BYTES_V1 || work > SEG_HIDING_COURT_MAX_WORK_V1 {
                        r.unbounded += 1;
                        r.first_unbounded.get_or_insert((s as u16, n as u16, bytes, work));
                    }
                }
                SegValueModeV1::Clear => {}
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN};
    use misaka_palw_tir::{Dim, TensorType};

    #[test]
    fn candidate_gather_price_bounds_actual_dependency_leaves_at_all_ranks() {
        use crate::element::leaves_covering_v1;
        use crate::merkle3::TreesV3;
        use misaka_palw_tir::Tensor;
        use std::collections::BTreeSet;
        // Scattered scalar lookups, duplicate indices, full coverage, several
        // rows per selected rank-3/4 item and a partial final tile.
        for shape in [vec![9001], vec![7, 5001], vec![7, 2, 19], vec![7, 2, 3, 19]] {
            let t = Tensor::zeros(DType::I8, &shape);
            let trees = TreesV3::of(&t);
            let stride = t.data.len() / shape[0];
            for ids in [vec![0], vec![0, 0, 0], vec![0, shape[0] - 1], (0..shape[0]).collect()] {
                let reads: BTreeSet<_> = ids.iter().flat_map(|j| (*j * stride..(*j + 1) * stride).map(|e| e as u64)).collect();
                let leaves = leaves_covering_v1(&t, &trees, &reads);
                let wire = leaves.iter().map(|l| borsh::to_vec(l).unwrap().len() as u64).sum::<u64>();
                let bound = gather_param_leaves_bytes(&shape, DType::I8, ids.len() as u64);
                assert!(wire <= bound, "{shape:?} {ids:?}: {wire} > {bound}");
            }
        }
        // A large declaration changes path depth, not a full weight allocation
        // or a linear-size court. The one selected row remains bounded.
        let shape = [1usize << 28, 2048];
        let sparse = gather_param_leaves_bytes(&shape, DType::I8, 1);
        assert!(sparse < 8192);
        assert!(all_leaves_bytes(&shape, DType::I8) > 1 << 38);
    }

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
