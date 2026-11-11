//! Persistence checks for the protected original's service binding. These
//! enforce co-publication and preserve prior reservations. They do not derive
//! the complete profile graph, establish delivery, or authorize settlement.

use super::{
    native_frame::PublicTfp1FrameV1,
    service_booking::{book_extends_v1, validation_requests_v1},
};
use crate::{
    palw_native_resource_v1::reserve_native_pending_requests_v1,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::{PalwChainStateV2, PalwStateV2Error},
};
use misaka_palw_positive_contract::public_one_honest::OriginalClock;

fn bad(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}

/// Validate the common book once before checking individual protected frames.
/// Legacy-only snapshots retain their existing validation and row versions.
pub(crate) fn check_protected_service_book_v1(state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    if state.public_tfp1_frame_rows_v1().next().is_none() {
        return Ok(());
    }
    let Some(Row::CourtServiceV3(row)) = state.staged_court_row_v1(&Key::CourtService) else {
        return Err(bad("protected TFP1 frames require their native V3 service book"));
    };
    let scratch = validation_requests_v1(row.state.book.packets.len()).map_err(PalwStateV2Error::NativeResource)?;
    let _scratch = reserve_native_pending_requests_v1(scratch).map_err(PalwStateV2Error::NativeResource)?;
    row.check().map_err(PalwStateV2Error::CarriageInconsistent)?;
    let point = state.last_point().ok_or_else(|| bad("protected service book has no native state point"))?;
    if row.state.book.last_observed_daa.is_some_and(|observed| observed > point.daa_score) {
        return Err(bad("protected service observation is ahead of native state"));
    }
    Ok(())
}

/// Allocation-free scope check. The caller checks book structure once and
/// validates the frame/claim/link/history separately. A released binding is
/// only service accounting; it is not a protected claim terminal or payout.
pub(crate) fn check_protected_service_state_v1(state: &PalwChainStateV2, frame: &PublicTfp1FrameV1) -> Result<(), PalwStateV2Error> {
    let Some(Row::CourtServiceV3(row)) = state.staged_court_row_v1(&Key::CourtService) else {
        return Err(bad("protected TFP1 frame has no native service book"));
    };
    let claim = frame.claim_v1();
    match (row.state.claims.get(&claim), row.resolved.get(&claim)) {
        (Some(binding), None) => {
            frame.check_service_scope_v1(binding).map_err(|_| bad("protected TFP1 live service scope differs from its frame"))?;
            super::service_graph::check_frame_service_graph_v2(frame, row, binding)
                .map_err(|_| bad("protected TFP1 service graph differs from its immutable frame"))?;
            if binding.packet_ids.is_empty() {
                return Err(bad("protected TFP1 live service binding is empty"));
            }
        }
        (None, Some(record)) => {
            if frame.service_graph_v2().is_some() {
                return Err(bad("graph-bound TFP1 service requires its dedicated finite disposition"));
            }
            let hard =
                OriginalClock::new(frame.origin_v1().accepted_daa).map_err(|_| bad("protected TFP1 original clock overflows"))?.hard();
            if record.claim != claim
                || record.allocation_id != frame.identity_v1()
                || record.source_id != frame.source_v1()
                || record.original_accepted_daa != frame.origin_v1().accepted_daa
                || record.hard_deadline_daa != hard
                || record.released_daa < hard
                || row.state.book.last_observed_daa.is_none_or(|observed| record.released_daa > observed)
                || state.last_point().is_none_or(|point| record.released_daa > point.daa_score)
            {
                return Err(bad("protected TFP1 released service scope or clock differs from native state"));
            }
        }
        _ => return Err(bad("protected TFP1 frame must have exactly one live or released service binding")),
    }
    Ok(())
}

