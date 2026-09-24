//! **ADR-0144 §9: the objective-offence ledger.**
//!
//! Claim lifecycle (`Provisional → … → Final`) and economic accountability are different
//! questions. A claim that has already reached `Final` is not rolled back; a cryptographically
//! attributable offence still debits the accused PALW bond, once, and the debit reverts with the
//! delta that wrote it.
//!
//! This module is the identity of an offence (`offence_id`) and the record the chain stores after
//! it has been consumed. The fold in [`crate::palw_state_v2`] is the writer. Below
//! `PalwTransitionExtrasV1::objective_offence_daa` nothing here is consulted, so a dormant chain
//! commits the state root it always did.
//!
//! `Params::palw_objective_offence` is `None` on mainnet (Some-only in the
//! fingerprint). Testnet-11 schedules it at [`crate::config::params::PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA`].
//! Tests arm extras directly. The virtual processor is the cryptographic gate: it verifies
//! kind-specific evidence against the accused PALW bond before the fold consumes `offence_id`.

use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;
use kaspa_hashes::Hash64;

/// Keyed-BLAKE2b-512 domain of [`palw_offence_id_v1`].
pub const PALW_OFFENCE_DOMAIN_ID_V1: &[u8] = b"misaka-palw/objective-offence-id/v1";

/// Keyed-BLAKE2b-512 domain of [`palw_panel_false_valid_ledger_evidence_id_v1`].
pub const PALW_PANEL_FALSE_VALID_LEDGER_DOMAIN_V1: &[u8] = b"misaka-palw/panel-false-valid-ledger/v1";

pub const PALW_OFFENCE_ALL_DOMAINS: &[&[u8]] = &[PALW_OFFENCE_DOMAIN_ID_V1, PALW_PANEL_FALSE_VALID_LEDGER_DOMAIN_V1];

/// Wire version of [`PalwPanelFalseValidEvidenceV1`].
pub const PALW_PANEL_FALSE_VALID_VERSION_V1: u16 = 1;

/// Smallest colluding quorum of a 3-of-5 panel.
pub const PALW_PANEL_COLLUDING_QUORUM_V1: u64 = 3;

/// What kind of objectively attributable offence this record is.
///
/// Quality, usefulness, hardware speedup, honest cache use, delay, crash, silence and class
/// divergence are not in this table (ADR-0144 §9). A protocol defect that the active rules
/// accepted is not converted into an offence either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwOffenceKindV1 {
    /// Two signed execution attestations, one signer, two roots (PALW-S / carriage 0x07).
    ExecutorEquivocation = 0,
    /// A panel seat signed `Valid` and an objective protocol fact later contradicts that signature.
    PanelFalseValid = 1,
    /// A court (or one-move court) found the executor guilty. Independent of whether the claim
    /// is still live — `Final` does not consume this offence.
    CourtExecutorGuilty = 2,
    /// **ADR-0152 v2 F2: a panel seat signed `Valid` on a claim, and an objective contradiction
    /// pinned to that claim's committed `execution_root` shows the work false** — judged by
    /// [`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`] alone, on the
    /// receipt form the chain licensed (full or segmented) and under the chain's own domain.
    /// Appended last, so no existing row or payload moves; admitted only past
    /// `Params::palw_offence_attribution`, where [`Self::PanelFalseValid`] is refused.
    PanelFalseValidV2 = 3,
}

/// The chain's memory that this offence has already been paid.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConsumedOffenceV1 {
    pub kind: PalwOffenceKindV1,
    pub accused: TransactionOutpoint,
    pub amount: u64,
    pub accepted_daa: u64,
    /// **ADR-0151: the execution whose rights this conviction forfeits.**
    ///
    /// The `CanonicalWork` the convicted Final named, so the economic-safety bundle can find every
    /// downstream right and revoke it — the unused quanta of that root, in `round_finals`, in a span
    /// snapshot, and in a seeded schedule. Zero for a conviction that names no execution
    /// (`ExecutorEquivocation` is about a key signing two roots, not about one root's rights).
    ///
    /// Recorded HERE rather than in a new state map because `consumed_offences` is already rooted,
    /// already has a delta entry and is already the one place a conviction is written once — and the
    /// forfeiture set is a function of it (`palw_forfeited_execution_roots_v1`), not a second ledger
    /// that could disagree with it.
    pub execution_root: crate::Hash64,
}

/// **A panel seat signed Valid, and the same claim has an objective Invalid.**
///
/// One seat, one offence. Three colluding Valid signers are three evidence objects and three
/// `offence_id`s. A later "someone said Invalid" is not this payload — the contradiction must
/// itself be an independently verifiable PALW-S fact about this claim's work.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelFalseValidEvidenceV1 {
    pub version: u16,
    pub claim_id: Hash64,
    pub network_domain: Hash64,
    pub accused_seat: TransactionOutpoint,
    pub valid_receipt: crate::palw_panel_v2::PalwSeatReceiptV2,
    /// The claim executor's registered key. [`palw_verify_objective_offence_v1`] checks the
    /// contradiction under this key and cannot know whose it is; below `palw_audit_2026_09_23`
    /// NOTHING checked it against the claim row (a fresh key convicted any seat). Past that fence
    /// the processor and the fold (`bind_panel_false_valid`) require it to be the key the claim's
    /// executor bond registered, and an `ExecutorEquivocation` carriage to accuse that bond.
    pub executor_pubkey: Vec<u8>,
    pub contradiction: PalwPanelContradictionV1,
}

