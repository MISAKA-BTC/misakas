//! **`palw_panel`'s `seat_r_tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

//! **SEAT-R (ADR-0152 v3.1, Q-7, with the F2 review's per-mask rule), pinned where it lives and
//! run on the fixtures it judges.** Wherever F2's `palw_offence_attribution` is scheduled, past
//! Verification V2, a seat signs a `Valid` only for a mask whose every leaf it recomputed against
//! hashes the claim committed — a replay of the whole job — so no material, sample, interval, S3
//! site or unauthenticated S1 resume ends a verdict with `Valid`, and the filing refuses one no
//! replay licensed; outside C7 a partial seat, and on any class the claim's outsider, runs the same
//! replay and files the whole-job V2 beside its V3 (the liveness fallback until SEAT-S4); a
//! refuting replay stops every `Valid` and none of the full seat's fault finders; the duty lives
//! until the chain's per-claim receipt deadline; the V1 door is offered its quorum, rotated; and
//! the replays run off the loop, retried once, detached when their claim leaves, timed per class,
//! the claim's own job first, never all held by C7 replays running or detached. Below either fence
//! every arm, form and deadline is what it was.
use super::{
    PalwMaterialArmV1, PalwSeatArmV1, PalwSeatClassReadV1, PalwSeatClassRowsV1, PalwSeatRRoleV1, PalwSeatReceiptFormsV1,
    PalwSeatReplayPollV1, PalwSeatReplayStepV1, PalwSeatReplaysV1, SeatDutyPanelKeyV1, palw_attempt_material_arm_v1,
    palw_seat_arm_licenses_v1, palw_seat_claim_receipt_window_v1, palw_seat_duty_order_v1, palw_seat_is_outsider_v1,
    palw_seat_r_in_force_v1, palw_seat_r_role_v1, palw_seat_receipt_deadline_v1, palw_seat_receipt_forms_v1, palw_seat_replay_step_v1,
    palw_v1_offer_v1, prune_foreign_retention_v1, replay_licenses_v1, seat_duty_is_due_until_v1, seat_duty_panel_key_v1,
};
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_rc_shipped_params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1, PalwReplayRootsV1};
use kaspa_consensus_core::palw_model_registry_v1::{
    PalwDerivedProfileV1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, PalwModelRegistryClassReadV1, PalwModelRegistryReadV1,
    PalwModelWorkV1,
};
use kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1};
use kaspa_consensus_core::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2, palw_segment_count_v2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::HashSet;
use std::sync::Arc;

const NETWORK: &[u8] = b"misaka-palw-rc";
/// The two class ids the queue tests time replays under.
fn floor_class() -> Hash64 {
    h(0xF1)
}

fn two_m_class() -> Hash64 {
    h(0x2E)
}

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

/// The draw rule testnet-12 attempts are built and replayed under.
fn t12_draw() -> bool {
    palw_t12_shipped_params().palw_prefill_draw_active_at(0)
}

/// The floor, as base0's `seat0_review_regressions` resolves it.
fn floor_backend() -> misaka_palw_base0::backend::Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
    misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

/// The held A16 v7 row at a two-layer geometry, as base0's `seat0_review_regressions` builds it.
fn a16_geometry(vocab: u32) -> kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
    kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: vocab,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    }
}

fn a16_artifact(vocab: u32) -> Arc<misaka_palw_base0::artifact::Base0ArtifactV1> {
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    let g = a16_geometry(vocab);
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("sorted and unique"),
    )
}

fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &checkpoint_root, &step_root);
}

fn duty(claim: u64, bound_daa: u64, anchor: u64) -> PalwSeatDutyV2 {
    PalwSeatDutyV2 {
        accepted_block: h(1),
        claim_id: h(claim),
        class_id: h(2),
        artifact_root: h(3),
        seat_bond: PalwBondKeyV2(TransactionOutpoint::new(h(4), 0)),
        executor_bond: PalwBondKeyV2(TransactionOutpoint::new(h(5), 0)),
        execution_root: h(6),
        trace_root: h(7),
        output_root: h(8),
        bound_daa,
        receipt_deadline: bound_daa + 600,
        panel_anchor: h(anchor),
        seat_index: 0,
        panel_seat_count: 5,
        pwu: 1_000,
        quanta: 0,
        free_prompt: false,
        work_leaves: 0,
        job_identity: Hash64::default(),
        tir_shard: None,
    }
}

/// A registry row whose verification window is `spans` — the one field SEAT-R reads.
/// A row as the registry writes one: its work's `verification_ccu` is the one that derives
/// `spans` (`palw_verification_window_spans_v1`: `⌈2 × ccu / reference⌉` + one receipt span), so the
/// chain's `D(c)` — `5 × ` those spans past §4-quater, read from the work — agrees with its profile.
pub(super) fn lifecycle_row(spans: u32) -> PalwModelLifecycleRowV1 {
    use kaspa_consensus_core::palw_model_registry_v1::{PALW_REGISTRY_GLOBALS_V1 as G, palw_verification_window_spans_v1};
    let base = u128::from(spans.saturating_sub(G.receipt_allowance_spans).max(1));
    let work = PalwModelWorkV1 {
        verification_ccu: base * G.reference_work_per_span * 1_000 / u128::from(G.safety_permille),
        ..Default::default()
    };
    if spans > G.receipt_allowance_spans {
        assert_eq!(palw_verification_window_spans_v1(&work, &G), spans, "the work derives the spans");
    }
    PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: PalwDerivedProfileV1 { verification_window_spans: spans, max_inflight_claims: 1, ..Default::default() },
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    }
}

/// One class as op 186 reads it, rowed or not.
fn class_read(class_id: Hash64, row: Option<PalwModelLifecycleRowV1>) -> PalwModelRegistryClassReadV1 {
    PalwModelRegistryClassReadV1 {
        economic_ccu_per_claim: 0,
        work_ratio_permille: 0,
        expected_forwards_q32: 0,
        work_ticket_target: 0,
        class_target: 0,
        panel_room: 0,
        final_work_share_10_permille: 0,
        final_work_share_100_permille: 0,
        class_id,
        artifact_root: h(3),
        roots_in_force: Vec::new(),
        is_base_class: false,
        row,
        ready_seats_now: 0,
        seating: None,
        inflight_now: 0,
        share_permille: None,
        no_capable_panel_voids: 0,
        eligibility: Default::default(),
        reason: String::new(),
    }
}

/// Poll `key` until its replay returns.
async fn returned(replays: &mut PalwSeatReplaysV1, key: &(Hash64, Hash64), now_daa: u64) -> Result<PalwReplayRootsV1, String> {
    loop {
        match replays.poll(key, now_daa).await {
            PalwSeatReplayPollV1::Running => std::thread::sleep(std::time::Duration::from_millis(5)),
            PalwSeatReplayPollV1::Done { result, .. } => return result,
            PalwSeatReplayPollV1::Absent => panic!("a started replay is never absent"),
        }
    }
}

/// A replay that blocks until `gate` releases it, then refuses.
fn gated(
    gate: &Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
) -> impl FnOnce(&dyn PalwExecutionBackendV1) -> Result<PalwReplayRootsV1, String> + Send + 'static {
    let gate = gate.clone();
    move |_| {
        let _ = gate.lock().unwrap().recv();
        Err("held".into())
    }
}

