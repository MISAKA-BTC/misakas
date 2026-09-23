//! **THE SEVEN GLOBAL INVARIANTS OF PALW testnet-12, AS RUNNING TESTS.**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads.
//!
//! Each test states the invariant, then evaluates it against the RUNTIME — the same functions the
//! fold and the admission gate call — on `palw_t12_shipped_params()` and t12's own genesis classes.
//! Every test prints its measurements BEFORE it asserts, so a failure carries its numbers.
//!
//! **A failing test in this file is an audit finding, not a broken test.** The assertions are the
//! invariants as the design states them; where the runtime does not hold one, the test is the
//! measurement of by how much.
//!
//! Every test's doc block carries two paragraphs that matter more than the assertion:
//!   COVERS  — the exact inputs and code paths evaluated.
//!   DOES NOT COVER — the part of the invariant this test cannot reach with the pub API.
//! An invariant test that silently covers one shape reads as a guarantee; these say what they are.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_global_invariants -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::class_manifest_const_v1 as manifest;
use kaspa_consensus_core::config::params::{
    PALW_RC_GENESIS_ARTIFACT_ROOT, PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT, PALW_T12_DENSE_N_CTX,
    PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT, Params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_attempted_compute_q32_per_claim_v1,
    palw_expected_attempts_q32_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::palw_work_ticket_target_v1;

// =================================================================================================
// Shared t12 ground truth. Every constant below is either read from the card at runtime or
// recomputed from a fence; none is transcribed from prose.
// =================================================================================================

/// The rate fence, sompi per 10^9 CCU. Read from the card in `t12_economics`.
const RATE_SOMPI_PER_GIGA: u64 = 900_000_000;
/// The t12 block subsidy (CoinbaseManager::calc_block_subsidy at every height on this card).
/// Not re-derivable inside `kaspa-consensus-core` (the coinbase manager lives in `kaspa-consensus`),
/// so it is pinned here and its two dependents are recomputed from the fence.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

/// escrow = subsidy through the overlay's worker carve, read from the armed fence.
fn t12_escrow_sompi(p: &Params) -> u64 {
    let carve = p.palw_overlay_carve.expect("t12 arms palw_overlay_carve").worker_carve_permille as u64;
    T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve
}

/// W0, the work floor, in CCU: escrow * 1e9 / rate.
fn t12_work_floor_ccu(escrow: u64) -> u128 {
    kaspa_consensus_core::palw_work_target_v1::palw_work_floor_v1(escrow, RATE_SOMPI_PER_GIGA)
}

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the BASE-0 floor profile projects")
}
fn floor_job() -> PalwJobContextV2 {
    rc_job_context(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)
}
fn hybrid_512() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid v7@512")
}
fn hybrid_job() -> PalwJobContextV2 {
    let (pf, d) = qwen36_held_canonical_v1(512);
    rc_job_context(&hybrid_512(), pf, d)
}
fn dense_2m() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: PALW_T12_DENSE_N_CTX, ..QWEN25_1_5B }).expect("dense v7@2M")
}
fn dense_job() -> PalwJobContextV2 {
    let (pf, d) = qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX);
    rc_job_context(&dense_2m(), pf, d)
}

/// The three classes testnet-12 registers at genesis, as (name, profile, canonical job).
fn t12_classes() -> Vec<(&'static str, PalwShapeProfileV3, PalwJobContextV2)> {
    vec![
        ("BASE-0 floor", floor_profile(), floor_job()),
        ("Qwen3.6 v7@512", hybrid_512(), hybrid_job()),
        ("Qwen2.5 A16 v7@2M", dense_2m(), dense_job()),
    ]
}

/// **U2** — the derived per-draw work, MAC-eq. This is `economic_ccu_per_claim`
/// (`palw_model_work_from_carriage_v1`) and, by the pin in `palw_canonical_work_v1`, also the
/// fork-weight scalar `provisional_scalar_v1()`. It is what the admission gate enforces `claim.pwu`
/// against past `palw_canonical_work` (armed at DAA 0 on t12).
fn u2_draw_mac_eq(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("the draw prices")
}

/// **U3** — the floor-normalised exposure pwu: U2 x floor_declared_leaves / floor_U2. This is the
/// unit `palw_exposure_pwu_v3` reserves collateral in.
fn u3_exposure_pwu(u2: u128, floor_declared_leaves: u64, floor_u2: u128) -> u64 {
    palw_exposure_unit_pwu_v1(u2, floor_declared_leaves, floor_u2)
}

/// The declared leaf count the admission gate forces `pwu_per_inference` to equal (**U1**).
fn u1_declared_leaves(b: &PalwConsensusParamsV2, p: &Params, profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u64 {
    let shape = palw_admission_shape_at_v1(p, b, profile, 0).expect("admission shape at DAA 0");
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    };
    kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, job, ladder_cap).unwrap_or(u64::MAX)
}

/// **The t12 admission gate, exactly as `processor.rs:6886` calls it, at DAA 0.**
/// `share_permille` is 0 because `palw_admission_independence` is armed at 0 (processor.rs:6779).
fn admit(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    root: Hash64,
) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(p, b, profile, 0).expect("admission shape at DAA 0");
    let counted = u1_declared_leaves(b, p, profile, job);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v9(
        b,
        profile,
        job,
        &reg,
        &certified,
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        p.palw_canonical_work_at(0),
        shape.held,
        shape.kimi_family,
        // 2026-09-23 audit C-4 fence, as the network resolves it.
        p.palw_audit_2026_09_23_active_at(0),
    )
    .map(|e| e.canonical_step_leaf_count)
}

/// The reward a claim of a class declaring `u2` MAC-eq/draw is paid, in sompi.
/// `block_bits = 0` on a chain with no bits-priced lane, so the network factor is exactly one.
fn priced_reward_sompi(u2: u128, w0: u128, escrow: u64) -> u64 {
    let target = palw_work_ticket_target_v1(u2, w0);
    let draws_q32 = palw_expected_attempts_q32_v1(target);
    let attempted_ccu = palw_attempted_compute_q32_per_claim_v1(draws_q32, u2);
    palw_rate_priced_reward_v1(escrow, attempted_ccu, RATE_SOMPI_PER_GIGA as u128)
}

/// The real MAC-eq a producer of that class burns to be paid once.
fn real_work_per_paid_claim(u2_declared: u128, u2_real: u128, w0: u128) -> u128 {
    let target = palw_work_ticket_target_v1(u2_declared, w0);
    let draws_q32 = palw_expected_attempts_q32_v1(target);
    palw_attempted_compute_q32_per_claim_v1(draws_q32, u2_real)
}

