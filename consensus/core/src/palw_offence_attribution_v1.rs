//! **ADR-0152 v2, stage F2: a false `Valid` is judged by one adjudicator, bound to the claim's
//! committed root.**
//!
//! The V1 route ([`crate::palw_offence_v1::PalwPanelFalseValidEvidenceV1`]) failed on every real
//! claim in three ways:
//!
//! * **Its job check is a fixed point.** It convicted only when the contradiction's
//!   `job_context.job_id` equalled the claim id. A block-lane claim's id is `attempt_id_v2`, a hash
//!   of the attempt, which holds the `execution_root`, which is rebuilt from the context that holds
//!   the job id — so the job id would have to be a hash of itself. A free-prompt claim's id hashes
//!   its commitment the same way. No honest claim could be prosecuted; only a forged one could.
//! * **It read only the V2 receipt**, under a `network_domain` the payload chose and nobody
//!   compared with the chain's. Past Verification V2 a kaspad seat signs only V3 (segmented)
//!   receipts, so the seats that actually licensed a claim could not be named at all — while a
//!   receipt signed by hand in the V2 format still cleared the gate for the state-bound kinds,
//!   `ConflictingPermit` among them, which names no claim.
//! * **Its gate ran only in the processor.** The fold checked the root and nothing else, so a
//!   test that exercised the fold alone proved nothing about what a node accepts.
//!
//! Past `Params::palw_offence_attribution` the V1 kind is refused and
//! [`crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2`] is judged by
//! [`palw_check_panel_false_valid_v2`] — called by the processor with the signature check and by
//! the fold without it, so the two cannot reach different verdicts from the same state. It
//! enforces this chain of attribution, one link each:
//!
//! | Link | Where |
//! |---|---|
//! | claim | the receipt names it, and the target is resolved from it ([`palw_offence_target_v1`]) |
//! | committed root | the contradiction's binding must rebuild `claim.execution_root` |
//! | execution | `verify_binding` rebuilds that root from every field of the job context |
//! | leaf | the fault's site ([`PalwFaultSiteV1`]), read from the verdict, never from the filer — with every committed leaf the verdict read |
//! | signer | the ML-DSA receipt under the chain's domain, and its segment mask, which must be the one the panel assigned |
//! | fault | the arithmetic, structural or decode checker |
//! | slash target | the seat's lock; the executor through `void_and_slash` or the `Final`'s reversal |
//!
//! **F1 (ADR-0152 v3.1 J-1…J-5) adds the job-identity link** — that the root answers THIS claim's
//! job and not a borrowed one. The claim records the identity it was mined under
//! (`PalwClaimStateV2::job_identity`: the carrying header's execution anchor, or a free-prompt
//! commitment's [`crate::palw_fp_execution_v3::palw_fp_job_pin_v1`]), and
//! [`palw_binding_identity_fault_v1`] holds a binding's context to it; `IdentityMismatch` (9) and
//! `OutputMismatch` (10) convict on it, against a `Valid` signer (kind 3) or against the executor
//! itself ([`palw_check_executor_refuted_v1`], kind 4, no receipt). A fault that proves the claim
//! answers the wrong job forfeits by CLAIM, never by root: a borrowed root is an honest lender's too.
//!
//! **What a partial seat vouched for** (F2 review, F-1). A segmented seat replays its segment from
//! the prompt or from the committed checkpoint at the segment's start, and compares the step leaves
//! of its segment with the committed ones. It vouches that those leaves are the correct function of
//! what it resumed from — not that they agree with committed leaves of OTHER segments, which it
//! recomputes (or restores) and never compares. A step refutation recomputes its leaf from the
//! COMMITTED inputs, so one lie in segment 1 makes every downstream reader in segments 2 and 3
//! "convict" — their holders replayed honestly and matched. A partial seat is therefore liable
//! only for a verdict every committed leaf of which lies in the segments its mask covers, and that
//! read nothing a segment replay does not recompute (a KV checkpoint, a generated token).

use crate::palw_offence_v1::{PalwOffenceVerifyError, PalwPanelContradictionV1, palw_panel_contradiction_convicts_execution_v1};
use crate::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwVoidReasonV2};
use crate::palw_verification_v2::{
    PalwSegmentMaskV2, palw_segment_assignment_v2, palw_segment_count_v2, palw_segment_index_of_leaf_v2,
};
use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

/// Wire version of [`PalwPanelFalseValidEvidenceV2`]. `1` is the V1 payload's, so a V1 body can
/// never be read as this one.
pub const PALW_PANEL_FALSE_VALID_VERSION_V2: u16 = 2;

/// **The most bytes a `PanelFalseValidV2` evidence may carry: what ONE carrier holds** (F2 review,
/// F-3). Asked before a byte is decoded.
///
/// A kind-3 object rides exactly one 0x4b lifecycle transaction: the chunk group assembles only a
/// `FamilyCertified` ([`crate::palw_state_v2::palw_chunked_object_kind_admitted_v1`]), and this
/// stage adds no chunking of its own. One carrier's payload is
/// [`crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES`] — ADR-0080's measured figure: the largest
/// round number that relays under the 120,000 bytes a standard transaction carries
/// ([`crate::palw_mode_v2::PALW_STANDARD_TX_BYTES`] = the mempool's `MAXIMUM_STANDARD_TRANSACTION_MASS`
/// over `TRANSIENT_BYTE_TO_MASS_FACTOR`) with the worst-case carrier beside it, and under the
/// 125,000 bytes a block holds (`max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR`). The object's own
/// framing — the payload version, the enum tag, the kind, the accused outpoint, the evidence id and
/// the length prefix — rides in that carrier allowance beside it. The cap this replaces (the court's
/// 2,250,000-byte close ceiling plus a receipt) stated a size no carrier could deliver: above
/// ~110 KB the object never relays at all.
///
/// **Residual, recorded rather than closed:** a contradiction larger than one carrier cannot reach
/// kind 3. A model class's softmax or KV-reading step opens one committed K/V leaf per cached
/// position, so at 512 and more positions its step refutation is past this cap; such a fault
/// convicts a full seat only through a court's `CourtFraud` void (the one-move court, the checkpoint
/// court or a held data-availability answer), which a kind-3 `CourtFraud` then names in a few
/// hundred bytes. Kind 3 is not chunked in this stage.
pub const PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES: u64 = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;

/// Keyed-BLAKE2b-512 domain of [`palw_false_valid_ledger_key_v2`].
pub const PALW_FALSE_VALID_KEY_DOMAIN_V2: &[u8] = b"misaka-palw/false-valid-key/v2";

/// **The most bytes F7's reporter slot may carry, once its fence arms it** (agreed with the peer).
/// Until then the slot must be empty; after, a commit–reveal opening is a few hashes, and a slot
/// the filer could fill up to the evidence cap would be free bytes riding every conviction.
pub const PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES: usize = 256;

/// **The ladder a bundle-less judge opens a non-held class's step tree at**: the widest any
/// ruleset may freeze ([`crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`]), the
/// bound `PALW_FP_STRUCTURAL_WORK_LEAVES_CAP` gives the free-prompt door for the same reason. The
/// fold holds no bundle, and the ladder only bounds the walk: the binding's own `step_leaf_count`
/// is pinned by the claim's root, which acceptance already bounded under the network's ladder. The
/// V1 route's `64` refused every real claim's step refutation as a leaf count out of range.
pub const PALW_FALSE_VALID_NETWORK_LADDER_V1: u64 = crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;

/// **The receipt the accused seat signed, in the form the chain licensed on.**
///
/// A V1 licence (`ReceiptLicensed`) carries full V2 receipts; a Verification V2 licence carries V3
/// receipts with a segment mask. Both are `Valid` attestations the chain relied on, so both must
/// be convictable — accepting only one form would let the other's signers escape.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwFalseValidReceiptV1 {
    /// A whole-job attestation, signed over `palw_receipt_message_v2`.
    Full(PalwSeatReceiptV2) = 0,
    /// A segment-scoped attestation, signed over `palw_receipt_message_v3` with its mask.
    Segmented(PalwSeatReceiptV3) = 1,
}

impl PalwFalseValidReceiptV1 {
    /// The V2 receipt inside either form: the claim, verdict, seat and signature.
    pub fn inner(&self) -> &PalwSeatReceiptV2 {
        match self {
            Self::Full(receipt) => receipt,
            Self::Segmented(signed) => &signed.receipt,
        }
    }

    /// What the seat signed under `chain_domain`, and the ML-DSA context it signed it in. The
    /// domain is always the CHAIN's — the payload carries none, so a receipt signed for another
    /// network verifies against nothing here.
    pub fn signed_message_v1(&self, chain_domain: Hash64) -> (Hash64, &'static [u8]) {
        let inner = self.inner();
        match self {
            Self::Full(_) => (
                crate::palw_panel_v2::palw_receipt_message_v2(chain_domain, inner.claim, inner.verdict, inner.signed_daa),
                crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
            ),
            Self::Segmented(signed) => (
                crate::palw_panel_v2::palw_receipt_message_v3(
                    chain_domain,
                    inner.claim,
                    inner.verdict,
                    inner.signed_daa,
                    signed.segments,
                ),
                crate::palw_panel_v2::PALW_RECEIPT_V3_MLDSA87_CONTEXT,
            ),
        }
    }
}

/// **A panel seat signed `Valid`, and an objective contradiction pinned to the claim's committed
/// root says the work was false.** One seat, one claim, one offence.
///
/// It deliberately carries no `network_domain` and no `executor_pubkey`: the verifier uses the
/// chain's own domain and reads every key from state, so nothing a filer supplies decides whose
/// signature a check runs under.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelFalseValidEvidenceV2 {
    /// = [`PALW_PANEL_FALSE_VALID_VERSION_V2`].
    pub version: u16,
    pub claim_id: Hash64,
    pub accused_seat: TransactionOutpoint,
    pub receipt: PalwFalseValidReceiptV1,
    pub contradiction: PalwPanelContradictionV1,
    /// **The prompt's one tile a `StepArithmetic` reads, where the job commits its ids as a Merkle
    /// root** (F2 review, F-3; ADR-0081 Decision 3, the one-move court's carriage,
    /// `PalwShardCourtAccusationV1::prompt_ids_opening`). The refutation then carries no id list and
    /// the evidence grows with a path, never with the prompt; the pair is what
    /// [`crate::palw_step_refute::palw_refutation_prompt_carriage_v1`] builds from a prover's
    /// refutation. `None` on a flat commitment (where the refutation carries the whole list, the V1
    /// route's form) and for a step that reads no prompt id. The job's commitment decides which of
    /// the two is right — an opening against a flat digest, or a whole list against a Merkle root,
    /// is refused by the arithmetic — so no per-network or per-class form is read to judge it.
    /// Carried beside any other contradiction it is evidence for a question nobody asked, and
    /// refused.
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
    /// **F7's commit–reveal slot.** Empty until its own fence arms it, at most
    /// [`PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES`] once it does, and never part of the ledger
    /// key, so filling it cannot make one false `Valid` two offences.
    pub reporter_reveal: Vec<u8>,
}

/// Which lane a claim came from. Not stored as such: read off the claim row when a target is
/// resolved, or off the liability row's `free_prompt` where the row recorded the claim's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimSourceKindV1 {
    Attempt,
    FreePrompt,
}

/// Wire version of [`PalwExecutorRefutedEvidenceV1`].
pub const PALW_EXECUTOR_REFUTED_VERSION_V1: u16 = 1;

/// Keyed-BLAKE2b-512 domain of [`palw_executor_refuted_ledger_key_v1`].
pub const PALW_EXECUTOR_REFUTED_KEY_DOMAIN_V1: &[u8] = b"misaka-palw/executor-refuted-key/v1";

/// **The claim's executor refuted by an objective contradiction of its own committed execution**
/// (ADR-0152 v3.1 J-4, kind 4). No receipt and no signature: the evidence is objective, and the
/// accused is whoever the claim names as its executor. One offence per claim.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecutorRefutedEvidenceV1 {
    /// = [`PALW_EXECUTOR_REFUTED_VERSION_V1`].
    pub version: u16,
    pub claim_id: Hash64,
    /// One of the execution- or claim-proving contradictions (5, 6, 8, 9, 10).
    pub contradiction: PalwPanelContradictionV1,
    /// A `StepArithmetic`'s one prompt tile under a Merkle prompt commitment — the same carriage
    /// kind 3 takes (F2 review, F-3); `None` otherwise.
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
    /// F7's commit–reveal slot, as kind 3's: empty until its own fence, never in the ledger key.
    pub reporter_reveal: Vec<u8>,
}

/// **What the identity checks read besides the target and the binding**: the network's prompt-id
/// form (a class's form is derived from it, `palw_prompt_ids_form_of_class_v1`) and which class is
/// the base one (whose canonical job the chain derives, J5) — and the one ruleset switch the
/// adjudicators read beside them: whether a DA-confirmed withholding makes a `Valid` signer liable
/// (`da_signer_liability`, [`crate::palw_state_v2::palw_da_signer_liability_armed_v1`]; N9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwIdentityRulesV1 {
    pub prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    pub base_class_id: Hash64,
    /// ADR-0152 N9 / X7 (M3): `ProducerWithholding` may restate a DA-7 default against a covering
    /// `Valid` signer. `false` wherever `palw_rcore_plus` is dormant, and wherever the ruleset keeps
    /// signer liability dormant.
    pub da_signer_liability: bool,
}

/// **Which identity check a binding failed** — the first, in the addendum's order (§4-bis.2):
/// J2, J1, J3, J5a, J5b, J4, J6, J7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwIdentityFaultV1 {
    /// J2: the binding's shape profile is not the claim's class (a class id IS its profile id).
    ClassNotTheClaims,
    /// J1: the context's job is not the one the claim recorded (attempt: the job id against the
    /// carrying header's anchor; free prompt: the context's pin against the commitment's).
    JobNotTheClaims,
    /// J3 (attempt): the execution seed is not the anchor's first 32 bytes.
    SeedNotTheJobs,
    /// J5a (attempt, a class whose canonical job the chain derives): the whole context is not the
    /// one the anchor implies — prefill, decode, ceiling, network, nullifier, model, runtime,
    /// tokenizer, assignment, cu, version or scheme moved.
    ContextNotCanonical,
    /// J5b (attempt, a canonical prompt of at most 4,096 ids): the prompt root is not the anchor's
    /// prompt — a relabelled run of another anchor's prompt.
    PromptNotTheAnchors,
    /// J4: the binding's logits trace root is not the claim's committed trace root.
    TraceNotTheClaims,
    /// J6 (both lanes): the activation leg is not the integer classes' "taps nothing" statement
    /// over this context (`palw_int_activation_leg_root_v1`) — a free root with a real preimage.
    ActivationLegNotCanonical,
    /// J7 (both lanes): the checkpoint profile is not the class's canonical one
    /// (`palw_canonical_checkpoint_profile_v1`) — the interval the filer would otherwise choose.
    CheckpointProfileNotCanonical,
}

impl PalwIdentityFaultV1 {
    /// The check's name, as the ADR spells it.
    pub fn code(self) -> &'static str {
        match self {
            Self::ClassNotTheClaims => "J2",
            Self::JobNotTheClaims => "J1",
            Self::SeedNotTheJobs => "J3",
            Self::ContextNotCanonical => "J5a",
            Self::PromptNotTheAnchors => "J5b",
            Self::TraceNotTheClaims => "J4",
            Self::ActivationLegNotCanonical => "J6",
            Self::CheckpointProfileNotCanonical => "J7",
        }
    }
}

/// **What a conviction forfeits of the rights the convicted work already holds** (ADR-0152 v3.1
/// V-2b, addendum §4-bis.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwForfeitScopeV1 {
    /// A named void: it names a claim the chain already voided, and proves nothing about a root.
    None,
    /// The claim answers the wrong job or output (9, 10): its own rights go, by claim id — never
    /// the root's, which an honest lender's claim may carry too.
    ByClaim,
    /// The execution itself is false (5, 6, 8): every right on that root goes (ADR-0151).
    ByRoot,
}

/// **What a false-Valid conviction lands on**, resolved from state — the live claim first, its
/// liability row once it has retired — never from the evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwOffenceTargetV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    /// The class's artifact root, which operand openings prove against; zero for a class no longer
    /// in state (no arithmetic refutation can prove against it, which is the fail-closed side).
    pub artifact_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub execution_root: Hash64,
    /// `None` on a liability row, which does not record the lane (F1 adds it).
    pub lane: Option<PalwClaimSourceKindV1>,
    /// `K = seats − 1` of the claim's bound panel; `None` when no panel is in state (a retired
    /// claim's row does not record it until F1), and a segmented receipt cannot then be placed.
    pub segment_count: Option<u16>,
    /// `None` when only the liability row is left.
    pub phase: Option<PalwClaimPhaseV2>,
    /// **F1 (ADR-0152 v3.1 J-2, R9): the job identity the claim recorded** — the claim record's,
    /// else the liability row's copy, else the vesting row's; 0 when none recorded it, which
    /// convicts nobody (`IdentityNotRecorded`).
    pub job_identity: Hash64,
    /// The claim's committed logits trace root (J4), from the same source.
    pub trace_root: Hash64,
    /// The claim's committed output root (`OutputMismatch`); 0 where only a vesting row is left,
    /// which records none.
    pub output_root: Hash64,
}

/// **Where in the execution a proven fault sits, and what the verdict read to find it.** A seat is
/// liable for the fault only if its receipt attested all of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFaultSiteV1 {
    /// **A fault a segment replay decides on its own.** Step leaf `leaf` is wrong, and the verdict
    /// read no committed step leaf outside `first_read..=last_read` — and nothing a segment replay
    /// does not itself recompute. A structural fault of the leaf alone reads only the leaf
    /// ([`Self::leaf_alone`]); a recomputed step reads its canonical inputs as well, which the
    /// verdict authenticated against the committed step root.
    Leaf { leaf: u64, first_read: u64, last_read: u64 },
    /// The execution as a whole: a shape, a checkpoint chain, a decoded output, a void the chain
    /// wrote — or a step whose verdict read what no segment replay recomputes (a committed KV
    /// checkpoint, a generated token) or a leaf outside the main step space no segment replay
    /// compares. Only a whole attestation covers it.
    Whole,
    /// **A fault in the JOB the claim answers, which every `Valid` attests** (ADR-0152 v3.1
    /// addendum §4-bis.7; the user's decision of 2026-09-24): 9's J1/J2/J3/J5a/J5b and 13. A partial
    /// seat signs only after its SEAT-S4 opening has checked the binding's job field by field
    /// (`base0_material_job_is_the_claims_v1`, SEAT-S1) — the class, the anchor, the seed, the
    /// canonical context and the prompt it resumes from — so a `Valid` under ANY non-empty mask
    /// vouched for them. The ship gate (SEAT-S4 + SEAT-R + T18p-M in the same binary) is on the
    /// release checklist.
    AnyValid,
}

impl PalwFaultSiteV1 {
    /// A fault in step leaf `leaf` that the verdict found in that leaf and read nothing else for.
    pub fn leaf_alone(leaf: u64) -> Self {
        Self::Leaf { leaf, first_read: leaf, last_read: leaf }
    }
}

/// The adjudicator's verdict: whom and what a conviction lands on, where the fault is, whether the
/// contradiction proves the claim false — its execution, or (F1) the job or output it answers — and
/// so voids or reverses it, or only names a void the chain already wrote, and what of the convicted
/// work's rights it forfeits (addendum §4-bis.7: `execution_proving` became these two).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFalseValidFindingV1 {
    pub target: PalwOffenceTargetV1,
    pub site: PalwFaultSiteV1,
    /// The claim is proven false (execution- or claim-proving): void it before `Final`, reverse
    /// its `Final` after.
    pub acts_on_claim: bool,
    pub forfeit: PalwForfeitScopeV1,
}

/// **Kind 4's verdict**: the claim whose executor is refuted, and what of its rights the
/// conviction forfeits. The executor always acts on the claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecutorRefutedFindingV1 {
    pub target: PalwOffenceTargetV1,
    pub forfeit: PalwForfeitScopeV1,
    /// The identity check that failed, for a 9 (`None` for every other contradiction).
    pub identity_fault: Option<PalwIdentityFaultV1>,
}

