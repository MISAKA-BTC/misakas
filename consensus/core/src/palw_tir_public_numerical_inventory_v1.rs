//! Original-wide numerical topology, not a complete service/funding profile.
//! Reconstructs authenticated native geometry; no retained root or owner input.
use super::PalwTirPublicSourceV1;
use crate::{Hash64, palw_tir_dissect_v1 as d};
use d::tree_inventory_v1::{NumericalTreeInventoryV1, numerical_tree_inventory_v1};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirPublicNumericalInventoryV1 {
    pub source_id: Hash64,
    pub original_leaves: u64,
    pub eligible_roots: u64,
    pub arity: u8,
    pub demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    pub trees: NumericalTreeInventoryV1,
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "original numerical inventory overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "original numerical inventory overflow".into())
}
fn accumulate(total: &mut NumericalTreeInventoryV1, one: NumericalTreeInventoryV1, count: u64) -> Result<(), String> {
    if count == 0 { return Ok(()); }
    total.disclosures = add(total.disclosures, mul(one.disclosures, count)?)?;
    total.terminals = add(total.terminals, mul(one.terminals, count)?)?;
    total.child_claims = add(total.child_claims, mul(one.child_claims, count)?)?;
    total.disclosure_path_bytes = add(total.disclosure_path_bytes, mul(one.disclosure_path_bytes, count)?)?;
    total.round_body_bytes_upper = add(total.round_body_bytes_upper, mul(one.round_body_bytes_upper, count)?)?;
    total.max_depth = total.max_depth.max(one.max_depth);
    total.max_round_body_bytes_upper = total.max_round_body_bytes_upper.max(one.max_round_body_bytes_upper);
    Ok(())
}

/// Histories over the half-open original position range. Window saturation and
/// H-free blocks collapse to one group; unsaturated histories remain distinct.
/// No group is a selected path or an assumed participant response.
fn histories(first: u32, end: u32, window: Option<u32>, mut visit: impl FnMut(u32, u64) -> Result<(), String>) -> Result<(), String> {
    if first >= end { return Ok(()); }
    match window {
        None => visit(1, u64::from(end - first)),
        Some(0) => Err("original numerical history window is zero".into()),
        Some(w) => {
            for h in (first + 1).min(w)..=end.min(w) {
                let count = if h == w { u64::from(end - first.max(w - 1)) } else { 1 };
                visit(h, count)?;
            }
            Ok(())
        }
    }
}

impl PalwTirPublicSourceV1 {
    /// Covers every dissected commit tile of every executed occurrence and
    /// position, including ragged output tiles and post's prefill boundary.
    /// Registry/layout and literal source header are revalidated. Caller must
    /// resolve the native box-demand rules, as existing class admission does;
    /// the returned inventory records that choice, never authenticates it.
    ///
    /// Caller supplies the actual native court parameters. This analytical
    /// function does not admit its own reconstruction/iteration work: a funded
    /// ingress must bound that work too. No root/finalize/terminal-witness body,
    /// signature/envelope, semantic work, IO, retention or fee is priced here.
    pub fn numerical_tree_upper_v1(
        &self,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        court: &crate::palw_mode_v2::PalwCourtParamsV2,
        demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
    ) -> Result<TirPublicNumericalInventoryV1, String> {
        let arity = court.dissection_arity();
        if !crate::palw_attn_dissect::palw_attn_arity_is_legal_v1(arity) {
            return Err("original numerical inventory arity".into());
        }
        let binding = self.header.resolve_registered_layout_v1(registered, layout,
            self.header.committed_execution_root, self.maximum_leaves)?;
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, self.maximum_leaves)
            .map_err(|e| e.to_string())?;
        if verified.class_id != self.class_id || verified.context_hash != self.context_hash {
            return Err("original numerical inventory substitutes source geometry".into());
        }
        let space = &verified.space;
        let job = space.job_shape(&self.header.job_context).map_err(|e| e.to_string())?;
        let occurrences = space.occurrences();
        let last = occurrences.len().checked_sub(1).ok_or("original has no occurrences")?;
        let mut out = TirPublicNumericalInventoryV1 { source_id: self.source_id,
            original_leaves: self.header.step_leaf_count, eligible_roots: 0, arity, demand,
            trees: NumericalTreeInventoryV1::default() };
        for (bi, block) in space.program.blocks.iter().enumerate() {
            let block_id = u8::try_from(bi).map_err(|_| "original block index overflow")?;
            let non_post = occurrences.iter().take(last).filter(|(b, _)| *b == block_id).count() as u64;
            let post = u64::from(occurrences[last].0 == block_id);
            if non_post == 0 && post == 0 { continue; }
            for (ni, node) in block.nodes.iter().enumerate() {
                if !node.commit { continue; }
                let node_id = u16::try_from(ni).map_err(|_| "original node index overflow")?;
                let reductions = d::palw_tir_cone_reductions_v1(block, node_id).len();
                if reductions == 0 { continue; }
                d::palw_tir_dissect_obligations_v1(block, node_id).map_err(|e| e.to_string())?;
                let tile = space.commit_tile_len(block_id, node_id).ok_or("original commit tile missing")?;
                let values = d::palw_tir_dissect_value_bound_v2(&space.program, block, node_id, tile, demand);
                let mut charge = |h: u32, positions: u64, copies: u64| -> Result<(), String> {
                    let output_tiles = node.out.elements_at(u64::from(h)).div_ceil(u64::from(tile));
                    let roots = mul(output_tiles, mul(positions, copies)?)?;
                    if roots == 0 { return Ok(()); }
                    let tree = numerical_tree_inventory_v1(h.div_ceil(layout.h_tile), arity, reductions, values)
                        .ok_or("original numerical tree exceeds existing native bounds")?;
                    out.eligible_roots = add(out.eligible_roots, roots)?;
                    accumulate(&mut out.trees, tree, roots)
                };
                let window = space.info.blocks.get(bi).ok_or("original block info missing")?.window;
                if non_post != 0 {
                    histories(0, job.positions, window, |h, n| charge(h, n, non_post))?;
                }
                if post != 0 {
                    histories(job.prefill - 1, job.positions, window, |h, n| charge(h, n, post))?;
                }
            }
        }
        if out.eligible_roots > out.original_leaves {
            return Err("original numerical inventory exceeds original leaf universe".into());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_groups_match_each_original_position_including_saturation() {
        for end in 1..=31 {
            for first in 0..=end {
                for window in [None, Some(1), Some(2), Some(7), Some(32)] {
                    let mut actual = std::collections::BTreeMap::new();
                    histories(first, end, window, |h, n| { *actual.entry(h).or_insert(0) += n; Ok(()) }).unwrap();
                    let mut expected = std::collections::BTreeMap::new();
                    for p in first..end { *expected.entry(window.map(|w| (p + 1).min(w)).unwrap_or(1)).or_insert(0) += 1; }
                    assert_eq!(actual, expected);
                }
            }
        }
        assert!(histories(0, 1, Some(0), |_, _| Ok(())).is_err());
    }
}
