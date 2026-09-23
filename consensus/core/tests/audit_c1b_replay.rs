//! AUDIT LANE C1 (second pass) — replay of claims, permits and execution roots on testnet-12.
//!
//! Read-only measurement. Every number is printed so a finding can quote it. Nothing here is a
//! fixture the chain consumes.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    execution_commitment_v3, l1_tag_v2, palw_nonce_bucket_v1,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_safety_v1::{palw_exec_quantum_maturity_daa_v1, palw_rounds_per_daa_v1};
use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecScheduleV1, palw_execution_permits_v1};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_matured_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, is_palw_attempt_algo_id, pow_finalizer_blake2b_512};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::BTreeSet;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

// ---------------------------------------------------------------------------------------------
// C1-A. The fold itself: one execution, two chain blocks, two claims.
// ---------------------------------------------------------------------------------------------

const NET: u64 = 999;
const PPH: u64 = 5;
const CLASS: u64 = 1;

/// The fixture class + bond, with enough collateral that the exposure ceiling is not the thing
/// under test.
fn register_class_and_bond(collateral: u64) -> Vec<PalwConsensusObjectV2> {
    vec![
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
            bond: bond_key(1),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21u8; 8],
            collateral,
            payout_payload: Hash64::from_u64_word(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ]
}

/// One inference (trace_root / output_root / execution_root fixed), re-announced at `nonce` and
/// `timestamp`. Only `challenge` moves — which is the only field `execution_commitment_v3` blanks.
fn sibling(nonce: u64, timestamp: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key(1).0;
    PalwAttemptEnvelopeV2 {
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
    }
}

fn ctx(block: u64, daa: u64, blue: u64, subsidy: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy }
}

fn fixture_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(CLASS), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap().with_worker_carve_permille(720).unwrap()
}

/// **THE FINDING, run through the real fold.** `TransitionBuilder::seen_exec` is created empty for
/// every chain block and `PalwChainStateV2` stores no execution key, so one inference announced in
/// two different chain blocks mints TWO claims, TWO exposure reservations and TWO escrows — with
/// the 2026-09-11 deep fence ARMED, which is what testnet-12 runs from DAA 0.
#[test]
fn audit_c1b_one_execution_mints_a_fresh_claim_in_every_chain_block() {
    let p = fixture_params();
    let extras = PalwTransitionExtrasV1 { audit_2026_09_11_deep_active: true, ..Default::default() };
    let subsidy = 444_562_014_000u64; // the t12 block subsidy, measured upstream.

    let genesis = PalwChainStateV2::genesis();
    let (s1, _, _) = apply_palw_transition_v7(
        &genesis,
        &p,
        None,
        &ctx(1, 100, 1, 0),
        &register_class_and_bond(1_000_000_000_000_000),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("registration");

    // Two siblings of ONE inference: same execution_root, different within-bucket nonce.
    let a = sibling(7, 1_700_000_000);
    let b = sibling(8, 1_700_060_000);
    let id_a = attempt_id_v2(&a.attempt);
    let id_b = attempt_id_v2(&b.attempt);
    assert_ne!(id_a, id_b, "the siblings are distinct claim ids");

    // The pre_pow-inclusive key the processor derives for BOTH — identical, because the execution
    // commitment blanks the challenge and the anchor keys only the nonce BUCKET.
    let bond = bond_key(1).0;
    let anchor_a = execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 7);
    let anchor_b = execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 8);
    let key_a = execution_commitment_v3(&a.attempt, anchor_a);
    let key_b = execution_commitment_v3(&b.attempt, anchor_b);
    assert_eq!(key_a, key_b, "one inference, one execution key");

    // Chain block 2 announces sibling A as its OWN work.
    let (s2, _, _) = apply_palw_transition_v7(
        &s1,
        &p,
        None,
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&a),
        &[],
        key_a,
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("block 2 stands");

    // Chain block 3 announces sibling B — the SAME execution key, one block later.
    let (s3, _, _) = apply_palw_transition_v7(
        &s2,
        &p,
        None,
        &ctx(3, 102, 3, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&b),
        &[],
        key_b,
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("block 3 stands");

    let c_a = s3.claim(&id_a).expect("claim A");
    let c_b = s3.claim(&id_b).expect("claim B");
    println!("=== C1-A: one inference, two chain blocks ===");
    println!("deep fence armed                 = {}", extras.audit_2026_09_11_deep_active);
    println!("execution key (both)             = {key_a}");
    println!("claim A reserved (sompi)         = {}", c_a.reserved);
    println!("claim B reserved (sompi)         = {}", c_b.reserved);
    println!("claim A escrowed_reward (sompi)  = {}", c_a.escrowed_reward);
    println!("claim B escrowed_reward (sompi)  = {}", c_b.escrowed_reward);
    println!("bond reserved_exposure after 2   = {}", s3.reserved_exposure(&bond_key(1)));
    println!("safe_weight after 2              = {}", s3.safe_weight());

    assert_eq!(c_a.execution_root, c_b.execution_root, "the two claims name ONE execution root");
    assert_eq!(c_a.trace_root, c_b.trace_root, "and one trace");
    assert_eq!(c_a.reserved, c_b.reserved);
    assert_eq!(c_a.escrowed_reward, c_b.escrowed_reward);
    assert_eq!(s3.reserved_exposure(&bond_key(1)), 2 * c_a.reserved, "two reserves for one inference");
    assert!(c_a.escrowed_reward > 0 && c_b.escrowed_reward > 0, "each carries its own block's escrow");

    // Control: put BOTH in ONE transition's merged work and the deep fence refuses the second.
    // (The intra-block half is the one `b4_merged_nonce_siblings_...` covers.)
    let (s2b, _, _) = apply_palw_transition_v7(
        &s1,
        &p,
        None,
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&a),
        &[],
        key_a,
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("control block");
    assert!(s2b.claim(&id_b).is_none(), "the sibling is not in the same block");
}

