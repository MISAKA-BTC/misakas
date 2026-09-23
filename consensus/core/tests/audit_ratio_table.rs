//! **AUDIT LANE A5 — the value-per-real-work ratio table for testnet-12.**
//!
//! Everything here CALLS the runtime. Nothing is reimplemented except where a row is explicitly
//! labelled MODELLED, and each such label names the private function it stands in for.
//!
//! Run:
//!   cargo test -p kaspa-consensus-core --test audit_ratio_table -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_network_draws_q32_from_bits_v1};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_permit_value_sompi_v1, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_canonical_work_id_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1;
use kaspa_consensus_core::palw_panel_var_v1::palw_fork_weight_sompi_v1;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwClassStateV2, PalwClassStatusV2, PalwExposureBasisV1, PalwPwuRuleV2, palw_exposure_pwu_v2, palw_exposure_pwu_v3,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// =================================================================================================
// 0. The t12 constants. Everything here is READ OFF `palw_t12_shipped_params()` except the block
//    subsidy, which is computed by `CoinbaseManager` in the `kaspa-consensus` crate — not linkable
//    from a `kaspa-consensus-core` integration test. That one number is SUPPLIED; the carve is then
//    taken by calling the real `palw_reward_carve_v2`.
// =================================================================================================

/// `CoinbaseManager::calc_block_subsidy(0)` on t12 (deflationary from height 0, 120 s cadence).
/// SUPPLIED, not measured in this crate. Every reward below is linear in it, so the discontinuity
/// STRUCTURE is independent of it; only the location of the escrow cap moves.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

/// `escrow_for_a_genesis_claim_v1` fed `pre_deflationary_phase_base_subsidy` — the number the
/// genesis collateral derivation actually uses (recon §5B). Carried so both are on the table.
const T12_GENESIS_DERIVATION_ESCROW_SOMPI: u64 = 266_736_960;

/// Every t12 class registers `slash_value_per_pwu = 5` (sompi per exposure pwu).
const T12_SLASH_VALUE_PER_PWU: u64 = 5;

const T12_WINDOW_CHALLENGE_DAA: u64 = 1_200;
const T12_WINDOW_COURT_DAA: u64 = 3_000;
const T12_CADENCE_MS: u64 = 120_000;
const T12_COLLUDING_QUORUM: u64 = 3;

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn t12_worker_carve_permille() -> u16 {
    t12().palw_overlay_carve.expect("t12 arms palw_overlay_carve at DAA 0").worker_carve_permille
}

fn t12_escrow_sompi() -> u64 {
    palw_reward_carve_v2(T12_BLOCK_SUBSIDY_SOMPI, &PalwRewardParamsV2::new(t12_worker_carve_permille()).expect("carve <= 1000")).worker
}

fn t12_rate_sompi_per_giga() -> u64 {
    t12().palw_economic_payout.expect("t12 arms palw_economic_payout at DAA 0").rate_sompi_per_giga
}

// =================================================================================================
// 1. The three t12 classes and the synthetic sweep vehicles
// =================================================================================================

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("BASE-0 builds")
}

/// Several shipped builders PANIC on boundary geometries instead of returning `Err` (measured:
/// `qwen25_a16_artifact_row_profile_v7` with `tile_len = 0` hits `Ord::clamp`'s `min <= max`
/// assert). A sweep has to survive that, so each build runs under `catch_unwind` and a panic is
/// reported as PANICKED rather than swallowed.
fn guarded<T>(f: impl FnOnce() -> Option<T> + std::panic::UnwindSafe) -> Result<Option<T>, ()> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(f);
    std::panic::set_hook(hook);
    out.map_err(|_| ())
}

fn dense_profile(g: PalwQwen25GeometryV1) -> Option<PalwShapeProfileV3> {
    guarded(move || qwen25_a16_artifact_row_profile_v7(g).ok()).unwrap_or(None)
}

fn dense_profile_guarded(g: PalwQwen25GeometryV1) -> Result<Option<PalwShapeProfileV3>, ()> {
    guarded(move || qwen25_a16_artifact_row_profile_v7(g).ok())
}

fn hybrid_profile(g: PalwQwen36GeometryV1) -> Option<PalwShapeProfileV3> {
    guarded(move || qwen36_profile_v7(qwen36_geometry_artifact_eps(g)).ok()).unwrap_or(None)
}

/// The 8k dense row testnet-12 registers at genesis since 2026-09-23 (in place of the hybrid @512 row).
fn t12_dense_8k_geometry() -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 { n_ctx: kaspa_consensus_core::config::params::PALW_T12_NARROW_DENSE_N_CTX, ..t12_dense_geometry() }
}

fn t12_dense_geometry() -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }
}

fn t12_hybrid_geometry() -> PalwQwen36GeometryV1 {
    PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B }
}

/// **The honest arithmetic cost of ONE DRAW, in MAC-equivalents.**
///
/// `PalwCanonicalWorkVectorV1::arithmetic_mac_eq()` — the sum of the seven MAC-eq dimensions, with
/// the three BYTE dimensions dropped exactly as `provisional_scalar_v1` drops them. Chosen over
/// `PalwEconomicBreakdownV1::total()` only because it is the one the chain's own weight reads; the
/// two are asserted equal below, so the choice is not load-bearing.
fn draw_mac_eq(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> Option<u128> {
    let job = rc_job_context(profile, prefill, decode);
    let d = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).ok()?;
    Some(palw_canonical_draw_work_v1(&d, &job, true).ok()?.arithmetic_mac_eq())
}

fn class_state(pwu_per_inference: u64) -> PalwClassStateV2 {
    PalwClassStateV2 {
        artifact_root: Hash64::default(),
        slash_value_per_pwu: T12_SLASH_VALUE_PER_PWU,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
        status: PalwClassStatusV2::Active,
        registered_daa: 0,
        registrant_bond: None,
        fused_attention: false,
    }
}

// =================================================================================================
// 2. THE ROW — one class, every numerator, every unit.
// =================================================================================================

#[derive(Clone, Copy, Debug)]
struct Row {
    /// MAC-eq the producer burns for ONE DRAW. The honest unit of arithmetic.
    draw_mac_eq: u128,
    /// The class ticket target the chain assigns: `MAX · min(1, CCU/W)`.
    class_target: u128,
    /// Expected draws per won claim at that target (integer, as `palw_pwu_v1` uses it).
    attempts: u64,
    /// **real_work** = MAC-eq the producer burns, in expectation, to WIN ONE CLAIM.
    real_work_claim: u128,
    /// ADR-0124 arm, the one `work_priced_escrow` runs on t12: `escrow · min(1, exposure/W)`, sompi.
    reward_0124_sompi: u64,
    /// ADR-0132 arm: `min(escrow, attempted_ccu · rate / 1e9)`, sompi.
    reward_0132_sompi: u64,
    /// The execution-lane credit `record_round_final` writes into `PalwExecFinalV1::credit`.
    /// Unit as the code passes it: RAW derived MAC-eq per draw (u64-saturated).
    credit: u64,
    /// Permits `palw_execution_quantum_count_v1` mints from that credit at quantum 100_000.
    permits: u32,
    /// Those permits priced at `palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI)`.
    permit_sompi: u128,
    /// `claim.pwu` as admission forces it: `palw_attempt_derived_pwu_v1(target, draw)`, u64-saturating.
    #[allow(dead_code)]
    claim_pwu: u64,
    /// `palw_fork_weight_sompi_v1(claim_pwu, 5)` — fork weight in the sompi the slash uses.
    fork_weight_sompi: u128,
    /// `palw_exposure_pwu_v3` — the collateral unit, for contrast with `credit`.
    exposure_pwu_v3: u64,
}

