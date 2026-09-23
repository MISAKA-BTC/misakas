//! **DoS/econ audit lane L3 (2026-09-24): panel Sybil/collusion and Final -> rights laundering.**
//!
//! Computes, with the functions the fold calls, the inequality
//!   slashable collateral of the MINIMUM colluding Valid set  >  unrecoverable gain
//! for every licensing door the t12 fold accepts, and the rights a Final mints that the seat lock
//! does not price. Pure arithmetic: nothing here allocates more than a few KiB.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l3_quorum_and_rights -- --nocapture --test-threads=2
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_permit_value_sompi_v1, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecScheduleV1, palw_execution_permits_v1};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXEC_MAX_QUANTA_PER_SPAN_V1, PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_bounded_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
use kaspa_consensus_core::palw_optimistic_licence_v2::palw_optimistic_licence_v2;
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, PalwPanelLiabilityRecordV1, PalwSlashableLockV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwVoidReasonV2};
use kaspa_consensus_core::palw_verification_v2::{palw_coverage_v2, palw_segment_assignment_v2};
use kaspa_consensus_core::palw_work_target_v1::palw_work_floor_v1;
/// The fold's `mul_div_u128` is pub(crate); the operands here stay far below 2^128 (ccu < 2^50, x7,708).
fn mul_div_u128(a: u128, b: u128, d: u128) -> u128 { a * b / d }
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// `SUBSIDY_BY_MONTH_TABLE[0]` (consensus/src/processes/coinbase.rs:598), a per-SECOND reward; the
/// per-block subsidy is `value * ttpb / 1000` (coinbase.rs:90-92). t12 has
/// `deflationary_phase_daa_score = 0`, so `calc_block_subsidy` returns this row from DAA 0.
const SUBSIDY_PER_SECOND_MONTH0: u64 = 3_704_683_450;
const MSK: f64 = 1e8;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn msk(s: u128) -> f64 {
    s as f64 / MSK
}

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

struct T12Facts {
    subsidy: u64,
    escrow: u64,
    worker_base: u64,
    window_challenge: u64,
    window_court: u64,
    ttpb: u64,
    quorum: u16,
    seats: u16,
    floor_draw: u128,
    floor_declared: u64,
    classes: Vec<(&'static str, u128)>,
    w0: u128,
}

fn facts_t12() -> T12Facts {
    let p = t12();
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("ConsensusV2") };
    assert_eq!(p.deflationary_phase_daa_score, 0, "t12 pays the month-0 table from DAA 0");
    let ttpb = p.target_time_per_block();
    let subsidy = (SUBSIDY_PER_SECOND_MONTH0 as u128 * ttpb as u128).div_ceil(1000) as u64;
    assert_eq!(subsidy, 444_562_014_000, "the t11/t12 per-block subsidy the params tests pin");
    let carve = p.palw_overlay_carve.expect("t12 arms the 720 permille carve");
    let escrow = palw_reward_carve_v2(subsidy, &PalwRewardParamsV2::new(carve.worker_carve_permille).unwrap()).worker;
    // The worker base a receipt-lane blue is paid UNESCROWED (coinbase.rs:246-273: escrow is withheld
    // only from the block whose transition created a claim). 72 % of the subsidy past ADR-0126.
    let worker_base = escrow;
    let draw = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, c: (u32, u32)| -> u128 {
        let d = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).unwrap();
        let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, c.0, c.1);
        palw_canonical_draw_work_v1(&d, &j, true).unwrap().provisional_scalar_v1()
    };
    let floor_p =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .unwrap();
    let floor_draw = draw(&floor_p, kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    let h_p = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: 512,
            ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
        }),
    )
    .unwrap();
    let h_draw = draw(&h_p, kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(512));
    let d_p = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B },
    )
    .unwrap();
    let d_draw = draw(&d_p, kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(2_097_152));
    let rate = p.palw_economic_payout.expect("t12 arms Upgrade C").rate_sompi_per_giga;
    T12Facts {
        subsidy,
        escrow,
        worker_base,
        window_challenge: b.state.window_challenge(),
        window_court: b.state.window_court(),
        ttpb,
        quorum: b.panel.quorum(),
        seats: b.panel.seat_count(),
        floor_draw,
        floor_declared: 7_708,
        classes: vec![("BASE-0 floor", floor_draw), ("Qwen3.6@512", h_draw), ("Qwen2.5@2M", d_draw)],
        w0: palw_work_floor_v1(escrow, rate),
    }
}

