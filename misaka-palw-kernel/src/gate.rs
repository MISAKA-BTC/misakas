//! **`PUBLIC_PROSECUTION_COMPLETE(plan, profile)` — derived from code, never declared.**
//!
//! A plan may be Active and a profile rewardable only if every relation the plan covers has a path from public material to an
//! objective outcome for an ordinary bond outside the Panel (ADR-0173; RFC-0015 §1.1 items 3–5): an implemented checker, an
//! implemented terminal court that reads only public authenticated material, a DA demand/default path for every committed
//! value it reads, and finite bounds on what the prosecution costs. [`public_prosecution_complete_v1`] derives this from the
//! descriptor, the plan, the profile's material and the network's prosecution policy, and returns the bounds — or every gap.
//!
//! The bounds are the code-side half of "no hidden unbounded fallback": public bytes a fresh verifier fetches, the bytes one
//! court opens, the on-chain rounds a prosecution needs, court work, verifier working memory, per-claim retained state,
//! concurrent sessions per claim and the deadline. Measuring them on real hardware is an external gate; here they are finite or
//! the gate fails.

use crate::descriptor::KernelDescriptorV1;
use crate::family::{CheckerIdV1, CourtIdV1};
use crate::plan::VerificationPlanV1;
use crate::public::ProfileMaterialV1;
use misaka_palw_tir::program::{StateKind, TirProgramV1};

/// The network's prosecution policy (one for every class on the route).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ProsecutionPolicyV1 {
    /// A demand's response window, and a filed proof's inclusion bound.
    pub court_deadline_daa: u64,
    /// The ceiling on concurrent demand sessions per claim. A demand names one position (every committed value of it), so a
    /// plan needs `max_positions` sessions: every position demandable at once, and nobody's demands can starve anybody's.
    /// Direct proofs are never limited: they settle in the block that carries them.
    pub max_sessions_per_claim: u32,
    /// Ceilings the derived bounds must fit.
    pub max_public_bytes: u128,
    pub max_verifier_ram: u128,
    pub max_retained_state: u128,
}

/// What one public prosecution of a claim of this plan can cost, at most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ProsecutionBoundsV1 {
    /// Everything a fresh verifier fetches: committed node values, the artifact, the commitments and the evidence object.
    pub max_public_bytes: u128,
    /// The largest single court opening (a filed proof's payload).
    pub max_opening_bytes: u64,
    /// The envelope a filed proof's bytes must fit (the court's bytes plus the wire headers).
    pub max_filing_bytes: u64,
    /// The envelope one demand's response must fit: one position's committed values, with their wire headers.
    pub max_response_bytes: u128,
    /// The commitment/evidence carrier, independently of responses and prosecution metadata retained later.
    pub max_commit_bytes: u128,
    /// On-chain rounds: one round of demands (every missing position at once, each answered or defaulted by its deadline) and
    /// one direct proof.
    pub max_localization_rounds: u32,
    /// The largest single court's work.
    pub max_court_work: u64,
    /// Conservative live memory of the current i128 Tensor verifier, including whole-scope caches and temporaries.
    pub max_verifier_ram: u128,
    /// Kernel claim rows, all served responses and bounded demand/proof metadata (not a global chain storage bound).
    pub max_retained_state: u128,
    /// One demand session per position.
    pub max_concurrent_sessions: u32,
    pub deadline_daa: u64,
}

/// One reason a plan/profile is not publicly prosecutable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProsecutionGapV1 {
    /// A relation's family has no checker/court in the descriptor.
    NoCourt { block: u8, node: u16 },
    /// A relation's court reads material a public bond cannot obtain, or is not one this code implements.
    PrivateCourt { block: u8, node: u16, court: CourtIdV1 },
    /// A checker this code does not implement.
    UnknownChecker { block: u8, node: u16 },
    /// The profile needs private weights/input/state, a FOLD prefix or a fused preimage (held/fused classes).
    PrivateMaterial,
    /// A bound is infinite or past its ceiling.
    Unbounded { what: &'static str, required: u128, limit: u128 },
    /// The plan names another descriptor.
    WrongDescriptor,
    /// The supplied program is invalid or its root/budgets do not match the checked plan.
    WrongProgram,
    /// A pipeline edge without the edge court (its checker or court is not `EdgeRecompute`).
    NoEdgeCourt { stage: u8, input: u16 },
    /// A pipeline stage's own gaps.
    Stage { stage: u8, gaps: Vec<ProsecutionGapV1> },
}

