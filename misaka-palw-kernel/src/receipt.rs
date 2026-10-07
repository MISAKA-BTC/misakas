//! **`PalwConstraintReceiptV1` and the coverage-aware tally** (RFC-0007 §V.6, RFC-0011 §15.3 step 4).
//!
//! A receipt is a seat's signed statement that it ran the kernel's suite over **one scope** of one claim's evidence and what it
//! found. It is not an existing `PalwSeatReceiptV3` and never reinterprets one. The fold-side checks here are deterministic and
//! structural: scheme and profile eligibility, the evidence and challenge it is bound to, the scope it attests (recomputed), the
//! derived soundness for its check count (recomputed, never trusted), the assignment and the deadline. The heavy verification is
//! the seat's ([`crate::verify::verify_scope_v1`]); a node does not redo it, so a malicious accepting Panel stays a security event
//! that the public fault proof ([`crate::verify::verify_fault_proof_v1`]) answers.
//!
//! The tally counts **per segment**: a segment is covered when `quorum` distinct operators filed a counted passing receipt whose
//! scope includes it (a `WholeClaim` receipt includes every segment). An `AuditOnly` receipt never counts. A seat's first receipt for
//! a duty is the one counted; a later receipt of that seat for the same duty is ignored, and a pass and a fail from one seat on one
//! duty is recorded as equivocation. Any counted failing receipt opens a dispute: a later pass cannot erase it.
//!
//! Signing is the node's (ML-DSA-87 under a fresh network-separated domain): this module supplies the exact message bytes and a
//! verifier trait, and links no signature library.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::descriptor::KernelDescriptorV1;
use crate::evidence::VerificationEvidenceV1;
use crate::hash::{Digest, finish, keyed, object_id};
use crate::plan::derived_error_bits;
use crate::verify::{ScopeV1, ScopeVerdictV1};

pub const CONSTRAINT_RECEIPT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/constraint-receipt/v1";
/// The ML-DSA-87 context a node signs a receipt under (distinct from every existing PALW signature context).
pub const CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/constraint-receipt/v1";

/// What the seat found.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ReceiptVerdictV1 {
    Pass,
    /// A localized fault: `(position, occurrence, node)` — the seat files the proof with the court.
    Fail {
        position: u32,
        occurrence: u16,
        node: u16,
    },
    /// Material was not served or not the committed value: the DA path, never a pass.
    Unavailable,
}

/// RFC-0007 §V.6's schema (conceptual there; this crate's spelling, no wire tag allocated).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintReceiptV1 {
    pub network_domain: Digest,
    pub claim_id: Digest,
    pub class_id: Digest,
    /// The plan root (the verification profile the class binds).
    pub verification_profile_hash: Digest,
    pub verification_scheme_id: u32,
    pub scheme_version: u16,
    pub descriptor_digest: Digest,
    pub assignment_root: Digest,
    pub scope: ScopeV1,
    pub scope_root: Digest,
    pub covered_positions: u32,
    pub challenge_anchor: Digest,
    pub sample_seed: Digest,
    /// Independent vector-equality checks the suite ran (before repetitions).
    pub sample_count: u128,
    pub field_policy_id: u32,
    pub freivalds_rounds: u8,
    pub soundness_policy_id: u32,
    pub derived_soundness_bits: u16,
    pub evidence_manifest_root: Digest,
    pub verdict: ReceiptVerdictV1,
    pub seat_bond: Digest,
    pub seat_operator: Digest,
    pub signed_daa: u64,
}

impl PalwConstraintReceiptV1 {
    pub fn id(&self) -> Digest {
        object_id(CONSTRAINT_RECEIPT_DOMAIN_V1, self)
    }