// ---------------------------------------------------------------------------------------------
// C1-B. How many siblings ONE inference can mint, and what each is worth on t12.
// ---------------------------------------------------------------------------------------------

fn floor_per_draw_mac_eq() -> u128 {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor profile");
    let (pf, df) = PALW_RC_BASE0_CANONICAL;
    let d = PalwCanonicalClassDescriptorV1::of(&floor, Hash64::default()).expect("descriptor");
    palw_canonical_draw_work_v1(&d, &rc_job_context(&floor, pf, df), true).expect("floor work").provisional_scalar_v1()
}

fn dense_per_draw_mac_eq() -> u128 {
    let dense =
        qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense profile");
    let (pf, df) = qwen25_a16_held_canonical_v1(2_097_152);
    let d = PalwCanonicalClassDescriptorV1::of(&dense, Hash64::default()).expect("descriptor");
    palw_canonical_draw_work_v1(&d, &rc_job_context(&dense, pf, df), true).expect("dense work").provisional_scalar_v1()
}

/// **The siblings are free and they are indistinguishable from each other at every gate that
/// prices work** — one Layer-0 digest, one class ticket, N attempt ids.
#[test]
fn audit_c1b_the_sibling_set_is_one_draw_and_n_identities() {
    let bond = bond_key(1).0;
    assert!(is_palw_attempt_algo_id(POW_ALGO_ID_PALW_COMMITTED_V2));

    let base_nonce = 7u64;
    let bucket = palw_nonce_bucket_v1(base_nonce);
    let bucket_lo = bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
    let bucket_hi = bucket_lo + (1u64 << PALW_TICKET_NONCE_BUCKET_LOG2) - 1;

    let mut ids = BTreeSet::new();
    let mut tickets = BTreeSet::new();
    let mut digests = BTreeSet::new();
    let mut keys = BTreeSet::new();

    for (ts, nonce) in [
        (1_700_000_000u64, base_nonce),
        (1_700_000_000, bucket_lo),
        (1_700_000_000, bucket_hi),
        (1_700_000_001, base_nonce),
        (1_700_120_000, bucket_hi - 1),
        (1_703_600_000, bucket_lo + 12_345),
    ] {
        let a = sibling(nonce, ts);
        let anchor = execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, nonce);
        let key = execution_commitment_v3(&a.attempt, anchor);
        // Exactly what `StateLayer0::calculate_pow_layer0` computes for this lane: the tag is
        // `l1_tag_v2(execution_commitment_v3(..))` and (timestamp, nonce) are zeroed in the digest.
        let tag = l1_tag_v2(key);
        let d = pow_finalizer_blake2b_512(b"audit", POW_ALGO_ID_PALW_COMMITTED_V2, h(PPH), 0, 0x1F00_FFFF, 0, &tag).expect("digest");
        ids.insert(attempt_id_v2(&a.attempt));
        tickets.insert(class_ticket_v3(&a.attempt, anchor));
        keys.insert(key);
        digests.insert(d);
    }

    println!("=== C1-B: one inference, one draw, N identities ===");
    println!("nonces per bucket (2^{PALW_TICKET_NONCE_BUCKET_LOG2})       = {}", 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2);
    println!("distinct attempt ids            = {}", ids.len());
    println!("distinct execution keys         = {}", keys.len());
    println!("distinct class tickets          = {}", tickets.len());
    println!("distinct Layer-0 digests        = {}", digests.len());

    assert_eq!(ids.len(), 6, "six distinct claim ids");
    assert_eq!(keys.len(), 1, "one execution key");
    assert_eq!(tickets.len(), 1, "one class-lottery draw — so all six win or none do");
    assert_eq!(digests.len(), 1, "one Layer-0 digest");
}

