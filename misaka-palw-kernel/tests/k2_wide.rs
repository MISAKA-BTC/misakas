//! **K2-TIR-v2: the multi-modulus dense relation, and kernel coexistence** (RFC-0005 §K.3/§K.5, RFC-0011 §15.7, §16.3–§16.5).
//!
//! An `i128` accumulator whose ranges are proven is `KERNEL_EXTENSION_REQUIRED` under K2-TIR-v1 and expressible under K2-TIR-v2;
//! a lie that aliases to zero modulo `2^127 − 1` is caught by the second modulus and convicted by the exact court; v1 and v2
//! coexist on one schedule (bound classes keep their kernel, deprecation stops only new work), and a claim, evidence object or
//! receipt under one kernel is never judged by the other.

mod common;

use common::*;
use misaka_palw_kernel::check::registration_outcome_v1;
use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor, k2_tir_v2_descriptor,
};
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1};
use misaka_palw_kernel::field::{F107, Fp};
use misaka_palw_kernel::outcome::RegistrationOutcomeV1;
use misaka_palw_kernel::plan::{dense_moduli_v2, relation_moduli};
use misaka_palw_kernel::verify::{ClaimVerdictV1, FaultKindV1, ScopeV1, TraceMaterialV1};
use misaka_palw_tir::{DType, Prim};
use misaka_palw_tir_sketch::fixture::{dense_moe_v1, wide128_v1};

fn wide_claim() -> Claim {
    Claim::of(wide128_v1(3), k2_tir_v2_descriptor())
}

/// The `i128` product's `(position, occurrence, node)` at `position`.
fn wide_product(c: &Claim, position: u32) -> (u32, u16, u16) {
    let at = c.find(position, |p| matches!(p, Prim::MatMul));
    let t = &c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize];
    assert_eq!(t.dtype, DType::I128, "the first product is the wide one");
    at
}

#[test]
fn moduli_are_the_fewest_whose_product_exceeds_the_span() {
    assert_eq!(dense_moduli_v2(4096, DType::I8, DType::I8, DType::I32), Some(1));
    assert_eq!(dense_moduli_v2(16, DType::I32, DType::I32, DType::I64), Some(1), "k·2^62 + 2^63 < 2^127 − 1");
    assert_eq!(dense_moduli_v2(16, DType::I64, DType::I32, DType::I128), Some(2), "|out| = 2^127 alone needs a second modulus");
    assert_eq!(dense_moduli_v2(1 << 20, DType::I64, DType::I64, DType::I128), Some(2), "2^148 < 2^(127+107−1)");
    assert_eq!(dense_moduli_v2(16, DType::I128, DType::I128, DType::I128), Some(3), "2^258 needs the third");
    // The widest span TIR v1 types can make (k < 2^64, i128 operands) stays below the three-modulus product (≈ 2^323).
    assert_eq!(dense_moduli_v2(u64::MAX, DType::I128, DType::I128, DType::I128), Some(3));
}

#[test]
fn an_i128_accumulator_is_an_extension_under_v1_and_eligible_under_an_armed_v2() {
    let fx = wide128_v1(3);
    let root = root_of(&fx.program);
    let (v1, v2) = (k2_tir_v1_descriptor(), k2_tir_v2_descriptor());
    let o = registration_outcome_v1(&active_for(&v1), &v1, &fx.program, root, MAX_POSITIONS, 0);
    assert!(matches!(o, RegistrationOutcomeV1::KernelExtensionRequired { family: Some(ConstraintFamilyV1::DenseMatrix), .. }), "{o}");
    let shipped = registration_outcome_v1(&builtin_schedule_v1(), &v2, &fx.program, root, MAX_POSITIONS, 0);
    assert_eq!(shipped.code(), "KERNEL_NOT_ACTIVE", "v2 ships implemented, not active: {shipped}");
    let o = registration_outcome_v1(&active_for(&v2), &v2, &fx.program, root, MAX_POSITIONS, 0);
    let RegistrationOutcomeV1::EligibleAt { error_bits, .. } = o else { panic!("{o}") };
    assert!(error_bits >= 128, "2 repetitions × 88 bits − log2 R still reaches the target: 2^-{error_bits}");
    // The same v1 class (dense) is also expressible under v2, as a NEW class: another descriptor, another plan root.
    let dense = dense_moe_v1(7);
    let a = registration_outcome_v1(&active_for(&v1), &v1, &dense.program, root_of(&dense.program), MAX_POSITIONS, 0);
    let b = registration_outcome_v1(&active_for(&v2), &v2, &dense.program, root_of(&dense.program), MAX_POSITIONS, 0);
    let (RegistrationOutcomeV1::EligibleAt { plan_root: pa, .. }, RegistrationOutcomeV1::EligibleAt { plan_root: pb, .. }) = (a, b)
    else {
        panic!("dense is eligible under both")
    };
    assert_ne!(pa, pb, "no reinterpretation: a v2 plan is a different object");
}

#[test]
fn the_honest_wide_claim_passes_with_two_moduli_on_its_i128_product() {
    let c = wide_claim();
    let rel = c.plan.relations.iter().find(|r| r.checker == CheckerIdV1::FreivaldsCrtV2 && r.out_dtype == DType::I128.tag()).unwrap();
    assert_eq!(relation_moduli(rel), Some(2));
    let material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let (v, _) = c.verify_with(&c.trace, &material);
    let ClaimVerdictV1::Pass { probabilistic_checks, error_bits, .. } = v else { panic!("{v:?}") };
    assert!(probabilistic_checks > 0);
    assert!(error_bits >= 128, "{error_bits}");
}

