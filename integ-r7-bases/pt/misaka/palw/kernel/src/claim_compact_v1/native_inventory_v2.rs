//! **RFC-0007 item 1: the per-class check inventory.** For every relation of every stage (and every step of a memory stage) of a
//! registered native class, which of the three permitted dispositions covers it:
//!
//! * **exact recomputation** — the verifier re-executes the relation deterministically over authenticated operands (`ε = 0`);
//! * **direct check** — Freivalds / linear evaluation over operands the verifier itself reads, with a stated per-repetition miss
//!   bound for a single-element (sparse) error, and the exact relation as its localizer and court
//!   ([`super::exact_relation_fault_v1`]);
//! * **refusal** — the relation cannot be covered; the class's inventory names it.
//!
//! **Sparse-error rule.** A direct check over integer/modular arithmetic states its per-repetition miss bound for an output with
//! exactly one wrong element ([`sparse_single_error_bound_v2`]). Where that bound is not established — the integer error span is
//! not below the product of the moduli the check uses (so a single wrong element can vanish modulo every one of them), or the
//! bound computes to zero bits — the relation falls back to exact recomputation, never to a weaker check.
//!
//! **Public refutation routes.** Routing (TopK / compare / select / reduce-max), history (state write, history append, the plan's
//! state boundaries, memory step continuity) and gather (index gather, snapshot retrieval) each name the court a refutation of
//! them reaches and its bounded adjudication cost: the derived worst court filing of the authenticated plan (re-derived by
//! `check_plan_v1` at registration) and a named measured constant, **UNKNOWN until measured**.
//!
//! PRINCIPLES §8: nothing here is, carries or credits a cryptographic proof of computation correctness. Hashes and Merkle paths
//! only authenticate the operands; the verdict is always the deterministic recomputation. The stated bounds are conditional on an
//! honest capable verifier actually running the check, on available material and on timely court submission — separate,
//! explicit assumptions, never consequences of a bit count. This is a read API: it grants no receipt, credit, weight or Final.
use super::native_v2::NativeExactBoundaryV2;
use super::*;
use crate::family::CourtIdV1;
use crate::ledger::KernelLedgerV1;
use crate::spec::{SpecClassKindV1, composite::ComponentV1};
use std::collections::BTreeSet;

/// UNKNOWN until measured: one routing refutation (a TopK / compare / select / reduce-max relation) adjudicated by the native
/// court its plan names, µs on the reference verifier. Read by no rule.
pub const NATIVE_ROUTE_MEASURED_US_ROUTING_V2: Option<u64> = None;
/// UNKNOWN until measured: one history refutation (state write, history append, state boundary, memory step continuity), µs.
pub const NATIVE_ROUTE_MEASURED_US_HISTORY_V2: Option<u64> = None;
/// UNKNOWN until measured: one gather refutation (index gather, snapshot retrieval), µs.
pub const NATIVE_ROUTE_MEASURED_US_GATHER_V2: Option<u64> = None;

/// The public refutation routes RFC-0007 item 1 requires to reach a court with a bounded adjudication cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeRefutationRouteV2 {
    Routing,
    History,
    Gather,
}

impl NativeRefutationRouteV2 {
    pub const ALL: [Self; 3] = [Self::Routing, Self::History, Self::Gather];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Routing => "routing",
            Self::History => "history",
            Self::Gather => "gather",
        }
    }

    /// The route's measured adjudication cost (µs) — `None` is UNKNOWN.
    pub const fn measured_us(self) -> Option<u64> {
        match self {
            Self::Routing => NATIVE_ROUTE_MEASURED_US_ROUTING_V2,
            Self::History => NATIVE_ROUTE_MEASURED_US_HISTORY_V2,
            Self::Gather => NATIVE_ROUTE_MEASURED_US_GATHER_V2,
        }
    }
}

