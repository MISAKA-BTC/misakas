//! Exact native aux codecs; no opaque row size stands for nested decoded demand.
use super::*;
use std::{collections::BTreeMap, io::Read, mem::size_of};
impl NativeWireScanV1 for crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        let mut sum = crate::palw_kernel_route_v1::PalwKernelRouteHeaderV1::scan_native_wire_v1(reader)?;
        // The persisted ledger rows are native byte Vecs. State root/restore hashes
        // them directly; later full ledger class/program expansion is a separate
        // consumer operation, never inferred from their encoded row length here.
        sum = sum.merge(BTreeMap::<(u8, Vec<u8>), Vec<u8>>::scan_native_wire_v1(reader)?)?;
        let count = reader.word()? as u64;
        for _ in 0..count {
            let table = reader.byte()?;
            let key_bytes = reader.word()? as u64;
            if key_bytes > reader.remaining() {
                return Err(wire::wire_bad("native aux key exceeds frame"));
            }
            if key_bytes > NATIVE_AUX_KEY_MAX_BYTES_V1 {
                return Err(wire::wire_bad("native aux key exceeds the audited aux key bound"));
            }
            let mut key = [0u8; NATIVE_AUX_KEY_MAX_BYTES_V1 as usize];
            reader.step()?;
            reader.read_exact(&mut key[..key_bytes as usize])?;
            sum = sum.allocation(key_bytes.checked_mul(2).ok_or_else(|| wire::wire_bad("native aux key allocation overflow"))?, 1)?;
            let row_bytes = reader.word()? as u64;
            if row_bytes > reader.remaining() {
                return Err(wire::wire_bad("native aux row exceeds frame"));
            }
            let mut frame = NativeWireReaderV1::new((&mut *reader).take(row_bytes), row_bytes);
            let shape = scan_native_aux_row_v1(table, &key[..key_bytes as usize], &mut frame)?;
            if frame.remaining() != 0 {
                return Err(wire::wire_bad("native aux row codec has trailing/mismatched bytes"));
            }
            sum = sum.merge(
                shape.allocation(row_bytes.checked_mul(2).ok_or_else(|| wire::wire_bad("native aux byte allocation overflow"))?, 1)?,
            )?;
        }
        sum = sum.allocation(
            count.checked_mul(2).ok_or_else(|| wire::wire_bad("native aux Vec overflow"))?,
            size_of::<((u8, Vec<u8>), Vec<u8>)>(),
        )?;
        sum = sum.allocation(count, size_of::<typed::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>())?;
        // Restore consistency builds per-job and per-producer counters from the
        // actual aux rows. The total row count bounds both fixed-key scratch maps.
        sum = sum.allocation(
            count.checked_mul(2).ok_or_else(|| wire::wire_bad("native aux scratch count overflow"))?,
            size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], u32>>(),
        )?;
        sum.outer_entries = count;
        sum.collection_entries =
            sum.collection_entries.checked_add(count).ok_or_else(|| wire::wire_bad("native aux count overflow"))?;
        Ok(sum)
    }
}

