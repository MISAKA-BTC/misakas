//! **AGENT A (t12 audit lane A3+A4): integer behaviour and multi-counting.**
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_agent_a_integer_and_multicount -- --nocapture
//!
//! Read-only probe. It builds nothing the chain does not build and asserts only numbers it prints.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_attempted_compute_per_claim_v1, palw_attempted_compute_q32_per_claim_v1, palw_expected_attempts_q32_v1,
    palw_network_expected_attempts_q32_v1, palw_priced_reward_u128_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{
    palw_attempted_ccu_v1, palw_cap_utilization_permille_v1, palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1,
};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_realizable_before_maturity_v1, palw_rounds_per_daa_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_economics_ledger_v1::{PALW_LEDGER_RATE_SCALE_V1, palw_rate_priced_reward_v1};
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---- t12 class fixtures (same builders the genesis card uses) -----------------------------------

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor builds")
}
fn dense_2m_profile() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense builds")
}
fn hybrid_512_profile() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid builds")
}

/// `economic_ccu_per_claim` as `palw_model_work_from_carriage_v1` writes it: the PREFILL-DRAW job's
/// economic compute. This is exactly what `PalwChainStateV2::canonical_per_draw` later returns and
/// what `record_round_final` credits.
fn per_draw_ccu(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> u128 {
    let canonical = rc_job_context(profile, prefill, decode);
    palw_attempt_economic_compute_v1(profile, &canonical, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("economic compute")
}

struct Row {
    name: &'static str,
    declared_leaves: u64,
    per_draw: u128,
}

fn t12_rows() -> Vec<Row> {
    let floor = floor_profile();
    let hybrid = hybrid_512_profile();
    let dense = dense_2m_profile();
    let (fp, fd) = PALW_RC_BASE0_CANONICAL;
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    vec![
        Row { name: "BASE-0 floor", declared_leaves: 7_708, per_draw: per_draw_ccu(&floor, fp, fd) },
        Row { name: "Qwen3.6 @512", declared_leaves: 20_717_968, per_draw: per_draw_ccu(&hybrid, hp, hd) },
        Row { name: "Qwen2.5 @2M ", declared_leaves: 27_002_967_184, per_draw: per_draw_ccu(&dense, dp, dd) },
    ]
}

// t12 runtime constants (recon-verified, re-stated here so the arithmetic is self-contained).
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const T12_WORKER_CARVE_PERMILLE: u64 = 720;
const T12_ESCROW_SOMPI: u64 = 320_084_650_080; // 444_562_014_000 * 720 / 1000
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;
const T12_FLOOR_DECLARED: u64 = 7_708;
const T12_FLOOR_CANONICAL: u64 = 21_657_728;
const T12_WINDOW_CHALLENGE: u64 = 1_200;
const T12_WINDOW_COURT: u64 = 3_000;
const T12_TARGET_TIME_MS: u64 = 120_000;
const T12_SLASH_VALUE_PER_PWU: u64 = 5;

// =================================================================================================
// A3-1 / A4-1  The unit of `credited_work` against PALW_EXECUTION_QUANTUM_V1
// =================================================================================================

/// `record_round_final` (palw_state_v2.rs:10975-10983) credits `palw_exposure_pwu_v2(...)` — the
/// RAW derived per-draw MAC-eq — and `palw_execution_quantum_count_v1` divides it by
/// `PALW_EXECUTION_QUANTUM_V1 = 100_000`, a constant whose own doc declares it is in "exposure pwu".
#[test]
fn a3_the_quantum_constant_is_divided_into_a_unit_it_was_not_calibrated_in() {
    println!("\n=== quantum unit: credited_work / PALW_EXECUTION_QUANTUM_V1 ({PALW_EXECUTION_QUANTUM_V1}) ===");
    println!("{:<14} {:>16} {:>22} {:>18} {:>14} {:>12} {:>14}", "class", "declared U1", "raw per-draw U2", "norm U3", "quanta(U2)", "quanta(U3)", "ratio U2/U3");
    for row in t12_rows() {
        let u3 = row.per_draw * (T12_FLOOR_DECLARED as u128) / (T12_FLOOR_CANONICAL as u128);
        let q_u2 = palw_execution_quantum_count_v1(row.per_draw, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        let q_u3 = palw_execution_quantum_count_v1(u3, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        let ratio = if u3 > 0 { row.per_draw as f64 / u3 as f64 } else { f64::NAN };
        println!("{:<14} {:>16} {:>22} {:>18} {:>14} {:>12} {:>14.1}", row.name, row.declared_leaves, row.per_draw, u3, q_u2, q_u3, ratio);
    }
    // The saturation is real: the dense row's raw credit overruns the u32 the count returns.
    let dense = t12_rows().pop().expect("three rows");
    let uncapped = dense.per_draw / PALW_EXECUTION_QUANTUM_V1 as u128;
    let capped = palw_execution_quantum_count_v1(dense.per_draw, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
    println!("\n  dense @2M uncapped quanta = {uncapped}, returned (u32-clamped) = {capped}");
    assert!(uncapped > u32::MAX as u128, "the dense row's raw credit overruns u32 in quanta");
    assert_eq!(capped, u32::MAX, "and the count clamps rather than refusing");
}

// =================================================================================================
// A3-2  What the mint COSTS when n is that large
// =================================================================================================

fn one_final(credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(2), 0)),
        operator_id: Hash64::from_u64_word(3),
        claim_id: Hash64::from_u64_word(4),
        execution_root: Hash64::from_u64_word(5),
        credit,
    }
}

/// The mint loop is `for index in 0..n` with a `BTreeSet` linear probe per ticket
/// (palw_execution_quanta_v1.rs:182-194, 200-219). The probe horizon is 2^16 rounds, so past
/// ~131k tickets from one mint the fallback walk at :214-217 restarts from `open_round + 2^16`
/// every time and the cost becomes quadratic.
#[test]
#[ignore = "MEASUREMENT, not a guard: times the PRE-FENCE mint up to 200,000 tickets (minutes at 100% CPU in a debug build); its one assertion is that the tickets it asked for were issued. The live guard on the bounded mint is audit_repro_01's repro_06."]
fn a3_the_mint_cost_is_quadratic_past_the_probe_horizon() {
    println!("\n=== palw_execution_mint_quanta_v1 wall time vs ticket count ===");
    let mut last: Option<(u64, f64)> = None;
    for n in [1_000u64, 4_000, 16_000, 64_000, 128_000, 200_000] {
        let credit = n * PALW_EXECUTION_QUANTUM_V1;
        let t0 = std::time::Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[one_final(credit)], Hash64::from_u64_word(9), PALW_EXECUTION_QUANTUM_V1 as u128, 0);
        let dt = t0.elapsed().as_secs_f64();
        let scaling = last.map(|(pn, pt)| (dt / pt) / ((n as f64) / (pn as f64))).unwrap_or(f64::NAN);
        println!("  n = {:>7}  issued = {:>7}  {:>9.3} s   (per-ticket cost vs previous: {:.2}x)", n, issued.len(), dt, scaling);
        assert_eq!(issued.len() as u64, n, "every ticket is materialized");
        last = Some((n, dt));
    }
    if let Some((n, dt)) = last {
        let per_ticket_us = dt / n as f64 * 1e6;
        println!("\n  at n = {n}: {per_ticket_us:.2} us/ticket (and rising quadratically)");
        // The reachable hybrid row mints 1,585,741; the clamped dense row mints 4,294,967,295.
        let hybrid_n = 1_585_741f64;
        println!("  linear-in-n lower bound for the hybrid @512 Final ({hybrid_n} tickets): {:.1} s", hybrid_n * per_ticket_us / 1e6);
        println!("  quadratic estimate (t ~ n^2): {:.1} s", dt * (hybrid_n / n as f64).powi(2));
        let dense_n = u32::MAX as f64;
        println!("  linear-in-n lower bound for the dense @2M Final ({dense_n} tickets): {:.0} s", dense_n * per_ticket_us / 1e6);
        println!("  memory at 1 ticket ~ {} B: hybrid {:.1} GiB, dense {:.1} GiB",
            std::mem::size_of::<kaspa_consensus_core::palw_execution_quanta_v1::PalwExecQuantumV1>(),
            hybrid_n * std::mem::size_of::<kaspa_consensus_core::palw_execution_quanta_v1::PalwExecQuantumV1>() as f64 / 1024f64.powi(3),
            dense_n * std::mem::size_of::<kaspa_consensus_core::palw_execution_quanta_v1::PalwExecQuantumV1>() as f64 / 1024f64.powi(3));
    }
}

// =================================================================================================
// A3-3  Q32: is 2^32 applied exactly once on each path?
// =================================================================================================

#[test]
fn a3_q32_scaling_is_applied_once_on_each_path() {
    println!("\n=== Q32 audit ===");
    println!("  PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 = {PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1} (2^32)");
    // integer vs Q32 at the same targets
    for (label, target) in [("MAX", u128::MAX), ("MAX/2", u128::MAX / 2), ("MAX/4", u128::MAX / 4), ("2*MAX/3", u128::MAX / 3 * 2)] {
        let int = palw_expected_attempts_v1(target);
        let q32 = palw_expected_attempts_q32_v1(target);
        println!("  target {label:<8} integer = {int:<20} q32 = {q32:<24} q32>>32 = {}", q32 >> 32);
        assert_eq!(q32 >> 32, int as u128, "the Q32 integer part must equal the consensus integer");
    }
    // The double product: class Q32 x network Q32 x draw. Each shift must happen once.
    let draw = 1_000_000u128;
    let class_q32 = palw_expected_attempts_q32_v1(u128::MAX / 2); // 2.0
    let net_q32 = palw_network_draws_q32_from_bits_v1(0x207f_ffff); // 2.0 at the difficulty floor
    let ccu = palw_attempted_ccu_v1(class_q32, net_q32, draw);
    println!("\n  draw = {draw}, class_q32 = {class_q32} (={}), net_q32 = {net_q32} (={})",
        class_q32 as f64 / 2f64.powi(32), net_q32 as f64 / 2f64.powi(32));
    println!("  palw_attempted_ccu_v1 = {ccu}  (expect 2 x 2 x draw = {})", 4 * draw);
    assert_eq!(ccu, 4 * draw, "both Q32 factors shift out exactly once");

    // The floor clamp is on the Q32 value, not the integer: a sub-one Q32 is lifted to one draw.
    let below_one = palw_attempted_compute_q32_per_claim_v1(1, draw);
    println!("  a Q32 of 1 (=2^-32 draws) prices at {below_one} — floored to one whole draw");
    assert_eq!(below_one, draw);

    // ADR-0131 vs ADR-0132 spellings on the same claim: the network factor is the only difference
    // when the class target is MAX. The recon brief measured 16_777_472x — that is the network
    // factor at the floor bits, not a scaling bug. Confirm it here.
    let adr131 = palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(u128::MAX), draw);
    let adr132 = palw_attempted_ccu_v1(palw_expected_attempts_q32_v1(u128::MAX), palw_network_draws_q32_from_bits_v1(0x1d00_ffff), draw);
    println!("\n  ADR-0131 palw_attempted_compute_per_claim_v1 = {adr131}");
    println!("  ADR-0132 palw_attempted_ccu_v1 (bits 0x1d00ffff) = {adr132}  ratio = {}", adr132 / adr131.max(1));
}