/// The route a relation's primitive puts it on (`None`: an ordinary relation, refuted through the same court without a route).
pub fn native_relation_route_v2(prim: &Prim) -> Option<NativeRefutationRouteV2> {
    match prim {
        Prim::TopK { .. } | Prim::Compare { .. } | Prim::Select | Prim::ReduceMax { .. } => Some(NativeRefutationRouteV2::Routing),
        Prim::Gather { .. } => Some(NativeRefutationRouteV2::Gather),
        Prim::StateWrite { .. } | Prim::HistAppend { .. } => Some(NativeRefutationRouteV2::History),
        _ => None,
    }
}

/// Why a relation is covered by exact recomputation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeExactReasonV2 {
    /// The family's checker is exact (structure, quant/range, nonlinear, selection, state, edges; `Mul`/`ReduceSum`/`Cast`).
    FamilyExact,
    /// The registrant did not opt the exact-arithmetic family into direct checks.
    NotOptedIn,
    /// A direct check would carry no fewer bytes than the material it replaces.
    DirectCheckNotCheaper,
    /// A one-element (`ν = 0`) identity: over verifier-read operands whose error span is below the modulus it IS the integer identity.
    ScalarIdentity,
    /// The sparse-error fallback: no per-repetition miss bound is established for a single-element error.
    SparseErrorUnbounded,
}

/// A direct check's stated sparse-error bound: `−log2` of the probability that a check misses an output with exactly one wrong
/// element (a nonzero integer error inside the relation's error span).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSparseErrorBoundV2 {
    /// One repetition (every modulus of it, each from its own post-commit stream).
    pub per_repetition_miss_bits: u32,
    /// One instance: the repetitions draw independent streams, so their bits add.
    pub instance_miss_bits: u32,
}

/// How one relation is covered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRelationCheckV2 {
    Exact { reason: NativeExactReasonV2 },
    Direct { compiler: CompactRelationCompilerV1, moduli: Vec<u32>, repetitions: u8, sparse: NativeSparseErrorBoundV2 },
    Refused { why: String },
}

/// The integer error span of a direct check stays below the product of the moduli it uses — the precondition of every sparse bound:
/// a nonzero error `e` with `|e| < Π p_i` is nonzero modulo at least one `p_i` (the moduli are distinct primes).
fn span_below_moduli_v2(p: &CompactRelationProgramV1, rel: &PlanRelationV1) -> bool {
    let dt = |t: Option<&u8>| t.and_then(|t| dtype_of_tag(*t));
    let (Some(a), Some(b), Some(out)) = (dt(rel.in_dtypes.first()), dt(rel.in_dtypes.get(1)), dtype_of_tag(rel.out_dtype)) else {
        return false;
    };
    let needed = match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => {
            let Some(d) = rel.matmul else { return false };
            // `FreivaldsM127` declares one modulus whatever the span: the bound holds only where the span is below it.
            if crate::plan::matmul_error_span(d.k, a, b, out).is_some_and(|s| s < crate::field::P) {
                Some(1)
            } else {
                crate::plan::dense_moduli_v2(d.k, a, b, out).filter(|n| *n >= 2)
            }
        }
        CompactRelationCompilerV1::LinearEvaluationV1 => {
            let exact = max_abs(a).checked_add(max_abs(b)).and_then(|v| v.checked_add(max_abs(out)));
            let bits = ceil_log2(max_abs(a)).max(ceil_log2(max_abs(b))).max(ceil_log2(max_abs(out))) + 2;
            moduli_for_span(exact, bits)
        }
        CompactRelationCompilerV1::SumCheckHadamardV1 | CompactRelationCompilerV1::SumCheckReduceSumV1 => None,
    };
    needed.is_some_and(|n| n <= p.moduli.len())
}