/// Objective Invalid of the work a Valid receipt attested.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwPanelContradictionV1 {
    /// Two signed execution attestations, one executor, two roots, bound to this claim's job id.
    ExecutorEquivocation(crate::palw_carriage::PalwEquivocationCarriageV1) = 0,
    /// The executor of this claim has already been consumed as `CourtExecutorGuilty` or
    /// `ExecutorEquivocation`. The Valid signers attested a job the chain has since convicted.
    CourtExecutorGuilty { offence_id: Hash64 } = 1,
    /// The claim was voided for producer withholding after this seat signed Valid.
    ProducerWithholding { voided_daa: u64 } = 2,
    /// A round permit this claim consumed was burned for equivocation.
    ConflictingPermit { span: u64, round: u64, permit_index: u16 } = 3,
    /// The claim was voided `CourtFraud` after this seat signed Valid. The court already
    /// adjudicated; this object attributes that Invalid to the Valid signer.
    CourtFraud { voided_daa: u64 } = 4,
    /// One-step arithmetic refutation of this claim's committed execution (the court Arithmetic close,
    /// without a live session — so it still binds after Final, until liability expiry).
    StepArithmetic {
        refutation: crate::palw_step_refute::PalwExecutionStepRefutationV1,
        operand_openings: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
    } = 5,
    /// Structural step-leg refutation (shape, leaf count, non-finite) of the committed program.
    StepStructural(crate::palw_step_leg::PalwStepRefutationV1) = 6,
    /// Structural / legs refutation of the committed activation and checkpoint program.
    Legs(crate::palw_legs::PalwLegsRefutationV1) = 7,
    /// The committed decode token is not what the pinned selection rule produces from that
    /// position's own logits — a forged output against the committed execution.
    ForgedOutput {
        binding: crate::palw_step_leg::PalwStepBindingV2,
        pin: crate::palw_step_refute::PalwBase0DecodeTokensV1,
        position: u32,
    } = 8,
}

