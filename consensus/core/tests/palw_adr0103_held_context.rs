//! **ADR-0103 — the context is held off the chain, and the chain carries a root, an opening and a
//! logarithm.**
//!
//! Two halves. §1.1's table is pinned as the LIMITATIONS it lists: on every shipped preset the
//! order column (Decision 8) reads the terms the ADR names as linear, so the day one of them moves
//! a test says so rather than staying green (`pin-the-limitation-not-just-the-behaviour`). And on a
//! network minted with `Params::palw_held_context`, §5's invariants 2, 3, 5 and 8 hold at 2M for the
//! dense lineage's held row: every chain wall reads constant or logarithmic, the close grows one
//! path element a doubling, the window holds the dissection at the derived arity, and the gate
//! admits the class at a context the shipped geometry ceiling refuses — refusing it by name where
//! the fence, the tiled ids or `PanelDa` are missing.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, devnet_shipped_params, palw_held_context_mint_v1, palw_rc_shipped_params,
};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwAdmissionShapeV1, PalwClassAdmissionError, PalwHeldAdmissionV1, palw_admission_shape_at_v1,
    palw_post_genesis_registration_capped_v1, verify_class_admission_v7, verify_class_admission_v8,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_fit_v1::{
    PalwFitOrderV1, PalwFitRegimeV1, PalwFitVerdictV1, PalwFitWallV1, PalwHeldTermV1, PalwModelFitReportV1,
    palw_geometry_ceiling_fit_v1, palw_model_fit_v2,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Every scheduled fence armed, every `never()` fence dormant — "on this ruleset, ever".
const EVER: u64 = u64::MAX - 1;

fn bundle(params: &Params) -> &PalwConsensusParamsV2 {
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle,
        _ => panic!("the preset ships no ConsensusV2 bundle; this test's premise is wrong, not its subject"),
    }
}

/// **A network minted with the held regime**, on testnet-11's own lattice (its 3,000-DAA court
/// window, its 42-DAA turns, its 27-carrier close), with the ladder minted at `ladder` —
/// `palw_held_context_mint_v1`, the one spelling the generator and the devnet flag share.
fn held_network(ladder: u64) -> Params {
    palw_held_context_mint_v1(palw_rc_shipped_params(), ladder).unwrap_or_else(|e| panic!("the held network does not assemble: {e:?}"))
}

fn fit(params: &Params, profile: &PalwShapeProfileV3, regime: PalwFitRegimeV1) -> PalwModelFitReportV1 {
    let shape = palw_admission_shape_at_v1(params, bundle(params), profile, EVER).expect("the preset has an admission shape");
    palw_model_fit_v2(profile, bundle(params), shape.court, params.palw_prompt_ids_form_at(EVER), regime)
}

fn order(report: &PalwModelFitReportV1, wall: PalwFitWallV1) -> PalwFitOrderV1 {
    report.row(wall).unwrap_or_else(|| panic!("the report has no {} row", wall.name())).order
}

fn held_order(report: &PalwModelFitReportV1, term: PalwHeldTermV1) -> PalwFitOrderV1 {
    report.held_term(term).unwrap_or_else(|| panic!("the report has no {} term", term.name())).order
}

/// The dense lineage's held row (graph-v7) at `n_ctx`.
fn dense_v7(n_ctx: u32) -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).expect("graph-v7 builds at every context")
}

