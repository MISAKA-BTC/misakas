//! **Public prosecution on the kernel route** (ADR-0173; the 2026-10-07 mission-alignment amendments of RFC-0004, RFC-0005,
//! RFC-0007 and RFC-0011; RFC-0015 §1.1's G14 criteria).
//!
//! The amendments make one thing the measure of this route: an **ordinary, non-seat public bond** that started after the claim was
//! published fetches only public, authenticated bytes, localizes a lie to one primitive instance and carries it to an objective
//! conviction — or, when the producer withholds, to an objective DA/default. Kernel-only certificates, an honest Final, HF coverage
//! and `ε_check` are each reported, and none of them substitutes for this.
//!
//! * [`PublicClaimRecordV1`] — what a claim publishes (canonical program bytes, plan, §15.3 evidence object, trace and param
//!   commitments, job input, claim id, beacon). [`FreshVerifierV1`] is built from those **bytes** and the binary's own descriptors:
//!   no producer object, no seat secret, no sketch. A plan naming a descriptor the binary does not implement is refused.
//! * [`TensorWireV1`], [`FaultProofWireV1`] — the canonical bytes of an opening and of a fault proof, so filing and the court are
//!   byte-level operations any node repeats.
//! * [`MaterialDemandV1`] / [`settle_demand_v1`] — the withholding path: an on-chain demand for one committed value with a deadline;
//!   a served value that matches its commitment settles it; silence, or bytes that are not the committed value, become
//!   `ProducerDefault` at the deadline — an availability outcome, never an arithmetic conviction (ADR-0173 D3).
//! * [`ProsecutionGateV1`] — RFC-0015 §1.1's seven criteria as a release record per profile, and [`reward_eligible_v1`]: no new
//!   reward for a profile until the gate is complete and its material is public (no private weights/input/state, no FOLD prefix or
//!   fused preimage). [`ReleaseMetricsV1`] keeps coverage, `ε_check`, source fidelity and the gate apart.

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::Tensor;
use misaka_palw_tir::program::TirProgramV1;

use crate::challenge::ChallengeBindingV1;
use crate::descriptor::KernelDescriptorV1;
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1};
use crate::hash::{Digest, id};
use crate::outcome::{CoverageBucketV1, RegistrationOutcomeV1};
use crate::plan::{VerificationPlanV1, dtype_of_tag};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, tensor_commitment};
use crate::verify::{
    ClaimContextV1, ConvictionV1, DismissalV1, FaultKindV1, KernelFaultProofV1, MaterialV1, ScopeV1, ScopeVerdictV1,
    verify_fault_proof_v1, verify_scope_v1,
};

pub const PROGRAM_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/program-root/v1";
/// A parse bound on a published record (a DoS limit, not a semantic one).
pub const MAX_PUBLIC_RECORD_BYTES_V1: usize = 1 << 30;

/// The root a class binds for its canonical program bytes.
pub fn program_root_v1(program_bytes: &[u8]) -> Digest {
    id(PROGRAM_ROOT_DOMAIN_V1, program_bytes)
}

/// One tensor's canonical bytes: the dtype tag, the shape, and the little-endian elements at the dtype's width — exactly what its
/// commitment hashes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TensorWireV1 {
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub bytes: Vec<u8>,
}

impl TensorWireV1 {
    pub fn of(t: &Tensor) -> Self {
        Self { dtype: t.dtype.tag(), shape: t.shape.iter().map(|d| *d as u64).collect(), bytes: t.to_le_bytes() }
    }

    pub fn decode(&self) -> Result<Tensor, String> {
        let dtype = dtype_of_tag(self.dtype).ok_or("an unknown dtype tag")?;
        let shape: Vec<usize> = self.shape.iter().map(|d| usize::try_from(*d)).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        Tensor::from_le_bytes(dtype, &shape, &self.bytes).map_err(|e| e.to_string())
    }
}

/// **What a claim publishes**: enough for anyone to check it and to prosecute it, and nothing the producer keeps.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PublicClaimRecordV1 {
    pub claim_id: Digest,
    /// The canonical TIR program bytes (their root is the header's `program_root`).
    pub program_bytes: Vec<u8>,
    pub plan: VerificationPlanV1,
    pub evidence: VerificationEvidenceV1,
    /// `trace_commitments[position][occurrence][node]`.
    pub trace_commitments: Vec<Vec<Vec<Digest>>>,
    /// `(param, layer, commitment)` in order (their root is the header's `artifact_root`).
    pub param_commitments: Vec<(u16, Option<u16>, Digest)>,
    pub tokens: Vec<u32>,
    /// The beacon the challenge was drawn from ([`crate::beacon`]).
    pub beacon: Digest,
}

