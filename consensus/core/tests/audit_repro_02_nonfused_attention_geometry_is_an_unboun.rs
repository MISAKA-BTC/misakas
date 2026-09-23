//! REPRO 02 — "a non-fused class's `attn_heads x attn_head_dim` price the attention nodes and are
//! bounded by nothing".
//!
//! Independent re-derivation of the finding, against this worktree's runtime only. Nothing here is
//! reimplemented: every number below comes out of a `pub` consensus function.
//!
//! The chain measured, with the unit of every quantity named:
//!
//!   profile scalars `attn_heads` (count) x `attn_head_dim` (count)
//!     -> `palw_economic_compute_v1::node_cost`, the `over_cache` arm  (MAC-eq per cached position)
//!     -> `palw_attempt_economic_compute_v1(profile, canonical, true, TABLE)`  = ccu/draw, MAC-eq
//!        == `palw_model_work_from_carriage_v1(..).economic_ccu_per_claim`     (MAC-eq per DRAW)
//!     -> `palw_work_ticket_target_v1(ccu, W0)` = MAX * min(1, ccu/W0)         (u128 target)
//!     -> `palw_expected_attempts_q32_v1(target)`                              (Q32 draws)
//!     -> `palw_claim_economics_snapshot_v1(..).priced_reward(escrow)`         (sompi)
//!        which is verbatim what `palw_state_v2.rs:11866` pays a Final.
//!
//! beside it, the arithmetic the producer really runs: draws_q32 (x) ccu of the HONEST floor.
//!
//! The forgery is the BASE-0 liveness floor with exactly two scalar fields rewritten. Every node
//! table, every `out_len`, the declared leaf count and the canonical job are untouched — the test
//! asserts that, so the price move cannot be attributed to anything the executor would notice.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_attempted_compute_q32_per_claim_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{PalwEconomicPayoutFoldV1, palw_claim_economics_snapshot_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// ------------------------------------------------------------------ the t12 card

/// The block subsidy a t12 block pays. Not derivable from `kaspa-consensus-core` (the subsidy
/// table lives behind `CoinbaseManager` in `kaspa-consensus`), so it is pinned here with the same
/// value the tree pins it with in two places it IS checked:
/// `consensus/core/src/palw_state_v2.rs:31019` and `consensus/core/src/config/params.rs:22303`,
/// both `const T11_SUBSIDY: u64 = 444_562_014_000;` = `YEAR1_PER_BLOCK_TWO_MINUTE`, 4,445.62 MSK.
/// t12 runs the same 120,000 ms cadence with `deflationary_phase_daa_score = 0`.
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

/// The escrow a t12 claim carries, through the runtime's own carve function
/// (`PalwStateParamsV2::worker_carve_at`, the one `palw_state_v2` escrows with).
fn escrow_sompi(p: &Params, b: &PalwConsensusParamsV2) -> u64 {
    let carve = p.palw_overlay_carve.expect("t12 arms palw_overlay_carve").worker_carve_permille;
    b.state.worker_carve_at(T12_BLOCK_SUBSIDY_SOMPI, Some(PalwRewardParamsV2::new(carve).expect("carve")))
}

// ------------------------------------------------------------------ the profiles

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the BASE-0 liveness floor")
}

fn floor_job() -> PalwJobContextV2 {
    rc_job_context(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)
}

/// The floor with EXACTLY two scalars rewritten. Node tables untouched.
fn forged(attn_heads: u16, attn_head_dim: u32) -> PalwShapeProfileV3 {
    let mut v = floor_profile();
    v.attn_heads = attn_heads;
    v.attn_head_dim = attn_head_dim;
    v
}

/// `palw_attempt_economic_compute_v1` — the prefill-draw price, MAC-eq per draw. This IS
/// `economic_ccu_per_claim` (`palw_model_registry_v1.rs:581`) and, by the pin at
/// `palw_canonical_work_v1.rs:889`, also the fork-weight scalar `provisional_scalar_v1()`.
fn ccu_per_draw(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("priced")
}

// ------------------------------------------------------------------ the live gate

/// The t12 admission gate at DAA 0, argument for argument as `processor.rs:6886` calls it.
/// `share_permille = 0` is what `palw_admission_independence` (armed at 0 on t12) requires of a
/// post-genesis registration.
fn admit(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    artifact_root: Hash64,
) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(p, b, profile, 0).expect("admission shape");
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    };
    let declared_leaves = step_leaf_count_capped_v1(profile, job, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: declared_leaves },
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

// ------------------------------------------------------------------ the payout path