/// One aux row's exact codec, dispatched on its table and FULL key (`k`) and, where two family layouts share a key shape, its row
/// length (`n`). Tables 42–106 and 112 follow scripts/drills/rfc06_native_aux_spec.py; a key shape no fold writes fails closed.
pub(crate) fn scan_native_aux_row_v1<R: Read>(
    table: u8,
    key: &[u8],
    frame: &mut NativeWireReaderV1<R>,
) -> std::io::Result<NativeWireShapeV1> {
    let k = key;
    let n = frame.remaining();
    let key_bytes = k.len() as u64;
    let prefix = k.first().copied().unwrap_or(0);
    Ok(match table {
        32 => <crate::palw_state_v2::PalwBondKeyV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        33 => <crate::palw_kernel_route_v1::KernelAssignmentV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        34 => <Vec<misaka_palw_kernel::receipt::PalwConstraintReceiptV1> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        35 => <(u64, u32, u64) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        36 => <crate::palw_onboarding_v1::ArtifactBindingRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        37 => <crate::palw_onboarding_v1::KernelBindingRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        38 => <crate::palw_onboarding_v1::ConformanceRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        39 => <crate::palw_onboarding_v1::ConformanceAttemptRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        40 => <crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        41 => <crate::palw_kernel_route_v1::PalwKernelChunkGroupV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        107 => <u64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        108 => <misaka_palw_kernel::beacon_source::BeaconSourceTargetV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        109 => <misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        110 => <crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        111 => <crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        128 => <crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        113 => <misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        114 => <misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        115 if key_bytes == 64 => {
            <crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        116 => <crate::palw_claim_challenge_v2::PalwPaidClaimLockV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        117 => <misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        118 => <misaka_palw_challenge::approval_scope::LifetimeCounterV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        119 => <crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        120 => <crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        122 => <misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        123 if key_bytes == 64 => {
            <crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        124 => <crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        115 if key_bytes == 65 && matches!(prefix, 0x49 | 0x42) => {
            crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(frame)?
        }
        123 if key_bytes == 65 && prefix == 0x49 => crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3::scan_native_wire_v1(frame)?,
        // RFC07/RFC08 signed economic contracts (2026-10-15): tallies keyed by a 64-byte id; extensions and contracts under their
        // 0x01 prefix; the minimum-price job index (0x02) holds a 64-byte contract id.
        125 if key_bytes == 64 => crate::palw_econ_contracts_v1::PalwModelVerificationTallyV1::scan_native_wire_v1(frame)?,
        126 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::HORIZON_EXTENSION_PREFIX_V1 => {
            crate::palw_econ_contracts_v1::PalwHorizonExtensionRowV1::scan_native_wire_v1(frame)?
        }
        127 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::MINIMUM_PRICE_CONTRACT_PREFIX_V1 => {
            crate::palw_econ_contracts_v1::PalwMinimumPriceRowV1::scan_native_wire_v1(frame)?
        }
        127 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::MINIMUM_PRICE_JOB_PREFIX_V1 => {
            <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        // Round 3 (2026-10-15): the verification gate wiring rows (aux 125, 65-byte prefixed keys) and the N3
        // BootstrapExact rounds (aux 130, keyed by the 64-byte round id).
        125 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::MODEL_VERIFICATION_CONTEXT_PREFIX_V1 => {
            crate::palw_econ_contracts_v1::PalwModelVerificationContextV1::scan_native_wire_v1(frame)?
        }
        125 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::MODEL_VERIFICATION_DEFERRED_PREFIX_V1 => {
            crate::palw_econ_contracts_v1::PalwModelVerificationDeferredV1::scan_native_wire_v1(frame)?
        }
        125 if key_bytes == 65 && prefix == crate::palw_econ_contracts_v1::MODEL_VERIFICATION_DECIDED_PREFIX_V1 => {
            crate::palw_econ_contracts_v1::PalwModelVerificationDecidedV1::scan_native_wire_v1(frame)?
        }
        130 if key_bytes == 64 => crate::palw_beacon_bootstrap_v1::PalwBootstrapRoundRowV1::scan_native_wire_v1(frame)?,
        // RFC-0017 (2026-10-15): the model eligibility rows (aux 131, 65-byte keys: prefix ‖ 64-byte id).
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_MODEL_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityModelRowV1::scan_native_wire_v1(frame)?
        }
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_LINEAGE_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityLineageRowV1::scan_native_wire_v1(frame)?
        }
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_PRINCIPAL_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityPrincipalRowV1::scan_native_wire_v1(frame)?
        }
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_NETWORK_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityNetworkRowV1::scan_native_wire_v1(frame)?
        }
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_EXAM_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityExamRowV1::scan_native_wire_v1(frame)?
        }
        131 if key_bytes == 65 && prefix == crate::palw_model_eligibility_v1::ELIGIBILITY_PROBATION_CLAIM_PREFIX_V1 => {
            crate::palw_model_eligibility_v1::PalwEligibilityProbationClaimRowV1::scan_native_wire_v1(frame)?
        }
        134 if key_bytes == 64 => {
            let shape = crate::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2::scan_native_wire_v1(frame)?;
            let count =
                shape.collection_entries.checked_add(1).ok_or_else(|| wire::wire_bad("recovery restore membership overflow"))?;
            shape.allocation(
                count,
                size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u64>>()
                    + size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, ()>>(),
            )?
        }
        134 if key_bytes == 69 && prefix == 0xA1 => {
            crate::palw_material_release_recovery_v2::PalwMaterialProviderAckCheckpointV2::scan_native_wire_v1(frame)?.allocation(
                1,
                size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<crate::palw_state_v2::PalwBondKeyV2, u64>>(),
            )?
        }
        134 if key_bytes == 69 && prefix == 0xA2 => {
            crate::palw_material_challenge_checkpoint_v4::PalwMaterialChallengeCheckpointV4::scan_native_wire_v1(frame)?
        }
        134 if key_bytes == 69 && prefix == 0xA3 => {
            crate::palw_material_challenge_history_v4::PalwMaterialChallengeHistoryV4::scan_native_wire_v1(frame)?
        }
        134 => return Err(wire::wire_bad("unallocated material recovery row namespace")),
        135 if key_bytes == 64 => crate::palw_claim_compact_v4::PalwPaidCompactContractV4::scan_native_wire_v1(frame)?,
        135 if key_bytes == 65 && prefix == 0x50 => {
            crate::palw_claim_compact_v4::PalwPaidCompactProofBindingV4::scan_native_wire_v1(frame)?
        }
        136 if key_bytes == 64 => crate::palw_reader_router_v2::PalwReaderRouterRowV2::scan_native_wire_v1(frame)?,
        136 if key_bytes == 65 && prefix == 0xA1 => <u64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        136 if key_bytes == 65 && prefix == 0xA2 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        136 => return Err(wire::wire_bad("unallocated reader/router V2 row namespace")),
        135 => return Err(wire::wire_bad("unallocated compact V4 row namespace")),
        121 => match (key_bytes, prefix) {
            (65, 0x41) => crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2::scan_native_wire_v1(frame)?,
            (65, 0x42) => crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2::scan_native_wire_v1(frame)?,
            (64, _) => crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2::scan_native_wire_v1(frame)?,
            _ => return Err(wire::wire_bad("unallocated Panel lifetime row namespace")),
        },
        // Tables 42–106 and 112 (rfc06_native_aux_spec.py; inventory: docs/design/palw/native-resource-aux-inventory-2026-10-15.md).
        43 => <crate::palw_provider_court_v1::ProviderLeaseRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        44 => <crate::palw_provider_court_v1::ProviderChallengeRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        45 => <crate::palw_provider_court_v1::ProviderSubjectRowV1 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        46 if k.len() == 64 => {
            <crate::palw_model_artifact_v2::ModelArtifactBindingRowV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        47 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        48 if k.len() == 64 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        49 if k.len() == 64 => {
            <crate::palw_model_conformance_v2::ModelConformanceRowV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        50 if k.len() == 0 => <(u64, u32) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        51 if k.len() == 64 => <crate::palw_model_vector_v2::ModelVectorRowV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        52 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        53 if k.len() == 64 => <crate::palw_model_vector_da_v2::ModelVectorDemandV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        54 if k.len() == 69 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        55 => <crate::palw_model_vector_da_v2::ModelVectorServedV2 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        56 if k.len() == 64 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        57 if k.len() == 64 => {
            <crate::palw_flat_model_vector_v3::FlatModelVectorRowV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        58 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        59 if k.len() == 64 => {
            <crate::palw_flat_model_vector_da_v3::FlatVectorDemandV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        60 if k.len() == 69 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        61 => <crate::palw_flat_model_vector_da_v3::FlatVectorServedRowV3 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        62 if k.len() == 64 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        63 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        64 if k.len() == 69 => {
            <crate::palw_model_refutation_seal_v4::ModelRefutationSealRowV4 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        65 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        66 if k.len() == 128 => <crate::palw_flat_model_seal_v6::FlatNativeSealRowV6 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        67 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        68 if k.len() == 0 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        69 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        70 if k.len() == 128 => {
            <crate::palw_flat_model_vector_v7::FlatNativeSealRowV7 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        71 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        72 if k.len() == 0 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        78 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        79 if k.len() == 128 => {
            <crate::palw_flat_model_vector_v9::FlatNativeSealRowV9 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        80 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        81 if k.len() == 0 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        73 if k.len() == 64 => {
            <crate::palw_flat_model_vector_da_v7::FlatVectorDemandV7 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        74 if k.len() == 69 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        75 => <crate::palw_flat_model_vector_da_v7::FlatVectorServedRowV7 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        76 if k.len() == 64 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        77 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        82 if k.len() == 64 => {
            <crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        83 if k.len() == 69 => <crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        84 => <crate::palw_flat_model_vector_da_v9::FlatVectorServedRowV9 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        85 if k.len() == 64 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        86 if k.len() == 64 => <bool as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        87 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionClassRowV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        87 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <(
            crate::palw_native_text_stop_v29::NativeTextClassV29,
            crate::palw_state_v2::PalwBondKeyV2,
            u32,
            u64,
            misaka_palw_kernel::flat_execution11::NativeSourceBindingV11,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        87 if k.len() == 66 && k[0] == 29 && k[1] == 1 => raw_aux_row_v1(frame, (1 << 20) as u64)?,
        87 if k.len() == 66 && k[0] == 37 && k[1] == 0 => <(
            crate::palw_window_class_v37::NativeWindowClassV37,
            crate::palw_state_v2::PalwBondKeyV2,
            u64,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        87 if k.len() == 66 && k[0] == 37 && k[1] == 1 => {
            raw_aux_row_v1(frame, (crate::palw_window_class_v37::WINDOW_CLASS_MAX_BODY_BYTES_V37) as u64)?
        }
        87 if k.len() == 70 && k[0] == 37 && k[1] == 2 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        88 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionJobRowV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        88 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <(
            ([u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64]),
            (crate::palw_state_v2::PalwBondKeyV2, crate::palw_state_v2::PalwBondKeyV2, crate::Hash64, crate::Hash64, u32, u32),
            (
                crate::palw_native_text_stop_v29::NativeTextTerminalV29,
                u64,
                misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11,
                [u8; 64],
            ),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        88 if k.len() == 66 && k[0] == 29 && k[1] == 1 => {
            <crate::palw_native_text_stop_v29::NativeTextBookV29 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        88 if k.len() == 66 && k[0] == 37 && k[1] == 3 => {
            <crate::palw_window_class_v37::NativeWindowJobBindingV37 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        89 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeRequesterNonceRowV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        89 if k.len() == 65 && k[0] == 13 => {
            <crate::palw_flat_execution_v11::NativeRequesterNonceRowV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        89 if k.len() == 65 && k[0] == 29 => <u64 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        90 if k.len() == 64 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        91 if k.len() == 64 => raw_aux_row_v1(frame, (crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1) as u64)?,
        91 if k.len() == 66 && k[0] == 29 && k[1] == 0 => {
            raw_aux_row_v1(frame, (crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1) as u64)?
        }
        92 if k.len() == 1 && k[0] == 0 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::QuotaGlobalRowV11) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 65 && k[0] == 1 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::QuotaActorRowV11) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 65 && k[0] == 2 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 65 && k[0] == 3 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::ProtectedSealSummaryV11) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 65 && k[0] == 4 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::discovery13::DiscoveryRowV13) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 65 && k[0] == 16 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::source16::SourceObligationV16) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 66 && k[0] == 16 && k[65] == 1 => {
            <misaka_palw_kernel::flat_quota11::source16::SourcePlanRecordV16 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 17 => {
            <([u8; 64], misaka_palw_kernel::flat_quota11::trace17::TraceSupplementV17) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 2 && k[0] == 22 && k[1] == 0 => {
            <([u8; 64], crate::palw_flat_execution_capacity_v22::NativeCarriageGlobalV22) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 65 && k[0] == 22 => {
            <([u8; 64], crate::palw_flat_execution_capacity_v22::NativeCarriageLeaseV22) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 66 && k[0] == 22 && k[65] == 1 => {
            <crate::palw_flat_execution_capacity_v22::NativeCarriagePlanV22 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 22 && k[65] == 2 => <(
            [u8; 64],
            crate::palw_flat_execution_capacity_v22::NativePhysicalSlotsV22,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        92 if k.len() == 2 && k[0] == 30 && k[1] == 0 => {
            <([u8; 64], crate::palw_native_queue_v30::NativeQueueGlobalV30) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 30 && k[65] == 1 => {
            <([u8; 64], crate::palw_native_queue_v30::NativeQueueLeaseV30) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 30 && k[65] == 2 => {
            <crate::palw_native_queue_v30::NativeQueuePlanRecordV30 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 129 && k[0] == 26 => <(
            [u8; 64],
            misaka_palw_kernel::flat_quota11::stage14::source26::StageSourceCallerRowV26,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        92 if k.len() == 66 && k[0] == 26 && k[65] == 0 => {
            <misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26 as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        92 if k.len() == 66 && k[0] == 28 && k[65] == 29 => {
            <([u8; 64], crate::palw_native_reply_parts_v29::NativeReplyProgressV29) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 29 && k[65] == 0 => {
            <([u8; 64], crate::palw_native_paged_proof_v29::NativePagedJobStatusV29) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 29 && k[65] == 1 => {
            <crate::palw_native_paged_proof_v29::NativePagedProofPlanRecordV29 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 29 && k[65] == 3 => {
            <misaka_palw_kernel::flat_quota11::QuotaPolicyV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 29 && k[65] == 22 => <(
            [u8; 64],
            crate::palw_native_paged_proof_v29::NativePagedLeaseJournalV29,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        92 if k.len() == 66 && k[0] == 29 && k[65] == 23 => any_aux_row_v1(
            frame,
            &[
                try_exact_aux_row_v1::<([u8; 64], crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29)>,
                try_exact_aux_row_v1::<crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29>,
            ],
        )?,
        92 if k.len() == 129 && k[0] == 29 => {
            <([u8; 64], crate::palw_native_paged_proof_v29::NativePagedSealV29) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 2 && k[0] == 31 && k[1] == 0 => {
            <(u16, u64, Vec<([u8; 64], [u8; 64])>) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 66 && k[0] == 31 && k[65] == 1 => {
            <([u8; 64], crate::palw_native_initial_v31::NativeInitialHeaderV31) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        92 if k.len() == 68 && k[0] == 31 && k[65] == 2 => {
            raw_aux_row_v1(frame, (crate::palw_native_initial_v31::INITIAL_PART_BYTES_V31) as u64)?
        }
        93 if k.len() == 0 => {
            <crate::palw_flat_execution_v11::NativeExecutionPoliciesV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        93 if k.len() == 65 && k[0] == 19 => {
            <crate::palw_flat_execution_pursuit_v19::NativePublicPursuitV19 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        93 if k.len() == 65 && k[0] == 23 => {
            <crate::palw_flat_execution_capacity_v22::NativeExecutionConsentV23 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        93 if k.len() == 65 && k[0] == 27 => <(
            [u8; 64],
            (
                (u16, [u8; 64], [u8; 64], [u8; 64], crate::palw_native_final_v27::NativeFinalClassBasisV27, [u8; 64]),
                ([u8; 64], [u8; 64]),
            ),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        93 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <(
            crate::palw_native_text_stop_v29::NativeTextPrepareV29,
            crate::palw_native_text_stop_v29::NativeTextMaximumV29,
            crate::palw_native_final_v27::NativeFinalClassBasisV27,
            crate::palw_state_v2::PalwBondKeyV2,
            u64,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        94 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeVerifiedSourceFactV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        94 if k.len() == 65 && k[0] == 16 => {
            <crate::palw_flat_execution_v11::NativeVerifiedHeaderFactV16 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        94 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <([u8; 64], [u8; 64], u64) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        95 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        95 if k.len() == 70 && k[0] == 29 && k[1] == 0 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        96 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        96 if k.len() == 69 && k[0] == 1 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        96 if k.len() == 70 && k[0] == 29 && k[1] == 0 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        96 if k.len() == 70 && k[0] == 29 && k[1] == 1 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        97 if k.len() == 0 => <u32 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionPreparationV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        98 if k.len() == 68 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 69 && k[0] == 13 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 65 && k[0] == 13 => {
            <([u8; 64], crate::palw_flat_execution_v11::NativeExecutionConsentV13) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        98 if k.len() == 65 && k[0] == 18 => {
            <([u8; 64], crate::palw_flat_execution_v11::NativeExecutionConsentV18) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        98 if k.len() == 65 && k[0] == 23 => {
            <([u8; 64], crate::palw_flat_execution_capacity_v22::NativeExecutionConsentV23) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        98 if k.len() == 65 && k[0] == 27 => <(
            [u8; 64],
            (
                (u16, [u8; 64], [u8; 64], [u8; 64], crate::palw_native_final_v27::NativeFinalClassBasisV27, [u8; 64]),
                ([u8; 64], [u8; 64]),
            ),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 70 && k[0] == 29 && k[1] == 0 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 66 && k[0] == 29 && k[1] == 1 => <(
            crate::palw_native_text_stop_v29::NativeTextPrepareV29,
            crate::palw_native_text_stop_v29::NativeTextMaximumV29,
            crate::palw_native_final_v27::NativeFinalClassBasisV27,
            crate::palw_state_v2::PalwBondKeyV2,
            u64,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 70 && k[0] == 29 && k[1] == 2 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        98 if k.len() == 66 && k[0] == 29 && k[1] == 3 => {
            <crate::palw_native_text_stop_v29::NativeTextConsentV29 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        99 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionSealV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        99 if k.len() == 65 && k[0] == 12 => {
            <crate::palw_flat_execution_v11::NativeExecutionProofGroupV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        99 if k.len() == 65 && k[0] == 13 => {
            <crate::palw_flat_execution_v11::NativeExecutionDiscoveryV13 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        99 if k.len() == 66 && k[0] == 15 => {
            raw_aux_row_v1(frame, (misaka_palw_kernel::flat_quota11::PROTECTED_FRAGMENT_BYTES_V11) as u64)?
        }
        99 if k.len() == 68 && k[0] == 28 => raw_aux_row_v1(frame, (32 << 10) as u64)?,
        99 if k.len() == 131 && k[0] == 29 => {
            raw_aux_row_v1(frame, (crate::palw_native_text_stop_v29::TEXT_STOP_PART_BYTES_V29) as u64)?
        }
        100 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        102 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionDemandV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        102 if k.len() == 65 && k[0] == 16 => {
            <crate::palw_flat_execution_v11::NativeSourceDemandV16 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        102 if k.len() == 65 && k[0] == 22 => <u8 as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        102 if k.len() == 65 && k[0] == 28 => {
            <crate::palw_flat_stage_v28::NativeStagePendingV28 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        102 if k.len() == 66 && k[0] == 29 && k[1] == 0 => <(
            (
                u16,
                [u8; 64],
                crate::palw_native_trace_da_v29::NativePublicRequestV29,
                crate::palw_state_v2::PalwBondKeyV2,
                [u8; 64],
                crate::palw_state_v2::PalwBondKeyV2,
            ),
            ([u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64], u64),
            (u64, u64, u64, Option<crate::palw_flat_model_vector_da_v9::FlatVectorDemandV9>, [u8; 64], u8),
            (bool, u32),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        102 if k.len() == 66 && k[0] == 29 && k[1] == 1 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        102 if k.len() == 70 && k[0] == 29 && k[1] == 2 => <([u8; 64], [u8; 64]) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        102 if k.len() == 66 && k[0] == 29 && k[1] == 3 => {
            <([u8; 64], crate::palw_native_reply_parts_v29::NativeReplyProgressV29) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        103 if k.len() == 69 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        103 if k.len() == 70 && k[0] == 16 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        103 if k.len() == 70 && k[0] == 28 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        103 if k.len() == 71 && k[0] == 29 && k[1] == 0 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        104 if (k.len() == 73 || k.len() == 81) => {
            <crate::palw_flat_model_vector_da_v9::FlatVectorServedRowV9 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        104 if k.len() == 65 && k[0] == 16 => {
            raw_aux_row_v1(frame, (crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1) as u64)?
        }
        104 if k.len() == 65 && k[0] == 28 => raw_aux_row_v1(frame, (128 << 10) as u64)?,
        104 if k.len() == 66 && k[0] == 29 && k[1] == 0 => raw_aux_row_v1(frame, (128 << 10) as u64)?,
        104 if k.len() == 69 && k[0] == 29 && k[1] == 3 => raw_aux_row_v1(frame, (32 << 10) as u64)?,
        105 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionPursuitV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        105 if k.len() == 65 && k[0] == 13 => {
            <crate::palw_flat_execution_v11::NativeExecutionDemandCountersV13 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        105 if k.len() == 69 && k[0] == 14 && n == 64 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        105 if k.len() == 69 && k[0] == 14 && n == 65 => <(u8, [u8; 64]) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        105 if k.len() == 66 && k[0] == 29 && k[1] == 0 => {
            <(u32, u32, u32, u32, u32, u64) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        106 if k.len() == 64 => {
            <crate::palw_flat_execution_v11::NativeExecutionCreditV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        106 if k.len() == 65 && k[0] == 19 => {
            <crate::palw_flat_execution_v11::NativeExecutionFeesV19 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        106 if k.len() == 65 && k[0] == 24 => <(
            (u16, [u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64]),
            ([u8; 64], [u8; 64], [u8; 64], [u8; 64], u64),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        106 if k.len() == 129 && k[0] == 24 => <([u8; 64], [u8; 64], u64) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        106 if k.len() == 66 && k[0] == 29 && k[1] == 0 => {
            <(crate::palw_native_text_stop_v29::NativeTextSettlementV29, crate::Hash64, u64) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        106 if k.len() == 66 && k[0] == 29 && k[1] == 3 => {
            <([u8; 64], [u8; 64], [u8; 64]) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        106 if k.len() == 66 && k[0] == 29 && k[1] == 4 => any_aux_row_v1(
            frame,
            &[
                try_exact_aux_row_v1::<([u8; 64], [u8; 64], [u8; 64], [u8; 64], u64)>,
                try_exact_aux_row_v1::<([u8; 64], [u8; 64], [u8; 64], u64)>,
            ],
        )?,
        112 if k.len() == 65 && k[0] == 0 => {
            <crate::palw_flat_stage_v14::NativeStageSourceRowV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 2 => {
            <crate::palw_flat_stage_v14::NativeStageClassRowV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 5 => {
            <crate::palw_flat_stage_v14::NativeStageJobRowV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 9 => {
            <crate::palw_flat_stage_v14::NativeStageAcceptanceV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 10 => {
            <crate::palw_flat_stage_v14::NativeStageAcceptanceV18 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 11 => {
            <crate::palw_flat_stage_v14::NativeStagePrincipalRefundV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 12 => {
            <crate::palw_flat_stage_v14::NativeStageFeesV20 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 13 => {
            <misaka_palw_kernel::flat_quota11::stage14::StageSettlementV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 14 => {
            <crate::palw_flat_stage_v14::NativeStageNegativeSourceFactV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 16 => {
            <crate::palw_flat_stage_v14::NativeStagePursuitV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 17 => {
            <crate::palw_flat_stage_v14::NativeStageCollectedV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 18 => {
            <((u16, [u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64]), ([u8; 64], [u8; 64])) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        112 if k.len() == 65 && k[0] == 19 => {
            <crate::palw_flat_stage_v14::NativeStageSealV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 65 && k[0] == 20 => {
            <crate::palw_flat_execution_v11::NativeExecutionProofGroupV11 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 23 => {
            raw_aux_row_v1(frame, (misaka_palw_kernel::flat_quota11::PROTECTED_FRAGMENT_BYTES_V11) as u64)?
        }
        112 if k.len() == 69 && k[0] == 1 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 69 && k[0] == 3 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 69 && k[0] == 6 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 69 && k[0] == 7 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 69 && k[0] == 8 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 1 && k[0] == 4 => {
            <crate::palw_flat_stage_v14::NativeStagePoliciesV14 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 0 => {
            <(u16, [u8; 64], u32, crate::palw_state_v2::PalwBondKeyV2, u64, Vec<[u8; 64]>) as NativeWireScanV1>::scan_native_wire_v1(
                frame,
            )?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 1 => <Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 2 => <(
            (u16, [u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64]),
            ([u8; 64], u32, crate::palw_state_v2::PalwBondKeyV2),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 3 => <Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 5 => <(
            (u16, [u8; 64], [u8; 64], [u8; 64], [u8; 64], [u8; 64]),
            (
                crate::palw_flat_stage_v28::NativeStageMaximumV28,
                crate::palw_state_v2::PalwBondKeyV2,
                crate::palw_state_v2::PalwBondKeyV2,
                crate::Hash64,
                crate::Hash64,
                u32,
            ),
            (u32, u32, u32, misaka_palw_kernel::flat_quota11::QuotaPlanRecordV11, u32, u64),
            (misaka_palw_kernel::flat_quota11::stage14::bounded28::StageTerminalV28, u64),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 6 => {
            <(Vec<u8>, Vec<Vec<u8>>, Vec<u8>) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 70 && k[0] == 28 && k[1] == 7 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 70 && k[0] == 28 && k[1] == 8 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 70 && k[0] == 28 && k[1] == 9 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 10 => {
            <crate::palw_flat_stage_v28::NativeStageConsentV28 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 11 => {
            <([u8; 64], [u8; 64], [u8; 64]) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 70 && k[0] == 28 && k[1] == 12 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 70 && k[0] == 28 && k[1] == 13 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 14 => <(
            misaka_palw_kernel::flat_quota11::stage14::bounded28::StageSettlementV28,
            crate::Hash64,
            crate::Hash64,
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 15 => {
            <([u8; 64], [u8; 64], [u8; 64], [u8; 64]) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 16 => {
            <((u16, [u8; 64], [u8; 64], u32, u32, u32), (u32, u64, [[u8; 64]; 11])) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 18 => {
            <(u16, [u8; 64], [u8; 64], u32, u32) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 28 && k[1] == 19 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 28 && k[1] == 20 => <(
            (
                [u8; 64],
                [u8; 64],
                [u8; 64],
                [u8; 64],
                Option<[u8; 64]>,
                misaka_palw_kernel::flat_quota11::stage14::bounded28::StageSettlementV28,
            ),
            [u8; 64],
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 70 && k[0] == 28 && k[1] == 17 => {
            <([u8; 64], [u8; 64], crate::palw_state_v2::PalwBondKeyV2, u8) as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 2 && k[0] == 28 && k[1] == 4 => {
            <crate::palw_flat_stage_v28::NativeStagePoliciesV28 as NativeWireScanV1>::scan_native_wire_v1(frame)?
        }
        112 if k.len() == 66 && k[0] == 32 && k[1] == 0 => <(
            (u16, [u8; 64], crate::palw_state_v2::PalwBondKeyV2, [u8; 64], [u8; 64], [u8; 64]),
            ([u8; 64], [u8; 64], u64, u32, Vec<[u8; 64]>, Vec<[u8; 64]>),
            (Vec<[u8; 64]>, u16, Option<[u8; 64]>),
        ) as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 66 && k[0] == 32 && k[1] == 1 => <Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        112 if k.len() == 70 && k[0] == 32 && k[1] == 2 => <Vec<[u8; 64]> as NativeWireScanV1>::scan_native_wire_v1(frame)?,
        42 | 101 => return Err(wire::wire_bad("reserved aux table holds no rows")),
        46 => return Err(wire::wire_bad("unallocated aux table 46 row namespace")),
        47 => return Err(wire::wire_bad("unallocated aux table 47 row namespace")),
        48 => return Err(wire::wire_bad("unallocated aux table 48 row namespace")),
        49 => return Err(wire::wire_bad("unallocated aux table 49 row namespace")),
        50 => return Err(wire::wire_bad("unallocated aux table 50 row namespace")),
        51 => return Err(wire::wire_bad("unallocated aux table 51 row namespace")),
        52 => return Err(wire::wire_bad("unallocated aux table 52 row namespace")),
        53 => return Err(wire::wire_bad("unallocated aux table 53 row namespace")),
        54 => return Err(wire::wire_bad("unallocated aux table 54 row namespace")),
        56 => return Err(wire::wire_bad("unallocated aux table 56 row namespace")),
        57 => return Err(wire::wire_bad("unallocated aux table 57 row namespace")),
        58 => return Err(wire::wire_bad("unallocated aux table 58 row namespace")),
        59 => return Err(wire::wire_bad("unallocated aux table 59 row namespace")),
        60 => return Err(wire::wire_bad("unallocated aux table 60 row namespace")),
        62 => return Err(wire::wire_bad("unallocated aux table 62 row namespace")),
        63 => return Err(wire::wire_bad("unallocated aux table 63 row namespace")),
        64 => return Err(wire::wire_bad("unallocated aux table 64 row namespace")),
        65 => return Err(wire::wire_bad("unallocated aux table 65 row namespace")),
        66 => return Err(wire::wire_bad("unallocated aux table 66 row namespace")),
        67 => return Err(wire::wire_bad("unallocated aux table 67 row namespace")),
        68 => return Err(wire::wire_bad("unallocated aux table 68 row namespace")),
        69 => return Err(wire::wire_bad("unallocated aux table 69 row namespace")),
        70 => return Err(wire::wire_bad("unallocated aux table 70 row namespace")),
        71 => return Err(wire::wire_bad("unallocated aux table 71 row namespace")),
        72 => return Err(wire::wire_bad("unallocated aux table 72 row namespace")),
        73 => return Err(wire::wire_bad("unallocated aux table 73 row namespace")),
        74 => return Err(wire::wire_bad("unallocated aux table 74 row namespace")),
        76 => return Err(wire::wire_bad("unallocated aux table 76 row namespace")),
        77 => return Err(wire::wire_bad("unallocated aux table 77 row namespace")),
        78 => return Err(wire::wire_bad("unallocated aux table 78 row namespace")),
        79 => return Err(wire::wire_bad("unallocated aux table 79 row namespace")),
        80 => return Err(wire::wire_bad("unallocated aux table 80 row namespace")),
        81 => return Err(wire::wire_bad("unallocated aux table 81 row namespace")),
        82 => return Err(wire::wire_bad("unallocated aux table 82 row namespace")),
        83 => return Err(wire::wire_bad("unallocated aux table 83 row namespace")),
        85 => return Err(wire::wire_bad("unallocated aux table 85 row namespace")),
        86 => return Err(wire::wire_bad("unallocated aux table 86 row namespace")),
        87 => return Err(wire::wire_bad("unallocated aux table 87 row namespace")),
        88 => return Err(wire::wire_bad("unallocated aux table 88 row namespace")),
        89 => return Err(wire::wire_bad("unallocated aux table 89 row namespace")),
        90 => return Err(wire::wire_bad("unallocated aux table 90 row namespace")),
        91 => return Err(wire::wire_bad("unallocated aux table 91 row namespace")),
        92 => return Err(wire::wire_bad("unallocated aux table 92 row namespace")),
        93 => return Err(wire::wire_bad("unallocated aux table 93 row namespace")),
        94 => return Err(wire::wire_bad("unallocated aux table 94 row namespace")),
        95 => return Err(wire::wire_bad("unallocated aux table 95 row namespace")),
        96 => return Err(wire::wire_bad("unallocated aux table 96 row namespace")),
        97 => return Err(wire::wire_bad("unallocated aux table 97 row namespace")),
        98 => return Err(wire::wire_bad("unallocated aux table 98 row namespace")),
        99 => return Err(wire::wire_bad("unallocated aux table 99 row namespace")),
        100 => return Err(wire::wire_bad("unallocated aux table 100 row namespace")),
        102 => return Err(wire::wire_bad("unallocated aux table 102 row namespace")),
        103 => return Err(wire::wire_bad("unallocated aux table 103 row namespace")),
        104 => return Err(wire::wire_bad("unallocated aux table 104 row namespace")),
        105 => return Err(wire::wire_bad("unallocated aux table 105 row namespace")),
        106 => return Err(wire::wire_bad("unallocated aux table 106 row namespace")),
        112 => return Err(wire::wire_bad("unallocated aux table 112 row namespace")),
        115 | 123 | 125 | 126 | 127 | 130 | 131 => return Err(wire::wire_bad("unallocated aux row namespace")),
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                NativeCoverageV1::unsupported::<crate::palw_kernel_route_v1::PalwKernelRouteStateV1>(
                    &format!("native aux table {table}"),
                    "nested row codec is not in the explicit supported shape inventory",
                ),
            ));
        }
    })
}

/// The longest aux key any fold writes is far below this; a longer key is malformed, never truncated.
const NATIVE_AUX_KEY_MAX_BYTES_V1: u64 = 4096;
/// A raw byte row (stored verbatim, not Borsh-framed, never decoded by restore or ledger projection): its declared bound, then
/// one transient copy of its bytes (the outer loop already holds the stored row and its copy).
fn raw_aux_row_v1<R: Read>(frame: &mut NativeWireReaderV1<R>, cap: u64) -> std::io::Result<NativeWireShapeV1> {
    let n = frame.remaining();
    if n > cap {
        return Err(wire::wire_bad("raw aux row exceeds its declared bound"));
    }
    frame.step()?;
    frame.skip(n)?;
    NativeWireShapeV1 { byte_entries: n, ..Default::default() }.allocation(n, 1)
}
/// One exact layout of an ambiguous key shape over a buffered row: accepted only if it consumes every byte.
fn try_exact_aux_row_v1<T: NativeWireScanV1>(row: &[u8]) -> Option<NativeWireShapeV1> {
    let mut reader = NativeWireReaderV1::new(row, row.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader).ok()?;
    (reader.remaining() == 0).then_some(shape)
}
/// A key shape two families write with different layouts: buffer the bounded row, try every layout, account the LARGEST
/// accepted shape plus the buffer (never under-count); no layout accepting fails closed.
fn any_aux_row_v1<R: Read>(
    frame: &mut NativeWireReaderV1<R>,
    layouts: &[fn(&[u8]) -> Option<NativeWireShapeV1>],
) -> std::io::Result<NativeWireShapeV1> {
    let n = frame.remaining();
    if n > 1 << 20 {
        return Err(wire::wire_bad("ambiguous aux row exceeds its buffered bound"));
    }
    let mut row = vec![0u8; n as usize];
    frame.step()?;
    frame.read_exact(&mut row)?;
    let mut best: Option<NativeWireShapeV1> = None;
    for layout in layouts {
        if let Some(shape) = layout(&row) {
            best = Some(match best {
                Some(b) if b.heap >= shape.heap => b,
                _ => shape,
            });
        }
    }
    best.ok_or_else(|| wire::wire_bad("no audited layout accepts this ambiguous aux row"))?.allocation(n, 1)
}