/// A registration of `profile` whose canonical job meets ADR-0077 Decision 14's floor exactly
/// (`n_ctx / 8` positions), counted against `ladder` as the gate recounts it.
fn registration(
    profile: &PalwShapeProfileV3,
    ladder: u64,
) -> (PalwJobContextV2, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) {
    let decode = 2u32;
    let prefill = (profile.n_ctx / 8).max(2);
    let canonical = PalwJobContextV2 {
        version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"adr-0103".to_vec(),
        job_id: Hash64::from_u64_word(1),
        job_nullifier: Hash64::from_u64_word(2),
        assignment_id: Hash64::from_u64_word(3),
        execution_seed: [3; 32],
        model_profile_id: Hash64::from_u64_word(4),
        runtime_manifest_hash: Hash64::from_u64_word(5),
        runtime_class_id: Hash64::from_u64_word(6),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: profile.logits_scheme_id,
        cu_ruleset_id: Hash64::from_u64_word(7),
        tokenizer_id: Hash64::from_u64_word(8),
        prompt_token_ids_hash: Hash64::from_u64_word(9),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    };
    let object = palw_post_genesis_registration_capped_v1(
        profile.clone(),
        canonical.clone(),
        Hash64::from_u64_word(0xA16),
        0,
        1,
        1,
        0,
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0)),
        Vec::new(),
        ladder,
    )
    .expect("the canonical job counts against the ladder");
    (canonical, object)
}

fn gate(params: &Params, profile: &PalwShapeProfileV3, shape: PalwAdmissionShapeV1) -> Result<(), PalwClassAdmissionError> {
    let b = bundle(params);
    let (canonical, object) = registration(profile, b.court.max_step_leaf_count());
    verify_class_admission_v8(
        b,
        profile,
        &canonical,
        &object,
        &[],
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        shape.held,
    )
    .map(|_| ())
}