/// **The per-repetition miss bound of a compiled direct check for a single-element error**, or `None` where it is not bounded.
///
/// * **Freivalds** over `F_p` (each `r_c` rejection-sampled uniform): an output with one wrong element `e` at column `c` passes
///   iff `e·r_c ≡ 0` in every modulus. With the span below the moduli product, `e ≢ 0` modulo some `p = 2^b − 1`, where the miss
///   is exactly `Pr[r_c = 0] = 1/p < 2^-(b-1)`. Bits: `min(b) − 1`.
/// * **Linear evaluation** (the output's multilinear extension at a uniform `ρ ∈ F_p^ν`): one wrong element at index `x` moves
///   the evaluation by `e·eq(ρ, x) = e·Π_i (ρ_i or 1 − ρ_i)`, zero iff one of `ν` independent uniform factors is zero:
///   `1 − (1 − 1/p)^ν ≤ ν/p`. Bits: `min(b) − 1 − ⌈log2 ν⌉`.
/// * Retired tags, programs outside the direct-check set, a span not below the moduli product and a zero-bit bound: `None`.
///
/// `ν = 0` is not a probabilistic check at all (see [`NativeExactReasonV2::ScalarIdentity`]); this returns `None` for it too.
pub fn sparse_single_error_bound_v2(p: &CompactRelationProgramV1, rel: &PlanRelationV1) -> Option<NativeSparseErrorBoundV2> {
    if !detection_program_v1(p) || p.moduli.is_empty() || p.repetitions == 0 || !span_below_moduli_v2(p, rel) {
        return None;
    }
    let b0 = p.moduli.iter().copied().min()?.saturating_sub(1);
    let per = match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => b0,
        CompactRelationCompilerV1::LinearEvaluationV1 if p.point_variables == 0 => return None,
        CompactRelationCompilerV1::LinearEvaluationV1 => b0.saturating_sub(ceil_log2(p.point_variables as u128)),
        CompactRelationCompilerV1::SumCheckHadamardV1 | CompactRelationCompilerV1::SumCheckReduceSumV1 => return None,
    };
    (per > 0).then(|| NativeSparseErrorBoundV2 {
        per_repetition_miss_bits: per,
        instance_miss_bits: per.saturating_mul(p.repetitions as u32),
    })
}

/// **One relation's disposition.** Never fails: a relation the compiler refuses is listed as refused (the whole class is not
/// thrown away to hide which relation it was), except an exact-arithmetic relation whose direct check has no sparse bound, which
/// keeps its own exact checker.
pub fn native_relation_check_v2(
    relation: u32,
    prim: &Prim,
    rel: &PlanRelationV1,
    params: &CompactCompileParamsV1,
) -> NativeRelationCheckV2 {
    use CheckerIdV1 as C;
    use ConstraintFamilyV1 as F;
    let arithmetic_opt_in =
        rel.family == F::ExactArithmetic && rel.checker == C::ExactRecompute && matches!(prim, Prim::Add | Prim::Sub);
    match compile_relation_v1(relation, prim, rel, params) {
        Ok(CompactCoverageV1::ExactRetained { .. }) => NativeRelationCheckV2::Exact {
            reason: match (arithmetic_opt_in, params.compact_exact_arithmetic) {
                (true, false) => NativeExactReasonV2::NotOptedIn,
                (true, true) => NativeExactReasonV2::DirectCheckNotCheaper,
                (false, _) => NativeExactReasonV2::FamilyExact,
            },
        },
        Ok(CompactCoverageV1::Compact(p)) => {
            if p.compiler == CompactRelationCompilerV1::LinearEvaluationV1 && p.point_variables == 0 {
                return NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::ScalarIdentity };
            }
            match sparse_single_error_bound_v2(&p, rel) {
                Some(sparse) => {
                    NativeRelationCheckV2::Direct { compiler: p.compiler, moduli: p.moduli, repetitions: p.repetitions, sparse }
                }
                None => NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::SparseErrorUnbounded },
            }
        }
        // The exact-arithmetic family's own checker is exact recomputation: an unexpressible span falls back to it.
        Err(CompactRefusalV1::SpanBeyondModuli { .. }) if arithmetic_opt_in => {
            NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::SparseErrorUnbounded }
        }
        Err(e) => NativeRelationCheckV2::Refused { why: e.to_string() },
    }
}