impl PublicClaimRecordV1 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PUBLIC_RECORD_BYTES_V1 {
            return Err("the record exceeds the parse bound".into());
        }
        borsh::from_slice(bytes).map_err(|e| format!("not a public claim record: {e}"))
    }
}

/// **A verifier that knows only public bytes** and the descriptors its own binary implements.
pub struct FreshVerifierV1 {
    pub record: PublicClaimRecordV1,
    pub descriptor: KernelDescriptorV1,
    pub program: TirProgramV1,
    pub trace: EvidenceV1,
    pub params: ParamCommitmentsV1,
    pub header: EvidenceHeaderV1,
}

impl FreshVerifierV1 {
    /// Build from a record's bytes. `known` is the binary's implemented descriptors (never bytes from the claim); `header` is what
    /// the chain says the claim's header is (its class binding, network and ruleset).
    pub fn from_public_bytes(bytes: &[u8], known: &[KernelDescriptorV1], header: EvidenceHeaderV1) -> Result<Self, String> {
        let record = PublicClaimRecordV1::from_bytes(bytes)?;
        let descriptor = known
            .iter()
            .find(|d| d.digest() == record.plan.descriptor_digest)
            .cloned()
            .ok_or("the plan names a kernel this binary does not implement (never success)")?;
        let program = TirProgramV1::decode_canonical(&record.program_bytes).map_err(|e| format!("program bytes: {e}"))?;
        if program_root_v1(&record.program_bytes) != header.program_root {
            return Err("the published program is not the class's".into());
        }
        let params = ParamCommitmentsV1 { by_instance: record.param_commitments.iter().map(|(j, l, d)| ((*j, *l), *d)).collect() };
        if params.root() != header.artifact_root || params.by_instance.len() != record.param_commitments.len() {
            return Err("the published param commitments are not the class's artifact".into());
        }
        let trace = EvidenceV1::new(record.trace_commitments.clone());
        Ok(Self { record, descriptor, program, trace, params, header })
    }

    fn ctx(&self) -> ClaimContextV1<'_> {
        ClaimContextV1 {
            descriptor: &self.descriptor,
            program: &self.program,
            plan: &self.record.plan,
            trace: &self.trace,
            evidence: &self.record.evidence,
            header: self.header,
            params: &self.params,
            tokens: &self.record.tokens,
            binding: ChallengeBindingV1 {
                network_domain: self.header.network_domain,
                claim_id: self.record.claim_id,
                class_binding_id: self.header.class_binding_id,
                plan_root: self.record.plan.root(),
                evidence_root: self.record.evidence.root(),
                beacon: self.record.beacon,
            },
            stage: None,
        }
    }

    /// Check a scope with values served by whoever serves them (every one authenticated against its commitment).
    pub fn check(&self, served: &dyn MaterialV1, scope: &ScopeV1) -> ScopeVerdictV1 {
        verify_scope_v1(&self.ctx(), served, scope)
    }

    /// **The court**, from the proof's bytes.
    pub fn try_proof(&self, proof_bytes: &[u8]) -> Result<ConvictionV1, DismissalV1> {
        let wire: FaultProofWireV1 =
            borsh::from_slice(proof_bytes).map_err(|e| DismissalV1::NotAuthentic(format!("not a fault proof: {e}")))?;
        let proof = wire.decode().map_err(DismissalV1::NotAuthentic)?;
        verify_fault_proof_v1(&self.ctx(), &proof)
    }

    /// The committed value a demand names (`None`: no such value in this claim).
    pub fn commitment_of(&self, key: &MaterialKeyV1) -> Option<Digest> {
        match *key {
            MaterialKeyV1::Node { position, occurrence, node } => {
                self.trace.commitments.get(position as usize)?.get(occurrence as usize)?.get(node as usize).copied()
            }
            MaterialKeyV1::Param { index, layer } => self.params.by_instance.get(&(index, layer)).copied(),
        }
    }
}

/// A fault proof's canonical bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FaultProofWireV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    /// 0 malformed, 1 recompute, 2 matmul scalar.
    pub kind: u8,
    pub scalar: (u64, u64, u64),
    pub output: TensorWireV1,
    pub inputs: Vec<TensorWireV1>,
    pub prior_rows: Vec<TensorWireV1>,
}

