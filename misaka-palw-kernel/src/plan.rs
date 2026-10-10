//! **`VerificationPlanV1` — data in a bounded, typed grammar** (ADR-0172 §2, `docs/design/palw/versioned-kernels.md` §K.2–K.4).
//!
//! A plan names, for every node of the program, the implemented relation it is checked by: the
//! primitive, its family, the descriptor's checker, the repetitions, and the dimensions that price it;
//! plus one boundary per state (initialization and continuity across positions) and the budgets the
//! whole claim costs at its worst position count. It is canonical borsh; its root is what a class binds.
//!
//! A plan is **untrusted input**. [`plan_for_tir_program_v1`] is the reference frontend (what an
//! off-chain builder runs); [`crate::check::check_plan_v1`] re-derives everything from the program and
//! the descriptor and refuses any difference — a plan cannot omit a node, pick a weaker checker, lower
//! a repetition, understate a budget or declare more confidence than the suite derives.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program::{Ref, StateKind, TirProgramV1};
use misaka_palw_tir::{DType, Prim, TensorType};

use crate::descriptor::KernelDescriptorV1;
use crate::family::{CheckerIdV1, ConstraintFamilyV1, CourtIdV1, family_of_prim};
use crate::hash::{Digest, object_id};

pub const PLAN_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/plan/v1";
pub const PLAN_GRAMMAR_V1: u16 = 1;

/// `[batch…, M, K] × [batch…, K, N] → [batch…, M, N]`, the batch flattened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MatMulDimsV1 {
    pub batch: u64,
    pub m: u64,
    pub k: u64,
    pub n: u64,
}

/// One relation: one node of one block, checked at every occurrence of the block at every position.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PlanRelationV1 {
    pub block: u8,
    pub node: u16,
    pub prim_tag: u8,
    pub family: ConstraintFamilyV1,
    pub checker: CheckerIdV1,
    pub court: CourtIdV1,
    /// Repetitions of a probabilistic check (0 for an exact one).
    pub repetitions: u8,
    /// How many times the block runs per position.
    pub occurrences: u32,
    pub out_dtype: u8,
    /// Elements of the output and of each input at the worst history length.
    pub out_elements: u64,
    pub in_elements: Vec<u64>,
    pub in_dtypes: Vec<u8>,
    pub matmul: Option<MatMulDimsV1>,
}

/// One state's boundary relation: zero initialization and continuity by wiring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StateBoundaryV1 {
    pub state: u16,
    /// 0 = Fixed, 1 = Hist.
    pub kind: u8,
    pub per_layer: bool,
    /// The Hist window (0 for Fixed).
    pub window: u32,
}

/// The costs the plan declares and the checker re-derives (they must agree exactly).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PlanBudgetsV1 {
    /// Verifier operations for one position (reading every opened value, every field op, every exact recompute).
    pub verifier_work_per_position: u128,
    /// Committed node-value bytes the verifier opens for one position.
    pub evidence_bytes_per_position: u128,
    /// The artifact bytes the verifier reads once per claim (the params the relations read).
    pub artifact_bytes: u128,
    /// Probabilistic relation instances in one position (each counted once per repetition in the error).
    pub probabilistic_instances_per_position: u64,
    /// The worst single fault proof.
    pub worst_court_bytes: u64,
    pub worst_court_work: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VerificationPlanV1 {
    pub grammar: u16,
    pub descriptor_digest: Digest,
    pub program_root: Digest,
    pub max_positions: u32,
    pub relations: Vec<PlanRelationV1>,
    pub boundaries: Vec<StateBoundaryV1>,
    /// The whole-claim error the registrant states, in bits. The checker derives its own and refuses
    /// a declaration above it.
    pub declared_error_bits: u16,
    pub budgets: PlanBudgetsV1,
}

impl VerificationPlanV1 {
    pub fn root(&self) -> Digest {
        object_id(PLAN_ROOT_DOMAIN_V1, self)
    }

    pub fn encoded_len(&self) -> usize {
        borsh::to_vec(self).map(|v| v.len()).unwrap_or(usize::MAX)
    }
}

