//! Canonical material ranges for Selection's potential row, before a notice.
//! A role has its own demands: equal material IDs from different suppliers
//! must never be folded into one opportunity. This is delivery geometry only;
//! replay, default/recovery, fees and actual booking remain native planner work.
use super::PlainBatchSelectionV2;
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::{MaterialTargetV1, batch::native_ranges::partition_maximums_v1},
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1 as E, NativeRetainedV1, TirNativeRequestsV1, reserve_native_pending_requests_v1,
    },
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectionBatchMaterialV2 {
    pub id: Hash64,
    pub target: MaterialTargetV1,
    pub maximum_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_batch_court_v1::{
            BatchRangeAuthenticationV1, BatchRangeV1, MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1,
        },
        palw_court_tir_requests_v1::TirSharedRequestSetV1,
        palw_tir_public_source_v1::PalwTirPublicRequestV1,
    };

    #[test]
    fn bct2_selection_delivery_covers_canonical_rows_and_distinct_roles() {
        for fixture in [
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 3),
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1(),
        ] {
            let frame = &fixture.frame;
            let source = PalwTirPublicSourceV1::new_native_v1(
                &fixture.record,
                &fixture.layout,
                frame.header_v1(),
                frame.original_v1().execution_root,
                frame.original_v1().trace_root,
                fixture.rules.maximum_leaves,
            )
            .unwrap();
            let roles = frame.custody_v1().pledge_v1().maximum_custodians;
            let shape = PlainBatchSelectionV2::derive_v2(frame.original_v1(), &source, roles, u64::MAX).unwrap();
            let count = shape.tiles as usize + 1;
            let visits = count as u64 * u64::from(shape.positions);
            let envelope = SelectionBatchEnvelopeV2::derive_v2(shape, &source, count, visits).unwrap();
            assert_eq!(envelope.roles_v2(), roles);
            let requests = TirSharedRequestSetV1::new(frame, &source, Hash64::from_u64_word(91)).unwrap();
            let mut actual_maximums = vec![0; count];
            let mut saw_full_ids_not_first = false;
            for position in 0..source.decode_count_v1() {
                let mut role_zero = None;
                for role in 0..roles {
                    let row = envelope.project_row_v2(&source, position, role).unwrap();
                    assert_eq!((row.position, row.role), (position, role));
                    assert_eq!(row.materials.len(), count);
                    assert!(row.materials.windows(2).all(|pair| pair[0].id < pair[1].id));
                    saw_full_ids_not_first |= row.materials[0].target != MaterialTargetV1::FullIds;
                    let mut tiles = std::collections::BTreeSet::new();
                    let mut ids = 0;
                    for (offset, material) in row.materials.iter().enumerate() {
                        assert_eq!(material.id, material.target.id_v1(frame.plan_v1().original_claim).unwrap());
                        match material.target {
                            MaterialTargetV1::FullIds => ids += 1,
                            MaterialTargetV1::Unit { ordinal } => {
                                let request = requests.request_at(ordinal).unwrap();
                                let PalwTirPublicRequestV1::TraceTile { row, tile } = request else {
                                    panic!("Selection delivery substituted a non-row material")
                                };
                                assert_eq!(row, position);
                                assert!(tiles.insert(tile));
                                assert_eq!(material.maximum_bytes, source.unit_bound_v1(&request).unwrap().maximum_response_bytes);
                            }
                            _ => panic!("Selection delivery substituted a foreign family"),
                        }
                        actual_maximums[offset] = actual_maximums[offset].max(material.maximum_bytes);
                        assert!(material.maximum_bytes <= envelope.range_maximums_v2()[offset]);
                    }
                    assert_eq!(ids, 1);
                    assert_eq!(tiles.into_iter().collect::<Vec<_>>(), (0..shape.tiles).collect::<Vec<_>>());
                    let demands = envelope.row_demands_v2(&row).unwrap().collect::<Vec<_>>();
                    assert_eq!(demands.iter().map(|demand| demand.len()).sum::<usize>(), count);
                    for (actual, maximum) in demands.iter().zip(envelope.demands_per_role_v2()) {
                        assert_eq!(actual.len(), maximum.len());
                        assert!(actual.len() <= MAX_BATCH_RANGES_V1);
                        assert!(
                            maximum.iter().map(|bytes| bytes.div_ceil(u64::from(MAX_BATCH_PART_BYTES_V1))).sum::<u64>()
                                <= MAX_BATCH_PARTS_V1 as u64
                        );
                    }
                    for (index, demand) in demands.iter().enumerate() {
                        let ranges = demand
                            .iter()
                            .map(|material| BatchRangeV1 {
                                id: material.id,
                                bytes: u32::try_from(material.maximum_bytes).unwrap(),
                                authentication: BatchRangeAuthenticationV1::WholeRange,
                            })
                            .collect::<Vec<_>>();
                        envelope.check_demand_ranges_v2(&row, index, &ranges).unwrap();
                        let mut changed = ranges.clone();
                        changed[0].bytes += 1;
                        assert!(envelope.check_demand_ranges_v2(&row, index, &changed).is_err());
                        changed[0].bytes = 0;
                        assert!(envelope.check_demand_ranges_v2(&row, index, &changed).is_err());
                        changed = ranges.clone();
                        changed[0].authentication = BatchRangeAuthenticationV1::IndependentParts;
                        assert!(envelope.check_demand_ranges_v2(&row, index, &changed).is_err());
                        changed = ranges.clone();
                        changed[0].id = Hash64::default();
                        assert!(envelope.check_demand_ranges_v2(&row, index, &changed).is_err());
                        assert!(envelope.check_demand_ranges_v2(&row, index, &ranges[1..]).is_err());
                        assert!(envelope.check_demand_ranges_v2(&row, demands.len(), &ranges).is_err());
                        if ranges.len() > 1 {
                            changed = ranges.clone();
                            changed.swap(0, 1);
                            assert!(envelope.check_demand_ranges_v2(&row, index, &changed).is_err());
                        }
                    }
                    if let Some(ref first) = role_zero {
                        assert_eq!(&row.materials, first);
                    } else {
                        role_zero = Some(row.materials.clone());
                    }
                    let (row, owner) = row.into_parts_v1();
                    assert!(owner.is_some(), "ordinary nodes must own projected rows");
                    let row = NativeRetainedV1::from_owner_reserved_v1(row, owner);
                    let other_shape = PlainBatchSelectionV2::derive_v2(frame.original_v1(), &source, roles + 1, u64::MAX).unwrap();
                    let other = SelectionBatchEnvelopeV2::derive_v2(other_shape, &source, count, visits).unwrap();
                    assert!(other.row_demands_v2(&row).is_err());
                }
            }
            assert!(saw_full_ids_not_first, "fixture must detect an assumed FullIds-first ordering");
            assert_eq!(envelope.range_maximums_v2(), actual_maximums);
            assert_eq!(envelope.maximum_payload_bytes_v2(), actual_maximums.iter().sum::<u64>() * roles);
            assert!(envelope.maximum_payload_bytes_v2() >= shape.payload_bytes_per_target_v2());
            assert!(matches!(SelectionBatchEnvelopeV2::derive_v2(shape, &source, count - 1, visits), Err(E::Resource(_))));
            assert!(matches!(SelectionBatchEnvelopeV2::derive_v2(shape, &source, count, visits - 1), Err(E::Resource(_))));
            assert!(envelope.project_row_v2(&source, shape.positions, 0).is_err());
            assert!(envelope.project_row_v2(&source, 0, roles).is_err());
            let (_, owner) = envelope.into_parts_v1();
            assert!(owner.is_some(), "ordinary nodes must own the preclaim envelope");
        }
    }
}

