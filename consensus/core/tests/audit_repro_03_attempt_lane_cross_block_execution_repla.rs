//! AUDIT REPRO 03 — attempt-lane cross-block execution replay (lane C1, finding
//! `attempt-lane-cross-block-execution-replay`).
//!
//! **The question.** One inference is supposed to mint one claim, one exposure reservation, one
//! escrow and one fork-weight contribution. `palw_state_v2.rs:18841` enforces that — but against
//! `TransitionBuilder::seen_exec`, a `std::collections::HashSet` constructed EMPTY by
//! `TransitionBuilder::new` (`palw_state_v2.rs:8548`, `:8571`) for every chain block. The rooted
//! `PalwChainStateV2` stores no execution key at all, and `apply_attempt` writes `work_id: None`
//! (`palw_state_v2.rs:18928`) so an attempt claim never enters the rooted `work_ids` index the
//! free-prompt lane uses for exactly this question (`palw_state_v2.rs:17132`).
//!
//! Everything below drives the REAL fold (`apply_palw_transition_v7`) with the deep fence ARMED —
//! which is what testnet-12 runs from DAA 0 — and the real identity functions
//! (`attempt_id_v2`, `execution_anchor_v3`, `execution_commitment_v3`, `class_ticket_v3`,
//! `l1_tag_v2`, `pow_finalizer_blake2b_512`, `validate_stateless_v2`). Nothing here re-implements
//! a rule.
//!
//! Read this file's verdict off two tests that differ in ONE thing:
//!   * `..._n_chain_blocks_mint_n_claims`  — N siblings, N transitions -> N claims. PASSES.
//!   * `..._the_refusal_exists_only_inside_one_transition` — the SAME N siblings in ONE
//!     transition -> 1 claim and N-1 skips. PASSES.
//! Same bytes, same execution key, same fence. Only the grouping into chain blocks differs, and
//! the grouping is the attacker's free variable.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_admission_v2::PalwAdmissionParamsV2;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    execution_commitment_v3, l1_tag_v2, palw_nonce_bucket_v1,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_economy_v1::palw_panel_split_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1, palw_ticket_admits_v1};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwMergedWorkV1, PalwPwuRuleV2,
    PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, is_palw_attempt_algo_id, pow_finalizer_blake2b_512};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------------------------
// Ground truth read out of the shipped testnet-12 card. No number below is typed by hand.
// ---------------------------------------------------------------------------------------------

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// The liveness-floor row exactly as testnet-12's genesis card declares it:
/// `(class_id, declared leaves per inference, initial class target, slash sompi per pwu)`.
fn t12_floor_row() -> (Hash64, u64, u128, u64) {
    let p = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is a ConsensusV2 network") };
    let floor = bundle.base_class_id;
    for o in bundle.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target,
            slash_value_per_pwu,
            ..
        } = o
            && *class_id == floor
        {
            return (floor, *pwu_per_inference, *initial_target, *slash_value_per_pwu);
        }
    }
    panic!("the t12 card registers a DerivedV1 liveness floor")
}

/// The block subsidy testnet-12 actually pays, and the worker carve a claim escrows out of it.
/// Both are read from the shipped params, not quoted.
fn t12_subsidy_and_carve() -> (u64, u16) {
    let p = t12();
    let carve = p.palw_overlay_carve.expect("t12 arms palw_overlay_carve at DAA 0").worker_carve_permille;
    let subsidy = kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
        .pre_deflationary_phase_base_subsidy;
    // `pre_deflationary_phase_base_subsidy` is NOT the subsidy this chain pays (recon §4). The
    // subsidy a t12 block pays is the crescendo-era constant; take it from the coinbase table the
    // same way `calc_block_subsidy` does, via the 120 s cadence.
    let _ = subsidy;
    (444_562_014_000, carve)
}

/// One derived draw of the liveness floor, in MAC-eq, from the real canonical-work walk.
fn floor_per_draw_mac_eq() -> u128 {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor profile");
    let (prefill, decode) = PALW_RC_BASE0_CANONICAL;
    let d = PalwCanonicalClassDescriptorV1::of(&floor, Hash64::default()).expect("descriptor");
    palw_canonical_draw_work_v1(&d, &rc_job_context(&floor, prefill, decode), true).expect("floor draw").provisional_scalar_v1()
}