/// **The fence is testnet-12's, from genesis, and it is both fences.** Every other preset keeps
/// every arm; a card with either fence alone keeps every arm too. And wherever F2 is scheduled,
/// SEAT-R holds from Verification V2 rather than from F2's height (the review, LOW): F2 judges a
/// `Valid` at the block that carries the accusation, a window and a court after it was signed.
#[test]
fn seat_r_is_in_force_on_testnet_12_from_genesis_and_nowhere_else() {
    use kaspa_consensus_core::config::params::{devnet_shipped_params, mainnet_shipped_params};
    let t12 = palw_t12_shipped_params();
    assert!(palw_seat_r_in_force_v1(&t12, 0), "testnet-12 from its first block");
    for (name, p) in
        [("mainnet", mainnet_shipped_params()), ("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
    {
        assert!(!palw_seat_r_in_force_v1(&p, 0) && !palw_seat_r_in_force_v1(&p, u64::MAX), "{name}: every arm as it was");
    }
    let mut v2_alone = t12.clone();
    v2_alone.palw_offence_attribution = None;
    assert!(!palw_seat_r_in_force_v1(&v2_alone, u64::MAX), "Verification V2 without F2's fence");
    let mut f2_alone = t12.clone();
    f2_alone.palw_verification_v2 = None;
    assert!(!palw_seat_r_in_force_v1(&f2_alone, u64::MAX), "F2's fence without Verification V2");
    let mut later = t12.clone();
    later.palw_offence_attribution = Some(ForkActivation::new(10_000));
    assert!(!later.palw_offence_attribution_active_at(9_999), "F2 is not yet in force…");
    assert!(palw_seat_r_in_force_v1(&later, 0) && palw_seat_r_in_force_v1(&later, 9_999), "…and a Valid signed now is judged past it");
    let mut never = t12;
    never.palw_offence_attribution = Some(ForkActivation::never());
    assert!(!palw_seat_r_in_force_v1(&never, u64::MAX), "a fence scheduled never is no fence");
}

/// **The one rule, as a table.** Below SEAT-R every arm licenses; past it the replays of the
/// claim's whole job (an attempt's, a free-prompt claim's, RFC-0004's evaluation claim's), and SEAT-S4's verified resume of a seat's own mask — not the legacy segment
/// resume, not S3 inside it, not the material, the capture, or either interval arm.
#[test]
fn past_seat_r_only_the_replays_license() {
    for arm in PalwSeatArmV1::ALL {
        assert!(palw_seat_arm_licenses_v1(false, arm), "{arm:?} below the fences, as today");
        assert_eq!(
            palw_seat_arm_licenses_v1(true, arm),
            matches!(
                arm,
                PalwSeatArmV1::AttemptReplay
                    | PalwSeatArmV1::FreePromptReplay
                    | PalwSeatArmV1::EvaluationReplay
                    | PalwSeatArmV1::VerifiedSegmentResume
            ),
            "{arm:?} past the fences"
        );
    }
}

/// One class as the class table reads it, registered at `registered_daa`.
fn table_row(class_id: Hash64, registered_daa: u64) -> kaspa_consensus_core::palw_state_v2::PalwClassRowV2 {
    kaspa_consensus_core::palw_state_v2::PalwClassRowV2 {
        class_id,
        status: "Active".into(),
        share_permille: None,
        budget_blocks: 0,
        canonical_leaves: 0,
        is_base_class: false,
        artifact_root: h(3),
        fp_certified: false,
        held: true,
        registered_daa,
    }
}

/// **(d) The roles, and the receipts each files for a replay's `Valid`.** The full seat replays
/// on every class; a partial seat replays outside C7, and on a C7 class only as the claim's
/// ADR-0147 outsider (the review, HIGH: every door of a bought class needs the outsider's
/// `Valid`, and the outsider is a partial seat four panels in five); on a C7 class otherwise it
/// resumes its own mask (SEAT-S4 re-enabled, `PartialResumes`), and on one this seat has not read
/// it files nothing. A partial seat whose mask is the whole job replays (L1). Every replaying
/// seat files its V3 over its assigned mask beside the whole-job V2. The class reading is the registry's own — the
/// 8k row (3 spans) and the 2M row (2,799) through `palw_panel_held_to_final_v1` — with the class
/// table's registration DAA for "bought"; a class the read does not list stays unread.
#[test]
fn a_partial_seat_replays_outside_c7_and_resumes_on_it_past_seat_r() {
    use PalwSeatRRoleV1::*;
    for held in [Some(true), Some(false), None] {
        for outsider in [true, false] {
            for full_mask in [true, false] {
                assert_eq!(
                    palw_seat_r_role_v1(true, held, outsider, full_mask),
                    FullSeat,
                    "the full seat replays whatever the class ({held:?})"
                );
            }
        }
    }
    assert_eq!(palw_seat_r_role_v1(false, Some(false), false, false), PartialReplays, "outside C7: the liveness fallback");
    assert_eq!(palw_seat_r_role_v1(false, Some(false), true, false), PartialReplays);
    assert_eq!(
        palw_seat_r_role_v1(false, Some(true), false, false),
        PartialResumes,
        "C7: its replay is the class's whole window, so it resumes its own mask (SEAT-S4)"
    );
    assert_eq!(palw_seat_r_role_v1(false, Some(true), true, false), PartialReplays, "C7's outsider: no door opens without it");
    for outsider in [true, false] {
        for full_mask in [true, false] {
            assert_eq!(
                palw_seat_r_role_v1(false, None, outsider, full_mask),
                PartialAbstains,
                "unread: nothing started on a guess about its size"
            );
        }
    }

    // The registry's reading of testnet-12's two model rows, a bought 2M-sized row, an unrowed
    // class, one not asked, and one asked that the read raced (not listed).
    let (short, heavy, bought, unrowed, stranger, raced) = (h(0x8C), h(0x2E), h(0xB0), h(0x0F), h(0x55), h(0x66));
    let read = PalwModelRegistryReadV1 {
        classes: vec![
            class_read(short, Some(lifecycle_row(3))),
            class_read(heavy, Some(lifecycle_row(2_799))),
            class_read(bought, Some(lifecycle_row(2_799))),
            class_read(unrowed, None),
            class_read(stranger, None),
        ],
        ..Default::default()
    };
    let table = vec![table_row(short, 0), table_row(heavy, 0), table_row(bought, 4_096), table_row(unrowed, 0)];
    let mut rows = PalwSeatClassRowsV1::default();
    let now = std::time::Instant::now();
    assert!(rows.wants_read([short].iter(), now), "never read");
    rows.absorb(Some(&read), &table, 0, &[short, heavy, bought, unrowed, raced], now);
    assert_eq!(rows.class(&short), PalwSeatClassReadV1::Rowed { verification_window_spans: 3, held_to_final: false, bought: false });
    assert_eq!(
        rows.class(&heavy),
        PalwSeatClassReadV1::Rowed { verification_window_spans: 2_799, held_to_final: true, bought: false },
        "a genesis row has no registrant"
    );
    assert_eq!(
        rows.class(&bought),
        PalwSeatClassReadV1::Rowed { verification_window_spans: 2_799, held_to_final: true, bought: true },
        "registered after genesis: by a registrant bond"
    );
    assert_eq!(rows.class(&unrowed), PalwSeatClassReadV1::Unrowed);
    assert_eq!(rows.class(&stranger), PalwSeatClassReadV1::Unread, "an unrowed class nobody asked about is not kept");
    assert_eq!(rows.class(&raced), PalwSeatClassReadV1::Unread, "(the review, LOW) not listed: unread, never outside C7");
    assert_eq!(palw_seat_r_role_v1(false, rows.class(&raced).held_to_final(), false, false), PartialAbstains);
    assert_ne!(rows.class(&raced).held_to_final(), Some(false), "its replay would never take a light slot");
    assert_eq!(palw_seat_r_role_v1(false, rows.class(&short).held_to_final(), false, false), PartialReplays);
    assert_eq!(palw_seat_r_role_v1(false, rows.class(&heavy).held_to_final(), false, false), PartialResumes);
    assert_eq!(
        palw_seat_r_role_v1(false, rows.class(&unrowed).held_to_final(), false, false),
        PartialReplays,
        "no row: C7 cannot select it"
    );
    assert!(!rows.wants_read([short, heavy, bought].iter(), now), "a row is its class's for life");
    assert!(!rows.wants_read([unrowed].iter(), now + PalwSeatClassRowsV1::RETRY), "an unrowed class waits out the re-read");
    assert!(rows.wants_read([unrowed].iter(), now + PalwSeatClassRowsV1::REREAD), "…and is asked again after it");
    assert!(!rows.wants_read([raced].iter(), now), "a class not yet read waits for the retry…");
    assert!(rows.wants_read([raced].iter(), now + PalwSeatClassRowsV1::RETRY), "…not for the re-read");
    let mut unanswered = PalwSeatClassRowsV1::default();
    unanswered.absorb(None, &table, 0, &[short], now);
    assert_eq!(unanswered.class(&short), PalwSeatClassReadV1::Unread, "a read that did not answer reads nothing");
    let mut untabled = PalwSeatClassRowsV1::default();
    untabled.absorb(Some(&read), &[], 0, &[heavy], now);
    assert!(
        matches!(untabled.class(&heavy), PalwSeatClassReadV1::Rowed { bought: true, .. }),
        "a row the table does not list is read as bought: the outsider's replay is the price of the doubt"
    );

    // The outsider: seat 0, of a bought class's claim, past `palw_admission_independence`
    // (testnet-12 arms it at genesis).
    let t12 = palw_t12_shipped_params();
    assert!(t12.palw_admission_independence_at(120));
    let seat = |index: u8| PalwSeatDutyV2 { seat_index: index, ..duty(9, 120, 0xA1) };
    assert!(palw_seat_is_outsider_v1(&t12, &seat(0), rows.class(&bought)), "seat 0 of a bought class");
    assert!(!palw_seat_is_outsider_v1(&t12, &seat(1), rows.class(&bought)), "any other seat");
    assert!(!palw_seat_is_outsider_v1(&t12, &seat(0), rows.class(&heavy)), "a genesis class has no outsider");
    assert!(!palw_seat_is_outsider_v1(&t12, &seat(0), rows.class(&raced)), "nor one not yet read");
    let mut independent_later = t12.clone();
    independent_later.palw_admission_independence = Some(ForkActivation::new(1_000));
    assert!(!palw_seat_is_outsider_v1(&independent_later, &seat(0), rows.class(&bought)), "bound before the fence");

    // What each role files for a replay's `Valid`, on a five-seat panel.
    let assignment = palw_segment_assignment_v2(h(0xA1), h(9), 5);
    let full = PalwSegmentMaskV2::full(palw_segment_count_v2(5));
    assert_eq!(assignment.mask_of(assignment.full_seat), full, "the full seat's mask is the whole job");
    let partial = (0..5u16).find(|i| *i != assignment.full_seat).unwrap();
    let segment = assignment.mask_of(partial);
    assert_eq!(
        palw_seat_receipt_forms_v1(true, true, true, FullSeat, full),
        PalwSeatReceiptFormsV1 { segments: Some(full), whole: true }
    );
    assert_eq!(
        palw_seat_receipt_forms_v1(true, true, true, PartialReplays, segment),
        PalwSeatReceiptFormsV1 { segments: Some(segment), whole: true },
        "its replay recomputed every leaf of its segment too"
    );
    assert_eq!(
        palw_seat_receipt_forms_v1(true, true, true, PartialAbstains, segment),
        PalwSeatReceiptFormsV1 { segments: None, whole: false },
        "a Valid in a role that files nothing files nothing"
    );
}

/// **(c) Below the fences every verdict files the receipt it always did** — the V2 receipt below
/// Verification V2, the one V3 over the seat's mask past it — and past SEAT-R so does every verdict
/// that is not a `Valid` (an `Incapable`, the half-window `Unavailable`).
#[test]
fn below_seat_r_every_verdict_files_the_receipt_it_always_did() {
    use PalwSeatRRoleV1::*;
    let mask = PalwSegmentMaskV2::single(2);
    for role in [FullSeat, PartialReplays, PartialAbstains] {
        for valid in [true, false] {
            for seat_r in [true, false] {
                assert_eq!(
                    palw_seat_receipt_forms_v1(false, seat_r, valid, role, mask),
                    PalwSeatReceiptFormsV1 { segments: None, whole: true },
                    "below Verification V2"
                );
            }
            assert_eq!(
                palw_seat_receipt_forms_v1(true, false, valid, role, mask),
                PalwSeatReceiptFormsV1 { segments: Some(mask), whole: false },
                "past Verification V2, below SEAT-R ({role:?}, valid {valid})"
            );
        }
        assert_eq!(
            palw_seat_receipt_forms_v1(true, true, false, role, mask),
            PalwSeatReceiptFormsV1 { segments: Some(mask), whole: false },
            "past SEAT-R, a verdict that is not Valid ({role:?})"
        );
    }
}

/// **(a) Past the fences an honest floor claim is licensed by the replay, and licenses through
/// the V1 door with the partial seats' fallback — and through the coverage door beside it.** The
/// floor's replay reproduces the claim (the seat's `Valid`); every seat of the floor's panel
/// replays (the floor is outside C7) and files its V3 over its assigned mask beside the whole-job
/// V2; and on a real bound five-seat panel with real ML-DSA-87 keys, three partial seats'
/// whole-job V2 `Valid`s — admitted by a collector's receipt pool, own and heard — license
/// through the V1 validator with no full seat among them (the door takes any seat's V2 whatever
/// its S1 role), and the five V3s license through coverage. Without the fallback the full seat is
/// alone and neither door opens; and without the full seat's whole-job V2 its V3 counts for
/// nothing at the V1 door, since its inner half is signed over the V3 message.
#[test]
fn past_seat_r_a_floor_claim_is_licensed_by_the_replay_and_through_the_v1_door() {
    use crate::palw_receipt_pool::tests::Chain;
    use crate::palw_receipt_pool::{ArrivedReceiptV1, PalwReceiptPoolV1, VerifyBudgetV1, admit_receipt_arrivals_v1};
    use kaspa_consensus_core::palw_panel_v2::{PalwSeatReceiptV2, PalwSeatReceiptV3};

    // The seat's verdict: the floor's replay of the anchor's job reproduces the claim.
    let backend = floor_backend();
    let anchor = h(0x00C0_FFEE);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, t12_draw());
    let claim = backend.execute(&job, &prompt).expect("the producer's run");
    let replayed = backend.execute_for_verdict(&job, &prompt);
    assert_eq!(
        palw_seat_replay_step_v1(&replayed, claim.execution_root, claim.trace_root, 0, claim.output_root),
        PalwSeatReplayStepV1::Licensed,
        "the replay licenses the honest floor claim — its roots and its answer (SEAT-S2)"
    );

    let chain = Chain::new();
    let panel = chain.state.panel(&chain.claim).expect("bound");
    let assignment = palw_segment_assignment_v2(panel.anchor, chain.claim, chain.seats.len() as u16);
    let full = chain.seats[assignment.full_seat as usize];
    let partials: Vec<PalwBondKeyV2> = chain.seats.iter().copied().filter(|seat| *seat != full).collect();
    // The floor is outside C7: every seat replays, and every one files the whole-job V2 beside
    // its V3 over its own mask.
    let floor = PalwSeatClassReadV1::Unrowed;
    for (index, _) in chain.seats.iter().enumerate() {
        let role = palw_seat_r_role_v1(index as u16 == assignment.full_seat, floor.held_to_final(), index == 0, false);
        let mask = assignment.mask_of(index as u16);
        let forms = palw_seat_receipt_forms_v1(true, true, true, role, mask);
        assert_eq!(forms, PalwSeatReceiptFormsV1 { segments: Some(mask), whole: true }, "seat {index}");
    }

    // A collector: its own partial seat's V2 kept apart, two more heard over gossip.
    let facts = chain.facts();
    let mut v2 = PalwReceiptPoolV1::<PalwSeatReceiptV2>::new(chain.net);
    let mut v3 = PalwReceiptPoolV1::<PalwSeatReceiptV3>::new(chain.net);
    v2.insert_own(chain.v2(&partials[0], 1), 108);
    let heard = vec![ArrivedReceiptV1::V2(chain.v2(&partials[1], 2)), ArrivedReceiptV1::V2(chain.v2(&partials[2], 3))];
    let mut budget = VerifyBudgetV1::per_tick();
    let kept: HashSet<Hash64> = [chain.claim].into();
    admit_receipt_arrivals_v1(heard, &mut v2, &mut v3, &facts, &super::verify_receipt_signature_v1, &mut budget, 110, &kept);
    let pool = v2.candidates(&chain.claim, &facts);
    assert_eq!(pool.len(), 3, "own first, then the two heard");
    assert!(chain.v2_licenses(&pool), "three partial seats' whole-job V2 Valids open the V1 door");

    // Every seat's V3 over its own mask: the coverage door, which the collector asks first.
    let every_v3: Vec<PalwSeatReceiptV3> = chain.seats.iter().enumerate().map(|(i, seat)| chain.v3(seat, 20 + i as u8)).collect();
    assert!(chain.v3_licenses(&every_v3), "five replays' V3s cover every segment twice");

    // Without the fallback: the full seat alone.
    assert!(!chain.v2_licenses(&[chain.v2(&full, 4)]), "one Valid is no quorum");
    assert!(!chain.v3_licenses(&[chain.v3(&full, 5)]), "coverage needs every segment attested twice");
    // The full seat's whole-job V2 is what makes it count at the V1 door.
    let inner = chain.v3(&full, 6).receipt;
    assert!(
        !chain.v2_licenses(&[inner, chain.v2(&partials[0], 7), chain.v2(&partials[1], 8)]),
        "a V3's inner half is signed over the V3 message"
    );
    assert!(chain.v2_licenses(&[chain.v2(&full, 9), chain.v2(&partials[0], 7), chain.v2(&partials[1], 8)]));
}

/// **(a) The V1 set a node offers past SEAT-R: its quorum, and a different one each time** (the
/// review, MEDIUM). Five seats' whole-job V2 `Valid`s on the real bound panel: the greedy
/// assembler would carry all five, and one seat short of its lock leaves the set inert in the
/// fold. `palw_v1_offer_v1` offers the shortest prefix the assembler licenses from — three — and
/// rotates by the sets already sent, so every seat is left out of some offer within five sends:
/// a seat that cannot post its lock is left out by the fourth send whatever its place — forty
/// DAA of re-plans on testnet-12, against the floor's 600-DAA window.
#[test]
fn past_seat_r_the_v1_door_is_offered_its_quorum_rotated() {
    use crate::palw_receipt_pool::tests::Chain;
    use kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2;
    let chain = Chain::new();
    let every: Vec<PalwSeatReceiptV2> = chain.seats.iter().enumerate().map(|(i, seat)| chain.v2(seat, 40 + i as u8)).collect();
    let asked = std::cell::Cell::new(0usize);
    // The assembler, as the processor runs it: the set it licenses from, or nothing.
    let assemble = |set: Vec<PalwSeatReceiptV2>| {
        asked.set(asked.get() + 1);
        chain.v2_licenses(&set).then_some(set)
    };
    let mut left_out: HashSet<PalwBondKeyV2> = HashSet::new();
    for sent in 0..5u32 {
        let offered = palw_v1_offer_v1(&every, sent, assemble).expect("five Valids hold a quorum");
        assert_eq!(offered.len(), 3, "the quorum and nothing past it (send {sent})");
        assert!(chain.v2_licenses(&offered));
        left_out.extend(chain.seats.iter().copied().filter(|seat| !offered.iter().any(|r| r.seat_bond == *seat)));
    }
    assert_eq!(left_out.len(), 5, "every seat is left out of some offer");
    for short in chain.seats.iter() {
        let sends = (0..5u32)
            .position(|sent| {
                !palw_v1_offer_v1(&every, sent, assemble).expect("a quorum").iter().any(|receipt| receipt.seat_bond == *short)
            })
            .expect("left out within five");
        assert!(sends <= 3, "a seat short of its lock is left out by the fourth send at the latest");
    }
    assert!(asked.get() > 0);
    assert!(palw_v1_offer_v1(&every[..2], 0, assemble).is_none(), "two Valids are no quorum, however offered");
    assert!(palw_v1_offer_v1::<PalwSeatReceiptV2, Vec<PalwSeatReceiptV2>>(&[], 3, assemble).is_none());
}

/// **(e) Past SEAT-R a duty stays due until the chain's per-claim deadline.** A 2M claim is
/// judged against `W_r(c)` = 13,995 DAA on testnet-12 (ADR-0152 §4-quater V3: 5 × its 2,799 spans),
/// not the global 600: its duty is due — and its receipt signed inside what acceptance takes —
/// until then, and never past it. The 8k row, the floor and an unrowed class keep 600; a class not
/// yet read (or a rowed reading without its row) keeps the duty's own deadline, never later than
/// the chain's; and below SEAT-R the duty's own deadline stands whatever the class.
#[test]
fn past_seat_r_a_duty_stays_due_until_the_chains_per_claim_deadline() {
    let p = palw_t12_shipped_params();
    let d = duty(9, 120, 0xA1);
    assert_eq!(d.receipt_deadline, 720);
    let two_m = PalwSeatClassReadV1::Rowed { verification_window_spans: 2_799, held_to_final: true, bought: false };
    let two_m_row = lifecycle_row(2_799);
    let deadline = palw_seat_receipt_deadline_v1(&p, true, &d, two_m, Some(&two_m_row));
    assert_eq!(deadline, 120 + 13_995);
    let mut answered: HashSet<SeatDutyPanelKeyV1> = HashSet::new();
    for daa in [120, 720, 721, 1_519, 120 + 2_799, 120 + 13_995] {
        assert!(seat_duty_is_due_until_v1(&d, &answered, daa, deadline), "due at {daa}");
        let signed = daa.clamp(d.bound_daa, deadline);
        assert!(signed >= d.bound_daa && signed <= deadline, "signed inside the chain's window at {daa}");
    }
    assert!(!seat_duty_is_due_until_v1(&d, &answered, 120 + 13_996, deadline), "never past the chain's");
    assert_eq!((deadline - d.bound_daa) / 2, 6_997, "the half-window tail reads the same window");
    answered.insert(seat_duty_panel_key_v1(&d));
    assert!(!seat_duty_is_due_until_v1(&d, &answered, 721, deadline), "and answered once");

    let eight_k = lifecycle_row(3);
    for (name, class, row) in [
        (
            "the 8k row",
            PalwSeatClassReadV1::Rowed { verification_window_spans: 3, held_to_final: false, bought: false },
            Some(&eight_k),
        ),
        ("an unrowed class", PalwSeatClassReadV1::Unrowed, None),
        ("a class not yet read", PalwSeatClassReadV1::Unread, None),
        ("a rowed reading without its row", two_m, None),
    ] {
        assert_eq!(palw_seat_receipt_deadline_v1(&p, true, &d, class, row), 720, "{name}: the global window");
    }
    for class in [two_m, PalwSeatClassReadV1::Unread] {
        assert_eq!(
            palw_seat_receipt_deadline_v1(&p, false, &d, class, Some(&two_m_row)),
            d.receipt_deadline,
            "below SEAT-R: the duty's own"
        );
    }
    // testnet-11 has no SEAT-R: whatever its class window, the seat reads the duty's own deadline.
    let t11 = palw_rc_shipped_params();
    assert!(!palw_seat_r_in_force_v1(&t11, u64::MAX));
    assert_eq!(
        palw_seat_receipt_deadline_v1(&t11, palw_seat_r_in_force_v1(&t11, 10_000), &d, two_m, Some(&two_m_row)),
        d.receipt_deadline
    );
}

/// **The seat's per-claim window IS the chain's** (ADR-0152 §4-quater N-1).
/// `palw_seat_claim_receipt_window_v1` against `PalwStateParamsV2::receipt_window_for_claim_v1` on
/// a real state holding one row, on testnet-12 (§11.3 and the class-verify deadline at genesis:
/// `W_r(c) = max(600, 5 × spans)` in reference spans) and testnet-11 (§11.3 alone, its own spans),
/// for no row and for windows either side of the global one, both lanes, at bound heights either
/// side of each fence — and the values testnet-12 gives, pinned: the node once computed
/// `spans × lane span` and gave the 2M row 2,799 DAA of the chain's 13,995.
#[test]
fn the_seats_per_claim_window_is_the_chains() {
    use kaspa_consensus_core::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1;
    use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2};
    let class = h(0x7C);
    let chain_window = |p: &Params, row: Option<&PalwModelLifecycleRowV1>, shape: PalwClaimVerifyShapeV1, bound: u64| -> u64 {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else {
            panic!("a ConsensusV2 preset")
        };
        let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        if let Some(row) = row {
            carriage.model_lifecycles.insert(class, row.clone());
        }
        let state = carriage.into_state(&bundle.state, None).expect("a state holding one row");
        bundle.state.receipt_window_for_claim_v1(&state, &class, shape, bound)
    };
    let shapes = [PalwClaimVerifyShapeV1::Attempt, PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 4_096 }];
    for (name, p) in [("testnet-12", palw_t12_shipped_params()), ("testnet-11", palw_rc_shipped_params())] {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else {
            panic!("{name} is ConsensusV2")
        };
        let fence = bundle.state.class_receipt_window_daa().expect("both arm ADR-0133 §11.3");
        let global = bundle.state.window_receipt();
        for spans in [None, Some(1), Some(3), Some(global as u32 - 1), Some(global as u32), Some(global as u32 + 1), Some(2_799)] {
            let row = spans.map(lifecycle_row);
            for bound in [fence.saturating_sub(1), fence, fence + 1, fence + 10_000] {
                for shape in shapes {
                    assert_eq!(
                        palw_seat_claim_receipt_window_v1(&p, &class, row.as_ref(), shape, bound),
                        Some(chain_window(&p, row.as_ref(), shape, bound)),
                        "{name}: {spans:?} spans bound at {bound}, {shape:?}"
                    );
                }
            }
        }
    }
    let t12 = palw_t12_shipped_params();
    let window = |spans: u32| palw_seat_claim_receipt_window_v1(&t12, &class, Some(&lifecycle_row(spans)), shapes[0], 0);
    assert_eq!(window(2_799), Some(13_995), "the 2M row: 5 × 2,799, not 2,799 × the 1-DAA lane span");
    assert_eq!(window(601), Some(3_005), "past the global window: 5 × spans");
    assert_eq!(window(3), Some(600), "the 8k row: the global window");
    assert_eq!(palw_seat_claim_receipt_window_v1(&t12, &class, None, shapes[1], 0), Some(600), "no row: the global window");
}