/// One relation's row of the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRelationInventoryV2 {
    pub relation: u32,
    pub block: u8,
    pub node: u16,
    pub prim_tag: u8,
    pub family: ConstraintFamilyV1,
    pub checker: CheckerIdV1,
    /// The court the plan files a fault of this relation in (also a direct check's: the localizer then files the exact court).
    pub court: CourtIdV1,
    pub check: NativeRelationCheckV2,
    pub route: Option<NativeRefutationRouteV2>,
}

/// One stage of the class (a pipeline / composite stage; `0` for a program or memory class).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStageInventoryV2 {
    pub stage: u16,
    /// Steps: a memory stage re-runs every relation below up to `max_steps` times, each step with the same disposition.
    pub maximum_executions: u32,
    pub plan_root: Digest,
    pub relations: Vec<NativeRelationInventoryV2>,
    /// The plan's state boundaries (zero initialization, continuity by wiring): exact, on the history route.
    pub state_boundaries: u32,
    /// The authenticated plan's worst single court filing `(bytes, work)` — every relation's court of this stage fits it.
    pub court_bound: (u64, u64),
}

/// One exact class boundary (program re-execution, pipeline/composite edges, memory continuity, snapshot retrieval).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBoundaryInventoryV2 {
    pub boundary: NativeExactBoundaryV2,
    pub route: Option<NativeRefutationRouteV2>,
    /// Its exact re-execution work (derived from the authenticated rows, not measured) — the same figure
    /// [`super::native_v2::NativeCompactCoverageV2::exact_verifier_work_upper`] sums.
    pub exact_work: u128,
}

/// A route's bounded adjudication cost over one class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeRouteBoundV2 {
    /// Relations (per execution) and boundaries on the route.
    pub subjects: u64,
    /// The worst single court filing on the route, bytes (derived).
    pub worst_court_bytes: u64,
    /// The worst single court's work on the route (derived; a boundary's whole exact re-execution where that is its court).
    pub worst_court_work: u128,
    /// Measured verifier time of one adjudication, µs — UNKNOWN (`None`) until measured.
    pub measured_us: Option<u64>,
}

/// What keeps a class's inventory from being closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeInventoryGapV2 {
    Refused {
        stage: u16,
        relation: u32,
        why: String,
    },
    /// A route subject with no finite derived court price (a saturated or absent bound).
    UnboundedRoute {
        route: NativeRefutationRouteV2,
        stage: Option<u16>,
    },
}

/// **The per-class inventory**: every stage, every relation, every exact boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeClassInventoryV2 {
    pub base_class: Digest,
    pub target: PaidClaimTargetV2,
    pub params: CompactCompileParamsV1,
    pub stages: Vec<NativeStageInventoryV2>,
    pub boundaries: Vec<NativeBoundaryInventoryV2>,
}

fn unbounded_price(bytes: u64, work: u128) -> bool {
    bytes == u64::MAX || work >= u64::MAX as u128 || (bytes == 0 && work == 0)
}

impl NativeClassInventoryV2 {
    /// Every relation with its stage.
    pub fn relations(&self) -> impl Iterator<Item = (u16, &NativeRelationInventoryV2)> {
        self.stages.iter().flat_map(|s| s.relations.iter().map(move |r| (s.stage, r)))
    }

    /// Every route this class has, with its derived bound and its measured (UNKNOWN) constant.
    pub fn route_bounds(&self) -> BTreeMap<NativeRefutationRouteV2, NativeRouteBoundV2> {
        let mut out: BTreeMap<NativeRefutationRouteV2, NativeRouteBoundV2> = BTreeMap::new();
        let mut add = |route: NativeRefutationRouteV2, subjects: u64, bytes: u64, work: u128| {
            let b = out.entry(route).or_insert(NativeRouteBoundV2 { measured_us: route.measured_us(), ..Default::default() });
            b.subjects = b.subjects.saturating_add(subjects);
            b.worst_court_bytes = b.worst_court_bytes.max(bytes);
            b.worst_court_work = b.worst_court_work.max(work);
        };
        for s in &self.stages {
            let (bytes, work) = s.court_bound;
            for r in &s.relations {
                if let Some(route) = r.route {
                    add(route, 1, bytes, work as u128);
                }
            }
            if s.state_boundaries > 0 {
                add(NativeRefutationRouteV2::History, s.state_boundaries as u64, bytes, work as u128);
            }
        }
        for b in &self.boundaries {
            if let Some(route) = b.route {
                add(route, 1, 0, b.exact_work);
            }
        }
        out
    }

