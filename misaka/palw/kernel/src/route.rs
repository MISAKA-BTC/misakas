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
// 16–18: K2-TIR-v4 (lane K2S, allocated 2026-10-08).
/// K2-TIR-v4: a segmented claim (`docs/design/palw/k2-real-scale.md`).
pub const TAG_COMMIT_SEGMENTED_CLAIM_V1: u8 = 16;
/// K2-TIR-v4: a job whose prompt is posted in tiles.
pub const TAG_POST_TILED_JOB_V1: u8 = 17;
/// K2-TIR-v4: one prompt tile of a tiled job.
pub const TAG_POST_PROMPT_TILE_V1: u8 = 18;
/// RFC-0004 Part II: a typed-root object (registration, job, claim — versioned inside [`crate::spec::SpecObjectV1`]).
pub const TAG_SPEC_V1: u8 = 19;
// 22–25: RFC02 descriptor-scoped model onboarding and complete conformance (2026-10-11).
/// RFC-0002: conformance-only preparation; never a reward registration.
pub const TAG_REGISTER_CONFORMANCE_CLASS_V1: u8 = 21;
pub const TAG_BIND_MODEL_ARTIFACT_V2: u8 = 22;
pub const TAG_REFUTE_MODEL_ARTIFACT_V2: u8 = 23;
pub const TAG_REGISTER_MODEL_CONFORMANCE_CLASS_V2: u8 = 24;
pub const TAG_COMPLETE_MODEL_CONFORMANCE_V2: u8 = 25;
pub const TAG_POST_MODEL_VECTOR_V2: u8 = 26;
pub const TAG_REFUTE_MODEL_VECTOR_V2: u8 = 27;
pub const TAG_DEMAND_MODEL_VECTOR_V2: u8 = 28;
pub const TAG_ANSWER_MODEL_VECTOR_V2: u8 = 29;
pub const TAG_SETTLE_MODEL_VECTOR_DEMAND_V2: u8 = 30;
pub const TAG_POST_FLAT_MODEL_VECTOR_V3: u8 = 31;
pub const TAG_REFUTE_FLAT_MODEL_VECTOR_V3: u8 = 32;
pub const TAG_DEMAND_FLAT_MODEL_VECTOR_V3: u8 = 33;
pub const TAG_ANSWER_FLAT_MODEL_VECTOR_V3: u8 = 34;
pub const TAG_SETTLE_FLAT_MODEL_VECTOR_V3: u8 = 35;
// 36–37: RFC-0002 P0 (lane RFC02-T, 2026-10-11): the protected refutation of a candidate statement, past
// `palw_model_refutation_seal_v1` (consumer-owned; the bare ledger refuses both by rule).
/// A hidden, actor-bound refutation seal in one of the signer's slots.
pub const TAG_SEAL_MODEL_REFUTATION_V4: u8 = 36;
/// The reveal of a sealed refutation (kind, target, salt, exact proof bytes).
pub const TAG_REVEAL_MODEL_REFUTATION_V4: u8 = 37;
// 16–18 are K2S's (K2-TIR-v4); 21–37 are RFC-02's (conformance classes, model artifact binding / conformance, model refutation
// seal v4) on the integration line, 38/39 are G14R's verifier pay (39 `AttestDrawnCheckV1` landed 2026-10-15; 38 reserved); registry
// `remaining-rfc-integration-matrix.md` §2.
// 41–60: RFC-0002 codex final (`codex/rfc02-review-completion`): protected flat candidate v6 (41–44), complete-scope mixed
// candidate v7 (45–52), ordinary-point mixed candidate v9 (53–60). Codex numbered them 36–55 on its branch; 36/37 are
// RFC02-T's, 38/39 G14R's (reserved) and 40 RFC-7's, so they moved +5 at the `rfc02/merge-final` merge (2026-10-11).
pub const TAG_POST_FLAT_MODEL_VECTOR_V6: u8 = 41;
pub const TAG_SEAL_FLAT_MODEL_PROOF_V6: u8 = 42;
pub const TAG_REVEAL_FLAT_MODEL_PROOF_V6: u8 = 43;
pub const TAG_EXPIRE_FLAT_MODEL_SEAL_V6: u8 = 44;
pub const TAG_REGISTER_FLAT_MODEL_CANDIDATE_V7: u8 = 45;
pub const TAG_POST_FLAT_MODEL_VECTOR_V7: u8 = 46;
pub const TAG_SEAL_FLAT_MODEL_PROOF_V7: u8 = 47;
pub const TAG_REVEAL_FLAT_MODEL_PROOF_V7: u8 = 48;
pub const TAG_EXPIRE_FLAT_MODEL_SEAL_V7: u8 = 49;
pub const TAG_DEMAND_FLAT_MODEL_UNIT_V7: u8 = 50;
pub const TAG_ANSWER_FLAT_MODEL_UNIT_V7: u8 = 51;
pub const TAG_SETTLE_FLAT_MODEL_DEMAND_V7: u8 = 52;
pub const TAG_REGISTER_FLAT_MODEL_CANDIDATE_V9: u8 = 53;
pub const TAG_POST_FLAT_MODEL_VECTOR_V9: u8 = 54;
pub const TAG_SEAL_FLAT_MODEL_PROOF_V9: u8 = 55;
pub const TAG_REVEAL_FLAT_MODEL_PROOF_V9: u8 = 56;
pub const TAG_EXPIRE_FLAT_MODEL_SEAL_V9: u8 = 57;
pub const TAG_DEMAND_FLAT_MODEL_UNIT_V9: u8 = 58;
pub const TAG_ANSWER_FLAT_MODEL_UNIT_V9: u8 = 59;
pub const TAG_SETTLE_FLAT_MODEL_DEMAND_V9: u8 = 60;
/// OPV-BOOT GAP-B1a: a claim reveal that carries its seal's salt (claim seal v2), past `palw_panel_free_v1`.
pub const TAG_COMMIT_CLAIM_SALTED_V1: u8 = 20;
/// Per-attempt beacon source opt-in (codex RFC-7, lane R6781). Refused without its own consumer-supplied fence and authenticated
/// target. codex used 21 (RFC-02's `RegisterConformanceClass` on the integration line); renumbered to 40 at the merge.
pub const TAG_OPT_IN_BEACON_SOURCE_V1: u8 = 40;
pub const MAX_OPT_IN_BEACON_SOURCE_BYTES_V1: usize = 384;
// 61–62: codex RFC-7 final prepaid source fees (lane R6781-F merge, 2026-10-12): codex numbered them 22 / 23 (RFC-02's here).
pub const TAG_FUND_BEACON_SOURCE_FEE_V1: u8 = 61;
pub const TAG_OPT_IN_PAID_BEACON_SOURCE_V1: u8 = 62;
/// Native-owned, signed Panel sponsor permit and atomic funding. Renumbered 63 → 70 and 64 → 71 (2026-10-15, lead decision): the
/// RFC02 branch owns 63 (`FundedExecutionV11`) and 64 (`StageExecutionV14`); neither number was ever live on this branch.
pub const TAG_FUND_PANEL_BEACON_SOURCE_FEE_V2: u8 = 70;
pub const TAG_PAID_CLAIM_V2: u8 = 71;
pub const TAG_ENROLL_PANEL_CONSUMER_LIFETIME_V2: u8 = 65;
pub const TAG_AUTHORIZE_PANEL_LIFETIME_V2: u8 = 66;
/// G14R's reserved verifier-pay slot 39 (M\*-49, `palw_verifier_pay_v1`): a drawn slot holder's attestation of one claim's check.
pub const TAG_ATTEST_DRAWN_CHECK_V1: u8 = 39;
// 67–69: RFC07/RFC08 signed economic contracts (2026-10-15, inside the RFC07 reservation 65–73; consumer-owned, each past its own
// dormant fence): the model-epoch verification attestation, the consented horizon extension and the minimum-price contract.
pub const TAG_ATTEST_MODEL_VERIFICATION_V1: u8 = 67;
pub const TAG_CONSENT_HORIZON_EXTENSION_V1: u8 = 68;
pub const TAG_SIGN_MINIMUM_PRICE_CONTRACT_V1: u8 = 69;
// 73: N3's BootstrapExact source contribution (2026-10-15 round 3; the top of the RFC07 reservation 65–73, consumer-owned past
// `palw_beacon_bootstrap_v1`). 70/71: the renumbered Panel sponsor funding and paid ClaimV2 (above). 72 stays unallocated; 63/64
// belong to the RFC02 branch.
pub const TAG_BOOTSTRAP_EXACT_V1: u8 = 73;
// 74–75: RFC-0017's model eligibility exam commit and drawn-verifier attestation (2026-10-15; consumer-owned past
// `palw_model_eligibility_v1`). Grepped free on `integ/rfc02-rfc06-12-g14` and `pre` before allocating; 72 stays unallocated.
pub const TAG_COMMIT_ELIGIBILITY_EXAM_V1: u8 = 74;
pub const TAG_ATTEST_ELIGIBILITY_EXAM_V1: u8 = 75;
/// version, tag, verifier, claim.
pub const MAX_ATTEST_DRAWN_CHECK_BYTES_V1: usize = 1 + 1 + 64 + 64;
pub const MAX_FUND_PANEL_BEACON_SOURCE_FEE_BYTES_V2: usize = 288;
pub const MAX_FUND_BEACON_SOURCE_FEE_BYTES_V1: usize = 256;
pub const MAX_OPT_IN_PAID_BEACON_SOURCE_BYTES_V1: usize = 448;
/// Explicit funded execution grammar, separately fenced by the native consumer.
pub const TAG_FUNDED_EXECUTION_V11: u8 = 63;
pub const MAX_FUNDED_EXECUTION_BYTES_V11: usize = (1 << 20) + (16 << 10);
pub const TAG_STAGE_EXECUTION_V14: u8 = 64;
pub const MAX_STAGE_EXECUTION_BYTES_V14: usize = 2 << 20;
/// Only the explicitly versioned Master32 invocation actions 34..=36 use this
/// larger envelope (bounded public upstream material plus the old typed body).
/// The frozen stage actions 0..=33 retain their original 2MiB ceiling.
pub const MAX_STAGE_INVOCATION_BYTES_V32: usize = (4 << 20) + (256 << 10) + 8192 + 7;
/// Version32 compact whole Commit body is at most 9 MiB. Its paid part
/// transport uses 143-byte metadata and at most 32 KiB of body per carrier.
/// These are new inner64 actions; no old action acquires the larger ceiling.
pub const MAX_STAGE_COMPACT_COMMIT_BYTES_V32: usize = (9 << 20) + 7;
pub const MAX_STAGE_COMMIT_PART_BYTES_V32: usize = (32 << 10) + 143 + 7;
/// Owning39 source body; large bodies travel through its separately paid source upload.
pub const MAX_STAGE_SOURCE_BYTES_V39: usize = crate::flat_source39::SOURCE39_MAX_BYTES + 7;