fn msk(sompi: u64) -> f64 {
    sompi as f64 / 1e8
}

// =================================================================================================
// I1 — CHEAP REAL COMPUTE CANNOT CLAIM AN EXPENSIVE CLASS
// =================================================================================================

/// **I1. A class the admission gate accepts must not be priced above what executing it costs.**
///
/// COVERS: the real t12 admission gate (`verify_class_admission_v9` under
/// `palw_admission_shape_at_v1(&t12, &bundle, profile, 0)` — the exact shape `processor.rs:6886`
/// passes) against the BASE-0 floor profile with exactly TWO scalar fields rewritten
/// (`attn_heads`, `attn_head_dim`) and every node table, every weight name, every dtype and the
/// canonical job left byte-identical to the shipped floor. Price is measured with
/// `palw_attempt_economic_compute_v1` — the function `palw_model_work_from_carriage_v1` writes into
/// the lifecycle row and therefore the one the reward and the fork weight both read. The invariant
/// is evaluated as: does the gate refuse a profile whose declared price exceeds the price of the
/// arithmetic its own node tables describe?
///
/// DOES NOT COVER: the executor side. This test never runs an inference, so it cannot prove the
/// arithmetic the two mutated scalars describe is in fact unchanged; it establishes only that the
/// node tables, the canonical job and the leaf count are identical, which is what the executor
/// enumerates. It also sweeps ONE lever on ONE class. Other priced scalars (`gdn_heads`,
/// `gdn_head_k_dim`, `gdn_head_v_dim`, the `weight_dtypes` declaration, the `.routed` weight-name
/// suffix) are separate levers on separate rows and are not swept here.
#[test]
fn i1_cheap_real_compute_cannot_claim_an_expensive_class() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w0 = t12_work_floor_ccu(escrow);

    let honest = floor_profile();
    let job = floor_job();
    let honest_u2 = u2_draw_mac_eq(&honest, &job);

    // The two scalars. Nothing else moves: same nodes, same dtypes, same canonical job.
    let mut inflated = floor_profile();
    inflated.attn_heads = 65_535;
    inflated.attn_head_dim = 41_854;
    assert_eq!(inflated.attn_nodes, honest.attn_nodes, "the node table is untouched");
    assert_eq!(inflated.gdn_nodes, honest.gdn_nodes, "the gdn table is untouched");
    assert_eq!(inflated.layer_count, honest.layer_count);
    let inflated_u2 = u2_draw_mac_eq(&inflated, &job);

    let honest_leaves = u1_declared_leaves(&b, &p, &honest, &job);
    let inflated_leaves = u1_declared_leaves(&b, &p, &inflated, &job);

    let honest_admit = admit(&p, &b, &honest, &job, PALW_RC_GENESIS_ARTIFACT_ROOT);
    let inflated_admit = admit(&p, &b, &inflated, &job, Hash64::from_u64_word(0xA271));

    let honest_reward = priced_reward_sompi(honest_u2, w0, escrow);
    let inflated_reward = priced_reward_sompi(inflated_u2, w0, escrow);
    // The attacker executes the HONEST floor arithmetic while declaring the inflated price.
    let honest_real = real_work_per_paid_claim(honest_u2, honest_u2, w0);
    let inflated_real = real_work_per_paid_claim(inflated_u2, honest_u2, w0);

    println!("\n=== I1: cheap real compute vs an expensive declaration ===");
    println!("  escrow per claim (sompi)     = {escrow}  ({:.4} MSK)", msk(escrow));
    println!("  W0 (CCU)                     = {w0}");
    println!("  honest   attn_heads/head_dim = {}/{}", honest.attn_heads, honest.attn_head_dim);
    println!("  inflated attn_heads/head_dim = {}/{}", inflated.attn_heads, inflated.attn_head_dim);
    println!("  honest   U2 (MAC-eq/draw)    = {honest_u2}");
    println!("  inflated U2 (MAC-eq/draw)    = {inflated_u2}   ({:.2}x)", inflated_u2 as f64 / honest_u2 as f64);
    println!("  honest   declared leaves     = {honest_leaves}");
    println!("  inflated declared leaves     = {inflated_leaves}   (IDENTICAL: the leaf count is what the executor enumerates)");
    println!("  gate verdict honest          = {honest_admit:?}");
    println!("  gate verdict inflated        = {inflated_admit:?}");
    println!("  honest   reward (sompi)      = {honest_reward}  ({:.4} MSK)", msk(honest_reward));
    println!("  inflated reward (sompi)      = {inflated_reward}  ({:.4} MSK)", msk(inflated_reward));
    println!("  honest   real MAC-eq/paid    = {honest_real}");
    println!("  inflated real MAC-eq/paid    = {inflated_real}");
    println!(
        "  MSK per giga real MAC-eq     honest {:.4}   inflated {:.4}",
        msk(honest_reward) / (honest_real as f64 / 1e9),
        msk(inflated_reward) / (inflated_real as f64 / 1e9)
    );

    // The structural precondition: the mutation changes nothing the executor enumerates.
    assert_eq!(honest_leaves, inflated_leaves, "precondition: the leaf count — what is executed and opened — is unchanged");

    // THE INVARIANT. A profile the gate admits must not be priced above the arithmetic its own
    // node tables describe. Here the node tables are byte-identical to the floor's, so the
    // admitted price must equal the floor's.
    if inflated_admit.is_ok() {
        assert_eq!(
            inflated_u2, honest_u2,
            "I1 VIOLATED: the gate ADMITTED a profile whose node tables, dtypes, weight names, leaf \
             count ({honest_leaves}) and canonical job are byte-identical to the BASE-0 floor's, but \
             which is priced at {inflated_u2} MAC-eq/draw against the floor's {honest_u2} — a factor \
             of {:.2}x for two scalar fields. It is paid {inflated_reward} sompi ({:.4} MSK) for \
             {inflated_real} MAC-eq of real arithmetic, against the honest floor's {honest_reward} \
             sompi for {honest_real} MAC-eq: {:.2}x the reward per unit of real work.",
            inflated_u2 as f64 / honest_u2 as f64,
            msk(inflated_reward),
            (inflated_reward as f64 / inflated_real as f64) / (honest_reward as f64 / honest_real as f64),
        );
    }
}

// =================================================================================================
// I2 — THE WORK USED FOR REWARD AND THE WORK THE RUNTIME ENFORCES USE THE SAME CANONICAL DEFINITION
// =================================================================================================