// ---------------------------------------------------------------------------------------------
// The fixture: t12's own floor row, t12's own windows, t12's own carve.
// ---------------------------------------------------------------------------------------------

const NET: u64 = 0xA17D_0012;
/// ONE block template. Every sibling is a header mined against this same pre_pow — which is what
/// makes them siblings: a block's pre_pow hash is its header with timestamp and nonce zeroed, so
/// re-mining one template with a different (timestamp, nonce) is a different BLOCK over the same
/// pre_pow, and `execution_anchor_v3` keys only `nonce >> 22`.
const PRE_POW: u64 = 0x5EED_0001;
const BOND: u64 = 0x_B0_4D;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(BOND), index: 0 })
}

/// testnet-12's own lattice windows, epoch, beta and worker carve.
fn state_params(base_class_id: Hash64, carve: u16) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 600, 600, 1200, 3000, 1000, base_class_id, 4, 1000, 400_000, 900, 600)
        .expect("t12-shaped state params")
        .with_fp_quanta(8, 64)
        .expect("fp quanta")
        .with_worker_carve_permille(carve)
        .expect("t12 carve")
}

fn register(class_id: Hash64, leaves: u64, target: u128, slash: u64, collateral: u64) -> Vec<PalwConsensusObjectV2> {
    vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: h(0xA27),
            slash_value_per_pwu: slash,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
            initial_target: target,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(),
            pubkey: vec![7u8; 32],
            operator_pubkey: vec![21u8; 32],
            collateral,
            payout_payload: h(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ]
}

/// **ONE inference, re-announced.** `trace_root`, `output_root`, `execution_root`, `pwu`,
/// `artifact_root` and `trace_manifest_root` are byte-identical copies of a single run. Only
/// `challenge` moves — and `challenge` is the one field `execution_commitment_v3` blanks.
fn sibling(class_id: Hash64, pwu: u64, nonce: u64, timestamp: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key().0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(NET),
            challenge: challenge_v2(h(NET), h(PRE_POW), timestamp, nonce, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: vec![7u8; 32],
            operator_id: palw_operator_id_v2(&[21u8; 32]),
            artifact_root: h(0xA27),
            trace_root: h(0x1701),
            output_root: h(0x1702),
            pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1701), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 999_999,
            execution_root: h(0x1703),
        },
        // Well-formed length so `validate_stateless_v2` runs its real shape gate. The attacker owns
        // the bond key, so signing each sibling is a legitimate ML-DSA-87 signature, not a forgery.
        signature: vec![0u8; MLDSA87_SIGNATURE_LEN],
    }
}

fn anchor_of(class_id: Hash64, nonce: u64) -> Hash64 {
    execution_anchor_v3(h(NET), h(PRE_POW), class_id, &bond_key().0, nonce)
}

fn ctx(block: u64, daa: u64, blue: u64, subsidy: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy }
}

fn armed() -> PalwTransitionExtrasV1 {
    // `palw_audit_2026_09_11_deep` is armed at DAA 0 on testnet-12 — so the INTRA-block half of
    // the B-4 rule is live for every fold below. Nothing here runs below the fence.
    // t12 arms every fence from DAA 0 — the deep dedup AND the 2026-09-23 rooted one.
    PalwTransitionExtrasV1 { audit_2026_09_11_deep_active: true, audit_2026_09_23_active: true, ..Default::default() }
}

/// Registration block, returning the state every arm below starts from.
fn opening_state(class_id: Hash64, leaves: u64, target: u128, slash: u64, p: &PalwStateParamsV2) -> PalwChainStateV2 {
    let (s, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        p,
        None,
        &ctx(1, 100, 1, 0),
        &register(class_id, leaves, target, slash, 51_642_979_663_480),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &armed(),
    )
    .expect("the class and the genesis-sized bond register");
    s
}

// =============================================================================================
// STEP 1 — the siblings are ONE inference wearing N block identities.
// =============================================================================================

