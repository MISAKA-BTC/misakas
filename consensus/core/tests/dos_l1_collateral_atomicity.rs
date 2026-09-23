//! DoS lane L1 (areas 1 + 4): collateral atomicity and claim flooding, folded through the REAL
//! transition (`apply_palw_transition_v7`) with testnet-12's own state params, admission params,
//! floor class row and bond size. No rule is re-implemented; every number is printed.
//!
//! Questions settled here:
//!   Q1  merged attempts from parallel blocks against one bond: re-checked against the RUNNING
//!       reservation? (expected: yes — `palw_state_v2.rs` step 4b re-runs admission on live state)
//!   Q2  the chain block's OWN attempt: the processor admits it against the PARENT state
//!       (`processor.rs:1891` passes `state`, not the step-3 folded state) and `apply_attempt`
//!       re-checks no ceiling. Objects folded at step 3 that raise the SAME bond's
//!       `reserved_exposure` (PanelBound seat duties, free-prompt commitments, accusations) are
//!       therefore invisible to the own attempt's ceiling.
//!   Q3  seat locks (`slashable_locks`) vs the exposure ledger: does either ceiling see the other?
//!   Q4  retire-after-draw: a seat drawn while Active retires with no live lock, then signs Valid;
//!       the lock lands on a Retiring bond. The 09-23 withdrawal predicate (`v3`) never read locks;
//!       since `6bb8c844` (fix #1) the gate is `v4`, which also locks a bond that still backs
//!       seat duty or a lock live on the escaped clocks — Q4/Q4b assert that, the record keeps the
//!       audit's measurement.
//!   Q5  reorg: every write above reverted exactly by the delta.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_SETTLED_ANCHOR_DEPTH, Params, palw_v2_bond_withdrawal_delay_at_v1};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionParamsV2, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwConsensusObjectV2, PalwMergedWorkV1,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateDeltaV2, PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_bond_backs_live_duty_v1, palw_bond_collateral_is_locked_v3, palw_bond_collateral_is_locked_v4, palw_operator_id_v2,
    palw_second_clock_depth_v1, revert_delta_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---------------------------------------------------------------------------------------------
// testnet-12 ground truth
// ---------------------------------------------------------------------------------------------

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

/// `(class_id, declared leaves, initial target, slash per pwu)` of t12's liveness floor.
fn floor_row(b: &PalwConsensusParamsV2) -> (Hash64, u64, u128, u64) {
    for o in b.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target,
            slash_value_per_pwu,
            ..
        } = o
            && *class_id == b.base_class_id
        {
            return (*class_id, *pwu_per_inference, *initial_target, *slash_value_per_pwu);
        }
    }
    panic!("t12 registers a DerivedV1 floor")
}

const NET: u64 = 0xD05_0012;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
}

fn pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(1); 32]
}

fn op_pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(101); 32]
}

fn register_bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_pubkey(n),
        collateral,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// One distinct inference (`seed` moves every root) by bond `n`.
fn attempt(class_id: Hash64, pwu: u64, n: u64, seed: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key(n).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(NET),
            challenge: challenge_v2(h(NET), h(0x5EED_0000 + seed), 1_700_000_000 + seed, seed, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: pubkey(n),
            operator_id: palw_operator_id_v2(&op_pubkey(n)),
            artifact_root: h(0xA27),
            trace_root: h(0x1700_0000 + seed),
            output_root: h(0x2700_0000 + seed),
            pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1700_0000 + seed), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 9_999_999,
            execution_root: h(0x3700_0000 + seed),
        },
        signature: vec![0u8; MLDSA87_SIGNATURE_LEN],
    }
}

fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(0xC000_0000 + block), daa_score: daa, blue_score: blue, subsidy: 0 }
}

fn armed() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        panel_economy_active: true,
        objective_offence_daa: Some(0),
        settled_anchor_depth: Some(PALW_T12_SETTLED_ANCHOR_DEPTH),
        ..Default::default()
    }
}

fn fences() -> PalwEpochBudgetFencesV1 {
    PalwEpochBudgetFencesV1 { audit_2026_09_23_active: true, ..Default::default() }
}

struct Fold<'a> {
    p: &'a PalwStateParamsV2,
    admission: &'a PalwAdmissionParamsV2,
    extras: PalwTransitionExtrasV1,
}

impl Fold<'_> {
    fn go(
        &self,
        s: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        own: PalwBlockWorkV3<'_>,
        merged: &[PalwMergedWorkV1<'_>],
        own_key: Hash64,
    ) -> (PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>) {
        apply_palw_transition_v7(
            s,
            self.p,
            Some(self.admission),
            c,
            objects,
            own,
            merged,
            own_key,
            false,
            false,
            false,
            false,
            &self.extras,
        )
        .unwrap_or_else(|e| panic!("fold at daa {} refused: {e:?}", c.daa_score))
    }
}

fn merged(env: &PalwAttemptEnvelopeV2, carrying: u64, key: u64) -> PalwMergedWorkV1<'_> {
    PalwMergedWorkV1 {
        carrying_block: h(0xCB00_0000 + carrying),
        work: PalwBlockWorkV3::Attempt(env),
        execution_key: h(0xE000_0000 + key),
        subsidy: 0,
        escrow_carve: None,
        bits: 0,
    }
}

fn ceiling(collateral: u64, ratio: u32) -> u128 {
    collateral as u128 * ratio as u128 / 1000
}

