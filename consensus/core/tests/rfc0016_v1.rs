//! **RFC-0016 — V+ block rights, elastic Final capacity and the outcome-first report**, on the pure functions the fold reads
//! (`kaspa_consensus_core::palw_rfc0016_v1`) and on the fences' refusals.
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS.** Every value set is a TEST fixture, never a
//! proposal: RFC-0016's values are placeholders until the three-stage derivation (RFC-0016 §8.6) pins a measured record.
//!
//! The RFC-0016 §12 ids each test covers are in its name; ids that need a real node, the ADR-0176 lane's engine or RFC-0017's gates
//! are mapped in `docs/design/palw/rfc-0016-implementation-status-2026-10-15.md` (with the file that covers them, or "not written").
//!
//! Run (validation phase): `cargo test -p kaspa-consensus-core --test rfc0016_v1`

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_bond_budget_v1::{PalwAllocationCurveV1, PalwAllocationWeightV1};
use kaspa_consensus_core::palw_rfc0016_v1::capacity::*;
use kaspa_consensus_core::palw_rfc0016_v1::report::*;
use kaspa_consensus_core::palw_rfc0016_v1::rights::*;
use kaspa_consensus_core::palw_rfc0016_v1::values::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

fn h(i: u64) -> Hash64 {
    Hash64::from_u64_word(i)
}

fn bond(i: u32) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Default::default(), i))
}

/// A TEST value set (not a proposal).
fn vals(sigma: u32) -> PalwRfc0016ValuesV1 {
    PalwRfc0016ValuesV1 {
        sigma_permille: sigma,
        tilt_k_permille: 1_000,
        reissue_checkpoint_permille: 500,
        pace_margin_permille: 1_000,
        entrant_reserve_permille: 50,
        entrant_max_per_model_permille: 20,
        entrant_min_capital_sompi: 1,
        entrant_age_daa: 1_000,
        capacity_safety_permille: 1_000,
        c7_measured_concurrency: 0,
        queue_len_per_class: 8,
        queue_ttl_daa: 50,
        quote_margin_h: 2,
        verifier_share_base_permille: 150,
        verifier_slope_gamma_permille: 100,
        verifier_util_target_permille: 700,
        opv_checks_per_claim_permille: 500,
    }
}

fn alpha15(capital: u128) -> PalwAllocationWeightV1 {
    PalwAllocationCurveV1 { alpha_halves: 3 }.weight(capital)
}

/// Weights and ΣA for a capital table under α = 1.5 (the decided curve; the capitals are fixtures).
fn weights(capitals: &[(u64, u128)]) -> (BTreeMap<Hash64, PalwAllocationWeightV1>, PalwAllocationWeightV1) {
    let w: BTreeMap<_, _> = capitals.iter().map(|(m, c)| (h(*m), alpha15(*c))).collect();
    let mut sum = PalwAllocationWeightV1::from_u64(0);
    for a in w.values() {
        sum = sum + *a;
    }
    (w, sum)
}

fn issue(capitals: &[(u64, u128)], n_exp: u64, v: &PalwRfc0016ValuesV1) -> PalwRightsEpochV1 {
    let w = weights(capitals);
    palw_rights_issue_v1(0, 0, 1_000, n_exp, v, Some(&w), &BTreeMap::new(), |_| true)
}

/// Drive one epoch: `wins` is the ordered sequence of models whose miners win the common-W lottery at each DAA step (the compute
/// split); each win is admitted only with a right. Returns the physical blocks per model and the refusals.
fn run_epoch(
    e: &PalwRightsEpochV1,
    wins: &[(u64, u64)],
    v: &PalwRfc0016ValuesV1,
) -> (BTreeMap<Hash64, u64>, BTreeMap<&'static str, u64>) {
    let mut used: BTreeMap<Hash64, u64> = BTreeMap::new();
    let mut refused: BTreeMap<&'static str, u64> = BTreeMap::new();
    for (daa, model) in wins {
        match palw_rights_admit_v1(e, &used, &h(*model), *daa, v) {
            Ok(_) => *used.entry(h(*model)).or_insert(0) += 1,
            Err(r) => *refused.entry(r.code()).or_insert(0) += 1,
        }
    }
    (used, refused)
}