/// A tensor's wire header (dtype, shape at rank ≤ 12, lengths, the response variant), an upper bound.
pub const WIRE_HEADER_BYTES_V1: u64 = 128;
/// A filing's fixed allowance beyond twice the court's bytes (position, node, kind, scalar, every tensor's header).
pub const FILING_HEADER_BYTES_V1: u64 = 64 * 1024;

/// Collateral participants in one shared position demand. An excess join cannot cancel or restart that public demand, nor
/// prevent its response/default or a direct proof. These constants bound metadata, not the number of public proof filers.
pub const MAX_DEMANDERS_PER_SESSION_V1: usize = 64;
/// A seal controls bounty attribution, not court admission: a direct proof remains admissible when this table is full.
pub const MAX_PROOF_SEALS_PER_CLAIM_V1: usize = 64;
/// Per demand: key/row headers and one bounded participant's collateral, settlement and requester scope metadata.
const DEMAND_PARTICIPANT_BYTES_V1: u128 = 512;
const CLAIM_ROW_OVERHEAD_BYTES_V1: u128 = 16 * 1024;

/// All positions can be served and retained at once. Shared progress must not hide an unbounded participant Vec.
pub(crate) fn retained_state_bound_v1(commit: u128, response: u128, sessions: u32) -> u128 {
    commit
        .saturating_add(
            (sessions as u128).saturating_mul(
                response
                    .saturating_add(WIRE_HEADER_BYTES_V1 as u128)
                    .saturating_add(DEMAND_PARTICIPANT_BYTES_V1.saturating_mul(MAX_DEMANDERS_PER_SESSION_V1 as u128)),
            ),
        )
        .saturating_add((MAX_PROOF_SEALS_PER_CLAIM_V1 as u128).saturating_mul(256))
        .saturating_add(CLAIM_ROW_OVERHEAD_BYTES_V1)
}