/// Per-claim reservation of one floor attempt on t12's fold, read back from a real fold.
fn probe_reserved(p: &PalwStateParamsV2, a: &PalwAdmissionParamsV2, class: (Hash64, u64, u128, u64), collateral: u64) -> u128 {
    let f = Fold { p, admission: a, extras: armed() };
    let (class_id, leaves, target, slash) = class;
    let (s1, _, _) = f.go(
        &PalwChainStateV2::genesis(),
        &ctx(1, 100, 1),
        &setup(class, &[(0, collateral)]),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    let env = attempt(class_id, palw_pwu_v1(target, leaves), 0, 1);
    let (s2, _, _) = f.go(&s1, &ctx(2, 101, 2), &[], PalwBlockWorkV3::Attempt(&env), &[], h(0xE1));
    let _ = slash;
    s2.claim(&attempt_id_v2(&env.attempt)).expect("claim").reserved
}

fn setup(class: (Hash64, u64, u128, u64), bonds: &[(u64, u64)]) -> Vec<PalwConsensusObjectV2> {
    let (class_id, leaves, target, slash) = class;
    let mut v = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root: h(0xA27),
        slash_value_per_pwu: slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: target,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    for (n, c) in bonds {
        v.push(register_bond(*n, *c));
    }
    v
}

// =============================================================================================
// Q0 — the t12 numbers every other test reads
// =============================================================================================

#[test]
fn dos_l1_q0_t12_numbers() {
    let p = t12();
    let b = bundle(&p);
    let s = &b.state;
    let delay0 = palw_v2_bond_withdrawal_delay_at_v1(&b, p.palw_da_court, 0);
    println!("=== Q0: testnet-12 windows and ceilings (read from Params) ===");
    println!("window_bind              = {} DAA", s.window_bind());
    println!("window_receipt           = {} DAA", s.window_receipt());
    println!("window_challenge         = {} DAA", s.window_challenge());
    println!("window_court             = {} DAA", s.window_court());
    println!("claim_retirement_daa     = {} DAA", s.claim_retirement_daa());
    println!("epoch_length             = {} DAA", s.epoch_length());
    println!("bond withdrawal delay    = {} DAA (bundle {} + DA lattice)", delay0, b.bond.withdrawal_delay_daa());
    println!("settled anchor depth     = {PALW_T12_SETTLED_ANCHOR_DEPTH} Final attempt claims");
    println!("admission ratio          = {} permille (attempt lane ceiling)", b.admission.max_exposure_ratio_permille());
    println!("fp ratio                 = {} permille (fp + seat headroom ceiling)", s.fp_max_exposure_ratio_permille());
    println!("min_collateral_sompi     = {}", s.min_collateral_sompi());
    println!("genesis bond collateral  = {}", kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
    println!("target_time_per_block    = {} ms", p.target_time_per_block());
    let cls = floor_row(&b);
    println!("fp_certified_classes (param) = {:?}", s.fp_certified_classes().map(|c| c.len()));
    let classes: Vec<_> = b
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(*class_id),
            _ => None,
        })
        .collect();
    println!("genesis classes = {}; floor = {}", classes.len(), b.base_class_id);
    if let Some(c) = s.fp_certified_classes() {
        for id in c {
            println!("  fp-certified: {id} (floor? {})", *id == b.base_class_id);
        }
    }
    println!("floor row                = leaves {} target {} slash {}", cls.1, cls.2, cls.3);
    let r = probe_reserved(s, &b.admission, cls, kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
    println!("floor claim reserved     = {r} sompi = {:.6} MSK (folded, fence armed)", r as f64 / 1e8);
    let cap = ceiling(
        kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI,
        b.admission.max_exposure_ratio_permille(),
    );
    println!("genesis bond ceiling     = {cap} sompi -> {} concurrent floor claims", cap / r.max(1));
}

// =============================================================================================
// Q1 — merged parallel blocks: live re-check (expected SAFE)
// =============================================================================================

#[test]
fn dos_l1_q1_merged_parallel_attempts_are_rechecked_against_the_running_reservation() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let r = probe_reserved(sp, &b.admission, cls, kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
    let ratio = b.admission.max_exposure_ratio_permille();
    // Collateral whose ceiling holds exactly TWO claims.
    let collateral = ((2 * r * 1000) / ratio as u128) as u64 + 1;
    let collateral = collateral.max(sp.min_collateral_sompi());
    let k = ceiling(collateral, ratio) / r;
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    let (s1, _, _) = f.go(
        &PalwChainStateV2::genesis(),
        &ctx(1, 100, 1),
        &setup(cls, &[(1, collateral)]),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    let pwu = palw_pwu_v1(target, leaves);
    let envs: Vec<_> = (0..(k + 3)).map(|i| attempt(class_id, pwu, 1, 100 + i as u64)).collect();
    // Every one passes the processor's per-blue pre-check against the SAME parent.
    let c2 = ctx(2, 101, 2);
    let mut prechecked = 0;
    for e in &envs {
        if check_palw_attempt_admission_v2(&s1, sp, &b.admission, &c2, e, fences()).is_ok() {
            prechecked += 1;
        }
    }
    let works: Vec<_> = envs.iter().enumerate().map(|(i, e)| merged(e, i as u64, i as u64)).collect();
    let (s2, delta, skips) = f.go(&s1, &c2, &[], PalwBlockWorkV3::None, &works, Hash64::default());
    let held = s2.reserved_exposure(&bond_key(1));
    println!("=== Q1: {} parallel blues, one bond, ceiling fits {k} ===", envs.len());
    println!("collateral {collateral}  ceiling {}  per-claim {r}", ceiling(collateral, ratio));
    println!("pre-checked against parent : {prechecked}/{}", envs.len());
    println!("admitted by the fold       : {}", envs.len() - skips.len());
    println!("skipped                    : {}  (first reason: {:?})", skips.len(), skips.first().map(|s| &s.1));
    println!("reserved after fold        : {held}  <= ceiling {}", ceiling(collateral, ratio));
    assert_eq!(prechecked as usize, envs.len(), "the per-blue pre-check sees only the parent");
    assert!(held <= ceiling(collateral, ratio), "the fold re-checks merged work against the running reservation");
    assert_eq!((envs.len() - skips.len()) as u128, k);
    // Q5 on this fold: the delta reverts bit-for-bit.
    let back = revert_delta_v2(&s2, &delta, sp).expect("revert");
    assert_eq!(back, s1, "revert restores the parent exactly");
    assert_eq!(back.state_root(), s1.state_root());
}

// =============================================================================================
// Q2 — the OWN attempt is checked against the parent; step-3 objects are invisible to it
// =============================================================================================

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the own attempt checked against the parent state (finding 17); closed by b38356fe (#9, AttemptExposureCeiling on the live state) — converting it is the #9/#10 owner's"]
fn dos_l1_q2_own_attempt_overruns_the_ceiling_after_same_block_seat_duty() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let ratio = b.admission.max_exposure_ratio_permille();
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let r = probe_reserved(sp, &b.admission, cls, big);
    // Bond 1 (the executor): the registry's minimum bond (ceiling 200,000 sompi on t12).
    let small = sp.min_collateral_sompi();
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    // Bond 2 produces a claim; bonds 1,3,4,5 will be its panel.
    let bonds = [(1, small), (2, big), (3, big), (4, big), (5, big)];
    let (s1, _, _) =
        f.go(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &setup(cls, &bonds), PalwBlockWorkV3::None, &[], Hash64::default());
    let pwu = palw_pwu_v1(target, leaves);
    let other = attempt(class_id, pwu, 2, 7);
    let other_id = attempt_id_v2(&other.attempt);
    let (s2, _, _) = f.go(&s1, &ctx(2, 101, 2), &[], PalwBlockWorkV3::Attempt(&other), &[], h(0xE7));
    // Bond 1 already backs two claims of its own (each admitted in its own block).
    let mut s2 = s2;
    for (i, seed) in [(0u64, 70u64), (1, 71)] {
        let e = attempt(class_id, pwu, 1, seed);
        let c = ctx(20 + i, 101, 20 + i);
        check_palw_attempt_admission_v2(&s2, sp, &b.admission, &c, &e, fences()).expect("bond 1's earlier claims are admissible");
        s2 = f.go(&s2, &c, &[], PalwBlockWorkV3::Attempt(&e), &[], h(0xE700 + i)).0;
    }
    // Block 3: its accepted objects bind bond 2's claim to a panel that seats bond 1 (bond 1 HAD
    // headroom when the draw was validated), and its own work is bond 1's attempt.
    let seats: Vec<PalwPanelSeatV2> = [1u64, 3, 4, 5]
        .iter()
        .map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) })
        .collect();
    let panel = PalwConsensusObjectV2::PanelBound { claim: other_id, anchor: h(0xA2C), seats };
    let own = attempt(class_id, pwu, 1, 8);
    let c3 = ctx(30, 102, 30);
    // The processor's pre-check: against the PARENT (`processor.rs:1891` passes `state`).
    let pre = check_palw_attempt_admission_v2(&s2, sp, &b.admission, &c3, &own, fences());
    let (s3, delta, _) = f.go(&s2, &c3, &[panel.clone()], PalwBlockWorkV3::Attempt(&own), &[], h(0xE8));
    let held = s3.reserved_exposure(&bond_key(1));
    let cap = ceiling(small, ratio);
    // Contrast: the SAME attempt carried as MERGED work in the same block is refused.
    let (s3m, _, skips) = f.go(&s2, &c3, &[panel], PalwBlockWorkV3::None, &[merged(&own, 9, 9)], Hash64::default());
    println!("=== Q2: own attempt vs same-block seat duty on the same bond ===");
    println!("bond 1 collateral {small}  ceiling {cap}  own claim reserves {r}");
    println!("bond 1 reserved in parent      : {}", s2.reserved_exposure(&bond_key(1)));
    println!("after step-3 seat duty (merged): {}", s3m.reserved_exposure(&bond_key(1)));
    println!("pre-check vs parent            : {:?}", pre.as_ref().map(|_| "Ok"));
    println!("reserved after own-work fold   : {held}  ({} over the ceiling)", held.saturating_sub(cap));
    println!("same attempt as MERGED work    : skipped = {} ({:?})", skips.len(), skips.first().map(|s| &s.1));
    assert!(pre.is_ok(), "the processor admits the own attempt against the parent");
    assert!(held > cap, "the own attempt is folded without re-checking the ceiling the objects just consumed");
    assert_eq!(skips.len(), 1, "the merged path re-checks and refuses the identical attempt");
    let back = revert_delta_v2(&s3, &delta, sp).expect("revert");
    assert_eq!(back, s2, "revert is exact");
}