fn stage_action_cap(action: u8) -> usize {
    match action {
        34..=36 => MAX_STAGE_INVOCATION_BYTES_V32,
        37 => 64 + 7,
        38 => MAX_STAGE_COMPACT_COMMIT_BYTES_V32,
        39 => MAX_STAGE_COMMIT_PART_BYTES_V32,
        40 => 130 + 7,
        41 => MAX_STAGE_SOURCE_BYTES_V39,
        42 => 262 + 7,
        71 => 67 + 7,
        72 => 64 + 7,
        65 => (46 << 20) + 7,
        57 | 67 => 256 + 7,
        50 => (32 << 10) + 143 + 7,
        58 | 68 => (32 << 10) + 72 + 7,
        59 | 60 | 66 | 69 | 70 => 64 + 7,
        _ => MAX_STAGE_EXECUTION_BYTES_V14,
    }
}

fn stage_action_extent(action: u8, encoded: usize) -> bool {
    encoded <= stage_action_cap(action)
        && match action {
            37 => encoded == 71,
            39 => encoded >= 143 + 7,
            40 => encoded == 130 + 7,
            42 => encoded == 262 + 7,
            71 => encoded == 67 + 7,
            72 => encoded == 64 + 7,
            50 => encoded >= 143 + 7,
            58 | 68 => encoded >= 72 + 7,
            59 | 60 | 66 | 69 | 70 => encoded == 64 + 7,
            _ => true,
        }
}

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

/// **The commit a salted reveal carries** (inner kind 20): the commit objects' own fields, under their own discriminants (5, 6 and
/// 19, the commit tags — pinned by a test). A separate,
/// non-recursive enum — a reveal can never carry another reveal. (K2S's segmented commit is 16.)
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SaltedCommitV1 {
    /// A single-program claim, as `CommitClaim` carries it.
    Claim { claim: KernelClaimV1, evidence: VerificationEvidenceV1, commitments: Vec<Vec<Vec<Digest>>> } = 5,
    /// A pipeline claim, as `CommitPipelineClaim` carries it.
    Pipeline { claim: PipelineClaimV1, evidence: PipelineEvidenceV1, stages: Vec<StageCommitmentsV1> } = 6,
    /// RFC-0004 Part II: a typed-root claim, as `Spec { CommitClaim }` (kind 19) carries it — typed classes are OPV-only, so past
    /// the fence every typed claim reveals salted. It needs `palw_typed_roots_v1` too (the ledger's schedule, the node's gate).
    Spec { claim: crate::spec::SpecClaimV1 } = 19,
    /// K2-TIR-v4/v5: a segmented claim, as `CommitSegmentedClaim` (kind 16) carries it — its classes are OPV-only, so past the fence
    /// every segmented claim reveals salted.
    Segmented { claim: KernelClaimV1, evidence: crate::seg::SegmentedEvidenceV2, segment_roots: Vec<Digest> } = 16,
}

impl SaltedCommitV1 {
    /// The kind the commit would have been carried as without its salt (its ceiling and its name).
    pub const fn commit_tag(&self) -> u8 {
        match self {
            Self::Claim { .. } => TAG_COMMIT_CLAIM_V1,
            Self::Pipeline { .. } => TAG_COMMIT_PIPELINE_CLAIM_V1,
            Self::Spec { .. } => TAG_SPEC_V1,
            Self::Segmented { .. } => TAG_COMMIT_SEGMENTED_CLAIM_V1,
        }
    }

    /// The producer bond the commit names (the reveal's signer).
    pub fn producer(&self) -> Digest {
        match self {
            Self::Claim { claim, .. } => claim.producer_bond,
            Self::Pipeline { claim, .. } => claim.producer_bond,
            Self::Spec { claim } => claim.producer(),
            Self::Segmented { claim, .. } => claim.producer_bond,
        }
    }
}