/// **I2. One canonical definition of "the work of this class", read by every consumer.**
///
/// COVERS: the three definitions `palw_model_work_from_carriage_v1` and
/// `palw_canonical_draw_work_v1` produce for each of testnet-12's three genesis classes at its own
/// genesis canonical job:
///   (a) `economic_ccu_per_claim` = `palw_attempt_economic_compute_v1(.., prefill_draw = true)` —
///       what admission ENFORCES `claim.pwu` against (`palw_admission_v2` equality, armed at 0) and
///       what `palw_attempted_ccu_v1` prices the reward from;
///   (b) `provisional_scalar_v1()` of `palw_canonical_draw_work_v1` — the FORK WEIGHT scalar;
///   (c) `verification_ccu` = `palw_job_economic_compute_v1(profile, canonical, table)` — what
///       `palw_panel_share_permille_v1` charges the panel's replay at, and therefore what splits
///       the escrow between producer and seats.
/// The invariant is that (a), (b) and (c) are one number per class.
///
/// DOES NOT COVER: the free-prompt lane's `fp_derive_work_v1`, which is a fourth spelling of "the
/// work" and is not exercised here; and it does not follow (c) through
/// `palw_state_v2::record_round_final` into the paid split — it stops at the registry row, which is
/// the last point reachable from `consensus/core/tests/`.
#[test]
#[ignore = "OPEN FINDING (2026-09-23 audit I2, not fixed on this branch): verification_ccu keeps the declared decode budget, so three spellings of one class's work disagree (floor 1.408x). Run with --ignored to see the numbers."]
fn i2_reward_work_and_enforced_work_are_one_definition() {
    let mut violations: Vec<String> = Vec::new();

    println!("\n=== I2: is there ONE canonical work per class? ===");
    for (name, profile, job) in t12_classes() {
        let work = palw_model_work_from_carriage_v1(&profile, &job).expect("the registry row builds");
        let enforced = work.economic_ccu_per_claim; // (a)
        let descriptor = PalwCanonicalClassDescriptorV1::of(&profile, Hash64::default()).expect("descriptor");
        let weight = palw_canonical_draw_work_v1(&descriptor, &job, true).expect("the draw work").provisional_scalar_v1(); // (b)
        let verification = work.verification_ccu; // (c)
        let direct = palw_job_economic_compute_v1(&profile, &job, &PALW_ECONOMIC_COST_TABLE_V1).expect("the job prices");

        println!("  {name}");
        println!("    (a) enforced / priced  economic_ccu_per_claim = {enforced} MAC-eq");
        println!("    (b) fork weight        provisional_scalar_v1  = {weight} MAC-eq");
        println!("    (c) panel replay       verification_ccu       = {verification} MAC-eq   ({:.6}x of (a))", verification as f64 / enforced as f64);
        assert_eq!(verification, direct, "verification_ccu is palw_job_economic_compute_v1 of the same job");

        if weight != enforced {
            violations.push(format!("{name}: fork weight {weight} != enforced {enforced}"));
        } else {
            println!("    (a) == (b): HOLDS — the fork-weight scalar IS the enforced/priced draw");
        }
        if verification != enforced {
            violations.push(format!(
                "{name}: verification_ccu {verification} != enforced {enforced} ({:.6}x)",
                verification as f64 / enforced as f64
            ));
        }
    }

    // THE INVARIANT.
    assert!(
        violations.is_empty(),
        "I2 VIOLATED: the same class has more than one canonical work. The divergence is \
         `palw_model_registry_v1.rs:580` (verification_ccu = palw_job_economic_compute_v1, the FULL \
         registrant-declared canonical job, P prefill positions + D-1 decode calls) against \
         `:581` (economic_ccu_per_claim = palw_attempt_economic_compute_v1(.., prefill_draw = true), \
         the job `palw_attempt_job_v1` actually runs: P prefill positions, 0 decode calls). Line 580 \
         never consults `palw_prefill_draw`; line 581 hardcodes it true. `verification_ccu` is what \
         `palw_panel_share_permille_v1` charges the panel replay at, so the declared decode budget \
         — which no execution performs — moves the producer/panel split. Divergences: {violations:#?}"
    );
}

// =================================================================================================
// I3 — REWARD, CREDIT, COLLATERAL AND FORK WEIGHT ALL DERIVE FROM THE SAME WORK UNIT
// =================================================================================================

