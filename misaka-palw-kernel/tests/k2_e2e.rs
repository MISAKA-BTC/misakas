//! K2-TIR-v1 end to end on a real TIR class (a dense GQA attention layer over a KV history, a SwiGLU MLP, a top-2-of-4 MoE
//! layer gathered by a committed TopK, a head): registration outcomes, an honest claim's pass, a producer's lie localized to one
//! primitive instance and convicted by a court that reads only public material, the DA path kept apart from fraud, and every
//! way a plan can try to weaken its own check.

mod common;

use common::*;
use misaka_palw_kernel::challenge::ChallengeBindingV1;
use misaka_palw_kernel::check::{check_plan_v1, registration_outcome_v1};
use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor};
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1};
use misaka_palw_kernel::outcome::{CoverageBucketV1, CoverageEvidenceV1, RegistrationOutcomeV1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::trace::TraceV1;
use misaka_palw_kernel::verify::{
    ClaimContextV1, ClaimVerdictV1, DismissalV1, FaultKindV1, MaterialV1, ScopeV1, TraceMaterialV1, verify_scope_v1,
};
use misaka_palw_tir::{Interpreter, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::{dense_moe_v1, wide_v1, wide128_v1};

#[test]
fn the_honest_trace_is_the_reference_evaluators() {
    let c = Claim::honest();
    let interp = Interpreter::new(&c.program).unwrap();
    let reference = interp.run(&c.params, &TOKENS).unwrap();
    let post = c.program.schedule.layers.len() + 1;
    for (p, step) in reference.iter().enumerate() {
        assert_eq!(c.trace.values[p][post][c.program.logits as usize], step.logits, "position {p}: the tracer is not the evaluator");
    }
}

#[test]
fn registration_is_kernel_not_active_on_the_shipped_schedule_and_eligible_under_an_armed_one() {
    let fx = dense_moe_v1(7);
    let d = k2_tir_v1_descriptor();
    let shipped = registration_outcome_v1(&builtin_schedule_v1(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 1_000);
    assert_eq!(shipped.code(), "KERNEL_NOT_ACTIVE", "{shipped}");
    assert_eq!(shipped.coverage_bucket(CoverageEvidenceV1::NONE), CoverageBucketV1::KernelExtensionGap);
    let armed = registration_outcome_v1(&active(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 1_000);
    let RegistrationOutcomeV1::EligibleAt { error_bits, .. } = armed else { panic!("{armed}") };
    assert!(error_bits >= 128, "the derived whole-claim error reaches the target: 2^-{error_bits}");
    assert_eq!(armed.coverage_bucket(CoverageEvidenceV1::NONE), CoverageBucketV1::Untested, "eligibility alone is not coverage");
    let registered = CoverageEvidenceV1 { onchain_registration: true, public_prosecution_measured: false };
    assert_eq!(armed.coverage_bucket(registered), CoverageBucketV1::Untested, "nor is registration without public prosecution");
    let both = CoverageEvidenceV1 { onchain_registration: true, public_prosecution_measured: true };
    assert!(armed.coverage_bucket(both).counts_as_success());
}

#[test]
fn an_i128_accumulator_and_a_missing_family_are_kernel_extensions_not_successes() {
    let d = k2_tir_v1_descriptor();
    // Ranges proven, but one Mersenne modulus cannot hold the i128 product's error span.
    let fx = wide128_v1(3);
    let o = registration_outcome_v1(&active(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 0);
    let RegistrationOutcomeV1::KernelExtensionRequired { family, .. } = &o else { panic!("{o}") };
    assert_eq!(*family, Some(ConstraintFamilyV1::DenseMatrix), "{o}");
    // Ranges NOT proven (a partial sum can overflow its type): the frontend's to narrow, whatever kernel is asked.
    let fx = wide_v1(3);
    let o = registration_outcome_v1(&active(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 0);
    assert_eq!(o.code(), "FRONTEND_REQUIRED", "{o}");

    let mut narrow = d.clone();
    narrow.families.retain(|f| f.family != ConstraintFamilyV1::RecurrentState);
    let schedule = KernelScheduleV1::default().with(narrow.digest(), KernelStatusV1::Active { since_daa: 0 });
    let fx = dense_moe_v1(7);
    let o = registration_outcome_v1(&schedule, &narrow, &fx.program, root_of(&fx.program), MAX_POSITIONS, 0);
    assert!(
        matches!(o, RegistrationOutcomeV1::KernelExtensionRequired { family: Some(ConstraintFamilyV1::RecurrentState), .. }),
        "{o}"
    );
}

#[test]
fn a_plan_cannot_omit_weaken_overclaim_or_underprice() {
    let c = Claim::honest();
    let d = k2_tir_v1_descriptor();
    let root = root_of(&c.program);
    let check = |p: &misaka_palw_kernel::VerificationPlanV1| check_plan_v1(&active(), &d, &c.program, root, p, 0);
    check(&c.plan).unwrap();

    let mut omitted = c.plan.clone();
    omitted.relations.pop();
    assert_eq!(check(&omitted).unwrap_err().code(), "INCOMPLETE_COVERAGE");

    let mut weaker = c.plan.clone();
    let mm = weaker.relations.iter_mut().find(|r| r.checker == CheckerIdV1::FreivaldsM127).unwrap();
    mm.repetitions = 1;
    assert_eq!(check(&weaker).unwrap_err().code(), "PLAN_FORGED");

    let mut swapped = c.plan.clone();
    let mm = swapped.relations.iter_mut().find(|r| r.checker == CheckerIdV1::FreivaldsM127).unwrap();
    mm.checker = CheckerIdV1::StateContinuity;
    assert_eq!(check(&swapped).unwrap_err().code(), "PLAN_FORGED");

    let mut boast = c.plan.clone();
    boast.declared_error_bits = u16::MAX;
    assert_eq!(check(&boast).unwrap_err().code(), "PLAN_FORGED");

    let mut cheap = c.plan.clone();
    cheap.budgets.worst_court_bytes /= 2;
    assert_eq!(check(&cheap).unwrap_err().code(), "INCOMPLETE_COVERAGE");

    let mut doubled = c.plan.clone();
    let first = doubled.relations[0].clone();
    doubled.relations.push(first);
    assert_eq!(check(&doubled).unwrap_err().code(), "INCOMPLETE_COVERAGE");

    let mut other_program = c.plan.clone();
    other_program.program_root = [0xAB; 64];
    assert_eq!(check(&other_program).unwrap_err().code(), "PLAN_FORGED");

    let mut long = c.plan.clone();
    long.max_positions = u32::MAX;
    assert_eq!(check(&long).unwrap_err().code(), "BOUNDS_EXCEEDED");

    let mut tight = d.clone();
    tight.limits.max_court_bytes = 16;
    let schedule = KernelScheduleV1::default().with(tight.digest(), KernelStatusV1::Active { since_daa: 0 });
    let plan = plan_for_tir_program_v1(&tight, &c.program, root, MAX_POSITIONS).unwrap();
    assert_eq!(check_plan_v1(&schedule, &tight, &c.program, root, &plan, 0).unwrap_err().code(), "BOUNDS_EXCEEDED");
}

#[test]
fn an_honest_claim_passes_with_its_derived_bound() {
    let c = Claim::honest();
    let material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let (v, _) = c.verify_with(&c.trace, &material);
    let ClaimVerdictV1::Pass { probabilistic_checks, error_bits, .. } = v else { panic!("{v:?}") };
    assert!(probabilistic_checks > 0);
    assert!(error_bits >= 128);
}

/// The producer commits a false value at one node and serves exactly what it committed.
fn lie_at(c: &Claim, at: (u32, u16, u16), element: usize) -> TraceV1 {
    let mut t = c.trace.clone();
    bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], element);
    t
}

#[test]
fn a_false_matmul_scalar_is_found_by_freivalds_localized_and_convicted_by_the_public_court() {
    let c = Claim::honest();
    let at = c.find(2, |p| matches!(p, Prim::MatMul));
    let lie = lie_at(&c, at, 0);
    let material = TraceMaterialV1 { trace: &lie, params: &c.params };
    let (v, court) = c.verify_with(&lie, &material);
    let ClaimVerdictV1::Fault(proof) = v else { panic!("{v:?}") };
    assert_eq!((proof.position, proof.occurrence, proof.node), at);
    assert_eq!(proof.kind, FaultKindV1::MatMulScalar { slice: 0, i: 0, j: 0 });
    let conviction = court(&proof).expect("any node convicts from the public openings");
    assert_eq!((conviction.position, conviction.occurrence, conviction.node), at);

    // The same accusation against the honest claim is dismissed: the scalar recomputes.
    let honest_material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let (_, honest_court) = c.verify_with(&c.trace, &honest_material);
    let mut false_accusation = (*proof).clone();
    false_accusation.output = c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize].clone();
    assert_eq!(honest_court(&false_accusation).unwrap_err(), DismissalV1::NoFault);

    // A proof whose openings are not the committed values is not authentic.
    let mut forged = (*proof).clone();
    bump(&mut forged.inputs[0], 0);
    assert!(matches!(court(&forged), Err(DismissalV1::NotAuthentic(_))));
}

#[test]
fn a_false_value_in_every_exact_family_is_recomputed_and_convicted() {
    let c = Claim::honest();
    let families: [(&str, fn(&Prim) -> bool); 5] = [
        ("selection (TopK)", |p| matches!(p, Prim::TopK { .. })),
        ("selection (Gather)", |p| matches!(p, Prim::Gather { .. })),
        ("quant-range (Div)", |p| matches!(p, Prim::Div { .. })),
        ("exact arithmetic (Add)", |p| matches!(p, Prim::Add)),
        ("recurrent state (HistAppend)", |p| matches!(p, Prim::HistAppend { .. })),
    ];
    for (name, pick) in families {
        let at = c.find(1, pick);
        let lie = lie_at(&c, at, 0);
        let material = TraceMaterialV1 { trace: &lie, params: &c.params };
        let (v, court) = c.verify_with(&lie, &material);
        let ClaimVerdictV1::Fault(proof) = v else { panic!("{name}: {v:?}") };
        assert_eq!((proof.position, proof.occurrence, proof.node), at, "{name}");
        court(&proof).unwrap_or_else(|e| panic!("{name}: dismissed {e:?}"));
    }
}

#[test]
fn a_forged_history_row_cannot_hide_behind_its_own_position() {
    // The lie is in what position 1 appended; position 2's window re-reads it from position 1's COMMITTED row, so the
    // fabricated entry state of a later position is never something the producer gets to state.
    let c = Claim::honest();
    let at = c.find(1, |p| matches!(p, Prim::HistAppend { .. }));
    let mut lie = c.trace.clone();
    // Corrupt the window's PRIOR rows at position 2 only (the row appended at 1 stays honest).
    let later = (2, at.1, at.2);
    bump(&mut lie.values[later.0 as usize][later.1 as usize][later.2 as usize], 0);
    let material = TraceMaterialV1 { trace: &lie, params: &c.params };
    let (v, court) = c.verify_with(&lie, &material);
    let ClaimVerdictV1::Fault(proof) = v else { panic!("{v:?}") };
    assert_eq!((proof.position, proof.occurrence, proof.node), later);
    court(&proof).unwrap();
}

#[test]
fn a_value_served_but_not_committed_is_the_da_path_never_a_conviction() {
    let c = Claim::honest();
    let at = c.find(1, |p| matches!(p, Prim::MatMul));
    // Committed honestly; a provider serves altered bytes.
    let served = lie_at(&c, at, 0);
    let material = TraceMaterialV1 { trace: &served, params: &c.params };
    let (v, _) = c.verify_with(&c.trace, &material);
    assert!(matches!(v, ClaimVerdictV1::Unavailable { .. }), "{v:?}");

    struct Withholding<'a>(TraceMaterialV1<'a>);
    impl MaterialV1 for Withholding<'_> {
        fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
            if p == 3 { None } else { self.0.node_value(p, s, n) }
        }
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            self.0.param(j, l)
        }
    }
    let (v, _) = c.verify_with(&c.trace, &Withholding(TraceMaterialV1 { trace: &c.trace, params: &c.params }));
    assert!(matches!(v, ClaimVerdictV1::Unavailable { .. }), "{v:?}");
}