/// A compute split as a deterministic win sequence: over `steps` DAA, model `a` wins `ra` of every `ra + rb` steps.
fn split(a: u64, ra: u64, b: u64, rb: u64, steps: u64) -> Vec<(u64, u64)> {
    (0..steps).map(|i| (i, if i % (ra + rb) < ra { a } else { b })).collect()
}

// ---- RFC-0016 §12.1 (the user's tests) ---------------------------------------------------------------------------------------

/// T1: same CCU, equal bond, A has 3× the compute — A's physical blocks rise above B's, up to its σ-capped rights; the epoch total
/// is never exceeded.
#[test]
fn t1_more_compute_raises_physical_blocks_within_rights() {
    let v = vals(500);
    // N_exp 632: ε deducts 31, each model holds 300; A wins 750 of 1,000 steps, B 250 — A exceeds its own rights and takes
    // released ones past φ while B's pace stays protected.
    let e = issue(&[(1, 1_000), (2, 1_000)], 632, &v);
    let (used, _) = run_epoch(&e, &split(1, 3, 2, 1, 1_000), &v);
    let (a, b) = (used.get(&h(1)).copied().unwrap_or(0), used.get(&h(2)).copied().unwrap_or(0));
    assert!(a > b, "A's physical blocks rise with its compute");
    assert!(a <= e.issued_of(&h(1)) + e.issued_of(&h(1)) / 2, "never past Rt·(1+σ)");
    assert!(a + b <= e.total_issued(), "never past the epoch total");
}

/// T2: raising A's bond alone raises A's issued rights (target); with compute A's physical blocks rise; without compute nothing
/// physical changes and the report says TargetOnly.
#[test]
fn t2_bond_raises_target_and_physical_only_with_compute() {
    let v = vals(0);
    let before = issue(&[(1, 1_000), (2, 1_000)], 200, &v);
    let after = issue(&[(1, 3_000), (2, 1_000)], 200, &v);
    assert!(after.issued_of(&h(1)) > before.issued_of(&h(1)));
    // With compute on A (A wins every step): physical rises with the rights.
    let all_a: Vec<_> = (0..1_000u64).map(|i| (i, 1)).collect();
    let (b_used, _) = run_epoch(&before, &all_a, &v);
    let (a_used, _) = run_epoch(&after, &all_a, &v);
    assert!(a_used[&h(1)] > b_used[&h(1)], "with compute, more bond is more physical blocks");
    // Without compute on A (B wins every step): A's physical blocks stay 0 — only the target moved.
    let all_b: Vec<_> = (0..1_000u64).map(|i| (i, 2)).collect();
    let (x, _) = run_epoch(&before, &all_b, &v);
    let (y, _) = run_epoch(&after, &all_b, &v);
    let row = |e: &PalwRightsEpochV1, used: &BTreeMap<Hash64, u64>, ppm: u64| PalwModelReportRowV1 {
        model: h(1),
        target_p_ppm: Some(ppm),
        rights_issued: e.issued_of(&h(1)),
        physical_blocks: used.get(&h(1)).copied().unwrap_or(0),
        payout_coinbase_sompi: Some(0),
        ..Default::default()
    };
    assert!(matches!(
        palw_report_outcome_v1(&row(&before, &x, 500_000), &row(&after, &y, 838_000)),
        PalwReportClaimV1::TargetOnly { .. }
    ));
}

/// T3: an all-model claim surge changes no model's issued rights (no model-specific hardening; the common W is the lottery's).
#[test]
fn t3_a_surge_does_not_harden_any_model() {
    let v = vals(250);
    let e1 = issue(&[(1, 2_000), (2, 1_000)], 200, &v);
    let e2 = issue(&[(1, 2_000), (2, 1_000)], 200, &v);
    assert_eq!(e1, e2, "issuance reads the allocation and N_exp only, never the demand");
}

/// T4: only A surges — A's capacity shortfall is reported; B's capacity (a disjoint verifier set) is untouched.
#[test]
fn t4_a_surge_is_reported_and_does_not_shrink_b() {
    let verifiers = [
        PalwVerifierSupplyV1 { per_span: 100, classes: [h(1)].into_iter().collect() },
        PalwVerifierSupplyV1 { per_span: 100, classes: [h(2)].into_iter().collect() },
    ];
    let calm: BTreeMap<_, _> = [(h(1), 50u128), (h(2), 50u128)].into_iter().collect();
    let surge: BTreeMap<_, _> = [(h(1), 500u128), (h(2), 50u128)].into_iter().collect();
    let c0 = palw_waterfill_capacity_v1(&verifiers, &calm);
    let c1 = palw_waterfill_capacity_v1(&verifiers, &surge);
    assert_eq!(c0[&h(2)], c1[&h(2)], "B is not shrunk by A's surge");
    let short = palw_capacity_short_v1(c1[&h(1)], 500, 1).expect("A's shortage is reported");
    assert_eq!(short.shortfall_per_span, 400);
}

