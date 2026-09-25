//! AUDIT LANE C1 — replay of claims, permits and execution roots on testnet-12.
//!
//! Read-only measurement. Nothing here is a fixture for the chain; every number is printed so the
//! finding can quote it.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_maturity_daa_v1, palw_rounds_per_daa_v1,
};
use kaspa_consensus_core::palw_execution_lane_v1::{
    PalwExecFinalV1, PalwExecScheduleV1, palw_execution_schedule_assign_quanta_matured_v1, palw_execution_schedule_snapshot_v1,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_matured_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7,
    qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps,
    qwen36_held_canonical_v1, qwen36_profile_v7};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::{BTreeMap, BTreeSet};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn final_of(claim: u64, root: u64, credit: u64, domain: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: h(domain),
        bond: bond(domain),
        operator_id: h(domain + 50),
        claim_id: h(claim),
        execution_root: h(root),
        credit,
        accepted_blue_score: 0,
    }
}

/// derived MAC-eq of ONE draw for the three classes t12 registers at genesis.
fn per_draw_mac_eq() -> BTreeMap<&'static str, u128> {
    let mut out = BTreeMap::new();

    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor profile");
    let (pf, df) = PALW_RC_BASE0_CANONICAL;
    let d = PalwCanonicalClassDescriptorV1::of(&floor, Hash64::default()).expect("floor descriptor");
    out.insert(
        "base0-floor",
        palw_canonical_draw_work_v1(&d, &rc_job_context(&floor, pf, df), true).expect("floor work").provisional_scalar_v1(),
    );

    let hybrid = qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid");
    let (ph, dh) = qwen36_held_canonical_v1(512);
    let d = PalwCanonicalClassDescriptorV1::of(&hybrid, Hash64::default()).expect("hybrid descriptor");
    out.insert(
        "qwen36@512",
        palw_canonical_draw_work_v1(&d, &rc_job_context(&hybrid, ph, dh), true).expect("hybrid work").provisional_scalar_v1(),
    );

    let dense = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense");
    let (pd, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let d = PalwCanonicalClassDescriptorV1::of(&dense, Hash64::default()).expect("dense descriptor");
    out.insert(
        "qwen25-a16@2M",
        palw_canonical_draw_work_v1(&d, &rc_job_context(&dense, pd, dd), true).expect("dense work").provisional_scalar_v1(),
    );
    out
}

/// The t12 lane constants every other test here quotes.
#[test]
fn audit_c1_t12_lane_constants() {
    let p = t12();
    assert_eq!(p.consensus_params_id(), palw_t12_shipped_params().consensus_params_id(), "Params::from routes to the shipped card");
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let st = &bundle.state;

    let lane = p.palw_execution_lane.expect("the execution lane is armed on t12");
    let span_daa = lane.schedule_span_daa;
    // testnet-12's maturity is the challenge window it APPLIES (user decision 2026-09-25): 120, not
    // the lattice `None` rule's 1,200.
    assert_eq!(palw_exec_quantum_maturity_daa_v1(st.window_challenge(), st.window_court()), 1_200, "the None rule");
    let maturity_daa = p.palw_exec_quantum_maturity_v1();
    let rpd = palw_rounds_per_daa_v1(p.target_time_per_block);
    let maturity_rounds = maturity_daa * rpd;

    println!("t12 target_time_per_block_ms      = {}", p.target_time_per_block);
    println!("t12 rounds_per_daa                = {rpd}");
    println!("t12 schedule_span_daa             = {span_daa}");
    println!("t12 window_challenge / court      = {} / {}", st.window_challenge(), st.window_court());
    println!("t12 quantum maturity (DAA)        = {maturity_daa}");
    println!("t12 quantum maturity (rounds)     = {maturity_rounds}");
    println!("t12 PALW_EXECUTION_QUANTUM_V1     = {PALW_EXECUTION_QUANTUM_V1}");
    println!("t12 permit value (sompi)          = {PALW_T12_PERMIT_FEE_CEILING_SOMPI}");
    println!("t12 claim_retirement_daa          = {}", st.claim_retirement_daa());
    println!("t12 epoch_length                  = {}", st.epoch_length());
    for (name, mac) in per_draw_mac_eq() {
        let quanta = palw_execution_quantum_count_v1(mac, u128::from(PALW_EXECUTION_QUANTUM_V1), h(1), h(2));
        println!("  class {name:<14} per-draw MAC-eq = {mac:>22}   quanta/Final = {quanta}");
    }
    assert_eq!(rpd, 120);
    assert_eq!(span_daa, 1, "a t12 span is ONE DAA tick");
    assert_eq!(maturity_rounds, 14_400, "120 DAA x 120 rounds (144,000 at the unshortened 1,200)");
}