/// `H(domain ‖ kind ‖ accused outpoint ‖ evidence_id)` — one offence, one id, across archival,
/// pruned, IBD, restart and post-reorg nodes.
pub fn palw_offence_id_v1(kind: PalwOffenceKindV1, accused: &TransactionOutpoint, evidence_id: &Hash64) -> Hash64 {
    let mut h = Params::new().hash_length(64).key(PALW_OFFENCE_DOMAIN_ID_V1).to_state();
    h.update(&[kind as u8]);
    h.update(accused.transaction_id.as_bytes().as_slice());
    h.update(&accused.index.to_le_bytes());
    h.update(evidence_id.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Keyed-BLAKE2b-512 domain of [`palw_offence_evidence_digest_v1`].
pub const PALW_OFFENCE_EVIDENCE_DOMAIN_V1: &[u8] = b"misaka-palw/objective-offence-evidence/v1";

/// Digest of the kind-specific evidence bytes. `ObjectiveOffence.evidence_id` must equal this,
/// so a renamed id cannot point a genuine certificate at a different offence.
pub fn palw_offence_evidence_digest_v1(evidence: &[u8]) -> Hash64 {
    let mut h = Params::new().hash_length(64).key(PALW_OFFENCE_EVIDENCE_DOMAIN_V1).to_state();
    h.update(&(evidence.len() as u64).to_le_bytes());
    h.update(evidence);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The evidence id the consumed-offence ledger keys on** (ADR-0144 §9: one offence, one penalty).
///
/// `ObjectiveOffence.evidence_id` binds the submitted bytes so a renamed id cannot point a
/// genuine certificate at a different accused. The ledger must not use that binding: swapping
/// `attestation_a`/`attestation_b` is a different serialization of the same two signed roots,
/// and two serializations of one equivocation are still one offence.
///
/// Empty evidence (fold unit tests that bypass the processor) falls back to the named id.
pub fn palw_ledger_evidence_id_v1(kind: PalwOffenceKindV1, evidence: &[u8], named: &Hash64) -> Result<Hash64, PalwOffenceVerifyError> {
    if evidence.is_empty() {
        if *named == Hash64::default() {
            return Err(PalwOffenceVerifyError::EvidenceEmpty);
        }
        return Ok(*named);
    }
    match kind {
        PalwOffenceKindV1::ExecutorEquivocation => {
            let carriage: crate::palw_carriage::PalwEquivocationCarriageV1 =
                borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::EquivocationUndecodable)?;
            palw_equivocation_pair_id_v1(&carriage)
        }
        PalwOffenceKindV1::PanelFalseValid => {
            let payload: PalwPanelFalseValidEvidenceV1 =
                borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            palw_panel_false_valid_ledger_evidence_id_from_payload_v1(&payload)
        }
        PalwOffenceKindV1::CourtExecutorGuilty => Ok(palw_offence_evidence_digest_v1(evidence)),
        // Keyed per (seat, claim) by `palw_false_valid_offence_id_v2`, never by the bytes: one
        // Valid is one offence whatever contradiction or wrapper a filer attaches to it.
        PalwOffenceKindV1::PanelFalseValidV2 => Err(PalwOffenceVerifyError::AttributionDormant),
    }
}

/// `palw_offence_id_v1` under [`palw_ledger_evidence_id_v1`]. Archival, pruned, IBD, restart
/// and post-reorg nodes all consume this id.
pub fn palw_ledger_offence_id_v1(
    kind: PalwOffenceKindV1,
    accused: &TransactionOutpoint,
    evidence: &[u8],
    named: &Hash64,
) -> Result<Hash64, PalwOffenceVerifyError> {
    Ok(palw_offence_id_v1(kind, accused, &palw_ledger_evidence_id_v1(kind, evidence, named)?))
}

/// `H(claim ‖ contradiction_digest)` — the ledger evidence id for one seat's false-Valid.
/// Combined with [`palw_offence_id_v1`] this is
/// `H(PanelFalseValid, accused_seat, claim, contradiction)`, so three colluding seats are
/// three offences and a renamed wrapper is still one.
pub fn palw_panel_false_valid_ledger_evidence_id_v1(claim: &Hash64, contradiction_digest: &Hash64) -> Hash64 {
    let mut h = Params::new().hash_length(64).key(PALW_PANEL_FALSE_VALID_LEDGER_DOMAIN_V1).to_state();
    h.update(claim.as_byte_slice());
    h.update(contradiction_digest.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn palw_panel_false_valid_ledger_evidence_id_from_payload_v1(
    payload: &PalwPanelFalseValidEvidenceV1,
) -> Result<Hash64, PalwOffenceVerifyError> {
    let digest = palw_panel_contradiction_digest_v1(&payload.contradiction)?;
    Ok(palw_panel_false_valid_ledger_evidence_id_v1(&payload.claim_id, &digest))
}

fn palw_panel_contradiction_digest_v1(contradiction: &PalwPanelContradictionV1) -> Result<Hash64, PalwOffenceVerifyError> {
    match contradiction {
        PalwPanelContradictionV1::ExecutorEquivocation(carriage) => palw_equivocation_pair_id_v1(carriage),
        PalwPanelContradictionV1::CourtExecutorGuilty { offence_id } => {
            if *offence_id == Hash64::default() {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
            Ok(*offence_id)
        }
        PalwPanelContradictionV1::ProducerWithholding { voided_daa } => {
            if *voided_daa == 0 {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
            Ok(palw_offence_id_v1(
                PalwOffenceKindV1::PanelFalseValid,
                &TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0u8; 64]), index: 0 },
                &Hash64::from_u64_word(*voided_daa),
            ))
        }
        PalwPanelContradictionV1::ConflictingPermit { span, round, permit_index } => {
            let mut h = Params::new().hash_length(64).key(PALW_PANEL_FALSE_VALID_LEDGER_DOMAIN_V1).to_state();
            h.update(b"permit");
            h.update(&span.to_le_bytes());
            h.update(&round.to_le_bytes());
            h.update(&permit_index.to_le_bytes());
            let mut out = [0u8; 64];
            out.copy_from_slice(h.finalize().as_bytes());
            Ok(Hash64::from_bytes(out))
        }
        PalwPanelContradictionV1::CourtFraud { voided_daa } => {
            if *voided_daa == 0 {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
            Ok(palw_tagged_contradiction_digest_v1(b"court-fraud", &voided_daa.to_le_bytes()))
        }
        PalwPanelContradictionV1::StepArithmetic { refutation, operand_openings } => {
            let body = borsh::to_vec(&(refutation, operand_openings))
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            Ok(palw_tagged_contradiction_digest_v1(b"step-arithmetic", &body))
        }
        PalwPanelContradictionV1::StepStructural(refutation) => {
            let body = borsh::to_vec(refutation).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            Ok(palw_tagged_contradiction_digest_v1(b"step-structural", &body))
        }
        PalwPanelContradictionV1::Legs(refutation) => {
            let body = borsh::to_vec(refutation).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            Ok(palw_tagged_contradiction_digest_v1(b"legs", &body))
        }
        PalwPanelContradictionV1::ForgedOutput { binding, pin, position } => {
            let body =
                borsh::to_vec(&(binding, pin, position)).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            Ok(palw_tagged_contradiction_digest_v1(b"forged-output", &body))
        }
    }
}

fn palw_tagged_contradiction_digest_v1(tag: &[u8], payload: &[u8]) -> Hash64 {
    let mut h = Params::new().hash_length(64).key(PALW_PANEL_FALSE_VALID_LEDGER_DOMAIN_V1).to_state();
    h.update(tag);
    h.update(&(payload.len() as u64).to_le_bytes());
    h.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Smallest integer `s` such that `s × colluding_quorum > max_fraud_gain`.
///
/// This is the protocol sizing rule for a slashable seat, not a shipped `Params` literal.
/// Arming `Params::palw_objective_offence` on a network whose panel floor is below this
/// number for an economically relevant class is the mainnet blocker ADR-0144 §9 names.
pub fn palw_min_slashable_per_colluding_seat_v1(max_fraud_gain: u128, colluding_quorum: u64) -> u128 {
    let q = colluding_quorum.max(1) as u128;
    max_fraud_gain / q + 1
}

/// `seat_slashable × colluding_quorum > max_fraud_gain`.
pub fn palw_colluding_quorum_covers_v1(seat_slashable: u128, colluding_quorum: u64, max_fraud_gain: u128) -> bool {
    seat_slashable.saturating_mul(colluding_quorum.max(1) as u128) > max_fraud_gain
}

fn palw_equivocation_pair_id_v1(
    carriage: &crate::palw_carriage::PalwEquivocationCarriageV1,
) -> Result<Hash64, PalwOffenceVerifyError> {
    let network_id = carriage.certificate.job_context.network_id.as_slice();
    let a = carriage.certificate.attestation_a.message(network_id);
    let b = carriage.certificate.attestation_b.message(network_id);
    if a.as_bytes() == b.as_bytes() {
        return Err(PalwOffenceVerifyError::EquivocationDoesNotContradict);
    }
    let (lo, hi) = if a.as_bytes() <= b.as_bytes() { (a, b) } else { (b, a) };
    let mut h = Params::new().hash_length(64).key(crate::palw_slash::PALW_S_DOMAIN_CONTRADICTION_EVIDENCE_ID).to_state();
    h.update(&lo.as_bytes());
    h.update(&hi.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// Why the processor refused an `ObjectiveOffence`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwOffenceVerifyError {
    #[error("an objective offence must carry evidence (ADR-0144 §9)")]
    EvidenceEmpty,
    #[error("evidence_id is not the digest of the evidence bytes")]
    EvidenceIdMismatch,
    #[error("executor equivocation evidence does not decode")]
    EquivocationUndecodable,
    #[error("the two attestations do not contradict")]
    EquivocationDoesNotContradict,
    #[error("the evidence accuses {got:?}, not the named PALW bond {named:?}")]
    AccusedMismatch { named: TransactionOutpoint, got: TransactionOutpoint },
    #[error("the PALW bond's key is not the equivocating signer")]
    BondNotTheSigner,
    #[error("the PALW bond is not on the registry")]
    BondNotActive,
    #[error("{0}")]
    Carriage(String),
    #[error("CourtExecutorGuilty is filed by CourtClosed, not as a standalone object")]
    CourtGuiltyIsNotStandalone,
    #[error("panel false-Valid needs a signed Valid receipt contradicted by an objective protocol fact")]
    PanelFalseValidNeedsContradiction,
    #[error("the Valid receipt does not name this accused seat")]
    PanelFalseValidSeatMismatch,
    #[error("the Valid receipt is not a Valid verdict")]
    PanelFalseValidNotValidVerdict,
    #[error("the contradiction is not about this claim's work")]
    PanelFalseValidWorkMismatch,
    #[error("the Valid receipt signature does not verify under the accused seat's key")]
    PanelFalseValidReceiptUnverified,
    // ---- ADR-0152 v2 F2 (`palw_offence_attribution_v1`) ----------------------------------------
    #[error("PanelFalseValidV2 is judged by palw_check_panel_false_valid_v2 past palw_offence_attribution, and nowhere else")]
    AttributionDormant,
    #[error("PanelFalseValid is superseded on this network by PanelFalseValidV2 (palw_offence_attribution)")]
    SupersededOnThisNetwork,
    #[error("this contradiction is not admitted against a Valid signer past palw_offence_attribution: {0}")]
    ContradictionNotAdmitted(&'static str),
    #[error("the false-Valid evidence names neither a live claim nor a liability row")]
    NoTarget,
    #[error("the accused seat's receipt does not attest the segment the fault is in")]
    SiteNotAttested,
    #[error("the claim's segment cut is no longer known, so a segmented receipt cannot be placed")]
    SegmentsUnknown,
    #[error("the claim has an open court or data-availability session; file again once it closes")]
    ClaimUnderSession,
    #[error("the reporter slot must stay empty until its own fence arms it")]
    ReporterSlotNotArmed,
    #[error("the false-Valid evidence is above the one carrier a kind-3 object rides")]
    EvidenceTooLarge,
    #[error("the accused seat's segmented receipt carries a partial mask the claim's panel did not assign it")]
    SegmentMaskNotAssigned,
}

/// **The processor's cryptographic gate** (ADR-0144 §9). A named kind and an evidence id are
/// not a proof. Quality, cache, speedup, delay and unavailability are not in
/// [`PalwOffenceKindV1`] and cannot pass.
pub fn palw_verify_objective_offence_v1<F>(
    kind: PalwOffenceKindV1,
    accused: &TransactionOutpoint,
    evidence_id: &Hash64,
    evidence: &[u8],
    palw_pubkey: &[u8],
    palw_bond_active: bool,
    network_id: &[u8],
    // **The RULESET's step ladder, ADR-0084 U-08** — `bundle.court.max_step_leaf_count()`, never the
    // executor's `PALW_STEP_MAX_LEAVES`. A `PanelFalseValid` whose contradiction is a step refutation
    // is adjudicated here, and an adjudicator walking a shallower ladder than the class was admitted
    // at refuses honest evidence: on a network with the held 2M row (step space ≈2^37.6) against the
    // default 2^22, EVERY structural refutation of that class is `PanelFalseValidNeedsContradiction`,
    // so a seat that voted Valid on a lie it could see cannot be convicted through this route at all.
    max_step_leaf_count: u64,
    verify_signature: F,
) -> Result<(), PalwOffenceVerifyError>
where
    F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if evidence.is_empty() {
        return Err(PalwOffenceVerifyError::EvidenceEmpty);
    }
    if palw_offence_evidence_digest_v1(evidence) != *evidence_id {
        return Err(PalwOffenceVerifyError::EvidenceIdMismatch);
    }
    match kind {
        PalwOffenceKindV1::ExecutorEquivocation => {
            verify_palw_executor_equivocation_v1(accused, evidence, palw_pubkey, palw_bond_active, network_id, verify_signature)
        }
        PalwOffenceKindV1::PanelFalseValid => {
            verify_palw_panel_false_valid_v1(accused, evidence, palw_pubkey, palw_bond_active, network_id, max_step_leaf_count, verify_signature)
        }
        PalwOffenceKindV1::CourtExecutorGuilty => Err(PalwOffenceVerifyError::CourtGuiltyIsNotStandalone),
        // Never through this gate: past `palw_offence_attribution` the processor sends it to
        // `palw_check_panel_false_valid_v2`, and below it the kind does not exist.
        PalwOffenceKindV1::PanelFalseValidV2 => Err(PalwOffenceVerifyError::AttributionDormant),
    }
}

fn verify_palw_panel_false_valid_v1<F>(
    accused: &TransactionOutpoint,
    evidence: &[u8],
    seat_pubkey: &[u8],
    palw_bond_active: bool,
    network_id: &[u8],
    max_step_leaf_count: u64,
    verify_signature: F,
) -> Result<(), PalwOffenceVerifyError>
where
    F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if !palw_bond_active {
        return Err(PalwOffenceVerifyError::BondNotActive);
    }
    let payload: PalwPanelFalseValidEvidenceV1 =
        borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
    if payload.version != PALW_PANEL_FALSE_VALID_VERSION_V1 {
        return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
    }
    if payload.accused_seat != *accused || payload.valid_receipt.seat_bond.0 != *accused {
        return Err(PalwOffenceVerifyError::PanelFalseValidSeatMismatch);
    }
    if payload.valid_receipt.claim != payload.claim_id {
        return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
    }
    match payload.valid_receipt.verdict {
        crate::palw_panel_v2::PalwReceiptVerdictV2::Valid => {}
        crate::palw_panel_v2::PalwReceiptVerdictV2::Unavailable { .. } | crate::palw_panel_v2::PalwReceiptVerdictV2::Incapable => {
            return Err(PalwOffenceVerifyError::PanelFalseValidNotValidVerdict);
        }
    }
    let message = crate::palw_panel_v2::palw_receipt_message_v2(
        payload.network_domain,
        payload.valid_receipt.claim,
        payload.valid_receipt.verdict,
        payload.valid_receipt.signed_daa,
    );
    if !verify_signature(
        seat_pubkey,
        message.as_byte_slice(),
        &payload.valid_receipt.signature,
        crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT,
    ) {
        return Err(PalwOffenceVerifyError::PanelFalseValidReceiptUnverified);
    }
    match &payload.contradiction {
        PalwPanelContradictionV1::ExecutorEquivocation(carriage) => {
            if carriage.certificate.job_context.job_id != payload.claim_id {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            verify_palw_executor_equivocation_v1(
                &carriage.accused_bond_outpoint,
                &borsh::to_vec(carriage).map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?,
                &payload.executor_pubkey,
                true,
                network_id,
                verify_signature,
            )
            .map_err(|e| match e {
                PalwOffenceVerifyError::EquivocationDoesNotContradict
                | PalwOffenceVerifyError::EquivocationUndecodable
                | PalwOffenceVerifyError::BondNotTheSigner
                | PalwOffenceVerifyError::Carriage(_) => PalwOffenceVerifyError::PanelFalseValidNeedsContradiction,
                other => other,
            })?;
        }
        PalwPanelContradictionV1::CourtExecutorGuilty { offence_id } => {
            if *offence_id == Hash64::default() {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
        }
        PalwPanelContradictionV1::ProducerWithholding { voided_daa } => {
            if *voided_daa == 0 {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
        }
        PalwPanelContradictionV1::ConflictingPermit { .. } => {}
        PalwPanelContradictionV1::CourtFraud { voided_daa } => {
            if *voided_daa == 0 {
                return Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
            }
        }
        PalwPanelContradictionV1::StepArithmetic { refutation, .. } => {
            crate::palw_step_leg::verify_binding_v1(&refutation.binding)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            if refutation.binding.job_context.job_id != payload.claim_id {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
        }
        PalwPanelContradictionV1::StepStructural(refutation) => {
            // ADR-0084 U-08: the RULESET's ladder, so a class admitted deep can be prosecuted deep.
            crate::palw_step_leg::check_step_refutation_capped_v1(refutation, max_step_leaf_count)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            if refutation.binding.job_context.job_id != payload.claim_id {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
        }
        PalwPanelContradictionV1::Legs(refutation) => {
            crate::palw_legs::check_legs_refutation_v1(refutation)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            if refutation.binding.job_context.job_id != payload.claim_id {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
        }
        PalwPanelContradictionV1::ForgedOutput { binding, pin, position } => {
            crate::palw_step_refute::check_base0_decode_token_refutation_v1(binding, pin, *position)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            if binding.job_context.job_id != payload.claim_id {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
        }
    }
    Ok(())
}

/// Bind a proof-carrying contradiction to this claim's committed execution. State-bound kinds
/// (court-guilty, DA void, permit burn, court-fraud) are checked by the processor against chain
/// state; this function is the independent arithmetic/structural gate for Step / legs / forged
/// output, so a Valid signer can still be named after Final.
pub fn palw_panel_contradiction_convicts_execution_v1(
    contradiction: &PalwPanelContradictionV1,
    claim_execution_root: Hash64,
    class_artifact_root: Hash64,
    step_ladder: u64,
) -> Result<(), PalwOffenceVerifyError> {
    match contradiction {
        PalwPanelContradictionV1::ExecutorEquivocation(_)
        | PalwPanelContradictionV1::CourtExecutorGuilty { .. }
        | PalwPanelContradictionV1::ProducerWithholding { .. }
        | PalwPanelContradictionV1::ConflictingPermit { .. }
        | PalwPanelContradictionV1::CourtFraud { .. } => Ok(()),
        PalwPanelContradictionV1::StepArithmetic { refutation, operand_openings } => {
            if refutation.binding.committed_execution_root != claim_execution_root {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(operand_openings, class_artifact_root)
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)?;
            match crate::palw_step_refute::check_execution_step_refutation_capped_v1(refutation, &operands, step_ladder) {
                Ok(_) => Ok(()),
                Err(_) => Err(PalwOffenceVerifyError::PanelFalseValidNeedsContradiction),
            }
        }
        PalwPanelContradictionV1::StepStructural(refutation) => {
            if refutation.binding.committed_execution_root != claim_execution_root {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            crate::palw_step_leg::check_step_refutation_capped_v1(refutation, step_ladder)
                .map(|_| ())
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)
        }
        PalwPanelContradictionV1::Legs(refutation) => {
            if refutation.binding.committed_execution_root != claim_execution_root {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            crate::palw_legs::check_legs_refutation_v1(refutation)
                .map(|_| ())
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)
        }
        PalwPanelContradictionV1::ForgedOutput { binding, pin, position } => {
            if binding.committed_execution_root != claim_execution_root {
                return Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch);
            }
            crate::palw_step_refute::check_base0_decode_token_refutation_v1(binding, pin, *position)
                .map(|_| ())
                .map_err(|_| PalwOffenceVerifyError::PanelFalseValidNeedsContradiction)
        }
    }
}

fn verify_palw_executor_equivocation_v1<F>(
    accused: &TransactionOutpoint,
    evidence: &[u8],
    palw_pubkey: &[u8],
    palw_bond_active: bool,
    network_id: &[u8],
    verify_signature: F,
) -> Result<(), PalwOffenceVerifyError>
where
    F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    if !palw_bond_active {
        return Err(PalwOffenceVerifyError::BondNotActive);
    }
    let carriage: crate::palw_carriage::PalwEquivocationCarriageV1 =
        borsh::from_slice(evidence).map_err(|_| PalwOffenceVerifyError::EquivocationUndecodable)?;
    if carriage.accused_bond_outpoint != *accused {
        return Err(PalwOffenceVerifyError::AccusedMismatch { named: *accused, got: carriage.accused_bond_outpoint });
    }
    let signer = crate::mldsa87_primitives::mldsa87_key_id(palw_pubkey);
    let record = crate::dns_finality::StakeBondRecord {
        version: 1,
        bond_outpoint: *accused,
        owner_pubkey_hash: signer,
        validator_pubkey_hash: signer,
        validator_pubkey: palw_pubkey.to_vec(),
        amount: 1,
        activation_daa_score: 0,
        created_daa_score: 0,
        unbonding_period_blocks: 1,
        owner_reward_spk_payload: [0u8; 64],
        unbond_request_daa_score: None,
        slashed_at_daa_score: None,
        status: crate::dns_finality::BondStatus::Active,
    };
    crate::palw_carriage::adjudicate_equivocation_carriage_v1(&carriage, &record, 0, network_id, |key, digest, sig, ctx| {
        verify_signature(key, &digest.as_bytes(), sig, ctx)
    })
    .map(|_| ())
    .map_err(|e| match e {
        crate::palw_carriage::PalwCarriageError::EquivocationBondNotTheSigner => PalwOffenceVerifyError::BondNotTheSigner,
        other => PalwOffenceVerifyError::Carriage(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionId;

    fn op(n: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 }
    }

    #[test]
    fn offence_id_is_the_evidence_not_the_submitter() {
        let a = palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &op(1), &Hash64::from_u64_word(0xE1));
        assert_eq!(a, palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &op(1), &Hash64::from_u64_word(0xE1)));
        assert_ne!(a, palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &op(2), &Hash64::from_u64_word(0xE1)));
        assert_ne!(a, palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &op(1), &Hash64::from_u64_word(0xE2)));
        assert_ne!(a, palw_offence_id_v1(PalwOffenceKindV1::PanelFalseValid, &op(1), &Hash64::from_u64_word(0xE1)));
    }

    #[test]
    fn empty_evidence_is_not_a_proof() {
        let err = palw_verify_objective_offence_v1(
            PalwOffenceKindV1::ExecutorEquivocation,
            &op(1),
            &Hash64::from_u64_word(1),
            &[],
            &[7; 4],
            true,
            b"testnet-11",
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .unwrap_err();
        assert_eq!(err, PalwOffenceVerifyError::EvidenceEmpty);
    }

    #[test]
    fn court_guilty_is_not_a_standalone_object() {
        let evidence = b"session";
        let id = palw_offence_evidence_digest_v1(evidence);
        let err = palw_verify_objective_offence_v1(
            PalwOffenceKindV1::CourtExecutorGuilty,
            &op(1),
            &id,
            evidence,
            &[7; 4],
            true,
            b"testnet-11",
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .unwrap_err();
        assert_eq!(err, PalwOffenceVerifyError::CourtGuiltyIsNotStandalone);
    }

    #[test]
    fn a_renamed_evidence_id_does_not_pass() {
        let err = palw_verify_objective_offence_v1(
            PalwOffenceKindV1::ExecutorEquivocation,
            &op(1),
            &Hash64::from_u64_word(0xDEAD),
            b"carriage",
            &[7; 4],
            true,
            b"testnet-11",
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .unwrap_err();
        assert_eq!(err, PalwOffenceVerifyError::EvidenceIdMismatch);
    }

    #[test]
    fn panel_false_valid_is_fail_closed_until_the_contradiction_exists() {
        for seat in 1u64..=5 {
            let evidence = format!("seat {seat} signed Valid; that is not a contradiction");
            let evidence = evidence.as_bytes();
            let id = palw_offence_evidence_digest_v1(evidence);
            let err = palw_verify_objective_offence_v1(
                PalwOffenceKindV1::PanelFalseValid,
                &op(seat),
                &id,
                evidence,
                &[7; 4],
                true,
                b"testnet-11",
                    crate::palw_step::PALW_STEP_MAX_LEAVES,
                    |_, _, _, _| true,
            )
            .unwrap_err();
            assert_eq!(err, PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);
        }
    }

    #[test]
    fn a_proven_panel_false_valid_is_one_offence_per_seat() {
        let claim = Hash64::from_u64_word(0xC1);
        let executor_pk = [9u8; 4];
        let network = b"testnet-11";
        let domain = Hash64::from_u64_word(0xD1);
        let mut ids = Vec::new();
        for seat in 1u64..=3 {
            let payload = mock_panel_false_valid(
                op(seat),
                claim,
                &executor_pk,
                network,
                domain,
                crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            );
            let evidence = borsh::to_vec(&payload).expect("serializes");
            let named = palw_offence_evidence_digest_v1(&evidence);
            palw_verify_objective_offence_v1(
                PalwOffenceKindV1::PanelFalseValid,
                &op(seat),
                &named,
                &evidence,
                &[7; 4],
                true,
                network,
                crate::palw_step::PALW_STEP_MAX_LEAVES,
                |_, _, _, _| true,
            )
            .expect("Valid receipt + executor equivocation on the same claim");
            let id = palw_ledger_offence_id_v1(PalwOffenceKindV1::PanelFalseValid, &op(seat), &evidence, &named).expect("ledger");
            ids.push(id);
        }
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);

        let again =
            mock_panel_false_valid(op(1), claim, &executor_pk, network, domain, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid);
        let mut renamed = again.clone();
        renamed.valid_receipt.signed_daa = 99;
        let a = borsh::to_vec(&again).unwrap();
        let b = borsh::to_vec(&renamed).unwrap();
        assert_ne!(palw_offence_evidence_digest_v1(&a), palw_offence_evidence_digest_v1(&b));
        assert_eq!(
            palw_ledger_offence_id_v1(PalwOffenceKindV1::PanelFalseValid, &op(1), &a, &palw_offence_evidence_digest_v1(&a)).unwrap(),
            palw_ledger_offence_id_v1(PalwOffenceKindV1::PanelFalseValid, &op(1), &b, &palw_offence_evidence_digest_v1(&b)).unwrap(),
            "same seat, same claim, same contradiction is one offence even if the wrapper is renamed"
        );
    }

    #[test]
    fn honest_unavailable_and_incapable_are_zero_penalty() {
        let claim = Hash64::from_u64_word(0xC1);
        for verdict in [
            crate::palw_panel_v2::PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 1 },
            crate::palw_panel_v2::PalwReceiptVerdictV2::Incapable,
        ] {
            let payload = mock_panel_false_valid(op(1), claim, &[9u8; 4], b"testnet-11", Hash64::from_u64_word(0xD1), verdict);
            let evidence = borsh::to_vec(&payload).unwrap();
            let err = palw_verify_objective_offence_v1(
                PalwOffenceKindV1::PanelFalseValid,
                &op(1),
                &palw_offence_evidence_digest_v1(&evidence),
                &evidence,
                &[7; 4],
                true,
                b"testnet-11",
                    crate::palw_step::PALW_STEP_MAX_LEAVES,
                    |_, _, _, _| true,
            )
            .unwrap_err();
            assert_eq!(err, PalwOffenceVerifyError::PanelFalseValidNotValidVerdict);
        }
    }

    #[test]
    fn court_fraud_and_independent_proofs_are_fail_closed_until_they_convict() {
        let claim = Hash64::from_u64_word(0xC1);
        let mut payload = mock_panel_false_valid(
            op(1),
            claim,
            &[9u8; 4],
            b"testnet-11",
            Hash64::from_u64_word(0xD1),
            crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
        );
        payload.contradiction = PalwPanelContradictionV1::CourtFraud { voided_daa: 0 };
        let evidence = borsh::to_vec(&payload).unwrap();
        let err = palw_verify_objective_offence_v1(
            PalwOffenceKindV1::PanelFalseValid,
            &op(1),
            &palw_offence_evidence_digest_v1(&evidence),
            &evidence,
            &[7; 4],
            true,
            b"testnet-11",
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .unwrap_err();
        assert_eq!(err, PalwOffenceVerifyError::PanelFalseValidNeedsContradiction);

        payload.contradiction = PalwPanelContradictionV1::CourtFraud { voided_daa: 124 };
        let evidence = borsh::to_vec(&payload).unwrap();
        palw_verify_objective_offence_v1(
            PalwOffenceKindV1::PanelFalseValid,
            &op(1),
            &palw_offence_evidence_digest_v1(&evidence),
            &evidence,
            &[7; 4],
            true,
            b"testnet-11",
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .expect("a named CourtFraud void is well-formed; the processor binds it to the claim phase");
    }

    #[test]
    fn the_colluding_quorum_must_cover_max_fraud_gain() {
        const DENSE: u128 = 33_152_720;
        let seat = palw_min_slashable_per_colluding_seat_v1(DENSE, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert_eq!(seat, 11_050_907);
        assert!(palw_colluding_quorum_covers_v1(seat, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE));
        assert!(!palw_colluding_quorum_covers_v1(400_000, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE));
        assert!(!palw_colluding_quorum_covers_v1(seat - 1, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE));
    }

    fn mock_equivocation_carriage(
        accused: TransactionOutpoint,
        pubkey: &[u8],
        network_id: &[u8],
        job_id: Hash64,
    ) -> crate::palw_carriage::PalwEquivocationCarriageV1 {
        use crate::palw_slash::{PALW_S_OBJECT_VERSION_V3, PalwClassContradictionCertificateV1, PalwExecutionAttestationV1};
        use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
        let signer = crate::mldsa87_primitives::mldsa87_key_id(pubkey);
        let ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: network_id.to_vec(),
            job_id,
            job_nullifier: Hash64::from_u64_word(0x12),
            assignment_id: Hash64::from_u64_word(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: Hash64::from_u64_word(0x31),
            runtime_manifest_hash: Hash64::from_u64_word(0x32),
            runtime_class_id: Hash64::from_u64_word(0x33),
            shape_profile_id: Hash64::from_u64_word(0x34),
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: Hash64::from_u64_word(0x36),
            tokenizer_id: Hash64::from_u64_word(0x37),
            prompt_token_ids_hash: Hash64::from_u64_word(0x38),
            declared_prefill_tokens: 7,
            exact_decode_tokens: 3,
            max_context_tokens: 64,
        };
        let att = |root: Hash64| PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id: signer,
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: accused,
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        crate::palw_carriage::PalwEquivocationCarriageV1 {
            version: crate::palw_carriage::PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: accused,
            certificate: PalwClassContradictionCertificateV1 {
                version: PALW_S_OBJECT_VERSION_V3,
                attestation_a: att(Hash64::from_u64_word(0x01)),
                attestation_b: att(Hash64::from_u64_word(0x02)),
                job_context: ctx,
            },
        }
    }

    fn mock_panel_false_valid(
        seat: TransactionOutpoint,
        claim: Hash64,
        executor_pubkey: &[u8],
        network: &[u8],
        network_domain: Hash64,
        verdict: crate::palw_panel_v2::PalwReceiptVerdictV2,
    ) -> PalwPanelFalseValidEvidenceV1 {
        let executor = op(99);
        let carriage = mock_equivocation_carriage(executor, executor_pubkey, network, claim);
        PalwPanelFalseValidEvidenceV1 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V1,
            claim_id: claim,
            network_domain,
            accused_seat: seat,
            valid_receipt: crate::palw_panel_v2::PalwSeatReceiptV2 {
                claim,
                verdict,
                seat_bond: crate::palw_state_v2::PalwBondKeyV2(seat),
                signed_daa: 7,
                signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
            executor_pubkey: executor_pubkey.to_vec(),
            contradiction: PalwPanelContradictionV1::ExecutorEquivocation(carriage),
        }
    }

    #[test]
    fn a_proven_equivocation_carriage_passes_the_processor_gate() {
        let accused = op(1);
        let pubkey = [7u8; 4];
        let network = b"testnet-11";
        let evidence = borsh::to_vec(&mock_equivocation_carriage(accused, &pubkey, network, Hash64::from_u64_word(0x11)))
            .expect("carriage serializes");
        let id = palw_offence_evidence_digest_v1(&evidence);
        palw_verify_objective_offence_v1(
            PalwOffenceKindV1::ExecutorEquivocation,
            &accused,
            &id,
            &evidence,
            &pubkey,
            true,
            network,
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        )
        .expect("one signer, two roots, matching accused PALW key");
    }

    #[test]
    fn swapping_attestation_order_is_the_same_offence() {
        let accused = op(1);
        let pubkey = [7u8; 4];
        let network = b"testnet-11";
        let mut carriage = mock_equivocation_carriage(accused, &pubkey, network, Hash64::from_u64_word(0x11));
        let a = borsh::to_vec(&carriage).expect("serializes");
        std::mem::swap(&mut carriage.certificate.attestation_a, &mut carriage.certificate.attestation_b);
        let b = borsh::to_vec(&carriage).expect("swapped serializes");
        assert_ne!(a, b, "the submitter can choose attestation order");
        assert_ne!(palw_offence_evidence_digest_v1(&a), palw_offence_evidence_digest_v1(&b));
        let id_a =
            palw_ledger_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &accused, &a, &palw_offence_evidence_digest_v1(&a))
                .expect("canonical a");
        let id_b =
            palw_ledger_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &accused, &b, &palw_offence_evidence_digest_v1(&b))
                .expect("canonical b");
        assert_eq!(id_a, id_b, "one pair of signed roots is one offence, whatever the byte order");
    }

    #[test]
    fn a_renamed_id_and_a_garbage_blob_are_zero_penalty_at_the_gate() {
        let accused = op(1);
        let pubkey = [7u8; 4];
        let network = b"testnet-11";
        let evidence =
            borsh::to_vec(&mock_equivocation_carriage(accused, &pubkey, network, Hash64::from_u64_word(0x11))).expect("serializes");
        assert_eq!(
            palw_verify_objective_offence_v1(
                PalwOffenceKindV1::ExecutorEquivocation,
                &accused,
                &Hash64::from_u64_word(0xBEEF),
                &evidence,
                &pubkey,
                true,
                network,
                crate::palw_step::PALW_STEP_MAX_LEAVES,
                |_, _, _, _| true,
            )
            .unwrap_err(),
            PalwOffenceVerifyError::EvidenceIdMismatch
        );
        let garbage = b"not-a-carriage";
        assert_eq!(
            palw_verify_objective_offence_v1(
                PalwOffenceKindV1::ExecutorEquivocation,
                &accused,
                &palw_offence_evidence_digest_v1(garbage),
                garbage,
                &pubkey,
                true,
                network,
                crate::palw_step::PALW_STEP_MAX_LEAVES,
                |_, _, _, _| true,
            )
            .unwrap_err(),
            PalwOffenceVerifyError::EquivocationUndecodable
        );
    }

    #[test]
    fn quality_usefulness_cache_and_speedup_are_not_offence_kinds() {
        assert_eq!(PalwOffenceKindV1::ExecutorEquivocation as u8, 0);
        assert_eq!(PalwOffenceKindV1::PanelFalseValid as u8, 1);
        assert_eq!(PalwOffenceKindV1::CourtExecutorGuilty as u8, 2);
        // ADR-0152 v2 F2, appended: the same false Valid, judged against the claim's root.
        assert_eq!(PalwOffenceKindV1::PanelFalseValidV2 as u8, 3);
        let kinds = [
            PalwOffenceKindV1::ExecutorEquivocation,
            PalwOffenceKindV1::PanelFalseValid,
            PalwOffenceKindV1::CourtExecutorGuilty,
            PalwOffenceKindV1::PanelFalseValidV2,
        ];
        assert_eq!(kinds.len(), 4, "ADR-0144 §9: no quality, cache, speedup, delay or unavailability kind");
        for kind in kinds {
            assert_eq!(borsh::to_vec(&kind).expect("a kind serializes"), vec![kind as u8], "{kind:?}: the tag is the discriminant");
        }
    }
}