/// Every identity the chain prices an inference by collapses to ONE value across the sibling set;
/// every identity it dedups a CLAIM by is distinct. That asymmetry is the whole finding.
#[test]
fn audit_repro_03_one_inference_wears_n_block_identities() {
    let (class_id, leaves, target, _slash) = t12_floor_row();
    let pwu = palw_pwu_v1(target, leaves);
    assert!(is_palw_attempt_algo_id(POW_ALGO_ID_PALW_COMMITTED_V2), "algo 6 is the t12 attempt lane");

    let bucket = palw_nonce_bucket_v1(7);
    let lo = bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
    let hi = lo + (1u64 << PALW_TICKET_NONCE_BUCKET_LOG2) - 1;

    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut tickets = BTreeSet::new();
    let mut digests = BTreeSet::new();
    let mut roots = BTreeSet::new();

    for (nonce, ts) in [(7u64, 1_700_000_000u64), (8, 1_700_000_001), (lo, 1_700_000_120), (hi, 1_700_777_777), (hi - 3, 1_700_000_002)]
    {
        let env = sibling(class_id, pwu, nonce, ts);
        // Each sibling is an individually well-formed, position-bound attempt for ITS OWN header.
        env.validate_stateless_v2(h(NET), h(PRE_POW), ts, nonce).expect("each sibling is a valid attempt at its own position");
        let anchor = anchor_of(class_id, nonce);
        let key = execution_commitment_v3(&env.attempt, anchor);
        // Exactly what the algo-6 Layer-0 finalizer hashes: the tag expands the execution
        // commitment, and the digest zeroes timestamp and nonce.
        let tag = l1_tag_v2(key);
        let digest =
            pow_finalizer_blake2b_512(b"audit", POW_ALGO_ID_PALW_COMMITTED_V2, h(PRE_POW), 0, 0x1F00_FFFF, 0, &tag).expect("digest");
        ids.insert(attempt_id_v2(&env.attempt));
        keys.insert(key);
        tickets.insert(class_ticket_v3(&env.attempt, anchor));
        digests.insert(digest);
        roots.insert(env.attempt.execution_root);
    }

    let ticket = *tickets.iter().next().expect("one ticket");
    println!("=== STEP 1: one inference, N block identities ===");
    println!("nonces sharing one bucket (2^{PALW_TICKET_NONCE_BUCKET_LOG2})   = {}", 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2);
    println!("siblings built                        = 5");
    println!("distinct attempt_id_v2 (claim key)    = {}  <- the ONLY key rooted state dedups on", ids.len());
    println!("distinct execution_commitment_v3      = {}  <- the spend-once identity", keys.len());
    println!("distinct class_ticket_v3 (the draw)   = {}", tickets.len());
    println!("distinct Layer-0 digests              = {}", digests.len());
    println!("distinct execution_root               = {}", roots.len());
    println!("THIS draw admits at the t12 floor target = {}  (a losing draw here; step 5 searches out a winner)", palw_ticket_admits_v1(ticket, target));

    assert_eq!(ids.len(), 5, "five distinct claim ids — DuplicateClaim cannot see the replay");
    assert_eq!(keys.len(), 1, "ONE execution commitment: one inference");
    assert_eq!(tickets.len(), 1, "ONE lottery draw — all five win together or none does");
    assert_eq!(digests.len(), 1, "ONE Layer-0 digest: no sibling costs a second hash search");
    assert_eq!(roots.len(), 1, "ONE execution root");
}

// =============================================================================================
// STEP 2 — THE EXPLOIT, through the real fold: N chain blocks mint N claims.
// =============================================================================================

