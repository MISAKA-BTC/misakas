//! Registered-class coverage, including all pipeline/typed model components.
//! This closes the plan-map boundary; it is not an implemented GKR/PCS/lookup
//! argument or authority to omit the exact retained relations/edge/state checks.
use super::*;
use crate::ledger::KernelLedgerV1;
use crate::spec::{SpecClassKindV1, composite::ComponentV1};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCompactPartV2 {
    /// Native pipeline stage / composite stage (zero for Program/Memory).
    pub stage: u16,
    /// Memory has up to max_steps separate executions; other stages run once.
    pub maximum_executions: u32,
    pub coverage: CompactPlanCoverageV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeExactBoundaryV2 {
    Program { plan: Digest },
    Pipeline { pipeline_root: Digest, plan_root: Digest },
    Memory { spec_root: Digest },
    Retrieval { stage: u16, snapshot: Digest, rule: Digest },
    Composite { spec_root: Digest },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCompactCoverageV2 {
    pub version: u16,
    pub base_class: Digest,
    pub target: PaidClaimTargetV2,
    pub params: CompactCompileParamsV1,
    pub parts: Vec<NativeCompactPartV2>,
    pub exact_boundaries: Vec<NativeExactBoundaryV2>,
    /// Full native reexecution/localization upper bound retained beside compact
    /// work. Exact nonlinear/routing/edges/state/snapshot work is never free.
    pub exact_verifier_work_upper: u128,
}
impl NativeCompactCoverageV2 {
    pub fn root(&self) -> Digest {
        object_id(b"MISAKA/PALW/NATIVE-COMPACT-COVERAGE/V2", self)
    }
    /// Recompile from the authenticated ledger, not caller-declared relation
    /// counts, stage roots or memory repetition multipliers.
    pub fn validate_against(&self, ledger: &KernelLedgerV1) -> Result<(), String> {
        if *self != compile_native_class_v2(ledger, self.base_class, self.params)? {
            return Err("compact native coverage differs from registered class/whole plan".into());
        }
        Ok(())
    }
    pub fn check_families(&self) -> Result<Vec<CheckFamilyV1>, String> {
        let mut families = Vec::new();
        for part in &self.parts {
            if part.maximum_executions == 0 {
                return Err("zero compact component execution bound".into());
            }
            for mut family in compact_check_families_v1(&part.coverage) {
                family.instances = family
                    .instances
                    .checked_mul(u128::from(part.maximum_executions))
                    .ok_or("compact whole-class instance overflow")?;
                families.push(family);
            }
        }
        Ok(families)
    }
    pub fn composed_bound(&self, binding_bits: u32, budget: &AdversaryBudgetV1) -> Result<ComposedBoundV1, String> {
        composed_false_accept_bits_v1(
            &self.check_families()?,
            InstanceCompositionV1::UnionBound,
            Some(binding_bits),
            CoverageV1::Complete,
            budget,
        )
        .map_err(|e| e.to_string())
    }
    pub fn lifetime_term(
        &self,
        binding_bits: u32,
        budget: &AdversaryBudgetV1,
        admissions: u128,
    ) -> Result<CumulativeFailureTermV1, String> {
        if budget.adaptive_statements != 1 || admissions == 0 {
            return Err("lifetime admission Q must be charged exactly once".into());
        }
        let b = self.composed_bound(binding_bits, budget)?.effective_bits.ok_or("missing compact binding failure term")?;
        if !(0..=u16::MAX as i64).contains(&b) {
            return Err("compact failure bits not representable".into());
        }
        Ok(CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Bits(b as u16), opportunities: admissions })
    }
    pub fn limits(&self) -> Result<(u128, u128), String> {
        let (mut bytes, mut work) = (0u128, self.exact_verifier_work_upper);
        for part in &self.parts {
            let positions = u128::from(part.coverage.max_positions)
                .checked_mul(u128::from(part.maximum_executions))
                .ok_or("compact execution bound overflow")?;
            bytes = bytes
                .checked_add(part.coverage.proof_bytes_per_position().checked_mul(positions).ok_or("compact proof bound overflow")?)
                .ok_or("compact aggregate proof bound overflow")?;
            work = work
                .checked_add(
                    part.coverage.verifier_work_per_position().checked_mul(positions).ok_or("compact verifier bound overflow")?,
                )
                .ok_or("compact aggregate verifier bound overflow")?;
        }
        Ok((bytes, work))
    }
}
/// Every component is derived from the native registered class. Typed retrieval
/// has no probabilistic tensor relation; exact snapshot/ranking/query checks are
/// explicit obligations. Memory continuity and pipeline/composite edges likewise
/// remain exact, and cannot disappear from a coverage root.
pub fn compile_native_class_v2(
    ledger: &KernelLedgerV1,
    class: Digest,
    params: CompactCompileParamsV1,
) -> Result<NativeCompactCoverageV2, String> {
    params.validate().map_err(|e| e.to_string())?;
    let mut result = NativeCompactCoverageV2 {
        version: 2,
        base_class: class,
        target: PaidClaimTargetV2::Program,
        params,
        parts: Vec::new(),
        exact_boundaries: Vec::new(),
        exact_verifier_work_upper: 0,
    };
    if let Some(rule) = ledger.classes.get(&class) {
        result.target = if crate::descriptor::is_segmented_v1(&rule.descriptor) {
            PaidClaimTargetV2::Segmented
        } else {
            PaidClaimTargetV2::Program
        };
        let coverage = compile_plan_v1(result.target, &rule.program, &rule.plan, &params).map_err(|e| e.to_string())?;
        add_exact_work(&mut result, crate::plan::reexecution_bounds_v1(&rule.program, &rule.plan).0)?;
        result.exact_boundaries.push(NativeExactBoundaryV2::Program { plan: rule.plan.root() });
        result.parts.push(NativeCompactPartV2 { stage: 0, maximum_executions: 1, coverage });
    } else if let Some(rule) = ledger.pipeline_classes.get(&class) {
        result.target = PaidClaimTargetV2::Pipeline;
        if rule.pipeline.stages.len() != rule.plan.stages.len() {
            return Err("native pipeline stage/plan cardinality mismatch".into());
        }
        for (s, stage) in rule.pipeline.stages.iter().enumerate() {
            let program = rule.programs.get(stage.program as usize).ok_or("native pipeline program missing")?;
            let view = crate::pipeline::stage_view_v1(program);
            let coverage =
                compile_plan_v1(PaidClaimTargetV2::Pipeline, &view.view, &rule.plan.stages[s], &params).map_err(|e| e.to_string())?;
            add_exact_work(&mut result, crate::plan::reexecution_bounds_v1(&view.view, &rule.plan.stages[s]).0)?;
            result.parts.push(NativeCompactPartV2 { stage: s as u16, maximum_executions: 1, coverage });
        }
        result.exact_boundaries.push(NativeExactBoundaryV2::Pipeline {
            pipeline_root: crate::pipeline::pipeline_root_v1(&rule.pipeline, &rule.programs),
            plan_root: rule.plan.root(),
        });
    } else if let Some(rule) = ledger.typed.classes.get(&class) {
        result.target = PaidClaimTargetV2::TypedSpec;
        match &rule.kind {
            SpecClassKindV1::Memory { rule: memory, root, .. } => {
                let coverage =
                    compile_plan_v1(PaidClaimTargetV2::Program, &memory.program, &memory.plan, &params).map_err(|e| e.to_string())?;
                add_exact_work(
                    &mut result,
                    crate::plan::reexecution_bounds_v1(&memory.program, &memory.plan)
                        .0
                        .checked_mul(u128::from(root.max_steps))
                        .ok_or("memory exact work overflow")?,
                )?;
                result.parts.push(NativeCompactPartV2 { stage: 0, maximum_executions: root.max_steps, coverage });
                result
                    .exact_boundaries
                    .push(NativeExactBoundaryV2::Memory { spec_root: object_id(b"MISAKA/PALW/COMPACT-TYPED-SPEC/V2", &rule.spec) });
            }
            SpecClassKindV1::Retrieval { root } => {
                add_exact_work(&mut result, crate::spec::retrieval_reexecution_work_v1(root))?;
                result.exact_boundaries.push(NativeExactBoundaryV2::Retrieval {
                    stage: 0,
                    snapshot: root.snapshot.root(),
                    rule: object_id(b"MISAKA/PALW/COMPACT-RETRIEVAL-RULE/V2", root),
                });
            }
            SpecClassKindV1::Composite { root, components } => {
                if root.stages.len() != components.len() {
                    return Err("native composite component cardinality mismatch".into());
                }
                result
                    .exact_boundaries
                    .push(NativeExactBoundaryV2::Composite { spec_root: object_id(b"MISAKA/PALW/COMPACT-TYPED-SPEC/V2", &rule.spec) });
                for (s, component) in components.iter().enumerate() {
                    add_exact_work(
                        &mut result,
                        match component {
                            ComponentV1::Model(model) => crate::plan::reexecution_bounds_v1(&model.program, &model.plan).0,
                            ComponentV1::Retrieval(root) => crate::spec::retrieval_reexecution_work_v1(root),
                        },
                    )?;
                    match component {
                        ComponentV1::Model(model) => result.parts.push(NativeCompactPartV2 {
                            stage: s as u16,
                            maximum_executions: 1,
                            coverage: compile_plan_v1(PaidClaimTargetV2::Program, &model.program, &model.plan, &params)
                                .map_err(|e| e.to_string())?,
                        }),
                        ComponentV1::Retrieval(root) => result.exact_boundaries.push(NativeExactBoundaryV2::Retrieval {
                            stage: s as u16,
                            snapshot: root.snapshot.root(),
                            rule: object_id(b"MISAKA/PALW/COMPACT-RETRIEVAL-RULE/V2", root),
                        }),
                    }
                }
            }
        }
    } else {
        return Err("compact coverage names no native registered class".into());
    }
    result.check_families()?;
    result.limits()?;
    Ok(result)
}

/// Explicit whole-class contract; numeric caps and claimed security are supplied
/// by a prospective registrant/reviewer, never filled from a mechanics default.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCompactContractV2 {
    pub version: u16,
    pub base_class: Digest,
    pub coverage_root: Digest,
    pub binding_bits: u16,
    pub claimed_soundness_bits: u16,
    pub maximum_proof_bytes: u128,
    pub maximum_verifier_work: u128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompactContractSummaryV2 {
    pub effective_bits: i64,
    pub proof_bytes: u128,
    pub verifier_work: u128,
}
impl NativeCompactContractV2 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/NATIVE-COMPACT-CONTRACT/V2", self)
    }
    pub fn suite_id(&self, coverage: &NativeCompactCoverageV2) -> Digest {
        let mut compilers: Vec<_> =
            coverage.parts.iter().flat_map(|p| p.coverage.compact()).map(|p| compact_relation_compiler_id_v1(p.compiler)).collect();
        compilers.sort();
        compilers.dedup();
        object_id(b"MISAKA/PALW/NATIVE-COMPACT-SUITE/V2", &(self.id(), coverage.root(), compilers))
    }
    pub fn check(
        &self,
        ledger: &KernelLedgerV1,
        coverage: &NativeCompactCoverageV2,
        budget: &AdversaryBudgetV1,
    ) -> Result<NativeCompactContractSummaryV2, String> {
        coverage.validate_against(ledger)?;
        if self.version != 2
            || self.base_class != coverage.base_class
            || self.coverage_root != coverage.root()
            || self.binding_bits == 0
            || self.claimed_soundness_bits == 0
            || self.maximum_verifier_work == 0
        {
            return Err("unversioned/unbound/unbounded native compact contract".into());
        }
        let (proof_bytes, verifier_work) = coverage.limits()?;
        let effective_bits = coverage
            .composed_bound(u32::from(self.binding_bits), budget)?
            .effective_bits
            .ok_or("compact effective bound unavailable")?;
        if proof_bytes > self.maximum_proof_bytes
            || verifier_work > self.maximum_verifier_work
            || effective_bits < i64::from(self.claimed_soundness_bits)
        {
            return Err("native compact contract exceeds derived proof/work/security bound".into());
        }
        Ok(NativeCompactContractSummaryV2 { effective_bits, proof_bytes, verifier_work })
    }
}
/// Requires actual registered native coverage before an independent exact suite
/// review is even considered. A contract with only exact retrieval obligations
/// may carry zero compact proof bytes; its exact checker remains mandatory.
pub fn register_native_compact_contract_v2(
    contract: &NativeCompactContractV2,
    ledger: &KernelLedgerV1,
    coverage: &NativeCompactCoverageV2,
    budget: &AdversaryBudgetV1,
    registry: &[ReviewedTupleV2],
    policy: &PostCommitChallengePolicyV1,
) -> Result<NativeCompactContractSummaryV2, String> {
    let summary = contract.check(ledger, coverage, budget)?;
    misaka_palw_challenge::approved_v2(registry, &contract.suite_id(coverage), policy)
        .map_err(|e| format!("native compact whole-class suite is not reviewed: {e}"))?;
    Ok(summary)
}

fn add_exact_work(coverage: &mut NativeCompactCoverageV2, work: u128) -> Result<(), String> {
    if work == u128::MAX {
        return Err("native exact verifier bound saturated".into());
    }
    coverage.exact_verifier_work_upper =
        coverage.exact_verifier_work_upper.checked_add(work).ok_or("native exact verifier union overflow")?;
    Ok(())
}