/// Each offset bounds that offset in *every* canonically sorted row. FullIds
/// is not necessarily first; hashing a different row changes the ordering.
#[allow(dead_code)]
pub(crate) struct SelectionBatchEnvelopeV2 {
    shape: PlainBatchSelectionV2,
    maximums: Vec<u64>,
    maximum_payload_bytes: u64,
}

/// One supplier's row. It cannot substitute a second role's reserved duty.
#[allow(dead_code)]
pub(crate) struct SelectionBatchRowV2 {
    shape: PlainBatchSelectionV2,
    position: u32,
    role: u64,
    materials: Vec<SelectionBatchMaterialV2>,
}

fn resource(why: &str) -> E<String> {
    E::Resource(NativeCoverageV1::unsupported::<SelectionBatchEnvelopeV2>("Selection Batch geometry", why))
}

fn empty_exact<T>(count: usize) -> Result<Vec<T>, E<String>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| resource("range allocation failed"))?;
    if values.capacity() != count {
        return Err(resource("allocator exceeded admitted range capacity"));
    }
    Ok(values)
}

fn fill_row(
    shape: &PlainBatchSelectionV2,
    source: &PalwTirPublicSourceV1,
    position: u32,
    role: u64,
    materials: &mut Vec<SelectionBatchMaterialV2>,
) -> Result<(), E<String>> {
    if position >= shape.positions || role >= shape.roles || source.source_id_v1() != shape.source {
        return Err(E::Decode("Selection Batch row substitutes source, position or supplier role".into()));
    }
    materials.clear();
    for group in 0..=u64::from(shape.tiles) {
        // Beat does not change potential materials; the authenticated FullIds
        // and selected tiles determine the actual comparison at verification.
        let material = shape.material_at_v2(source, position, 0, group * shape.roles + role).map_err(E::Decode)?;
        materials.push(SelectionBatchMaterialV2 {
            id: material.target.id_v1(shape.claim).map_err(E::Decode)?,
            target: material.target,
            maximum_bytes: material.maximum_bytes,
        });
    }
    materials.sort_unstable_by_key(|material| material.id);
    if materials.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(E::Decode("Selection Batch material identity collision".into()));
    }
    Ok(())
}

