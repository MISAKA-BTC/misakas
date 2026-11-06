//! Actual namespaced Kernel source birth for contract ownership. No paid
//! enrollment or selected approval is inferred, and no partial world is made.
use super::*;
use crate::palw_claim_local_v1::{PalwClaimLiabilityFrameV1, PalwClaimLocalRowOwnerV1};
use crate::palw_native_resource_v1::{NativeRetainedV1, NativeWireScanV1};
use borsh::BorshDeserialize;
use misaka_palw_kernel::ledger::ClaimRowV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActualKernelSourceOwnerV9 {
    claim: Hash64,
    accepting_point: PalwBlockContextV2,
    catalogue_id: Hash64,
}
impl ActualKernelSourceOwnerV9 {
    pub(super) fn claim(&self) -> Hash64 {
        self.claim
    }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 {
        self.accepting_point
    }
    /// Stable exact-born native subject identity, not paid certification.
    pub(super) fn catalogue_id(&self) -> Hash64 {
        self.catalogue_id
    }
}
fn bad(s: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn unavailable() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable)
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(raw: &[u8]) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw)).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("kernel origin typed row codec"),
    })
}
pub(super) fn identity(row: &ClaimRowV1) -> Result<Hash64, PalwStateV2Error> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/kernel-source-native-identity/v9",
        &(row.producer, row.class_binding_id, row.job_id, &row.body, row.committed_daa),
    )
    .map_err(|e| bad(&e))
}
fn mint(
    claim: Hash64,
    birth: PalwBlockContextV2,
    identity: Hash64,
    _origin: Hash64,
    _ordinal: u64,
) -> Result<ActualKernelSourceOwnerV9, PalwStateV2Error> {
    let catalogue_id =
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/kernel-source-owner/v9", &(2u8, claim, birth, identity))
            .map_err(|e| bad(&e))?;
    Ok(ActualKernelSourceOwnerV9 { claim, accepting_point: birth, catalogue_id })
}
/// Called only while accepting a NEW actual source-bootstrap Open. The owning
/// native handler already authenticates the producer, full source profile,
/// canonical work, economic provenance and native source scope. This function
/// adds birth identity only; it grants no approval, credit, money or service.
pub(super) fn kernel_owner_for_open_v9(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
) -> Result<Option<ActualKernelSourceOwnerV9>, PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) || !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Err(unavailable());
    }
    let _native_key = crate::palw_native_resource_v1::reserve_native_requests_v1(64).map_err(PalwStateV2Error::NativeResource)?;
    let raw = super::palw_claim_local_rewards_v1::kernel_claim_row(&b.state, &claim).ok_or_else(unavailable)?;
    let current = decode::<ClaimRowV1>(raw)?;
    // An old pre-fence source keeps its literal native fee/Open behavior. It
    // is not retroactively assigned an invented accepting block or marked as
    // captured/closed by the new watcher.
    if !b.params.claim_local_settlement_active_at(current.committed_daa) {
        return Ok(None);
    }
    let current_identity = identity(&current)?;
    // Same-block admission is the executing native journal. No prior carrier
    // or guessed block at current.committed_daa substitutes for that context.
    for (ordinal, entry) in b.entries.iter().enumerate().rev() {
        if let PalwDeltaEntryV2::KernelRouteRow { table, key, old: None, new: Some(raw) } = entry
            && *table == misaka_palw_kernel::rows::TABLE_CLAIMS_V1
            && key.as_slice() == claim.as_byte_slice()
        {
            let original = decode::<ClaimRowV1>(raw)?;
            if original.committed_daa == ctx.daa_score && identity(&original)? == current_identity {
                return mint(
                    claim,
                    *ctx,
                    current_identity,
                    crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
                    ordinal as u64,
                )
                .map(Some);
            }
        }
    }
    // Ordinary post-catalogue native sources require no F1/carriage decode.
    if let Some(point) = super::palw_claim_local_kernel_current_birth_v9::current_point_v9(&b.state, claim, &current)? {
        if point.daa_score > ctx.daa_score
            || point.blue_score > ctx.blue_score
            || !b.params.claim_local_settlement_active_at(point.daa_score)
        {
            return Err(bad("kernel scoped current source point is future/unarmed"));
        }
        return mint(claim, point, current_identity, Hash64::default(), 0).map(Some);
    }
    // Authentic F1 compatibility input until all Kernel original consumers have
    // their own source catalogue. Open persists the opaque exact origin mapping;
    // subsequent old-contract settlement must read that mapping, not this raw ID.
    let mut found = None;
    for (_, bytes) in super::palw_claim_local_rewards_v1::frames(&b.state) {
        let frame = decode::<PalwClaimLiabilityFrameV1>(bytes)?;
        if frame.point.daa_score != current.committed_daa {
            continue;
        }
        frame.validate_context_v1().map_err(|e| match e {
            crate::palw_claim_local_v1::PalwClaimCheckpointErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
            other => bad(&other.to_string()),
        })?;
        if frame.point.blue_score > ctx.blue_score
            || frame.point.daa_score > ctx.daa_score
            || !b.params.claim_local_settlement_active_at(frame.point.daa_score)
        {
            return Err(bad("kernel origin future/unarmed source point"));
        }
        for (ordinal, event) in frame.native_changes.iter().enumerate() {
            if event.owner != PalwClaimLocalRowOwnerV1::KernelRoute
                || event.table != misaka_palw_kernel::rows::TABLE_CLAIMS_V1
                || event.key.as_slice() != claim.as_byte_slice()
                || event.old.is_some()
            {
                continue;
            }
            let Some(raw) = &event.new else { continue };
            let original = decode::<ClaimRowV1>(raw)?;
            if original.committed_daa != frame.point.daa_score || identity(&original)? != current_identity {
                continue;
            }
            let token = mint(
                claim,
                frame.point,
                current_identity,
                crate::palw_claim_local_v1::palw_liability_material_id_v1(bytes),
                ordinal as u64,
            )?;
            if found.is_some_and(|prior: ActualKernelSourceOwnerV9| prior != token) {
                return Err(bad("kernel source birth has conflicting actual origin evidence"));
            }
            found = Some(token);
        }
    }
    found.map(Some).ok_or_else(unavailable)
}
