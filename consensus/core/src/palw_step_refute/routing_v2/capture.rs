//! Actual retained capture -> exact reader inputs -> same arithmetic operand
//! queries -> authenticated artifact openings. No independent parameter list.
use super::*;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1};
use crate::palw_native_resource_v1::{NativeDemandLeaseV1, native_value_requests_v1};
use crate::palw_step_leg as leg;
use std::cell::RefCell;

fn extend(lease: &mut Option<NativeDemandLeaseV1>, bytes: u64) -> Result<(), PalwRoutingCourtErrorV2> {
    if let Some(lease) = lease {
        lease.grow_memory_v1(bytes)?;
    } else {
        *lease = reserve_native_requests_v1(bytes)?;
    }
    Ok(())
}
/// Assemble only the selected routed reader's canonical output/input openings.
/// The caller owns the real input capture; this owner covers the independent
/// evidence copy and its membership workspace before construction.
pub fn routed_refutation_from_capture_v2(
    binding: &leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    declaration: &PalwReaderRouterDeclarationV2,
    hashes: &[Hash64],
    tiles: &[(u64, leg::PalwStepTileLeafV1)],
    max_leaves: u64,
) -> Result<NativeRetainedV1<PalwRoutedStepRefutationV2>, PalwRoutingCourtErrorV2> {
    if binding.step_leaf_count == 0 || binding.step_leaf_count > max_leaves || hashes.len() as u64 != binding.step_leaf_count {
        return Err(evidence("routed capture does not contain the actual bounded step leaf list"));
    }
    let required = canonical_reader_inputs_v2(&binding.shape_profile, &binding.job_context, &target, declaration)?;
    let index = crate::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &target)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let leaf = |wanted| {
        let mut found = tiles.iter().filter(|(at, _)| *at == wanted);
        let (_, row) = found.next().ok_or_else(|| evidence("routed capture is missing a canonical tile"))?;
        if found.next().is_some() {
            return Err(evidence("routed capture repeats a canonical tile"));
        }
        Ok(row)
    };
    let output = leaf(index)?;
    let depth = u64::from(64 - binding.step_leaf_count.leading_zeros());
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(native_value_requests_v1(binding)?)?;
    q.add_v1(native_value_requests_v1(output)?)?;
    q.vec_v1::<u8>(
        (borsh::object_length(&binding.shape_profile).map_err(|_| evidence("cannot measure routed profile"))? as u64).max(1024),
    )?;
    q.vec_v1::<u8>((754 + binding.job_context.network_id.len() as u64).max(1024))?;
    q.vec_v1::<u8>(512)?;
    q.vec_v1::<Hash64>(depth)?;
    q.vec_v1::<Hash64>(binding.step_leaf_count)?;
    q.vec_v1::<Hash64>(binding.step_leaf_count.div_ceil(2))?;
    q.vec_v1::<PalwStepInputRowV1>(required.len() as u64)?;
    for row in required.iter() {
        q.vec_v1::<leg::PalwStepTileLeafV1>(row.len() as u64)?;
        q.vec_v1::<Vec<Hash64>>(row.len() as u64)?;
        q.vec_v1::<(usize, usize)>(row.len() as u64)?;
        for (at, _) in row {
            let value = leaf(*at)?;
            q.add_v1(native_value_requests_v1(value)?)?;
            q.vec_v1::<u8>((150 + value.values_le.len() as u64).max(256))?;
            q.vec_v1::<Hash64>(2 * depth)?;
        }
    }
    q.vec_v1::<u8>((150 + output.values_le.len() as u64).max(256))?;
    let lease = reserve_native_requests_v1(q.bytes_v1())?;
    leg::verify_binding_v1(binding)?;
    let check = |value: &leg::PalwStepTileLeafV1, wanted: u64, coord: &PalwStepCoordinateV1| {
        if value.coord != *coord
            || value.version != leg::PALW_STEP_LEG_OBJECT_VERSION_V1
            || value.values_le.len() as u64 != u64::from(value.value_count) * 4
            || hashes.get(wanted as usize)
                != Some(&leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &declaration.class_id, value))
        {
            return Err(evidence("routed capture tile differs from its committed coordinate/hash"));
        }
        Ok(())
    };
    check(output, index, &target)?;
    let output_opening = leg::step_opening_capped_v1(hashes, index, max_leaves)?;
    if leg::step_opening_root_capped_v1(binding.step_leaf_count, &output_opening, max_leaves)? != binding.step_merkle_root {
        return Err(evidence("routed capture belongs to another step root"));
    }
    let mut inputs = Vec::with_capacity(required.len());
    for row in required.iter() {
        let mut preimages = Vec::with_capacity(row.len());
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, (wanted, coord)) in row.iter().enumerate() {
            let value = leaf(*wanted)?;
            check(value, *wanted, coord)?;
            preimages.push(value.clone());
            match runs.last_mut() {
                Some((from, len)) if row[*from].0 + *len as u64 == *wanted => *len += 1,
                _ => runs.push((i, 1)),
            }
        }
        let mut run_siblings = Vec::with_capacity(runs.len());
        for (from, len) in runs {
            run_siblings.push(leg::step_merkle_range_siblings_capped_v1(hashes, row[from].0 as usize, len, max_leaves)?);
        }
        inputs.push(PalwStepInputRowV1 { preimages, run_siblings });
    }
    let value = PalwRoutedStepRefutationV2 {
        version: 2,
        step: PalwExecutionStepRefutationV1 {
            binding: binding.clone(),
            output_opening,
            output_preimage: output.clone(),
            inputs,
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
            kv_checkpoint: None,
        },
    };
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRoutedLeafEvidenceV2 {
    pub refutation: PalwRoutedStepRefutationV2,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
}