/// **The public objects of the kernel route.** Every one is signed by a bond (see [`AuthV1`]); none mints or moves money by
/// itself — payments are the ledger's [`crate::settle::SettlementInstructionV1`] receipts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum KernelRouteObjectV1 {
    PostFlatModelVectorV9 {
        class: Digest,
        binding: Digest,
        post: Vec<u8>,
    } = 54,
    PostFlatModelVectorV7 {
        class: Digest,
        binding: Digest,
        post: Vec<u8>,
    } = 46,
    SealFlatModelProofV9 {
        vector: Digest,
        seal: Digest,
    } = 55,
    SealFlatModelProofV7 {
        vector: Digest,
        seal: Digest,
    } = 47,
    RevealFlatModelProofV9 {
        vector: Digest,
        salt: Digest,
        proof: Vec<u8>,
    } = 56,
    RevealFlatModelProofV7 {
        vector: Digest,
        salt: Digest,
        proof: Vec<u8>,
    } = 48,
    ExpireFlatModelSealV9 {
        vector: Digest,
        actor: Digest,
    } = 57,
    ExpireFlatModelSealV7 {
        vector: Digest,
        actor: Digest,
    } = 49,
    DemandFlatModelUnitV9 {
        vector: Digest,
        slot: u8,
        position: u32,
        occurrence: u16,
        node: u16,
        kind: u8,
        index: u64,
    } = 58,
    DemandFlatModelUnitV7 {
        vector: Digest,
        slot: u8,
        position: u32,
        occurrence: u16,
        node: u16,
        kind: u8,
        index: u64,
    } = 50,
    AnswerFlatModelUnitV9 {
        demand: Digest,
        answer: Vec<u8>,
    } = 59,
    AnswerFlatModelUnitV7 {
        demand: Digest,
        answer: Vec<u8>,
    } = 51,
    SettleFlatModelDemandV9 {
        demand: Digest,
    } = 60,
    SettleFlatModelDemandV7 {
        demand: Digest,
    } = 52,
    RegisterFlatModelCandidateV9 {
        binding: Digest,
        descriptor: Digest,
        program_bytes: Vec<u8>,
        param_commitments: ParamCommitmentsV1,
        max_positions: u32,
        scope_root: Digest,
        replay_root: Digest,
        resource_root: Digest,
    } = 53,
    RegisterFlatModelCandidateV7 {
        binding: Digest,
        descriptor: Digest,
        program_bytes: Vec<u8>,
        param_commitments: ParamCommitmentsV1,
        max_positions: u32,
        scope_root: Digest,
        replay_root: Digest,
    } = 45,
    /// Protected statement: the unsealed v3 refutation cannot target its identity.
    PostFlatModelVectorV6 {
        class: Digest,
        binding: Digest,
        post: Vec<u8>,
    } = 41,
    /// The actor is the authenticated signer, never a carrier-selected beneficiary.
    SealFlatModelProofV6 {
        vector: Digest,
        seal: Digest,
    } = 42,
    RevealFlatModelProofV6 {
        vector: Digest,
        salt: Digest,
        proof: Vec<u8>,
    } = 43,
    /// Anyone bonded may close one expired fixed row; no global scan is needed.
    ExpireFlatModelSealV6 {
        vector: Digest,
        actor: Digest,
    } = 44,
    /// Node commitment or flat-v5 hash leaf only; four slots shared with v2.
    DemandFlatModelVectorV3 {
        vector: Digest,
        slot: u8,
        position: u32,
        occurrence: u16,
        node: u16,
        leaf: Option<u64>,
    } = 33,
    AnswerFlatModelVectorV3 {
        demand: Digest,
        answer: Vec<u8>,
    } = 34,
    SettleFlatModelVectorDemandV3 {
        demand: Digest,
    } = 35,
    /// Explicit flat-v5 profile and resource-root statement; non-reward candidate trace.
    PostFlatModelVectorV3 {
        class: Digest,
        binding: Digest,
        post: Vec<u8>,
    } = 31,
    /// Consumer-owned bounded flat-v5 leaf/whole/header/decode fault.
    RefuteFlatModelVectorV3 {
        vector: Digest,
        proof: Vec<u8>,
    } = 32,
    /// Claim-specific trace tile (axis 0/1), or node commitment (axis 2).
    /// No param or model-file unit is expressible. Slots belong to the signer.
    DemandModelVectorV2 {
        vector: Digest,
        slot: u8,
        position: u32,
        occurrence: u16,
        node: u16,
        axis: u8,
        line: u64,
        tile: u64,
    } = 28,
    AnswerModelVectorV2 {
        demand: Digest,
        answer: Vec<u8>,
    } = 29,
    /// Permissionless explicit close; no network-wide deadline scan.
    SettleModelVectorDemandV2 {
        demand: Digest,
    } = 30,
    /// Non-reward conformance trace of a scoped candidate, authenticated by the consumer.
    PostModelVectorV2 {
        class: Digest,
        binding: Digest,
        post: Vec<u8>,
    } = 26,
    /// Bounded public element/type/decode court of that trace; needs no execution Final.
    RefuteModelVectorV2 {
        vector: Digest,
        proof: Vec<u8>,
    } = 27,
    /// Consumer-owned exact finite-domain conformance over an explicit model statement.
    CompleteModelConformanceV2 {
        class: Digest,
        binding: Digest,
        proof: Vec<u8>,
    } = 25,
    /// Consumer-owned, bonded statement: exact descriptor/program, model-only inventory and PC.
    /// Its signer owns liability, never execution or reward priority.
    BindModelArtifactV2 {
        descriptor: Digest,
        program_bytes: Vec<u8>,
        model_inventory_root: Digest,
        param_commitments: ParamCommitmentsV1,
    } = 22,
    /// Consumer-owned bounded proof bytes. The consumer charges shared court work before decoding.
    RefuteModelArtifactV2 {
        binding: Digest,
        proof: Vec<u8>,
    } = 23,
    /// Prepare metadata over an explicit matured model statement. The consumer authenticates
    /// the scope, then delegates the ordinary conformance-only admission to the ledger.
    RegisterModelConformanceClassV2 {
        binding: Digest,
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        param_commitments: ParamCommitmentsV1,
    } = 24,
    /// Prepare the OPV class identity for onboarding under the active kernel and OPV resource policy.
    /// Stored separately: no job, claim, reward, Final or beacon rights. Ordinary registration still needs derived eligibility.
    RegisterConformanceClass {
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        param_commitments: ParamCommitmentsV1,
    } = 21,
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
    /// **RFC-0002 P0: a hidden refutation seal** (consumer-owned, past `palw_model_refutation_seal_v1`):
    /// `seal = H(kind ‖ target ‖ signer ‖ salt ‖ H(proof))` — the actor is the authenticated signer, never a field.
    SealModelRefutationV4 {
        slot: u8,
        seal: Digest,
    } = 36,
    /// **RFC-0002 P0: the reveal of a sealed refutation** — admissible only with the signer's own mature seal of these exact bytes;
    /// `kind` names the candidate court (1 artifact binding v2, 2 model vector v2, 3 flat model vector v3).
    RevealModelRefutationV4 {
        slot: u8,
        kind: u8,
        target: Digest,
        salt: Digest,
        proof: Vec<u8>,
    } = 37,
    /// The producer signs the exact target and terms, in addition to its ordinary salted race seal.
    /// Each target reserves a fresh source deposit. No transaction can supply the target inventory or fence.
    OptInBeaconSource {
        producer: Digest,
        job: Digest,
        seal: Digest,
        context: Digest,
        terms: Digest,
    } = 40,
    /// The authorized sponsor signs exact prepaid fee terms. Separate fee fence, no implied source approval.
    /// (codex RFC-7 final numbered it 22; renumbered to 61 at the R6781-F merge.)
    FundBeaconSourceFee {
        terms: crate::beacon_source_fee::BeaconSourceFeeTermsV1,
    } = 61,
    /// The source producer signs the prepaid contract's exact fee as well as its original source terms.
    /// (codex RFC-7 final numbered it 23; renumbered to 62 at the R6781-F merge.)
    OptInPaidBeaconSource {
        producer: Digest,
        job: Digest,
        seal: Digest,
        context: Digest,
        source_terms: Digest,
        fee_terms: Digest,
    } = 62,
    /// Native-owned nonce, original escrow, quotas, courts and terminal credits.
    /// A bare ledger cannot apply these actions or trust a caller's settlement.
    FundedExecutionV11 {
        action: u8,
        body: Vec<u8>,
    } = 63,
    /// Explicit typed external-stage namespace. Source/class/funded actions
    /// require their independent native14 policy and never use text11 grants.
    StageExecutionV14 {
        action: u8,
        body: Vec<u8>,
    } = 64,
    /// Native-owned Panel sponsor permit and funding in one signed object.
    /// Bare ledgers refuse this variant: only the authenticated epoch fold may authorize it.
    FundPanelBeaconSourceFeeV2 {
        epoch: u64,
        terms: crate::beacon_source_fee::BeaconSourceFeeTermsV1,
    } = 70,
    PaidClaimV2 {
        object: crate::claim_paid_v2::PaidClaimObjectV2,
    } = 71,
    EnrollPanelConsumerLifetimeV2 {
        terms: crate::panel_lifetime_v2::PanelConsumerLifetimeTermsV2,
    } = 65,
    AuthorizePanelLifetimeV2 {
        authorization: crate::panel_lifetime_v2::PanelLifetimeAuthorizationV2,
    } = 66,
    /// **M\*-49 (`palw_verifier_pay_v1`, G14R's reserved 39)**: a drawn slot holder attests its check of `claim`; the ledger pays its
    /// unpaid slots `check_fee` out of the poster's escrow before the attestation deadline. Signed by `verifier`.
    AttestDrawnCheckV1 {
        verifier: Digest,
        claim: Digest,
    } = 39,
    /// RFC07 R7-ECON: a drawn verifier's attestation for a model-epoch verification tally (consumer-owned).
    AttestModelVerificationV1 {
        attestation: crate::econ_contracts_v1::ModelVerificationAttestationV1,
    } = 67,
    /// RFC08 R8-WINDOW: one affected party's consent to a live dependency horizon extension (consumer-owned).
    ConsentHorizonExtensionV1 {
        terms: crate::econ_contracts_v1::HorizonExtensionTermsV1,
    } = 68,
    /// RFC08 R8-ECON: one party's signature on a worker/poster minimum-price contract (consumer-owned).
    SignMinimumPriceContractV1 {
        contract: crate::econ_contracts_v1::MinimumPriceContractTermsV1,
    } = 69,
    /// N3 (`palw_beacon_bootstrap_v1`): one contributor's seal or reveal in a BootstrapExact source round (consumer-owned).
    BootstrapExactV1 {
        contribution: crate::beacon_bootstrap_v1::BootstrapExactContributionV1,
    } = 73,
    /// RFC-0017 (`palw_model_eligibility_v1`): commit an eligibility exam of one model before its draw exists (consumer-owned).
    CommitEligibilityExamV1 {
        commit: crate::model_eligibility_v1::EligibilityExamCommitV1,
    } = 74,
    /// RFC-0017 (`palw_model_eligibility_v1`): one drawn verifier's attestation for one eligibility exam (consumer-owned).
    AttestEligibilityExamV1 {
        attestation: crate::model_eligibility_v1::EligibilityAttestationV1,
    } = 75,
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
/// Borrow the two independently fenced native action frames before allocating
/// their body vectors. Callers still authenticate the exact outer bytes and
/// native owns any expensive inner decode after its original paid predebit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedNativeActionV1<'a> {
    pub tag: u8,
    pub action: u8,
    pub body: &'a [u8],
}
pub fn borrowed_native_action_v1(bytes: &[u8]) -> Result<Option<BorrowedNativeActionV1<'_>>, KernelRefusalV1> {
    let Some(tag) = bytes.get(1).copied() else {
        return Ok(None);
    };
    let (cap, last) = match tag {
        TAG_FUNDED_EXECUTION_V11 => (MAX_FUNDED_EXECUTION_BYTES_V11, 46),
        TAG_STAGE_EXECUTION_V14 => (stage_action_cap(bytes.get(2).copied().unwrap_or(0)), 79),
        _ => return Ok(None),
    };
    let bad = |why: &str| KernelRefusalV1::new("NativeAction", RefusalKindV1::Malformed, why);
    if bytes.first() != Some(&KERNEL_ROUTE_VERSION_V1) || bytes.len() > cap {
        return Err(bad("native route version/byte cap"));
    }
    let action = *bytes.get(2).ok_or_else(|| bad("native action absent"))?;
    if tag == TAG_STAGE_EXECUTION_V14 && !stage_action_extent(action, bytes.len()) {
        return Err(bad("Master32 action-specific bounded extent differs"));
    }
    if tag == TAG_FUNDED_EXECUTION_V11 && matches!(action, 41..=44) && bytes.len() > (32 << 10) + 79 {
        return Err(bad("initial31 complete bounded wire packet cap"));
    }
    let raw = bytes.get(3..7).ok_or_else(|| bad("native action count absent"))?;
    let n = u32::from_le_bytes(raw.try_into().unwrap()) as usize;
    if action > last || n != bytes.len().saturating_sub(7) {
        return Err(bad("native action exact bounded count"));
    }
    Ok(Some(BorrowedNativeActionV1 { tag, action, body: &bytes[7..] }))
}

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
            Self::PostFlatModelVectorV9 { .. } => TAG_POST_FLAT_MODEL_VECTOR_V9,
            Self::PostFlatModelVectorV7 { .. } => TAG_POST_FLAT_MODEL_VECTOR_V7,
            Self::SealFlatModelProofV9 { .. } => TAG_SEAL_FLAT_MODEL_PROOF_V9,
            Self::SealFlatModelProofV7 { .. } => TAG_SEAL_FLAT_MODEL_PROOF_V7,
            Self::RevealFlatModelProofV9 { .. } => TAG_REVEAL_FLAT_MODEL_PROOF_V9,
            Self::RevealFlatModelProofV7 { .. } => TAG_REVEAL_FLAT_MODEL_PROOF_V7,
            Self::ExpireFlatModelSealV9 { .. } => TAG_EXPIRE_FLAT_MODEL_SEAL_V9,
            Self::ExpireFlatModelSealV7 { .. } => TAG_EXPIRE_FLAT_MODEL_SEAL_V7,
            Self::DemandFlatModelUnitV9 { .. } => TAG_DEMAND_FLAT_MODEL_UNIT_V9,
            Self::DemandFlatModelUnitV7 { .. } => TAG_DEMAND_FLAT_MODEL_UNIT_V7,
            Self::AnswerFlatModelUnitV9 { .. } => TAG_ANSWER_FLAT_MODEL_UNIT_V9,
            Self::AnswerFlatModelUnitV7 { .. } => TAG_ANSWER_FLAT_MODEL_UNIT_V7,
            Self::SettleFlatModelDemandV9 { .. } => TAG_SETTLE_FLAT_MODEL_DEMAND_V9,
            Self::SettleFlatModelDemandV7 { .. } => TAG_SETTLE_FLAT_MODEL_DEMAND_V7,
            Self::RegisterFlatModelCandidateV9 { .. } => TAG_REGISTER_FLAT_MODEL_CANDIDATE_V9,
            Self::RegisterFlatModelCandidateV7 { .. } => TAG_REGISTER_FLAT_MODEL_CANDIDATE_V7,
            Self::PostFlatModelVectorV6 { .. } => TAG_POST_FLAT_MODEL_VECTOR_V6,
            Self::SealFlatModelProofV6 { .. } => TAG_SEAL_FLAT_MODEL_PROOF_V6,
            Self::RevealFlatModelProofV6 { .. } => TAG_REVEAL_FLAT_MODEL_PROOF_V6,
            Self::ExpireFlatModelSealV6 { .. } => TAG_EXPIRE_FLAT_MODEL_SEAL_V6,
            Self::DemandFlatModelVectorV3 { .. } => TAG_DEMAND_FLAT_MODEL_VECTOR_V3,
            Self::AnswerFlatModelVectorV3 { .. } => TAG_ANSWER_FLAT_MODEL_VECTOR_V3,
            Self::SettleFlatModelVectorDemandV3 { .. } => TAG_SETTLE_FLAT_MODEL_VECTOR_V3,
            Self::PostFlatModelVectorV3 { .. } => TAG_POST_FLAT_MODEL_VECTOR_V3,
            Self::RefuteFlatModelVectorV3 { .. } => TAG_REFUTE_FLAT_MODEL_VECTOR_V3,
            Self::DemandModelVectorV2 { .. } => TAG_DEMAND_MODEL_VECTOR_V2,
            Self::AnswerModelVectorV2 { .. } => TAG_ANSWER_MODEL_VECTOR_V2,
            Self::SettleModelVectorDemandV2 { .. } => TAG_SETTLE_MODEL_VECTOR_DEMAND_V2,
            Self::PostModelVectorV2 { .. } => TAG_POST_MODEL_VECTOR_V2,
            Self::RefuteModelVectorV2 { .. } => TAG_REFUTE_MODEL_VECTOR_V2,
            Self::CompleteModelConformanceV2 { .. } => TAG_COMPLETE_MODEL_CONFORMANCE_V2,
            Self::BindModelArtifactV2 { .. } => TAG_BIND_MODEL_ARTIFACT_V2,
            Self::RefuteModelArtifactV2 { .. } => TAG_REFUTE_MODEL_ARTIFACT_V2,
            Self::RegisterModelConformanceClassV2 { .. } => TAG_REGISTER_MODEL_CONFORMANCE_CLASS_V2,
            Self::RegisterConformanceClass { .. } => TAG_REGISTER_CONFORMANCE_CLASS_V1,
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
            Self::OptInBeaconSource { .. } => TAG_OPT_IN_BEACON_SOURCE_V1,
            Self::FundBeaconSourceFee { .. } => TAG_FUND_BEACON_SOURCE_FEE_V1,
            Self::FundPanelBeaconSourceFeeV2 { .. } => TAG_FUND_PANEL_BEACON_SOURCE_FEE_V2,
            Self::PaidClaimV2 { .. } => TAG_PAID_CLAIM_V2,
            Self::EnrollPanelConsumerLifetimeV2 { .. } => TAG_ENROLL_PANEL_CONSUMER_LIFETIME_V2,
            Self::AuthorizePanelLifetimeV2 { .. } => TAG_AUTHORIZE_PANEL_LIFETIME_V2,
            Self::AttestDrawnCheckV1 { .. } => TAG_ATTEST_DRAWN_CHECK_V1,
            Self::AttestModelVerificationV1 { .. } => TAG_ATTEST_MODEL_VERIFICATION_V1,
            Self::ConsentHorizonExtensionV1 { .. } => TAG_CONSENT_HORIZON_EXTENSION_V1,
            Self::SignMinimumPriceContractV1 { .. } => TAG_SIGN_MINIMUM_PRICE_CONTRACT_V1,
            Self::BootstrapExactV1 { .. } => TAG_BOOTSTRAP_EXACT_V1,
            Self::CommitEligibilityExamV1 { .. } => TAG_COMMIT_ELIGIBILITY_EXAM_V1,
            Self::AttestEligibilityExamV1 { .. } => TAG_ATTEST_ELIGIBILITY_EXAM_V1,
            Self::OptInPaidBeaconSource { .. } => TAG_OPT_IN_PAID_BEACON_SOURCE_V1,
            Self::FundedExecutionV11 { .. } => TAG_FUNDED_EXECUTION_V11,
            Self::StageExecutionV14 { .. } => TAG_STAGE_EXECUTION_V14,
            Self::CommitSegmentedClaim { .. } => TAG_COMMIT_SEGMENTED_CLAIM_V1,
            Self::PostTiledJob { .. } => TAG_POST_TILED_JOB_V1,
            Self::PostPromptTile { .. } => TAG_POST_PROMPT_TILE_V1,
            Self::Spec { .. } => TAG_SPEC_V1,
            Self::SealModelRefutationV4 { .. } => TAG_SEAL_MODEL_REFUTATION_V4,
            Self::RevealModelRefutationV4 { .. } => TAG_REVEAL_MODEL_REFUTATION_V4,
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
        if tag == TAG_FUNDED_EXECUTION_V11 {
            let count = rest.get(2..6).ok_or_else(|| bad(RefusalKindV1::Malformed, "truncated funded action frame".into()))?;
            // 45/46 = typed window class v37 register/attach (RFC02 §II.14.60), judged only past its own fence.
            if rest.get(1).is_none_or(|a| *a > 46)
                || rest.get(1).is_some_and(|a| matches!(*a, 41..=44)) && bytes.len() > (32 << 10) + 79
                || u32::from_le_bytes(count.try_into().unwrap()) as usize != rest.len().saturating_sub(6)
            {
                return Err(bad(RefusalKindV1::Malformed, "funded action/count differs from exact bounded carrier".into()));
            }
        }
        if tag == TAG_STAGE_EXECUTION_V14 {
            let count = rest.get(2..6).ok_or_else(|| bad(RefusalKindV1::Malformed, "truncated stage14 action frame".into()))?;
            if rest.get(1).is_none_or(|a| *a > 79 || !stage_action_extent(*a, bytes.len()))
                || u32::from_le_bytes(count.try_into().unwrap()) as usize != rest.len().saturating_sub(6)
            {
                return Err(bad(RefusalKindV1::Malformed, "stage14 action/count differs from exact bounded carrier".into()));
            }
        }
        if tag == TAG_REGISTER_FLAT_MODEL_CANDIDATE_V7 {
            bounded_flat_candidate_v7(rest).map_err(|e| bad(RefusalKindV1::Malformed, e))?;
        }
        if tag == TAG_REGISTER_FLAT_MODEL_CANDIDATE_V9 {
            bounded_flat_candidate_v9(rest).map_err(|e| bad(RefusalKindV1::Malformed, e))?;
        }
        if matches!(
            tag,
            TAG_POST_FLAT_MODEL_VECTOR_V7
                | TAG_REVEAL_FLAT_MODEL_PROOF_V7
                | TAG_ANSWER_FLAT_MODEL_UNIT_V7
                | TAG_POST_FLAT_MODEL_VECTOR_V9
                | TAG_REVEAL_FLAT_MODEL_PROOF_V9
                | TAG_ANSWER_FLAT_MODEL_UNIT_V9
        ) {
            let prefix = if matches!(tag, TAG_ANSWER_FLAT_MODEL_UNIT_V7 | TAG_ANSWER_FLAT_MODEL_UNIT_V9) { 65 } else { 129 };
            let count = rest
                .get(prefix..prefix + 4)
                .ok_or_else(|| bad(RefusalKindV1::Malformed, "truncated mixed-court blob count".into()))?;
            if u32::from_le_bytes(count.try_into().unwrap()) as usize != rest.len().saturating_sub(prefix + 4) {
                return Err(bad(RefusalKindV1::Malformed, "mixed-court blob count differs from actual carrier".into()));
            }
        }
        if tag == TAG_DEMAND_FLAT_MODEL_UNIT_V7
            && (rest.get(74).is_none_or(|kind| *kind > 2) || (rest[74] == 0 && rest.get(75..83) != Some(&[0u8; 8][..])))
        {
            return Err(bad(RefusalKindV1::Malformed, "invalid mixed-court public unit kind/index".into()));
        }
        if tag == TAG_DEMAND_FLAT_MODEL_UNIT_V9
            && (rest.get(74).is_none_or(|kind| *kind > 3) || (matches!(rest[74], 0 | 3) && rest.get(75..83) != Some(&[0u8; 8][..])))
        {
            return Err(bad(RefusalKindV1::Malformed, "invalid typed9 public unit kind/index".into()));
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

/// Inspect all v7 allocation counts against the actual carrier before the
/// derived Borsh decoder can allocate a program or parameter map. Old kinds
/// retain their frozen decoder. This does not execute or hash model material.
fn bounded_flat_candidate_v7(mut bytes: &[u8]) -> Result<(), String> {
    fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
        if b.len() < n {
            return Err("truncated flat-v7 candidate".into());
        }
        let (head, tail) = b.split_at(n);
        *b = tail;
        Ok(head)
    }
    fn count(b: &mut &[u8]) -> Result<usize, String> {
        Ok(u32::from_le_bytes(take(b, 4)?.try_into().unwrap()) as usize)
    }
    take(&mut bytes, 1 + 64 + 64)?;
    let n = count(&mut bytes)?;
    if n > misaka_palw_tir::program::MAX_PROGRAM_BYTES {
        return Err("oversized flat-v7 canonical program".into());
    }
    take(&mut bytes, n)?;
    let instances = count(&mut bytes)?;
    if instances > bytes.len().saturating_sub(4 + 64 + 64) / 67 {
        return Err("flat-v7 parameter count exceeds its actual carrier".into());
    }
    for _ in 0..instances {
        take(&mut bytes, 2)?;
        match take(&mut bytes, 1)?[0] {
            0 => {}
            1 => {
                take(&mut bytes, 2)?;
            }
            _ => return Err("invalid flat-v7 parameter layer option".into()),
        }
        take(&mut bytes, 64)?;
    }
    take(&mut bytes, 4 + 64 + 64)?;
    if !bytes.is_empty() {
        return Err("trailing flat-v7 candidate bytes".into());
    }
    Ok(())
}

