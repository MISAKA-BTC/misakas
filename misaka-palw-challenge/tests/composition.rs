//! The soundness review dossier's worked numbers (`docs/design/palw/soundness-review-dossier/05-composition.md` §5.5), pinned:
//! any change to the calculator, or to a number the dossier quotes, fails here first. Plus a toy-field experiment that the
//! single-modulus error model and the multi-modulus (CRT) alias argument hold with this crate's own sampler.

use misaka_palw_challenge::composition::*;
use misaka_palw_challenge::hash::h;
use misaka_palw_challenge::seed::{ChallengeStreamV1, StreamKindV1, StreamLabelV1};

/// 9B-8k on the K2 route (`coverage-p1p2-record.md` §1.2): 2,210 probabilistic instances a position × 8,192 positions.
const R_9B_8K: u128 = 2_210 * 8_192;
/// `⌊log2 (2^127 − 1)⌋`, `⌊log2 (2^89 − 1)⌋`.
const B_M127: u32 = 126;
const B_M89: u32 = 88;

fn fam(b: u32, t: u32, r: u128) -> [CheckFamilyV1; 1] {
    [CheckFamilyV1 { per_repetition_bits: b, repetitions: t, instances: r }]
}

fn whole_claim(f: &[CheckFamilyV1], adv: AdversaryBudgetV1) -> ComposedBoundV1 {
    composed_false_accept_bits_v1(f, InstanceCompositionV1::UnionBound, Some(256), CoverageV1::Complete, &adv).unwrap()
}

/// The interim onboarding policy's adversary as OPV-BOOT's grinding table states it: `retry_limit` 2, `G = P(32, 2) = 992`.
fn opv_boot_interim() -> AdversaryBudgetV1 {
    let g = ceil_log2_v1(ordered_choices_v1(32, 2).unwrap());
    AdversaryBudgetV1 { retry_limit: 2, grinding_bits_per_beacon: g, beacons_per_attempt: 1, adaptive_statements: 1 }
}

/// Today's beacon (v2 accumulator) against a last contributor with 2^40 offline candidate works, 2^20 statements.
fn last_contributor_2_40() -> AdversaryBudgetV1 {
    AdversaryBudgetV1 { retry_limit: 2, grinding_bits_per_beacon: 40, beacons_per_attempt: 1, adaptive_statements: 1 << 20 }
}

#[test]
fn rows_a_b_reproduce_the_kernels_derived_bounds_for_9b_8k() {
    assert_eq!(ceil_log2_v1(R_9B_8K), 25);
    let a = whole_claim(&fam(B_M127, 2, R_9B_8K), AdversaryBudgetV1::NONE);
    assert_eq!((a.check_bits, a.effective_bits), (Some(227), Some(226)), "K2-TIR-v1: ε ≤ 2^-226 (coverage record)");
    let b = whole_claim(&fam(B_M89, 2, R_9B_8K), AdversaryBudgetV1::NONE);
    assert_eq!((b.check_bits, b.effective_bits), (Some(151), Some(150)), "K2-TIR-v2: ε ≤ 2^-150 (coverage record)");
}

#[test]
fn rows_c_to_f_charge_retries_grinding_and_statements() {
    // C: the OPV-BOOT interim adversary (2 + 10 bits).
    let c = whole_claim(&fam(B_M89, 2, R_9B_8K), opv_boot_interim());
    assert_eq!((c.loss_bits, c.effective_bits), (12, Some(138)));
    // D: today's beacon, a last contributor with 2^40 hashes, 2^20 statements: below 128.
    let d = whole_claim(&fam(B_M89, 2, R_9B_8K), last_contributor_2_40());
    assert_eq!((d.loss_bits, d.effective_bits), (62, Some(88)));
    // E: the same adversary against three repetitions.
    let e = whole_claim(&fam(B_M89, 3, R_9B_8K), last_contributor_2_40());
    assert_eq!(e.effective_bits, Some(176));
    assert_eq!(min_repetitions_v1(B_M89, R_9B_8K, Some(256), &last_contributor_2_40(), 128), Some(3));
    assert_eq!(min_repetitions_v1(B_M127, R_9B_8K, Some(256), &last_contributor_2_40(), 128), Some(2));
    // F: an outsider's private salt (no beacon to grind), 2^20 statements over the horizon.
    let outsider = AdversaryBudgetV1 { adaptive_statements: 1 << 20, ..AdversaryBudgetV1::NONE };
    assert_eq!(whole_claim(&fam(B_M89, 2, R_9B_8K), outsider).effective_bits, Some(130));
}

