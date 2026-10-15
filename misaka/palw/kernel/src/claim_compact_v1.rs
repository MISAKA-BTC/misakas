//! **R7-RELATION compact relation proofs** (RFC-0007 2026-10-15 round 3; dormant, ClaimV2 namespace, NOT reviewed).
//!
//! The full-material-opening compiler ([`crate::claim_interactive_v3::full_material_compiler_v3`]) re-binds the complete public
//! material in every round message. The compilers here replace that per-round opening, relation by relation, with a **compact
//! proof** whose bytes are bounded by the relation's dimensions' logarithm (or a constant header), checked against operands the
//! verifier reads through the claim's own authenticated commitments (the claim's DA obligation, not the round message's).
//!
//! | compiler | relations (plan family / checker) | proof per (repetition, modulus) | one repetition's error |
//! |---|---|---|---|
//! | [`CompactRelationCompilerV1::FreivaldsSeededV1`] | `MatMul` (DenseMatrix / `FreivaldsM127`, `FreivaldsCrtV2`) | none (vectors from the paid seed stream) | `1/p` per batch slice |
//! | [`CompactRelationCompilerV1::LinearEvaluationV1`] | `Add`, `Sub` (ExactArithmetic / `ExactRecompute`, broadcast) | none (one random point) | `ν/p` |
//! | [`CompactRelationCompilerV1::SumCheckHadamardV1`] | `Mul` (ExactArithmetic, broadcast) | `ν` rounds × 4 field elements | `4ν/p` (+ transcript grinding) |
//! | [`CompactRelationCompilerV1::SumCheckReduceSumV1`] | `ReduceSum{axis}` (ExactArithmetic) | `ν_j` rounds × 2 field elements | `(ν_o+ν_j)/p` (+ grinding) |
//!
//! `p` is the smallest Mersenne modulus the relation uses (`2^127−1`, or the CRT set of [`crate::field::MODULI_V2`] whose product
//! exceeds the relation's integer error span, exactly as [`crate::plan::dense_moduli_v2`] derives it); `ν` the Boolean variables of
//! the zero-padded output. Every other family/checker pair keeps its descriptor's exact checker ([`CompactCoverageV1::ExactRetained`],
//! `ε = 0`) or is refused ([`COMPACT_REFUSED_FAMILIES_V1`]); an unknown pair refuses the whole plan (fail closed).
//!
//! **What this is not.** No polynomial commitment exists in this tree: every evaluation claim a sum-check reduces to is discharged by
//! reading the operand from authenticated material. V1 therefore does NOT reduce the verifier's reads below the operands' size; it
//! reduces the round message to a compact proof and gives each relation an explicit error term. A GKR-style chain (intermediate
//! tensors never read) and a PCS-backed evaluation are refused families. The sum-check challenges are a transcript-bound
//! Fiat–Shamir keyed by the paid ClaimV2 round seed; their grinding term is charged with the contract's declared offline query
//! budget and repetitions are NOT credited against it. Admission additionally requires an approved tuple in the review registry,
//! which ships empty: nothing here is approved, activated or claimed as reviewed.
//!
//! Precondition: the plan was accepted by [`crate::check::check_plan_v1`], whose interval admission proves no exact primitive can
//! overflow (so an integer identity of the operands is the TIR relation, including `MatMul`/`ReduceSum` partial sums).

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::composition::{
    AdversaryBudgetV1, CheckFamilyV1, ComposedBoundV1, CompositionRefusalV1, CoverageV1, InstanceCompositionV1,
    composed_false_accept_bits_v1,
};
use misaka_palw_challenge::seed::{ChallengeStreamV1, SampleRefusalV1, StreamKindV1, StreamLabelV1};
use misaka_palw_challenge::{CumulativeFailureTermV1, EffectiveBitsV1, PostCommitChallengePolicyV1, ReviewedTupleV2};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, Prim, Tensor};

use crate::claim_paid_v2::PaidClaimTargetV2;
use crate::family::{CheckerIdV1, ConstraintFamilyV1};
use crate::field::{F89, F107, FieldElemV1, Fp, MODULI_V2, mersenne};
use crate::hash::{Digest, object_id};
use crate::plan::{PlanRelationV1, VerificationPlanV1, dtype_of_tag, relation_moduli};

pub const COMPACT_RELATION_COMPILER_DOMAIN_V1: &[u8] = b"MISAKA/PALW/CLAIM-PROOF-COMPILER/COMPACT-RELATION/V1";
pub const COMPACT_RELATION_SCOPE_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/SCOPE/V1";
pub const COMPACT_RELATION_FS_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/FS/V1";
pub const COMPACT_RELATION_COVERAGE_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/COVERAGE/V1";
pub const COMPACT_RELATION_CONTRACT_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/CONTRACT/V1";
pub const COMPACT_RELATION_SUITE_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/SUITE/V1";
pub const COMPACT_RELATION_PROOF_DOMAIN_V1: &[u8] = b"MISAKA/PALW/COMPACT-RELATION/PROOF/V1";

/// The smallest declared offline transcript-query budget (`log2 Q`) a contract may state: a lower declaration is implausible.
pub const COMPACT_FS_MIN_QUERY_BITS_V1: u16 = 64;
/// The largest `log2 Q` the integer bound arithmetic accepts.
pub const COMPACT_FS_MAX_QUERY_BITS_V1: u16 = 120;

/// **The implemented compact relation compilers.** Each has its own compiler identity ([`compact_relation_compiler_id_v1`]),
/// distinct from the full-material-opening compiler and from the refused family-level stubs of
/// [`crate::claim_interactive_v3::CompactProofFamilyV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CompactRelationCompilerV1 {
    FreivaldsSeededV1 = 0,
    LinearEvaluationV1 = 1,
    SumCheckHadamardV1 = 2,
    SumCheckReduceSumV1 = 3,
}

impl CompactRelationCompilerV1 {
    pub const ALL: [Self; 4] = [Self::FreivaldsSeededV1, Self::LinearEvaluationV1, Self::SumCheckHadamardV1, Self::SumCheckReduceSumV1];

    pub const fn name(self) -> &'static str {
        match self {
            Self::FreivaldsSeededV1 => "freivalds-seeded/v1",
            Self::LinearEvaluationV1 => "linear-evaluation/v1",
            Self::SumCheckHadamardV1 => "sumcheck-hadamard/v1",
            Self::SumCheckReduceSumV1 => "sumcheck-reduce-sum/v1",
        }
    }

    /// Whether the compiler's challenges after the first are transcript-bound Fiat–Shamir (grinding charged, repetitions uncredited).
    pub const fn transcript_bound(self) -> bool {
        matches!(self, Self::SumCheckHadamardV1 | Self::SumCheckReduceSumV1)
    }
}

pub fn compact_relation_compiler_id_v1(compiler: CompactRelationCompilerV1) -> Digest {
    object_id(COMPACT_RELATION_COMPILER_DOMAIN_V1, &compiler)
}

pub fn is_compact_relation_compiler_v1(id: &Digest) -> bool {
    CompactRelationCompilerV1::ALL.iter().any(|c| compact_relation_compiler_id_v1(*c) == *id)
}

/// Why the V3 full-material round plan refuses a compact relation compiler id (no silent reinterpretation, no fallback).
pub const COMPACT_RELATION_NOT_A_V3_PLAN_V1: &str = "a compact relation compiler (R7-RELATION round 3) is registered only through \
     CompactRelationContractV1 under the dormant ClaimV2 fence and an approved review tuple; the V3 full-material round plan cannot \
     carry it";

/// One row of the family inventory: what the compact compilers do with a (family, checker) pair the shipped descriptors use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactInventoryStatusV1 {
    /// Compiled to a compact relation proof.
    Implemented(&'static [CompactRelationCompilerV1]),
    /// Kept on the descriptor's exact checker (`ε = 0`); its operands are read from the claim's authenticated material.
    ExactRetained,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactInventoryRowV1 {
    pub family: ConstraintFamilyV1,
    pub checker: CheckerIdV1,
    pub status: CompactInventoryStatusV1,
    pub note: &'static str,
}

/// **Every (family, checker) pair a shipped descriptor (K2-TIR-v1…v5) lists**, and its compact status. A pair outside this table is
/// refused by [`compile_relation_v1`] — fail closed.
pub const COMPACT_RELATION_INVENTORY_V1: [CompactInventoryRowV1; 9] = {
    use CheckerIdV1 as C;
    use CompactInventoryStatusV1 as S;
    use CompactRelationCompilerV1 as K;
    use ConstraintFamilyV1 as F;
    const FREIVALDS: &[CompactRelationCompilerV1] = &[K::FreivaldsSeededV1];
    const ARITH: &[CompactRelationCompilerV1] = &[K::LinearEvaluationV1, K::SumCheckHadamardV1, K::SumCheckReduceSumV1];
    [
        CompactInventoryRowV1 { family: F::DenseMatrix, checker: C::FreivaldsM127, status: S::Implemented(FREIVALDS), note: "MatMul, modulus 2^127-1" },
        CompactInventoryRowV1 { family: F::DenseMatrix, checker: C::FreivaldsCrtV2, status: S::Implemented(FREIVALDS), note: "MatMul, CRT moduli" },
        CompactInventoryRowV1 {
            family: F::ExactArithmetic,
            checker: C::ExactRecompute,
            status: S::Implemented(ARITH),
            note: "Add/Sub/Mul/ReduceSum when the contract opts in; Cast stays exact",
        },
        CompactInventoryRowV1 { family: F::Structure, checker: C::ExactRecompute, status: S::ExactRetained, note: "index maps: exact copy, no error term" },
        CompactInventoryRowV1 { family: F::QuantRange, checker: C::ExactRecompute, status: S::ExactRetained, note: "rounding/saturation needs a lookup argument" },
        CompactInventoryRowV1 { family: F::Nonlinear, checker: C::ExactRecompute, status: S::ExactRetained, note: "fixed-iteration transcendentals need a lookup argument" },
        CompactInventoryRowV1 { family: F::Selection, checker: C::ExactRecompute, status: S::ExactRetained, note: "TopK/compare/gather routing needs a permutation/lookup argument" },
        CompactInventoryRowV1 { family: F::RecurrentState, checker: C::StateContinuity, status: S::ExactRetained, note: "state continuity by wiring" },
        CompactInventoryRowV1 { family: F::MediaPipeline, checker: C::EdgeRecompute, status: S::ExactRetained, note: "pipeline edges recomputed exactly" },
    ]
};

/// **Compact families refused with their reasons** (listed, never silently accepted).
pub const COMPACT_REFUSED_FAMILIES_V1: [(&str, &str); 7] = [
    ("gkr-layered-chain", "needs intermediate tensors never read: no polynomial commitment suite exists for the chain's endpoints and the stub contract stays refused"),
    ("pcs-evaluation", "a hash-Merkle tensor commitment cannot open a multilinear evaluation; a code-based PCS (Ligero/Brakedown) is not implemented or reviewed"),
    ("lookup-quant-nonlinear", "QuantRange/Nonlinear relations need a lookup argument; they stay on exact recompute"),
    ("permutation-selection-routing", "TopK/Gather/Compare routing needs a permutation/lookup argument; it stays on exact recompute"),
    ("state-continuity-memory", "Hist/Fixed continuity stays on wiring + exact recompute; no compact memory argument"),
    ("staged-beacon-sumcheck", "one fresh beacon per sum-check round is not compiled; V1 uses the transcript-bound in-round challenge with its grinding charged"),
    ("typed-spec-target", "typed memory/retrieval/composite steps have no VerificationPlanV1 relation map"),
];

/// Why a relation or a plan cannot be compiled compactly.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompactRefusalV1 {
    #[error("relation {relation}: family {family:?} with checker {checker:?} is not in the compact inventory")]
    UnknownFamilyChecker { relation: u32, family: ConstraintFamilyV1, checker: CheckerIdV1 },
    #[error("relation {relation}: {why}")]
    Malformed { relation: u32, why: String },
    #[error("relation {relation}: the integer error span exceeds every supported modulus set")]
    SpanBeyondModuli { relation: u32 },
    #[error("target {0:?} has no plan relation map for compact compilation")]
    Target(PaidClaimTargetV2),
    #[error("compile parameters: {0}")]
    Params(String),
}