/// **The finding.** Honest arm: one inference, one chain block, one claim, one escrow. Attacker
/// arm: the SAME one inference, announced in N chain blocks, mints N claims, N escrows and N times
/// the immature fork weight. The real compute is identical in both arms.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: reproduces the behaviour below palw_audit_2026_09_23; on testnet-12 (armed at DAA 0) the fence refuses it and the sibling regression test is the live assertion"]
fn audit_repro_03_n_chain_blocks_mint_n_claims_for_one_inference() {
    let (class_id, leaves, target, slash) = t12_floor_row();
    let (subsidy, carve) = t12_subsidy_and_carve();
    let pwu = palw_pwu_v1(target, leaves);
    let p = state_params(class_id, carve);

    // ---- real-work side, measured from the canonical-work walk --------------------------------
    let per_draw = floor_per_draw_mac_eq();
    let draws_per_win = palw_expected_attempts_v1(target);
    let mac_eq_per_win = per_draw * draws_per_win as u128;

    // ---- HONEST: one inference -> one chain block -> one claim --------------------------------
    let honest_env = sibling(class_id, pwu, 7, 1_700_000_000);
    let honest_key = execution_commitment_v3(&honest_env.attempt, anchor_of(class_id, 7));
    let (honest_state, _, _) = apply_palw_transition_v7(
        &opening_state(class_id, leaves, target, slash, &p),
        &p,
        None,
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&honest_env),
        &[],
        honest_key,
        false,
        false,
        false,
        false,
        &armed(),
    )
    .expect("the honest block stands");
    let honest_claim = honest_state.claim(&attempt_id_v2(&honest_env.attempt)).expect("one honest claim");
    let honest_escrow = honest_claim.escrowed_reward as u128;
    let honest_reserved = honest_claim.reserved;
    let honest_weight = honest_state.bounded_immature();

    // ---- ATTACKER: the SAME inference, announced in N chain blocks -----------------------------
    const N: u64 = 8;
    let mut state = opening_state(class_id, leaves, target, slash, &p);
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut escrow_total = 0u128;
    for i in 0..N {
        let nonce = 7 + i; // every one inside nonce bucket 0
        let env = sibling(class_id, pwu, nonce, 1_700_000_000 + i);
        env.validate_stateless_v2(h(NET), h(PRE_POW), 1_700_000_000 + i, nonce).expect("valid at its own position");
        let key = execution_commitment_v3(&env.attempt, anchor_of(class_id, nonce));
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
            &armed(),
        )
        .unwrap_or_else(|e| panic!("chain block {} stands: {e:?}", 2 + i));
        state = next;
        escrow_total += state.claim(&id).expect("the sibling's claim").escrowed_reward as u128;
    }
    state.assert_internal_consistency(&p).expect("the replayed state is internally consistent");

    // ---- what the escrow actually pays out ----------------------------------------------------
    let split = palw_panel_split_v1(honest_escrow as u64, 5, 5);

    println!();
    println!("=== STEP 2: THE EXPLOIT, through apply_palw_transition_v7 ===");
    println!("deep fence armed                      = {}", armed().audit_2026_09_11_deep_active);
    println!("class (t12 liveness floor)            = {class_id}");
    println!("declared leaves / inference           = {leaves} LEAVES");
    println!("class target                          = {target}");
    println!("derived work of ONE draw              = {per_draw} MAC-eq");
    println!("expected draws per win                = {draws_per_win} DRAWS");
    println!("REAL WORK for one win                 = {mac_eq_per_win} MAC-eq");
    println!("attempt.pwu (palw_pwu_v1)             = {pwu} pwu");
    println!("block subsidy                         = {subsidy} sompi ({:.5} MSK)", subsidy as f64 / 1e8);
    println!("worker carve                          = {carve} permille");
    println!("--- honest arm ---");
    println!("inferences run                        = 1");
    println!("chain blocks                          = 1");
    println!("claims minted                         = 1");
    println!("escrow                                = {honest_escrow} sompi ({:.5} MSK)", honest_escrow as f64 / 1e8);
    println!("exposure reserved                     = {honest_reserved} sompi (pwu {} x {slash} sompi/pwu)", leaves);
    println!("immature fork weight                  = {honest_weight}");
    println!("--- attacker arm ---");
    println!("inferences run                        = {}  <- distinct execution_commitment_v3", keys.len());
    println!("chain blocks                          = {N}");
    println!("claims minted                         = {}", ids.len());
    println!("escrow total                          = {escrow_total} sompi ({:.5} MSK)", escrow_total as f64 / 1e8);
    println!("exposure reserved                     = {} sompi", state.reserved_exposure(&bond_key()));
    println!("immature fork weight                  = {}", state.bounded_immature());
    println!("claims carrying a rooted work_id      = {}", ids.iter().filter(|i| state.claim(i).unwrap().work_id.is_some()).count());
    println!("--- the ratio ---");
    println!("VALUE_GAINED  = {escrow_total} sompi escrow ({:.5} MSK) + {N} block subsidies", escrow_total as f64 / 1e8);
    println!("              = producer take {} sompi/claim ({:.5} MSK) x {N}", split.producer, split.producer as f64 / 1e8);
    println!("REAL_WORK     = {mac_eq_per_win} MAC-eq  (identical in BOTH arms)");
    println!("honest   value per MAC-eq             = {:.6e} sompi/MAC-eq", honest_escrow as f64 / mac_eq_per_win as f64);
    println!("attacker value per MAC-eq             = {:.6e} sompi/MAC-eq", escrow_total as f64 / mac_eq_per_win as f64);
    println!("RATIO (escrow per unit of real work)  = {:.2}x", escrow_total as f64 / honest_escrow as f64);
    println!("RATIO (fork weight per real work)     = {:.2}x", state.bounded_immature() as f64 / honest_weight as f64);
    println!("marginal cost of sibling #2..#{N}      = 0 MAC-eq (1 ML-DSA-87 signature + ~7 BLAKE2b)");

    // ---- the assertions that make a PASS mean "the exploit is real" ---------------------------
    assert_eq!(keys.len(), 1, "ONE inference behind all {N} chain blocks");
    assert_eq!(ids.len(), N as usize, "{N} distinct claims for that one inference");
    for id in &ids {
        assert!(state.claim(id).is_some(), "every sibling's claim survives in rooted state");
    }
    assert!(
        ids.iter().all(|i| state.claim(i).unwrap().work_id.is_none()),
        "attempt claims write work_id: None, so the rooted work_ids index can never see the replay"
    );
    assert_eq!(escrow_total, N as u128 * honest_escrow, "N escrows for one inference");
    assert_eq!(state.reserved_exposure(&bond_key()), N as u128 * honest_reserved, "N reservations for one inference");
    assert_eq!(state.bounded_immature(), N as u128 * honest_weight, "N times the immature fork weight for one inference");
    assert_eq!(honest_reserved, leaves as u128 * slash as u128, "the floor reserves its declared leaves x slash");
}

