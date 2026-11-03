//! Native numerical input geometry for Batch admission.
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::{
        MATERIAL_PIPELINED_MAX_BYTES, MaterialTargetV1, parameter_body_bound_v1, prompt_tile_bound,
    },
    palw_court_tir_requests_v1::TirSharedRequestSetV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        reserve_native_requests_v1,
    },
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1},
    palw_tir_court_v1::parameter_cover::RootInputBoundaryV1,
    palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicSourceV1, parameter_cover_v1::OriginalRootInputCoverV1},
};
use misaka_palw_tir::program::TirProgramV1;
use std::collections::{BTreeMap, BTreeSet};

type E = NativeDecodeErrorV1<String>;

/// Maximum byte cap per canonical range position across all numerical roots.
/// This covers one selected root's inputs, including the root leaf. It does
/// not price later turns, native verdicts, or simultaneous independent roots.
pub(crate) struct NumericalBatchEnvelopeV1 {
    pub scope: Hash64,
    maximums: Vec<u64>,
}

impl NumericalBatchEnvelopeV1 {
    pub(crate) fn maximums_v1(&self) -> &[u64] {
        &self.maximums
    }

    /// The same position boundaries apply to every selected root, even when
    /// individual payloads are smaller. A shorter root uses a prefix.
    pub(crate) fn demand_ranges_v1(&self) -> impl Iterator<Item = &[u64]> {
        partition_maximums_v1(&self.maximums)
    }
}

fn partition_maximums_v1(mut remaining: &[u64]) -> impl Iterator<Item = &[u64]> {
    use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1};
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let mut parts = 0;
        let count = remaining
            .iter()
            .take(MAX_BATCH_RANGES_V1)
            .take_while(|&&bytes| {
                parts += bytes.div_ceil(u64::from(MAX_BATCH_PART_BYTES_V1));
                parts <= MAX_BATCH_PARTS_V1 as u64
            })
            .count();
        // Every retained range passed the individual native codec bound.
        debug_assert!(count > 0);
        let (next, rest) = remaining.split_at(count);
        remaining = rest;
        Some(next)
    })
}

/// Use canonical target ordering for both the demand and its admission quote.
fn fold_maximums(maximums: &mut Vec<u64>, ranges: impl ExactSizeIterator<Item = u64>, cap: usize) -> Result<(), String> {
    if ranges.len() > cap {
        return Err("numerical Batch range cap exhausted".into());
    }
    maximums.resize(maximums.len().max(ranges.len()), 0);
    for (reserved, actual) in maximums.iter_mut().zip(ranges) {
        *reserved = (*reserved).max(actual);
    }
    Ok(())
}