/// **The dedup key the mint enforces is per-CALL, not per-chain.**
///
/// `palw_execution_mint_quanta_matured_v1` collapses Finals by `execution_root` only inside the
/// `finals` slice it is handed. A span's schedule is minted from that span's snapshot alone
/// (`palw_state_v2::rotate_round_lane` -> `palw_execution_schedule_assign_quanta_matured_v1`), so
/// two claims over ONE execution that finalize in two different spans are minted by two different
/// calls and the dedup never sees the pair.
#[test]
fn audit_c1_one_execution_root_mints_a_full_ticket_set_in_every_span_it_finalizes_in() {
    let credit = 10_000u64;
    let quantum = 1_000u128;

    // The case the shipped test covers: both copies in ONE mint.
    let one_mint = palw_execution_mint_quanta_matured_v1(
        &[final_of(1, 0xE0, credit, 1), final_of(2, 0xE0, credit, 1)],
        h(5),
        quantum,
        100,
        0,
        &BTreeSet::new(),
    );

    // The case it misses: the same execution_root in two separate mints (two spans).
    let span_a = palw_execution_mint_quanta_matured_v1(&[final_of(1, 0xE0, credit, 1)], h(5), quantum, 100, 0, &BTreeSet::new());
    let span_b = palw_execution_mint_quanta_matured_v1(&[final_of(2, 0xE0, credit, 1)], h(6), quantum, 220, 0, &BTreeSet::new());

    println!("one execution_root, one mint   -> {} tickets", one_mint.len());
    println!("one execution_root, two mints  -> {} + {} = {} tickets", span_a.len(), span_b.len(), span_a.len() + span_b.len());

    let ids: BTreeSet<Hash64> = span_a.iter().chain(span_b.iter()).map(|q| q.quantum_id).collect();
    println!("distinct quantum ids across the two mints = {}", ids.len());

    assert_eq!(one_mint.len(), 10, "in one mint the two copies collapse to one job's tickets");
    assert_eq!(span_a.len() + span_b.len(), 20, "in two mints the same execution mints twice");
    assert_eq!(ids.len(), 20, "and every ticket is a distinct spend-once id — nothing later can tell them apart");
}

/// **The snapshot's dedup key is `claim_id`, so duplicate execution roots double a domain's
/// credits inside ONE span** — the quota input, even where the quanta collapse.
#[test]
fn audit_c1_duplicate_execution_roots_double_the_snapshot_credits() {
    let credit = 1_000_000u64;
    let honest = palw_execution_schedule_snapshot_v1(7, &[final_of(1, 0xE0, credit, 1), final_of(2, 0xE1, credit, 1)]);
    let doubled = palw_execution_schedule_snapshot_v1(7, &[final_of(1, 0xE0, credit, 1), final_of(2, 0xE0, credit, 1)]);

    let honest_credits: u64 = honest.domains.iter().map(|d| d.credits).sum();
    let doubled_credits: u64 = doubled.domains.iter().map(|d| d.credits).sum();
    println!("two distinct executions  -> snapshot credits {honest_credits}, finals {}", honest.finals.len());
    println!("one execution twice      -> snapshot credits {doubled_credits}, finals {}", doubled.finals.len());

    assert_eq!(doubled.finals.len(), 2, "the snapshot keeps BOTH claims — it dedups on claim_id, not execution_root");
    assert_eq!(doubled_credits, 2 * credit, "one execution counted twice in the domain's credits");
    assert_eq!(honest_credits, doubled_credits, "indistinguishable from two real jobs at the quota stage");

    // And the quanta stage does collapse them, so the two stages disagree about how much work exists.
    let mut sched =
        PalwExecScheduleV1 { span_index: 7, seed: h(9), domains: doubled.domains.clone(), finals: doubled.finals.clone(), quanta: vec![] };
    palw_execution_schedule_assign_quanta_matured_v1(&mut sched, 100_000, 0, 0, &BTreeSet::new());
    println!("quanta minted from the doubled snapshot = {}", sched.quanta.len());
    assert_eq!(sched.quanta.len(), 10, "quota says 2x, the mint says 1x — two answers from one snapshot");
}

