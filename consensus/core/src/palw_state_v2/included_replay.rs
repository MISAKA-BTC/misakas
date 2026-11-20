//! Include a panel member's signed full-replay approval from an accepted
//! transaction. The fold already writes `extras.public_replay_approvals`.
//! This module fills that input from the bound panel and the literal carrier.
//! A seat receipt is a different signature and is not read here.

use std::collections::BTreeMap;

use kaspa_hashes::Hash64;
use misaka_palw_positive_contract::{
    identity::{Digest, PUBKEY_BYTES, SIGNATURE_BYTES, principal},
    public_one_honest::{PanelBond, PublicDrawMemberV1, protected_seat_rescue_reservation_v1},
    rows::{AcceptedOrigin, NativeHash64},
};

use super::{PalwBondKeyV2, PalwChainStateV2, PalwPanelStateV2};
use crate::palw_positive_contract_v2::{PreparedPublicDrawRecordV1, PreparedPublicReplayApprovalV1, PublicDrawRecordV1};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::tx::TransactionOutpoint;

struct BoundPanelV1 {
    beacon: Digest,
    members: Vec<PublicDrawMemberV1>,
}

/// The protected original's hard deadline, available before its draw is stored.
pub fn protected_full_replay_deadline_v1(state: &PalwChainStateV2, claim: Hash64) -> Option<u64> {
    let frame = state.public_tfp1_frame_v1(claim).ok()??;
    frame.origin_v1().accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
}

/// Pin the replayed admission even when a reorg installs the same claim anew.
/// The signer and payload builder each call this on their own atomic snapshot.
pub fn protected_full_replay_digest_bound_v2(
    state: &PalwChainStateV2,
    expected: crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1,
    bond: PalwBondKeyV2,
    key: &[u8],
) -> Option<[u8; 64]> {
    let read = crate::palw_protected_original_readback_v1::protected_original_readback_v1(state, expected.claim).ok()??;
    if read.replay_binding_v1() != expected {
        return None;
    }
    protected_full_replay_digest_v1(state, expected.claim, bond, key)
}

pub fn protected_full_replay_payload_bound_v2(
    state: &PalwChainStateV2,
    expected: crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1,
    bond: PalwBondKeyV2,
    key: &[u8],
    signature: &[u8],
) -> Option<Vec<u8>> {
    let read = crate::palw_protected_original_readback_v1::protected_original_readback_v1(state, expected.claim).ok()??;
    if read.replay_binding_v1() != expected {
        return None;
    }
    protected_full_replay_payload_v1(state, expected.claim, bond, key, signature)
}

/// The digest a bound seat signs after its replay. The stored draw supplies the
/// scope. An approval already stored at this tip needs no new signature.
/// A seat receipt is not this digest.
pub fn protected_full_replay_digest_v1(state: &PalwChainStateV2, claim: Hash64, bond: PalwBondKeyV2, key: &[u8]) -> Option<[u8; 64]> {
    let key: &[u8; PUBKEY_BYTES] = key.try_into().ok()?;
    let approval = approval_for_bound_seat_v1(state, claim, bond, key)?;
    // A mempool send is not acceptance. Stop offering this exact seat's
    // approval only while its row exists on the current branch. A reverted
    // inclusion makes the same completed replay eligible for retransmission.
    if !matches!(state.public_replay_approval_record_v1(approval.original, approval.bond), Ok(None)) {
        return None;
    }
    Some(approval.signing_digest().0)
}

/// Exact transaction payload consensus stores when this signature matches the stored draw.
pub fn protected_full_replay_payload_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    bond: PalwBondKeyV2,
    key: &[u8],
    signature: &[u8],
) -> Option<Vec<u8>> {
    use misaka_palw_positive_contract::public_one_honest::{ReplayApprovalContext, SignedFullReplayApprovalV1};
    let key: &[u8; PUBKEY_BYTES] = key.try_into().ok()?;
    let signature: &[u8; SIGNATURE_BYTES] = signature.try_into().ok()?;
    let approval = approval_for_bound_seat_v1(state, claim, bond, key)?;
    let context = ReplayApprovalContext {
        original: approval.original,
        draw: approval.draw,
        bond: approval.bond,
        member: approval.member,
        full_scope: approval.full_scope,
    };
    approval.authenticate(&context, key, signature).ok()?;
    let bytes = borsh::to_vec(&SignedFullReplayApprovalV1 { approval, key: *key, signature: *signature }).ok()?;
    SignedFullReplayApprovalV1::decode(&bytes).ok()?;
    Some(bytes)
}