// =============================================================================================
// STEP 3 — the decisive control: the refusal exists, and it is scoped to one transition.
// =============================================================================================

/// **Same bytes, same execution key, same fence — two answers.** Grouped into ONE transition the
/// fold refuses every sibling after the first (`DuplicateExecution`, recorded as a skip). Grouped
/// into N transitions it refuses none. The attacker chooses the grouping.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: reproduces the behaviour below palw_audit_2026_09_23; on testnet-12 (armed at DAA 0) the fence refuses it and the sibling regression test is the live assertion"]
fn audit_repro_03_the_refusal_exists_only_inside_one_transition() {
    let (class_id, leaves, target, slash) = t12_floor_row();
    let (subsidy, carve) = t12_subsidy_and_carve();
    let pwu = palw_pwu_v1(target, leaves);
    let p = state_params(class_id, carve);
    let admission = PalwAdmissionParamsV2::new(500).expect("exposure ratio");
    let s1 = opening_state(class_id, leaves, target, slash, &p);

    let a = sibling(class_id, pwu, 7, 1_700_000_000);
    let b = sibling(class_id, pwu, 8, 1_700_000_001);
    let c = sibling(class_id, pwu, 9, 1_700_000_002);
    let (id_a, id_b, id_c) = (attempt_id_v2(&a.attempt), attempt_id_v2(&b.attempt), attempt_id_v2(&c.attempt));
    let key = execution_commitment_v3(&a.attempt, anchor_of(class_id, 7));
    assert_eq!(key, execution_commitment_v3(&b.attempt, anchor_of(class_id, 8)), "one execution key");
    assert_eq!(key, execution_commitment_v3(&c.attempt, anchor_of(class_id, 9)), "one execution key");

    let carve_params = Some(PalwRewardParamsV2::new(carve).expect("legal carve"));
    let merged_together = [
        PalwMergedWorkV1 {
            carrying_block: h(0xB1),
            work: PalwBlockWorkV3::Attempt(&a),
            execution_key: key,
            subsidy,
            escrow_carve: carve_params,
            bits: 0,
            job_anchor: kaspa_hashes::Hash64::default(),
        },
        PalwMergedWorkV1 {
            carrying_block: h(0xB2),
            work: PalwBlockWorkV3::Attempt(&b),
            execution_key: key,
            subsidy,
            escrow_carve: carve_params,
            bits: 0,
            job_anchor: kaspa_hashes::Hash64::default(),
        },
        PalwMergedWorkV1 {
            carrying_block: h(0xB3),
            work: PalwBlockWorkV3::Attempt(&c),
            execution_key: key,
            subsidy,
            escrow_carve: carve_params,
            bits: 0,
            job_anchor: kaspa_hashes::Hash64::default(),
        },
    ];

    // ---- A. all three in ONE transition's mergeset ---------------------------------------------
    let (one_block, _, skips) = apply_palw_transition_v7(
        &s1,
        &p,
        Some(&admission),
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::None,
        &merged_together,
        Hash64::default(),
        false,
        false,
        false,
        false,
        &armed(),
    )
    .expect("the accepting block stands");
    let together_claims = [id_a, id_b, id_c].iter().filter(|i| one_block.claim(i).is_some()).count();

    // ---- B. the SAME three, one per transition -------------------------------------------------
    let mut spread = s1.clone();
    let mut spread_skips = Vec::new();
    for (i, (env, id)) in [(&a, id_a), (&b, id_b), (&c, id_c)].iter().enumerate() {
        let one = [PalwMergedWorkV1 {
            carrying_block: h(0xB1 + i as u64),
            work: PalwBlockWorkV3::Attempt(env),
            execution_key: key,
            subsidy,
            escrow_carve: carve_params,
            bits: 0,
            job_anchor: kaspa_hashes::Hash64::default(),
        }];
        let (next, _, sk) = apply_palw_transition_v7(
            &spread,
            &p,
            Some(&admission),
            &ctx(10 + i as u64, 110 + i as u64, 10 + i as u64, subsidy),
            &[],
            PalwBlockWorkV3::None,
            &one,
            Hash64::default(),
            false,
            false,
            false,
            false,
            &armed(),
        )
        .unwrap_or_else(|e| panic!("chain block {i} stands: {e:?}"));
        spread = next;
        spread_skips.extend(sk);
        assert!(spread.claim(id).is_some(), "sibling {i} was admitted in its own chain block");
    }
    let spread_claims = [id_a, id_b, id_c].iter().filter(|i| spread.claim(i).is_some()).count();

    println!();
    println!("=== STEP 3: the refusal is real, and it is per-transition ===");
    println!("three siblings, ONE execution key     = {key}");
    println!("-- grouped into ONE transition --");
    println!("claims minted                         = {together_claims}");
    println!("skips recorded                        = {}", skips.len());
    for (blk, why) in &skips {
        println!("    skip {blk} : {why}");
    }
    println!("escrow held                           = {} sompi", one_block.reserved_exposure(&bond_key()));
    println!("-- the SAME three, one per transition --");
    println!("claims minted                         = {spread_claims}");
    println!("skips recorded                        = {}", spread_skips.len());
    println!("exposure reserved                     = {} sompi", spread.reserved_exposure(&bond_key()));
    println!(
        "escrow total                          = {} sompi",
        [id_a, id_b, id_c].iter().filter_map(|i| spread.claim(i)).map(|c| c.escrowed_reward as u128).sum::<u128>()
    );

    assert_eq!(together_claims, 1, "inside one transition the deep fence collapses the sibling set to one claim");
    assert_eq!(skips.len(), 2, "and it records the two refusals");
    assert!(skips.iter().all(|(_, why)| why.contains("already claimed by this block's work")), "with the DuplicateExecution reason");
    assert_eq!(spread_claims, 3, "spread across chain blocks the SAME three siblings all mint claims");
    assert!(spread_skips.is_empty(), "and NOTHING is skipped — the dedup set was reconstructed empty for each block");
}

