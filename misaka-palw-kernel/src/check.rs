//! **The deterministic plan checker** (RFC-0005 §K.4 step 2, RFC-0011 §16.2–§16.3).
//!
//! Everything a plan states is re-derived here from the program and the descriptor and compared:
//!
//! 1. the descriptor the plan names is this one (else `PLAN_FORGED`);
//! 2. the program validates, its ranges are proven (the exact-result rule), and it is in the descriptor's primitive set (else
//!    `FRONTEND_REQUIRED` / `KERNEL_EXTENSION_REQUIRED`), and the plan is about this program;
//! 3. every node of every scheduled block has exactly one relation, with the descriptor's checker,
//!    court and repetitions, the right dimensions, and nothing extra (else `INCOMPLETE_COVERAGE`);
//! 4. every family is implemented (else `KERNEL_EXTENSION_REQUIRED`, naming the node) and every
//!    `MatMul` satisfies its checker's alias bound: an integer error stays nonzero in GF(2^127 − 1), or (multi-modulus) modulo
//!    one of the moduli the relation uses;
//! 5. every state has its boundary;
//! 6. the budgets agree with the derivation and every total fits the descriptor's ceilings;
//! 7. the whole-claim error is derived (`t·126 − ⌈log2 R⌉`, union with the binding term), must reach
//!    the descriptor's target, and the declaration may not exceed it;
//! 8. last, the descriptor is **Active** at the DAA (else `KERNEL_NOT_ACTIVE`, which therefore always means "expressible, not active").
//!
//! Static admission does not assert that future outputs are correct (RFC-0005 §K.4).

use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, Prim};

use crate::descriptor::{KernelDescriptorV1, KernelScheduleV1, KernelStandingV1};
use crate::family::CheckerIdV1;
use crate::family::{ConstraintFamilyV1, family_of_prim};
use crate::field::P;
use crate::hash::Digest;
use crate::outcome::RegistrationOutcomeV1 as O;
use crate::plan::{
    PLAN_GRAMMAR_V1, VerificationPlanV1, block_occurrences, dense_moduli_v2, derive_budgets, derived_error_bits, dtype_of_tag,
    expected_boundaries, expected_relation, matmul_error_span,
};

/// What a plan that passed carries forward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanAcceptanceV1 {
    pub plan_root: Digest,
    pub error_bits: u16,
    pub claim_verifier_work: u128,
    pub claim_evidence_bytes: u128,
}

pub fn check_plan_v1(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    program_root: Digest,
    plan: &VerificationPlanV1,
    daa: u64,
) -> Result<PlanAcceptanceV1, O> {
    check_plan_with_v1(schedule, descriptor, program, program_root, plan, daa, RangeRuleV1::TirV1)
}

/// Which range analysis proves the exact-result rule for the program checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeRuleV1 {
    /// PALW-TIR v1's analysis over the program itself.
    TirV1,
    /// The program is a TIR v2 stage's version-1 view whose ranges the caller proved with the v2 analysis (its inputs carry
    /// declared intervals the view's params do not).
    ProvenByV2,
}