fn bounded_flat_candidate_v9(mut bytes: &[u8]) -> Result<(), String> {
    fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
        if b.len() < n {
            return Err("truncated flat-v9 candidate".into());
        }
        let (head, tail) = b.split_at(n);
        *b = tail;
        Ok(head)
    }
    fn count(b: &mut &[u8]) -> Result<usize, String> {
        Ok(u32::from_le_bytes(take(b, 4)?.try_into().unwrap()) as usize)
    }
    take(&mut bytes, 1 + 64 + 64)?;
    let n = count(&mut bytes)?;
    if n > misaka_palw_tir::program::MAX_PROGRAM_BYTES {
        return Err("oversized flat-v9 canonical program".into());
    }
    take(&mut bytes, n)?;
    let instances = count(&mut bytes)?;
    if instances > bytes.len().saturating_sub(4 + 64 + 64 + 64) / 67 {
        return Err("flat-v9 parameter count exceeds its actual carrier".into());
    }
    for _ in 0..instances {
        take(&mut bytes, 2)?;
        match take(&mut bytes, 1)?[0] {
            0 => {}
            1 => {
                take(&mut bytes, 2)?;
            }
            _ => return Err("invalid flat-v9 parameter layer option".into()),
        }
        take(&mut bytes, 64)?;
    }
    take(&mut bytes, 4 + 64 + 64 + 64)?;
    if !bytes.is_empty() {
        return Err("trailing flat-v9 candidate bytes".into());
    }
    Ok(())
}