/// Boundary sweep of every threshold on the price path.
#[test]
fn a3_boundary_sweep_of_the_price_path() {
    println!("\n=== boundary sweep ===");
    println!("-- palw_expected_attempts_v1 --");
    for t in [0u128, 1, 2, u128::MAX / 2 - 1, u128::MAX / 2, u128::MAX / 2 + 1, u128::MAX - 1, u128::MAX] {
        println!("  target {t:>40} -> attempts {}", palw_expected_attempts_v1(t));
    }
    println!("-- palw_pwu_v1 saturation, dense @2M per-draw --");
    let dense_draw = t12_rows().pop().expect("rows").per_draw as u64;
    let sat_at = (u64::MAX as u128 / dense_draw as u128) + 1;
    println!("  per_draw = {dense_draw}; pwu saturates at expected_attempts >= {sat_at}");
    for a in [1u128, sat_at - 1, sat_at, sat_at + 1] {
        // Find a target giving exactly `a` expected attempts: target = 2^128/a - 1.
        let target = if a == 1 { u128::MAX } else { (u128::MAX / a).saturating_sub(1) };
        let att = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, dense_draw);
        println!("  attempts(target)={att:<8} pwu = {pwu:<22} saturated = {}", pwu == u64::MAX);
    }
    println!("-- palw_rate_priced_reward_v1 against the t12 escrow --");
    for ccu in [0u128, 1, 355_649_611_200 - 1, 355_649_611_200, 355_649_611_200 + 1, u128::MAX] {
        let r = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, ccu, T12_RATE_SOMPI_PER_GIGA as u128);
        println!("  ccu {ccu:>42} -> reward {r} sompi (escrow {T12_ESCROW_SOMPI})");
    }
    println!("-- palw_cap_utilization_permille_v1 --");
    for ccu in [0u128, 1, 284_519_688_960, 355_649_611_200, u128::MAX] {
        println!("  ccu {ccu:>42} -> cap {} permille", palw_cap_utilization_permille_v1(ccu, T12_RATE_SOMPI_PER_GIGA, T12_ESCROW_SOMPI));
    }
    println!("-- palw_panel_share_permille_v1 denominators --");
    println!("  c_p=0,c_v=0 -> {}", palw_panel_share_permille_v1(0, 0, 100, 100, 300));
    println!("  c_p=0,c_v=1 -> {}", palw_panel_share_permille_v1(0, 1, 100, 100, 300));
    println!("  c_p=u128::MAX,c_v=u128::MAX,alpha=100 -> {}", palw_panel_share_permille_v1(u128::MAX, u128::MAX, 100, 100, 300));
    println!("-- palw_priced_reward_u128_v1 unit=0 --");
    println!("  unit 0 pays the escrow whole: {}", palw_priced_reward_u128_v1(T12_ESCROW_SOMPI, 1, 0));
}