// =============================================================================================
// Q3 — seat locks and the exposure ledger never see each other
// =============================================================================================

#[test]
fn dos_l1_q3_one_collateral_backs_a_full_exposure_ceiling_and_a_full_lock_ledger() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let ratio = b.admission.max_exposure_ratio_permille();
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    let bonds = [(1, big), (2, big), (3, big), (4, big), (5, big)];
    let (mut s, _, _) =
        f.go(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &setup(cls, &bonds), PalwBlockWorkV3::None, &[], Hash64::default());
    let pwu = palw_pwu_v1(target, leaves);
    let mut daa = 101u64;
    let mut blue = 2u64;
    // Bond 2 produces claims; bond 1 sits on every panel and signs Valid, taking a lock each time.
    let mut locked_claims = 0u64;
    for i in 0..6u64 {
        let env = attempt(class_id, pwu, 2, 1000 + i);
        let id = attempt_id_v2(&env.attempt);
        let (s_a, _, _) = f.go(&s, &ctx(10 + 3 * i, daa, blue), &[], PalwBlockWorkV3::Attempt(&env), &[], h(0xF000 + i));
        daa += 1;
        blue += 1;
        let seats: Vec<PalwPanelSeatV2> = [1u64, 3, 4, 5]
            .iter()
            .map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) })
            .collect();
        let (s_b, _, _) = f.go(
            &s_a,
            &ctx(11 + 3 * i, daa, blue),
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA2C), seats }],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
        );
        daa += 1;
        blue += 1;
        let receipts: Vec<PalwSeatReceiptV2> = [1u64, 3, 4, 5]
            .iter()
            .map(|n| PalwSeatReceiptV2 {
                claim: id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond_key(*n),
                signed_daa: daa,
                signature: vec![],
            })
            .collect();
        let (s_c, _, _) = f.go(
            &s_b,
            &ctx(12 + 3 * i, daa, blue),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
        );
        daa += 1;
        blue += 1;
        if s_c.slashable_lock(bond_key(1), id).is_some() {
            locked_claims += 1;
        }
        s = s_c;
    }
    let depth = Some(PALW_T12_SETTLED_ANCHOR_DEPTH);
    let lock_total: u128 = s.slashable_available_v2(&bond_key(1), daa, depth);
    let lock_used = (big as u128).saturating_sub(lock_total);
    let per_lock = if locked_claims > 0 { lock_used / locked_claims as u128 } else { 0 };
    // Now bond 1 produces its OWN claims up to the attempt ceiling. Admission never reads locks.
    let r = probe_reserved(sp, &b.admission, cls, big);
    let mut own = 0u64;
    loop {
        let env = attempt(class_id, pwu, 1, 5000 + own);
        let c = ctx(100 + own, daa, blue);
        if check_palw_attempt_admission_v2(&s, sp, &b.admission, &c, &env, fences()).is_err() || own > 5 {
            break;
        }
        let (n, _, _) = f.go(&s, &c, &[], PalwBlockWorkV3::Attempt(&env), &[], h(0xF100 + own));
        s = n;
        daa += 1;
        blue += 1;
        own += 1;
    }
    println!("=== Q3: two ceilings on one bond, folded ===");
    println!("bond 1 collateral                 = {big}");
    println!("Valid locks taken (6 panels)      = {locked_claims}, {per_lock} sompi each, {lock_used} total");
    println!("slashable_available_v2 after locks= {lock_total}  (collateral - locks; exposure ledger NOT subtracted)");
    println!(
        "own claims admitted afterwards    = {own} x {r} = {} reserved (admission reads no lock)",
        s.reserved_exposure(&bond_key(1))
    );
    println!("exposure ceiling                  = {}", ceiling(big, ratio));
    println!("bond now answers for             = {} sompi against {} posted", lock_used + s.reserved_exposure(&bond_key(1)), big);
    // Analytic bound at t12 sizes: locks may reach the whole collateral, exposure 500‰ of it.
    println!("structural max obligation         = collateral x (1 + {ratio}/1000) = {}", big as u128 + ceiling(big, ratio));
    assert!(locked_claims > 0, "the fold wrote Valid locks on bond 1");
    assert!(own > 0, "bond 1 still produced its own claims after locking");
    let seat_part = s.reserved_exposure(&bond_key(1));
    let avail = s.slashable_available_v2(&bond_key(1), daa, depth);
    assert_eq!(
        avail, lock_total,
        "producing claims did not reduce the lock headroom: the lock ledger is blind to reserved_exposure ({seat_part})"
    );
}