impl FaultProofWireV1 {
    pub fn of(p: &KernelFaultProofV1) -> Self {
        let (kind, scalar) = match p.kind {
            FaultKindV1::Malformed => (0, (0, 0, 0)),
            FaultKindV1::Recompute => (1, (0, 0, 0)),
            FaultKindV1::MatMulScalar { slice, i, j } => (2, (slice, i, j)),
        };
        Self {
            position: p.position,
            occurrence: p.occurrence,
            node: p.node,
            kind,
            scalar,
            output: TensorWireV1::of(&p.output),
            inputs: p.inputs.iter().map(TensorWireV1::of).collect(),
            prior_rows: p.prior_rows.iter().map(TensorWireV1::of).collect(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }

    pub fn decode(&self) -> Result<KernelFaultProofV1, String> {
        let kind = match self.kind {
            0 => FaultKindV1::Malformed,
            1 => FaultKindV1::Recompute,
            2 => FaultKindV1::MatMulScalar { slice: self.scalar.0, i: self.scalar.1, j: self.scalar.2 },
            k => return Err(format!("unknown fault kind {k} (never success)")),
        };
        let all = |v: &[TensorWireV1]| v.iter().map(TensorWireV1::decode).collect::<Result<Vec<_>, _>>();
        Ok(KernelFaultProofV1 {
            position: self.position,
            occurrence: self.occurrence,
            node: self.node,
            kind,
            output: self.output.decode()?,
            inputs: all(&self.inputs)?,
            prior_rows: all(&self.prior_rows)?,
        })
    }
}

/// One committed value a demand can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MaterialKeyV1 {
    Node { position: u32, occurrence: u16, node: u16 },
    Param { index: u16, layer: Option<u16> },
}

/// An on-chain demand, filed by any public bond, for one committed value of a claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialDemandV1 {
    pub claim_id: Digest,
    pub key: MaterialKeyV1,
    pub demander_bond: Digest,
    pub filed_daa: u64,
    pub deadline_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DemandOutcomeV1 {
    /// Served, and it is the committed value: prosecution continues from it.
    Served(Tensor),
    /// Before the deadline, nothing valid served yet.
    Pending,
    /// The deadline passed with nothing valid served: the producer's availability default (never an arithmetic conviction).
    ProducerDefault,
    /// The demand names no value of this claim (dismissed; the demander's filing was wrong).
    NoSuchValue,
}

/// **Settle a demand** at `now_daa` given what was served (if anything). Bytes that are not the committed value count as not
/// served; whether the producer lied about arithmetic is a separate question only a fault proof answers.
pub fn settle_demand_v1(
    v: &FreshVerifierV1,
    demand: &MaterialDemandV1,
    response: Option<&TensorWireV1>,
    now_daa: u64,
) -> DemandOutcomeV1 {
    let Some(committed) = v.commitment_of(&demand.key) else { return DemandOutcomeV1::NoSuchValue };
    if demand.claim_id != v.record.claim_id {
        return DemandOutcomeV1::NoSuchValue;
    }
    if let Some(t) = response.and_then(|w| w.decode().ok())
        && tensor_commitment(&t) == committed
    {
        return DemandOutcomeV1::Served(t);
    }
    if now_daa >= demand.deadline_daa { DemandOutcomeV1::ProducerDefault } else { DemandOutcomeV1::Pending }
}

/// RFC-0015 §1.1 (G14): each criterion a release drill must evidence for a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ProsecutionCriterionV1 {
    /// A bond that is neither a genesis operator nor a seat joined under the public rules.
    OrdinaryPublicEntry = 1,
    /// A node started after the claim was published fetched what it needed before the cutoff.
    FreshVerifier = 2,
    /// Every byte used was bound to claim/program/artifact roots; no producer secret, private API or liar instance.
    AuthenticatedMaterialOnly = 3,
    /// The plan's fault was localized to a bounded exact terminal within declared bytes/work/rounds.
    CompleteLocalization = 4,
    /// Disclosed fraud convicted, withheld material defaulted, honest claims and false accusations dismissed.
    ObjectiveOutcome = 5,
    /// Filing was permissionless and not pre-empted by another bond's court.
    PermissionlessFiling = 6,
    /// RPC fetch through signature, fee, inclusion, fold, conviction/slash and blocked Final on a real node.
    ActualChainPath = 7,
}

impl ProsecutionCriterionV1 {
    pub const ALL: [ProsecutionCriterionV1; 7] = [
        Self::OrdinaryPublicEntry,
        Self::FreshVerifier,
        Self::AuthenticatedMaterialOnly,
        Self::CompleteLocalization,
        Self::ObjectiveOutcome,
        Self::PermissionlessFiling,
        Self::ActualChainPath,
    ];
}

/// What a profile needs a prosecutor to obtain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProfileMaterialV1 {
    pub weights_public: bool,
    pub inputs_public: bool,
    pub state_public: bool,
    /// The profile's check needs a FOLD prefix or another seat-only preimage.
    pub needs_fold_prefix: bool,
    /// The profile's check needs a fused kernel's private preimage.
    pub needs_fused_preimage: bool,
}

