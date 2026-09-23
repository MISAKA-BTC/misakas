//! AUDIT REPRO 00 — the registrant's free split of the canonical job moves the per-claim
//! SLASHABLE RESERVATION without moving the FORK WEIGHT it backs.
//!
//! Target: testnet-12 (`palw_t12_shipped_params()`), worktree wt-audit @ 077d4c7f.
//!
//! Run:
//!   cargo test -p kaspa-consensus-core \
//!     --test audit_repro_00_canonical_job_split_moves_the_slashable_ -- --nocapture
//!
//! The claim under test
//! --------------------
//! ONE profile — Qwen2.5-1.5B A16 graph-v7 @ n_ctx 512 — registered twice with two canonical
//! jobs that BOTH clear every admission bound the chain places on the canonical job
//! (`palw_class_admission_v2.rs:2276-2289`: `counted <= worst` and `pwu_per_inference == counted`,
//! and nothing else):
//!
//!   A = (prefill 510, decode   2)
//!   B = (prefill   1, decode 511)
//!
//! Same weights, same artifact root, same reachable kernels, same `slash_value_per_pwu`.
//! The declared leaf count barely moves (B is 1.18x A). The DERIVED per-draw work moves 440x.
//! `reserved` — the sompi of the producer's bond a claim puts at risk — is written from the
//! per-draw work with NO expected-attempts factor (`palw_state_v2.rs:18871-18873`), while
//! `claim.pwu` — the fork weight and the fraud-gain weight term — is expected_attempts x per_draw
//! (`palw_admission_v2.rs:345-347`, forced equal at `:447-450`). Past `palw_work_target` the
//! target is `MAX * min(1, CCU/W0)`, so expected_attempts ~= W0/CCU and `claim.pwu` pins near W0
//! for every class below the floor — while `reserved` falls linearly in CCU.
//!
//! Every quantity below runs through the real runtime function. Nothing is reimplemented.
//!
//! UNITS
//!   U1 declared leaves   `pwu_per_inference`, forced == `step_leaf_count_capped_v1`   [leaves]
//!   U2 derived per draw  `PalwModelWorkV1::economic_ccu_per_claim`                    [MAC-eq/draw]
//!   U3 exposure pwu      `palw_exposure_pwu_v3` = U2 * base_declared / base_canonical [floor-pwu]
//!   reserved             U3 * `slash_value_per_pwu`                                   [sompi]
//!   claim.pwu            `palw_attempt_derived_pwu_v1(target, U2)` = attempts * U2     [MAC-eq]
//!   W0                   `palw_work_floor_v1(escrow, rate)` = escrow * 1e9 / rate      [CCU]

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use kaspa_consensus_core::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwClassStateV2, PalwClassStatusV2, PalwExposureBasisV1, PalwPwuRuleV2, palw_exposure_pwu_v3,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_deepest_job_capped_v1};
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

/// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` on the shipped card: 2^26.
const LADDER: u64 = 1 << 26;
/// Every t12 genesis class carries `slash_value_per_pwu = 5` (`palw_genesis_v2.rs:538`).
const SLASH_VALUE_PER_PWU: u64 = 5;
/// The block subsidy t12 pays from DAA 0 — `CoinbaseManager::calc_block_subsidy` lives in the
/// `kaspa-consensus` crate, so it is pinned here exactly as the tree's own fold tests pin it
/// (`palw_state_v2.rs:31019`, `config/params.rs:22303`). The test sweeps W0 below so the finding
/// does not rest on this one value.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

/// One measured row. Every field is the output of a real runtime function.
#[derive(Debug)]
struct Row {
    label: &'static str,
    prefill: u32,
    decode: u32,
    /// U1, leaves.
    declared_leaves: u64,
    /// U2, MAC-eq per draw.
    per_draw_mac_eq: u128,
    /// The class ticket target past `palw_work_target`.
    target: u128,
    /// Expected draws a win costs at that target.
    attempts: u64,
    /// U7, MAC-eq — the fork weight a claim of this class carries.
    claim_pwu: u64,
    /// U3, floor-normalised pwu.
    exposure_pwu: u64,
    /// sompi of the producer's bond this claim puts at risk.
    reserved_sompi: u128,
    /// MAC-eq the producer honestly executes per block of chain weight bought.
    honest_mac_eq_per_block: u128,
}

