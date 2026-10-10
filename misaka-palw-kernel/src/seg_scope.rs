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
//! tiles, is its remedy and is not built. Such a class is **not reward-bearing** under RFC-0015 §1.1, by name
//! ([`seg_scope_report_v1`]), never silently.
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
    let occurrences = program.occurrences();
    let block = occurrences[s].0 as usize;
    let node = &program.blocks[block].nodes[n];
    let h = crate::plan::worst_h(program, block);
    let encoder = crate::seg_encoder::encoder_binding_v1(program).ok();
    let node_bytes = crate::element::node_opening_bytes_v1(node_count);
    let shell = 5u64;
    let mut bytes = 64 + node_bytes;
    let mut elements = 0u64;
    let mut operand = |shape: Vec<usize>, dtype: DType, committed: bool| {
        elements += LayoutV3::of(&shape).len;
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
    let out_len = LayoutV3::of(&node.out.resolve(h)).len;
    (bytes, out_len.saturating_mul(16).saturating_add(elements))
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
                .map(|(n, m)| *m && whole_value_court_cost_v1(program, s, n, node_count).0 <= SEG_WHOLE_VALUE_COURT_MAX_BYTES_V1)
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
    /// **Whether the class can be reward-bearing on DA16b's ground** (RFC-0015 §1.1): no model-dependent value is served in the clear.
    /// `false` is stated, never a silent shrink of the envelope: the class registers and is judged, it does not earn.
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
