//! **R2 through the registry's span step** (ADR-0152-adjacent: Activation Pool, user decision
//! 2026-09-25; the review's M2): past `Params::palw_activation_pool` the step skips the rows of
//! Dormant and Frozen classes, and a `Candidate` row reads its ready seats and its jury only at its
//! own staggered audit span. The cost is counted in readiness-predicate evaluations
//! (`PALW_READY_PREDICATE_EVALS_FOR_TESTS`), never in wall time.

use super::*;
use crate::palw_activation_pool_v1::{PALW_ACTIVATION_POOL_TERMS_V1, palw_admission_audit_due_staggered_v1};
use crate::palw_model_registry_v1::palw_admission_audit_period_spans_v2;

/// The ADR-0135 fixture's extras, with the pool armed or not.
fn pool_extras(armed: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { activation_pool: armed.then_some(PALW_ACTIVATION_POOL_TERMS_V1), ..extras(Some(fold(kimi_work()))) }
}

/// The floor, bond 1 and twenty more bonds, stepped once so the base class has its row.
fn network_of_bonds() -> PalwChainStateV2 {
    let p = params();
    let mut objects = register_class_and_bond();
    objects.extend((2..=21).map(|n| bond(n, 1_000)));
    let (s1, _) = step(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None, Some(fold(kimi_work()))).unwrap();
    let (s2, _) = step(&s1, &p, &ctx(2, 110, 2), &[], None, Some(fold(kimi_work()))).unwrap();
    assert!(s2.model_lifecycle(&h64(1)).is_some(), "the premise: the floor has its row");
    s2
}

fn candidate_id(i: u64) -> Hash64 {
    h64(0x7000 + i)
}

/// `n` bought classes installed as `Candidate` rows (a registrant bond, a zero share), exactly what
/// `open_model_lifecycle` writes past the independence fence.
fn with_candidates(base: &PalwChainStateV2, n: u64) -> PalwChainStateV2 {
    let mut s = base.clone();
    let floor_row = s.model_lifecycle(&h64(1)).cloned().expect("the floor's row");
    for i in 0..n {
        let mut class = s.class(&h64(1)).cloned().expect("the floor's record");
        class.artifact_root = h64(0xA000 + i);
        class.registrant_bond = Some(bond_key(2));
        s.set_class_for_tests(candidate_id(i), class);
        let mut row = floor_row.clone();
        row.state = PalwModelLifecycleV1::Candidate;
        row.work = kimi_work();
        row.admission_milli = 0;
        s.set_model_lifecycle_for_tests(candidate_id(i), row);
    }
    s
}

/// The predicate evaluations one span-opening block costs on `state`.
fn evals_of_one_boundary(state: &PalwChainStateV2, daa: u64, armed: bool) -> (u64, PalwChainStateV2) {
    let p = params();
    PALW_READY_PREDICATE_EVALS_FOR_TESTS.with(|count| count.set(0));
    let blue = state.last_point().map(|point| point.blue_score + 1).unwrap_or(3);
    let (next, _) = apply_palw_transition_v2_with_extras(
        state,
        &p,
        &ctx(blue, daa, blue),
        &[],
        None,
        false,
        false,
        false,
        false,
        &pool_extras(armed),
    )
    .expect("the boundary folds");
    (PALW_READY_PREDICATE_EVALS_FOR_TESTS.with(|count| count.get()), next)
}