/// **The signature half of the adjudicator, which only the processor runs.** The fold passes
/// `None`: it folds only what the processor admitted, and holds no verifier.
pub struct PalwFalseValidSigCheckV1<'a> {
    /// `palw_network_domain_v2_for(network id, genesis)` — the chain's, never the payload's.
    pub chain_domain: Hash64,
    /// The key the accused seat's bond registered, read from state.
    pub seat_pubkey: &'a [u8],
    /// The bond is `Active` or `Retiring`.
    pub seat_active: bool,
    /// `(pubkey, message, signature, context) -> verified`.
    pub verify: &'a dyn Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
}

/// How a contradiction reaches a conviction, when it is admitted at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFalseValidAdmissionV1 {
    /// A void the chain wrote on this claim for a proven false execution (`CourtFraud`), tied to
    /// state.
    NamedVoid { reason: PalwVoidReasonV2, voided_daa: u64 },
    /// A proof against the committed execution itself (`StepArithmetic`, `StepStructural`,
    /// `ForgedOutput`), pinned to the claim's root.
    ExecutionProving,
    /// **F1: a proof that the claim's committed execution answers another job or class
    /// (`IdentityMismatch`), or that its output root is not that execution's output
    /// (`OutputMismatch`)** — pinned to the claim's root and its recorded identity. It proves the
    /// CLAIM false, not the root: a borrowed root may be an honest lender's.
    ClaimProving,
}

/// `H(domain ‖ claim)` — the evidence half of a V2 offence id, which names the claim and nothing
/// the filer chose.
pub fn palw_false_valid_ledger_key_v2(claim_id: &Hash64) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_FALSE_VALID_KEY_DOMAIN_V2).to_state();
    h.update(claim_id.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **One offence per (seat, claim)**: `palw_offence_id_v1(PanelFalseValidV2, seat, H(claim))`.
/// The contradiction, the receipt form, the reporter slot and the bytes are all outside it, so a
/// second proof of the same false `Valid` is the same offence and the ledger pays it once.
pub fn palw_false_valid_offence_id_v2(accused: &TransactionOutpoint, claim_id: &Hash64) -> Hash64 {
    crate::palw_offence_v1::palw_offence_id_v1(
        crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2,
        accused,
        &palw_false_valid_ledger_key_v2(claim_id),
    )
}

/// **The target of a false-Valid, from state**: the live claim first, then its liability row —
/// the order the processor's V1 gate and the fold's `false_valid_execution_roots` resolve in. A
/// live claim's segment cut is its bound panel's (`palw_segment_count_v2` of the seat count; the
/// panel leaves only when the claim retires).
pub fn palw_offence_target_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Option<PalwOffenceTargetV1> {
    let artifact_of = |class_id: &Hash64| state.class(class_id).map(|class| class.artifact_root).unwrap_or_default();
    if let Some(claim) = state.claim(claim_id) {
        let lane = match claim.source {
            PalwClaimSourceV2::Attempt => PalwClaimSourceKindV1::Attempt,
            PalwClaimSourceV2::FreePrompt { .. } => PalwClaimSourceKindV1::FreePrompt,
        };
        let segment_count =
            state.panel(claim_id).map(|panel| palw_segment_count_v2(u16::try_from(panel.seats.len()).unwrap_or(u16::MAX)));
        return Some(PalwOffenceTargetV1 {
            claim_id: *claim_id,
            class_id: claim.class_id,
            artifact_root: artifact_of(&claim.class_id),
            executor_bond: claim.bond,
            execution_root: claim.execution_root,
            lane: Some(lane),
            segment_count,
            phase: Some(claim.phase.clone()),
            job_identity: claim.job_identity,
            trace_root: claim.trace_root,
            output_root: claim.output_root,
        });
    }
    // A retired claim's row records its lane and its segment cut only where it recorded its
    // identity (F1, `persist_panel_liability` past `palw_offence_attribution`); a row written below
    // the fence reads as F2 read it — lane and cut unknown.
    let recorded_lane = |job_identity: &Hash64, free_prompt: bool| {
        (*job_identity != Hash64::default()).then_some(if free_prompt {
            PalwClaimSourceKindV1::FreePrompt
        } else {
            PalwClaimSourceKindV1::Attempt
        })
    };
    if let Some(row) = state.panel_liability(claim_id) {
        return Some(PalwOffenceTargetV1 {
            claim_id: *claim_id,
            class_id: row.class_id,
            artifact_root: artifact_of(&row.class_id),
            executor_bond: row.executor_bond,
            execution_root: row.execution_root,
            lane: recorded_lane(&row.job_identity, row.free_prompt),
            segment_count: (row.segment_count != 0).then_some(row.segment_count),
            phase: None,
            job_identity: row.job_identity,
            trace_root: row.trace_root,
            output_root: row.output_root,
        });
    }
    // ADR-0152 v3.1 J-2 (R-core): the vesting row is the third source. Liability rows are not pruned
    // while a vesting row exists (X29), so this is a fallback, not a path the live chain expects;
    // the map is empty wherever `palw_rcore_plus` is dormant.
    let row = state.vesting_row(claim_id)?;
    Some(PalwOffenceTargetV1 {
        claim_id: *claim_id,
        class_id: row.class_id,
        artifact_root: row.artifact_root,
        executor_bond: row.producer_bond,
        execution_root: row.execution_root,
        lane: recorded_lane(&row.job_identity, row.free_prompt),
        segment_count: (row.segment_count != 0).then_some(row.segment_count),
        phase: None,
        job_identity: row.job_identity,
        trace_root: row.trace_root,
        output_root: Hash64::default(),
    })
}

/// **The mask the claim's panel assigned `seat`** (F2 review, F-4) — the coverage licence's own
/// reading (`palw_panel_v2`'s `MaskNotAssigned`: `palw_segment_assignment_v2` of the panel's
/// anchor, the claim and the seat count, at the seat's position in the panel). `None` when no
/// panel is in state; [`PalwSegmentMaskV2::NONE`] for a bond the panel does not seat.
pub fn palw_false_valid_assigned_mask_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    seat: &PalwBondKeyV2,
) -> Option<PalwSegmentMaskV2> {
    let panel = state.panel(claim_id)?;
    let assignment = palw_segment_assignment_v2(panel.anchor, *claim_id, u16::try_from(panel.seats.len()).unwrap_or(u16::MAX));
    Some(
        panel
            .seats
            .iter()
            .position(|s| s.bond == *seat)
            .and_then(|index| u16::try_from(index).ok())
            .map_or(PalwSegmentMaskV2::NONE, |index| assignment.mask_of(index)),
    )
}

/// **Which contradictions may convict a `Valid` signer past the fence, and how.**
///
/// Refused by name, each for a stated reason — a route the chain cannot tie to THIS claim's
/// execution is a route to convict an honest seat:
///
/// * `ProducerWithholding` (F2 review, F-2): the void a producer's own silence writes. A bystander
///   files `DefaultAccusedHeld`, the colluding producer stays silent, the claim voids — and the
///   honest full seat, which owes no disclosure and has no move in that session, would lose its
///   whole lock. Refused HERE, always; past `palw_rcore_plus` M3's court gives the seat the move it
///   lacked (any locked signer answers, X7) and [`palw_check_panel_false_valid_v2`] admits the
///   contradiction before this list is read, but only as the restatement of a DA-7 default (N9,
///   [`palw_da_default_confirms_withholding_v1`]);
/// * `ExecutorEquivocation`: it proves a key signed two roots, not that this claim's root is
///   false, and no production code builds the attestations it carries;
/// * `CourtExecutorGuilty`: no code ever writes the consumed row it names;
/// * `ConflictingPermit`: a burned permit is tied to no claim;
/// * `Legs`: it rebuilds a v1 execution root, and this network commits v2 roots.
pub fn palw_false_valid_admission_v1(
    contradiction: &PalwPanelContradictionV1,
) -> Result<PalwFalseValidAdmissionV1, PalwOffenceVerifyError> {
    use PalwPanelContradictionV1 as C;
    match contradiction {
        C::CourtFraud { voided_daa } => {
            Ok(PalwFalseValidAdmissionV1::NamedVoid { reason: PalwVoidReasonV2::CourtFraud, voided_daa: *voided_daa })
        }
        C::StepArithmetic { .. }
        | C::StepStructural(_)
        | C::ForgedOutput { .. }
        | C::ForgedOutputTiled { .. }
        | C::LogitsNotStepOutput { .. } => Ok(PalwFalseValidAdmissionV1::ExecutionProving),
        C::IdentityMismatch { .. } | C::OutputMismatch { .. } | C::PromptNotAnchored { .. } => {
            Ok(PalwFalseValidAdmissionV1::ClaimProving)
        }
        C::ProducerWithholding { .. } => Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "ProducerWithholding convicts a Valid signer only as the restatement of a DA-confirmed default (ADR-0152 N9): no \
             DA session defaulted on this claim at that DAA, and a seat owes no disclosure it could answer any other void with",
        )),
        C::ExecutorEquivocation(_) => Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "ExecutorEquivocation proves a key signed two roots, not that this claim's execution is false",
        )),
        C::CourtExecutorGuilty { .. } => {
            Err(PalwOffenceVerifyError::ContradictionNotAdmitted("CourtExecutorGuilty names a consumed row no code ever writes"))
        }
        C::ConflictingPermit { .. } => Err(PalwOffenceVerifyError::ContradictionNotAdmitted("ConflictingPermit is tied to no claim")),
        C::Legs(_) => Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "Legs rebuilds a v1 execution root, and this network commits v2 roots",
        )),
    }
}

/// What the structural pass answered about one opened step leaf.
enum PalwStructuralPassV1 {
    /// A shape fault, answered from the binding alone before anything was opened.
    Shape,
    /// A fault in the opened leaf itself: its coordinates, index, length or encoding.
    LeafFault,
    /// The leaf is well-formed (for a `StepArithmetic`, the recomputation decides).
    Clean,
}

fn structural_pass(
    refutation: &crate::palw_step_leg::PalwStepRefutationV1,
    step_ladder: u64,
) -> Result<PalwStructuralPassV1, PalwOffenceVerifyError> {
    let root = &refutation.binding.committed_execution_root;
    match crate::palw_step_leg::check_step_refutation_capped_v1(refutation, step_ladder) {
        // The shape pass mints its evidence id at kind 0, leaf 0; every opened verdict at its own
        // kind and leaf. That id is the one mark of which pass answered.
        Ok(verdict) if verdict.evidence_id == crate::palw_step_leg::step_refutation_evidence_id(root, 0, 0, verdict.fault) => {
            Ok(PalwStructuralPassV1::Shape)
        }
        Ok(_) => Ok(PalwStructuralPassV1::LeafFault),
        Err(crate::palw_step_leg::PalwStepLegError::NoFaultFound) => Ok(PalwStructuralPassV1::Clean),
        Err(_) => Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction),
    }
}

/// A fault in `leaf` alone — placeable only on a main step coordinate. A segment replay compares
/// the step tiles of its segment and nothing else, so a KV aux leaf (or any index past the main
/// step space) is no segment's to have attested: only a whole attestation covers it.
fn leaf_alone_site(binding: &crate::palw_step_leg::PalwStepBindingV2, leaf: u64) -> PalwFaultSiteV1 {
    match crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf) {
        Some(_) => PalwFaultSiteV1::leaf_alone(leaf),
        None => PalwFaultSiteV1::Whole,
    }
}

/// **The site of a recomputed step: the output leaf and every committed leaf it was recomputed
/// from** (F2 review, F-1). The refutation's inputs are exactly the canonical input set, each
/// preimage's coordinates checked equal to the canonical ones and every run opened against the
/// committed step root, so their leaf indices are the chain's, not the filer's.
///
/// `Whole` when the verdict read anything a segment replay does not recompute from its own resume
/// point: a KV checkpoint anchor (the history arrives as committed state), or a generated token —
/// the decode pin, which the checker reads only at a decode call (`call_index > 0`; a prefill
/// gather reads the prompt), and which a segment replay derives itself from its own logits. A
/// filer whose step reads no generated token can omit the pin; the checker refuses a gather that
/// needed it, so the omission is proof it was not read.
fn arithmetic_site(refutation: &crate::palw_step_refute::PalwExecutionStepRefutationV1) -> PalwFaultSiteV1 {
    if refutation.kv_checkpoint.is_some() || (refutation.decode_tokens.is_some() && refutation.output_preimage.coord.call_index > 0) {
        return PalwFaultSiteV1::Whole;
    }
    let (profile, context) = (&refutation.binding.shape_profile, &refutation.binding.job_context);
    let leaf = refutation.output_opening.leaf_index;
    let (mut first_read, mut last_read) = (leaf, leaf);
    for preimage in refutation.inputs.iter().flat_map(|row| row.preimages.iter()) {
        let Some(index) = crate::palw_step::canonical_step_leaf_index(profile, context, &preimage.coord) else {
            return PalwFaultSiteV1::Whole;
        };
        first_read = first_read.min(index);
        last_read = last_read.max(index);
    }
    PalwFaultSiteV1::Leaf { leaf, first_read, last_read }
}

/// **The site of a proven fault, and the committed leaf count its segment is cut from** (`0` for
/// [`PalwFaultSiteV1::Whole`], where no cut is read). Called only after the contradiction
/// convicted against the claim's root.
///
/// A step refutation's site is read from the VERDICT, not from the opening it carries. The
/// structural pass answers a shape fault — a non-canonical leaf count, profile or checkpoint count
/// — from the binding alone, before it opens anything, so on that answer the carried
/// `leaf_index` is whatever the filer wrote; taking it as the site would let a filer aim a
/// whole-execution fault at any partial seat it chose. A shape verdict is therefore `Whole`. A
/// fault the structural pass found in the opened leaf is that leaf's alone; a recomputed step
/// (reached only once the structural pass found the output leaf well-formed) sits at its output
/// leaf and reads its inputs ([`arithmetic_site`]).
pub fn palw_false_valid_fault_site_v1(
    contradiction: &PalwPanelContradictionV1,
    step_ladder: u64,
) -> Result<(PalwFaultSiteV1, u64), PalwOffenceVerifyError> {
    use crate::palw_step_leg::{PalwStepEvidenceV1, PalwStepRefutationV1};
    match contradiction {
        PalwPanelContradictionV1::StepArithmetic { refutation, .. } => {
            // The arithmetic check's own first step, run again for its verdict alone.
            let structural = PalwStepRefutationV1 {
                binding: refutation.binding.clone(),
                evidence: PalwStepEvidenceV1::StepTile {
                    opening: refutation.output_opening.clone(),
                    preimage: refutation.output_preimage.clone(),
                },
            };
            let count = refutation.binding.step_leaf_count;
            Ok(match structural_pass(&structural, step_ladder)? {
                PalwStructuralPassV1::Shape => (PalwFaultSiteV1::Whole, 0),
                PalwStructuralPassV1::LeafFault => (leaf_alone_site(&refutation.binding, refutation.output_opening.leaf_index), count),
                PalwStructuralPassV1::Clean => (arithmetic_site(refutation), count),
            })
        }
        PalwPanelContradictionV1::StepStructural(refutation) => match &refutation.evidence {
            PalwStepEvidenceV1::StepTile { opening, .. } | PalwStepEvidenceV1::KvChunk { opening, .. } => {
                Ok(match structural_pass(refutation, step_ladder)? {
                    PalwStructuralPassV1::Shape => (PalwFaultSiteV1::Whole, 0),
                    PalwStructuralPassV1::LeafFault | PalwStructuralPassV1::Clean => {
                        (leaf_alone_site(&refutation.binding, opening.leaf_index), refutation.binding.step_leaf_count)
                    }
                })
            }
            PalwStepEvidenceV1::Shape | PalwStepEvidenceV1::Checkpoint { .. } | PalwStepEvidenceV1::CheckpointChain { .. } => {
                Ok((PalwFaultSiteV1::Whole, 0))
            }
        },
        // F1/F1c: 10, 11 and 12 at `Whole` — a partial seat never sees the output, the logits
        // trace or the argmax — and 9 and 13 at `Whole` HERE: which of their faults are job faults
        // every `Valid` attests (`AnyValid`) is known only once the identity rule has named the
        // failing check, so [`palw_claim_proving_site_v1`] raises them after adjudication.
        PalwPanelContradictionV1::ForgedOutput { .. }
        | PalwPanelContradictionV1::ForgedOutputTiled { .. }
        | PalwPanelContradictionV1::LogitsNotStepOutput { .. }
        | PalwPanelContradictionV1::PromptNotAnchored { .. }
        | PalwPanelContradictionV1::ProducerWithholding { .. }
        | PalwPanelContradictionV1::CourtFraud { .. }
        | PalwPanelContradictionV1::ExecutorEquivocation(_)
        | PalwPanelContradictionV1::CourtExecutorGuilty { .. }
        | PalwPanelContradictionV1::ConflictingPermit { .. }
        | PalwPanelContradictionV1::Legs(_)
        | PalwPanelContradictionV1::IdentityMismatch { .. }
        | PalwPanelContradictionV1::OutputMismatch { .. } => Ok((PalwFaultSiteV1::Whole, 0)),
    }
}

/// **The site of a claim-proving fault, once the identity rule has named it** (addendum §4-bis.7,
/// the user's decision of 2026-09-24): `AnyValid` for a job fault — 9's J2 (the class), J1 (the
/// anchor), J3 (the seed), J5a (the canonical context), J5b (the prompt root) and 13 (the prompt)
/// — which a partial seat's SEAT-S4 opening checked field by field before it signed; `None` (keep
/// the table's `Whole`) for 9's J4/J6/J7 (a trace root, an activation leg, a checkpoint profile: no
/// partial seat reads them) and for everything else.
pub fn palw_claim_proving_site_v1(
    contradiction: &PalwPanelContradictionV1,
    identity_fault: Option<PalwIdentityFaultV1>,
) -> Option<PalwFaultSiteV1> {
    use PalwIdentityFaultV1 as J;
    match contradiction {
        PalwPanelContradictionV1::PromptNotAnchored { .. } => Some(PalwFaultSiteV1::AnyValid),
        PalwPanelContradictionV1::IdentityMismatch { .. } => match identity_fault? {
            J::ClassNotTheClaims | J::JobNotTheClaims | J::SeedNotTheJobs | J::ContextNotCanonical | J::PromptNotTheAnchors => {
                Some(PalwFaultSiteV1::AnyValid)
            }
            J::TraceNotTheClaims | J::ActivationLegNotCanonical | J::CheckpointProfileNotCanonical => None,
        },
        _ => None,
    }
}

