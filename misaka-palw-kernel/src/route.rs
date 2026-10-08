//! **The consensus-embeddable shape of the kernel route: the public objects, who signed them, and why one is refused.**
//!
//! A consensus consumer (lane D's `PalwChainStateV2` fold) embeds [`crate::ledger::KernelLedgerV1`] by feeding it
//! [`KernelRouteObjectV1`]s that it has already **parsed, size-limited and signature-verified**, together with the [`AuthV1`] it
//! derived (the bond whose key signed the object). The ledger never sees a key or a signature; it checks that the signer is the
//! producer / accuser / demander the object names, applies the rule, and either returns the receipts or refuses the object
//! without touching its state ([`KernelRefusalV1`]).
//!
//! # Wire form
//!
//! `version(1 byte) ‖ borsh(KernelRouteObjectV1)`, where the Borsh encoding starts with the variant's declared 1-byte
//! discriminant. [`KernelRouteObjectV1::decode`] is strict: the version byte, a known discriminant, the variant's
//! [`max_encoded_bytes`](KernelRouteObjectV1::max_encoded_bytes) (checked before any parsing), exact Borsh (no trailing byte) and
//! canonicality (re-encoding must reproduce the input). The ceilings here are the kernel's own hard limits; a consensus consumer
//! only ever tightens them (its transaction mass limit applies first).
//!
//! What is **not** a public object: bonds and the Panel's coverage. Both are derived by the consumer from authenticated chain state
//! (`KernelLedgerV1::sync_bond`, `KernelLedgerV1::apply_panel_tally`), so nobody can submit them as a transaction.

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::evidence::VerificationEvidenceV1;
use crate::hash::Digest;
use crate::job::{DecodeFaultV1, DecodeRuleV1, KernelClaimV1, KernelJobV1};
use crate::mode::VerificationModeV1;
use crate::pipeline::{PipelineEvidenceV1, PipelinePlanV1};
use crate::pipeline_public::{PipelineClaimV1, PipelineJobPostV1, StageCommitmentsV1};
use crate::plan::VerificationPlanV1;
use crate::trace::ParamCommitmentsV1;

/// The route's wire version (the first byte of every encoded object).
pub const KERNEL_ROUTE_VERSION_V1: u8 = 1;

/// Declared discriminants (never renumbered; a new object is a new number).
pub const TAG_REGISTER_CLASS_V1: u8 = 1;
pub const TAG_REGISTER_PIPELINE_CLASS_V1: u8 = 2;
pub const TAG_POST_JOB_V1: u8 = 3;
pub const TAG_POST_PIPELINE_JOB_V1: u8 = 4;
pub const TAG_COMMIT_CLAIM_V1: u8 = 5;
pub const TAG_COMMIT_PIPELINE_CLAIM_V1: u8 = 6;
pub const TAG_FILE_PROOF_V1: u8 = 7;
pub const TAG_FILE_DEMAND_V1: u8 = 8;
pub const TAG_RESPOND_V1: u8 = 9;
pub const TAG_REQUEST_EXIT_V1: u8 = 10;
pub const TAG_WITHDRAW_V1: u8 = 11;
pub const TAG_SEAL_CLAIM_V1: u8 = 12;
/// RFC-0015: a class registration that carries its verification mode (a non-legacy mode; the mode is bound into the class id).
pub const TAG_REGISTER_CLASS_V2: u8 = 13;
pub const TAG_REGISTER_PIPELINE_CLASS_V2: u8 = 14;
/// GAP-R7: an accuser's seal of its proof (seal, then reveal; the earliest seal of the convicting bytes is paid the bounty).
pub const TAG_SEAL_PROOF_V1: u8 = 15;
// 16–18 are K2S's (K2-TIR-v4), 19 R4X's (Spec); 21 and 22 were reserved beside 20 and are not used.
/// OPV-BOOT GAP-B1a: a claim reveal that carries its seal's salt (claim seal v2), past `palw_panel_free_v1`.
pub const TAG_COMMIT_CLAIM_SALTED_V1: u8 = 20;