    /// The bytes the seat's key signs: the receipt's id under the network domain (the network is inside the id as well).
    pub fn signing_message(&self) -> Digest {
        let mut s = keyed(CONSTRAINT_RECEIPT_DOMAIN_V1);
        s.update(&self.network_domain);
        s.update(&self.id());
        finish(s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedConstraintReceiptV1 {
    pub receipt: PalwConstraintReceiptV1,
    pub signature: Vec<u8>,
}

/// The node's signature check (ML-DSA-87 against the seat bond's registered key).
pub trait ReceiptSignatureVerifier {
    fn verify(&self, seat_bond: &Digest, message: &Digest, signature: &[u8]) -> bool;
}

/// What a seat fills its receipt from.
pub struct ReceiptInputsV1<'a> {
    pub descriptor: &'a KernelDescriptorV1,
    pub evidence: &'a VerificationEvidenceV1,
    pub claim_id: Digest,
    pub assignment_root: Digest,
    pub challenge_anchor: Digest,
    pub sample_seed: Digest,
    pub seat_bond: Digest,
    pub seat_operator: Digest,
    pub signed_daa: u64,
}

/// **The receipt a seat files for its scope verdict.** `None` for a verdict that is no statement about the claim (malformed
/// evidence is refused before any duty; an inconsistent verifier files nothing).
pub fn receipt_for_v1(i: &ReceiptInputsV1<'_>, scope: &ScopeV1, verdict: &ScopeVerdictV1) -> Option<PalwConstraintReceiptV1> {
    let (v, count, positions) = match verdict {
        ScopeVerdictV1::Pass { probabilistic_checks, positions, .. } => (ReceiptVerdictV1::Pass, *probabilistic_checks, *positions),
        ScopeVerdictV1::Fault(p) => (ReceiptVerdictV1::Fail { position: p.position, occurrence: p.occurrence, node: p.node }, 0, 0),
        ScopeVerdictV1::Unavailable { .. } => (ReceiptVerdictV1::Unavailable, 0, 0),
        ScopeVerdictV1::EvidenceMalformed { .. } | ScopeVerdictV1::Inconsistent { .. } => return None,
    };
    let d = i.descriptor;
    Some(PalwConstraintReceiptV1 {
        network_domain: i.evidence.header.network_domain,
        claim_id: i.claim_id,
        class_id: i.evidence.header.class_binding_id,
        verification_profile_hash: i.evidence.header.plan_root,
        verification_scheme_id: d.checker_suite_id,
        scheme_version: d.version,
        descriptor_digest: d.digest(),
        assignment_root: i.assignment_root,
        scope: scope.clone(),
        scope_root: scope.root(i.evidence),
        covered_positions: positions,
        challenge_anchor: i.challenge_anchor,
        sample_seed: i.sample_seed,
        sample_count: count,
        field_policy_id: d.arithmetic_id,
        freivalds_rounds: d.soundness.repetitions,
        soundness_policy_id: d.soundness_policy_id,
        derived_soundness_bits: if v == ReceiptVerdictV1::Pass { derived_error_bits(d, count) } else { 0 },
        evidence_manifest_root: i.evidence.root(),
        verdict: v,
        seat_bond: i.seat_bond,
        seat_operator: i.seat_operator,
        signed_daa: i.signed_daa,
    })
}

/// What the fold knows about the claim a receipt is for.
pub struct ClaimFactsV1<'a> {
    pub descriptor: &'a KernelDescriptorV1,
    pub evidence: &'a VerificationEvidenceV1,
    pub claim_id: Digest,
    pub assignment_root: Digest,
    pub challenge_anchor: Digest,
    pub sample_seed: Digest,
    /// The duties: which seat (bond) was assigned which scope.
    pub assignments: &'a [(Digest, ScopeV1)],
    pub deadline_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReceiptRefusalV1 {
    #[error("the signature does not verify under the seat bond's key")]
    BadSignature,
    #[error("another claim, network, class, profile or evidence")]
    WrongClaim,
    #[error("another scheme, version or descriptor than the class binds")]
    IneligibleScheme,
    #[error("the challenge anchor or seed is not the claim's")]
    WrongChallenge,
    #[error("the scope root is not the scope's")]
    WrongScope,
    #[error("the seat was not assigned this scope")]
    NotAssigned,
    #[error("the stated soundness or rounds are not the policy's for the stated check count")]
    WrongSoundness,
    #[error("signed at DAA {signed}, after the deadline {deadline}")]
    Late { signed: u64, deadline: u64 },
}

/// **The fold's structural check of one signed receipt.**
pub fn admit_receipt_v1(
    signed: &SignedConstraintReceiptV1,
    facts: &ClaimFactsV1<'_>,
    sig: &dyn ReceiptSignatureVerifier,
) -> Result<(), ReceiptRefusalV1> {
    let r = &signed.receipt;
    let (d, ev) = (facts.descriptor, facts.evidence);
    if !sig.verify(&r.seat_bond, &r.signing_message(), &signed.signature) {
        return Err(ReceiptRefusalV1::BadSignature);
    }
    if r.claim_id != facts.claim_id
        || r.network_domain != ev.header.network_domain
        || r.class_id != ev.header.class_binding_id
        || r.verification_profile_hash != ev.header.plan_root
        || r.evidence_manifest_root != ev.root()
    {
        return Err(ReceiptRefusalV1::WrongClaim);
    }
    if r.descriptor_digest != d.digest()
        || r.verification_scheme_id != d.checker_suite_id
        || r.scheme_version != d.version
        || r.field_policy_id != d.arithmetic_id
        || r.soundness_policy_id != d.soundness_policy_id
    {
        return Err(ReceiptRefusalV1::IneligibleScheme);
    }
    if r.challenge_anchor != facts.challenge_anchor || r.sample_seed != facts.sample_seed || r.assignment_root != facts.assignment_root
    {
        return Err(ReceiptRefusalV1::WrongChallenge);
    }
    if r.scope_root != r.scope.root(ev) || r.scope.positions(ev).is_err() {
        return Err(ReceiptRefusalV1::WrongScope);
    }
    if !facts.assignments.iter().any(|(bond, scope)| *bond == r.seat_bond && *scope == r.scope) {
        return Err(ReceiptRefusalV1::NotAssigned);
    }
    let expected_bits = if r.verdict == ReceiptVerdictV1::Pass { derived_error_bits(d, r.sample_count) } else { 0 };
    if r.freivalds_rounds != d.soundness.repetitions || r.derived_soundness_bits != expected_bits {
        return Err(ReceiptRefusalV1::WrongSoundness);
    }
    if r.signed_daa > facts.deadline_daa {
        return Err(ReceiptRefusalV1::Late { signed: r.signed_daa, deadline: facts.deadline_daa });
    }
    Ok(())
}

/// The licence rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TallyPolicyV1 {
    /// Distinct operators whose counted passing receipts must cover each segment.
    pub per_segment_quorum: u8,
}

/// Where a claim's tally stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TallyStateV1 {
    /// Segments still short of their quorum.
    Incomplete { uncovered: Vec<u32> },
    /// Every segment covered by `quorum` distinct operators' passes, and no counted failure.
    Covered,
    /// A counted failing receipt: a dispute is open and no later pass erases it.
    Disputed { first: (Digest, ReceiptVerdictV1) },
    /// A counted Unavailable: the DA path.
    Unavailable { seat: Digest },
}