/// **Whether a receipt attested everything a fault's verdict rests on** — the seat is liable only
/// for what it said it replayed.
///
/// * A full receipt attested the whole job: always liable.
/// * A segmented receipt whose mask is the full mask over the claim's `k` segments is a full
///   attestation, whatever the seat's assignment: a seat assigned a segment that replayed the whole
///   job and signed for all of it is held to all of it.
/// * A partial mask must be the one the panel assigned the seat (`assigned_mask`, the coverage
///   licence's `MaskNotAssigned` rule) — otherwise `SegmentMaskNotAssigned`. It is then liable
///   only at a [`PalwFaultSiteV1::Leaf`] whose leaf and every leaf the verdict read lie in segments
///   the mask covers (segments are cut from the binding's committed `step_leaf_count`, as the
///   producer cut them; every segment between the first and the last read is asked for, which is
///   exact for the one-segment masks a panel assigns), and always at [`PalwFaultSiteV1::AnyValid`]
///   (a job fault its SEAT-S4 opening checked). Never at `Whole`.
///
/// Refused with `SiteNotAttested` when the receipt does not reach all of the site, and
/// `SegmentsUnknown` when `k` is not in state and the mask is neither empty nor placeable without it.
pub fn palw_false_valid_liable_v1(
    receipt: &PalwFalseValidReceiptV1,
    site: PalwFaultSiteV1,
    segment_count: Option<u16>,
    assigned_mask: Option<PalwSegmentMaskV2>,
    step_leaf_count: u64,
) -> Result<(), PalwOffenceVerifyError> {
    let mask = match receipt {
        PalwFalseValidReceiptV1::Full(_) => return Ok(()),
        PalwFalseValidReceiptV1::Segmented(signed) => signed.segments,
    };
    if mask == PalwSegmentMaskV2::NONE {
        return Err(PalwOffenceVerifyError::SiteNotAttested);
    }
    let Some(k) = segment_count else {
        return Err(PalwOffenceVerifyError::SegmentsUnknown);
    };
    if mask.is_full(k) {
        return Ok(());
    }
    if assigned_mask != Some(mask) {
        return Err(PalwOffenceVerifyError::SegmentMaskNotAssigned);
    }
    match site {
        PalwFaultSiteV1::Whole => Err(PalwOffenceVerifyError::SiteNotAttested),
        // A job fault every `Valid` attested: the seat's own assigned partial mask is liable too,
        // wherever its segment sits (the assignment check above still holds a mask to the one the
        // panel drew — a receipt no licence would take is not a `Valid` this rule reads).
        PalwFaultSiteV1::AnyValid => Ok(()),
        PalwFaultSiteV1::Leaf { leaf, first_read, last_read } => {
            let segment_of = |l: u64| palw_segment_index_of_leaf_v2(step_leaf_count, k, l);
            match (segment_of(first_read.min(leaf)), segment_of(last_read.max(leaf))) {
                (Some(first), Some(last)) if (first..=last).all(|segment| mask.covers(segment)) => Ok(()),
                _ => Err(PalwOffenceVerifyError::SiteNotAttested),
            }
        }
    }
}

/// **Whether an execution-proving contradiction proves the claim's committed execution false.**
///
/// With no prompt-id opening this IS the V1 route's check
/// ([`palw_panel_contradiction_convicts_execution_v1`]), for every kind, byte for byte: a
/// `StepArithmetic` on a flat commitment carries the whole id list and is matched against the flat
/// digest. With an opening (F2 review, F-3) a `StepArithmetic` is judged by the one-move court's
/// opened check, [`crate::palw_step_refute::check_execution_step_refutation_opened_capped_v1`]: the
/// one tile the step reads, opened against the job's Merkle root before an id is read — so the
/// evidence grows with a path and never with the prompt, and on a Merkle commitment (testnet-12
/// from genesis) a whole list is refused by the flat comparison exactly as the checker refuses it.
/// The job's commitment is the discriminator; neither the network's nor the class's form is read.
/// An opening beside any other contradiction is refused.
pub fn palw_false_valid_convicts_execution_v2(
    contradiction: &PalwPanelContradictionV1,
    prompt_ids_opening: Option<&PalwPromptIdsOpeningV1>,
    claim_execution_root: Hash64,
    class_artifact_root: Hash64,
    step_ladder: u64,
) -> Result<(), PalwOffenceVerifyError> {
    let Some(opening) = prompt_ids_opening else {
        return palw_panel_contradiction_convicts_execution_v1(contradiction, claim_execution_root, class_artifact_root, step_ladder);
    };
    let PalwPanelContradictionV1::StepArithmetic { refutation, operand_openings } = contradiction else {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    };
    if refutation.binding.committed_execution_root != claim_execution_root {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(operand_openings, class_artifact_root)
        .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
    crate::palw_step_refute::check_execution_step_refutation_opened_capped_v1(refutation, &operands, Some(opening), step_ladder)
        .map(|_| ())
        .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)
}

/// **F1's identity rule** (ADR-0152 v3.1 J-5; the audit's SPEC §4.3, addendum §4-bis.2): does
/// `binding` — the claim's own committed execution — answer the job and class the claim RECORDED?
///
/// Refused (no verdict either way, nothing is written) when the binding does not reproduce its own
/// root (`BindingUnverified`) or is not the claim's (`PanelFalseValidWorkMismatch`), when the claim
/// recorded no identity (`IdentityNotRecorded`: 0 never convicts) and when its lane is unknown
/// (`LaneUnknown`). Otherwise the FIRST failing check, in the addendum's order:
///
/// | Check | Fault |
/// |---|---|
/// | J2 | the binding's profile is not the claim's class |
/// | J1 | attempt: `job_id` is not the recorded anchor; free prompt: the context's pin is not the commitment's |
/// | J3 | attempt: `execution_seed` is not the anchor's first 32 bytes |
/// | J5a | attempt: the whole context is not the `CoreV1` one the anchor implies at the class's canonical job (the floor's, or the model formula `(n_ctx/8 − 1, 2)`); a class too narrow for the formula skips J5 and, if J4/J6/J7 find nothing, is refused `IdentityNotDerivable` |
/// | J5b | the same, a canonical prompt of at most 4,096 ids: the prompt root is not the anchor's (a longer one is `PromptNotAnchored`'s) |
/// | J4 | the logits trace root is not the claim's |
/// | J6 | the activation leg is not `palw_int_activation_leg_root_v1(ctx)` (both lanes) |
/// | J7 | the checkpoint profile is not `palw_canonical_checkpoint_profile_v1(profile)` (both lanes) |
///
/// `with_prompt_root = false` leaves J5b out — the data-availability answers run every other check
/// (SPEC §4.6: its cost belongs to the contradictions).
///
/// An honest binding is its own job's by construction, so no check here can fault an honest
/// producer: its context is the anchor's canonical one (the rule its own seats hold it to), its
/// profile is its class's and its trace root is the one it committed.
pub fn palw_binding_identity_fault_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    rules: PalwIdentityRulesV1,
    with_prompt_root: bool,
) -> Result<Option<PalwIdentityFaultV1>, PalwOffenceVerifyError> {
    use crate::palw_attempt_rules_v1::{
        PALW_J5_INLINE_PROMPT_IDS_V1, palw_attempt_canonical_v1, palw_attempt_context_v1, palw_attempt_prompt_root_v1,
    };
    crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| PalwOffenceVerifyError::BindingUnverified)?;
    if binding.committed_execution_root != target.execution_root {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    let identity = target.job_identity;
    if identity == Hash64::default() {
        return Err(PalwOffenceVerifyError::IdentityNotRecorded);
    }
    let lane = target.lane.ok_or(PalwOffenceVerifyError::LaneUnknown)?;
    let (ctx, profile) = (&binding.job_context, &binding.shape_profile);
    // A class too narrow for the formula derives no attempt context (J5), and that is a refusal
    // only once every OTHER check has found nothing (the 3a review's L-a): J4, J6 and J7 read no
    // canonical job, so a narrow class must not escape them too.
    let mut not_derivable = false;
    // J2 — the class. Every check below reads the profile, so it must be the claim's first.
    if profile.shape_profile_id() != target.class_id {
        return Ok(Some(PalwIdentityFaultV1::ClassNotTheClaims));
    }
    match lane {
        PalwClaimSourceKindV1::FreePrompt => {
            // J1, free prompt: the commitment's pin, recomputed from the context.
            if crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx) != identity {
                return Ok(Some(PalwIdentityFaultV1::JobNotTheClaims));
            }
        }
        PalwClaimSourceKindV1::Attempt => {
            // J1 and J3: the anchor names the job and seeds it.
            if ctx.job_id != identity {
                return Ok(Some(PalwIdentityFaultV1::JobNotTheClaims));
            }
            if ctx.execution_seed[..] != identity.as_byte_slice()[..32] {
                return Ok(Some(PalwIdentityFaultV1::SeedNotTheJobs));
            }
            // J5: the whole context `CoreV1` derives from the anchor at the class's canonical job.
            match palw_attempt_canonical_v1(profile, target.class_id == rules.base_class_id) {
                None => not_derivable = true,
                Some(canonical) => {
                    let expected = palw_attempt_context_v1(profile, &identity, canonical, ctx.prompt_token_ids_hash);
                    if ctx.context_hash() != expected.context_hash() {
                        return Ok(Some(PalwIdentityFaultV1::ContextNotCanonical));
                    }
                    if with_prompt_root && canonical.0 <= PALW_J5_INLINE_PROMPT_IDS_V1 {
                        let root = palw_attempt_prompt_root_v1(profile, &identity, canonical.0, rules.prompt_ids_form)
                            .ok_or(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
                        if ctx.prompt_token_ids_hash != root {
                            return Ok(Some(PalwIdentityFaultV1::PromptNotTheAnchors));
                        }
                    }
                }
            }
        }
    }
    // J4 — the trace root the claim committed.
    if binding.full_logits_trace_root != target.trace_root {
        return Ok(Some(PalwIdentityFaultV1::TraceNotTheClaims));
    }
    // J6 and J7 (addendum §4-bis.2), both lanes: the two leg parts a filer would otherwise choose
    // freely with a real preimage (the probe's T2 and T3).
    if binding.activation_leg_root != crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(ctx) {
        return Ok(Some(PalwIdentityFaultV1::ActivationLegNotCanonical));
    }
    if binding.checkpoint_profile != crate::palw_attempt_rules_v1::palw_canonical_checkpoint_profile_v1(profile) {
        return Ok(Some(PalwIdentityFaultV1::CheckpointProfileNotCanonical));
    }
    if not_derivable {
        return Err(PalwOffenceVerifyError::IdentityNotDerivable);
    }
    Ok(None)
}

/// **F1's output rule** (ADR-0152 v3.1 J-5, `OutputMismatch`; addendum §4-bis.4): is the claim's
/// committed `output_root` the output its committed execution generated?
///
/// The binding must be the claim's (its root) and the class's (J2 is a refusal here: file
/// `IdentityMismatch`), and the claim must have recorded its identity. The pin names the generated
/// ids and is authenticated against the binding's logits trace root by the class's own scheme
/// (`Base0V1` flat, `TiledV1` tiled; `FloatV2` is refused). The fault is
/// `output_commitment_v2(ctx, ids, rendered_output_hash_v2(&[])) != output_root` — the one rendered
/// rule, attempts and free prompts alike.
///
/// Every class, both lanes: past `palw_offence_attribution` every producer commits its output root
/// by `CoreV1`'s one rendered rule (ADR-0152 v3.1 post-edit 4 unified the free-prompt lane's too),
/// so the rule holds a model class's claim as it holds the floor's.
pub fn palw_output_fault_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    pin: &crate::palw_step_refute::PalwDecodeTokenPinV1,
) -> Result<bool, PalwOffenceVerifyError> {
    use crate::palw_step_refute::PalwDecodeTokenPinV1 as Pin;
    crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| PalwOffenceVerifyError::BindingUnverified)?;
    if binding.committed_execution_root != target.execution_root {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    if target.job_identity == Hash64::default() {
        return Err(PalwOffenceVerifyError::IdentityNotRecorded);
    }
    if target.lane.is_none() {
        return Err(PalwOffenceVerifyError::LaneUnknown);
    }
    if binding.shape_profile.shape_profile_id() != target.class_id {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "the binding is another class's: that is an IdentityMismatch (J2), not an output question",
        ));
    }
    if target.output_root == Hash64::default() {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted("the claim's output root is no longer recorded"));
    }
    let ids = match pin {
        Pin::Base0V1(tokens) => {
            crate::palw_step_refute::check_base0_decode_pin(binding, tokens)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            &tokens.generated_token_ids
        }
        Pin::TiledV1(tokens) => {
            crate::palw_step_refute::check_tiled_decode_pin(binding, tokens)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            &tokens.generated_token_ids
        }
        Pin::FloatV2(_) => return Err(PalwOffenceVerifyError::PinNotAdmitted("a Float32 class's pin")),
    };
    Ok(crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, ids) != target.output_root)
}

/// The common prefix of 11, 12 and 13: the binding verifies, it IS the claim's execution, and it
/// is the claim's class (J2 is `IdentityMismatch`'s, a refusal here).
fn palw_claims_own_binding_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
) -> Result<(), PalwOffenceVerifyError> {
    crate::palw_step_leg::verify_binding_v1(binding).map_err(|_| PalwOffenceVerifyError::BindingUnverified)?;
    if binding.committed_execution_root != target.execution_root {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    if binding.shape_profile.shape_profile_id() != target.class_id {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "the binding is another class's: that is an IdentityMismatch (J2)",
        ));
    }
    Ok(())
}

/// **`ForgedOutputTiled` (11)** (addendum §4-bis.5): a TILED integer class's committed token is not
/// its row's selection (`NotSelected`, the tiled decode-token close:
/// [`crate::palw_step_refute::check_tiled_decode_token_refutation_capped_v1`]) or is past the
/// vocabulary (`OutOfVocab`, the ids pinned by the rows root). `Ok(())` convicts; `TokenHolds` is the
/// honest answer. `verify_binding` runs first — the tiled checker authenticates rows against the
/// binding's trace root and never asks whether the binding is the claim's, so without it a copied
/// root with a bent binding convicted an honest claim. A flat class files `ForgedOutput` (8); under
/// ADR-0082's decode rules a free-prompt claim's token is not the greedy argmax and is refused.
pub fn palw_forged_output_tiled_fault_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    proof: &crate::palw_offence_v1::PalwForgedOutputTiledProofV1,
    ladder: u64,
    fp_decode_rules_active: bool,
) -> Result<(), PalwOffenceVerifyError> {
    use crate::palw_offence_v1::PalwForgedOutputTiledProofV1 as P;
    palw_claims_own_binding_v1(target, binding)?;
    let profile = &binding.shape_profile;
    if profile.lane != crate::palw_step::PalwStepLaneV1::Int32
        || profile.logits_scheme_id != crate::palw_step_refute::tiled_logits_scheme_id_v1()
    {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "ForgedOutputTiled judges a tiled integer class; a flat class's forged token is ForgedOutput (8)",
        ));
    }
    if fp_decode_rules_active && target.lane != Some(PalwClaimSourceKindV1::Attempt) {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "under ADR-0082's decode rules a free-prompt token is not the greedy selection",
        ));
    }
    match proof {
        P::NotSelected { pin } => match crate::palw_step_refute::check_tiled_decode_token_refutation_capped_v1(binding, pin, ladder) {
            Ok(verdict) if matches!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { .. }) => Ok(()),
            Ok(_) => Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction),
            Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound) => Err(PalwOffenceVerifyError::TokenHolds),
            Err(_) => Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction),
        },
        P::OutOfVocab { position, tokens } => {
            crate::palw_step_refute::check_tiled_decode_pin(binding, tokens)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            let id =
                tokens.generated_token_ids.get(*position as usize).ok_or(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            if *id >= profile.vocab_size { Ok(()) } else { Err(PalwOffenceVerifyError::TokenHolds) }
        }
    }
}

/// **`LogitsNotStepOutput` (12)** (addendum §4-bis.6): a committed logits row is not its step
/// tree's head output. In order — every refusal before the one comparison that convicts:
///
/// 1. the event opens a row (`OutOfRange` is refused); its binding is the claim's (verify, root,
///    class);
/// 2. the class's graph passes [`crate::palw_step::palw_logits_head_v1`], else `HeadUnproven`;
/// 3. `row` is a committed row and `head_tile` a head tile;
/// 4. the binding's shape is clean (`check_step_refutation_capped_v1` on the `Shape` evidence answers
///    `NoFaultFound`) — a non-canonical shape is `StepStructural`'s (6), and the head coordinate
///    below is only meaningful on a canonical one;
/// 5. the event authenticates the row's lanes against the claim's trace root
///    ([`crate::palw_step_refute::check_trace_event_disclosure_v1`], at the logits tile holding the
///    head tile — `0` on the flat scheme);
/// 6. the head leaf's coordinate is DERIVED (row, `G − 1`, `P − 1` or `0`, `head_tile`), never
///    carried, and `head_opening` names its canonical index and walks to the binding's step root;
/// 7. **the fault**: the step leaf those lanes hash to (`step_tile_leaf_hash_v1`) is not the leaf the
///    claim committed there. `LogitsHold` otherwise.
///
/// An honest producer cannot be convicted: its binding reproduces the root, so both sides are that
/// execution's own committed values and the head's output IS its logits row (Tier B,
/// `f1c_logits_not_step_output`). No sampler is read.
pub fn palw_logits_not_step_output_fault_v1(
    target: &PalwOffenceTargetV1,
    event: &crate::palw_step_refute::PalwTraceEventDisclosureV1,
    row: u32,
    head_tile: u32,
    head_opening: &crate::palw_step_leg::PalwStepOpeningV1,
    ladder: u64,
) -> Result<(), PalwOffenceVerifyError> {
    use crate::palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwTraceEventDisclosureV1 as Ev};
    let needs = PalwOffenceVerifyError::PanelFalseValidNeedsContradiction;
    if matches!(event, Ev::OutOfRange { .. }) {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted("an OutOfRange disclosure opens no logits row"));
    }
    let binding = event.binding();
    palw_claims_own_binding_v1(target, binding)?;
    let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
    let head = crate::palw_step::palw_logits_head_v1(profile).ok_or(PalwOffenceVerifyError::HeadUnproven)?;
    if row >= ctx.exact_decode_tokens || head_tile >= head.tiles {
        return Err(needs);
    }
    let shape = crate::palw_step_leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: crate::palw_step_leg::PalwStepEvidenceV1::Shape,
    };
    if crate::palw_step_leg::check_step_refutation_capped_v1(&shape, ladder)
        != Err(crate::palw_step_leg::PalwStepLegError::NoFaultFound)
    {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "the binding's shape is not canonical: that is StepStructural's (6), and no head coordinate is meaningful on it",
        ));
    }
    let vocab = profile.vocab_size as usize;
    let lo = head_tile as usize * head.tile_len as usize;
    let hi = (lo + head.tile_len as usize).min(vocab);
    let flat = profile.logits_scheme_id == crate::palw_step_refute::flat_logits_scheme_id_v1();
    let logits_tile = if flat { 0usize } else { lo / PALW_LOGITS_TILE_LANES };
    let logits_tile_u8 = u8::try_from(logits_tile).map_err(|_| PalwOffenceVerifyError::HeadUnproven)?;
    crate::palw_step_refute::check_trace_event_disclosure_v1(
        binding.full_logits_trace_root,
        target.execution_root,
        row,
        logits_tile_u8,
        event,
        ladder,
    )
    .map_err(|_| needs.clone())?;
    let lanes: &[i32] = match event {
        Ev::Flat { pin, .. } => pin.logits_rows.get(row as usize).and_then(|r| r.get(lo..hi)).ok_or(needs.clone())?,
        Ev::Tiled { tile_lanes, .. } => {
            let base = logits_tile * PALW_LOGITS_TILE_LANES;
            tile_lanes.get(lo - base..hi - base).ok_or(needs.clone())?
        }
        Ev::OutOfRange { .. } => return Err(needs),
    };
    let coord = crate::palw_step::palw_logits_head_coordinate_v1(&head, ctx, row, head_tile).ok_or(needs.clone())?;
    let index = crate::palw_step::canonical_step_leaf_index(profile, ctx, &coord).ok_or(needs.clone())?;
    if head_opening.leaf_index != index {
        return Err(needs);
    }
    let root =
        crate::palw_step_leg::step_opening_root_capped_v1(binding.step_leaf_count, head_opening, ladder).map_err(|_| needs.clone())?;
    if root != binding.step_merkle_root {
        return Err(needs);
    }
    let values_le: Vec<u8> = lanes.iter().flat_map(|v| v.to_le_bytes()).collect();
    let derived = crate::palw_step_leg::step_tile_leaf_hash_v1(
        &ctx.context_hash(),
        &profile.shape_profile_id(),
        &crate::palw_step_leg::PalwStepTileLeafV1 {
            version: crate::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord,
            value_count: (hi - lo) as u32,
            values_le,
        },
    );
    if derived != head_opening.leaf_hash { Ok(()) } else { Err(PalwOffenceVerifyError::LogitsHold) }
}

