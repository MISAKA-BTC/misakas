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
// 15 is G14-R4's (the accuser seal). 16–18: K2-TIR-v4 (lane K2S, allocated 2026-10-08); 19 is RFC-0004 Part II's `Spec`.
/// K2-TIR-v4: a segmented claim (`docs/design/palw/k2-real-scale.md`).
pub const TAG_COMMIT_SEGMENTED_CLAIM_V1: u8 = 16;
/// K2-TIR-v4: a job whose prompt is posted in tiles.
pub const TAG_POST_TILED_JOB_V1: u8 = 17;
/// K2-TIR-v4: one prompt tile of a tiled job.
pub const TAG_POST_PROMPT_TILE_V1: u8 = 18;
/// RFC-0004 Part II: a typed-root object (registration, job, claim — versioned inside [`crate::spec::SpecObjectV1`]).
pub const TAG_SPEC_V1: u8 = 19;

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
/// A segmented claim: the claim, the evidence object and ≤ 2,048 segment roots (2^21 positions).
pub const MAX_COMMIT_SEGMENTED_CLAIM_BYTES_V1: usize = 4 << 20;
pub const MAX_POST_TILED_JOB_BYTES_V1: usize = 1024;
/// One tile of 4,096 ids and its path.
pub const MAX_POST_PROMPT_TILE_BYTES_V1: usize = 64 << 10;
/// The `Spec` object's one ceiling: its largest sub-object's ([`crate::spec::MAX_SPEC_CLAIM_BYTES_V1`]); each sub-object's own is
/// checked by the ledger.
pub const MAX_SPEC_BYTES_V1: usize = MAX_COMMIT_CLAIM_BYTES_V1;

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
    /// K2-TIR-v4: a segmented fault's canonical bytes ([`crate::element::SegFaultV1`]: element, malformed or decode).
    Segmented(Vec<u8>) = 3,
    /// RFC-0004 Part II: a typed claim's fault (borsh [`crate::spec::SpecFaultV1`]: a memory step, a retrieval item, a composite stage
    /// or edge).
    Spec(Vec<u8>) = 4,
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
    /// **K2-TIR-v4**: commit a segmented claim (signed by `claim.producer_bond`): the evidence object and ONE ROOT PER SEGMENT of
    /// 1,024 positions — never the node commitments, which are served material.
    CommitSegmentedClaim {
        claim: KernelClaimV1,
        evidence: crate::seg::SegmentedEvidenceV2,
        segment_roots: Vec<Digest>,
    } = 16,
    /// **K2-TIR-v4**: post a job whose prompt is committed by its tile root; its tiles follow as `PostPromptTile`s.
    PostTiledJob {
        job: crate::seg::TiledJobV1,
    } = 17,
    /// **K2-TIR-v4**: post one tile of a tiled job's prompt (any bond). A claim commits only once every tile is posted.
    PostPromptTile {
        job: Digest,
        tile: crate::seg::PromptTileOpeningV1,
    } = 18,
    /// **RFC-0004 Part II**: a typed-root object. Accepted only while the ledger's schedule has the typed-roots extension `K2-TR-v1`
    /// Active (the consumer's `palw_typed_roots_v1` fence); a claim is signed by its producer.
    Spec {
        object: crate::spec::SpecObjectV1,
    } = 19,
}

/// The seal of a claim: `H("misaka-palw/kernel/claim-seal/v1"; claim id)` (the claim id binds the producer, job, output and evidence).
pub fn claim_seal_v1(claim_id: &Digest) -> Digest {
    crate::hash::id(b"misaka-palw/kernel/claim-seal/v1", claim_id)
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
            Self::CommitSegmentedClaim { .. } => TAG_COMMIT_SEGMENTED_CLAIM_V1,
            Self::PostTiledJob { .. } => TAG_POST_TILED_JOB_V1,
            Self::PostPromptTile { .. } => TAG_POST_PROMPT_TILE_V1,
            Self::Spec { .. } => TAG_SPEC_V1,
        }
    }

    pub const fn name(&self) -> &'static str {
        name_of_tag(self.tag())
    }

    /// The ceiling on this variant's encoded size (the version byte included).
    pub const fn max_encoded_bytes(&self) -> usize {
        match max_encoded_bytes_of_tag(self.tag()) {
            Some(n) => n,
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
        TAG_COMMIT_SEGMENTED_CLAIM_V1 => "CommitSegmentedClaim",
        TAG_POST_TILED_JOB_V1 => "PostTiledJob",
        TAG_POST_PROMPT_TILE_V1 => "PostPromptTile",
        TAG_SPEC_V1 => "Spec",
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
        TAG_COMMIT_SEGMENTED_CLAIM_V1 => MAX_COMMIT_SEGMENTED_CLAIM_BYTES_V1,
        TAG_POST_TILED_JOB_V1 => MAX_POST_TILED_JOB_BYTES_V1,
        TAG_POST_PROMPT_TILE_V1 => MAX_POST_PROMPT_TILE_BYTES_V1,
        TAG_SPEC_V1 => MAX_SPEC_BYTES_V1,
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
        ];
        for o in objects {
            let bytes = o.encode();
            assert_eq!(bytes[0], KERNEL_ROUTE_VERSION_V1);
            assert_eq!(bytes[1], o.tag(), "{}", o.name());
            assert!(bytes.len() <= o.max_encoded_bytes(), "{}", o.name());
            assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
            assert_ne!(o.name(), "Unknown");
        }
        let tags: Vec<u8> = (1..=14).chain(16..=18).collect();
        assert!(tags.iter().all(|t| max_encoded_bytes_of_tag(*t).is_some() && name_of_tag(*t) != "Unknown"));
        assert!(
            max_encoded_bytes_of_tag(0).is_none() && max_encoded_bytes_of_tag(15).is_none() && max_encoded_bytes_of_tag(20).is_none()
        );
        assert!(max_encoded_bytes_of_tag(TAG_SPEC_V1).is_some() && name_of_tag(TAG_SPEC_V1) == "Spec", "RFC-0004 Part II's tag 19");
    }

    #[test]
    fn malformed_and_oversized_encodings_are_refused_with_their_kind() {
        let good = demand().encode();
        let kind = |b: &[u8]| KernelRouteObjectV1::decode(b).unwrap_err().kind;
        assert_eq!(kind(&[]), RefusalKindV1::Malformed);
        assert_eq!(kind(&[2]), RefusalKindV1::Malformed, "another version");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1]), RefusalKindV1::Malformed, "no variant");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1, 0]), RefusalKindV1::Malformed, "tag 0 is not declared");
        assert_eq!(kind(&[KERNEL_ROUTE_VERSION_V1, 15]), RefusalKindV1::Malformed, "an undeclared tag");
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
