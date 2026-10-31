//! Original-layout legacy leaves reconstructed from one retained typed replay.
//! No model execution, full leaf spool, tree copy, wire format or hash domain is
//! introduced. A locally reconstructed leaf must match the original run hash.
use super::super::positive_typed_capture::{TypedRecord, TypedRecordKind};
use super::{AdapterCharge, RecordKey, TypedAdapterError, TypedCaptureAdapter};
use crate::{Elem, Slice, with_slice};
use kaspa_consensus_core::{
    palw_step_leg::{
        PalwStepOpeningV1, PalwStepTileLeafV1, step_leg_max_opening_siblings_v1, step_opening_from_leaf_source_capped_v1,
        step_opening_root_capped_v1, step_range_siblings_from_edges_capped_v1, step_tile_leaf_hash_v1,
    },
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirLeafV1, palw_tir_leaf_preimage_v1},
};

impl TypedCaptureAdapter<'_> {
    fn legacy_count(&self) -> Result<u64, TypedAdapterError> {
        let run = self.capture.run();
        let count = u64::try_from(run.leaf_hashes.len()).map_err(|_| TypedAdapterError::Budget)?;
        if count == 0 || count != run.leaf_count || count > self.limits.max_legacy_leaves {
            return Err(TypedAdapterError::Binding);
        }
        Ok(count)
    }

    /// Charge the existing geometry's binary position lookup before invoking
    /// it. Include the 32 prefix probes, initial/final extent checks, and
    /// bounded traversal over declared blocks and scheduled state/node metadata.
    fn legacy_geometry(&self, index: u64) -> Result<PalwTirLeafV1, TypedAdapterError> {
        if index >= self.legacy_count()? {
            return Err(TypedAdapterError::Geometry);
        }
        let space = self.expected.step_space;
        let nodes = space.occurrences().iter().try_fold(0u64, |sum, (block, _)| {
            sum.checked_add(space.program.blocks[*block as usize].nodes.len() as u64).ok_or(TypedAdapterError::Budget)
        })?;
        let visits = nodes
            .checked_add(space.program.blocks.len() as u64)
            .and_then(|n| n.checked_add(space.occurrences().len() as u64))
            .and_then(|n| n.checked_add(space.fixed_instances().len() as u64))
            .and_then(|n| n.checked_add(space.hist_instances().len() as u64))
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(35))
            .ok_or(TypedAdapterError::Budget)?;
        self.charge(AdapterCharge::LookupVisits, visits)?;
        let leaf = space.leaf_at(self.expected.job, index).ok_or(TypedAdapterError::Geometry)?;
        let bytes = u64::from(leaf.value_count).checked_mul(4).and_then(|n| n.checked_add(26)).ok_or(TypedAdapterError::Budget)?;
        if leaf.index != index
            || leaf.value_count == 0
            || !leaf.dtype.committable()
            || u64::from(leaf.value_count) > self.limits.max_tile_lanes
            || bytes > self.limits.max_legacy_payload_bytes
        {
            return Err(TypedAdapterError::Budget);
        }
        Ok(leaf)
    }

    fn legacy_values_for(&self, leaf: &PalwTirLeafV1) -> Result<Vec<i128>, TypedAdapterError> {
        self.ensure_backing(
            u64::from(leaf.value_count).checked_mul(20).and_then(|n| n.checked_add(512)).ok_or(TypedAdapterError::Budget)?,
        )?;
        let mut values = Vec::new();
        let count = usize::try_from(leaf.value_count).map_err(|_| TypedAdapterError::Budget)?;
        values.try_reserve_exact(count).map_err(|_| TypedAdapterError::Budget)?;
        if values.capacity() != count {
            return Err(TypedAdapterError::Budget);
        }
        match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, block, layer, node, first_element } => {
                let occurrence = u16::try_from(occurrence).map_err(|_| TypedAdapterError::Geometry)?;
                let record = self.record(RecordKey::Node(leaf.position, occurrence, node))?;
                match record.kind {
                    TypedRecordKind::Node { position, occurrence: actual, slot, block: b, layer: l, node: n, committed: true }
                        if position == leaf.position
                            && actual == occurrence
                            && slot == leaf.coord.node_slot
                            && b == block
                            && l == layer
                            && n == node => {}
                    _ => return Err(TypedAdapterError::Geometry),
                }
                append_values(&mut values, record, leaf.dtype, first_element, u64::from(leaf.value_count))?;
            }
            PalwTirLeafKindV1::State { instance, state, layer, first_element } => {
                let inst = self.expected.step_space.fixed_instances().get(instance as usize).ok_or(TypedAdapterError::Geometry)?;
                if inst.state != state || inst.layer != layer || inst.dtype != leaf.dtype {
                    return Err(TypedAdapterError::Geometry);
                }
                let record = self.record(RecordKey::Fixed(leaf.position, state, layer))?;
                if !matches!(record.kind, TypedRecordKind::FixedAfter { position, state: s, layer: l }
                    if position == leaf.position && s == state && l == layer)
                    || record.values.len() as u64 != inst.elements
                {
                    return Err(TypedAdapterError::Geometry);
                }
                append_values(&mut values, record, leaf.dtype, first_element, u64::from(leaf.value_count))?;
            }
            PalwTirLeafKindV1::HistTile { instance, state, layer, first_lane, row_lanes, first_position } => {
                let inst = self.expected.step_space.hist_instances().get(instance as usize).ok_or(TypedAdapterError::Geometry)?;
                let rows =
                    leaf.position.checked_add(1).and_then(|n| n.checked_sub(first_position)).ok_or(TypedAdapterError::Geometry)?;
                if inst.state != state
                    || inst.layer != layer
                    || inst.dtype != leaf.dtype
                    || rows != self.expected.step_space.layout.h_tile
                    || row_lanes == 0
                    || row_lanes.checked_mul(rows) != Some(leaf.value_count)
                    || first_lane.checked_add(u64::from(row_lanes)).is_none_or(|end| end > inst.elements)
                {
                    return Err(TypedAdapterError::Geometry);
                }
                for row in first_position..=leaf.position {
                    let record = self.record(RecordKey::History(row, state, layer))?;
                    if !matches!(record.kind, TypedRecordKind::HistoryAppendRow { position, state: s, layer: l }
                        if position == row && s == state && l == layer)
                        || record.values.len() as u64 != inst.elements
                    {
                        return Err(TypedAdapterError::Geometry);
                    }
                    append_values(&mut values, record, leaf.dtype, first_lane, u64::from(row_lanes))?;
                }
            }
        }
        if values.len() != count {
            return Err(TypedAdapterError::Geometry);
        }
        Ok(values)
    }

    /// Reconstruct only this old leaf's bounded lanes, then verify its original
    /// local hash. Wide uncommitted nodes never pass through this four-byte API.
    pub fn legacy_preimage(&self, index: u64) -> Result<PalwStepTileLeafV1, TypedAdapterError> {
        let leaf = self.legacy_geometry(index)?;
        let values = self.legacy_values_for(&leaf)?;
        let preimage = palw_tir_leaf_preimage_v1(&leaf, &values).map_err(|_| TypedAdapterError::Value)?;
        // The old hash writer copies the encoded preimage. Release i128 scratch
        // first, keeping peak tile backing bounded by the 20-byte/lane quote.
        drop(values);
        self.charge(AdapterCharge::HashWork, 1)?;
        let actual = step_tile_leaf_hash_v1(&self.capture.origin().execution, &self.capture.origin().class, &preimage);
        let expected = *self
            .capture
            .run()
            .leaf_hashes
            .get(usize::try_from(index).map_err(|_| TypedAdapterError::Budget)?)
            .ok_or(TypedAdapterError::Missing)?;
        if actual != expected {
            return Err(TypedAdapterError::Binding);
        }
        Ok(preimage)
    }

    /// Requested legacy tile values only, after verifying its original hash.
    /// Conversion allocates no full-node or retained-window copy.
    pub(crate) fn legacy_values(&self, index: u64) -> Result<Vec<i128>, TypedAdapterError> {
        let preimage = self.legacy_preimage(index)?;
        let leaf = self.legacy_geometry(index)?;
        let mut values = Vec::new();
        let count = usize::try_from(leaf.value_count).map_err(|_| TypedAdapterError::Budget)?;
        values.try_reserve_exact(count).map_err(|_| TypedAdapterError::Budget)?;
        if values.capacity() != count {
            return Err(TypedAdapterError::Budget);
        }
        for bytes in preimage.values_le.chunks_exact(4) {
            let word: [u8; 4] = bytes.try_into().map_err(|_| TypedAdapterError::Value)?;
            values.push(if leaf.dtype == misaka_palw_tir::DType::Idx {
                u32::from_le_bytes(word) as i128
            } else {
                i32::from_le_bytes(word) as i128
            });
        }
        Ok(values)
    }

    /// Stream the existing saved leaf hashes through the core's O(log N)-RAM
    /// membership builder. Reserve ALL reads/hash work before its infallible
    /// callback. Session meters persist across successful and failed requests.
    pub fn legacy_opening(&self, index: u64) -> Result<PalwStepOpeningV1, TypedAdapterError> {
        self.charge(AdapterCharge::ProofAttempts, 1)?;
        let count = self.legacy_count()?;
        if index >= count {
            return Err(TypedAdapterError::Geometry);
        }
        let depth = step_leg_max_opening_siblings_v1(count);
        self.ensure_backing(
            (depth as u64)
                .checked_add(1)
                .and_then(|n| n.checked_mul(8 * 64))
                .and_then(|n| n.checked_add(4096))
                .ok_or(TypedAdapterError::Budget)?,
        )?;
        if depth as u64 > u64::from(self.limits.max_path_siblings) {
            return Err(TypedAdapterError::Budget);
        }
        self.charge(AdapterCharge::ProofLeafReads, count)?;
        self.charge(AdapterCharge::HashWork, count.checked_mul(2).ok_or(TypedAdapterError::Budget)?)?;
        let hashes = &self.capture.run().leaf_hashes;
        let opening = step_opening_from_leaf_source_capped_v1(count, index, self.limits.max_legacy_leaves, |i| hashes[i as usize])
            .map_err(|error| TypedAdapterError::Source(error.to_string()))?;
        if opening.siblings.len() > depth
            || opening.leaf_hash != hashes[index as usize]
            || step_opening_root_capped_v1(count, &opening, self.limits.max_legacy_leaves)
                .map_err(|error| TypedAdapterError::Source(error.to_string()))?
                != self.capture.run().step_merkle_root
        {
            return Err(TypedAdapterError::Binding);
        }
        Ok(opening)
    }

    /// Use two bounded streamed edge openings and the unchanged core range
    /// constructor. No full-tree level or range leaf-hash buffer is materialized.
    pub fn legacy_range_siblings(&self, first: u64, count: u64) -> Result<Vec<kaspa_consensus_core::Hash64>, TypedAdapterError> {
        let leaves = self.legacy_count()?;
        let end = first.checked_add(count).filter(|end| count > 0 && *end <= leaves).ok_or(TypedAdapterError::Geometry)?;
        let first_edge = self.legacy_opening(first)?;
        let last_edge = if end - 1 == first { None } else { Some(self.legacy_opening(end - 1)?) };
        let last_edge = last_edge.as_ref().unwrap_or(&first_edge);
        let depth = step_leg_max_opening_siblings_v1(leaves) as u64;
        self.charge(AdapterCharge::HashWork, depth.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or(TypedAdapterError::Budget)?)?;
        let siblings = step_range_siblings_from_edges_capped_v1(
            leaves,
            &first_edge,
            last_edge,
            self.capture.run().step_merkle_root,
            self.limits.max_legacy_leaves,
        )
        .map_err(|error| TypedAdapterError::Source(error.to_string()))?;
        if siblings.len() as u64 > u64::from(self.limits.max_path_siblings).checked_mul(2).ok_or(TypedAdapterError::Budget)? {
            return Err(TypedAdapterError::Budget);
        }
        Ok(siblings)
    }

    pub fn validate_legacy(
        &self,
        index: u64,
        preimage: &PalwStepTileLeafV1,
        opening: &PalwStepOpeningV1,
    ) -> Result<(), TypedAdapterError> {
        self.charge(AdapterCharge::ProofAttempts, 1)?;
        let count = self.legacy_count()?;
        if opening.leaf_index != index
            || opening.siblings.len() as u64 > u64::from(self.limits.max_path_siblings)
            || preimage.values_le.len() as u64 > self.limits.max_legacy_payload_bytes
        {
            return Err(TypedAdapterError::Binding);
        }
        let expected = self.legacy_preimage(index)?;
        if &expected != preimage || opening.leaf_hash != self.capture.run().leaf_hashes[index as usize] {
            return Err(TypedAdapterError::Binding);
        }
        self.charge(AdapterCharge::HashWork, (opening.siblings.len() as u64).checked_add(1).ok_or(TypedAdapterError::Budget)?)?;
        if step_opening_root_capped_v1(count, opening, self.limits.max_legacy_leaves)
            .map_err(|error| TypedAdapterError::Source(error.to_string()))?
            != self.capture.run().step_merkle_root
        {
            return Err(TypedAdapterError::Binding);
        }
        Ok(())
    }
}

fn append_values(
    target: &mut Vec<i128>,
    record: &TypedRecord,
    dtype: misaka_palw_tir::DType,
    first: u64,
    count: u64,
) -> Result<(), TypedAdapterError> {
    let shape_lanes = record.shape.iter().try_fold(1usize, |n, dim| n.checked_mul(*dim)).ok_or(TypedAdapterError::Geometry)?;
    if record.dtype != dtype || record.values.dtype() != dtype || shape_lanes != record.values.len() {
        return Err(TypedAdapterError::Geometry);
    }
    let first = usize::try_from(first).map_err(|_| TypedAdapterError::Geometry)?;
    let count = usize::try_from(count).map_err(|_| TypedAdapterError::Geometry)?;
    let end = first.checked_add(count).filter(|end| *end <= shape_lanes).ok_or(TypedAdapterError::Geometry)?;
    if target.len().checked_add(count).is_none_or(|end| end > target.capacity()) {
        return Err(TypedAdapterError::Budget);
    }
    let data: Slice<'_> = record.values.slice();
    with_slice!(data, values => {
        for value in &values[first..end] {
            let value = value.to_i128();
            if !dtype.contains(value) { return Err(TypedAdapterError::Value); }
            target.push(value);
        }
    });
    Ok(())
}