#[test]
fn a_challenge_drawn_before_the_evidence_was_bound_is_refused() {
    let c = Claim::honest();
    let d = k2_tir_v1_descriptor();
    let trace = c.trace.evidence();
    let ev = c.evidence_of(&c.trace);
    let header = c.header();
    let ctx = ClaimContextV1 {
        descriptor: &d,
        program: &c.program,
        plan: &c.plan,
        trace: &trace,
        evidence: &ev,
        header,
        params: &c.pc,
        tokens: &TOKENS,
        binding: ChallengeBindingV1 {
            network_domain: header.network_domain,
            claim_id: [8; 64],
            class_binding_id: header.class_binding_id,
            plan_root: c.plan.root(),
            evidence_root: [0; 64],
            beacon: [1; 64],
        },
    };
    let v = verify_scope_v1(&ctx, &TraceMaterialV1 { trace: &c.trace, params: &c.params }, &ScopeV1::WholeClaim);
    assert!(matches!(v, ClaimVerdictV1::EvidenceMalformed { .. }), "{v:?}");
}

#[test]
fn every_single_scalar_lie_in_one_product_is_caught() {
    // Exhaustive over one MatMul output: each element bumped alone is caught (the error is nonzero in the field and the
    // probability of a miss is 2^-252 per instance; a miss here would be a bug).
    let c = Claim::honest();
    let at = c.find(1, |p| matches!(p, Prim::MatMul));
    let len = c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize].len();
    for e in 0..len {
        let lie = lie_at(&c, at, e);
        let material = TraceMaterialV1 { trace: &lie, params: &c.params };
        let (v, court) = c.verify_with(&lie, &material);
        let ClaimVerdictV1::Fault(proof) = v else { panic!("element {e}: {v:?}") };
        court(&proof).unwrap();
    }
}