/// Per-variant ceilings on the encoded object (version byte included), in bytes. A consumer's mass limit is tighter; these only
/// bound what the kernel will ever parse.
pub const MAX_REGISTER_CLASS_BYTES_V1: usize = 8 << 20;
pub const MAX_REGISTER_PIPELINE_CLASS_BYTES_V1: usize = 32 << 20;
pub const MAX_POST_JOB_BYTES_V1: usize = 1 << 20;
pub const MAX_POST_PIPELINE_JOB_BYTES_V1: usize = 64 << 20;
pub const MAX_COMMIT_CLAIM_BYTES_V1: usize = 128 << 20;
pub const MAX_COMMIT_PIPELINE_CLAIM_BYTES_V1: usize = 256 << 20;
pub const MAX_FILE_PROOF_BYTES_V1: usize = 64 << 20;
pub const MAX_FILE_DEMAND_BYTES_V1: usize = 256;
pub const MAX_RESPOND_BYTES_V1: usize = 128 << 20;
pub const MAX_REQUEST_EXIT_BYTES_V1: usize = 256;
pub const MAX_WITHDRAW_BYTES_V1: usize = 256;
pub const MAX_SEAL_CLAIM_BYTES_V1: usize = 256;
pub const MAX_REGISTER_CLASS_V2_BYTES_V1: usize = MAX_REGISTER_CLASS_BYTES_V1;
pub const MAX_REGISTER_PIPELINE_CLASS_V2_BYTES_V1: usize = MAX_REGISTER_PIPELINE_CLASS_BYTES_V1;
pub const MAX_SEAL_PROOF_BYTES_V1: usize = 256;
/// A salted reveal: the largest commit it can carry, its salt and the inner discriminant. The commit it carries is ALSO held to its
/// own kind's ceiling ([`KernelRouteObjectV1::max_encoded_bytes`]), so a salt never buys a larger claim.
pub const MAX_COMMIT_CLAIM_SALTED_BYTES_V1: usize = MAX_COMMIT_PIPELINE_CLAIM_BYTES_V1 + SALTED_REVEAL_OVERHEAD_V1;
/// What a salt adds to the commit it carries: the salt and the commit's own discriminant inside [`SaltedCommitV1`].
pub const SALTED_REVEAL_OVERHEAD_V1: usize = 64 + 1;

/// **Who signed an object**: the bond whose key the consumer verified. The ledger checks it names the actor the object names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct AuthV1 {
    pub signer_bond: Digest,
}

/// What a bond files against a claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ProsecutionV1 {
    /// A kernel fault proof's canonical bytes ([`crate::public::FaultProofWireV1`]).
    Kernel(Vec<u8>) = 0,
    /// A delivered token that is not the decode of the committed logits.
    Decode(DecodeFaultV1) = 1,
    /// A pipeline fault's canonical bytes ([`crate::pipeline_public::PipelineFaultWireV1`]: stage, edge or decode).
    Pipeline(Vec<u8>) = 2,
}

/// **The commit a salted reveal carries** (inner kind 20): the commit objects' own fields, under their own discriminants (5 and 6,
/// the commit tags — pinned by a test). A separate,
/// non-recursive enum — a reveal can never carry another reveal. (K2S appends its segmented commit here at integration.)
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SaltedCommitV1 {
    /// A single-program claim, as `CommitClaim` carries it.
    Claim { claim: KernelClaimV1, evidence: VerificationEvidenceV1, commitments: Vec<Vec<Vec<Digest>>> } = 5,
    /// A pipeline claim, as `CommitPipelineClaim` carries it.
    Pipeline { claim: PipelineClaimV1, evidence: PipelineEvidenceV1, stages: Vec<StageCommitmentsV1> } = 6,
}

impl SaltedCommitV1 {
    /// The kind the commit would have been carried as without its salt (its ceiling and its name).
    pub const fn commit_tag(&self) -> u8 {
        match self {
            Self::Claim { .. } => TAG_COMMIT_CLAIM_V1,
            Self::Pipeline { .. } => TAG_COMMIT_PIPELINE_CLAIM_V1,
        }
    }