#[allow(dead_code)]
impl SelectionBatchEnvelopeV2 {
    /// Walk D rows, never D*V comparisons. Limits are local exploration bounds,
    /// not permission to omit a row or reduce the native supplier role count.
    pub(crate) fn derive_v2(
        shape: PlainBatchSelectionV2,
        source: &PalwTirPublicSourceV1,
        maximum_ranges_per_role: usize,
        maximum_material_visits: u64,
    ) -> Result<NativeRetainedV1<Self>, E<String>> {
        let count = u64::from(shape.tiles) + 1;
        let visits = count.checked_mul(u64::from(shape.positions)).ok_or_else(|| resource("row visit count overflow"))?;
        let cap = usize::try_from(count).map_err(|_| resource("range count exceeds platform"))?;
        if cap > maximum_ranges_per_role || visits > maximum_material_visits {
            return Err(resource("complete Selection range exploration exceeds local budget"));
        }
        if shape.source != source.source_id_v1() {
            return Err(E::Decode("Selection Batch envelope substitutes source".into()));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Self>(1).map_err(E::Resource)?;
        q.exact_v1::<u64>(count).map_err(E::Resource)?;
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut scratch = TirNativeRequestsV1::new();
        scratch.vec_v1::<SelectionBatchMaterialV2>(count).map_err(E::Resource)?;
        // Material IDs stream directly into a stack hash state; sorting is
        // in-place and the one scratch row is reused across positions.
        let _scratch_owner = reserve_native_pending_requests_v1(scratch.bytes_v1()).map_err(E::Resource)?;
        let mut maximums = empty_exact::<u64>(cap)?;
        maximums.resize(cap, 0);
        let mut materials = empty_exact::<SelectionBatchMaterialV2>(cap)?;
        for position in 0..shape.positions {
            fill_row(&shape, source, position, 0, &mut materials)?;
            for (maximum, material) in maximums.iter_mut().zip(&materials) {
                *maximum = (*maximum).max(material.maximum_bytes);
            }
        }
        // The pointwise sorted envelope can exceed every individual row's
        // sum. Price its whole role product, not shape.payload_bytes.
        let maximum_payload_bytes = maximums
            .iter()
            .try_fold(0u64, |sum, bytes| sum.checked_add(*bytes))
            .and_then(|bytes| bytes.checked_mul(shape.roles))
            .ok_or_else(|| E::Decode("Selection Batch complete supplier payload overflows".into()))?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(Self { shape, maximums, maximum_payload_bytes }, owner))
    }

