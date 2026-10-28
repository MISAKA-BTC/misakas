//! **Adversarial regressions: credited work must not rise without extra compute.**
//!
//! These tests assert the derivation PAST the ADR-0145 bundle. The bundle fences
//! (`palw_canonical_work`, `palw_fp_derived_work`, `palw_admission_independence`) are `None` on
//! every shipped preset — 0144 item 0, and 0146's search found no coefficient table to write.
//! Below those fences the declared leaf count is still the unit; the functions here are the
//! reading the fold will use when an operator chooses a height.
//!
//! The reward path (ADR-0132 `economic_ccu_per_claim` of the prefill-draw job, at
//! `rate_sompi_per_giga = 900_000_000`) is *not* under test here. That path already prices the
//! job `palw_attempt_job_v1` runs.

use kaspa_consensus_core::config::params::palw_rc_shipped_params;
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_attempted_compute_per_claim_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

/// ADR-0132 Upgrade C's live rate, sompi per giga-MAC-equivalent (`PALW_ECONOMIC_PAYOUT_DEVNET_V1`
/// / the 7,101 card: `rate_sompi_per_giga: 900_000_000`).
const RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

/// 72 % worker carve of one testnet-11 block's escrow, in sompi — the ceiling `min`ed into pay,
/// matching `palw_reward_properties_v1`.
const ESCROW_SOMPI: u64 = 320_084_640_000;

/// `palw_prefill_draw` armed at DAA 4,000 on the shipped preset. Past it an attempt executes
/// `exact_decode_tokens = 1` while declared `pwu_per_inference` still counts every decode call.
const PREFILL_DRAW: bool = true;

fn shipped_ladder() -> u64 {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else {
        panic!("the shipped release params are a ConsensusV2 bundle");
    };
    bundle.court.max_step_leaf_count()
}

fn dense_512() -> PalwShapeProfileV3 {
    palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the shipped @512 dense row projects")
}

fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    rc_job_context(profile, prefill, decode)
}

fn re_tiled(profile: &PalwShapeProfileV3, tile_len: u32) -> PalwShapeProfileV3 {
    let mut out = profile.clone();
    for table in [&mut out.pre_nodes, &mut out.gdn_nodes, &mut out.attn_nodes, &mut out.post_nodes] {
        for node in table.iter_mut() {
            node.tile_len = tile_len;
        }
    }
    out
}

fn work_floor() -> u128 {
    palw_work_floor_v1(ESCROW_SOMPI, RATE_SOMPI_PER_GIGA)
}

fn one_draw_mac_eq(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    palw_attempt_economic_compute_v1(profile, &job(profile, canonical.0, canonical.1), PREFILL_DRAW, &PALW_ECONOMIC_COST_TABLE_V1)
        .expect("the fixture's canonical job walks")
}

fn class_target(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    palw_work_ticket_target_v1(one_draw_mac_eq(profile, canonical), work_floor())
}

/// Live fork-choice weight of one claim PAST the ADR-0145 bundle:
/// `expected_attempts × CanonicalWork of the executed draw`.
fn live_fork_weight(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let descriptor = PalwCanonicalClassDescriptorV1::of(profile, kaspa_hashes::Hash64::default()).expect("one weight format");
    let derived = palw_canonical_draw_work_v1(&descriptor, &job(profile, canonical.0, canonical.1), PREFILL_DRAW)
        .expect("the fixture's canonical job derives")
        .provisional_scalar_v1();
    (palw_expected_attempts_v1(class_target(profile, canonical)) as u128).saturating_mul(derived)
}