    /// The producer bond the commit names (the reveal's signer).
    pub fn producer(&self) -> Digest {
        match self {
            Self::Claim { claim, .. } => claim.producer_bond,
            Self::Pipeline { claim, .. } => claim.producer_bond,
        }
    }
}

/// **The public objects of the kernel route.** Every one is signed by a bond (see [`AuthV1`]); none mints or moves money by
/// itself — payments are the ledger's [`crate::settle::SettlementInstructionV1`] receipts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum KernelRouteObjectV1 {
    /// Register a single-program class: the program, its plan and the artifact's **commitments only** (never the artifact).
    RegisterClass {
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        param_commitments: ParamCommitmentsV1,
    } = 1,
    /// Register a pipeline class (K2-TIR-v3).
    RegisterPipelineClass {
        descriptor: Digest,
        pipeline_bytes: Vec<u8>,
        program_bytes: Vec<Vec<u8>>,
        plan: PipelinePlanV1,
        param_commitments: Vec<ParamCommitmentsV1>,
        decode: Option<DecodeRuleV1>,
    } = 2,
    PostJob {
        job: KernelJobV1,
    } = 3,
    PostPipelineJob {
        job: PipelineJobPostV1,
    } = 4,
    /// Signed by `claim.producer_bond`.
    CommitClaim {
        claim: KernelClaimV1,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
    } = 5,
    /// Signed by `claim.producer_bond`.
    CommitPipelineClaim {
        claim: PipelineClaimV1,
        evidence: PipelineEvidenceV1,
        stages: Vec<StageCommitmentsV1>,
    } = 6,
    /// Signed by `accuser`.
    FileProof {
        accuser: Digest,
        claim: Digest,
        proof: ProsecutionV1,
    } = 7,
    /// A demand for every committed value (and stage input) of one stage position. Signed by `demander`.
    FileDemand {
        demander: Digest,
        claim: Digest,
        stage: u8,
        position: u32,
    } = 8,
    /// Anyone bonded (the producer, a DA provider) answers a position demand: borsh [`crate::public::PositionResponseV1`].
    Respond {
        claim: Digest,
        stage: u8,
        position: u32,
        bytes: Vec<u8>,
    } = 9,
    /// Signed by `bond`.
    RequestExit {
        bond: Digest,
    } = 10,
    /// Signed by `bond`.
    Withdraw {
        bond: Digest,
    } = 11,
    /// **Seal a claim before revealing it** (signed by `producer`): `seal = claim_seal_v1(claim id)`. A claim commits only over its
    /// producer's seal at least `claim_seal_delay_daa` old, so a mempool observer who copies a revealed claim's evidence under its
    /// own bond is always too late — the original's seal precedes anything the copyist could seal after seeing it.
    SealClaim {
        producer: Digest,
        job: Digest,
        seal: Digest,
    } = 12,
    /// **RFC-0015**: register a single-program class under an explicit non-legacy `mode`. The mode is part of the class id, so the
    /// same program, plan and artifact under another mode is another class. `PanelLicensed` is refused here (tag 1 is that mode's
    /// one registration path). A node accepts this tag only once the `palw_panel_free_v1` fence is reached.
    RegisterClassV2 {
        mode: VerificationModeV1,
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        param_commitments: ParamCommitmentsV1,
    } = 13,
    /// **RFC-0015**: register a pipeline class (K2-TIR-v3) under an explicit non-legacy `mode`.
    RegisterPipelineClassV2 {
        mode: VerificationModeV1,
        descriptor: Digest,
        pipeline_bytes: Vec<u8>,
        program_bytes: Vec<Vec<u8>>,
        plan: PipelinePlanV1,
        param_commitments: Vec<ParamCommitmentsV1>,
        decode: Option<DecodeRuleV1>,
    } = 14,
    /// **Seal a proof before filing it** (GAP-R7; signed by `accuser`): `seal = proof_seal_v1(claim, accuser, proof)`. A proof names
    /// no accuser inside its bytes, so anyone who sees a `FileProof`'s public carrier can lift the proof, re-sign it under its own
    /// bond and get it included first. At a conviction the bounty goes to the bond holding the EARLIEST seal (at least
    /// `claim_seal_delay_daa` old) of the convicting proof's exact bytes — whoever filed them — so a copyist that lifts a sealed proof
    /// pays its sealer, never itself. One seal per `(claim, accuser)` (a re-seal replaces it and restarts its clock); unrevealed
    /// seals expire after `seal_ttl_daa`.
    SealProof {
        accuser: Digest,
        claim: Digest,
        seal: Digest,
    } = 15,
    /// **A salted claim reveal** (OPV-BOOT GAP-B1a; signed by the commit's producer): the commit and the 64-byte salt its seal was
    /// made with, `seal = claim_seal_v2(claim id, salt)`, in ONE object — the salt is public exactly when the claim is. A claim of a
    /// deterministic class is a function of its public job and producer, so `claim_seal_v1(claim id)` hides nothing and a beacon
    /// over such seals could be ground by its last contributor; a secret salt (the producer's CSPRNG) makes the seal hiding. A node
    /// accepts this tag only once `palw_panel_free_v1` is reached; past it a seal accepted at or after the fence is revealed only
    /// this way, and the ledger keeps the salt (`claim_beacon_salts`) for the sealed-source beacon v3.
    CommitClaimSalted {
        salt: Digest,
        commit: SaltedCommitV1,
    } = 20,
}