#[allow(clippy::too_many_arguments)]
fn measure(draw: u128, escrow: u64, work_target_w: u128, rate: u64, bits: u32, basis: Option<PalwExposureBasisV1>) -> Row {
    let class_target = palw_work_ticket_target_v1(draw, work_target_w);
    let attempts = palw_expected_attempts_v1(class_target);
    let real_work_claim = (attempts as u128).saturating_mul(draw);

    let class = class_state(0);
    let canonical_u64 = draw.min(u64::MAX as u128) as u64;
    let claim_pwu = palw_attempt_derived_pwu_v1(class_target, draw);

    // ADR-0124: `work_priced_escrow` passes `palw_exposure_pwu_v2` (RAW canonical) over the work
    // target unit, both clamped into u64 by `work_price_unit_at` / `canonical_per_draw`.
    let exposure_v2 = palw_exposure_pwu_v2(&class, claim_pwu, Some(canonical_u64));
    let unit_u64 = work_target_w.min(u64::MAX as u128) as u64;
    let reward_0124_sompi = palw_work_priced_reward_v1(escrow, exposure_v2, unit_u64);

    // ADR-0132.
    let attempted = palw_attempted_ccu_v1(palw_expected_attempts_q32_v1(class_target), palw_network_draws_q32_from_bits_v1(bits), draw);
    let reward_0132_sompi = palw_rate_priced_reward_v1(escrow, attempted, rate as u128);

    // Execution lane: `record_round_final` sets `credit = exposure` whenever the round lane's
    // `execution_quantum > 0`, which t12 does from DAA 0.
    let credit = exposure_v2;
    let permits = palw_execution_quantum_count_v1(
        u128::from(credit),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        Hash64::default(),
        palw_execution_canonical_work_id_v1(Hash64::default()),
    );
    let permit_sompi = u128::from(permits).saturating_mul(u128::from(palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI)));

    Row {
        draw_mac_eq: draw,
        class_target,
        attempts,
        real_work_claim,
        reward_0124_sompi,
        reward_0132_sompi,
        credit,
        permits,
        permit_sompi,
        claim_pwu,
        fork_weight_sompi: palw_fork_weight_sompi_v1(claim_pwu, T12_SLASH_VALUE_PER_PWU),
        exposure_pwu_v3: palw_exposure_pwu_v3(&class, claim_pwu, Some(canonical_u64), basis),
    }
}

/// value / real_work, as a float, for the table only. Every underlying number is integer.
fn per(numerator: u128, real_work: u128) -> f64 {
    numerator as f64 / real_work.max(1) as f64
}

// =================================================================================================
// 3. Sanity: the two spellings of "real work" agree, so the denominator choice is not load-bearing.
// =================================================================================================

#[test]
fn the_two_spellings_of_real_work_agree() {
    let cases: Vec<(&str, PalwShapeProfileV3, (u32, u32))> = vec![
        ("BASE-0 floor", floor_profile(), PALW_RC_BASE0_CANONICAL),
        ("Qwen2.5 dense @2M", dense_profile(t12_dense_geometry()).unwrap(), qwen25_a16_held_canonical_v1(2_097_152)),
        ("Qwen3.6 hybrid @512", hybrid_profile(t12_hybrid_geometry()).unwrap(), qwen36_held_canonical_v1(512)),
    ];
    println!("\n  class                   arithmetic_mac_eq()      palw_attempt_economic_compute_v1");
    for (name, p, (pf, dc)) in &cases {
        let job = rc_job_context(p, *pf, *dc);
        let vec_mac = draw_mac_eq(p, *pf, *dc).unwrap();
        let breakdown = palw_attempt_economic_compute_v1(p, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        println!("  {name:22}  {vec_mac:>22}  {breakdown:>22}");
        assert_eq!(vec_mac, breakdown, "{name}: the canonical vector's scalar IS the economic breakdown total");
    }
}

// =================================================================================================
// 4. THE TABLE — every class registered on t12.
// =================================================================================================

#[test]
fn the_t12_ratio_table() {
    let carve = t12_worker_carve_permille();
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w0 = palw_work_floor_v1(escrow, rate);

    println!("\n=== t12 MEASURED PARAMETERS ===============================================");
    println!("  palw_overlay_carve.worker_carve_permille   {carve} permille       [Params]");
    println!("  block subsidy                              {T12_BLOCK_SUBSIDY_SOMPI} sompi   [SUPPLIED from kaspa-consensus]");
    println!("  escrow = palw_reward_carve_v2(...).worker  {escrow} sompi   [MEASURED]");
    println!("  rate_sompi_per_giga                        {rate} sompi/1e9 CCU [Params]");
    println!("  W0 = palw_work_floor_v1(escrow, rate)      {w0} CCU         [MEASURED]");
    println!("  PALW_EXECUTION_QUANTUM_V1                  {PALW_EXECUTION_QUANTUM_V1}  (declared in EXPOSURE pwu)");
    println!("  permit value                               {} sompi", palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI));
    println!("  slash_value_per_pwu                        {T12_SLASH_VALUE_PER_PWU} sompi/pwu");

    let floor = floor_profile();
    let floor_draw = draw_mac_eq(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1).unwrap();
    let basis = Some(PalwExposureBasisV1 { base_declared: 7_708, base_canonical: floor_draw.min(u64::MAX as u128) as u64 });
    println!("\n  exposure basis: base_declared 7708 leaves / base_canonical {floor_draw} MAC-eq");

    let dense = dense_profile(t12_dense_geometry()).unwrap();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let hybrid = hybrid_profile(t12_hybrid_geometry()).unwrap();
    let (hp, hd) = qwen36_held_canonical_v1(512);

    let classes: Vec<(&str, u128)> = vec![
        ("BASE-0 floor (8,4)", floor_draw),
        ("Qwen3.6 hybrid @512", draw_mac_eq(&hybrid, hp, hd).unwrap()),
        ("Qwen2.5 dense @2M", draw_mac_eq(&dense, dp, dd).unwrap()),
    ];

    println!("\n=== RAW NUMERATORS, W = W0, bits = 0 (network factor = exactly one draw) ===");
    println!(
        "  {:<22} {:>20} {:>8} {:>22} {:>16} {:>16} {:>12} {:>22}",
        "class", "draw MAC-eq", "attempts", "real_work MAC-eq", "reward0124", "reward0132", "permits", "fork_weight sompi"
    );
    let mut rows = Vec::new();
    for (name, draw) in &classes {
        let r = measure(*draw, escrow, w0, rate, 0, basis);
        println!(
            "  {:<22} {:>20} {:>8} {:>22} {:>16} {:>16} {:>12} {:>22}",
            name, r.draw_mac_eq, r.attempts, r.real_work_claim, r.reward_0124_sompi, r.reward_0132_sompi, r.permits, r.fork_weight_sompi
        );
        rows.push((*name, r));
    }

    println!("\n=== THE FOUR RATIOS (value per MAC-eq of real work, per WON CLAIM) ===");
    println!(
        "  {:<22} {:>16} {:>16} {:>16} {:>16} {:>14}",
        "class", "reward/work", "credit/work", "permit_sompi/wk", "forkwt/work", "credit unit"
    );
    for (name, r) in &rows {
        println!(
            "  {:<22} {:>16.6e} {:>16.6e} {:>16.6e} {:>16.6e} {:>14}",
            name,
            per(r.reward_0124_sompi as u128, r.real_work_claim),
            per(u128::from(r.credit), r.real_work_claim),
            per(r.permit_sompi, r.real_work_claim),
            per(r.fork_weight_sompi, r.real_work_claim),
            r.credit
        );
    }

    println!("\n=== NORMALISED TO THE HYBRID ROW (x = how many times more value per real MAC-eq) ===");
    let base = rows.iter().find(|(n, _)| n.starts_with("Qwen3.6")).map(|(_, r)| *r).unwrap();
    let b_rew = per(base.reward_0124_sompi as u128, base.real_work_claim);
    let b_cr = per(u128::from(base.credit), base.real_work_claim);
    let b_pm = per(base.permit_sompi, base.real_work_claim);
    let b_fw = per(base.fork_weight_sompi, base.real_work_claim);
    println!("  {:<22} {:>14} {:>14} {:>14} {:>14}", "class", "reward x", "credit x", "permit x", "fork_weight x");
    for (name, r) in &rows {
        println!(
            "  {:<22} {:>14.4} {:>14.4} {:>14.4} {:>14.4}",
            name,
            per(r.reward_0124_sompi as u128, r.real_work_claim) / b_rew,
            per(u128::from(r.credit), r.real_work_claim) / b_cr,
            per(r.permit_sompi, r.real_work_claim) / b_pm,
            per(r.fork_weight_sompi, r.real_work_claim) / b_fw
        );
    }

    println!("\n=== EXPOSURE (collateral) UNIT vs CREDIT/PERMIT UNIT — the same claim, two units ===");
    println!("  {:<22} {:>22} {:>22} {:>12}", "class", "credit (raw MAC-eq)", "exposure_pwu_v3", "ratio");
    for (name, r) in &rows {
        println!(
            "  {:<22} {:>22} {:>22} {:>12.1}",
            name,
            r.credit,
            r.exposure_pwu_v3,
            r.credit as f64 / r.exposure_pwu_v3.max(1) as f64
        );
    }

    println!("\n=== SEAT LOCK the fork weight demands (ADR-0151 D1, 3-of-5 quorum, +10% margin) ===");
    println!("  {:<22} {:>24} {:>24} {:>18}", "class", "max_fraud_gain sompi", "seat lock sompi", "seat lock MSK");
    for (name, r) in &rows {
        let realizable = palw_realizable_before_maturity_v1(
            r.permits,
            T12_WINDOW_CHALLENGE_DAA,
            T12_WINDOW_COURT_DAA,
            T12_CADENCE_MS,
            palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI),
        );
        let gain = (escrow as u128).saturating_add(r.fork_weight_sompi).saturating_add(realizable);
        let lock = palw_seat_lock_required_v2(gain, T12_COLLUDING_QUORUM);
        println!("  {:<22} {:>24} {:>24} {:>18.2}", name, gain, lock, lock as f64 / 1e8);
    }
    println!("  posted t12 genesis collateral per seat: 51_642_979_663_480 sompi = 516_429.80 MSK");

    println!("\n  (reference) escrow used by the genesis collateral derivation: {T12_GENESIS_DERIVATION_ESCROW_SOMPI} sompi");
}