#[derive(Debug, Clone, Copy)]
struct Paid {
    ccu_declared: u128,
    target: u128,
    draws_q32: u128,
    attempted_ccu: u128,
    reward_sompi: u64,
    real_mac_eq_per_claim: u128,
}

/// The reward a Final of this class is paid, through the SAME two calls the fold makes:
/// `palw_claim_economics_snapshot_v1(..)` then `.priced_reward(escrow)` (`palw_state_v2.rs:11866`).
/// `carrying_bits = 0` because t12 has no bits-priced lane, which prices the network draw at
/// exactly one (`palw_network_draws_q32_from_bits_v1`).
fn paid(
    p: &Params,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    ccu_real: u128,
    w0: u128,
    escrow: u64,
) -> Paid {
    let fence = p.palw_economic_payout.expect("t12 arms palw_economic_payout");
    let fold = PalwEconomicPayoutFoldV1 {
        rate_sompi_per_giga: fence.rate_sompi_per_giga,
        panel_share_alpha_permille: fence.panel_share_alpha_permille,
        panel_share_min_permille: fence.panel_share_min_permille,
        panel_share_max_permille: fence.panel_share_max_permille,
        cap_utilization_max_permille: fence.cap_utilization_max_permille,
        block_bits: 0,
    };
    let work = palw_model_work_from_carriage_v1(profile, job).expect("the registry's work row");
    let target = palw_work_ticket_target_v1(work.economic_ccu_per_claim, w0);
    let snapshot = palw_claim_economics_snapshot_v1(&fold, &work, 5, target, 0);
    Paid {
        ccu_declared: work.economic_ccu_per_claim,
        target,
        draws_q32: snapshot.expected_attempts_q32,
        attempted_ccu: snapshot.attempted_ccu(),
        reward_sompi: snapshot.priced_reward(escrow),
        real_mac_eq_per_claim: palw_attempted_compute_q32_per_claim_v1(snapshot.expected_attempts_q32, ccu_real),
    }
}

fn show(tag: &str, x: &Paid) {
    println!("  {tag}");
    println!("    ccu declared        {:>22} MAC-eq / draw", x.ccu_declared);
    println!("    ticket target       {} ({:.9} of u128::MAX)", x.target, x.target as f64 / u128::MAX as f64);
    println!(
        "    expected draws      {:.6} (Q32 {})",
        x.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
        x.draws_q32
    );
    println!("    attempted_ccu       {:>22} MAC-eq (declared, what is paid on)", x.attempted_ccu);
    println!("    priced_reward       {:>22} sompi = {:.4} MSK", x.reward_sompi, x.reward_sompi as f64 / 1e8);
    println!("    REAL arithmetic     {:>22} MAC-eq / paid claim", x.real_mac_eq_per_claim);
    println!(
        "    MSK per G real MAC-eq {:>20.4}",
        (x.reward_sompi as f64 / 1e8) / (x.real_mac_eq_per_claim as f64 / 1e9)
    );
}