/// The digest of a filed proof's exact bytes: `H("misaka-palw/kernel/proof-digest/v1"; borsh(proof))`.
pub fn proof_digest_v1(proof: &ProsecutionV1) -> Digest {
    crate::hash::object_id(b"misaka-palw/kernel/proof-digest/v1", proof)
}

/// **The seal of a proof** (GAP-R7): `H("misaka-palw/kernel/proof-seal/v1"; borsh(claim, accuser, proof_digest_v1(proof)))` — the
/// accuser is inside the seal, so a seal can never be re-attributed, and the proof is hidden until it is filed.
pub fn proof_seal_v1(claim: &Digest, accuser: &Digest, proof: &ProsecutionV1) -> Digest {
    proof_seal_of_digest_v1(claim, accuser, &proof_digest_v1(proof))
}

/// [`proof_seal_v1`] from the proof's digest (a ledger checks every seal on a claim against one digest).
pub fn proof_seal_of_digest_v1(claim: &Digest, accuser: &Digest, proof_digest: &Digest) -> Digest {
    crate::hash::object_id(b"misaka-palw/kernel/proof-seal/v1", &(*claim, *accuser, *proof_digest))
}

/// The seal of a claim: `H("misaka-palw/kernel/claim-seal/v1"; claim id)` (the claim id binds the producer, job, output and evidence).
pub fn claim_seal_v1(claim_id: &Digest) -> Digest {
    crate::hash::id(b"misaka-palw/kernel/claim-seal/v1", claim_id)
}

/// **The salted seal of a claim** (OPV-BOOT GAP-B1a): `H("misaka-palw/kernel/claim-seal/v2"; claim id ‖ salt)` with a 64-byte salt
/// from the producer's CSPRNG — the seal hides the claim until the salted reveal ([`KernelRouteObjectV1::CommitClaimSalted`]).
pub fn claim_seal_v2(claim_id: &Digest, salt: &Digest) -> Digest {
    let mut preimage = [0u8; 128];
    preimage[..64].copy_from_slice(claim_id);
    preimage[64..].copy_from_slice(salt);
    crate::hash::id(b"misaka-palw/kernel/claim-seal/v2", &preimage)
}