// =================================================================================================
// 5. THE SWEEPS — shrink real work, watch whether the ratio shrinks with it.
// =================================================================================================

/// Print one sweep and flag every point where the ratio moved.
fn sweep(label: &str, points: Vec<(String, Option<u128>)>, escrow: u64, w: u128, rate: u64, basis: Option<PalwExposureBasisV1>) {
    println!("\n--- SWEEP {label} ------------------------------------------------------");
    println!(
        "  {:<26} {:>22} {:>10} {:>16} {:>13} {:>13} {:>13} {:>13}",
        "value", "draw MAC-eq", "attempts", "real_work", "reward/wk", "credit/wk", "permit/wk", "forkwt/wk"
    );
    let mut previous: Option<(u128, f64, f64, f64, f64)> = None;
    for (value, draw) in points {
        let Some(draw) = draw else {
            println!("  {value:<26} {:>22}", "REFUSED by the builder");
            continue;
        };
        let r = measure(draw, escrow, w, rate, 0, basis);
        let rw = per(r.reward_0124_sompi as u128, r.real_work_claim);
        let cr = per(u128::from(r.credit), r.real_work_claim);
        let pm = per(r.permit_sompi, r.real_work_claim);
        let fw = per(r.fork_weight_sompi, r.real_work_claim);
        println!(
            "  {value:<26} {:>22} {:>10} {:>16} {rw:>13.4e} {cr:>13.4e} {pm:>13.4e} {fw:>13.4e}",
            r.draw_mac_eq, r.attempts, r.real_work_claim
        );
        if let Some((pw, prw, pcr, ppm, pfw)) = previous {
            let work_move = r.real_work_claim as f64 / pw.max(1) as f64;
            let flags: Vec<String> = [("reward", rw / prw.max(f64::MIN_POSITIVE)), ("credit", cr / pcr.max(f64::MIN_POSITIVE)), ("permit", pm / ppm.max(f64::MIN_POSITIVE)), ("forkwt", fw / pfw.max(f64::MIN_POSITIVE))]
                .iter()
                .filter(|(_, m)| !m.is_finite() || (*m - 1.0).abs() > 0.005)
                .map(|(n, m)| format!("{n} x{m:.4}"))
                .collect();
            if !flags.is_empty() {
                println!("      ^ real_work x{work_move:.4}  ==> RATIO MOVED: {}", flags.join(", "));
            }
        }
        previous = Some((r.real_work_claim, rw, cr, pm, fw));
    }
}

