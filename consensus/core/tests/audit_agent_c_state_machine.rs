//! AUDIT (agent C) — measurements only. Read-only probe of testnet-12 runtime values.

use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_maturity_daa_v1, palw_realizable_before_maturity_v1,
    palw_rounds_per_daa_v1,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_matured_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, palw_execution_span_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_v2::palw_seat_maturity_floor_v1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensus_core::Hash64;

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle(p: &Params) -> &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b,
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

#[test]
fn audit_c_windows_and_lane() {
    let p = t12();
    let b = bundle(&p);
    let s = &b.state;
    println!("--- t12 state windows ---");
    println!("window_bind         = {}", s.window_bind());
    println!("window_receipt      = {}", s.window_receipt());
    println!("window_challenge    = {}", s.window_challenge());
    println!("window_court        = {}", s.window_court());
    println!("claim_retirement    = {}", s.claim_retirement_daa());
    println!("epoch_length        = {}", s.epoch_length());
    println!("min_collateral      = {}", s.min_collateral_sompi());
    println!("panel seat_count    = {}", b.panel.seat_count());
    println!("panel quorum        = {}", b.panel.quorum());
    println!("panel anchor_delay  = {}", b.panel.anchor_delay());
    println!("cadence ms          = {}", b.cadence_target_time_per_block_ms);
    println!("target_time_per_block = {}", p.target_time_per_block);
    println!("lane                = {:?}", p.palw_execution_lane);
    println!("economic_safety     = {:?}", p.palw_economic_safety);
    println!("bond_maturity       = {:?}", p.palw_bond_maturity);
}

#[test]
fn audit_c_quantum_maturity_versus_schedule_retention() {
    let p = t12();
    let b = bundle(&p);
    let s = &b.state;
    let lane = p.palw_execution_lane.expect("t12 opens the lane");
    let span_daa = lane.schedule_span_daa;

    let maturity_daa = palw_exec_quantum_maturity_daa_v1(s.window_challenge(), s.window_court());
    let rounds_per_daa = palw_rounds_per_daa_v1(p.target_time_per_block);
    let maturity_rounds = maturity_daa * rounds_per_daa;
    println!("maturity_daa        = {maturity_daa}");
    println!("rounds_per_daa      = {rounds_per_daa}");
    println!("maturity_rounds     = {maturity_rounds}");
    println!("span_daa            = {span_daa}");
    // rotate_round_lane keeps round_schedules with key >= span_now - 1: two spans.
    let retention_daa = 2 * span_daa;
    let retention_rounds = retention_daa * rounds_per_daa;
    println!("schedule retention  = {retention_daa} DAA = {retention_rounds} rounds");
    println!("ratio maturity/retention = {}", maturity_rounds as f64 / retention_rounds as f64);

    // Mint one Final's quanta the way rotate_round_lane does and read the EARLIEST round.
    let outpoint = TransactionOutpoint::default();
    let f = PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: PalwBondKeyV2(outpoint),
        operator_id: Hash64::from_u64_word(2),
        claim_id: Hash64::from_u64_word(3),
        execution_root: Hash64::from_u64_word(4),
        credit: 21_657_728, // BASE-0 floor: derived MAC-eq per draw, the unit record_round_final uses
        accepted_blue_score: 0,
    };
    let open_round = 1_000u64; // the wall-clock round the span opened at
    let issued = palw_execution_mint_quanta_matured_v1(
        &[f],
        Hash64::from_u64_word(0xABCD),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        open_round,
        maturity_rounds,
        &std::collections::BTreeSet::new(),
    );
    println!("quanta minted       = {}", issued.len());
    let earliest = issued.iter().map(|q| q.scheduled_round).min().unwrap();
    println!("open_round          = {open_round}");
    println!("earliest scheduled  = {earliest}");
    println!("delta rounds        = {}", earliest - open_round);
    println!("last retained round = {}", open_round + retention_rounds);
    assert!(
        earliest > open_round + retention_rounds,
        "MEASURED: the earliest ticket {earliest} lands after the schedule row is deleted at round {}",
        open_round + retention_rounds
    );
}