/// **(a), (c) on the floor.** An honest attempt's material reproduces its roots: below the
/// fences it licenses as today; past them it is kept and signs nothing — and the replay of the
/// anchor's job licenses the claim. Another claim's roots are nothing either way.
#[test]
fn the_floors_material_is_kept_past_seat_r_and_its_replay_licenses() {
    let backend = floor_backend();
    let anchor = h(0x00C0_FFEE);
    let draw = t12_draw();
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, draw);
    let claim = backend.execute(&job, &prompt).expect("the producer's run");
    let roots = PalwClaimRootsV1 {
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        anchor,
        attempt_draw: Some(draw),
        output_root: Some(claim.output_root),
        job_pin: None,
    };
    for arm in [PalwSeatArmV1::AttemptMaterial, PalwSeatArmV1::RetainedMaterial] {
        assert_eq!(
            palw_attempt_material_arm_v1(
                false,
                arm,
                kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(&backend),
                &claim.material,
                roots
            ),
            PalwMaterialArmV1::Licenses,
            "{arm:?}"
        );
        assert_eq!(
            palw_attempt_material_arm_v1(
                true,
                arm,
                kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(&backend),
                &claim.material,
                roots
            ),
            PalwMaterialArmV1::Evidence,
            "{arm:?}"
        );
        let other = PalwClaimRootsV1 { execution_root: h(0xBAD), ..roots };
        assert_eq!(
            palw_attempt_material_arm_v1(
                false,
                arm,
                kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(&backend),
                &claim.material,
                other
            ),
            PalwMaterialArmV1::Nothing
        );
        assert_eq!(
            palw_attempt_material_arm_v1(
                true,
                arm,
                kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(&backend),
                &claim.material,
                other
            ),
            PalwMaterialArmV1::Nothing
        );
    }
    let replayed = backend.execute_for_verdict(&job, &prompt).expect("the seat's replay runs");
    assert!(replay_licenses_v1(&replayed, claim.execution_root, claim.trace_root, 0), "the replay licenses the floor");
    assert!(!replay_licenses_v1(&replayed, h(0xBAD), claim.trace_root, 0), "and no other execution root");
}