/// **On t12 no execution quantum can ever be spent**: every ticket is scheduled at least
/// `maturity_rounds` (14,400 rounds = 4 h at testnet-12's 120-DAA maturity; 144,000 = 40 h at the
/// 1,200 this was measured at) past the round its span opened in, while the schedule
/// that holds it is dropped by `rotate_round_lane` once the chain is two spans (two DAA ticks,
/// ~240 rounds) further on.
#[test]
fn audit_c1_t12_quanta_outlive_the_schedule_that_holds_them() {
    let p = t12();
    let lane = p.palw_execution_lane.expect("armed");
    let rpd = palw_rounds_per_daa_v1(p.target_time_per_block);
    let maturity_rounds = p.palw_exec_quantum_maturity_v1() * rpd;

    // A dense-row Final: credit is the RAW derived MAC-eq per draw (record_round_final takes the
    // unclamped exposure when execution_quantum > 0). Counted, never materialised — the real count
    // is u32::MAX tickets, which is ~1.2 TiB of PalwExecQuantumV1 and is itself the finding.
    let dense = per_draw_mac_eq()["qwen25-a16@2M"];
    let dense_credit = dense.min(u128::from(u64::MAX)) as u64;
    let dense_tickets =
        palw_execution_quantum_count_v1(u128::from(dense_credit), u128::from(PALW_EXECUTION_QUANTUM_V1), h(42), h(1));
    println!("dense Final credit (MAC-eq)       = {dense_credit}");
    println!("dense tickets (counted)           = {dense_tickets}  (unsaturated {})", dense / u128::from(PALW_EXECUTION_QUANTUM_V1));
    assert_eq!(dense_tickets, u32::MAX, "the ticket count saturates on the 2M row");

    // The SCHEDULING property, measured on a small Final so the vector fits in memory.
    let credit = 1_000_000u64;
    let span_open_round = 1_000_000u64;

    let issued = palw_execution_mint_quanta_matured_v1(
        &[final_of(1, 0xE0, credit, 1)],
        h(42),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        span_open_round,
        maturity_rounds,
        &BTreeSet::new(),
    );
    let earliest = issued.iter().map(|q| q.scheduled_round).min().expect("tickets");
    // `rotate_round_lane` keeps schedules for span_now-1 and span_now only; a span is
    // schedule_span_daa DAA, and a DAA tick is `rounds_per_daa` rounds.
    let schedule_life_rounds = 2 * lane.schedule_span_daa * rpd;
    let last_reachable_round = span_open_round + schedule_life_rounds;

    println!("tickets minted (small Final)      = {}", issued.len());
    println!("span opened at round              = {span_open_round}");
    println!("earliest scheduled round          = {earliest}  (+{})", earliest - span_open_round);
    println!("last round the schedule still exists = {last_reachable_round} (+{schedule_life_rounds})");
    println!("gap the lane can never cross      = {} rounds", earliest.saturating_sub(last_reachable_round));

    assert!(earliest >= span_open_round + maturity_rounds);
    assert!(earliest > last_reachable_round, "no ticket of this span is reachable while its schedule exists");
}

// ---------------------------------------------------------------------------------------------
// C1-1: ONE winning draw, N distinct valid attempt blocks, N claims.
// ---------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptUnsignedV2,
    attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3, execution_commitment_v3,
    l1_tag_v2, palw_nonce_bucket_v1,
};
use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, is_palw_attempt_algo_id, pow_finalizer_blake2b_512};

fn net() -> Hash64 {
    h(0x11)
}
fn pph() -> Hash64 {
    h(0xB0)
}
fn class() -> Hash64 {
    h(0xC1)
}
const BITS: u32 = 0x1F00_FFFF;

fn attempt_at(timestamp: u64, nonce: u64) -> PalwAttemptUnsignedV2 {
    let bond = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
    PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: net(),
        // The ONE field that moves with the block's position.
        challenge: challenge_v2(net(), pph(), timestamp, nonce, class(), &bond),
        class_id: class(),
        executor_bond: bond,
        executor_pubkey: vec![7u8; 32],
        operator_id: h(0xE0),
        artifact_root: h(0xA7),
        // The inference: one execution, quoted verbatim into every sibling.
        trace_root: h(0x7A),
        output_root: h(0x00),
        pwu: 4_242,
        trace_manifest_root: attempt_trace_manifest_root_v1(h(0x7A), PALW_ATTEMPT_V2_TRACE_CHUNKS),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: 999_999,
        execution_root: h(0x41),
    }
}