pub const fn name_of_tag(tag: u8) -> &'static str {
    match tag {
        TAG_POST_FLAT_MODEL_VECTOR_V9 => "PostFlatModelVectorV9",
        TAG_POST_FLAT_MODEL_VECTOR_V7 => "PostFlatModelVectorV7",
        TAG_SEAL_FLAT_MODEL_PROOF_V9 => "SealFlatModelProofV9",
        TAG_SEAL_FLAT_MODEL_PROOF_V7 => "SealFlatModelProofV7",
        TAG_REVEAL_FLAT_MODEL_PROOF_V9 => "RevealFlatModelProofV9",
        TAG_REVEAL_FLAT_MODEL_PROOF_V7 => "RevealFlatModelProofV7",
        TAG_EXPIRE_FLAT_MODEL_SEAL_V9 => "ExpireFlatModelSealV9",
        TAG_EXPIRE_FLAT_MODEL_SEAL_V7 => "ExpireFlatModelSealV7",
        TAG_DEMAND_FLAT_MODEL_UNIT_V9 => "DemandFlatModelUnitV9",
        TAG_DEMAND_FLAT_MODEL_UNIT_V7 => "DemandFlatModelUnitV7",
        TAG_ANSWER_FLAT_MODEL_UNIT_V9 => "AnswerFlatModelUnitV9",
        TAG_ANSWER_FLAT_MODEL_UNIT_V7 => "AnswerFlatModelUnitV7",
        TAG_SETTLE_FLAT_MODEL_DEMAND_V9 => "SettleFlatModelDemandV9",
        TAG_SETTLE_FLAT_MODEL_DEMAND_V7 => "SettleFlatModelDemandV7",
        TAG_REGISTER_FLAT_MODEL_CANDIDATE_V9 => "RegisterFlatModelCandidateV9",
        TAG_REGISTER_FLAT_MODEL_CANDIDATE_V7 => "RegisterFlatModelCandidateV7",
        TAG_POST_FLAT_MODEL_VECTOR_V6 => "PostFlatModelVectorV6",
        TAG_SEAL_FLAT_MODEL_PROOF_V6 => "SealFlatModelProofV6",
        TAG_REVEAL_FLAT_MODEL_PROOF_V6 => "RevealFlatModelProofV6",
        TAG_EXPIRE_FLAT_MODEL_SEAL_V6 => "ExpireFlatModelSealV6",
        TAG_DEMAND_FLAT_MODEL_VECTOR_V3 => "DemandFlatModelVectorV3",
        TAG_ANSWER_FLAT_MODEL_VECTOR_V3 => "AnswerFlatModelVectorV3",
        TAG_SETTLE_FLAT_MODEL_VECTOR_V3 => "SettleFlatModelVectorDemandV3",
        TAG_POST_FLAT_MODEL_VECTOR_V3 => "PostFlatModelVectorV3",
        TAG_REFUTE_FLAT_MODEL_VECTOR_V3 => "RefuteFlatModelVectorV3",
        TAG_COMPLETE_MODEL_CONFORMANCE_V2 => "CompleteModelConformanceV2",
        TAG_DEMAND_MODEL_VECTOR_V2 => "DemandModelVectorV2",
        TAG_ANSWER_MODEL_VECTOR_V2 => "AnswerModelVectorV2",
        TAG_SETTLE_MODEL_VECTOR_DEMAND_V2 => "SettleModelVectorDemandV2",
        TAG_POST_MODEL_VECTOR_V2 => "PostModelVectorV2",
        TAG_REFUTE_MODEL_VECTOR_V2 => "RefuteModelVectorV2",
        TAG_BIND_MODEL_ARTIFACT_V2 => "BindModelArtifactV2",
        TAG_REFUTE_MODEL_ARTIFACT_V2 => "RefuteModelArtifactV2",
        TAG_REGISTER_MODEL_CONFORMANCE_CLASS_V2 => "RegisterModelConformanceClassV2",
        TAG_REGISTER_CONFORMANCE_CLASS_V1 => "RegisterConformanceClass",
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
        TAG_OPT_IN_BEACON_SOURCE_V1 => "OptInBeaconSource",
        TAG_FUND_BEACON_SOURCE_FEE_V1 => "FundBeaconSourceFee",
        TAG_FUND_PANEL_BEACON_SOURCE_FEE_V2 => "FundPanelBeaconSourceFeeV2",
        TAG_PAID_CLAIM_V2 => "PaidClaimV2",
        TAG_ENROLL_PANEL_CONSUMER_LIFETIME_V2 => "EnrollPanelConsumerLifetimeV2",
        TAG_AUTHORIZE_PANEL_LIFETIME_V2 => "AuthorizePanelLifetimeV2",
        TAG_ATTEST_DRAWN_CHECK_V1 => "AttestDrawnCheckV1",
        TAG_ATTEST_MODEL_VERIFICATION_V1 => "AttestModelVerificationV1",
        TAG_CONSENT_HORIZON_EXTENSION_V1 => "ConsentHorizonExtensionV1",
        TAG_SIGN_MINIMUM_PRICE_CONTRACT_V1 => "SignMinimumPriceContractV1",
        TAG_BOOTSTRAP_EXACT_V1 => "BootstrapExactV1",
        TAG_COMMIT_ELIGIBILITY_EXAM_V1 => "CommitEligibilityExamV1",
        TAG_ATTEST_ELIGIBILITY_EXAM_V1 => "AttestEligibilityExamV1",
        TAG_OPT_IN_PAID_BEACON_SOURCE_V1 => "OptInPaidBeaconSource",
        TAG_FUNDED_EXECUTION_V11 => "FundedExecutionV11",
        TAG_STAGE_EXECUTION_V14 => "StageExecutionV14",
        TAG_COMMIT_SEGMENTED_CLAIM_V1 => "CommitSegmentedClaim",
        TAG_POST_TILED_JOB_V1 => "PostTiledJob",
        TAG_POST_PROMPT_TILE_V1 => "PostPromptTile",
        TAG_SPEC_V1 => "Spec",
        TAG_SEAL_MODEL_REFUTATION_V4 => "SealModelRefutationV4",
        TAG_REVEAL_MODEL_REFUTATION_V4 => "RevealModelRefutationV4",
        _ => "Unknown",
    }
}