/// The saturating multiply inside `palw_panel_share_permille_v1` can invert the share: once
/// `c_p * 1000` saturates, the denominator stops growing while the numerator still can.
#[test]
fn a3_the_panel_share_denominator_saturates_before_the_numerator_does() {
    println!("\n=== panel share saturation ===");
    let huge = u128::MAX / 500; // c_p * 1000 saturates, c_v * alpha does not
    let s_small = palw_panel_share_permille_v1(huge, 1, 1_000, 100, 300);
    let s_big = palw_panel_share_permille_v1(huge, huge, 1_000, 100, 300);
    println!("  c_p = MAX/500, c_v = 1        -> {s_small} permille");
    println!("  c_p = MAX/500, c_v = MAX/500  -> {s_big} permille");
    // And the honest reading, in u128 without saturation, of the second case is 500 -> clamped 300.
    println!("  (both saturate the denominator at u128::MAX; the ratio the code reports is");
    println!("   numerator/u128::MAX, not alpha*C_V/(C_P + alpha*C_V))");
}

// =================================================================================================
// A4  Multi-counting: everything one accepted Final collects
// =================================================================================================

#[test]
fn a4_value_per_unit_of_real_work_summed_across_primitives() {
    println!("\n=== A4: what ONE Final collects on t12 ===");
    println!("  block subsidy            {T12_BLOCK_SUBSIDY_SOMPI} sompi/block");
    println!("  worker carve             {T12_WORKER_CARVE_PERMILLE} permille -> escrow {T12_ESCROW_SOMPI} sompi");
    println!("  rate                     {T12_RATE_SOMPI_PER_GIGA} sompi / 10^9 CCU (scale {PALW_LEDGER_RATE_SCALE_V1})");
    println!("  permit ceiling           {PALW_T12_PERMIT_FEE_CEILING_SOMPI} sompi/round");
    println!();
    println!("{:<14} {:>20} {:>16} {:>16} {:>18} {:>12}", "class", "per-draw CCU", "priced reward", "quanta", "permit value", "permit/pay");
    for row in t12_rows() {
        // (1) the escrow reward, priced at the rate, capped by the escrow.
        let ccu = palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, row.per_draw);
        let reward = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, ccu, T12_RATE_SOMPI_PER_GIGA as u128);
        // (2) the execution quanta the SAME Final mints, each redeemable for one algo-10 round.
        let quanta = palw_execution_quantum_count_v1(row.per_draw, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        let permit_value = (quanta as u128) * (PALW_T12_PERMIT_FEE_CEILING_SOMPI as u128);
        let ratio = permit_value as f64 / reward.max(1) as f64;
        println!("{:<14} {:>20} {:>16} {:>16} {:>18} {:>12.1}", row.name, row.per_draw, reward, quanta, permit_value, ratio);
    }
    println!("\n  (permit value is the DECLARED ceiling x the quanta minted; it is a SECOND payment");
    println!("   for the same execution, on top of the escrow reward, and it is not capped by the escrow)");
}

