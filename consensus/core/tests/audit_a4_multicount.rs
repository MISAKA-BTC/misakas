//! **AUDIT LANE A4 — multi-counting and per-X confusion on testnet-12.**
//!
//! Read-only measurement. Every number printed here is produced from `Params::from(testnet-12)`
//! and the pub derivation functions the fold itself calls. Nothing is asserted from a doc comment.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a4_multicount -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, PalwEconomicPayoutV1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempted_compute_per_claim_v1, palw_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{
    PalwEconomicPayoutFoldV1, palw_attempted_ccu_v1, palw_cap_utilization_permille_v1, palw_claim_economics_snapshot_v1,
    palw_network_draws_q32_from_bits_v1,
};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_maturity_daa_v1, palw_permit_value_sompi_v1,
    palw_realizable_before_maturity_v1, palw_rounds_per_daa_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelWorkV1, palw_genesis_model_works_v1};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, palw_max_exposure_pwu_of_rule_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---------------------------------------------------------------------------------------------
// Shared fixture: the shipped t12 card, its three class rows, and each row's registry work.
// ---------------------------------------------------------------------------------------------

struct Row {
    name: &'static str,
    class_id: Hash64,
    /// U1: `pwu_per_inference`, declared leaves per draw, out of the genesis registration.
    declared_leaves: u64,
    /// The genesis `initial_target` the registration carries.
    initial_target: u128,
    slash_value_per_pwu: u64,
    /// The registry row's work, from the SAME function `palw_known_model_works_v1` uses for a
    /// genesis class with an admission carriage.
    work: Option<PalwModelWorkV1>,
}

impl Row {
    /// U2: the derived per-DRAW compute the fold reads as `canonical_per_draw`, saturated into the
    /// `u64` the exposure/credit path uses (`palw_state_v2.rs:9848`, `palw_admission_v2.rs:596`).
    fn u2_draw_ccu(&self) -> u64 {
        self.work.map(|w| w.economic_ccu_per_claim.min(u64::MAX as u128) as u64).unwrap_or(0)
    }
}

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle_base(params: &Params) -> Hash64 {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    bundle.base_class_id
}

fn bundle_state(params: &Params) -> &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2 {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    &bundle.state
}

fn rows(params: &Params) -> Vec<Row> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let works = palw_genesis_model_works_v1(&bundle.genesis_objects);
    let base = bundle_base(params);
    let mut out = Vec::new();
    for object in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, slash_value_per_pwu, .. } = object else {
            continue;
        };
        let name = if *class_id == base {
            "BASE-0 floor"
        } else if palw_max_exposure_pwu_of_rule_v1(pwu_rule) > 1_000_000_000 {
            "Qwen2.5 dense @2M"
        } else {
            "Qwen3.6 hybrid @512"
        };
        out.push(Row {
            name,
            class_id: *class_id,
            declared_leaves: palw_max_exposure_pwu_of_rule_v1(pwu_rule),
            initial_target: *initial_target,
            slash_value_per_pwu: *slash_value_per_pwu,
            work: works.get(class_id).copied(),
        });
    }
    out
}

/// The floor's basis, exactly as `palw_exposure_basis_v2` builds it: the floor's declared leaves
/// over the floor's derived draw.
fn basis(rows: &[Row], base: &Hash64) -> (u64, u64) {
    let floor = rows.iter().find(|r| r.class_id == *base).expect("the floor row");
    // The floor carries no admission carriage in the bundle, so its work comes from the build's
    // canonical class table. Derive it here from the same pub builder the table uses.
    let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .expect("floor profile");
    let (p, d) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, p, d);
    let work = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&profile, &canonical)
        .expect("floor work derives");
    (floor.declared_leaves, work.economic_ccu_per_claim.min(u64::MAX as u128) as u64)
}

/// `palw_exposure_pwu_v3`'s expression, byte for byte.
fn u3_normalised(u2: u64, base_declared: u64, base_canonical: u64) -> u64 {
    if base_canonical == 0 || base_declared == 0 {
        return u2;
    }
    (((u2 as u128) * (base_declared as u128)) / (base_canonical as u128)).min(u64::MAX as u128) as u64
}