/// What the registrant chooses (bound into the coverage root).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactCompileParamsV1 {
    /// Compile `Add`/`Sub`/`Mul`/`ReduceSum` compactly (otherwise they keep exact recompute, `ε = 0`).
    pub compact_exact_arithmetic: bool,
    /// Repetitions of the compact arithmetic families (each its own random point); the dense family keeps the plan's.
    pub arithmetic_repetitions: u8,
    /// `log2` of the declared offline transcript-query budget the Fiat–Shamir grinding term charges.
    pub fs_query_bits: u16,
}

impl CompactCompileParamsV1 {
    pub fn validate(&self) -> Result<(), CompactRefusalV1> {
        if self.compact_exact_arithmetic && self.arithmetic_repetitions == 0 {
            return Err(CompactRefusalV1::Params("compact arithmetic needs at least one repetition".into()));
        }
        if !(COMPACT_FS_MIN_QUERY_BITS_V1..=COMPACT_FS_MAX_QUERY_BITS_V1).contains(&self.fs_query_bits) {
            return Err(CompactRefusalV1::Params(format!(
                "the transcript-query budget must be declared in [2^{COMPACT_FS_MIN_QUERY_BITS_V1}, 2^{COMPACT_FS_MAX_QUERY_BITS_V1}]"
            )));
        }
        Ok(())
    }
}

/// One compiled relation: everything its prover and verifier need besides the operands and the seed.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactRelationProgramV1 {
    pub compiler: CompactRelationCompilerV1,
    /// The relation's index in the plan (its stream label).
    pub relation: u32,
    pub block: u8,
    pub node: u16,
    pub prim_tag: u8,
    /// `ReduceSum`'s axis (0 otherwise).
    pub axis: u32,
    pub occurrences: u32,
    /// Mersenne exponents used, largest first (a prefix of [`MODULI_V2`]).
    pub moduli: Vec<u32>,
    pub repetitions: u8,
    /// `ν` (`ν_o` for `ReduceSum`) at the plan's worst history length.
    pub point_variables: u32,
    /// Sum-check rounds at the worst history length (`ν` for Hadamard, `ν_j` for `ReduceSum`, 0 otherwise).
    pub sumcheck_rounds: u32,
    /// Sum-check round polynomial degree (3 Hadamard, 1 `ReduceSum`, 0 none).
    pub degree: u32,
    /// Checked instances per position (`occurrences × batch` for a product, `occurrences` otherwise).
    pub instances_per_position: u64,
    pub out_dtype: u8,
    pub in_dtypes: Vec<u8>,
    pub out_elements: u64,
    pub in_elements: Vec<u64>,
    pub fs_query_bits: u16,
}

/// One relation's place in a compact coverage.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CompactCoverageV1 {
    Compact(CompactRelationProgramV1),
    /// The descriptor's exact checker stays (`ε = 0`); the verifier reads the relation's operands from authenticated material.
    ExactRetained { relation: u32, family: ConstraintFamilyV1, checker: CheckerIdV1 },
}

/// **A plan's compact coverage**: every relation of the plan, in plan order, compiled or exactly retained. Its root is what a
/// [`CompactRelationContractV1`] binds.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactPlanCoverageV1 {
    pub version: u16,
    pub plan_root: Digest,
    pub max_positions: u32,
    pub params: CompactCompileParamsV1,
    pub entries: Vec<CompactCoverageV1>,
}

impl CompactPlanCoverageV1 {
    pub fn root(&self) -> Digest {
        object_id(COMPACT_RELATION_COVERAGE_DOMAIN_V1, self)
    }

    pub fn compact(&self) -> impl Iterator<Item = &CompactRelationProgramV1> {
        self.entries.iter().filter_map(|e| match e {
            CompactCoverageV1::Compact(p) => Some(p),
            CompactCoverageV1::ExactRetained { .. } => None,
        })
    }

    /// Compact proof bytes for one position (every compact relation, every occurrence, worst history).
    pub fn proof_bytes_per_position(&self) -> u128 {
        self.compact().map(|p| proof_bytes_bound_v1(p).saturating_mul(p.occurrences as u128)).fold(0u128, u128::saturating_add)
    }

    /// The full material the compact relations would otherwise open per position (their operands and outputs).
    pub fn material_bytes_per_position(&self) -> u128 {
        self.compact().map(|p| material_bytes_v1(p).saturating_mul(p.occurrences as u128)).fold(0u128, u128::saturating_add)
    }

    pub fn verifier_work_per_position(&self) -> u128 {
        self.compact().map(|p| verifier_work_bound_v1(p).saturating_mul(p.occurrences as u128)).fold(0u128, u128::saturating_add)
    }
}

fn ceil_log2(x: u128) -> u32 {
    if x <= 1 { 0 } else { 128 - (x - 1).leading_zeros() }
}

fn max_abs(d: DType) -> u128 {
    d.min_value().unsigned_abs().max(d.max_value().unsigned_abs())
}

fn width(tag: u8) -> u128 {
    dtype_of_tag(tag).map(|d| d.width() as u128).unwrap_or(16)
}

/// The fewest moduli of [`MODULI_V2`], largest first, whose product exceeds an integer error span (`exact` when it fits a `u128`,
/// else `span < 2^bits`). Same rule as [`crate::plan::dense_moduli_v2`].
fn moduli_for_span(exact: Option<u128>, bits: u32) -> Option<usize> {
    if exact.is_some_and(|s| s < crate::field::P) {
        return Some(1);
    }
    let mut sum = 0u32;
    for (j, e) in MODULI_V2.iter().enumerate() {
        sum += e;
        if j >= 1 && bits < sum {
            return Some(j + 1);
        }
    }
    None
}

/// **Compile one plan relation.** `Ok(ExactRetained)` for a family that keeps its exact checker; `Err` for any pair outside the
/// inventory or a relation the compiler cannot express (the whole plan is then refused).
pub fn compile_relation_v1(
    relation: u32,
    prim: &Prim,
    rel: &PlanRelationV1,
    params: &CompactCompileParamsV1,
) -> Result<CompactCoverageV1, CompactRefusalV1> {
    use CheckerIdV1 as C;
    use ConstraintFamilyV1 as F;
    let malformed = |why: &str| CompactRefusalV1::Malformed { relation, why: why.into() };
    if prim.tag() != rel.prim_tag {
        return Err(malformed("the plan's primitive tag is not the program node's"));
    }
    let retained = Ok(CompactCoverageV1::ExactRetained { relation, family: rel.family, checker: rel.checker });
    let base = |compiler, moduli: usize, repetitions: u8| CompactRelationProgramV1 {
        compiler,
        relation,
        block: rel.block,
        node: rel.node,
        prim_tag: rel.prim_tag,
        axis: 0,
        occurrences: rel.occurrences,
        moduli: MODULI_V2[..moduli].to_vec(),
        repetitions,
        point_variables: 0,
        sumcheck_rounds: 0,
        degree: 0,
        instances_per_position: rel.occurrences as u64,
        out_dtype: rel.out_dtype,
        in_dtypes: rel.in_dtypes.clone(),
        out_elements: rel.out_elements,
        in_elements: rel.in_elements.clone(),
        fs_query_bits: params.fs_query_bits,
    };
    match (rel.family, rel.checker) {
        (F::DenseMatrix, C::FreivaldsM127 | C::FreivaldsCrtV2) => {
            let Some(d) = rel.matmul else { return Err(malformed("a dense relation without its product dimensions")) };
            if !matches!(prim, Prim::MatMul) || rel.in_dtypes.len() != 2 || rel.repetitions == 0 {
                return Err(malformed("a dense relation that is not a repeated two-operand MatMul"));
            }
            let moduli = relation_moduli(rel).ok_or(CompactRefusalV1::SpanBeyondModuli { relation })?;
            let mut p = base(CompactRelationCompilerV1::FreivaldsSeededV1, moduli, rel.repetitions);
            p.degree = 1;
            p.instances_per_position = (rel.occurrences as u64).saturating_mul(d.batch.max(1));
            Ok(CompactCoverageV1::Compact(p))
        }
        (F::ExactArithmetic, C::ExactRecompute) => {
            if !params.compact_exact_arithmetic {
                return retained;
            }
            let dt = |i: usize| rel.in_dtypes.get(i).and_then(|t| dtype_of_tag(*t));
            let out = dtype_of_tag(rel.out_dtype).ok_or_else(|| malformed("an unknown output dtype"))?;
            if rel.out_elements == 0 {
                return Err(malformed("an empty output"));
            }
            let nu = ceil_log2(rel.out_elements as u128);
            match prim {
                Prim::Add | Prim::Sub | Prim::Mul => {
                    let (Some(a), Some(b)) = (dt(0), dt(1)) else { return Err(malformed("a binary relation without two typed operands")) };
                    let (exact, bits) = if matches!(prim, Prim::Mul) {
                        let la = ceil_log2(max_abs(a));
                        let lb = ceil_log2(max_abs(b));
                        let e = max_abs(a).checked_mul(max_abs(b)).and_then(|v| v.checked_add(max_abs(out)));
                        (e, (la + lb).max(ceil_log2(max_abs(out))) + 2)
                    } else {
                        let e = max_abs(a).checked_add(max_abs(b)).and_then(|v| v.checked_add(max_abs(out)));
                        (e, ceil_log2(max_abs(a)).max(ceil_log2(max_abs(b))).max(ceil_log2(max_abs(out))) + 2)
                    };
                    let moduli = moduli_for_span(exact, bits).ok_or(CompactRefusalV1::SpanBeyondModuli { relation })?;
                    let compiler = if matches!(prim, Prim::Mul) {
                        CompactRelationCompilerV1::SumCheckHadamardV1
                    } else {
                        CompactRelationCompilerV1::LinearEvaluationV1
                    };
                    let mut p = base(compiler, moduli, params.arithmetic_repetitions);
                    p.point_variables = nu;
                    if matches!(prim, Prim::Mul) {
                        p.sumcheck_rounds = nu;
                        p.degree = 3;
                    }
                    Ok(CompactCoverageV1::Compact(p))
                }
                Prim::ReduceSum { axis } => {
                    let Some(a) = dt(0) else { return Err(malformed("a reduction without its typed operand")) };
                    let ins = *rel.in_elements.first().ok_or_else(|| malformed("a reduction without its operand"))?;
                    if ins % rel.out_elements != 0 {
                        return Err(malformed("a reduction whose operand is not a multiple of its output"));
                    }
                    let j = ins / rel.out_elements;
                    let exact = (j as u128).checked_mul(max_abs(a)).and_then(|v| v.checked_add(max_abs(out)));
                    let bits = (ceil_log2(j as u128) + ceil_log2(max_abs(a))).max(ceil_log2(max_abs(out))) + 2;
                    let moduli = moduli_for_span(exact, bits).ok_or(CompactRefusalV1::SpanBeyondModuli { relation })?;
                    let mut p = base(CompactRelationCompilerV1::SumCheckReduceSumV1, moduli, params.arithmetic_repetitions);
                    p.axis = *axis as u32;
                    p.point_variables = nu;
                    p.sumcheck_rounds = ceil_log2(j as u128);
                    p.degree = 1;
                    Ok(CompactCoverageV1::Compact(p))
                }
                Prim::Cast => retained,
                _ => Err(malformed("an exact-arithmetic relation on a primitive outside the family")),
            }
        }
        (F::Structure | F::QuantRange | F::Nonlinear | F::Selection, C::ExactRecompute)
        | (F::RecurrentState, C::StateContinuity)
        | (F::MediaPipeline, C::EdgeRecompute) => retained,
        (family, checker) => Err(CompactRefusalV1::UnknownFamilyChecker { relation, family, checker }),
    }
}