/// Why an object was not applied. The ledger's state is **byte-identical** to what it was before the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelRefusalV1 {
    pub object: &'static str,
    pub kind: RefusalKindV1,
    pub why: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefusalKindV1 {
    /// Not a canonical encoding of an object of this version.
    Malformed,
    /// Past the variant's encoded-size ceiling.
    Oversized,
    /// The signer is not the actor the object names (or not a bond the ledger knows).
    Unauthorized,
    /// The block's adjudication budget is spent; the object is dropped (never fatal), and may be included in a later block.
    OverBudget,
    /// The block is older than the ledger's clock.
    Stale,
    /// A rule of the route refuses it (a missing job, a binding fault, no free collateral, a closed window, …).
    Rule,
}

impl KernelRefusalV1 {
    pub fn new(object: &'static str, kind: RefusalKindV1, why: impl Into<String>) -> Self {
        Self { object, kind, why: why.into() }
    }

    pub fn rule(object: &'static str, why: impl Into<String>) -> Self {
        Self::new(object, RefusalKindV1::Rule, why)
    }
}

impl fmt::Display for KernelRefusalV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} refused ({:?}): {}", self.object, self.kind, self.why)
    }
}

impl std::error::Error for KernelRefusalV1 {}

/// A writer that only counts (the encoded size of an object without allocating it).
struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl KernelRouteObjectV1 {
    /// The variant's declared discriminant.
    pub const fn tag(&self) -> u8 {
        match self {
            Self::RegisterClass { .. } => TAG_REGISTER_CLASS_V1,
            Self::RegisterPipelineClass { .. } => TAG_REGISTER_PIPELINE_CLASS_V1,
            Self::PostJob { .. } => TAG_POST_JOB_V1,
            Self::PostPipelineJob { .. } => TAG_POST_PIPELINE_JOB_V1,
            Self::CommitClaim { .. } => TAG_COMMIT_CLAIM_V1,
            Self::CommitPipelineClaim { .. } => TAG_COMMIT_PIPELINE_CLAIM_V1,
            Self::FileProof { .. } => TAG_FILE_PROOF_V1,
            Self::FileDemand { .. } => TAG_FILE_DEMAND_V1,
            Self::Respond { .. } => TAG_RESPOND_V1,
            Self::RequestExit { .. } => TAG_REQUEST_EXIT_V1,
            Self::Withdraw { .. } => TAG_WITHDRAW_V1,
            Self::SealClaim { .. } => TAG_SEAL_CLAIM_V1,
            Self::RegisterClassV2 { .. } => TAG_REGISTER_CLASS_V2,
            Self::RegisterPipelineClassV2 { .. } => TAG_REGISTER_PIPELINE_CLASS_V2,
            Self::SealProof { .. } => TAG_SEAL_PROOF_V1,
            Self::CommitClaimSalted { .. } => TAG_COMMIT_CLAIM_SALTED_V1,
        }
    }

    pub const fn name(&self) -> &'static str {
        name_of_tag(self.tag())
    }

    /// The ceiling on this variant's encoded size (the version byte included). A salted reveal's is the ceiling of the commit it
    /// carries plus the salt: a salt never buys a larger claim.
    pub const fn max_encoded_bytes(&self) -> usize {
        let tag = match self {
            Self::CommitClaimSalted { commit, .. } => commit.commit_tag(),
            _ => self.tag(),
        };
        let extra = if matches!(self, Self::CommitClaimSalted { .. }) { SALTED_REVEAL_OVERHEAD_V1 } else { 0 };
        match max_encoded_bytes_of_tag(tag) {
            Some(n) => n + extra,
            None => 0,
        }
    }

    /// The encoded size, counted without allocating the encoding.
    pub fn encoded_len(&self) -> usize {
        let mut w = CountingWriter(1);
        self.serialize(&mut w).expect("counting cannot fail");
        w.0
    }

    /// `version ‖ borsh(self)`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![KERNEL_ROUTE_VERSION_V1];
        borsh::to_writer(&mut out, self).expect("in-memory borsh");
        out
    }

    /// **Strict decode**: version, known discriminant, the variant's size ceiling (before any parsing), exact Borsh, canonical.
    pub fn decode(bytes: &[u8]) -> Result<Self, KernelRefusalV1> {
        let bad = |kind, why: String| KernelRefusalV1::new("KernelRouteObject", kind, why);
        let (&version, rest) = bytes.split_first().ok_or_else(|| bad(RefusalKindV1::Malformed, "empty".into()))?;
        if version != KERNEL_ROUTE_VERSION_V1 {
            return Err(bad(RefusalKindV1::Malformed, format!("unknown route version {version}")));
        }
        let &tag = rest.first().ok_or_else(|| bad(RefusalKindV1::Malformed, "no variant".into()))?;
        let Some(limit) = max_encoded_bytes_of_tag(tag) else {
            return Err(bad(RefusalKindV1::Malformed, format!("unknown object tag {tag}")));
        };
        if bytes.len() > limit {
            return Err(KernelRefusalV1::new(
                name_of_tag(tag),
                RefusalKindV1::Oversized,
                format!("{} bytes past the {limit}-byte ceiling", bytes.len()),
            ));
        }
        let object: Self =
            borsh::from_slice(rest).map_err(|e| KernelRefusalV1::new(name_of_tag(tag), RefusalKindV1::Malformed, e.to_string()))?;
        // A variant whose ceiling depends on what it carries (a salted reveal: its commit's own) is held to it once parsed.
        if bytes.len() > object.max_encoded_bytes() {
            return Err(KernelRefusalV1::new(
                name_of_tag(tag),
                RefusalKindV1::Oversized,
                format!("{} bytes past the {}-byte ceiling of what it carries", bytes.len(), object.max_encoded_bytes()),
            ));
        }
        if object.encode() != bytes {
            return Err(KernelRefusalV1::new(name_of_tag(tag), RefusalKindV1::Malformed, "not the canonical encoding"));
        }
        Ok(object)
    }
}