/// **`PromptNotAnchored` (13)** (addendum §4-bis.3): an attempt claim's committed prompt is not the
/// one its anchor names, on a class whose canonical prompt is past J5b's inline bound. `Ok(true)` is
/// the fault. In order: the binding is the claim's (verify, root, class — J2 is a refusal); the
/// claim recorded its identity; the lane is the attempt lane; the class derives a canonical job and
/// the binding declares its prefill (else 9's J5a), past [`PALW_J5_INLINE_PROMPT_IDS_V1`] (else 9's
/// J5b recomputes it inline); the class's form is `MerkleV1`. Then `Tile`: the opening verifies
/// against the binding's prompt root and its ids are not the anchor's at that range
/// (`palw_attempt_prompt_ids_range_v1`, ~10 µs); `Whole`: the anchor's root recomputed
/// (`palw_attempt_prompt_root_v1`, 13.6 ms at 2M, remembered per claim) is not the binding's — the
/// caller charges the prefill against the block's heavy budget BEFORE this runs, once per claim
/// ([`palw_offence_heavy_prompt_charge_v1`]).
pub fn palw_prompt_not_anchored_fault_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    proof: &crate::palw_offence_v1::PalwPromptProofV1,
    rules: PalwIdentityRulesV1,
) -> Result<bool, PalwOffenceVerifyError> {
    use crate::palw_attempt_rules_v1::{palw_attempt_prompt_ids_range_v1, palw_attempt_prompt_root_memo_v1};
    use crate::palw_offence_v1::PalwPromptProofV1 as P;
    let prefill = palw_prompt_not_anchored_admit_v1(target, binding, rules)?;
    let (profile, ctx, identity) = (&binding.shape_profile, &binding.job_context, target.job_identity);
    match proof {
        P::Tile(opening) => {
            crate::palw_prompt_ids_v1::verify_prompt_ids_opening_v1(&ctx.prompt_token_ids_hash, prefill, opening)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            let start = u64::from(opening.tile_index) * u64::from(crate::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN);
            let anchored =
                palw_attempt_prompt_ids_range_v1(&identity, u64::from(profile.vocab_size), start, opening.tile_ids.len() as u64);
            Ok(opening.tile_ids != anchored)
        }
        P::Whole => {
            // Remembered: a second Whole on this claim in the block recomputes nothing (the heavy
            // budget charges a claim once).
            let root = palw_attempt_prompt_root_memo_v1(profile, &identity, prefill, rules.prompt_ids_form)
                .ok_or(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            Ok(ctx.prompt_token_ids_hash != root)
        }
    }
}

/// **Everything `PromptNotAnchored` (13) asks before it reads a prompt id** — the cheap half, which
/// also decides whether a `Whole` will recompute and so what the heavy budget charges
/// ([`palw_offence_heavy_prompt_charge_v1`]). Returns the canonical prefill.
pub fn palw_prompt_not_anchored_admit_v1(
    target: &PalwOffenceTargetV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    rules: PalwIdentityRulesV1,
) -> Result<u32, PalwOffenceVerifyError> {
    use crate::palw_attempt_rules_v1::{PALW_J5_INLINE_PROMPT_IDS_V1, palw_attempt_canonical_v1};
    palw_claims_own_binding_v1(target, binding)?;
    if target.job_identity == Hash64::default() {
        return Err(PalwOffenceVerifyError::IdentityNotRecorded);
    }
    if target.lane.ok_or(PalwOffenceVerifyError::LaneUnknown)? != PalwClaimSourceKindV1::Attempt {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted("PromptNotAnchored judges an attempt's anchored prompt"));
    }
    let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
    let canonical = palw_attempt_canonical_v1(profile, target.class_id == rules.base_class_id)
        .ok_or(PalwOffenceVerifyError::IdentityNotDerivable)?;
    if ctx.declared_prefill_tokens != canonical.0 {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "the binding's prefill is not the canonical one: that is an IdentityMismatch (J5a)",
        ));
    }
    if canonical.0 <= PALW_J5_INLINE_PROMPT_IDS_V1 {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "a canonical prompt of at most 4,096 ids is recomputed inline: that is an IdentityMismatch (J5b)",
        ));
    }
    let form = crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(rules.prompt_ids_form, profile);
    if form != crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 {
        return Err(PalwOffenceVerifyError::ContradictionNotAdmitted("PromptNotAnchored opens a Merkle prompt root"));
    }
    Ok(canonical.0)
}

/// **The prompt ids a kind-3 or kind-4 object DECLARES it will have recomputed whole**: the binding's
/// declared prefill for a `PromptNotAnchored { proof: Whole }` (13) that decodes, and 0 for
/// everything else. Read from the object alone — no state — because it prices the carrier's rent
/// (`palw_object_rent_ceiling_v2`), which is read where no state is in hand; what the heavy budget
/// CHARGES is [`palw_offence_heavy_prompt_charge_v1`], which asks whether the recompute will run.
pub fn palw_offence_heavy_prompt_ids_v1(kind: crate::palw_offence_v1::PalwOffenceKindV1, evidence: &[u8]) -> u64 {
    use crate::palw_offence_v1::{PalwOffenceKindV1 as K, PalwPromptProofV1};
    if evidence.len() as u64 > PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES {
        return 0;
    }
    let contradiction = match kind {
        K::PanelFalseValidV2 => borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok().map(|p| p.contradiction),
        K::ExecutorRefuted => borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok().map(|p| p.contradiction),
        _ => None,
    };
    match contradiction {
        Some(PalwPanelContradictionV1::PromptNotAnchored { binding, proof: PalwPromptProofV1::Whole }) => {
            u64::from(binding.job_context.declared_prefill_tokens)
        }
        _ => 0,
    }
}

/// **A `Whole` 13 the block would recompute**: the claim whose anchor's prompt root it asks for,
/// and how many ids that is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHeavyPromptChargeV1 {
    pub claim_id: Hash64,
    pub prompt_ids: u64,
}

/// **What the heavy budget charges for this object, in this state** (addendum §4-bis.3; the Phase 3
/// review's heavy-budget finding) — one reading for the processor's acceptance walk and the fold,
/// so the two charge identically.
///
/// `Some` only for a kind-3 or kind-4 `PromptNotAnchored { Whole }` that will REACH the recompute:
/// it decodes at its version, it names this accused (kind 3: the seat its `Valid` receipt names;
/// kind 4: the claim's executor), the target resolves, and the binding clears every cheap check 13
/// asks first ([`palw_prompt_not_anchored_admit_v1`]). An object refused before the recompute costs
/// the budget nothing, so junk that fails early cannot fill the block's one slot. The charge is per
/// CLAIM: the caller charges a claim once per block and the recompute is remembered
/// ([`crate::palw_attempt_rules_v1::palw_attempt_prompt_root_memo_v1`]), so a failing Whole ahead of
/// an honest one on the same claim does not cost it the slot. What is left — a Whole that reaches the
/// recompute on ANOTHER claim — pays for it: its carrier's rent is the carriage of the prompt it
/// recomputes (`palw_object_rent_ceiling_v2`).
pub fn palw_offence_heavy_prompt_charge_v1(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    kind: crate::palw_offence_v1::PalwOffenceKindV1,
    evidence: &[u8],
    rules: PalwIdentityRulesV1,
) -> Option<PalwHeavyPromptChargeV1> {
    use crate::palw_offence_v1::{PalwOffenceKindV1 as K, PalwPromptProofV1};
    if evidence.len() as u64 > PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES {
        return None;
    }
    let (claim_id, contradiction, executor_only) = match kind {
        K::PanelFalseValidV2 => {
            let payload = borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok()?;
            let inner = payload.receipt.inner();
            if payload.version != PALW_PANEL_FALSE_VALID_VERSION_V2
                || payload.accused_seat != accused.0
                || inner.seat_bond.0 != accused.0
                || inner.claim != payload.claim_id
                || !matches!(inner.verdict, PalwReceiptVerdictV2::Valid)
            {
                return None;
            }
            (payload.claim_id, payload.contradiction, false)
        }
        K::ExecutorRefuted => {
            let payload = borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok()?;
            if payload.version != PALW_EXECUTOR_REFUTED_VERSION_V1 {
                return None;
            }
            (payload.claim_id, payload.contradiction, true)
        }
        _ => return None,
    };
    let PalwPanelContradictionV1::PromptNotAnchored { binding, proof: PalwPromptProofV1::Whole } = contradiction else {
        return None;
    };
    let target = palw_offence_target_v1(state, &claim_id)?;
    if executor_only && *accused != target.executor_bond {
        return None;
    }
    let prefill = palw_prompt_not_anchored_admit_v1(&target, &binding, rules).ok()?;
    Some(PalwHeavyPromptChargeV1 { claim_id: target.claim_id, prompt_ids: u64::from(prefill) })
}

/// **What a claim-proving contradiction (9, 10, 13) proves about `target`**, or the refusal. The
/// identity fault, for a 9.
fn claim_proving_fault_v1(
    target: &PalwOffenceTargetV1,
    contradiction: &PalwPanelContradictionV1,
    rules: PalwIdentityRulesV1,
) -> Result<Option<PalwIdentityFaultV1>, PalwOffenceVerifyError> {
    match contradiction {
        PalwPanelContradictionV1::IdentityMismatch { binding } => {
            palw_binding_identity_fault_v1(target, binding, rules, true)?.map(Some).ok_or(PalwOffenceVerifyError::IdentityHolds)
        }
        PalwPanelContradictionV1::OutputMismatch { binding, pin } => {
            if palw_output_fault_v1(target, binding, pin)? {
                Ok(None)
            } else {
                Err(PalwOffenceVerifyError::OutputHolds)
            }
        }
        PalwPanelContradictionV1::PromptNotAnchored { binding, proof } => {
            if palw_prompt_not_anchored_fault_v1(target, binding, proof, rules)? {
                Ok(None)
            } else {
                Err(PalwOffenceVerifyError::PromptHolds)
            }
        }
        _ => Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction),
    }
}

/// **A proof against the claim — its execution or the job it answers — judged once for both
/// convicting kinds** (3 against a `Valid` signer, 4 against the executor): the execution checks
/// against the target's root and artifact at the class's ladder, and the claim checks against its
/// recorded identity. Returns the forfeiture scope and, for a 9, the identity check that failed.
fn adjudicate_against_claim_v1(
    state: &PalwChainStateV2,
    target: &PalwOffenceTargetV1,
    admission: PalwFalseValidAdmissionV1,
    contradiction: &PalwPanelContradictionV1,
    prompt_ids_opening: Option<&PalwPromptIdsOpeningV1>,
    fp_decode_rules_active: bool,
    rules: PalwIdentityRulesV1,
) -> Result<(PalwForfeitScopeV1, Option<PalwIdentityFaultV1>), PalwOffenceVerifyError> {
    let ladder = state.class_step_ladder_v1(&target.class_id, PALW_FALSE_VALID_NETWORK_LADDER_V1);
    match admission {
        PalwFalseValidAdmissionV1::NamedVoid { .. } => Ok((PalwForfeitScopeV1::None, None)),
        PalwFalseValidAdmissionV1::ExecutionProving => {
            if fp_decode_rules_active
                && matches!(contradiction, PalwPanelContradictionV1::ForgedOutput { .. })
                && target.lane != Some(PalwClaimSourceKindV1::Attempt)
            {
                return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
                    "ForgedOutput under ADR-0082's decode rules convicts only a claim known to be an attempt",
                ));
            }
            match contradiction {
                PalwPanelContradictionV1::ForgedOutputTiled { binding, proof } => {
                    palw_forged_output_tiled_fault_v1(target, binding, proof, ladder, fp_decode_rules_active)?
                }
                PalwPanelContradictionV1::LogitsNotStepOutput { event, row, head_tile, head_opening } => {
                    palw_logits_not_step_output_fault_v1(target, event, *row, *head_tile, head_opening, ladder)?
                }
                _ => palw_false_valid_convicts_execution_v2(
                    contradiction,
                    prompt_ids_opening,
                    target.execution_root,
                    target.artifact_root,
                    ladder,
                )?,
            }
            Ok((PalwForfeitScopeV1::ByRoot, None))
        }
        PalwFalseValidAdmissionV1::ClaimProving => {
            let fault = claim_proving_fault_v1(target, contradiction, rules)?;
            Ok((PalwForfeitScopeV1::ByClaim, fault))
        }
    }
}

/// **The one adjudicator of a false `Valid`** (ADR-0152 v2 F2) — the processor calls it with
/// `sig = Some(..)`, the fold with `None`, on the same state, so the verdict a node accepts and the
/// verdict every node folds are one function. The caller has already checked the evidence digest.
/// In order:
///
/// 1. the evidence is at most [`PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES`], what one carrier holds;
/// 2. it decodes as version 2, the reporter slot is empty unless `reporter_armed` — and never
///    longer than [`PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES`] — and a prompt-id opening rides
///    only beside a `StepArithmetic`;
/// 3. it accuses the seat the receipt names, the receipt names the claim, and the verdict is
///    `Valid`;
/// 4. (`sig` only) the seat is `Active` or `Retiring` and signed the receipt under the chain's
///    domain, in the V2 or V3 message and context its form was licensed with;
/// 5. the target resolves from state ([`palw_offence_target_v1`]);
/// 6. no COURT is open on the claim — the filer waits (ADR-0152 v3.1 J-3, post-edit 2: a
///    data-availability session, held or not, no longer blocks a conviction; voiding under one is
///    handled where the claim is written — the held rows dropped, the accuser's reservation given
///    back — and a filer can no longer open a session to delay one);
/// 7. the contradiction is admitted ([`palw_false_valid_admission_v1`]; `ProducerWithholding` only as
///    the restatement of a DA-7 default, [`palw_da_default_confirms_withholding_v1`], N9), and a named
///    void is the one the chain wrote on this claim
///    (`palw_void_binds_claim_v1`, the V1 fold's own reading) — a PROVEN `CourtFraud`, never a
///    court's `CourtDefault` (the executor's silence, which proves nothing a seat replayed; F2
///    residual);
/// 8. an execution-proving contradiction convicts against the TARGET's `execution_root` and
///    artifact root at the class's ladder ([`palw_false_valid_convicts_execution_v2`], the prompt
///    read through the evidence's opening where the job commits a Merkle root) — the root pin is the
///    link from claim to execution, and nothing compares a job id with the claim id; a
///    `ForgedOutput` is refused under ADR-0082's decode rules unless the claim is known to be an
///    attempt; a claim-proving one (F1: 9, 10) convicts against the claim's recorded identity and
///    output ([`palw_binding_identity_fault_v1`], [`palw_output_fault_v1`]);
/// 9. the receipt attested everything the fault's verdict rests on ([`palw_false_valid_liable_v1`]),
///    a partial mask being the one the panel assigned.
pub fn palw_check_panel_false_valid_v2(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    evidence: &[u8],
    fp_decode_rules_active: bool,
    reporter_armed: bool,
    rules: PalwIdentityRulesV1,
    sig: Option<PalwFalseValidSigCheckV1<'_>>,
) -> Result<PalwFalseValidFindingV1, PalwOffenceVerifyError> {
    if evidence.len() as u64 > PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES {
        return Err(PalwOffenceVerifyError::EvidenceTooLarge);
    }
    let payload: PalwPanelFalseValidEvidenceV2 =
        borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
    if payload.version != PALW_PANEL_FALSE_VALID_VERSION_V2 {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    }
    if !payload.reporter_reveal.is_empty() && !reporter_armed {
        return Err(PalwOffenceVerifyError::ReporterSlotNotArmed);
    }
    if payload.reporter_reveal.len() > PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES {
        return Err(PalwOffenceVerifyError::EvidenceTooLarge);
    }
    if payload.prompt_ids_opening.is_some() && !matches!(payload.contradiction, PalwPanelContradictionV1::StepArithmetic { .. }) {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    }
    let inner = payload.receipt.inner();
    if payload.accused_seat != accused.0 || inner.seat_bond.0 != accused.0 {
        return Err(PalwOffenceVerifyError::PanelFalseValidSeatMismatch);
    }
    if inner.claim != payload.claim_id {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    if !matches!(inner.verdict, PalwReceiptVerdictV2::Valid) {
        return Err(PalwOffenceVerifyError::PanelFalseValidNotValidVerdict);
    }
    if let Some(sig) = sig {
        if !sig.seat_active {
            return Err(PalwOffenceVerifyError::BondNotActive);
        }
        let (message, context) = payload.receipt.signed_message_v1(sig.chain_domain);
        if !(sig.verify)(sig.seat_pubkey, message.as_byte_slice(), &inner.signature, context) {
            return Err(PalwOffenceVerifyError::PanelFalseValidReceiptUnverified);
        }
    }
    let target = palw_offence_target_v1(state, &payload.claim_id).ok_or(PalwOffenceVerifyError::NoTarget)?;
    if state.open_courts_of(&target.claim_id) > 0 {
        return Err(PalwOffenceVerifyError::ClaimUnderSession);
    }
    // ADR-0152 N9 (M3): `ProducerWithholding` is admitted against a `Valid` signer exactly when a
    // DA-7 default confirmed the claim's withholding at that DAA — the `DaDefault` record (kind 5),
    // which only M3's sweep writes, past `palw_rcore_plus`. It restates the default: the signer's
    // (seat, claim) key is the one DA-7's own S4 on covering signers is written under, so whichever
    // lands second is a no-op; its site is `Whole` (only a full attestation is liable); and the
    // adjudicator already requires the receipt to be `Valid` (never `Sampled`, `Incapable` or
    // `Unavailable`). Every other `ProducerWithholding` is refused as F-2 refused it.
    let da_confirmed = rules.da_signer_liability
        && matches!(payload.contradiction, PalwPanelContradictionV1::ProducerWithholding { voided_daa }
            if palw_da_default_confirms_withholding_v1(state, &target, voided_daa));
    let admission = if let (true, PalwPanelContradictionV1::ProducerWithholding { voided_daa }) = (da_confirmed, &payload.contradiction) {
        PalwFalseValidAdmissionV1::NamedVoid { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa: *voided_daa }
    } else {
        palw_false_valid_admission_v1(&payload.contradiction)?
    };
    if let PalwFalseValidAdmissionV1::NamedVoid { reason, voided_daa } = admission
        && !da_confirmed
    {
        // A court's DEFAULT is not its verdict (F2 residual): the void the executor's own
        // silence wrote proves nothing about the execution the seats replayed, so a
        // `CourtFraud` contradiction naming it is refused by name rather than as a mismatch.
        if state.palw_void_binds_claim_v1(&target.claim_id, PalwVoidReasonV2::CourtDefault, voided_daa) {
            return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
                "the claim was voided by a court DEFAULT (the executor's silence), not a proven fraud; a Valid signer is \
                 convicted only by a proof of the execution",
            ));
        }
        // ADR-0152 §4-ter (the review's F3, decision (B)): a held dissection's verdict proves the
        // producer's own DISCLOSURE false — a responder may split a lie across siblings over an honest
        // root — never the execution the seats replayed. The producer pays for it; no signer does.
        if state.palw_void_binds_claim_v1(&target.claim_id, PalwVoidReasonV2::CourtHeldVerdict, voided_daa) {
            return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
                "the claim was voided by a HELD DISSECTION's verdict, which proves the producer's disclosure false, not the \
                 execution; a Valid signer is convicted only by a proof of the execution",
            ));
        }
        if !state.palw_void_binds_claim_v1(&target.claim_id, reason, voided_daa) {
            return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
        }
    }
    let (forfeit, identity_fault) = adjudicate_against_claim_v1(
        state,
        &target,
        admission,
        &payload.contradiction,
        payload.prompt_ids_opening.as_ref(),
        fp_decode_rules_active,
        rules,
    )?;
    let ladder = state.class_step_ladder_v1(&target.class_id, PALW_FALSE_VALID_NETWORK_LADDER_V1);
    let (site, step_leaf_count) = match palw_claim_proving_site_v1(&payload.contradiction, identity_fault) {
        Some(site) => (site, 0),
        None => palw_false_valid_fault_site_v1(&payload.contradiction, ladder)?,
    };
    let assigned = palw_false_valid_assigned_mask_v1(state, &target.claim_id, accused);
    palw_false_valid_liable_v1(&payload.receipt, site, target.segment_count, assigned, step_leaf_count)?;
    let acts_on_claim = !matches!(admission, PalwFalseValidAdmissionV1::NamedVoid { .. });
    Ok(PalwFalseValidFindingV1 { target, site, acts_on_claim, forfeit })
}