#[test]
fn a_fabricated_segment_boundary_a_weaker_suite_or_another_output_is_refused_before_any_check() {
    let c = Claim::honest();
    let material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let honest = c.evidence_of(&c.trace);
    assert_eq!(honest.segments.len(), 3, "5 positions in segments of 2");
    assert_eq!(honest.segments[1].entry_state_root, honest.segments[0].exit_state_root, "a segment's entry is its predecessor's exit");
    let mut forged = honest.clone();
    forged.segments[1].entry_state_root = [0xEE; 64];
    let (v, _) = c.verify_object(&c.trace, &forged, &material, &ScopeV1::Segments(vec![1]));
    assert!(matches!(&v, ClaimVerdictV1::EvidenceMalformed { why } if why.contains("fabricated boundary")), "{v:?}");
    let mut weak = honest.clone();
    weak.suite.repetitions = 1;
    assert!(matches!(c.verify_object(&c.trace, &weak, &material, &ScopeV1::WholeClaim).0, ClaimVerdictV1::EvidenceMalformed { .. }));
    let mut other_output = honest.clone();
    other_output.output_root = [1; 64];
    assert!(matches!(
        c.verify_object(&c.trace, &other_output, &material, &ScopeV1::WholeClaim).0,
        ClaimVerdictV1::EvidenceMalformed { .. }
    ));
    let mut gap = honest.clone();
    gap.segments.remove(1);
    assert!(matches!(c.verify_object(&c.trace, &gap, &material, &ScopeV1::WholeClaim).0, ClaimVerdictV1::EvidenceMalformed { .. }));
}