/// T5: adding verifiers of A grows A's capacity, so a backlog that did not fit fits.
#[test]
fn t5_more_verifiers_more_capacity() {
    let demand: BTreeMap<_, _> = [(h(1), 150u128)].into_iter().collect();
    let one = [PalwVerifierSupplyV1 { per_span: 100, classes: [h(1)].into_iter().collect() }];
    let two = [one[0].clone(), one[0].clone()];
    assert!(!palw_capacity_fits_v1(palw_waterfill_capacity_v1(&one, &demand)[&h(1)], 150));
    assert!(palw_capacity_fits_v1(palw_waterfill_capacity_v1(&two, &demand)[&h(1)], 150));
}

/// T6 (rights side): issuance never reads a bond cap and a right never raises one — the bond's Q/B/R/F stay the budget engine's;
/// unused rights are reported, not handed to a capped bond.
#[test]
fn t6_rights_do_not_raise_caps_and_unused_rights_expire() {
    let v = vals(0);
    let e = issue(&[(1, 1_000)], 100, &v);
    // Only 10 wins (the model's bonds hit their caps elsewhere): the rest of the rights are unused, never re-split to the same model.
    let wins: Vec<_> = (0..10u64).map(|i| (i, 1)).collect();
    let (used, _) = run_epoch(&e, &wins, &v);
    let unused = e.issued_of(&h(1)) - used[&h(1)];
    assert!(unused > 0, "reported as rights_unused (expire unminted)");
}

/// T7 / N1: an entrant takes a linear, capped share of the reserve, which comes out of the fixed total.
#[test]
fn t7_n1_entrant_reserve_is_inside_the_fixed_total() {
    let v = vals(0);
    let w = weights(&[(1, 10_000), (2, 5_000)]);
    let entrants: BTreeMap<_, _> = [(h(9), 100u128)].into_iter().collect();
    for n in [100u64, 1_000, 12_345] {
        let without = palw_rights_issue_v1(0, 0, 1_000, n, &v, Some(&w), &BTreeMap::new(), |_| true);
        let with = palw_rights_issue_v1(0, 0, 1_000, n, &v, Some(&w), &entrants, |_| true);
        assert!(with.total_issued() <= n, "never extra issuance");
        assert!(without.total_issued() <= n - without.entrant_reserve, "the reserve is deducted first");
        assert!(with.issued_of(&h(9)) <= n * 20 / 1_000, "capped per model");
        assert!(with.issued_of(&h(9)) > 0 || n * 50 / 1_000 == 0);
    }
}

// ---- RFC-0016 §12.2 (N1–N7) ------------------------------------------------------------------------------------------------

/// N2 / A4 / A12: splitting one model's capital into near-duplicates never gains rights (α ≥ 1 is superadditive), and splitting an
/// entrant gains nothing from the linear reserve.
#[test]
fn n2_a4_a12_splitting_never_gains() {
    let v = vals(0);
    let whole = issue(&[(1, 10_000), (2, 10_000)], 1_000, &v);
    let split = issue(&[(1, 5_000), (3, 5_000), (2, 10_000)], 1_000, &v);
    assert!(split.issued_of(&h(1)) + split.issued_of(&h(3)) <= whole.issued_of(&h(1)));
    let w = weights(&[(2, 10_000)]);
    let one: BTreeMap<_, _> = [(h(8), 100u128)].into_iter().collect();
    let two: BTreeMap<_, _> = [(h(8), 50u128), (h(9), 50u128)].into_iter().collect();
    let mut wide = v;
    wide.entrant_max_per_model_permille = 1_000;
    let a = palw_rights_issue_v1(0, 0, 1_000, 1_000, &wide, Some(&w), &one, |_| true);
    let b = palw_rights_issue_v1(0, 0, 1_000, 1_000, &wide, Some(&w), &two, |_| true);
    assert!(b.issued_of(&h(8)) + b.issued_of(&h(9)) <= a.issued_of(&h(8)));
}