/// **Compile a whole plan.** Every relation must be compiled or exactly retained; the first refusal refuses the plan. The caller
/// must have accepted `plan` against `program` with [`crate::check::check_plan_v1`] first.
pub fn compile_plan_v1(
    target: PaidClaimTargetV2,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    params: &CompactCompileParamsV1,
) -> Result<CompactPlanCoverageV1, CompactRefusalV1> {
    if target == PaidClaimTargetV2::TypedSpec {
        return Err(CompactRefusalV1::Target(target));
    }
    params.validate()?;
    let mut entries = Vec::with_capacity(plan.relations.len());
    for (i, rel) in plan.relations.iter().enumerate() {
        let relation = u32::try_from(i).map_err(|_| CompactRefusalV1::Params("too many relations".into()))?;
        let node = program
            .blocks
            .get(rel.block as usize)
            .and_then(|b| b.nodes.get(rel.node as usize))
            .ok_or_else(|| CompactRefusalV1::Malformed { relation, why: "the relation names no program node".into() })?;
        entries.push(compile_relation_v1(relation, &node.prim, rel, params)?);
    }
    Ok(CompactPlanCoverageV1 { version: 1, plan_root: plan.root(), max_positions: plan.max_positions, params: *params, entries })
}

// ------------------------------------------------------------------------------------------------------------------------------
// Bounds.

/// One relation's soundness, in bits (`−log2 ε`). `None`: the relation is checked exactly (a zero-variable identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactRelationBoundV1 {
    /// One repetition against a fixed false instance, interactive challenges (no transcript grinding).
    pub per_repetition_bits: Option<u32>,
    /// One instance's whole check: repetitions credited only where every challenge comes from the post-commit seed stream; the
    /// transcript-bound families instead charge `Q = 2^fs_query_bits` grinding queries and credit no repetition.
    pub instance_bits: Option<u32>,
}

/// **The bound arithmetic** (integer, every logarithm ceiled, every factor charged):
///
/// * Freivalds: `ε_rep ≤ 1/p` → `b₀ = ⌊log2 p⌋`; instance `t·b₀`;
/// * linear evaluation (`ν ≥ 1`): `ε_rep ≤ ν/p` (Schwartz–Zippel on the multilinear difference) → `b₀ − ⌈log2 ν⌉`; instance `t·(…)`;
/// * Hadamard sum-check: `ε_rep ≤ ν/p + 3ν/p` → `b₀ − ⌈log2 4ν⌉`; instance `b₀ − ⌈log2 (4ν + 3Q)⌉`;
/// * reduce-sum sum-check: `ε_rep ≤ ν_o/p + ν_j/p` → `b₀ − ⌈log2 (ν_o+ν_j)⌉`; instance `b₀ − ⌈log2 (ν_o + ν_j + Q)⌉`;
///
/// with `p` the SMALLEST modulus the relation uses (a nonzero integer error below the moduli product is nonzero modulo one of them,
/// and that modulus's check alone bounds the pass probability).
pub fn relation_bound_v1(p: &CompactRelationProgramV1) -> CompactRelationBoundV1 {
    let b0 = p.moduli.iter().copied().min().unwrap_or(0).saturating_sub(1);
    let t = p.repetitions as u32;
    let q = 1u128 << p.fs_query_bits.min(COMPACT_FS_MAX_QUERY_BITS_V1);
    let (nu, rounds) = (p.point_variables as u128, p.sumcheck_rounds as u128);
    match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => {
            CompactRelationBoundV1 { per_repetition_bits: Some(b0), instance_bits: Some(t.saturating_mul(b0)) }
        }
        CompactRelationCompilerV1::LinearEvaluationV1 if nu == 0 => CompactRelationBoundV1 { per_repetition_bits: None, instance_bits: None },
        CompactRelationCompilerV1::LinearEvaluationV1 => {
            let rep = b0.saturating_sub(ceil_log2(nu));
            CompactRelationBoundV1 { per_repetition_bits: Some(rep), instance_bits: Some(t.saturating_mul(rep)) }
        }
        CompactRelationCompilerV1::SumCheckHadamardV1 if nu == 0 => CompactRelationBoundV1 { per_repetition_bits: None, instance_bits: None },
        CompactRelationCompilerV1::SumCheckHadamardV1 => CompactRelationBoundV1 {
            per_repetition_bits: Some(b0.saturating_sub(ceil_log2(4 * nu))),
            instance_bits: Some(b0.saturating_sub(ceil_log2(4 * nu + 3 * q))),
        },
        CompactRelationCompilerV1::SumCheckReduceSumV1 if nu + rounds == 0 => {
            CompactRelationBoundV1 { per_repetition_bits: None, instance_bits: None }
        }
        CompactRelationCompilerV1::SumCheckReduceSumV1 => CompactRelationBoundV1 {
            per_repetition_bits: Some(b0.saturating_sub(ceil_log2(nu + rounds))),
            instance_bits: Some(b0.saturating_sub(ceil_log2(nu + rounds + if rounds == 0 { 0 } else { q }))),
        },
    }
}

/// The compact proof bytes one instance may carry (borsh: header, then `t × moduli` transcripts of `rounds` messages of
/// `degree + 1` canonical field elements).
pub fn proof_bytes_bound_v1(p: &CompactRelationProgramV1) -> u128 {
    let msg = 4 + (p.degree as u128 + 1) * 16;
    let transcript = 4 + p.sumcheck_rounds as u128 * msg;
    64 + p.repetitions as u128 * p.moduli.len() as u128 * transcript
}

/// The operand and output bytes one instance's full opening carries.
pub fn material_bytes_v1(p: &CompactRelationProgramV1) -> u128 {
    let ins: u128 = p.in_elements.iter().zip(&p.in_dtypes).map(|(e, t)| *e as u128 * width(*t)).sum();
    ins + p.out_elements as u128 * width(p.out_dtype)
}

/// Verifier field operations for one instance (reads, eq tables, inner products, sum-check rounds).
pub fn verifier_work_bound_v1(p: &CompactRelationProgramV1) -> u128 {
    let reads: u128 = p.out_elements as u128 + p.in_elements.iter().map(|e| *e as u128).sum::<u128>();
    let per = match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => 2 * reads,
        CompactRelationCompilerV1::LinearEvaluationV1 => (1u128 << p.point_variables) + 3 * p.out_elements as u128,
        CompactRelationCompilerV1::SumCheckHadamardV1 => {
            3 * (1u128 << p.point_variables) + 4 * p.out_elements as u128 + 8 * p.sumcheck_rounds as u128
        }
        CompactRelationCompilerV1::SumCheckReduceSumV1 => {
            (1u128 << p.point_variables) + (1u128 << p.sumcheck_rounds) + 3 * reads + 4 * p.sumcheck_rounds as u128
        }
    };
    reads.saturating_add((p.repetitions as u128 * p.moduli.len() as u128).saturating_mul(per))
}

/// **The coverage's probabilistic families for the composed bound** (`misaka_palw_challenge::composition`): one family per compact
/// relation with an error term, `t·b` already folded into `per_repetition_bits` (repetitions = 1), instances over the claim's worst
/// positions.
pub fn compact_check_families_v1(coverage: &CompactPlanCoverageV1) -> Vec<CheckFamilyV1> {
    coverage
        .compact()
        .filter_map(|p| {
            relation_bound_v1(p).instance_bits.map(|bits| CheckFamilyV1 {
                per_repetition_bits: bits,
                repetitions: 1,
                instances: (p.instances_per_position as u128).saturating_mul(coverage.max_positions.max(1) as u128),
            })
        })
        .collect()
}

/// **The whole-claim composed bound** of the compact coverage under the union bound, with the binding term and an adversary budget.
pub fn compact_composed_bound_v1(
    coverage: &CompactPlanCoverageV1,
    binding_bits: u32,
    adversary: &AdversaryBudgetV1,
) -> Result<ComposedBoundV1, CompositionRefusalV1> {
    composed_false_accept_bits_v1(
        &compact_check_families_v1(coverage),
        InstanceCompositionV1::UnionBound,
        Some(binding_bits),
        CoverageV1::Complete,
        adversary,
    )
}

/// **The lifetime failure term** this coverage contributes to `misaka_palw_challenge::cumulative_failure_bits_v1`: the per-attempt
/// bound (retries, grinding and beacons charged, ONE statement) over `admissions` lifetime opportunities. A per-attempt budget that
/// already charges adaptive statements is refused (it would double count `Q`).
pub fn compact_lifetime_failure_term_v1(
    coverage: &CompactPlanCoverageV1,
    binding_bits: u32,
    per_attempt: &AdversaryBudgetV1,
    admissions: u128,
) -> Result<CumulativeFailureTermV1, String> {
    if per_attempt.adaptive_statements != 1 || admissions == 0 {
        return Err("the lifetime term charges admissions once; the per-attempt budget must name one statement".into());
    }
    let bound = compact_composed_bound_v1(coverage, binding_bits, per_attempt).map_err(|e| e.to_string())?;
    let bits = bound.effective_bits.ok_or("no failure term to charge")?.clamp(0, u16::MAX as i64) as u16;
    Ok(CumulativeFailureTermV1 { per_opportunity: EffectiveBitsV1::Bits(bits), opportunities: admissions })
}

// ------------------------------------------------------------------------------------------------------------------------------
// Contract and registration.