/// **The Layer-0 digest of an algo-6 header is invariant under nonce (inside the bucket) and under
/// the timestamp — so ONE winning draw is an unbounded supply of distinct valid blocks.**
///
/// `StateLayer0::calculate_pow_layer0` (consensus/pow/src/lib.rs:557) passes `(0, 0)` for
/// `(timestamp, nonce)` whenever `is_palw_attempt_algo_id`, and the tag is
/// `l1_tag_v2(execution_commitment_v3(attempt, anchor))`, which blanks the challenge. Everything
/// the digest reads is therefore fixed by the inference and the template.
#[test]
fn audit_c1_one_winning_draw_is_an_unbounded_supply_of_valid_blocks() {
    let bond = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
    assert!(is_palw_attempt_algo_id(POW_ALGO_ID_PALW_COMMITTED_V2));

    let digest_of = |timestamp: u64, nonce: u64| {
        let a = attempt_at(timestamp, nonce);
        let anchor = execution_anchor_v3(net(), pph(), class(), &bond, nonce);
        let tag = l1_tag_v2(execution_commitment_v3(&a, anchor));
        // Exactly what calculate_pow_layer0 does for this lane: timestamp and nonce are zeroed.
        let d = pow_finalizer_blake2b_512(b"audit", POW_ALGO_ID_PALW_COMMITTED_V2, pph(), 0, BITS, 0, &tag).expect("digest");
        (d, attempt_id_v2(&a), class_ticket_v3(&a, anchor), anchor)
    };

    let base_ts = 1_700_000_000u64;
    let base_nonce = 7u64;
    let (d0, id0, ticket0, anchor0) = digest_of(base_ts, base_nonce);

    let mut ids = BTreeSet::new();
    let mut digests = BTreeSet::new();
    let mut tickets = BTreeSet::new();
    ids.insert(id0);
    digests.insert(d0);
    tickets.insert(ticket0);

    // Siblings the attacker can mint from the SAME inference: any nonce in the bucket, any legal
    // timestamp. Nothing here re-runs the model and nothing here re-solves anything.
    let bucket_top = (base_nonce >> PALW_TICKET_NONCE_BUCKET_LOG2 << PALW_TICKET_NONCE_BUCKET_LOG2) + (1 << PALW_TICKET_NONCE_BUCKET_LOG2) - 1;
    for (ts, nonce) in [
        (base_ts, base_nonce + 1),
        (base_ts, bucket_top),
        (base_ts + 1, base_nonce),
        (base_ts + 120_000, base_nonce + 999),
        (base_ts + 3_600_000, bucket_top - 5),
    ] {
        let (d, id, ticket, anchor) = digest_of(ts, nonce);
        assert_eq!(palw_nonce_bucket_v1(nonce), palw_nonce_bucket_v1(base_nonce), "stayed in the bucket");
        assert_eq!(anchor, anchor0, "same anchor -> same job -> same prompt -> the SAME inference answers");
        ids.insert(id);
        digests.insert(d);
        tickets.insert(ticket);
    }

    println!("distinct attempt ids (=> distinct claim ids) = {}", ids.len());
    println!("distinct Layer-0 digests                     = {}", digests.len());
    println!("distinct class-lottery tickets               = {}", tickets.len());
    println!("nonces per bucket                            = {}", 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2);

    assert_eq!(digests.len(), 1, "one draw: every sibling has the identical Layer-0 digest, so all win or none do");
    assert_eq!(tickets.len(), 1, "and the identical class ticket");
    assert_eq!(ids.len(), 6, "but SIX distinct attempt ids — so `DuplicateClaim(attempt_id)` never fires");
}