#[test]
fn row_g_a_sampled_check_is_bounded_by_its_selection_term() {
    // An outsider that checks 8 of 8,192 positions: a one-position lie escapes with probability 8,184 / 8,192.
    let g = composed_false_accept_bits_v1(
        &fam(B_M89, 2, R_9B_8K),
        InstanceCompositionV1::UnionBound,
        Some(256),
        CoverageV1::Sampled { units: 8_192, checked: 8 },
        &AdversaryBudgetV1::NONE,
    )
    .unwrap();
    assert_eq!((g.selection_bits, g.effective_bits), (Some(0), Some(-2)), "no security from the check: deterrence only");
}

#[test]
fn row_h_the_single_false_instance_alternative_drops_the_instance_union() {
    let h = composed_false_accept_bits_v1(
        &fam(B_M89, 2, R_9B_8K),
        InstanceCompositionV1::SingleFalseInstance,
        Some(256),
        CoverageV1::Complete,
        &AdversaryBudgetV1::NONE,
    )
    .unwrap();
    assert_eq!((h.check_bits, h.effective_bits), (Some(176), Some(175)), "reviewer question Q-04, not a policy");
}

#[test]
fn rows_i_j_sampled_conformance_interim_and_a_128_bit_scope() {
    // I: the interim drill scope (2 vectors + 2 leaves, one repetition, the unreviewed fault model of density 1): 2 bits each.
    let s = sampled_scope_bits_v1(2, 1_000_000) as u32;
    assert_eq!(s, 2);
    let interim = [CheckFamilyV1 { per_repetition_bits: s, repetitions: 1, instances: 1 }; 2];
    let i =
        composed_false_accept_bits_v1(&interim, InstanceCompositionV1::UnionBound, None, CoverageV1::Complete, &opv_boot_interim())
            .unwrap();
    assert_eq!((i.check_bits, i.loss_bits, i.effective_bits), (Some(1), 12, Some(-11)), "2 − 1 − 2 − 10: a drill, no security");
    // J: scope v1's default fault model (vectors 1/2, leaves 1/16), retries 2, G 2^10, Q 2^10, binding 2^-256, target 128.
    let adv = AdversaryBudgetV1 { retry_limit: 2, grinding_bits_per_beacon: 10, beacons_per_attempt: 1, adaptive_statements: 1 << 10 };
    let need = 128 + 1 + adv.loss_bits() as u128 + 1; // + the family union, the loss, the binding union
    assert_eq!(need, 152);
    let (nv, nl) = (draws_for_bits_v1(need, 500_000).unwrap(), draws_for_bits_v1(need, 62_500).unwrap());
    assert_eq!((nv, nl), (211, 1_686));
    assert_eq!((sampled_scope_bits_v1(nv - 1, 500_000), sampled_scope_bits_v1(nl - 1, 62_500)), (151, 151), "the fewest");
    let scope = [
        CheckFamilyV1 { per_repetition_bits: sampled_scope_bits_v1(nv, 500_000) as u32, repetitions: 1, instances: 1 },
        CheckFamilyV1 { per_repetition_bits: sampled_scope_bits_v1(nl, 62_500) as u32, repetitions: 1, instances: 1 },
    ];
    let j = composed_false_accept_bits_v1(&scope, InstanceCompositionV1::UnionBound, Some(256), CoverageV1::Complete, &adv).unwrap();
    assert_eq!(j.effective_bits, Some(128));
    assert!(nv + nl <= 4_096, "fits PALW_CONFORMANCE_MAX_CHECKS_V1");
}