// ==================================================================== the exploit

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: reproduces the behaviour below palw_audit_2026_09_23; on testnet-12 (armed at DAA 0) the fence refuses it and the sibling regression test is the live assertion"]
fn repro02_the_attention_geometry_is_an_unbounded_price_multiplier() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = escrow_sompi(&p, &b);
    let rate = p.palw_economic_payout.expect("armed").rate_sompi_per_giga;
    let w0 = palw_work_floor_v1(escrow, rate);

    let honest = floor_profile();
    let job = floor_job();
    let cheat = forged(65_535, 41_854);

    println!("\n================ REPRO 02: non-fused attention geometry ================");
    println!("  network             testnet-12 (palw_t12_shipped_params)");
    println!("  block subsidy       {T12_BLOCK_SUBSIDY_SOMPI} sompi");
    println!("  worker carve        {} permille", p.palw_overlay_carve.unwrap().worker_carve_permille);
    println!("  escrow per claim    {escrow} sompi = {:.4} MSK", escrow as f64 / 1e8);
    println!("  rate                {rate} sompi per 1e9 MAC-eq");
    println!("  W0 = escrow*1e9/rate{w0:>22} MAC-eq");
    println!("  canonical job       prefill {} / decode {}", PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    println!(
        "  honest geometry     attn_heads {} x attn_head_dim {}  (product {})",
        honest.attn_heads,
        honest.attn_head_dim,
        honest.attn_heads as u64 * honest.attn_head_dim as u64
    );
    println!(
        "  forged geometry     attn_heads {} x attn_head_dim {}  (product {})",
        cheat.attn_heads,
        cheat.attn_head_dim,
        cheat.attn_heads as u64 * cheat.attn_head_dim as u64
    );

    // ---- 1. the executor sees the same graph: every node table is byte-identical -------------
    assert_eq!(honest.attn_nodes, cheat.attn_nodes, "the attention node table must be untouched");
    assert_eq!(honest.pre_nodes, cheat.pre_nodes, "the pre table must be untouched");
    assert_eq!(honest.post_nodes, cheat.post_nodes, "the post table must be untouched");
    assert_eq!(honest.hidden_dim, cheat.hidden_dim, "the residual width must be untouched");
    assert_eq!(honest.layer_count, cheat.layer_count);
    assert_eq!(honest.n_ctx, cheat.n_ctx);

    println!("\n  --- the nodes the attention price is charged on (identical in both) ---");
    let mut over_cache = 0usize;
    for (i, n) in honest.attn_nodes.iter().enumerate() {
        if n.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V) {
            over_cache += 1;
            println!("    attn[{i:>2}] op {:?}  out_len {:?}  weight {:?}", n.op_kind, n.out_len, n.weight_name);
            assert_eq!(n.out_len, cheat.attn_nodes[i].out_len, "the node's own out_len is identical");
        }
    }
    assert!(over_cache >= 1, "the floor must have at least one node reading the KV cache");
    println!("    (the `over_cache` arm at palw_economic_compute_v1.rs:348 returns before `out` is read)");

    // ---- 2. the price moves anyway ------------------------------------------------------------
    let ccu_honest = ccu_per_draw(&honest, &job);
    let ccu_cheat = ccu_per_draw(&cheat, &job);
    println!("\n  --- the price ---");
    println!("    honest ccu/draw   {ccu_honest:>22} MAC-eq");
    println!("    forged ccu/draw   {ccu_cheat:>22} MAC-eq");
    println!("    price multiplier  x{:.2}", ccu_cheat as f64 / ccu_honest as f64);
    assert_eq!(ccu_honest, 21_657_728, "the honest floor's measured price");
    assert!(ccu_cheat > ccu_honest, "the forged geometry must cost more, or there is nothing here");
    let price_multiplier = ccu_cheat as f64 / ccu_honest as f64;
    assert!(price_multiplier > 1_000.0, "the multiplier must be large, got {price_multiplier}");

    // the declared leaf count — the quantity that IS checked — did not move
    let cap = 1u64 << 40;
    let leaves_honest = step_leaf_count_capped_v1(&honest, &job, cap).expect("honest leaves");
    let leaves_cheat = step_leaf_count_capped_v1(&cheat, &job, cap).expect("forged leaves");
    println!("\n    declared leaves   honest {leaves_honest}   forged {leaves_cheat}  (LEAVES, unchanged)");
    assert_eq!(leaves_honest, leaves_cheat, "the declared leaf count must not move — it is what IS checked");

    // ---- 3. the live gate admits it ------------------------------------------------------------
    let root_h = Hash64::from_u64_word(0x1111_0002);
    let root_c = Hash64::from_u64_word(0x2222_0002);
    let a_honest = admit(&p, &b, &honest, &job, root_h);
    let a_cheat = admit(&p, &b, &cheat, &job, root_c);
    println!("\n  --- verify_class_admission_v9 at DAA 0, t12 shape ---");
    println!("    honest -> {a_honest:?}");
    println!("    forged -> {a_cheat:?}");
    assert!(a_honest.is_ok(), "the honest floor must be admissible: {a_honest:?}");
    let admitted_leaves = a_cheat.expect("THE GATE MUST ADMIT THE FORGED GEOMETRY for this finding to hold");
    assert_eq!(admitted_leaves, leaves_honest, "admitted with the floor's own leaf count");

    // the first refusal just past it, to show what stops it is not a work check
    let next = admit(&p, &b, &forged(65_535, 41_855), &job, root_c);
    println!("    attn_head_dim 41855 -> {next:?}");

    // ---- 4. the reward, through the fold's own two calls ----------------------------------------
    let ph = paid(&p, &honest, &job, ccu_honest, w0, escrow);
    let pc = paid(&p, &cheat, &job, ccu_honest, w0, escrow);
    println!("\n  --- what a Final is paid (palw_claim_economics_snapshot_v1 -> priced_reward) ---");
    show("HONEST floor", &ph);
    show("FORGED floor", &pc);

    assert_eq!(ph.ccu_declared, ccu_honest, "the registry row prices what the draw prices");
    assert_eq!(pc.ccu_declared, ccu_cheat, "the registry row carries the forged price");

    // both are paid the whole escrow
    assert_eq!(pc.reward_sompi, escrow, "the forged class saturates the escrow cap");
    // the honest floor lands one sompi short of the cap: its Q32 draw count is 16421.372140, and
    // 16421.372140 x 21,657,728 = 355,649,611,199 MAC-eq, one MAC-eq under W0 = 355,649,611,200.
    assert!(
        ph.reward_sompi >= escrow - 1 && ph.reward_sompi <= escrow,
        "the honest floor is paid the escrow too — but it pays for it in draws: got {} vs escrow {escrow}",
        ph.reward_sompi
    );

    // the real arithmetic per paid claim is what differs
    println!("\n  --- the gain ---");
    println!("    honest real work / paid claim  {:>22} MAC-eq", ph.real_mac_eq_per_claim);
    println!("    forged real work / paid claim  {:>22} MAC-eq", pc.real_mac_eq_per_claim);
    let gain = ph.real_mac_eq_per_claim as f64 / pc.real_mac_eq_per_claim as f64;
    let msk_per_g_honest = (ph.reward_sompi as f64 / 1e8) / (ph.real_mac_eq_per_claim as f64 / 1e9);
    let msk_per_g_cheat = (pc.reward_sompi as f64 / 1e8) / (pc.real_mac_eq_per_claim as f64 / 1e9);
    println!("    honest  {msk_per_g_honest:>14.4} MSK per G real MAC-eq");
    println!("    forged  {msk_per_g_cheat:>14.4} MSK per G real MAC-eq");
    println!("    GAIN    x{gain:.4}   (= W0 / ccu_real, the saturation ceiling)");

    assert!(
        pc.real_mac_eq_per_claim < ph.real_mac_eq_per_claim,
        "the forged class must buy its escrow with less real arithmetic"
    );
    assert!(gain > 10_000.0, "the gain must exceed 10,000x, got {gain}");
    assert!(
        (gain - (w0 as f64 / ccu_honest as f64)).abs() / gain < 0.001,
        "the gain is W0/ccu_real: gain {gain}, W0/ccu {}",
        w0 as f64 / ccu_honest as f64
    );

    println!("\n  VERDICT: the two scalars move the price x{price_multiplier:.2} with the node table, the");
    println!("  declared leaves, the canonical job and every out_len byte-identical; the live t12 gate");
    println!("  admits it; the fold's own reward call pays it the whole {:.4} MSK escrow.", escrow as f64 / 1e8);
    println!("  NOT settled by this test: whether such a claim can reach a Final (a panel must sign");
    println!("  Valid over a profile no stock backend can plan). That is a realization question.");
    println!("========================================================================\n");
}