    pub(crate) fn roles_v2(&self) -> u64 {
        self.shape.roles
    }

    pub(crate) fn range_maximums_v2(&self) -> &[u64] {
        &self.maximums
    }

    pub(crate) fn maximum_payload_bytes_v2(&self) -> u64 {
        self.maximum_payload_bytes
    }

    /// Check the signed range list against one native row/role demand. The
    /// caller still has to authenticate notice, supplier and booked slot.
    pub(crate) fn check_demand_ranges_v2(
        &self,
        row: &SelectionBatchRowV2,
        demand_index: usize,
        ranges: &[crate::palw_batch_court_v1::BatchRangeV1],
    ) -> Result<(), E<String>> {
        use crate::palw_batch_court_v1::BatchRangeAuthenticationV1;
        let expected = self
            .row_demands_v2(row)?
            .nth(demand_index)
            .ok_or_else(|| E::Decode("Selection Batch demand outside its original partition".into()))?;
        if expected.len() != ranges.len()
            || expected.iter().zip(ranges).any(|(material, range)| {
                range.id != material.id
                    || range.bytes == 0
                    || u64::from(range.bytes) > material.maximum_bytes
                    || range.authentication != BatchRangeAuthenticationV1::WholeRange
            })
        {
            return Err(E::Decode("Selection Batch demand substitutes material, length or authentication".into()));
        }
        Ok(())
    }

    /// Every supplier gets these same boundaries, even when its actual bytes
    /// are smaller. No signed wire ordinal is assigned by this projection.
    pub(crate) fn demands_per_role_v2(&self) -> impl Iterator<Item = &[u64]> {
        partition_maximums_v1(&self.maximums)
    }

    pub(crate) fn project_row_v2(
        &self,
        source: &PalwTirPublicSourceV1,
        position: u32,
        role: u64,
    ) -> Result<NativeRetainedV1<SelectionBatchRowV2>, E<String>> {
        if position >= self.shape.positions || role >= self.shape.roles || source.source_id_v1() != self.shape.source {
            return Err(E::Decode("Selection Batch row outside original source/role domain".into()));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<SelectionBatchRowV2>(1).map_err(E::Resource)?;
        q.exact_v1::<SelectionBatchMaterialV2>(self.maximums.len() as u64).map_err(E::Resource)?;
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut materials = empty_exact(self.maximums.len())?;
        fill_row(&self.shape, source, position, role, &mut materials)?;
        if materials.iter().zip(&self.maximums).any(|(material, maximum)| material.maximum_bytes > *maximum) {
            return Err(E::Decode("Selection Batch row exceeds its original envelope".into()));
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(SelectionBatchRowV2 { shape: self.shape, position, role, materials }, owner))
    }

    pub(crate) fn row_demands_v2<'a>(
        &'a self,
        row: &'a SelectionBatchRowV2,
    ) -> Result<impl Iterator<Item = &'a [SelectionBatchMaterialV2]>, E<String>> {
        if row.shape != self.shape
            || row.position >= self.shape.positions
            || row.role >= self.shape.roles
            || row.materials.len() != self.maximums.len()
        {
            return Err(E::Decode("Selection Batch row differs from its envelope".into()));
        }
        let mut offset = 0;
        Ok(self.demands_per_role_v2().map(move |maximums| {
            let start = offset;
            offset += maximums.len();
            &row.materials[start..offset]
        }))
    }
}