/// Current Tensor values use i128 regardless of wire dtype. Count every node/operand at the largest reachable history, over
/// the whole scope, so derived caches and a material store are covered. The copy allowance covers resident material, decoded
/// operands, cache clones, exact evaluation and field projections; this is deliberately conservative, not a streaming estimate.
fn verifier_ram_bound_v1(program: &TirProgramV1, plan: &VerificationPlanV1, commit: u128, response: u128) -> u128 {
    const TENSOR_OVERHEAD: u128 = 512; // rank <= 12, Vec headers, map entries and allocator headroom
    let tensor = |elements: u128| elements.saturating_mul(16).saturating_add(TENSOR_OVERHEAD);
    let params = program.params.iter().fold(0u128, |sum, p| {
        let instances = if p.per_layer { program.schedule.layers.len() as u128 } else { 1 };
        let elements = p.shape.iter().fold(1u128, |n, d| n.saturating_mul(*d as u128));
        sum.saturating_add(tensor(elements).saturating_mul(instances))
    });
    let mut position = 0u128;
    for (block, _) in program.occurrences() {
        let bi = block as usize;
        let h = crate::plan::worst_h(program, bi).min(plan.max_positions as usize).max(1);
        let elements = |t: &misaka_palw_tir::TensorType| t.resolve(h).iter().fold(1u128, |n, d| n.saturating_mul(*d as u128));
        for node in &program.blocks[bi].nodes {
            position = position.saturating_add(tensor(elements(&node.out)));
            for input in &node.inputs {
                position = position.saturating_add(tensor(elements(&crate::plan::ref_type(program, bi, input))));
            }
            if let misaka_palw_tir::Prim::HistAppend { state } = node.prim {
                let s = &program.states[state as usize];
                let rows = match s.kind {
                    StateKind::Hist { window } => (window as u128).min(plan.max_positions as u128).saturating_sub(1),
                    StateKind::Fixed { .. } => 0,
                };
                let row = s.shape.iter().fold(1u128, |n, d| n.saturating_mul(*d as u128));
                position = position.saturating_add(tensor(row).saturating_mul(rows));
            }
        }
    }
    // Scope-wide batched projections persist once per occurrence, repetition and modulus. They are not one-position evidence.
    let projections = plan.relations.iter().filter(|r| r.checker.is_probabilistic()).fold(0u128, |sum, r| {
        let Some(d) = r.matmul else { return sum };
        let elements = (d.m as u128).saturating_add(d.k as u128).saturating_add(d.n as u128);
        sum.saturating_add(
            tensor(elements)
                .saturating_mul(r.occurrences as u128)
                .saturating_mul(r.repetitions as u128)
                .saturating_mul(crate::field::MODULI_V2.len() as u128),
        )
    });
    params
        .saturating_mul(4)
        .saturating_add(position.saturating_mul(plan.max_positions as u128).saturating_mul(8))
        .saturating_add(commit.saturating_mul(8))
        // Authenticate/check a response only after decoding it: an adversary can commit a wrong dtype or shape. Price the
        // wire ceiling separately at the narrowest dtype, including simultaneous encoded/decoded buffers and Vec metadata.
        .saturating_add(response.saturating_mul(plan.max_positions as u128).saturating_mul(64))
        .saturating_add(projections.saturating_mul(4))
        .saturating_add((program.encode().len() as u128).saturating_add(plan.encoded_len() as u128).saturating_mul(16))
}

/// Courts this code implements, every one over public authenticated material.
fn public_court(c: CourtIdV1) -> bool {
    matches!(c, CourtIdV1::InstanceRecompute | CourtIdV1::MatMulScalar | CourtIdV1::EdgeRecompute | CourtIdV1::ElementRecompute)
}

fn known_checker(c: CheckerIdV1) -> bool {
    matches!(
        c,
        CheckerIdV1::ExactRecompute
            | CheckerIdV1::FreivaldsM127
            | CheckerIdV1::StateContinuity
            | CheckerIdV1::FreivaldsCrtV2
            | CheckerIdV1::EdgeRecompute
    )
}

/// **The gate.** The validated program supplies the decoded memory layout and the commitments' count; wire bytes alone do not.
pub fn public_prosecution_complete_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, Vec<ProsecutionGapV1>> {
    if misaka_palw_tir::validate::validate(program).is_err() {
        return Err(vec![ProsecutionGapV1::WrongProgram]);
    }
    prosecution_bounds_for_view_v1(descriptor, program, crate::public::program_root_v1(&program.encode()), plan, material, policy)
}