    /// The gaps: refused relations and route subjects without a finite derived court price. A measured constant that is still
    /// UNKNOWN is not a gap here (it is reported by [`Self::route_bounds`]); it is never filled with a default.
    pub fn gaps(&self) -> Vec<NativeInventoryGapV2> {
        let mut gaps = Vec::new();
        for s in &self.stages {
            for r in &s.relations {
                if let NativeRelationCheckV2::Refused { why } = &r.check {
                    gaps.push(NativeInventoryGapV2::Refused { stage: s.stage, relation: r.relation, why: why.clone() });
                }
            }
            let routed =
                s.relations.iter().filter_map(|r| r.route).chain((s.state_boundaries > 0).then_some(NativeRefutationRouteV2::History));
            let mut seen = BTreeSet::new();
            for route in routed {
                if seen.insert(route) && unbounded_price(s.court_bound.0, s.court_bound.1 as u128) {
                    gaps.push(NativeInventoryGapV2::UnboundedRoute { route, stage: Some(s.stage) });
                }
            }
        }
        for b in &self.boundaries {
            if let Some(route) = b.route
                && (b.exact_work == 0 || b.exact_work == u128::MAX)
            {
                gaps.push(NativeInventoryGapV2::UnboundedRoute { route, stage: None });
            }
        }
        gaps
    }
}

fn stage_inventory_v2(
    stage: u16,
    maximum_executions: u32,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    params: &CompactCompileParamsV1,
) -> Result<NativeStageInventoryV2, String> {
    let mut relations = Vec::with_capacity(plan.relations.len());
    for (i, rel) in plan.relations.iter().enumerate() {
        let relation = u32::try_from(i).map_err(|_| "too many relations".to_string())?;
        let node = program.blocks.get(rel.block as usize).and_then(|b| b.nodes.get(rel.node as usize));
        let (check, route) = match node {
            None => (NativeRelationCheckV2::Refused { why: "the relation names no program node".into() }, None),
            Some(n) => (native_relation_check_v2(relation, &n.prim, rel, params), native_relation_route_v2(&n.prim)),
        };
        relations.push(NativeRelationInventoryV2 {
            relation,
            block: rel.block,
            node: rel.node,
            prim_tag: rel.prim_tag,
            family: rel.family,
            checker: rel.checker,
            court: rel.court,
            check,
            route,
        });
    }
    Ok(NativeStageInventoryV2 {
        stage,
        maximum_executions,
        plan_root: plan.root(),
        relations,
        state_boundaries: u32::try_from(plan.boundaries.len()).map_err(|_| "too many state boundaries".to_string())?,
        court_bound: (plan.budgets.worst_court_bytes, plan.budgets.worst_court_work),
    })
}

