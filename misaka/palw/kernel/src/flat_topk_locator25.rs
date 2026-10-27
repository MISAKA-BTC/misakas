//! A selected TopK relation's bounded locator over original authenticated v5
//! roots and v25 value replies. Root/node coordinate discovery is independent
//! (SDK root locator); no producer witness/coordinate is accepted as discovery.
//! It selects one bad row, never scans all output rows. The row still costs K
//! selected replies and a precharged N-log-K independent source scan. This is
//! not a logarithmic-K theorem or a complete claim/Final/eligibility verifier.
use crate::{
    flat_court5::flat_profile_commitment_v5,
    flat_court25::FlatContextV25,
    flat_detect5::{FlatReadV5, read_node},
    flat_detect25::{FlatFindingV25, FlatPublicReadsV25, FlatValueReadV25, FlatValueV25, topk_relation_rows_v25},
    flat_topk7::{TopKScanBudgetV7, output_point},
    merkle5::{FlatDescentV5, FlatTreeV5},
};
use misaka_palw_tir::{DType, Prim, Tensor};

/// Worst successful callbacks AFTER independently finding the first divergent
/// node. Charges two nodes (output and possible source), one bad-leaf value,
/// K selected values and up to three filing-slot refetches. Small legacy outputs
/// need at most 63 scalar probes. Reply retries/missing responses spend the
/// caller's separate original paid attempt/read/wire/timeout quota.
pub fn topk_relation_callback_bound_v25(shape: &[usize], k: u32) -> Option<u64> {
    let len = shape.iter().try_fold(1u64, |a, n| a.checked_mul(*n as u64))?;
    if len == 0 || k == 0 {
        return None;
    }
    let (hashes, values) = if len >= 64 {
        let dims = shape.iter().map(|n| *n as u64).collect::<Vec<_>>();
        (crate::merkle5::flat_material_bounds_v5(DType::Idx, &dims)?.localization_replies, 1)
    } else {
        (0, len)
    };
    4u64.checked_add(hashes)?.checked_add(values)?.checked_add(k as u64)?.checked_add(3)
}
fn row_of(shape: &[usize], axis: usize, element: u64) -> Option<u64> {
    if axis >= shape.len() {
        return None;
    }
    let mut stride = shape.iter().try_fold(1u64, |a, n| a.checked_mul(*n as u64))?;
    if element >= stride {
        return None;
    }
    let mut row = 0u64;
    for (i, n) in shape.iter().enumerate() {
        stride /= *n as u64;
        let coord = element / stride % *n as u64;
        if i != axis {
            row = row.checked_mul(*n as u64)?.checked_add(coord)?;
        }
    }
    Some(row)
}
fn error(s: impl std::fmt::Debug) -> FlatFindingV25 {
    FlatFindingV25::Inconsistent(format!("{s:?}"))
}
/// The complete independently replayed expected output must bind the registered
/// original job/program/PC. Only the first divergent node may be submitted here;
/// earlier source correctness is established by that root discovery, not by a
/// trusted private prefix. Wrong typed headers require the caller's header court
/// before valid v7 selected-value proofs can be served; malformed values demand DA.
pub fn locate_selected_topk_v25(
    c: &FlatContextV25<'_>,
    public: &dyn FlatPublicReadsV25,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    scan: &mut TopKScanBudgetV7,
) -> FlatFindingV25 {
    let pr = match c.prepare() {
        Ok(p) => p,
        Err(e) => return error(e),
    };
    locate_selected_topk_prepared_v32(c, public, unit, expected, own, scan, &pr, false)
}
pub(crate) fn locate_selected_topk_prepared_v32(
    c: &FlatContextV25<'_>,
    public: &dyn FlatPublicReadsV25,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    scan: &mut TopKScanBudgetV7,
    pr: &crate::element::PreparedV1<'_>,
    original32: bool,
) -> FlatFindingV25 {
    if unit.leaf.is_some() || unit.position >= c.base.claim.positions || !pr.valid(unit.occurrence, unit.node) {
        return error("invalid selected TopK coordinate");
    }
    let Prim::TopK { axis, k } = pr.w.node(unit.occurrence, unit.node).prim else { return error("selected relation is not TopK") };
    if crate::flat_scope7::point_relation(&pr, unit.position, unit.occurrence, unit.node) != Ok(true) {
        return error("TopK point court not selected by original scope25");
    }
    let shape = pr.shapes(unit.position, unit.occurrence, unit.node).out;
    if expected.dtype != DType::Idx || expected.shape != shape {
        return error("independent TopK metadata differs");
    }
    let output = match read_node(&c.base, &pr, public, unit) {
        Ok(o) => o,
        Err(f) => return f.into(),
    };
    match flat_profile_commitment_v5(expected) {
        Ok(root) if root == output.commitment => return FlatFindingV25::Clean,
        Err(e) => return error(e),
        _ => {}
    }
    if let Err(f) = crate::flat_detect25::header(c, &c.scope.root(), public, unit, &output) {
        return f;
    }
    let mut mismatch = None;
    if expected.data.len() >= 64 {
        let tree = match FlatTreeV5::of(expected) {
            Ok(t) => t,
            Err(e) => return error(e),
        };
        let max = tree.layout().bounds_for::<5>(DType::Idx).unwrap().localization_replies;
        let mut next = 0;
        let mut selected = None;
        for _ in 0..max {
            let read = FlatReadV5 { leaf: Some(next), ..unit };
            let Some(hash) = public.hash(read).filter(|h| h.index == next) else { return FlatFindingV25::Demand(read) };
            match hash.descend(&output.commitment, &tree) {
                Ok(FlatDescentV5::Probe(i)) => next = i,
                Ok(FlatDescentV5::DivergentLeaf(_)) => {
                    selected = Some(next);
                    break;
                }
                _ => return FlatFindingV25::Demand(read),
            }
        }
        let Some(leaf) = selected else { return error("TopK leaf localization exceeded bound") };
        let (first, count) = tree.layout().span(leaf).unwrap();
        let read = FlatValueReadV25 { scope: c.scope.root(), node: unit, element: first };
        let Some(reply) = public
            .value(read)
            .and_then(|b| FlatValueV25::from_bytes(&b).ok())
            .filter(|r| r.scope == c.scope.root() && r.value.node.as_ref() == Some(&output))
        else {
            return FlatFindingV25::ValueDemand(read);
        };
        for e in first..first + count as u64 {
            let actual = match output_point(&reply.value, &shape, e) {
                Ok(v) => v,
                Err(_) => return FlatFindingV25::ValueDemand(read),
            };
            if actual != expected.data[e as usize] {
                mismatch = Some(e);
                break;
            }
        }
    } else {
        for e in 0..expected.data.len() as u64 {
            let read = FlatValueReadV25 { scope: c.scope.root(), node: unit, element: e };
            let Some(reply) = public
                .value(read)
                .and_then(|b| FlatValueV25::from_bytes(&b).ok())
                .filter(|r| r.scope == c.scope.root() && r.value.node.as_ref() == Some(&output))
            else {
                return FlatFindingV25::ValueDemand(read);
            };
            let actual = match output_point(&reply.value, &shape, e) {
                Ok(v) => v,
                Err(_) => return FlatFindingV25::ValueDemand(read),
            };
            if actual != expected.data[e as usize] {
                mismatch = Some(e);
                break;
            }
        }
    }
    let Some(row) = mismatch.and_then(|e| row_of(&shape, axis as usize, e)) else {
        return error("authenticated different root has no differing selected value");
    };
    let _ = k; // K remains charged in the independently supplied scan/read budget.
    if original32 {
        crate::flat_detect25::topk_relation_rows_checked_v32(
            c,
            &c.scope.root(),
            pr,
            public,
            unit,
            expected,
            own,
            scan,
            Some(row),
            Some(output),
            &|f| crate::flat_detect25::checked_prepared_v32(c, pr, f),
        )
    } else {
        topk_relation_rows_v25(c, &c.scope.root(), &pr, public, unit, expected, own, scan, Some(row), Some(output))
    }
}