/// **(b) Material matches, the replay does not: no `Valid` anywhere, the bytes kept — the audit's
/// residual, closed at the seat.** On the held A16 v7 row an attempt folds, the fold's selecting
/// row is not tied to its step tree, and one honest execution re-bent four ways gives four fresh
/// roots whose material reproduces them (base0's
/// `held_fold_attempt_bent_rows_are_licensed_by_material_and_refused_by_the_replay`). Below the
/// fences the material arm licenses every bend, as today; past them each is kept as evidence and
/// licenses nothing, and the replay licenses the honest claim and refutes every bend — which the
/// verdict block makes terminal for `Valid`, and hands to the full seat's fault finders.
#[test]
fn a_bent_held_fold_attempt_is_kept_as_evidence_and_refuted_by_the_replay_past_seat_r() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_step_refute::{base0_decode_token_select_v1, tiled_logits_trace_root_v1};
    use misaka_palw_base0::produce::{
        Base0FpMaterialV2, PALW_BASE0_FP_MATERIAL_MAGIC_V2, base0_fp_material_decode_v2, base0_fp_material_encode_v2,
    };
    use misaka_palw_base0::qwen25_a16_backend::{Qwen25A16Backend, a16_execute_free_prompt_streaming_v1};
    let artifact = a16_artifact(8_292);
    let profile = qwen25_a16_profile_v7(a16_geometry(8_292)).expect("held v7");
    assert!(kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&profile), "a held attempt folds");
    let backend =
        Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), qwen25_a16_held_canonical_v1(profile.n_ctx))
            .expect("servable")
            .with_step_ladder_cap(kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES)
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1);
    let plan = misaka_palw_base0::engine_a16::A16Engine::new(&artifact).expect("a16").plan_from_profile(&profile).expect("plan");
    let anchor = h(0xF01D_BE17);
    let draw = true; // D = 1: the only logits row is the selecting row
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, draw);
    let run =
        a16_execute_free_prompt_streaming_v1(&artifact, &profile, Some(&plan), &job, &prompt, backend.step_ladder_cap(), &mut |_| {})
            .expect("one honest execution");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let honest_bytes = base0_fp_material_encode_v2(&run, &ids).expect("fold");
    let honest_roots = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor,
        attempt_draw: Some(draw),
        output_root: Some(run.output_root),
        job_pin: None,
    };
    let arm = |seat_r: bool, bytes: &[u8], roots: PalwClaimRootsV1| {
        palw_attempt_material_arm_v1(
            seat_r,
            PalwSeatArmV1::AttemptMaterial,
            kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(&backend),
            bytes,
            roots,
        )
    };
    assert_eq!(arm(true, &honest_bytes, honest_roots), PalwMaterialArmV1::Evidence, "the honest fold is kept, and signs nothing");
    let replayed = backend.execute_for_verdict(&job, &prompt);
    assert_eq!(
        palw_seat_replay_step_v1(&replayed, run.execution_root, run.trace_root, 0, run.output_root),
        PalwSeatReplayStepV1::Licensed,
        "(a) the replay licenses the honest held claim"
    );

    let honest = base0_fp_material_decode_v2(&honest_bytes).expect("decodes");
    for bend in 1..=4usize {
        let mut m: Base0FpMaterialV2 = honest.clone();
        let last = m.logits_rows.len() - 1;
        let row = &mut m.logits_rows[last];
        let a = base0_decode_token_select_v1(row);
        let k = (a + bend * 97) % row.len();
        row[k] = row[a] + 1;
        m.generated_token_ids[last] = k as u32;
        m.binding.full_logits_trace_root =
            tiled_logits_trace_root_v1(&m.binding.job_context, &m.logits_rows, &m.generated_token_ids).expect("tiled");
        rebind(&mut m.binding);
        let mut bytes = PALW_BASE0_FP_MATERIAL_MAGIC_V2.to_vec();
        bytes.extend_from_slice(&borsh::to_vec(&m).expect("encodes"));
        let roots = PalwClaimRootsV1 {
            execution_root: m.binding.committed_execution_root,
            trace_root: m.binding.full_logits_trace_root,
            anchor,
            attempt_draw: Some(draw),
            output_root: Some(run.output_root),
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&bytes, roots), PalwMaterialVerdictV1::Matches, "bend #{bend}: the residual");
        assert_eq!(arm(false, &bytes, roots), PalwMaterialArmV1::Licenses, "bend #{bend}: below the fences, as today");
        assert_eq!(arm(true, &bytes, roots), PalwMaterialArmV1::Evidence, "bend #{bend}: past them, kept and unsigned");
        assert_eq!(
            palw_seat_replay_step_v1(&replayed, roots.execution_root, roots.trace_root, 0, run.output_root),
            PalwSeatReplayStepV1::Refuted,
            "bend #{bend}: (b) the replay refutes it — no Valid from this seat about it again"
        );
    }
}

/// **The production replay step judges the ANSWER under the network's attempt rule** (the
/// audit's H-A, closed at the seat on a real model-class replay). On the held A16 v7 row with the
/// testnet-12 rule (`CoreV1`) the seat's replay reproduces the producer's roots AND its committed
/// `output_root`, so `palw_seat_replay_step_v1` — the one rule the off-loop poll reads — licenses
/// it. The same roots under a `Legacy` producer's `output_root` (the root a worker that never set
/// its rule commits: the hybrid FP worker before a4682a8d) are the honest arithmetic with another
/// answer, and the step refutes them; so does any ground answer. The two rules really differ here
/// (a model class), which a floor test cannot show — the floor's root is the same under both.
#[test]
fn past_seat_r_the_replay_step_refutes_a_claim_whose_answer_is_not_the_core_v1_root() {
    use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
    let artifact = a16_artifact(8_292);
    let profile = qwen25_a16_profile_v7(a16_geometry(8_292)).expect("held v7");
    let backend_under = |rules: PalwAttemptRulesV1| {
        Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), qwen25_a16_held_canonical_v1(profile.n_ctx))
            .expect("servable")
            .with_step_ladder_cap(kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES)
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
            .with_attempt_rules(rules)
    };
    let (core, legacy) = (backend_under(PalwAttemptRulesV1::CoreV1), backend_under(PalwAttemptRulesV1::Legacy));
    let anchor = h(0x0A57_0E12);
    let (job, prompt) = core.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, true);
    let claim = core.execute(&job, &prompt).expect("the CoreV1 producer's run");
    let replayed = core.execute_for_verdict(&job, &prompt);
    assert_eq!(
        palw_seat_replay_step_v1(&replayed, claim.execution_root, claim.trace_root, 0, claim.output_root),
        PalwSeatReplayStepV1::Licensed,
        "the honest CoreV1 claim: its roots and its answer"
    );
    let legacy_claim = legacy.execute(&job, &prompt).expect("the Legacy producer's run");
    assert_eq!(
        (legacy_claim.execution_root, legacy_claim.trace_root),
        (claim.execution_root, claim.trace_root),
        "the premise: the same arithmetic under either rule"
    );
    assert_ne!(legacy_claim.output_root, claim.output_root, "the premise: a model class's two rules name different answers");
    assert_eq!(
        palw_seat_replay_step_v1(&replayed, claim.execution_root, claim.trace_root, 0, legacy_claim.output_root),
        PalwSeatReplayStepV1::Refuted,
        "the honest roots under the Legacy answer: refuted by a CoreV1 seat"
    );
    assert_eq!(
        palw_seat_replay_step_v1(&replayed, claim.execution_root, claim.trace_root, 0, h(0xA11)),
        PalwSeatReplayStepV1::Refuted,
        "and any ground answer"
    );
}

/// **The replay runs off the panel's loop and is polled to its verdict.** Absent until started;
/// running while the task runs, and its claim held; returned once (`fresh`) and kept, so it is
/// never run again; a replay that panics is a refusal, not a hang; and a claim that leaves the
/// duties takes its result with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_seat_replay_runs_off_the_loop_and_is_polled_to_its_verdict() {
    let backend = floor_backend();
    let anchor = h(0x00C0_FFEE);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, t12_draw());
    let claim = backend.execute(&job, &prompt).expect("the producer's run");

    let mut replays = PalwSeatReplaysV1::default();
    let key = (h(9), job.job_id);
    assert!(matches!(replays.poll(&key, 100).await, PalwSeatReplayPollV1::Absent));
    assert!(!replays.holds_claim(&h(9)) && replays.has_room(false));
    let (j, p) = (job.clone(), prompt.clone());
    replays.start(key, floor_class(), false, 100, None, Box::new(backend), move |b| b.execute_for_verdict(&j, &p));
    assert!(replays.holds_claim(&h(9)), "a standing replay holds its own reservation");
    let (roots, fresh) = loop {
        match replays.poll(&key, 101).await {
            PalwSeatReplayPollV1::Running => std::thread::sleep(std::time::Duration::from_millis(5)),
            PalwSeatReplayPollV1::Done { result, fresh } => break (result.expect("the replay runs"), fresh),
            PalwSeatReplayPollV1::Absent => panic!("a started replay is never absent"),
        }
    };
    assert!(fresh, "the first poll that sees it");
    assert!(replay_licenses_v1(&roots, claim.execution_root, claim.trace_root, 0), "the honest claim is licensed by it");
    assert!(matches!(replays.poll(&key, 102).await, PalwSeatReplayPollV1::Done { fresh: false, result: Ok(_) }), "kept, never re-run");
    assert!(!replays.retry_refused(&key, 102, 10_000), "a replay that ran is never retried");

    // A panic in the backend is a refusal the verdict block reads, not a hang.
    let boom = (h(30), h(30));
    replays.start(boom, floor_class(), false, 102, None, Box::new(floor_backend()), |_| panic!("the backend fell over"));
    let refused = returned(&mut replays, &boom, 103).await;
    assert!(refused.is_err_and(|e| e.contains("did not finish")), "a panicked replay is refused");

    replays.retain_live(|claim| *claim != h(9));
    assert!(!replays.holds_claim(&h(9)), "a claim that left the duties takes its result with it");
    assert!(matches!(replays.poll(&key, 104).await, PalwSeatReplayPollV1::Absent));
}

