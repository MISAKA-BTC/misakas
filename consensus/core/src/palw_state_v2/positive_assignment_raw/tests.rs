//! Structural raw-hook/rollback guards only. Dummy scripts below are never
//! represented as real Full native transaction validation or accepted intake.
use super::*;
use crate::{
    config::params::*,
    palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1},
    tx::{ScriptPublicKey, TransactionInput, TransactionOutput},
};
use misaka_palw_positive_contract::{
    assignment::{AssignmentId, AssignmentModeV2, policy::*},
    rows::NativeHash64,
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn policy(positive: &PositiveCourtFenceV2) -> AssignmentPolicyV2 {
    AssignmentPolicyV2 {
        version: 2,
        revision: 1,
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        positive_policy: Digest(positive.policy.root().as_bytes()),
        derivation_policy: canonical_assignment_derivation_id(),
        source_policy: d(11),
        assignment_fee_schedule: d(41),
        case_fee_schedule: d(42),
        reserve_schedule: d(12),
        physical_service_profile: d(43),
        cancellation_policy: d(18),
        live: AssignmentLiveCapsV2 { global: 10, per_poster: 2, per_worker: 2, per_prover: 2, per_class: 4 },
        max_frame_bytes: 32_000,
        max_program_bytes: 65_536,
        max_layout_entries: 1000,
        max_source_catalog_entries: 1000,
        max_role_candidates: 5,
        max_prover_joins: 2,
        max_history_frames: 100,
        max_history_bytes: 100_000,
        max_retained_bytes: 200_000,
        max_replay_bytes: 300_000,
        max_replay_work: 1_000_000,
        max_heap_bytes: 1_000_000,
        accept_window_daa: 100,
        result_window_daa: 100,
        build_window_daa: 100,
        check_window_daa: 100,
        fraud_tail_daa: 100,
        refund_window_daa: 100,
        retention_window_daa: 1000,
    }
}
fn selected(at: u64) -> Params {
    let mut p = TESTNET_PARAMS;
    let positive = PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(at));
    let assignment =
        PositiveAssignmentFenceV2 { activation: ForkActivation::new(at), policy: policy(&positive), native_preimages: None };
    p.palw_positive_court_v2 = Some(positive);
    p.palw_positive_assignment_v2 = Some(assignment);
    p
}

fn action(ordinal: u32) -> PositiveAssignmentRawActionV2 {
    let mut payload = vec![1, 0, 176, 2, 0, 1, 0, 11];
    payload.extend_from_slice(&[42; 64]);
    action_payload(ordinal, payload)
}
fn action_payload(ordinal: u32, payload: Vec<u8>) -> PositiveAssignmentRawActionV2 {
    let params = selected(0);
    let rules = NativeAssignmentRulesV2::resolve(&params, 1, Hash64::from_bytes([1; 64]), PalwPromptIdsFormV1::Flat).unwrap();
    let spk = ScriptPublicKey::from_vec(0, vec![]);
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_bytes([ordinal as u8; 64]), 0), vec![], 0, 0)],
        vec![TransactionOutput::new(9, spk.clone())],
        0,
        crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([5; 64]),
        carrier_block: NativeHash64([6; 64]),
        carrier_transaction: NativeHash64(tx.id().as_bytes()),
        accepted_tx_ordinal: ordinal,
        event_ordinal: 0,
        accepted_daa: 1,
        accepted_blue_score: 1,
    };
    let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 1_000_000, database_bytes: 1_000_000 });
    PositiveAssignmentRawActionV2::from_native_transport_v2(
        vec![tx].into(),
        0,
        vec![UtxoEntry::new(10, spk, 0, false)],
        1,
        origin,
        Hash64::from_bytes([7; 64]),
        None,
        rules,
        checker.reserve_requests_v1(100_000, 0).unwrap(),
        checker.reserve_requests_v1(100_000, 0).unwrap(),
    )
    .unwrap()
}
fn point() -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Hash64::from_bytes([5; 64]), daa_score: 1, blue_score: 1, subsidy: 0 }
}
fn state_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_bytes([1; 64]), 4, 1000, 100, 1000, 0).unwrap()
}

#[test]
fn pending_relay_archive_transport_requires_its_exact_parent_and_earlier_block() {
    let relay = || action_payload(2, vec![1, 0, 176, 2, 0, 1, 0, 14]);
    let carrier = relay();
    let original = AcceptedOrigin { accepting_block: NativeHash64([9; 64]), accepted_tx_ordinal: 1, ..carrier.origin() };
    let replay = NativePendingRelayReplayV2 {
        selected_parent: carrier.selected_parent(),
        reservation: AcceptedMaterial { origin: original, digest: d(10) },
        fees: FundingRelayCarrierFeesV2::new(7, 1, 5, 2).unwrap(),
        physical: FundingRelayPhysicalProfileV2 {
            version: 2,
            revision: 1,
            max_frame_bytes: 32_000,
            max_decoded_bytes: 32_000,
            max_heap_bytes: 128_000,
            max_retained_bytes: 64_000,
            max_database_bytes: 64_000,
            max_batch_bytes: 64_000,
            max_replay_reads: 100,
            max_replay_work: 1_000_000,
            max_signatures: 2,
        },
    };
    assert!(action(2).with_pending_relay_replay_v2(replay).is_err());
    assert!(relay().with_pending_relay_replay_v2(NativePendingRelayReplayV2 { selected_parent: point().block, ..replay }).is_err());
    let same_block =
        NativePendingRelayReplayV2 { reservation: AcceptedMaterial { origin: carrier.origin(), ..replay.reservation }, ..replay };
    assert!(relay().with_pending_relay_replay_v2(same_block).is_err());
    let attached = carrier.with_pending_relay_replay_v2(replay).unwrap();
    assert_eq!(attached.pending_relay_replay_v2(), Some(replay));
    assert!(attached.with_pending_relay_replay_v2(replay).is_err());
}