struct Recorder<'a> {
    inventory: &'a [PalwArtifactOperandV1],
    used: RefCell<Vec<bool>>,
}
impl PalwWeightOracleV1 for Recorder<'_> {
    fn operand_bytes(&self, name: &str, layer: Option<u16>, offset: u32, len: u32) -> Option<Vec<u8>> {
        // No BTree membership allocation occurs during a request: the complete
        // finite recording bitmap was reserved before arithmetic began.
        let (index, value) =
            self.inventory.iter().enumerate().find(|(_, v)| v.tensor_name == name && v.layer == layer && v.row_start == offset)?;
        if value.bytes.len() != len as usize {
            return None;
        }
        self.used.borrow_mut()[index] = true;
        Some(value.bytes.clone()) // admitted first by the terminal's oracle
    }
}
/// Record the parameters the actual selected-router arithmetic requests and
/// open precisely those rows against the native class's registered artifact.
/// Input capture and inventory lifetimes remain the caller's ownership duty.
/// Provider transport/publication must retain the returned construction owner.
pub fn routed_leaf_evidence_from_capture_v2(
    binding: &leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    declaration: &PalwReaderRouterDeclarationV2,
    hashes: &[Hash64],
    tiles: &[(u64, leg::PalwStepTileLeafV1)],
    inventory: &[PalwArtifactOperandV1],
    max_leaves: u64,
) -> Result<NativeRetainedV1<PalwRoutedLeafEvidenceV2>, PalwRoutingCourtErrorV2> {
    let built = routed_refutation_from_capture_v2(binding, target, declaration, hashes, tiles, max_leaves)?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<bool>(inventory.len() as u64)?;
    let _recording = reserve_native_requests_v1(q.bytes_v1())?;
    let recorder = Recorder { inventory, used: RefCell::new(vec![false; inventory.len()]) };
    match check_routed_step_refutation_v2(&built, binding.committed_execution_root, declaration, &recorder, max_leaves) {
        Ok(_) | Err(PalwRoutingCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => {}
        Err(other) => return Err(other),
    }
    let used = recorder.used.borrow();
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<PalwArtifactOpeningV1>(used.iter().filter(|x| **x).count() as u64)?;
    // The existing opening builder hashes the finite inventory and one parent
    // level per opening. Quote those concrete buffers before any construction.
    q.vec_v1::<Hash64>(inventory.len() as u64)?;
    q.vec_v1::<Hash64>(inventory.len() as u64)?;
    q.vec_v1::<Hash64>((inventory.len() as u64).div_ceil(2))?;
    let depth = u64::from(usize::BITS - inventory.len().leading_zeros());
    let maximum_hash = inventory.iter().map(|v| v.bytes.len() as u64 + v.tensor_name.len() as u64 + 128).max().unwrap_or(0);
    q.vec_v1::<u8>(maximum_hash.max(256))?;
    for (at, value) in inventory.iter().enumerate() {
        if used[at] {
            q.add_v1(native_value_requests_v1(value)?)?;
            q.vec_v1::<Hash64>(depth)?;
        }
    }
    let (refutation, mut lease) = built.into_parts_v1();
    extend(&mut lease, q.bytes_v1())?;
    let mut artifact_openings = Vec::with_capacity(used.iter().filter(|x| **x).count());
    for (at, used) in used.iter().enumerate() {
        if !*used {
            continue;
        }
        let opening = crate::palw_artifact::open_artifact_leaf_v1(
            inventory,
            u32::try_from(at).map_err(|_| evidence("artifact index exceeds wire range"))?,
        )
        .ok_or_else(|| evidence("recorded routed parameter cannot be opened"))?;
        crate::palw_artifact::verify_artifact_opening_v1(&opening, declaration.artifact_root)
            .map_err(|_| evidence("recorded routed parameter belongs to another artifact"))?;
        artifact_openings.push(opening);
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(PalwRoutedLeafEvidenceV2 { refutation, artifact_openings }, lease))
}