pub(crate) fn numerical_batch_envelope_v1(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    cover: &OriginalRootInputCoverV1,
    program: &TirProgramV1,
    profile: Hash64,
    maximum_ranges: usize,
) -> Result<NativeRetainedV1<NumericalBatchEnvelopeV1>, E> {
    let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
    let parameters = cover.parameters_v1();
    let roots = cover.roots_v1();
    if parameters.source_id != set.source_id()
        || roots.is_empty()
        || roots.len() != parameters.roots.len()
        || roots.iter().zip(&parameters.roots).any(|(root, params)| root.leaf != params.leaf)
    {
        return Err(E::Decode("numerical Batch cover differs from the original".into()));
    }
    let maximum_ranges = maximum_ranges.min(
        roots
            .iter()
            .zip(&parameters.roots)
            .map(|(root, params)| root.inputs.len().saturating_add(params.parameters.len()).saturating_add(1))
            .max()
            .unwrap_or(0),
    );
    let mut retained = TirNativeRequestsV1::new();
    retained.vec_v1::<u64>(maximum_ranges as u64).map_err(E::Resource)?;
    retained.exact_v1::<NumericalBatchEnvelopeV1>(1).map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(retained.bytes_v1()).map_err(E::Resource)?;
    let mut maximums = Vec::with_capacity(maximum_ranges);
    let claim = frame.plan_v1().original_claim;
    let count = source.header_v1().job_context.declared_prefill_tokens;
    let prompt = frame.plan_v1().owner_work.rules.prompt_form;
    let full_ids = |source| {
        crate::palw_tir_public_owned_v1::request_body_bound_v1(source, PalwTirPublicRequestV1::Ids { offset: 0 })
            .map(|(bytes, _)| bytes)
    };
    let mut ids_bytes = None;

    for (root, params) in roots.iter().zip(&parameters.roots) {
        use RootInputBoundaryV1::{CommittedStepLeaf, GeneratedId, PromptPosition};
        let boundaries = root.inputs.len() as u64 + 1;
        let overflow = || E::Resource(NativeCoverageV1::overflow::<NumericalBatchEnvelopeV1>("Batch range enumeration"));
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(boundaries.checked_mul(btree_node_envelope_bytes_v1::<RootInputBoundaryV1, ()>() as u64).ok_or_else(overflow)?)
            .map_err(E::Resource)?;
        q.add_v1(
            (maximum_ranges as u64)
                .min(boundaries.saturating_add(params.parameters.len() as u64))
                .checked_mul(btree_node_envelope_bytes_v1::<Hash64, (MaterialTargetV1, u64)>() as u64)
                .ok_or_else(overflow)?,
        )
        .map_err(E::Resource)?;
        let _scratch = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut ranges = BTreeMap::new();
        let mut add = |target: MaterialTargetV1, bytes: u64| -> Result<(), String> {
            if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&bytes) {
                return Err("numerical Batch native range exceeds the codec capacity".into());
            }
            let id = target.id_v1(claim)?;
            if let Some(old) = ranges.get(&id) {
                return if *old == (target, bytes) { Ok(()) } else { Err("numerical Batch range identity collision".into()) };
            }
            if ranges.len() >= maximum_ranges {
                return Err("numerical Batch range cap exhausted".into());
            }
            ranges.insert(id, (target, bytes));
            Ok(())
        };
        let boundaries = root.inputs.iter().copied().chain([CommittedStepLeaf(root.leaf)]).collect::<BTreeSet<_>>();
        set.visit_batch_step_targets_v1(
            &boundaries,
            crate::palw_batch_court_v1::MAX_BATCH_PARTS_V1,
            maximum_ranges,
            |target, _, bound| add(target, bound.maximum_response_bytes),
        )
        .map_err(E::Decode)?;
        for &leaf in &params.parameters {
            let bytes = parameter_body_bound_v1(program, leaf).map_err(E::Decode)?;
            add(MaterialTargetV1::Parameter { leaf }, bytes).map_err(E::Decode)?;
        }
        for boundary in &boundaries {
            match *boundary {
                PromptPosition(position) => {
                    if position >= count {
                        return Err(E::Decode("numerical Batch prompt position outside original".into()));
                    }
                    let (target, bytes) = match prompt {
                        PalwPromptIdsFormV1::Flat => (MaterialTargetV1::PromptFlat, 4 + 4 * u64::from(count)),
                        PalwPromptIdsFormV1::MerkleV1 => {
                            let tile = position / PALW_PROMPT_IDS_TILE_LEN;
                            (MaterialTargetV1::PromptTile { tile }, prompt_tile_bound(count, tile).map_err(E::Decode)?)
                        }
                    };
                    add(target, bytes).map_err(E::Decode)?;
                }
                GeneratedId(index) => {
                    if index >= source.decode_count_v1() {
                        return Err(E::Decode("numerical Batch generated ID outside original".into()));
                    }
                    let bytes = match ids_bytes {
                        Some(bytes) => bytes,
                        None => {
                            let bytes = full_ids(source).map_err(E::Decode)?;
                            ids_bytes = Some(bytes);
                            bytes
                        }
                    };
                    add(MaterialTargetV1::FullIds, bytes).map_err(E::Decode)?;
                }
                _ => {}
            }
        }
        fold_maximums(&mut maximums, ranges.into_values().map(|(_, bytes)| bytes), maximum_ranges).map_err(E::Decode)?;
    }

    let scope = set.scope_id().map_err(E::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(NumericalBatchEnvelopeV1 { scope, maximums }, lease))
}

#[cfg(test)]
mod tests {
    use super::{fold_maximums, partition_maximums_v1};

    #[test]
    fn envelope_preserves_every_position_without_combining_root_material() {
        let mut maximums = Vec::new();
        fold_maximums(&mut maximums, [7, 11, 2].into_iter(), 4).unwrap();
        fold_maximums(&mut maximums, [13, 4].into_iter(), 4).unwrap();
        fold_maximums(&mut maximums, [1, 3, 7, 9].into_iter(), 4).unwrap();
        assert_eq!(maximums, [13, 11, 7, 9]);
        assert!(fold_maximums(&mut maximums, [1; 5].into_iter(), 4).is_err());
        assert_eq!(maximums, [13, 11, 7, 9]);
    }

    #[test]
    fn demand_partitions_fit_the_wire_caps_without_reordering_positions() {
        use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1};
        let cap = MAX_BATCH_PARTS_V1 as u64 * u64::from(MAX_BATCH_PART_BYTES_V1);
        let mut maximums = vec![1; MAX_BATCH_RANGES_V1 + 1];
        maximums.push(cap);
        maximums.push(1);
        let groups = partition_maximums_v1(&maximums).collect::<Vec<_>>();
        assert_eq!(groups.iter().map(|r| r.len()).collect::<Vec<_>>(), [MAX_BATCH_RANGES_V1, 1, 1, 1]);
        assert_eq!(groups.into_iter().flatten().copied().collect::<Vec<_>>(), maximums);
    }
}