/// **I3. One work unit, read by reward, execution credit, collateral and fork weight.**
///
/// COVERS: for each of testnet-12's three genesis classes, the unit each of the four consumers
/// actually reads, computed with the runtime function at that site:
///   reward           `palw_attempted_compute_q32_per_claim_v1(draws, U2)`      — U2, raw MAC-eq
///   fork weight      `palw_pwu_v1(target, U2)` then x slash_value_per_pwu      — U2, raw MAC-eq
///   collateral       `palw_exposure_unit_pwu_v1(U2, floor_U1, floor_U2)` x 5   — U3, floor-normalised
///   execution credit raw U2 divided by `PALW_EXECUTION_QUANTUM_V1` (100_000)   — U2 over a U3 constant
/// The invariant is evaluated as: does one claim's slashable reservation and that same claim's
/// fraud gain (the two quantities the colluding-quorum inequality compares) use one unit? And does
/// the execution-quantum divisor meet its own declared unit?
///
/// DOES NOT COVER: it does not drive `palw_state_v2::record_round_final` or
/// `panel_valid_lock_required` — those are `pub(crate)`/fold-internal. It reproduces the expressions
/// at those sites from their pub components, which is weaker than executing them: if a call site
/// were changed to convert, this test would not notice. It also does not evaluate the free-prompt
/// lane's `palw_fp_compute_reserved_v1`, which is a third normalisation site.
#[test]
#[ignore = "OPEN FINDING (2026-09-23 audit I3): this test rebuilds the fold's unit arithmetic from pub parts, so it still reports the pre-fix U2-vs-U3 split that palw_audit_2026_09_23 closes inside the fold; it needs rewriting against the fold before it can pass. Run with --ignored."]
fn i3_reward_credit_collateral_and_weight_share_one_work_unit() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w0 = t12_work_floor_ccu(escrow);
    const SLASH_VALUE_PER_PWU: u128 = 5; // sompi per pwu; every t12 class registers 5.

    let floor_u1 = u1_declared_leaves(&b, &p, &floor_profile(), &floor_job());
    let floor_u2 = u2_draw_mac_eq(&floor_profile(), &floor_job());

    println!("\n=== I3: do the four consumers read one unit? ===");
    println!("  exposure basis: floor declared leaves (U1) = {floor_u1}, floor derived per draw (U2) = {floor_u2}");
    println!("  U2/U3 conversion factor = {:.1}x", floor_u2 as f64 / floor_u1 as f64);

    let mut worst_ratio = 0.0f64;
    let mut worst = String::new();
    let mut quantum_mismatch: Vec<String> = Vec::new();

    for (name, profile, job) in t12_classes() {
        let u2 = u2_draw_mac_eq(&profile, &job);
        let u1 = u1_declared_leaves(&b, &p, &profile, &job);
        let u3 = u3_exposure_pwu(u2, floor_u1, floor_u2);

        // The claim the chain writes past `palw_canonical_work` (armed at 0): pwu = attempts x U2.
        let target = palw_work_ticket_target_v1(u2, w0);
        let claim_pwu = palw_pwu_v1(target, u2.min(u64::MAX as u128) as u64);

        // Collateral actually reserved (palw_state_v2:18871): U3 x slash.
        let reserved_sompi = (u3 as u128).saturating_mul(SLASH_VALUE_PER_PWU);
        // Fraud gain's weight half (palw_panel_var_v1:60): claim.pwu (U2) x the SAME slash price.
        let fork_weight_sompi = (claim_pwu as u128).saturating_mul(SLASH_VALUE_PER_PWU);

        // Execution credit (palw_state_v2:10975-10983): raw U2 divided by a U3-declared quantum.
        let quanta_from_u2 = palw_execution_quantum_count_v1(u2, u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default());
        let quanta_from_u3 = palw_execution_quantum_count_v1(u3 as u128, u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default());

        let ratio = fork_weight_sompi as f64 / reserved_sompi.max(1) as f64;
        println!("  {name}");
        println!("    U1 declared leaves            = {u1}");
        println!("    U2 derived MAC-eq / draw      = {u2}");
        println!("    U3 floor-normalised pwu       = {u3}   (U3/U1 = {:.2}x)", u3 as f64 / u1 as f64);
        println!("    claim.pwu (U2 x attempts)     = {claim_pwu}");
        println!("    collateral RESERVED  (U3 x 5) = {reserved_sompi} sompi  ({:.6} MSK)", reserved_sompi as f64 / 1e8);
        println!("    fraud gain WEIGHT half (U2x5) = {fork_weight_sompi} sompi  ({:.6} MSK)", fork_weight_sompi as f64 / 1e8);
        println!("    gain / reservation            = {ratio:.1}x   <-- both are 'pwu x 5 sompi'");
        println!("    exec quanta from RAW U2       = {quanta_from_u2}");
        println!("    exec quanta from U3 (declared unit of PALW_EXECUTION_QUANTUM_V1) = {quanta_from_u3}");

        if quanta_from_u2 != quanta_from_u3 {
            quantum_mismatch.push(format!("{name}: {quanta_from_u2} from U2 vs {quanta_from_u3} from U3"));
        }
        if ratio > worst_ratio {
            worst_ratio = ratio;
            worst = name.to_string();
        }
    }

    // THE INVARIANT, evaluated as ONE assertion so that neither half masks the other.
    //
    // Half one: the two quantities the colluding-quorum inequality compares — the gain a lying
    // producer takes and the collateral that claim put at risk — are the same pwu through the same
    // sompi-per-pwu price, so their ratio must be 1.
    //
    // Half two: the execution-quantum divisor meets the unit it declares.
    let mut i3: Vec<String> = Vec::new();
    if (worst_ratio - 1.0).abs() >= 1e-9 {
        i3.push(format!(
            "COLLATERAL vs FORK WEIGHT: `palw_state_v2.rs:18871` reserves \
             `palw_exposure_pwu_v3(..) x slash_value_per_pwu` — U3, floor-normalised — while \
             `palw_panel_var_v1.rs:60` computes the SAME claim's fraud gain as \
             `claim.pwu x slash_value_per_pwu` with `claim.pwu` in RAW U2 MAC-eq. Both call the \
             constant `slash_value_per_pwu = 5 sompi/pwu`, but the two pwu are different units. \
             Worst measured class: {worst}, gain/reservation = {worst_ratio:.1}x. The seat lock \
             `palw_seat_lock_required_v2` is sized from the U2 side and the collateral that backs \
             it from the U3 side."
        ));
    } else {
        println!("  collateral vs fork weight: HOLDS");
    }
    if !quantum_mismatch.is_empty() {
        i3.push(format!(
            "EXECUTION CREDIT: `palw_state_v2.rs:10975-10983` sets \
             `credit = palw_exposure_pwu_v2(..)` — RAW U2 MAC-eq, the un-normalised draw — and \
             divides it by `PALW_EXECUTION_QUANTUM_V1 = 100_000`, whose own doc \
             (`palw_execution_quanta_v1.rs:44-47`) declares it 'in the same units \
             PalwExecFinalV1::credit is stored in (exposure pwu, capped)'. Quanta minted from the \
             unit the code divides, against the unit the constant declares: {quantum_mismatch:#?}"
        ));
    } else {
        println!("  execution credit unit: HOLDS");
    }
    assert!(i3.is_empty(), "I3 VIOLATED — the four consumers do not read one work unit:\n{}", i3.join("\n\n"));
}

// =================================================================================================
// I4 — CHANGING ONLY THE REPRESENTATION DOES NOT INCREASE ECONOMIC VALUE
// =================================================================================================