/// **§1.1, pinned as the limitations it lists** (Invariant 8's first half). On both shipped
/// presets the dense graph-v5 row at 512 reads the terms the ADR calls linear as `Linear`, the
/// window as `Logarithmic`, and the seat's recompute as the whole prefill. The close reads
/// `Linear` too, which §1.1 got wrong: it listed the close as already flat after ADR-0082, and the
/// order column says otherwise on both presets — the prompt ids are flat there, and the
/// generated-token pin is priced at the whole context. Neither moves until a network mints with
/// the fence; this test is what says so the day it does.
#[test]
fn section_1_1_is_what_the_shipped_presets_read() {
    let dense_v5 = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("graph-v5 at 512");
    for (name, params) in [("testnet-11 (RC)", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        assert!(params.palw_held_context.is_none(), "{name}: the fence is dormant on every shipped preset (Invariant 10)");
        let report = fit(&params, &dense_v5, PalwFitRegimeV1::Shipped);
        assert!(report.admitted(), "{name}: the shipped row is admitted at 512 — {:?}", report.refusing_walls());
        for wall in [PalwFitWallV1::GeometryCeiling, PalwFitWallV1::Ladder, PalwFitWallV1::StateChunks, PalwFitWallV1::PublicDaPayload]
        {
            assert_eq!(order(&report, wall), PalwFitOrderV1::Linear, "{name}: §1.1 says the {} is linear", wall.name());
        }
        assert_eq!(order(&report, PalwFitWallV1::CourtWindow), PalwFitOrderV1::Logarithmic, "{name}: the window is the ladder's log");
        assert_eq!(params.palw_prompt_ids_form_at(EVER), PalwPromptIdsFormV1::Flat, "{name}: flat ids — the premise below");
        assert_eq!(order(&report, PalwFitWallV1::CloseBytes), PalwFitOrderV1::Linear, "{name}: the close is not flat on this preset");
        assert_eq!(order(&report, PalwFitWallV1::CloseChunks), PalwFitOrderV1::Linear, "{name}: chunks follow bytes");
        assert_eq!(held_order(&report, PalwHeldTermV1::SeatReplay), PalwFitOrderV1::Linear, "{name}: interval 0 is the prefill");
        assert_eq!(held_order(&report, PalwHeldTermV1::SeatFetch), PalwFitOrderV1::Constant, "{name}: a shipped seat fetches nothing");
        assert_eq!(held_order(&report, PalwHeldTermV1::ExecutorRetention), PalwFitOrderV1::Linear, "{name}: the cache is the history");
        assert!(!report.linear_chain_walls().is_empty());
    }
}

/// **Invariants 2, 3 and 8 at 2M, under the fence.** The dense held row at 512, 32,768 and
/// 2,097,152: every chain wall reads `Constant` or `Logarithmic` and admits; the close grows by at
/// most one 64-byte path element a doubling; the window is ADR-0103 Decision 5's arithmetic at the
/// derived arity (37 moves at arity 2 — 1,770 DAA — at 2M); and what is linear is held — the
/// retention and the fetch — while the seat's replay is `P`.
#[test]
fn under_the_fence_the_dense_rows_chain_walls_are_constant_or_logarithmic_to_2m() {
    let params = held_network(1 << 48);
    let widths = [512u32, 32_768, 1 << 21];
    let reports: Vec<PalwModelFitReportV1> =
        widths.iter().map(|&n| fit(&params, &dense_v7(n), PalwFitRegimeV1::Held { panel_da: true })).collect();
    for report in &reports {
        let n = report.n_ctx;
        assert!(report.admitted(), "n_ctx {n}: every wall admits under the fence — {:?}", report.rows);
        assert!(report.linear_chain_walls().is_empty(), "n_ctx {n}: no chain wall is linear — {:?}", report.linear_chain_walls());
        assert!(report.unordered_chain_walls().is_empty(), "n_ctx {n}: every order is read — {:?}", report.unordered_chain_walls());
        assert_eq!(held_order(report, PalwHeldTermV1::ExecutorRetention), PalwFitOrderV1::Linear, "n_ctx {n}: held, and linear");
        assert_eq!(held_order(report, PalwHeldTermV1::SeatFetch), PalwFitOrderV1::Linear, "n_ctx {n}: the one linear seat term");
        assert_eq!(held_order(report, PalwHeldTermV1::SeatReplay), PalwFitOrderV1::Constant, "n_ctx {n}: a seat replays P");
        assert_eq!(report.row(PalwFitWallV1::PublicDaPayload).unwrap().need, 0, "n_ctx {n}: the ids never ride");
    }
    // Invariant 2: one path element a doubling, and no more.
    let close = |r: &PalwModelFitReportV1| r.row(PalwFitWallV1::CloseBytes).unwrap().need;
    let doublings = 12; // 512 → 2^21
    assert!(close(&reports[2]) - close(&reports[0]) <= 64 * doublings, "{} → {}", close(&reports[0]), close(&reports[2]));
    // Invariant 3: the window at 2M is (1 + 2·17 + terminal) × deadline + reserve at arity 2.
    let at_2m = &reports[2];
    let court = bundle(&params).court;
    assert_eq!(at_2m.arity_played, 2);
    assert_eq!(at_2m.arity_derived_for_this_row, Some(2), "the smallest arity that fits is what the derivation selects");
    let reserve = kaspa_consensus_core::palw_context_ladder::palw_close_assembly_daa_v1(court.max_close_chunks());
    let moves = 1 + 2 * 17 + u64::from(court.terminal_rounds());
    assert_eq!(at_2m.row(PalwFitWallV1::CourtWindow).unwrap().need, moves * court.turn_deadline_daa() + reserve);
    assert_eq!(moves * court.turn_deadline_daa() + reserve, 1_770, "ADR-0103 Decision 5's table, arity 2");
    assert!(1_770 < bundle(&params).state.window_court(), "the RC's own window holds a 2M dissection");
    // The ladder is a depth: 38 levels at 2M, inside the 48 it was minted at.
    assert_eq!(at_2m.row(PalwFitWallV1::Ladder).unwrap().need, 38);
}

/// **The gate admits a held class at a context the shipped geometry ceiling refuses** — ADR-0103's
/// "done when", first clause — and refuses the same class by name wherever the regime is
/// incomplete: no fence (`HeldMapNeedsItsFence`, and v7 is v8 with the fence down), no `PanelDa`
/// (the ids would ride: `LinearInTheContext` on the payload), flat ids (the close carries them:
/// `LinearInTheContext` on the close).
#[test]
fn the_gate_admits_a_held_class_the_shipped_ceiling_refuses_and_names_what_is_missing() {
    let params = held_network(1 << 48);
    let profile = dense_v7(1 << 21);
    assert_eq!(
        palw_geometry_ceiling_fit_v1(1 << 21, profile.layer_count).verdict,
        PalwFitVerdictV1::Refused,
        "2M × 28 layers is past the shipped ceiling"
    );
    let shape = palw_admission_shape_at_v1(&params, bundle(&params), &profile, EVER).expect("a shape");
    assert_eq!(shape.held, PalwHeldAdmissionV1 { armed: true, panel_da: true }, "the shape reads both fences");
    gate(&params, &profile, shape).unwrap_or_else(|e| panic!("the held class at 2M is admitted under the fence: {e}"));

    let dormant = PalwAdmissionShapeV1 { held: PalwHeldAdmissionV1::default(), ..shape };
    assert_eq!(gate(&params, &profile, dormant), Err(PalwClassAdmissionError::HeldMapNeedsItsFence));
    let b = bundle(&params);
    let (canonical, object) = registration(&profile, b.court.max_step_leaf_count());
    assert_eq!(
        verify_class_admission_v7(b, &profile, &canonical, &object, &[], &[], shape.ladder, shape.court, false, shape.token_lift)
            .map(|_| ()),
        Err(PalwClassAdmissionError::HeldMapNeedsItsFence),
        "v7 is v8 with the fence down"
    );

    let public = PalwAdmissionShapeV1 { held: PalwHeldAdmissionV1 { armed: true, panel_da: false }, ..shape };
    assert_eq!(gate(&params, &profile, public), Err(PalwClassAdmissionError::LinearInTheContext { wall: "public-da payload" }));

    let mut flat_court = shape.court.expect("the k-ary court is armed");
    flat_court.prompt_ids_form = PalwPromptIdsFormV1::Flat;
    let flat = PalwAdmissionShapeV1 { court: Some(flat_court), ..shape };
    assert_eq!(gate(&params, &profile, flat), Err(PalwClassAdmissionError::LinearInTheContext { wall: "close bytes" }));
}

/// **Invariant 5 — the registration gate costs one position.** The held gate at 2M runs the order
/// sweep to 2^27 positions and every wall's predicate at each: if any of them walked the context,
/// this test would not finish (2^21 positions × 103,008 leaves is 2 × 10^11). It is asked at 2 and
/// at 2^21 and must answer both — and the same answer kind, because nothing about the class but
/// its context moved.
#[test]
fn the_registration_gate_visits_the_same_nodes_at_every_context() {
    let params = held_network(1 << 48);
    for n_ctx in [16u32, 1 << 21] {
        let profile = dense_v7(n_ctx);
        let shape = palw_admission_shape_at_v1(&params, bundle(&params), &profile, EVER).expect("a shape");
        let started = std::time::Instant::now();
        let answer = gate(&params, &profile, shape);
        let elapsed = started.elapsed();
        assert!(answer.is_ok(), "n_ctx {n_ctx}: {answer:?}");
        // Generous: a debug build answers in milliseconds; a walk over 2 × 10^11 leaves would take
        // days. The bound is there to turn a regression into a failure rather than a hang.
        assert!(elapsed < std::time::Duration::from_secs(60), "n_ctx {n_ctx}: the gate took {elapsed:?}");
    }
}

/// **Decision 1's ladder needs the regime from genesis** (Invariant 10's assembly half). A ladder
/// past the bisection's clock is refused on a network that arms the fence later, and on one that
/// never commits to the held contexts: before the fence, a bisection this window cannot hold is a
/// legal move.
#[test]
fn a_ladder_past_the_clock_needs_the_regime_from_genesis() {
    // The held mint with the shipped prompt cap, so the ladder is the only thing past a bound.
    let held_network = |ladder: u64| {
        let mut p = held_network(ladder);
        let shipped_fp = bundle(&palw_rc_shipped_params()).freeprompt.clone();
        let PalwConsensusMode::ConsensusV2(b) = &mut p.palw_consensus_mode else { unreachable!() };
        b.freeprompt = shipped_fp;
        p.validate_palw_v2().expect("the held mint at the shipped prompt cap");
        p
    };
    let mut late = held_network(1 << 48);
    late.palw_held_context = Some(ForkActivation::new(1_000));
    let err = format!("{:?}", late.validate_palw_v2().expect_err("a later fence leaves a bisection unplayable before it"));
    assert!(err.contains("palw_held_context is not armed from genesis") || err.contains("window_court"), "{err}");

    let mut v3 = held_network(1 << 48);
    let PalwConsensusMode::ConsensusV2(b) = &mut v3.palw_consensus_mode else { unreachable!() };
    b.signature_contexts_root = kaspa_consensus_core::palw_mode_v2::palw_v2_signature_contexts_root_v3();
    v3.palw_held_context = None;
    let err = format!("{:?}", v3.validate_palw_v2().expect_err("V3 cannot carry a ladder no bisection plays"));
    assert!(err.contains("window_court"), "{err}");

    // At the clock's own ladder the fence may arm at any height.
    let mut shallow = held_network(1 << 26);
    shallow.palw_held_context = Some(ForkActivation::new(1_000));
    shallow.validate_palw_v2().expect("a ladder the bisection can play needs no genesis fence");
}

/// **The seventh wall moves under the fence** (ADR-0103 §10). The advertised free-prompt cap was
/// bounded by the IPC frame's 4,096 ids — a bound on ids that RODE a frame. On the held mint the
/// cap is the regime's own, far past 2M; the same cap on a network without the regime from genesis
/// is refused, as is one on a bundle that does not commit to the held contexts.
#[test]
fn the_prompt_cap_passes_the_frame_only_on_a_held_mint() {
    let held = held_network(1 << 48);
    let cap = bundle(&held).freeprompt.max_prompt_tokens();
    assert_eq!(cap, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_HELD_MAX_PROMPT_TOKENS_V1);
    assert!(cap > 1 << 21, "a 2M prompt is under the held mint's cap");
    assert!(cap as usize > kaspa_consensus_core::palw_v2::PALW_V2_MAX_PROMPT_TOKENS, "and past the frame's");

    let mut late = held.clone();
    late.palw_held_context = Some(ForkActivation::new(1_000));
    let err = format!("{:?}", late.validate_palw_v2().expect_err("the regime must cover every height"));
    assert!(err.contains("from genesis"), "{err}");

    let mut shipped = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(b) = &mut shipped.palw_consensus_mode else { unreachable!() };
    b.freeprompt = b.freeprompt.clone().with_held_prompt_cap_v1(1 << 21).expect("a legal held cap");
    let err = format!("{:?}", shipped.validate_palw_v2().expect_err("a shipped bundle cannot carry it"));
    assert!(err.contains("held regime's contexts") || err.contains("from genesis"), "{err}");
}

/// **Decision 7 — the plan prices the fetch, and the window binds the shard count.** The K3
/// stand-in on the held composition at 2M: the fetch column is the shard's state at the last
/// interval's start (linear in the position, the one linear term the regime keeps), the whole-model
/// shard fetches exactly its cache and state at that position, and at a seat's gigabit link the
/// fewest shards that also RESUME inside `window_receipt` is no fewer — and at a slow link strictly
/// more — than the fewest that merely fit the bytes. A link too slow for any plan is refused by
/// name, `WindowTooShort`, never as a budget it did meet.
#[test]
fn the_shard_plan_prices_the_fetch_and_the_window_binds_the_shard_count() {
    use kaspa_consensus_core::palw_held_context_v1::{palw_held_interval_positions_v1, palw_held_replay_row_v1};
    use kaspa_consensus_core::palw_model_fit_v1::stand_ins;
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v7};
    use kaspa_consensus_core::palw_shard_plan_v1::{
        PalwSeatResumeBudgetV1, PalwShardPlanError, palw_qwen36_artifact_bytes_v1, palw_shard_plan_for_seat_v1,
        palw_shard_plan_for_seat_within_window_v1, palw_shard_plan_v1,
    };
    const GIB: u64 = 1 << 30;
    let g = PalwQwen36GeometryV1 { n_ctx: 1 << 21, ..stand_ins::KIMI_K3_AS_HYBRID_V1 };
    let profile = qwen36_profile_v7(g).expect("the held stand-in builds at 2M");
    let artifact = palw_qwen36_artifact_bytes_v1(&g).scaled_to_total(stand_ins::KIMI_K3_TOTAL_PARAMETERS);

    // The fetch column: the whole model as one shard fetches its whole state at the position.
    let whole = palw_shard_plan_v1(&profile, &artifact, 1).expect("one shard");
    let shard = &whole.shards[0];
    let at = u64::from(g.n_ctx);
    assert_eq!(shard.fetch_bytes_at_v1(whole.kv_row_bytes, at), shard.kv_cache_bytes + shard.recurrent_state_bytes);
    assert_eq!(
        shard.fetch_bytes_at_v1(whole.kv_row_bytes, 2 * at) - shard.fetch_bytes_at_v1(whole.kv_row_bytes, at),
        shard.fetch_bytes_at_v1(whole.kv_row_bytes, at) - shard.recurrent_state_bytes,
        "linear in the position: a doubling adds the cache again"
    );

    let window_receipt = bundle(&palw_rc_shipped_params()).state.window_receipt();
    let budget = |bandwidth: u64| PalwSeatResumeBudgetV1 {
        seat_budget_bytes: 256 * GIB,
        bandwidth_bytes_per_second: bandwidth,
        window_receipt_daa: window_receipt,
        replay_ms_per_position: palw_held_replay_row_v1(&profile).replay_ms_per_position(),
        interval_positions: palw_held_interval_positions_v1(&profile),
    };
    let by_bytes = palw_shard_plan_for_seat_v1(&profile, &artifact, 256 * GIB, 92).expect("the bytes fit some plan");
    let fast =
        palw_shard_plan_for_seat_within_window_v1(&profile, &artifact, &budget(125_000_000), 92).expect("a gigabit seat resumes");
    assert!(fast.shard_count >= by_bytes.shard_count, "the window never needs fewer shards than the bytes");
    let slow =
        palw_shard_plan_for_seat_within_window_v1(&profile, &artifact, &budget(500_000), 92).expect("a slow seat resumes, sharded");
    assert!(
        slow.shard_count > by_bytes.shard_count,
        "a slow link needs more shards than the bytes alone: {} vs {}",
        slow.shard_count,
        by_bytes.shard_count
    );
    assert!(matches!(
        palw_shard_plan_for_seat_within_window_v1(&profile, &artifact, &budget(1), 92),
        Err(PalwShardPlanError::WindowTooShort { .. })
    ));
}