/// **The compact relation contract** a registrant signs for one class plan.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactRelationContractV1 {
    pub version: u16,
    pub target: PaidClaimTargetV2,
    pub coverage_root: Digest,
    /// The effective whole-claim bits the registrant states; the derived bound must reach it.
    pub claimed_soundness_bits: u16,
    pub binding_bits: u16,
    pub maximum_proof_bytes_per_position: u64,
    pub maximum_verifier_work_per_position: u128,
}

/// What a structurally valid contract derives (not an approval).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactContractSummaryV1 {
    pub effective_bits: Option<i64>,
    pub proof_bytes_per_position: u128,
    pub material_bytes_per_position: u128,
    pub verifier_work_per_position: u128,
}

impl CompactRelationContractV1 {
    pub fn id(&self) -> Digest {
        object_id(COMPACT_RELATION_CONTRACT_DOMAIN_V1, self)
    }

    /// The checker-suite identity a review tuple must name: the coverage and every compiler it uses.
    pub fn suite_id(&self, coverage: &CompactPlanCoverageV1) -> Digest {
        let mut compilers: Vec<Digest> = coverage.compact().map(|p| compact_relation_compiler_id_v1(p.compiler)).collect();
        compilers.sort();
        compilers.dedup();
        object_id(COMPACT_RELATION_SUITE_DOMAIN_V1, &(self.id(), compilers))
    }

    /// Structure, coverage binding, finite bounds and the derived soundness against `adversary`.
    pub fn check(&self, coverage: &CompactPlanCoverageV1, adversary: &AdversaryBudgetV1) -> Result<CompactContractSummaryV1, String> {
        if self.version != 1
            || self.target == PaidClaimTargetV2::TypedSpec
            || self.coverage_root != coverage.root()
            || coverage.version != 1
            || self.claimed_soundness_bits == 0
            || self.binding_bits == 0
            || self.maximum_proof_bytes_per_position == 0
            || self.maximum_verifier_work_per_position == 0
        {
            return Err("compact relation contract is unversioned, unbound to its coverage or unbounded".into());
        }
        coverage.params.validate().map_err(|e| e.to_string())?;
        let summary = CompactContractSummaryV1 {
            effective_bits: compact_composed_bound_v1(coverage, self.binding_bits as u32, adversary)
                .map_err(|e| e.to_string())?
                .effective_bits,
            proof_bytes_per_position: coverage.proof_bytes_per_position(),
            material_bytes_per_position: coverage.material_bytes_per_position(),
            verifier_work_per_position: coverage.verifier_work_per_position(),
        };
        if summary.proof_bytes_per_position > self.maximum_proof_bytes_per_position as u128
            || summary.verifier_work_per_position > self.maximum_verifier_work_per_position
        {
            return Err("the coverage's derived proof bytes or verifier work exceed the contract's bounds".into());
        }
        if summary.effective_bits.is_none_or(|b| b < self.claimed_soundness_bits as i64) {
            return Err("the contract claims more soundness than its coverage derives".into());
        }
        Ok(summary)
    }
}

/// **Registration** of a compact relation contract: structure and derived bounds ([`CompactRelationContractV1::check`]), then an
/// approved review tuple naming the contract's suite and the class's challenge policy. The caller (consensus) gates this behind the
/// dormant ClaimV2 fence; the shipped registry is empty, so registration is refused as unreviewed today.
pub fn register_compact_relation_contract_v1(
    contract: &CompactRelationContractV1,
    coverage: &CompactPlanCoverageV1,
    adversary: &AdversaryBudgetV1,
    registry: &[ReviewedTupleV2],
    policy: &PostCommitChallengePolicyV1,
) -> Result<CompactContractSummaryV1, String> {
    let summary = contract.check(coverage, adversary)?;
    misaka_palw_challenge::approved_v2(registry, &contract.suite_id(coverage), policy)
        .map_err(|e| format!("compact relation suite is not an approved review tuple: {e}"))?;
    Ok(summary)
}

// ------------------------------------------------------------------------------------------------------------------------------
// Proofs.

/// **One relation instance's compact proof.** `transcripts[rep · moduli + j]` holds the sum-check messages of repetition `rep`
/// under modulus `moduli[j]` (empty for the families without rounds): each message is the round polynomial's values at
/// `0, 1, …, degree`, canonical and below the modulus.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompactRelationProofV1 {
    pub compiler: CompactRelationCompilerV1,
    pub relation: u32,
    pub position: u32,
    pub occurrence: u16,
    pub transcripts: Vec<Vec<Vec<u128>>>,
}

impl CompactRelationProofV1 {
    pub fn encoded_len(&self) -> usize {
        borsh::to_vec(self).map(|v| v.len()).unwrap_or(usize::MAX)
    }
}

/// The root a round message binds for a set of compact proofs (in canonical order) under one coverage.
pub fn compact_proof_root_v1(coverage: &CompactPlanCoverageV1, proofs: &[CompactRelationProofV1]) -> Digest {
    object_id(COMPACT_RELATION_PROOF_DOMAIN_V1, &(coverage.root(), proofs))
}

/// Authenticate the compact proofs against the producer's signed round message before any verification.
pub fn bind_compact_round_message_v1(
    message_root: &Digest,
    coverage: &CompactPlanCoverageV1,
    proofs: &[CompactRelationProofV1],
) -> Result<(), String> {
    if *message_root == [0; 64] || compact_proof_root_v1(coverage, proofs) != *message_root {
        return Err("compact proofs differ from the signed round message".into());
    }
    Ok(())
}

/// Why a compact check did not accept.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompactVerdictV1 {
    /// Operands, proof shape or encoding do not match the compiled relation (a producer fault, not arithmetic fraud).
    #[error("malformed: {0}")]
    Malformed(String),
    /// The relation's check failed: localize with the exact court ([`exact_relation_fault_v1`]).
    #[error("relation check failed at repetition {repetition}, modulus 2^{modulus}-1: {why}")]
    Rejected { repetition: u8, modulus: u32, why: String },
    /// The seed stream budget was exhausted: unavailable, never success or fraud.
    #[error("unavailable: {0}")]
    Unavailable(String),
}

/// A shared word budget across every stream one verification draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactStreamBudgetV1 {
    pub remaining_words: u64,
}

impl CompactStreamBudgetV1 {
    fn stream(&self, seed: &Digest, label: &StreamLabelV1) -> ChallengeStreamV1 {
        ChallengeStreamV1::with_bound(seed, label, self.remaining_words)
    }

    fn spend(&mut self, s: &ChallengeStreamV1) {
        self.remaining_words = self.remaining_words.saturating_sub(s.words_drawn());
    }
}

fn unavailable(e: SampleRefusalV1) -> CompactVerdictV1 {
    CompactVerdictV1::Unavailable(e.to_string())
}

fn draw<F: FieldElemV1>(s: &mut ChallengeStreamV1) -> Result<F, SampleRefusalV1> {
    loop {
        let lo = s.next_u64()? as u128;
        let hi = s.next_u64()? as u128;
        if let Some(f) = F::of_word((hi << 64) | lo) {
            return Ok(f);
        }
    }
}

fn draw_vector<F: FieldElemV1>(
    seed: &Digest,
    label: &StreamLabelV1,
    len: usize,
    budget: &mut CompactStreamBudgetV1,
) -> Result<Vec<F>, CompactVerdictV1> {
    let mut s = budget.stream(seed, label);
    let out = (0..len).map(|_| draw::<F>(&mut s)).collect::<Result<Vec<F>, _>>();
    budget.spend(&s);
    out.map_err(unavailable)
}

fn one<F: FieldElemV1>() -> F {
    F::of_i128(1)
}

fn fsub<F: FieldElemV1>(a: F, b: F) -> F {
    a.add(b.mul(F::of_i128(-1)))
}

fn fpow<F: FieldElemV1>(mut b: F, mut e: u128) -> F {
    let mut acc = one::<F>();
    while e > 0 {
        if e & 1 == 1 {
            acc = acc.mul(b);
        }
        b = b.mul(b);
        e >>= 1;
    }
    acc
}

/// `a^(p−2)`, the inverse of a nonzero element.
fn finv<F: FieldElemV1>(a: F) -> F {
    fpow(a, mersenne(F::EXP) - 2)
}

/// `eq(ρ, x)` for every `x ∈ {0,1}^ν`, the first variable the most significant index bit.
fn eq_table<F: FieldElemV1>(point: &[F]) -> Vec<F> {
    let mut t = vec![one::<F>()];
    for z in point {
        let mut next = Vec::with_capacity(t.len() * 2);
        let nz = fsub(one::<F>(), *z);
        for e in &t {
            next.push(e.mul(nz));
            next.push(e.mul(*z));
        }
        t = next;
    }
    t
}

fn eq_eval<F: FieldElemV1>(a: &[F], b: &[F]) -> F {
    a.iter().zip(b).fold(one::<F>(), |acc, (x, y)| acc.mul(x.mul(*y).add(fsub(one::<F>(), *x).mul(fsub(one::<F>(), *y)))))
}

/// `Σ_x eq(ρ,x)·v(x)` over the zero-padded vector: the multilinear extension of `v` at `ρ`.
fn mle_at<F: FieldElemV1>(eq: &[F], v: &[i128]) -> F {
    F::dot_iter(eq.iter().copied(), v.iter().map(|x| F::of_i128(*x)))
}

/// Bind the first (most significant) variable of a table at `c`.
fn fold<F: FieldElemV1>(t: &mut Vec<F>, c: F) {
    let half = t.len() / 2;
    for i in 0..half {
        t[i] = t[i].add(c.mul(fsub(t[i + half], t[i])));
    }
    t.truncate(half);
}

/// The degree-`d` polynomial through `(0, v_0), …, (d, v_d)`, evaluated at `c`.
fn interpolate<F: FieldElemV1>(values: &[F], c: F) -> F {
    let d = values.len();
    let mut acc = F::zero();
    for i in 0..d {
        let (mut num, mut den) = (one::<F>(), 1i128);
        for j in 0..d {
            if j != i {
                num = num.mul(fsub(c, F::of_i128(j as i128)));
                den *= i as i128 - j as i128;
            }
        }
        acc = acc.add(values[i].mul(num).mul(finv(F::of_i128(den))));
    }
    acc
}

/// The relation's stream scope: compiler, node and the instance's position/occurrence.
fn scope_id(p: &CompactRelationProgramV1, position: u32, occurrence: u16) -> Digest {
    object_id(
        COMPACT_RELATION_SCOPE_DOMAIN_V1,
        &(compact_relation_compiler_id_v1(p.compiler), p.relation, p.block, p.node, position, occurrence),
    )
}

/// **Transcript-bound round challenges** keyed by the round seed: each challenge absorbs the scope, the repetition label, the
/// initial claim and every message so far.
struct FsRoundsV1<'a> {
    seed: &'a Digest,
    relation: u32,
    label: u32,
    transcript: Digest,
}

impl<'a> FsRoundsV1<'a> {
    fn new<F: FieldElemV1>(seed: &'a Digest, scope: Digest, relation: u32, label: u32, claim: F) -> Self {
        let transcript = object_id(COMPACT_RELATION_FS_DOMAIN_V1, &(scope, label, claim.raw()));
        Self { seed, relation, label, transcript }
    }