/// The fifth insertion in the admission journal must append exactly this
/// frame's live reservation, preserving all earlier bindings and supply.
/// Run against the forward old/new pair even while undoing the journal.
pub(crate) fn check_protected_service_insert_v1(
    frame: &PublicTfp1FrameV1,
    before: Option<&Row>,
    after: &Row,
) -> Result<(), PalwStateV2Error> {
    let (old, old_v3) = match before {
        Some(Row::CourtService(row)) => (row.as_ref(), None),
        Some(Row::CourtServiceV3(row)) => (&row.state, Some(row.as_ref())),
        _ => return Err(bad("protected service insertion has no prior native book")),
    };
    let Row::CourtServiceV3(next) = after else {
        return Err(bad("protected service insertion requires a V3 row"));
    };
    let scratch =
        validation_requests_v1(old.book.packets.len().max(next.state.book.packets.len())).map_err(PalwStateV2Error::NativeResource)?;
    let _scratch = reserve_native_pending_requests_v1(scratch).map_err(PalwStateV2Error::NativeResource)?;
    match old_v3 {
        Some(row) => row.check(),
        None => old.check(),
    }
    .map_err(PalwStateV2Error::CarriageInconsistent)?;
    next.check().map_err(PalwStateV2Error::CarriageInconsistent)?;
    let claim = frame.claim_v1();
    let binding = next.state.claims.get(&claim).ok_or_else(|| bad("protected service insertion omits its new binding"))?;
    frame.check_service_scope_v1(binding).map_err(|_| bad("protected service insertion substitutes frame scope"))?;
    super::service_graph::check_frame_service_graph_v2(frame, next, binding)
        .map_err(|_| bad("protected service insertion substitutes its immutable graph"))?;
    if old.claims.contains_key(&claim)
        || old_v3.is_some_and(|row| row.resolved.contains_key(&claim))
        || (old.profile_id, old.source_profile_id) != (next.state.profile_id, next.state.source_profile_id)
        || old.claims.len().checked_add(1) != Some(next.state.claims.len())
        || old.claims.iter().any(|(key, value)| next.state.claims.get(key) != Some(value))
        || old.book.last_observed_daa.is_some_and(|daa| daa > frame.origin_v1().accepted_daa)
        || !book_extends_v1(&old.book, &next.state.book, binding.packet_ids.len())
        || binding.packet_ids.iter().any(|id| old.book.packet(id).is_some())
    {
        return Err(bad("protected service insertion is not a strict original append"));
    }
    match old_v3 {
        Some(row) => {
            if row.resolved != next.resolved || row.causal.iter().any(|(key, value)| next.causal.get(key) != Some(value)) {
                return Err(bad("protected service insertion changes prior released records or causal edges"));
            }
        }
        None if !next.resolved.is_empty() => return Err(bad("protected service insertion invents released records")),
        None => {}
    }
    if next.causal.keys().any(|id| !old_v3.is_some_and(|row| row.causal.contains_key(id)) && !binding.packet_ids.contains(id)) {
        return Err(bad("protected service insertion adds another binding's causal edges"));
    }
    Ok(())
}

/// A forward native update cannot discard a graph-bound reservation through
/// the legacy compact release, which loses the authenticated packet promises.
/// The dedicated finite disposition remains a separate, required connection.
pub(crate) fn check_proposed_protected_graphs_v2(state: &PalwChainStateV2, next: Option<&Row>) -> Result<(), PalwStateV2Error> {
    for (claim, stored) in state.public_tfp1_frame_rows_v1() {
        let Row::PublicTfp1FrameV1(frame) = stored else {
            return Err(bad("protected service update has a mistyped native frame"));
        };
        if frame.claim_v1() != claim {
            return Err(bad("protected service update has a foreign native frame key"));
        }
        if frame.service_graph_v2().is_none() {
            continue;
        }
        let Some(Row::CourtServiceV3(row)) = next else {
            return Err(bad("graph-bound TFP1 service cannot be deleted or retyped"));
        };
        let binding = row
            .state
            .claims
            .get(&frame.claim_v1())
            .ok_or_else(|| bad("graph-bound TFP1 service requires its dedicated finite disposition"))?;
        if row.resolved.contains_key(&frame.claim_v1()) {
            return Err(bad("graph-bound TFP1 service cannot use legacy compact release"));
        }
        super::service_graph::check_frame_service_graph_v2(frame, row, binding)
            .map_err(|_| bad("native service update changes the protected original graph"))?;
    }
    Ok(())
}