fn approval_for_bound_seat_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    bond: PalwBondKeyV2,
    key: &[u8; PUBKEY_BYTES],
) -> Option<misaka_palw_positive_contract::public_one_honest::FullReplayApproval> {
    use misaka_palw_positive_contract::public_one_honest::FullReplayApproval;
    let frame = state.public_tfp1_frame_v1(claim).ok()??;
    let panel = state.panel(&claim)?;
    if !panel.seats.iter().any(|seat| seat.bond == bond) {
        return None;
    }
    let panel_bond = panel_bond_v1(bond);
    let registered = seat_key_v1(state, panel_bond)?;
    if registered != *key {
        return None;
    }
    let original = frame.original_id_v1();
    let draw = state.public_draw_record_v1(original).ok()??;
    let members = seat_members_v1(state, panel, frame.reservation_v1())?;
    if !draw.accepts_members_v1(&members) {
        return None;
    }
    let member = members.iter().find(|member| member.bond == panel_bond)?;
    if member.principal != principal(key) {
        return None;
    }
    Some(FullReplayApproval {
        original,
        draw: draw.draw_v1(),
        bond: panel_bond,
        member: member.principal,
        full_scope: draw.full_scope_v1(),
    })
}

/// At least one bound seat has a full-replay approval stored for this claim's original.
/// A seat `Valid` receipt is not this approval.
pub fn protected_replay_approval_recorded_v1(state: &PalwChainStateV2, claim: Hash64) -> bool {
    let Ok(Some(frame)) = state.public_tfp1_frame_v1(claim) else { return false };
    let Some(panel) = state.panel(&claim) else { return false };
    let original = frame.original_id_v1();
    panel.seats.iter().any(|seat| matches!(state.public_replay_approval_record_v1(original, panel_bond_v1(seat.bond)), Ok(Some(_))))
}

/// Exact carrier origins for the same bound seats used by the Final gate.
pub(crate) fn protected_replay_approval_origins_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
) -> impl Iterator<Item = (PalwBondKeyV2, AcceptedOrigin)> + '_ {
    let original = state.public_tfp1_frame_v1(claim).ok().flatten().map(|frame| frame.original_id_v1());
    state.panel(&claim).into_iter().flat_map(|panel| panel.seats.iter()).filter_map(move |seat| {
        let row = state.public_replay_approval_record_v1(original?, panel_bond_v1(seat.bond)).ok()??;
        Some((seat.bond, row.accepted_origin_v1()))
    })
}

/// A seat's own approval completes its work; another seat's approval does not.
pub(crate) fn protected_seat_replay_recorded_v1(state: &PalwChainStateV2, claim: Hash64, bond: PalwBondKeyV2) -> bool {
    let Ok(Some(frame)) = state.public_tfp1_frame_v1(claim) else { return false };
    matches!(state.public_replay_approval_record_v1(frame.original_id_v1(), panel_bond_v1(bond)), Ok(Some(_)))
}

/// A protected claim already has a bound panel, so this block may carry a draw or an approval.
pub fn protected_bound_panel_present_v1(state: &PalwChainStateV2) -> bool {
    bound_panels_v1(state).next().is_some()
}

/// Draw records this accepting point can store for panels bound on the parent.
/// An original that already has a draw is left unchanged.
pub fn protected_draws_for_point_v1(state: &PalwChainStateV2, point: &super::PalwBlockContextV2) -> Vec<PreparedPublicDrawRecordV1> {
    let mut draws = Vec::new();
    for (original_bytes, panel) in panels_by_original_v1(state) {
        let original = misaka_palw_positive_contract::public_one_honest::OriginalId(Digest(original_bytes));
        if !matches!(state.public_draw_record_v1(original), Ok(None)) {
            continue;
        }
        let Ok(Some(admission)) = state.public_original_admission_record_v1(original) else {
            continue;
        };
        if let Ok(draw) = admission.panel_draw_at_v1(point, panel.beacon, &panel.members) {
            draws.push(draw);
        }
    }
    draws
}

/// Verify one accepted payload. Payloads that are not a full-replay approval,
/// or that do not match the bound seat and the stored draw, are absent.
pub fn included_replay_approval_v1(
    state: &PalwChainStateV2,
    point: &super::PalwBlockContextV2,
    draws: &[PreparedPublicDrawRecordV1],
    carrier_block: Hash64,
    carrier_tx: Hash64,
    ordinal: u32,
    payload: &[u8],
) -> Option<PreparedPublicReplayApprovalV1> {
    if payload.len() != misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1::WIRE_BYTES {
        return None;
    }
    let signed = misaka_palw_positive_contract::public_one_honest::SignedFullReplayApprovalV1::decode(payload).ok()?;
    let original = signed.approval.original;
    let panels = panels_by_original_v1(state);
    let panel = panels.get(&original.0.0)?;
    let draw = draw_record_v1(state, draws, original)?;
    let seat_key = seat_key_v1(state, signed.approval.bond)?;
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64(point.block.as_bytes()),
        carrier_block: NativeHash64(carrier_block.as_bytes()),
        carrier_transaction: NativeHash64(carrier_tx.as_bytes()),
        accepted_tx_ordinal: ordinal,
        event_ordinal: 0,
        accepted_daa: point.daa_score,
        accepted_blue_score: point.blue_score,
    };
    draw.verify_panel_replay_bytes_v1(&panel.members, origin, payload, &seat_key).ok()
}