#[test]
fn sweep_the_shape_parameters_for_discontinuities() {
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w = palw_work_floor_v1(escrow, rate);
    let floor_draw = draw_mac_eq(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1).unwrap();
    let basis = Some(PalwExposureBasisV1 { base_declared: 7_708, base_canonical: floor_draw.min(u64::MAX as u128) as u64 });

    println!("\n############ SHAPE SWEEPS (dense Qwen2.5 lineage unless stated) ############");
    println!("  W (work target) held at W0 = {w} CCU; escrow {escrow} sompi; bits = 0.");

    // --- n_ctx: the ladder's own boundary values plus the t12 row -------------------------------
    let n_ctx_points: Vec<(String, Option<u128>)> = [
        16u32, 64, 128, 256, 512, 1_024, 4_096, 16_384, 65_536, 262_144, 524_288, 1_048_576, 2_097_152, 4_194_304,
    ]
    .iter()
    .map(|&n| {
        let d = dense_profile(PalwQwen25GeometryV1 { n_ctx: n, ..QWEN25_1_5B }).and_then(|p| {
            let (pf, dc) = qwen25_a16_held_canonical_v1(n);
            draw_mac_eq(&p, pf, dc)
        });
        (format!("n_ctx = {n}"), d)
    })
    .collect();
    sweep("n_ctx (dense, canonical job follows the ladder)", n_ctx_points, escrow, w, rate, basis);

    // --- layer_count: 0, 1, 2, N-1, N, N+1, max ------------------------------------------------
    let layer_points: Vec<(String, Option<u128>)> = [0u16, 1, 2, 27, 28, 29, 56, 255, 1024, u16::MAX]
        .iter()
        .map(|&l| {
            let g = PalwQwen25GeometryV1 { layer_count: l, n_ctx: 4_096, ..QWEN25_1_5B };
            let d = dense_profile(g).and_then(|p| {
                let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("layer_count = {l}"), d)
        })
        .collect();
    sweep("layer_count (dense @ n_ctx 4096)", layer_points, escrow, w, rate, basis);

    // --- hidden_dim -----------------------------------------------------------------------------
    let hidden_points: Vec<(String, Option<u128>)> = [0u32, 1, 2, 128, 1_535, 1_536, 1_537, 3_072, 16_384, 65_536]
        .iter()
        .map(|&h| {
            let g = PalwQwen25GeometryV1 { hidden_dim: h, n_ctx: 4_096, ..QWEN25_1_5B };
            let d = dense_profile(g).and_then(|p| {
                let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("hidden_dim = {h}"), d)
        })
        .collect();
    sweep("hidden_dim (dense @ n_ctx 4096)", hidden_points, escrow, w, rate, basis);

    // --- ffn_dim ---------------------------------------------------------------------------------
    let ffn_points: Vec<(String, Option<u128>)> = [0u32, 1, 2, 8_959, 8_960, 8_961, 17_920, 131_072]
        .iter()
        .map(|&f| {
            let g = PalwQwen25GeometryV1 { ffn_dim: f, n_ctx: 4_096, ..QWEN25_1_5B };
            let d = dense_profile(g).and_then(|p| {
                let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("ffn_dim = {f}"), d)
        })
        .collect();
    sweep("ffn_dim (dense @ n_ctx 4096)", ffn_points, escrow, w, rate, basis);

    // --- vocab_size --------------------------------------------------------------------------------
    let vocab_points: Vec<(String, Option<u128>)> = [0u32, 1, 2, 151_935, 151_936, 151_937, 303_872, 2_000_000]
        .iter()
        .map(|&v| {
            let g = PalwQwen25GeometryV1 { vocab_size: v, n_ctx: 4_096, ..QWEN25_1_5B };
            let d = dense_profile(g).and_then(|p| {
                let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("vocab_size = {v}"), d)
        })
        .collect();
    sweep("vocab_size (dense @ n_ctx 4096)", vocab_points, escrow, w, rate, basis);

    // --- tile_len ----------------------------------------------------------------------------------
    let mut tile_points: Vec<(String, Option<u128>)> = Vec::new();
    for t in [0u32, 1, 2, 24, 32, 127, 128, 129, 256, 4_096, 65_536] {
        let g = PalwQwen25GeometryV1 { tile_len: t, n_ctx: 4_096, ..QWEN25_1_5B };
        match dense_profile_guarded(g) {
            Err(()) => println!("  tile_len = {t}: the BUILDER PANICKED (Ord::clamp min <= max), it did not return Err"),
            Ok(p) => {
                let d = p.and_then(|p| {
                    let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                    draw_mac_eq(&p, pf, dc)
                });
                tile_points.push((format!("tile_len = {t}"), d));
            }
        }
    }
    sweep("tile_len (dense @ n_ctx 4096) — a PACKAGING field, not arithmetic", tile_points, escrow, w, rate, basis);

    // --- attn_heads / kv_heads ----------------------------------------------------------------------
    let head_points: Vec<(String, Option<u128>)> = [0u16, 1, 2, 11, 12, 13, 24, 128]
        .iter()
        .map(|&h| {
            let g = PalwQwen25GeometryV1 { attn_heads: h, n_ctx: 4_096, ..QWEN25_1_5B };
            let d = dense_profile(g).and_then(|p| {
                let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("attn_heads = {h}"), d)
        })
        .collect();
    sweep("attn_heads (dense @ n_ctx 4096)", head_points, escrow, w, rate, basis);

    // --- MoE: n_experts and experts_per_token (hybrid lineage) ---------------------------------------
    let expert_points: Vec<(String, Option<u128>)> = [0u32, 1, 2, 8, 128, 255, 256, 257, 512, 4_096]
        .iter()
        .map(|&e| {
            let g = PalwQwen36GeometryV1 { n_experts: e, n_ctx: 512, ..QWEN36_35B_A3B };
            let d = hybrid_profile(g).and_then(|p| {
                let (pf, dc) = qwen36_held_canonical_v1(512);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("n_experts = {e}"), d)
        })
        .collect();
    sweep("n_experts (hybrid @512) — the WEIGHT-MEMORY parameter", expert_points, escrow, w, rate, basis);

    let active_points: Vec<(String, Option<u128>)> = [0u32, 1, 2, 7, 8, 9, 16, 64, 256]
        .iter()
        .map(|&k| {
            let g = PalwQwen36GeometryV1 { experts_per_token: k, n_ctx: 512, ..QWEN36_35B_A3B };
            let d = hybrid_profile(g).and_then(|p| {
                let (pf, dc) = qwen36_held_canonical_v1(512);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("experts_per_token = {k}"), d)
        })
        .collect();
    sweep("experts_per_token (hybrid @512) — the ACTIVE-COMPUTE parameter", active_points, escrow, w, rate, basis);

    // --- hybrid n_ctx --------------------------------------------------------------------------------
    let hyb_ctx: Vec<(String, Option<u128>)> = [8u32, 64, 128, 256, 512, 1_024, 4_096, 65_536, 1_048_576, 2_097_152]
        .iter()
        .map(|&n| {
            let g = PalwQwen36GeometryV1 { n_ctx: n, ..QWEN36_35B_A3B };
            let d = hybrid_profile(g).and_then(|p| {
                let (pf, dc) = qwen36_held_canonical_v1(n);
                draw_mac_eq(&p, pf, dc)
            });
            (format!("n_ctx = {n}"), d)
        })
        .collect();
    sweep("n_ctx (hybrid)", hyb_ctx, escrow, w, rate, basis);
}

// =================================================================================================
// 6. QUANTIZATION / dtype — the one sweep that needs a profile mutation rather than a geometry.
// =================================================================================================

#[test]
fn sweep_the_weight_dtype() {
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w = palw_work_floor_v1(escrow, rate);
    let floor_draw = draw_mac_eq(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1).unwrap();
    let basis = Some(PalwExposureBasisV1 { base_declared: 7_708, base_canonical: floor_draw.min(u64::MAX as u128) as u64 });

    // GGML dtype codes and what `palw_weight_dtype_cost_v1` charges:
    //   0 F32 -> 4, 1 F16 -> 2, 30 BF16 -> 2, 25 I16 -> 2, 26 I32 -> 4, everything else -> 1.
    let codes: [(u8, &str); 8] =
        [(24, "I8 (t12 ships this)"), (1, "F16"), (30, "BF16"), (25, "I16"), (26, "I32"), (0, "F32"), (2, "Q4_0"), (15, "Q8_K-ish")];

    let base = dense_profile(PalwQwen25GeometryV1 { n_ctx: 4_096, ..QWEN25_1_5B }).unwrap();
    let (pf, dc) = qwen25_a16_held_canonical_v1(4_096);

    println!("\n--- SWEEP weight dtype (dense @4096, EVERY node's weight_dtypes rewritten) ---");
    println!(
        "  {:<24} {:>22} {:>16} {:>13} {:>13} {:>13} {:>13}",
        "dtype", "draw MAC-eq", "real_work", "reward/wk", "credit/wk", "permit/wk", "forkwt/wk"
    );
    let mut previous: Option<(u128, f64, f64, f64, f64)> = None;
    for (code, name) in codes {
        let mut p = base.clone();
        for table in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
            for node in table.iter_mut() {
                if !node.weight_dtypes.is_empty() {
                    node.weight_dtypes = vec![code; node.weight_dtypes.len()];
                }
            }
        }
        let job = rc_job_context(&p, pf, dc);
        let Ok(d) = PalwCanonicalClassDescriptorV1::of(&p, Hash64::default()) else {
            println!("  {:<24} descriptor REFUSED (mixed weight formats)", format!("{code} {name}"));
            continue;
        };
        let Ok(vec) = palw_canonical_draw_work_v1(&d, &job, true) else {
            println!("  {:<24} work REFUSED", format!("{code} {name}"));
            continue;
        };
        let draw = vec.arithmetic_mac_eq();
        let r = measure(draw, escrow, w, rate, 0, basis);
        let rw = per(r.reward_0124_sompi as u128, r.real_work_claim);
        let cr = per(u128::from(r.credit), r.real_work_claim);
        let pm = per(r.permit_sompi, r.real_work_claim);
        let fw = per(r.fork_weight_sompi, r.real_work_claim);
        println!(
            "  {:<24} {:>22} {:>16} {rw:>13.4e} {cr:>13.4e} {pm:>13.4e} {fw:>13.4e}",
            format!("{code} {name}"),
            draw,
            r.real_work_claim
        );
        println!("      weight_traffic_bytes {:>22}  (priced at ZERO by provisional_scalar_v1)", vec.weight_traffic_bytes);
        if let Some((pw, prw, pcr, ppm, pfw)) = previous {
            let m = r.real_work_claim as f64 / pw.max(1) as f64;
            let moved: Vec<String> = [("reward", rw / prw), ("credit", cr / pcr), ("permit", pm / ppm), ("forkwt", fw / pfw)]
                .iter()
                .filter(|(_, x)| !x.is_finite() || (*x - 1.0).abs() > 0.005)
                .map(|(n, x)| format!("{n} x{x:.4}"))
                .collect();
            if !moved.is_empty() {
                println!("      ^ real_work x{m:.4}  ==> RATIO MOVED: {}", moved.join(", "));
            }
        }
        previous = Some((r.real_work_claim, rw, cr, pm, fw));
    }
}

// =================================================================================================
// 7. THE PURE-NUMERATOR BOUNDARY SWEEPS — 0,1,2,N-1,N,N+1,max-1,max on the credit and pwu paths,
//    independent of any shape, so the saturation points are exact.
// =================================================================================================

#[test]
fn boundary_sweep_of_the_permit_mint_and_the_pwu_product() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = Hash64::default();
    let fid = palw_execution_canonical_work_id_v1(Hash64::default());

    // The exact credit at which the u32 clamp starts eating permits.
    let clamp_credit = u128::from(u32::MAX) * q; // 429_496_729_500_000
    println!("\n--- palw_execution_quantum_count_v1 BOUNDARIES (quantum = {PALW_EXECUTION_QUANTUM_V1}) ---");
    println!("  {:<34} {:>14} {:>16}", "credited_work (raw MAC-eq)", "permits", "permits/work");
    for credit in [
        0u128,
        1,
        2,
        q - 1,
        q,
        q + 1,
        2 * q,
        clamp_credit - q,
        clamp_credit - 1,
        clamp_credit,
        clamp_credit + 1,
        clamp_credit + q,
        clamp_credit * 2,
        3_357_281_757_221_376, // the t12 dense @2M draw
        u128::from(u64::MAX),
    ] {
        let n = palw_execution_quantum_count_v1(credit, q, seed, fid);
        println!("  {credit:<34} {n:>14} {:>16.6e}", per(u128::from(n), credit));
    }
    println!("  => the clamp bites at credited_work = {clamp_credit} raw MAC-eq.");
    println!("  => the t12 dense @2M draw is 3_357_281_757_221_376, i.e. {:.3}x past it.", 3_357_281_757_221_376f64 / clamp_credit as f64);

    // `palw_pwu_v1` / `palw_attempt_derived_pwu_v1` saturate into u64.
    println!("\n--- palw_attempt_derived_pwu_v1 BOUNDARIES (fork weight's only input) ---");
    println!("  {:<24} {:>24} {:>10} {:>24}", "per_draw MAC-eq", "class_target", "attempts", "claim_pwu (u64 SAT)");
    for draw in [3_357_281_757_221_376u128, 158_574_197_994, 21_657_728] {
        for target in [u128::MAX, u128::MAX / 2, u128::MAX / 5_494, u128::MAX / 5_495, u128::MAX / 5_496, u128::MAX / 1_000_000] {
            let pwu = palw_attempt_derived_pwu_v1(target, draw);
            let a = palw_expected_attempts_v1(target);
            let honest = (a as u128).saturating_mul(draw);
            let sat = if u128::from(pwu) < honest { "  <-- SATURATED" } else { "" };
            println!("  {draw:<24} {target:>24} {a:>10} {pwu:>24}{sat}");
        }
    }
}

// =================================================================================================
// 8. The W sweep — the reward cap is a function of the chain's work target, not of the shape.
// =================================================================================================

#[test]
fn sweep_the_work_target_and_locate_the_reward_cliff() {
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w0 = palw_work_floor_v1(escrow, rate);
    let floor_draw = draw_mac_eq(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1).unwrap();
    let basis = Some(PalwExposureBasisV1 { base_declared: 7_708, base_canonical: floor_draw.min(u64::MAX as u128) as u64 });

    let dense = dense_profile(t12_dense_geometry()).unwrap();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let dense_draw = draw_mac_eq(&dense, dp, dd).unwrap();
    let hybrid = hybrid_profile(t12_hybrid_geometry()).unwrap();
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let hybrid_draw = draw_mac_eq(&hybrid, hp, hd).unwrap();

    println!("\n--- The two REGISTERED t12 model rows against one W, W = W0 = {w0} ---");
    for (name, draw) in [("Qwen3.6 hybrid @512", hybrid_draw), ("Qwen2.5 dense @2M", dense_draw)] {
        let r = measure(draw, escrow, w0, rate, 0, basis);
        println!(
            "  {name:<22} draw {:>20}  target/MAX {:.6}  attempts {:>3}  real_work {:>20}  reward {:>14}  reward/work {:.6e}",
            r.draw_mac_eq,
            r.class_target as f64 / u128::MAX as f64,
            r.attempts,
            r.real_work_claim,
            r.reward_0124_sompi,
            per(r.reward_0124_sompi as u128, r.real_work_claim)
        );
    }
    let rh = measure(hybrid_draw, escrow, w0, rate, 0, basis);
    let rd = measure(dense_draw, escrow, w0, rate, 0, basis);
    println!(
        "\n  ARBITRAGE between the two rows t12 actually registers: {:.1}x more reward per MAC-eq for the hybrid.",
        per(rh.reward_0124_sompi as u128, rh.real_work_claim) / per(rd.reward_0124_sompi as u128, rd.real_work_claim)
    );

    println!("\n--- SWEEP the synthetic draw size against a fixed W, to locate the cliff ---");
    println!(
        "  {:<24} {:>10} {:>24} {:>16} {:>14} {:>14}",
        "draw (MAC-eq)", "attempts", "real_work", "reward sompi", "reward/work", "vs W0 row"
    );
    let at_w0 = measure(w0, escrow, w0, rate, 0, basis);
    let ref_ratio = per(at_w0.reward_0124_sompi as u128, at_w0.real_work_claim);
    for mult in [1u128, 2, 4, 10, 100, 1_000, 9_440, 10_000, 100_000] {
        for draw in [w0 / mult, w0.saturating_mul(mult)] {
            if draw == 0 {
                continue;
            }
            let r = measure(draw, escrow, w0, rate, 0, basis);
            let ratio = per(r.reward_0124_sompi as u128, r.real_work_claim);
            println!(
                "  {draw:<24} {:>10} {:>24} {:>16} {ratio:>14.4e} {:>14.4}",
                r.attempts,
                r.real_work_claim,
                r.reward_0124_sompi,
                ratio / ref_ratio
            );
        }
    }
}

// =================================================================================================
// 9. THE LIVE REWARD ARM. `apply_palw_transition`'s reward selection (palw_state_v2.rs:11863-11867)
//    is:
//        Some(snapshot) => snapshot.priced_reward(claim.escrowed_reward)      <- ADR-0132
//        None if work_priced_reward_active => self.work_priced_escrow(claim)  <- ADR-0124
//        None => claim.escrowed_reward
//    and `snapshot_claim_economics` (9911) writes a snapshot for EVERY class except the base class,
//    whenever `palw_economic_payout` is armed and the registry holds work. t12 arms it at DAA 0 and
//    the genesis registry holds work for both model rows. So on t12:
//      * the BASE-0 floor takes the `work_priced_escrow` arm, which returns the escrow WHOLE;
//      * both model rows take the ADR-0132 arm.
//    This test measures that arm through `palw_claim_economics_snapshot_v1` + `priced_reward`,
//    with the class targets read out of the t12 genesis objects.
// =================================================================================================

#[test]
fn the_live_reward_arm_at_the_t12_genesis_targets() {
    use kaspa_consensus_core::config::params::PalwEconomicPayoutV1;
    use kaspa_consensus_core::palw_economic_payout_v1::{PalwEconomicPayoutFoldV1, palw_claim_economics_snapshot_v1};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let params = t12();
    let escrow = t12_escrow_sompi();
    let fence: PalwEconomicPayoutV1 = params.palw_economic_payout.expect("armed");
    let fold = PalwEconomicPayoutFoldV1 {
        rate_sompi_per_giga: fence.rate_sompi_per_giga,
        panel_share_alpha_permille: fence.panel_share_alpha_permille,
        panel_share_min_permille: fence.panel_share_min_permille,
        panel_share_max_permille: fence.panel_share_max_permille,
        cap_utilization_max_permille: fence.cap_utilization_max_permille,
        block_bits: 0,
    };

    // --- the genesis registrations, straight out of the shipped bundle ---------------------------
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let mut genesis: Vec<(Hash64, u64, u128, u16)> = Vec::new(); // (class_id, declared_leaves, initial_target, share)
    for object in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, share_permille, .. } = object {
            let declared = kaspa_consensus_core::palw_state_v2::palw_max_exposure_pwu_of_rule_v1(pwu_rule);
            genesis.push((*class_id, declared, *initial_target, *share_permille));
        }
    }
    println!("\n=== t12 GENESIS CLASS REGISTRATIONS (read from the shipped bundle) ===");
    for (id, declared, target, share) in &genesis {
        println!("  {:.16}  declared_leaves {declared:>16}  initial_target {target:>42}  share {share}permille", id.to_string());
    }

    // --- the three profiles, with their canonical jobs and the registry's own work ---------------
    let floor = floor_profile();
    let dense = dense_profile(t12_dense_geometry()).unwrap();
    // The genesis rows since 2026-09-23: the 8k dense row replaced the hybrid @512 row.
    let dense_8k = dense_profile(t12_dense_8k_geometry()).unwrap();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let (ep, ed) = qwen25_a16_held_canonical_v1(8_192);
    let rows: Vec<(&str, PalwShapeProfileV3, (u32, u32), u64)> = vec![
        ("BASE-0 floor", floor, PALW_RC_BASE0_CANONICAL, 7_708),
        ("Qwen2.5 dense @8k", dense_8k, (ep, ed), 105_518_224),
        ("Qwen2.5 dense @2M", dense, (dp, dd), 27_002_967_184),
    ];

    // Match each profile to its genesis target by the declared leaf count its registration carries.
    let target_of = |declared: u64| genesis.iter().find(|(_, d, _, _)| *d == declared).map(|(_, _, t, _)| *t);

    println!("\n=== THE LIVE ARM AT THE GENESIS TARGETS (block 1 .. the first epoch boundary) ===");
    println!(
        "  {:<22} {:>10} {:>22} {:>18} {:>14} {:>12}",
        "class", "attempts", "real_work MAC-eq", "reward sompi", "reward/work", "cap util permille"
    );
    let mut measured: Vec<(&str, u128, u64, u32, u128, u64, u128)> = Vec::new();
    for (name, profile, (pf, dc), declared) in &rows {
        let job = rc_job_context(profile, *pf, *dc);
        let work = palw_model_work_from_carriage_v1(profile, &job).expect("the registry derives work for a shipped row");
        let draw = work.economic_ccu_per_claim;
        let target = target_of(*declared).expect("every shipped row's declared leaves match a genesis registration");
        let attempts = palw_expected_attempts_v1(target);
        let real_work = (attempts as u128).saturating_mul(draw);

        // The base class takes the OTHER arm: `snapshot_claim_economics` returns early for it, and
        // `work_priced_escrow` hands the floor its escrow whole.
        let is_floor = *name == "BASE-0 floor";
        let (reward, cap_util, _attempted) = if is_floor {
            (escrow, 0u32, 0u128)
        } else {
            let snap = palw_claim_economics_snapshot_v1(&fold, &work, 5, target, 0);
            let a = snap.attempted_ccu();
            (
                snap.priced_reward(escrow),
                kaspa_consensus_core::palw_economic_payout_v1::palw_cap_utilization_permille_v1(a, fold.rate_sompi_per_giga, escrow),
                a,
            )
        };
        let claim_pwu = palw_attempt_derived_pwu_v1(target, draw);
        let fork_weight = palw_fork_weight_sompi_v1(claim_pwu, T12_SLASH_VALUE_PER_PWU);
        println!(
            "  {name:<22} {attempts:>10} {real_work:>22} {reward:>18} {:>14.6e} {cap_util:>12}",
            per(reward as u128, real_work)
        );
        measured.push((name, real_work, reward, cap_util, fork_weight, *declared, draw));
    }

    println!("\n=== reward per MAC-eq, normalised to the BEST-PAID class ===");
    let best = measured.iter().map(|(_, rw, r, _, _, _, _)| per(*r as u128, *rw)).fold(0.0f64, f64::max);
    for (name, rw, r, _, _, _, _) in &measured {
        println!("  {name:<22} {:>14.6e} sompi/MAC-eq   = 1 / {:.1}", per(*r as u128, *rw), best / per(*r as u128, *rw));
    }

    println!("\n=== UNITS: declared leaves vs derived MAC-eq per draw, per class ===");
    println!("  {:<22} {:>18} {:>24} {:>14}", "class", "declared leaves", "derived MAC-eq/draw", "ratio U2/U1");
    for (name, _, _, _, _, declared, draw) in &measured {
        println!("  {name:<22} {declared:>18} {draw:>24} {:>14.1}", *draw as f64 / *declared as f64);
    }

    println!("\n=== fork weight vs the cash it was paid for the SAME claim ===");
    println!("  {:<22} {:>24} {:>18} {:>14}", "class", "fork_weight sompi", "reward sompi", "weight/cash");
    for (name, _, r, _, fw, _, _) in &measured {
        println!("  {name:<22} {fw:>24} {r:>18} {:>14.1}", *fw as f64 / (*r).max(1) as f64);
    }

    println!("\n=== SEAT LOCK vs the t12 posted collateral (516,429.80 MSK per seat) ===");
    const POSTED: u128 = 51_642_979_663_480;
    println!("  {:<22} {:>24} {:>24} {:>12}", "class", "max_fraud_gain sompi", "seat lock required", "short by");
    for (name, _, r, _, fw, _, draw) in &measured {
        let credit = (*draw).min(u128::from(u64::MAX)) as u64;
        let permits = palw_execution_quantum_count_v1(
            u128::from(credit),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            Hash64::default(),
            palw_execution_canonical_work_id_v1(Hash64::default()),
        );
        let realizable = palw_realizable_before_maturity_v1(
            permits,
            T12_WINDOW_CHALLENGE_DAA,
            T12_WINDOW_COURT_DAA,
            T12_CADENCE_MS,
            palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI),
        );
        let gain = u128::from(*r).saturating_add(*fw).saturating_add(realizable);
        let lock = palw_seat_lock_required_v2(gain, T12_COLLUDING_QUORUM);
        println!("  {name:<22} {gain:>24} {lock:>24} {:>12.1}x", lock as f64 / POSTED as f64);
    }
}

// =================================================================================================
// 10. The permit clamp, stated as an equal-mint pair.
// =================================================================================================

#[test]
fn two_very_different_executions_mint_exactly_the_same_permits() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = Hash64::default();
    let fid = palw_execution_canonical_work_id_v1(Hash64::default());
    let clamp = u128::from(u32::MAX) * q;

    let cheap = clamp; // 429_496_729_500_000 MAC-eq
    let dear = 3_357_281_757_221_376u128; // the t12 dense @2M draw, MEASURED above

    let n_cheap = palw_execution_quantum_count_v1(cheap, q, seed, fid);
    let n_dear = palw_execution_quantum_count_v1(dear, q, seed, fid);
    println!("\n  credited_work {cheap:>20} MAC-eq -> {n_cheap} permits");
    println!("  credited_work {dear:>20} MAC-eq -> {n_dear} permits");
    println!("  real work ratio {:.4}x, permit ratio {:.4}x", dear as f64 / cheap as f64, f64::from(n_dear) / f64::from(n_cheap));
    assert_eq!(n_cheap, n_dear, "the u32 clamp makes 7.8x less real compute mint the identical permit set");

    // And what the constant's own doc predicts if `credited_work` were in the unit the constant is
    // declared in (exposure pwu), for the same two executions.
    let basis_declared = 7_708u128;
    let basis_canonical = 21_657_728u128;
    for (label, raw) in [("clamp threshold", cheap), ("t12 dense @2M", dear)] {
        let in_exposure_units = raw * basis_declared / basis_canonical;
        println!(
            "  {label:<18} raw {raw:>20} MAC-eq -> {:>12} permits;  as exposure pwu {:>16} -> {:>12} permits",
            palw_execution_quantum_count_v1(raw, q, seed, fid),
            in_exposure_units,
            palw_execution_quantum_count_v1(in_exposure_units, q, seed, fid)
        );
    }
}

// =================================================================================================
// 11. Is the cheapest-paid class THROTTLED? The live cadence cap is `derive_epoch_budgets_v2`
//     (palw_state_v2.rs:13816), denominated in BLOCKS:
//         budget_c(e) = floor( epoch_length * share_c * tolerance / (1000 * denom_c) )
//     with `denom_c` the sum of shares over the classes that produced in the closed epoch.
//     (`palw_class_daa::class_epoch_budgets_v1` is the PWU-denominated form and has no consensus
//     caller — see palw_economic_locus_v1.rs:64 "Blocks, never pwu".)
// =================================================================================================

#[test]
fn the_cheapest_paid_class_is_not_throttled() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, derive_epoch_budgets_v2};
    use std::collections::{BTreeMap, BTreeSet};

    let params = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("ConsensusV2") };

    let mut shares: BTreeMap<Hash64, u16> = BTreeMap::new();
    let mut names: BTreeMap<Hash64, String> = BTreeMap::new();
    for object in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, share_permille, pwu_rule, .. } = object {
            shares.insert(*class_id, *share_permille);
            let declared = kaspa_consensus_core::palw_state_v2::palw_max_exposure_pwu_of_rule_v1(pwu_rule);
            names.insert(
                *class_id,
                match declared {
                    7_708 => "BASE-0 floor".into(),
                    105_518_224 => "Qwen2.5 dense @8k".into(),
                    _ => "Qwen2.5 dense @2M".into(),
                },
            );
        }
    }
    let epoch_length = bundle.state.epoch_length();
    let tolerance = bundle.state.budget_tolerance_permille();
    println!("\n=== t12 CADENCE (read from the shipped bundle) ===");
    println!("  epoch_length {epoch_length} DAA   budget_tolerance_permille {tolerance}");
    println!("  declared shares: {:?}", shares.values().collect::<Vec<_>>());

    let all: BTreeSet<Hash64> = shares.keys().copied().collect();
    let floor_id = *shares.iter().max_by_key(|(_, s)| **s).map(|(id, _)| id).unwrap();
    let floor_only: BTreeSet<Hash64> = [floor_id].into_iter().collect();

    for (label, competing) in [("all three classes produced last epoch", &all), ("ONLY the floor produced last epoch", &floor_only)] {
        let budgets = derive_epoch_budgets_v2(&shares, &BTreeSet::new(), competing, epoch_length, tolerance, 0);
        println!("\n  --- {label} ---");
        for (id, blocks) in budgets.budget_blocks.iter() {
            println!(
                "    {:<22} budget {blocks:>6} blocks of the {epoch_length}-block epoch   ({:.2}% of the cadence)",
                names[id],
                *blocks as f64 * 100.0 / epoch_length as f64
            );
        }
    }
    println!("\n  => the floor is budgeted ~99.8% of every epoch's attempt-lane blocks, and every one of");
    println!("     them escrows the SAME worker carve as a dense-2M block (escrowed_reward is");
    println!("     worker_carve_v2 of the block's own subsidy, palw_state_v2.rs:18922 — class-independent).");
}