// =============================================================================================
// Q4 — retire after the draw, lock on a Retiring bond: the withdrawal gate reads live duty
// =============================================================================================

/// The withdrawal gate exactly as `palw_v2_locked_bond_outpoints` reads it past the fence: the
/// second clock's depth after the liveness escape (`palw_second_clock_depth_at`), then `v3` (the
/// bond's own status and clocks) and `v4` with the duty gate on. Returns `(depth, v3, v4)`.
fn withdrawal_gate(
    s: &PalwChainStateV2,
    sp: &PalwStateParamsV2,
    bond: PalwBondKeyV2,
    now: u64,
    delay: u64,
) -> (Option<u64>, bool, bool) {
    let depth = palw_second_clock_depth_v1(Some(PALW_T12_SETTLED_ANCHOR_DEPTH), s.recent_anchor_daas(), now, sp.window_court());
    let record = s.bond(&bond).expect("bond");
    let v3 = palw_bond_collateral_is_locked_v3(record, now, delay, s.settled_attempt_finals(), depth);
    let v4 = palw_bond_collateral_is_locked_v4(s, &bond, record, now, delay, depth, true);
    (depth, v3, v4)
}

/// Is `seat`'s lock on `claim` live on the ESCAPED clocks at `now` — the liability the gate must
/// not let the collateral outrun?
fn lock_live_escaped(s: &PalwChainStateV2, sp: &PalwStateParamsV2, seat: PalwBondKeyV2, claim: Hash64, now: u64) -> bool {
    let depth = palw_second_clock_depth_v1(Some(PALW_T12_SETTLED_ANCHOR_DEPTH), s.recent_anchor_daas(), now, sp.window_court());
    s.slashable_lock(seat, claim).is_some_and(|lock| lock.is_live_v2(now, s.settled_attempt_finals(), depth))
}

/// **Q4 (fix #1).** A seat drawn onto a panel still retires before it signs — the fix chose the
/// withdrawal gate, not the retire gate — and its `Valid` still writes a lock onto the `Retiring`
/// bond. What changed is the gate: `palw_bond_collateral_is_locked_v4` reads what the bond still
/// stands behind. While the seat holds panel duty (a reservation) its collateral is locked whatever
/// its clocks say, and after the `Final` it is locked for as long as its lock is live on the
/// escaped clocks — checked on every block of a heartbeat-only history across the lock's DAA
/// expiry, the liveness escape and the withdrawal delay.
#[test]
fn dos_l1_q4_a_seat_that_retires_between_draw_and_receipt_stays_locked_while_it_backs_duty() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    let bonds = [(1, big), (2, big), (3, big), (4, big), (5, big)];
    let (s1, _, _) =
        f.go(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &setup(cls, &bonds), PalwBlockWorkV3::None, &[], Hash64::default());
    let pwu = palw_pwu_v1(target, leaves);
    let env = attempt(class_id, pwu, 2, 77);
    let id = attempt_id_v2(&env.attempt);
    let (s2, _, _) = f.go(&s1, &ctx(2, 101, 2), &[], PalwBlockWorkV3::Attempt(&env), &[], h(0xE77));
    let seats: Vec<PalwPanelSeatV2> = [1u64, 3, 4, 5]
        .iter()
        .map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) })
        .collect();
    let (s3, _, _) = f.go(
        &s2,
        &ctx(3, 102, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA2C), seats }],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    // Seat bond 1 retires: it holds NO live lock yet (it has not signed), so the fold accepts it.
    let retire = PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1), signature: vec![] };
    let (s4, d4, _) = f.go(&s3, &ctx(4, 103, 4), &[retire], PalwBlockWorkV3::None, &[], Hash64::default());
    let status = s4.bond(&bond_key(1)).unwrap().status.clone();
    let PalwBondStatusV2::Retiring { since_daa, .. } = status else {
        panic!("retirement is accepted while the bond sits on a bound panel")
    };
    let delay = palw_v2_bond_withdrawal_delay_at_v1(&b, p.palw_da_court, since_daa);
    // While it sits on the panel the seat carries duty: the gate holds it at ANY DAA — here the
    // first its own clocks would release it (the delay run out, and with no anchor ever settled
    // the second clock long since escaped).
    let withdraw_daa = since_daa + delay;
    assert!(s4.reserved_exposure(&bond_key(1)) > 0, "the seat's panel duty is a reservation on its bond");
    let (depth, v3, v4) = withdrawal_gate(&s4, sp, bond_key(1), withdraw_daa, delay);
    println!("=== Q4: retire-after-draw ===");
    println!("retire accepted with seat on a bound panel: {status:?}; seat duty reserved {}", s4.reserved_exposure(&bond_key(1)));
    println!("gate at DAA {withdraw_daa} on the bound panel: depth {depth:?} v3 locked {v3} v4 locked {v4}");
    assert!(!v3, "v3 alone reads only the bond's clocks, and they have run out");
    assert!(v4, "v4: a bond that still backs seat duty is locked whatever its clocks say");
    assert!(palw_bond_backs_live_duty_v1(&s4, &bond_key(1), withdraw_daa, depth));

    // It then signs Valid, and the licence locks collateral on a bond that is already Retiring.
    let receipts: Vec<PalwSeatReceiptV2> = [1u64, 3, 4, 5]
        .iter()
        .map(|n| PalwSeatReceiptV2 {
            claim: id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(*n),
            signed_daa: 104,
            signature: vec![],
        })
        .collect();
    let (s5, _, _) = f.go(
        &s4,
        &ctx(5, 104, 5),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    assert!(s5.slashable_lock(bond_key(1), id).is_some(), "a Valid lock is written on a Retiring bond");
    assert_eq!(s5.recent_anchor_daas(), &[104], "the licence is the second clock's anchor");
    // Finalize, then heartbeat-only history across every boundary the gate reads.
    let fin_daa = 104 + sp.window_challenge_at(104) + 1;
    let (mut s, _, _) = f.go(&s5, &ctx(6, fin_daa, 6), &[], PalwBlockWorkV3::None, &[], Hash64::default());
    let lock = *s.slashable_lock(bond_key(1), id).expect("the lock survives Final as the liability");
    let escape = 104 + 2 * sp.window_court();
    let mut probes = vec![
        fin_daa + 1,
        lock.expiry_daa - 1,
        lock.expiry_daa,
        escape - 1,
        escape,
        escape + 1,
        withdraw_daa - 1,
        withdraw_daa,
        withdraw_daa + 1,
    ];
    probes.sort_unstable();
    probes.dedup();
    let mut blue = 7;
    for now in probes {
        (s, _, _) = f.go(&s, &ctx(blue, now, blue), &[], PalwBlockWorkV3::None, &[], Hash64::default());
        blue += 1;
        let live = lock_live_escaped(&s, sp, bond_key(1), id, now);
        let reserved = s.reserved_exposure(&bond_key(1));
        let (depth, v3, v4) = withdrawal_gate(&s, sp, bond_key(1), now, delay);
        println!("DAA {now}: depth {depth:?} lock live {live} reserved {reserved} v3 {v3} v4 {v4}");
        assert!(!live || v4, "DAA {now}: the collateral must stay locked while its seat lock is live on the escaped clocks");
        assert_eq!(v4, v3 || live || reserved > 0, "DAA {now}: v4 is v3 plus the bond's live duties, nothing else");
    }
    let back = revert_delta_v2(&s4, &d4, sp).expect("revert");
    assert_eq!(back, s3);
}