#[test]
fn a_segment_scope_checks_only_its_positions_and_reads_its_entry_from_the_committed_predecessor() {
    let c = Claim::honest();
    // A lie at position 4 (segment 2) leaves segments 0 and 1 passing and is found by segment 2's scope.
    let at = c.find(4, |p| matches!(p, Prim::MatMul));
    let lie = lie_at(&c, at, 0);
    let material = TraceMaterialV1 { trace: &lie, params: &c.params };
    let (v, _) = c.verify_scope(&lie, &material, &ScopeV1::Segments(vec![0, 1]));
    let ClaimVerdictV1::Pass { positions, scope_root, .. } = v else { panic!("{v:?}") };
    assert_eq!(positions, 4);
    let (v, court) = c.verify_scope(&lie, &material, &ScopeV1::Segments(vec![2]));
    let ClaimVerdictV1::Fault(proof) = v else { panic!("{v:?}") };
    assert_eq!(proof.position, 4);
    court(&proof).unwrap();
    // Different scopes attest different roots; an audit sample is its own kind.
    let (a, _) =
        c.verify_scope(&c.trace, &TraceMaterialV1 { trace: &c.trace, params: &c.params }, &ScopeV1::AuditOnly(vec![0, 1, 2, 3]));
    let ClaimVerdictV1::Pass { scope_root: audit_root, .. } = a else { panic!("{a:?}") };
    let (h, _) = c.verify_scope(&c.trace, &TraceMaterialV1 { trace: &c.trace, params: &c.params }, &ScopeV1::Segments(vec![0, 1]));
    let ClaimVerdictV1::Pass { scope_root: honest_root, .. } = h else { panic!("{h:?}") };
    assert_ne!(audit_root, honest_root);
    assert_ne!(scope_root, [0; 64]);
}