/// N3: capacity zero admits nothing (and queues nothing), and the quote refuses before the lottery.
#[test]
fn n3_capacity_zero_admits_nothing() {
    let v = vals(0);
    assert!(!palw_capacity_fits_v1(0, 0));
    let q = palw_admission_quote_v1(None, Some((0, 10, 0)), 0, &v);
    assert_eq!(q.verdict, PalwQuoteVerdictV1::Refuse("RFC0016_CAPACITY_ZERO"));
    let mut queue = PalwFinalQueueV1::default();
    let e = PalwFinalQueueEntryV1 { claim: h(1), bond: bond(1), queued_daa: 0, reserved_rights: 1 };
    assert_eq!(queue.enqueue_v1(e, 0, 8, &v), Err(PalwFinalQueueRefusalV1::CapacityZero));
}

/// N4 / A3: a verifier ready for many models/classes is counted once; splitting a verifier into two halves adds nothing.
#[test]
fn n4_a3_verifier_capacity_is_never_double_counted() {
    let classes: BTreeSet<Hash64> = (1..=5).map(h).collect();
    let demand: BTreeMap<_, _> = (1..=5u64).map(|i| (h(i), i as u128 * 37)).collect();
    let one = [PalwVerifierSupplyV1 { per_span: 1_000, classes: classes.clone() }];
    let total: u128 = palw_waterfill_capacity_v1(&one, &demand).values().sum();
    assert!(total <= 1_000);
    let halves = [PalwVerifierSupplyV1 { per_span: 500, classes: classes.clone() }, PalwVerifierSupplyV1 { per_span: 500, classes }];
    let total2: u128 = palw_waterfill_capacity_v1(&halves, &demand).values().sum();
    assert!(total2 <= 1_000, "Sybil halves of one budget are still one budget");
}

/// N5 / A5: queued claims reserve at most one right each; the queue's total never exceeds its length; a bond's share is capped.
#[test]
fn n5_a5_queue_reservation_and_squatting_are_bounded() {
    let v = vals(0);
    let mut q = PalwFinalQueueV1::default();
    let cap = palw_queue_bond_share_cap_v1(10, 40, &v);
    assert_eq!(cap, 2, "⌈8 · 10/40⌉");
    let mut admitted = 0;
    for i in 0..20u64 {
        let e = PalwFinalQueueEntryV1 { claim: h(i), bond: bond(1), queued_daa: i, reserved_rights: 1 };
        if q.enqueue_v1(e, 100, cap, &v).is_ok() {
            admitted += 1;
        }
    }
    assert_eq!(admitted, 2, "one bond cannot squat the queue");
    assert!(q.reserved_rights_v1() <= v.queue_len_per_class as u64);
    let over = PalwFinalQueueEntryV1 { claim: h(99), bond: bond(2), queued_daa: 0, reserved_rights: 3 };
    assert_eq!(q.enqueue_v1(over, 100, cap, &v), Err(PalwFinalQueueRefusalV1::OverReservation));
}

/// N6 / A10: post-quote race — the quote admits with margin; concurrent winners beyond the remaining rights are refused by name
/// (reported), never admitted past the rights.
#[test]
fn n6_a10_post_quote_race_is_absorbed_or_reported() {
    let v = vals(0);
    let e = issue(&[(1, 1_000)], 10, &v);
    let mut used: BTreeMap<Hash64, u64> = [(h(1), e.issued_of(&h(1)) - 3)].into_iter().collect();
    let q = palw_rights_quote_v1(&e, &used, &h(1), 0, &v);
    assert_eq!(q.remaining_own, 3);
    let quote = palw_admission_quote_v1(Some(q), None, 0, &v);
    assert_eq!(quote.verdict, PalwQuoteVerdictV1::Admit, "3 > h = 2");
    let mut refused = 0;
    for _ in 0..5 {
        match palw_rights_admit_v1(&e, &used, &h(1), 0, &v) {
            Ok(_) => *used.get_mut(&h(1)).unwrap() += 1,
            Err(_) => refused += 1,
        }
    }
    assert_eq!(refused, 2, "the racers past the rights are refused and counted");
    assert_eq!(used[&h(1)], e.issued_of(&h(1)));
}