/// A pipeline's v1 cost view binds its original v2 bytes, not the synthetic view's encoding. Its caller validates the v2 program.
fn prosecution_bounds_for_view_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    program_root: crate::hash::Digest,
    plan: &VerificationPlanV1,
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, Vec<ProsecutionGapV1>> {
    use ProsecutionGapV1 as G;
    let mut gaps = Vec::new();
    if plan.program_root != program_root {
        return Err(vec![G::WrongProgram]);
    }
    // Never price attacker-supplied dimensions/occurrences before matching them against the validated program.
    let expected = crate::plan::plan_for_tir_program_v1(descriptor, program, plan.program_root, plan.max_positions)
        .map_err(|_| vec![G::WrongProgram])?;
    if plan.relations != expected.relations || plan.boundaries != expected.boundaries || plan.budgets != expected.budgets {
        return Err(vec![G::WrongProgram]);
    }
    if plan.descriptor_digest != descriptor.digest() {
        gaps.push(G::WrongDescriptor);
    }
    if !material.is_public() {
        gaps.push(G::PrivateMaterial);
    }
    for rel in &plan.relations {
        let (block, node) = (rel.block, rel.node);
        match descriptor.support(rel.family) {
            None => gaps.push(G::NoCourt { block, node }),
            Some(s) => {
                if !known_checker(rel.checker) || rel.checker != s.checker {
                    gaps.push(G::UnknownChecker { block, node });
                }
                if !public_court(rel.court) || rel.court != s.court {
                    gaps.push(G::PrivateCourt { block, node, court: rel.court });
                }
            }
        }
    }
    let b = &plan.budgets;
    let positions = plan.max_positions as u128;
    let occurrences = program.occurrences();
    let nodes_per_position: u64 = occurrences.iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let commitments = positions.saturating_mul(nodes_per_position as u128).saturating_mul(64);
    // Nested Vec headers, up to one 136-byte segment per position, tokens/output and fixed claim/evidence fields.
    let commit = commitments
        .saturating_add(positions.saturating_mul(4 * occurrences.len() as u128 + 160))
        .saturating_add(CLAIM_ROW_OVERHEAD_BYTES_V1);
    // A plan's relation costs cover the program's maximum window. An actual response can only reach the claim's context:
    // count non-derived outputs at min(window, max_positions), including empty-occurrence Vec headers and tensor headers.
    let derived = crate::trace::derived_nodes_v1(program);
    let mut response = (WIRE_HEADER_BYTES_V1 as u128).saturating_add(4 * occurrences.len() as u128);
    for (block, _) in &occurrences {
        let bi = *block as usize;
        let h = crate::plan::worst_h(program, bi).min(plan.max_positions as usize).max(1);
        for (ni, node) in program.blocks[bi].nodes.iter().enumerate() {
            response = response.saturating_add(WIRE_HEADER_BYTES_V1 as u128);
            if !derived.contains(&(*block, ni as u16)) {
                let elements = node.out.resolve(h).iter().fold(1u128, |n, d| n.saturating_mul(*d as u128));
                response = response.saturating_add(elements.saturating_mul(node.out.dtype.width() as u128));
            }
        }
    }
    let bounds = ProsecutionBoundsV1 {
        max_public_bytes: response.saturating_mul(positions).saturating_add(b.artifact_bytes).saturating_add(commit),
        max_opening_bytes: b.worst_court_bytes,
        max_filing_bytes: b.worst_court_bytes.saturating_mul(2).saturating_add(FILING_HEADER_BYTES_V1),
        max_response_bytes: response,
        max_commit_bytes: commit,
        max_localization_rounds: 2,
        max_court_work: b.worst_court_work,
        max_verifier_ram: verifier_ram_bound_v1(program, plan, commit, response),
        max_retained_state: retained_state_bound_v1(commit, response, plan.max_positions),
        max_concurrent_sessions: plan.max_positions,
        deadline_daa: policy.court_deadline_daa,
    };
    let l = &descriptor.limits;
    for (what, required, limit) in [
        ("public bytes", bounds.max_public_bytes, policy.max_public_bytes),
        ("opening bytes", bounds.max_opening_bytes as u128, l.max_court_bytes as u128),
        ("court work", bounds.max_court_work as u128, l.max_court_work as u128),
        ("verifier RAM", bounds.max_verifier_ram, policy.max_verifier_ram),
        ("retained state", bounds.max_retained_state, policy.max_retained_state),
        ("concurrent sessions", bounds.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128),
    ] {
        if required >= u128::MAX / 2 || required > limit {
            gaps.push(G::Unbounded { what, required, limit });
        }
    }
    if policy.court_deadline_daa == 0 || plan.max_positions == 0 {
        gaps.push(G::Unbounded { what: "deadline or sessions (zero: nobody can prosecute)", required: 1, limit: 0 });
    }
    if gaps.is_empty() { Ok(bounds) } else { Err(gaps) }
}