// =============================================================================================
// Q4b — the same, with the second clock driven by REAL licences, and every delta reverted (Q5)
// =============================================================================================

/// **Q4b (fixes #1 and #2), folded.** `depth + 1` claims are licensed between seat 1's retirement
/// and its own licence — so its retiring bond's `settled_at_since` is long satisfied — and after
/// the `Final` the lane licenses SPARSELY: three claims, far fewer than `depth`, each inside
/// `2 × window_court` of the last so the liveness escape never fires. At the first DAA the
/// withdrawal delay allows, `v3` alone releases the collateral while seat 1's liability is still
/// live on the second clock; `v4` must not. It releases only at the escape, `2 × window_court` after
/// the last licence (`E − 1` locked, `E` and `E + 1` free). Every block — licences that push and
/// prune the anchor ring included — reverts bit-for-bit.
#[test]
fn dos_l1_q4b_sparse_licences_do_not_release_a_retiring_seat_with_a_live_liability() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, _, _) = cls;
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    let depth = PALW_T12_SETTLED_ANCHOR_DEPTH;
    let bonds: Vec<(u64, u64)> = (1..=6).map(|n| (n, big)).collect();
    let seat = |ns: &[u64]| -> Vec<PalwPanelSeatV2> {
        ns.iter().map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) }).collect()
    };
    let valid = |id: Hash64, ns: &[u64], daa: u64| -> Vec<PalwSeatReceiptV2> {
        ns.iter()
            .map(|n| PalwSeatReceiptV2 {
                claim: id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond_key(*n),
                signed_daa: daa,
                signature: vec![],
            })
            .collect()
    };
    // The pwu a floor attempt declares against the class's CURRENT target.
    let pwu_now = |s: &PalwChainStateV2| palw_pwu_v1(s.class_target(&class_id).expect("the floor's target").target, leaves);
    let mut deltas: Vec<(PalwChainStateV2, PalwStateDeltaV2, PalwChainStateV2)> = Vec::new();
    let mut step = |s: &PalwChainStateV2,
                    c: PalwBlockContextV2,
                    objs: Vec<PalwConsensusObjectV2>,
                    own: Option<&PalwAttemptEnvelopeV2>,
                    key: u64| {
        let work = own.map(PalwBlockWorkV3::Attempt).unwrap_or(PalwBlockWorkV3::None);
        let (n, d, _) = f.go(s, &c, &objs, work, &[], if own.is_some() { h(0xEE00_0000 + key) } else { Hash64::default() });
        deltas.push((s.clone(), d, n.clone()));
        n
    };
    let mut s = step(&PalwChainStateV2::genesis(), ctx(1, 100, 1), setup(cls, &bonds), None, 0);
    // X: bond 2's claim, seat bond 1 on its panel.
    let x = attempt(class_id, pwu_now(&s), 2, 900);
    let xid = attempt_id_v2(&x.attempt);
    s = step(&s, ctx(2, 101, 2), vec![], Some(&x), 900);
    s = step(
        &s,
        ctx(3, 102, 3),
        vec![PalwConsensusObjectV2::PanelBound { claim: xid, anchor: h(1), seats: seat(&[1, 3, 4, 5]) }],
        None,
        0,
    );
    // Seat 1 retires: no lock yet.
    s = step(&s, ctx(4, 103, 4), vec![PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1), signature: vec![] }], None, 0);
    // depth+1 other claims by bond 6, judged by 2,3,4,5 — honest traffic that settles anchors.
    let ys: Vec<_> = (0..(depth + 1)).map(|i| attempt(class_id, pwu_now(&s), 6, 1000 + i)).collect();
    let mut blue = 5u64;
    for (i, y) in ys.iter().enumerate() {
        s = step(&s, ctx(blue, 104, blue), vec![], Some(y), 1000 + i as u64);
        blue += 1;
    }
    let panels: Vec<_> = ys
        .iter()
        .map(|y| PalwConsensusObjectV2::PanelBound { claim: attempt_id_v2(&y.attempt), anchor: h(2), seats: seat(&[2, 3, 4, 5]) })
        .collect();
    s = step(&s, ctx(blue, 105, blue), panels, None, 0);
    blue += 1;
    let lic: Vec<_> = ys
        .iter()
        .map(|y| {
            let id = attempt_id_v2(&y.attempt);
            PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid(id, &[2, 3, 4, 5], 106) }
        })
        .collect();
    s = step(&s, ctx(blue, 106, blue), lic, None, 0);
    blue += 1;
    // X licensed one DAA later.
    s = step(
        &s,
        ctx(blue, 107, blue),
        vec![PalwConsensusObjectV2::ReceiptLicensed { claim: xid, receipts: valid(xid, &[1, 3, 4, 5], 107) }],
        None,
        0,
    );
    blue += 1;
    assert_eq!(s.settled_attempt_finals(), depth + 2, "one anchor per licence");
    let fin = 107 + sp.window_challenge_at(107) + 1;
    s = step(&s, ctx(blue, fin, blue), vec![], None, 0);
    blue += 1;
    let lock = *s.slashable_lock(bond_key(1), xid).expect("seat 1's liability on X");
    assert_eq!(lock.settled_at_final, depth + 2, "the liability began at X's Final; the swept Finals settled nothing");
    let PalwBondStatusV2::Retiring { since_daa, settled_at_since } = s.bond(&bond_key(1)).unwrap().status else { panic!("retiring") };
    let delay = palw_v2_bond_withdrawal_delay_at_v1(&b, p.palw_da_court, fin);
    let withdraw_daa = since_daa + delay;
    let wc = sp.window_court();

    // Sparse licences: one claim every `2 × window_court − 1000` DAA, so the escape never fires.
    let mut at = fin;
    let mut sparse = 0u64;
    let mut nonce = 5_000u64;
    while at + 2 * wc - 1_000 < withdraw_daa {
        at += 2 * wc - 1_000;
        let z = attempt(class_id, pwu_now(&s), 6, nonce);
        let zid = attempt_id_v2(&z.attempt);
        s = step(&s, ctx(blue, at, blue), vec![], Some(&z), nonce);
        s = step(
            &s,
            ctx(blue + 1, at + 1, blue + 1),
            vec![PalwConsensusObjectV2::PanelBound { claim: zid, anchor: h(3), seats: seat(&[2, 3, 4, 5]) }],
            None,
            0,
        );
        s = step(
            &s,
            ctx(blue + 2, at + 2, blue + 2),
            vec![PalwConsensusObjectV2::ReceiptLicensed { claim: zid, receipts: valid(zid, &[2, 3, 4, 5], at + 2) }],
            None,
            0,
        );
        blue += 3;
        nonce += 1;
        sparse += 1;
    }
    let last_licence = *s.recent_anchor_daas().last().expect("the ring holds the last licence");
    assert!(sparse > 0 && sparse < depth, "{sparse} licences after X's Final: sparse");
    assert!(withdraw_daa < last_licence + 2 * wc, "the withdrawal DAA falls inside the second clock's reach");

    // The first DAA the withdrawal delay allows, on a heartbeat.
    s = step(&s, ctx(blue, withdraw_daa, blue), vec![], None, 0);
    blue += 1;
    let settled = s.settled_attempt_finals();
    let live = lock_live_escaped(&s, sp, bond_key(1), xid, withdraw_daa);
    let (clock, v3, v4) = withdrawal_gate(&s, sp, bond_key(1), withdraw_daa, delay);
    println!("=== Q4b: {} licences between retirement and liability, then {sparse} sparse ones ===", depth + 1);
    println!("retire since_daa {since_daa}, settled_at_since {settled_at_since}; delay {delay}");
    println!("seat lock on X: amount {} expiry_daa {} settled_at_final {}", lock.amount, lock.expiry_daa, lock.settled_at_final);
    println!("at DAA {withdraw_daa}: counter {settled}, last licence {last_licence}, depth {clock:?}");
    println!("  lock live {live}; v3 locked {v3}; v4 locked {v4}; ring {} entries", s.recent_anchor_daas().len());
    assert!(settled - settled_at_since >= depth, "the retiring bond's own second-clock half is satisfied");
    assert!(
        settled - lock.settled_at_final < depth,
        "…but the liability's is not: {} licences since X's Final",
        settled - lock.settled_at_final
    );
    assert!(live, "seat 1's liability on X is live on the escaped clocks");
    assert!(!v3, "v3 alone would release the collateral here — the audit's escape");
    assert!(v4, "v4 must keep it locked while the liability is live");

    // With no further licence, the liveness escape releases it — and not a DAA before.
    let e = last_licence + 2 * wc;
    for now in [e - 1, e, e + 1] {
        s = step(&s, ctx(blue, now, blue), vec![], None, 0);
        blue += 1;
        let live = lock_live_escaped(&s, sp, bond_key(1), xid, now);
        let (clock, v3, v4) = withdrawal_gate(&s, sp, bond_key(1), now, delay);
        println!("at DAA {now} (E = {e}): depth {clock:?} lock live {live} v3 {v3} v4 {v4}");
        assert!(!live || v4, "DAA {now}: never released under a live liability");
        assert_eq!(v4, now < e, "DAA {now}: locked until E, released from E");
    }
    // Q5: every block of this history reverts bit-for-bit — anchor pushes and prunes included.
    let pruned: usize = deltas
        .iter()
        .map(|(_, d, _)| {
            d.entries
                .iter()
                .filter(|e| matches!(e, kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::AnchorDaaPruned { .. }))
                .count()
        })
        .sum();
    assert!(pruned > 0, "the history prunes the anchor ring");
    for (i, (parent, d, child)) in deltas.iter().enumerate() {
        let back = revert_delta_v2(child, d, sp).unwrap_or_else(|e| panic!("revert {i}: {e:?}"));
        assert_eq!(&back, parent, "block {i} reverts exactly");
        assert_eq!(back.state_root(), parent.state_root());
    }
    println!("Q5: {} deltas ({pruned} ring prunes) revert bit-for-bit", deltas.len());
}