fn draw_record_v1(
    state: &PalwChainStateV2,
    draws: &[PreparedPublicDrawRecordV1],
    original: misaka_palw_positive_contract::public_one_honest::OriginalId,
) -> Option<PublicDrawRecordV1> {
    if let Ok(Some(stored)) = state.public_draw_record_v1(original) {
        return Some(stored);
    }
    draws.iter().find(|draw| draw.as_record_v1().original_v1() == original).map(|draw| *draw.as_record_v1())
}

fn panels_by_original_v1(state: &PalwChainStateV2) -> BTreeMap<[u8; 64], BoundPanelV1> {
    let mut out = BTreeMap::new();
    for (claim, frame) in bound_panels_v1(state) {
        let Some(panel) = state.panels.get(&claim) else { continue };
        let Some(members) = seat_members_v1(state, panel, frame.reservation_v1()) else { continue };
        let key = frame.original_id_v1().0.0;
        let previous = out.insert(key, BoundPanelV1 { beacon: Digest(panel.anchor.as_bytes()), members });
        if previous.is_some() {
            out.remove(&key);
        }
    }
    out
}

fn bound_panels_v1(
    state: &PalwChainStateV2,
) -> impl Iterator<Item = (Hash64, &crate::palw_positive_contract_v2::PublicTfp1FrameV1)> + '_ {
    state.staged_courts.iter().filter_map(|(key, row)| match (key, row) {
        (Key::PublicTfp1FrameV1(claim), Row::PublicTfp1FrameV1(frame))
            if state.panels.get(claim).is_some_and(|panel| !panel.seats.is_empty()) =>
        {
            Some((*claim, frame.as_ref()))
        }
        _ => None,
    })
}

pub(crate) fn seat_members_v1(
    state: &PalwChainStateV2,
    panel: &PalwPanelStateV2,
    reservation: Digest,
) -> Option<Vec<PublicDrawMemberV1>> {
    let mut members = Vec::with_capacity(panel.seats.len());
    for seat in &panel.seats {
        let bond = panel_bond_v1(seat.bond);
        let key = seat_key_v1(state, bond)?;
        members.push(PublicDrawMemberV1 {
            bond,
            principal: principal(&key),
            rescue_reservation: protected_seat_rescue_reservation_v1(reservation, bond),
        });
    }
    Some(members)
}

fn seat_key_v1(state: &PalwChainStateV2, bond: PanelBond) -> Option<[u8; PUBKEY_BYTES]> {
    let key = PalwBondKeyV2(TransactionOutpoint { transaction_id: Hash64::from_bytes(bond.transaction.0), index: bond.index });
    let record = state.bonds.get(&key)?;
    record.pubkey.as_slice().try_into().ok()
}

fn panel_bond_v1(bond: PalwBondKeyV2) -> PanelBond {
    PanelBond { transaction: Digest(bond.0.transaction_id.as_bytes()), index: bond.0.index }
}