/// `claim_realizable_rights_v1` (palw_state_v2.rs:9417-9454), past both fences, on the claim's
/// exposure: the quanta the mint would issue (ceiling 2^16) + 1, priced over the maturity gap.
fn rights_of(f: &T12Facts, exposure: u64) -> u128 {
    let counted = palw_execution_quantum_count_v1(
        u128::from(exposure),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        Hash64::default(),
        Hash64::default(),
    )
    .min(PALW_EXEC_MAX_QUANTA_PER_SPAN_V1 as u32);
    palw_realizable_before_maturity_v1(
        counted.saturating_add(1),
        f.window_challenge,
        f.window_court,
        f.ttpb,
        palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI),
    )
}

/// The smallest number of `Valid` receipts that makes each door's ACCEPTANCE predicate true, found by
/// brute force over every subset of the panel with the real predicates, for 256 claim ids.
/// Returns (V1 quorum, V2 coverage, S2 genesis class, S2 outsider-judged worst, S2 outsider-judged mean).
fn min_valid_per_door(f: &T12Facts) -> (u32, u32, u32, u32, f64) {
    let n = f.seats as usize;
    let seats: Vec<PalwBondKeyV2> = (0..n as u64).map(|i| bond(100 + i)).collect();
    let (mut v2_min, mut s2_min, mut s2o_worst, mut s2o_sum) = (u32::MAX, u32::MAX, 0u32, 0u64);
    let trials = 256u64;
    for t in 0..trials {
        let anchor = Hash64::from_u64_word(0xA000 + t);
        let claim = Hash64::from_u64_word(0xC000 + t);
        let assignment = palw_segment_assignment_v2(anchor, claim, f.seats);
        let (mut v2_here, mut s2_here, mut s2o_here) = (u32::MAX, u32::MAX, u32::MAX);
        for mask in 1u32..(1u32 << n) {
            let valid: Vec<usize> = (0..n).filter(|i| mask & (1 << i) != 0).collect();
            let count = valid.len() as u32;
            // V2 (validate_receipt_coverage_v2, palw_panel_v2.rs:1690-1705): V1 quorum AND every
            // segment attested twice, each Valid seat attesting exactly its assigned mask.
            let masks: Vec<_> = valid.iter().map(|i| assignment.mask_of(*i as u16)).collect();
            if count >= f.quorum as u32 && palw_coverage_v2(assignment.segments, &masks).licenses() {
                v2_here = v2_here.min(count);
            }
            // S2 (processor.rs:7356-7393 + palw_optimistic_licence_v2): the full seat's Valid is
            // "necessary and sufficient"; `NoQuorum` from the coverage check is tolerated.
            let valid_bonds: Vec<PalwBondKeyV2> = valid.iter().map(|i| seats[*i]).collect();
            if palw_optimistic_licence_v2(&assignment, &seats, &valid_bonds) {
                s2_here = s2_here.min(count);
                // Outsider-judged claim (a registered class past ADR-0147): the fold also demands
                // seat 0's Served answer (palw_licence_names_its_outsider_v1, palw_state_v2.rs:627).
                if valid.contains(&0) {
                    s2o_here = s2o_here.min(count);
                }
            }
        }
        v2_min = v2_min.min(v2_here);
        s2_min = s2_min.min(s2_here);
        s2o_worst = s2o_worst.max(s2o_here);
        s2o_sum += s2o_here as u64;
    }
    (f.quorum as u32, v2_min, s2_min, s2o_worst, s2o_sum as f64 / trials as f64)
}