#[test]
fn weight_products_are_batched_across_the_scope_so_checks_and_weight_reads_do_not_grow_per_token() {
    let c = Claim::honest();
    let material = TraceMaterialV1 { trace: &c.trace, params: &c.params };
    let pass = |scope: ScopeV1| match c.verify_scope(&c.trace, &material, &scope).0 {
        ClaimVerdictV1::Pass { probabilistic_checks, cost, .. } => (probabilistic_checks, cost),
        v => panic!("{v:?}"),
    };
    let (one, one_cost) = pass(ScopeV1::AuditOnly(vec![0]));
    let (all, all_cost) = pass(ScopeV1::WholeClaim);
    assert!(all < 5 * one, "5 tokens batched: {all} checks against {one} for one token");
    assert!(
        all_cost.field_mults < 5 * one_cost.field_mults,
        "W r once per scope: {} vs {}",
        all_cost.field_mults,
        one_cost.field_mults
    );
    // Every param instance is opened once per scope, whatever the number of tokens.
    assert_eq!(all_cost.param_bytes, one_cost.param_bytes);
    assert!(all_cost.param_bytes > 0);
}

mod receipts {
    use super::*;
    use misaka_palw_kernel::receipt::{
        ClaimFactsV1, ConstraintTallyV1, ReceiptInputsV1, ReceiptRefusalV1, ReceiptSignatureVerifier, ReceiptVerdictV1,
        SignedConstraintReceiptV1, TallyPolicyV1, TallyStateV1, admit_receipt_v1, receipt_for_v1,
    };

    /// A stand-in signature: the message itself, keyed by the bond (the node's verifier is ML-DSA-87).
    struct Toy;
    impl ReceiptSignatureVerifier for Toy {
        fn verify(&self, bond: &[u8; 64], message: &[u8; 64], sig: &[u8]) -> bool {
            sig.len() == 128 && sig[..64] == bond[..] && sig[64..] == message[..]
        }
    }
    fn sign(r: misaka_palw_kernel::receipt::PalwConstraintReceiptV1) -> SignedConstraintReceiptV1 {
        let mut sig = r.seat_bond.to_vec();
        sig.extend_from_slice(&r.signing_message());
        SignedConstraintReceiptV1 { receipt: r, signature: sig }
    }

    fn seat(
        c: &Claim,
        committed: &TraceV1,
        scope: ScopeV1,
        bond: u8,
        operator: u8,
    ) -> misaka_palw_kernel::receipt::PalwConstraintReceiptV1 {
        let material = TraceMaterialV1 { trace: committed, params: &c.params };
        let (v, _) = c.verify_scope(committed, &material, &scope);
        let ev = c.evidence_of(committed);
        let d = k2_tir_v1_descriptor();
        receipt_for_v1(
            &ReceiptInputsV1 {
                descriptor: &d,
                evidence: &ev,
                claim_id: [8; 64],
                assignment_root: [5; 64],
                challenge_anchor: [0x42; 64],
                sample_seed: [0x43; 64],
                seat_bond: [bond; 64],
                seat_operator: [operator; 64],
                signed_daa: 100,
            },
            &scope,
            &v,
        )
        .unwrap()
    }

    #[test]
    fn segment_receipts_license_only_with_per_segment_quorum_and_audits_and_repeats_never_count() {
        let c = Claim::honest();
        let ev = c.evidence_of(&c.trace);
        let d = k2_tir_v1_descriptor();
        let assignments = vec![
            ([1; 64], ScopeV1::Segments(vec![0, 1])),
            ([2; 64], ScopeV1::Segments(vec![1, 2])),
            ([3; 64], ScopeV1::Segments(vec![0, 2])),
            ([4; 64], ScopeV1::AuditOnly(vec![0, 1, 2, 3, 4])),
            ([6; 64], ScopeV1::Segments(vec![0, 1])),
        ];
        let facts = ClaimFactsV1 {
            descriptor: &d,
            evidence: &ev,
            claim_id: [8; 64],
            assignment_root: [5; 64],
            challenge_anchor: [0x42; 64],
            sample_seed: [0x43; 64],
            assignments: &assignments,
            deadline_daa: 1_000,
        };
        let tally = std::cell::RefCell::new(ConstraintTallyV1::new(TallyPolicyV1 { per_segment_quorum: 2 }, &ev));
        let file = |r| {
            let s = sign(r);
            admit_receipt_v1(&s, &facts, &Toy).unwrap();
            tally.borrow_mut().add(&s.receipt, &ev)
        };
        assert!(file(seat(&c, &c.trace, ScopeV1::AuditOnly(vec![0, 1, 2, 3, 4]), 4, 4)));
        assert!(
            matches!(tally.borrow().state(), TallyStateV1::Incomplete { uncovered } if uncovered == vec![0, 1, 2]),
            "an audit is never coverage"
        );
        assert!(file(seat(&c, &c.trace, ScopeV1::Segments(vec![0, 1]), 1, 1)));
        assert!(!file(seat(&c, &c.trace, ScopeV1::Segments(vec![0, 1]), 1, 1)), "the same duty twice counts once");
        // Bond 6 is the same operator as bond 1: no independence gained.
        assert!(file(seat(&c, &c.trace, ScopeV1::Segments(vec![0, 1]), 6, 1)));
        assert!(matches!(tally.borrow().state(), TallyStateV1::Incomplete { .. }));
        assert!(file(seat(&c, &c.trace, ScopeV1::Segments(vec![1, 2]), 2, 2)));
        assert!(matches!(tally.borrow().state(), TallyStateV1::Incomplete { uncovered } if uncovered == vec![0, 2]));
        assert!(file(seat(&c, &c.trace, ScopeV1::Segments(vec![0, 2]), 3, 3)));
        assert_eq!(tally.borrow().state(), TallyStateV1::Covered);
    }