/// **I4. A change that alters no executed arithmetic buys no economic value.**
///
/// COVERS two representation-only levers on the real t12 dense row (Qwen2.5 A16 graph-v7 @ 2M),
/// each measured through the runtime:
///   (a) `tile_len` — re-tiling the SAME operands. Measured against `palw_attempt_economic_compute_v1`
///       (the price) and against `shape_profile_id()` (the chain's class identity).
///   (b) `n_threads` — a host scheduling hint. `validate_shape` refuses `flash_attn_disabled != 1`
///       precisely so a thread count cannot change an answer, and the cost table never reads it.
/// For each, the invariant is that the price does not rise AND that the chain identity does not
/// split — because two class ids over one artifact are two independent full-strength draw lanes
/// past `palw_work_target` (armed at 0: the ticket is `MAX * min(1, CCU/W0)`, which reads neither
/// share nor class count), i.e. twice the economic value for one model.
///
/// DOES NOT COVER: it does not prove the executed arithmetic is unchanged — that would need an
/// executor, which `kaspa-consensus-core` cannot link. It infers it from `weight_dtypes`,
/// `out_len`, `kernel_semantics_id` and `input_refs` being identical across the pair. It sweeps two
/// levers of the ~14 `PalwShapeProfileV3` fields that `canonical_class_id_v1` excludes; the
/// `weight_dtypes` declaration and the `.routed` weight-name suffix are separate representation
/// levers that move the PRICE and are not swept here.
#[test]
#[ignore = "OPEN FINDING (2026-09-23 audit I4, not fixed on this branch): n_threads+1 splits a class id at identical price and root, so one artifact can register more lanes. Run with --ignored."]
fn i4_representation_alone_buys_no_economic_value() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w0 = t12_work_floor_ccu(escrow);

    let honest = dense_2m();
    let job = dense_job();
    let honest_u2 = u2_draw_mac_eq(&honest, &job);
    let honest_id = honest.shape_profile_id();
    let honest_target = palw_work_ticket_target_v1(honest_u2, w0);
    let honest_reward = priced_reward_sompi(honest_u2, w0, escrow);

    println!("\n=== I4: does representation alone buy value? ===");
    println!("  honest dense v7@2M");
    println!("    shape_profile_id  = {honest_id}");
    println!("    U2 (MAC-eq/draw)  = {honest_u2}");
    println!("    ticket target     = {honest_target}  ({:.9} of MAX)", honest_target as f64 / u128::MAX as f64);
    println!("    reward (sompi)    = {honest_reward}  ({:.4} MSK)", msk(honest_reward));

    let mut price_moves: Vec<String> = Vec::new();
    let mut identity_splits: Vec<String> = Vec::new();

    // (a) re-tiling, and (b) the thread count.
    let mut retiled = dense_2m();
    for n in retiled.attn_nodes.iter_mut().chain(retiled.gdn_nodes.iter_mut()) {
        n.tile_len = if n.tile_len == 4096 { 256 } else { 4096 };
    }
    let mut rethreaded = dense_2m();
    rethreaded.n_threads = honest.n_threads.saturating_add(1);

    for (lever, twin) in [("tile_len re-tiled", retiled), ("n_threads + 1", rethreaded)] {
        let twin_u2 = u2_draw_mac_eq(&twin, &job);
        let twin_id = twin.shape_profile_id();
        let twin_target = palw_work_ticket_target_v1(twin_u2, w0);
        let twin_reward = priced_reward_sompi(twin_u2, w0, escrow);
        let admitted = admit(&p, &b, &twin, &job, PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT);

        // The executable content that the executor and the leaf enumeration read.
        let same_widths = twin.attn_nodes.iter().map(|n| (n.out_len, n.kernel_semantics_id, n.weight_dtypes.clone()))
            .eq(honest.attn_nodes.iter().map(|n| (n.out_len, n.kernel_semantics_id, n.weight_dtypes.clone())));

        println!("  lever: {lever}");
        println!("    same out_len/kernel_id/dtypes = {same_widths}");
        println!("    U2 (MAC-eq/draw)  = {twin_u2}  ({:.6}x)", twin_u2 as f64 / honest_u2 as f64);
        println!("    shape_profile_id  = {twin_id}");
        println!("    identity split    = {}", twin_id != honest_id);
        println!("    ticket target     = {twin_target}  ({:.9} of MAX)", twin_target as f64 / u128::MAX as f64);
        println!("    reward (sompi)    = {twin_reward}  ({:.4} MSK)", msk(twin_reward));
        println!("    gate verdict      = {}", match &admitted { Ok(l) => format!("ADMITTED (leaves {l})"), Err(e) => format!("REFUSED {e:?}") });

        if twin_u2 == honest_u2 {
            println!("    price: HOLDS — this lever buys no price");
        }
        if twin_id == honest_id {
            println!("    identity: HOLDS — one model, one chain class id");
        } else if admitted.is_err() {
            println!("    identity: the id splits, but the GATE REFUSES the twin — this lever is closed at admission");
        }
        if twin_u2 > honest_u2 {
            price_moves.push(format!("{lever}: U2 {twin_u2} > honest {honest_u2} ({:.6}x)", twin_u2 as f64 / honest_u2 as f64));
        }
        if twin_id != honest_id && admitted.is_ok() {
            identity_splits.push(format!(
                "{lever}: a second class id {twin_id} over the SAME artifact root, ADMITTED, priced \
                 identically at {twin_u2} MAC-eq/draw and drawn at the same target {twin_target} — a \
                 second full-strength lottery lane, reward {twin_reward} sompi ({:.4} MSK) per Final",
                msk(twin_reward)
            ));
        }
    }

    // THE INVARIANT, half one: the price does not rise for a representation change.
    assert!(price_moves.is_empty(), "I4 VIOLATED (price): {price_moves:#?}");

    // THE INVARIANT, half two: representation does not mint a second economic identity.
    assert!(
        identity_splits.is_empty(),
        "I4 VIOLATED (identity): a field that changes no executed arithmetic mints a SECOND chain \
         class id over the same artifact root, and ADR-0143's 'one owner per artifact root' does not \
         stop it because `artifact_owners` is keyed `(class_id, root)` \
         (`palw_state_v2.rs:5752`, gate at `:9719-9737`) — a different class id is a different map \
         key, so the duplicate is written, not refused. Past `palw_work_target` (armed at DAA 0) the \
         ticket is `MAX * min(1, CCU/W0)` and reads neither the share nor the class count \
         (`palw_state_v2.rs:14706`), so N duplicate rows are N independent full-strength lanes, not N \
         slices of one. Registration exposure is 40,000 sompi (0.0004 MSK) per extra row. \
         Splits: {identity_splits:#?}"
    );
}

// =================================================================================================
// I5 — THE SAME WORK CAN BE MONETISED EXACTLY ONCE
// =================================================================================================