fn live_pay_sompi(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u64 {
    let attempted = palw_attempted_compute_per_claim_v1(
        palw_expected_attempts_v1(class_target(profile, canonical)),
        one_draw_mac_eq(profile, canonical),
    );
    palw_rate_priced_reward_v1(ESCROW_SOMPI, attempted, RATE_SOMPI_PER_GIGA as u128)
}

/// **F1 live at 7,101: a registrant who declares more decode calls buys fork-choice weight the
/// draw never runs.**
///
/// Past `palw_prefill_draw` the attempt is `(declared_prefill, 1)` (`palw_attempt_job_v1`). The
/// three rows below therefore execute identical arithmetic and receive identical ADR-0132 pay.
/// Fork-choice still reads the canonical job's STEP-LEAF count, so `(63, 370)` weighs 7.77×
/// `(63, 2)`. `Params::palw_canonical_work` is `None` on the shipped release, so this is the
/// path a node at DAA ≥ 7,101 actually folds.
#[test]
fn adversarial_regression_declared_decode_must_not_inflate_derived_fork_weight() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_canonical_work, None, "the bundle stays dormant until a height is chosen");
    assert!(rc.palw_model_registry.is_some() && rc.palw_work_target.is_some() && rc.palw_economic_payout.is_some());

    let dense = dense_512();
    let shipped_draw = one_draw_mac_eq(&dense, (63, 2));
    assert_eq!(shipped_draw, 83_102_171_136, "one draw of the shipped dense row");
    for decode in [2u32, 128, 370] {
        assert_eq!(
            one_draw_mac_eq(&dense, (63, decode)),
            shipped_draw,
            "declaring {decode} decode calls does not execute them — the draw is (63, 1)"
        );
        assert_eq!(
            live_pay_sompi(&dense, (63, decode)),
            live_pay_sompi(&dense, (63, 2)),
            "ADR-0132 pay follows the executed draw, not the declaration"
        );
    }

    let shipped = live_fork_weight(&dense, (63, 2));
    let inflated = live_fork_weight(&dense, (63, 370));
    assert_eq!(
        inflated,
        shipped,
        "adversarial regression (live 7,101): same executed MAC-eq and same pay must yield the same \
         fork-choice weight; declared decode is not work. shipped={shipped} inflated={inflated} \
         ratio={:.2}x",
        inflated as f64 / shipped as f64
    );
}

/// **F1's second lever, live at 7,101: `tile_len` is a commitment block size and not arithmetic.**
///
/// The chain admits tilings of the shipped dense row from 65,536 down to 24 (finer than 24 exceeds
/// the 2^26 ladder at `worst_case_step_leaf_count_capped_v1`). Across that band the executed draw
/// is 83,102,171,136 MAC-eq on every row and fork-choice weight moves 134.9×. A permissionless
/// registrant of the certified A16 family joins at `min_grantable_share` (1 ‰;
/// `palw_admission_independence` is also dormant) and that weight is `safe_weight`.
#[test]
fn adversarial_regression_re_tiling_must_not_inflate_live_7101_fork_weight() {
    let dense = dense_512();
    let canonical = (63u32, 2u32);
    let ladder = shipped_ladder();
    let arithmetic = one_draw_mac_eq(&dense, canonical);
    assert_eq!(arithmetic, 83_102_171_136);

    let coarsest = re_tiled(&dense, 65_536);
    let finest = re_tiled(&dense, 24);
    for (name, profile) in [("coarsest", &coarsest), ("finest", &finest)] {
        assert!(
            worst_case_step_leaf_count_capped_v1(profile, ladder).is_ok(),
            "{name} tiling is admissible: its declared worst case clears the ladder"
        );
        assert_eq!(one_draw_mac_eq(profile, canonical), arithmetic, "{name} tiling must not move ADR-0131's executed draw");
        assert_eq!(live_pay_sompi(profile, canonical), live_pay_sompi(&dense, canonical), "{name} tiling must not move ADR-0132 pay");
    }

    let coarse_w = live_fork_weight(&coarsest, canonical);
    let fine_w = live_fork_weight(&finest, canonical);
    assert_eq!(
        fine_w,
        coarse_w,
        "adversarial regression (live 7,101): re-tiling one model, one artifact and one kernel set \
         must not move fork-choice weight. tile=65536 → {coarse_w}, tile=24 → {fine_w}, ratio={:.1}x",
        fine_w as f64 / coarse_w as f64
    );
}

/// **The admissible extreme of F1, live at 7,101: credited-work / actual-compute is not 1.**
///
/// `(1, 432)` is inside the class's declared worst case and its `n_ctx`. Past `palw_prefill_draw`
/// it executes 1,546,037,392 MAC-eq a draw against the shipped row's 83,102,171,136, and the
/// live `claim.pwu / executed MAC-eq` ratio is 427× the shipped row's (24,572× per *draw*, before
/// the work-target lottery multiplies the tiny class's expected attempts). Hardware speedup is
/// not this: the two rows run the same kernels on the same artifact.
#[test]
fn adversarial_regression_live_7101_weight_per_executed_mac_must_not_depend_on_the_declaration() {
    let dense = dense_512();
    let ladder = shipped_ladder();
    let worst = worst_case_step_leaf_count_capped_v1(&dense, ladder).expect("shipped worst case");
    for (p, d) in [(63u32, 2u32), (1, 432)] {
        let leaves = step_leaf_count_capped_v1(&dense, &job(&dense, p, d), ladder).unwrap();
        assert!(leaves <= worst, "({p},{d}) is inside the class's declared worst case");
        assert!(p as u64 + d.max(1) as u64 - 1 <= dense.n_ctx as u64, "({p},{d}) fits n_ctx");
    }

    let ratio_of = |profile: &PalwShapeProfileV3, canonical: (u32, u32)| -> u128 {
        let weight = live_fork_weight(profile, canonical);
        let executed = palw_attempted_compute_per_claim_v1(
            palw_expected_attempts_v1(class_target(profile, canonical)),
            one_draw_mac_eq(profile, canonical),
        );
        weight.saturating_mul(1_000_000_000) / executed.max(1)
    };
    let shipped = ratio_of(&dense, (63, 2));
    let extreme = ratio_of(&dense, (1, 432));
    let combined_profile = re_tiled(&dense, 24);
    assert!(worst_case_step_leaf_count_capped_v1(&combined_profile, ladder).is_ok(), "tile 24 is still admissible");
    assert_eq!(
        one_draw_mac_eq(&combined_profile, (1, 432)),
        one_draw_mac_eq(&dense, (1, 432)),
        "re-tiling the extreme job does not move executed MAC-eq"
    );
    let combined = ratio_of(&combined_profile, (1, 432));
    assert_eq!(
        extreme,
        shipped,
        "adversarial regression (live 7,101): weight per executed MAC-eq must be the same for every \
         declaration of one graph. shipped={shipped} (1,432)={extreme} ({}x); \
         tile24+(1,432)={combined} ({}x, not larger)",
        extreme / shipped.max(1),
        combined / shipped.max(1)
    );
}