/// **PRE-FIX RECORD of Q4b** — the audit's measurement, kept: the 09-23 gate (`v3` alone, handed
/// the un-escaped depth) released seat 1's collateral at DAA 13003 while its liability was live.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the 09-23 withdrawal gate (palw_bond_collateral_is_locked_v3 alone, un-escaped depth) releasing a seat with a live liability (dos_l1_q4b); closed by 6bb8c844's v4 duty gate"]
fn dos_l1_q4b_record_v3_alone_outruns_the_seat_liability() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let f = Fold { p: sp, admission: &b.admission, extras: armed() };
    let depth = PALW_T12_SETTLED_ANCHOR_DEPTH;
    let bonds: Vec<(u64, u64)> = (1..=6).map(|n| (n, big)).collect();
    let pwu = palw_pwu_v1(target, leaves);
    let seat = |ns: &[u64]| -> Vec<PalwPanelSeatV2> {
        ns.iter().map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) }).collect()
    };
    let valid = |id: Hash64, ns: &[u64], daa: u64| -> Vec<PalwSeatReceiptV2> {
        ns.iter()
            .map(|n| PalwSeatReceiptV2 {
                claim: id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond_key(*n),
                signed_daa: daa,
                signature: vec![],
            })
            .collect()
    };
    let mut deltas: Vec<(PalwChainStateV2, PalwStateDeltaV2, PalwChainStateV2)> = Vec::new();
    let mut step = |s: &PalwChainStateV2,
                    c: PalwBlockContextV2,
                    objs: Vec<PalwConsensusObjectV2>,
                    own: Option<&PalwAttemptEnvelopeV2>,
                    key: u64| {
        let work = own.map(PalwBlockWorkV3::Attempt).unwrap_or(PalwBlockWorkV3::None);
        let (n, d, _) = f.go(s, &c, &objs, work, &[], if own.is_some() { h(0xEE00_0000 + key) } else { Hash64::default() });
        deltas.push((s.clone(), d, n.clone()));
        n
    };
    let mut s = step(&PalwChainStateV2::genesis(), ctx(1, 100, 1), setup(cls, &bonds), None, 0);
    // X: bond 2's claim, seat bond 1 on its panel.
    let x = attempt(class_id, pwu, 2, 900);
    let xid = attempt_id_v2(&x.attempt);
    s = step(&s, ctx(2, 101, 2), vec![], Some(&x), 900);
    s = step(
        &s,
        ctx(3, 102, 3),
        vec![PalwConsensusObjectV2::PanelBound { claim: xid, anchor: h(1), seats: seat(&[1, 3, 4, 5]) }],
        None,
        0,
    );
    // Seat 1 retires: no lock yet.
    s = step(&s, ctx(4, 103, 4), vec![PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1), signature: vec![] }], None, 0);
    // depth+1 other claims by bond 6, judged by 2,3,4,5 — honest traffic that settles anchors.
    let ys: Vec<_> = (0..(depth + 1)).map(|i| attempt(class_id, pwu, 6, 1000 + i)).collect();
    let mut blue = 5u64;
    for (i, y) in ys.iter().enumerate() {
        s = step(&s, ctx(blue, 104, blue), vec![], Some(y), 1000 + i as u64);
        blue += 1;
    }
    let panels: Vec<_> = ys
        .iter()
        .map(|y| PalwConsensusObjectV2::PanelBound { claim: attempt_id_v2(&y.attempt), anchor: h(2), seats: seat(&[2, 3, 4, 5]) })
        .collect();
    s = step(&s, ctx(blue, 105, blue), panels, None, 0);
    blue += 1;
    let lic: Vec<_> = ys
        .iter()
        .map(|y| {
            let id = attempt_id_v2(&y.attempt);
            PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid(id, &[2, 3, 4, 5], 106) }
        })
        .collect();
    s = step(&s, ctx(blue, 106, blue), lic, None, 0);
    blue += 1;
    // X licensed one DAA later, so every Y settles before X's Final.
    s = step(
        &s,
        ctx(blue, 107, blue),
        vec![PalwConsensusObjectV2::ReceiptLicensed { claim: xid, receipts: valid(xid, &[1, 3, 4, 5], 107) }],
        None,
        0,
    );
    blue += 1;
    let fin = 107 + sp.window_challenge_at(107) + 1;
    s = step(&s, ctx(blue, fin, blue), vec![], None, 0);
    let lock = *s.slashable_lock(bond_key(1), xid).expect("seat 1's liability on X");
    let rec = s.bond(&bond_key(1)).unwrap().clone();
    let PalwBondStatusV2::Retiring { since_daa, settled_at_since } = rec.status else { panic!("retiring") };
    let settled = s.settled_attempt_finals();
    let delay = palw_v2_bond_withdrawal_delay_at_v1(&b, p.palw_da_court, fin);
    // From here the attacker (or nobody at all) produces heartbeat-only history: DAA moves, no
    // anchor settles. Evaluate the production predicates at the first DAA withdrawal allows.
    let now = since_daa + delay;
    let withdrawable = !palw_bond_collateral_is_locked_v3(&rec, now, delay, settled, Some(depth));
    let live = lock.is_live_v2(now, settled, Some(depth));
    let available_now = s.slashable_available_v2(&bond_key(1), now, Some(depth));
    println!("=== Q4b: folded — {} Finals settled between seat 1's retirement and its own liability ===", depth + 1);
    println!("retire since_daa {since_daa}, settled_at_since {settled_at_since}");
    println!("seat lock on X: amount {} expiry_daa {} settled_at_final {}", lock.amount, lock.expiry_daa, lock.settled_at_final);
    println!("chain settled_attempt_finals now = {settled}");
    println!("at daa {now} (heartbeat-only after Final): withdrawable = {withdrawable}, lock live = {live}");
    println!(
        "  withdrawal needs settled >= {} (have {settled}); the liability needs settled >= {} to expire",
        settled_at_since + depth,
        lock.settled_at_final + depth
    );
    println!("  slashable_available_v2(seat 1) = {available_now} (locks still counted as live)");
    assert!(withdrawable && live, "the collateral is released while its seat liability is still live on the second clock");
    // Q5: every block of this history reverts bit-for-bit.
    for (i, (parent, d, child)) in deltas.iter().enumerate() {
        let back = revert_delta_v2(child, d, sp).unwrap_or_else(|e| panic!("revert {i}: {e:?}"));
        assert_eq!(&back, parent, "block {i} reverts exactly");
        assert_eq!(back.state_root(), parent.state_root());
    }
    println!(
        "Q5: {} deltas (retire, 31 licences, {} Finals with liability rewrites, settled clock) revert bit-for-bit",
        deltas.len(),
        depth + 2
    );
}

