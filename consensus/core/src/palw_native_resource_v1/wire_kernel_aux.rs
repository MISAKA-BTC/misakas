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
            let prefix = if key_bytes != 0 {
                let p = reader.byte()?;
                reader.skip(key_bytes - 1)?;
                p
            } else {
                0
            };
            sum = sum.allocation(key_bytes.checked_mul(2).ok_or_else(|| wire::wire_bad("native aux key allocation overflow"))?, 1)?;
            let row_bytes = reader.word()? as u64;
            if row_bytes > reader.remaining() {
                return Err(wire::wire_bad("native aux row exceeds frame"));
            }
            let mut frame = NativeWireReaderV1::new((&mut *reader).take(row_bytes), row_bytes);
            let shape = scan_native_aux_row_v1(table, key_bytes, prefix, &mut frame)?;
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

pub(crate) fn scan_native_aux_row_v1<R: Read>(
    table: u8,
    key_bytes: u64,
    prefix: u8,
    frame: &mut NativeWireReaderV1<R>,
) -> std::io::Result<NativeWireShapeV1> {
    match table {
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
        121 => match (key_bytes, prefix) {
            (65, 0x41) => crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2::scan_native_wire_v1(frame)?,
            (65, 0x42) => crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2::scan_native_wire_v1(frame)?,
            (64, _) => crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2::scan_native_wire_v1(frame)?,
            _ => return Err(wire::wire_bad("unallocated Panel lifetime row namespace")),
        },
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                NativeCoverageV1::unsupported::<crate::palw_kernel_route_v1::PalwKernelRouteStateV1>(
                    &format!("native aux table {table}"),
                    "nested row codec is not in the explicit supported shape inventory",
                ),
            ));
        }
    }
}
