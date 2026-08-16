//! PALW chain carriage v1 — the ADR-0029 envelope and bodies, at Land stage.
//!
//! Normative source: ADR-0029 §1–§4. This module is the **format half** of carriage: the
//! Stage-0 magic envelope, the five payload bodies, their caps, and the stateless validation
//! that will become the Stage-1 admission validators verbatim. It is consensus-inert — no
//! transaction validation, no mempool rule, no store reads any of it; the Stage-0 consumer is
//! an external watcher over the RPC acceptance stream.
//!
//! # The two-stage contract this module encodes
//!
//! Stage 0 rides the native subnetwork as `"MPALW2" ‖ kind u8 ‖ borsh body` (native payloads
//! are admission-legal on this fork and standardness does not inspect them; mass is the only
//! bound). Stage 1 moves the SAME bodies onto dedicated subnetwork ids under the existing
//! DNS-overlay discipline. Two rules keep that migration a change of address, not of format:
//!
//! * **Bodies never embed their carriage.** No subnetwork id, no magic, nothing
//!   transport-shaped inside any body or any signed digest — a Stage-0 object and its Stage-1
//!   twin are byte-identical and verify identically.
//! * **The version trap (ADR-0029 premise):** these bodies' stateless validators ship together
//!   with their future subnetwork ids. Never retrofit a new version into a deployed validator —
//!   a deployed node that rejects the new version keeps the tx out of blocks entirely.
//!
//! # What stateless means here
//!
//! Exactly what it means one module over in `dns_finality`: everything decidable from the
//! payload bytes alone. Signature *lengths*, structural caps, internal coherence (a composite
//! commitment's binding must recompute the root it claims, and must describe the envelope it
//! rides with) — but no bond lookups, no registry membership, no signature verification, no
//! class parameters. A stateless-valid object can still be a lie; refutation and the credit
//! walk are where lies go to die.
//!
//! # The cap that is a carriage fact, not a wire fact
//!
//! [`PALW_CARRIAGE_MAX_OPENINGS_PER_CALL`] = 16 while the wire cap
//! (`PALW_LEGS_MAX_REQUESTED_OPENINGS`) stays 64: a 16-opening answer is ≈ 152 KB — 3.2× under
//! the 480 000 standard transaction mass — while the wire-cap 64 (~600 KB) exceeds it
//! outright (ADR-0029 §3's audit). A request wanting more splits into several calls, each with
//! its own `W_answer`.
//!
//! # Deliberately absent
//!
//! The refutation enum carries **no logits-event variant**: a bare-v2 logits-event refutation
//! is ≈ 0.99 MB against a 0.5 MB block mass (ADR-0029 §6) and does not fit any transaction.
//! Adding it is the chunked-evidence increment, a new variant with its own reassembly rules —
//! never a field bolted onto this one. Until then, computational divergence localizes through
//! the legs (8 KB activation rows), which is one of the two reasons composite is the
//! recommended registered form.

use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
use crate::palw_legs::{
    PALW_LEGS_OBJECT_VERSION_V1, PalwLegsBindingV1, PalwLegsOpeningAnswerV1, PalwLegsOpeningCallV1, PalwLegsRefutationV1,
    activation_leg_root_v1, canonical_decode_calls, checkpoint_leg_root_v1, execution_commitment_root_v1,
};
use crate::palw_slash::{PalwExecutionAttestationV1, PalwTraceSummaryRefutationV1, check_job_context_shape};
use crate::palw_v2::{PalwJobContextV2, PalwJobEnvelopeV2};
use crate::subnets::{
    SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_ATTESTATION, SUBNETWORK_ID_PALW_COMMITMENT, SUBNETWORK_ID_PALW_EVIDENCE_CHUNK,
    SUBNETWORK_ID_PALW_OPENING_ANSWER, SUBNETWORK_ID_PALW_OPENING_CALL, SUBNETWORK_ID_PALW_REFUTATION, SubnetworkId,
};
use crate::tx::{Transaction, TransactionId, TransactionOutpoint, TransactionOutput};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use kaspa_utils::mem_size::MemSizeEstimator;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Envelope constants
// ---------------------------------------------------------------------------------------------

/// The Stage-0 payload magic. Six bytes so a native payload that merely starts with valid Borsh
/// cannot collide; a payload without it is *foreign*, never an error.
pub const PALW_CARRIAGE_MAGIC: &[u8; 6] = b"MPALW2";

/// Wire version of every carriage body in this module.
pub const PALW_CARRIAGE_VERSION_V1: u16 = 1;

/// Kind bytes (ADR-0029 §2). Frozen: a renumbering is a new magic, not an edit.
pub const PALW_CARRIAGE_KIND_COMMITMENT: u8 = 0x01;
pub const PALW_CARRIAGE_KIND_ATTESTATION: u8 = 0x02;
pub const PALW_CARRIAGE_KIND_OPENING_CALL: u8 = 0x03;
pub const PALW_CARRIAGE_KIND_OPENING_ANSWER: u8 = 0x04;
pub const PALW_CARRIAGE_KIND_REFUTATION: u8 = 0x05;
/// ADR-0029 §6's chunked evidence: the reassembly envelope for a refutation too big for one
/// standard transaction (the bare-v2 logits-event row, ≈ 0.99 MiB → 3 chunks). Added before
/// any Stage-1 validator deployed — the version-trap rule is about DEPLOYED validators.
pub const PALW_CARRIAGE_KIND_EVIDENCE_CHUNK: u8 = 0x06;

/// Most openings one carried call may request across both legs (and one carried answer may
/// hold). A CARRIAGE cap, deliberately below the wire cap — see the module doc.
pub const PALW_CARRIAGE_MAX_OPENINGS_PER_CALL: usize = 16;

/// Largest single evidence chunk (bytes). ≈331 KB payloads ride a 480 KB standard tx with
/// 3.2× aggregate headroom across a 3-chunk group (ADR-0029 §3/§6 mass arithmetic).
pub const PALW_CARRIAGE_MAX_CHUNK_BYTES: usize = 340_000;
/// Most chunks one evidence group may declare (3 needed for the 0.99 MiB case; one spare).
pub const PALW_CARRIAGE_MAX_CHUNKS: u8 = 4;
/// Most concurrently-assembling evidence groups a reassembler retains before refusing new
/// ones — a userspace watcher bound, not consensus state.
pub const PALW_CARRIAGE_MAX_ASSEMBLING_GROUPS: usize = 64;

/// Domain of [`palw_carriage_envelope_hash_v1`].
pub const PALW_CARRIAGE_DOMAIN_ENVELOPE_HASH: &[u8] = b"misaka-palw/carriage-envelope-hash/v1";
/// Domain of [`palw_commitment_carriage_message_v1`].
pub const PALW_CARRIAGE_DOMAIN_COMMITMENT_MESSAGE: &[u8] = b"misaka-palw/carriage-commitment-message/v1";
/// ML-DSA-87 signing context for a commitment carriage signature.
pub const PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT: &[u8] = b"misaka-palw/carriage-commitment/mldsa87/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family and
/// the VLT sortition key — one string shared across families is a preimage bridge).
/// The reassembly key and integrity check of a chunked evidence group: keyed BLAKE2b-512
/// over the COMPLETE reassembled Stage-0 payload (magic, kind, body — the exact bytes that
/// would have been one oversized carriage).
pub const PALW_CARRIAGE_DOMAIN_EVIDENCE_GROUP: &[u8] = b"misaka-palw/evidence-chunk-group/v1";