#[cfg(test)]
pub(crate) fn exercise_included_replay_v1(
    admitted: &PalwChainStateV2,
    params: &super::PalwStateParamsV2,
    extras: &super::PalwTransitionExtrasV1,
    claim: Hash64,
    prepared: &crate::palw_positive_contract_v2::tfp1_original::PreparedTfp1PublicOriginalV1,
) {
    use super::{
        PalwBlockContextV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwTransitionExtrasV1,
        apply_delta_v2, apply_palw_transition_v2_with_extras, revert_delta_v2,
    };
    use crate::palw_positive_contract_v2::{PUBLIC_DRAW_TABLE_V1, PUBLIC_REPLAY_APPROVAL_TABLE_V1};
    use misaka_palw_positive_contract::public_one_honest::FULL_REPLAY_SIGN_CONTEXT;

    // Start from the real paired admission fixture. Only the selected panel and
    // its registered key are fixture inputs here; this is not a draw-selection test.
    let mut state = admitted.clone();
    let producer = state.claim(&claim).unwrap().bond;
    let bond = *state.bonds.keys().find(|bond| **bond != producer).unwrap();
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x71; 32]);
    let key = keys.verification_key.as_ref();
    state.bonds.get_mut(&bond).unwrap().pubkey = key.to_vec();
    let operator_id = super::palw_operator_id_v2(key);
    state.bonds.get_mut(&bond).unwrap().operator_id = operator_id;
    let prior = *state.last_point().unwrap();
    let bound_point = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x7101),
        daa_score: prior.daa_score + 1,
        blue_score: prior.blue_score + 1,
        subsidy: 0,
    };
    let mut following = extras.clone();
    following.tir_funded_originals.clear();
    let check_delta = |parent: &PalwChainStateV2, child: &PalwChainStateV2, delta: &PalwStateDeltaV2| {
        assert_eq!(apply_delta_v2(parent, delta, params).unwrap().state_root(), child.state_root());
        assert_eq!(revert_delta_v2(child, delta, params).unwrap().state_root(), parent.state_root());
    };
    let (bound, bound_delta) = apply_palw_transition_v2_with_extras(
        &state,
        params,
        &bound_point,
        &[PalwConsensusObjectV2::PanelBound {
            claim,
            anchor: Hash64::from_u64_word(0x7102),
            seats: vec![PalwPanelSeatV2 { bond, operator_id }],
        }],
        None,
        false,
        false,
        false,
        false,
        &following,
    )
    .unwrap();
    check_delta(&state, &bound, &bound_delta);
    assert!(matches!(bound.claim(&claim).unwrap().phase, super::PalwClaimPhaseV2::PanelBound { .. }));
    state = bound;
    let draw_point = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x7103),
        daa_score: bound_point.daa_score + 1,
        blue_score: bound_point.blue_score + 1,
        subsidy: 0,
    };
    assert!(!protected_replay_approval_recorded_v1(&state, claim));
    assert!(protected_full_replay_digest_v1(&state, claim, bond, key).is_none());
    assert!(prepared.restore_bound_request_draw_v3(&state).is_err(), "a bound panel alone is not an accepted draw");
    let draws = protected_draws_for_point_v1(&state, &draw_point);
    assert_eq!(draws.len(), 1);
    let original = state.public_tfp1_frame_v1(claim).unwrap().unwrap().original_id_v1();
    let read_view = crate::palw_protected_original_readback_v1::protected_filer_tir_view_v1;
    let leaves = state.claim(&claim).unwrap().work_leaves;
    assert!(state.staged_bundle_contract_v1(claim).is_none());
    let (view, header) = read_view(&state, claim, leaves).unwrap().unwrap();
    assert_eq!(header.as_ref(), state.public_tfp1_frame_v1(claim).unwrap().unwrap().header_v1());
    assert_eq!(view.committed_execution_root, state.claim(&claim).unwrap().execution_root);
    assert!(read_view(&state, claim, leaves - 1).is_err());
    assert!(read_view(&state, Hash64::default(), leaves).unwrap().is_none());
    let (drawn, draw_delta) = apply_palw_transition_v2_with_extras(
        &state,
        params,
        &draw_point,
        &[],
        None,
        false,
        false,
        false,
        false,
        &PalwTransitionExtrasV1 { public_draws: draws, ..following.clone() },
    )
    .unwrap();
    check_delta(&state, &drawn, &draw_delta);
    assert!(
        draw_delta
            .entries
            .iter()
            .any(|entry| matches!(entry, super::PalwDeltaEntryV2::KernelRouteRow { table: PUBLIC_DRAW_TABLE_V1, .. }))
    );
    state = drawn;
    let restored_draw = prepared.restore_bound_request_draw_v3(&state).unwrap();
    assert_eq!(restored_draw.record(), state.public_draw_record_v1(original).unwrap().unwrap());
    drop(restored_draw);
    let mut changed = state.clone();
    changed.panels.get_mut(&claim).unwrap().anchor = Hash64::from_u64_word(0x7190);
    assert!(prepared.restore_bound_request_draw_v3(&changed).is_err(), "beacon substitution");
    changed = state.clone();
    changed.panels.get_mut(&claim).unwrap().bound_daa = draw_point.daa_score + 1;
    assert!(prepared.restore_bound_request_draw_v3(&changed).is_err(), "panel bound after its purported draw");
    changed = state.clone();
    changed.panels.remove(&claim);
    assert!(prepared.restore_bound_request_draw_v3(&changed).is_err(), "missing native panel");
    changed = state.clone();
    changed.panels.get_mut(&claim).unwrap().seats.clear();
    assert!(prepared.restore_bound_request_draw_v3(&changed).is_err(), "empty native panel");
    changed = state.clone();
    changed.bonds.get_mut(&bond).unwrap().pubkey[0] ^= 1;
    assert!(prepared.restore_bound_request_draw_v3(&changed).is_err(), "registered key substitution");
    let digest = protected_full_replay_digest_v1(&state, claim, bond, key).unwrap();
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &digest, FULL_REPLAY_SIGN_CONTEXT, [0; 32]).unwrap();
    let payload = protected_full_replay_payload_v1(&state, claim, bond, key, signature.as_ref()).unwrap();
    let binding = crate::palw_protected_original_readback_v1::protected_original_readback_v1(&state, claim)
        .unwrap()
        .unwrap()
        .replay_binding_v1();
    // Scheduling views use the same native admission before and after licence.
    // Phase-only fixtures below do not claim to test licence acceptance or draw selection.
    let duties_in_phase = |source: &PalwChainStateV2, phase| {
        let mut view = source.clone();
        view.claims.get_mut(&claim).unwrap().phase = phase;
        crate::palw_producer_v2::palw_seat_duties_v2(&view, params, &[bond])
    };
    let licensed_phase = super::PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: draw_point.daa_score };
    let before_approval = duties_in_phase(&state, licensed_phase.clone());
    assert_eq!(before_approval.len(), 1, "a licence cannot cancel the independent full replay");
    assert_eq!(before_approval[0].protected_replay, Some(binding));
    assert_eq!(protected_full_replay_digest_bound_v2(&state, binding, bond, key), Some(digest));
    assert_eq!(protected_full_replay_payload_bound_v2(&state, binding, bond, key, signature.as_ref()), Some(payload.clone()));
    // A cached successful replay must never be signed for a changed original,
    // carrier, execution, or acceptance point, even when the claim ID is equal.
    let mut changed = Vec::new();
    for edit in [
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.original = Hash64::from_u64_word(991),
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.execution_root = Hash64::from_u64_word(991),
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| {
            b.carrier_transaction = Hash64::from_u64_word(991)
        },
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.carrier_block = Hash64::from_u64_word(991),
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.accepting_block = Hash64::from_u64_word(991),
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.accepted_daa += 1,
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.accepted_blue_score += 1,
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.accepted_tx_ordinal += 1,
        |b: &mut crate::palw_protected_original_readback_v1::ProtectedReplayBindingV1| b.event_ordinal += 1,
    ] {
        let mut other = binding;
        edit(&mut other);
        changed.push(other);
    }
    for other in changed {
        assert!(protected_full_replay_digest_bound_v2(&state, other, bond, key).is_none());
        assert!(protected_full_replay_payload_bound_v2(&state, other, bond, key, signature.as_ref()).is_none());
    }
    assert!(protected_full_replay_digest_v1(&state, claim, producer, key).is_none());
    let foreign = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x72; 32]);
    assert!(protected_full_replay_digest_v1(&state, claim, bond, foreign.verification_key.as_ref()).is_none());

    let point = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x7104),
        daa_score: draw_point.daa_score + 1,
        blue_score: draw_point.blue_score + 1,
        subsidy: 0,
    };
    let tx = Hash64::from_u64_word(0x7105);
    let included = included_replay_approval_v1(&state, &point, &[], point.block, tx, 0, &payload).unwrap();
    let mut altered = payload.clone();
    *altered.last_mut().unwrap() ^= 1;
    assert!(included_replay_approval_v1(&state, &point, &[], point.block, tx, 0, &altered).is_none());
    assert!(included_replay_approval_v1(&state, &prior, &[], prior.block, tx, 0, &payload).is_none());
    let too_late = PalwBlockContextV2 { daa_score: prior.daa_score + 10_001, ..point };
    assert!(included_replay_approval_v1(&state, &too_late, &[], point.block, tx, 0, &payload).is_none());
    let (_, expected) = included.storage_row(&state, &point).unwrap().unwrap();
    let (voted, vote_delta) = apply_palw_transition_v2_with_extras(
        &state,
        params,
        &point,
        &[],
        None,
        false,
        false,
        false,
        false,
        &PalwTransitionExtrasV1 { public_replay_approvals: vec![included, included], ..following.clone() },
    )
    .unwrap();
    check_delta(&state, &voted, &vote_delta);
    assert_eq!(voted.public_replay_approval_record_v1(original, panel_bond_v1(bond)).unwrap(), Some(*expected));
    assert!(protected_replay_approval_recorded_v1(&voted, claim));
    let approvals: Vec<_> = protected_replay_approval_origins_v1(&voted, claim).collect();
    assert_eq!(approvals, vec![(bond, expected.accepted_origin_v1())]);
    assert_eq!(approvals[0].1.carrier_transaction.0, tx.as_bytes());
    assert!(duties_in_phase(&voted, licensed_phase.clone()).is_empty(), "this seat's included full approval completes its duty");
    assert_eq!(
        crate::palw_producer_v2::palw_seat_duties_v2(&voted, params, &[bond]).len(),
        1,
        "an approval does not replace the still-bound panel's ordinary licence receipt"
    );
    let mut other_seat_view = voted.clone();
    let other_bond = *other_seat_view.bonds.keys().find(|other| **other != producer && **other != bond).unwrap();
    other_seat_view
        .panels
        .get_mut(&claim)
        .unwrap()
        .seats
        .push(PalwPanelSeatV2 { bond: other_bond, operator_id: other_seat_view.bonds.get(&other_bond).unwrap().operator_id });
    other_seat_view.claims.get_mut(&claim).unwrap().phase = licensed_phase.clone();
    let other_duties = crate::palw_producer_v2::palw_seat_duties_v2(&other_seat_view, params, &[other_bond]);
    assert_eq!(other_duties.len(), 1, "another seat's approval must not cancel this seat's whole replay");
    assert_eq!(other_duties[0].protected_replay, Some(binding));

    // The public reservation predates the draw. Only the owner that has actually
    // signed an included full replay can retire its untouched reservation. Work
    // already opened under an outside reservation keeps its independent liability.
    let mut reserved = other_seat_view.clone();
    let vacant = crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1 {
        reserved_daa: prior.daa_score,
        deposit: 1,
        ..Default::default()
    };
    for owner in [bond, other_bond] {
        reserved.legacy_disputes.entry(claim).or_default().live.insert(owner, vacant);
    }
    assert!(reserved.unused_reservation_after_replay(claim, bond));
    assert!(!reserved.unused_reservation_after_replay(claim, other_bond));
    reserved.legacy_disputes.get_mut(&claim).unwrap().live.get_mut(&bond).unwrap().sessions_opened = 1;
    assert!(!reserved.unused_reservation_after_replay(claim, bond));
    let row = reserved.legacy_disputes.get_mut(&claim).unwrap().live.get_mut(&bond).unwrap();
    *row = vacant;
    row.reserved_daa = bound_point.daa_score;
    assert!(!reserved.unused_reservation_after_replay(claim, bond));
    reserved.legacy_disputes.get_mut(&claim).unwrap().live.insert(bond, vacant);
    reserved.set_open_courts_for_tests(claim, 1);
    assert!(!reserved.unused_reservation_after_replay(claim, bond));

    assert!(protected_full_replay_digest_v1(&voted, claim, bond, key).is_none(), "included approval stops retransmission");
    assert_eq!(
        vote_delta
            .entries
            .iter()
            .filter(|entry| matches!(entry, super::PalwDeltaEntryV2::KernelRouteRow { table: PUBLIC_REPLAY_APPROVAL_TABLE_V1, .. }))
            .count(),
        1,
        "duplicate carrier does not add another approval"
    );
    let encoded = borsh::to_vec(&PalwStateCarriageV2::from_state(&voted)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(params, Some(voted.state_root())).unwrap();
    assert_eq!(restored.public_replay_approval_record_v1(original, panel_bond_v1(bond)).unwrap(), Some(*expected));
    assert!(protected_replay_approval_recorded_v1(&restored, claim));
    use crate::palw_protected_original_readback_v1::protected_original_readback_v1;
    let observed = protected_original_readback_v1(&restored, claim).unwrap().unwrap();
    assert!(observed.full_replay_approval_recorded);
    assert_eq!(
        observed.carrier_transaction.as_bytes(),
        restored.public_tfp1_frame_v1(claim).unwrap().unwrap().origin_v1().carrier_transaction.0
    );

    let reverted = revert_delta_v2(&voted, &vote_delta, params).unwrap();
    assert!(!protected_replay_approval_recorded_v1(&reverted, claim), "reverting inclusion removes approval authority");
    assert_eq!(duties_in_phase(&reverted, licensed_phase).len(), 1, "restart or reorg can reconstruct the missing full-replay work");

    let reorged = protected_original_readback_v1(&reverted, claim).unwrap().unwrap();
    assert!(!reorged.full_replay_approval_recorded);
    assert_eq!(reorged.carrier_transaction, observed.carrier_transaction);
    assert_eq!(reorged.accepting_block, observed.accepting_block);

    assert_eq!(protected_full_replay_digest_v1(&reverted, claim, bond, key), Some(digest), "reorg restores retransmission");
    assert_eq!(protected_full_replay_digest_bound_v2(&reverted, binding, bond, key), Some(digest), "same origin may retransmit");

    let network = voted.public_tfp1_frame_v1(claim).unwrap().unwrap().original_v1().job.network_domain;
    let signed_daa = point.daa_score + 1;
    let verdict = crate::palw_panel_v2::PalwReceiptVerdictV2::Valid;
    let message = crate::palw_panel_v2::palw_receipt_message_v2(network, claim, verdict, signed_daa);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &keys.signing_key,
        message.as_byte_slice(),
        crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
        [0; 32],
    )
    .unwrap();
    let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
        claim,
        verdict,
        seat_bond: bond,
        signed_daa,
        signature: signature.as_ref().to_vec(),
    };
    // Exercise the exact validator used by the collector, not only the fold's
    // direct licence object. This historical panel configuration still needs
    // three Valid receipts for an unprotected claim.
    use crate::palw_panel_v2::{PalwPanelParamsV2, PalwPanelV2Error, PalwReceiptQuorumV2};
    let panel_params = PalwPanelParamsV2::new(5, 3, 1).unwrap();
    assert!(!params.model_panel_one_governs_v1(voted.claim(&claim).unwrap().accepted_daa));
    let receipt_point = PalwBlockContextV2 { daa_score: signed_daa, ..point };
    let validate = |receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2]| {
        crate::palw_panel_v2::validate_receipt_quorum_v2_with_policy(
            &voted,
            &panel_params,
            params,
            &receipt_point,
            network,
            &claim,
            receipts,
            crate::palw_attempt_material_v3::verify_ready_signature_v3,
            true,
            None,
        )
    };
    assert!(matches!(validate(std::slice::from_ref(&receipt)), Ok(PalwReceiptQuorumV2::Licensed { valid: 1 })));
    assert!(matches!(validate(&[]), Err(PalwPanelV2Error::NoQuorum { .. })));
    let mut forged = receipt.clone();
    forged.signature[0] ^= 1;
    assert!(matches!(validate(&[forged]), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));
    assert!(matches!(validate(&[receipt.clone(), receipt.clone()]), Err(PalwPanelV2Error::DuplicateSeat(_))));
    let mut foreign = receipt.clone();
    foreign.claim = Hash64::from_u64_word(0x7199);
    assert!(matches!(validate(&[foreign]), Err(PalwPanelV2Error::ReceiptClaimMismatch { .. })));
    let vertex_params = params.clone().with_vertex_from_daa(Some(0));
    assert!(vertex_params.vertex_active_at(signed_daa));
    assert!(
        !crate::palw_vertex_v1::palw_vertex_claim_licenses_by_tally_v1(&voted, &vertex_params, signed_daa, &claim),
        "the protected receipt must reach its licence even when the legacy vertex lane is active"
    );
    exercise_final_and_receipt_fold_v1(&voted, &reverted, params, &following, claim, receipt);
}

