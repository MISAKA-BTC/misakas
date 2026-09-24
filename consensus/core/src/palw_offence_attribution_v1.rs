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
//! The job-identity link — that the root answers THIS claim's job and not a borrowed one — is
//! stage F1, which stores the identity the claim was mined under.
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

/// Which lane a live claim came from. Not stored: read off the claim row when a target is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimSourceKindV1 {
    Attempt,
    FreePrompt,
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
    /// compares. Only a whole attestation covers it. (F1 adds a site every `Valid` covers.)
    Whole,
}

impl PalwFaultSiteV1 {
    /// A fault in step leaf `leaf` that the verdict found in that leaf and read nothing else for.
    pub fn leaf_alone(leaf: u64) -> Self {
        Self::Leaf { leaf, first_read: leaf, last_read: leaf }
    }
}

/// The adjudicator's verdict: whom and what a conviction lands on, where the fault is, and whether
/// the contradiction proves the EXECUTION false (and so voids or reverses the claim) or only names
/// a void the chain already wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFalseValidFindingV1 {
    pub target: PalwOffenceTargetV1,
    pub site: PalwFaultSiteV1,
    pub execution_proving: bool,
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
        });
    }
    let row = state.panel_liability(claim_id)?;
    Some(PalwOffenceTargetV1 {
        claim_id: *claim_id,
        class_id: row.class_id,
        artifact_root: artifact_of(&row.class_id),
        executor_bond: row.executor_bond,
        execution_root: row.execution_root,
        lane: None,
        segment_count: None,
        phase: None,
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
///   whole lock. Refused until F3 lands seat-side disclosure and the S-cap;
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
        C::StepArithmetic { .. } | C::StepStructural(_) | C::ForgedOutput { .. } => Ok(PalwFalseValidAdmissionV1::ExecutionProving),
        C::ProducerWithholding { .. } => Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
            "ProducerWithholding is a void the producer's own silence writes, and a seat owes no disclosure it could answer it with (until F3)",
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
        PalwPanelContradictionV1::ForgedOutput { .. }
        | PalwPanelContradictionV1::ProducerWithholding { .. }
        | PalwPanelContradictionV1::CourtFraud { .. }
        | PalwPanelContradictionV1::ExecutorEquivocation(_)
        | PalwPanelContradictionV1::CourtExecutorGuilty { .. }
        | PalwPanelContradictionV1::ConflictingPermit { .. }
        | PalwPanelContradictionV1::Legs(_) => Ok((PalwFaultSiteV1::Whole, 0)),
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
///   exact for the one-segment masks a panel assigns). Never at `Whole`.
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
/// 6. no session is open on the claim — a court, or a data-availability session held or not (the
///    claim `DefaultDisputed`); the filer waits. F2 review, F-4: this is stricter than the "court
///    only" the peer agreed, on purpose — a void in the middle of a DA session is unhandled until F3
///    (the session's accuser reservation and deadline would be left to a claim that is no longer
///    disputed);
/// 7. the contradiction is admitted ([`palw_false_valid_admission_v1`]; `ProducerWithholding` is
///    refused until F3), and a named void is the one the chain wrote on this claim
///    (`palw_void_binds_claim_v1`, the V1 fold's own reading);
/// 8. an execution-proving contradiction convicts against the TARGET's `execution_root` and
///    artifact root at the class's ladder ([`palw_false_valid_convicts_execution_v2`], the prompt
///    read through the evidence's opening where the job commits a Merkle root) — the root pin is the
///    link from claim to execution, and nothing compares a job id with the claim id; a
///    `ForgedOutput` is refused under ADR-0082's decode rules unless the claim is known to be an
///    attempt;
/// 9. the receipt attested everything the fault's verdict rests on ([`palw_false_valid_liable_v1`]),
///    a partial mask being the one the panel assigned.
pub fn palw_check_panel_false_valid_v2(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    evidence: &[u8],
    fp_decode_rules_active: bool,
    reporter_armed: bool,
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
    if state.open_courts_of(&target.claim_id) > 0
        || state.held_da_missing_of(&target.claim_id).is_some()
        || matches!(target.phase, Some(PalwClaimPhaseV2::DefaultDisputed { .. }))
    {
        return Err(PalwOffenceVerifyError::ClaimUnderSession);
    }
    let ladder = state.class_step_ladder_v1(&target.class_id, PALW_FALSE_VALID_NETWORK_LADDER_V1);
    let execution_proving = match palw_false_valid_admission_v1(&payload.contradiction)? {
        PalwFalseValidAdmissionV1::NamedVoid { reason, voided_daa } => {
            if !state.palw_void_binds_claim_v1(&target.claim_id, reason, voided_daa) {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            false
        }
        PalwFalseValidAdmissionV1::ExecutionProving => {
            if fp_decode_rules_active
                && matches!(payload.contradiction, PalwPanelContradictionV1::ForgedOutput { .. })
                && target.lane != Some(PalwClaimSourceKindV1::Attempt)
            {
                return Err(PalwOffenceVerifyError::ContradictionNotAdmitted(
                    "ForgedOutput under ADR-0082's decode rules convicts only a claim known to be an attempt",
                ));
            }
            palw_false_valid_convicts_execution_v2(
                &payload.contradiction,
                payload.prompt_ids_opening.as_ref(),
                target.execution_root,
                target.artifact_root,
                ladder,
            )?;
            true
        }
    };
    let (site, step_leaf_count) = palw_false_valid_fault_site_v1(&payload.contradiction, ladder)?;
    let assigned = palw_false_valid_assigned_mask_v1(state, &target.claim_id, accused);
    palw_false_valid_liable_v1(&payload.receipt, site, target.segment_count, assigned, step_leaf_count)?;
    Ok(PalwFalseValidFindingV1 { target, site, execution_proving })
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
        palw_check_panel_false_valid_v2(state, &PalwBondKeyV2(payload.accused_seat), &bytes(payload), false, false, None)
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
        let armed = palw_check_panel_false_valid_v2(&fp, &seat(1), &bytes(&payload), true, false, None);
        assert!(matches!(armed, Err(E::ContradictionNotAdmitted(_))), "{armed:?}");
        let attempt = palw_check_panel_false_valid_v2(&state, &seat(1), &bytes(&payload), true, false, None);
        assert_eq!(attempt, Err(E::PanelFalseValidWorkMismatch), "an attempt claim is still judged");
        // A named void binds only the void the chain wrote: this claim, CourtFraud, this DAA.
        let contradiction = C::CourtFraud { voided_daa: 55 };
        let finding = judge(&state, &evidence(full(1), contradiction.clone())).expect("the void the chain wrote binds");
        assert_eq!(finding.site, PalwFaultSiteV1::Whole);
        assert!(!finding.execution_proving, "a named void names a claim, not a root");
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
            "a retired claim's cut is not in state until F1"
        );
        // No target, and a claim in session: a court, or a data-availability session held or not.
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
        assert_eq!(judge(&under_da, &evidence(full(1), step.clone())), Err(E::ClaimUnderSession), "a DA session (F-4)");
        let licensed = live_state(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 30 }, root, artifact_root);
        judge(&licensed, &evidence(full(1), step)).expect("and the same proof convicts once the session is gone");
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
        assert!(finding.execution_proving);
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
            palw_check_panel_false_valid_v2(&state, &seat(1), &v1_bytes, false, false, None),
            Err(E::PanelFalseValidNeedsContradiction),
            "a V1 payload is not read as V2"
        );
        let mut revealing = good.clone();
        revealing.reporter_reveal = vec![0xAB; 32];
        let revealing_bytes = bytes(&revealing);
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &revealing_bytes, false, false, None),
            Err(E::ReporterSlotNotArmed)
        );
        let armed = palw_check_panel_false_valid_v2(&state, &seat(1), &revealing_bytes, false, true, None).expect("F7 reads the slot");
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
        for verdict in [PalwReceiptVerdictV2::Incapable, PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 1 }] {
            let mut not_valid = good.clone();
            not_valid.receipt = PalwFalseValidReceiptV1::Full(v2_receipt(1, h64(CLAIM), verdict));
            assert_eq!(judge(&state, &not_valid), Err(E::PanelFalseValidNotValidVerdict), "{verdict:?}");
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
            palw_check_panel_false_valid_v2(&state, &seat(1), &at_bound, false, true, None),
            Ok(expected),
            "armed, a reveal of exactly the bound is read and changes nothing the adjudicator finds"
        );
        let past_bound = with_reveal(PALW_FALSE_VALID_MAX_REPORTER_REVEAL_BYTES + 1);
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &past_bound, false, true, None),
            Err(E::EvidenceTooLarge),
            "armed, one byte past the bound is refused"
        );
        for payload in [with_reveal(1), at_bound, past_bound] {
            assert_eq!(
                palw_check_panel_false_valid_v2(&state, &seat(1), &payload, false, false, None),
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
            palw_check_panel_false_valid_v2(&state, &seat(1), &vec![0u8; cap + 1], false, false, None),
            Err(E::EvidenceTooLarge)
        );
        assert_eq!(
            palw_check_panel_false_valid_v2(&state, &seat(1), &vec![0u8; cap], false, false, None),
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
    fn zeroed<T: borsh::BorshDeserialize>() -> T {
        let zeros = vec![0u8; 1 << 16];
        T::deserialize(&mut zeros.as_slice()).expect("an all-zero encoding decodes")
    }
}