pub const fn name_of_tag(tag: u8) -> &'static str {
    match tag {
        TAG_REGISTER_CLASS_V1 => "RegisterClass",
        TAG_REGISTER_PIPELINE_CLASS_V1 => "RegisterPipelineClass",
        TAG_POST_JOB_V1 => "PostJob",
        TAG_POST_PIPELINE_JOB_V1 => "PostPipelineJob",
        TAG_COMMIT_CLAIM_V1 => "CommitClaim",
        TAG_COMMIT_PIPELINE_CLAIM_V1 => "CommitPipelineClaim",
        TAG_FILE_PROOF_V1 => "FileProof",
        TAG_FILE_DEMAND_V1 => "FileDemand",
        TAG_RESPOND_V1 => "Respond",
        TAG_REQUEST_EXIT_V1 => "RequestExit",
        TAG_WITHDRAW_V1 => "Withdraw",
        TAG_SEAL_CLAIM_V1 => "SealClaim",
        TAG_REGISTER_CLASS_V2 => "RegisterClassV2",
        TAG_REGISTER_PIPELINE_CLASS_V2 => "RegisterPipelineClassV2",
        TAG_SEAL_PROOF_V1 => "SealProof",
        TAG_COMMIT_CLAIM_SALTED_V1 => "CommitClaimSalted",
        _ => "Unknown",
    }
}