#[test]
fn rows_k_l_m_reservation_sealed_beacon_and_interactive_rounds() {
    // K: the deterrent reservation (BILI): interim terms; a full-coverage assumption; an 8-of-8,192 sampled coverage.
    assert_eq!(deterrent_reservation_v1(20, 100, 500, 1_000, 500), Some(240), "G14-R4's interim figure");
    assert_eq!(deterrent_reservation_v1(20, 100, 1, 1, 500), Some(240));
    assert_eq!(deterrent_reservation_v1(20, 100, 8, 8_192, 500), Some(40_960));
    assert_eq!(deterrent_reservation_v1(20, 100, 0, 8_192, 500), None, "no detection: no finite reservation");
    // L: a sealed-source beacon where the adversary can withhold any subset of 8 sealed sources (G ≤ 2^8), 2^10 statements.
    let sealed =
        AdversaryBudgetV1 { retry_limit: 2, grinding_bits_per_beacon: 8, beacons_per_attempt: 1, adaptive_statements: 1 << 10 };
    assert_eq!(whole_claim(&fam(B_M89, 2, R_9B_8K), sealed).effective_bits, Some(130));
    // M: a staged beacon of 30 rounds, 10 grinding bits each, charged as G^β: the conservative bound is unusable.
    let staged = AdversaryBudgetV1 { retry_limit: 2, grinding_bits_per_beacon: 10, beacons_per_attempt: 30, adaptive_statements: 1 };
    assert_eq!(staged.loss_bits(), 302);
}

// ── toy-field experiment ────────────────────────────────────────────────────────────────────────────────────────────────────

/// Draws trial `i`'s vector over GF(p) from the crate's own labelled stream, and returns whether `E·r ≡ 0 (mod p)`.
fn passes(seed: &[u8; 64], p: u64, e: &[Vec<i64>], trial: u32) -> bool {
    let label = StreamLabelV1 { kind: StreamKindV1::Freivalds, scope_id: [p as u8; 64], relation: 0, repetition: trial };
    let mut s = ChallengeStreamV1::new(seed, &label);
    let r: Vec<i64> = (0..e[0].len()).map(|_| s.index_below(p).unwrap() as i64).collect();
    e.iter().all(|row| row.iter().zip(&r).map(|(a, b)| a.rem_euclid(p as i64) * b).sum::<i64>() % p as i64 == 0)
}

fn pass_count(p: u64, e: &[Vec<i64>], trials: u32) -> u32 {
    let seed = h(b"MISAKA/PALW/DOSSIER/TOY-FREIVALDS/V1", &p.to_le_bytes());
    (0..trials).filter(|t| passes(&seed, p, e, *t)).count() as u32
}

#[test]
fn toy_field_freivalds_misses_a_fixed_error_with_probability_p_to_the_minus_rank() {
    const TRIALS: u32 = 31_000;
    // Rank 1 over GF(31): exactly 1/31 (mean 1,000, sd ≈ 31).
    let rank1 = vec![vec![0, 0, 5, 0], vec![0, 0, 0, 0], vec![0, 0, 0, 0]];
    let n1 = pass_count(31, &rank1, TRIALS);
    assert!((850..=1_150).contains(&n1), "rank 1: {n1} of {TRIALS}, expected ≈ 1,000");
    // Rank 2: 1/961 (mean ≈ 32).
    let rank2 = vec![vec![1, 0, 3, 0], vec![0, 2, 0, 7], vec![0, 0, 0, 0]];
    let n2 = pass_count(31, &rank2, TRIALS);
    assert!((8..=70).contains(&n2), "rank 2: {n2} of {TRIALS}, expected ≈ 32");
    // The zero error always passes: the check is complete.
    assert_eq!(pass_count(31, &[vec![0, 0, 0, 0]], 100), 100);
}

#[test]
fn toy_crt_an_integer_error_below_the_moduli_product_survives_and_one_at_the_product_does_not() {
    // Moduli 7 and 31 (both 2^e − 1 primes), product 217. Every nonzero |e| < 217 is nonzero modulo one of them.
    assert!((1..217i64).all(|e| e % 7 != 0 || e % 31 != 0));
    const TRIALS: u32 = 31_000;
    let (s7, s31) = (h(b"MISAKA/PALW/DOSSIER/TOY-CRT/V1", &[7]), h(b"MISAKA/PALW/DOSSIER/TOY-CRT/V1", &[31]));
    let both = |err: i64| {
        let e = vec![vec![0, err, 0]];
        (0..TRIALS).filter(|t| passes(&s7, 7, &e, *t) && passes(&s31, 31, &e, *t)).count() as u32
    };
    // e = 31 vanishes mod 31; the mod-7 check catches it with probability 6/7 (mean ≈ 4,429 passes, sd ≈ 62).
    let n31 = both(31);
    assert!((4_100..=4_760).contains(&n31), "e = 31: {n31} of {TRIALS}, expected ≈ 4,429");
    // e = 217 = 7 · 31 vanishes in both: a span at the product is a hole (why the checker refuses it).
    assert_eq!(both(217), TRIALS);
}