/// **The only dedup that sees them is the per-mergeset execution key, and it is keyed on the
/// carrying block's `pre_pow`.** Siblings share it, so two siblings in ONE mergeset collide — and a
/// sibling released into the NEXT accepting block is compared against an empty set.
#[test]
fn audit_c1_the_execution_key_collides_only_inside_one_mergeset() {
    let bond = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 };
    let key_of = |timestamp: u64, nonce: u64, pre_pow: Hash64| {
        let a = attempt_at(timestamp, nonce);
        let a = PalwAttemptUnsignedV2 { challenge: challenge_v2(net(), pre_pow, timestamp, nonce, class(), &bond), ..a };
        execution_commitment_v3(&a, execution_anchor_v3(net(), pre_pow, class(), &bond, nonce))
    };
    let k1 = key_of(1_700_000_000, 7, pph());
    let k2 = key_of(1_700_060_000, 8, pph());
    println!("sibling execution keys equal = {}", k1 == k2);
    assert_eq!(k1, k2, "seen_exec / seen_here_exec sees the pair — but only when both are in ONE transition");

    // TransitionBuilder::new (consensus/core/src/palw_state_v2.rs:8571) creates seen_exec empty for
    // every chain block, and nothing in PalwChainStateV2 stores an execution key. So the model of
    // "the state answers for identities the chain has already seen" is:
    let chain_state_remembers_execution_keys = false;
    assert!(!chain_state_remembers_execution_keys, "no rooted store holds an execution key — grep palw_state_v2 for seen_exec");
}

/// **What one replayed sibling is worth** — the escrow its claim carries and the fork weight it
/// folds, against an attacker cost of one signature.
#[test]
fn audit_c1_what_a_replayed_sibling_is_worth() {
    use kaspa_consensus_core::palw_reward_v2::palw_reward_carve_v2;
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    // The carve in force from DAA 0 is the FENCE's, not the bundle's (palw_state_v2::worker_carve_at).
    let carve = p.palw_overlay_carve.expect("armed on t12");
    // Runtime-verified upstream by CoinbaseManager::calc_block_subsidy(any daa) on t12.
    const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
    let split = palw_reward_carve_v2(
        T12_BLOCK_SUBSIDY_SOMPI,
        &kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2::new(carve.worker_carve_permille).expect("a legal carve"),
    );
    println!("t12 worker_carve_permille (fence) = {}", carve.worker_carve_permille);
    println!("t12 bundle worker_carve_permille  = {}", bundle.state.worker_carve_permille());
    println!("escrow per claim (sompi)          = {}", split.worker);
    println!("escrow per claim (MSK)            = {:.5}", split.worker as f64 / 1e8);
    assert_eq!(carve.worker_carve_permille, 720);
    assert_eq!(split.worker, 320_084_650_080);

    // The fork weight one dense claim folds: pwu = expected_attempts(target) x derived per draw,
    // and the dense row's t12 target is u128::MAX (the boot target), so expected_attempts == 1.
    use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1, palw_ticket_admits_v1};
    let dense = per_draw_mac_eq()["qwen25-a16@2M"];
    let attempts = palw_expected_attempts_v1(u128::MAX);
    let pwu = palw_pwu_v1(u128::MAX, dense.min(u128::from(u64::MAX)) as u64);
    println!("dense expected_attempts at MAX    = {attempts}");
    println!("dense claim pwu (MAC-eq)          = {pwu}");
    assert_eq!(attempts, 1);
    assert_eq!(pwu, 3_357_281_757_221_376);
    // And at the boot target every ticket admits: the class lottery is not a gate on this row.
    for t in [0u128, 1, u128::MAX / 2, u128::MAX] {
        assert!(palw_ticket_admits_v1(t, u128::MAX), "a target of u128::MAX admits every ticket");
    }
}

/// **The dense row's class lottery is not a gate on t12** — its work ticket target saturates at
/// `u128::MAX`, and its genesis object already declares that target.
#[test]
fn audit_c1_the_dense_rows_lottery_admits_every_ticket() {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let payout = p.palw_economic_payout.expect("ADR-0132 Upgrade C is armed on t12");
    let escrow = 320_084_650_080u64;
    let w0 = palw_work_floor_v1(escrow, payout.rate_sompi_per_giga);
    println!("W0 (CCU) from escrow {escrow} at rate {} = {w0}", payout.rate_sompi_per_giga);
    for (name, ccu) in per_draw_mac_eq() {
        let target = palw_work_ticket_target_v1(ccu, w0);
        println!("  {name:<14} ccu={ccu:>22}  ccu/W0={:>10.4}  target/MAX={:.6}", ccu as f64 / w0 as f64, target as f64 / u128::MAX as f64);
    }
    assert_eq!(palw_work_ticket_target_v1(per_draw_mac_eq()["qwen25-a16@2M"], w0), u128::MAX, "the dense row saturates");

    for obj in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, initial_target, pwu_rule, share_permille, .. } = obj {
            println!("  genesis class {class_id} target={initial_target} share={share_permille} rule={pwu_rule:?}");
        }
    }
}