// ==================================================================== the regression

/// **The regression test that should pass after the fix.** It is `#[ignore]`d because it FAILS on
/// this commit — that failure is the finding.
///
/// The correct behaviour is one of two, and this asserts the disjunction so a fix by either route
/// satisfies it:
///
///   (a) the price of a KV-reading matmul follows the row the node actually commits — its own
///       `out_len` — so rewriting `attn_heads`/`attn_head_dim` without touching a node cannot move
///       `palw_attempt_economic_compute_v1`; or
///   (b) `validate_geometry` / `verify_class_admission_v9` refuses a profile whose
///       `attn_heads x attn_head_dim` does not match the width the attention nodes commit.
///
/// Today neither holds: the price moves x36,475 and the gate returns `Ok`.
#[test]
fn repro02_regression_the_attention_price_must_follow_the_node_it_charges() {
    let p = t12();
    let b = bundle_of(&p);
    let honest = floor_profile();
    let job = floor_job();

    let ccu_honest = ccu_per_draw(&honest, &job);
    let mut failures = Vec::new();

    for (heads, dim) in [(65_535u16, 41_854u32), (65_535, 4_096), (256, 8_192), (4, 65_535), (1_024, 1_024)] {
        let cheat = forged(heads, dim);
        // the graph is untouched by construction
        assert_eq!(honest.attn_nodes, cheat.attn_nodes);
        let ccu_cheat = ccu_per_draw(&cheat, &job);
        let admitted = admit(&p, &b, &cheat, &job, Hash64::from_u64_word(heads as u64)).is_ok();
        println!(
            "  heads {heads:>6} x dim {dim:>6}: ccu {ccu_cheat:>22} MAC-eq (honest {ccu_honest}, x{:.2}), admitted {admitted}",
            ccu_cheat as f64 / ccu_honest as f64
        );
        // (a) price unchanged, OR (b) refused
        if ccu_cheat != ccu_honest && admitted {
            failures.push(format!(
                "heads {heads} x dim {dim}: price moved to {ccu_cheat} MAC-eq (x{:.2}) and the gate ADMITTED it",
                ccu_cheat as f64 / ccu_honest as f64
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "a registrant-written attention geometry priced the class without the gate bounding it:\n  {}",
        failures.join("\n  ")
    );
}