/// The type a node input refers to.
pub(crate) fn ref_type(program: &TirProgramV1, block: usize, r: &Ref) -> TensorType {
    let b = &program.blocks[block];
    match *r {
        Ref::Node(j) => b.nodes[j as usize].out.clone(),
        Ref::CarryIn(k) => b.carry_in[k as usize].clone(),
        Ref::Param(j) => TensorType::fixed(program.params[j as usize].dtype, &program.params[j as usize].shape),
        Ref::Const(j) => TensorType::fixed(program.consts[j as usize].dtype, &program.consts[j as usize].shape),
        Ref::State(j) => TensorType::fixed(program.states[j as usize].dtype, &program.states[j as usize].shape),
        Ref::Input(_) => TensorType::fixed(DType::Idx, &[]),
    }
}

/// The largest `H` a block sees: its Hist window, or 1.
pub(crate) fn worst_h(program: &TirProgramV1, block: usize) -> usize {
    let mut w = 1usize;
    for node in &program.blocks[block].nodes {
        if let Prim::HistAppend { state } = node.prim
            && let StateKind::Hist { window } = program.states[state as usize].kind
        {
            w = w.max(window as usize);
        }
    }
    w
}

fn elements(t: &TensorType, h: usize) -> u64 {
    t.resolve(h).iter().map(|d| *d as u64).product()
}

pub(crate) fn dtype_of_tag(tag: u8) -> Option<DType> {
    DType::ALL.into_iter().find(|d| d.tag() == tag)
}

/// Per-element cost factor of an exact recompute.
fn exact_factor(family: ConstraintFamilyV1) -> u128 {
    match family {
        ConstraintFamilyV1::Nonlinear => 64,
        _ => 1,
    }
}

/// The relation the reference frontend writes for `(block, node)` — and the one the checker expects.
pub(crate) fn expected_relation(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    block: usize,
    node: usize,
    occurrences: u32,
) -> Result<PlanRelationV1, (ConstraintFamilyV1, String)> {
    let n = &program.blocks[block].nodes[node];
    let family = family_of_prim(&n.prim);
    let support = descriptor.support(family).ok_or_else(|| (family, format!("no {} checker in this kernel", family.name())))?;
    let h = worst_h(program, block);
    let in_types: Vec<TensorType> = n.inputs.iter().map(|r| ref_type(program, block, r)).collect();
    let matmul = if matches!(n.prim, Prim::MatMul) {
        let out = n.out.resolve(h);
        let a = in_types[0].resolve(h);
        let r = out.len();
        if r < 2 || a.len() < 2 {
            return Err((family, "a MatMul of rank below 2".into()));
        }
        Some(MatMulDimsV1 {
            batch: out[..r - 2].iter().map(|d| *d as u64).product(),
            m: out[r - 2] as u64,
            k: a[a.len() - 1] as u64,
            n: out[r - 1] as u64,
        })
    } else {
        None
    };
    Ok(PlanRelationV1 {
        block: block as u8,
        node: node as u16,
        prim_tag: n.prim.tag(),
        family,
        checker: support.checker,
        court: support.court,
        repetitions: if support.checker.is_probabilistic() { descriptor.soundness.repetitions } else { 0 },
        occurrences,
        out_dtype: n.out.dtype.tag(),
        out_elements: elements(&n.out, h),
        in_elements: in_types.iter().map(|t| elements(t, h)).collect(),
        in_dtypes: in_types.iter().map(|t| t.dtype.tag()).collect(),
        matmul,
    })
}

/// The boundaries of every state, in state order.
pub(crate) fn expected_boundaries(program: &TirProgramV1) -> Vec<StateBoundaryV1> {
    program
        .states
        .iter()
        .enumerate()
        .map(|(j, s)| {
            let (kind, window) = match s.kind {
                StateKind::Fixed { .. } => (0, 0),
                StateKind::Hist { window } => (1, window),
            };
            StateBoundaryV1 { state: j as u16, kind, per_layer: s.per_layer, window }
        })
        .collect()
}

/// How many times each block runs per position.
pub(crate) fn block_occurrences(program: &TirProgramV1) -> Vec<u32> {
    let mut occ = vec![0u32; program.blocks.len()];
    for (b, _) in program.occurrences() {
        occ[b as usize] += 1;
    }
    occ
}

fn width(tag: u8) -> u128 {
    dtype_of_tag(tag).map(|d| d.width() as u128).unwrap_or(16)
}

fn max_abs(d: DType) -> u128 {
    (d.min_value().unsigned_abs()).max(d.max_value().unsigned_abs())
}

