//! Owner hints from actual executed journal effects. These are not a birth,
//! source, approval, or credit authority. A source owner still resolves the
//! exact cohort and validates its complete typed catalogue before publication.
use super::*;
use crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1;
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalWorkRefV1 as Work};
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AcceptedControlV2 {
    pub ordinal: usize,
    pub object_id: Hash64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExecutedOwnerHintV2 {
    pub work: Work,
    /// Only explicit existing birth-qualified native rows may supply this.
    /// A raw claim ID or current claim's carrying block is not promoted to it.
    pub explicit_birth: Option<PalwBlockContextV2>,
    pub journal: usize,
    pub control: Option<AcceptedControlV2>,
}
fn bad(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}
fn typed<T: NativeWireScanV1 + borsh::BorshDeserialize>(bytes: &[u8]) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("executed owner row codec"),
    })
}
/// Resolve a companion row at this actual effect, not at an unrelated later
/// raw-ID reuse. Deletion may read the old row that the executing journal owns.
fn row_at<'a>(b: &'a TransitionBuilder<'_>, ordinal: usize, table: u8, key: &[u8]) -> Result<Option<&'a [u8]>,PalwStateV2Error> {
    for entry in b.entries[..=ordinal].iter().rev() {
        if let PalwDeltaEntryV2::KernelRouteRow { table: t, key: k, old, new } = entry {
            if *t == table && k.as_slice() == key {
                return Ok(new.as_deref().or(old.as_deref()));
            }
        }
    }
    // Undo later writes logically by borrowing their OLD bytes. No state/map
    // clone is constructed solely to resolve one historical coordinate.
    for entry in b.entries[ordinal + 1..].iter() {
        if let PalwDeltaEntryV2::KernelRouteRow { table: t, key: k, old, .. } = entry {
            if *t == table && k.as_slice() == key {
                return Ok(old.as_deref());
            }
        }
    }
    let Some(route)=b.state.kernel_route.as_ref()else{return Ok(None)};
    let _key=reserve_native_requests_v1(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    Ok(route.aux.get(&(table,key.to_vec())).map(Vec::as_slice))
}
fn base_for_enrollment(b: &TransitionBuilder<'_>, ordinal: usize, enrollment: [u8; 64]) -> Result<Hash64, PalwStateV2Error> {
    use crate::palw_claim_challenge_v2 as paid;
    let mut key = [0u8; 65];
    key[0] = paid::PAID_CLAIM_ID_INDEX_PREFIX_V3;
    key[1..].copy_from_slice(&enrollment);
    let index = typed::<paid::PalwPaidClaimIndexV3>(
        row_at(b, ordinal, paid::PAID_CLAIM_ENROLLMENTS_V2, &key)?
            .ok_or_else(|| bad("executed receipt lost actual enrollment index"))?,
    )?;
    if index.version != 3 || index.kind != paid::PalwPaidClaimIndexKindV3::Enrollment || index.owner != enrollment {
        return Err(bad("executed enrollment index identity"));
    }
    let authority = typed::<paid::PalwPaidClaimAuthorityV2>(
        row_at(b, ordinal, paid::PAID_CLAIM_ENROLLMENTS_V2, &index.target)?
            .ok_or_else(|| bad("executed receipt lost actual source authority"))?,
    )?;
    if authority.context.id() != index.target || authority.enrollment.id() != enrollment {
        return Err(bad("executed enrollment substituted its source authority"));
    }
    Ok(Hash64::from_bytes(authority.enrollment.base_claim))
}
fn aux_owner(
    b: &TransitionBuilder<'_>,
    ordinal: usize,
    table: u8,
    key: &[u8],
    raw: &[u8],
) -> Result<Option<(Work, Option<PalwBlockContextV2>)>, PalwStateV2Error> {
    use crate::palw_claim_challenge_v2 as paid;
    use crate::palw_security_certification_v1 as cert;
    let kernel = |id: [u8; 64]| Some((Work::KernelClaim { claim: Hash64::from_bytes(id) }, None));
    if table == paid::PAID_CLAIM_ENROLLMENTS_V2 {
        if key.len() == 64 {
            let a = typed::<paid::PalwPaidClaimAuthorityV2>(raw)?;
            if a.context.id().as_slice() != key {
                return Err(bad("executed paid source context key"));
            }
            return Ok(kernel(a.enrollment.base_claim));
        }
        if key.len() == 65 && matches!(key[0], paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3 | paid::PAID_CLAIM_ID_INDEX_PREFIX_V3) {
            let index = typed::<paid::PalwPaidClaimIndexV3>(raw)?;
            if index.version != 3 || key[1..] != index.owner {
                return Err(bad("executed paid source index key"));
            }
            if key[0] == paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3 {
                if index.kind != paid::PalwPaidClaimIndexKindV3::BaseClaim {
                    return Err(bad("executed paid base index kind"));
                }
                return Ok(kernel(index.owner));
            }
            if index.kind != paid::PalwPaidClaimIndexKindV3::Enrollment {
                return Err(bad("executed paid enrollment index kind"));
            }
            return Ok(Some((Work::KernelClaim { claim: base_for_enrollment(b, ordinal, index.owner)? }, None)));
        }
    }
    if table == paid::PAID_CLAIM_FINALS_V2 {
        let final_row = typed::<paid::PalwPaidClaimFinalV2>(raw)?;
        if key != final_row.enrollment {
            return Err(bad("executed paid Final enrollment key"));
        }
        return Ok(kernel(final_row.base_claim));
    }
    if table == paid::PAID_CLAIM_RECEIPTS_V2 {
        let receipt = typed::<misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2>(raw)?;
        let id = misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(receipt.enrollment, receipt.signer));
        if key != id {
            return Err(bad("executed paid receipt key"));
        }
        return Ok(Some((Work::KernelClaim { claim: base_for_enrollment(b, ordinal, receipt.enrollment)? }, None)));
    }
    if table == paid::PAID_CLAIM_ROUNDS_V2 {
        let round = typed::<crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3>(raw)?;
        if key != round.id() {
            return Err(bad("executed interactive round key"));
        }
        return Ok(Some((Work::KernelClaim { claim: base_for_enrollment(b, ordinal, round.enrollment)? }, None)));
    }
    if table == paid::PAID_CLAIM_LOCKS_V2 {
        let lock = typed::<paid::PalwPaidClaimLockV2>(raw)?;
        if key != lock.enrollment {
            return Err(bad("executed paid lock key"));
        }
        return Ok(Some((Work::KernelClaim { claim: base_for_enrollment(b, ordinal, lock.enrollment)? }, None)));
    }
    if table == cert::PALW_SECURITY_CERTIFICATION_TABLE_V1
        && key.len() == 65
        && matches!(
            key[0],
            cert::CERTIFICATION_CLAIM_PREFIX_V1
                | cert::CERTIFICATION_NATIVE_MINING_PREFIX_V39
                | cert::CERTIFICATION_KERNEL_CLAIM_PREFIX_V1
                | cert::CERTIFICATION_KERNEL_HOLD_PREFIX_V1
        )
    {
        let claim = if key[0] == cert::CERTIFICATION_KERNEL_HOLD_PREFIX_V1 {
            typed::<cert::PalwKernelCertifiedRewardHoldV1>(raw)?.claim
        } else {
            typed::<cert::PalwClaimCertificationRowV1>(raw)?.claim
        };
        let id = Hash64::from_bytes(claim);
        let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        let (expected, work) = match key[0] {
            cert::CERTIFICATION_CLAIM_PREFIX_V1 => {
                (cert::certification_claim_key_v1(&cert::palw_certification_id_v1(&id)), Work::ChainClaim { claim: id })
            }
            cert::CERTIFICATION_NATIVE_MINING_PREFIX_V39 => (
                cert::certification_native_key_v39(&cert::palw_native_certification_id_v39(&id)),
                Work::NativeMiningClaim39 { claim: id },
            ),
            cert::CERTIFICATION_KERNEL_CLAIM_PREFIX_V1 => (
                cert::certification_kernel_claim_key_v1(&cert::palw_kernel_claim_certification_id_v1(&claim)),
                Work::KernelClaim { claim: id },
            ),
            _ => (
                cert::certification_kernel_hold_key_v1(&cert::palw_kernel_claim_certification_id_v1(&claim)),
                Work::KernelClaim { claim: id },
            ),
        };
        if key != expected {
            return Err(bad("executed selected-approval subject key"));
        }
        return Ok(Some((work, None)));
    }
    if table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'E' {
        let export = typed::<PalwPrefixExportReservationV1>(raw)?;
        if key[1..] != export.budget_id.as_bytes() {
            return Err(bad("executed export reservation key"));
        }
        return Ok(Some((Work::ChainClaim { claim: export.source_claim }, Some(export.point))));
    }
    Ok(None)
}
/// Borrow only accepted-object boundaries; metadata is private and written by
/// the dispatcher. System-only effects deliberately carry `control=None`.
/// Every hint still needs exact-cohort/source-policy validation before archive
/// minting, and unsupported effects do not become an empty-ready source.
pub(super) fn executed_owner_hints_v2(
    b: &TransitionBuilder<'_>,
    objects: &[PalwConsensusObjectV2],
) -> Result<NativeRetainedV1<Vec<ExecutedOwnerHintV2>>, PalwStateV2Error> {
    if b.claim_local_object_spans.len() != objects.len() {
        return Err(bad("owner source accepting-list observations absent"));
    }
    let mut previous = 0;
    for (ordinal, (span, object)) in b.claim_local_object_spans.iter().zip(objects).enumerate() {
        let id = crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/executed-object/v2", object)
            .map_err(PalwStateV2Error::CarriageInconsistent)?;
        if span.ordinal != ordinal
            || span.first > span.end
            || span.end > b.entries.len()
            || span.first < previous
            || span.object_id != id
        {
            return Err(bad("owner source accepting-list identity/range"));
        }
        previous = span.end;
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ExecutedOwnerHintV2>(b.entries.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut out = Vec::with_capacity(b.entries.len());
    let mut span_cursor = 0usize;
    for (ordinal, entry) in b.entries.iter().enumerate() {
        let owner = match entry {
            PalwDeltaEntryV2::Claim { key, .. } => Some((Work::ChainClaim { claim: *key }, None)),
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } => {
                if *table == misaka_palw_kernel::rows::TABLE_CLAIMS_V1 {
                    let id: [u8; 64] = key.as_slice().try_into().map_err(|_| bad("executed kernel claim coordinate"))?;
                    Some((Work::KernelClaim { claim: Hash64::from_bytes(id) }, None))
                } else if let Some(raw) = new.as_deref().or(old.as_deref()) {
                    aux_owner(b, ordinal, *table, key, raw)?
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some((work, explicit_birth)) = owner else { continue };
        while b.claim_local_object_spans.get(span_cursor).is_some_and(|s| s.end <= ordinal) {
            span_cursor += 1;
        }
        let control = b
            .claim_local_object_spans
            .get(span_cursor)
            .filter(|s| s.first <= ordinal && ordinal < s.end)
            .map(|s| AcceptedControlV2 { ordinal: s.ordinal, object_id: s.object_id });
        out.push(ExecutedOwnerHintV2 { work, explicit_birth, journal: ordinal, control });
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