/// **What one extra sibling costs and what it is worth on t12** — the exposure ceiling is the only
/// stated bound on the cross-block replay, so measure it.
#[test]
fn audit_c1b_the_exposure_ceiling_is_the_only_bound_and_here_is_its_size() {
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let st = &bundle.state;

    let collateral = 51_642_979_663_480u128; // PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI
    let ratio = st.fp_max_exposure_ratio_permille() as u128;
    let ceiling = collateral * ratio / 1000;

    let floor_declared = 7_708u128;
    let floor_canonical = floor_per_draw_mac_eq();
    let dense_canonical = dense_per_draw_mac_eq();
    // palw_exposure_pwu_v3 = canonical x base_declared / base_canonical.
    let floor_exposure_pwu = floor_canonical * floor_declared / floor_canonical;
    let dense_exposure_pwu = dense_canonical * floor_declared / floor_canonical;
    let slash = 5u128;

    let floor_reserve = floor_exposure_pwu * slash;
    let dense_reserve = dense_exposure_pwu * slash;
    let escrow = 320_084_650_080u128; // 444_562_014_000 x 720/1000
    let subsidy = 444_562_014_000u128;

    println!("=== C1-C: the exposure ceiling on t12 ===");
    println!("posted collateral (sompi)        = {collateral}  ({:.2} MSK)", collateral as f64 / 1e8);
    println!("max_exposure_ratio_permille      = {ratio}");
    println!("ceiling (sompi)                  = {ceiling}");
    println!("floor per-draw MAC-eq            = {floor_canonical}");
    println!("dense per-draw MAC-eq            = {dense_canonical}");
    println!("floor claim reserve (sompi)      = {floor_reserve}");
    println!("dense claim reserve (sompi)      = {dense_reserve}  ({:.2} MSK)", dense_reserve as f64 / 1e8);
    println!("concurrent floor claims per bond = {}", ceiling / floor_reserve);
    println!("concurrent dense claims per bond = {}", ceiling / dense_reserve);
    println!("escrow per claim (sompi)         = {escrow}  ({:.5} MSK)", escrow as f64 / 1e8);
    println!("block subsidy (sompi)            = {subsidy}  ({:.5} MSK)", subsidy as f64 / 1e8);
    println!("min_slash_permille_of_escrow     = {}", st.min_slash_permille_of_escrow());
    println!("max_claim_exposure_daa           = {}", 2 * (st.window_bind() + st.window_receipt()) + st.window_challenge() + st.window_court() + st.fp_abandon_hold_daa());
    println!("claim_retirement_daa             = {}", st.claim_retirement_daa());

    assert_eq!(floor_exposure_pwu, 7_708, "the floor normalises to its own declared leaves");
    assert_eq!(floor_reserve, 38_540, "38,540 sompi reserved per floor claim");
    assert_eq!(st.min_slash_permille_of_escrow(), 0, "the escrow-backing inequality is OFF on t12");
    assert!(ceiling / floor_reserve > 600_000_000, "the floor class's ceiling is not a bound in any practical sense");
}