    fn challenge<F: FieldElemV1>(&mut self, round: u32, msg: &[F], budget: &mut CompactStreamBudgetV1) -> Result<F, CompactVerdictV1> {
        let raw: Vec<u128> = msg.iter().map(|f| f.raw()).collect();
        self.transcript = object_id(COMPACT_RELATION_FS_DOMAIN_V1, &(self.transcript, round, raw));
        let label = StreamLabelV1 { kind: StreamKindV1::ProofRound, scope_id: self.transcript, relation: self.relation, repetition: self.label };
        Ok(draw_vector::<F>(self.seed, &label, 1, budget)?[0])
    }
}

/// The sum-check prover over the product of equal-length tables.
fn sumcheck_prove<F: FieldElemV1>(
    mut tables: Vec<Vec<F>>,
    fs: &mut FsRoundsV1<'_>,
    budget: &mut CompactStreamBudgetV1,
) -> Result<Vec<Vec<F>>, CompactVerdictV1> {
    let degree = tables.len();
    let mut msgs = Vec::new();
    let mut round = 0u32;
    while tables[0].len() > 1 {
        let half = tables[0].len() / 2;
        let mut evals = vec![F::zero(); degree + 1];
        for i in 0..half {
            for (t, e) in evals.iter_mut().enumerate() {
                let tt = F::of_i128(t as i128);
                let prod = tables.iter().fold(one::<F>(), |acc, tab| acc.mul(tab[i].add(tt.mul(fsub(tab[i + half], tab[i])))));
                *e = e.add(prod);
            }
        }
        let c = fs.challenge(round, &evals, budget)?;
        for tab in tables.iter_mut() {
            fold(tab, c);
        }
        msgs.push(evals);
        round += 1;
    }
    Ok(msgs)
}

/// The sum-check verifier: checks every round against the running claim and returns the final claim and the challenge point.
fn sumcheck_verify<F: FieldElemV1>(
    mut claim: F,
    msgs: &[Vec<F>],
    degree: usize,
    fs: &mut FsRoundsV1<'_>,
    budget: &mut CompactStreamBudgetV1,
    (rep, modulus): (u8, u32),
) -> Result<(F, Vec<F>), CompactVerdictV1> {
    let mut point = Vec::with_capacity(msgs.len());
    for (r, msg) in msgs.iter().enumerate() {
        if msg.len() != degree + 1 {
            return Err(CompactVerdictV1::Malformed("a round message of another degree".into()));
        }
        if msg[0].add(msg[1]) != claim {
            return Err(CompactVerdictV1::Rejected { repetition: rep, modulus, why: format!("sum-check round {r} does not sum to its claim") });
        }
        let c = fs.challenge(r as u32, msg, budget)?;
        claim = interpolate(msg, c);
        point.push(c);
    }
    Ok((claim, point))
}

// ---- operand layout --------------------------------------------------------------------------------------------------------

fn strides(shape: &[usize]) -> Vec<usize> {
    let mut s = vec![1usize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1] * shape[i + 1];
    }
    s
}

/// For every element of `out_shape` (row-major), the index of the `in_shape` element it reads under numpy broadcasting (TIR's rule).
fn broadcast_index_map(out_shape: &[usize], in_shape: &[usize]) -> Option<Vec<usize>> {
    if in_shape.len() > out_shape.len() {
        return None;
    }
    let off = out_shape.len() - in_shape.len();
    if (0..in_shape.len()).any(|k| in_shape[k] != 1 && in_shape[k] != out_shape[off + k]) {
        return None;
    }
    let (ost, ist) = (strides(out_shape), strides(in_shape));
    let n: usize = out_shape.iter().product();
    Some(
        (0..n)
            .map(|i| (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { ((i / ost[off + k]) % out_shape[off + k]) * ist[k] }).sum())
            .collect(),
    )
}

fn broadcast_values(out_shape: &[usize], t: &Tensor) -> Option<Vec<i128>> {
    Some(broadcast_index_map(out_shape, &t.shape)?.into_iter().map(|i| t.data[i]).collect())
}

fn well_typed(t: &Tensor, tag: u8) -> bool {
    t.dtype.tag() == tag && t.shape.iter().product::<usize>() == t.data.len() && t.data.iter().all(|v| t.dtype.contains(*v))
}

/// `ReduceSum{axis}`'s operand laid out as `[out_index][j]` (`j` the reduced coordinate, fastest), each row padded to `2^ν_j`.
fn reduce_layout(a: &Tensor, axis: usize, nu_j: u32) -> Option<(Vec<i128>, usize)> {
    if axis >= a.shape.len() {
        return None;
    }
    let outer: usize = a.shape[..axis].iter().product();
    let j = a.shape[axis];
    let inner: usize = a.shape[axis + 1..].iter().product();
    let row = 1usize << nu_j;
    if j > row {
        return None;
    }
    let mut out = vec![0i128; outer * inner * row];
    for o in 0..outer {
        for i in 0..inner {
            for t in 0..j {
                out[(o * inner + i) * row + t] = a.data[(o * j + t) * inner + i];
            }
        }
    }
    Some((out, outer * inner))
}

/// The instance's shape checks against its compiled relation; returns `ν` (and `ν_j`) at this instance's actual size.
fn instance_variables(p: &CompactRelationProgramV1, inputs: &[Tensor], y: &Tensor) -> Result<(u32, u32), CompactVerdictV1> {
    let bad = |why: &str| Err(CompactVerdictV1::Malformed(why.into()));
    if inputs.len() != p.in_dtypes.len() || !well_typed(y, p.out_dtype) || y.data.len() as u64 > p.out_elements {
        return bad("operands or output differ from the compiled relation");
    }
    for ((t, tag), max) in inputs.iter().zip(&p.in_dtypes).zip(&p.in_elements) {
        if !well_typed(t, *tag) || t.data.len() as u64 > *max {
            return bad("an operand differs from the compiled relation");
        }
    }
    if y.data.is_empty() {
        return bad("an empty output");
    }
    let nu = ceil_log2(y.data.len() as u128);
    match p.compiler {
        CompactRelationCompilerV1::SumCheckReduceSumV1 => {
            let a = &inputs[0];
            let axis = p.axis as usize;
            let mut kept = a.shape.clone();
            if axis >= kept.len() {
                return bad("a reduction axis outside its operand");
            }
            kept[axis] = 1;
            if kept != y.shape {
                return bad("a reduction whose output is not its operand with the axis kept at 1");
            }
            let nu_j = ceil_log2(a.shape[axis] as u128);
            if nu > p.point_variables || nu_j > p.sumcheck_rounds {
                return bad("an instance beyond the compiled worst case");
            }
            Ok((nu, nu_j))
        }
        CompactRelationCompilerV1::FreivaldsSeededV1 => Ok((0, 0)),
        _ => {
            if nu > p.point_variables {
                return bad("an instance beyond the compiled worst case");
            }
            Ok((nu, 0))
        }
    }
}

// ---- per-modulus checks ----------------------------------------------------------------------------------------------------

/// Freivalds over one modulus: `Y_b·r = X_b·(W_b·r)` for every output batch slice `b`, one `r` per repetition shared by the slices.
fn freivalds_check<F: FieldElemV1>(x: &Tensor, w: &Tensor, y: &Tensor, r: &[F]) -> Result<Option<usize>, CompactVerdictV1> {
    let (xr, wr, yr) = (x.shape.len(), w.shape.len(), y.shape.len());
    if xr < 2 || wr < 2 || yr < 2 {
        return Err(CompactVerdictV1::Malformed("a product operand of rank below 2".into()));
    }
    let (m, k, n) = (x.shape[xr - 2], x.shape[xr - 1], w.shape[wr - 1]);
    if w.shape[wr - 2] != k || y.shape[yr - 2] != m || y.shape[yr - 1] != n || r.len() != n {
        return Err(CompactVerdictV1::Malformed("product dimensions disagree".into()));
    }
    let batch = &y.shape[..yr - 2];
    let (Some(xb), Some(wb)) = (broadcast_index_map(batch, &x.shape[..xr - 2]), broadcast_index_map(batch, &w.shape[..wr - 2])) else {
        return Err(CompactVerdictV1::Malformed("product batch dimensions do not broadcast".into()));
    };
    let mut projections: BTreeMap<usize, Vec<F>> = BTreeMap::new();
    for b in 0..xb.len() {
        let wo = wb[b] * k * n;
        let wr = projections.entry(wb[b]).or_insert_with(|| {
            (0..k).map(|t| F::dot_iter((0..n).map(|c| F::of_i128(w.data[wo + t * n + c])), r.iter().copied())).collect()
        });
        let (xo, yo) = (xb[b] * m * k, b * m * n);
        for i in 0..m {
            let lhs = F::dot_iter((0..k).map(|t| F::of_i128(x.data[xo + i * k + t])), wr.iter().copied());
            let rhs = F::dot_iter((0..n).map(|c| F::of_i128(y.data[yo + i * n + c])), r.iter().copied());
            if lhs != rhs {
                return Ok(Some(b));
            }
        }
    }
    Ok(None)
}

fn canonical<F: FieldElemV1>(raw: &[u128]) -> Result<Vec<F>, CompactVerdictV1> {
    let p = mersenne(F::EXP);
    if raw.iter().any(|v| *v >= p) {
        return Err(CompactVerdictV1::Malformed("a non-canonical field element".into()));
    }
    Ok(raw.iter().map(|v| F::of_raw(*v)).collect())
}

/// The operands as full-output-shape vectors (broadcast materialized) for the elementwise families.
fn elementwise_operands(inputs: &[Tensor], y: &Tensor) -> Result<(Vec<i128>, Vec<i128>), CompactVerdictV1> {
    match (broadcast_values(&y.shape, &inputs[0]), broadcast_values(&y.shape, &inputs[1])) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(CompactVerdictV1::Malformed("an operand does not broadcast to the output".into())),
    }
}

fn labels(p: &CompactRelationProgramV1, scope: Digest, rep: u8, j: usize, kind: StreamKindV1) -> StreamLabelV1 {
    StreamLabelV1 { kind, scope_id: scope, relation: p.relation, repetition: rep as u32 * MODULI_V2.len() as u32 + j as u32 }
}