// =================================================================================================
// 12. TWO TARGETS FOR ONE CLAIM.
//
//   * the DRAW (what the producer must actually beat) is
//     `palw_effective_class_target_v1` = `palw_work_ticket_target_v1(ccu, max(W0, W))`
//     (palw_admission_v2.rs:882-896), and `check_palw_attempt_admission_v2` forces
//     `attempt.pwu == palw_attempt_derived_pwu_v1(that target, per_draw)` (palw_admission_v2.rs:431-450).
//   * the PAY (`expected_attempts_q32` inside the ADR-0132 snapshot) is
//     `self.state.class_targets.get(class_id)` (palw_state_v2.rs:9919) — the STORED target.
//
//   And past `work_target_active` the retarget SKIPS every non-base class
//   (palw_state_v2.rs:14706 `if builder.extras.work_target_active && !counters_are_receipts &&
//   class_id != base { continue }`), so a model row's stored target stays at its genesis
//   `initial_target` while its real draw follows `CCU / W`.
// =================================================================================================

#[test]
fn the_draw_target_and_the_pay_target_are_not_the_same_number() {
    use kaspa_consensus_core::config::params::PalwEconomicPayoutV1;
    use kaspa_consensus_core::palw_economic_payout_v1::{
        PalwEconomicPayoutFoldV1, palw_cap_utilization_permille_v1, palw_claim_economics_snapshot_v1,
    };
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let params = t12();
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w0 = palw_work_floor_v1(escrow, rate);
    let fence: PalwEconomicPayoutV1 = params.palw_economic_payout.unwrap();
    let fold = PalwEconomicPayoutFoldV1 {
        rate_sompi_per_giga: fence.rate_sompi_per_giga,
        panel_share_alpha_permille: fence.panel_share_alpha_permille,
        panel_share_min_permille: fence.panel_share_min_permille,
        panel_share_max_permille: fence.panel_share_max_permille,
        cap_utilization_max_permille: fence.cap_utilization_max_permille,
        block_bits: 0,
    };
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let stored: Vec<(u64, u128)> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { pwu_rule, initial_target, .. } => {
                Some((kaspa_consensus_core::palw_state_v2::palw_max_exposure_pwu_of_rule_v1(pwu_rule), *initial_target))
            }
            _ => None,
        })
        .collect();

    let dense = dense_profile(t12_dense_geometry()).unwrap();
    let dense_8k = dense_profile(t12_dense_8k_geometry()).unwrap();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let (ep, ed) = qwen25_a16_held_canonical_v1(8_192);

    println!("\n=== THE TWO TARGETS, PER MODEL ROW (the floor is exempt from both rules) ===");
    println!(
        "  {:<22} {:>12} {:>12} {:>12} {:>14} {:>14} {:>13}",
        "class", "draw att.", "pay att.", "overpay x", "cap util pm", "panel share pm", "panel share pm"
    );
    println!("  {:<22} {:>12} {:>12} {:>12} {:>14} {:>14} {:>13}", "", "(lottery)", "(snapshot)", "", "(ceiling 800)", "(paid, stale)", "(if honest)");
    for (name, profile, job, declared) in
        [("Qwen2.5 dense @8k", &dense_8k, (ep, ed), 105_518_224u64), ("Qwen2.5 dense @2M", &dense, (dp, dd), 27_002_967_184)]
    {
        let ctx = rc_job_context(profile, job.0, job.1);
        let work = palw_model_work_from_carriage_v1(profile, &ctx).unwrap();
        let pay_target = stored.iter().find(|(d, _)| *d == declared).unwrap().1;
        let draw_target = palw_work_ticket_target_v1(work.economic_ccu_per_claim, w0);

        let draw_att = palw_expected_attempts_v1(draw_target);
        let pay_att = palw_expected_attempts_v1(pay_target);
        let stale = palw_claim_economics_snapshot_v1(&fold, &work, 5, pay_target, 0);
        let honest = palw_claim_economics_snapshot_v1(&fold, &work, 5, draw_target, 0);
        let cap = palw_cap_utilization_permille_v1(stale.attempted_ccu(), rate, escrow);
        println!(
            "  {name:<22} {draw_att:>12} {pay_att:>12} {:>12.1} {cap:>14} {:>14} {:>13}",
            pay_att as f64 / draw_att.max(1) as f64,
            stale.panel_share_permille,
            honest.panel_share_permille
        );
        println!(
            "      stored (genesis) target {pay_target}\n      lottery target          {draw_target}\n      attempted_ccu paid on {:>24}  vs really run {:>24}",
            stale.attempted_ccu(),
            honest.attempted_ccu()
        );
        println!("      reward either way: {} sompi (the escrow cap absorbs the difference)", stale.priced_reward(escrow));
    }
}