/// **(g) A refused replay is started once more, while the time it took fits again before the
/// deadline.** A second refusal stands; a refusal whose second run could only return after the
/// chain stops taking the receipt stands too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_replay_is_retried_once_while_time_remains() {
    let mut replays = PalwSeatReplaysV1::default();
    let key = (h(9), h(0xA));
    replays.start(key, floor_class(), false, 100, None, Box::new(floor_backend()), |_| Err("refused".into()));
    assert!(returned(&mut replays, &key, 110).await.is_err(), "it took ten DAA to refuse");
    assert!(!replays.retry_refused(&key, 116, 125), "116 + 10 is past a deadline at 125: no second run");
    assert!(matches!(replays.poll(&key, 116).await, PalwSeatReplayPollV1::Done { result: Err(_), fresh: false }), "it stands");
    assert!(replays.retry_refused(&key, 112, 125), "112 + 10 fits");
    assert!(matches!(replays.poll(&key, 112).await, PalwSeatReplayPollV1::Absent), "forgotten, so the caller starts it again");
    replays.start(key, floor_class(), false, 112, None, Box::new(floor_backend()), |_| Err("refused again".into()));
    assert!(returned(&mut replays, &key, 113).await.is_err());
    assert!(!replays.retry_refused(&key, 113, 10_000), "once, and only once");
    assert!(matches!(replays.poll(&key, 114).await, PalwSeatReplayPollV1::Done { result: Err(_), fresh: false }));
    // A retry is the claim's: a claim that leaves and comes back starts with its one retry.
    replays.retain_live(|_| false);
    replays.start(key, floor_class(), false, 200, None, Box::new(floor_backend()), |_| Err("refused".into()));
    assert!(returned(&mut replays, &key, 200).await.is_err());
    assert!(replays.retry_refused(&key, 200, 600));
    // A refusal times nothing: the class is still started at any deadline.
    assert!(replays.fits(&floor_class(), 599, 600));
}

/// **`runs_for_claim` is "still running", not "held"** (the M4 review's LOW: Q-7's `Sampled` gate
/// read it untested). True while this seat's task for the claim runs, and while it runs detached
/// (its claim left the duties); false once it has returned — though `holds_claim` still holds the
/// result — and false for a claim that never started one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_for_claim_is_true_while_a_task_runs_or_is_detached_and_false_once_it_returned() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let mut replays = PalwSeatReplaysV1::default();
    let key = (h(1), h(0xA));
    assert!(!replays.runs_for_claim(&h(1)), "nothing started");
    replays.start(key, floor_class(), false, 10, None, Box::new(floor_backend()), gated(&gate));
    assert!(replays.runs_for_claim(&h(1)) && !replays.runs_for_claim(&h(2)), "running, for its own claim only");
    replays.retain_live(|claim| *claim != h(1));
    assert!(replays.runs_for_claim(&h(1)), "detached, still running");
    assert!(matches!(replays.poll(&key, 11).await, PalwSeatReplayPollV1::Running), "taken back");
    release.send(()).expect("the gate is held");
    assert!(returned(&mut replays, &key, 12).await.is_err(), "the gated replay refuses");
    assert!(!replays.runs_for_claim(&h(1)), "returned: nothing runs");
    assert!(replays.holds_claim(&h(1)), "though its result is held");
}

/// **(g) A claim that leaves the duties detaches its running replay**: it stops holding a slot at
/// once, so the queue moves; it is taken back, not started twice, if the claim returns; the
/// running and detached together stay bounded for a second light replay; and one that returns
/// while detached is dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_that_leaves_the_duties_detaches_its_replay_and_frees_its_slot() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let mut replays = PalwSeatReplaysV1::default();
    let (a, b) = ((h(1), h(0xA)), (h(2), h(0xB)));
    replays.start(a, floor_class(), false, 10, None, Box::new(floor_backend()), gated(&gate));
    replays.start(b, floor_class(), false, 10, None, Box::new(floor_backend()), gated(&gate));
    assert!(!replays.has_room(false), "both slots held");
    replays.retain_live(|claim| *claim != h(1));
    assert!(replays.has_room(false), "a detached replay holds no slot: the queue moves");
    assert!(replays.holds_claim(&h(1)), "its reservation is still in use until it returns");
    assert!(matches!(replays.poll(&a, 11).await, PalwSeatReplayPollV1::Running), "the claim came back: taken back");
    assert!(!replays.has_room(false), "…into its slot, not started twice");
    // Bounded: with a light replay running, a second one waits while four are held.
    replays.retain_live(|_| false);
    let (c, d) = ((h(3), h(0xC)), (h(4), h(0xD)));
    replays.start(c, floor_class(), false, 12, None, Box::new(floor_backend()), gated(&gate));
    assert!(replays.has_room(false), "two detached, one running");
    replays.start(d, floor_class(), false, 12, None, Box::new(floor_backend()), gated(&gate));
    replays.retain_live(|claim| *claim != h(3));
    assert!(!replays.has_room(false), "three detached and one light running: a second light replay waits");
    replays.retain_live(|_| false);
    assert!(replays.has_room(false), "four detached and none running: a light replay always has its slot");
    for _ in 0..4 {
        release.send(()).unwrap();
    }
    for _ in 0..200 {
        replays.retain_live(|_| false);
        if !(1..=4).any(|n| replays.holds_claim(&h(n))) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!(1..=4).any(|n| replays.holds_claim(&h(n))), "returned while detached: dropped");
    assert!(replays.has_room(true) && replays.has_room(false));
}

/// **F2 (the 2026-10-01 backlog): a thrashing host cannot hold every slot.** A light replay that has
/// run past its class's timing (an untimed class: [`PalwSeatReplaysV1::OVERDUE_UNTIMED_DAA`]; a timed
/// one: four times what the last took, never under [`PalwSeatReplaysV1::OVERDUE_MIN_DAA`]) keeps its
/// thread and reservation but stops holding a slot, the held total stays under twice the slots, and
/// the slots are `--palw-seat-replay-slots` — one on a host over [`PalwSeatReplaysV1::LOAD_LIMIT_MILLI`]
/// load per CPU until it falls under the release level.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn a_replay_past_its_timing_yields_its_slot_and_a_loaded_host_runs_one() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let mut replays = PalwSeatReplaysV1::default();
    assert_eq!(replays.in_flight(), 2, "the default");
    let (a, b) = ((h(1), h(0xA)), (h(2), h(0xB)));
    replays.start(a, floor_class(), false, 100, None, Box::new(floor_backend()), gated(&gate));
    replays.start(b, floor_class(), false, 100, None, Box::new(floor_backend()), gated(&gate));
    assert!(!replays.has_room(false), "both slots held");
    replays.mark_overdue(100 + PalwSeatReplaysV1::OVERDUE_UNTIMED_DAA - 1);
    assert!(!replays.has_room(false), "not overdue yet");
    replays.mark_overdue(100 + PalwSeatReplaysV1::OVERDUE_UNTIMED_DAA);
    assert!(replays.has_room(false), "overdue: the queue moves");
    assert_eq!(replays.occupancy().overdue, 2);
    // The held total is still bounded by twice the slots: two overdue + two fresh, then nothing more.
    let (c, d) = ((h(3), h(0xC)), (h(4), h(0xD)));
    replays.start(c, floor_class(), false, 130, None, Box::new(floor_backend()), gated(&gate));
    replays.start(d, floor_class(), false, 130, None, Box::new(floor_backend()), gated(&gate));
    assert!(!replays.has_room(false), "four held: a thrashing host cannot pile up more");
    // A heavy replay is never relaxed.
    let mut replays_h = PalwSeatReplaysV1::default();
    replays_h.start((h(9), h(1)), floor_class(), true, 100, None, Box::new(floor_backend()), gated(&gate));
    replays_h.mark_overdue(10_000);
    assert_eq!(replays_h.occupancy().overdue, 0, "a C7 replay holds its slot for as long as it runs");
    assert!(!replays_h.has_room(true));
    for _ in 0..5 {
        release.send(()).unwrap();
    }
    for _ in 0..200 {
        replays.retain_live(|_| false);
        replays_h.retain_live(|_| false);
        if !(1..=4).any(|n| replays.holds_claim(&h(n))) && !replays_h.holds_claim(&h(9)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // `--palw-seat-replay-slots`.
    let mut one = PalwSeatReplaysV1::default();
    one.configure_slots(Some(1));
    assert_eq!(one.in_flight(), 1);
    one.start((h(30), h(1)), floor_class(), false, 100, None, Box::new(floor_backend()), gated(&gate));
    assert!(!one.has_room(false), "one slot, held");
    let mut four = PalwSeatReplaysV1::default();
    four.configure_slots(Some(99));
    assert_eq!(four.in_flight(), PalwSeatReplaysV1::MAX_IN_FLIGHT, "clamped");
    four.configure_slots(None);
    assert_eq!(four.in_flight(), PalwSeatReplaysV1::IN_FLIGHT);

    // The host's load: one replay at a time over the limit, the configured number again under the
    // release level, and no flapping between them.
    let mut loaded = PalwSeatReplaysV1::default();
    loaded.configure_slots(Some(3));
    assert_eq!(loaded.in_flight(), 3);
    loaded.note_host_load(Some(PalwSeatReplaysV1::LOAD_LIMIT_MILLI));
    assert_eq!(loaded.in_flight(), 1, "over the limit");
    loaded.note_host_load(Some((PalwSeatReplaysV1::LOAD_LIMIT_MILLI + PalwSeatReplaysV1::LOAD_RELEASE_MILLI) / 2));
    assert_eq!(loaded.in_flight(), 1, "between the two: stays limited");
    loaded.note_host_load(Some(PalwSeatReplaysV1::LOAD_RELEASE_MILLI - 1));
    assert_eq!(loaded.in_flight(), 3, "released");
    loaded.note_host_load(Some(PalwSeatReplaysV1::LOAD_LIMIT_MILLI + 1));
    loaded.note_host_load(None);
    assert_eq!(loaded.in_flight(), 3, "no reading: no limit");
    release.send(()).unwrap();
}

/// **F2: a big replay on a slow seat waits for the lighter work** (the 2026-10-01 collapse at 05:00: an
/// 8k replay of 37-55 minutes held 3.37 of the share's 3.5 GiB and every floor replay and audit behind it
/// waited on the ledger). Held back only when it is big, the seat has found the class slow, lighter
/// work waits and the claim is not yet urgent — an unknown class, a fast seat, an idle seat, an urgent
/// claim and a small replay all start.
#[test]
fn a_big_replay_of_a_slow_class_is_held_back_only_while_lighter_work_waits() {
    const GIB: u64 = 1 << 30;
    let mut replays = PalwSeatReplaysV1::default();
    let class = floor_class();
    let (now, far, near) = (1_000u64, 1_000 + 500, 1_000 + 100);
    replays.set_big_gate(true);
    assert!(
        !replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, far, now),
        "a class this seat has not timed is started: its first run is the measurement"
    );
    replays.timed.insert(class, 5);
    assert!(!replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, far, now), "5 DAA (13 minutes) is not slow");
    replays.timed.insert(class, 14); // the 05:00 replay: 37 minutes
    assert!(replays.is_slow_class(&class));
    assert!(
        replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, far, now),
        "slow, big, lighter work waits, far from the deadline: held"
    );
    assert!(!replays.big_replay_deferred(&class, GIB / 2, far, now), "a small replay is never held");
    // int-10.3: the caller passes the unpinned weight — a pinned 8k (1.70 GiB reserved + 1.68 GiB pinned) is still big.
    let pinned_8k_total = GIB + GIB * 7 / 10;
    let pinned_8k_file = GIB + GIB * 68 / 100;
    assert!(
        replays.big_replay_deferred(&class, pinned_8k_total + pinned_8k_file, far, now),
        "a pinned 8k is held back on a slow seat by its unpinned weight"
    );
    assert!(!replays.big_replay_deferred(&class, pinned_8k_total, far, now), "the pinned figure alone would not be");
    assert!(!replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, near, now), "within 120 DAA of the deadline it is urgent");
    assert!(replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, now + 121, now), "121 DAA out: still held");
    replays.set_big_gate(false);
    assert!(!replays.big_replay_deferred(&class, 3 * GIB + GIB / 3, far, now), "nothing lighter waits: an idle seat starts it");
}