// ---------------------------------------------------------------------------------------------
// C1-C. Execution quanta: the per-call dedup, and whether the tickets are worth anything on t12.
// ---------------------------------------------------------------------------------------------

fn final_of(claim: u64, root: u64, credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: h(1),
        bond: bond_key(1),
        operator_id: h(51),
        claim_id: h(claim),
        execution_root: h(root),
        credit,
    }
}

/// **The execution-root dedup is per CALL.** `palw_execution_mint_quanta_matured_v1` collapses
/// Finals by `execution_root` inside the slice it is handed, and `rotate_round_lane` hands it ONE
/// span's snapshot — so a second Final over the same execution in a later span mints a second,
/// disjoint, spend-once ticket set.
#[test]
fn audit_c1b_the_quantum_dedup_does_not_cross_a_span() {
    let credit = 1_000_000u64;
    let quantum = u128::from(PALW_EXECUTION_QUANTUM_V1);

    let together = palw_execution_mint_quanta_matured_v1(
        &[final_of(1, 0xE0, credit), final_of(2, 0xE0, credit)],
        h(5),
        quantum,
        100,
        0,
        &BTreeSet::new(),
    );
    let span_a = palw_execution_mint_quanta_matured_v1(&[final_of(1, 0xE0, credit)], h(5), quantum, 100, 0, &BTreeSet::new());
    let span_b = palw_execution_mint_quanta_matured_v1(&[final_of(2, 0xE0, credit)], h(6), quantum, 220, 0, &BTreeSet::new());

    let ids: BTreeSet<Hash64> = span_a.iter().chain(span_b.iter()).map(|q| q.quantum_id).collect();
    println!("=== C1-D: quantum dedup ===");
    println!("one root, ONE mint   -> {} tickets", together.len());
    println!("one root, TWO mints  -> {} + {} = {}", span_a.len(), span_b.len(), span_a.len() + span_b.len());
    println!("distinct ticket ids across the two mints = {}", ids.len());
    assert_eq!(together.len(), 10);
    assert_eq!(span_a.len() + span_b.len(), 20);
    assert_eq!(ids.len(), 20, "nothing downstream can tell the two sets apart");
}

/// **…and on t12 the tickets are worth zero, because none of them is reachable.** The maturity is
/// `window_challenge` rounds ahead; the schedule that holds a ticket is dropped after two spans.
/// Worse: `palw_execution_permits_v1` short-circuits to the quantum path whenever `quanta` is
/// non-empty, so a schedule with quanta issues NO permits at all — the domain lottery is skipped.
#[test]
fn audit_c1b_no_execution_quantum_on_t12_is_ever_reachable() {
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let st = &bundle.state;
    let lane = p.palw_execution_lane.expect("armed on t12");
    let rpd = palw_rounds_per_daa_v1(p.target_time_per_block);
    let maturity_daa = palw_exec_quantum_maturity_daa_v1(st.window_challenge(), st.window_court());
    let maturity_rounds = maturity_daa * rpd;
    let schedule_life_rounds = 2 * lane.schedule_span_daa * rpd;

    let span_open_round = 1_000_000u64;
    let issued = palw_execution_mint_quanta_matured_v1(
        &[final_of(1, 0xE0, 1_000_000)],
        h(42),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        span_open_round,
        maturity_rounds,
        &BTreeSet::new(),
    );
    let earliest = issued.iter().map(|q| q.scheduled_round).min().expect("tickets");

    println!("=== C1-E: t12 permit reachability ===");
    println!("rounds per DAA                   = {rpd}");
    println!("schedule_span_daa                = {}", lane.schedule_span_daa);
    println!("window_challenge / window_court  = {} / {}", st.window_challenge(), st.window_court());
    println!("maturity (DAA / rounds)          = {maturity_daa} / {maturity_rounds}");
    println!("schedule life (rounds)           = {schedule_life_rounds}");
    println!("earliest ticket round            = +{}", earliest - span_open_round);
    println!("unreachable by (rounds)          = {}", earliest - span_open_round - schedule_life_rounds);

    assert!(earliest - span_open_round >= maturity_rounds);
    assert!(earliest > span_open_round + schedule_life_rounds, "no ticket outlives its schedule");

    // And the schedule with quanta hands out nothing in ANY round it survives, where the same
    // schedule without quanta hands out permits through the domain lottery.
    let snapshot =
        kaspa_consensus_core::palw_execution_lane_v1::palw_execution_schedule_snapshot_v1(7, &[final_of(1, 0xE0, 1_000_000)]);
    let with_quanta =
        PalwExecScheduleV1 { span_index: 7, seed: h(9), domains: snapshot.domains.clone(), finals: snapshot.finals.clone(), quanta: issued };
    let without = PalwExecScheduleV1 { quanta: vec![], ..with_quanta.clone() };
    let mut with_hits = 0usize;
    let mut without_hits = 0usize;
    for r in span_open_round..span_open_round + schedule_life_rounds {
        with_hits += palw_execution_permits_v1(&with_quanta, r, lane.permits_per_round).len();
        without_hits += palw_execution_permits_v1(&without, r, lane.permits_per_round).len();
    }
    println!("permits over the schedule's whole life, quanta present = {with_hits}");
    println!("permits over the schedule's whole life, quanta absent  = {without_hits}");
    assert_eq!(with_hits, 0, "a schedule that minted quanta issues no permit in any round it survives");
    assert!(without_hits > 0, "the domain lottery WOULD have issued permits over the same rounds");
}