#[test]
fn dos_l3_colluding_quorum_inequality_per_door_attempt_claims() {
    let f = facts_t12();
    let (v1, v2, s2, s2o_worst, s2o_mean) = min_valid_per_door(&f);
    println!("\n=== t12 constants (from Params) ===");
    println!("  subsidy/block {:.2} MSK   escrow (720 permille) {:.2} MSK   panel {} seats quorum {}", msk(f.subsidy as u128), msk(f.escrow as u128), f.seats, f.quorum);
    println!("  window_challenge() {}  window_court {}  ttpb {} ms   W0 = escrow/rate = {} MAC-eq", f.window_challenge, f.window_court, f.ttpb, f.w0);
    println!("\n=== minimum colluding Valid receipts per door (brute force, real predicates, 256 claims) ===");
    println!("  V1 ReceiptLicensed           {v1}");
    println!("  V2/S1 ReceiptLicensedV2       {v2}");
    println!("  S2 OptimisticLicensed genesis {s2}");
    println!("  S2 outsider-judged            worst {s2o_worst}, mean {s2o_mean:.2}");
    println!("  ShardReceiptLicensed          (dormant on t12; the arm calls no lock_valid_receipts at all)");
    assert_eq!(v1, 3);
    assert_eq!(v2, 5, "coverage 2/segment with K = seats-1 disjoint partials needs the whole panel");
    assert_eq!(s2, 1, "S2 licenses on the full-replay seat alone");
    assert_eq!(s2o_worst, 2);

    println!("\n=== per class: G = palw_max_fraud_gain_v1 as panel_valid_lock_required builds it; lock = palw_seat_lock_required_v2(G, 3) ===");
    println!(
        "  {:14} {:>14} {:>12} {:>12} {:>12} | {:>7} {:>13} {:>7} {:>13} {:>13}",
        "class", "reserved MSK", "rights MSK", "G MSK", "lock MSK", "door", "slashable", "holds", "cash-slash", "G-slash"
    );
    let mut s2_floor_net = 0f64;
    for (name, d) in &f.classes {
        let exposure = palw_exposure_unit_pwu_v1(*d, f.floor_declared, f.floor_draw);
        let reserved = u128::from(exposure) * 5;
        let rights = rights_of(&f, exposure);
        let facts = PalwClaimFraudFactsV1 {
            reserved,
            escrowed_reward: f.escrow,
            exposure_pwu: exposure,
            slash_value_per_pwu: 5,
            extra_economic_rights_sompi: rights,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let lock = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        for (door, n) in [("V1", v1), ("V2", v2), ("S2", s2), ("S2-out", s2o_worst)] {
            let slashable = lock * n as u128;
            let holds = slashable > gain;
            // Cash the colluding producer+seats keep if EVERY such Final is convicted after Final:
            // the escrow was paid at Final (finalize_claim), the conviction slashes only the locks
            // (consume_objective_offence, palw_state_v2.rs:9690-9693) and never the producer.
            let cash_net = f.escrow as f64 / MSK - msk(slashable);
            let g_net = msk(gain) - msk(slashable);
            println!(
                "  {:14} {:>14.4} {:>12.2} {:>12.2} {:>12.2} | {:>7} {:>13.2} {:>7} {:>13.2} {:>13.2}",
                name,
                msk(reserved),
                msk(rights),
                msk(gain),
                msk(lock),
                door,
                msk(slashable),
                holds,
                cash_net,
                g_net
            );
            if *name == "BASE-0 floor" && door == "S2" {
                s2_floor_net = cash_net;
            }
            match door {
                "V1" | "V2" => assert!(holds, "{name} {door}"),
                _ => assert!(!holds, "{name} {door}: the minimum colluding set does NOT out-value the gain"),
            }
        }
    }
    println!("\n  floor class via S2: the producer+full-seat keep {s2_floor_net:.2} MSK CASH per Final even when convicted after Final");
    assert!(s2_floor_net > 2_000.0);
    // What finalize_claim actually NAMES the colluders on a floor claim (no economics snapshot, no
    // model line): the ADR-0124 split with only the full seat credited (S2 carries one receipt).
    let split = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_split_permille_v1(
        f.escrow,
        kaspa_consensus_core::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u16,
        f.seats as usize,
        1,
    );
    let named = split.producer + split.per_seat;
    let floor_exposure0 = palw_exposure_unit_pwu_v1(f.floor_draw, f.floor_declared, f.floor_draw);
    let lock0 = palw_seat_lock_required_v2(
        palw_max_fraud_gain_v1(&PalwClaimFraudFactsV1 {
            reserved: u128::from(floor_exposure0) * 5,
            escrowed_reward: f.escrow,
            exposure_pwu: floor_exposure0,
            slash_value_per_pwu: 5,
            extra_economic_rights_sompi: rights_of(&f, floor_exposure0),
        }),
        3,
    );
    println!(
        "  exact cash named to producer+full seat: {:.2} + {:.2} = {:.2} MSK; minus the one lock {:.2} = {:.2} MSK net per convicted Final",
        msk(split.producer as u128),
        msk(split.per_seat as u128),
        msk(named as u128),
        msk(lock0),
        msk(named as u128) - msk(lock0)
    );
    assert!(named as u128 > lock0 * 2);

    // The failure branch: a fraudulent attempt whose panel the attacker does NOT control voids pre-Final
    // and costs the producer `reserved` (void_and_slash). With probability f the attacker holds the full seat.
    let floor_exposure = palw_exposure_unit_pwu_v1(f.floor_draw, f.floor_declared, f.floor_draw);
    let floor_reserved = f64::from(floor_exposure as u32) * 5.0 / MSK;
    let lock_floor = msk(palw_seat_lock_required_v2(
        palw_max_fraud_gain_v1(&PalwClaimFraudFactsV1 {
            reserved: u128::from(floor_exposure) * 5,
            escrowed_reward: f.escrow,
            exposure_pwu: floor_exposure,
            slash_value_per_pwu: 5,
            extra_economic_rights_sompi: rights_of(&f, floor_exposure),
        }),
        3,
    ));
    let escrow = f.escrow as f64 / MSK;
    // EV(f) = f*(escrow - lock) - (1-f)*fail_cost ; break-even f* = fail/(escrow - lock + fail)
    let today = floor_reserved;
    let option_a = floor_reserved + escrow; // option A: the reservation also carries the escrow
    let be = |fail: f64| fail / (escrow - lock_floor + fail);
    println!("\n=== floor S2 break-even share of full seats (EV>0 above it), every success convicted post-Final ===");
    println!("  failure cost today   (reserved)          {today:.6} MSK  -> f* = {:.2e}", be(today));
    println!("  failure cost option A (reserved+escrow)  {option_a:.2} MSK  -> f* = {:.3}", be(option_a));
    assert!(be(today) < 1e-6);
    assert!(be(option_a) > 0.5 && be(option_a) < 0.7);
}

/// **A free-prompt Final mints receipt-lane block rights that no lock prices and no conviction revokes.**
#[test]
fn dos_l3_fp_final_mints_unpriced_unforfeitable_receipt_blocks() {
    let f = facts_t12();
    // Compute-priced reservation (palw_fp_compute_reserved_v1, palw_state_v2.rs:17683-17697): the
    // credited compute in the FLOOR's collateral unit, at 5 sompi.
    let reserved_of = |ccu: u128| mul_div_u128(ccu, f.floor_declared as u128, f.floor_draw) * 5;
    let (v1, _v2, s2, _, _) = min_valid_per_door(&f);
    let q36 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: 512,
            ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
        }),
    )
    .unwrap();
    let runs: Vec<(&str, u128)> = vec![
        ("one W0 of compute", f.w0),
        ("Qwen3.6@512 256+256", kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1(&q36, 256, 256, 0).unwrap()),
    ];
    // Receipt lane: 1000 - fp_attempt_share_permille = 100 permille of production; a quantum's win is
    // spendable only inside [Final + receipt_maturity, + use_window] (fp_draw_slot_v3 /
    // fp_spend_window_contains_v3) = [Final+400, Final+1000] DAA, all inside the 3,000-DAA liability.
    let lane_blocks_in_window = 600u128 * 100 / 1000;
    println!("\n=== FP claim: G as the lock prices it vs the receipt blocks its Final licenses ===");
    println!("  W0 {} MAC-eq; receipt block pays the unescrowed worker base {:.2} MSK", f.w0, msk(f.worker_base as u128));
    println!(
        "  {:22} {:>16} {:>12} {:>10} {:>10} {:>10} {:>12} {:>14} {:>10}",
        "run", "credited MAC-eq", "reserved", "G(code)", "lock", "E[blocks]", "E[payout]", "S2/V1 slash", "payout/V1"
    );
    for (name, ccu) in runs {
        let reserved = reserved_of(ccu);
        let exposure = (reserved / 5) as u64;
        // escrowed_reward: 0 on the commitment lane (palw_state_v2.rs:17372); the rights term is the
        // EXECUTION-quanta formula, which record_round_final never mints for a free-prompt source.
        let facts = PalwClaimFraudFactsV1 {
            reserved,
            escrowed_reward: 0,
            exposure_pwu: exposure,
            slash_value_per_pwu: 5,
            extra_economic_rights_sompi: rights_of(&f, exposure),
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let lock = palw_seat_lock_required_v2(gain, 3);
        // At the pooled-target ceiling (fp_pooled_target_ceiling_v1) expected wins = compute / W,
        // W >= W0; capped by what the lane can produce inside the use window.
        let expected_blocks_milli = (ccu * 1000 / f.w0).min(lane_blocks_in_window * 1000);
        let payout = expected_blocks_milli * f.worker_base as u128 / 1000;
        println!(
            "  {:22} {:>16} {:>12.4} {:>10.4} {:>10.4} {:>10.3} {:>12.2} {:>6.2}/{:>6.2} {:>10.0}",
            name,
            ccu,
            msk(reserved),
            msk(gain),
            msk(lock),
            expected_blocks_milli as f64 / 1000.0,
            msk(payout),
            msk(lock * s2 as u128),
            msk(lock * v1 as u128),
            payout as f64 / (lock * v1 as u128).max(1) as f64
        );
        if ccu == f.w0 {
            assert!(payout > 100 * lock * v1 as u128, "even the full V1 quorum's locks are <1% of the licensed payout");
        }
    }
    println!("  UNPRICED (the escrow half of #5, still open): the lock above does not carry these receipt blocks.");
    println!("  FORFEITABLE past the 2026-09-23 fence (#5/#8): a conviction voids the named Final and");
    println!("  apply_receipt_spend refuses any claim of a convicted root (dos_repro_1 folds both).");
}