/// `⌈log2 x⌉` for `x ≥ 1`.
fn log2_up(x: u128) -> u32 {
    if x <= 1 { 0 } else { 128 - (x - 1).leading_zeros() }
}

/// The largest integer error a `MatMul` can make, exactly, when it fits a `u128`:
/// `|Y_claim| + |Y_true| ≤ max|out| + k·max|a|·max|b|`.
pub(crate) fn matmul_error_span(k: u64, a: DType, b: DType, out: DType) -> Option<u128> {
    (k as u128).checked_mul(max_abs(a))?.checked_mul(max_abs(b))?.checked_add(max_abs(out))
}

/// An `s` with `span < 2^s`, for spans past `u128` (`span ≤ 2^(max(⌈log2 k⌉ + ⌈log2 |a|⌉ + ⌈log2 |b|⌉, ⌈log2 |out|⌉) + 1)`).
fn matmul_error_span_bits(k: u64, a: DType, b: DType, out: DType) -> u32 {
    let prod = log2_up(k as u128) + log2_up(max_abs(a)) + log2_up(max_abs(b));
    prod.max(log2_up(max_abs(out))) + 2
}

/// **How many moduli of [`crate::field::MODULI_V2`] the multi-modulus relation needs** for a `MatMul` of these operand types:
/// the fewest, largest first, whose product exceeds the integer error span. `None`: beyond all three (still an extension).
/// The product of `2^e_1 − 1, …, 2^e_j − 1` exceeds `2^(Σe − 1)`, so a span below `2^(Σe − 1)` fits.
pub fn dense_moduli_v2(k: u64, a: DType, b: DType, out: DType) -> Option<usize> {
    if matmul_error_span(k, a, b, out).is_some_and(|s| s < crate::field::P) {
        return Some(1);
    }
    let bits = matmul_error_span_bits(k, a, b, out);
    let mut sum = 0u32;
    for (j, e) in crate::field::MODULI_V2.iter().enumerate() {
        sum += e;
        if j >= 1 && bits < sum {
            return Some(j + 1);
        }
    }
    None
}

/// The moduli a relation's dense check uses: 1 for `FreivaldsM127`, the derived count for `FreivaldsCrtV2`.
pub fn relation_moduli(rel: &PlanRelationV1) -> Option<usize> {
    match (rel.checker, rel.matmul) {
        (CheckerIdV1::FreivaldsM127, Some(_)) => Some(1),
        (CheckerIdV1::FreivaldsCrtV2, Some(d)) => {
            let dt = |i: usize| rel.in_dtypes.get(i).and_then(|t| dtype_of_tag(*t));
            dense_moduli_v2(d.k, dt(0)?, dt(1)?, dtype_of_tag(rel.out_dtype)?)
        }
        _ => None,
    }
}

/// One relation instance's verifier work and opened node bytes, and its worst court.
pub(crate) fn relation_costs(rel: &PlanRelationV1, window_rows: u64, row_bytes: u128) -> (u128, u128, u64, u64) {
    let out_bytes = rel.out_elements as u128 * width(rel.out_dtype);
    let in_bytes: u128 = rel.in_elements.iter().zip(&rel.in_dtypes).map(|(e, t)| *e as u128 * width(*t)).sum();
    let reads: u128 = rel.out_elements as u128 + rel.in_elements.iter().map(|e| *e as u128).sum::<u128>();
    let (work, court_bytes, court_work) = match (rel.checker, rel.matmul) {
        (CheckerIdV1::FreivaldsM127 | CheckerIdV1::FreivaldsCrtV2, Some(d)) => {
            // One pass per modulus (an unexpressible span is refused by the checker before pricing matters).
            let moduli = relation_moduli(rel).unwrap_or(crate::field::MODULI_V2.len()) as u128;
            let per_rep =
                moduli * d.batch as u128 * (d.k as u128 * d.n as u128 + d.m as u128 * d.k as u128 + d.m as u128 * d.n as u128);
            // The court opens one Merkle row of X (k elements), one column of W (k), one row of Y (n), each with its path and the
            // other root, and computes one dot product: O(k + n) bytes whatever the product's size.
            let wd = |i: usize| rel.in_dtypes.get(i).map(|t| width(*t)).unwrap_or(16);
            let (rows, cols) = (d.batch.saturating_mul(d.m), d.batch.saturating_mul(d.n));
            let path = |leaves: u64| (crate::merkle::depth(leaves) as u128 + 1) * 64 + 96;
            let court_bytes = d.k as u128 * (wd(0) + wd(1)) + d.n as u128 * width(rel.out_dtype) + 2 * path(rows) + path(cols);
            let court_work = d.k + 3 * (crate::merkle::depth(rows.max(cols)) + 1);
            (reads + rel.repetitions as u128 * per_rep, court_bytes, court_work)
        }
        (CheckerIdV1::StateContinuity, _) => {
            // The window's prior rows are opened from earlier positions' committed rows.
            let prior = window_rows as u128 * row_bytes;
            (reads + window_rows as u128, in_bytes + out_bytes + prior, reads as u64 + window_rows)
        }
        _ => {
            let w = reads * exact_factor(rel.family);
            (w, in_bytes + out_bytes, w as u64)
        }
    };
    (work, out_bytes, court_bytes.min(u64::MAX as u128) as u64, court_work)
}