/// **The gate for a pipeline plan** (K2-TIR-v3): every stage's plan through [`public_prosecution_complete_v1`], every edge on the
/// public edge court, and the bounds of the whole claim — public bytes, RAM and retained state summed over stages, one demand
/// session per stage position, the largest opening, filing (an edge filing opens every upstream output) and response.
pub fn public_pipeline_prosecution_complete_v1(
    descriptor: &KernelDescriptorV1,
    plan: &crate::pipeline::PipelinePlanV1,
    pipeline: &misaka_palw_tir::pipeline::TirPipelineV1,
    programs: &[misaka_palw_tir::program_v2::TirProgramV2],
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, Vec<ProsecutionGapV1>> {
    use ProsecutionGapV1 as G;
    let mut gaps = Vec::new();
    if misaka_palw_tir::pipeline::validate_pipeline(pipeline, programs).is_err() {
        return Err(vec![G::WrongProgram]);
    }
    if plan.descriptor_digest != descriptor.digest() || plan.stages.len() != pipeline.stages.len() {
        gaps.push(G::WrongDescriptor);
    }
    // Per stage, against ceilings that never bind: the whole claim's bounds are checked against the policy below.
    let open = ProsecutionPolicyV1 {
        court_deadline_daa: policy.court_deadline_daa,
        max_sessions_per_claim: u32::MAX,
        max_public_bytes: u128::MAX / 4,
        max_verifier_ram: u128::MAX / 4,
        max_retained_state: u128::MAX / 4,
    };
    let mut total = ProsecutionBoundsV1 {
        max_public_bytes: 0,
        max_opening_bytes: 0,
        max_filing_bytes: 0,
        max_response_bytes: 0,
        max_commit_bytes: 0,
        max_localization_rounds: 2,
        max_court_work: 0,
        max_verifier_ram: 0,
        max_retained_state: 0,
        max_concurrent_sessions: 0,
        deadline_daa: policy.court_deadline_daa,
    };
    let mut upstream_bytes: Vec<u128> = Vec::new();
    for (si, (st, sp)) in pipeline.stages.iter().zip(&plan.stages).enumerate() {
        let Some(prog) = programs.get(st.program as usize) else {
            gaps.push(G::WrongDescriptor);
            continue;
        };
        let v = crate::pipeline::stage_view_v1(prog);
        match prosecution_bounds_for_view_v1(descriptor, &v.view, crate::public::program_root_v1(&prog.encode()), sp, material, &open)
        {
            Err(g) => gaps.push(G::Stage { stage: si as u8, gaps: g }),
            Ok(b) => {
                // A stage position's response carries its stage inputs too.
                let inputs: u128 = plan
                    .edges
                    .iter()
                    .filter(|e| e.stage as usize == si)
                    .map(|e| (e.elements as u128).saturating_mul(16).saturating_add(WIRE_HEADER_BYTES_V1 as u128))
                    .sum();
                total.max_public_bytes = total.max_public_bytes.saturating_add(b.max_public_bytes);
                total.max_opening_bytes = total.max_opening_bytes.max(b.max_opening_bytes);
                total.max_filing_bytes = total.max_filing_bytes.max(b.max_filing_bytes);
                total.max_response_bytes = total.max_response_bytes.max(b.max_response_bytes.saturating_add(inputs));
                total.max_court_work = total.max_court_work.max(b.max_court_work);
                total.max_verifier_ram = total.max_verifier_ram.saturating_add(b.max_verifier_ram);
                total.max_verifier_ram =
                    total.max_verifier_ram.saturating_add(inputs.saturating_mul(sp.max_positions as u128).saturating_mul(16));
                total.max_retained_state = total.max_retained_state.saturating_add(b.max_retained_state);
                total.max_commit_bytes = total.max_commit_bytes.saturating_add(b.max_commit_bytes);
                let input_commitments = (plan.edges.iter().filter(|e| e.stage as usize == si).count() as u128)
                    .saturating_mul(sp.max_positions as u128)
                    .saturating_mul(64);
                total.max_commit_bytes = total.max_commit_bytes.saturating_add(input_commitments);
                total.max_retained_state = total.max_retained_state.saturating_add(input_commitments);
                total.max_retained_state = total.max_retained_state.saturating_add(inputs.saturating_mul(sp.max_positions as u128));
                total.max_public_bytes = total.max_public_bytes.saturating_add(inputs.saturating_mul(sp.max_positions as u128));
                total.max_concurrent_sessions = total.max_concurrent_sessions.saturating_add(b.max_concurrent_sessions);
            }
        }
        // Every output value of the stage, as an edge filing opens it: at most a position's bytes per position.
        upstream_bytes.push(sp.budgets.evidence_bytes_per_position.saturating_mul(sp.max_positions as u128));
    }
    for e in &plan.edges {
        if e.checker != crate::family::CheckerIdV1::EdgeRecompute || e.court != CourtIdV1::EdgeRecompute {
            gaps.push(G::NoEdgeCourt { stage: e.stage, input: e.input });
        }
    }
    let edge_filing = upstream_bytes.iter().copied().max().unwrap_or(0).saturating_add(FILING_HEADER_BYTES_V1 as u128);
    total.max_filing_bytes = total.max_filing_bytes.max(edge_filing.min(u64::MAX as u128) as u64);
    for (what, required, limit) in [
        ("public bytes", total.max_public_bytes, policy.max_public_bytes),
        ("verifier RAM", total.max_verifier_ram, policy.max_verifier_ram),
        ("retained state", total.max_retained_state, policy.max_retained_state),
        ("concurrent sessions", total.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128),
    ] {
        if required >= u128::MAX / 2 || required > limit {
            gaps.push(G::Unbounded { what, required, limit });
        }
    }
    if gaps.is_empty() { Ok(total) } else { Err(gaps) }
}

/// **K2-TIR-v4's per-prosecution quantities** beside [`ProsecutionBoundsV1`] (`docs/design/palw/k2-real-scale.md` §6): what one position's
/// material is, how many parts it is served in, the committed values a position, and the whole claim's material — the producer's DA
/// obligation, reported and bounded by the descriptor, never what a prosecution reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegBoundsV4 {
    pub position_material_bytes: u128,
    pub parts_per_position: u32,
    pub node_count: u64,
    pub claim_material_bytes: u128,
    pub segments: u32,
}