/// **The drill's network is the ADR's network** (ADR-0103 §6 step 6; Invariant 1's re-run). The
/// floor-only devnet `--palw-held-context-devnet` builds — `palw_held_context_mint_v1` at the
/// devnet's own ladder — assembles, arms the fence and every precondition from genesis, and moves
/// the fingerprint (every node of the drill must carry the flag).
#[test]
fn the_held_devnet_mint_assembles_from_the_floor_only_devnet() {
    use kaspa_consensus_core::config::params::{
        DEVNET_PARAMS, PALW_RC_GENESIS_ARTIFACT_ROOT, palw_devnet_genesis_bonds_v1, palw_v2_params_from_artifacts_on_base,
    };
    let floor =
        palw_v2_params_from_artifacts_on_base(DEVNET_PARAMS.clone(), PALW_RC_GENESIS_ARTIFACT_ROOT, palw_devnet_genesis_bonds_v1())
            .expect("the floor-only devnet assembles");
    let ladder = bundle(&floor).court.max_step_leaf_count();
    let held = palw_held_context_mint_v1(floor.clone(), ladder).expect("the held devnet assembles");
    assert!(held.palw_held_context_active_at(0), "the fence is armed from genesis");
    assert!(held.palw_shard_court_active_at(0) && held.palw_kary_court_active_at(0) && held.palw_panel_da_at(0));
    assert_eq!(held.palw_prompt_ids_form_at(0), PalwPromptIdsFormV1::MerkleV1);
    assert_eq!(bundle(&held).court.max_step_leaf_count(), ladder, "the devnet's own ladder");
    assert_ne!(held.consensus_params_id(), floor.consensus_params_id(), "the fingerprint says the network moved");
}