/// **I5. One execution is paid once.**
///
/// COVERS: the REAL fold. `apply_palw_transition_v7` is driven over three chain blocks with the
/// 2026-09-11 deep fence ARMED (`audit_2026_09_11_deep_active: true` — what testnet-12 runs from
/// DAA 0). One inference — one `trace_root`, one `output_root`, one `execution_root`, one
/// `pwu` — is announced twice, as two envelopes differing ONLY in `nonce` (both inside one
/// `PALW_TICKET_NONCE_BUCKET_LOG2 = 22` bucket) and `timestamp`, the two fields
/// `execution_commitment_v3` blanks and `execution_anchor_v3` buckets. The test first proves the two
/// carry ONE execution key, then reads back from the folded state how many claims, how much escrow
/// and how much exposure that one execution minted.
///
/// DOES NOT COVER: this is a synthetic `PalwStateParamsV2` fixture class, not the t12 genesis
/// registry — `PalwChainStateV2::genesis()` plus a registration is the only state a test outside
/// `consensus/src` can fold. The escrow figure is therefore driven by the carve (720 permille, the
/// t12 fence) applied to the t12 block subsidy passed in as `ctx.subsidy`, not read from a t12
/// coinbase. It also does not bound the replay: the residual limit is the DAG's merge depth (30 on
/// t12) against `palw_v2_merged_works`, which needs the virtual processor and is NOT evaluated here.
/// N = 2 is a lower bound on the multiple, not the multiple.
#[test]
fn i5_the_same_work_is_monetised_exactly_once() {
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
        attempt_trace_manifest_root_v1, challenge_v2, execution_anchor_v3, execution_commitment_v3,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }
    const NET: u64 = 999;
    const PPH: u64 = 5;
    const CLASS: u64 = 1;
    let bond_key = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 });
    let bond = bond_key.0;

    let subsidy = T12_BLOCK_SUBSIDY_SOMPI;
    let carve = t12().palw_overlay_carve.expect("carve").worker_carve_permille;
    let sp = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(CLASS), 4, 1000, 100, 1000, 0)
        .expect("state params")
        .with_fp_quanta(8, 64)
        .expect("fp quanta")
        .with_worker_carve_permille(carve)
        .expect("the t12 carve");
    // t12 arms every fence from DAA 0 — the deep dedup AND the 2026-09-23 rooted one.
    let extras = PalwTransitionExtrasV1 { audit_2026_09_11_deep_active: true, audit_2026_09_23_active: true, ..Default::default() };

    let setup = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h(CLASS),
            artifact_root: h(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::BondRegistered {
            bond: bond_key,
            pubkey: vec![7; 4],
            operator_pubkey: vec![21u8; 8],
            collateral: 1_000_000_000_000_000,
            payout_payload: Hash64::from_u64_word(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ];

    let sibling = |nonce: u64, timestamp: u64| PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(NET),
            challenge: challenge_v2(h(NET), h(PPH), timestamp, nonce, h(CLASS), &bond),
            class_id: h(CLASS),
            executor_bond: bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(11),
            trace_root: h(31),
            output_root: h(32),
            pwu: 160,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(31), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 999_999,
            execution_root: h(41),
        },
        signature: vec![0; 8],
    };
    let ctx = |block: u64, daa: u64, blue: u64, sub: u64| PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: sub };

    let (s1, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(), &sp, None, &ctx(1, 100, 1, 0), &setup,
        PalwBlockWorkV3::None, &[], Hash64::default(), false, false, false, false, &extras,
    )
    .expect("the registration folds");

    let a = sibling(7, 1_700_000_000);
    let bb = sibling(8, 1_700_060_000);
    let id_a = attempt_id_v2(&a.attempt);
    let id_b = attempt_id_v2(&bb.attempt);
    let key_a = execution_commitment_v3(&a.attempt, execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 7));
    let key_b = execution_commitment_v3(&bb.attempt, execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 8));

    let (s2, _, _) = apply_palw_transition_v7(
        &s1, &sp, None, &ctx(2, 101, 2, subsidy), &[], PalwBlockWorkV3::Attempt(&a), &[], key_a, false, false, false, false, &extras,
    )
    .expect("block 2 stands");
    // Past `palw_audit_2026_09_23` the second announcement of ONE execution is refused on sight —
    // its key is already in the rooted `work_ids` index — and that refusal IS the invariant. Below
    // the fence block 3 stood and minted a second claim; that is the defect this test recorded.
    let s3 = match apply_palw_transition_v7(
        &s2, &sp, None, &ctx(3, 102, 3, subsidy), &[], PalwBlockWorkV3::Attempt(&bb), &[], key_b, false, false, false, false, &extras,
    ) {
        Ok((s3, _, _)) => s3,
        Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::DuplicateWork { work_id, claim }) => {
            println!("  block 3 REFUSED: execution key {work_id} is already claimed by {claim} (rooted index)");
            s2.clone()
        }
        Err(e) => panic!("block 3 failed for an unexpected reason: {e}"),
    };

    let c_a = s3.claim(&id_a);
    let c_b = s3.claim(&id_b);
    let escrow_a = c_a.map(|c| c.escrowed_reward).unwrap_or(0);
    let escrow_b = c_b.map(|c| c.escrowed_reward).unwrap_or(0);
    let total_escrow = escrow_a.saturating_add(escrow_b);
    let claims = [c_a.is_some(), c_b.is_some()].iter().filter(|x| **x).count();

    println!("\n=== I5: one execution, how many payments? ===");
    println!("  deep fence armed (t12 runs this from DAA 0) = {}", extras.audit_2026_09_11_deep_active);
    println!("  nonce bucket log2                            = {}", kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2);
    println!("  sibling A attempt_id                         = {id_a}");
    println!("  sibling B attempt_id                         = {id_b}");
    println!("  sibling A execution key                      = {key_a}");
    println!("  sibling B execution key                      = {key_b}");
    println!("  one execution key for both                   = {}", key_a == key_b);
    println!("  claims minted from that ONE execution        = {claims}");
    println!("  escrow A (sompi)                             = {escrow_a}  ({:.4} MSK)", msk(escrow_a));
    println!("  escrow B (sompi)                             = {escrow_b}  ({:.4} MSK)", msk(escrow_b));
    println!("  TOTAL escrow for ONE inference               = {total_escrow}  ({:.4} MSK)", msk(total_escrow));
    println!("  bond exposure reserved                       = {}", s3.reserved_exposure(&bond_key));

    assert_ne!(id_a, id_b, "precondition: the two announcements are distinct claim ids");
    assert_eq!(key_a, key_b, "precondition: they are ONE execution — the commitment blanks the challenge and the anchor buckets the nonce");
    assert!(escrow_a > 0, "precondition: the first announcement is paid");

    // THE INVARIANT.
    assert_eq!(
        claims, 1,
        "I5 VIOLATED: ONE inference (one trace_root, one output_root, one execution_root, one \
         execution key {key_a}) minted {claims} claims across {claims} chain blocks and \
         {total_escrow} sompi ({:.4} MSK) of escrow — {:.2}x the escrow of a single claim — for ZERO \
         additional MAC-eq. The dedup that should stop it is `TransitionBuilder::seen_exec` \
         (`palw_state_v2.rs:8548`), a `HashSet` constructed EMPTY for every chain block at `:8571`; \
         the only rooted dedup is `claims.contains_key(attempt_id_v2(attempt))` at `:18832`, and \
         `attempt_id_v2` hashes the whole struct including `challenge`, which carries the nonce and \
         the timestamp. `PalwChainStateV2` holds no execution-key map, so the cross-block half of the \
         rule is absent rather than dormant.",
        msk(total_escrow),
        total_escrow as f64 / escrow_a as f64,
    );
}

// =================================================================================================
// I6 — A NEW MODEL CLASS CANNOT PRODUCE AN ABNORMAL REWARD/WORK RATIO
// =================================================================================================