pub const PALW_CARRIAGE_ALL_DOMAINS: &[&[u8]] = &[
    PALW_CARRIAGE_DOMAIN_ENVELOPE_HASH,
    PALW_CARRIAGE_DOMAIN_COMMITMENT_MESSAGE,
    PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT,
    PALW_CARRIAGE_DOMAIN_EVIDENCE_GROUP,
];

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCarriageError {
    #[error("payload claims the PALW magic but its kind byte {0:#04x} is unknown")]
    UnknownKind(u8),
    #[error("payload claims the PALW magic but is too short to carry a kind")]
    TruncatedEnvelope,
    #[error("payload body does not decode as its declared kind: {0}")]
    BodyDecode(String),
    #[error("unsupported carriage version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("inner object is malformed: {0}")]
    Inner(String),
    #[error("signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("committed_form {0} is not a known form (0 = bare v2, 1 = execution composite)")]
    UnknownCommittedForm(u8),
    #[error("committed_form and binding presence disagree: {0}")]
    FormBindingMismatch(&'static str),
    #[error("the carried binding does not describe the carried envelope (field drift on {0})")]
    BindingEnvelopeMismatch(&'static str),
    #[error("the carried binding does not recompute the committed root it claims")]
    BindingRootMismatch,
    #[error("committed_root does not equal the composite binding's committed execution root")]
    CommittedRootMismatch,
    #[error("carriage attester_id does not equal the attestation's executor_id")]
    AttesterMismatch,
    #[error("carried call requests {got} openings; the carriage cap is {max} (wire cap unchanged — split the call)")]
    TooManyOpenings { got: usize, max: usize },
    #[error("carried object requests or holds zero openings")]
    EmptyOpenings,
    #[error("the payload ({bytes} bytes) fits one transaction — send the object directly, not chunks")]
    ChunkingUnnecessary { bytes: usize },
    #[error("evidence payload of {bytes} bytes exceeds the {max}-byte group budget")]
    EvidenceTooLarge { bytes: usize, max: usize },
    #[error("evidence chunk group is incoherent: {0}")]
    ChunkGroupIncoherent(&'static str),
    #[error("reassembler is tracking the maximum {max} groups")]
    TooManyAssemblingGroups { max: usize },
    #[error("a PALW evidence carrier must declare no outputs (the reporter-reward slot is (tx_id, 0)); found {0}")]
    EvidenceCarrierHasOutputs(usize),
}

// ---------------------------------------------------------------------------------------------
// The five bodies (ADR-0029 §2)
// ---------------------------------------------------------------------------------------------

/// A miner's on-chain job commitment: the full input (replays are self-contained, so the
/// input-availability objection of ADR-0028 §3 is unreachable for carried jobs), the committed
/// root in its registered form, and the bonded identity that stands behind it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCommitmentCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    pub envelope: PalwJobEnvelopeV2,
    /// 0 = bare v2 logits root; 1 = execution-commitment composite. Which form a class
    /// produces is a registration fact; carriage only enforces internal coherence.
    pub committed_form: u8,
    pub committed_root: Hash64,
    /// Required iff composite: the transparent preimage refuters open against, carried once so
    /// a refutation never depends on the miner serving it later.
    pub binding: Option<PalwLegsBindingV1>,
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    /// ML-DSA-87 over [`palw_commitment_carriage_message_v1`] under
    /// [`PALW_CARRIAGE_MLDSA87_COMMITMENT_CONTEXT`]. Verified statefully (the bond registry
    /// resolves the key); carriage checks the length.
    pub signature: Vec<u8>,
}

/// An assigned re-executor's bonded claim, carried. The inner attestation is
/// `palw_slash`'s object unchanged — its message stays the signed digest; carriage adds the
/// linkage the credit gate and the dedup key need.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttestationCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    /// The commitment this attestation is filed against (the §1 credit gate's key). Whether
    /// the attested logits root matches that commitment is resolved against the commitment
    /// record — statefully, since the composite binding holds the logits root.
    pub commitment_root: Hash64,
    pub attestation: PalwExecutionAttestationV1,
    /// Must equal `attestation.executor_id`; duplicated at carriage level ONLY as the explicit
    /// dedup-key component, and the equality is enforced so it cannot drift.
    pub attester_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
}

/// An opening challenge, carried. `PalwLegsOpeningCallV1` is already the complete message
/// (envelope + request) by the frame-contract argument; carriage adds nothing but the cap.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOpeningCallCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    pub call: PalwLegsOpeningCallV1,
}

/// An opening answer, carried, bound to the on-chain fact that started its clock.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOpeningAnswerCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    /// The carrying transaction of the call being answered — the acceptance of THAT id is what
    /// `W_answer` runs from, so the answer names it rather than re-deriving it.
    pub call_tx_id: TransactionId,
    pub answer: PalwLegsOpeningAnswerV1,
}

/// The refutations that fit a transaction (ADR-0029 §3's mass table). Discriminants frozen.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwCarriedEvidenceV1 {
    /// Structural/legs refutation — worst case one activation row ≈ 15 KB.
    Legs(PalwLegsRefutationV1) = 0,
    /// Trace-summary refutation — the committed root's transparent preimage breaks the
    /// pinned schedule; a few hundred bytes.
    Summary(PalwTraceSummaryRefutationV1) = 1,
    // A logits-event variant is DELIBERATELY ABSENT: ≈ 0.99 MB against a 0.5 MB block mass
    // (ADR-0029 §6). Adding it is the chunked-evidence increment — a new variant with
    // reassembly rules, never a field bolted onto these.
}

/// A refutation, carried. Pure evidence carrier: the Stage-1 stateless validator additionally
/// requires the carrying transaction to declare **no outputs** (the slashing-evidence rule, so
/// the reporter-reward slot `(tx_id, 0)` is never a retrofit); that check needs the
/// transaction, not the body, and lives with the extractor's Stage-1 twin.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRefutationCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    pub evidence: PalwCarriedEvidenceV1,
}

/// One decoded carriage object of any kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwCarriageV1 {
    Commitment(PalwCommitmentCarriageV1),
    Attestation(PalwAttestationCarriageV1),
    OpeningCall(PalwOpeningCallCarriageV1),
    OpeningAnswer(PalwOpeningAnswerCarriageV1),
    Refutation(PalwRefutationCarriageV1),
    EvidenceChunk(PalwEvidenceChunkCarriageV1),
}

impl PalwCarriageV1 {
    pub fn kind_byte(&self) -> u8 {
        match self {
            PalwCarriageV1::Commitment(_) => PALW_CARRIAGE_KIND_COMMITMENT,
            PalwCarriageV1::Attestation(_) => PALW_CARRIAGE_KIND_ATTESTATION,
            PalwCarriageV1::OpeningCall(_) => PALW_CARRIAGE_KIND_OPENING_CALL,
            PalwCarriageV1::OpeningAnswer(_) => PALW_CARRIAGE_KIND_OPENING_ANSWER,
            PalwCarriageV1::Refutation(_) => PALW_CARRIAGE_KIND_REFUTATION,
            PalwCarriageV1::EvidenceChunk(_) => PALW_CARRIAGE_KIND_EVIDENCE_CHUNK,
        }
    }
}

/// One chunk of an over-mass refutation (ADR-0029 §6). The group id is the keyed hash of the
/// COMPLETE reassembled payload, so a group cannot be assembled into anything other than what
/// its submitter hashed — chunk substitution changes the id and never assembles.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvidenceChunkCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    pub evidence_group_id: Hash64,
    pub chunk_index: u8,
    pub chunk_count: u8,
    pub bytes: Vec<u8>,
}

/// The group identity of a complete (unsplit) evidence payload.
pub fn palw_evidence_group_id_v1(full_payload: &[u8]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_CARRIAGE_DOMAIN_EVIDENCE_GROUP).to_state();
    h.update(full_payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Splits one oversized carriage payload into a chunk group (the submitter half). Refuses
/// payloads that fit one transaction (send the object directly) or exceed the group budget.
pub fn palw_evidence_chunks_v1(full_payload: &[u8]) -> Result<Vec<PalwEvidenceChunkCarriageV1>, PalwCarriageError> {
    palw_evidence_chunks_with_cap(full_payload, PALW_CARRIAGE_MAX_CHUNK_BYTES)
}

/// The splitter with an explicit per-chunk cap — the production cap is the wrapper above;
/// tests drive multi-chunk assembly of normal-sized payloads through a small cap.
fn palw_evidence_chunks_with_cap(full_payload: &[u8], cap: usize) -> Result<Vec<PalwEvidenceChunkCarriageV1>, PalwCarriageError> {
    if full_payload.len() <= cap {
        return Err(PalwCarriageError::ChunkingUnnecessary { bytes: full_payload.len() });
    }
    let count = full_payload.len().div_ceil(cap);
    if count > PALW_CARRIAGE_MAX_CHUNKS as usize {
        return Err(PalwCarriageError::EvidenceTooLarge { bytes: full_payload.len(), max: cap * PALW_CARRIAGE_MAX_CHUNKS as usize });
    }
    let group = palw_evidence_group_id_v1(full_payload);
    Ok(full_payload
        .chunks(cap)
        .enumerate()
        .map(|(i, bytes)| PalwEvidenceChunkCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            evidence_group_id: group,
            chunk_index: i as u8,
            chunk_count: count as u8,
            bytes: bytes.to_vec(),
        })
        .collect())
}

/// The watcher-side reassembler: first-accepted-wins per (group, index) (the ADR-0029 dedup
/// rule), assembly only when every index is present, the group id recomputed over the
/// concatenation — and the result must decode as a REFUTATION carriage, the only kind big
/// enough to have earned chunking. `W_round` timing against the LAST chunk is the scheduling
/// layer's business; this type only assembles.
#[derive(Default)]
pub struct PalwEvidenceChunkAssemblerV1 {
    groups: std::collections::HashMap<Hash64, Vec<Option<Vec<u8>>>>,
}