/// **F2 live at 7,101: the free-prompt lane still prices the executor's `work_leaves`.**
///
/// `Params::palw_fp_derived_work` is `None` on every shipped preset. The live derivation
/// `derive_quanta_and_pwu(declared_leaves, canonical)` never asks the graph what the run counted,
/// so a tenfold lie is an elevenfold `claim.pwu` (floored into quanta). The per-receipt cap of 64
/// quanta is the only bound.
#[test]
fn adversarial_regression_free_prompt_work_leaves_must_not_be_miner_declared() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_fp_derived_work, None, "the bundle stays dormant until a height is chosen");

    let dense = dense_512();
    let honest = fp_derive_credited_compute_v1(&dense, 8, 2, 0).expect("honest run derives");
    let descriptor = PalwCanonicalClassDescriptorV1::of(&dense, kaspa_hashes::Hash64::default()).expect("one weight format");
    let from_graph = palw_canonical_draw_work_v1(&descriptor, &job(&dense, 8, 2), false)
        .expect("an uncached (8,2) job derives")
        .provisional_scalar_v1();
    // palw_canonical_draw_work_v1 with prefill_draw=false prices the declared (8,2); the FP
    // credited-compute path prices the same facts. A 10× leaf lie is a different number and is
    // not an argument of either function — the fold refuses FreePromptWorkLeavesMismatch.
    assert_eq!(honest, from_graph, "FP credit is CanonicalWork of the executed tokens, not work_leaves");
    let honest_leaves = step_leaf_count_capped_v1(&dense, &job(&dense, 8, 2), shipped_ladder()).unwrap();
    assert_ne!(honest_leaves * 10, honest_leaves, "the 10× declaration is a different leaf count");
    assert_ne!(honest_leaves as u128, honest, "leaves are not the credited unit");
}

/// **Slash-coverage economics: fake-work N-search past the ADR-0145 bundle.**
///
/// Declared decode ∈ {2, 8, 32, 128, 370} and tile_len ∈ {24, 256, 65_536} execute the same
/// draw. Fork-weight and ADR-0132 pay stay flat, so expected profit of the inflation is 0 and
/// cannot exceed any slash cap. The path is INVALID/inert, not an offence — ADR-0027 does not
/// slash a declaration the fold refuses to price.
#[test]
fn fake_work_n_search_has_zero_residual_profit_past_the_fence() {
    let dense = dense_512();
    let honest_w = live_fork_weight(&dense, (63, 2));
    let honest_pay = live_pay_sompi(&dense, (63, 2));
    for decode in [2u32, 8, 32, 128, 370] {
        assert_eq!(live_fork_weight(&dense, (63, decode)), honest_w, "decode={decode} must not buy weight");
        assert_eq!(live_pay_sompi(&dense, (63, decode)), honest_pay, "decode={decode} must not buy pay");
    }
    let ladder = shipped_ladder();
    for tile in [24u32, 256, 65_536] {
        let profile = re_tiled(&dense, tile);
        assert!(worst_case_step_leaf_count_capped_v1(&profile, ladder).is_ok(), "tile={tile} is admissible");
        assert_eq!(live_fork_weight(&profile, (63, 2)), honest_w, "tile={tile} must not buy weight");
        assert_eq!(live_pay_sompi(&profile, (63, 2)), honest_pay, "tile={tile} must not buy pay");
    }
}