    #[test]
    fn a_failing_receipt_opens_a_dispute_no_later_pass_erases_and_equivocation_is_recorded() {
        let c = Claim::honest();
        let at = c.find(2, |p| matches!(p, Prim::MatMul));
        let lie = lie_at(&c, at, 0);
        let ev = c.evidence_of(&lie);
        let fail = seat(&c, &lie, ScopeV1::WholeClaim, 1, 1);
        assert!(matches!(fail.verdict, ReceiptVerdictV1::Fail { position: 2, .. }));
        let mut tally = ConstraintTallyV1::new(TallyPolicyV1 { per_segment_quorum: 1 }, &ev);
        tally.add(&fail, &ev);
        let mut pass = seat(&c, &c.trace, ScopeV1::WholeClaim, 2, 2);
        pass.evidence_manifest_root = ev.root();
        tally.add(&pass, &ev);
        assert!(matches!(tally.state(), TallyStateV1::Disputed { .. }), "a counted failure is not outvoted");
        let mut flip = fail.clone();
        flip.verdict = ReceiptVerdictV1::Pass;
        assert!(!tally.add(&flip, &ev));
        assert_eq!(tally.equivocations, vec![[1; 64]]);
    }

    #[test]
    fn the_fold_refuses_a_receipt_that_overstates_soundness_names_another_scope_or_is_late() {
        let c = Claim::honest();
        let ev = c.evidence_of(&c.trace);
        let d = k2_tir_v1_descriptor();
        let assignments = vec![([1; 64], ScopeV1::Segments(vec![0]))];
        let facts = ClaimFactsV1 {
            descriptor: &d,
            evidence: &ev,
            claim_id: [8; 64],
            assignment_root: [5; 64],
            challenge_anchor: [0x42; 64],
            sample_seed: [0x43; 64],
            assignments: &assignments,
            deadline_daa: 99,
        };
        let honest = seat(&c, &c.trace, ScopeV1::Segments(vec![0]), 1, 1);
        assert_eq!(admit_receipt_v1(&sign(honest.clone()), &facts, &Toy), Err(ReceiptRefusalV1::Late { signed: 100, deadline: 99 }));
        let facts = ClaimFactsV1 { deadline_daa: 1_000, ..facts };
        admit_receipt_v1(&sign(honest.clone()), &facts, &Toy).unwrap();
        let mut boast = honest.clone();
        boast.derived_soundness_bits += 1;
        assert_eq!(admit_receipt_v1(&sign(boast), &facts, &Toy), Err(ReceiptRefusalV1::WrongSoundness));
        let mut wider = honest.clone();
        wider.scope = ScopeV1::Segments(vec![0, 1]);
        assert_eq!(admit_receipt_v1(&sign(wider.clone()), &facts, &Toy), Err(ReceiptRefusalV1::WrongScope));
        wider.scope_root = wider.scope.root(&ev);
        assert_eq!(admit_receipt_v1(&sign(wider), &facts, &Toy), Err(ReceiptRefusalV1::NotAssigned));
        let mut forged = sign(honest.clone());
        forged.receipt.seat_bond = [2; 64];
        assert_eq!(admit_receipt_v1(&forged, &facts, &Toy), Err(ReceiptRefusalV1::BadSignature));
        let mut weak = honest;
        weak.freivalds_rounds = 1;
        assert_eq!(admit_receipt_v1(&sign(weak), &facts, &Toy), Err(ReceiptRefusalV1::WrongSoundness));
    }
}