#[test]
fn audit_c_quantum_counts_per_t12_class() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = Hash64::from_u64_word(9);
    let cid = Hash64::from_u64_word(11);
    for (name, mac_eq, declared) in [
        ("BASE-0 floor", 21_657_728u128, 7_708u128),
        ("Qwen3.6@512", 158_574_197_994u128, 20_717_968u128),
        ("Qwen2.5@2M", 3_357_281_757_221_376u128, 27_002_967_184u128),
    ] {
        println!(
            "{name}: quanta(raw MAC-eq credit)={} quanta(declared leaves)={}",
            palw_execution_quantum_count_v1(mac_eq, q, seed, cid),
            palw_execution_quantum_count_v1(declared, q, seed, cid)
        );
    }
}

#[test]
fn audit_c_realizable_residual() {
    let p = t12();
    let b = bundle(&p);
    let s = &b.state;
    for (name, quanta) in [("BASE-0", 216u32), ("Qwen3.6@512", 1_585_741u32), ("Qwen2.5@2M", u32::MAX)] {
        let r = palw_realizable_before_maturity_v1(
            quanta,
            s.window_challenge(),
            s.window_court(),
            p.target_time_per_block,
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        println!("{name}: quanta={quanta} realizable={r} sompi = {} MSK", r as f64 / 1e8);
    }
}

#[test]
fn audit_c_bond_maturity_at_999_1000_1001() {
    let p = t12();
    let m = p.palw_bond_maturity.expect("t12 schedules D1 at 1000");
    for daa in [998u64, 999, 1000, 1001, 1500, 2000] {
        let active = m.activation.is_active(daa);
        let window = if active { Some(m.window_daa) } else { None };
        println!("anchor_daa={daa} active={active} seat_maturity_floor={:?}", palw_seat_maturity_floor_v1(daa, window));
    }
}

#[test]
fn audit_c_genesis_bond_operators() {
    let p = t12();
    let b = bundle(&p);
    let mut bonds = 0usize;
    let mut keys: Vec<Vec<u8>> = Vec::new();
    for o in &b.genesis_objects {
        if let PalwConsensusObjectV2::BondRegistered { operator_pubkey, collateral, .. } = o {
            bonds += 1;
            println!("bond {bonds}: collateral={collateral} pubkey_len={}", operator_pubkey.len());
            keys.push(operator_pubkey.clone());
        }
    }
    keys.sort();
    let distinct = { let mut k = keys.clone(); k.dedup(); k.len() };
    println!("genesis bonds={bonds} distinct operator pubkeys={distinct}");
}

#[test]
fn audit_c_span_arithmetic() {
    let p = t12();
    let lane = p.palw_execution_lane.unwrap();
    for daa in [0u64, 1, 2, 1000] {
        println!("daa={daa} span={}", palw_execution_span_v1(daa, lane.schedule_span_daa));
    }
}


/// The colluding-quorum inequality, re-derived from the t12 genesis card at runtime.
#[test]
fn audit_c_colluding_quorum_inequality_from_the_card() {
    use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
    use kaspa_consensus_core::palw_economic_safety_v1::palw_seat_lock_required_v2;
    use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
    use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
    use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
    use kaspa_consensus_core::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};
    use kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2;

    let p = t12();
    let b = bundle(&p);
    let s = &b.state;
    let _lane = p.palw_execution_lane.unwrap();
    let posted: u128 = 51_642_979_663_480; // PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI
    // The escrow a t12 claim really carries: the chain's own block subsidy through the carve.
    let subsidy: u64 = 444_562_014_000;
    let escrow = palw_reward_carve_v2(subsidy, &PalwRewardParamsV2::new(s.worker_carve_permille()).unwrap()).worker;
    println!("worker_carve_permille(bundle)={} escrow_from_real_subsidy={escrow}", s.worker_carve_permille());
    let carve_fence = p.palw_overlay_carve.map(|c| c.worker_carve_permille).unwrap();
    let escrow_fence = palw_reward_carve_v2(subsidy, &PalwRewardParamsV2::new(carve_fence).unwrap()).worker;
    println!("worker_carve_permille(fence)={carve_fence} escrow={escrow_fence} = {} MSK", escrow_fence as f64 / 1e8);

    for o in &b.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered {
            class_id, slash_value_per_pwu, pwu_rule, initial_target, admission, ..
        } = o
        else {
            continue;
        };
        let declared = match pwu_rule {
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
            _ => 0,
        };
        let attempts = palw_expected_attempts_v1(*initial_target);
        // Derived per draw, from the class's own carriage where it has one.
        let per_draw: Option<u128> = admission.as_ref().and_then(|c| {
            let d = PalwCanonicalClassDescriptorV1::of(&c.profile, Hash64::default()).ok()?;
            Some(palw_canonical_draw_work_v1(&d, &c.canonical, true).ok()?.provisional_scalar_v1())
        });
        let pwu = match per_draw {
            Some(w) => palw_pwu_v1(*initial_target, w.min(u64::MAX as u128) as u64),
            None => palw_pwu_v1(*initial_target, declared),
        };
        let quanta = kaspa_consensus_core::palw_execution_quanta_v1::palw_execution_quantum_count_v1(
            u128::from(pwu.min(u64::MAX)),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            Hash64::default(),
            Hash64::default(),
        )
        .saturating_add(1);
        let extra = palw_realizable_before_maturity_v1(
            quanta,
            s.window_challenge(),
            s.window_court(),
            p.target_time_per_block,
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        let facts = PalwClaimFraudFactsV1 {
            reserved: 0,
            escrowed_reward: escrow_fence,
            exposure_pwu: pwu,
            slash_value_per_pwu: *slash_value_per_pwu,
            extra_economic_rights_sompi: extra,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let lock = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        println!("--- class {class_id}");
        println!("  declared leaves      = {declared}");
        println!("  initial_target       = {initial_target}");
        println!("  expected_attempts    = {attempts}");
        println!("  derived per draw     = {per_draw:?}");
        println!("  claim.pwu            = {pwu}");
        println!("  quanta priced        = {quanta}");
        println!("  extra rights (sompi) = {extra}");
        println!("  max_fraud_gain       = {gain} sompi = {} MSK", gain as f64 / 1e8);
        println!("  seat lock required   = {lock} sompi = {} MSK", lock as f64 / 1e8);
        println!("  posted per bond      = {posted} sompi = {} MSK", posted as f64 / 1e8);
        println!("  shortfall factor     = {:.2}x", lock as f64 / posted as f64);
        println!("  affordable           = {}", posted >= lock);
    }
}

/// The exposure ceiling the B-4 residual comment names as its bound, measured per t12 class.
#[test]
fn audit_c_exposure_ceiling_is_not_a_bound_on_replay() {
    use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
    use kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2;

    let p = t12();
    let b = bundle(&p);
    let posted: u128 = 51_642_979_663_480;
    let ratio_permille: u128 = b.admission.max_exposure_ratio_permille() as u128;
    let ceiling = posted * ratio_permille / 1000;
    println!("max_exposure_ratio_permille = {ratio_permille}");
    println!("ceiling per bond            = {ceiling} sompi");

    // The floor's basis: declared leaves over derived MAC-eq per draw (palw_exposure_pwu_v3).
    let mut base_declared = 0u128;
    let mut base_canonical = 0u128;
    let mut rows: Vec<(Hash64, u128, Option<u128>, u64)> = Vec::new();
    for o in &b.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, slash_value_per_pwu, pwu_rule, admission, .. } = o else {
            continue;
        };
        let declared = match pwu_rule {
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference as u128,
            _ => 0,
        };
        let per_draw: Option<u128> = admission.as_ref().and_then(|c| {
            let d = PalwCanonicalClassDescriptorV1::of(&c.profile, Hash64::default()).ok()?;
            Some(palw_canonical_draw_work_v1(&d, &c.canonical, true).ok()?.provisional_scalar_v1())
        });
        if *class_id == b.base_class_id {
            base_declared = declared;
            // The floor carries no carriage; its derived draw is the known-work table's 21,657,728.
            base_canonical = 21_657_728;
        }
        rows.push((*class_id, declared, per_draw, *slash_value_per_pwu));
    }
    println!("basis: base_declared={base_declared} base_canonical={base_canonical}");
    for (class_id, declared, per_draw, slash) in rows {
        let exposure_pwu = match per_draw {
            Some(w) => w * base_declared / base_canonical,
            None => base_declared,
        };
        let reserved = exposure_pwu * slash as u128;
        println!("class {}", &format!("{class_id}")[..16]);
        println!("  declared leaves   = {declared}");
        println!("  exposure pwu (v3) = {exposure_pwu}");
        println!("  reserved / claim  = {reserved} sompi = {} MSK", reserved as f64 / 1e8);
        println!("  concurrent claims one genesis bond may hold = {}", ceiling / reserved.max(1));
    }
}