fn floor_u2(rows_: &[Row], params: &Params) -> u64 {
    let (_, bc) = basis(rows_, &bundle_base(params));
    bc
}

// =============================================================================================
// Q4 / Q2 — the unit of `credited_work` against the unit of `PALW_EXECUTION_QUANTUM_V1`.
// =============================================================================================

#[test]
fn q4_the_execution_quantum_divides_a_mac_eq_credit_by_a_leaf_sized_constant() {
    let params = t12();
    let rs = rows(&params);
    let (bd, bc) = basis(&rs, &bundle_base(&params));

    println!("\n=== t12 fences that decide the unit ===");
    println!("  palw_canonical_work    {:?}", params.palw_canonical_work);
    println!("  palw_execution_quanta  {:?}", params.palw_execution_quanta);
    println!("  palw_model_registry    {}", params.palw_model_registry.is_some());
    println!("  palw_execution_lane    span_daa={:?}", params.palw_execution_lane.map(|l| l.schedule_span_daa));
    assert!(params.palw_canonical_work.is_some(), "t12 arms canonical work");
    assert!(params.palw_execution_quanta.is_some(), "t12 arms execution quanta");

    println!("\n=== the floor's basis (palw_exposure_basis_v2) ===");
    println!("  base_declared  (U1) = {bd} leaves");
    println!("  base_canonical (U2) = {bc} MAC-eq");
    println!("  U2/U1 for the floor = {:.2}x", bc as f64 / bd as f64);

    println!("\n=== PALW_EXECUTION_QUANTUM_V1 = {PALW_EXECUTION_QUANTUM_V1} ===");
    println!("  its doc: \"in the same units PalwExecFinalV1::credit is stored in (exposure pwu, capped).");
    println!("            A ~1.6M-pwu QWEN25-scale job is about 16 tickets\"  <- 1_589_424/100_000 = 15.9, DECLARED LEAVES");
    println!(
        "\n  {:<22} {:>16} {:>22} {:>20} {:>12} {:>12} {:>14}",
        "class", "U1 declared", "U2 derived MAC-eq", "U3 normalised", "q@U1", "q@U3", "q@U2 (LIVE)"
    );
    let seed = Hash64::default();
    let fid = Hash64::default();
    for r in &rs {
        let u1 = r.declared_leaves;
        let u2 = r.u2_draw_ccu();
        let u2 = if u2 == 0 { floor_u2(&rs, &params) } else { u2 };
        let u3 = u3_normalised(u2, bd, bc);
        let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
        let q1 = palw_execution_quantum_count_v1(u128::from(u1), q, seed, fid);
        let q3 = palw_execution_quantum_count_v1(u128::from(u3), q, seed, fid);
        let q2 = palw_execution_quantum_count_v1(u128::from(u2), q, seed, fid);
        println!("  {:<22} {u1:>16} {u2:>22} {u3:>20} {q1:>12} {q3:>12} {q2:>14}", r.name);
    }

    println!("\n=== ratio the permit count is off by (live credit / documented credit) ===");
    for r in &rs {
        let u1 = r.declared_leaves;
        let u2 = r.u2_draw_ccu();
        let u2 = if u2 == 0 { floor_u2(&rs, &params) } else { u2 };
        let u3 = u3_normalised(u2, bd, bc);
        println!(
            "  {:<22} U2/U1 = {:>14.2}x   U2/U3 = {:>10.2}x   U3/U1 = {:>10.4}x",
            r.name,
            u2 as f64 / u1 as f64,
            u2 as f64 / u3 as f64,
            u3 as f64 / u1 as f64
        );
    }

    println!("\n=== u32 saturation of palw_execution_quantum_count_v1 ===");
    let dense = rs.iter().max_by_key(|r| r.u2_draw_ccu()).expect("a row");
    let raw = u128::from(dense.u2_draw_ccu()) / u128::from(PALW_EXECUTION_QUANTUM_V1);
    println!("  {} credit {} / {PALW_EXECUTION_QUANTUM_V1} = {raw} unclamped", dense.name, dense.u2_draw_ccu());
    println!("  returned as u32:                  {}", palw_execution_quantum_count_v1(u128::from(dense.u2_draw_ccu()), u128::from(PALW_EXECUTION_QUANTUM_V1), seed, fid));
    assert!(raw > u32::MAX as u128, "the dense row's live credit overflows the u32 the count returns");
}