/// A segmented filing's fixed allowance beyond the court's bytes.
pub const SEG_FILING_HEADER_BYTES_V4: u64 = 4096;
/// What a segmented claim keeps on chain beside its segment roots: the claim, the evidence object and the claim row's fields.
pub const SEG_CLAIM_FIXED_BYTES_V4: u128 = 4096;

/// **`PUBLIC_PROSECUTION_COMPLETE` for a K2-TIR-v4 plan**: the same family / checker / court / material gaps as
/// [`public_prosecution_complete_v1`], and the bounds of ONE prosecution — two positions' material and the artifact, one demand round
/// and one filing, two sessions, the worst element court — with what the chain keeps per claim (the segment roots).
pub fn public_prosecution_complete_v4(
    descriptor: &KernelDescriptorV1,
    plan: &VerificationPlanV1,
    program: &misaka_palw_tir::program::TirProgramV1,
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<(ProsecutionBoundsV1, SegBoundsV4), Vec<ProsecutionGapV1>> {
    use ProsecutionGapV1 as G;
    let mut gaps = Vec::new();
    if plan.descriptor_digest != descriptor.digest() || !crate::descriptor::is_segmented_v1(descriptor) {
        gaps.push(G::WrongDescriptor);
    }
    if !material.is_public() {
        gaps.push(G::PrivateMaterial);
    }
    for rel in &plan.relations {
        let (block, node) = (rel.block, rel.node);
        match descriptor.support(rel.family) {
            None => gaps.push(G::NoCourt { block, node }),
            Some(s) => {
                if !known_checker(rel.checker) || rel.checker != s.checker {
                    gaps.push(G::UnknownChecker { block, node });
                }
                if rel.court != CourtIdV1::ElementRecompute || rel.court != s.court {
                    gaps.push(G::PrivateCourt { block, node, court: rel.court });
                }
            }
        }
    }
    let node_count: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let last = plan.max_positions.saturating_sub(1);
    let (position_bytes, parts) = match crate::seg_da::position_material_v1(program, last) {
        Ok(v) => v,
        Err(why) => {
            gaps.push(G::Unbounded { what: "position material (the program does not lay out)", required: 1, limit: 0 });
            let _ = why;
            (u128::MAX / 4, u32::MAX)
        }
    };
    let b = &plan.budgets;
    let positions = plan.max_positions as u128;
    let segments = crate::seg::segment_count_v1(plan.max_positions);
    let node_lists = 2 * node_count as u128 * 64;
    let filing = b.worst_court_bytes.saturating_add(SEG_FILING_HEADER_BYTES_V4);
    let bounds = ProsecutionBoundsV1 {
        max_public_bytes: position_bytes
            .saturating_mul(2)
            .saturating_add(b.artifact_bytes)
            .saturating_add(node_lists)
            .saturating_add(filing as u128),
        max_opening_bytes: b.worst_court_bytes,
        max_filing_bytes: filing,
        max_response_bytes: crate::seg_da::SEG_PART_BYTES_V4 as u128,
        max_localization_rounds: 2,
        max_court_work: b.worst_court_work,
        max_verifier_ram: b.artifact_bytes.saturating_add(position_bytes.saturating_mul(2)),
        max_retained_state: (segments as u128).saturating_mul(64).saturating_add(SEG_CLAIM_FIXED_BYTES_V4),
        max_concurrent_sessions: 2,
        deadline_daa: policy.court_deadline_daa,
    };
    let seg = SegBoundsV4 {
        position_material_bytes: position_bytes,
        parts_per_position: parts,
        node_count,
        claim_material_bytes: position_bytes.saturating_mul(positions),
        segments,
    };
    let l = &descriptor.limits;
    for (what, required, limit) in [
        ("public bytes (one prosecution)", bounds.max_public_bytes, policy.max_public_bytes),
        ("opening bytes (one element court)", bounds.max_opening_bytes as u128, l.max_court_bytes as u128),
        ("court work", bounds.max_court_work as u128, l.max_court_work as u128),
        ("verifier RAM", bounds.max_verifier_ram, policy.max_verifier_ram),
        ("retained state (on chain, per claim)", bounds.max_retained_state, policy.max_retained_state),
        ("concurrent sessions (one prosecution)", bounds.max_concurrent_sessions as u128, policy.max_sessions_per_claim as u128),
        ("parts per position", parts as u128, crate::seg_da::SEG_MAX_PARTS_V4 as u128),
        ("claim material (the producer's DA obligation)", seg.claim_material_bytes, l.max_claim_evidence_bytes),
    ] {
        if required >= u128::MAX / 2 || required > limit {
            gaps.push(G::Unbounded { what, required, limit });
        }
    }
    if policy.court_deadline_daa == 0 || plan.max_positions == 0 {
        gaps.push(G::Unbounded { what: "deadline or sessions (zero: nobody can prosecute)", required: 1, limit: 0 });
    }
    if gaps.is_empty() { Ok((bounds, seg)) } else { Err(gaps) }
}

/// **The gate of a single-program class, whatever its descriptor**: [`public_prosecution_complete_v4`] for a segmented (K2-TIR-v4)
/// plan, [`public_prosecution_complete_v1`] otherwise.
pub fn class_prosecution_bounds_v1(
    descriptor: &KernelDescriptorV1,
    plan: &VerificationPlanV1,
    program: &misaka_palw_tir::program::TirProgramV1,
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, Vec<ProsecutionGapV1>> {
    if crate::descriptor::is_segmented_v1(descriptor) {
        public_prosecution_complete_v4(descriptor, plan, program, material, policy).map(|(b, _)| b)
    } else {
        public_prosecution_complete_v1(descriptor, program, plan, material, policy)
    }
}