#[test]
fn rejected_raw_action_restores_exact_canonical_state_and_delta() {
    let state = PalwChainStateV2::genesis();
    let params = state_params();
    let extras = PalwTransitionExtrasV1::default();
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let before = builder.state.state_root_preimage();
    let entries = builder.entries.len();
    let error = builder.apply_positive_assignment_raw_v2(&point(), &action(1)).unwrap_err();
    assert!(format!("{error}").contains("missing actual") || format!("{error}").contains("MissingKernelRoute"));
    assert_eq!(builder.state.state_root_preimage(), before);
    assert_eq!(builder.entries.len(), entries);
}
#[test]
fn final_raw_schedule_rejects_reverse_ordinals_without_state_change() {
    let state = PalwChainStateV2::genesis();
    let params = state_params();
    let extras = PalwTransitionExtrasV1 {
        positive_assignment_actions: vec![
            PositiveAssignmentScheduledActionV2 { before_object: 0, action: action(2) },
            PositiveAssignmentScheduledActionV2 { before_object: 0, action: action(1) },
        ],
        ..Default::default()
    };
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let before = builder.state.state_root_preimage();
    let mut cursor = 0;
    let mut previous = None;
    assert!(
        super::super::positive_assignment::apply_raw_before_object_v2(&mut builder, &point(), 0, &mut cursor, &mut previous).is_err()
    );
    assert_eq!(builder.state.state_root_preimage(), before);
    assert!(builder.entries.is_empty());
}
#[test]
fn final_raw_schedule_rejects_a_slot_behind_current_prefix() {
    let state = PalwChainStateV2::genesis();
    let params = state_params();
    let extras = PalwTransitionExtrasV1 {
        positive_assignment_actions: vec![PositiveAssignmentScheduledActionV2 { before_object: 0, action: action(1) }],
        ..Default::default()
    };
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let mut cursor = 0;
    let mut previous = None;
    assert!(
        super::super::positive_assignment::apply_raw_before_object_v2(&mut builder, &point(), 1, &mut cursor, &mut previous).is_err()
    );
    assert!(builder.entries.is_empty());
}

#[test]
fn old_transition_wrapper_cannot_silently_drop_raw_assignment_actions() {
    let state = PalwChainStateV2::genesis();
    let params = state_params();
    let extras = PalwTransitionExtrasV1 {
        positive_assignment_actions: vec![PositiveAssignmentScheduledActionV2 { before_object: 0, action: action(1) }],
        ..Default::default()
    };
    let error = super::super::apply_palw_transition_retained_v7(
        &state,
        &params,
        None,
        &point(),
        &[],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .err()
    .expect("v7 has no owned new-material output");
    assert!(format!("{error}").contains("require v8"));
}

#[test]
fn missing_native_history_aborts_without_advancing_the_raw_schedule() {
    use crate::palw_positive_court_v2::coordinator::native_body::{PositiveNativeBodyV2, PositiveNativeOperationV2};
    use misaka_palw_positive_contract::identity::{CaseId, ResultId};
    let mut payload = vec![1, 0, 176];
    payload.extend_from_slice(
        &borsh::to_vec(&PositiveNativeBodyV2 {
            version: 2,
            revision: 1,
            operation: PositiveNativeOperationV2::Expire { case: CaseId(d(42)), result: ResultId(d(43)) },
        })
        .unwrap(),
    );
    let state = PalwChainStateV2::genesis();
    let params = state_params();
    let extras = PalwTransitionExtrasV1 {
        positive_assignment_actions: vec![PositiveAssignmentScheduledActionV2 {
            before_object: 0,
            action: action_payload(1, payload),
        }],
        ..Default::default()
    };
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let before = builder.state.state_root_preimage();
    let mut cursor = 0;
    let mut previous = None;
    let error = super::super::positive_assignment::apply_raw_before_object_v2(&mut builder, &point(), 0, &mut cursor, &mut previous)
        .unwrap_err();
    assert!(matches!(error, PalwStateV2Error::NativeContextUnavailable(_)));
    assert_eq!(cursor, 0);
    assert_eq!(previous, None);
    assert_eq!(builder.state.state_root_preimage(), before);
    assert!(builder.entries.is_empty());
}
