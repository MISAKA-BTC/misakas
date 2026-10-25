//! Admitted copies derived from already authenticated original material.
use super::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicAuthenticatedUnitV1, PalwTirPublicResponseV1, SharedMaterialV2};
use kaspa_consensus_core::{
    Hash64,
    palw_backend::PalwTirPublicConeFaultV2 as Fault,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, reserve_native_requests_v1},
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_step_leg::{PalwStepRangeOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::PalwTiledDecodeTokensV1,
    palw_tir_court_v1::PalwTirStepRunDisclosureV1,
    palw_tir_step_v1::PalwTirStepBindingV1,
};
use std::sync::Arc;
fn local(why: NativeCoverageV1) -> Fault {
    Fault::LocalResource(why.to_string())
}
fn alloc<T>(role: &str) -> Fault {
    local(NativeCoverageV1::unsupported::<T>(role, "allocator refused admitted Cone copy"))
}
pub(super) struct OwnedConeDerivedV2 {
    pub(super) runs: Vec<PalwTirStepRunDisclosureV1>,
    pub(super) intervals: Vec<(u64, u64)>,
    pub(super) trace: Option<PalwTiledDecodeTokensV1>,
    _lease: Option<NativeDemandLeaseV1>,
}
#[cfg(test)]
thread_local! {static STAGES:std::cell::Cell<(u64,u64)>=const{std::cell::Cell::new((0,0))};}
impl OwnedConeDerivedV2 {
    pub(super) fn requests_v2(
        compact: &PalwTirStepBindingV1,
        units: &[Arc<SharedMaterialV2<PalwTirPublicAuthenticatedUnitV1>>],
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) -> Result<(usize, u64), NativeCoverageV1> {
        let mut q = PalwMaterialConstructionRequestsV2::new();
        let binding = compact.native_heap_v1("Cone compact binding clone");
        let binding = binding.bytes().ok_or(binding)?;
        let (mut count, mut widest) = (0usize, 0usize);
        for unit in units {
            match unit.response_v1() {
                PalwTirPublicResponseV1::StepLeaf { preimage, opening } => {
                    q.vec_v1::<PalwStepTileLeafV1>(1)?;
                    let b = preimage.native_heap_v1("Cone singleton preimage clone");
                    q.add_v1(b.bytes().ok_or(b)?)?;
                    q.vec_v1::<Hash64>(1)?;
                    let b = opening.siblings.native_heap_v1("Cone singleton siblings clone");
                    q.add_v1(b.bytes().ok_or(b)?)?;
                    count += 1;
                    widest = widest.max(1);
                }
                PalwTirPublicResponseV1::StepRun { preimages, range } => {
                    let b = preimages.native_heap_v1("Cone run preimages clone");
                    q.add_v1(b.bytes().ok_or(b)?)?;
                    let b = range.native_heap_v1("Cone run opening clone");
                    q.add_v1(b.bytes().ok_or(b)?)?;
                    count += 1;
                    widest = widest.max(range.leaf_hashes.len());
                }
                _ => continue,
            }
            q.add_v1(binding)?;
        }
        q.vec_v1::<PalwTirStepRunDisclosureV1>(count as u64)?;
        q.vec_v1::<(u64, u64)>(count as u64)?;
        // Range verification keeps its initial mapped hashes and next-level
        // capacity n/2+2 simultaneously. 2*max(n,4) bounds that actual pair;
        // checks are sequential, so retain one widest workspace, not sum(n).
        q.vec_v1::<Hash64>(widest as u64)?;
        if let Some(ids) = ids {
            q.vec_v1::<u32>(ids.ids_v1().len() as u64)?;
        }
        Ok((count, q.bytes_v1()))
    }
    pub(super) fn prepare_v2(
        source_id: Hash64,
        compact: &PalwTirStepBindingV1,
        units: &[Arc<SharedMaterialV2<PalwTirPublicAuthenticatedUnitV1>>],
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
        maximum: u64,
    ) -> Result<Self, Fault> {
        if units.iter().any(|unit| unit.source_id_v1() != source_id) || ids.is_some_and(|ids| ids.source_id_v1() != source_id) {
            return Err(Fault::Unavailable("TIR Cone material belongs to another original source".into()));
        }
        let (count, bytes) = Self::requests_v2(compact, units, ids).map_err(local)?;
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b) = s.get();
            s.set((a + 1, b));
        });
        let lease = reserve_native_requests_v1(bytes).map_err(local)?;
        let mut runs = Vec::new();
        let mut intervals = Vec::new();
        runs.try_reserve_exact(count).map_err(|_| alloc::<Vec<PalwTirStepRunDisclosureV1>>("Cone runs"))?;
        intervals.try_reserve_exact(count).map_err(|_| alloc::<Vec<(u64, u64)>>("Cone intervals"))?;
        if runs.capacity() != count || intervals.capacity() != count {
            return Err(alloc::<Self>("unexpected exact Cone vector capacity"));
        }
        for unit in units {
            let (preimages, range) = match unit.response_v1() {
                PalwTirPublicResponseV1::StepLeaf { preimage, opening } => (
                    vec![preimage.clone()],
                    PalwStepRangeOpeningV1 {
                        first_leaf_index: opening.leaf_index,
                        leaf_hashes: vec![opening.leaf_hash],
                        siblings: opening.siblings.clone(),
                    },
                ),
                PalwTirPublicResponseV1::StepRun { preimages, range } => (preimages.clone(), range.clone()),
                _ => continue,
            };
            if kaspa_consensus_core::palw_step_leg::step_range_opening_root_capped_v1(compact.step_leaf_count, &range, maximum)
                .map_err(|why| Fault::Unavailable(why.to_string()))?
                != compact.step_merkle_root
            {
                return Err(Fault::Unavailable("TIR public range conversion changes the original step root".into()));
            }
            let first = range.first_leaf_index;
            intervals.push((first, first.saturating_add(preimages.len() as u64)));
            runs.push(PalwTirStepRunDisclosureV1 { binding: compact.clone(), preimages, range });
        }
        intervals.sort_unstable();
        let trace =
            ids.map(|ids| PalwTiledDecodeTokensV1 { rows_root: ids.rows_root_v1(), generated_token_ids: ids.ids_v1().to_vec() });
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b) = s.get();
            s.set((a, b + 1));
        });
        Ok(Self { runs, intervals, trace, _lease: lease })
    }
}
#[cfg(test)]
#[path = "palw_tir_cone_derived_owned_tests_v2.rs"]
mod tests;