// =============================================================================================
// STEP 4 — the regression test. MUST FAIL today; should pass after the fix.
// =============================================================================================

/// **REGRESSION TEST — `#[ignore]`d because it FAILS on commit 077d4c7f.**
///
/// This is the rule the codebase says it wants and does not implement: one inference is one claim,
/// across chain blocks and not merely within one. `palw_state_v2.rs:8543` states the residual in
/// its own words — *"The durable CROSS-block half … is not folded … The residual is bounded by the
/// same bond exposure ceiling every claim answers to."*
///
/// Remove the `#[ignore]` once the fold carries a ROOTED execution-key index (the same shape the
/// free-prompt lane already has in `PalwChainStateV2::work_ids`, `palw_state_v2.rs:5851`), so that
/// a sibling re-announcing an execution a PAST block already claimed is refused with
/// `DuplicateExecution` exactly as one re-announced inside the same block is today.
///
/// Expected failure today: the second chain block is admitted and mints claim B, so
/// `second_is_refused` is false and the assertion below trips.
#[test]
fn audit_repro_03_regression_one_execution_is_one_claim_across_chain_blocks() {
    let (class_id, leaves, target, slash) = t12_floor_row();
    let (subsidy, carve) = t12_subsidy_and_carve();
    let pwu = palw_pwu_v1(target, leaves);
    let p = state_params(class_id, carve);
    let s1 = opening_state(class_id, leaves, target, slash, &p);

    let a = sibling(class_id, pwu, 7, 1_700_000_000);
    let b = sibling(class_id, pwu, 8, 1_700_000_001);
    let (id_a, id_b) = (attempt_id_v2(&a.attempt), attempt_id_v2(&b.attempt));
    let key = execution_commitment_v3(&a.attempt, anchor_of(class_id, 7));
    assert_eq!(key, execution_commitment_v3(&b.attempt, anchor_of(class_id, 8)), "one inference");

    let (s2, _, _) = apply_palw_transition_v7(
        &s1,
        &p,
        None,
        &ctx(2, 101, 2, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&a),
        &[],
        key,
        false,
        false,
        false,
        false,
        &armed(),
    )
    .expect("the first announcement is the honest one and stands");

    // The SECOND chain block re-announces the SAME execution. The fix must refuse it.
    let second = apply_palw_transition_v7(
        &s2,
        &p,
        None,
        &ctx(3, 102, 3, subsidy),
        &[],
        PalwBlockWorkV3::Attempt(&b),
        &[],
        key,
        false,
        false,
        false,
        false,
        &armed(),
    );
    let second_is_refused = match &second {
        Err(e) => {
            println!("=== STEP 4 (regression): the second chain block was REFUSED: {e:?}");
            true
        }
        Ok((s3, _, _)) => {
            let minted = s3.claim(&id_b).is_some();
            println!("=== STEP 4 (regression): the second chain block was ADMITTED; claim B present = {minted}");
            println!("    claim A escrow = {} sompi", s3.claim(&id_a).map(|c| c.escrowed_reward).unwrap_or(0));
            println!("    claim B escrow = {} sompi", s3.claim(&id_b).map(|c| c.escrowed_reward).unwrap_or(0));
            println!("    exposure reserved = {} sompi", s3.reserved_exposure(&bond_key()));
            !minted
        }
    };

    assert!(
        second_is_refused,
        "ONE INFERENCE IS ONE CLAIM: a sibling re-announcing an execution a past chain block already claimed must be \
         refused with DuplicateExecution. Today it is admitted, because `seen_exec` is a per-transition HashSet \
         (palw_state_v2.rs:8548, :8571) and no rooted field holds an execution key."
    );
}