/// **(g) C7 replays, running or detached, never keep a floor replay waiting** (the review,
/// MEDIUM). One C7 replay held at a time, and a detached one counts: its claim leaving does not
/// let a second 2M replay start, so the first coming back cannot put two in both slots. And a
/// light replay starts whenever none runs — beside a running C7 replay, behind any number of
/// detached ones, and after a detached C7 replay is taken back.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn a_heavy_replay_never_holds_every_slot() {
    assert_eq!((PalwSeatReplaysV1::IN_FLIGHT, PalwSeatReplaysV1::HEAVY_IN_FLIGHT), (2, 1));
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let mut replays = PalwSeatReplaysV1::default();
    let heavy = (h(0x2E), h(1));
    replays.start(heavy, two_m_class(), true, 10, None, Box::new(floor_backend()), gated(&gate));
    assert!(!replays.has_room(true), "one C7 replay at a time");
    assert!(replays.has_room(false), "a floor replay still has its slot");
    // The review's first scenario: the 2M claim leaves (RT#1), and comes back on a redraw.
    replays.retain_live(|claim| *claim != h(0x2E));
    assert!(!replays.has_room(true), "a detached C7 replay is still held: no second one starts");
    assert!(replays.has_room(false));
    assert!(matches!(replays.poll(&heavy, 11).await, PalwSeatReplayPollV1::Running), "taken back");
    assert!(!replays.has_room(true), "still one");
    let light = (h(0x8C), h(2));
    replays.start(light, floor_class(), false, 11, None, Box::new(floor_backend()), gated(&gate));
    assert!(!replays.has_room(false) && !replays.has_room(true), "both slots held, one light");
    // The review's second scenario: detached replays pile up behind a days-long C7 replay.
    replays.retain_live(|claim| *claim == h(0x2E));
    for n in 0..3u64 {
        let key = (h(0x100 + n), h(n));
        replays.start(key, floor_class(), false, 12, None, Box::new(floor_backend()), gated(&gate));
        replays.retain_live(|claim| *claim == h(0x2E));
    }
    assert!(replays.has_room(false), "one C7 replay running, four light ones detached: the floor still starts");
    assert!(!replays.has_room(true));
    for _ in 0..5 {
        release.send(()).unwrap();
    }
    assert!(returned(&mut replays, &heavy, 13).await.is_err());
    for _ in 0..200 {
        replays.retain_live(|claim| *claim == h(0x2E));
        if replays.has_room(true) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(replays.has_room(true), "the C7 slot comes back when its replay returns and every detached one has");
}

/// **(g) A replay is not started when this host's last replay of its class would return after
/// the claim's deadline** (the review, MEDIUM). A class not yet timed is started — its first run
/// is the measurement; a replay that returned roots times its class; a refusal times nothing; and
/// the timing is the host's, kept across the claims that leave.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replay_that_cannot_return_by_the_deadline_is_not_started() {
    let mut replays = PalwSeatReplaysV1::default();
    assert!(replays.fits(&two_m_class(), 0, 0), "a class not yet timed is started");
    let key = (h(0x2E), h(1));
    replays.start(key, two_m_class(), true, 100, None, Box::new(floor_backend()), |_| {
        Ok(PalwReplayRootsV1 { execution_root: h(6), trace_root: h(7), work_leaves: None, output_root: Some(h(8)) })
    });
    assert!(returned(&mut replays, &key, 1_500).await.is_ok(), "it took 1,400 DAA");
    replays.retain_live(|_| false);
    assert!(replays.fits(&two_m_class(), 1_000, 2_400), "1,000 + 1,400 returns by 2,400");
    assert!(!replays.fits(&two_m_class(), 1_001, 2_400), "one DAA later it would not");
    assert!(replays.fits(&floor_class(), 2_399, 2_400), "another class is not timed by it");
}

/// **(f) The claim's own served job first, and a stranger's never beside it** (the review, LOW):
/// whose a served job is, asked once per payload count — a `true` stands for the claim's life, a
/// `false` is asked again when a payload arrives — and forgotten when the claim leaves.
#[test]
fn whose_job_is_asked_once_per_payload_count() {
    let mut replays = PalwSeatReplaysV1::default();
    let (own, junk) = ((h(9), h(1)), (h(9), h(2)));
    assert_eq!(replays.own_job(&own, 1), None, "never asked");
    replays.note_own_job(own, true, 1);
    replays.note_own_job(junk, false, 1);
    assert_eq!(replays.own_job(&own, 4), Some(true), "the claim's roots never change");
    assert_eq!(replays.own_job(&junk, 1), Some(false));
    assert_eq!(replays.own_job(&junk, 2), None, "a payload arrived since: asked again");
    replays.retain_live(|_| false);
    assert_eq!(replays.own_job(&own, 1), None, "forgotten with the claim");
}

/// **(g) Past SEAT-R the duties are answered soonest deadline first** — a floor claim's 600-DAA
/// window ahead of a 2M claim listed before it — and in the chain's order below it.
#[test]
fn past_seat_r_the_duties_are_answered_soonest_deadline_first() {
    let deadlines = [(120 + 2_799, h(1)), (720, h(2)), (650, h(3)), (720, h(4))];
    assert_eq!(palw_seat_duty_order_v1(&deadlines, false), vec![0, 1, 2, 3], "the chain's order below SEAT-R");
    let order = palw_seat_duty_order_v1(&deadlines, true);
    assert_eq!(order[0], 2, "the soonest window first");
    assert_eq!(order[3], 0, "the 2M claim's window closes last");
    let tie: Vec<usize> = order[1..3].to_vec();
    assert_eq!(tie, if h(2) < h(4) { vec![1, 3] } else { vec![3, 1] }, "a tie falls to the claim id");
}

/// The verdict block, as `a_seat_that_found_a_fault_consults_the_ledger_before_every_verdict`
/// reads it.
fn verdict_block(source: &str) -> &str {
    let start = source.find("let verdict = 'verdict: {").expect("the verdict block");
    let end = start + source[start..].find("let Some(verdict) = verdict else { continue };").expect("its end");
    &source[start..end]
}