// =============================================================================================
// Q4b — what the mint actually costs. Measured, extrapolated, never assumed.
// =============================================================================================

fn bond(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn final_of(credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: bond(1),
        operator_id: Hash64::from_u64_word(2),
        claim_id: Hash64::from_u64_word(3),
        execution_root: Hash64::from_u64_word(4),
        credit,
    }
}

#[test]
#[ignore = "MEASUREMENT, not a guard: times the PRE-FENCE mint up to 200,000 tickets and asserts nothing (minutes at 100% CPU in a debug build). The live guard on the bounded mint is audit_repro_01's repro_06."]
fn q4b_the_mint_cost_scales_quadratically_in_the_ticket_count() {
    println!("\n=== palw_execution_mint_quanta_v1: measured wall time vs ticket count ===");
    println!("  (assign_round probes a 2^16 horizon then linear-walks; `taken` is per-mint)");
    let mut last: Option<(u64, f64)> = None;
    for n in [1_000u64, 4_000, 16_000, 64_000, 96_000, 128_000, 200_000] {
        let credit = n * PALW_EXECUTION_QUANTUM_V1;
        let t0 = std::time::Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[final_of(credit)], Hash64::from_u64_word(9), u128::from(PALW_EXECUTION_QUANTUM_V1), 0);
        let dt = t0.elapsed().as_secs_f64();
        let scaling = last.map(|(pn, pt)| (dt / pt) / ((n as f64 / pn as f64))).unwrap_or(f64::NAN);
        println!("  n={n:>8} tickets  minted={:>8}  {:>9.3} s   (dt ratio / n ratio = {scaling:.2})", issued.len(), dt);
        last = Some((n, dt));
    }
    let (n, t) = last.expect("measured");
    // Quadratic extrapolation from the last measured point.
    for target in [1_585_741u64, 4_294_967_295] {
        let est = t * (target as f64 / n as f64).powi(2);
        println!("  extrapolated to {target:>12} tickets: {:.0} s = {:.2} h (quadratic)", est, est / 3600.0);
    }
    println!("  state bytes at ~300 B / PalwExecQuantumV1:");
    for target in [216u64, 1_585_741, 4_294_967_295] {
        println!("    {target:>12} tickets -> {:>12.2} MiB", (target as f64 * 300.0) / (1024.0 * 1024.0));
    }
}

// =============================================================================================
// Q1 — every primitive one accepted inference mints, and what each reads.
// =============================================================================================