// =============================================================================================
// Q6 — seat duty: how much OTHER bonds' headroom one sompi of the claimant's reservation takes
// =============================================================================================

#[test]
fn dos_l1_q6_seat_duty_moves_the_claimants_reservation_onto_other_bonds_times_three_per_seat() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let seat_count = b.panel.seat_count();
    let lambda = p.palw_panel_exposure_floor_fence().map(|f| f.reward_multiple_permille).unwrap_or(0);
    let economy = p.palw_panel_economy_fence();
    let mut extras = armed();
    extras.panel_reward_multiple_permille = lambda;
    let f = Fold { p: sp, admission: &b.admission, extras };
    // Attacker bond 9 at the registry minimum; seats 1..=seat_count honest genesis-sized bonds.
    let seats_n: Vec<u64> = (1..=seat_count as u64).collect();
    let mut bonds: Vec<(u64, u64)> = seats_n.iter().map(|n| (*n, big)).collect();
    bonds.push((9, sp.min_collateral_sompi()));
    let (s1, _, _) =
        f.go(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &setup(cls, &bonds), PalwBlockWorkV3::None, &[], Hash64::default());
    let env = attempt(class_id, palw_pwu_v1(target, leaves), 9, 4242);
    let id = attempt_id_v2(&env.attempt);
    let (s2, _, _) = f.go(&s1, &ctx(2, 101, 2), &[], PalwBlockWorkV3::Attempt(&env), &[], h(0x4242));
    let seats: Vec<PalwPanelSeatV2> =
        seats_n.iter().map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) }).collect();
    let (s3, _, _) = f.go(
        &s2,
        &ctx(3, 102, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(3), seats }],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    let own = s3.reserved_exposure(&bond_key(9));
    let on_others: u128 = seats_n.iter().map(|n| s3.reserved_exposure(&bond_key(*n))).sum();
    println!(
        "=== Q6: seat duty amplification (t12 seat_count {seat_count}, lambda {lambda} permille, panel economy {:?}) ===",
        economy.map(|a| a.is_active(0))
    );
    println!("claimant reserves on its own bond      : {own}");
    println!("reserved on the {seat_count} seats' bonds (sum) : {on_others}  -> {}x the claimant's", on_others / own.max(1));
    println!(
        "released only at Final / void (receipt {} + challenge {} DAA after binding, x2 on a redraw)",
        sp.window_receipt(),
        sp.window_challenge()
    );
    let honest_ceiling = ceiling(big, b.admission.max_exposure_ratio_permille());
    println!(
        "to fill ONE honest genesis bond's ceiling ({honest_ceiling}) with duty, the claimant needs {} reserved = {} collateral at 500 permille",
        honest_ceiling / 3,
        honest_ceiling / 3 * 2
    );
    assert!(on_others >= own * 3 * seat_count as u128, "each seat reserves at least 3x the claim");
}