/// **(f) Past SEAT-R only the replays can end a verdict with `Valid`** — the source-structure pin,
/// and the run-time guard behind it.
///
/// Every `break 'verdict` of the verdict block is one of six spellings, and every mention of
/// `PalwReceiptVerdictV2::Valid` in it is a `Valid` exit or one of the two interval gates — so a
/// `Valid` cannot leave the block by a spelling this pin does not see (the review, LOW). Every
/// `Valid` exit is attributed to the nearest arm named before it, fifteen in all, in order. The four
/// replay exits past the fence are the `Licensed` arms of the three off-loop replays
/// (`gen_tensor_seat_pass_v1` — RFC-0003's tensor claim, first in the free-prompt lane —
/// `fp_seat_replay_pass_v1`, `attempt_seat_replay_v1`) and RFC-0004's evaluation replay (the `Valid` arm of
/// its own step, at the head of the block), the only lines that set
/// `licensed_by_replay`; the two SEAT-S4 exits are the `Licensed` arms of the verified resume, one
/// a lane, the only lines that set `licensed_by_verified_resume` (for a `PartialResumes` seat's
/// own-mask V3 alone); those four are the only exits whose arm the rule admits. The two other
/// replay exits — the free-prompt
/// `FPM1` replay and the awaited attempt replay — sit behind the fence's `continue` and the
/// fence's `else`. Every material, retained, capture, interval and S1 exit consults the rule,
/// which refuses them past the fence; S3 and S1 cannot reach a `Valid` because the partial resume
/// abstains before either reads a byte. And the filing refuses, before anything is signed, a
/// `Valid` past the fence that `licensed_by_replay` does not stand behind — in a release build.
/// Then the per-arm facts: a partial seat on C7 files nothing; a refuting replay stops every
/// `Valid` and none of the full seat's fault finders, and goes to the challenger's half when the
/// duty ends; the material is kept before the rule is read; the tail accuses nobody whose verified
/// material it holds; and a `Valid` past SEAT-R is filed only in a role that replays.
#[test]
fn past_seat_r_only_the_replays_can_end_a_verdict_with_valid() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let block = verdict_block(source);

    // The explicit collusion drill predates this refactor and deliberately signs without replay.
    // Attribute that branch separately, while keeping every ordinary exit under the existing pin.
    assert!(source.contains_code("let colluding = self.config.drill_collude_with.contains(&duty.executor_bond);"));
    let drill = block.find_code("if colluding {").expect("the explicit drill branch");
    let after_drill = drill + block[drill..].find_code("// **Class capability").expect("the ordinary path resumes");
    let drill_body = &block[drill..after_drill];
    assert_eq!(drill_body.matches("break 'verdict Some(PalwReceiptVerdictV2::Valid);").count(), 1);
    for gate in [
        "seat_r_duty.role == PalwSeatRRoleV1::PartialResumes",
        "licensed_by_verified_resume = true;",
        "licensed_by_replay = true;",
        ".mask_of(duty.seat_index as u16)",
    ] {
        assert!(drill_body.contains(gate), "the drill remains attributed to its explicit role: {gate}");
    }
    let ordinary = format!("{}{}", &block[..drill], &block[after_drill..]);
    let block = ordinary.as_str();

    // Every exit, in a spelling this pin reads.
    const VALID: &str = "break 'verdict Some(PalwReceiptVerdictV2::Valid);";
    const ANY: &str = "break 'verdict Some(verdict);";
    const SPELLINGS: [&str; 8] = [
        "break 'verdict None;",
        "break 'verdict None,",
        VALID,
        ANY,
        "break 'verdict Some(PalwReceiptVerdictV2::Incapable);",
        "break 'verdict Some(PalwReceiptVerdictV2::Unavailable {",
        "break 'verdict None\n",
        // ADR-0152 Q-7: a finished S3 sampling's answer — `Sampled`, never a `Valid`.
        "break 'verdict Some(PalwReceiptVerdictV2::Sampled);",
    ];
    for (at, _) in block.match_indices("break 'verdict") {
        assert!(
            SPELLINGS.iter().any(|spelling| block[at..].starts_with(spelling)),
            "an exit this pin cannot read: {:?}",
            &block[at..(at + 80).min(block.len())]
        );
    }
    assert!(block.trim_end().ends_with("None\n            };"), "the block's own value is None");
    const GATE: &str = "(!matches!(verdict, PalwReceiptVerdictV2::Valid)";
    assert_eq!(block.matches(GATE).count(), 2, "the two interval arms' gates");
    assert_eq!(
        block.matches("PalwReceiptVerdictV2::Valid").count(),
        block.matches(VALID).count() + block.matches(GATE).count(),
        "every Valid the block names is an exit this pin attributes, or an interval gate"
    );

    let mut exits: Vec<usize> = block.match_indices(VALID).map(|(i, _)| i).collect();
    exits.extend(block.match_indices(ANY).map(|(i, _)| i));
    exits.sort();
    let arm_before = |at: usize| -> (PalwSeatArmV1, usize) {
        let named = block[..at].rfind("PalwSeatArmV1::").expect("every exit names its arm");
        let rest = &block[named + "PalwSeatArmV1::".len()..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let arm = PalwSeatArmV1::ALL.into_iter().find(|a| format!("{a:?}") == name).unwrap_or_else(|| panic!("no arm {name}"));
        (arm, named)
    };
    use PalwSeatArmV1::*;
    assert_eq!(
        exits.iter().map(|at| arm_before(*at).0).collect::<Vec<_>>(),
        vec![
            // RFC-0004 (A10): the evaluation claim's replay, at the head of the block (after the
            // capability check): a claim with no quanta is never judged by the free-prompt arms below.
            EvaluationReplay,
            // RFC-0003: a tensor claim's own seat pass, first in the free-prompt lane.
            FreePromptReplay,
            VerifiedSegmentResume,
            FreePromptReplay,
            SegmentResume,
            FreePromptInterval,
            FreePromptCapture,
            FreePromptReplay,
            AttemptMaterial,
            RetainedMaterial,
            VerifiedSegmentResume,
            AttemptReplay,
            SegmentResume,
            AttemptReplay,
            AttemptInterval,
        ],
        "the verdict block's Valid exits, in order"
    );
    let fp_pass = block
        .find_code(".fp_seat_replay_pass_v1(session, duty, seat_r_duty, current_daa, pooled, &mut seat_replays)")
        .expect("the FP pass");
    let attempt_pass = block.find_code(".attempt_seat_replay_v1(").expect("the attempt pass");
    let attempt_args: String = block[attempt_pass..].chars().take_while(|c| *c != ')').filter(|c| !c.is_whitespace()).collect();
    assert_eq!(
        attempt_args.trim_end_matches(','),
        ".attempt_seat_replay_v1(&session,duty,seat_r_duty,current_daa,resolved,&ctx,&prompt,&mutseat_replays",
        "the attempt pass reads the duty's SEAT-R view"
    );
    let fpm1 = block.find_code("palw_fp_material_decode_v1(").expect("the FPM1 arm");
    let seat_r_branch = block.find_code("let resolved = if seat_r {").expect("the attempt replay arm's fence");
    let old_replay =
        seat_r_branch + block[seat_r_branch..].find_code("&& replayed.insert(seat_duty_panel_key_v1(duty))").expect("the old path");
    let attempt_lane = block.find_code("if !duty.free_prompt && !kept_as_evidence {").expect("the attempt lane");
    assert!(
        fpm1 < attempt_lane && attempt_pass < old_replay,
        "the FPM1 arm ends the free-prompt lane; the old replay follows the fence"
    );
    // The run-time guard's two writers: the two off-loop replays' `Licensed` arms, each just
    // before its `Valid`.
    const LICENSED: &str = "licensed_by_replay = true;";
    let writers: Vec<usize> = block.match_indices(LICENSED).map(|(i, _)| i).collect();
    assert_eq!(
        writers.len(),
        4,
        "four writers, and no other: the three off-loop replays' `Licensed` arms (the tensor pass, the FP pass, the attempt pass), and RFC-0004's evaluation replay"
    );
    let tensor_pass = block.find_code(".gen_tensor_seat_pass_v1(").expect("the tensor pass (RFC-0003)");
    assert!(tensor_pass < fp_pass, "a tensor claim's pass runs before the FP replay arm: it never reaches the FPM1 arms");
    for at in &writers {
        assert!(block[at + LICENSED.len()..].trim_start().starts_with(VALID), "a writer is its exit's last line");
        // RFC-0004 (A10): the evaluation replay's writer is the `Valid` arm of its own step.
        if block[..*at].rfind("improve::PalwImproveSeatStepV1::Valid => {").is_some_and(|v| *at - v < 300) {
            continue;
        }
        let licensed = block[..*at]
            .rfind("(PalwSeatReplayStepV1::Licensed, ")
            .into_iter()
            .chain(block[..*at].rfind("PalwSeatReplayStepV1::Licensed => {"))
            .max()
            .expect("a Licensed arm");
        assert!(*at - licensed < 600, "inside the Licensed arm");
        assert!(
            (tensor_pass < licensed && licensed - tensor_pass < 900)
                || (fp_pass < licensed && licensed - fp_pass < 400)
                || (attempt_pass < licensed && licensed - attempt_pass < 900)
        );
    }
    // SEAT-S4's two writers: the verified resume's `Licensed` arms, one a lane, each just before
    // its `Valid` and inside the arm of a `seat_s4_resume_v1` step.
    const RESUMED: &str = "licensed_by_verified_resume = true;";
    let resumers: Vec<usize> = block.match_indices(RESUMED).map(|(i, _)| i).collect();
    assert_eq!(resumers.len(), 2, "two writers, and no other");
    for at in &resumers {
        assert!(block[at + RESUMED.len()..].trim_start().starts_with(VALID), "a writer is its exit's last line");
        let licensed = block[..*at].rfind("PalwSeatResumeStepV1::Licensed(mask) => {").expect("a Licensed arm");
        assert!(*at - licensed < 400, "inside the Licensed arm");
        let resumed = block[..licensed].rfind(".seat_s4_resume_v1(").expect("the verified resume");
        assert!(licensed - resumed < 2_600, "of a SEAT-S4 step");
    }
    for (n, at) in exits.iter().enumerate() {
        let (arm, named_at) = arm_before(*at);
        let path = &block[named_at.saturating_sub(200)..*at];
        if palw_seat_arm_licenses_v1(true, arm) {
            assert!(
                path.contains_code("debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::"),
                "exit {n} ({arm:?}) states the rule"
            );
            // A replay exit past the fence is an off-loop replay's `Licensed` (a verified resume's,
            // for its own arm), or sits behind the fence.
            let writers = if arm == VerifiedSegmentResume { &resumers } else { &writers };
            let off_loop = writers.iter().any(|w| *w < *at && *at - *w < 120);
            let below_the_fence = (fpm1 < *at && *at < attempt_lane) || old_replay < *at;
            assert!(off_loop || below_the_fence, "exit {n} ({arm:?}) is an off-loop replay's licence, or unreachable past the fence");
        } else {
            assert!(
                path.contains_code("palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::")
                    || (path.contains_code("palw_attempt_material_arm_v1(") && path.contains_code("== PalwMaterialArmV1::Licenses")),
                "exit {n} ({arm:?}): its Valid is gated by the rule, which refuses it past the fence"
            );
        }
    }
    // The guard, where the receipt is signed: before the forms, and before any signature.
    let filing = &source[source.find_code("let Some(verdict) = verdict else { continue };").expect("the filing")..];
    let filing = &filing[..filing.find_code("if let Some(fence) = vertex_fence {").expect("its end")];
    let guard = filing
        .find_code(
            "if seat_r && matches!(verdict, PalwReceiptVerdictV2::Valid) && !(licensed_by_replay || licensed_by_verified_resume) {",
        )
        .expect("the run-time guard");
    assert!(filing[guard..].find_code("continue;").is_some_and(|c| c < 400), "it files nothing");
    assert!(guard < filing.find_code("let forms = palw_seat_receipt_forms_v1(").expect("the forms"));
    assert!(guard < filing.find_code("sign(").expect("the signing"));
    for flag in ["let mut licensed_by_replay = false;", "let mut licensed_by_verified_resume = false;"] {
        let declared = source.find(flag).expect("declared false for every duty");
        assert!(
            declared < source.find_code("let verdict = 'verdict: {").expect("the block")
                && declared > source.find_code("for index in order {").unwrap()
        );
    }
    // A verified resume files its own mask's V3 as a resuming seat, and nothing wider.
    let own_mask = filing.find_code("if licensed_by_verified_resume\n").expect("the verified resume's guard");
    assert!(filing[own_mask..].contains_code("seat_r_duty.role != PalwSeatRRoleV1::PartialResumes"));
    assert!(filing[own_mask..].find_code("continue;").is_some_and(|c| c < 900), "it files nothing");
    assert!(own_mask < filing.find_code("let forms = palw_seat_receipt_forms_v1(").expect("the forms"));

    // The two off-loop replays run only in a role that replays, and never again on a refuted claim.
    let squeezed: String = block[..fp_pass].chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        squeezed.contains_code(
            "ifseat_r&&matches!(seat_r_duty.role,PalwSeatRRoleV1::FullSeat|PalwSeatRRoleV1::PartialReplays)&&!refuted{"
        ),
        "the FP pass: the replaying roles only"
    );
    let abstains = block[..attempt_pass]
        .rfind_code("if seat_r_duty.role == PalwSeatRRoleV1::PartialAbstains {\n                                break 'verdict None;");
    assert!(abstains.is_some_and(|a| a > seat_r_branch), "the attempt pass: a partial seat on C7 files nothing");
    assert!(
        block[seat_r_branch..attempt_pass].contains_code("if refuted {\n                                resolved\n"),
        "no second replay"
    );
    assert!(
            block[attempt_pass..old_replay]
                .contains_code("Some(backend) if seat_r_duty.role == PalwSeatRRoleV1::FullSeat => backend,\n                                            _ => break 'verdict None,"),
            "only the full seat's fault finders run on, refuted or not"
        );
    // The FPM1 replay below the fence only; the old attempt replay is the fence's `else`.
    assert!(
        block[fpm1.saturating_sub(300)..fpm1]
            .contains_code("if seat_r {\n                                continue;\n                            }"),
        "the FPM1 replay is skipped past SEAT-R"
    );
    assert!(
        block[seat_r_branch..old_replay]
            .contains_code("} else if current_daa >= duty.bound_daa.saturating_add(PALW_ATTEMPT_REPLAY_GRACE_DAA)")
    );
    // Every material and retained arm keeps its bytes before the rule is read.
    for arm in ["PalwSeatArmV1::AttemptMaterial", "PalwSeatArmV1::RetainedMaterial"] {
        let from = block.find(arm).expect("the material arm");
        let licenses = from + block[from..].find_code("if arm == PalwMaterialArmV1::Licenses").expect("the rule");
        let kept = &block[from..licenses];
        assert!(
            kept.contains_code("self.persist_foreign_material(&duty.claim_id, bytes);")
                || kept.contains_code("pool_books.material.admit("),
            "{arm}: kept before the rule is read"
        );
    }
    // A refuting replay is terminal for `Valid` (the review, MEDIUM): recorded by both lanes, it
    // stops a partial seat before every arm and the full seat at no arm; the full seat's fault
    // finders — the FP interval arm and capture sampler, the attempt interval arm — still run.
    let refuted = source.find_code("let refuted = seat_r && replay_refuted.contains(&duty.claim_id);").expect("read per duty");
    assert!(refuted < source.find_code("let verdict = 'verdict: {").unwrap());
    let gate = block.find_code("if refuted && seat_r_duty.role != PalwSeatRRoleV1::FullSeat {").expect("the partial seat's gate");
    assert!(gate < block.find_code("if duty.free_prompt {").expect("the first lane"), "before every arm");
    assert!(block[gate..].find_code("if refuted && seat_r_duty.role != PalwSeatRRoleV1::FullSeat { break 'verdict None;") == Some(0));
    assert!(!block.contains_code("if seat_r && replay_refuted.contains(&duty.claim_id) {"), "no gate stops the full seat");
    assert_eq!(
        block.matches("replay_refuted.insert(duty.claim_id);\n").count(),
        3,
        "the tensor lane's, the free-prompt lane's and the attempt lane's refutations are recorded"
    );
    let fp_refuted = block.find_code("(PalwSeatReplayStepV1::Refuted, _) => {\n").expect("the FP refutation");
    assert!(
            block[fp_refuted..]
                .contains_code("replay_refuted.insert(duty.claim_id);\n                                    if seat_r_duty.role != PalwSeatRRoleV1::FullSeat {\n                                        break 'verdict None;\n                                    }\n                                }"),
            "the FP pass: the full seat falls through to its fault finders"
        );
    assert!(
        fp_refuted < block.find_code(".interval_seat_outcome_v1(").unwrap()
            && fp_refuted < block.find_code("palw_fp_capture_decode_v1(").unwrap()
    );
    let sweep = &source[source.find_code("seat_replays.retain_live(|claim| duty_claims.contains(claim));").expect("the sweep")..];
    assert!(
            sweep.find_code("seat_replays.retain_live(|claim| duty_claims.contains(claim)); // A claim this seat's replay refuted goes to the challenger's half") == Some(0) && sweep.contains_code("for claim in replay_refuted.iter().filter(|claim| !duty_claims.contains(*claim)) {\n                self.note_seat_fault_v1(*claim, 0, 0);"),
            "a refutation reaches the challenger's half once the duty ends"
        );
    // The free-prompt replay pass runs before the interval seat, the S1 resume and the capture arm.
    assert!(fp_pass < block.find_code(".interval_seat_outcome_v1(").expect("the FP interval arm"));
    assert!(fp_pass < block.find_code("palw_fp_capture_decode_v1(").expect("the capture arm"));
    // The capture sampler past SEAT-R runs to an end once, never on every tick of a replay.
    let capture = block.find_code("palw_fp_capture_decode_v1(").expect("the capture arm");
    assert!(
        block[capture..]
            .contains_code("if capture_sampled.contains(&duty.claim_id) {\n                                        continue;")
    );
    // The tail: a seat holding the claim's verified material accuses nobody of withholding it.
    let tail = block.rfind("break 'verdict Some(PalwReceiptVerdictV2::Unavailable").expect("the tail");
    let evidence = block.find_code("if seat_r && evidence_kept.contains(&duty.claim_id) {").expect("the evidence gate");
    assert!(evidence < tail && block[..tail].contains_code("self.request_material_signed("), "before the pull and the accusation");
    assert!(
        block[..tail].contains_code("let window = deadline.saturating_sub(duty.bound_daa);"),
        "the tail reads the per-claim window"
    );

    // S3 and S1: the partial resume abstains past SEAT-R before either reads a byte.
    let helper = &source[source.find_code("async fn palw_v2_try_partial_resume_v1(").expect("the S1 helper")..];
    let helper = &helper[..helper.find_code("/// **Ask the network for the openings").expect("its end")];
    let full = helper.find_code("return PalwV2SeatPathV1::FullReplay;").expect("the full seat's path");
    let abstain = helper.find_code("return PalwV2SeatPathV1::Abstain;").expect("the partial seat abstains");
    assert!(helper[full..abstain].contains_code("if palw_seat_r_in_force_v1(&self.consensus_config.params, current_daa) {"));
    for read in [
        "palw_verification_s3_at(current_daa)",
        ".replay_layer_site_v3(",
        ".open_segment_checkpoint_v1(",
        ".replay_segment_from_checkpoint_v1(",
    ] {
        assert!(abstain < helper.find(read).unwrap_or_else(|| panic!("the helper reads {read}")), "{read} after the abstention");
    }
    assert_eq!(
        block.matches("PalwV2SeatPathV1::Abstain => break 'verdict None,").count(),
        2,
        "every resume call site files nothing on it"
    );

    // The replays' own verdicts: the whole job, both roots, the price, reserved, timed, retried
    // once, and a C7 replay in the heavy slot.
    let attempt = &source[source.find_code("async fn attempt_seat_replay_v1(").expect("the attempt replay")..];
    let attempt = &attempt[..attempt.find("\n    }\n").expect("its end")];
    assert!(attempt.contains_code("b.execute_for_verdict(&ctx, &prompt)"), "the whole anchor job");
    assert!(
        attempt.contains_code("let backend = (step != PalwSeatReplayStepV1::Licensed).then_some(backend);"),
        "the fault finders' backend"
    );
    let fp = &source[source.find_code("async fn fp_seat_replay_pass_v1(").expect("the FP replay")..];
    let fp = &fp[..fp.find("\n    }\n").expect("its end")];
    // The whole served job now runs through the routed FP start (`start_fp_routed_v2` → `bound.execute(&job,&prompt)`).
    assert!(fp.contains_code("replays.start_fp_routed_v2("), "the whole served job");
    for (name, body) in [("attempt", attempt), ("free prompt", fp)] {
        let squeezed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(squeezed.contains_code("palw_seat_replay_step_v1(&"), "{name}: the one replay rule");
        assert!(squeezed.contains_code("duty.work_leaves,duty.output_root"), "{name}: and the claim's answer (SEAT-S2)");
        // The FP pass reserves through the routed replay's own backend (`bound.backend_and_context_mut_v2()`), the attempt's
        // through `backend.as_mut()` — both under the full seat's role, for the replay's life.
        assert!(body.contains_code("self.reserve_replay_at_widest_run_v1(\"full-seat\", "), "{name}: reserved for its life");
        assert!(body.contains_code("replays.retry_refused(&key, current_daa, seat_r_duty.deadline)"), "{name}: retried once, in time");
        assert!(body.contains_code("replays.has_room(seat_r_duty.heavy)"), "{name}: a C7 replay takes the heavy slot");
        assert!(body.contains_code("replays.fits(&duty.class_id, current_daa, seat_r_duty.deadline)"), "{name}: never started late");
        let fits = body.find_code("replays.fits(").unwrap();
        let start = body.find_code("replays.start(").or_else(|| body.find_code("replays.start_fp_routed_v2(")).unwrap();
        assert!(fits < start && fits < body.find_code("self.reserve_replay_at_widest_run_v1(").unwrap());
    }
    // The free-prompt pass: the claim's own job first, and a stranger's never started beside it.
    assert!(fp.contains_code("order.sort_by_key(|i| !own[*i]);"));
    assert!(fp.contains_code("let decides = is_own || !holds_own;"));
    assert!(fp.contains_code("PalwSeatReplayPollV1::Absent if !decides => {}"));
    assert!(
        fp.find_code("PalwSeatReplayPollV1::Absent if !decides => {}").unwrap() < fp.find_code("replays.start_fp_routed_v2(").unwrap()
    );
    let step = &source[source.find_code("pub(crate) fn palw_seat_replay_step_v1(").expect("the one replay rule")..];
    let step = &step[..step.find("\n}\n").expect("its end")];
    assert!(
        step.contains_code("replay_licenses_v1(roots, claimed_execution_root, claimed_trace_root, priced_work_leaves)"),
        "both roots, and the work where the claim prices it"
    );

    // The filing: the forms rule, inside the deadline the loop read.
    assert!(filing.contains_code("let signed_daa = current_daa.clamp(duty.bound_daa, deadline);"));
    assert!(filing.contains_code("let forms = palw_seat_receipt_forms_v1(v2_at, seat_r, valid, seat_r_duty.role, mask);"));
    assert!(filing.contains_code("own_receipts.insert(duty.claim_id, (carried, deadline));"));
    assert!(
        source.contains_code("if !seat_duty_is_due_until_v1(duty, &answered, current_daa, deadline) {"),
        "due until the loop's deadline"
    );
    // The roles read the outsider; the V1 door is offered its quorum past the fence only.
    assert!(source.contains_code("role: palw_seat_r_role_v1(full_seat, class.held_to_final(), outsider, mask_is_full),"));
    let collector = source
        .find_code("let Some((object, door)) = kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(")
        .expect("the collector");
    let collector = &source[collector..collector + 1_200];
    assert!(collector.contains_code(
        "if seat_r {\n                                palw_v1_offer_v1(&pool, v1_sent.get(&claim).copied().unwrap_or(0), |set| {"
    ));
    assert!(
        collector
            .contains_code("} else {\n                                session.palw_v2_receipt_quorum_assemble(claim, pool.clone())\n"),
        "below it, as always"
    );
}