/// Prove (`Some(proof)` ⇒ write the transcript) or verify one (repetition, modulus) of one instance.
#[allow(clippy::too_many_arguments)]
fn run_one<F: FieldElemV1>(
    p: &CompactRelationProgramV1,
    seed: &Digest,
    scope: Digest,
    (rep, j): (u8, usize),
    (nu, nu_j): (u32, u32),
    inputs: &[Tensor],
    y: &Tensor,
    transcript: Option<&[Vec<u128>]>,
    budget: &mut CompactStreamBudgetV1,
) -> Result<Vec<Vec<u128>>, CompactVerdictV1> {
    let modulus = F::EXP;
    let reject = |why: String| CompactVerdictV1::Rejected { repetition: rep, modulus, why };
    let label = labels(p, scope, rep, j, StreamKindV1::Vector).repetition;
    match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => {
            let n = *y.shape.last().ok_or_else(|| CompactVerdictV1::Malformed("a scalar product output".into()))?;
            let r = draw_vector::<F>(seed, &labels(p, scope, rep, j, StreamKindV1::Freivalds), n, budget)?;
            match freivalds_check(&inputs[0], &inputs[1], y, &r)? {
                None => Ok(Vec::new()),
                Some(b) => Err(reject(format!("Freivalds differs at batch slice {b}"))),
            }
        }
        CompactRelationCompilerV1::LinearEvaluationV1 => {
            let (a, b) = elementwise_operands(inputs, y)?;
            let rho = draw_vector::<F>(seed, &labels(p, scope, rep, j, StreamKindV1::Vector), nu as usize, budget)?;
            let eq = eq_table(&rho);
            let (ya, aa, ba) = (mle_at(&eq, &y.data), mle_at(&eq, &a), mle_at(&eq, &b));
            let rhs = if p.prim_tag == Prim::Sub.tag() { fsub(aa, ba) } else { aa.add(ba) };
            if ya != rhs {
                return Err(reject("the output's multilinear extension differs at the random point".into()));
            }
            Ok(Vec::new())
        }
        CompactRelationCompilerV1::SumCheckHadamardV1 => {
            let (a, b) = elementwise_operands(inputs, y)?;
            let rho = draw_vector::<F>(seed, &labels(p, scope, rep, j, StreamKindV1::Vector), nu as usize, budget)?;
            let eq = eq_table(&rho);
            let claim = mle_at(&eq, &y.data);
            let mut fs = FsRoundsV1::new(seed, scope, p.relation, label, claim);
            let msgs: Vec<Vec<F>> = match transcript {
                None => {
                    let pad = |v: &[i128]| {
                        let mut t: Vec<F> = v.iter().map(|x| F::of_i128(*x)).collect();
                        t.resize(1usize << nu, F::zero());
                        t
                    };
                    sumcheck_prove(vec![eq.clone(), pad(&a), pad(&b)], &mut fs, budget)?
                }
                Some(raw) => raw.iter().map(|m| canonical::<F>(m)).collect::<Result<_, _>>()?,
            };
            if msgs.len() != nu as usize {
                return Err(CompactVerdictV1::Malformed("a transcript of another round count".into()));
            }
            if transcript.is_some() {
                let mut fs = FsRoundsV1::new(seed, scope, p.relation, label, claim);
                let (fin, kappa) = sumcheck_verify(claim, &msgs, 3, &mut fs, budget, (rep, modulus))?;
                let ek = eq_table(&kappa);
                if fin != eq_eval(&rho, &kappa).mul(mle_at(&ek, &a)).mul(mle_at(&ek, &b)) {
                    return Err(reject("the sum-check's final evaluation differs from the operands'".into()));
                }
            }
            Ok(msgs.iter().map(|m| m.iter().map(|f| f.raw()).collect()).collect())
        }
        CompactRelationCompilerV1::SumCheckReduceSumV1 => {
            let (layout, n_out) = reduce_layout(&inputs[0], p.axis as usize, nu_j)
                .ok_or_else(|| CompactVerdictV1::Malformed("a reduction operand the layout cannot hold".into()))?;
            if n_out != y.data.len() {
                return Err(CompactVerdictV1::Malformed("a reduction whose output count disagrees".into()));
            }
            let rho = draw_vector::<F>(seed, &labels(p, scope, rep, j, StreamKindV1::Vector), nu as usize, budget)?;
            let eq = eq_table(&rho);
            let claim = mle_at(&eq, &y.data);
            let row = 1usize << nu_j;
            // g(j) = Ã'(ρ, j): the reduced coordinate's table with the output coordinates bound at ρ.
            let mut g = vec![F::zero(); row];
            for (o, e) in eq.iter().enumerate().take(n_out) {
                for (t, gt) in g.iter_mut().enumerate() {
                    *gt = gt.add(e.mul(F::of_i128(layout[o * row + t])));
                }
            }
            let mut fs = FsRoundsV1::new(seed, scope, p.relation, label, claim);
            let msgs: Vec<Vec<F>> = match transcript {
                None => sumcheck_prove(vec![g.clone()], &mut fs, budget)?,
                Some(raw) => raw.iter().map(|m| canonical::<F>(m)).collect::<Result<_, _>>()?,
            };
            if msgs.len() != nu_j as usize {
                return Err(CompactVerdictV1::Malformed("a transcript of another round count".into()));
            }
            if transcript.is_some() {
                let mut fs = FsRoundsV1::new(seed, scope, p.relation, label, claim);
                let (fin, kappa) = sumcheck_verify(claim, &msgs, 1, &mut fs, budget, (rep, modulus))?;
                let ek = eq_table(&kappa);
                if fin != F::dot_iter(ek.iter().copied(), g.iter().copied()) {
                    return Err(reject("the sum-check's final evaluation differs from the operand's".into()));
                }
            }
            Ok(msgs.iter().map(|m| m.iter().map(|f| f.raw()).collect()).collect())
        }
    }
}

fn dispatch(
    p: &CompactRelationProgramV1,
    seed: &Digest,
    scope: Digest,
    (rep, j): (u8, usize),
    vars: (u32, u32),
    inputs: &[Tensor],
    y: &Tensor,
    transcript: Option<&[Vec<u128>]>,
    budget: &mut CompactStreamBudgetV1,
) -> Result<Vec<Vec<u128>>, CompactVerdictV1> {
    match p.moduli.get(j) {
        Some(127) => run_one::<Fp>(p, seed, scope, (rep, j), vars, inputs, y, transcript, budget),
        Some(107) => run_one::<F107>(p, seed, scope, (rep, j), vars, inputs, y, transcript, budget),
        Some(89) => run_one::<F89>(p, seed, scope, (rep, j), vars, inputs, y, transcript, budget),
        _ => Err(CompactVerdictV1::Malformed("a modulus outside the compiled set".into())),
    }
}

fn moduli_valid(p: &CompactRelationProgramV1) -> bool {
    !p.moduli.is_empty() && p.moduli.len() <= MODULI_V2.len() && p.moduli[..] == MODULI_V2[..p.moduli.len()] && p.repetitions > 0
}

/// **The honest prover**: one instance's compact proof from its authenticated operands and the paid round seed.
pub fn prove_relation_v1(
    p: &CompactRelationProgramV1,
    seed: &Digest,
    position: u32,
    occurrence: u16,
    inputs: &[Tensor],
    y: &Tensor,
    budget: &mut CompactStreamBudgetV1,
) -> Result<CompactRelationProofV1, CompactVerdictV1> {
    if !moduli_valid(p) {
        return Err(CompactVerdictV1::Malformed("a compiled relation without its moduli or repetitions".into()));
    }
    let vars = instance_variables(p, inputs, y)?;
    let scope = scope_id(p, position, occurrence);
    let mut transcripts = Vec::with_capacity(p.repetitions as usize * p.moduli.len());
    for rep in 0..p.repetitions {
        for j in 0..p.moduli.len() {
            transcripts.push(dispatch(p, seed, scope, (rep, j), vars, inputs, y, None, budget)?);
        }
    }
    Ok(CompactRelationProofV1 { compiler: p.compiler, relation: p.relation, position, occurrence, transcripts })
}

/// **The verifier** of one instance: `inputs` and `y` must already be authenticated against the claim's commitments (use
/// [`verify_relation_committed_v1`] to do it here). `Ok(())` only when every repetition under every modulus passes.
pub fn verify_relation_v1(
    p: &CompactRelationProgramV1,
    seed: &Digest,
    inputs: &[Tensor],
    y: &Tensor,
    proof: &CompactRelationProofV1,
    budget: &mut CompactStreamBudgetV1,
) -> Result<(), CompactVerdictV1> {
    if !moduli_valid(p) {
        return Err(CompactVerdictV1::Malformed("a compiled relation without its moduli or repetitions".into()));
    }
    if proof.compiler != p.compiler || proof.relation != p.relation || proof.transcripts.len() != p.repetitions as usize * p.moduli.len()
    {
        return Err(CompactVerdictV1::Malformed("the proof names another relation or another repetition count".into()));
    }
    if proof.encoded_len() as u128 > proof_bytes_bound_v1(p) {
        return Err(CompactVerdictV1::Malformed("the proof exceeds its compiled byte bound".into()));
    }
    let vars = instance_variables(p, inputs, y)?;
    let scope = scope_id(p, proof.position, proof.occurrence);
    for rep in 0..p.repetitions {
        for j in 0..p.moduli.len() {
            let t = &proof.transcripts[rep as usize * p.moduli.len() + j];
            if !p.compiler.transcript_bound() && !t.is_empty() {
                return Err(CompactVerdictV1::Malformed("a transcript on a family without rounds".into()));
            }
            dispatch(p, seed, scope, (rep, j), vars, inputs, y, Some(t), budget)?;
        }
    }
    Ok(())
}

/// [`verify_relation_v1`] after authenticating every operand and the output against the claim's commitments under the class's
/// commitment function (e.g. [`crate::trace::tensor_commitment`] or [`crate::merkle3::tensor_commitment_v3`]).
#[allow(clippy::too_many_arguments)]
pub fn verify_relation_committed_v1(
    p: &CompactRelationProgramV1,
    seed: &Digest,
    inputs: &[Tensor],
    input_commitments: &[Digest],
    y: &Tensor,
    output_commitment: &Digest,
    commit: fn(&Tensor) -> Digest,
    proof: &CompactRelationProofV1,
    budget: &mut CompactStreamBudgetV1,
) -> Result<(), CompactVerdictV1> {
    if inputs.len() != input_commitments.len()
        || inputs.iter().zip(input_commitments).any(|(t, c)| commit(t) != *c)
        || commit(y) != *output_commitment
    {
        return Err(CompactVerdictV1::Malformed("an operand or the output is not the committed value".into()));
    }
    verify_relation_v1(p, seed, inputs, y, proof, budget)
}