impl PalwEvidenceChunkAssemblerV1 {
    /// Feeds one validated chunk. `Ok(Some(refutation))` when its group completes and checks
    /// out; `Ok(None)` while waiting; `Err` on incoherent groups (countable anomalies).
    pub fn insert(&mut self, chunk: &PalwEvidenceChunkCarriageV1) -> Result<Option<PalwRefutationCarriageV1>, PalwCarriageError> {
        validate_evidence_chunk(chunk)?;
        if !self.groups.contains_key(&chunk.evidence_group_id) && self.groups.len() >= PALW_CARRIAGE_MAX_ASSEMBLING_GROUPS {
            return Err(PalwCarriageError::TooManyAssemblingGroups { max: PALW_CARRIAGE_MAX_ASSEMBLING_GROUPS });
        }
        let slots = self.groups.entry(chunk.evidence_group_id).or_insert_with(|| vec![None; chunk.chunk_count as usize]);
        if slots.len() != chunk.chunk_count as usize {
            return Err(PalwCarriageError::ChunkGroupIncoherent("chunk_count differs within one group"));
        }
        let slot = &mut slots[chunk.chunk_index as usize];
        if slot.is_none() {
            *slot = Some(chunk.bytes.clone()); // first-accepted-wins; a duplicate is ignored
        }
        if slots.iter().any(Option::is_none) {
            return Ok(None);
        }
        let full: Vec<u8> = slots.iter().flat_map(|s| s.as_ref().expect("all present").iter().copied()).collect();
        let group = chunk.evidence_group_id;
        self.groups.remove(&group);
        if palw_evidence_group_id_v1(&full) != group {
            return Err(PalwCarriageError::ChunkGroupIncoherent("reassembled bytes do not hash to the group id"));
        }
        match decode_palw_carriage_v1(&full)? {
            Some(PalwCarriageV1::Refutation(r)) => {
                validate_palw_carriage_v1(&PalwCarriageV1::Refutation(r.clone()))?;
                Ok(Some(r))
            }
            _ => Err(PalwCarriageError::ChunkGroupIncoherent("reassembled payload is not a refutation carriage")),
        }
    }
}