/// **ADR-0151's forfeiture reaches the snapshot and the mint — and, past the 2026-09-23 audit
/// fence, the already-minted schedule (fix #7).**
///
/// The defect as measured stays measured: the round verdict (`palw_round_verdicts_v1`) reads the
/// parent state's schedule and `palw_execution_permit_of_v1` with no forfeiture argument, so a
/// schedule minted before the conviction grants every convicted ticket. The fix rewrites the
/// schedule at the conviction (`consume_objective_offence` -> `forfeit_minted_round_rights` ->
/// `palw_execution_schedule_forfeit_v1`); the rewritten schedule grants none. The fold half —
/// the rewrite landing in state, reloading and reverting, and not landing below the fence — is
/// `dos_g2_conviction_takes_back.rs`.
#[test]
fn dos_l3_forfeiture_reaches_a_minted_schedule() {
    let root = Hash64::from_u64_word(0xF00D);
    let fin = PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: bond(7),
        operator_id: Hash64::from_u64_word(2),
        claim_id: Hash64::from_u64_word(3),
        execution_root: root,
        credit: 5 * PALW_EXECUTION_QUANTUM_V1,
    };
    let seed = Hash64::from_u64_word(9);
    let quanta =
        palw_execution_mint_quanta_bounded_v1(&[fin], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 1_000, 0, &Default::default(), 1 << 16);
    assert_eq!(quanta.len(), 5);
    let schedule = PalwExecScheduleV1 { span_index: 3, seed, domains: vec![], finals: vec![fin], quanta: quanta.clone() };
    // The conviction lands AFTER the mint: the fold's only readers of the forfeiture set are the
    // snapshot filter and the mint (palw_state_v2.rs:10952-11017); the round verdict
    // (processor.rs:8134-8190) reads `state.round_schedule(span)` and palw_execution_permit_of_v1
    // with no forfeiture argument.
    let forfeited: std::collections::BTreeSet<Hash64> = [root].into();
    let remint =
        palw_execution_mint_quanta_bounded_v1(&[fin], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 1_000, 0, &forfeited, 1 << 16);
    assert!(remint.is_empty(), "a mint AFTER the conviction issues nothing");
    let still = quanta.iter().filter(|q| !palw_execution_permits_v1(&schedule, q.scheduled_round, 1).is_empty()).count();
    assert_eq!(still, 5, "the schedule as minted grants every quantum of the convicted Final: the verdict reads nothing else");
    // What the fold writes at the conviction past the fence.
    let pruned = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_schedule_forfeit_v1(&schedule, &root)
        .expect("the schedule holds the convicted work, so the conviction rewrites it");
    let after = quanta.iter().filter(|q| !palw_execution_permits_v1(&pruned, q.scheduled_round, 1).is_empty()).count();
    println!("\n=== forfeiture vs a minted schedule ===");
    println!("  quanta minted before conviction {}; permits the minted schedule grants: {still}; after the #7 rewrite: {after}", quanta.len());
    assert_eq!(after, 0, "no quantum of the convicted Final is a permit once the conviction reaches the schedule");
    assert!(pruned.quanta.is_empty() && pruned.domains.is_empty(), "and the emptied schedule does not reopen the lottery");
    assert!(
        kaspa_consensus_core::palw_execution_lane_v1::palw_execution_schedule_forfeit_v1(&pruned, &root).is_none(),
        "a second conviction of the same root writes nothing"
    );
    assert!(
        kaspa_consensus_core::palw_execution_lane_v1::palw_execution_schedule_forfeit_v1(&schedule, &Hash64::default()).is_none(),
        "the zero root (a conviction that named no execution) forfeits nothing"
    );
}