/// **Localization**: the first output element the exact relation disagrees with (`None`: the relation holds exactly). Used after
/// [`CompactVerdictV1::Rejected`] to file the existing exact court; never a conviction by itself.
pub fn exact_relation_fault_v1(p: &CompactRelationProgramV1, inputs: &[Tensor], y: &Tensor) -> Option<usize> {
    let expect: Vec<Option<i128>> = match p.compiler {
        CompactRelationCompilerV1::FreivaldsSeededV1 => {
            let (x, w) = (&inputs[0], &inputs[1]);
            let (xr, wr, yr) = (x.shape.len(), w.shape.len(), y.shape.len());
            let (m, k, n) = (x.shape[xr - 2], x.shape[xr - 1], w.shape[wr - 1]);
            let batch = &y.shape[..yr - 2];
            let (xb, wb) = (broadcast_index_map(batch, &x.shape[..xr - 2])?, broadcast_index_map(batch, &w.shape[..wr - 2])?);
            let mut out = Vec::with_capacity(y.data.len());
            for b in 0..xb.len() {
                for i in 0..m {
                    for c in 0..n {
                        out.push((0..k).try_fold(0i128, |acc, t| {
                            acc.checked_add(x.data[xb[b] * m * k + i * k + t].checked_mul(w.data[wb[b] * k * n + t * n + c])?)
                        }));
                    }
                }
            }
            out
        }
        CompactRelationCompilerV1::LinearEvaluationV1 | CompactRelationCompilerV1::SumCheckHadamardV1 => {
            let a = broadcast_values(&y.shape, &inputs[0])?;
            let b = broadcast_values(&y.shape, &inputs[1])?;
            a.iter()
                .zip(&b)
                .map(|(a, b)| match p.prim_tag {
                    t if t == Prim::Add.tag() => a.checked_add(*b),
                    t if t == Prim::Sub.tag() => a.checked_sub(*b),
                    _ => a.checked_mul(*b),
                })
                .collect()
        }
        CompactRelationCompilerV1::SumCheckReduceSumV1 => {
            let (layout, n_out) = reduce_layout(&inputs[0], p.axis as usize, ceil_log2(inputs[0].shape[p.axis as usize] as u128))?;
            let row = layout.len() / n_out.max(1);
            (0..n_out).map(|o| layout[o * row..(o + 1) * row].iter().try_fold(0i128, |acc, v| acc.checked_add(*v))).collect()
        }
    };
    if expect.len() != y.data.len() {
        return Some(0);
    }
    expect.iter().zip(&y.data).position(|(e, v)| *e != Some(*v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::CourtIdV1;
    use crate::plan::MatMulDimsV1;

    fn params(on: bool) -> CompactCompileParamsV1 {
        CompactCompileParamsV1 { compact_exact_arithmetic: on, arithmetic_repetitions: 2, fs_query_bits: 64 }
    }

    fn data(n: usize, seed: u64, lo: i128, hi: i128) -> Vec<i128> {
        let mut s = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                lo + ((s >> 11) as i128).rem_euclid(hi - lo + 1)
            })
            .collect()
    }

    fn tensor(dtype: DType, shape: &[usize], data: Vec<i128>) -> Tensor {
        Tensor { dtype, shape: shape.to_vec(), data }
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

    fn compact(c: Result<CompactCoverageV1, CompactRefusalV1>) -> CompactRelationProgramV1 {
        match c.expect("compiles") {
            CompactCoverageV1::Compact(p) => p,
            other => panic!("expected a compact relation, got {other:?}"),
        }
    }

    fn budget() -> CompactStreamBudgetV1 {
        CompactStreamBudgetV1 { remaining_words: 1 << 24 }
    }

    fn matmul_program(checker: CheckerIdV1, xd: DType, wd: DType, yd: DType) -> CompactRelationProgramV1 {
        let mut r = rel(&Prim::MatMul, ConstraintFamilyV1::DenseMatrix, checker, &[(xd, 2 * 3 * 4), (wd, 4 * 5)], (yd, 2 * 3 * 5));
        r.court = CourtIdV1::MatMulScalar;
        r.matmul = Some(MatMulDimsV1 { batch: 2, m: 3, k: 4, n: 5 });
        compact(compile_relation_v1(0, &Prim::MatMul, &r, &params(false)))
    }

    fn matmul_operands(xd: DType, wd: DType, yd: DType, lo: i128, hi: i128) -> (Vec<Tensor>, Tensor) {
        let x = tensor(xd, &[2, 3, 4], data(24, 1, lo, hi));
        let w = tensor(wd, &[4, 5], data(20, 2, lo, hi));
        let mut y = vec![0i128; 30];
        for b in 0..2 {
            for i in 0..3 {
                for c in 0..5 {
                    y[b * 15 + i * 5 + c] = (0..4).map(|t| x.data[b * 12 + i * 4 + t] * w.data[t * 5 + c]).sum();
                }
            }
        }
        (vec![x, w], tensor(yd, &[2, 3, 5], y))
    }

    #[test]
    fn freivalds_accepts_honest_products_and_catches_one_tampered_scalar_under_every_modulus() {
        for (checker, xd, wd, yd, lo, hi) in [
            (CheckerIdV1::FreivaldsM127, DType::I8, DType::I8, DType::I32, -128, 127),
            (CheckerIdV1::FreivaldsCrtV2, DType::I64, DType::I64, DType::I128, -(1i128 << 40), 1i128 << 40),
        ] {
            let p = matmul_program(checker, xd, wd, yd);
            if checker == CheckerIdV1::FreivaldsCrtV2 {
                assert!(p.moduli.len() >= 2, "an i64·i64→i128 product needs the CRT set");
            }
            let (ins, y) = matmul_operands(xd, wd, yd, lo, hi);
            let seed = [9u8; 64];
            let proof = prove_relation_v1(&p, &seed, 4, 0, &ins, &y, &mut budget()).unwrap();
            assert!(proof.transcripts.iter().all(Vec::is_empty), "Freivalds sends no rounds");
            assert_eq!(verify_relation_v1(&p, &seed, &ins, &y, &proof, &mut budget()), Ok(()));
            let mut bad = y.clone();
            bad.data[17] += 1;
            assert!(matches!(verify_relation_v1(&p, &seed, &ins, &bad, &proof, &mut budget()), Err(CompactVerdictV1::Rejected { .. })));
            assert_eq!(exact_relation_fault_v1(&p, &ins, &bad), Some(17));
            assert_eq!(exact_relation_fault_v1(&p, &ins, &y), None);
        }
    }

    fn arithmetic(prim: Prim, a_shape: &[usize], b_shape: Option<&[usize]>, out_shape: &[usize]) -> (CompactRelationProgramV1, Vec<Tensor>, Tensor) {
        let n = |s: &[usize]| s.iter().product::<usize>();
        let a = tensor(DType::I32, a_shape, data(n(a_shape), 3, -1000, 1000));
        let mut ins = vec![a.clone()];
        let y = match (&prim, b_shape) {
            (Prim::ReduceSum { axis }, None) => {
                let ax = *axis as usize;
                let (outer, j, inner) = (n(&a_shape[..ax]), a_shape[ax], n(&a_shape[ax + 1..]));
                let mut out = vec![0i128; outer * inner];
                for o in 0..outer {
                    for i in 0..inner {
                        out[o * inner + i] = (0..j).map(|t| a.data[(o * j + t) * inner + i]).sum();
                    }
                }
                tensor(DType::I64, out_shape, out)
            }
            (_, Some(bs)) => {
                let b = tensor(DType::I32, bs, data(n(bs), 4, -1000, 1000));
                let (av, bv) = (broadcast_values(out_shape, &a).unwrap(), broadcast_values(out_shape, &b).unwrap());
                ins.push(b);
                let out = av
                    .iter()
                    .zip(&bv)
                    .map(|(x, z)| match prim {
                        Prim::Add => x + z,
                        Prim::Sub => x - z,
                        _ => x * z,
                    })
                    .collect();
                tensor(DType::I64, out_shape, out)
            }
            _ => unreachable!(),
        };
        let r = rel(
            &prim,
            ConstraintFamilyV1::ExactArithmetic,
            CheckerIdV1::ExactRecompute,
            &ins.iter().map(|t| (t.dtype, t.data.len() as u64)).collect::<Vec<_>>(),
            (DType::I64, y.data.len() as u64),
        );
        (compact(compile_relation_v1(1, &prim, &r, &params(true))), ins, y)
    }

    #[test]
    fn arithmetic_families_are_complete_on_honest_values_and_catch_a_tampered_output() {
        let cases = [
            arithmetic(Prim::Add, &[3, 5], Some(&[5]), &[3, 5]),
            arithmetic(Prim::Sub, &[4, 4], Some(&[4, 4]), &[4, 4]),
            arithmetic(Prim::Mul, &[2, 3, 5], Some(&[1, 3, 1]), &[2, 3, 5]),
            arithmetic(Prim::ReduceSum { axis: 1 }, &[3, 7, 2], None, &[3, 1, 2]),
        ];
        let seed = [5u8; 64];
        for (p, ins, y) in cases {
            let proof = prove_relation_v1(&p, &seed, 0, 1, &ins, &y, &mut budget()).unwrap();
            assert_eq!(verify_relation_v1(&p, &seed, &ins, &y, &proof, &mut budget()), Ok(()), "{:?}", p.compiler);
            let mut bad = y.clone();
            bad.data[1] -= 3;
            assert!(
                matches!(verify_relation_v1(&p, &seed, &ins, &bad, &proof, &mut budget()), Err(CompactVerdictV1::Rejected { .. })),
                "{:?} accepted a tampered output",
                p.compiler
            );
            assert_eq!(exact_relation_fault_v1(&p, &ins, &bad), Some(1));
            // Another seed is another proof: the transcript is bound to the round seed.
            if p.compiler.transcript_bound() {
                assert!(verify_relation_v1(&p, &[6u8; 64], &ins, &y, &proof, &mut budget()).is_err());
            }
        }
    }

    #[test]
    fn a_forged_round_message_or_noncanonical_element_is_refused() {
        let (p, ins, y) = arithmetic(Prim::Mul, &[4, 8], Some(&[4, 8]), &[4, 8]);
        let seed = [1u8; 64];
        let proof = prove_relation_v1(&p, &seed, 0, 0, &ins, &y, &mut budget()).unwrap();
        let mut forged = proof.clone();
        forged.transcripts[0][1][2] = (forged.transcripts[0][1][2] + 1) % mersenne(127);
        assert!(verify_relation_v1(&p, &seed, &ins, &y, &forged, &mut budget()).is_err());
        let mut noncanonical = proof.clone();
        noncanonical.transcripts[0][0][0] = mersenne(127);
        assert!(matches!(verify_relation_v1(&p, &seed, &ins, &y, &noncanonical, &mut budget()), Err(CompactVerdictV1::Malformed(_))));
        let mut short = proof.clone();
        short.transcripts[1].pop();
        assert!(matches!(verify_relation_v1(&p, &seed, &ins, &y, &short, &mut budget()), Err(CompactVerdictV1::Malformed(_))));
        let mut other = proof.clone();
        other.relation += 1;
        assert!(matches!(verify_relation_v1(&p, &seed, &ins, &y, &other, &mut budget()), Err(CompactVerdictV1::Malformed(_))));
        assert!(matches!(
            verify_relation_v1(&p, &seed, &ins, &y, &proof, &mut CompactStreamBudgetV1 { remaining_words: 3 }),
            Err(CompactVerdictV1::Unavailable(_))
        ));
    }

    #[test]
    fn the_proof_is_much_smaller_than_the_material_it_replaces() {
        let (p, ins, y) = arithmetic(Prim::Mul, &[64, 64], Some(&[64, 64]), &[64, 64]);
        let proof = prove_relation_v1(&p, &[2u8; 64], 0, 0, &ins, &y, &mut budget()).unwrap();
        assert!((proof.encoded_len() as u128) <= proof_bytes_bound_v1(&p));
        assert!((proof.encoded_len() as u128) * 20 < material_bytes_v1(&p), "{} vs {}", proof.encoded_len(), material_bytes_v1(&p));
        let m = matmul_program(CheckerIdV1::FreivaldsM127, DType::I8, DType::I8, DType::I32);
        assert_eq!(proof_bytes_bound_v1(&m), 64 + 2 * 4, "a Freivalds proof is its header");
    }

    #[test]
    fn the_bound_arithmetic_is_exact_at_known_points() {
        let m127 = matmul_program(CheckerIdV1::FreivaldsM127, DType::I8, DType::I8, DType::I32);
        assert_eq!(relation_bound_v1(&m127), CompactRelationBoundV1 { per_repetition_bits: Some(126), instance_bits: Some(252) });
        let crt = matmul_program(CheckerIdV1::FreivaldsCrtV2, DType::I64, DType::I64, DType::I128);
        let b0 = crt.moduli.iter().min().unwrap() - 1;
        assert_eq!(relation_bound_v1(&crt).instance_bits, Some(2 * b0));
        let mut lin = arithmetic(Prim::Add, &[32, 32], Some(&[32, 32]), &[32, 32]).0; // ν = 10
        assert_eq!(lin.point_variables, 10);
        assert_eq!(relation_bound_v1(&lin), CompactRelationBoundV1 { per_repetition_bits: Some(122), instance_bits: Some(244) });
        lin.point_variables = 0;
        assert_eq!(relation_bound_v1(&lin).instance_bits, None, "a scalar identity is exact");
        let had = arithmetic(Prim::Mul, &[32, 32], Some(&[32, 32]), &[32, 32]).0;
        // 4ν = 40 → ⌈log2 40⌉ = 6; 40 + 3·2^64 → ⌈log2⌉ = 66.
        assert_eq!(relation_bound_v1(&had), CompactRelationBoundV1 { per_repetition_bits: Some(120), instance_bits: Some(60) });
        let red = arithmetic(Prim::ReduceSum { axis: 1 }, &[3, 9, 2], None, &[3, 1, 2]).0; // ν_o = 3, ν_j = 4
        assert_eq!((red.point_variables, red.sumcheck_rounds), (3, 4));
        assert_eq!(relation_bound_v1(&red), CompactRelationBoundV1 { per_repetition_bits: Some(123), instance_bits: Some(61) });
        let mut more = red.clone();
        more.fs_query_bits = 100;
        assert_eq!(relation_bound_v1(&more).instance_bits, Some(25), "a larger declared grinding budget costs bits one for one");
    }

    #[test]
    fn the_composed_bound_charges_the_union_and_the_lifetime_term_charges_admissions() {
        let (p, ..) = arithmetic(Prim::Mul, &[32, 32], Some(&[32, 32]), &[32, 32]);
        let m = matmul_program(CheckerIdV1::FreivaldsM127, DType::I8, DType::I8, DType::I32);
        let coverage = CompactPlanCoverageV1 {
            version: 1,
            plan_root: [3; 64],
            max_positions: 1024,
            params: params(true),
            entries: vec![
                CompactCoverageV1::Compact(m),
                CompactCoverageV1::Compact(p),
                CompactCoverageV1::ExactRetained {
                    relation: 2,
                    family: ConstraintFamilyV1::Nonlinear,
                    checker: CheckerIdV1::ExactRecompute,
                },
            ],
        };
        let fams = compact_check_families_v1(&coverage);
        assert_eq!(fams.len(), 2);
        assert_eq!(fams[0].instances, 2 * 1024, "batch slices × positions");
        let none = compact_composed_bound_v1(&coverage, 256, &AdversaryBudgetV1::NONE).unwrap();
        // check = min(252, 60) − ⌈log2 (2048 + 1024)⌉ = 60 − 12 = 48; union of {check, binding} costs one more bit.
        assert_eq!(none.check_bits, Some(48));
        assert_eq!(none.effective_bits, Some(47));
        let term = compact_lifetime_failure_term_v1(&coverage, 256, &AdversaryBudgetV1::NONE, 1 << 20).unwrap();
        assert_eq!(term.per_opportunity, EffectiveBitsV1::Bits(47));
        let lifetime = misaka_palw_challenge::cumulative_failure_bits_v1(&[term]).unwrap();
        assert_eq!(lifetime, EffectiveBitsV1::Bits(27), "2^20 admissions cost 20 bits");
        let adaptive = AdversaryBudgetV1 { adaptive_statements: 2, ..AdversaryBudgetV1::NONE };
        assert!(compact_lifetime_failure_term_v1(&coverage, 256, &adaptive, 4).is_err(), "Q is charged once");
    }

    #[test]
    fn unknown_pairs_and_unimplemented_families_are_refused_or_kept_exact_never_silently_compiled() {
        let p = params(true);
        // A dense family with an exact checker is not in the inventory: the whole plan is refused.
        let r = rel(&Prim::MatMul, ConstraintFamilyV1::DenseMatrix, CheckerIdV1::ExactRecompute, &[(DType::I8, 4), (DType::I8, 4)], (DType::I32, 4));
        assert!(matches!(compile_relation_v1(0, &Prim::MatMul, &r, &p), Err(CompactRefusalV1::UnknownFamilyChecker { .. })));
        // A plan tag that is not the node's primitive is refused.
        let r = rel(&Prim::Add, ConstraintFamilyV1::ExactArithmetic, CheckerIdV1::ExactRecompute, &[(DType::I8, 4), (DType::I8, 4)], (DType::I32, 4));
        assert!(matches!(compile_relation_v1(0, &Prim::Mul, &r, &p), Err(CompactRefusalV1::Malformed { .. })));
        // Every non-implemented family of the inventory is kept exact, never compiled.
        for row in COMPACT_RELATION_INVENTORY_V1 {
            if row.status != CompactInventoryStatusV1::ExactRetained {
                continue;
            }
            let prim = match row.family {
                ConstraintFamilyV1::Structure => Prim::Reshape,
                ConstraintFamilyV1::QuantRange => Prim::Log2Floor,
                ConstraintFamilyV1::Nonlinear => Prim::IntExp,
                ConstraintFamilyV1::Selection => Prim::TopK { axis: 0, k: 1 },
                ConstraintFamilyV1::RecurrentState => Prim::StateWrite { state: 0 },
                _ => Prim::Reshape,
            };
            let r = rel(&prim, row.family, row.checker, &[(DType::I32, 4)], (DType::I32, 4));
            assert!(matches!(compile_relation_v1(0, &prim, &r, &p), Ok(CompactCoverageV1::ExactRetained { .. })), "{:?}", row.family);
        }
        // Cast stays exact even when arithmetic is compiled; arithmetic stays exact when not opted in.
        let r = rel(&Prim::Cast, ConstraintFamilyV1::ExactArithmetic, CheckerIdV1::ExactRecompute, &[(DType::I8, 4)], (DType::I32, 4));
        assert!(matches!(compile_relation_v1(0, &Prim::Cast, &r, &p), Ok(CompactCoverageV1::ExactRetained { .. })));
        let r = rel(&Prim::Mul, ConstraintFamilyV1::ExactArithmetic, CheckerIdV1::ExactRecompute, &[(DType::I8, 4), (DType::I8, 4)], (DType::I32, 4));
        assert!(matches!(compile_relation_v1(0, &Prim::Mul, &r, &params(false)), Ok(CompactCoverageV1::ExactRetained { .. })));
        // Typed spec targets and an undeclared grinding budget are refused.
        let program = misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program;
        let d = crate::descriptor::k2_tir_v2_descriptor();
        let plan = crate::plan::plan_for_tir_program_v1(&d, &program, [7; 64], 4).unwrap();
        assert!(matches!(compile_plan_v1(PaidClaimTargetV2::TypedSpec, &program, &plan, &p), Err(CompactRefusalV1::Target(_))));
        let undeclared = CompactCompileParamsV1 { fs_query_bits: 0, ..p };
        assert!(matches!(compile_plan_v1(PaidClaimTargetV2::Program, &program, &plan, &undeclared), Err(CompactRefusalV1::Params(_))));
        assert_eq!(COMPACT_REFUSED_FAMILIES_V1.len(), 7);
        assert!(COMPACT_REFUSED_FAMILIES_V1.iter().any(|(name, _)| *name == "gkr-layered-chain"));
    }

    #[test]
    fn a_shipped_class_plan_compiles_with_every_relation_covered_and_registration_stays_unreviewed() {
        let program = misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program;
        let d = crate::descriptor::k2_tir_v2_descriptor();
        let plan = crate::plan::plan_for_tir_program_v1(&d, &program, [7; 64], 4).unwrap();
        for on in [false, true] {
            let coverage = compile_plan_v1(PaidClaimTargetV2::Program, &program, &plan, &params(on)).unwrap();
            assert_eq!(coverage.entries.len(), plan.relations.len(), "every relation is compiled or exactly retained");
            let dense = plan.relations.iter().filter(|r| r.checker.is_probabilistic()).count();
            assert!(dense > 0 && coverage.compact().count() >= dense);
            assert!(coverage.proof_bytes_per_position() < coverage.material_bytes_per_position());
            let contract = CompactRelationContractV1 {
                version: 1,
                target: PaidClaimTargetV2::Program,
                coverage_root: coverage.root(),
                claimed_soundness_bits: 1,
                binding_bits: 256,
                maximum_proof_bytes_per_position: u64::MAX,
                maximum_verifier_work_per_position: u128::MAX,
            };
            assert!(contract.check(&coverage, &AdversaryBudgetV1::NONE).is_ok());
            let policy = misaka_palw_challenge::reference_policy_v1();
            let refused = register_compact_relation_contract_v1(
                &contract,
                &coverage,
                &AdversaryBudgetV1::NONE,
                &misaka_palw_challenge::shipped_registry_v2(),
                &policy,
            );
            assert!(refused.unwrap_err().contains("not an approved review tuple"), "the shipped registry is empty");
            let overclaim = CompactRelationContractV1 { claimed_soundness_bits: u16::MAX, ..contract.clone() };
            assert!(overclaim.check(&coverage, &AdversaryBudgetV1::NONE).is_err());
            let unbound = CompactRelationContractV1 { coverage_root: [1; 64], ..contract };
            assert!(unbound.check(&coverage, &AdversaryBudgetV1::NONE).is_err());
        }
    }

    #[test]
    fn compiler_identities_are_distinct_and_the_v3_round_plan_refuses_them_by_name() {
        use crate::claim_interactive_v3::{CompactProofFamilyV1, CompiledClaimProofPlanV3, compact_proof_family_compiler_v1, full_material_compiler_v3};
        let mut ids = std::collections::BTreeSet::new();
        for c in CompactRelationCompilerV1::ALL {
            assert!(ids.insert(compact_relation_compiler_id_v1(c)));
        }
        assert!(!ids.contains(&full_material_compiler_v3()));
        for f in CompactProofFamilyV1::ALL {
            assert!(!ids.contains(&compact_proof_family_compiler_v1(f)));
        }
        let mut plan = CompiledClaimProofPlanV3::compile(
            PaidClaimTargetV2::Program,
            misaka_palw_challenge::InteractiveModeV1::StagedBeacon,
            2,
            4096,
            100,
        )
        .unwrap();
        plan.compiler = compact_relation_compiler_id_v1(CompactRelationCompilerV1::SumCheckHadamardV1);
        assert_eq!(
            plan.validate(PaidClaimTargetV2::Program, misaka_palw_challenge::InteractiveModeV1::StagedBeacon, 2, 100),
            Err(COMPACT_RELATION_NOT_A_V3_PLAN_V1.to_string())
        );
    }
}