#[test]
fn q1_every_primitive_one_accepted_inference_mints() {
    let params = t12();
    let rs = rows(&params);
    let (bd, bc) = basis(&rs, &bundle_base(&params));
    let carve = params.palw_overlay_carve.expect("t12 arms the carve");
    // The escrow a t12 claim really carries: the block subsidy through the worker carve.
    const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
    let escrow = T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve.worker_carve_permille as u64;
    println!("\n=== the escrow one Final releases ===");
    println!("  block subsidy            {T12_BLOCK_SUBSIDY_SOMPI} sompi");
    println!("  worker_carve_permille    {}", carve.worker_carve_permille);
    println!("  escrow per claim         {escrow} sompi = {:.2} MSK", escrow as f64 / 1e8);

    let payout: PalwEconomicPayoutV1 = params.palw_economic_payout.expect("t12 arms the payout");
    println!("\n=== PalwEconomicPayoutV1 on t12 ===");
    println!("  rate_sompi_per_giga          {}", payout.rate_sompi_per_giga);
    println!("  panel_share min/max permille {}/{}", payout.panel_share_min_permille, payout.panel_share_max_permille);
    println!("  cap_utilization_max_permille {}", payout.cap_utilization_max_permille);

    let fold = PalwEconomicPayoutFoldV1 {
        rate_sompi_per_giga: payout.rate_sompi_per_giga,
        panel_share_alpha_permille: payout.panel_share_alpha_permille,
        panel_share_min_permille: payout.panel_share_min_permille,
        panel_share_max_permille: payout.panel_share_max_permille,
        cap_utilization_max_permille: payout.cap_utilization_max_permille,
        // t12 has no bits-priced lane: every block is a heartbeat/attempt, so the factor is what
        // `palw_network_draws_q32_from_bits_v1` gives for whatever `bits` the block carried.
        block_bits: 0,
    };
    println!("  network draws Q32 at bits=0  {} (= {:.4} draws)", palw_network_draws_q32_from_bits_v1(0), palw_network_draws_q32_from_bits_v1(0) as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64);

    let maturity = palw_exec_quantum_maturity_daa_v1(bundle_state(&params).window_challenge(), bundle_state(&params).window_court());
    let rounds_per_daa = palw_rounds_per_daa_v1(params.target_time_per_block_history().after());
    let gap = bundle_state(&params).window_court().saturating_sub(maturity);
    println!("\n=== the permit window ===");
    println!("  window_challenge {} DAA, window_court {} DAA", bundle_state(&params).window_challenge(), bundle_state(&params).window_court());
    println!("  maturity {maturity} DAA, gap {gap} DAA, rounds/DAA {rounds_per_daa}, rounds in gap {}", gap * rounds_per_daa);
    println!("  PALW_T12_PERMIT_FEE_CEILING_SOMPI {PALW_T12_PERMIT_FEE_CEILING_SOMPI} ({:.4} MSK)", PALW_T12_PERMIT_FEE_CEILING_SOMPI as f64 / 1e8);

    println!("\n=== per class: every primitive one Final mints ===");
    for r in &rs {
        let u2 = r.u2_draw_ccu();
        let u2 = if u2 == 0 { floor_u2(&rs, &params) } else { u2 };
        let u3 = u3_normalised(u2, bd, bc);
        let attempts = palw_expected_attempts_v1(r.initial_target);
        let attempts_q32 = palw_expected_attempts_q32_v1(r.initial_target);
        // claim.pwu past the fence = palw_pwu_v1(target, derived draw)  (palw_admission_v2.rs:345)
        let claim_pwu = palw_pwu_v1(r.initial_target, u2);
        let work = r.work.unwrap_or(PalwModelWorkV1 {
            verification_ccu: u2 as u128,
            economic_ccu_per_claim: u2 as u128,
            ..Default::default()
        });
        let snap = palw_claim_economics_snapshot_v1(&fold, &work, 5, r.initial_target, 0);
        let attempted = snap.attempted_ccu();
        let priced = snap.priced_reward(escrow);
        let cap = palw_cap_utilization_permille_v1(attempted, payout.rate_sompi_per_giga, escrow);
        let quanta = palw_execution_quantum_count_v1(u128::from(u2), u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default());
        let realizable = palw_realizable_before_maturity_v1(
            quanta.saturating_add(1),
            bundle_state(&params).window_challenge(),
            bundle_state(&params).window_court(),
            params.target_time_per_block_history().after(),
            palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI),
        );
        let fork_weight_sompi = (claim_pwu as u128).saturating_mul(r.slash_value_per_pwu as u128);
        let reserved = (u3 as u128).saturating_mul(r.slash_value_per_pwu as u128);
        let gain = fork_weight_sompi.saturating_add(priced as u128).saturating_add(realizable);
        let seat_lock = palw_seat_lock_required_v2(gain, 3);

        println!("\n  --- {} ({:.16}…) ---", r.name, r.class_id.to_string());
        println!("    initial_target             {}", r.initial_target);
        println!("    expected_attempts (int)    {attempts}   Q32 {:.6}", attempts_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64);
        println!("    U1 declared leaves/draw    {}", r.declared_leaves);
        println!("    U2 derived MAC-eq/draw     {u2}");
        println!("    U3 exposure pwu            {u3}");
        println!("    claim.pwu (= attempts*U2)  {claim_pwu}{}", if claim_pwu == u64::MAX { "   <-- SATURATED at u64::MAX" } else { "" });
        println!("    [P1] escrow priced reward  {priced} sompi = {:.2} MSK  (cap util {cap} permille)", priced as f64 / 1e8);
        println!("         attempted_ccu (C_P)   {attempted}");
        println!("         panel share permille  {}", snap.panel_share_permille);
        println!("    [P2] fork weight           claim.pwu = {claim_pwu} (MAC-eq) -> x slash {} = {fork_weight_sompi} sompi = {:.2} MSK", r.slash_value_per_pwu, fork_weight_sompi as f64 / 1e8);
        println!("    [P3] exposure reserved     {reserved} sompi = {:.2} MSK (released at Final)", reserved as f64 / 1e8);
        println!("    [P4] execution quanta      {quanta} permits  (credit = U2, quantum = {PALW_EXECUTION_QUANTUM_V1})");
        println!("         realizable value      {realizable} sompi = {:.2} MSK", realizable as f64 / 1e8);
        println!("    == max_fraud_gain          {gain} sompi = {:.2} MSK", gain as f64 / 1e8);
        println!("    == seat lock required      {seat_lock} sompi = {:.2} MSK  (per Valid seat)", seat_lock as f64 / 1e8);
        println!("    real compute of one claim  {} MAC-eq (= attempts x U2, unsaturated)", (attempts as u128).saturating_mul(u2 as u128));
    }

    println!("\n=== posted genesis collateral per seat ===");
    println!("  PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI = 51_642_979_663_480 = {:.2} MSK", 51_642_979_663_480f64 / 1e8);
}