/// **Invariant 9 — the regime moves no economics.** Everything the held mint changes is a court,
/// a carriage or a map: the state parameters (windows, quanta sources, collateral floor), the bond
/// parameters and the free-prompt lane's pricing (quanta per canonical job, the per-receipt cap,
/// the maturity) are the base lattice's byte for byte, and `palw_fp_decode_rules` — ADR-0082
/// Decision 10's numerator, the one thing that makes a claim's earnings its decode leaves' — is
/// untouched by the mint (and refused at assembly on every network today, audit D M-1), so a held
/// network prices a claim exactly as its base does. No bond grows with the context.
#[test]
fn the_held_mint_moves_no_economics() {
    let base = palw_rc_shipped_params();
    let held = held_network(1 << 48);
    let (b, h) = (bundle(&base), bundle(&held));
    assert_eq!(b.state, h.state, "windows, quanta sources, collateral floor: the base's");
    assert_eq!(b.bond, h.bond, "the bond parameters: the base's");
    assert_eq!(b.freeprompt.quanta_per_canonical_job(), h.freeprompt.quanta_per_canonical_job());
    assert_eq!(b.freeprompt.max_quanta_per_receipt(), h.freeprompt.max_quanta_per_receipt());
    assert_eq!(b.freeprompt.max_decode_tokens(), h.freeprompt.max_decode_tokens());
    assert_eq!(b.freeprompt.receipt_maturity_daa(), h.freeprompt.receipt_maturity_daa());
    assert_eq!(base.palw_fp_decode_rules, held.palw_fp_decode_rules, "Decision 10's numerator is not the mint's to arm");
}