// =================================================================================================
// 13. `initial_target` is a REGISTRANT-WRITABLE field of `ClassRegistered`, and past the work
//     target nothing re-prices the stored copy (palw_state_v2.rs:14706 skips it). The admission
//     path was fixed to read `palw_effective_class_target_v1` instead — the comment at
//     palw_admission_v2.rs:414-419 names exactly this attack for `pwu` — but the ADR-0132 payout
//     snapshot still reads the stored value. This measures what a registrant buys with it.
// =================================================================================================

#[test]
fn a_registrant_chosen_initial_target_moves_the_panels_pay() {
    use kaspa_consensus_core::config::params::PalwEconomicPayoutV1;
    use kaspa_consensus_core::palw_economic_payout_v1::{PalwEconomicPayoutFoldV1, palw_claim_economics_snapshot_v1};
    use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
    use kaspa_consensus_core::palw_panel_economy_v1::palw_panel_split_permille_v1;

    let params = t12();
    let escrow = t12_escrow_sompi();
    let rate = t12_rate_sompi_per_giga();
    let w0 = palw_work_floor_v1(escrow, rate);
    let fence: PalwEconomicPayoutV1 = params.palw_economic_payout.unwrap();
    let fold = PalwEconomicPayoutFoldV1 {
        rate_sompi_per_giga: fence.rate_sompi_per_giga,
        panel_share_alpha_permille: fence.panel_share_alpha_permille,
        panel_share_min_permille: fence.panel_share_min_permille,
        panel_share_max_permille: fence.panel_share_max_permille,
        cap_utilization_max_permille: fence.cap_utilization_max_permille,
        block_bits: 0,
    };
    println!(
        "\n  fence: alpha {}permille, S_min {}permille, S_max {}permille, rate {rate}",
        fence.panel_share_alpha_permille, fence.panel_share_min_permille, fence.panel_share_max_permille
    );

    let hybrid = hybrid_profile(t12_hybrid_geometry()).unwrap();
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let job = rc_job_context(&hybrid, hp, hd);
    let work = palw_model_work_from_carriage_v1(&hybrid, &job).unwrap();
    let honest_target = palw_work_ticket_target_v1(work.economic_ccu_per_claim, w0);

    println!("\n=== what a registrant's declared `initial_target` does to the panel's pay ===");
    println!("  (the class really draws at {honest_target}, i.e. {} expected draws)", palw_expected_attempts_v1(honest_target));
    println!(
        "  {:<34} {:>14} {:>12} {:>18} {:>18} {:>16}",
        "declared initial_target", "panel share pm", "reward", "panel pool sompi", "producer sompi", "vs honest"
    );
    let honest = palw_claim_economics_snapshot_v1(&fold, &work, 5, honest_target, 0);
    let honest_split = palw_panel_split_permille_v1(honest.priced_reward(escrow), honest.panel_share_permille, 5, 5);
    for (label, target) in [
        ("the honest draw target", honest_target),
        ("t12's genesis initial_target", 3_282_893_071_338_656_179_608_481_139_309_674_495u128),
        ("u128::MAX / 1_000", u128::MAX / 1_000),
        ("u128::MAX / 1_000_000", u128::MAX / 1_000_000),
        ("1  (the hardest declarable)", 1),
    ] {
        let snap = palw_claim_economics_snapshot_v1(&fold, &work, 5, target, 0);
        let reward = snap.priced_reward(escrow);
        let split = palw_panel_split_permille_v1(reward, snap.panel_share_permille, 5, 5);
        let pool = split.paid + split.reserve;
        let honest_pool = honest_split.paid + honest_split.reserve;
        println!(
            "  {label:<34} {:>14} {reward:>12} {pool:>18} {:>18} {:>16}",
            snap.panel_share_permille,
            split.producer,
            format!("{:+} sompi", split.producer as i128 - honest_split.producer as i128)
        );
        let _ = honest_pool;
    }
    println!("\n  the producer keeps every sompi the panel does not get (palw_panel_split_permille_v1:");
    println!("  producer is the exact rest of the reward once the pool is carved).");
}