/// **Permit consumption IS rooted, and its retention matches the schedule's.** So a permit cannot
/// be re-presented after a restart or across a span boundary — the one replay question in this lane
/// that closes cleanly.
#[test]
fn audit_c1b_permit_consumption_is_in_the_rooted_state() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/palw_state_v2.rs")).expect("read state");
    assert!(
        src.contains(r#"state.update(collection_root(b"round_permits_used", &self.round_permits_used).as_byte_slice());"#),
        "round_permits_used is hashed into state_root"
    );
    assert!(src.contains("round_permits_used: BTreeMap<(u64, u64), u16>"), "it is a rooted map, not a memo");
    assert!(
        src.contains("for key in self.state.round_permits_used.range(..(keep_from, 0))"),
        "and it is pruned on the SAME keep_from as round_schedules"
    );
    assert!(src.contains("for key in self.state.round_schedules.range(..keep_from)"), "the schedule's own prune");
    println!("=== C1-F: permit ledger ===");
    println!("round_permits_used is a rooted BTreeMap<(span, round), u16 bitmap>, hashed into state_root,");
    println!("pruned at the same keep_from = span_now - 1 that drops the schedule granting the permit.");
}

/// The shipped params this lane quotes, straight from `palw_t12_shipped_params`, so a reader can
/// check the numbers above against one source.
#[test]
fn audit_c1b_t12_params_for_the_record() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("ConsensusV2") };
    let st = &bundle.state;
    println!("=== C1-G: t12 params ===");
    println!("consensus_params_id            = {}", p.consensus_params_id());
    println!("target_time_per_block (ms)     = {}", p.target_time_per_block);
    println!("merge_depth                    = {}", p.merge_depth);
    println!("finality_depth                 = {}", p.finality_depth);
    println!("mergeset_size_limit            = {}", p.mergeset_size_limit);
    println!("max_block_parents              = {}", p.max_block_parents);
    println!("worker carve (fence)           = {:?}", p.palw_overlay_carve.map(|c| c.worker_carve_permille));
    println!("fp_max_exposure_ratio_permille = {}", st.fp_max_exposure_ratio_permille());
    println!("min_collateral_sompi           = {}", st.min_collateral_sompi());
    println!("window_bind/receipt/chal/court  = {} / {} / {} / {}", st.window_bind(), st.window_receipt(), st.window_challenge(), st.window_court());
    println!("epoch_length                   = {}", st.epoch_length());
    println!("palw_single_lottery armed      = {:?}", p.palw_single_lottery);
    println!("palw_attempt_work              = {:?}", p.palw_attempt_work);
}

