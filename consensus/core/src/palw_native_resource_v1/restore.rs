//! Direct typed-carriage restoration. The input may already have been decoded
//! by another owner; this boundary carries typed restoration reservations and
//! continuing residency, not that earlier decode. The shared scanner's open
//! consistency-workspace coverage is not converted into physical approval.
use super::*;
use crate::palw_state_v2::{PalwStateCarriageV2, PalwStateParamsV2, PalwStateV2Error};
use borsh::BorshSerialize;

fn restore_fault_v1(why: impl std::fmt::Display) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<PalwStateCarriageV2>("direct native carriage restoration", &why.to_string())
}

/// Restore the ACTUAL typed carriage under an owner retained by the returned
/// state. No sender-supplied shape or resource metadata is accepted. This API
/// preserves the existing root/consistency checks and their semantic errors.
///
/// The direct API has no canonical frame to scan. It first counts the existing
/// carriage's canonical representation without a buffer, reserves that exact
/// byte Vec before serialization, then applies the same schema preflight as
/// the native wire decoder. Encoded bytes authorize only that explicit Vec;
/// native allocations are independently derived by the typed schema scanner.
pub fn restore_native_carriage_v1(
    point: crate::BlockHash,
    carriage: PalwStateCarriageV2,
    params: &PalwStateParamsV2,
    expected_root: Option<crate::Hash64>,
    uncertified_weightless: bool,
    canonical_work_daa: Option<u64>,
) -> Result<NativeChainStateV1, NativeStateMutationErrorV1> {
    let enabled = native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1();
    restore_with_reservation_v1(
        point,
        carriage,
        params,
        expected_root,
        uncertified_weightless,
        canonical_work_daa,
        enabled,
        reserve_native_requests_v1,
    )
}

