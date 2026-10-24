//! Last native duty read gates the actual signer after asynchronous source work.
//! These checks establish current packet identity/eligibility, never correctness
//! of model computation, personal fault, or selected-verifier approval.
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1 as Action, PalwConstraintStreamMoveV1 as Move},
    palw_staged_court_state_v1::{
        PalwStagedFlatProgressV1 as Flat, PalwStagedFloatProgressV1 as Float, PalwStagedTirPrefixProgressV1 as Tir,
    },
    palw_state_v2::PalwBondKeyV2,
};

/// Native read faults retry; absent or superseded duties retire only this local
/// prepared packet. The signer is never invoked until the fresh read is checked.
pub(super) fn sign_current_v1<P>(
    fresh: Result<Option<P>, String>,
    current: impl FnOnce(&P) -> bool,
    sign: impl FnOnce() -> Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, String> {
    let Some(fresh) = fresh? else { return Ok(None) };
    if !current(&fresh) {
        return Ok(None);
    }
    sign().map(Some).ok_or_else(|| "current response custodian signing key unavailable".into())
}
pub(super) fn flat_current_v1(old: &Flat, fresh: &Flat, movement: &Move, signer: PalwBondKeyV2) -> bool {
    let Action::FlatSourceAnswer(answer) = &movement.action else { return false };
    let (a, b) = (&old.session, &fresh.session);
    movement.version == 1
        && movement.phase == kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt
        && old.context == fresh.context
        && fresh.now_daa >= old.now_daa
        && (a.claim, a.owner, a.reserved_daa, a.bundle_id, a.source_id) == (b.claim, b.owner, b.reserved_daa, b.bundle_id, b.source_id)
        && (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
            == (b.claim, b.owner, b.reserved_daa, b.bundle_id)
        && a.producer == b.producer
        && a.custodians == b.custodians
        && a.nomination_v1() == b.nomination_v1()
        && a.response_deadline_v1(&signer) == b.response_deadline_v1(&signer)
        && fresh.now_daa >= b.reserved_daa
        && b.response_deadline_v1(&signer).is_some_and(|until| fresh.now_daa < until)
        && b.has_response_duty_v1(&signer)
        && !b.collected_v1()
        && answer.answerer == signer
        && answer.source_id == b.source_id
        && b.slot_cursor_v1(&signer).is_ok_and(|cursor| cursor == answer.cursor)
}
pub(super) fn float_current_v1(old: &Float, fresh: &Float, movement: &Move, signer: PalwBondKeyV2) -> bool {
    let Action::FloatSourceAnswer(answer) = &movement.action else { return false };
    let (a, b) = (&old.session, &fresh.session);
    movement.version == 1
        && movement.phase == kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt
        && old.context == fresh.context
        && fresh.now_daa >= old.now_daa
        && (a.claim, a.owner, a.reserved_daa, a.bundle_id, a.source_id) == (b.claim, b.owner, b.reserved_daa, b.bundle_id, b.source_id)
        && (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
            == (b.claim, b.owner, b.reserved_daa, b.bundle_id)
        && a.producer == b.producer
        && a.custodians == b.custodians
        && a.nomination_v1() == b.nomination_v1()
        && a.response_deadline_v1(&signer) == b.response_deadline_v1(&signer)
        && fresh.now_daa >= b.reserved_daa
        && b.response_deadline_v1(&signer).is_some_and(|until| fresh.now_daa < until)
        && b.has_response_duty_v1(&signer)
        && !b.collected_v1()
        && answer.answerer == signer
        && answer.source_id == b.source_id
        && b.slot_cursor_v1(&signer).is_ok_and(|cursor| cursor == answer.cursor)
}
pub(super) fn tir_current_v1(old: &Tir, fresh: &Tir, movement: &Move, signer: PalwBondKeyV2) -> bool {
    let Action::NativeTirPrefixAnswer(answer) = &movement.action else { return false };
    let (a, b) = (&old.row, &fresh.row);
    movement.version == 1
        && movement.phase == kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt
        && old.header == fresh.header
        && fresh.now_daa >= old.now_daa
        && (a.claim, a.reserver, a.reserved_daa, a.seed_id()) == (b.claim, b.reserver, b.reserved_daa, b.seed_id())
        && (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
            == (b.claim, b.reserver, b.reserved_daa, b.seed_id())
        && a.terms == b.terms
        && a.producer == b.producer
        && a.custodians == b.custodians
        && a.respond_until_daa == b.respond_until_daa
        && fresh.now_daa >= b.reserved_daa
        && fresh.now_daa < b.respond_until_daa
        && b.has_response_duty_v1(&signer)
        && answer.answerer == signer
        && answer.seed_id == b.seed_id()
        && b.slot_ordinal_v1(&signer) == Some(answer.ordinal)
}

#[cfg(test)]
#[path = "palw_response_presign_tests.rs"]
mod tests;

/// Share the reader and its complete host reservation as one lifetime. A book
/// eviction/canceled async waiter cannot release memory held by a blocking poll.
/// Field order drops the actual reader before either allocation ticket.
pub(super) struct OwnedSourceReaderV1<R: ?Sized> {
    pub(super) reader: std::sync::Mutex<Box<R>>,
    _memory: crate::palw_memory_ledger::PalwMemoryReservationV1,
    _metadata: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl<R: ?Sized> OwnedSourceReaderV1<R> {
    pub(super) fn new(
        reader: Box<R>,
        memory: crate::palw_memory_ledger::PalwMemoryReservationV1,
    ) -> Result<std::sync::Arc<Self>, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        // Assemble only stack/pointer fields first so every early refusal also
        // drops the reader before its host guard (not reverse argument order).
        let mut value = Self { reader: std::sync::Mutex::new(reader), _memory: memory, _metadata: None };
        let mut q = kaspa_consensus_core::palw_routed_material_v2::PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<Self>(1)?;
        q.vec_v1::<usize>(3)?;
        value._metadata = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1())?;
        Ok(std::sync::Arc::new(value))
    }
}