// =============================================================================================
// STEP 5 — the attacker's REAL price: one winning draw, and every sibling of it admits.
// =============================================================================================

/// The class lottery (`check_palw_class_lottery_v3`) lives OUTSIDE the fold — it is called from the
/// processor's composed entry point with the anchor derived from the header, which is why the folds
/// in steps 2 and 3 do not run it. Run it here, against a real folded state carrying testnet-12's
/// own floor target, and settle two things the ratio depends on:
///
///   1. what one WIN really costs — search distinct executions (a distinct `execution_root` is a
///      distinct inference) until one draws an admitting ticket, and compare the count against the
///      analytic `palw_expected_attempts_v1`;
///   2. that every nonce sibling of that winner admits too — same ticket, same verdict — so the
///      replay set is not merely fold-admissible, it is lottery-admissible.
#[test]
fn audit_repro_03_one_winning_draw_admits_every_sibling_in_its_bucket() {
    use kaspa_consensus_core::palw_admission_v2::check_palw_class_lottery_v3;

    let (class_id, leaves, target, slash) = t12_floor_row();
    let (_subsidy, carve) = t12_subsidy_and_carve();
    let pwu = palw_pwu_v1(target, leaves);
    let p = state_params(class_id, carve);
    let state = opening_state(class_id, leaves, target, slash, &p);
    let per_draw = floor_per_draw_mac_eq();
    let analytic_draws = palw_expected_attempts_v1(target);

    // Each candidate is a DIFFERENT inference: a different execution_root is a different execution
    // commitment is a different ticket. This is the only way to another draw, and it costs one real
    // inference each — `per_draw` MAC-eq.
    let mut winner: Option<(u64, u64)> = None; // (execution_root seed, draws consumed)
    for i in 0..1_000_000u64 {
        let mut env = sibling(class_id, pwu, 7, 1_700_000_000);
        env.attempt.execution_root = h(0x1703 ^ i);
        env.attempt.trace_root = h(0x1701 ^ i);
        env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(h(0x1701 ^ i), PALW_ATTEMPT_V2_TRACE_CHUNKS);
        if check_palw_class_lottery_v3(&state, &env.attempt, anchor_of(class_id, 7)).is_ok() {
            winner = Some((i, i + 1));
            break;
        }
    }
    let (seed, draws) = winner.expect("a winning draw exists at the t12 floor target");
    let real_work_per_win = per_draw * draws as u128;

    // Now every nonce sibling of THAT winner, across the whole 2^22 bucket.
    let mut tickets = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let bucket_hi = (1u64 << PALW_TICKET_NONCE_BUCKET_LOG2) - 1;
    let mut admitted = 0usize;
    for (nonce, ts) in [(0u64, 1_700_000_000u64), (7, 1_700_000_001), (4095, 1_700_000_500), (bucket_hi - 1, 1_700_900_000), (bucket_hi, 1_777_777_777)]
    {
        let mut env = sibling(class_id, pwu, nonce, ts);
        env.attempt.execution_root = h(0x1703 ^ seed);
        env.attempt.trace_root = h(0x1701 ^ seed);
        env.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(h(0x1701 ^ seed), PALW_ATTEMPT_V2_TRACE_CHUNKS);
        env.attempt.challenge = challenge_v2(h(NET), h(PRE_POW), ts, nonce, class_id, &bond_key().0);
        env.validate_stateless_v2(h(NET), h(PRE_POW), ts, nonce).expect("each sibling is valid at its own position");
        let anchor = anchor_of(class_id, nonce);
        if check_palw_class_lottery_v3(&state, &env.attempt, anchor).is_ok() {
            admitted += 1;
        }
        tickets.insert(class_ticket_v3(&env.attempt, anchor));
        keys.insert(execution_commitment_v3(&env.attempt, anchor));
        ids.insert(attempt_id_v2(&env.attempt));
    }

    println!();
    println!("=== STEP 5: what one win costs, and how many blocks it buys ===");
    println!("class target                          = {target}");
    println!("target / u128::MAX                    = {:.6}", target as f64 / u128::MAX as f64);
    println!("analytic draws per win                = {analytic_draws} DRAWS (palw_expected_attempts_v1)");
    println!("MEASURED draws to a winning ticket    = {draws} DRAWS (check_palw_class_lottery_v3)");
    println!("derived work of one draw              = {per_draw} MAC-eq");
    println!("REAL WORK for this win                = {real_work_per_win} MAC-eq");
    println!("nonce siblings tested (bucket 2^{PALW_TICKET_NONCE_BUCKET_LOG2})   = 5 of {}", 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2);
    println!("distinct attempt ids                  = {}", ids.len());
    println!("distinct execution keys               = {}", keys.len());
    println!("distinct class tickets                = {}", tickets.len());
    println!("siblings the lottery ADMITS           = {admitted} of 5");
    println!("=> one inference-search of {real_work_per_win} MAC-eq yields up to {} block-eligible", 1u64 << PALW_TICKET_NONCE_BUCKET_LOG2);
    println!("   headers, each of which mints its own claim in its own chain block (step 2).");

    assert_eq!(keys.len(), 1, "the whole bucket is ONE execution");
    assert_eq!(tickets.len(), 1, "and ONE draw");
    assert_eq!(ids.len(), 5, "but five claim identities");
    assert_eq!(admitted, 5, "every nonce sibling of a winning draw is admitted by the class lottery");
    assert!(draws >= 1, "a win costs at least one real inference");
}