/// **The genesis door** (ADR-0103, the ADR-0102 precedent). A genesis row is verified against the
/// committed catalog and never meets `verify_class_admission_v8`, so a held class in a genesis set
/// is refused unless the fence is armed from genesis — and admitted by the mint that arms it.
#[test]
fn a_held_class_at_genesis_needs_the_regime_from_genesis() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    let mut with_held = devnet_shipped_params();
    let PalwConsensusMode::ConsensusV2(b) = &mut with_held.palw_consensus_mode else { unreachable!() };
    let template = b
        .genesis_objects
        .iter()
        .find(|o| matches!(o, PalwConsensusObjectV2::ClassRegistered { admission: Some(_), .. }))
        .cloned()
        .expect("the bundled devnet registers a class with its carriage");
    let PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission: Some(mut carriage),
    } = template
    else {
        unreachable!()
    };
    carriage.profile = dense_v7(512);
    b.genesis_objects.push(PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission: Some(carriage),
    });
    assert!(kaspa_consensus_core::palw_class_admission_v2::palw_genesis_registers_held_class_v1(b));
    let err = format!("{:?}", with_held.validate_palw_v2().expect_err("a held genesis row with the fence dormant"));
    assert!(err.contains("held map") && err.contains("palw_held_context"), "{err}");
    let ladder = bundle(&with_held).court.max_step_leaf_count();
    palw_held_context_mint_v1(with_held, ladder).expect("the mint that arms the fence from genesis admits it");
}