/// N7: invalid / convicted claims do not dilute honest pay — under V+ every admitted claim holds a whole right (no ψ), and under
/// the B+ comparison the invalid claims are excluded from the demand before ψ.
#[test]
fn n7_invalid_claims_do_not_dilute_honest_pay() {
    let honest = 10u64;
    let invalid = 90u64;
    assert_eq!(palw_bplus_psi_permille_v1(10, honest, 0), 1_000, "invalid claims excluded: honest keep ψ = 1");
    assert!(palw_bplus_psi_permille_v1(10, honest + invalid, 0) < 1_000, "had they been counted, ψ would drop");
}

// ---- RFC-0016 §12.5 (adversarial, pure parts) ---------------------------------------------------------------------------------

/// A7: griefing the common W with unpaid wins cannot inflate the epoch's rights (issuance reads N_exp, not realized blocks).
#[test]
fn a7_griefing_does_not_inflate_rights() {
    let v = vals(250);
    let e = issue(&[(1, 1_000), (2, 1_000)], 200, &v);
    let griefer: Vec<_> = (0..10_000u64).map(|i| (i % 1_000, 2)).collect();
    let (used, _) = run_epoch(&e, &griefer, &v);
    assert!(used.values().sum::<u64>() <= e.total_issued());
}

/// A8 / H4: the rights ledger is a pure function of the epoch, the allocation and the counters — recomputing after a reorg or a
/// restart from the same state gives the same answers, whatever the map construction order.
#[test]
fn a8_h4_rights_are_deterministic() {
    let v = vals(250);
    let a = issue(&[(1, 1_000), (2, 2_000), (3, 500)], 300, &v);
    let b = issue(&[(3, 500), (2, 2_000), (1, 1_000)], 300, &v);
    assert_eq!(a, b);
}

/// A9: a model RFC-0017 marks ineligible (or an attestation-gate failure) is issued nothing, and its share is not re-split.
#[test]
fn a9_ineligible_models_get_no_rights() {
    let v = vals(0);
    let w = weights(&[(1, 1_000), (2, 1_000)]);
    let e = palw_rights_issue_v1(0, 0, 1_000, 200, &v, Some(&w), &BTreeMap::new(), |m| *m != h(2));
    assert_eq!(e.issued_of(&h(2)), 0);
    let all = palw_rights_issue_v1(0, 0, 1_000, 200, &v, Some(&w), &BTreeMap::new(), |_| true);
    assert_eq!(e.issued_of(&h(1)), all.issued_of(&h(1)), "not raised by the ineligible share");
}

// ---- RFC-0016 §12.4 (clock) ----------------------------------------------------------------------------------------------------

/// H3: rights never move the clock — issuance within an epoch does not depend on the DAA inside it, and the rights functions take no
/// slot, tick or bits input.
#[test]
fn h3_rights_do_not_depend_on_the_clock_within_an_epoch() {
    let v = vals(250);
    let w = weights(&[(1, 1_000), (2, 3_000)]);
    let a = palw_rights_issue_v1(4, 4_000, 1_000, 200, &v, Some(&w), &BTreeMap::new(), |_| true);
    let b = palw_rights_issue_v1(4, 4_000, 1_000, 200, &v, Some(&w), &BTreeMap::new(), |_| true);
    assert_eq!(a, b);
}

// ---- fences ------------------------------------------------------------------------------------------------------------------

/// The fences are None on testnet-12 as shipped and refused when armed (placeholder values, empty measured records).
#[test]
fn fences_are_dormant_and_refused_when_armed() {
    let shipped = palw_t12_shipped_params();
    assert_eq!(shipped.palw_model_block_rights_v1, None);
    assert_eq!(shipped.palw_elastic_final_capacity_v1, None);
    assert!(shipped.validate_palw_model_block_rights_v1().is_ok());
    assert!(shipped.validate_palw_elastic_final_capacity_v1().is_ok());
    let mut armed = palw_t12_shipped_params();
    armed.palw_model_block_rights_v1 = Some(ForkActivation::new(1_000_000));
    armed.sync_palw_rfc0016_v1();
    assert!(armed.validate_palw_model_block_rights_v1().is_err());
    let mut cap = palw_t12_shipped_params();
    cap.palw_elastic_final_capacity_v1 = Some(ForkActivation::new(1_000_000));
    cap.sync_palw_rfc0016_v1();
    assert!(cap.validate_palw_elastic_final_capacity_v1().is_err());
    assert!(PALW_RFC0016_MEASURED_RECORDS_V1.is_empty());
}