/// Exact re-execution and the commitment-mismatch court. This court computes the whole output commitment: price authenticated
/// operands, history, hashing and multiplication. The caller must re-derive the plan from the validated program first.
pub fn reexecution_bounds_v1(program: &TirProgramV1, plan: &VerificationPlanV1) -> (u128, u64, u64) {
    let mut total = 0u128;
    let mut bytes = 0u128;
    let mut court = 0u128;
    for r in &plan.relations {
        let node = &program.blocks[r.block as usize].nodes[r.node as usize];
        let (prior_elements, prior_bytes, prior_rows) = match node.prim {
            Prim::HistAppend { state } => {
                let st = &program.states[state as usize];
                let rows = match st.kind {
                    StateKind::Hist { window } => (window as u128).min(plan.max_positions as u128).saturating_sub(1),
                    _ => 0,
                };
                let elems = st.shape.iter().fold(1u128, |n, d| n.saturating_mul(*d as u128));
                (rows.saturating_mul(elems), rows.saturating_mul(elems).saturating_mul(st.dtype.width() as u128), rows)
            }
            _ => (0, 0, 0),
        };
        let reads =
            r.in_elements.iter().fold(r.out_elements as u128, |n, e| n.saturating_add(*e as u128)).saturating_add(prior_elements);
        let multiply = r.matmul.map_or(0, |d| {
            (d.batch as u128).saturating_mul(d.m as u128).saturating_mul(d.k as u128).saturating_mul(d.n as u128).saturating_mul(2)
        });
        // Include hashing, nonlinear semantics and checking/copying operands, not only the dot product.
        let work = reads.saturating_mul(64).saturating_add(multiply).saturating_add(128);
        let wire = r
            .in_elements
            .iter()
            .zip(&r.in_dtypes)
            .fold(prior_bytes, |n, (e, t)| n.saturating_add((*e as u128).saturating_mul(width(*t))))
            .saturating_add((r.out_elements as u128).saturating_mul(width(r.out_dtype)))
            .saturating_add((prior_rows + r.in_elements.len() as u128 + 1).saturating_mul(128));
        // The localizer authenticates computed commitments and independently checks a fault before returning it.
        total = total
            .saturating_add(work.saturating_mul(r.occurrences as u128).saturating_mul(plan.max_positions as u128).saturating_mul(2));
        bytes = bytes.max(wire);
        court = court.max(work);
    }
    // Each public court currently rebuilds and authenticates the complete public record, not only its local operands.
    // Include inline commitments, evidence/segment headers, artifact commitment metadata and program/plan validation.
    let nodes =
        program.occurrences().iter().fold(0u128, |n, (b, _)| n.saturating_add(program.blocks[*b as usize].nodes.len() as u128));
    let instances = program
        .params
        .iter()
        .fold(0u128, |n, p| n.saturating_add(if p.per_layer { program.schedule.layers.len() as u128 } else { 1 }));
    let record = nodes
        .saturating_mul(64)
        .saturating_add(160 + 4 * program.occurrences().len() as u128)
        .saturating_mul(plan.max_positions as u128)
        .saturating_add(instances.saturating_mul(128))
        .saturating_add(program.encode().len() as u128)
        .saturating_add(plan.encoded_len() as u128)
        .saturating_add(16 * 1024);
    let structural_work = record.saturating_mul(16);
    (
        total.saturating_add(structural_work.saturating_mul(4)),
        bytes.min(u64::MAX as u128) as u64,
        court.saturating_add(structural_work).min(u64::MAX as u128) as u64,
    )
}