/// **I6. Every admissible class pays about the same per unit of real work.**
///
/// COVERS: the reward-per-real-MAC-eq of the three classes testnet-12 registers, and of one class a
/// stranger may register through the live permissionless path (`palw_model_registry` and
/// `palw_admission_independence` both armed at DAA 0), measured through the runtime chain
/// `U2 -> palw_work_ticket_target_v1 -> palw_expected_attempts_q32_v1 ->
/// palw_attempted_compute_q32_per_claim_v1 -> palw_rate_priced_reward_v1` against the real MAC-eq
/// that producer burns per paid claim. The newcomer is the BASE-0 floor with two scalars rewritten
/// — the I1 profile — so its real cost is the floor's own, measured, not assumed. The invariant is
/// that a newly registered class's MSK-per-giga-real-MAC-eq lies within a band of the existing
/// classes'.
///
/// DOES NOT COVER: the newcomer is ONE synthetic class built from ONE lever. This test is not a
/// search: it does not establish that the band it measures is the worst an admissible registration
/// can reach, only that the band is not closed. It also holds `block_bits = 0` (no bits-priced lane
/// exists on t12), so the `palw_network_draws_q32_from_bits_v1` factor is one throughout; a chain
/// that later prices a lane by bits would move every row.
#[test]
fn i6_a_new_class_cannot_pay_an_abnormal_rate() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w0 = t12_work_floor_ccu(escrow);

    // The newcomer: the floor's own node tables, two scalars rewritten. Its REAL cost is the
    // floor's own price, because its node tables are the floor's.
    let honest_floor_u2 = u2_draw_mac_eq(&floor_profile(), &floor_job());
    let mut newcomer = floor_profile();
    newcomer.attn_heads = 65_535;
    newcomer.attn_head_dim = 41_854;

    let mut rows: Vec<(String, f64, bool)> = Vec::new(); // (name, MSK per giga real MAC-eq, is_newcomer)

    println!("\n=== I6: reward per unit of REAL work, by class ===");
    println!("  {:<26} {:>22} {:>22} {:>16} {:>14}", "class", "U2 declared", "real MAC-eq / paid", "reward (MSK)", "MSK / G real");
    for (name, profile, job) in t12_classes() {
        let u2 = u2_draw_mac_eq(&profile, &job);
        let reward = priced_reward_sompi(u2, w0, escrow);
        let real = real_work_per_paid_claim(u2, u2, w0);
        let rate = msk(reward) / (real as f64 / 1e9);
        println!("  {name:<26} {u2:>22} {real:>22} {:>16.4} {rate:>14.4}", msk(reward));
        rows.push((name.to_string(), rate, false));
    }

    let n_u2 = u2_draw_mac_eq(&newcomer, &floor_job());
    let n_reward = priced_reward_sompi(n_u2, w0, escrow);
    let n_real = real_work_per_paid_claim(n_u2, honest_floor_u2, w0);
    let n_rate = msk(n_reward) / (n_real as f64 / 1e9);
    let n_admit = admit(&p, &b, &newcomer, &floor_job(), Hash64::from_u64_word(0xA271));
    println!("  {:<26} {n_u2:>22} {n_real:>22} {:>16.4} {n_rate:>14.4}   <- NEWCOMER, gate: {}",
        "newcomer (floor + 2 scalars)", msk(n_reward),
        match &n_admit { Ok(l) => format!("ADMITTED (leaves {l})"), Err(e) => format!("REFUSED {e:?}") });
    rows.push(("newcomer".to_string(), n_rate, true));

    let existing_max = rows.iter().filter(|r| !r.2).map(|r| r.1).fold(0.0f64, f64::max);
    let existing_min = rows.iter().filter(|r| !r.2).map(|r| r.1).fold(f64::INFINITY, f64::min);
    println!("  existing-class band: {existing_min:.4} .. {existing_max:.4} MSK per giga real MAC-eq");
    println!("  newcomer: {n_rate:.4}  ({:.2}x the existing maximum)", n_rate / existing_max);

    // THE INVARIANT: a newly admissible class's rate must not exceed the existing band. A 2x
    // allowance is generous — the three shipped rows sit inside a 1.0x band of each other.
    if n_admit.is_ok() {
        assert!(
            n_rate <= existing_max * 2.0,
            "I6 VIOLATED: the gate ADMITTED a class paying {n_rate:.4} MSK per giga of REAL MAC-eq \
             against an existing-class band of {existing_min:.4}..{existing_max:.4} — {:.2}x the \
             maximum any shipped class earns. Nothing in `verify_class_admission_v9` compares a \
             registration's declared work against the work of the classes already registered: the \
             only bounds on the declaration are `counted <= worst` (the deepest-legal-job leaf bound) \
             and `pwu_per_inference == counted` (`palw_class_admission_v2.rs:2276-2289`), both of \
             which are leaf counts and neither of which is a price. The reward saturates at the \
             escrow ({escrow} sompi), so the ceiling on the abuse is W0/real_cost, not a rule.",
            n_rate / existing_max
        );
    }
}

// =================================================================================================
// I7 — CONSENSUS-CRITICAL IDENTITY NEVER DEPENDS ON A HAND-ENTERED HASH
// =================================================================================================