/// **Build a registered native class's inventory** from the authenticated ledger rows. The class walk is
/// [`super::native_v2::compile_native_class_v2`]'s, boundary for boundary (same order, same exact work), so the two read APIs
/// agree on what the class is; unlike that compiler, a relation it cannot cover is listed, not hidden behind the class's refusal.
pub fn native_class_inventory_v2(
    ledger: &KernelLedgerV1,
    class: Digest,
    params: CompactCompileParamsV1,
) -> Result<NativeClassInventoryV2, String> {
    use NativeRefutationRouteV2 as R;
    params.validate().map_err(|e| e.to_string())?;
    let mut inv = NativeClassInventoryV2 {
        base_class: class,
        target: PaidClaimTargetV2::Program,
        params,
        stages: Vec::new(),
        boundaries: Vec::new(),
    };
    let reexec = |program: &TirProgramV1, plan: &VerificationPlanV1| crate::plan::reexecution_bounds_v1(program, plan).0;
    if let Some(rule) = ledger.classes.get(&class) {
        inv.target = if crate::descriptor::is_segmented_v1(&rule.descriptor) {
            PaidClaimTargetV2::Segmented
        } else {
            PaidClaimTargetV2::Program
        };
        inv.stages.push(stage_inventory_v2(0, 1, &rule.program, &rule.plan, &params)?);
        inv.boundaries.push(NativeBoundaryInventoryV2 {
            boundary: NativeExactBoundaryV2::Program { plan: rule.plan.root() },
            route: None,
            exact_work: reexec(&rule.program, &rule.plan),
        });
    } else if let Some(rule) = ledger.pipeline_classes.get(&class) {
        inv.target = PaidClaimTargetV2::Pipeline;
        if rule.pipeline.stages.len() != rule.plan.stages.len() {
            return Err("native pipeline stage/plan cardinality mismatch".into());
        }
        let mut work = 0u128;
        for (s, stage) in rule.pipeline.stages.iter().enumerate() {
            let program = rule.programs.get(stage.program as usize).ok_or("native pipeline program missing")?;
            let view = crate::pipeline::stage_view_v1(program);
            inv.stages.push(stage_inventory_v2(s as u16, 1, &view.view, &rule.plan.stages[s], &params)?);
            work = work.checked_add(reexec(&view.view, &rule.plan.stages[s])).ok_or("native exact verifier union overflow")?;
        }
        inv.boundaries.push(NativeBoundaryInventoryV2 {
            boundary: NativeExactBoundaryV2::Pipeline {
                pipeline_root: crate::pipeline::pipeline_root_v1(&rule.pipeline, &rule.programs),
                plan_root: rule.plan.root(),
            },
            route: None,
            exact_work: work,
        });
    } else if let Some(rule) = ledger.typed.classes.get(&class) {
        inv.target = PaidClaimTargetV2::TypedSpec;
        match &rule.kind {
            SpecClassKindV1::Memory { rule: memory, root, .. } => {
                inv.stages.push(stage_inventory_v2(0, root.max_steps, &memory.program, &memory.plan, &params)?);
                inv.boundaries.push(NativeBoundaryInventoryV2 {
                    boundary: NativeExactBoundaryV2::Memory { spec_root: object_id(b"MISAKA/PALW/COMPACT-TYPED-SPEC/V2", &rule.spec) },
                    route: Some(R::History),
                    exact_work: reexec(&memory.program, &memory.plan)
                        .checked_mul(u128::from(root.max_steps))
                        .ok_or("memory exact work overflow")?,
                });
            }
            SpecClassKindV1::Retrieval { root } => inv.boundaries.push(NativeBoundaryInventoryV2 {
                boundary: NativeExactBoundaryV2::Retrieval {
                    stage: 0,
                    snapshot: root.snapshot.root(),
                    rule: object_id(b"MISAKA/PALW/COMPACT-RETRIEVAL-RULE/V2", root),
                },
                route: Some(R::Gather),
                exact_work: crate::spec::retrieval_reexecution_work_v1(root),
            }),
            SpecClassKindV1::Composite { root, components } => {
                if root.stages.len() != components.len() {
                    return Err("native composite component cardinality mismatch".into());
                }
                let composite = inv.boundaries.len();
                inv.boundaries.push(NativeBoundaryInventoryV2 {
                    boundary: NativeExactBoundaryV2::Composite {
                        spec_root: object_id(b"MISAKA/PALW/COMPACT-TYPED-SPEC/V2", &rule.spec),
                    },
                    route: None,
                    exact_work: 0,
                });
                for (s, component) in components.iter().enumerate() {
                    match component {
                        ComponentV1::Model(model) => {
                            inv.stages.push(stage_inventory_v2(s as u16, 1, &model.program, &model.plan, &params)?);
                            let w = reexec(&model.program, &model.plan);
                            let b = &mut inv.boundaries[composite].exact_work;
                            *b = b.checked_add(w).ok_or("native exact verifier union overflow")?;
                        }
                        ComponentV1::Retrieval(root) => inv.boundaries.push(NativeBoundaryInventoryV2 {
                            boundary: NativeExactBoundaryV2::Retrieval {
                                stage: s as u16,
                                snapshot: root.snapshot.root(),
                                rule: object_id(b"MISAKA/PALW/COMPACT-RETRIEVAL-RULE/V2", root),
                            },
                            route: Some(R::Gather),
                            exact_work: crate::spec::retrieval_reexecution_work_v1(root),
                        }),
                    }
                }
            }
        }
    } else {
        return Err("the inventory names no native registered class".into());
    }
    Ok(inv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::MatMulDimsV1;
    use misaka_palw_tir::DType;

    fn params(on: bool) -> CompactCompileParamsV1 {
        CompactCompileParamsV1 { compact_exact_arithmetic: on, arithmetic_repetitions: 2, fs_query_bits: 64 }
    }

    fn rel(prim: &Prim, family: ConstraintFamilyV1, checker: CheckerIdV1, ins: &[(DType, u64)], out: (DType, u64)) -> PlanRelationV1 {
        PlanRelationV1 {
            block: 0,
            node: 3,
            prim_tag: prim.tag(),
            family,
            checker,
            court: CourtIdV1::InstanceRecompute,
            repetitions: if checker.is_probabilistic() { 2 } else { 0 },
            occurrences: 1,
            out_dtype: out.0.tag(),
            out_elements: out.1,
            in_elements: ins.iter().map(|(_, e)| *e).collect(),
            in_dtypes: ins.iter().map(|(d, _)| d.tag()).collect(),
            matmul: None,
        }
    }

    fn matmul(checker: CheckerIdV1, xd: DType, wd: DType, yd: DType) -> PlanRelationV1 {
        let mut r = rel(&Prim::MatMul, ConstraintFamilyV1::DenseMatrix, checker, &[(xd, 2 * 3 * 4), (wd, 4 * 5)], (yd, 2 * 3 * 5));
        r.court = CourtIdV1::MatMulScalar;
        r.matmul = Some(MatMulDimsV1 { batch: 2, m: 3, k: 4, n: 5 });
        r
    }

    /// **Freivalds' sparse bound is the smallest modulus's** (`1/p` per repetition for one wrong scalar), the CRT set's included;
    /// it agrees with the compiler's own per-repetition figure.
    #[test]
    fn freivalds_states_its_single_element_miss_bound() {
        for (checker, xd, wd, yd) in [
            (CheckerIdV1::FreivaldsM127, DType::I8, DType::I8, DType::I32),
            (CheckerIdV1::FreivaldsCrtV2, DType::I64, DType::I64, DType::I128),
        ] {
            let r = matmul(checker, xd, wd, yd);
            let NativeRelationCheckV2::Direct { compiler, moduli, repetitions, sparse } =
                native_relation_check_v2(0, &Prim::MatMul, &r, &params(false))
            else {
                panic!("{checker:?}: a bounded direct check");
            };
            assert_eq!(compiler, CompactRelationCompilerV1::FreivaldsSeededV1);
            let b0 = moduli.iter().copied().min().unwrap() - 1;
            assert_eq!(sparse.per_repetition_miss_bits, b0);
            assert_eq!(sparse.instance_miss_bits, b0 * repetitions as u32);
            let CompactCoverageV1::Compact(p) = compile_relation_v1(0, &Prim::MatMul, &r, &params(false)).unwrap() else { panic!() };
            assert_eq!(relation_bound_v1(&p).per_repetition_bits, Some(sparse.per_repetition_miss_bits));
        }
    }

    /// **The sparse-error fallback**: an M127 product whose integer error span is not below `2^127 − 1` (only an unchecked plan
    /// can carry one) has no single-element bound — one wrong element can be a multiple of the modulus — so it is covered by
    /// exact recomputation, never by the direct check.
    #[test]
    fn an_m127_span_past_the_modulus_falls_back_to_exact() {
        let r = matmul(CheckerIdV1::FreivaldsM127, DType::I64, DType::I64, DType::I128);
        assert!(matches!(compile_relation_v1(0, &Prim::MatMul, &r, &params(false)), Ok(CompactCoverageV1::Compact(_))));
        assert_eq!(
            native_relation_check_v2(0, &Prim::MatMul, &r, &params(false)),
            NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::SparseErrorUnbounded }
        );
    }

    /// Linear evaluation's bound pays `⌈log2 ν⌉` for the `ν` factors of `eq`; opted out it is exact; other families are exact.
    #[test]
    fn linear_evaluation_pays_for_its_point_variables_and_exact_families_stay_exact() {
        let add = rel(
            &Prim::Add,
            ConstraintFamilyV1::ExactArithmetic,
            CheckerIdV1::ExactRecompute,
            &[(DType::I32, 4096), (DType::I32, 4096)],
            (DType::I64, 4096),
        );
        match native_relation_check_v2(0, &Prim::Add, &add, &params(true)) {
            NativeRelationCheckV2::Direct { compiler, moduli, sparse, .. } => {
                assert_eq!(compiler, CompactRelationCompilerV1::LinearEvaluationV1);
                // 4,096 elements are ν = 12 point variables; `eq` has degree ν, so the bound pays ⌈log2 12⌉ = 4 bits (not 12).
                assert_eq!(sparse.per_repetition_miss_bits, moduli.iter().copied().min().unwrap() - 1 - 4);
            }
            NativeRelationCheckV2::Exact { reason } => assert_eq!(reason, NativeExactReasonV2::DirectCheckNotCheaper),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            native_relation_check_v2(0, &Prim::Add, &add, &params(false)),
            NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::NotOptedIn }
        );
        let mul = rel(
            &Prim::Mul,
            ConstraintFamilyV1::ExactArithmetic,
            CheckerIdV1::ExactRecompute,
            &[(DType::I32, 64), (DType::I32, 64)],
            (DType::I64, 64),
        );
        assert_eq!(
            native_relation_check_v2(0, &Prim::Mul, &mul, &params(true)),
            NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::FamilyExact }
        );
        let topk = Prim::TopK { axis: 0, k: 2 };
        let sel = rel(&topk, ConstraintFamilyV1::Selection, CheckerIdV1::ExactRecompute, &[(DType::I32, 64)], (DType::I32, 2));
        assert_eq!(
            native_relation_check_v2(0, &topk, &sel, &params(true)),
            NativeRelationCheckV2::Exact { reason: NativeExactReasonV2::FamilyExact }
        );
        assert_eq!(native_relation_route_v2(&topk), Some(NativeRefutationRouteV2::Routing));
    }

    /// A pair outside the inventory is listed as refused — with its relation — instead of failing the whole inventory.
    #[test]
    fn an_unknown_pair_is_a_listed_refusal() {
        let bad = rel(
            &Prim::MatMul,
            ConstraintFamilyV1::Selection,
            CheckerIdV1::FreivaldsM127,
            &[(DType::I8, 4), (DType::I8, 4)],
            (DType::I32, 4),
        );
        assert!(matches!(native_relation_check_v2(7, &Prim::MatMul, &bad, &params(false)), NativeRelationCheckV2::Refused { .. }));
    }

    /// Routes: routing / gather / history by primitive; every route has a named measured constant, UNKNOWN until measured.
    #[test]
    fn every_route_has_a_named_unknown_measured_constant() {
        assert_eq!(native_relation_route_v2(&Prim::Gather { axis: 0, batch_dims: 0 }), Some(NativeRefutationRouteV2::Gather));
        assert_eq!(native_relation_route_v2(&Prim::HistAppend { state: 0 }), Some(NativeRefutationRouteV2::History));
        assert_eq!(native_relation_route_v2(&Prim::StateWrite { state: 0 }), Some(NativeRefutationRouteV2::History));
        assert_eq!(native_relation_route_v2(&Prim::MatMul), None);
        for route in NativeRefutationRouteV2::ALL {
            assert_eq!(route.measured_us(), None, "{}: UNKNOWN until measured", route.name());
        }
    }
}