/// The whole-claim error a suite derives (bits): `t·b − ⌈log2 instances⌉` (`b` the descriptor's per-repetition bits), then the binding
/// term by a union bound (`min − 1`). Exact relations contribute nothing; `None` when no probabilistic
/// relation exists (the bound is then the binding term alone).
pub fn derived_error_bits(descriptor: &KernelDescriptorV1, probabilistic_instances: u128) -> u16 {
    let binding = descriptor.soundness.binding_bits as i64;
    if probabilistic_instances == 0 {
        return binding.clamp(0, u16::MAX as i64) as u16;
    }
    let log2_up = 128 - (probabilistic_instances - 1).leading_zeros() as i64; // ⌈log2 R⌉ for R ≥ 1
    let check = descriptor.soundness.repetitions as i64 * descriptor.per_repetition_bits() as i64 - log2_up;
    (check.min(binding) - 1).clamp(0, u16::MAX as i64) as u16
}

/// **The reference frontend**: the plan an honest builder writes for `program` under `descriptor`.
/// `Err` names the first node whose family the descriptor does not implement.
pub fn plan_for_tir_program_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    program_root: Digest,
    max_positions: u32,
) -> Result<VerificationPlanV1, (ConstraintFamilyV1, String)> {
    let occ = block_occurrences(program);
    let mut relations = Vec::new();
    for (b, block) in program.blocks.iter().enumerate() {
        if occ[b] == 0 {
            continue;
        }
        for ni in 0..block.nodes.len() {
            relations.push(expected_relation(descriptor, program, b, ni, occ[b])?);
        }
    }
    let boundaries = expected_boundaries(program);
    let budgets = derive_budgets(program, &relations);
    let instances = budgets.probabilistic_instances_per_position as u128 * max_positions as u128;
    Ok(VerificationPlanV1 {
        grammar: PLAN_GRAMMAR_V1,
        descriptor_digest: descriptor.digest(),
        program_root,
        max_positions,
        relations,
        boundaries,
        declared_error_bits: derived_error_bits(descriptor, instances),
        budgets,
    })
}

pub(crate) fn derive_budgets(program: &TirProgramV1, relations: &[PlanRelationV1]) -> PlanBudgetsV1 {
    let mut b = PlanBudgetsV1 {
        artifact_bytes: program
            .params
            .iter()
            .map(|p| {
                let instances = if p.per_layer { program.schedule.layers.len() as u128 } else { 1 };
                p.shape.iter().map(|d| *d as u128).product::<u128>() * p.dtype.width() as u128 * instances
            })
            .sum(),
        ..Default::default()
    };
    // Derived values (a `Hist` window and its views) are rebuilt from committed rows, never served: they are no one's public bytes.
    let derived = crate::trace::derived_nodes_v1(program);
    for rel in relations {
        let (window_rows, row_bytes) = match program.blocks[rel.block as usize].nodes[rel.node as usize].prim {
            Prim::HistAppend { state } => {
                let s = &program.states[state as usize];
                let w = match s.kind {
                    StateKind::Hist { window } => window as u64,
                    StateKind::Fixed { .. } => 0,
                };
                (w.saturating_sub(1), s.shape.iter().map(|d| *d as u128).product::<u128>() * s.dtype.width() as u128)
            }
            _ => (0, 0),
        };
        let (work, bytes, cb, cw) = relation_costs(rel, window_rows, row_bytes);
        b.verifier_work_per_position += work * rel.occurrences as u128;
        if !derived.contains(&(rel.block, rel.node)) {
            b.evidence_bytes_per_position += bytes * rel.occurrences as u128;
        }
        if rel.checker.is_probabilistic() {
            b.probabilistic_instances_per_position += rel.occurrences as u64 * rel.matmul.map(|d| d.batch).unwrap_or(1);
        }
        b.worst_court_bytes = b.worst_court_bytes.max(cb);
        b.worst_court_work = b.worst_court_work.max(cw);
    }
    b
}