fn validate_evidence_chunk(c: &PalwEvidenceChunkCarriageV1) -> Result<(), PalwCarriageError> {
    if c.version != PALW_CARRIAGE_VERSION_V1 {
        return Err(PalwCarriageError::UnsupportedVersion { got: c.version, expected: PALW_CARRIAGE_VERSION_V1 });
    }
    if c.chunk_count < 2 || c.chunk_count > PALW_CARRIAGE_MAX_CHUNKS {
        return Err(PalwCarriageError::ChunkGroupIncoherent("chunk count is not 2..=max"));
    }
    if c.chunk_index >= c.chunk_count {
        return Err(PalwCarriageError::ChunkGroupIncoherent("chunk index is not below the count"));
    }
    if c.bytes.is_empty() || c.bytes.len() > PALW_CARRIAGE_MAX_CHUNK_BYTES {
        return Err(PalwCarriageError::ChunkGroupIncoherent("chunk bytes empty or over the cap"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The signed digest of a commitment carriage
// ---------------------------------------------------------------------------------------------

/// Context-free identity of a carried envelope: keyed BLAKE2b-512 over its Borsh bytes. The
/// network id is *inside* the envelope, so the commitment message below is network-bound
/// through this hash — no second copy (the dual-source rule).
pub fn palw_carriage_envelope_hash_v1(envelope: &PalwJobEnvelopeV2) -> Hash64 {
    let bytes = borsh::to_vec(envelope).expect("borsh of an in-memory envelope cannot fail");
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_CARRIAGE_DOMAIN_ENVELOPE_HASH).to_state();
    h.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Digest a miner signs to commit a job on chain — the same 32-byte keyed-BLAKE2b shape as
/// `compute_capability_message`, over the identity, the bond, the form, the root and the
/// envelope hash.
pub fn palw_commitment_carriage_message_v1(
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    committed_form: u8,
    committed_root: Hash64,
    envelope_hash: Hash64,
) -> Hash {
    let mut h = blake2b_simd::Params::new().hash_length(32).key(PALW_CARRIAGE_DOMAIN_COMMITMENT_MESSAGE).to_state();
    h.update(validator_id.as_byte_slice());
    h.update(bond_outpoint.transaction_id.as_byte_slice());
    h.update(&bond_outpoint.index.to_le_bytes());
    h.update(&[committed_form]);
    h.update(committed_root.as_byte_slice());
    h.update(envelope_hash.as_byte_slice());
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash::from_bytes(out)
}

impl PalwCommitmentCarriageV1 {
    /// The digest this carriage's signature must cover.
    pub fn message(&self) -> Hash {
        palw_commitment_carriage_message_v1(
            self.validator_id,
            self.bond_outpoint,
            self.committed_form,
            self.committed_root,
            palw_carriage_envelope_hash_v1(&self.envelope),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Encode / decode
// ---------------------------------------------------------------------------------------------

/// Serializes one carriage object into a Stage-0 payload: magic, kind, Borsh body. The Stage-1
/// payload is the same bytes MINUS the first seven (the body moves onto its subnetwork id).
pub fn encode_palw_carriage_v1(carriage: &PalwCarriageV1) -> Vec<u8> {
    let body = match carriage {
        PalwCarriageV1::Commitment(c) => borsh::to_vec(c),
        PalwCarriageV1::Attestation(a) => borsh::to_vec(a),
        PalwCarriageV1::OpeningCall(c) => borsh::to_vec(c),
        PalwCarriageV1::OpeningAnswer(a) => borsh::to_vec(a),
        PalwCarriageV1::Refutation(r) => borsh::to_vec(r),
        PalwCarriageV1::EvidenceChunk(c) => borsh::to_vec(c),
    }
    .expect("borsh of an in-memory carriage body cannot fail");
    let mut out = Vec::with_capacity(PALW_CARRIAGE_MAGIC.len() + 1 + body.len());
    out.extend_from_slice(PALW_CARRIAGE_MAGIC);
    out.push(carriage.kind_byte());
    out.extend_from_slice(&body);
    out
}

/// Decodes a payload. `Ok(None)` = not ours (no magic — a foreign native payload, which is not
/// an error); `Err` = claims the magic and then fails, which a watcher counts and a Stage-1
/// admission validator rejects.
pub fn decode_palw_carriage_v1(payload: &[u8]) -> Result<Option<PalwCarriageV1>, PalwCarriageError> {
    let Some(rest) = payload.strip_prefix(PALW_CARRIAGE_MAGIC.as_slice()) else {
        return Ok(None);
    };
    let Some((&kind, body)) = rest.split_first() else {
        return Err(PalwCarriageError::TruncatedEnvelope);
    };
    let decode_err = |e: borsh::io::Error| PalwCarriageError::BodyDecode(e.to_string());
    let carriage = match kind {
        PALW_CARRIAGE_KIND_COMMITMENT => PalwCarriageV1::Commitment(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_ATTESTATION => PalwCarriageV1::Attestation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_OPENING_CALL => PalwCarriageV1::OpeningCall(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_OPENING_ANSWER => PalwCarriageV1::OpeningAnswer(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_REFUTATION => PalwCarriageV1::Refutation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_EVIDENCE_CHUNK => PalwCarriageV1::EvidenceChunk(borsh::from_slice(body).map_err(decode_err)?),
        other => return Err(PalwCarriageError::UnknownKind(other)),
    };
    Ok(Some(carriage))
}

// ---------------------------------------------------------------------------------------------
// Stateless validation — the future Stage-1 admission validators, verbatim
// ---------------------------------------------------------------------------------------------

/// Everything decidable from the bytes alone, per kind. See the module doc for what stateless
/// does NOT mean: a valid object can still be a lie; it cannot be *incoherent*.
pub fn validate_palw_carriage_v1(carriage: &PalwCarriageV1) -> Result<(), PalwCarriageError> {
    match carriage {
        PalwCarriageV1::Commitment(c) => validate_commitment_carriage(c),
        PalwCarriageV1::Attestation(a) => {
            require_version(a.version)?;
            a.attestation.validate_shape().map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
            if a.attester_id != a.attestation.executor_id {
                return Err(PalwCarriageError::AttesterMismatch);
            }
            Ok(())
        }
        PalwCarriageV1::OpeningCall(c) => {
            require_version(c.version)?;
            if c.call.version != PALW_LEGS_OBJECT_VERSION_V1 {
                return Err(PalwCarriageError::Inner(format!("call version {} is not v1", c.call.version)));
            }
            // The envelope's context-free shape; the profile bound is the envelope's own
            // declaration at this layer (class ceilings are stateful).
            c.call.envelope.validate_shape(c.call.envelope.max_context_tokens).map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
            let total = c.call.request.activation.len() + c.call.request.checkpoint_indices.len();
            require_openings_within_cap(total)
        }
        PalwCarriageV1::OpeningAnswer(a) => {
            require_version(a.version)?;
            if a.answer.version != PALW_LEGS_OBJECT_VERSION_V1 {
                return Err(PalwCarriageError::Inner(format!("answer version {} is not v1", a.answer.version)));
            }
            let total = a.answer.activation.len() + a.answer.checkpoints.len();
            require_openings_within_cap(total)
        }
        PalwCarriageV1::Refutation(r) => {
            require_version(r.version)?;
            match &r.evidence {
                PalwCarriedEvidenceV1::Legs(legs) => {
                    if legs.binding.version != PALW_LEGS_OBJECT_VERSION_V1 {
                        return Err(PalwCarriageError::Inner(format!(
                            "legs refutation binding version {} is not v1",
                            legs.binding.version
                        )));
                    }
                }
                PalwCarriedEvidenceV1::Summary(summary) => {
                    if summary.version != crate::palw_slash::PALW_S_OBJECT_VERSION_V1 {
                        return Err(PalwCarriageError::Inner(format!("summary refutation version {} is not v1", summary.version)));
                    }
                }
            }
            Ok(())
        }
        PalwCarriageV1::EvidenceChunk(c) => validate_evidence_chunk(c),
    }
}

fn require_version(got: u16) -> Result<(), PalwCarriageError> {
    if got != PALW_CARRIAGE_VERSION_V1 {
        return Err(PalwCarriageError::UnsupportedVersion { got, expected: PALW_CARRIAGE_VERSION_V1 });
    }
    Ok(())
}

fn require_openings_within_cap(total: usize) -> Result<(), PalwCarriageError> {
    if total == 0 {
        return Err(PalwCarriageError::EmptyOpenings);
    }
    if total > PALW_CARRIAGE_MAX_OPENINGS_PER_CALL {
        return Err(PalwCarriageError::TooManyOpenings { got: total, max: PALW_CARRIAGE_MAX_OPENINGS_PER_CALL });
    }
    Ok(())
}

fn validate_commitment_carriage(c: &PalwCommitmentCarriageV1) -> Result<(), PalwCarriageError> {
    require_version(c.version)?;
    // The envelope's context-free shape (version, network id, prompt and budget bounds); the
    // profile bound is the envelope's own declaration at this layer.
    c.envelope.validate_shape(c.envelope.max_context_tokens).map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
    if c.signature.len() != STAKE_ATTESTATION_SIG_LEN {
        return Err(PalwCarriageError::SignatureLength { got: c.signature.len(), expected: STAKE_ATTESTATION_SIG_LEN });
    }
    match (c.committed_form, &c.binding) {
        (0, None) => Ok(()),
        (0, Some(_)) => Err(PalwCarriageError::FormBindingMismatch("bare form must not carry a binding")),
        (1, None) => Err(PalwCarriageError::FormBindingMismatch("composite form requires its binding")),
        (1, Some(binding)) => validate_composite_binding(c, binding),
        (other, _) => Err(PalwCarriageError::UnknownCommittedForm(other)),
    }
}

/// The composite coherence rules: the binding must describe THIS envelope (every shared field —
/// `tokenizer_id` is the one field the envelope does not carry, so equality is checked with the
/// binding's own), its profiles must be canonical, and it must recompute the committed root it
/// and the carriage both claim. All context-free, all recomputable by any node from the payload.
fn validate_composite_binding(c: &PalwCommitmentCarriageV1, binding: &PalwLegsBindingV1) -> Result<(), PalwCarriageError> {
    if binding.version != PALW_LEGS_OBJECT_VERSION_V1 {
        return Err(PalwCarriageError::Inner(format!("binding version {} is not v1", binding.version)));
    }
    check_job_context_shape(&binding.job_context).map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
    let derived = PalwJobContextV2::from_envelope(&c.envelope, binding.job_context.tokenizer_id);
    if derived != binding.job_context {
        return Err(PalwCarriageError::BindingEnvelopeMismatch("job context"));
    }
    binding.tap_profile.validate_shape().map_err(|reason| PalwCarriageError::Inner(format!("tap profile not canonical: {reason}")))?;
    binding
        .checkpoint_profile
        .validate_shape()
        .map_err(|reason| PalwCarriageError::Inner(format!("checkpoint profile not canonical: {reason}")))?;
    let context_hash = binding.job_context.context_hash();
    let decode_calls = canonical_decode_calls(&binding.job_context);
    let activation_root = activation_leg_root_v1(
        &context_hash,
        &binding.tap_profile.profile_hash(),
        binding.job_context.declared_prefill_tokens,
        decode_calls,
        binding.activation_leaf_count,
        &binding.activation_merkle_root,
    );
    let checkpoint_root = checkpoint_leg_root_v1(
        &context_hash,
        &binding.checkpoint_profile.profile_hash(),
        decode_calls,
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
    );
    let recomputed = execution_commitment_root_v1(&context_hash, &binding.full_logits_trace_root, &activation_root, &checkpoint_root);
    if recomputed != binding.committed_execution_root {
        return Err(PalwCarriageError::BindingRootMismatch);
    }
    if binding.committed_execution_root != c.committed_root {
        return Err(PalwCarriageError::CommittedRootMismatch);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The Stage-0 extractor — the watcher's read of an accepted chain block
// ---------------------------------------------------------------------------------------------

/// Stage-0 twin of `compute_capabilities_with_ids_from_accepted_txs`: native-subnetwork
/// transactions whose payload carries the magic AND decodes AND validates statelessly. There is
/// no admission layer at Stage 0, so the extractor is where invalid claimants are dropped;
/// a watcher that wants to count them calls [`decode_palw_carriage_v1`] itself.
pub fn palw_carriages_from_accepted_txs(txs: &[Transaction]) -> Vec<(TransactionId, PalwCarriageV1)> {
    let mut out = Vec::new();
    for tx in txs {
        if tx.subnetwork_id != SUBNETWORK_ID_NATIVE {
            continue;
        }
        if let Ok(Some(carriage)) = decode_palw_carriage_v1(&tx.payload)
            && validate_palw_carriage_v1(&carriage).is_ok()
        {
            out.push((tx.id(), carriage));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Stage-1 subnetwork carriage (ADR-0029 §1, right column)
// ---------------------------------------------------------------------------------------------
//
// At Stage 1 the SAME bodies ride dedicated subnetwork ids (`SUBNETWORK_ID_PALW_*`, band
// 0x40-0x45) instead of the native subnetwork: the Stage-1 payload is the Stage-0 payload minus
// its 7-byte `"MPALW2" ‖ kind` prefix, because the kind moved into the id. Migration is a change
// of address, not of format — the functions below reuse the per-kind validators above verbatim,
// which is what makes a Stage-0 object and its Stage-1 twin verify identically.
//
// DEPLOYMENT (the coordinated-release rule, restated where the validators live): an unknown
// subnetwork id is `SubnetworksDisabled` at admission on every deployed node, so a block
// carrying one of these ids splits an unupgraded fleet. Shipping these constants + validators
// IS the release artifact; nothing activates until the whole fleet admits the band.

/// Maps a PALW carriage subnetwork id to its kind byte — the Stage-1 mirror of
/// `dns_finality::dns_tx_kind`, except the kind is this module's own byte code
/// (`PALW_CARRIAGE_KIND_*`), so no second enum exists to drift from the Stage-0 envelope's.
/// `None` = not a PALW carriage id (native / coinbase / another band / unknown).
pub fn palw_carriage_tx_kind(subnetwork_id: &SubnetworkId) -> Option<u8> {
    if *subnetwork_id == SUBNETWORK_ID_PALW_COMMITMENT {
        Some(PALW_CARRIAGE_KIND_COMMITMENT)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_ATTESTATION {
        Some(PALW_CARRIAGE_KIND_ATTESTATION)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_OPENING_CALL {
        Some(PALW_CARRIAGE_KIND_OPENING_CALL)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_OPENING_ANSWER {
        Some(PALW_CARRIAGE_KIND_OPENING_ANSWER)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_REFUTATION {
        Some(PALW_CARRIAGE_KIND_REFUTATION)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_EVIDENCE_CHUNK {
        Some(PALW_CARRIAGE_KIND_EVIDENCE_CHUNK)
    } else {
        None
    }
}

/// Decodes a Stage-1 payload: the Borsh BODY directly, no magic and no kind byte — those are the
/// subnetwork id's job at this stage. The Stage-0 door (`decode_palw_carriage_v1`) is untouched;
/// the two decode the same bytes to the same object (`payload_stage0[7..] == payload_stage1`,
/// pinned by test).
pub fn decode_palw_stage1_body(kind: u8, body: &[u8]) -> Result<PalwCarriageV1, PalwCarriageError> {
    let decode_err = |e: borsh::io::Error| PalwCarriageError::BodyDecode(e.to_string());
    Ok(match kind {
        PALW_CARRIAGE_KIND_COMMITMENT => PalwCarriageV1::Commitment(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_ATTESTATION => PalwCarriageV1::Attestation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_OPENING_CALL => PalwCarriageV1::OpeningCall(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_OPENING_ANSWER => PalwCarriageV1::OpeningAnswer(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_REFUTATION => PalwCarriageV1::Refutation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_EVIDENCE_CHUNK => PalwCarriageV1::EvidenceChunk(borsh::from_slice(body).map_err(decode_err)?),
        other => return Err(PalwCarriageError::UnknownKind(other)),
    })
}

/// The Stage-1 admission validator: everything `check_transaction_subnetwork` can decide about a
/// PALW-band transaction from the transaction alone. Kind comes from the subnetwork id (via
/// [`palw_carriage_tx_kind`]); the body decodes as that kind and passes the SAME per-kind
/// stateless validation Stage 0 runs ([`validate_palw_carriage_v1`]). Bond existence, ML-DSA-87
/// signature validity and every cross-object question are stateful and are NOT admission rules —
/// a stateless-valid carriage can still be a lie; it cannot be incoherent.
///
/// The one check that needs the transaction rather than the body: **evidence carriers declare no
/// outputs** (ADR-0029 §2, the slashing/challenge/precommit-evidence rule, adopted so the
/// Stage-2 reporter-reward slot `(tx_id, 0)` is never a retrofit). Kind 0x05 is the rule's named
/// subject; kind 0x06 carries the same evidence in chunks, and whichever chunk transaction an
/// adjudication one day names (`W_round` runs from the LAST chunk) must have its slot equally
/// clear — so both are pure carriers, checked first like `validate_slashing_evidence_tx` does.
pub fn validate_palw_carriage_stage1_tx(kind: u8, payload: &[u8], outputs: &[TransactionOutput]) -> Result<(), PalwCarriageError> {
    if matches!(kind, PALW_CARRIAGE_KIND_REFUTATION | PALW_CARRIAGE_KIND_EVIDENCE_CHUNK) && !outputs.is_empty() {
        return Err(PalwCarriageError::EvidenceCarrierHasOutputs(outputs.len()));
    }
    let carriage = decode_palw_stage1_body(kind, payload)?;
    validate_palw_carriage_v1(&carriage)
}

/// One accepted Stage-1 carriage as the in-node store keeps it: the kind, the acceptance DAA of
/// the carrier (the object's protocol time, ADR-0029 §4), and the Borsh body bytes **verbatim** —
/// what the chain carried, never a re-encoding, so a Stage-2 reader decodes exactly what
/// admission validated and a schema change in this record cannot silently reshape a body.
///
/// Rows are keyed by carrying transaction id (the revert-friendly capability-store key); logical
/// identity — first-accepted-wins per `committed_root`, `(commitment_root, attester_id)`,
/// `call_tx_id` — is the Stage-2 reader's business, exactly where ADR-0029 §2 assigns it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwCarriageRecord {
    /// The carriage kind byte (`PALW_CARRIAGE_KIND_*`), as routed by the carrier's subnetwork id.
    pub kind: u8,
    /// DAA score of the chain block that accepted the carrier.
    pub accepted_daa_score: u64,
    /// The Stage-1 payload verbatim (the Borsh body — no magic, no kind prefix).
    pub body: Vec<u8>,
}

impl MemSizeEstimator for PalwCarriageRecord {}

/// Stage-1 twin of `compute_capabilities_with_ids_from_accepted_txs`: every accepted transaction
/// routed by a PALW carriage subnetwork id, as the store row it becomes. Admission already
/// validated these statelessly — a block carrying an invalid one is invalid on a fleet running
/// this code — but the walk re-checks anyway: the same defense the Stage-0 extractor applies,
/// and the backfill sweep crosses history this build cannot vouch was written under it.
pub fn palw_carriage_records_from_accepted_txs(
    txs: &[Transaction],
    accepted_daa_score: u64,
) -> Vec<(TransactionId, PalwCarriageRecord)> {
    let mut out = Vec::new();
    for tx in txs {
        let Some(kind) = palw_carriage_tx_kind(&tx.subnetwork_id) else { continue };
        if validate_palw_carriage_stage1_tx(kind, &tx.payload, &tx.outputs).is_ok() {
            out.push((tx.id(), PalwCarriageRecord { kind, accepted_daa_score, body: tx.payload.clone() }));
        }
    }
    out
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_legs::PalwActivationCoordinateV1;
    use crate::palw_legs::{
        PALW_LEGS_ALL_DOMAINS, PalwActivationLeafV1, PalwActivationTapProfileV1, PalwCheckpointProfileV1, PalwLegOpeningV1,
        PalwLegsCommitmentBuilderV1, PalwLegsOpeningRequestV1, PalwOpenedActivationLeafV1,
    };
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::{PALW_S_ALL_DOMAINS, PALW_S_OBJECT_VERSION_V1};
    use crate::palw_v2::{
        PALW_JOB_WIRE_VERSION_V2, PALW_V2_ALL_DOMAINS, PalwLogitsDtypeV2, PalwStopReasonV2, PalwTracePhaseV2, PalwTraceSummaryV2,
    };
    use crate::subnets::SUBNETWORK_ID_COINBASE;
    use crate::vlt::VERIFIER_SORTITION_KEY;

    fn h64(seed: u8) -> Hash64 {
        Hash64::from_bytes([seed; 64])
    }

    fn outpoint(seed: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index }
    }

    fn test_envelope() -> PalwJobEnvelopeV2 {
        PalwJobEnvelopeV2 {
            version: PALW_JOB_WIRE_VERSION_V2,
            network_id: b"misaka-carriage-test".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x12),
            mode: crate::palw_v2::PalwJobModeV2::Execute,
            model_profile_id: h64(0x31),
            runtime_manifest_hash: h64(0x32),
            runtime_class_id: h64(0x33),
            shape_profile_id: h64(0x34),
            trace_scheme_id: crate::palw_v2::trace_scheme_id_v2(),
            cu_ruleset_id: h64(0x36),
            execution_seed: [0x22; 32],
            prompt_token_ids: vec![1, 2, 3],
            exact_decode_tokens: 3,
            max_context_tokens: 64,
            assignment_id: h64(0x13),
            assignment_epoch: 7,
            deadline_unix_ms: 0,
        }
    }

    /// A real composite binding whose context derives from `test_envelope` — built through the
    /// producer, so it is honest by construction: 1 tap × (3 prefill + 2 decode calls) = 5 rows,
    /// interval 2 ⇒ one checkpoint covering decode call 2.
    fn test_binding() -> PalwLegsBindingV1 {
        let context = PalwJobContextV2::from_envelope(&test_envelope(), h64(0x37));
        let tap = PalwActivationTapProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            tap_semantics_id: h64(0x41),
            tap_layer_indices: vec![8],
            model_total_layers: 24,
            hidden_dim: 4,
            dtype: PalwLogitsDtypeV2::F32Le,
        };
        let ckpt =
            PalwCheckpointProfileV1 { version: PALW_LEGS_OBJECT_VERSION_V1, checkpoint_interval: 2, state_layout_id: h64(0x51) };
        let mut builder = PalwLegsCommitmentBuilderV1::new(context, tap, ckpt).expect("canonical inputs");
        for position in 0..3u32 {
            builder.push_activation_row(0, 0, position, &[1.0, 2.0, 3.0, 4.0 + position as f32]).unwrap();
        }
        for call in 1..3u32 {
            builder.push_activation_row(call, 0, 0, &[5.0 + call as f32, 6.0, 7.0, 8.0]).unwrap();
        }
        builder.push_checkpoint(2, h64(0x61)).unwrap();
        builder.finish(h64(0x71)).expect("complete legs")
    }

    fn commitment_composite() -> PalwCommitmentCarriageV1 {
        let binding = test_binding();
        PalwCommitmentCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            envelope: test_envelope(),
            committed_form: 1,
            committed_root: binding.committed_execution_root,
            binding: Some(binding),
            validator_id: h64(0xA1),
            bond_outpoint: outpoint(0xB1, 0),
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn commitment_bare() -> PalwCommitmentCarriageV1 {
        PalwCommitmentCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            envelope: test_envelope(),
            committed_form: 0,
            committed_root: h64(0x71),
            binding: None,
            validator_id: h64(0xA1),
            bond_outpoint: outpoint(0xB1, 0),
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn attestation() -> PalwAttestationCarriageV1 {
        PalwAttestationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            commitment_root: test_binding().committed_execution_root,
            attestation: PalwExecutionAttestationV1 {
                version: PALW_S_OBJECT_VERSION_V1,
                executor_id: h64(0xA2),
                job_context_hash: PalwJobContextV2::from_envelope(&test_envelope(), h64(0x37)).context_hash(),
                full_logits_trace_root: h64(0x71),
                signature: vec![0x33; STAKE_ATTESTATION_SIG_LEN],
            },
            attester_id: h64(0xA2),
            bond_outpoint: outpoint(0xB2, 1),
        }
    }

    fn opening_call() -> PalwOpeningCallCarriageV1 {
        PalwOpeningCallCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            call: PalwLegsOpeningCallV1 {
                version: PALW_LEGS_OBJECT_VERSION_V1,
                envelope: test_envelope(),
                request: PalwLegsOpeningRequestV1 {
                    version: PALW_LEGS_OBJECT_VERSION_V1,
                    committed_execution_root: test_binding().committed_execution_root,
                    activation: vec![PalwActivationCoordinateV1 { call_index: 0, tap_slot: 0, position: 0 }],
                    checkpoint_indices: vec![0],
                },
            },
        }
    }

    fn opening_answer() -> PalwOpeningAnswerCarriageV1 {
        // Encoding-golden material: a structurally well-formed answer (validity beyond the
        // carriage caps is the opening checker's business, not carriage's).
        PalwOpeningAnswerCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            call_tx_id: TransactionId::from_bytes([0xC4; 64]),
            answer: PalwLegsOpeningAnswerV1 {
                version: PALW_LEGS_OBJECT_VERSION_V1,
                binding: test_binding(),
                activation: vec![PalwOpenedActivationLeafV1 {
                    opening: PalwLegOpeningV1 { leaf_index: 0, leaf_hash: h64(0xD1), siblings: vec![h64(0xD2)] },
                    preimage: PalwActivationLeafV1 {
                        call_index: 0,
                        tap_slot: 0,
                        position: 0,
                        hidden_dim: 4,
                        value_count: 4,
                        values_le_bytes: [1.0f32, 2.0, 3.0, 4.0].iter().flat_map(|v| v.to_le_bytes()).collect(),
                    },
                }],
                checkpoints: vec![],
            },
        }
    }

    fn refutation() -> PalwRefutationCarriageV1 {
        PalwRefutationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            evidence: PalwCarriedEvidenceV1::Summary(PalwTraceSummaryRefutationV1 {
                version: PALW_S_OBJECT_VERSION_V1,
                job_context: PalwJobContextV2::from_envelope(&test_envelope(), h64(0x37)),
                summary: PalwTraceSummaryV2 {
                    vocab_size: 8,
                    logits_dtype: PalwLogitsDtypeV2::F32Le,
                    declared_prefill_tokens: 3,
                    exact_decode_tokens: 3,
                    event_count: 3,
                    first_event_kind: PalwTracePhaseV2::Prefill,
                    last_event_kind: PalwTracePhaseV2::Decode,
                    output_token_ids_hash: h64(0xE1),
                    stop_reason: PalwStopReasonV2::ExactBudgetReached,
                },
                ordered_event_commitment: h64(0xE2),
                committed_trace_root: h64(0xE3),
            }),
        }
    }

    fn all_five() -> Vec<PalwCarriageV1> {
        vec![
            PalwCarriageV1::Commitment(commitment_composite()),
            PalwCarriageV1::Attestation(attestation()),
            PalwCarriageV1::OpeningCall(opening_call()),
            PalwCarriageV1::OpeningAnswer(opening_answer()),
            PalwCarriageV1::Refutation(refutation()),
        ]
    }

    fn payload_hash_hex(payload: &[u8]) -> String {
        let mut h = blake2b_simd::Params::new().hash_length(32).to_state();
        h.update(payload);
        h.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    // -----------------------------------------------------------------------------------------
    // Envelope and identity
    // -----------------------------------------------------------------------------------------

    #[test]
    fn magic_and_kind_bytes_are_frozen() {
        assert_eq!(PALW_CARRIAGE_MAGIC, b"MPALW2");
        assert_eq!(
            [
                PALW_CARRIAGE_KIND_COMMITMENT,
                PALW_CARRIAGE_KIND_ATTESTATION,
                PALW_CARRIAGE_KIND_OPENING_CALL,
                PALW_CARRIAGE_KIND_OPENING_ANSWER,
                PALW_CARRIAGE_KIND_REFUTATION,
            ],
            [0x01, 0x02, 0x03, 0x04, 0x05]
        );
        for carriage in all_five() {
            let payload = encode_palw_carriage_v1(&carriage);
            assert_eq!(&payload[..6], PALW_CARRIAGE_MAGIC);
            assert_eq!(payload[6], carriage.kind_byte());
        }
    }

    /// Full-payload goldens: any byte moving in any preimage, layout or inner object shows up
    /// here first. Regenerating them is a conscious wire change, never a side effect.
    #[test]
    fn encoded_payloads_are_golden() {
        let golden: [(&str, &str); 5] = [
            ("commitment", "2ef18007f17d928e80c6ecf94e6e9b71eabd24ba9b879f7045a18b72665fef14"),
            ("attestation", "50331d96be49a36fca5cdb9638db467e7264c3b915609686927edfab120fd208"),
            ("opening-call", "aa0328847eae54b9fab887704631f020fed2e468d2906c564c32f2577001eea1"),
            ("opening-answer", "bb106d592e6d7ce811f40471497198d230cd6611106689b70bba180f6113bea7"),
            ("refutation", "b18fc4d07f142e6d424dafa9a2d45b26ceebb7db98132c39c2342e64f7f80680"),
        ];
        for (carriage, (name, expected)) in all_five().iter().zip(golden) {
            let got = payload_hash_hex(&encode_palw_carriage_v1(carriage));
            assert_eq!(got, expected, "{name} payload moved");
        }
    }

    #[test]
    fn roundtrip_and_validate_all_five() {
        for carriage in all_five() {
            let payload = encode_palw_carriage_v1(&carriage);
            let decoded = decode_palw_carriage_v1(&payload).unwrap().expect("ours");
            assert_eq!(decoded, carriage);
            validate_palw_carriage_v1(&decoded).unwrap();
        }
        // The bare form is valid too.
        validate_palw_carriage_v1(&PalwCarriageV1::Commitment(commitment_bare())).unwrap();
    }

    #[test]
    fn foreign_payloads_are_none_and_broken_claimants_are_errors() {
        assert_eq!(decode_palw_carriage_v1(b"").unwrap(), None);
        assert_eq!(decode_palw_carriage_v1(b"\x00rand-bytes").unwrap(), None);
        assert_eq!(decode_palw_carriage_v1(b"MPALW1\x01junk").unwrap(), None, "wrong magic is foreign");
        assert_eq!(decode_palw_carriage_v1(b"MPALW2"), Err(PalwCarriageError::TruncatedEnvelope));
        assert_eq!(decode_palw_carriage_v1(b"MPALW2\x09"), Err(PalwCarriageError::UnknownKind(0x09)));
        assert!(matches!(decode_palw_carriage_v1(b"MPALW2\x01truncated"), Err(PalwCarriageError::BodyDecode(_))));
        // A valid payload truncated mid-body is a decode error, not a smaller object.
        let payload = encode_palw_carriage_v1(&PalwCarriageV1::Attestation(attestation()));
        assert!(matches!(decode_palw_carriage_v1(&payload[..payload.len() - 1]), Err(PalwCarriageError::BodyDecode(_))));
    }

    #[test]
    fn commitment_message_binds_every_input() {
        let base = commitment_composite();
        let m = base.message();
        let mut other = base.clone();
        other.validator_id = h64(0xA9);
        assert_ne!(m, other.message());
        let mut other = base.clone();
        other.bond_outpoint = outpoint(0xB1, 1);
        assert_ne!(m, other.message());
        let mut other = base.clone();
        other.committed_form = 0;
        assert_ne!(m, other.message());
        let mut other = base.clone();
        other.committed_root = h64(0x72);
        assert_ne!(m, other.message());
        let mut other = base;
        other.envelope.prompt_token_ids = vec![1, 2, 4];
        assert_ne!(m, other.message(), "the network and the input are bound through the envelope hash");
    }

    // -----------------------------------------------------------------------------------------
    // Stateless validation
    // -----------------------------------------------------------------------------------------

    #[test]
    fn form_and_binding_must_agree() {
        let mut composite_without = commitment_composite();
        composite_without.binding = None;
        assert!(matches!(
            validate_palw_carriage_v1(&PalwCarriageV1::Commitment(composite_without)),
            Err(PalwCarriageError::FormBindingMismatch(_))
        ));
        let mut bare_with = commitment_bare();
        bare_with.binding = Some(test_binding());
        assert!(matches!(
            validate_palw_carriage_v1(&PalwCarriageV1::Commitment(bare_with)),
            Err(PalwCarriageError::FormBindingMismatch(_))
        ));
        let mut unknown = commitment_bare();
        unknown.committed_form = 2;
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::Commitment(unknown)), Err(PalwCarriageError::UnknownCommittedForm(2)));
    }

    #[test]
    fn a_composite_commitment_is_recomputed_not_trusted() {
        // The committed root is a field AND a recomputation — flip the field, the recompute
        // disagrees.
        let mut wrong_root = commitment_composite();
        wrong_root.committed_root = h64(0x99);
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::Commitment(wrong_root)), Err(PalwCarriageError::CommittedRootMismatch));
        // Tamper inside the binding: the recompute catches it before any root comparison.
        let mut tampered = commitment_composite();
        if let Some(binding) = tampered.binding.as_mut() {
            binding.activation_merkle_root = h64(0x98);
        }
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::Commitment(tampered)), Err(PalwCarriageError::BindingRootMismatch));
        // A binding for a DIFFERENT envelope cannot ride: the context equality fails on any
        // drifted field.
        let mut transplanted = commitment_composite();
        transplanted.envelope.job_id = h64(0x19);
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Commitment(transplanted)),
            Err(PalwCarriageError::BindingEnvelopeMismatch("job context"))
        );
    }

    #[test]
    fn signature_length_is_exactly_mldsa87() {
        let mut short = commitment_bare();
        short.signature = vec![0x5A; 64];
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Commitment(short)),
            Err(PalwCarriageError::SignatureLength { got: 64, expected: STAKE_ATTESTATION_SIG_LEN })
        );
    }

    #[test]
    fn attester_identity_cannot_drift_from_the_signer() {
        let mut drifted = attestation();
        drifted.attester_id = h64(0xA3);
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::Attestation(drifted)), Err(PalwCarriageError::AttesterMismatch));
    }

    #[test]
    fn the_carriage_cap_is_sixteen_and_zero_is_nothing() {
        let mut over = opening_call();
        over.call.request.activation =
            (0..17u32).map(|i| PalwActivationCoordinateV1 { call_index: 0, tap_slot: 0, position: i }).collect();
        over.call.request.checkpoint_indices.clear();
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::OpeningCall(over)),
            Err(PalwCarriageError::TooManyOpenings { got: 17, max: PALW_CARRIAGE_MAX_OPENINGS_PER_CALL })
        );
        let mut empty = opening_call();
        empty.call.request.activation.clear();
        empty.call.request.checkpoint_indices.clear();
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::OpeningCall(empty)), Err(PalwCarriageError::EmptyOpenings));
        // Same cap on the answer side.
        let mut fat_answer = opening_answer();
        let entry = fat_answer.answer.activation[0].clone();
        fat_answer.answer.activation = vec![entry; 17];
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::OpeningAnswer(fat_answer)),
            Err(PalwCarriageError::TooManyOpenings { got: 17, max: PALW_CARRIAGE_MAX_OPENINGS_PER_CALL })
        );
    }

    // -----------------------------------------------------------------------------------------
    // The Stage-0 extractor
    // -----------------------------------------------------------------------------------------

    fn tx_with(subnetwork: crate::subnets::SubnetworkId, payload: Vec<u8>) -> Transaction {
        Transaction::new(0, vec![], vec![], 0, subnetwork, 0, payload)
    }

    #[test]
    fn the_extractor_takes_valid_native_carriages_and_nothing_else() {
        let good = encode_palw_carriage_v1(&PalwCarriageV1::Attestation(attestation()));
        let mut invalid_claimant = attestation();
        invalid_claimant.attester_id = h64(0xA3); // decodes, fails stateless validation
        let bad = encode_palw_carriage_v1(&PalwCarriageV1::Attestation(invalid_claimant));
        let txs = vec![
            tx_with(SUBNETWORK_ID_NATIVE, good.clone()),
            tx_with(SUBNETWORK_ID_NATIVE, b"just some native payload".to_vec()),
            tx_with(SUBNETWORK_ID_NATIVE, bad),
            tx_with(SUBNETWORK_ID_NATIVE, b"MPALW2\x09".to_vec()),
            tx_with(SUBNETWORK_ID_COINBASE, good.clone()), // right payload, wrong lane
            tx_with(SUBNETWORK_ID_NATIVE, Vec::new()),
        ];
        let extracted = palw_carriages_from_accepted_txs(&txs);
        assert_eq!(extracted.len(), 1, "exactly the valid native carriage");
        assert_eq!(extracted[0].0, txs[0].id());
        assert_eq!(extracted[0].1, PalwCarriageV1::Attestation(attestation()));
    }

    // -----------------------------------------------------------------------------------------
    // Domains
    // -----------------------------------------------------------------------------------------

    #[test]
    fn carriage_domains_are_unique_across_all_palw_modules() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend_from_slice(PALW_CARRIAGE_ALL_DOMAINS);
        all.extend_from_slice(PALW_LEGS_ALL_DOMAINS);
        all.extend_from_slice(PALW_V2_ALL_DOMAINS);
        all.extend_from_slice(PALW_S_ALL_DOMAINS);
        all.extend_from_slice(PALW_REFERENCE_ALL_DOMAINS);
        all.extend_from_slice(PALW_SCHEDULE_ALL_DOMAINS);
        all.push(VERIFIER_SORTITION_KEY);
        let before = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), before, "a domain string is shared across families — a preimage bridge");
        for domain in PALW_CARRIAGE_ALL_DOMAINS {
            assert!(domain.len() <= 64, "blake2b key cap");
        }
    }

    // -----------------------------------------------------------------------------------------
    // Evidence chunk carriage (ADR-0029 §6)
    // -----------------------------------------------------------------------------------------

    #[test]
    fn evidence_chunks_split_ride_and_reassemble() {
        let full = encode_palw_carriage_v1(&PalwCarriageV1::Refutation(refutation()));
        // Small cap → a real multi-chunk group over a REAL refutation payload.
        let cap = full.len().div_ceil(3);
        let chunks = super::palw_evidence_chunks_with_cap(&full, cap).unwrap();
        assert_eq!(chunks.len(), 3);
        for c in &chunks {
            // Each chunk is itself a valid carriage object that encodes and decodes.
            let payload = encode_palw_carriage_v1(&PalwCarriageV1::EvidenceChunk(c.clone()));
            let back = decode_palw_carriage_v1(&payload).unwrap().unwrap();
            validate_palw_carriage_v1(&back).unwrap();
        }
        // Assembly in a scrambled arrival order.
        let mut asm = PalwEvidenceChunkAssemblerV1::default();
        assert_eq!(asm.insert(&chunks[2]).unwrap(), None);
        assert_eq!(asm.insert(&chunks[0]).unwrap(), None);
        // A duplicate of an already-held index is ignored (first-accepted-wins).
        assert_eq!(asm.insert(&chunks[0]).unwrap(), None);
        let out = asm.insert(&chunks[1]).unwrap().expect("group completes");
        assert_eq!(out, refutation());
    }

    #[test]
    fn evidence_chunk_substitution_never_assembles() {
        let full = encode_palw_carriage_v1(&PalwCarriageV1::Refutation(refutation()));
        let cap = full.len().div_ceil(3);
        let mut chunks = super::palw_evidence_chunks_with_cap(&full, cap).unwrap();
        // Tamper one byte of chunk 1: the group id no longer matches the reassembly.
        chunks[1].bytes[0] ^= 1;
        let mut asm = PalwEvidenceChunkAssemblerV1::default();
        asm.insert(&chunks[0]).unwrap();
        asm.insert(&chunks[1]).unwrap();
        let got = asm.insert(&chunks[2]);
        assert!(matches!(got, Err(PalwCarriageError::ChunkGroupIncoherent(_))), "got {got:?}");
    }

    #[test]
    fn evidence_chunk_rules_are_closed() {
        // Fits-one-transaction payloads must not be chunked.
        assert!(matches!(palw_evidence_chunks_v1(&[0u8; 1000]), Err(PalwCarriageError::ChunkingUnnecessary { .. })));
        // Over the group budget refuses.
        assert!(matches!(super::palw_evidence_chunks_with_cap(&vec![0u8; 100], 10), Err(PalwCarriageError::EvidenceTooLarge { .. })));
        // The 0.99 MiB bare-v2 case rides in 3 chunks under the production cap.
        let logits_case = 248_320usize * 4 + 24_000; // row + object overhead margin
        let count = logits_case.div_ceil(PALW_CARRIAGE_MAX_CHUNK_BYTES);
        assert_eq!(count, 3, "the ADR-0029 §6 arithmetic");
        // Malformed chunks are rejected before assembly.
        let bad = PalwEvidenceChunkCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            evidence_group_id: Hash64::from_bytes([9; 64]),
            chunk_index: 2,
            chunk_count: 2,
            bytes: vec![1],
        };
        assert!(matches!(PalwEvidenceChunkAssemblerV1::default().insert(&bad), Err(PalwCarriageError::ChunkGroupIncoherent(_))));
        // A completed group that decodes as a NON-refutation kind refuses.
        let not_refutation = encode_palw_carriage_v1(&PalwCarriageV1::Attestation(attestation()));
        let cap = not_refutation.len().div_ceil(2);
        let chunks = super::palw_evidence_chunks_with_cap(&not_refutation, cap).unwrap();
        let mut asm = PalwEvidenceChunkAssemblerV1::default();
        for c in &chunks[..chunks.len() - 1] {
            asm.insert(c).unwrap();
        }
        assert!(matches!(asm.insert(&chunks[chunks.len() - 1]), Err(PalwCarriageError::ChunkGroupIncoherent(_))));
    }

    // -----------------------------------------------------------------------------------------
    // Stage-1 subnetwork carriage
    // -----------------------------------------------------------------------------------------

    fn evidence_chunk() -> PalwEvidenceChunkCarriageV1 {
        PalwEvidenceChunkCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            evidence_group_id: h64(0xF1),
            chunk_index: 0,
            chunk_count: 2,
            bytes: vec![0xAB; 8],
        }
    }

    fn all_six() -> Vec<(SubnetworkId, PalwCarriageV1)> {
        let mut out: Vec<(SubnetworkId, PalwCarriageV1)> = vec![
            (SUBNETWORK_ID_PALW_COMMITMENT, PalwCarriageV1::Commitment(commitment_composite())),
            (SUBNETWORK_ID_PALW_ATTESTATION, PalwCarriageV1::Attestation(attestation())),
            (SUBNETWORK_ID_PALW_OPENING_CALL, PalwCarriageV1::OpeningCall(opening_call())),
            (SUBNETWORK_ID_PALW_OPENING_ANSWER, PalwCarriageV1::OpeningAnswer(opening_answer())),
            (SUBNETWORK_ID_PALW_REFUTATION, PalwCarriageV1::Refutation(refutation())),
            (SUBNETWORK_ID_PALW_EVIDENCE_CHUNK, PalwCarriageV1::EvidenceChunk(evidence_chunk())),
        ];
        debug_assert_eq!(out.len(), 6);
        out.sort_by_key(|(_, c)| c.kind_byte());
        out
    }

    /// The Stage-1 contract, pinned end to end: each new subnetwork id routes to exactly its
    /// kind byte, and the Stage-1 payload IS the Stage-0 payload minus the 7-byte magic+kind
    /// prefix — so the goldens above cover both stages and the two decode paths cannot drift.
    #[test]
    fn stage1_ids_route_and_bodies_are_stage0_minus_the_envelope() {
        for (id, carriage) in all_six() {
            let kind = palw_carriage_tx_kind(&id).expect("a PALW band id routes");
            assert_eq!(kind, carriage.kind_byte(), "id {id} routes to its own kind");
            let stage0 = encode_palw_carriage_v1(&carriage);
            let stage1_body = &stage0[PALW_CARRIAGE_MAGIC.len() + 1..];
            let decoded = decode_palw_stage1_body(kind, stage1_body).unwrap();
            assert_eq!(decoded, carriage, "one format, two addresses");
            validate_palw_carriage_v1(&decoded).unwrap();
        }
        // Foreign ids do not route: the band has hard edges on both sides.
        for id in [
            SUBNETWORK_ID_NATIVE,
            SUBNETWORK_ID_COINBASE,
            crate::subnets::SUBNETWORK_ID_TOKEN_BURN,
            SubnetworkId::from_byte(0x3F),
            SubnetworkId::from_byte(0x46),
        ] {
            assert_eq!(palw_carriage_tx_kind(&id), None);
        }
        // An unknown kind byte is an error, not a fallthrough.
        assert_eq!(decode_palw_stage1_body(0x09, b"anything"), Err(PalwCarriageError::UnknownKind(0x09)));
    }

    /// ADR-0029 §2: refutations and their chunks are pure evidence carriers — no outputs, so the
    /// Stage-2 reporter-reward slot `(tx_id, 0)` is never a retrofit. Checked before the body
    /// decode, like `validate_slashing_evidence_tx`. Every other kind may carry outputs (change).
    #[test]
    fn stage1_evidence_carriers_declare_no_outputs() {
        let outputs = vec![crate::tx::TransactionOutput::new(1_000, crate::tx::ScriptPublicKey::default())];
        let refutation_body = borsh::to_vec(&refutation()).unwrap();
        assert_eq!(
            validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_REFUTATION, &refutation_body, &outputs),
            Err(PalwCarriageError::EvidenceCarrierHasOutputs(1))
        );
        validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_REFUTATION, &refutation_body, &[]).unwrap();
        let chunk_body = borsh::to_vec(&evidence_chunk()).unwrap();
        assert_eq!(
            validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_EVIDENCE_CHUNK, &chunk_body, &outputs),
            Err(PalwCarriageError::EvidenceCarrierHasOutputs(1))
        );
        validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_EVIDENCE_CHUNK, &chunk_body, &[]).unwrap();
        // A non-evidence kind rides with outputs (a funded carrier has change).
        let attestation_body = borsh::to_vec(&attestation()).unwrap();
        validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_ATTESTATION, &attestation_body, &outputs).unwrap();
        // The outputs rule is checked first, but a clean carrier still fails on its body.
        assert!(matches!(
            validate_palw_carriage_stage1_tx(PALW_CARRIAGE_KIND_REFUTATION, b"garbage", &[]),
            Err(PalwCarriageError::BodyDecode(_))
        ));
    }

    /// The store walk's read of an accepted chain block: exactly the valid PALW-band carriers,
    /// stamped with the acceptance DAA and holding the payload bytes verbatim.
    #[test]
    fn stage1_extractor_takes_valid_palw_band_txs_and_nothing_else() {
        let good_body = borsh::to_vec(&attestation()).unwrap();
        let mut drifted = attestation();
        drifted.attester_id = h64(0xA3); // decodes, fails the same stateless validation Stage 0 runs
        let bad_body = borsh::to_vec(&drifted).unwrap();
        let stage0_payload = encode_palw_carriage_v1(&PalwCarriageV1::Attestation(attestation()));
        let txs = vec![
            tx_with(SUBNETWORK_ID_PALW_ATTESTATION, good_body.clone()),
            tx_with(SUBNETWORK_ID_PALW_ATTESTATION, bad_body),
            tx_with(SUBNETWORK_ID_PALW_ATTESTATION, b"not borsh".to_vec()),
            // A Stage-0 payload (magic still on) does not ride a Stage-1 id: the body must be bare.
            tx_with(SUBNETWORK_ID_PALW_ATTESTATION, stage0_payload.clone()),
            // The right body on the WRONG id of the band decodes as the id's kind — and fails.
            tx_with(SUBNETWORK_ID_PALW_COMMITMENT, good_body.clone()),
            // A native Stage-0 carrier is the OTHER extractor's business.
            tx_with(SUBNETWORK_ID_NATIVE, stage0_payload),
        ];
        let extracted = palw_carriage_records_from_accepted_txs(&txs, 4_242);
        assert_eq!(extracted.len(), 1, "exactly the valid Stage-1 carrier");
        assert_eq!(extracted[0].0, txs[0].id());
        assert_eq!(
            extracted[0].1,
            PalwCarriageRecord { kind: PALW_CARRIAGE_KIND_ATTESTATION, accepted_daa_score: 4_242, body: good_body }
        );
        // The record's body decodes back to the object admission validated — bytes verbatim.
        assert_eq!(
            decode_palw_stage1_body(extracted[0].1.kind, &extracted[0].1.body).unwrap(),
            PalwCarriageV1::Attestation(attestation())
        );
    }
}
