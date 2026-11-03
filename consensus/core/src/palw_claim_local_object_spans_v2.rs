//! Successful outer accepted-object journal boundaries, made by the executing
//! fold. These observations identify data; they grant no source/credit authority.
//! Inner chunk deliveries belong to their actual outer signed object, not to a
//! second position in the accepting block's object list.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExecutedObjectSpanV2 {
    pub ordinal: usize,
    pub object_id: Hash64,
    pub first: usize,
    pub end: usize,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct ScopedExecutionCheckpointV2 {
    pub family_spans: usize,
    pub object_spans: usize,
    pub scope_active: bool,
    pub lock_point_v6: Option<PalwBlockContextV2>,
    pub gc_sequence_v3: u64,
    pub gc_defer_v3: bool,
    pub compact_undo_v15: usize,
    pub compact_inactive_v15: Option<(usize, bool)>,
}
pub(super) fn checkpoint(b: &TransitionBuilder<'_>) -> ScopedExecutionCheckpointV2 {
    ScopedExecutionCheckpointV2 {
        family_spans: b.claim_local_family_spans.len(),
        object_spans: b.claim_local_object_spans.len(),
        scope_active: b.claim_local_object_scope_active,
        lock_point_v6: b.claim_local_lock_point_v6,
        gc_sequence_v3: b.claim_local_gc_sequence_v3,
        gc_defer_v3: b.claim_local_gc_defer_v3,
        compact_undo_v15: b.compact_root_undo_v15.len(),
        compact_inactive_v15: b.compact_root_inactive_undo_v15,
    }
}
pub(super) fn restore(b: &mut TransitionBuilder<'_>, point: ScopedExecutionCheckpointV2) {
    b.claim_local_family_spans.truncate(point.family_spans);
    b.claim_local_object_spans.truncate(point.object_spans);
    b.claim_local_object_scope_active = point.scope_active;
    b.claim_local_lock_point_v6 = point.lock_point_v6;
    b.compact_root_undo_v15.truncate(point.compact_undo_v15);
    b.compact_root_inactive_undo_v15 = point.compact_inactive_v15;
    super::palw_claim_local_context_gc_v3::restore_observations_v3(b, (point.gc_sequence_v3, point.gc_defer_v3));
}
pub(super) fn observation_request_bytes_v2(count: u64) -> Result<u64, NativeCoverageV1> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<ExecutedObjectSpanV2>(count)?;
    Ok(q.bytes_v1())
}
fn bad(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}
fn object_id(object: &PalwConsensusObjectV2) -> Result<Hash64, PalwStateV2Error> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/executed-object/v2", object).map_err(bad_owned)
}
fn bad_owned(why: String) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why)
}
/// Reserve the annotation before the object can mutate state. No growing Vec or
/// hashing error may arise after a successful inner financial/state transition.
pub(super) fn prepare(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
) -> Result<Option<ExecutedObjectSpanV2>, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) || b.claim_local_object_scope_active {
        return Ok(None);
    }
    let id = object_id(object)?;
    let bytes = observation_request_bytes_v2(1).map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation.grow_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    b.claim_local_object_spans.try_reserve_exact(1).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<ExecutedObjectSpanV2>("executed object observations"))
    })?;
    Ok(Some(ExecutedObjectSpanV2 {
        ordinal: b.claim_local_object_spans.len(),
        object_id: id,
        first: b.entries.len(),
        end: b.entries.len(),
    }))
}
/// The dispatcher calls this only after success. Capacity was admitted in
/// prepare(), and recursive inner dispatchers never append another outer slot.
pub(super) fn complete(b: &mut TransitionBuilder<'_>, mut span: ExecutedObjectSpanV2) {
    span.end = b.entries.len();
    b.claim_local_object_spans.push(span);
}
/// Resolve one actual journal effect to the exact accepting-list data. An
/// unrelated/mutated input list, overlap or missing annotation is not authority.
/// Gaps before/after object execution belong to system ticks, never to a guessed
/// caller. The eventual source writer decides their explicit separate ownership.
pub(super) fn object_for_entry<'a>(
    b: &TransitionBuilder<'_>,
    objects: &'a [PalwConsensusObjectV2],
    entry: usize,
) -> Result<Option<&'a PalwConsensusObjectV2>, PalwStateV2Error> {
    if entry >= b.entries.len() {
        return Err(bad("executed object lookup outside actual journal"));
    }
    if b.claim_local_object_spans.len() != objects.len() {
        return Err(bad("executed object observation count differs from actual accepting list"));
    }
    let mut previous = 0;
    let mut found = None;
    for (ordinal, (span, object)) in b.claim_local_object_spans.iter().zip(objects).enumerate() {
        if span.ordinal != ordinal
            || span.first > span.end
            || span.end > b.entries.len()
            || span.first < previous
            || span.object_id != object_id(object)?
        {
            return Err(bad("executed object observation identity/order differs"));
        }
        previous = span.end;
        if span.first <= entry && entry < span.end {
            found = Some(object)
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parameters(active: bool) -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 100, 10, 20, 50, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(active.then_some(0))
    }
    fn point() -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: Hash64::from_u64_word(2), daa_score: 1, blue_score: 1, subsidy: 0 }
    }
    fn registration(n: u8) -> PalwConsensusObjectV2 {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([n; 32]);
        // This test covers the executing fold's observation boundary, not the
        // transaction authentication boundary or granting computational credit.
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(u64::from(n)), 0)),
            pubkey: kp.verification_key.as_ref().to_vec(),
            operator_pubkey: kp.verification_key.as_ref().to_vec(),
            collateral: 1000,
            payout_payload: Hash64::from_u64_word(100 + u64::from(n)),
            capable_classes: BTreeSet::new(),
            signature: Vec::new(),
        }
    }
    #[test]
    fn rfc23s2_executed_object_observations_follow_success_order_and_rollback() {
        let p = parameters(true);
        let e = PalwTransitionExtrasV1::default();
        let state = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let saved = b.checkpoint();
        let objects = [registration(11), registration(12)];
        objects::apply_object(&mut b, &point(), &objects[0]).unwrap();
        assert_eq!(b.claim_local_object_spans.len(), 1);
        let first = b.claim_local_object_spans[0];
        assert_eq!(first.ordinal, 0);
        assert!(first.end > first.first);
        let root = b.state.state_root();
        let entries = b.entries.len();
        assert!(objects::apply_object(&mut b, &point(), &objects[0]).is_err());
        assert_eq!(b.claim_local_object_spans, [first]);
        assert_eq!(b.state.state_root(), root);
        assert_eq!(b.entries.len(), entries);
        assert!(!b.claim_local_object_scope_active);
        objects::apply_object(&mut b, &point(), &objects[1]).unwrap();
        assert_eq!(b.claim_local_object_spans[1].ordinal, 1);
        assert_eq!(object_id(object_for_entry(&b, &objects, first.first).unwrap().unwrap()).unwrap(), object_id(&objects[0]).unwrap());
        assert_eq!(
            object_id(object_for_entry(&b, &objects, b.claim_local_object_spans[1].first).unwrap().unwrap()).unwrap(),
            object_id(&objects[1]).unwrap()
        );
        b.restore(saved);
        assert_eq!(b.state, state);
        assert!(b.entries.is_empty());
        assert!(b.claim_local_object_spans.is_empty());
        assert!(!b.claim_local_object_scope_active);
    }
    #[test]
    fn rfc23s2_executed_object_observations_reject_substituted_input_and_leave_system_gaps() {
        let p = parameters(true);
        let e = PalwTransitionExtrasV1::default();
        let state = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        b.write_exposure(PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(90), 0)), Some(1)).unwrap();
        let objects = [registration(13)];
        objects::apply_object(&mut b, &point(), &objects[0]).unwrap();
        assert!(object_for_entry(&b, &objects, 0).unwrap().is_none(), "a system effect is not one caller's source");
        assert!(object_for_entry(&b, &[], 1).is_err());
        assert!(object_for_entry(&b, &[registration(14)], 1).is_err());
        b.claim_local_object_spans[0].first = usize::MAX;
        assert!(object_for_entry(&b, &objects, 1).is_err());
    }
    #[test]
    fn rfc23s2_executed_object_observations_are_dormant_and_nested_inputs_have_one_outer_slot() {
        let p = parameters(false);
        let e = PalwTransitionExtrasV1::default();
        let state = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let object = registration(15);
        objects::apply_object(&mut b, &point(), &object).unwrap();
        assert!(b.claim_local_object_spans.is_empty());
        let p = parameters(true);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &e);
        let outer = prepare(&mut b, &point(), &object).unwrap().unwrap();
        b.claim_local_object_scope_active = true;
        assert!(
            prepare(&mut b, &point(), &registration(16)).unwrap().is_none(),
            "decoded chunk controls have no second outer ordinal"
        );
        objects::apply_object(&mut b, &point(), &object).unwrap();
        assert!(b.claim_local_object_spans.is_empty());
        b.claim_local_object_scope_active = false;
        complete(&mut b, outer);
        assert_eq!(b.claim_local_object_spans.len(), 1);
        assert!(observation_request_bytes_v2(u64::MAX).is_err());
    }
}