/// **ADR-0152 N9: did a DA-7 default confirm this claim's withholding at `voided_daa`?** — the
/// claim's `DaDefault` record (kind 5, keyed `(executor, claim)`) exists and was written at that DAA.
/// Only M3's sweep writes it, past `palw_rcore_plus`, so below the fence this is always `false`.
pub fn palw_da_default_confirms_withholding_v1(state: &PalwChainStateV2, target: &PalwOffenceTargetV1, voided_daa: u64) -> bool {
    state
        .consumed_offence(&crate::palw_da_rcore_v1::palw_da_offence_id_v1(&target.executor_bond.0, &target.claim_id))
        .is_some_and(|row| row.kind == crate::palw_offence_v1::PalwOffenceKindV1::DaDefault && row.accepted_daa == voided_daa)
}

/// `H(domain ‖ claim)` — the evidence half of a kind-4 offence id: one refuted execution per claim,
/// whatever contradiction or bytes proved it.
pub fn palw_executor_refuted_ledger_key_v1(claim_id: &Hash64) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_EXECUTOR_REFUTED_KEY_DOMAIN_V1).to_state();
    h.update(claim_id.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **One kind-4 offence per claim**: `palw_offence_id_v1(ExecutorRefuted, executor, H(claim))`.
pub fn palw_executor_refuted_offence_id_v1(executor: &TransactionOutpoint, claim_id: &Hash64) -> Hash64 {
    crate::palw_offence_v1::palw_offence_id_v1(
        crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
        executor,
        &palw_executor_refuted_ledger_key_v1(claim_id),
    )
}

/// **Which contradictions refute an executor** (ADR-0152 v3.1 J-4, addendum §4-bis.7): the proofs
/// against its own committed execution (5 `StepArithmetic`, 6 `StepStructural`, 8 `ForgedOutput`,
/// 11 `ForgedOutputTiled`, 12 `LogitsNotStepOutput`) and against the job or output its claim answers
/// (9 `IdentityMismatch`, 10 `OutputMismatch`, 13 `PromptNotAnchored`). A void the chain
/// wrote, a permit, an equivocation and the v1 legs are not refutations of the claim's execution.
pub fn palw_executor_refuted_admission_v1(
    contradiction: &PalwPanelContradictionV1,
) -> Result<PalwFalseValidAdmissionV1, PalwOffenceVerifyError> {
    use PalwPanelContradictionV1 as C;
    match contradiction {
        C::StepArithmetic { .. }
        | C::StepStructural(_)
        | C::ForgedOutput { .. }
        | C::ForgedOutputTiled { .. }
        | C::LogitsNotStepOutput { .. } => Ok(PalwFalseValidAdmissionV1::ExecutionProving),
        C::IdentityMismatch { .. } | C::OutputMismatch { .. } | C::PromptNotAnchored { .. } => {
            Ok(PalwFalseValidAdmissionV1::ClaimProving)
        }
        C::ExecutorEquivocation(_)
        | C::CourtExecutorGuilty { .. }
        | C::ProducerWithholding { .. }
        | C::ConflictingPermit { .. }
        | C::CourtFraud { .. }
        | C::Legs(_) => Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "an ExecutorRefuted carries a proof against the claim's own execution, job or output (5, 6, 8–13)",
        )),
    }
}