#[test]
fn every_scalar_lie_in_the_i128_product_is_caught_and_convicted() {
    let c = wide_claim();
    for p in [0u32, 3] {
        let at = wide_product(&c, p);
        let len = c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize].len();
        for e in 0..len {
            let mut lie = c.trace.clone();
            bump(&mut lie.values[at.0 as usize][at.1 as usize][at.2 as usize], e);
            let material = TraceMaterialV1 { trace: &lie, params: &c.params };
            let (v, court) = c.verify_with(&lie, &material);
            let ClaimVerdictV1::Fault(proof) = v else { panic!("position {p} element {e}: {v:?}") };
            assert!(matches!(proof.kind, FaultKindV1::MatMulScalar { .. }), "{:?}", proof.kind);
            court(&proof).unwrap();
        }
    }
}

#[test]
fn a_lie_that_aliases_to_zero_mod_2_127_minus_1_is_caught_by_the_second_modulus() {
    let c = wide_claim();
    let at = wide_product(&c, 1);
    let honest = &c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize];
    // Pick a negative accumulator and add p = 2^127 − 1: still an i128, invisible to GF(2^127 − 1), visible to GF(2^107 − 1).
    let e = honest.data.iter().position(|v| *v < 0).expect("some accumulator is negative");
    let p = Fp::MODULUS as i128;
    let forged = honest.data[e] + p;
    assert!(DType::I128.contains(forged));
    assert_eq!(Fp::from_i128(forged), Fp::from_i128(honest.data[e]), "the single-modulus check cannot see this lie");
    assert_ne!(F107::from_i128(forged), F107::from_i128(honest.data[e]));
    let mut lie = c.trace.clone();
    lie.values[at.0 as usize][at.1 as usize][at.2 as usize].data[e] = forged;
    let material = TraceMaterialV1 { trace: &lie, params: &c.params };
    let (v, court) = c.verify_with(&lie, &material);
    let ClaimVerdictV1::Fault(proof) = v else { panic!("{v:?}") };
    let conviction = court(&proof).unwrap();
    assert_eq!((conviction.position, conviction.occurrence, conviction.node), at);
    // The honest claim, accused of the same scalar, is dismissed.
    let (_, honest_court) = c.verify_with(&c.trace, &TraceMaterialV1 { trace: &c.trace, params: &c.params });
    let mut accusation = (*proof).clone();
    accusation.output = c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize].clone();
    assert!(honest_court(&accusation).is_err());
}

#[test]
fn v1_and_v2_coexist_and_deprecation_stops_only_new_registrations() {
    let (v1, v2) = (k2_tir_v1_descriptor(), k2_tir_v2_descriptor());
    let schedule = KernelScheduleV1::default()
        .with(v1.digest(), KernelStatusV1::Deprecated { since_daa: 0, stop_new_daa: 100 })
        .with(v2.digest(), KernelStatusV1::LockedIn { activation_daa: 50 });
    let dense = dense_moe_v1(7);
    let root = root_of(&dense.program);
    let at = |d, daa| registration_outcome_v1(&schedule, d, &dense.program, root, MAX_POSITIONS, daa).code();
    assert_eq!((at(&v1, 49), at(&v2, 49)), ("ELIGIBLE_AT", "KERNEL_NOT_ACTIVE"), "before v2's activation boundary");
    assert_eq!((at(&v1, 50), at(&v2, 50)), ("ELIGIBLE_AT", "ELIGIBLE_AT"), "both at the boundary");
    assert_eq!((at(&v1, 100), at(&v2, 100)), ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT"), "v1 stops new work");
    // A claim already bound to v1 is still verified by v1's own rules after the stop (the schedule gates registration, not
    // the verification of what was bound under it).
    let c = Claim::honest();
    let (v, _) = c.verify_with(&c.trace, &TraceMaterialV1 { trace: &c.trace, params: &c.params });
    assert!(matches!(v, ClaimVerdictV1::Pass { .. }), "{v:?}");
}

#[test]
fn a_claim_bound_to_one_kernel_is_never_judged_by_the_other() {
    // v1's claim, its evidence object and plan, presented to a v2 verifier (and the reverse) are refused before any check.
    let mut c = Claim::honest();
    let ev = c.evidence_of(&c.trace);
    c.descriptor = k2_tir_v2_descriptor();
    let material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let (v, _) = c.verify_object(&c.trace, &ev, &material, &ScopeV1::WholeClaim);
    assert!(matches!(&v, ClaimVerdictV1::EvidenceMalformed { why } if why.contains("another kernel")), "{v:?}");
    let mut w = wide_claim();
    let ev = w.evidence_of(&w.trace);
    w.descriptor = k2_tir_v1_descriptor();
    let material = TraceMaterialV1 { trace: &w.trace, params: &w.params };
    let (v, _) = w.verify_object(&w.trace, &ev, &material, &ScopeV1::WholeClaim);
    assert!(matches!(v, ClaimVerdictV1::EvidenceMalformed { .. }), "{v:?}");
}