impl Row {
    /// Fork weight (MAC-eq) bought per sompi of collateral actually at risk.
    fn weight_per_sompi_at_risk(&self) -> f64 {
        self.claim_pwu as f64 / self.reserved_sompi as f64
    }
}

fn class_state(declared_leaves: u64) -> PalwClassStateV2 {
    PalwClassStateV2 {
        artifact_root: Hash64::default(),
        slash_value_per_pwu: SLASH_VALUE_PER_PWU,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: declared_leaves },
        status: PalwClassStatusV2::Active,
        registered_daa: 0,
        registrant_bond: None,
        fused_attention: false,
    }
}

/// Measure one admissible canonical split of `profile` exactly as the chain would price it.
fn measure(profile: &PalwShapeProfileV3, label: &'static str, prefill: u32, decode: u32, w0: u128, worst: u64, basis: PalwExposureBasisV1) -> Row {
    let job = rc_job_context(profile, prefill, decode);

    // ---- admission bound 1: the leaf count under the court ladder ----
    let declared_leaves = step_leaf_count_capped_v1(profile, &job, LADDER)
        .unwrap_or_else(|e| panic!("{label}: the leaf counter refused (prefill {prefill}, decode {decode}): {e:?}"));
    // `palw_class_admission_v2.rs:2280` — `counted > worst` is the ONLY shape bound on the split.
    assert!(
        declared_leaves <= worst,
        "{label}: (prefill {prefill}, decode {decode}) is NOT admissible — counted {declared_leaves} leaves > deepest-job bound {worst}"
    );
    // `palw_class_admission_v2.rs:2286` forces `pwu_per_inference == counted`; the registrant
    // declares exactly this number and nothing else about the price.

    // ---- U2: the DERIVED per-draw work the registry row carries ----
    let work = palw_model_work_from_carriage_v1(profile, &job).unwrap_or_else(|| panic!("{label}: no carriage work"));
    let per_draw_mac_eq = work.economic_ccu_per_claim;
    // Cross-check against the canonical-work vector the same fence derives it from.
    let descriptor = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).expect("one weight format");
    let scalar = palw_canonical_draw_work_v1(&descriptor, &job, true).expect("derives").provisional_scalar_v1();
    assert_eq!(scalar, per_draw_mac_eq, "{label}: the registry row IS palw_canonical_draw_work_v1's scalar");

    // ---- the ticket target and the fork weight (palw_admission_v2.rs:345-347, :447-450) ----
    let target = palw_work_ticket_target_v1(per_draw_mac_eq, w0);
    let attempts = palw_expected_attempts_v1(target);
    let claim_pwu = palw_attempt_derived_pwu_v1(target, per_draw_mac_eq);

    // ---- U3 and the ledger write (palw_state_v2.rs:18871-18873) ----
    let class = class_state(declared_leaves);
    let exposure_pwu = palw_exposure_pwu_v3(&class, claim_pwu, Some(per_draw_mac_eq.min(u64::MAX as u128) as u64), Some(basis));
    let reserved_sompi = (exposure_pwu as u128) * (class.slash_value_per_pwu as u128);

    Row {
        label,
        prefill,
        decode,
        declared_leaves,
        per_draw_mac_eq,
        target,
        attempts,
        claim_pwu,
        exposure_pwu,
        reserved_sompi,
        honest_mac_eq_per_block: (attempts as u128) * per_draw_mac_eq,
    }
}

fn print_row(r: &Row) {
    println!("  {} — canonical job (prefill {}, decode {})", r.label, r.prefill, r.decode);
    println!("     U1 declared leaves  (leaves)             {}", r.declared_leaves);
    println!("     U2 derived per draw (MAC-eq/draw)        {}", r.per_draw_mac_eq);
    println!("     ticket target == u128::MAX?              {}", r.target == u128::MAX);
    println!("     expected draws a win costs               {}", r.attempts);
    println!("     claim.pwu = fork weight  (MAC-eq)        {}", r.claim_pwu);
    println!("     U3 exposure pwu     (floor-pwu)          {}", r.exposure_pwu);
    println!("     reserved = U3 * 5   (sompi)              {}  = {:.6} MSK", r.reserved_sompi, r.reserved_sompi as f64 / 1e8);
    println!("     HONEST work really executed per block    {} MAC-eq", r.honest_mac_eq_per_block);
    println!("     fork weight bought per sompi at risk     {:.2}", r.weight_per_sompi_at_risk());
}

