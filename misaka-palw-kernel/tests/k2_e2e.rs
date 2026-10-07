//! K2-TIR-v1 end to end on a real TIR class (a dense GQA attention layer over a KV history, a SwiGLU MLP, a top-2-of-4 MoE
//! layer gathered by a committed TopK, a head): registration outcomes, an honest claim's pass, a producer's lie localized to one
//! primitive instance and convicted by a court that reads only public material, the DA path kept apart from fraud, and every
//! way a plan can try to weaken its own check.

use misaka_palw_kernel::check::{check_plan_v1, registration_outcome_v1};
use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor};
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1};
use misaka_palw_kernel::outcome::{CoverageBucketV1, RegistrationOutcomeV1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1, trace_v1};
use misaka_palw_kernel::verify::{
    ClaimContextV1, ClaimVerdictV1, DismissalV1, FaultKindV1, MaterialV1, TraceMaterialV1, verify_claim_v1, verify_fault_proof_v1,
};
use misaka_palw_kernel::{challenge::ChallengeBindingV1, hash::id};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{Interpreter, MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::{dense_moe_v1, wide_v1};

const TOKENS: [u32; 5] = [3, 17, 9, 30, 1];
const MAX_POSITIONS: u32 = 64;

fn root_of(program: &TirProgramV1) -> [u8; 64] {
    id(b"test/program", &program.encode())
}

fn active() -> KernelScheduleV1 {
    KernelScheduleV1::default().with(k2_tir_v1_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 })
}

struct Claim {
    program: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    trace: TraceV1,
    pc: ParamCommitmentsV1,
}

impl Claim {
    fn honest() -> Self {
        let fx = dense_moe_v1(7);
        let d = k2_tir_v1_descriptor();
        let plan = plan_for_tir_program_v1(&d, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
        let trace = trace_v1(&fx.program, &fx.params, &TOKENS).unwrap();
        let pc = ParamCommitmentsV1::of(&fx.params);
        Claim { program: fx.program, params: fx.params, plan, trace, pc }
    }

    /// Verify `trace` as the producer's COMMITTED values, served by `material`.
    fn verify_with(
        &self,
        committed: &TraceV1,
        material: &dyn MaterialV1,
    ) -> (
        ClaimVerdictV1,
        Box<dyn Fn(&misaka_palw_kernel::KernelFaultProofV1) -> Result<misaka_palw_kernel::verify::ConvictionV1, DismissalV1>>,
    ) {
        let d = k2_tir_v1_descriptor();
        let evidence = committed.evidence();
        let binding = ChallengeBindingV1 {
            network_domain: [9; 64],
            claim_id: [8; 64],
            class_binding_id: [7; 64],
            plan_root: self.plan.root(),
            evidence_root: evidence.root(),
            beacon: [0x42; 64],
        };
        let ctx = ClaimContextV1 {
            descriptor: &d,
            program: &self.program,
            plan: &self.plan,
            evidence: &evidence,
            params: &self.pc,
            tokens: &TOKENS,
            claimed_output_root: evidence.output_root(&self.program),
            binding,
        };
        let verdict = verify_claim_v1(&ctx, material);
        // The court, as a fresh party holding only the public material (the evidence, the param commitments, the tokens).
        let (program, plan, pc) = (self.program.clone(), self.plan.clone(), self.pc.clone());
        let court = move |proof: &misaka_palw_kernel::KernelFaultProofV1| {
            let d = k2_tir_v1_descriptor();
            let ctx = ClaimContextV1 {
                descriptor: &d,
                program: &program,
                plan: &plan,
                evidence: &evidence,
                params: &pc,
                tokens: &TOKENS,
                claimed_output_root: evidence.output_root(&program),
                binding,
            };
            verify_fault_proof_v1(&ctx, proof)
        };
        (verdict, Box::new(court))
    }

    /// The first `(position, occurrence, node)` whose primitive satisfies `pick`, from position `from`.
    fn find(&self, from: u32, pick: impl Fn(&Prim) -> bool) -> (u32, u16, u16) {
        let occ = self.program.occurrences();
        for p in from..TOKENS.len() as u32 {
            for (s, (b, _)) in occ.iter().enumerate() {
                for (n, node) in self.program.blocks[*b as usize].nodes.iter().enumerate() {
                    if pick(&node.prim) {
                        return (p, s as u16, n as u16);
                    }
                }
            }
        }
        panic!("no such node")
    }
}

fn bump(t: &mut Tensor, at: usize) {
    // Stay inside the dtype so the lie is arithmetic, not a malformed value.
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

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
    assert_eq!(shipped.coverage_bucket(false), CoverageBucketV1::KernelExtensionGap);
    let armed = registration_outcome_v1(&active(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 1_000);
    let RegistrationOutcomeV1::EligibleAt { error_bits, .. } = armed else { panic!("{armed}") };
    assert!(error_bits >= 128, "the derived whole-claim error reaches the target: 2^-{error_bits}");
    assert_eq!(armed.coverage_bucket(false), CoverageBucketV1::Untested, "eligibility without on-chain evidence is not coverage");
    assert!(armed.coverage_bucket(true).counts_as_success());
}

#[test]
fn an_i128_accumulator_and_a_missing_family_are_kernel_extensions_not_successes() {
    let d = k2_tir_v1_descriptor();
    let fx = wide_v1(3);
    let o = registration_outcome_v1(&active(), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 0);
    let RegistrationOutcomeV1::KernelExtensionRequired { family, .. } = &o else { panic!("{o}") };
    assert_eq!(*family, Some(ConstraintFamilyV1::DenseMatrix), "{o}");

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
    let ClaimVerdictV1::Pass { probabilistic_instances, error_bits } = v else { panic!("{v:?}") };
    assert!(probabilistic_instances > 0);
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
    let evidence = c.trace.evidence();
    let ctx = ClaimContextV1 {
        descriptor: &d,
        program: &c.program,
        plan: &c.plan,
        evidence: &evidence,
        params: &c.pc,
        tokens: &TOKENS,
        claimed_output_root: evidence.output_root(&c.program),
        binding: ChallengeBindingV1 {
            network_domain: [9; 64],
            claim_id: [8; 64],
            class_binding_id: [7; 64],
            plan_root: c.plan.root(),
            evidence_root: [0; 64],
            beacon: [1; 64],
        },
    };
    let v = verify_claim_v1(&ctx, &TraceMaterialV1 { trace: &c.trace, params: &c.params });
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