/// The ceiling of a declared tag (`None`: not a declared tag).
pub const fn max_encoded_bytes_of_tag(tag: u8) -> Option<usize> {
    Some(match tag {
        TAG_REGISTER_CLASS_V1 => MAX_REGISTER_CLASS_BYTES_V1,
        TAG_REGISTER_PIPELINE_CLASS_V1 => MAX_REGISTER_PIPELINE_CLASS_BYTES_V1,
        TAG_POST_JOB_V1 => MAX_POST_JOB_BYTES_V1,
        TAG_POST_PIPELINE_JOB_V1 => MAX_POST_PIPELINE_JOB_BYTES_V1,
        TAG_COMMIT_CLAIM_V1 => MAX_COMMIT_CLAIM_BYTES_V1,
        TAG_COMMIT_PIPELINE_CLAIM_V1 => MAX_COMMIT_PIPELINE_CLAIM_BYTES_V1,
        TAG_FILE_PROOF_V1 => MAX_FILE_PROOF_BYTES_V1,
        TAG_FILE_DEMAND_V1 => MAX_FILE_DEMAND_BYTES_V1,
        TAG_RESPOND_V1 => MAX_RESPOND_BYTES_V1,
        TAG_REQUEST_EXIT_V1 => MAX_REQUEST_EXIT_BYTES_V1,
        TAG_WITHDRAW_V1 => MAX_WITHDRAW_BYTES_V1,
        TAG_SEAL_CLAIM_V1 => MAX_SEAL_CLAIM_BYTES_V1,
        TAG_REGISTER_CLASS_V2 => MAX_REGISTER_CLASS_V2_BYTES_V1,
        TAG_REGISTER_PIPELINE_CLASS_V2 => MAX_REGISTER_PIPELINE_CLASS_V2_BYTES_V1,
        TAG_SEAL_PROOF_V1 => MAX_SEAL_PROOF_BYTES_V1,
        TAG_COMMIT_CLAIM_SALTED_V1 => MAX_COMMIT_CLAIM_SALTED_BYTES_V1,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demand() -> KernelRouteObjectV1 {
        KernelRouteObjectV1::FileDemand { demander: [1; 64], claim: [2; 64], stage: 3, position: 0x0102_0304 }
    }

    #[test]
    fn the_wire_form_is_version_discriminant_then_the_fields() {
        let bytes = demand().encode();
        let mut expect = vec![KERNEL_ROUTE_VERSION_V1, TAG_FILE_DEMAND_V1];
        expect.extend([1u8; 64]);
        expect.extend([2u8; 64]);
        expect.push(3);
        expect.extend([4u8, 3, 2, 1]);
        assert_eq!(bytes, expect, "the declared discriminant is the Borsh variant byte");
        assert_eq!(demand().encoded_len(), bytes.len());
        assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), demand());
    }

    #[test]
    fn every_declared_discriminant_is_the_variants_borsh_byte_and_has_a_ceiling() {
        let objects = [
            KernelRouteObjectV1::PostJob {
                job: KernelJobV1 {
                    class_binding_id: [1; 64],
                    prompt: vec![1],
                    max_new_tokens: 1,
                    decode: DecodeRuleV1::Greedy,
                    nonce: [0; 64],
                },
            },
            demand(),
            KernelRouteObjectV1::Respond { claim: [1; 64], stage: 0, position: 0, bytes: vec![1, 2, 3] },
            KernelRouteObjectV1::RequestExit { bond: [7; 64] },
            KernelRouteObjectV1::Withdraw { bond: [7; 64] },
            KernelRouteObjectV1::FileProof { accuser: [1; 64], claim: [2; 64], proof: ProsecutionV1::Kernel(vec![9]) },
            KernelRouteObjectV1::FileProof { accuser: [1; 64], claim: [2; 64], proof: ProsecutionV1::Pipeline(vec![9]) },
            KernelRouteObjectV1::SealProof { accuser: [1; 64], claim: [2; 64], seal: [3; 64] },
        ];
        for o in objects {
            let bytes = o.encode();
            assert_eq!(bytes[0], KERNEL_ROUTE_VERSION_V1);
            assert_eq!(bytes[1], o.tag(), "{}", o.name());
            assert!(bytes.len() <= o.max_encoded_bytes(), "{}", o.name());
            assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
            assert_ne!(o.name(), "Unknown");
        }
        let tags: Vec<u8> = (1..=15).chain([TAG_COMMIT_CLAIM_SALTED_V1]).collect();
        assert!(tags.iter().all(|t| max_encoded_bytes_of_tag(*t).is_some() && name_of_tag(*t) != "Unknown"));
        assert!(max_encoded_bytes_of_tag(0).is_none() && max_encoded_bytes_of_tag(16).is_none());
        assert!((21..=22).all(|t| max_encoded_bytes_of_tag(t).is_none()), "21 and 22 are not used");
    }

    /// **Inner kind 20** (OPV-BOOT GAP-B1a): the salted reveal's wire form is `version ‖ 20 ‖ salt ‖ the commit's own tag ‖ fields`,
    /// its ceiling is the carried commit's own plus the salt, and the seal it opens is `claim_seal_v2`, which no other salt (and no
    /// v1 seal) matches.
    #[test]
    fn the_salted_reveal_is_kind_20_carries_its_commits_own_tag_and_opens_only_its_v2_seal() {
        let claim = PipelineClaimV1 {
            job_id: [1; 64],
            producer_bond: [2; 64],
            generated: vec![3],
            output_root: [5; 64],
            evidence_root: [4; 64],
        };
        let evidence = PipelineEvidenceV1 {
            network_domain: [0; 64],
            ruleset_digest: [0; 64],
            class_binding_id: [0; 64],
            pipeline_root: [0; 64],
            plan_root: [0; 64],
            job_root: [0; 64],
            random_binding: [0; 64],
            stages: vec![],
            output_root: [0; 64],
        };
        let o = KernelRouteObjectV1::CommitClaimSalted {
            salt: [9; 64],
            commit: SaltedCommitV1::Pipeline { claim: claim.clone(), evidence, stages: vec![] },
        };
        let bytes = o.encode();
        assert_eq!(&bytes[..2], &[KERNEL_ROUTE_VERSION_V1, TAG_COMMIT_CLAIM_SALTED_V1]);
        assert_eq!(&bytes[2..66], &[9u8; 64]);
        assert_eq!(bytes[66], TAG_COMMIT_PIPELINE_CLAIM_V1, "the carried commit keeps its own discriminant");
        assert_eq!((o.name(), o.max_encoded_bytes()), ("CommitClaimSalted", MAX_COMMIT_PIPELINE_CLAIM_BYTES_V1 + 65));
        assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
        assert_eq!(o.tag(), 20);
        let KernelRouteObjectV1::CommitClaimSalted { commit, .. } = &o else { unreachable!() };
        assert_eq!((commit.commit_tag(), commit.producer()), (TAG_COMMIT_PIPELINE_CLAIM_V1, [2; 64]));
        let id = [7u8; 64];
        let seal = claim_seal_v2(&id, &[9; 64]);
        assert_ne!(seal, claim_seal_v2(&id, &[8; 64]), "another salt is another seal");
        assert_ne!(seal, claim_seal_v1(&id), "a v1 seal is not a v2 seal");
        assert_ne!(seal, claim_seal_v2(&[6; 64], &[9; 64]), "another claim is another seal");
    }

    #[test]
    fn malformed_and_oversized_encodings_are_refused_with_their_kind() {
        let good = demand().encode();
        let kind = |b: &[u8]| KernelRouteObjectV1::decode(b).unwrap_err().kind;
        assert_eq!(kind(&[]), RefusalKindV1::Malformed);
        assert_eq!(kind(&[2]), RefusalKindV1::Malformed, "another version");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1]), RefusalKindV1::Malformed, "no variant");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1, 0]), RefusalKindV1::Malformed, "tag 0 is not declared");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1, 16]), RefusalKindV1::Malformed, "an undeclared tag");
        assert_eq!(kind(&good[..good.len() - 1]), RefusalKindV1::Malformed, "truncated");
        let mut trailing = good.clone();
        trailing.push(0);
        assert_eq!(kind(&trailing), RefusalKindV1::Malformed, "a trailing byte is not an object");
        let mut huge = good.clone();
        huge.resize(MAX_FILE_DEMAND_BYTES_V1 + 1, 0);
        assert_eq!(kind(&huge), RefusalKindV1::Oversized);
        // A Vec length prefix that claims more than the bytes carry never allocates it: the size ceiling and the read fail first.
        let mut lie = vec![KERNEL_ROUTE_VERSION_V1, TAG_RESPOND_V1];
        lie.extend([1u8; 64]);
        lie.extend([0u8, 0, 0, 0, 0]);
        lie.extend(u32::MAX.to_le_bytes());
        lie.extend([1, 2, 3]);
        assert_eq!(kind(&lie), RefusalKindV1::Malformed);
    }
}
