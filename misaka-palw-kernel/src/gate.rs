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

/// The network's prosecution policy (one for every class on the route).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProsecutionBoundsV1 {
    /// Everything a fresh verifier fetches: committed node values, the artifact, the commitments and the evidence object.
    pub max_public_bytes: u128,
    /// The largest single court opening (a filed proof's payload).
    pub max_opening_bytes: u64,
    /// The envelope a filed proof's bytes must fit (the court's bytes plus the wire headers).
    pub max_filing_bytes: u64,
    /// The envelope one demand's response must fit: one position's committed values, with their wire headers.
    pub max_response_bytes: u128,
    /// On-chain rounds: one round of demands (every missing position at once, each answered or defaulted by its deadline) and
    /// one direct proof.
    pub max_localization_rounds: u32,
    /// The largest single court's work.
    pub max_court_work: u64,
    /// A verifier's working memory: the artifact plus one position's opened values.
    pub max_verifier_ram: u128,
    /// What the chain retains per claim until its liability horizon: the commitments and the evidence object.
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
}

/// A tensor's wire header (dtype, shape at rank ≤ 12, lengths, the response variant), an upper bound.
pub const WIRE_HEADER_BYTES_V1: u64 = 128;
/// A filing's fixed allowance beyond twice the court's bytes (position, node, kind, scalar, every tensor's header).
pub const FILING_HEADER_BYTES_V1: u64 = 64 * 1024;

/// Courts this code implements, every one over public authenticated material.
fn public_court(c: CourtIdV1) -> bool {
    matches!(c, CourtIdV1::InstanceRecompute | CourtIdV1::MatMulScalar | CourtIdV1::EdgeRecompute)
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

/// **The gate.** `nodes_per_position` is the number of committed node values per position (the commitments' count).
pub fn public_prosecution_complete_v1(
    descriptor: &KernelDescriptorV1,
    plan: &VerificationPlanV1,
    nodes_per_position: u64,
    material: &ProfileMaterialV1,
    policy: &ProsecutionPolicyV1,
) -> Result<ProsecutionBoundsV1, Vec<ProsecutionGapV1>> {
    use ProsecutionGapV1 as G;
    let mut gaps = Vec::new();
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
    let commitments = positions.saturating_mul(nodes_per_position as u128).saturating_mul(64);
    let bounds = ProsecutionBoundsV1 {
        max_public_bytes: b
            .evidence_bytes_per_position
            .saturating_mul(positions)
            .saturating_add(b.artifact_bytes)
            .saturating_add(commitments),
        max_opening_bytes: b.worst_court_bytes,
        max_filing_bytes: b.worst_court_bytes.saturating_mul(2).saturating_add(FILING_HEADER_BYTES_V1),
        max_response_bytes: b
            .evidence_bytes_per_position
            .saturating_add((nodes_per_position as u128).saturating_mul(WIRE_HEADER_BYTES_V1 as u128))
            .saturating_add(WIRE_HEADER_BYTES_V1 as u128),
        max_localization_rounds: 2,
        max_court_work: b.worst_court_work,
        max_verifier_ram: b.artifact_bytes.saturating_add(b.evidence_bytes_per_position),
        max_retained_state: commitments.saturating_add(64 * (positions + 16)),
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