/// **The adjudicator of `ExecutorRefuted` (kind 4)** (ADR-0152 v3.1 J-4) — the processor's gate and
/// the fold's consumer call it on the same state, so a node that accepts one convicts as every node
/// folds it. No signature: the evidence is objective and names nobody but the claim's executor. In
/// order: the byte cap; decode, version 1, the reporter slot, an opening only beside a
/// `StepArithmetic`; the target (claim, liability row, vesting row); the accused IS the target's
/// executor bond; no court is open on the claim (a DA session does not block, post-edit 2); the
/// contradiction is a refutation ([`palw_executor_refuted_admission_v1`]) and it convicts against
/// the claim ([`adjudicate_against_claim_v1`]'s one reading, shared with kind 3).
pub fn palw_check_executor_refuted_v1(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    evidence: &[u8],
    fp_decode_rules_active: bool,
    reporter_armed: bool,
    rules: PalwIdentityRulesV1,
) -> Result<PalwExecutorRefutedFindingV1, PalwOffenceVerifyError> {
    if evidence.len() as u64 > PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES {
        return Err(PalwOffenceVerifyError::EvidenceTooLarge);
    }
    let payload: PalwExecutorRefutedEvidenceV1 =
        borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
    if payload.version != PALW_EXECUTOR_REFUTED_VERSION_V1 {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    }
    if !payload.reporter_reveal.is_empty() && !reporter_armed {
        return Err(PalwOffenceVerifyError::ReporterSlotNotArmed);
    }
    if payload.reporter_reveal.len() > PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES {
        return Err(PalwOffenceVerifyError::EvidenceTooLarge);
    }
    if payload.prompt_ids_opening.is_some() && !matches!(payload.contradiction, PalwPanelContradictionV1::StepArithmetic { .. }) {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    }
    let target = palw_offence_target_v1(state, &payload.claim_id).ok_or(PalwOffenceVerifyError::NoTarget)?;
    if *accused != target.executor_bond {
        return Err(PalwOffenceVerifyError::AccusedNotTheExecutor);
    }
    if state.open_courts_of(&target.claim_id) > 0 {
        return Err(PalwOffenceVerifyError::ClaimUnderSession);
    }
    let admission = palw_executor_refuted_admission_v1(&payload.contradiction)?;
    let (forfeit, identity_fault) = adjudicate_against_claim_v1(
        state,
        &target,
        admission,
        &payload.contradiction,
        payload.prompt_ids_opening.as_ref(),
        fp_decode_rules_active,
        rules,
    )?;
    Ok(PalwExecutorRefutedFindingV1 { target, forfeit, identity_fault })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_offence_v1::PalwOffenceVerifyError as E;
    use crate::palw_state_v2::{PalwClaimStateV2, PalwPanelSeatV2, PalwPanelStateV2};
    use crate::palw_verification_v2::palw_segment_leaf_range_v2;
    use crate::tx::TransactionId;
    use std::cell::RefCell;

    fn h64(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    /// The identity rules these fixtures run under: the flat network form, and the fixture class as
    /// the base one (so J5 reads the floor's canonical job wherever a test builds a floor binding).
    fn rules() -> PalwIdentityRulesV1 {
        PalwIdentityRulesV1 {
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            base_class_id: h64(CLASS),
            da_signer_liability: false,
        }
    }

    fn seat(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }

    const CLAIM: u64 = 0xC1A1;
    const CLASS: u64 = 0xC1A5;
    const PANEL_ANCHOR: u64 = 0xA7;

    fn v2_receipt(seat_no: u64, claim: Hash64, verdict: PalwReceiptVerdictV2) -> PalwSeatReceiptV2 {
        PalwSeatReceiptV2 {
            claim,
            verdict,
            seat_bond: seat(seat_no),
            signed_daa: 40,
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    fn full(seat_no: u64) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Full(v2_receipt(seat_no, h64(CLAIM), PalwReceiptVerdictV2::Valid))
    }

    fn segmented(seat_no: u64, mask: PalwSegmentMaskV2) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Segmented(PalwSeatReceiptV3 {
            receipt: v2_receipt(seat_no, h64(CLAIM), PalwReceiptVerdictV2::Valid),
            segments: mask,
        })
    }

    fn evidence(receipt: PalwFalseValidReceiptV1, contradiction: PalwPanelContradictionV1) -> PalwPanelFalseValidEvidenceV2 {
        PalwPanelFalseValidEvidenceV2 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id: h64(CLAIM),
            accused_seat: receipt.inner().seat_bond.0,
            receipt,
            contradiction,
            prompt_ids_opening: None,
            reporter_reveal: Vec::new(),
        }
    }

    fn bytes(payload: &PalwPanelFalseValidEvidenceV2) -> Vec<u8> {
        borsh::to_vec(payload).expect("evidence serializes")
    }

    fn judge(state: &PalwChainStateV2, payload: &PalwPanelFalseValidEvidenceV2) -> Result<PalwFalseValidFindingV1, E> {
        palw_check_panel_false_valid_v2(state, &PalwBondKeyV2(payload.accused_seat), &bytes(payload), false, false, rules(), None)
    }

    fn claim_row(phase: PalwClaimPhaseV2, execution_root: Hash64) -> PalwClaimStateV2 {
        PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id: h64(CLASS),
            bond: seat(99),
            pwu: 100,
            accepted_daa: 10,
            rebound_daa: None,
            accepted_blue_score: 10,
            accepted_block: h64(0xB1),
            trace_root: h64(0x71),
            output_root: h64(0x72),
            execution_root,
            trace_chunk_count: 4,
            trace_retention_daa: 700,
            reserved: 1_000,
            immature_contribution: 0,
            escrowed_reward: 12,
            work_leaves: 0,
            work_id: None,
            phase,
            rights_reserved: 0,
            job_identity: Hash64::default(),
            rcore: crate::palw_state_v2::PalwClaimRcoreV1::default(),
        }
    }

    fn five_seat_panel() -> PalwPanelStateV2 {
        PalwPanelStateV2 {
            anchor: h64(PANEL_ANCHOR),
            seats: (1..=5).map(|n| PalwPanelSeatV2 { bond: seat(n), operator_id: h64(0x0900 + n) }).collect(),
            bound_daa: 20,
        }
    }

    /// The mask the five-seat panel's anchor assigned seat `n` (the seats sit in panel order 1–5).
    fn assigned(n: u64) -> PalwSegmentMaskV2 {
        palw_segment_assignment_v2(h64(PANEL_ANCHOR), h64(CLAIM), 5).mask_of((n - 1) as u16)
    }

    /// The seat the anchor drew for the full replay.
    fn full_seat() -> u64 {
        (1..=5).find(|n| assigned(*n) == PalwSegmentMaskV2::full(4)).expect("one seat replays whole")
    }

    /// The four partial seats, each with the one segment it was assigned.
    fn partial_seats() -> Vec<u64> {
        (1..=5).filter(|n| *n != full_seat()).collect()
    }

    fn liability_row(
        execution_root: Hash64,
        voided: Option<(u64, PalwVoidReasonV2)>,
    ) -> crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
        crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
            claim_id: h64(CLAIM),
            work_id: execution_root,
            class_id: h64(CLASS),
            execution_root,
            output_root: h64(0x72),
            executor_bond: seat(99),
            voided_daa: voided.map(|(daa, _)| daa),
            void_reason: voided.map(|(_, reason)| reason),
            valid_signers: (1..=3).map(|n| (seat(n).0, h64(CLAIM))).collect(),
            locked_sompi: 3,
            expiry_daa: 5_000,
            settled_at_final: 0,
            job_identity: Hash64::default(),
            free_prompt: false,
            trace_root: Hash64::default(),
            segment_count: 0,
            licence_door: None,
            basis_k: 0,
            g_res_sompi: 0,
            escrowed_reward: 0,
        }
    }

    /// A live claim in `phase`, a five-seat panel (so `k = 4`), and the class row whose artifact
    /// root operand openings prove against.
    fn live_state(phase: PalwClaimPhaseV2, execution_root: Hash64, artifact_root: Hash64) -> PalwChainStateV2 {
        let mut state = PalwChainStateV2::genesis();
        state.set_false_valid_rows_for_tests(h64(CLAIM), Some(claim_row(phase, execution_root)), Some(five_seat_panel()), None, 0);
        state.set_class_for_tests(
            h64(CLASS),
            crate::palw_state_v2::PalwClassStateV2 {
                artifact_root,
                slash_value_per_pwu: 5,
                pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(100),
                status: crate::palw_state_v2::PalwClassStatusV2::Active,
                registered_daa: 0,
                registrant_bond: None,
                fused_attention: false,
            },
        );
        state
    }

    fn voided(daa: u64, reason: PalwVoidReasonV2) -> PalwClaimPhaseV2 {
        PalwClaimPhaseV2::Voided { voided_daa: daa, reason }
    }

    /// **The site/mask truth table**, at `k = 1` (a two-seat panel, where the one partial seat's
    /// mask IS the full mask), `k = 4` (the five-seat panel every shipped class draws) and
    /// `k = 31` (one short of the mask's width). F2 review: a partial seat is liable only for a fault
    /// whose leaf AND every leaf its verdict read lie in its segment (F-1), and only under the mask
    /// its panel assigned it (F-4); a full mask is a full attestation whatever the assignment.
    #[test]
    fn the_site_and_mask_truth_table() {
        let leaves = 1_000u64;
        let at = PalwFaultSiteV1::leaf_alone;
        let everything = PalwFaultSiteV1::Leaf { leaf: leaves - 1, first_read: 0, last_read: leaves - 1 };
        for k in [1u16, 4, 31] {
            // A full receipt attested everything, whatever the cut, the assignment and whether
            // either is known.
            for site in [PalwFaultSiteV1::Whole, at(0), at(leaves - 1), everything] {
                for known in [Some(k), None] {
                    for assigned in [None, Some(PalwSegmentMaskV2::NONE), Some(PalwSegmentMaskV2::single(0))] {
                        assert_eq!(
                            palw_false_valid_liable_v1(&full(1), site, known, assigned, leaves),
                            Ok(()),
                            "k={k} {site:?} {known:?} {assigned:?}"
                        );
                    }
                }
            }
            // An empty mask attested nothing, known cut or not.
            for known in [Some(k), None] {
                assert_eq!(
                    palw_false_valid_liable_v1(
                        &segmented(2, PalwSegmentMaskV2::NONE),
                        PalwFaultSiteV1::Whole,
                        known,
                        Some(PalwSegmentMaskV2::NONE),
                        leaves
                    ),
                    Err(E::SiteNotAttested),
                    "k={k}: an empty mask"
                );
            }
            // The full mask is a full attestation — liable at the whole and at every leaf, whatever
            // the seat was assigned: a partial seat that replayed the whole job and signed for all
            // of it is held to all of it.
            let whole_mask = segmented(2, PalwSegmentMaskV2::full(k));
            for assigned in [Some(PalwSegmentMaskV2::full(k)), Some(PalwSegmentMaskV2::single(0)), Some(PalwSegmentMaskV2::NONE), None]
            {
                for site in [PalwFaultSiteV1::Whole, at(leaves - 1), everything] {
                    assert_eq!(palw_false_valid_liable_v1(&whole_mask, site, Some(k), assigned, leaves), Ok(()), "k={k} {assigned:?}");
                }
            }
            // ...but not placeable when the cut is not in state.
            assert_eq!(
                palw_false_valid_liable_v1(&whole_mask, PalwFaultSiteV1::Whole, None, None, leaves),
                Err(E::SegmentsUnknown),
                "k={k}: an unknown cut"
            );
            // One segment each, as assigned: liable for exactly the faults whose leaf and every read
            // are in it, never for the whole.
            for index in 0..k {
                let (start, end) = palw_segment_leaf_range_v2(leaves, k, index).expect("a segment of the cut");
                let mask = PalwSegmentMaskV2::single(index);
                let partial = segmented(3, mask);
                let liable_at = |site: PalwFaultSiteV1| palw_false_valid_liable_v1(&partial, site, Some(k), Some(mask), leaves);
                if k == 1 {
                    assert_eq!(liable_at(PalwFaultSiteV1::Whole), Ok(()), "k=1: the one segment is the whole job");
                } else {
                    assert_eq!(liable_at(PalwFaultSiteV1::Whole), Err(E::SiteNotAttested), "k={k} seg {index}: never the whole");
                }
                for leaf in [start, end - 1] {
                    assert_eq!(liable_at(at(leaf)), Ok(()), "k={k} seg {index} leaf {leaf}");
                }
                assert_eq!(
                    liable_at(PalwFaultSiteV1::Leaf { leaf: end - 1, first_read: start, last_read: end - 1 }),
                    Ok(()),
                    "k={k} seg {index}: a step that read only its own segment"
                );
                if k > 1 {
                    let outside = if end < leaves { end } else { start - 1 };
                    assert_eq!(liable_at(at(outside)), Err(E::SiteNotAttested), "k={k} seg {index}: the neighbour's leaf {outside}");
                    // **F-1**: the leaf is this segment's, but the verdict read a neighbour's leaf —
                    // which this seat never compared, so a lie there is not its lie.
                    if start > 0 {
                        assert_eq!(
                            liable_at(PalwFaultSiteV1::Leaf { leaf: start, first_read: start - 1, last_read: start }),
                            Err(E::SiteNotAttested),
                            "k={k} seg {index}: a read in the previous segment"
                        );
                    }
                    if end < leaves {
                        assert_eq!(
                            liable_at(PalwFaultSiteV1::Leaf { leaf: end - 1, first_read: end - 1, last_read: end }),
                            Err(E::SiteNotAttested),
                            "k={k} seg {index}: a read in the next segment"
                        );
                    }
                    // **F-4**: a partial mask the panel did not assign this seat is refused, whatever
                    // the site — and so is one with no assignment to compare against.
                    let other = PalwSegmentMaskV2::single((index + 1) % k);
                    for wrong in [Some(other), Some(PalwSegmentMaskV2::NONE), None] {
                        assert_eq!(
                            palw_false_valid_liable_v1(&partial, at(start), Some(k), wrong, leaves),
                            Err(E::SegmentMaskNotAssigned),
                            "k={k} seg {index}: assigned {wrong:?}"
                        );
                    }
                }
                assert_eq!(
                    palw_false_valid_liable_v1(&partial, at(start), None, Some(mask), leaves),
                    Err(E::SegmentsUnknown),
                    "k={k} seg {index}: an unknown cut"
                );
            }
            // A leaf the cut does not have is in no segment.
            let partial = segmented(3, PalwSegmentMaskV2::single(0));
            let expected = if k == 1 { Ok(()) } else { Err(E::SiteNotAttested) };
            assert_eq!(
                palw_false_valid_liable_v1(&partial, at(leaves), Some(k), Some(PalwSegmentMaskV2::single(0)), leaves),
                expected,
                "k={k}"
            );
        }
    }

    /// **With no prompt-id opening the execution check IS the V1 route's**, for every
    /// execution-proving kind and a named void, on the claim's root and on another. An opening rides
    /// only beside a `StepArithmetic`, and there the job's commitment decides: the fixture commits a
    /// flat digest, so a Merkle opening is refused by the arithmetic. The Merkle half on a real
    /// refutation is `t46o` (kaspa-consensus).
    #[test]
    fn the_execution_check_is_the_v1_routes_without_an_opening() {
        use crate::palw_step_leg::{PalwStepEvidenceV1, PalwStepRefutationV1};
        use PalwPanelContradictionV1 as C;
        let skeleton = crate::palw_step_refute::tests::skeleton_refutation();
        let root = skeleton.binding.committed_execution_root;
        let arithmetic = C::StepArithmetic { refutation: skeleton.clone(), operand_openings: Vec::new() };
        let cases = [
            C::StepStructural(PalwStepRefutationV1 { binding: skeleton.binding.clone(), evidence: PalwStepEvidenceV1::Shape }),
            C::ForgedOutput { binding: skeleton.binding.clone(), pin: zeroed(), position: 0 },
            arithmetic.clone(),
            C::CourtFraud { voided_daa: 5 },
        ];
        let opening = crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&[3, 5, 8, 13], 0).expect("a short prompt opens");
        for contradiction in &cases {
            for claim_root in [root, h64(0xE0)] {
                let v1 = palw_panel_contradiction_convicts_execution_v1(contradiction, claim_root, h64(0xAF), 1 << 20);
                assert_eq!(
                    palw_false_valid_convicts_execution_v2(contradiction, None, claim_root, h64(0xAF), 1 << 20),
                    v1,
                    "no opening: {contradiction:?}"
                );
            }
            if !matches!(contradiction, C::StepArithmetic { .. }) {
                assert_eq!(
                    palw_false_valid_convicts_execution_v2(contradiction, Some(&opening), root, h64(0xAF), 1 << 20),
                    Err(E::PanelFalseValidNeedsContradiction),
                    "an opening beside {contradiction:?} is evidence for a question nobody asked"
                );
            }
        }
        assert_eq!(
            palw_false_valid_convicts_execution_v2(&arithmetic, Some(&opening), h64(0xE0), h64(0xAF), 1 << 20),
            Err(E::PanelFalseValidWorkMismatch),
            "the root is pinned first"
        );
        assert_eq!(
            palw_false_valid_convicts_execution_v2(&arithmetic, Some(&opening), root, h64(0xAF), 1 << 20),
            Err(E::PanelFalseValidNeedsContradiction),
            "a flat commitment refuses a Merkle opening"
        );
        // And the adjudicator refuses an opening beside anything but a step before it reads more.
        let state = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        let mut payload = evidence(full(1), C::CourtFraud { voided_daa: 55 });
        judge(&state, &payload).expect("without the opening the void binds");
        payload.prompt_ids_opening = Some(opening);
        assert_eq!(judge(&state, &payload), Err(E::PanelFalseValidNeedsContradiction));
    }

    /// **The admission table**: which contradiction may convict a `Valid` signer, refused by
    /// name where it cannot be tied to this claim's execution — `ProducerWithholding` among them
    /// (F2 review, F-2).
    #[test]
    fn the_admission_table() {
        use crate::palw_step_leg::{PalwStepEvidenceV1, PalwStepRefutationV1};
        use PalwPanelContradictionV1 as C;
        let skeleton = crate::palw_step_refute::tests::skeleton_refutation();
        let carriage = crate::palw_carriage::PalwEquivocationCarriageV1 {
            version: crate::palw_carriage::PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: seat(99).0,
            certificate: crate::palw_state_v2::tests::contradiction(h64(CLASS)),
        };
        let legs: crate::palw_legs::PalwLegsRefutationV1 = zeroed();
        let refused = [
            C::ProducerWithholding { voided_daa: 55 },
            C::ExecutorEquivocation(carriage),
            C::CourtExecutorGuilty { offence_id: h64(0x0FF) },
            C::ConflictingPermit { span: 1, round: 2, permit_index: 0 },
            C::Legs(legs),
        ];
        let state = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        for contradiction in refused {
            assert!(
                matches!(palw_false_valid_admission_v1(&contradiction), Err(E::ContradictionNotAdmitted(_))),
                "{contradiction:?} is refused by name"
            );
            assert!(
                matches!(judge(&state, &evidence(full(1), contradiction.clone())), Err(E::ContradictionNotAdmitted(_))),
                "and the adjudicator refuses it before anything else is read"
            );
        }
        // **F-2: a ProducerWithholding void the chain really wrote is refused too** — a colluding
        // producer's silence must not take an honest full seat's lock.
        let withheld = live_state(voided(55, PalwVoidReasonV2::ProducerWithholding), h64(0xE0), h64(0xAF));
        assert!(
            matches!(
                judge(&withheld, &evidence(full(1), C::ProducerWithholding { voided_daa: 55 })),
                Err(E::ContradictionNotAdmitted(_))
            ),
            "the void binds the claim, and the route is shut"
        );
        // The execution-proving kinds are admitted: they go on to be pinned to the root.
        let structural =
            C::StepStructural(PalwStepRefutationV1 { binding: skeleton.binding.clone(), evidence: PalwStepEvidenceV1::Shape });
        let forged = C::ForgedOutput { binding: skeleton.binding.clone(), pin: zeroed(), position: 0 };
        let arithmetic = C::StepArithmetic { refutation: skeleton.clone(), operand_openings: Vec::new() };
        for contradiction in [&structural, &forged, &arithmetic] {
            assert_eq!(palw_false_valid_admission_v1(contradiction), Ok(PalwFalseValidAdmissionV1::ExecutionProving));
            assert_eq!(
                judge(&state, &evidence(full(1), contradiction.clone())),
                Err(E::PanelFalseValidWorkMismatch),
                "admitted, and refused only because it is not this claim's root"
            );
        }
        // Under ADR-0082's decode rules a ForgedOutput needs a claim known to be an attempt.
        let mut fp = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        let mut fp_claim = claim_row(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0));
        fp_claim.source = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() };
        fp.set_false_valid_rows_for_tests(h64(CLAIM), Some(fp_claim), Some(five_seat_panel()), None, 0);
        let payload = evidence(full(1), forged.clone());
        let armed = palw_check_panel_false_valid_v2(&fp, &seat(1), &bytes(&payload), true, false, rules(), None);
        assert!(matches!(armed, Err(E::ContradictionNotAdmitted(_))), "{armed:?}");
        let attempt = palw_check_panel_false_valid_v2(&state, &seat(1), &bytes(&payload), true, false, rules(), None);
        assert_eq!(attempt, Err(E::PanelFalseValidWorkMismatch), "an attempt claim is still judged");
        // A named void binds only the void the chain wrote: this claim, CourtFraud, this DAA.
        let contradiction = C::CourtFraud { voided_daa: 55 };
        let finding = judge(&state, &evidence(full(1), contradiction.clone())).expect("the void the chain wrote binds");
        assert_eq!(finding.site, PalwFaultSiteV1::Whole);
        assert!(!finding.acts_on_claim, "a named void names a claim, not a root");
        assert_eq!(finding.forfeit, PalwForfeitScopeV1::None);
        assert_eq!(finding.target.execution_root, h64(0xE0));
        let full_mask = full_seat();
        assert!(judge(&state, &evidence(segmented(full_mask, PalwSegmentMaskV2::full(4)), contradiction.clone())).is_ok());
        for partial in partial_seats() {
            assert_eq!(
                judge(&state, &evidence(segmented(partial, assigned(partial)), contradiction.clone())),
                Err(E::SiteNotAttested),
                "seat {partial}'s assigned segment did not attest the whole"
            );
        }
        let wrong_reason = live_state(voided(55, PalwVoidReasonV2::ProducerWithholding), h64(0xE0), h64(0xAF));
        assert_eq!(judge(&wrong_reason, &evidence(full(1), contradiction.clone())), Err(E::PanelFalseValidWorkMismatch));
        let wrong_daa = live_state(voided(56, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        assert_eq!(judge(&wrong_daa, &evidence(full(1), contradiction.clone())), Err(E::PanelFalseValidWorkMismatch));
        // Once the claim has retired, the liability row carries the void — and the cut is gone.
        let mut retired = PalwChainStateV2::genesis();
        retired.set_false_valid_rows_for_tests(
            h64(CLAIM),
            None,
            None,
            Some(liability_row(h64(0xE0), Some((55, PalwVoidReasonV2::CourtFraud)))),
            0,
        );
        let finding = judge(&retired, &evidence(full(1), contradiction.clone())).expect("the row binds the void");
        assert_eq!((finding.target.lane, finding.target.segment_count, finding.target.phase), (None, None, None));
        assert_eq!(
            judge(&retired, &evidence(segmented(2, PalwSegmentMaskV2::single(0)), contradiction.clone())),
            Err(E::SegmentsUnknown),
            "a row that recorded no identity (written below F1's fence) keeps its cut unknown"
        );
        // No target; a claim under a court; and (ADR-0152 v3.1 post-edit 2) a data-availability
        // session, which does NOT block a conviction.
        assert_eq!(judge(&PalwChainStateV2::genesis(), &evidence(full(1), contradiction.clone())), Err(E::NoTarget));
        let mut in_court = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        in_court.set_false_valid_rows_for_tests(
            h64(CLAIM),
            Some(claim_row(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0))),
            Some(five_seat_panel()),
            None,
            1,
        );
        assert_eq!(judge(&in_court, &evidence(full(1), contradiction.clone())), Err(E::ClaimUnderSession));
        let disputed = PalwClaimPhaseV2::DefaultDisputed {
            accused_daa: 50,
            missing_event_index: 0,
            accuser: seat(7),
            accuser_exposure: 10,
            resumed: Box::new(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }),
        };
        let (refutation, openings, artifact_root) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let root = refutation.binding.committed_execution_root;
        let under_da = live_state(disputed, root, artifact_root);
        let step = C::StepArithmetic { refutation, operand_openings: openings };
        judge(&under_da, &evidence(full(1), step.clone())).expect("a DA session does not defer a conviction (post-edit 2)");
        let licensed = live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, root, artifact_root);
        judge(&licensed, &evidence(full(1), step)).expect("and the same proof convicts with no session at all");
    }

    /// **A step fault whose verdict reads a neighbour segment convicts only whole attestations**
    /// (F2 review, F-1, in the unit fixture's shape). The arithmetic fixture's committed matmul tile
    /// at decode call 1 is off by one; the step reads the embedding tiles of the same call, which the
    /// four-way cut puts in the previous segment. The full seat, and a partial-assigned seat that
    /// signed the full mask, are liable; every partial seat under its assigned mask is not — the
    /// leaf's own segment holder included — and a partial mask the panel did not assign is refused.
    #[test]
    fn a_step_fault_that_reads_a_neighbour_segment_convicts_only_whole_attestations() {
        let (refutation, openings, artifact_root) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let root = refutation.binding.committed_execution_root;
        let leaves = refutation.binding.step_leaf_count;
        let leaf = refutation.output_opening.leaf_index;
        let first_read = refutation
            .inputs
            .iter()
            .flat_map(|row| row.preimages.iter())
            .map(|p| {
                crate::palw_step::canonical_step_leaf_index(
                    &refutation.binding.shape_profile,
                    &refutation.binding.job_context,
                    &p.coord,
                )
                .expect("a canonical input")
            })
            .min()
            .expect("a matmul reads its input row");
        let home = palw_segment_index_of_leaf_v2(leaves, 4, leaf).expect("the leaf is in the cut");
        assert_ne!(
            palw_segment_index_of_leaf_v2(leaves, 4, first_read),
            Some(home),
            "the premise: the step reads the previous segment"
        );
        let state = live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, root, artifact_root);
        let contradiction =
            PalwPanelContradictionV1::StepArithmetic { refutation: refutation.clone(), operand_openings: openings.clone() };
        let finding = judge(&state, &evidence(full(1), contradiction.clone())).expect("the full receipt attested everything");
        assert_eq!(finding.site, PalwFaultSiteV1::Leaf { leaf, first_read, last_read: leaf }, "the site carries what the step read");
        assert!(finding.acts_on_claim);
        assert_eq!(finding.forfeit, PalwForfeitScopeV1::ByRoot);
        assert_eq!(finding.target.segment_count, Some(4), "a five-seat panel is cut in four");
        judge(&state, &evidence(segmented(full_seat(), PalwSegmentMaskV2::full(4)), contradiction.clone()))
            .expect("the full seat's full mask");
        for partial in partial_seats() {
            let mask = assigned(partial);
            assert_eq!(
                judge(&state, &evidence(segmented(partial, mask), contradiction.clone())),
                Err(E::SiteNotAttested),
                "seat {partial} (mask {mask:?}; the leaf is in segment {home}) did not attest everything the verdict read"
            );
            judge(&state, &evidence(segmented(partial, PalwSegmentMaskV2::full(4)), contradiction.clone()))
                .expect("a partial-assigned seat that signed the full mask is held to it");
            let unassigned = PalwSegmentMaskV2::single((0..4).find(|s| !mask.covers(*s)).expect("another segment"));
            assert_eq!(
                judge(&state, &evidence(segmented(partial, unassigned), contradiction.clone())),
                Err(E::SegmentMaskNotAssigned),
                "seat {partial}: a partial mask the panel did not assign"
            );
        }
        // The same fault against another claim's root is not this claim's.
        let elsewhere = live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, h64(0xE0), artifact_root);
        assert_eq!(judge(&elsewhere, &evidence(full(1), contradiction)), Err(E::PanelFalseValidWorkMismatch));
        // And an honest step convicts nobody.
        let (honest, honest_openings, honest_root) = crate::palw_step_refute::tests::base0_honest_case();
        let state =
            live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, honest.binding.committed_execution_root, honest_root);
        let payload =
            evidence(full(1), PalwPanelContradictionV1::StepArithmetic { refutation: honest, operand_openings: honest_openings });
        assert_eq!(judge(&state, &payload), Err(E::PanelFalseValidNeedsContradiction));
    }

    /// **A step whose verdict reads what no segment replay recomputes is a whole-execution site**
    /// (F-1): a KV checkpoint anchor, or the generated-token pin at a decode call. The structural
    /// pass alone decides the site, so the attached objects need not verify for this reading.
    #[test]
    fn a_step_reading_a_checkpoint_or_a_generated_token_is_whole() {
        let (refutation, openings, _) = crate::palw_step_refute::tests::base0_matmul_fraud();
        assert!(refutation.output_preimage.coord.call_index > 0, "the fixture's step is at a decode call");
        let site = |r: crate::palw_step_refute::PalwExecutionStepRefutationV1| {
            palw_false_valid_fault_site_v1(
                &PalwPanelContradictionV1::StepArithmetic { refutation: r, operand_openings: openings.clone() },
                1 << 20,
            )
            .expect("the structural pass opens the output leaf")
            .0
        };
        assert!(matches!(site(refutation.clone()), PalwFaultSiteV1::Leaf { .. }), "no pin, no anchor: the leaves it read");
        let mut pinned = refutation.clone();
        pinned.decode_tokens = Some(zeroed());
        assert_eq!(site(pinned), PalwFaultSiteV1::Whole, "a decode-call step carrying the generated tokens");
        let mut anchored = refutation;
        anchored.kv_checkpoint = Some(zeroed());
        assert_eq!(site(anchored), PalwFaultSiteV1::Whole, "a step reading a committed checkpoint");
    }

    /// **A shape fault is a whole-execution fault, whatever leaf the filer attached.** The
    /// structural pass answers it before opening anything, so the carried `leaf_index` is
    /// unauthenticated and must not aim the conviction at a partial seat.
    #[test]
    fn a_shape_fault_is_whole_whatever_leaf_is_attached() {
        use crate::palw_step_leg::{PalwStepEvidenceV1, PalwStepRefutationV1};
        let (refutation, _, artifact_root) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let mut binding = refutation.binding.clone();
        binding.step_leaf_count += 1;
        crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
        let state =
            live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, binding.committed_execution_root, artifact_root);
        let aimed = PalwPanelContradictionV1::StepStructural(PalwStepRefutationV1 {
            binding: binding.clone(),
            evidence: PalwStepEvidenceV1::StepTile {
                opening: refutation.output_opening.clone(),
                preimage: refutation.output_preimage.clone(),
            },
        });
        let finding = judge(&state, &evidence(full(1), aimed.clone())).expect("a non-canonical leaf count convicts");
        assert_eq!(finding.site, PalwFaultSiteV1::Whole, "the verdict came from the shape pass");
        for partial in partial_seats() {
            assert_eq!(
                judge(&state, &evidence(segmented(partial, assigned(partial)), aimed.clone())),
                Err(E::SiteNotAttested),
                "seat {partial} attested no shape"
            );
        }
    }

    /// **The version and the reporter slot.** A V1 body is not a V2 body, the version is exact, the
    /// slot is empty until F7 arms it, and filling it never makes a second offence.
    #[test]
    fn the_version_and_the_reporter_slot() {
        let state = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        let good = evidence(full(1), PalwPanelContradictionV1::CourtFraud { voided_daa: 55 });
        assert!(judge(&state, &good).is_ok());
        for version in [0u16, 1, 3, u16::MAX] {
            let mut wrong = good.clone();
            wrong.version = version;
            assert_eq!(judge(&state, &wrong), Err(E::PanelFalseValidNeedsContradiction), "version {version}");
        }
        let v1 = crate::palw_offence_v1::PalwPanelFalseValidEvidenceV1 {
            version: crate::palw_offence_v1::PALW_PANEL_FALSE_VALID_VERSION_V1,
            claim_id: h64(CLAIM),
            network_domain: h64(0xD0),
            accused_seat: seat(1).0,
            valid_receipt: v2_receipt(1, h64(CLAIM), PalwReceiptVerdictV2::Valid),
            executor_pubkey: vec![7; 4],
            contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa: 55 },
        };
        let v1_bytes = borsh::to_vec(&v1).expect("a V1 payload serializes");
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &v1_bytes, false, false, rules(), None),
            Err(E::PanelFalseValidNeedsContradiction),
            "a V1 payload is not read as V2"
        );
        let mut revealing = good.clone();
        revealing.reporter_reveal = vec![0xAB; 32];
        let revealing_bytes = bytes(&revealing);
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &revealing_bytes, false, false, rules(), None),
            Err(E::ReporterSlotNotArmed)
        );
        let armed = palw_check_panel_false_valid_v2(&state, &seat(1), &revealing_bytes, false, true, rules(), None)
            .expect("F7 reads the slot");
        assert_eq!(armed, judge(&state, &good).unwrap(), "the slot changes nothing the adjudicator finds");
        // The ledger key is (seat, claim) and nothing else.
        let id = palw_false_valid_offence_id_v2(&seat(1).0, &h64(CLAIM));
        assert_ne!(id, palw_false_valid_offence_id_v2(&seat(2).0, &h64(CLAIM)), "one offence per seat");
        assert_ne!(id, palw_false_valid_offence_id_v2(&seat(1).0, &h64(CLAIM + 1)), "one offence per claim");
        assert_ne!(
            id,
            crate::palw_offence_v1::palw_offence_id_v1(
                crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid,
                &seat(1).0,
                &palw_false_valid_ledger_key_v2(&h64(CLAIM)),
            ),
            "the kind is inside the id"
        );
        // The receipt's claim and seat must be the payload's.
        let mut other_seat = good.clone();
        other_seat.accused_seat = seat(2).0;
        assert_eq!(judge(&state, &other_seat), Err(E::PanelFalseValidSeatMismatch));
        let mut other_claim = good.clone();
        other_claim.receipt = PalwFalseValidReceiptV1::Full(v2_receipt(1, h64(CLAIM + 1), PalwReceiptVerdictV2::Valid));
        assert_eq!(judge(&state, &other_claim), Err(E::PanelFalseValidWorkMismatch));
        for verdict in [
            PalwReceiptVerdictV2::Incapable,
            PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 1 },
            // ADR-0152 Q-1 / Q-6: a sampler is never liable — `Sampled` is not `Valid`.
            PalwReceiptVerdictV2::Sampled,
        ] {
            let mut not_valid = good.clone();
            not_valid.receipt = PalwFalseValidReceiptV1::Full(v2_receipt(1, h64(CLAIM), verdict));
            assert_eq!(judge(&state, &not_valid), Err(E::PanelFalseValidNotValidVerdict), "{verdict:?}");
            let mut segmented_not_valid = good.clone();
            segmented_not_valid.receipt = PalwFalseValidReceiptV1::Segmented(PalwSeatReceiptV3 {
                receipt: v2_receipt(1, h64(CLAIM), verdict),
                segments: PalwSegmentMaskV2::single(0),
            });
            assert_eq!(judge(&state, &segmented_not_valid), Err(E::PanelFalseValidNotValidVerdict), "segmented {verdict:?}");
        }
    }

    /// **The reporter slot's own bound, once F7 arms it**: at most
    /// [`PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES`], asked of an armed slot as well as an unarmed
    /// one — and an unarmed slot is refused for being filled at all, whatever its length.
    #[test]
    fn the_reporter_slot_is_bounded_once_armed() {
        let state = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        let good = evidence(full(1), PalwPanelContradictionV1::CourtFraud { voided_daa: 55 });
        let expected = judge(&state, &good).expect("the empty slot is the unarmed rule");
        let with_reveal = |len: usize| {
            let mut filled = good.clone();
            filled.reporter_reveal = vec![0xAB; len];
            bytes(&filled)
        };
        let at_bound = with_reveal(PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES);
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &at_bound, false, true, rules(), None),
            Ok(expected),
            "armed, a reveal of exactly the bound is read and changes nothing the adjudicator finds"
        );
        let past_bound = with_reveal(PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES + 1);
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &past_bound, false, true, rules(), None),
            Err(E::EvidenceTooLarge),
            "armed, one byte past the bound is refused"
        );
        for payload in [with_reveal(1), at_bound, past_bound] {
            assert_eq!(
                palw_check_panel_false_valid_v2(&state, &seat(1), &payload, false, false, rules(), None),
                Err(E::ReporterSlotNotArmed),
                "unarmed, any filled slot is refused before its length is read"
            );
        }
    }

    /// **The byte cap is what one carrier holds** (F2 review, F-3), and it is asked before a byte is
    /// decoded. A kind-3 object at the cap, in the 0x4b lifecycle payload that carries it, is the
    /// chunk lane's payload plus its own framing, and leaves a standard transaction the carrier
    /// allowance `the_close_ceiling_fits_a_carrier_transaction` asks of a chunk. The measured half —
    /// every offence T46 files, in a signed carrier, weighed by the mass calculator — is
    /// kaspa-consensus's `t46`.
    #[test]
    fn the_byte_cap_is_one_carrier() {
        use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let cap = PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES as usize;
        assert_eq!(cap, crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES, "one carrier's payload");
        let object = crate::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence {
            kind: crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2,
            accused: seat(1),
            evidence_id: h64(1),
            evidence: vec![0u8; cap],
        };
        let carried = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("the carriage serializes")
            .len() as u64;
        assert_eq!(carried, cap as u64 + 140, "the object's framing: the version, the tags, the outpoint, the id and the length");
        assert!(
            carried + 18_000 <= crate::palw_mode_v2::PALW_STANDARD_TX_BYTES,
            "a carrier at the cap leaves a standard transaction the chunk lane's allowance: {carried}"
        );
        let state = PalwChainStateV2::genesis();
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &vec![0u8; cap + 1], false, false, rules(), None),
            Err(E::EvidenceTooLarge)
        );
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &vec![0u8; cap], false, false, rules(), None),
            Err(E::PanelFalseValidNeedsContradiction),
            "at the cap the bytes are read, and these do not decode"
        );
        // A real step refutation sits below it.
        let (refutation, openings, _) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let heavy = evidence(full(1), PalwPanelContradictionV1::StepArithmetic { refutation, operand_openings: openings });
        assert!((bytes(&heavy).len() as u64) < PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
    }

    /// **The signature is the chain's**: the V2 message and context for a full receipt, the V3 ones
    /// (mask included) for a segmented one, under the domain the caller supplies — and a seat that
    /// is not active is not asked.
    #[test]
    fn the_signature_is_checked_under_the_chain_domain_in_the_licensed_form() {
        let state = live_state(voided(55, PalwVoidReasonV2::CourtFraud), h64(0xE0), h64(0xAF));
        let domain = h64(0xD0A1);
        let pubkey = [7u8; 4];
        for receipt in [full(1), segmented(1, PalwSegmentMaskV2::full(4))] {
            let payload = evidence(receipt.clone(), PalwPanelContradictionV1::CourtFraud { voided_daa: 55 });
            let seen: RefCell<Vec<(Vec<u8>, Vec<u8>)>> = RefCell::new(Vec::new());
            let verify = |_: &[u8], message: &[u8], _: &[u8], context: &[u8]| {
                seen.borrow_mut().push((message.to_vec(), context.to_vec()));
                true
            };
            let check = |active: bool, verify: &dyn Fn(&[u8], &[u8], &[u8], &[u8]) -> bool, domain: Hash64| {
                palw_check_panel_false_valid_v2(
                    &state,
                    &seat(1),
                    &bytes(&payload),
                    false,
                    false,
                    rules(),
                    Some(PalwFalseValidSigCheckV1 { chain_domain: domain, seat_pubkey: &pubkey, seat_active: active, verify }),
                )
            };
            check(true, &verify, domain).expect("a signed Valid under the chain's domain");
            let inner = receipt.inner();
            let expected = match &receipt {
                PalwFalseValidReceiptV1::Full(_) => (
                    crate::palw_panel_v2::palw_receipt_message_v2(domain, inner.claim, inner.verdict, inner.signed_daa),
                    crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                ),
                PalwFalseValidReceiptV1::Segmented(signed) => (
                    crate::palw_panel_v2::palw_receipt_message_v3(
                        domain,
                        inner.claim,
                        inner.verdict,
                        inner.signed_daa,
                        signed.segments,
                    ),
                    crate::palw_panel_v2::PALW_RECEIPT_V3_MLDSA87_CONTEXT,
                ),
            };
            assert_eq!(seen.borrow().as_slice(), &[(expected.0.as_byte_slice().to_vec(), expected.1.to_vec())]);
            // A receipt signed for another network verifies against nothing here.
            let other_network =
                crate::palw_panel_v2::palw_receipt_message_v2(h64(0x0711), inner.claim, inner.verdict, inner.signed_daa);
            let only_other = |_: &[u8], message: &[u8], _: &[u8], _: &[u8]| message == other_network.as_byte_slice();
            assert_eq!(check(true, &only_other, domain), Err(E::PanelFalseValidReceiptUnverified));
            assert_eq!(check(false, &verify, domain), Err(E::BondNotActive));
        }
    }

    /// An all-zero borsh encoding of `T` — a value of the right TYPE for a test that only needs to
    /// know which variant a contradiction is, never one any checker would accept.
    // ---- F1 (ADR-0152 v3.1 J-1…J-5) ----------------------------------------------------------------
    use crate::palw_attempt_rules_v1::floor_binding_for_tests_v1;
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 as Form;
    use crate::palw_step_leg::{PalwStepBindingV2, binding_commitment_root_v1};

    /// A floor attempt claim's target under `binding`'s roots, recorded under `anchor`.
    fn attempt_target(binding: &PalwStepBindingV2, anchor: Hash64) -> PalwOffenceTargetV1 {
        PalwOffenceTargetV1 {
            claim_id: h64(CLAIM),
            class_id: binding.shape_profile.shape_profile_id(),
            artifact_root: Hash64::default(),
            executor_bond: seat(99),
            execution_root: binding.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::Attempt),
            segment_count: Some(4),
            phase: Some(PalwClaimPhaseV2::Provisional),
            job_identity: anchor,
            trace_root: binding.full_logits_trace_root,
            output_root: h64(0x72),
        }
    }

    /// `binding` with one part moved and its committed root re-derived as `verify_binding` derives
    /// it — a self-consistent binding of another job, the only kind a filer can carry.
    fn moved(binding: &PalwStepBindingV2, edit: impl FnOnce(&mut PalwStepBindingV2)) -> PalwStepBindingV2 {
        use crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1 as canonical_leg;
        let mut b = binding.clone();
        let before = canonical_leg(&b.job_context);
        let was_canonical = b.activation_leg_root == before;
        edit(&mut b);
        // A context edit carries the statement over to the new context, as a producer of that
        // context would file it — so a test moves the one field it names and nothing else.
        if was_canonical && b.activation_leg_root == before {
            b.activation_leg_root = canonical_leg(&b.job_context);
        }
        b.committed_execution_root = binding_commitment_root_v1(&b);
        crate::palw_step_leg::verify_binding_v1(&b).expect("a re-committed binding verifies");
        b
    }

    /// **F1's identity rule, check by check, on the floor** (SPEC §4.3, addendum §4-bis.2): an honest
    /// binding has no fault; each check names the one moved field; the first failing check wins; the
    /// DA answers leave J5b out; and the refusals (unverified, not the claim's, unrecorded, lane
    /// unknown) write no verdict at all.
    #[test]
    fn f1_the_identity_rule_names_the_first_check_a_floor_binding_fails() {
        let anchor = h64(0xA1C0);
        for form in [Form::Flat, Form::MerkleV1] {
            let honest = floor_binding_for_tests_v1(&anchor, form);
            let class = honest.shape_profile.shape_profile_id();
            let rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: class, da_signer_liability: false };
            let judge = |binding: &PalwStepBindingV2, full: bool| {
                palw_binding_identity_fault_v1(&attempt_target(binding, anchor), binding, rules, full)
            };
            assert_eq!(judge(&honest, true), Ok(None), "{form:?}: the anchor's own job has no fault");
            let other = h64(0xB0B);
            let cases: Vec<(&str, PalwStepBindingV2, PalwIdentityFaultV1)> = vec![
                ("J1", moved(&honest, |b| b.job_context.job_id = other), PalwIdentityFaultV1::JobNotTheClaims),
                ("J3", moved(&honest, |b| b.job_context.execution_seed[0] ^= 1), PalwIdentityFaultV1::SeedNotTheJobs),
                (
                    "J5a max_context",
                    moved(&honest, |b| b.job_context.max_context_tokens -= 1),
                    PalwIdentityFaultV1::ContextNotCanonical,
                ),
                (
                    "J5a decode",
                    moved(&honest, |b| b.job_context.exact_decode_tokens = crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1),
                    PalwIdentityFaultV1::ContextNotCanonical,
                ),
                (
                    "J5a network",
                    moved(&honest, |b| b.job_context.network_id = b"another".to_vec()),
                    PalwIdentityFaultV1::ContextNotCanonical,
                ),
                ("J5a nullifier", moved(&honest, |b| b.job_context.job_nullifier = other), PalwIdentityFaultV1::ContextNotCanonical),
                (
                    "J5b",
                    moved(&honest, |b| {
                        b.job_context.prompt_token_ids_hash =
                            crate::palw_attempt_rules_v1::palw_attempt_prompt_root_v1(&b.shape_profile, &other, 8, form).unwrap()
                    }),
                    PalwIdentityFaultV1::PromptNotTheAnchors,
                ),
            ];
            for (what, binding, fault) in &cases {
                assert_eq!(judge(binding, true), Ok(Some(*fault)), "{form:?} {what}");
            }
            // The DA answers run every check but J5b.
            let relabel = &cases.iter().find(|(what, ..)| *what == "J5b").unwrap().1;
            assert_eq!(judge(relabel, false), Ok(None), "{form:?}: J5b is the contradictions' alone");
            // J2: a binding of another class (a different epsilon is a different class).
            let other_class = moved(&honest, |b| {
                b.shape_profile.base0_rms_eps_q += 1;
                b.job_context.shape_profile_id = b.shape_profile.shape_profile_id();
            });
            let target = attempt_target(&honest, anchor);
            let mut on_other = target.clone();
            on_other.execution_root = other_class.committed_execution_root;
            assert_eq!(
                palw_binding_identity_fault_v1(&on_other, &other_class, rules, true),
                Ok(Some(PalwIdentityFaultV1::ClassNotTheClaims))
            );
            // J4: the claim committed another trace root beside this binding.
            let mut swapped = target.clone();
            swapped.trace_root = h64(0x7ACE_0002);
            assert_eq!(
                palw_binding_identity_fault_v1(&swapped, &honest, rules, true),
                Ok(Some(PalwIdentityFaultV1::TraceNotTheClaims))
            );
            // The first failing check wins: J1 before J4.
            let j1 = &cases[0].1;
            let mut both = attempt_target(j1, anchor);
            both.trace_root = h64(0x7ACE_0003);
            assert_eq!(palw_binding_identity_fault_v1(&both, j1, rules, true), Ok(Some(PalwIdentityFaultV1::JobNotTheClaims)));
            // Refusals.
            let mut unrecorded = attempt_target(j1, anchor);
            unrecorded.job_identity = Hash64::default();
            assert_eq!(palw_binding_identity_fault_v1(&unrecorded, j1, rules, true), Err(E::IdentityNotRecorded), "0 never convicts");
            let mut laneless = target.clone();
            laneless.lane = None;
            assert_eq!(palw_binding_identity_fault_v1(&laneless, &honest, rules, true), Err(E::LaneUnknown));
            let mut not_its = target.clone();
            not_its.execution_root = h64(0xE0);
            assert_eq!(palw_binding_identity_fault_v1(&not_its, &honest, rules, true), Err(E::PanelFalseValidWorkMismatch));
            let mut unverified = honest.clone();
            unverified.job_context.job_id = other;
            assert_eq!(palw_binding_identity_fault_v1(&target, &unverified, rules, true), Err(E::BindingUnverified));
            // J6 and J7: each leg part moved alone, re-committed.
            assert_eq!(
                judge(&moved(&honest, |b| b.activation_leg_root = other), true),
                Ok(Some(PalwIdentityFaultV1::ActivationLegNotCanonical))
            );
            assert_eq!(
                judge(&moved(&honest, |b| b.checkpoint_profile.checkpoint_interval = 2), true),
                Ok(Some(PalwIdentityFaultV1::CheckpointProfileNotCanonical))
            );
            // **F1-M: a model class is held to its own formula job** (the floor's graph at 128, as a
            // class of its own): its CoreV1 binding has no fault; a relabel of another anchor's prompt
            // is J5b; the floor's job under a model class's identity is J5a; and a class too narrow
            // for the formula is not derivable.
            let model = crate::palw_attempt_rules_v1::model_binding_for_tests_v1(&anchor, 128, form);
            let model_class = model.shape_profile.shape_profile_id();
            assert_ne!(model_class, class, "a class of its own");
            let model_rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: class, da_signer_liability: false };
            let judge_model = |binding: &PalwStepBindingV2| {
                palw_binding_identity_fault_v1(&attempt_target(binding, anchor), binding, model_rules, true)
            };
            assert_eq!(judge_model(&model), Ok(None), "{form:?}: the model class's own CoreV1 job");
            assert_eq!(model.job_context.declared_prefill_tokens, 15, "(128/8 − 1, 2)");
            let model_relabel = moved(&model, |b| {
                b.job_context.prompt_token_ids_hash =
                    crate::palw_attempt_rules_v1::palw_attempt_prompt_root_v1(&b.shape_profile, &other, 15, form).unwrap()
            });
            assert_eq!(judge_model(&model_relabel), Ok(Some(PalwIdentityFaultV1::PromptNotTheAnchors)), "{form:?}: model relabel");
            let short = moved(&model, |b| b.job_context.declared_prefill_tokens = 8);
            assert_eq!(judge_model(&short), Ok(Some(PalwIdentityFaultV1::ContextNotCanonical)), "{form:?}: a short prefill");
            let legacy_field = moved(&model, |b| b.job_context.tokenizer_id = other);
            assert_eq!(judge_model(&legacy_field), Ok(Some(PalwIdentityFaultV1::ContextNotCanonical)), "{form:?}: an artifact field");
            let mut narrow = model.clone();
            narrow.shape_profile.n_ctx = 8;
            narrow.job_context.shape_profile_id = narrow.shape_profile.shape_profile_id();
            // An honest narrow binding: its activation leg is the statement over ITS context.
            narrow.activation_leg_root = crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&narrow.job_context);
            let narrow = moved(&narrow, |_| {});
            assert_eq!(
                palw_binding_identity_fault_v1(&attempt_target(&narrow, anchor), &narrow, model_rules, true),
                Err(E::IdentityNotDerivable)
            );
            // The 3a review's L-a: a narrow class escapes J5 only — J4, J6 and J7 still name it.
            let narrow_leg = moved(&narrow, |b| b.activation_leg_root = other);
            assert_eq!(
                palw_binding_identity_fault_v1(&attempt_target(&narrow_leg, anchor), &narrow_leg, model_rules, true),
                Ok(Some(PalwIdentityFaultV1::ActivationLegNotCanonical)),
                "{form:?}: J6 on a narrow class"
            );
            let narrow_ckpt = moved(&narrow, |b| b.checkpoint_profile.checkpoint_interval += 1);
            assert_eq!(
                palw_binding_identity_fault_v1(&attempt_target(&narrow_ckpt, anchor), &narrow_ckpt, model_rules, true),
                Ok(Some(PalwIdentityFaultV1::CheckpointProfileNotCanonical)),
                "{form:?}: J7 on a narrow class"
            );
            let mut narrow_trace = attempt_target(&narrow, anchor);
            narrow_trace.trace_root = other;
            assert_eq!(
                palw_binding_identity_fault_v1(&narrow_trace, &narrow, model_rules, true),
                Ok(Some(PalwIdentityFaultV1::TraceNotTheClaims)),
                "{form:?}: J4 on a narrow class"
            );
        }
    }

    fn fp_job(tokenizer: Hash64) -> crate::palw_freeprompt_v3::PalwFreePromptJobV3 {
        crate::palw_freeprompt_v3::PalwFreePromptJobV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h64(0xD0),
            class_id: h64(CLASS),
            executor_bond: seat(99).0,
            executor_pubkey: vec![7; 4],
            operator_id: h64(0x09),
            anchor_block: h64(0xAB),
            anchor_daa: 77,
            job_nonce: [0x46; 32],
            tokenizer_id: tokenizer,
            prompt_token_ids_hash: h64(0x1D5),
            prompt_tokens: 4,
            decode_token_limit: 6,
            max_context_tokens: 40,
            privacy_mode: crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    fn fp_commitment(
        job: crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        executed: u32,
    ) -> crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
        crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
            job,
            trace_root: Hash64::default(),
            output_root: Hash64::default(),
            schedule_root: Hash64::default(),
            execution_root: Hash64::default(),
            decode_tokens_executed: executed,
            stop_reason: crate::palw_freeprompt_v3::PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 0,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 0,
        }
    }

    /// **`fp_pin_spellings_agree` (T18h's half): the pin a free-prompt claim records from its
    /// commitment is the pin its own context reproduces** — the context `palw_fp_job_context_v3`
    /// builds from the same job and run — and each identity field of the context moves it. Then
    /// J1 on the free-prompt lane: a binding whose context is the commitment's has no fault; one whose
    /// context names another run (a prompt, a ceiling, a tokenizer, a decode count) is
    /// `JobNotTheClaims`.
    #[test]
    fn f1_fp_pin_spellings_agree_and_convict_on_the_free_prompt_lane() {
        use crate::palw_fp_execution_v3::{
            PalwFpClassFactsV3, palw_fp_job_context_v3, palw_fp_job_pin_of_context_v1, palw_fp_job_pin_v1,
            palw_fp_run_facts_for_executed_v1,
        };
        let floor = floor_binding_for_tests_v1(&h64(0xF9), Form::Flat);
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: floor.shape_profile.shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        };
        let job = fp_job(h64(0x70C));
        let executed = 3;
        let ctx = palw_fp_job_context_v3(&job, &class, &palw_fp_run_facts_for_executed_v1(&job, executed), b"misaka-palw-rc")
            .expect("a well-formed run");
        let commitment = fp_commitment(job.clone(), executed);
        let pin = palw_fp_job_pin_v1(&commitment);
        assert_eq!(palw_fp_job_pin_of_context_v1(&ctx), pin, "one spelling from the commitment and from the context");
        for (what, edit) in [
            ("job id", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.job_id = h64(1)) as Box<dyn Fn(&mut _)>),
            ("seed", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.execution_seed[3] ^= 1)),
            ("tokenizer", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.tokenizer_id = h64(2))),
            ("prompt", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.prompt_token_ids_hash = h64(3))),
            ("prefill", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.declared_prefill_tokens += 1)),
            ("decode", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.exact_decode_tokens += 1)),
            ("ceiling", Box::new(|c: &mut crate::palw_v2::PalwJobContextV2| c.max_context_tokens += 1)),
        ] {
            let mut moved_ctx = ctx.clone();
            edit(&mut moved_ctx);
            assert_ne!(palw_fp_job_pin_of_context_v1(&moved_ctx), pin, "the pin reads the {what}");
        }
        let rules = PalwIdentityRulesV1 { prompt_ids_form: Form::Flat, base_class_id: class.shape_profile_id, da_signer_liability: false };
        let fp_binding = moved(&floor, |b| {
            b.job_context = ctx.clone();
            b.activation_leg_root = crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&ctx);
        });
        let mut target = attempt_target(&fp_binding, pin);
        target.lane = Some(PalwClaimSourceKindV1::FreePrompt);
        assert_eq!(palw_binding_identity_fault_v1(&target, &fp_binding, rules, true), Ok(None), "the commitment's own run");
        let longer = moved(&fp_binding, |b| b.job_context.max_context_tokens += 1);
        let mut on_longer = target.clone();
        on_longer.execution_root = longer.committed_execution_root;
        assert_eq!(
            palw_binding_identity_fault_v1(&on_longer, &longer, rules, true),
            Ok(Some(PalwIdentityFaultV1::JobNotTheClaims)),
            "another run's context under the commitment's pin"
        );
    }

    /// **F1's output rule** (addendum §4-bis.4, flat classes in F1): the pin authenticates the
    /// generated ids against the binding's trace root, and the claim's `output_root` is held to
    /// `output_commitment_v2(ctx, ids, rendered_output_hash_v2(&[]))`.
    #[test]
    fn f1_the_output_rule_holds_the_output_root_to_the_generated_ids() {
        use crate::palw_step_refute::{PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, base0_logits_trace_root_v1};
        let anchor = h64(0x0A7);
        let floor = floor_binding_for_tests_v1(&anchor, Form::Flat);
        let vocab = floor.shape_profile.vocab_size as usize;
        let rows = vec![(0..vocab as i32).map(|v| (v * 7919) % 1013).collect::<Vec<i32>>()];
        let ids = vec![5u32];
        let binding = moved(&floor, |b| b.full_logits_trace_root = base0_logits_trace_root_v1(&b.job_context, &rows, &ids));
        let pin =
            PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 { logits_rows: rows.clone(), generated_token_ids: ids.clone() });
        let honest_output = crate::palw_v2::output_commitment_v2(
            &binding.job_context.context_hash(),
            &ids,
            &crate::palw_v2::rendered_output_hash_v2(&[]),
        );
        let mut target = attempt_target(&binding, anchor);
        target.output_root = honest_output;
        assert_eq!(palw_output_fault_v1(&target, &binding, &pin), Ok(false), "the run's own output");
        target.output_root = h64(0x0B0B);
        assert_eq!(palw_output_fault_v1(&target, &binding, &pin), Ok(true), "a ground output root");
        // The pin is authenticated: other ids under the same trace root are refused, not believed.
        let lying = PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: vec![6] });
        assert_eq!(palw_output_fault_v1(&target, &binding, &lying), Err(E::PanelFalseValidNeedsContradiction));
        // Refusals: no identity recorded; another class's binding is 9's (J2), not 10's.
        let mut unrecorded = target.clone();
        unrecorded.job_identity = Hash64::default();
        assert_eq!(palw_output_fault_v1(&unrecorded, &binding, &pin), Err(E::IdentityNotRecorded));
        let mut other_class = target.clone();
        other_class.class_id = h64(0xC1A55);
        assert!(matches!(palw_output_fault_v1(&other_class, &binding, &pin), Err(E::ContradictionNotAdmitted(_))));
    }

    /// **The admission tables** (addendum §4-bis.7): 9 and 10 are claim-proving for kind 3 and kind
    /// 4; kind 4 admits the execution- and claim-proving contradictions and refuses every void,
    /// permit, equivocation and the v1 legs by name; the finding's forfeiture scope follows the class.
    #[test]
    fn f1_the_admission_tables_and_the_executor_refuted_key() {
        use PalwPanelContradictionV1 as Cx;
        let binding = floor_binding_for_tests_v1(&h64(1), Form::Flat);
        let pin = crate::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(crate::palw_step_refute::PalwBase0DecodeTokensV1 {
            logits_rows: vec![],
            generated_token_ids: vec![],
        });
        for c in [Cx::IdentityMismatch { binding: binding.clone() }, Cx::OutputMismatch { binding: binding.clone(), pin }] {
            assert_eq!(palw_false_valid_admission_v1(&c), Ok(PalwFalseValidAdmissionV1::ClaimProving));
            assert_eq!(palw_executor_refuted_admission_v1(&c), Ok(PalwFalseValidAdmissionV1::ClaimProving));
        }
        for c in [
            Cx::CourtFraud { voided_daa: 5 },
            Cx::ProducerWithholding { voided_daa: 5 },
            Cx::ConflictingPermit { span: 1, round: 2, permit_index: 3 },
            Cx::CourtExecutorGuilty { offence_id: h64(4) },
        ] {
            assert!(matches!(palw_executor_refuted_admission_v1(&c), Err(E::ContradictionNotAdmitted(_))), "{c:?}");
        }
        // One offence per claim, whatever the proof.
        let executor = seat(99).0;
        assert_eq!(
            palw_executor_refuted_offence_id_v1(&executor, &h64(CLAIM)),
            crate::palw_offence_v1::palw_offence_id_v1(
                crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
                &executor,
                &palw_executor_refuted_ledger_key_v1(&h64(CLAIM))
            )
        );
        assert_ne!(palw_executor_refuted_ledger_key_v1(&h64(CLAIM)), palw_false_valid_ledger_key_v2(&h64(CLAIM)), "its own domain");
    }

    fn zeroed<T: borsh::BorshDeserialize>() -> T {
        let zeros = vec![0u8; 1 << 16];
        T::deserialize(&mut zeros.as_slice()).expect("an all-zero encoding decodes")
    }
}