/// Q6 with the escrow a real t12 floor claim carries (subsidy 444,562,014,000 sompi, t12's carve):
/// the ADR-0130 floor `lambda x max_seat_reward` takes over, and the seat duty stops scaling with
/// the claimant's reservation at all.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: a registry-minimum bond's floor claim with the real escrow (finding 4); since b38356fe (#9/#10) that attempt is no longer admitted, so the record has no claim to measure — converting it is the #9/#10 owner's"]
fn dos_l1_q6b_seat_duty_with_the_real_escrow_is_priced_by_the_reward_not_the_reservation() {
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let cls = floor_row(&b);
    let (class_id, leaves, target, _) = cls;
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let seat_count = b.panel.seat_count();
    let lambda = p.palw_panel_exposure_floor_fence().map(|f| f.reward_multiple_permille).unwrap_or(0);
    let carve = p.palw_overlay_carve.expect("t12 carve").worker_carve_permille;
    let subsidy = kaspa_consensus_core::config::params::PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI;
    let mut extras = armed();
    extras.panel_reward_multiple_permille = lambda;
    extras.escrow_carve = Some(kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2::new(carve).expect("carve"));
    let f = Fold { p: sp, admission: &b.admission, extras };
    let seats_n: Vec<u64> = (1..=seat_count as u64).collect();
    let mut bonds: Vec<(u64, u64)> = seats_n.iter().map(|n| (*n, big)).collect();
    bonds.push((9, sp.min_collateral_sompi()));
    let (s1, _, _) =
        f.go(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &setup(cls, &bonds), PalwBlockWorkV3::None, &[], Hash64::default());
    let env = attempt(class_id, palw_pwu_v1(target, leaves), 9, 4343);
    let id = attempt_id_v2(&env.attempt);
    let c2 = PalwBlockContextV2 { subsidy, ..ctx(2, 101, 2) };
    let (s2, _, _) = f.go(&s1, &c2, &[], PalwBlockWorkV3::Attempt(&env), &[], h(0x4343));
    let claim = s2.claim(&id).expect("claim").clone();
    let seats: Vec<PalwPanelSeatV2> =
        seats_n.iter().map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) }).collect();
    let (s3, _, _) = f.go(
        &s2,
        &ctx(3, 102, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(3), seats }],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
    );
    let own = s3.reserved_exposure(&bond_key(9));
    let per_seat = s3.reserved_exposure(&bond_key(1));
    let on_others: u128 = seats_n.iter().map(|n| s3.reserved_exposure(&bond_key(*n))).sum();
    let honest_ceiling = ceiling(big, b.admission.max_exposure_ratio_permille());
    println!("=== Q6b: seat duty with the real escrow (carve {carve} permille, lambda {lambda}) ===");
    println!(
        "claim escrowed_reward                  : {} sompi = {:.2} MSK",
        claim.escrowed_reward,
        claim.escrowed_reward as f64 / 1e8
    );
    println!("claimant reserves on its own bond      : {own} sompi = {:.6} MSK", own as f64 / 1e8);
    println!("each seat reserves                     : {per_seat} sompi = {:.2} MSK", per_seat as f64 / 1e8);
    println!(
        "sum on {seat_count} other bonds                : {on_others} = {:.2} MSK -> {}x the claimant's own reservation",
        on_others as f64 / 1e8,
        on_others / own.max(1)
    );
    println!(
        "one honest genesis bond (ceiling {:.0} MSK) is full after {} concurrent duties",
        honest_ceiling as f64 / 1e8,
        honest_ceiling / per_seat.max(1)
    );
    println!("the claimant's registry-minimum bond (ceiling 200,000 sompi) can hold {} such claims at once", 200_000u128 / own.max(1));
    assert!(on_others > own * 1000, "the duty on other bonds is not bounded by the claimant's own reservation");
}