/// The seat lock the same claim forces, in the unit the runtime actually feeds it.
#[test]
fn a4_the_seat_lock_priced_from_the_runtime_unit() {
    println!("\n=== A4: seat lock vs posted collateral (516,429.79663480 MSK = 51_642_979_663_480 sompi) ===");
    let posted = 51_642_979_663_480u128;
    let rounds_per_daa = palw_rounds_per_daa_v1(T12_TARGET_TIME_MS);
    println!("  rounds per DAA = {rounds_per_daa}");
    for row in t12_rows() {
        // claim.pwu past palw_canonical_work: expected_attempts x per_draw, at the easiest target
        // (which is what a class at or above W0 gets: target = u128::MAX, attempts = 1).
        let claim_pwu = palw_pwu_v1(u128::MAX, row.per_draw.min(u64::MAX as u128) as u64);
        let weight_sompi = (claim_pwu as u128) * (T12_SLASH_VALUE_PER_PWU as u128);
        let quanta = palw_execution_quantum_count_v1(row.per_draw, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default()).saturating_add(1);
        let rights = palw_realizable_before_maturity_v1(
            quanta,
            T12_WINDOW_CHALLENGE,
            T12_WINDOW_COURT,
            T12_TARGET_TIME_MS,
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        // gain = reserved-slash + fork-weight + extra rights (palw_max_fraud_gain_v1's three terms,
        // here without the escrow term the payout half adds, so this is a LOWER bound).
        let gain = weight_sompi.saturating_add(rights);
        let required = palw_seat_lock_required_v2(gain, 3);
        println!("  {:<14} claim.pwu = {:<22} weight = {:>22} sompi", row.name, claim_pwu, weight_sompi);
        println!("  {:<14} quanta+1  = {:<22} rights = {:>22} sompi", "", quanta, rights);
        println!("  {:<14} seat lock required = {:>22} sompi = {:.2} MSK  ({:.2}x posted)",
            "", required, required as f64 / 1e8, required as f64 / posted as f64);
    }
}

/// `palw_realizable_before_maturity_v1`'s `min(quanta, rounds_in_gap)` mask, at the boundary.
#[test]
fn a4_the_realizable_rights_mask_and_its_boundary() {
    println!("\n=== A4: min(quanta, rounds) mask ===");
    let rounds_per_daa = palw_rounds_per_daa_v1(T12_TARGET_TIME_MS);
    let gap = T12_WINDOW_CHALLENGE + T12_WINDOW_COURT;
    let rounds_in_gap = gap * rounds_per_daa;
    println!("  window_challenge {T12_WINDOW_CHALLENGE} + window_court {T12_WINDOW_COURT} = {gap} DAA");
    println!("  rounds_per_daa {rounds_per_daa} -> rounds in the gap = {rounds_in_gap}");
    for q in [0u32, 1, rounds_in_gap as u32 - 1, rounds_in_gap as u32, rounds_in_gap as u32 + 1, u32::MAX] {
        let v = palw_realizable_before_maturity_v1(q, T12_WINDOW_CHALLENGE, T12_WINDOW_COURT, T12_TARGET_TIME_MS, PALW_T12_PERMIT_FEE_CEILING_SOMPI);
        println!("  quanta {q:>12} -> realizable {v:>22} sompi  (masked = {})", q as u64 > rounds_in_gap);
    }
    println!("\n  NOTE: the mask hides the unit error in the LOCK, but the MINT at");
    println!("  palw_execution_quanta_v1.rs:181 has no such mask — it materializes every ticket.");
}

/// The network factor multiplies every class's attempted CCU. On a chain with no bits-priced lane
/// the bits a Final's carrying block holds are still read.
#[test]
fn a4_the_network_bits_factor_on_a_heartbeat_only_chain() {
    println!("\n=== A4: palw_network_draws_q32_from_bits_v1 across the compact range ===");
    for bits in [0u32, 0x207f_ffff, 0x1f00_ffff, 0x1d00_ffff, 0x1b00_ffff, 0x1800_0001] {
        let q32 = palw_network_draws_q32_from_bits_v1(bits);
        let draws = q32 as f64 / 2f64.powi(32);
        println!("  bits 0x{bits:08x} -> q32 {q32:>28}  = {draws:.4} network draws");
    }
    let draw = 1_000_000u128;
    let easy = palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_network_draws_q32_from_bits_v1(0x207f_ffff), draw);
    let hard = palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_network_draws_q32_from_bits_v1(0x1b00_ffff), draw);
    println!("\n  same draw, floor bits -> {easy} CCU; harder bits -> {hard} CCU  ({}x)", hard / easy.max(1));
    println!("  the CCU a claim is PAID for rises with the network difficulty of the block that");
    println!("  merely CARRIED it, with no additional compute run by the producer.");
    assert!(hard > easy);
}

/// `palw_network_expected_attempts_q32_v1` directly, at the mantissa boundaries.
#[test]
fn a3_network_q32_boundaries() {
    println!("\n=== network Q32 at compact-bits boundaries ===");
    // NOTE: bits with an exponent byte above ~0x21 panic inside Uint256::from_compact_target_bits
    // (math/src/lib.rs:90, `<< expt` with expt = 8*(e-3)). Header validation constrains `bits`, so the
    // sweep stays inside the reachable range.
    for bits in [0x0300_0000u32, 0x0300_0001, 0x2100_0000, 0x207f_ffff, 0x1d00_ffff, 0x0080_0000] {
        let q32 = palw_network_expected_attempts_q32_v1(bits);
        println!("  bits 0x{bits:08x} -> q32 {q32:>42}  saturated = {}", q32 == u128::MAX);
    }
    // A saturated network factor makes attempted_ccu saturate too.
    let sat = palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, u128::MAX, 1_000_000);
    println!("\n  a saturated network Q32 prices one draw at {sat} CCU");
    println!("  reward at the t12 rate: {} sompi (escrow {T12_ESCROW_SOMPI})",
        palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, sat, T12_RATE_SOMPI_PER_GIGA as u128));
}