/// **Between its audits a Candidate costs the span step nothing; at its audit it costs one walk of
/// the bonds.** Thirty-two Candidates on a twenty-one-bond network, measured at a span that is none
/// of their audits and at the span that is exactly one's: armed, the step's predicate evaluations
/// are the same as with no Candidate at all between audits, and one class's walk at its audit;
/// unarmed, every Candidate walks every bond at every span.
#[test]
fn r2_the_span_step_does_not_scale_with_candidate_rows_between_their_audits() {
    let base = network_of_bonds();
    let bonds = base.bonds_iter().count() as u64;
    assert_eq!(bonds, 21);
    let n = 32u64;
    let crowded = with_candidates(&base, n);
    let f = fold(kimi_work());
    let period = palw_admission_audit_period_spans_v2(params().epoch_length, f.span_daa, f.admission_audit_period_daa);
    assert_eq!(period, 100, "the fixture's epoch over its ten-DAA spans");
    let due_at = |span: u64| (0..n).filter(|i| palw_admission_audit_due_staggered_v1(&candidate_id(*i), span, period)).count();
    // A span no Candidate audits at, and one exactly one of them does, both past the grace.
    let quiet = (20u64..200).find(|span| due_at(*span) == 0).expect("32 classes leave a quiet span in 100");
    let single = (20u64..200).find(|span| due_at(*span) == 1).expect("and a span only one of them audits at");

    let (empty_quiet, _) = evals_of_one_boundary(&base, quiet * SPAN, true);
    let (armed_quiet, next) = evals_of_one_boundary(&crowded, quiet * SPAN, true);
    let (unarmed_quiet, _) = evals_of_one_boundary(&crowded, quiet * SPAN, false);
    println!(
        "predicate evaluations at a quiet span: no Candidate {empty_quiet}; {n} Candidates armed {armed_quiet}, unarmed {unarmed_quiet}"
    );
    assert_eq!(armed_quiet, empty_quiet, "armed: 32 Candidates between their audits cost the step nothing");
    assert_eq!(unarmed_quiet - empty_quiet, n * bonds, "unarmed: each Candidate walks every bond every span");
    for i in 0..n {
        assert_eq!(
            next.model_lifecycle(&candidate_id(i)).map(|row| row.state),
            Some(PalwModelLifecycleV1::Candidate),
            "and a skipped Candidate is still a Candidate"
        );
    }

    let (empty_single, _) = evals_of_one_boundary(&base, single * SPAN, true);
    let (armed_single, _) = evals_of_one_boundary(&crowded, single * SPAN, true);
    println!("at the span one Candidate audits at: no Candidate {empty_single}; armed {armed_single}");
    // Its ready count walks the bonds once; its jury draws nothing without the span before's anchor.
    assert_eq!(armed_single - empty_single, bonds, "armed: at its audit exactly one Candidate walks the bonds once");
}

/// **A Dormant or Frozen class's row is not stepped past the fence**: a Probation row whose class is
/// Frozen (or Dormant) stays exactly as it was, where the unarmed step would read its missing seats
/// and move it to `Held`.
#[test]
fn r2_the_rows_of_dormant_and_frozen_classes_are_not_stepped() {
    let base = network_of_bonds();
    for status in [PalwClassStatusV2::Frozen { since_daa: 105 }, PalwClassStatusV2::Dormant { since_daa: 105 }] {
        let mut s = with_candidates(&base, 1);
        let mut class = s.class(&candidate_id(0)).cloned().unwrap();
        class.status = status.clone();
        s.set_class_for_tests(candidate_id(0), class);
        let mut row = s.model_lifecycle(&candidate_id(0)).cloned().unwrap();
        row.state = PalwModelLifecycleV1::Probation { probes_passed: 3 };
        s.set_model_lifecycle_for_tests(candidate_id(0), row.clone());
        let (_, armed) = evals_of_one_boundary(&s, 30 * SPAN, true);
        let (_, unarmed) = evals_of_one_boundary(&s, 30 * SPAN, false);
        assert_eq!(armed.model_lifecycle(&candidate_id(0)), Some(&row), "{status:?}: armed, the row is not stepped at all");
        assert_eq!(
            unarmed.model_lifecycle(&candidate_id(0)).map(|r| r.state),
            Some(PalwModelLifecycleV1::Held),
            "{status:?}: unarmed, the old step reads its missing seats and holds it"
        );
    }
}

/// **A Candidate meets its jury at its own staggered span, and only there** — the seed still the
/// anchor of the span before. Past the fence the unstaggered audit span (a multiple of the period)
/// is no longer this class's audit unless its offset is zero, and its own span is.
#[test]
fn r2_a_candidate_is_audited_at_its_own_span_of_the_period() {
    let f = fold(kimi_work());
    let period = palw_admission_audit_period_spans_v2(params().epoch_length, f.span_daa, f.admission_audit_period_daa);
    let own: Vec<u64> = (1..=2 * period).filter(|span| palw_admission_audit_due_staggered_v1(&kimi_id(), *span, period)).collect();
    assert_eq!(own.len(), 2, "one audit per period: {own:?}");
    let base = network_of_bonds();
    let s = with_candidates(&base, 1);
    // The evaluation count at a span is the Candidate's walk exactly when that span is its audit.
    let class = candidate_id(0);
    let audit = (20u64..220).find(|span| palw_admission_audit_due_staggered_v1(&class, *span, period)).unwrap();
    let (at_audit, _) = evals_of_one_boundary(&s, audit * SPAN, true);
    let (at_audit_empty, _) = evals_of_one_boundary(&base, audit * SPAN, true);
    let (after, _) = evals_of_one_boundary(&s, (audit + 1) * SPAN, true);
    let (after_empty, _) = evals_of_one_boundary(&base, (audit + 1) * SPAN, true);
    assert!(at_audit > at_audit_empty, "its own span reads it");
    assert_eq!(after, after_empty, "the next span does not");
}