/// Continue this same paired claim through the real licence/Final/spend fold.
/// The header's lottery and signature validation remain the processor's duty;
/// this exercise proves neither a winning public ticket nor a submitted block.
#[cfg(test)]
fn exercise_final_and_receipt_fold_v1(
    approved: &PalwChainStateV2,
    unapproved: &PalwChainStateV2,
    params: &super::PalwStateParamsV2,
    extras: &super::PalwTransitionExtrasV1,
    claim: Hash64,
    receipt: crate::palw_panel_v2::PalwSeatReceiptV2,
) {
    use super::{
        PalwBlockContextV2, PalwBlockWorkV3, PalwClaimPhaseV2, PalwClaimSourceV2, PalwConsensusObjectV2, PalwStateCarriageV2,
    };
    use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
    let parent_point = *approved.last_point().unwrap();
    let origin = approved.claim(&claim).unwrap().accepted_daa;
    let notice = origin + crate::palw_court_policy_v1::NOTICE_WINDOW_DAA;
    let hard = origin + crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA;
    let point = |step, daa| PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x7200 + step),
        daa_score: daa,
        blue_score: parent_point.blue_score + step,
        subsidy: 0,
    };
    let fold = |state: &PalwChainStateV2, at, objects: &[PalwConsensusObjectV2], work| {
        super::apply_palw_transition_v7(
            state,
            params,
            None,
            &at,
            objects,
            work,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            extras,
        )
    };
    let licence_point = point(1, receipt.signed_daa);
    let licence = PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: vec![receipt] };
    let (licensed, _, _) = fold(approved, licence_point, std::slice::from_ref(&licence), PalwBlockWorkV3::None).unwrap();
    let licensed_claim = licensed.claim(&claim).unwrap();
    assert!(matches!(licensed_claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(licensed_claim.rcore.licence_door, Some(PalwLicenceDoorTagV1::ProtectedPanelReplay));
    assert_eq!(licensed_claim.rcore.basis_k, 1, "one actual signer, not a forged historical two-replay recount");
    assert!(!super::palw_rcore_licence_awaits_replay_v1(licensed_claim));
    assert_eq!(super::palw_rcore_upgraded_door_v1(&licensed, &claim), PalwLicenceDoorTagV1::ProtectedPanelReplay);
    let reader = super::TransitionBuilder::try_new(&licensed, params, false, false, false, false, extras).unwrap();
    let one_lock = reader.read().rcore_lock(&claim, licensed_claim, 1);
    assert!(one_lock > 0);
    assert_eq!(
        reader.read().rcore_lock(&claim, licensed_claim, 3),
        one_lock,
        "additional receipts cannot dilute the one-signer protected liability"
    );
    assert_eq!(reader.read().rcore_lock_required_v1(licensed_claim, 1), reader.read().rcore_lock_required_v1(licensed_claim, 3));
    PalwStateCarriageV2::from_state(&licensed).into_state(params, Some(licensed.state_root())).unwrap();
    let mut wrong_door = licensed.clone();
    wrong_door.claims.get_mut(&claim).unwrap().rcore.licence_door = Some(PalwLicenceDoorTagV1::Quorum);
    assert!(PalwStateCarriageV2::from_state(&wrong_door).into_state(params, None).is_err());

    let (waiting, _, _) = fold(unapproved, licence_point, &[licence], PalwBlockWorkV3::None).unwrap();
    let (waiting, _, _) = fold(&waiting, point(3, notice + 1), &[], PalwBlockWorkV3::None).unwrap();
    assert!(
        matches!(waiting.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "a Valid receipt alone cannot replace the included full-replay approval"
    );
    let (expired, _, _) = fold(&waiting, point(4, hard + 1), &[], PalwBlockWorkV3::None).unwrap();
    assert!(matches!(
        expired.claim(&claim).unwrap().phase,
        PalwClaimPhaseV2::Voided { reason: super::PalwVoidReasonV2::PanelUnavailable, .. }
    ));
    assert!(matches!(&expired.claim(&claim).unwrap().source, PalwClaimSourceV2::FreePrompt { spent, .. } if spent.is_empty()));

    let (at_notice, _, _) = fold(&licensed, point(2, notice), &[], PalwBlockWorkV3::None).unwrap();
    assert!(matches!(at_notice.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let (finalized, _, _) = fold(&at_notice, point(3, notice + 1), &[], PalwBlockWorkV3::None).unwrap();
    assert!(matches!(finalized.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    let restored = PalwStateCarriageV2::from_state(&finalized).into_state(params, Some(finalized.state_root())).unwrap();
    assert_eq!(restored.claim(&claim).unwrap().accepted_block, approved.claim(&claim).unwrap().accepted_block);
    assert_eq!(restored.claim(&claim).unwrap().execution_root, approved.claim(&claim).unwrap().execution_root);
    let producer = finalized.claim(&claim).unwrap().bond;
    let spend_point = point(4, notice + 2);
    let network = finalized.public_tfp1_frame_v1(claim).unwrap().unwrap().original_v1().job.network_domain;
    let spend = crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
        version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain: network,
        challenge: crate::palw_freeprompt_v3::spend_challenge_v3(network, point(3, notice + 1).block, 1_700, 7, claim, 0, &producer.0),
        claim_id: claim,
        quantum_index: 0,
        beacon_block: Hash64::from_u64_word(0x7300),
        producer_bond: producer.0,
        producer_pubkey: finalized.bond(&producer).unwrap().pubkey.clone(),
    };
    let (spent, delta, skipped) = fold(&finalized, spend_point, &[], PalwBlockWorkV3::ReceiptSpend(&spend)).unwrap();
    assert!(skipped.is_empty());
    assert!(matches!(&spent.claim(&claim).unwrap().source, PalwClaimSourceV2::FreePrompt { spent, .. } if spent.contains(&0)));
    assert_eq!(super::apply_delta_v2(&finalized, &delta, params).unwrap().state_root(), spent.state_root());
    let reverted = super::revert_delta_v2(&spent, &delta, params).unwrap();
    assert_eq!(reverted.state_root(), finalized.state_root());
    // The RPC observation must identify this exact consumed index and clear
    // it again after undo. Neither a submitted header nor a total spend count
    // establishes that its named quantum is consumed at the observed tip.
    let view = |state: &PalwChainStateV2| {
        crate::palw_user_views_v1::palw_claim_detail_json_v1(state, params, &claim, &Default::default(), None).unwrap()
    };
    let before_view = view(&finalized);
    let spent_view = view(&spent);
    assert_eq!(before_view["receiptQuanta"]["spentIndices"], serde_json::json!([]));
    assert_eq!(spent_view["receiptQuanta"]["spentIndices"], serde_json::json!([spend.quantum_index]));
    assert_eq!(spent_view["claim"]["quantaSpent"], 1);
    assert_eq!(spent_view["receiptQuanta"]["spentCount"], 1);
    assert_eq!(spent_view["receiptQuanta"]["spentIndicesComplete"], true);
    assert_eq!(spent_view["receiptQuanta"]["total"], spent_view["claim"]["quanta"]);
    assert_eq!(spent_view["tipBlock"], spend_point.block.to_string());
    assert_eq!(spent_view["protectedOriginal"]["claim"], claim.to_string());
    assert_eq!(spent_view["protectedOriginal"]["executionRoot"], finalized.claim(&claim).unwrap().execution_root.to_string());
    assert_eq!(spent_view["protectedOriginal"]["fullReplayApprovalRecorded"], true);
    assert_eq!(view(&reverted), before_view);
    assert!(matches!(fold(&spent, point(5, notice + 3), &[], PalwBlockWorkV3::ReceiptSpend(&spend)),
        Err(super::PalwStateV2Error::QuantumAlreadySpent { claim: id, index: 0 }) if id == claim));
    assert!(fold(&finalized, point(5, hard + 1), &[], PalwBlockWorkV3::ReceiptSpend(&spend)).is_err());
}