/// **I7. No hash consensus depends on is transcribed by hand.**
///
/// COVERS: the three artifact roots testnet-12 registers at genesis, read back FROM THE CARD
/// (`palw_t12_shipped_params()` -> the ConsensusV2 bundle's `ClassRegistered` objects), each matched
/// to its class by `profile.shape_profile_id()`. For each root the test asks whether this build can
/// re-derive it, and answers from the runtime rather than from prose:
///   - the dense 2M root against `class_manifest_const_v1::inventory_root_of_class(MANIFEST, 1)` —
///     a `const fn` parse of the committed `.palwmanifest`, and against
///     `artifact_digest_of(MANIFEST)`, the substitution that shut two networks' dense tiers
///     (t11 `c00faa48…`, t12 `b5baca63…`);
///   - the manifest's positional class-id read against the dense profile's own
///     `shape_profile_id()`, which is what makes the positional index safe;
///   - the floor and hybrid roots against every derivation reachable from this crate.
/// It also confirms the class IDS are all derived (they are `shape_profile_id()` of a profile this
/// build constructs), so the gap is specifically in the ROOT slots.
///
/// DOES NOT COVER: it cannot re-derive the floor root, because that derivation
/// (`misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1`) lives in a crate that DEPENDS on
/// `kaspa-consensus-core` and so cannot be linked from here — a CI run of `misaka-palw-base0`'s own
/// `the_pinned_rc_artifact_root_is_the_one_the_floor_derives` does cover it, and this test says so
/// rather than counting it against the invariant. It cannot re-derive the 2M inventory root from the
/// 2.87 GB artifact either (that is the `#[ignore]`d `a16_root_probe`), so "derived from the
/// committed manifest" is as far as this test reaches: the manifest's own binding to the file is one
/// `artifact_digest` field, and nothing in consensus ever measures it.
#[test]
#[ignore = "OPEN FINDING (2026-09-23 audit I7, not fixed on this branch): two of testnet-12's three registered roots are Hash64 literals with no in-tree derivation. Run with --ignored."]
fn i7_consensus_identity_never_rests_on_a_hand_entered_hash() {
    let p = t12();
    let b = bundle_of(&p);

    let registered: Vec<(Hash64, Hash64)> = b
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => Some((*class_id, *artifact_root)),
            _ => None,
        })
        .collect();

    println!("\n=== I7: which of the card's roots can this build re-derive? ===");
    println!("  registered classes on the t12 card = {}", registered.len());

    // Every class ID is derived: it is the profile's own shape_profile_id.
    let mut ids_derived = 0usize;
    for (name, profile, _) in t12_classes() {
        let want = profile.shape_profile_id();
        let found = registered.iter().any(|(id, _)| *id == want);
        println!("  class id {name:<20} derived from the profile, on the card = {found}   ({want})");
        assert!(found, "the card registers the class this build's profile describes: {name}");
        ids_derived += 1;
    }

    let root_of = |profile: &PalwShapeProfileV3| -> Hash64 {
        let want = profile.shape_profile_id();
        registered.iter().find(|(id, _)| *id == want).map(|(_, r)| *r).expect("the class is on the card")
    };
    let floor_root = root_of(&floor_profile());
    let hybrid_root = root_of(&hybrid_512());
    let dense_root = root_of(&dense_2m());

    // --- the dense root: derived, in this crate, from the committed manifest ---------------------
    let m = manifest::QWEN25_A16_2M_MANIFEST_V1;
    let manifest_root = manifest::inventory_root_of_class(m, 1);
    let manifest_class = manifest::class_id_of_class(m, 1);
    let manifest_digest = manifest::artifact_digest_of(m);
    println!("\n  dense 2M root on the card          = {dense_root}");
    println!("  inventory_root_of_class(MANIFEST,1) = {manifest_root}");
    println!("  artifact_digest_of(MANIFEST)        = {manifest_digest}");
    println!("  class_id_of_class(MANIFEST,1)       = {manifest_class}");
    println!("  dense profile shape_profile_id      = {}", dense_2m().shape_profile_id());

    assert_eq!(dense_root, PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT, "the card registers the constant");
    assert_eq!(dense_root, manifest_root, "the dense root IS the committed manifest's inventory root — derived, not transcribed");
    assert_eq!(manifest_class, dense_2m().shape_profile_id(), "the positional manifest read is bound to the class this card registers");
    assert_ne!(
        dense_root, manifest_digest,
        "I7: the dense root is a copy of the artifact DIGEST again — the substitution that shut the \
         dense tier of testnet-11 (c00faa48…) and testnet-12 (b5baca63…)"
    );
    println!("  => dense root: DERIVED in this crate, and != the digest in the same file. The 2026-09-23 incident is currently CLOSED for this row.");

    // --- the floor and hybrid roots: is there any derivation reachable from here? -----------------
    // The card's own constants are `Hash64::from_bytes([..])` literals. If a derivation existed in
    // this crate the constant would be a call, as the dense row's is. Measured structurally:
    let card_source = include_str!("../src/config/params.rs");
    fn const_body<'a>(src: &'a str, name: &str) -> &'a str {
        let at = src.find(&format!("pub const {name}: crate::Hash64")).unwrap_or_else(|| panic!("{name} is declared"));
        let tail = &src[at..];
        let end = tail.find("]);").map(|e| e + 3).unwrap_or(tail.len().min(2_000));
        &tail[..end.min(tail.len())]
    }
    let floor_src = const_body(card_source, "PALW_RC_GENESIS_ARTIFACT_ROOT");
    let hybrid_src = const_body(card_source, "PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT");
    let floor_is_literal = floor_src.contains("Hash64::from_bytes([");
    let hybrid_is_literal = hybrid_src.contains("Hash64::from_bytes([");

    println!("\n  floor  root on the card = {floor_root}");
    println!("    PALW_RC_GENESIS_ARTIFACT_ROOT is a byte literal        = {floor_is_literal}");
    println!("    a derivation exists, in misaka-palw-base0 (rc.rs:115, NOT #[ignore]d), which cannot");
    println!("    be linked from kaspa-consensus-core — so CI does cover this one, elsewhere.");
    println!("  hybrid root on the card = {hybrid_root}");
    println!("    PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT is a byte literal = {hybrid_is_literal}");
    println!("    grep the tree: no function anywhere derives it. It was re-pinned 2026-08-27 after a");
    println!("    prior pin (c970d693…) that no converter could reproduce.");

    assert_eq!(floor_root, PALW_RC_GENESIS_ARTIFACT_ROOT);
    assert_eq!(hybrid_root, PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT);
    assert_eq!(ids_derived, 3, "all three class ids are derived");

    let literals: Vec<&str> = [("PALW_RC_GENESIS_ARTIFACT_ROOT", floor_is_literal), ("PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT", hybrid_is_literal)]
        .iter()
        .filter(|(_, lit)| *lit)
        .map(|(n, _)| *n)
        .collect();

    // THE INVARIANT.
    assert!(
        literals.is_empty(),
        "I7 VIOLATED: {} of {} registered artifact roots are hand-entered byte literals on the t12 \
         card — {literals:?}. The dense row (74c67e63…) is the one that is derived, and only because \
         the 2026-09-23 incident forced it: its constant is \
         `class_manifest_const_v1::inventory_root_of_class(QWEN25_A16_2M_MANIFEST_V1, 1)`, a const-fn \
         parse of a committed 660-byte manifest. The other two were left as they were. \
         `PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT` has NO derivation anywhere in the tree and a recorded \
         history of having been pinned to a value (c970d693…) no converter could reproduce; \
         `PALW_RC_GENESIS_ARTIFACT_ROOT` does have one, in `misaka-palw-base0`, which a \
         `kaspa-consensus-core` run cannot reach. Separately, and not counted here because this crate \
         cannot express it: `class_manifest_const_v1::inventory_root_of_class` and \
         `::artifact_digest_of` are adjacent `const fn`s over the SAME string returning the SAME raw \
         `Hash64` type, so the substitution that shipped twice is one identifier apart at \
         `params.rs:10737`, guarded by a single hand-written `assert_ne!` in `t12_regenesis.rs:288` \
         covering 1 of 3 roots.",
        literals.len(),
        registered.len(),
    );
}