/// **Liability rows and slashable locks are never pruned** — the only removal of either is a
/// conviction (palw_state_v2.rs:9353-9381, 9691). Every licensed claim adds its Valid locks and every
/// terminal claim (Final or Voided, palw_state_v2.rs:12049/12090) adds a liability row, forever.
#[test]
fn dos_l3_liability_and_lock_rows_are_never_pruned() {
    let lock = PalwSlashableLockV1 { claim: Hash64::from_u64_word(1), amount: 1, expiry_daa: 1, settled_at_final: 1 };
    let key = (bond(1), Hash64::from_u64_word(1));
    let lock_bytes = borsh::to_vec(&key).unwrap().len() + borsh::to_vec(&lock).unwrap().len();
    let row = |signers: usize| PalwPanelLiabilityRecordV1 {
        claim_id: Hash64::from_u64_word(1),
        work_id: Hash64::from_u64_word(2),
        class_id: Hash64::from_u64_word(3),
        execution_root: Hash64::from_u64_word(4),
        output_root: Hash64::from_u64_word(5),
        executor_bond: bond(9),
        voided_daa: Some(1),
        void_reason: Some(PalwVoidReasonV2::ProducerWithholding),
        valid_signers: (0..signers as u64).map(|i| (bond(i).0, Hash64::from_u64_word(1))).collect(),
        locked_sompi: 1,
        expiry_daa: 1,
        settled_at_final: 1,
    };
    let row3 = borsh::to_vec(&row(3)).unwrap().len() + 64;
    let row5 = borsh::to_vec(&row(5)).unwrap().len() + 64;
    // One Final per 120 s is the attempt-lane rate: 720 a day.
    let per_claim_v1 = 3 * lock_bytes + row3;
    let per_year = per_claim_v1 as u128 * 720 * 365;
    println!("\n=== rooted, never-pruned bytes ===");
    println!("  lock entry {lock_bytes} B; liability row {row3} B (3 signers) / {row5} B (5 signers)");
    println!("  per V1-licensed Final {per_claim_v1} B; at 720 Finals/day: {:.1} MiB/year, {} lock rows/year", per_year as f64 / 1048576.0, 3 * 720 * 365);
    println!("  state_root re-serialises and re-hashes both maps whole on every block (collection_root, palw_state_v2.rs:8118-8132),");
    println!("  and slashable_available / slashable_live_locked scan every lock ever written per call (9463-9485).");
    assert!(lock_bytes >= 150 && row3 >= 500);
}