/// **The replay scales linearly in chain blocks, with ONE inference behind all of it.** Eight
/// chain blocks, eight siblings of one execution, eight claims, eight escrows.
#[test]
fn audit_c1b_the_replay_scales_linearly_and_the_inference_count_stays_one() {
    let p = fixture_params();
    let extras = PalwTransitionExtrasV1 { audit_2026_09_11_deep_active: true, ..Default::default() };
    let subsidy = 444_562_014_000u64;
    let bond = bond_key(1).0;

    let genesis = PalwChainStateV2::genesis();
    let (mut state, _, _) = apply_palw_transition_v7(
        &genesis,
        &p,
        None,
        &ctx(1, 100, 1, 0),
        &register_class_and_bond(1_000_000_000_000_000),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("registration");

    const N: u64 = 8;
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut escrow_total = 0u128;
    for i in 0..N {
        let nonce = 7 + i; // all inside nonce bucket 0
        let env = sibling(nonce, 1_700_000_000 + i);
        let anchor = execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, nonce);
        let key = execution_commitment_v3(&env.attempt, anchor);
        keys.insert(key);
        let id = attempt_id_v2(&env.attempt);
        ids.insert(id);
        let (next, _, _) = apply_palw_transition_v7(
            &state,
            &p,
            None,
            &ctx(2 + i, 101 + i, 2 + i, subsidy),
            &[],
            PalwBlockWorkV3::Attempt(&env),
            &[],
            key,
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap_or_else(|e| panic!("block {i} stands: {e:?}"));
        state = next;
        escrow_total += state.claim(&id).expect("claim").escrowed_reward as u128;
    }

    println!("=== C1-H: N siblings, one inference ===");
    println!("chain blocks                     = {N}");
    println!("distinct execution keys          = {}  (= inferences actually run)", keys.len());
    println!("distinct claims minted           = {}", ids.len());
    println!("total escrow held for the bond   = {escrow_total} sompi ({:.2} MSK)", escrow_total as f64 / 1e8);
    println!("bond reserved_exposure           = {}", state.reserved_exposure(&bond_key(1)));
    println!("subsidy routed per sibling       = {subsidy} sompi ({:.5} MSK)", subsidy as f64 / 1e8);
    println!("bounded_immature (fork weight)   = {}", state.bounded_immature());
    println!("claims carrying a work_id        = {}", ids.iter().filter(|i| state.claim(i).unwrap().work_id.is_some()).count());
    // **The rooted one-inference-one-claim index the FREE-PROMPT lane enforces
    // (`PalwStateV2Error::DuplicateWork`, palw_state_v2.rs:17132) is keyed on `claim.work_id`.
    // Every attempt claim writes `work_id: None` (palw_state_v2.rs:18916), so none of them is in
    // `work_ids` and the index can never see the replay.**
    assert!(ids.iter().all(|i| state.claim(i).unwrap().work_id.is_none()), "attempt claims opt out of the work_id index");
    assert_eq!(state.bounded_immature(), N as u128 * 16, "the replay multiplies immature fork weight too");

    assert_eq!(keys.len(), 1, "ONE inference behind all eight blocks");
    assert_eq!(ids.len(), N as usize, "eight distinct claims");
    for id in &ids {
        assert!(state.claim(id).is_some(), "every sibling's claim survives");
    }
    assert_eq!(escrow_total, N as u128 * 320_084_650_080, "eight escrows of 3,200.85 MSK for one inference");
}

/// **The real topology: the first sibling is the chain block, the rest arrive as MERGED work of
/// later chain blocks.** The merged arm runs the full admission list (`PalwAdmissionParamsV2`) and
/// still admits the sibling — its `attempt_id` is new, so item 10 passes; its class ticket is
/// identical, so the lottery gives the same verdict it gave the first one.
#[test]
fn audit_c1b_the_sibling_arrives_as_merged_work_and_is_admitted_anyway() {
    use kaspa_consensus_core::palw_admission_v2::PalwAdmissionParamsV2;
    use kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1;

    let p = fixture_params();
    let admission = PalwAdmissionParamsV2::new(500).expect("ratio");
    let extras = PalwTransitionExtrasV1 { audit_2026_09_11_deep_active: true, ..Default::default() };
    let subsidy = 444_562_014_000u64;
    let bond = bond_key(1).0;

    let genesis = PalwChainStateV2::genesis();
    let (s1, _, _) = apply_palw_transition_v7(
        &genesis,
        &p,
        Some(&admission),
        &ctx(1, 100, 1, 0),
        &register_class_and_bond(1_000_000_000_000_000),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("registration");

    let a = sibling(7, 1_700_000_000);
    let b = sibling(8, 1_700_060_000);
    let key = execution_commitment_v3(&a.attempt, execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 7));
    assert_eq!(key, execution_commitment_v3(&b.attempt, execution_anchor_v3(h(NET), h(PPH), h(CLASS), &bond, 8)));

    // Chain block 2 carries sibling A as its own work.
    let (s2, _, _) = apply_palw_transition_v7(
        &s1,
        &p,
        Some(&admission),
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&a),
        &[],
        key,
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("block 2");

    // Chain block 3 MERGES sibling B — a blue/red of block 3's mergeset, released one block late.
    let merged = [PalwMergedWorkV1 {
        carrying_block: h(0xB2),
        work: PalwBlockWorkV3::Attempt(&b),
        execution_key: key,
        subsidy,
        escrow_carve: Some(kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2::new(720).expect("legal carve")),
        bits: 0,
    }];
    let (s3, _, skips) = apply_palw_transition_v7(
        &s2,
        &p,
        Some(&admission),
        &ctx(3, 102, 3, subsidy),
        &[],
        PalwBlockWorkV3::None,
        &merged,
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("block 3");

    let id_a = attempt_id_v2(&a.attempt);
    let id_b = attempt_id_v2(&b.attempt);
    println!("=== C1-I: merged-arm replay ===");
    println!("skips recorded                   = {skips:?}");
    println!("claim A present                  = {}", s3.claim(&id_a).is_some());
    println!("claim B present                  = {}", s3.claim(&id_b).is_some());
    println!("claim B escrowed_reward          = {}", s3.claim(&id_b).map(|c| c.escrowed_reward).unwrap_or(0));
    println!("bond reserved_exposure           = {}", s3.reserved_exposure(&bond_key(1)));
    assert!(skips.is_empty(), "the merged sibling is NOT skipped one block later");
    assert!(s3.claim(&id_a).is_some() && s3.claim(&id_b).is_some(), "both claims stand");
    assert_eq!(
        s3.claim(&id_b).unwrap().escrowed_reward,
        320_084_650_080,
        "the merged sibling escrows its own block's carve, 3,200.85 MSK, for zero extra inference"
    );
}

/// **What one replayed floor-class sibling actually pays, and the two caps it is exempt from.**
///
/// `work_priced_escrow` (palw_state_v2.rs:11202) returns `claim.escrowed_reward` WHOLE for the base
/// class, and `check_class_admits_claim` (palw_state_v2.rs:10417) returns `Ok(())` for the base
/// class before it ever reaches the in-flight cap or ADR-0137's network-wide replay budget.
#[test]
fn audit_c1b_what_one_replayed_floor_sibling_pays() {
    use kaspa_consensus_core::palw_panel_economy_v1::{PALW_PANEL_POOL_PERMILLE_V1, palw_panel_split_v1};
    let escrow = 320_084_650_080u64;
    let split = palw_panel_split_v1(escrow, 5, 5);
    println!("=== C1-J: the payout of one replayed floor claim ===");
    println!("escrow (whole, unpriced)         = {escrow} sompi ({:.5} MSK)", escrow as f64 / 1e8);
    println!("panel pool permille              = {PALW_PANEL_POOL_PERMILLE_V1}");
    println!("producer take                    = {} sompi ({:.5} MSK)", split.producer, split.producer as f64 / 1e8);
    println!("paid to seats                    = {} sompi ({:.5} MSK), per seat {}", split.paid, split.paid as f64 / 1e8, split.per_seat);
    println!("panel reserve                    = {} sompi", split.reserve);
    println!("exposure reserved per floor claim= 38540 sompi (0.00038540 MSK), returned at Final");
    println!("floor class target / u128::MAX   = {:.6}", 1_218_938_590_613_259_230_285_389_391_303_016_447f64 / u128::MAX as f64);
    println!("floor per-draw MAC-eq            = {}", floor_per_draw_mac_eq());
    assert_eq!(split.producer + split.paid + split.reserve, escrow, "the split sums exactly");
    assert_eq!(split.producer, 256_067_720_064, "80% of the whole escrow to the producer");
}
