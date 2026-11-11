//! Allocation-independent Selection geometry and service floors. The complete
//! profile also needs the outer carrier, native case writer and bounded archive.
use super::plain_tokenizer::TOKENIZER_DEMAND_ORDINAL_V2;
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_v1::ServiceCost,
    palw_court_shared_material_intake_v1::batch::selection::{
        PlainBatchSelectionV2,
        budget::SelectionTerminalBoundsV2,
        delivery::{SelectionBatchEnvelopeV2, SelectionBatchRowV2},
    },
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1},
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use borsh::BorshSerialize;

const FIRST_SELECTION_DEMAND_ORDINAL_V3: u64 = 1 << 63;

/// Continuation of a timely material case, after its full inspection and
/// localization. This never opens a new case after the initial notice window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub(crate) struct SelectionContinuationBudgetV3 {
    pub ready_offset_daa: u64,
    pub bind: ServiceCost,
    pub bind_daa: u64,
    pub open_daa: u64,
    pub provider_wait_daa: u64,
    pub begin_daa: u64,
    pub parts_daa: u64,
    pub finish_daa: u64,
}

#[cfg(test)]
impl SelectionContinuationBudgetV3 {
    pub(crate) fn test_only_v3() -> Self {
        let bytes = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
        Self {
            // Test schedule: 3120 + 1000 delivery + 1000 inspection +
            // 1000 localization. Production derives this from its schedule.
            ready_offset_daa: 6121,
            bind: ServiceCost { bytes, work: bytes, io: bytes, controls: 1 },
            bind_daa: 1,
            open_daa: 1,
            provider_wait_daa: 1,
            begin_daa: 1,
            parts_daa: 8,
            finish_daa: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub(crate) struct PlainSelectionBudgetV3 {
    pub continuation: SelectionContinuationBudgetV3,
    pub verification: ServiceCost,
    pub verification_daa: u64,
    pub workspace_bytes: u64,
    pub maximum_proof_bytes: u64,
    pub maximum_dependencies: u64,
    pub maximum_ranges_per_role: u64,
    pub maximum_material_visits: u64,
}

pub(crate) struct PlainSelectionBranchV3 {
    pub(super) claim: Hash64,
    pub(super) source: Hash64,
    pub(super) envelope: NativeRetainedV1<SelectionBatchEnvelopeV2>,
    pub(super) bounds: SelectionTerminalBoundsV2,
    pub(super) budget: PlainSelectionBudgetV3,
    geometry: Hash64,
    demands_per_role: u64,
}

impl PlainSelectionBranchV3 {
    pub(crate) fn claim_id_v3(&self) -> Hash64 {
        self.claim
    }

    /// Roles come from the native original custody contract. Each role gets
    /// the same complete shape for one selected row of one protected case.
    pub(crate) fn derive_v3(
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        record: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        roles: u64,
        policy: PlainSelectionBudgetV3,
    ) -> Result<Self, NativeDecodeErrorV1<String>> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let maximum_ranges_per_role = usize::try_from(policy.maximum_ranges_per_role)
            .map_err(|_| Resource(NativeCoverageV1::overflow::<SelectionBatchEnvelopeV2>("plain Selection range cap")))?;
        let shape = PlainBatchSelectionV2::derive_v2(original, source, roles, policy.maximum_dependencies)
            .map_err(|why| Decode(format!("plain Selection geometry: {why:?}")))?;
        let geometry = shape.geometry_id_v2().map_err(Decode)?;
        let bounds = SelectionTerminalBoundsV2::derive_v2(original, source, record, layout)?;
        policy.check_v3(bounds).map_err(|why| Decode(format!("plain Selection budget: {why:?}")))?;
        let envelope = SelectionBatchEnvelopeV2::derive_v2(shape, source, maximum_ranges_per_role, policy.maximum_material_visits)?;
        let demands_per_role = envelope.demands_per_role_v2().count() as u64;
        let branch = Self {
            claim: fp_claim_id_v3(original),
            source: source.source_id_v1(),
            envelope,
            bounds,
            budget: policy,
            geometry,
            demands_per_role,
        };
        let last_demand = usize::try_from(demands_per_role.checked_sub(1).ok_or_else(|| Decode("Selection has no demand".into()))?)
            .map_err(|_| Decode("Selection demand index exceeds platform".into()))?;
        branch.demand_ordinal_v3(roles - 1, last_demand).map_err(|why| Decode(format!("plain Selection ordinal band: {why:?}")))?;
        Ok(branch)
    }

    pub(crate) fn role_count_v3(&self) -> u64 {
        self.envelope.roles_v2()
    }

    pub(crate) fn source_v3(&self) -> Hash64 {
        self.source
    }

    /// Geometry check only. The notice owner authenticates the selected row;
    /// the accepting writer must still check supplier standing and payment.
    pub(in crate::palw_court_shared_material_intake_v1::batch) fn project_checked_demand_v3(
        &self,
        source: &PalwTirPublicSourceV1,
        position: u32,
        role: u64,
        index: usize,
        ordinal: u64,
        ranges: &[crate::palw_batch_court_v1::BatchRangeV1],
    ) -> Result<NativeRetainedV1<SelectionBatchRowV2>, NativeDecodeErrorV1<String>> {
        // Reject a foreign role/ordinal before allocating the projected row.
        let expected = self
            .demand_ordinal_v3(role, index)
            .map_err(|why| NativeDecodeErrorV1::Decode(format!("invalid Selection demand coordinate: {why:?}")))?;
        if ordinal != expected {
            return Err(NativeDecodeErrorV1::Decode("Selection demand substitutes its reserved role or ordinal".into()));
        }
        let row = self.envelope.project_row_v2(source, position, role)?;
        self.envelope.check_demand_ranges_v2(&row, index, ranges)?;
        Ok(row)
    }

    pub(crate) fn demand_ordinal_v3(&self, role: u64, index: usize) -> Result<u64, BatchCourtErrorV1> {
        use BatchCourtErrorV1::CaseGraph;
        let index = u64::try_from(index).map_err(|_| CaseGraph)?;
        if role >= self.role_count_v3() || index >= self.demands_per_role {
            return Err(CaseGraph);
        }
        role.checked_mul(self.demands_per_role)
            .and_then(|n| n.checked_add(index))
            .and_then(|n| n.checked_add(FIRST_SELECTION_DEMAND_ORDINAL_V3))
            // The tokenizer compiler may need two separate demand ordinals.
            .filter(|&n| n < TOKENIZER_DEMAND_ORDINAL_V2 - 1)
            .ok_or(CaseGraph)
    }

    pub(crate) fn identity_v3(&self) -> Result<Hash64, BatchCourtErrorV1> {
        stream_digest(
            b"misaka-palw/plain-batch-selection-branch/v3",
            &(self.geometry, self.envelope.range_maximums_v2(), self.envelope.maximum_payload_bytes_v2(), self.bounds, self.budget),
        )
        .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    /// Physical packet identity. Its namespace is resolved independently of
    /// the later accepted logical case and the selected row.
    pub(crate) fn verification_id_v3(&self, claim: Hash64, scope: Hash64) -> Result<[u8; 64], BatchCourtErrorV1> {
        if claim != self.claim || claim == Hash64::default() || scope == Hash64::default() {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        stream_digest(b"misaka-palw/plain-batch-selection-verification/v3", &(claim, scope, self.identity_v3()?))
            .map(|id| id.as_bytes())
            .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    pub(crate) fn binding_id_v3(&self, claim: Hash64, scope: Hash64) -> Result<[u8; 64], BatchCourtErrorV1> {
        if claim != self.claim || claim == Hash64::default() || scope == Hash64::default() {
            return Err(BatchCourtErrorV1::CaseGraph);
        }
        stream_digest(b"misaka-palw/selection-case-target-binding/v3", &(claim, scope, self.identity_v3()?))
            .map(|id| id.as_bytes())
            .map_err(|_| BatchCourtErrorV1::CaseGraph)
    }

    pub(crate) fn check_terminal_v3(&self, base: ServiceCost, total: ServiceCost) -> Result<(), BatchCourtErrorV1> {
        self.budget.check_terminal_v3(base, total)
    }
}

impl PlainSelectionBudgetV3 {
    fn service_floors_v3(bounds: SelectionTerminalBoundsV2) -> Result<(u64, u64, u64), BatchCourtErrorV1> {
        use BatchCourtErrorV1::DeliveryCost;
        // Raw counts for one objective replay. Native pass count, signature
        // validation and settlement still need a complete production quote.
        let work = bounds
            .signed_control_bytes
            .checked_add(bounds.program_bytes)
            .and_then(|n| n.checked_add(bounds.program_structural_items))
            .and_then(|n| n.checked_add(bounds.ids))
            .and_then(|n| n.checked_add(bounds.tile_lanes.checked_mul(2)?))
            .and_then(|n| n.checked_add(bounds.row_siblings))
            .and_then(|n| n.checked_add(bounds.tile_siblings.checked_mul(2)?))
            .and_then(|n| n.checked_add(bounds.lane_keys))
            .and_then(|n| n.checked_add(bounds.penalty_id_visits))
            .and_then(|n| n.checked_add(bounds.bias_comparisons))
            .and_then(|n| n.checked_add(bounds.sampling_keys))
            .ok_or(DeliveryCost)?;
        // The replay reads the exact authenticated pin and program. The
        // entire row and alternate suppliers are priced in delivery.
        let io = bounds
            .pin_bytes
            .checked_add(bounds.program_bytes)
            .and_then(|n| n.checked_add(bounds.archive_cause_bytes))
            .ok_or(DeliveryCost)?;
        // Source, pin/proof and carrier evidence can coexist. These
        // quantities do not bound the full archive or by-root settlement.
        let workspace = bounds
            .source_construction_requests
            .checked_add(bounds.pin_bytes)
            .and_then(|n| n.checked_add(bounds.proof_bytes))
            .and_then(|n| n.checked_add(bounds.signed_control_bytes))
            .ok_or(DeliveryCost)?;
        Ok((work, io, workspace))
    }

    fn check_v3(self, bounds: SelectionTerminalBoundsV2) -> Result<(), BatchCourtErrorV1> {
        let (work, io, workspace) = Self::service_floors_v3(bounds)?;
        let continuation = self.continuation;
        let notice_bytes = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
        if continuation.ready_offset_daa <= crate::palw_court_policy_v1::NOTICE_WINDOW_DAA
            || [continuation.bind_daa, continuation.open_daa, continuation.provider_wait_daa,
                continuation.begin_daa, continuation.parts_daa, continuation.finish_daa].contains(&0)
            // Inner wire/storage floors only. The production native price must
            // also cover the outer carrier, signature work and receipt row.
            || continuation.bind.controls != 1
            || continuation.bind.bytes < notice_bytes
            || continuation.bind.work < notice_bytes
            || continuation.bind.io < notice_bytes
            || self.verification_daa == 0
            || self.maximum_dependencies == 0
            || self.maximum_ranges_per_role == 0
            || self.maximum_material_visits == 0
            || self.verification.controls != 1
            || !bounds.fits_signed_control_v2(self.maximum_proof_bytes)
            || self.verification.bytes < bounds.signed_control_bytes
            || self.verification.work < work
            || self.verification.io < io
            || self.workspace_bytes < workspace
        {
            return Err(BatchCourtErrorV1::DeliveryCost);
        }
        Ok(())
    }

    pub(crate) fn check_terminal_v3(self, base: ServiceCost, total: ServiceCost) -> Result<(), BatchCourtErrorV1> {
        use BatchCourtErrorV1::DeliveryCost;
        let work = base.work.checked_add(self.verification.work).ok_or(DeliveryCost)?;
        let io = base.io.checked_add(self.verification.io).ok_or(DeliveryCost)?;
        // The terminal has one carrier and must fit either evidence shape.
        // Work and IO must cover both the existing and Selection checks.
        if total.controls != 1 || total.bytes < base.bytes.max(self.verification.bytes) || total.work < work || total.io < io {
            return Err(DeliveryCost);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1;

    #[test]
    fn bct2_selection_branch_binds_complete_geometry_and_service_floors() {
        let fixture = palw_tir_public_frame_fixture_two_tile_v1();
        let frame = &fixture.frame;
        let original = frame.original_v1();
        let source = frame.source_v1(&fixture.record, &fixture.layout, fixture.rules.maximum_leaves).unwrap();
        let bounds = SelectionTerminalBoundsV2::derive_v2(original, &source, &fixture.record, &fixture.layout).unwrap();
        let (work, io, workspace_bytes) = PlainSelectionBudgetV3::service_floors_v3(bounds).unwrap();
        let verification = ServiceCost { bytes: bounds.signed_control_bytes, work, io, controls: 1 };
        let budget = PlainSelectionBudgetV3 {
            continuation: SelectionContinuationBudgetV3::test_only_v3(),
            verification,
            verification_daa: 1,
            workspace_bytes,
            maximum_proof_bytes: bounds.proof_bytes,
            maximum_dependencies: 16,
            maximum_ranges_per_role: 8,
            maximum_material_visits: u64::from(source.decode_count_v1()) * 3,
        };
        let derive =
            |roles, policy| PlainSelectionBranchV3::derive_v3(original, &source, &fixture.record, &fixture.layout, roles, policy);
        let branch = derive(2, budget).unwrap();
        assert_eq!(branch.source_v3(), source.source_id_v1());
        assert_eq!(branch.role_count_v3(), 2);
        assert_eq!(branch.bounds, bounds);
        let partitions = branch.envelope.demands_per_role_v2().count();
        assert_eq!(branch.demand_ordinal_v3(0, 0).unwrap(), FIRST_SELECTION_DEMAND_ORDINAL_V3);
        assert_eq!(branch.demand_ordinal_v3(1, 0).unwrap(), FIRST_SELECTION_DEMAND_ORDINAL_V3 + partitions as u64);
        assert!(branch.demand_ordinal_v3(2, 0).is_err());
        assert!(branch.demand_ordinal_v3(0, partitions).is_err());
        assert_eq!(branch.identity_v3().unwrap(), derive(2, budget).unwrap().identity_v3().unwrap());
        assert_ne!(branch.identity_v3().unwrap(), derive(1, budget).unwrap().identity_v3().unwrap());
        let longer = PlainSelectionBudgetV3 { verification_daa: 2, ..budget };
        assert_ne!(branch.identity_v3().unwrap(), derive(2, longer).unwrap().identity_v3().unwrap());
        let later = PlainSelectionBudgetV3 {
            continuation: SelectionContinuationBudgetV3 {
                ready_offset_daa: budget.continuation.ready_offset_daa + 1,
                ..budget.continuation
            },
            ..budget
        };
        assert_ne!(branch.identity_v3().unwrap(), derive(2, later).unwrap().identity_v3().unwrap());
        let scope = Hash64::from_u64_word(2);
        assert!(branch.verification_id_v3(Hash64::default(), scope).is_err());
        assert_ne!(branch.binding_id_v3(branch.claim, scope).unwrap(), branch.verification_id_v3(branch.claim, scope).unwrap());
        assert_ne!(
            branch.verification_id_v3(branch.claim, scope).unwrap(),
            branch.verification_id_v3(branch.claim, Hash64::from_u64_word(3)).unwrap()
        );

        for short in [
            PlainSelectionBudgetV3 { continuation: SelectionContinuationBudgetV3 { bind_daa: 0, ..budget.continuation }, ..budget },
            PlainSelectionBudgetV3 {
                continuation: SelectionContinuationBudgetV3 {
                    bind: ServiceCost { bytes: budget.continuation.bind.bytes - 1, ..budget.continuation.bind },
                    ..budget.continuation
                },
                ..budget
            },
            PlainSelectionBudgetV3 {
                continuation: SelectionContinuationBudgetV3 { ready_offset_daa: 3_120, ..budget.continuation },
                ..budget
            },
            PlainSelectionBudgetV3 { maximum_proof_bytes: bounds.proof_bytes - 1, ..budget },
            PlainSelectionBudgetV3 { workspace_bytes: workspace_bytes - 1, ..budget },
            PlainSelectionBudgetV3 { verification: ServiceCost { work: work - 1, ..verification }, ..budget },
            PlainSelectionBudgetV3 { verification: ServiceCost { io: io - 1, ..verification }, ..budget },
            PlainSelectionBudgetV3 { verification: ServiceCost { bytes: verification.bytes - 1, ..verification }, ..budget },
        ] {
            assert!(matches!(derive(2, short), Err(NativeDecodeErrorV1::Decode(_))));
        }
        assert!(matches!(
            derive(2, PlainSelectionBudgetV3 { maximum_material_visits: budget.maximum_material_visits - 1, ..budget }),
            Err(NativeDecodeErrorV1::Resource(_))
        ));
        let base = ServiceCost { bytes: 10, work: 3, io: 2, controls: 1 };
        let terminal = ServiceCost { bytes: verification.bytes, work: work + base.work, io: io + base.io, controls: 1 };
        assert!(branch.check_terminal_v3(base, terminal).is_ok());
        assert!(branch.check_terminal_v3(base, ServiceCost { work: terminal.work - 1, ..terminal }).is_err());
    }
}