/// [`check_plan_v1`], with the range rule named.
pub fn check_plan_with_v1(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    program_root: Digest,
    plan: &VerificationPlanV1,
    daa: u64,
    ranges: RangeRuleV1,
) -> Result<PlanAcceptanceV1, O> {
    // 1. The descriptor and its standing.
    let digest = descriptor.digest();
    if plan.descriptor_digest != digest {
        return Err(O::PlanForged { why: "the plan names another kernel descriptor".into() });
    }
    descriptor.well_formed().map_err(|why| O::PlanForged { why: format!("descriptor: {why}") })?;
    if plan.grammar != PLAN_GRAMMAR_V1 {
        return Err(O::KernelExtensionRequired {
            family: None,
            relation: format!("plan grammar {}", plan.grammar),
            required: format!("grammar {}", plan.grammar),
            available: format!("grammar {PLAN_GRAMMAR_V1}"),
        });
    }
    let plan_bytes = plan.encoded_len() as u64;
    if plan_bytes > descriptor.limits.max_plan_bytes {
        return Err(O::BoundsExceeded {
            what: "plan bytes",
            required: plan_bytes as u128,
            limit: descriptor.limits.max_plan_bytes as u128,
        });
    }
    // 2. The program.
    if program.prim_set_id != descriptor.primitive_set_id {
        return Err(O::KernelExtensionRequired {
            family: None,
            relation: "the program's primitive set".into(),
            required: format!("prim set {}…", crate::hash::hex(&program.prim_set_id[..8])),
            available: format!("prim set {}…", crate::hash::hex(&descriptor.primitive_set_id[..8])),
        });
    }
    misaka_palw_tir::validate::validate(program)
        .map_err(|e| O::FrontendRequired { reason: format!("the program does not validate: {e}") })?;
    // The exact-result rule (RFC-0002 spec 04b §7): every partial sum of every exact primitive must provably fit its type. A
    // probabilistic check compares a claimed output with the mathematical integer result; it cannot see a partial-sum overflow the
    // reference semantics refuses, so a program whose ranges are not proven is the frontend's to narrow, never a kernel success.
    if ranges == RangeRuleV1::TirV1 {
        misaka_palw_tir::interval::analyze_ranges(program).map_err(|e| O::FrontendRequired {
            reason: format!("the program's ranges are not proven (an exact primitive can overflow): {e}"),
        })?;
    }
    if plan.program_root != program_root {
        return Err(O::PlanForged { why: "the plan is about another program".into() });
    }
    if plan.max_positions == 0 || plan.max_positions > program.history_bound || plan.max_positions > descriptor.limits.max_positions {
        return Err(O::BoundsExceeded {
            what: "positions",
            required: plan.max_positions as u128,
            limit: program.history_bound.min(descriptor.limits.max_positions) as u128,
        });
    }
    // 3–4. Coverage, family support and the alias bound, node by node.
    let occ = block_occurrences(program);
    let mut expected = Vec::new();
    for (b, block) in program.blocks.iter().enumerate() {
        if occ[b] == 0 {
            continue;
        }
        for (ni, node) in block.nodes.iter().enumerate() {
            let rel = expected_relation(descriptor, program, b, ni, occ[b]).map_err(|(family, why)| O::KernelExtensionRequired {
                family: Some(family),
                relation: format!("block {b} node {ni} ({})", node.prim.name()),
                required: format!("a {} relation", family.name()),
                available: why,
            })?;
            if let (Prim::MatMul, Some(d)) = (&node.prim, rel.matmul) {
                let dt = |t: u8| dtype_of_tag(t).unwrap_or(DType::I128);
                let (a, bt, out) = (dt(rel.in_dtypes[0]), dt(rel.in_dtypes[1]), dt(rel.out_dtype));
                let expressible = match rel.checker {
                    CheckerIdV1::FreivaldsCrtV2 => dense_moduli_v2(d.k, a, bt, out).is_some(),
                    _ => matmul_error_span(d.k, a, bt, out).is_some_and(|s| s < P),
                };
                if !expressible {
                    return Err(O::KernelExtensionRequired {
                        family: Some(ConstraintFamilyV1::DenseMatrix),
                        relation: format!(
                            "block {b} node {ni} (MatMul {}×{}→{}, k {})",
                            dt(rel.in_dtypes[0]).name(),
                            dt(rel.in_dtypes[1]).name(),
                            dt(rel.out_dtype).name(),
                            d.k
                        ),
                        required: match rel.checker {
                            CheckerIdV1::FreivaldsCrtV2 => "an integer error span below (2^127 − 1)(2^107 − 1)(2^89 − 1)".into(),
                            _ => "an integer error span below 2^127 − 1 (or a multi-modulus dense-matrix relation)".into(),
                        },
                        available: match rel.checker {
                            CheckerIdV1::FreivaldsCrtV2 => "the three-modulus Freivalds relation".into(),
                            _ => "one GF(2^127 − 1) Freivalds relation".into(),
                        },
                    });
                }
            }
            debug_assert_eq!(family_of_prim(&node.prim), rel.family);
            expected.push(rel);
        }
    }
    if plan.relations.len() as u64 > descriptor.limits.max_relations as u64 {
        return Err(O::BoundsExceeded {
            what: "relations",
            required: plan.relations.len() as u128,
            limit: descriptor.limits.max_relations as u128,
        });
    }
    for rel in &plan.relations {
        if !expected.iter().any(|e| e.block == rel.block && e.node == rel.node) {
            return Err(O::IncompleteCoverage {
                what: format!("block {} node {}: a relation the program does not have", rel.block, rel.node),
            });
        }
    }
    for e in &expected {
        let mine: Vec<_> = plan.relations.iter().filter(|r| r.block == e.block && r.node == e.node).collect();
        match mine.as_slice() {
            [] => {
                return Err(O::IncompleteCoverage {
                    what: format!("block {} node {}: omitted (no relation checks it)", e.block, e.node),
                });
            }
            [one] if *one == e => {}
            [one] if one.checker != e.checker || one.repetitions < e.repetitions => {
                return Err(O::PlanForged {
                    why: format!(
                        "block {} node {}: checker {:?}×{} where the kernel requires {:?}×{}",
                        e.block, e.node, one.checker, one.repetitions, e.checker, e.repetitions
                    ),
                });
            }
            [_] => {
                return Err(O::IncompleteCoverage {
                    what: format!("block {} node {}: the relation's shape or family differs from the program's", e.block, e.node),
                });
            }
            _ => return Err(O::IncompleteCoverage { what: format!("block {} node {}: covered twice", e.block, e.node) }),
        }
    }
    // 5. Boundaries.
    if plan.boundaries != expected_boundaries(program) {
        return Err(O::IncompleteCoverage { what: "the state boundaries are not one per state, as declared".into() });
    }
    // 6. Budgets and ceilings.
    let budgets = derive_budgets(program, &expected, plan.max_positions);
    if plan.budgets != budgets {
        return Err(O::IncompleteCoverage { what: "the declared budgets are not the derived ones".into() });
    }
    let positions = plan.max_positions as u128;
    let claim_work = budgets.verifier_work_per_position.saturating_mul(positions);
    let claim_bytes = budgets.evidence_bytes_per_position.saturating_mul(positions).saturating_add(budgets.artifact_bytes);
    let l = &descriptor.limits;
    for (what, required, limit) in [
        ("claim verifier work", claim_work, l.max_claim_verifier_work),
        ("claim evidence bytes", claim_bytes, l.max_claim_evidence_bytes),
        ("court bytes", budgets.worst_court_bytes as u128, l.max_court_bytes as u128),
        ("court work", budgets.worst_court_work as u128, l.max_court_work as u128),
    ] {
        if required > limit {
            return Err(O::BoundsExceeded { what, required, limit });
        }
    }
    // 7. The derived error.
    let instances = budgets.probabilistic_instances_per_position as u128 * positions;
    let error_bits = derived_error_bits(descriptor, instances);
    if error_bits < descriptor.soundness.target_bits {
        return Err(O::BoundsExceeded {
            what: "whole-claim error bits (below the target)",
            required: descriptor.soundness.target_bits as u128,
            limit: error_bits as u128,
        });
    }
    if plan.declared_error_bits > error_bits {
        return Err(O::PlanForged {
            why: format!("declares ε ≤ 2^-{} where the suite derives 2^-{error_bits}", plan.declared_error_bits),
        });
    }
    // 8. Last: the descriptor's standing. `KERNEL_NOT_ACTIVE` means "this descriptor expresses the whole task within its bounds and is
    // not active" (RFC-0011 §16.2); a missing capability or a bound is reported as such whatever the schedule says.
    match schedule.standing_at(&digest, daa) {
        KernelStandingV1::Active => {}
        KernelStandingV1::NotActive(status) => return Err(O::KernelNotActive { descriptor: digest, status: Some(status) }),
        KernelStandingV1::Unknown => return Err(O::KernelNotActive { descriptor: digest, status: None }),
    }
    Ok(PlanAcceptanceV1 { plan_root: plan.root(), error_bits, claim_verifier_work: claim_work, claim_evidence_bytes: claim_bytes })
}

/// **Registration's static outcome** for a TIR v1 program: the reference plan, checked. A caller with a
/// plan from elsewhere calls [`check_plan_v1`] on it directly.
pub fn registration_outcome_v1(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    program_root: Digest,
    max_positions: u32,
    daa: u64,
) -> O {
    let plan = match crate::plan::plan_for_tir_program_v1(descriptor, program, program_root, max_positions) {
        Ok(p) => p,
        Err((family, why)) => {
            return O::KernelExtensionRequired {
                family: Some(family),
                relation: why.clone(),
                required: format!("a {} relation", family.name()),
                available: why,
            };
        }
    };
    match check_plan_v1(schedule, descriptor, program, program_root, &plan, daa) {
        Ok(a) => O::EligibleAt { daa, descriptor: descriptor.digest(), plan_root: a.plan_root, error_bits: a.error_bits },
        Err(o) => o,
    }
}