/// **ADR-0152 v3.1 F1c / F1-M, Tier A** (addendum §4-bis.3–7): the logits head on every shipped
/// profile, `PromptNotAnchored`'s rule at the 2M width, the heavy-prompt charge, and the site table
/// that puts the job faults at `AnyValid`.
#[cfg(test)]
mod f1c_tests {
    use super::*;
    use crate::palw_offence_v1::{PalwOffenceKindV1, PalwOffenceVerifyError as E, PalwPromptProofV1};
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use crate::palw_step::palw_logits_head_v1;

    fn h64(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn target_of(binding: &crate::palw_step_leg::PalwStepBindingV2, anchor: Hash64) -> PalwOffenceTargetV1 {
        PalwOffenceTargetV1 {
            claim_id: h64(0xC1A1),
            class_id: binding.shape_profile.shape_profile_id(),
            artifact_root: Hash64::default(),
            executor_bond: PalwBondKeyV2(TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(0xB0),
                index: 0,
            }),
            execution_root: binding.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::Attempt),
            segment_count: Some(4),
            phase: None,
            job_identity: anchor,
            trace_root: binding.full_logits_trace_root,
            output_root: h64(0x72),
        }
    }

    fn recommitted(mut b: crate::palw_step_leg::PalwStepBindingV2) -> crate::palw_step_leg::PalwStepBindingV2 {
        b.activation_leg_root = crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&b.job_context);
        b.committed_execution_root = crate::palw_step_leg::binding_commitment_root_v1(&b);
        crate::palw_step_leg::verify_binding_v1(&b).expect("a re-committed binding verifies");
        b
    }

    /// **`palw_logits_head_v1` holds on every profile the chain ships** — the floor, the t12 rows
    /// (A16 graph-v7 at 8,192 and 2,097,152, Qwen3.6 graph-v7 at 512) — and on nothing whose lane is
    /// `Float32`, whose last node is not the head, or whose head is not the vocabulary.
    #[test]
    fn the_logits_head_holds_on_every_shipped_profile() {
        use crate::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        use crate::palw_context_ladder::{palw_a16_context_row_profile_v7, palw_qwen36_context_row_profile_v7};
        let rows = [
            ("floor", base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap()),
            ("A16@8192", palw_a16_context_row_profile_v7(8_192).unwrap()),
            ("A16@2M", palw_a16_context_row_profile_v7(2_097_152).unwrap()),
            ("Q36@512", palw_qwen36_context_row_profile_v7(512).unwrap()),
        ];
        for (label, profile) in rows {
            let head = palw_logits_head_v1(&profile).unwrap_or_else(|| panic!("{label}: the head predicate holds"));
            assert_eq!(head.slot + 1, profile.global_node_count(), "{label}: the last slot");
            assert_eq!(head.tiles, profile.vocab_size.div_ceil(head.tile_len), "{label}: the row's head tiles");
            let mut float = profile.clone();
            float.lane = crate::palw_step::PalwStepLaneV1::Float32;
            assert_eq!(palw_logits_head_v1(&float), None, "{label}: a Float32 class has no provable head");
            let mut short = profile.clone();
            short.post_nodes.last_mut().unwrap().out_len =
                crate::palw_step::PalwStepOutLenV1::Fixed { elements: profile.vocab_size - 1 };
            assert_eq!(palw_logits_head_v1(&short), None, "{label}: a head that is not the vocabulary");
            let mut other_kernel = profile.clone();
            other_kernel.post_nodes.last_mut().unwrap().kernel_semantics_id = h64(0xBAD);
            assert_eq!(palw_logits_head_v1(&other_kernel), None, "{label}: a kernel outside the three heads");
        }
    }

    /// **`PromptNotAnchored` (13) at the 2M width** (a model class at `n_ctx` 2,097,152: the formula's
    /// 262,143-id prompt, past J5b's inline bound): a prompt root that is another anchor's convicts by
    /// a `Tile` and by `Whole`; the honest root is `PromptHolds` both ways; and the refusals — the
    /// floor's 8-id prompt (9's J5b), a moved prefill (9's J5a), a free-prompt claim, an unrecorded
    /// identity, another class's binding — never convict. 9 cannot see the relabel at this width (its
    /// J5b is not inline), which is why 13 exists.
    #[test]
    fn prompt_not_anchored_judges_the_2m_prompt() {
        use crate::palw_attempt_rules_v1::{
            model_binding_for_tests_v1, palw_attempt_canonical_v1, palw_attempt_prompt_ids_v1, palw_attempt_prompt_root_v1,
        };
        let form = PalwPromptIdsFormV1::MerkleV1;
        let rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: h64(0xF100), da_signer_liability: false };
        let (anchor, other) = (h64(0x13A0), h64(0x13B0));
        let honest = model_binding_for_tests_v1(&anchor, 2_097_152, form);
        let profile = honest.shape_profile.clone();
        let canonical = palw_attempt_canonical_v1(&profile, false).unwrap();
        assert_eq!(canonical, (262_143, 2));
        let vocab = u64::from(profile.vocab_size);
        let judge = |b: &crate::palw_step_leg::PalwStepBindingV2, proof: &PalwPromptProofV1| {
            palw_prompt_not_anchored_fault_v1(&target_of(b, anchor), b, proof, rules)
        };
        let anchored_ids = palw_attempt_prompt_ids_v1(&anchor, vocab, canonical.0);
        let tile_of = |ids: &[u32], position: u32| {
            PalwPromptProofV1::Tile(crate::palw_prompt_ids_v1::prompt_ids_opening_v1(ids, position).expect("an opening"))
        };
        assert_eq!(judge(&honest, &PalwPromptProofV1::Whole), Ok(false), "the honest root holds");
        for position in [0u32, 131_071, 262_142] {
            assert_eq!(judge(&honest, &tile_of(&anchored_ids, position)), Ok(false), "position {position}: the honest tile holds");
        }
        // The relabel: another anchor's prompt, committed under this anchor's context.
        let mut relabel = honest.clone();
        relabel.job_context.prompt_token_ids_hash = palw_attempt_prompt_root_v1(&profile, &other, canonical.0, form).unwrap();
        let relabel = recommitted(relabel);
        assert_eq!(
            palw_binding_identity_fault_v1(&target_of(&relabel, anchor), &relabel, rules, true),
            Ok(None),
            "9 cannot see it: J5b is not inline at 262,143 ids"
        );
        assert_eq!(judge(&relabel, &PalwPromptProofV1::Whole), Ok(true), "Whole convicts");
        let their_ids = palw_attempt_prompt_ids_v1(&other, vocab, canonical.0);
        for position in [0u32, 200_000] {
            assert_eq!(judge(&relabel, &tile_of(&their_ids, position)), Ok(true), "position {position}: a Tile convicts");
        }
        // A tile that is not the committed prompt's does not open against it.
        assert_eq!(judge(&relabel, &tile_of(&anchored_ids, 0)), Err(E::PanelFalseValidNeedsContradiction));
        // The refusals.
        let floor = crate::palw_attempt_rules_v1::floor_binding_for_tests_v1(&anchor, form);
        let floor_rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: floor.shape_profile.shape_profile_id(), da_signer_liability: false };
        assert!(matches!(
            palw_prompt_not_anchored_fault_v1(&target_of(&floor, anchor), &floor, &PalwPromptProofV1::Whole, floor_rules),
            Err(E::ContradictionNotAdmitted(_))
        ));
        let mut moved = relabel.clone();
        moved.job_context.declared_prefill_tokens -= 1;
        assert!(matches!(judge(&recommitted(moved), &PalwPromptProofV1::Whole), Err(E::ContradictionNotAdmitted(_))), "J5a's");
        let mut fp_target = target_of(&relabel, anchor);
        fp_target.lane = Some(PalwClaimSourceKindV1::FreePrompt);
        assert!(matches!(
            palw_prompt_not_anchored_fault_v1(&fp_target, &relabel, &PalwPromptProofV1::Whole, rules),
            Err(E::ContradictionNotAdmitted(_))
        ));
        let mut unrecorded = target_of(&relabel, anchor);
        unrecorded.job_identity = Hash64::default();
        assert_eq!(
            palw_prompt_not_anchored_fault_v1(&unrecorded, &relabel, &PalwPromptProofV1::Whole, rules),
            Err(E::IdentityNotRecorded)
        );
        let mut other_class = target_of(&relabel, anchor);
        other_class.class_id = h64(0xC1A55);
        assert!(matches!(
            palw_prompt_not_anchored_fault_v1(&other_class, &relabel, &PalwPromptProofV1::Whole, rules),
            Err(E::ContradictionNotAdmitted(_))
        ));
        let mut not_its = target_of(&relabel, anchor);
        not_its.execution_root = h64(0xE0);
        assert_eq!(
            palw_prompt_not_anchored_fault_v1(&not_its, &relabel, &PalwPromptProofV1::Whole, rules),
            Err(E::PanelFalseValidWorkMismatch)
        );
    }

    /// **The heavy charge is the whole prompt a `Whole` 13 recomputes, and nothing else costs one.**
    #[test]
    fn only_a_whole_prompt_not_anchored_is_charged() {
        let binding = crate::palw_attempt_rules_v1::model_binding_for_tests_v1(&h64(1), 2_097_152, PalwPromptIdsFormV1::MerkleV1);
        let evidence = |contradiction: PalwPanelContradictionV1| {
            borsh::to_vec(&PalwExecutorRefutedEvidenceV1 {
                version: PALW_EXECUTOR_REFUTED_VERSION_V1,
                claim_id: h64(2),
                contradiction,
                prompt_ids_opening: None,
                reporter_reveal: Vec::new(),
            })
            .unwrap()
        };
        let whole =
            evidence(PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof: PalwPromptProofV1::Whole });
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::ExecutorRefuted, &whole), 262_143);
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::PanelFalseValidV2, &whole), 0, "not a kind-3 payload");
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::ExecutorEquivocation, &whole), 0);
        let tile = crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&[1, 2, 3], 0).unwrap();
        let tiled =
            evidence(PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof: PalwPromptProofV1::Tile(tile) });
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::ExecutorRefuted, &tiled), 0, "a Tile is cheap");
        let nine = evidence(PalwPanelContradictionV1::IdentityMismatch { binding });
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::ExecutorRefuted, &nine), 0);
        assert_eq!(palw_offence_heavy_prompt_ids_v1(PalwOffenceKindV1::ExecutorRefuted, b"junk"), 0);
        assert!(262_143 <= crate::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1, "one 2M check a block");
        assert!(2 * 262_143 > crate::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1, "and not two");
    }

    /// **The remembered root is the root** (the Phase 3 review's heavy-budget finding): the memo a
    /// second Whole on the same claim reads answers exactly what the recompute does, for several
    /// anchors, and evicts without changing an answer.
    #[test]
    fn the_remembered_prompt_root_is_the_computed_one() {
        use crate::palw_attempt_rules_v1::{
            PALW_PROMPT_ROOT_MEMO_ENTRIES_V1, palw_attempt_prompt_root_memo_v1, palw_attempt_prompt_root_v1,
        };
        let profile =
            crate::palw_attempt_rules_v1::model_binding_for_tests_v1(&h64(1), 65_536, PalwPromptIdsFormV1::MerkleV1).shape_profile;
        for round in 0..2 {
            for n in 0..(PALW_PROMPT_ROOT_MEMO_ENTRIES_V1 as u64 + 3) {
                let anchor = h64(0x3E30_0000 + n);
                assert_eq!(
                    palw_attempt_prompt_root_memo_v1(&profile, &anchor, 8_191, PalwPromptIdsFormV1::MerkleV1),
                    palw_attempt_prompt_root_v1(&profile, &anchor, 8_191, PalwPromptIdsFormV1::MerkleV1),
                    "round {round}, anchor {n}"
                );
            }
        }
    }

    /// **The site table** (addendum §4-bis.7; the user's decision of 2026-09-24): 9's job faults
    /// (J2, J1, J3, J5a, J5b) and 13 at `AnyValid`; 9's J4/J6/J7, 10, 11, 12 at `Whole`. An
    /// `AnyValid` fault holds the full receipt and the seat's own assigned partial mask; it never
    /// holds an empty mask or a partial mask the panel did not assign.
    #[test]
    fn the_job_faults_are_attested_by_every_valid() {
        use PalwIdentityFaultV1 as J;
        let binding = crate::palw_attempt_rules_v1::floor_binding_for_tests_v1(&h64(1), PalwPromptIdsFormV1::MerkleV1);
        let nine = PalwPanelContradictionV1::IdentityMismatch { binding: binding.clone() };
        for (fault, any) in [
            (J::ClassNotTheClaims, true),
            (J::JobNotTheClaims, true),
            (J::SeedNotTheJobs, true),
            (J::ContextNotCanonical, true),
            (J::PromptNotTheAnchors, true),
            (J::TraceNotTheClaims, false),
            (J::ActivationLegNotCanonical, false),
            (J::CheckpointProfileNotCanonical, false),
        ] {
            assert_eq!(palw_claim_proving_site_v1(&nine, Some(fault)), any.then_some(PalwFaultSiteV1::AnyValid), "{fault:?}");
        }
        let thirteen = PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof: PalwPromptProofV1::Whole };
        assert_eq!(palw_claim_proving_site_v1(&thirteen, None), Some(PalwFaultSiteV1::AnyValid));
        let pin = crate::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(crate::palw_step_refute::PalwBase0DecodeTokensV1 {
            logits_rows: vec![],
            generated_token_ids: vec![],
        });
        assert_eq!(palw_claim_proving_site_v1(&PalwPanelContradictionV1::OutputMismatch { binding, pin }, None), None, "10 is Whole");

        // Liability at AnyValid, over a five-seat panel's four segments.
        let receipt = |mask: PalwSegmentMaskV2| {
            PalwFalseValidReceiptV1::Segmented(crate::palw_panel_v2::PalwSeatReceiptV3 {
                receipt: crate::palw_panel_v2::PalwSeatReceiptV2 {
                    claim: h64(1),
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: PalwBondKeyV2(TransactionOutpoint {
                        transaction_id: crate::tx::TransactionId::from_u64_word(9),
                        index: 0,
                    }),
                    signed_daa: 1,
                    signature: vec![],
                },
                segments: mask,
            })
        };
        let assignment = palw_segment_assignment_v2(h64(0xA7), h64(1), 5);
        for seat in 0..5u16 {
            let mask = assignment.mask_of(seat);
            let liable = palw_false_valid_liable_v1(&receipt(mask), PalwFaultSiteV1::AnyValid, Some(4), Some(mask), 1_000);
            assert_eq!(liable, Ok(()), "seat {seat}: its assigned mask attested the job");
            assert_eq!(
                palw_false_valid_liable_v1(&receipt(mask), PalwFaultSiteV1::Whole, Some(4), Some(mask), 1_000),
                if mask.is_full(4) { Ok(()) } else { Err(E::SiteNotAttested) },
                "seat {seat} at Whole"
            );
        }
        let partial = assignment.mask_of(if assignment.full_seat == 0 { 1 } else { 0 });
        let other = assignment.mask_of(if assignment.full_seat == 2 { 3 } else { 2 });
        assert_ne!(partial, other);
        assert_eq!(
            palw_false_valid_liable_v1(&receipt(partial), PalwFaultSiteV1::AnyValid, Some(4), Some(other), 1_000),
            Err(E::SegmentMaskNotAssigned),
            "a mask the panel did not assign is not a Valid this rule reads"
        );
        assert_eq!(
            palw_false_valid_liable_v1(&receipt(PalwSegmentMaskV2::NONE), PalwFaultSiteV1::AnyValid, Some(4), Some(partial), 1_000),
            Err(E::SiteNotAttested),
            "an empty mask attested nothing"
        );
    }
}