/// **The coverage-aware tally** of one claim's admitted receipts.
#[derive(Clone, Debug)]
pub struct ConstraintTallyV1 {
    pub policy: TallyPolicyV1,
    segments: u32,
    /// The counted receipt per `(seat, scope_root)` duty.
    counted: BTreeMap<(Digest, Digest), PalwConstraintReceiptV1>,
    /// `segment → operators` with a counted pass.
    covered: BTreeMap<u32, BTreeSet<Digest>>,
    pub equivocations: Vec<Digest>,
    first_failure: Option<(Digest, ReceiptVerdictV1)>,
    unavailable: Option<Digest>,
}

impl ConstraintTallyV1 {
    pub fn new(policy: TallyPolicyV1, evidence: &VerificationEvidenceV1) -> Self {
        Self {
            policy,
            segments: evidence.segments.len() as u32,
            counted: BTreeMap::new(),
            covered: BTreeMap::new(),
            equivocations: Vec::new(),
            first_failure: None,
            unavailable: None,
        }
    }

    /// Add one ADMITTED receipt ([`admit_receipt_v1`]). Returns whether it was counted.
    pub fn add(&mut self, r: &PalwConstraintReceiptV1, evidence: &VerificationEvidenceV1) -> bool {
        let duty = (r.seat_bond, r.scope_root);
        if let Some(first) = self.counted.get(&duty) {
            if first.verdict != r.verdict && !self.equivocations.contains(&r.seat_bond) {
                self.equivocations.push(r.seat_bond);
            }
            return false;
        }
        self.counted.insert(duty, r.clone());
        match &r.verdict {
            ReceiptVerdictV1::Pass => {
                // An audit sample is never coverage.
                for seg in r.scope.segments(evidence) {
                    self.covered.entry(seg).or_default().insert(r.seat_operator);
                }
            }
            v @ ReceiptVerdictV1::Fail { .. } => {
                self.first_failure.get_or_insert((r.seat_bond, v.clone()));
            }
            ReceiptVerdictV1::Unavailable => {
                self.unavailable.get_or_insert(r.seat_bond);
            }
        }
        true
    }

    pub fn state(&self) -> TallyStateV1 {
        if let Some(first) = &self.first_failure {
            return TallyStateV1::Disputed { first: first.clone() };
        }
        let need = self.policy.per_segment_quorum.max(1) as usize;
        let uncovered: Vec<u32> = (0..self.segments).filter(|s| self.covered.get(s).is_none_or(|ops| ops.len() < need)).collect();
        if uncovered.is_empty() {
            return TallyStateV1::Covered;
        }
        if let Some(seat) = self.unavailable {
            return TallyStateV1::Unavailable { seat };
        }
        TallyStateV1::Incomplete { uncovered }
    }
}