/// The exposure basis the fold builds from the t12 liveness floor, measured — not pinned.
fn t12_floor_basis() -> PalwExposureBasisV1 {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
    let job = rc_job_context(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let base_declared = step_leaf_count_capped_v1(&floor, &job, LADDER).expect("the floor counts");
    let work = palw_model_work_from_carriage_v1(&floor, &job).expect("the floor has carriage work");
    let base_canonical = work.economic_ccu_per_claim.min(u64::MAX as u128) as u64;
    PalwExposureBasisV1 { base_declared, base_canonical }
}

/// W0 from the live t12 params: escrow (the worker carve of the block subsidy) over the
/// `palw_economic_payout` rate.
fn t12_w0() -> (u64, u64, u128) {
    let p = palw_t12_shipped_params();
    let carve = p.palw_overlay_carve.expect("t12 arms palw_overlay_carve at 0").worker_carve_permille;
    let rate = p.palw_economic_payout.expect("t12 arms palw_economic_payout at 0").rate_sompi_per_giga;
    let escrow = palw_reward_carve_v2(T12_BLOCK_SUBSIDY_SOMPI, &PalwRewardParamsV2::new(carve).expect("carve <= 1000")).worker;
    (escrow, rate, palw_work_floor_v1(escrow, rate))
}

// ===========================================================================================
// THE EXPLOIT — this test PASSES, and a pass means the lever is real.
// ===========================================================================================
#[test]
fn the_registrants_canonical_split_moves_the_slashable_reservation_230x_at_equal_fork_weight() {
    let basis = t12_floor_basis();
    let (escrow, rate, w0) = t12_w0();

    println!("\n=== t12 economic inputs, read from palw_t12_shipped_params() ===");
    println!("  escrow per claim        {escrow} sompi = {:.2} MSK   (subsidy {T12_BLOCK_SUBSIDY_SOMPI} x carve)", escrow as f64 / 1e8);
    println!("  rate_sompi_per_giga     {rate}");
    println!("  W0                      {w0} CCU");
    println!("  exposure basis (floor)  base_declared {} leaves / base_canonical {} MAC-eq", basis.base_declared, basis.base_canonical);
    println!("  U2 -> U3 divisor        {:.4}x", basis.base_canonical as f64 / basis.base_declared as f64);

    let profile = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 512, ..QWEN25_1_5B }).expect("A16 @ n_ctx 512 is expressible");
    let worst = worst_case_step_leaf_count_deepest_job_capped_v1(&profile, LADDER).expect("deepest job fits the 2^26 ladder");

    println!("\n=== ONE profile: Qwen2.5-1.5B A16 graph-v7 @ n_ctx 512 ===");
    println!("  deepest-job leaf bound (`worst`) = {worst} leaves\n");

    let a = measure(&profile, "A prefill-heavy", 510, 2, w0, worst, basis);
    print_row(&a);
    println!();
    let b = measure(&profile, "B decode-heavy  ", 1, 511, w0, worst, basis);
    print_row(&b);

    // ---------------------------------------------------------------------------------------
    // 1. Both splits are admissible. The registrant genuinely gets to choose.
    // ---------------------------------------------------------------------------------------
    assert!(a.declared_leaves <= worst && b.declared_leaves <= worst, "both splits clear the only shape bound there is");

    // ---------------------------------------------------------------------------------------
    // 2. The DECLARED leaf count — the one number admission bounds — barely moves.
    // ---------------------------------------------------------------------------------------
    let declared_ratio = b.declared_leaves as f64 / a.declared_leaves as f64;
    println!("\n--- the lever ---");
    println!("  U1 declared leaves    B / A = {declared_ratio:.4}x   (leaves, the ONLY bounded quantity)");
    assert!(
        (1.0..1.5).contains(&declared_ratio),
        "the declared leaf count is nearly flat across the two splits — got {declared_ratio}"
    );

    // ---------------------------------------------------------------------------------------
    // 3. The DERIVED per-draw work — which nothing bounds — moves by two and a half orders.
    // ---------------------------------------------------------------------------------------
    let per_draw_ratio = a.per_draw_mac_eq as f64 / b.per_draw_mac_eq as f64;
    println!("  U2 per-draw MAC-eq    A / B = {per_draw_ratio:.1}x   (MAC-eq/draw, unbounded by admission)");
    assert!(per_draw_ratio > 100.0, "the per-draw work moves by orders of magnitude — got {per_draw_ratio}");

    // ---------------------------------------------------------------------------------------
    // 4. And with it the SLASHABLE RESERVATION, by the same factor — because the reservation
    //    is written from the per-draw work and carries no attempts factor.
    // ---------------------------------------------------------------------------------------
    let reserved_ratio = a.reserved_sompi as f64 / b.reserved_sompi as f64;
    println!("  reserved sompi        A / B = {reserved_ratio:.1}x   (sompi of bond actually at risk)");
    assert!(
        (per_draw_ratio - reserved_ratio).abs() / per_draw_ratio < 0.01,
        "the reservation tracks the per-draw work one-for-one: per_draw {per_draw_ratio} vs reserved {reserved_ratio}"
    );

    // ---------------------------------------------------------------------------------------
    // 5. The FORK WEIGHT does NOT move with it: past `palw_work_target` the target is
    //    MAX * min(1, CCU/W0), so attempts ~= W0/CCU and claim.pwu pins near W0.
    // ---------------------------------------------------------------------------------------
    let weight_ratio = a.claim_pwu as f64 / b.claim_pwu as f64;
    println!("  claim.pwu fork weight A / B = {weight_ratio:.4}x   (MAC-eq — within 2x, NOT 440x)");
    assert!(weight_ratio < 2.0, "the fork weight is essentially unchanged across the two splits — got {weight_ratio}");

    // ---------------------------------------------------------------------------------------
    // 6. THE FINDING: fork weight bought per sompi of collateral at risk.
    // ---------------------------------------------------------------------------------------
    let discount = b.weight_per_sompi_at_risk() / a.weight_per_sompi_at_risk();
    println!("\n  A: {:.2} MAC-eq of fork weight per sompi at risk", a.weight_per_sompi_at_risk());
    println!("  B: {:.2} MAC-eq of fork weight per sompi at risk", b.weight_per_sompi_at_risk());
    println!("  COLLATERAL DISCOUNT B over A = {discount:.1}x");
    assert!(
        discount > 100.0,
        "the registrant buys >100x the fork weight per sompi at risk by choosing B over A — got {discount}"
    );

    // ---------------------------------------------------------------------------------------
    // 7. The attacker's real cost. B is not paying for the discount — B is CHEAPER.
    // ---------------------------------------------------------------------------------------
    let honest_ratio = a.honest_mac_eq_per_block as f64 / b.honest_mac_eq_per_block as f64;
    println!("\n--- the attacker's real compute ---");
    println!("  A executes {} MAC-eq per block of weight", a.honest_mac_eq_per_block);
    println!("  B executes {} MAC-eq per block of weight", b.honest_mac_eq_per_block);
    println!("  A / B honest work = {honest_ratio:.2}x  -> B is {honest_ratio:.2}x CHEAPER per block");
    println!("  the choice costs ONE registration transaction; no extra hardware, no extra inference");
    assert!(
        honest_ratio > 1.0,
        "the discounted variant does not cost the attacker more real compute — got {honest_ratio}x"
    );

    // ---------------------------------------------------------------------------------------
    // 8. The finding is not an artefact of one W0. Sweep the escrow term.
    // ---------------------------------------------------------------------------------------
    println!("\n--- W0 sensitivity (the discount is structural, not a coincidence of the escrow) ---");
    println!("  {:>22}  {:>18}  {:>18}  {:>12}", "W0 (CCU)", "A wt/sompi", "B wt/sompi", "discount");
    for mult in [1u128, 2, 4, 8, 16] {
        let w0_x = w0 * mult;
        let ax = measure(&profile, "A", 510, 2, w0_x, worst, basis);
        let bx = measure(&profile, "B", 1, 511, w0_x, worst, basis);
        let d = bx.weight_per_sompi_at_risk() / ax.weight_per_sompi_at_risk();
        println!(
            "  {:>22}  {:>18.2}  {:>18.2}  {:>11.1}x",
            w0_x,
            ax.weight_per_sompi_at_risk(),
            bx.weight_per_sompi_at_risk(),
            d
        );
        assert!(d > 50.0, "the discount survives W0 x{mult} — got {d}");
    }

    println!("\nCONFIRMED: the canonical (prefill, decode) split is a registrant-chosen collateral discount.");
    println!("The only admission bounds on the split are the leaf count and the deepest-job ceiling");
    println!("(palw_class_admission_v2.rs:2276-2289). Neither is a function of the price.\n");
}