#[allow(clippy::too_many_arguments)]
fn restore_with_reservation_v1(
    _point: crate::BlockHash,
    carriage: PalwStateCarriageV2,
    params: &PalwStateParamsV2,
    expected_root: Option<crate::Hash64>,
    uncertified_weightless: bool,
    canonical_work_daa: Option<u64>,
    enabled: bool,
    reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
) -> Result<NativeChainStateV1, NativeStateMutationErrorV1> {
    let mut lease = None;
    let mut codec_lease = None;
    let mut consistency_lease = None;
    // The encoded-frame ticket ends only after the actual byte Vec is dropped;
    // the distinct native ticket follows the restored state.
    let mut canonical = Vec::new();
    if enabled {
        let bytes = borsh::object_length(&carriage).map_err(|e| NativeDecodeErrorV1::Resource(restore_fault_v1(e)))?;
        // RFC-0006: the consistency recount AND the root recomputation's own serialization workspace (entry key/value buffers
        // and the preimage) live until `into_state_v3` returns; both are derived from the actual carriage before it allocates.
        let consistency =
            super::restore_workspace::native_restore_consistency_requests_v1(&carriage).map_err(NativeDecodeErrorV1::Resource)?;
        let root_workspace =
            super::restore_workspace::native_restore_root_workspace_requests_v1(bytes as u64).map_err(NativeDecodeErrorV1::Resource)?;
        let consistency = consistency.checked_add(root_workspace).ok_or_else(|| {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<PalwStateCarriageV2>("restore consistency and root workspace"))
        })?;
        let heap = carriage.native_heap_v1("existing direct carriage ownership");
        let requests = heap
            .bytes()
            .ok_or(NativeDecodeErrorV1::Resource(heap))?
            .checked_add(bytes as u64)
            .and_then(|n| n.checked_add(consistency))
            .and_then(|n| n.checked_add(std::mem::size_of::<NativeChainStateV1>() as u64))
            .ok_or_else(|| {
                NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<PalwStateCarriageV2>("direct restore intake"))
            })?;
        lease = reserve(requests).map_err(NativeDecodeErrorV1::Resource)?;
        let ticket = lease.as_mut().ok_or_else(|| {
            NativeDecodeErrorV1::Resource(restore_fault_v1("enabled direct restore requires a live reservation owner"))
        })?;
        codec_lease = Some(ticket.split_memory_v1(bytes as u64).map_err(NativeDecodeErrorV1::Resource)?);
        consistency_lease = Some(ticket.split_memory_v1(consistency).map_err(NativeDecodeErrorV1::Resource)?);
        canonical.try_reserve_exact(bytes).map_err(|e| NativeDecodeErrorV1::Resource(restore_fault_v1(e)))?;
        carriage.serialize(&mut canonical).map_err(|e| NativeDecodeErrorV1::Resource(restore_fault_v1(e)))?;
        if canonical.len() != bytes {
            return Err(NativeDecodeErrorV1::Resource(restore_fault_v1("canonical length changed during borrowed serialization")));
        }
        let shape = preflight_native_carriage_v1(canonical.as_slice(), bytes as u64)
            .map_err(|e| NativeDecodeErrorV1::Resource(restore_fault_v1(e)))?;
        let restore = shape.memory_requests_v1().map_err(|e| NativeDecodeErrorV1::Resource(restore_fault_v1(e)))?;
        ticket.grow_memory_v1(restore).map_err(NativeDecodeErrorV1::Resource)?;
    }
    let state = carriage.into_state_v3(params, expected_root, uncertified_weightless, canonical_work_daa).map_err(|e| match e {
        PalwStateV2Error::NativeResource(why) => NativeDecodeErrorV1::Resource(why),
        other => NativeDecodeErrorV1::Decode(other),
    })?;
    drop(canonical);
    drop(codec_lease);
    drop(consistency_lease);
    Ok(NativeRetainedV1::from_owner_reserved_v1(state, lease))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::PalwChainStateV2;

    #[test]
    fn direct_restore_reservation_rejects_before_root_validation_and_releases_intake() {
        let params = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("V2") };
        let carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
        let result = restore_with_reservation_v1(
            crate::BlockHash::from_u64_word(1),
            carriage,
            &bundle.state,
            Some(crate::Hash64::from_bytes([1; 64])),
            false,
            None,
            true,
            |n| checker.reserve_requests_v1(n, 0).map(Some),
        );
        assert!(matches!(result, Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(checker.outstanding(), (0, 0));
    }

    #[test]
    fn direct_restore_root_error_keeps_semantic_classification_and_releases_ticket() {
        let params = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("V2") };
        let carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
        let result = restore_with_reservation_v1(
            crate::BlockHash::from_u64_word(1),
            carriage,
            &bundle.state,
            Some(crate::Hash64::from_bytes([1; 64])),
            false,
            None,
            true,
            |n| checker.reserve_requests_v1(n, 0).map(Some),
        );
        assert!(matches!(result, Err(NativeDecodeErrorV1::Decode(_))));
        assert_eq!(checker.outstanding(), (0, 0));
    }

    #[test]
    fn direct_restore_success_keeps_native_owner_and_default_off_state_parity() {
        let params = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("V2") };
        let state = PalwChainStateV2::genesis();
        let root = state.state_root();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: u64::MAX, database_bytes: 0 });
        let owned = restore_with_reservation_v1(
            crate::BlockHash::from_u64_word(1),
            PalwStateCarriageV2::from_state(&state),
            &bundle.state,
            Some(root),
            false,
            None,
            true,
            |n| checker.reserve_requests_v1(n, 0).map(Some),
        )
        .unwrap();
        assert_eq!(*owned, state);
        assert!(checker.outstanding().0 > 0);
        let legacy = restore_with_reservation_v1(
            crate::BlockHash::from_u64_word(1),
            PalwStateCarriageV2::from_state(&state),
            &bundle.state,
            Some(root),
            false,
            None,
            false,
            |_| panic!("disabled capacity must not call reservation"),
        )
        .unwrap();
        assert_eq!(*legacy, *owned);
        drop(owned);
        assert_eq!(checker.outstanding(), (0, 0));
    }
}