// =============================================================================================
// Q2 — per-draw used as per-claim, and the two incompatible spellings of attempted compute.
// =============================================================================================

#[test]
fn q2_per_draw_versus_per_claim_and_the_two_attempted_spellings() {
    let params = t12();
    let rs = rows(&params);
    println!("\n=== ADR-0131 vs ADR-0132 spellings of 'attempted compute' ===");
    for r in &rs {
        let u2 = r.u2_draw_ccu();
        let u2 = if u2 == 0 { floor_u2(&rs, &params) } else { u2 };
        let attempts = palw_expected_attempts_v1(r.initial_target);
        let q32 = palw_expected_attempts_q32_v1(r.initial_target);
        let a = palw_attempted_compute_per_claim_v1(attempts, u128::from(u2));
        let b = palw_attempted_ccu_v1(q32, palw_network_draws_q32_from_bits_v1(0), u128::from(u2));
        println!(
            "  {:<22} per_claim_v1={a:<26} attempted_ccu_v1={b:<26} ratio={:.6}",
            r.name,
            b as f64 / a.max(1) as f64
        );
    }

    println!("\n=== the field NAMED per-claim that holds a per-DRAW value ===");
    println!("  PalwModelWorkV1::economic_ccu_per_claim  <- palw_attempt_economic_compute_v1(.., prefill_draw=true)");
    println!("  = ONE DRAW. `palw_claim_economics_snapshot_v1` then multiplies it by");
    println!("    expected_attempts_q32 x network_q32 to recover the per-CLAIM figure.");
    println!("  But record_round_final credits `palw_exposure_pwu_v2` = the SAME per-DRAW value,");
    println!("  and divides it by a per-CLAIM-calibrated quantum. Both readings live in one build.");

    println!("\n=== fee x rounds x permits ===");
    let maturity = palw_exec_quantum_maturity_daa_v1(bundle_state(&params).window_challenge(), bundle_state(&params).window_court());
    let gap = bundle_state(&params).window_court().saturating_sub(maturity);
    let rpd = palw_rounds_per_daa_v1(params.target_time_per_block_history().after());
    println!("  rounds in the conviction gap = {gap} DAA x {rpd} rounds/DAA = {}", gap * rpd);
    println!("  palw_realizable_before_maturity_v1 = min(quanta, rounds) x permit_value");
    println!("  -> the `rounds` term MASKS every quanta count above {}", gap * rpd);
    for q in [216u32, 1_585_741, u32::MAX] {
        let v = palw_realizable_before_maturity_v1(
            q,
            bundle_state(&params).window_challenge(),
            bundle_state(&params).window_court(),
            params.target_time_per_block_history().after(),
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        println!("    quanta={q:>12} -> realizable {v} sompi = {:.2} MSK", v as f64 / 1e8);
    }
}