// ===========================================================================================
// THE MIRROR — the regression test. It FAILS today. It should PASS after the fix.
// ===========================================================================================
/// **The invariant the chain is missing.**
///
/// A claim's slashable reservation must scale with the fork weight it backs, so that the fork
/// weight bought per sompi of collateral actually at risk is a property of the CHAIN, not of a
/// registrant-chosen field. `palw_state_v2.rs:15569-15583` already refuses a registrant-chosen
/// `slash_value_per_pwu` for exactly this reason — "the weight a bond buys per sompi at risk is
/// beta / slash_value_per_pwu ... finality sold at a discount to the party that set its own
/// price" — and the canonical job is the same lever one field over.
///
/// A fix would make `reserved` a function of the CLAIM's weight (attempts x per-draw) rather than
/// of one draw, i.e. multiply the exposure by the same `expected_attempts` that `claim.pwu`
/// carries. Then the ratio below collapses to 1.0 for every admissible split.
///
/// `#[ignore]`d because it fails on 077d4c7f. Remove the attribute when the fix lands.
#[test]
#[ignore = "REGRESSION TEST: fails on 077d4c7f — remove #[ignore] once the reservation is a function of claim weight, not of one draw"]
fn regression_the_collateral_at_risk_per_unit_of_fork_weight_is_not_registrant_chosen() {
    let basis = t12_floor_basis();
    let (_escrow, _rate, w0) = t12_w0();
    let profile = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 512, ..QWEN25_1_5B }).expect("A16 @ n_ctx 512 is expressible");
    let worst = worst_case_step_leaf_count_deepest_job_capped_v1(&profile, LADDER).expect("deepest job fits the 2^26 ladder");

    // Every admissible canonical split of ONE profile must buy fork weight at the same collateral
    // price. A 2x band is generous; today the spread is >100x.
    const MAX_SPREAD: f64 = 2.0;

    let splits: [(&'static str, u32, u32); 5] =
        [("510/2", 510, 2), ("384/128", 384, 128), ("256/256", 256, 256), ("128/384", 128, 384), ("1/511", 1, 511)];

    let mut worst_price = f64::MIN;
    let mut best_price = f64::MAX;
    println!("\n  {:>10}  {:>16}  {:>22}  {:>20}", "split", "reserved (sompi)", "claim.pwu (MAC-eq)", "weight per sompi");
    for (label, prefill, decode) in splits {
        let job_leaves = step_leaf_count_capped_v1(&profile, &rc_job_context(&profile, prefill, decode), LADDER);
        let Ok(leaves) = job_leaves else { continue };
        if leaves > worst {
            continue;
        }
        let r = measure(&profile, "split", prefill, decode, w0, worst, basis);
        let price = r.weight_per_sompi_at_risk();
        println!("  {:>10}  {:>16}  {:>22}  {:>20.2}", label, r.reserved_sompi, r.claim_pwu, price);
        worst_price = worst_price.max(price);
        best_price = best_price.min(price);
    }
    let spread = worst_price / best_price;
    println!("  spread across admissible splits of ONE profile = {spread:.1}x (must be <= {MAX_SPREAD:.1}x)\n");
    assert!(
        spread <= MAX_SPREAD,
        "the canonical job is a collateral discount the registrant sets: {spread:.1}x spread in fork weight \
         bought per sompi of collateral at risk, across splits of a single profile that all clear admission"
    );
}