/// The ceiling of a declared tag (`None`: not a declared tag).
pub const fn max_encoded_bytes_of_tag(tag: u8) -> Option<usize> {
    Some(match tag {
        TAG_POST_FLAT_MODEL_VECTOR_V9 => 1_583_616,
        TAG_POST_FLAT_MODEL_VECTOR_V7 => 1_583_616,
        TAG_SEAL_FLAT_MODEL_PROOF_V9 | TAG_EXPIRE_FLAT_MODEL_SEAL_V9 => 130,
        TAG_SEAL_FLAT_MODEL_PROOF_V7 | TAG_EXPIRE_FLAT_MODEL_SEAL_V7 => 130,
        TAG_REVEAL_FLAT_MODEL_PROOF_V9 => (1 << 20) + 134,
        TAG_REVEAL_FLAT_MODEL_PROOF_V7 => (1 << 20) + 134,
        TAG_DEMAND_FLAT_MODEL_UNIT_V9 => 84,
        TAG_DEMAND_FLAT_MODEL_UNIT_V7 => 84,
        TAG_ANSWER_FLAT_MODEL_UNIT_V9 => (16 << 10) + 70,
        TAG_ANSWER_FLAT_MODEL_UNIT_V7 => (16 << 10) + 70,
        TAG_SETTLE_FLAT_MODEL_DEMAND_V9 => 66,
        TAG_SETTLE_FLAT_MODEL_DEMAND_V7 => 66,
        TAG_REGISTER_FLAT_MODEL_CANDIDATE_V9 => 1_583_616,
        TAG_REGISTER_FLAT_MODEL_CANDIDATE_V7 => 1_583_616,
        TAG_POST_FLAT_MODEL_VECTOR_V6 => 1_583_616,
        TAG_SEAL_FLAT_MODEL_PROOF_V6 | TAG_EXPIRE_FLAT_MODEL_SEAL_V6 => 130,
        TAG_REVEAL_FLAT_MODEL_PROOF_V6 => (1 << 20) + 134,
        TAG_BIND_MODEL_ARTIFACT_V2 | TAG_REGISTER_MODEL_CONFORMANCE_CLASS_V2 => MAX_REGISTER_CLASS_BYTES_V1,
        TAG_REFUTE_MODEL_ARTIFACT_V2 => 2 << 20,
        TAG_COMPLETE_MODEL_CONFORMANCE_V2 => 90_000 + 134,
        TAG_POST_MODEL_VECTOR_V2 | TAG_REFUTE_MODEL_VECTOR_V2 => MAX_FILE_PROOF_BYTES_V1,
        TAG_DEMAND_FLAT_MODEL_VECTOR_V3 => 84,
        TAG_ANSWER_FLAT_MODEL_VECTOR_V3 => (10 << 10) + 70,
        TAG_SETTLE_FLAT_MODEL_VECTOR_V3 => 66,
        TAG_POST_FLAT_MODEL_VECTOR_V3 => 1_583_616,
        TAG_REFUTE_FLAT_MODEL_VECTOR_V3 => (1 << 20) + 70,
        TAG_DEMAND_MODEL_VECTOR_V2 => 92,
        TAG_ANSWER_MODEL_VECTOR_V2 => (80 << 10) + 70,
        TAG_SETTLE_MODEL_VECTOR_DEMAND_V2 => 66,
        TAG_REGISTER_CONFORMANCE_CLASS_V1 => MAX_REGISTER_CLASS_BYTES_V1,
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
        TAG_OPT_IN_BEACON_SOURCE_V1 => MAX_OPT_IN_BEACON_SOURCE_BYTES_V1,
        TAG_FUND_BEACON_SOURCE_FEE_V1 => MAX_FUND_BEACON_SOURCE_FEE_BYTES_V1,
        TAG_FUND_PANEL_BEACON_SOURCE_FEE_V2 => MAX_FUND_PANEL_BEACON_SOURCE_FEE_BYTES_V2,
        TAG_PAID_CLAIM_V2 => 256 << 20,
        TAG_ENROLL_PANEL_CONSUMER_LIFETIME_V2 => 4096,
        TAG_AUTHORIZE_PANEL_LIFETIME_V2 => 4096,
        TAG_ATTEST_DRAWN_CHECK_V1 => MAX_ATTEST_DRAWN_CHECK_BYTES_V1,
        TAG_ATTEST_MODEL_VERIFICATION_V1 | TAG_CONSENT_HORIZON_EXTENSION_V1 | TAG_SIGN_MINIMUM_PRICE_CONTRACT_V1 => {
            crate::econ_contracts_v1::MAX_ECON_CONTRACT_OBJECT_BYTES_V1
        }
        TAG_BOOTSTRAP_EXACT_V1 => crate::beacon_bootstrap_v1::MAX_BOOTSTRAP_EXACT_OBJECT_BYTES_V1,
        TAG_COMMIT_ELIGIBILITY_EXAM_V1 | TAG_ATTEST_ELIGIBILITY_EXAM_V1 => {
            crate::model_eligibility_v1::MAX_ELIGIBILITY_OBJECT_BYTES_V1
        }
        TAG_OPT_IN_PAID_BEACON_SOURCE_V1 => MAX_OPT_IN_PAID_BEACON_SOURCE_BYTES_V1,
        TAG_FUNDED_EXECUTION_V11 => MAX_FUNDED_EXECUTION_BYTES_V11,
        TAG_STAGE_EXECUTION_V14 => MAX_STAGE_SOURCE_BYTES_V39,
        TAG_COMMIT_SEGMENTED_CLAIM_V1 => MAX_COMMIT_SEGMENTED_CLAIM_BYTES_V1,
        TAG_POST_TILED_JOB_V1 => MAX_POST_TILED_JOB_BYTES_V1,
        TAG_POST_PROMPT_TILE_V1 => MAX_POST_PROMPT_TILE_BYTES_V1,
        TAG_SPEC_V1 => MAX_SPEC_BYTES_V1,
        // version, tag, slot, seal.
        TAG_SEAL_MODEL_REFUTATION_V4 => 1 + 1 + 1 + 64,
        // version, tag, slot, kind, target, salt, length, proof (the largest candidate court carrier).
        TAG_REVEAL_MODEL_REFUTATION_V4 => 1 + 1 + 1 + 1 + 64 + 64 + 4 + MAX_FILE_PROOF_BYTES_V1,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_master_carriers_do_not_enlarge_frozen_stage_actions() {
        let body = vec![0u8; MAX_STAGE_EXECUTION_BYTES_V14];
        let old = KernelRouteObjectV1::StageExecutionV14 { action: 33, body: body.clone() }.encode();
        assert!(borrowed_native_action_v1(&old).is_err());
        assert!(KernelRouteObjectV1::decode(&old).is_err());
        let new = KernelRouteObjectV1::StageExecutionV14 { action: 34, body }.encode();
        assert!(borrowed_native_action_v1(&new).is_ok());
        assert!(KernelRouteObjectV1::decode(&new).is_ok());
        let mut oversized = new;
        oversized.resize(MAX_STAGE_INVOCATION_BYTES_V32 + 1, 0);
        let size = (oversized.len() - 7) as u32;
        oversized[3..7].copy_from_slice(&size.to_le_bytes());
        assert!(borrowed_native_action_v1(&oversized).is_err());
        assert!(KernelRouteObjectV1::decode(&oversized).is_err());
        let collect = KernelRouteObjectV1::StageExecutionV14 { action: 37, body: vec![7; 64] }.encode();
        assert!(borrowed_native_action_v1(&collect).is_ok());
        assert!(KernelRouteObjectV1::decode(&collect).is_ok());
        let mut unknown = collect;
        unknown[2] = 80;
        assert!(borrowed_native_action_v1(&unknown).is_err());
        assert!(KernelRouteObjectV1::decode(&unknown).is_err());
    }
    #[test]
    fn compact_commit_transport_has_its_own_whole_part_and_assembly_limits() {
        let whole = KernelRouteObjectV1::StageExecutionV14 { action: 38, body: vec![0; MAX_STAGE_INVOCATION_BYTES_V32] }.encode();
        assert!(borrowed_native_action_v1(&whole).is_ok());
        assert!(KernelRouteObjectV1::decode(&whole).is_ok());
        let mut wrong_action = whole;
        wrong_action[2] = 36;
        assert!(borrowed_native_action_v1(&wrong_action).is_err());
        assert!(KernelRouteObjectV1::decode(&wrong_action).is_err());
        for (action, sizes) in [(39, vec![143, 143 + (32 << 10)]), (40, vec![130])] {
            for size in sizes {
                let packet = KernelRouteObjectV1::StageExecutionV14 { action, body: vec![0; size] }.encode();
                assert!(borrowed_native_action_v1(&packet).is_ok());
                assert!(KernelRouteObjectV1::decode(&packet).is_ok());
            }
        }
        for (action, size) in [(39, 142), (39, 144 + (32 << 10)), (40, 129), (40, 131)] {
            let packet = KernelRouteObjectV1::StageExecutionV14 { action, body: vec![0; size] }.encode();
            assert!(borrowed_native_action_v1(&packet).is_err());
            assert!(KernelRouteObjectV1::decode(&packet).is_err());
        }
    }
    #[test]
    fn native_action_borrowed_frame_has_exact_count_and_preserves_old_owned_bytes() {
        for object in [
            KernelRouteObjectV1::FundedExecutionV11 { action: 15, body: vec![7; 128 << 10] },
            KernelRouteObjectV1::StageExecutionV14 { action: 14, body: vec![9; 4096] },
        ] {
            let raw = object.encode();
            let frame = borrowed_native_action_v1(&raw).unwrap().unwrap();
            assert_eq!(frame.body.as_ptr(), raw[7..].as_ptr());
            assert_eq!(frame.body.len(), raw.len() - 7);
            assert_eq!(KernelRouteObjectV1::decode(&raw).unwrap(), object);
            let mut lie = raw.clone();
            lie[3..7].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(borrowed_native_action_v1(&lie).is_err());
            let mut extra = raw;
            extra.push(0);
            assert!(borrowed_native_action_v1(&extra).is_err());
        }
        assert!(borrowed_native_action_v1(&KernelRouteObjectV1::Withdraw { bond: [1; 64] }.encode()).unwrap().is_none());
        // Funded actions run to 46: 45/46 are the typed window class v37 register/attach (RFC02 §II.14.60), as the decoder admits.
        for (tag, last, unknown) in [(TAG_FUNDED_EXECUTION_V11, 46, 47), (TAG_STAGE_EXECUTION_V14, 38, 80)] {
            let mut raw = vec![1, tag, last, 0, 0, 0, 0];
            assert_eq!(borrowed_native_action_v1(&raw).unwrap().unwrap().action, last);
            raw[2] = unknown;
            assert!(borrowed_native_action_v1(&raw).is_err());
            raw[2] = last;
            raw[0] = 2;
            assert!(borrowed_native_action_v1(&raw).is_err());
        }
    }

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
            KernelRouteObjectV1::RegisterFlatModelCandidateV7 {
                binding: [1; 64],
                descriptor: [2; 64],
                program_bytes: vec![7],
                param_commitments: ParamCommitmentsV1::default(),
                max_positions: 1,
                scope_root: [3; 64],
                replay_root: [4; 64],
            },
            KernelRouteObjectV1::Respond { claim: [1; 64], stage: 0, position: 0, bytes: vec![1, 2, 3] },
            KernelRouteObjectV1::RequestExit { bond: [7; 64] },
            KernelRouteObjectV1::Withdraw { bond: [7; 64] },
            KernelRouteObjectV1::FileProof { accuser: [1; 64], claim: [2; 64], proof: ProsecutionV1::Kernel(vec![9]) },
            KernelRouteObjectV1::FileProof { accuser: [1; 64], claim: [2; 64], proof: ProsecutionV1::Pipeline(vec![9]) },
            KernelRouteObjectV1::SealProof { accuser: [1; 64], claim: [2; 64], seal: [3; 64] },
            KernelRouteObjectV1::OptInBeaconSource {
                producer: [1; 64],
                job: [2; 64],
                seal: [3; 64],
                context: [4; 64],
                terms: [5; 64],
            },
            KernelRouteObjectV1::PostFlatModelVectorV6 { class: [1; 64], binding: [2; 64], post: vec![6, 0] },
            KernelRouteObjectV1::SealFlatModelProofV6 { vector: [1; 64], seal: [2; 64] },
            KernelRouteObjectV1::RevealFlatModelProofV6 { vector: [1; 64], salt: [2; 64], proof: vec![5, 0] },
            KernelRouteObjectV1::ExpireFlatModelSealV6 { vector: [1; 64], actor: [2; 64] },
            KernelRouteObjectV1::PostFlatModelVectorV7 { class: [1; 64], binding: [2; 64], post: vec![7, 0] },
            KernelRouteObjectV1::SealFlatModelProofV7 { vector: [1; 64], seal: [2; 64] },
            KernelRouteObjectV1::RevealFlatModelProofV7 { vector: [1; 64], salt: [2; 64], proof: vec![7, 0] },
            KernelRouteObjectV1::ExpireFlatModelSealV7 { vector: [1; 64], actor: [2; 64] },
            KernelRouteObjectV1::DemandFlatModelUnitV7 {
                vector: [1; 64],
                slot: 0,
                position: 1,
                occurrence: 2,
                node: 3,
                kind: 2,
                index: 65,
            },
            KernelRouteObjectV1::AnswerFlatModelUnitV7 { demand: [1; 64], answer: vec![7, 0, 1] },
            KernelRouteObjectV1::SettleFlatModelDemandV7 { demand: [1; 64] },
            KernelRouteObjectV1::RegisterFlatModelCandidateV9 {
                binding: [1; 64],
                descriptor: [2; 64],
                program_bytes: vec![9],
                param_commitments: ParamCommitmentsV1::default(),
                max_positions: 1,
                scope_root: [3; 64],
                replay_root: [4; 64],
                resource_root: [5; 64],
            },
            KernelRouteObjectV1::PostFlatModelVectorV9 { class: [1; 64], binding: [2; 64], post: vec![9, 0] },
            KernelRouteObjectV1::SealFlatModelProofV9 { vector: [1; 64], seal: [2; 64] },
            KernelRouteObjectV1::RevealFlatModelProofV9 { vector: [1; 64], salt: [2; 64], proof: vec![9, 0] },
            KernelRouteObjectV1::ExpireFlatModelSealV9 { vector: [1; 64], actor: [2; 64] },
            KernelRouteObjectV1::DemandFlatModelUnitV9 {
                vector: [1; 64],
                slot: 0,
                position: 1,
                occurrence: 2,
                node: 3,
                kind: 3,
                index: 0,
            },
            KernelRouteObjectV1::AnswerFlatModelUnitV9 { demand: [1; 64], answer: vec![9, 0, 1] },
            KernelRouteObjectV1::SettleFlatModelDemandV9 { demand: [1; 64] },
        ];
        for o in objects {
            let bytes = o.encode();
            assert_eq!(bytes[0], KERNEL_ROUTE_VERSION_V1);
            assert_eq!(bytes[1], o.tag(), "{}", o.name());
            assert!(bytes.len() <= o.max_encoded_bytes(), "{}", o.name());
            assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
            assert_ne!(o.name(), "Unknown");
        }
        let tags: Vec<u8> = (1..=18)
            .chain([TAG_SPEC_V1, TAG_COMMIT_CLAIM_SALTED_V1, TAG_REGISTER_CONFORMANCE_CLASS_V1, TAG_OPT_IN_BEACON_SOURCE_V1])
            .collect();
        assert!(tags.iter().all(|t| max_encoded_bytes_of_tag(*t).is_some() && name_of_tag(*t) != "Unknown"));
        assert!(max_encoded_bytes_of_tag(0).is_none());
        assert!(max_encoded_bytes_of_tag(38).is_none(), "38 stays reserved for G14R, not allocated");
        // 39: G14R's M*-49 drawn-check attestation (2026-10-15). 61–62: prepaid sources. 63: RFC02 explicit funded execution11;
        // 64: RFC02 typed external stage14. 65–66: prospective Panel consent. 67–69: RFC07/RFC08 signed economic contracts
        // (2026-10-15). 70–71: native Panel authority and paid ClaimV2 (renumbered from 63/64, which the RFC02 branch owns).
        // 73: N3's BootstrapExact contribution (2026-10-15 round 3). 72 is unallocated. 74–75: RFC-0017's eligibility exam.
        assert!(max_encoded_bytes_of_tag(72).is_none(), "72 is not allocated");
        assert!(max_encoded_bytes_of_tag(76).is_none(), "76 is not allocated");
        for tag in (22..=37).chain(39..=71).chain(73..=75) {
            assert!(max_encoded_bytes_of_tag(tag).is_some());
        }
        for o in [
            KernelRouteObjectV1::AttestDrawnCheckV1 { verifier: [1; 64], claim: [2; 64] },
            KernelRouteObjectV1::ConsentHorizonExtensionV1 {
                terms: crate::econ_contracts_v1::HorizonExtensionTermsV1 {
                    version: 1,
                    network: [1; 64],
                    claim: [2; 64],
                    root: [3; 64],
                    worker: [4; 64],
                    sponsor: [5; 64],
                    current_liability_until_daa: 100,
                    new_liability_until_daa: 150,
                    hard_deadline_daa: 200,
                    consent_by_daa: 90,
                    price_contract: [0; 64],
                },
            },
            KernelRouteObjectV1::BootstrapExactV1 {
                contribution: crate::beacon_bootstrap_v1::BootstrapExactContributionV1 {
                    version: 1,
                    network: [1; 64],
                    round: crate::beacon_bootstrap_v1::BootstrapExactRoundV1 { profile: [2; 64], target: [3; 64], slot: 1 },
                    contributor: [4; 64],
                    action: crate::beacon_bootstrap_v1::BootstrapExactActionV1::Reveal { preimage: [5; 64] },
                },
            },
            KernelRouteObjectV1::CommitEligibilityExamV1 {
                commit: crate::model_eligibility_v1::EligibilityExamCommitV1 {
                    version: 1,
                    network: [1; 64],
                    committer: [2; 64],
                    class_id: [3; 64],
                    purpose: crate::model_eligibility_v1::EligibilityExamPurposeV1::Promotion,
                    claims_root: [4; 64],
                    claims_count: 16,
                    attempt_index: 0,
                },
            },
            KernelRouteObjectV1::AttestEligibilityExamV1 {
                attestation: crate::model_eligibility_v1::EligibilityAttestationV1 {
                    version: 1,
                    network: [1; 64],
                    exam: [2; 64],
                    verifier: [3; 64],
                    verdict: crate::model_eligibility_v1::EligibilityVerdictV1::Reproduced,
                    transcript: [4; 64],
                    claim_mismatches: (0..64).collect(),
                },
            },
        ] {
            let bytes = o.encode();
            assert_eq!(bytes[1], o.tag(), "{}", o.name());
            assert!(bytes.len() <= o.max_encoded_bytes(), "{}", o.name());
            assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
        }
        assert_eq!(name_of_tag(TAG_FUND_BEACON_SOURCE_FEE_V1), "FundBeaconSourceFee");
        assert_eq!(name_of_tag(TAG_OPT_IN_PAID_BEACON_SOURCE_V1), "OptInPaidBeaconSource");
        assert_eq!((TAG_FUND_BEACON_SOURCE_FEE_V1, TAG_OPT_IN_PAID_BEACON_SOURCE_V1), (61, 62));
        // RFC-0002 P0 (RFC02-T): the seal and reveal round-trip at their declared discriminants and within their ceilings.
        for o in [
            KernelRouteObjectV1::SealModelRefutationV4 { slot: 3, seal: [5; 64] },
            KernelRouteObjectV1::RevealModelRefutationV4 { slot: 3, kind: 3, target: [6; 64], salt: [7; 64], proof: vec![8; 100] },
        ] {
            let bytes = o.encode();
            assert_eq!(bytes[1], o.tag());
            assert!(bytes.len() <= o.max_encoded_bytes(), "{}", o.name());
            assert_eq!(KernelRouteObjectV1::decode(&bytes).unwrap(), o);
        }
        assert_eq!(
            KernelRouteObjectV1::SealModelRefutationV4 { slot: 0, seal: [0; 64] }.encode().len(),
            max_encoded_bytes_of_tag(TAG_SEAL_MODEL_REFUTATION_V4).unwrap()
        );
        let complete = KernelRouteObjectV1::CompleteModelConformanceV2 { class: [1; 64], binding: [2; 64], proof: vec![0; 90_000] };
        assert_eq!(complete.encoded_len(), max_encoded_bytes_of_tag(25).unwrap());
        assert_eq!(KernelRouteObjectV1::decode(&complete.encode()).unwrap(), complete);
        assert_eq!(max_encoded_bytes_of_tag(21), Some(MAX_REGISTER_CLASS_BYTES_V1));
        assert_eq!(name_of_tag(21), "RegisterConformanceClass");
        assert!(max_encoded_bytes_of_tag(TAG_SPEC_V1).is_some() && name_of_tag(TAG_SPEC_V1) == "Spec", "RFC-0004 Part II's tag 19");
    }

    #[test]
    fn explicit_mixed7_outer_codec_refuses_truncation_inflated_counts_trailing_and_noncanonical_units() {
        let objects = [
            KernelRouteObjectV1::PostFlatModelVectorV7 { class: [1; 64], binding: [2; 64], post: vec![7, 0, 0] },
            KernelRouteObjectV1::RevealFlatModelProofV7 { vector: [1; 64], salt: [2; 64], proof: vec![7, 0, 0] },
            KernelRouteObjectV1::AnswerFlatModelUnitV7 { demand: [1; 64], answer: vec![7, 0, 0] },
        ];
        for object in objects {
            let raw = object.encode();
            for n in 0..raw.len() {
                assert!(KernelRouteObjectV1::decode(&raw[..n]).is_err());
            }
            let mut inflated = raw.clone();
            let count = if object.tag() == TAG_ANSWER_FLAT_MODEL_UNIT_V7 { 66 } else { 130 };
            inflated[count..count + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(KernelRouteObjectV1::decode(&inflated).is_err());
            let mut trailing = raw.clone();
            trailing.push(0);
            assert!(KernelRouteObjectV1::decode(&trailing).is_err());
        }
        for (kind, index) in [(0, 0), (1, 65), (2, u64::MAX)] {
            let object = KernelRouteObjectV1::DemandFlatModelUnitV7 {
                vector: [1; 64],
                slot: 0,
                position: 1,
                occurrence: 2,
                node: 3,
                kind,
                index,
            };
            let raw = object.encode();
            assert_eq!(raw.len(), 84);
            assert_eq!(KernelRouteObjectV1::decode(&raw).unwrap(), object);
            for n in 0..raw.len() {
                assert!(KernelRouteObjectV1::decode(&raw[..n]).is_err());
            }
            let mut bad = raw.clone();
            bad[75] = 3;
            assert!(KernelRouteObjectV1::decode(&bad).is_err());
        }
        let invalid = KernelRouteObjectV1::DemandFlatModelUnitV7 {
            vector: [1; 64],
            slot: 0,
            position: 1,
            occurrence: 2,
            node: 3,
            kind: 0,
            index: 1,
        };
        assert!(KernelRouteObjectV1::decode(&invalid.encode()).is_err());
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

#[cfg(test)]
mod model_vector_da_wire_tests {
    use super::*;
    #[test]
    fn vector_da_units_have_fixed_wire_and_bounded_response_ceiling() {
        let objects = [
            KernelRouteObjectV1::DemandModelVectorV2 {
                vector: [1; 64],
                slot: 3,
                position: 7,
                occurrence: 2,
                node: 5,
                axis: 2,
                line: 0,
                tile: 0,
            },
            KernelRouteObjectV1::AnswerModelVectorV2 { demand: [2; 64], answer: vec![0; 80 << 10] },
            KernelRouteObjectV1::SettleModelVectorDemandV2 { demand: [3; 64] },
        ];
        for o in objects {
            let b = o.encode();
            assert_eq!(b.len(), max_encoded_bytes_of_tag(o.tag()).unwrap());
            assert_eq!(KernelRouteObjectV1::decode(&b).unwrap(), o);
            let mut oversized = b;
            oversized.push(0);
            assert!(KernelRouteObjectV1::decode(&oversized).is_err());
        }
    }
}