impl ProfileMaterialV1 {
    /// A kernel-route profile: every node value committed and wired, no FOLD prefix, no fused preimage. Whether its weights are
    /// public is the class's rights/source fact, stated by the caller.
    pub fn kernel_route(weights_public: bool) -> Self {
        Self { weights_public, inputs_public: true, state_public: true, needs_fold_prefix: false, needs_fused_preimage: false }
    }

    pub fn is_public(&self) -> bool {
        self.weights_public && self.inputs_public && self.state_public && !self.needs_fold_prefix && !self.needs_fused_preimage
    }
}

/// One profile's G14 release record: which criteria a drill evidenced, and by what.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProsecutionGateV1 {
    /// The profile: a class binding id (or a plan root shared by a family of classes).
    pub profile: Digest,
    pub met: BTreeSet<ProsecutionCriterionV1>,
    /// The root of the drill's records (transcripts, txids, node logs).
    pub evidence_root: Digest,
    /// The prosecutor was a seat or an operator: the drill does not count for criterion 1.
    pub prosecutor_was_seat_or_operator: bool,
    /// The drill used producer state, a private endpoint or an injected instance: it does not count for criterion 3.
    pub used_private_material: bool,
}

impl ProsecutionGateV1 {
    /// The criteria not (validly) evidenced.
    pub fn missing(&self) -> Vec<ProsecutionCriterionV1> {
        ProsecutionCriterionV1::ALL
            .into_iter()
            .filter(|c| {
                !self.met.contains(c)
                    || (*c == ProsecutionCriterionV1::OrdinaryPublicEntry && self.prosecutor_was_seat_or_operator)
                    || (*c == ProsecutionCriterionV1::AuthenticatedMaterialOnly && self.used_private_material)
            })
            .collect()
    }

    pub fn complete(&self) -> bool {
        self.missing().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RewardBlockV1 {
    #[error("the class is not eligible on an active kernel ({0})")]
    NotEligible(&'static str),
    #[error("no prosecution drill is recorded for this profile")]
    NoDrill,
    #[error("the drill is for another profile")]
    OtherProfile,
    #[error("public prosecution is not complete: {0:?}")]
    Incomplete(Vec<ProsecutionCriterionV1>),
    #[error("the profile needs material a public bond cannot obtain (private weights/input/state, a FOLD prefix or a fused preimage)")]
    PrivateMaterial,
}

/// **May a new reward be enabled for this profile?** Static eligibility on an active kernel is necessary and never sufficient: the
/// G14 drill must be complete for exactly this profile, and every byte a prosecutor needs must be public.
pub fn reward_eligible_v1(
    profile: &Digest,
    outcome: &RegistrationOutcomeV1,
    material: &ProfileMaterialV1,
    gate: Option<&ProsecutionGateV1>,
) -> Result<(), RewardBlockV1> {
    if !outcome.is_eligible() {
        return Err(RewardBlockV1::NotEligible(outcome.code()));
    }
    if !material.is_public() {
        return Err(RewardBlockV1::PrivateMaterial);
    }
    let gate = gate.ok_or(RewardBlockV1::NoDrill)?;
    if gate.profile != *profile {
        return Err(RewardBlockV1::OtherProfile);
    }
    let missing = gate.missing();
    if !missing.is_empty() {
        return Err(RewardBlockV1::Incomplete(missing));
    }
    Ok(())
}

/// **The release metrics, side by side and never merged** (RFC-0011 amendment: HF coverage, `ε_check`, source fidelity and the
/// prosecution gate are separate measures).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseMetricsV1 {
    pub coverage: CoverageBucketV1,
    /// The derived whole-claim error, bits (`None` when not eligible).
    pub error_bits: Option<u16>,
    /// The importer's output was checked against the pinned source revision.
    pub source_fidelity: bool,
    /// Missing G14 criteria (empty: complete).
    pub prosecution_missing: Vec<ProsecutionCriterionV1>,
}

impl ReleaseMetricsV1 {
    pub fn of(
        outcome: &RegistrationOutcomeV1,
        evidence: crate::outcome::CoverageEvidenceV1,
        source_fidelity: bool,
        gate: Option<&ProsecutionGateV1>,
    ) -> Self {
        Self {
            coverage: outcome.coverage_bucket(evidence),
            error_bits: match outcome {
                RegistrationOutcomeV1::EligibleAt { error_bits, .. } => Some(*error_bits),
                _ => None,
            },
            source_fidelity,
            prosecution_missing: gate.map(|g| g.missing()).unwrap_or_else(|| ProsecutionCriterionV1::ALL.to_vec()),
        }
    }
}