/// **The one replay rule both lanes read**: `Licensed` only when both roots, the priced work and
/// the answer reproduce (SEAT-S2); a replay that ran and does not is `Refuted` (terminal for
/// `Valid`); one that did not run, or names no answer, is `NoVerdict`, never a licence.
#[test]
fn a_returned_replay_licenses_refutes_or_says_nothing() {
    use PalwSeatReplayStepV1 as S;
    let (o, other) = (h(0x0A), h(0x0B));
    let roots = |w: Option<u64>| {
        Ok(PalwReplayRootsV1 { execution_root: h(0xE), trace_root: h(0x7), work_leaves: w, output_root: Some(h(0x0A)) })
    };
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(30)), h(0xE), h(0x7), 30, o), S::Licensed);
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(30)), h(0xE), h(0x7), 0, o), S::Licensed, "an attempt prices nothing");
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(31)), h(0xE), h(0x7), 30, o), S::Refuted, "the work is not the claim's");
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(30)), h(0xF), h(0x7), 30, o), S::Refuted, "another execution root");
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(30)), h(0xE), h(0x8), 30, o), S::Refuted, "another trace root");
    assert_eq!(palw_seat_replay_step_v1(&Err("refused".into()), h(0xE), h(0x7), 30, o), S::NoVerdict);
    assert_eq!(palw_seat_replay_step_v1(&roots(Some(30)), h(0xE), h(0x7), 30, other), S::Refuted, "M2: another answer");
}

/// **The foreign retention never sweeps a live claim's file** (the review, LOW): a C7 claim's
/// window outlives the 72-hour age bound, so a file named for a claim the panel holds live is
/// kept whatever its age and counts toward no bound; every other file is swept by age, then by
/// count, as before.
#[test]
fn the_foreign_retention_keeps_a_live_claims_material_past_the_age_bound() {
    let dir = std::env::temp_dir().join(format!("seat-r-foreign-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (live, gone) = (h(0x2E), h(0x8C));
    for name in [format!("{live}.material"), format!("{live}.answer"), format!("{gone}.material"), "junk".to_string()] {
        std::fs::write(dir.join(name), b"bytes").unwrap();
    }
    let pinned: HashSet<Hash64> = [live].into();
    let hours = |n: u64| std::time::Duration::from_secs(n * 3600);
    // 93 hours on: the 2M claim is still in its window.
    prune_foreign_retention_v1(&dir, &pinned, std::time::SystemTime::now() + hours(93), 4_096);
    assert!(dir.join(format!("{live}.material")).exists() && dir.join(format!("{live}.answer")).exists(), "a live claim's files stay");
    assert!(!dir.join(format!("{gone}.material")).exists() && !dir.join("junk").exists(), "the rest go by age, as before");
    // By count: the live claim's files do not count, and are never the ones evicted.
    for n in 0..3u64 {
        std::fs::write(dir.join(format!("{}.material", h(0x100 + n))), b"bytes").unwrap();
    }
    prune_foreign_retention_v1(&dir, &pinned, std::time::SystemTime::now(), 1);
    let left = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(left, 3, "the two live files, and one of the three others");
    assert!(dir.join(format!("{live}.material")).exists());
    let _ = std::fs::remove_dir_all(&dir);
}
