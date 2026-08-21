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

use crate::BlockHash;
use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
use crate::dns_finality::StakeBondRecord;
use crate::palw_legs::{
    PALW_LEGS_OBJECT_VERSION_V1, PalwLegsBindingV1, PalwLegsOpeningAnswerV1, PalwLegsOpeningCallV1, PalwLegsRefutationV1,
    activation_leg_root_v1, canonical_decode_calls, checkpoint_leg_root_v1, execution_commitment_root_v1,
};
use crate::palw_slash::{
    PalwClassContradictionCertificateV1, PalwClassContradictionKindV1, PalwExecutionAttestationV1, PalwTraceSummaryRefutationV1,
    adjudicate_class_contradiction_v1, check_job_context_shape,
};
use crate::palw_v2::{PalwJobContextV2, PalwJobEnvelopeV2};
use crate::subnets::{
    SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_ATTESTATION, SUBNETWORK_ID_PALW_BISECT_MOVE, SUBNETWORK_ID_PALW_COMMITMENT,
    SUBNETWORK_ID_PALW_EQUIVOCATION, SUBNETWORK_ID_PALW_EVIDENCE_CHUNK, SUBNETWORK_ID_PALW_OPENING_ANSWER,
    SUBNETWORK_ID_PALW_OPENING_CALL, SUBNETWORK_ID_PALW_RECEIPT, SUBNETWORK_ID_PALW_REFUTATION, SUBNETWORK_ID_PALW_STEP_CONVICTION,
    SubnetworkId,
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

/// An executor-equivocation certificate — the ONE PALW offence that may slash at acceptance.
///
/// The rule is inherited verbatim from the VLT layer below
/// ([`crate::vlt::ComputeFraudKind::ContradictoryVerification`]): only an objectively provable
/// offence may slash, because slashing on an unprovable claim is worse than not slashing at all —
/// it lets any bonded party burn any other party's stake. Two signatures from one bonded key over
/// one job with different roots cannot both be true, and proving it requires no re-execution and
/// no lookup beyond the accused bond's own public key.
///
/// Class DIVERGENCE — the same certificate shape with two *different* signers — is deliberately
/// NOT carried here. It refutes the class rather than identifying an author, so under ADR-0027 P1
/// it may only freeze, never slash; and verifying it needs both signers' keys, where this kind
/// needs exactly one. It gets its own kind when the freeze machinery exists.
pub const PALW_CARRIAGE_KIND_EQUIVOCATION: u8 = 0x07;

/// An arithmetic conviction: the executor signed a trace root, and one step under that root is
/// provably not what the class's kernel computes.
///
/// The SECOND objectively-provable PALW offence, and the one ADR-0028 §6 names as Stage 2's
/// prerequisite. It became provable at acceptance only when the kernel catalog closed for a
/// class (ADR-0039 / ADR-0040): the adjudicator recomputes ONE step from opened tiles — a
/// bounded CPU primitive, no model, no GPU — so a full node can convict without ever running an
/// inference. On a class whose catalog is open the same evidence terminates `Unadjudicable`,
/// which is why coverage is an activation gate rather than a quality metric.
pub const PALW_CARRIAGE_KIND_STEP_CONVICTION: u8 = 0x08;

/// A move in a bisection ladder — the degraded path, when the miner withheld the intermediate
/// state the direct route needs to open.
///
/// Unlike the two conviction kinds, a ladder move decides nothing by itself: it is one turn in a
/// game whose OUTCOME is a no-show offence or a terminal one-step check. So this kind slashes
/// nobody at acceptance; it advances a session, and the session's own terminal handoff is what
/// reaches a bond.
pub const PALW_CARRIAGE_KIND_BISECT_MOVE: u8 = 0x09;

/// A verification receipt — the bonded bet that licenses a block's work to ramp (ADR-0038 B).
pub const PALW_CARRIAGE_KIND_RECEIPT: u8 = 0x0A;

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
    #[error("an equivocation certificate must have ONE signer; two signers is class divergence, which may freeze but never slash")]
    EquivocationNotOneSigner,
    #[error("the resolved bond {resolved:?} is not the accused bond {accused:?}")]
    EquivocationWrongBondRecord { resolved: TransactionOutpoint, accused: TransactionOutpoint },
    #[error(
        "the accused bond's validator key hash does not match the equivocating executor_id — a certificate may only accuse its own signer's bond"
    )]
    EquivocationBondNotTheSigner,
    #[error("the accused bond is not active at the point of view — a slash may not reach a bond that is not at risk")]
    EquivocationBondInactive,
    #[error("the certificate does not prove an equivocation: {0}")]
    EquivocationNotProven(String),
    #[error(
        "the attestation and the refutation name different job contexts — the certificate accuses one execution and refutes another"
    )]
    StepConvictionContextMismatch,
    #[error("the attestation signs a different trace root than the refutation binds")]
    StepConvictionRootMismatch,
    #[error("the attestation does not stand behind the composite execution root the refutation refutes")]
    StepConvictionCommittedRootMismatch,
    #[error("the carriage names a different filing bond than the attestation it carries signs for")]
    AttestationBondMismatch,
    #[error("the carriage names a different commitment than the attestation it carries signs for")]
    AttestationCommitmentMismatch,
    #[error("the step conviction does not prove a fault: {0}")]
    StepConvictionNotProven(String),
    /// ADR-0038 I10: this build's catalog cannot decide the refuted step.
    ///
    /// NOT a conviction and NOT a challenger fault — it is a fact about the accused class's
    /// catalog coverage, which is why it is its own variant rather than a
    /// [`Self::StepConvictionNotProven`] string. A caller that cannot tell the two apart cannot
    /// arm the class freeze I10 requires, and a class whose disputes are all undecidable is
    /// exactly the one that must stop minting.
    #[error("this build's catalog cannot decide the refuted step — Unadjudicable: nobody is slashed and the class must freeze")]
    StepUnadjudicable,
    #[error("bisection space {got} is outside the openable range — no ladder could be played over it")]
    BisectSpaceOutOfRange { got: u64 },
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
    ///
    /// Duplicated at carriage level ONLY as the dedup/index key, exactly like `attester_id` and
    /// `bond_outpoint` below, and like them the equality against the SIGNED
    /// `attestation.committed_root` is enforced at admission so it cannot drift. Consumers should
    /// nevertheless join on the signed copy: this field being free input is what made a copied
    /// attestation mint for zero work before the equality existed.
    pub commitment_root: Hash64,
    pub attestation: PalwExecutionAttestationV1,
    /// Must equal `attestation.executor_id`; duplicated at carriage level ONLY as the explicit
    /// dedup-key component, and the equality is enforced so it cannot drift.
    pub attester_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
}

/// An executor-equivocation certificate, carried, naming the bond it accuses.
///
/// The accused bond outpoint is at carriage level and is the **unique** key the slash targets.
/// The inner attestations carry only `executor_id`, which is a validator-key hash and is
/// explicitly not unique — nothing in consensus binds a validator key to a single bond. Resolving
/// a payee or a slash target by that hash is how a process-random map iteration decides who pays
/// (mainnet-readiness audit blocker 5); an outpoint cannot do that.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEquivocationCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    /// The bond this certificate accuses — the slash target, and the only key whose signature
    /// can prove the offence.
    pub accused_bond_outpoint: TransactionOutpoint,
    /// The two contradictory attestations plus the job context they both bind.
    pub certificate: PalwClassContradictionCertificateV1,
}

/// An arithmetic conviction, carried: authorship plus falsity, each proved separately.
///
/// The two halves are what make this slashable, and neither is sufficient alone:
///
/// * **Authorship** — the accused's ML-DSA-87 attestation over `(job_context_hash,
///   full_logits_trace_root)`. Without it a refutation proves that *some* execution was wrong,
///   not that *this bond* claimed it, and slashing on that is the "any bonded party can burn any
///   other party's stake" failure the VLT layer already refuses.
/// * **Falsity** — the step refutation, whose binding must carry the SAME job context and trace
///   root the attestation signed. Without that equality the certificate proves a lie about a
///   different execution than the one it accuses.
///
/// The accused bond outpoint rides at carriage level, as the unique slash key, for the same
/// reason as [`PalwEquivocationCarriageV1`]: `executor_id` is a validator-key hash and nothing
/// binds it to a single bond.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepConvictionCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    /// The bond this conviction accuses — the slash target.
    pub accused_bond_outpoint: TransactionOutpoint,
    /// The accused's signed claim: this is the execution it stands behind.
    pub attestation: PalwExecutionAttestationV1,
    /// The proof that one step of that execution is arithmetically wrong.
    pub refutation: crate::palw_step_refute::PalwExecutionStepRefutationV1,
}

/// Authorship of an execution, for a producer that **withheld** it (external audit P0-9 item 3).
///
/// `adjudicate_step_conviction_carriage_v1` accepts only a signed `PalwExecutionAttestationV1` as
/// its authorship half. The bisection ladder exists precisely for a producer that published a root
/// and withheld the execution behind it — and such a producer has signed no attestation, so the
/// ladder can never terminate in a conviction object that adjudicator will accept. A terminal that
/// charged the challenger for not filing what it structurally cannot file would be fail-open.
///
/// The block's own commitment is the authorship it does have: the executor signed
/// `PalwBlockCommitmentV1::message` over the payload AND the exact attempt, and that signature
/// names the trace root and the bond. This is the pure decision over that evidence.
///
/// **The signature is re-verified here rather than taken from admission**, for the reason the
/// attestation arm states about itself: this feeds a SLASH decision, and "admission ran first" is a
/// property of the current wiring, not of this function's contract. The attempt fields travel with
/// the evidence for the same reason — a digest this function cannot rebuild is one it cannot check.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwWithheldAuthorshipV1 {
    /// The commitment the accused block announced.
    pub commitment: crate::palw_block_commitment::PalwBlockCommitmentV1,
    /// The attempt the signature covers. Without all three the digest cannot be rebuilt, and a
    /// signature over one attempt would otherwise be replayable onto another.
    pub pre_pow_hash: Hash64,
    pub timestamp: u64,
    pub nonce: u64,
}

impl PalwWithheldAuthorshipV1 {
    /// Does this establish that `accused_bond` stands behind `refuted_trace_root`?
    ///
    /// Four conjuncts, and dropping any one of them admits a different forgery:
    ///
    /// * the commitment names the accused bond — otherwise a conviction slashes a bond that signed
    ///   nothing about this execution;
    /// * the commitment's trace root IS the refuted one — otherwise an honest block's signature
    ///   authorises a conviction over some other execution entirely;
    /// * the signature verifies under the accused bond's own key and the block-commitment domain —
    ///   the domain because a signature is only evidence about the family it was made for;
    /// * the bond is active at the point of view — the same liveness rule every other slash path
    ///   applies, so an already-slashed or unbonded party is not convicted twice.
    pub fn establishes_authorship_v1<F>(
        &self,
        accused_bond: &StakeBondRecord,
        refuted_trace_root: &Hash64,
        chain_network_id: &[u8],
        pov_daa_score: u64,
        verify_signature: F,
    ) -> bool
    where
        F: Fn(&[u8], &Hash, &[u8], &[u8]) -> bool,
    {
        if self.commitment.executor_bond_outpoint != accused_bond.bond_outpoint {
            return false;
        }
        if self.commitment.trace_root != *refuted_trace_root {
            return false;
        }
        if !crate::dns_finality::is_bond_active_at(accused_bond, pov_daa_score) {
            return false;
        }
        let digest = self.commitment.message(chain_network_id, self.pre_pow_hash, self.timestamp, self.nonce);
        verify_signature(
            &accused_bond.validator_pubkey,
            &digest,
            &self.commitment.signature,
            crate::palw_block_commitment::PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT,
        )
    }
}

/// One move in a bisection ladder, carried.
///
/// **No deadline field, deliberately.** The rung clock is `accepted_daa + w_round` — the DAA of
/// the block that accepted this move plus the class's registered window. Carrying a deadline is
/// what let the moving party set its opponent's clock to one DAA and win by expiry, which the
/// state machine no longer accepts and which this wire form therefore cannot express.
///
/// The challenger's bond outpoint rides every move: opening a ladder obliges a bonded party, so a
/// baseless dispute costs its filer rather than merely wasting the responder's windows. Both
/// parties' identities are already inside the session id, so a move cannot be replayed into
/// another session.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBisectMoveCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    /// The bonded challenger who opened this ladder — the party a baseless dispute charges.
    pub challenger_bond_outpoint: TransactionOutpoint,
    pub body: PalwBisectMoveBodyV1,
}

/// The three turns of the game, in one type so a relay cannot admit half of it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwBisectMoveBodyV1 {
    /// Opens a dispute over an index space. The session id is derived, never carried: it is a
    /// function of the job, the committed root, both parties and the space, so two openings of
    /// the same dispute cannot disagree about which session they are.
    Open {
        job_context_hash: Hash64,
        committed_root: Hash64,
        challenger_id: Hash64,
        responder_id: Hash64,
        /// **The responder's BOND**, and the reason it sits beside `responder_id` rather than
        /// replacing it (external audit P0-9).
        ///
        /// `responder_id` is a validator key hash, which `dns_finality` states is not unique to a
        /// bond. So no ladder outcome could name an executor bond to slash: a conviction reached
        /// through the ladder had no unambiguous target, and the whole dispute could only ever end
        /// in nobody being charged. The outpoint is unique by construction and is the key the
        /// panel, the receipts, the credit walk and the direct conviction route all already use.
        ///
        /// Both are kept because they answer different questions and a consumer that conflates them
        /// is the bug this closes: the outpoint says WHICH STAKE answers for the move, the key hash
        /// says WHOSE SIGNATURE must cover it, and a bond re-delegated to another key moves one
        /// without the other.
        responder_bond_outpoint: crate::tx::TransactionOutpoint,
        space: crate::palw_bisect::PalwBisectSpaceV1,
        space_size: u64,
    },
    /// The responder discloses the pinned midpoint's state commitment.
    Disclosure(crate::palw_bisect::PalwBisectDisclosureV1),
    /// The challenger agrees or disagrees, narrowing the interval.
    Verdict(crate::palw_bisect::PalwBisectVerdictV1),
}

/// A verification receipt, carried.
///
/// The receipt already names its verifier's bond outpoint, so unlike the conviction kinds the
/// carriage adds no accused party — a receipt accuses nobody. It is a claim about what the filer
/// saw, staked on the filer's own bond, and its whole effect is to let a block's pwu ramp.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReceiptCarriageV1 {
    /// = [`PALW_CARRIAGE_VERSION_V1`].
    pub version: u16,
    pub receipt: crate::palw_receipt::PalwVerificationReceiptV1,
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

// ---------------------------------------------------------------------------------------------
// ADR-0032 Phase E2 — the audit-call bond, as a bond
// ---------------------------------------------------------------------------------------------

/// ADR-0032 Phase E2: what may spend an audit-call bond UTXO (the opening-call carriage
/// transaction's output 0) at a given moment. The three states are the ADR's spend gate,
/// verbatim: unspendable before resolution; back to the caller after `W_answer +
/// settlement` if the answer never came (the miner's `DATA_WITHHOLDING` offense stands
/// separately — the bond returning does not absolve it); committed to the slash flow
/// (burn + answerer compensation, Stage-2 machinery) the moment an answer was accepted in
/// the window — an answered caller never gets its stake back directly, which is what makes
/// an abusive audit cost something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwAuditBondDispositionV1 {
    /// Before resolution: unspendable by anyone.
    Locked,
    /// No answer inside `W_answer`, settlement passed: spendable by the caller.
    CallerReturn,
    /// Answered in the window: spendable only INTO the slash flow, never by the caller.
    SlashFlowOnly,
}

/// The E2 disposition of one audit-call bond. A LATE answer (accepted past the deadline)
/// does not commit the caller's bond — the offense window had already closed against the
/// miner; the answer is evidence for that offense, not a claim on the caller's stake.
pub fn palw_audit_bond_disposition_v1(
    call_accepted_daa: u64,
    answer_accepted_daa: Option<u64>,
    now_daa: u64,
    w_answer: u64,
    settlement_slack: u64,
) -> PalwAuditBondDispositionV1 {
    let answer_deadline = call_accepted_daa.saturating_add(w_answer);
    match answer_accepted_daa {
        Some(answered) if answered <= answer_deadline => PalwAuditBondDispositionV1::SlashFlowOnly,
        _ => {
            if now_daa > answer_deadline.saturating_add(settlement_slack) {
                PalwAuditBondDispositionV1::CallerReturn
            } else {
                PalwAuditBondDispositionV1::Locked
            }
        }
    }
}

/// The audit-call bond slot: an opening-call carriage transaction's output 0, when it has
/// one. Recognition only — whether the value meets `F_audit` is a Stage-2 admission fact
/// (economic-simulation-gated), and a call WITHOUT outputs simply carries no bond (Stage-1
/// admission deliberately unchanged: adding an output requirement there would be an
/// unfenced consensus change).
pub fn palw_audit_call_bond_outpoint(tx: &Transaction) -> Option<TransactionOutpoint> {
    if palw_carriage_tx_kind(&tx.subnetwork_id) != Some(PALW_CARRIAGE_KIND_OPENING_CALL) || tx.outputs.is_empty() {
        return None;
    }
    Some(TransactionOutpoint::new(tx.id(), 0))
}

/// ADR-0032 Phase E2, the spend gate in the ADR-0016 shape: every transaction input that
/// spends a known audit-call bond outpoint must be permitted by that bond's disposition.
/// `resolve` is the store oracle (closure-injected, the registry-independent discipline):
/// it answers "this outpoint is an audit-call bond accepted at DAA X, answered at Y/never"
/// from the Stage-1 carriage store, or `None` for outpoints that are no such bond.
///
/// Only `CallerReturn` admits a plain spend. `SlashFlowOnly` is refused HERE even for the
/// slash flow — the Stage-2 slash transaction shape does not exist yet, and refusing
/// everything is the fail-closed reading of "spendable INTO the slash flow" until it does.
pub fn palw_audit_bond_spend_gate(
    txs: &[Transaction],
    resolve: impl Fn(&TransactionOutpoint) -> Option<(u64, Option<u64>)>,
    now_daa: u64,
    w_answer: u64,
    settlement_slack: u64,
    activated: bool,
) -> Result<(), (TransactionId, TransactionOutpoint)> {
    if !activated {
        return Ok(());
    }
    for tx in txs {
        for input in tx.inputs.iter() {
            let Some((call_daa, answer_daa)) = resolve(&input.previous_outpoint) else {
                continue;
            };
            let disposition = palw_audit_bond_disposition_v1(call_daa, answer_daa, now_daa, w_answer, settlement_slack);
            if disposition != PalwAuditBondDispositionV1::CallerReturn {
                return Err((tx.id(), input.previous_outpoint));
            }
        }
    }
    Ok(())
}

impl PalwCarriedEvidenceV1 {
    /// Whether this evidence stands against the given commitment — the ADR-0033 credit
    /// gate's "refutation against C" join. A legs refutation names the composite execution
    /// root it opens against; a trace-summary refutation names the v2 logits root (which IS
    /// the committed root for a bare-v2 class).
    ///
    /// # Naming a root is not refuting it
    ///
    /// This used to be that comparison alone, so ANY well-formed carriage quoting a target's root
    /// voided that block's credit — no bond, no signature, no proof, and the carriage is relayable,
    /// so denying an honest producer its mint cost one transaction fee (audit B9). "Adjudicating it
    /// is the slash path's business" was the reasoning, and it does not hold for a gate whose
    /// output is money: the gate is the only thing standing between a fabricated denial and a lost
    /// reward.
    ///
    /// Evidence must now PROVE a fault against this exact commitment. Both checkers are
    /// self-contained — no oracle, no chain state — and both begin by recomputing the announced
    /// root from the carried transparent preimage, so a filer that does not hold the target's real
    /// execution decomposition cannot produce evidence that passes. `NoFaultFound` is the answer
    /// for a record that names the right root and proves nothing, and that is exactly the
    /// fabricated-denial case.
    ///
    /// The conservative direction is preserved where it matters: this returns `true` only on a
    /// proven fault, and a proven fault is the one case where refusing to mint is certainly right.
    pub fn refutes(&self, committed_root: &Hash64, logits_root: &Hash64) -> bool {
        match self {
            PalwCarriedEvidenceV1::Legs(legs) => {
                legs.binding.committed_execution_root == *committed_root && crate::palw_legs::check_legs_refutation_v1(legs).is_ok()
            }
            PalwCarriedEvidenceV1::Summary(summary) => {
                summary.committed_trace_root == *logits_root && crate::palw_slash::check_trace_summary_refutation_v1(summary).is_ok()
            }
        }
    }
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
    Equivocation(PalwEquivocationCarriageV1),
    StepConviction(PalwStepConvictionCarriageV1),
    BisectMove(PalwBisectMoveCarriageV1),
    Receipt(PalwReceiptCarriageV1),
}

impl PalwCarriageV1 {
    pub fn kind_byte(&self) -> u8 {
        match self {
            PalwCarriageV1::Commitment(_) => PALW_CARRIAGE_KIND_COMMITMENT,
            PalwCarriageV1::Attestation(_) => PALW_CARRIAGE_KIND_ATTESTATION,
            PalwCarriageV1::OpeningCall(_) => PALW_CARRIAGE_KIND_OPENING_CALL,
            PalwCarriageV1::OpeningAnswer(_) => PALW_CARRIAGE_KIND_OPENING_ANSWER,
            PalwCarriageV1::Refutation(_) => PALW_CARRIAGE_KIND_REFUTATION,
            PalwCarriageV1::Equivocation(_) => PALW_CARRIAGE_KIND_EQUIVOCATION,
            PalwCarriageV1::StepConviction(_) => PALW_CARRIAGE_KIND_STEP_CONVICTION,
            PalwCarriageV1::BisectMove(_) => PALW_CARRIAGE_KIND_BISECT_MOVE,
            PalwCarriageV1::Receipt(_) => PALW_CARRIAGE_KIND_RECEIPT,
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
        PalwCarriageV1::Equivocation(e) => borsh::to_vec(e),
        PalwCarriageV1::StepConviction(c) => borsh::to_vec(c),
        PalwCarriageV1::BisectMove(m) => borsh::to_vec(m),
        PalwCarriageV1::Receipt(r) => borsh::to_vec(r),
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
        PALW_CARRIAGE_KIND_EQUIVOCATION => PalwCarriageV1::Equivocation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_STEP_CONVICTION => PalwCarriageV1::StepConviction(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_BISECT_MOVE => PalwCarriageV1::BisectMove(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_RECEIPT => PalwCarriageV1::Receipt(borsh::from_slice(body).map_err(decode_err)?),
        other => return Err(PalwCarriageError::UnknownKind(other)),
    };
    Ok(Some(carriage))
}

// ---------------------------------------------------------------------------------------------
// Stateless validation — the future Stage-1 admission validators, verbatim
// ---------------------------------------------------------------------------------------------

/// The acceptance-stage adjudication of an equivocation certificate: **the one PALW path that
/// may cost somebody their bond.**
///
/// Returns the outpoint to slash, or an error. There is no third answer and no "probably": this
/// is the objectively-provable line the VLT layer already draws, and the reason it is drawn is
/// that slashing on an unprovable claim lets any bonded party burn any other party's stake.
///
/// The order of the checks is the security argument, so it is worth reading as one:
///
/// 1. **The record is the accused record.** A caller that resolved the wrong bond is refused
///    rather than trusted, so a lookup bug upstream cannot become a slash of the wrong party.
/// 2. **The accused bond's own validator key is the equivocating signer.** Without this, a
///    certificate proving *someone* equivocated could be pointed at *anyone's* bond — the
///    signatures would still verify against the signer's key while the outpoint named a victim.
/// 3. **The bond is active at the point of view.** A bond that is not at risk cannot be taken.
/// 4. **Both signatures verify under that one key**, over the two messages the certificate itself
///    reconstructs — delegated to [`adjudicate_class_contradiction_v1`], which also enforces that
///    both attestations bind the same job context and that the roots actually differ.
/// 5. **The verdict is equivocation, not divergence.** Step 2 already implies it; checking it
///    anyway means a future change to either function cannot silently make divergence slashable.
///
/// `verify_signature` receives `(public_key, digest, signature)` and must verify ML-DSA-87 under
/// [`crate::palw_slash::PALW_S_MLDSA87_ATTESTATION_CONTEXT`]. Crypto stays outside consensus-core
/// (`verify_mldsa87_with_context` lives in `crypto/txscript`), which is also what lets every
/// branch above be unit-tested without a keypair.
pub fn adjudicate_equivocation_carriage_v1<F>(
    carriage: &PalwEquivocationCarriageV1,
    accused_bond: &StakeBondRecord,
    pov_daa_score: u64,
    chain_network_id: &[u8],
    verify_signature: F,
) -> Result<TransactionOutpoint, PalwCarriageError>
where
    F: Fn(&[u8], &Hash, &[u8], &[u8]) -> bool,
{
    require_version(carriage.version)?;
    if accused_bond.bond_outpoint != carriage.accused_bond_outpoint {
        return Err(PalwCarriageError::EquivocationWrongBondRecord {
            resolved: accused_bond.bond_outpoint,
            accused: carriage.accused_bond_outpoint,
        });
    }
    let signer = carriage.certificate.attestation_a.executor_id;
    if signer != accused_bond.validator_pubkey_hash || carriage.certificate.attestation_b.executor_id != signer {
        return Err(PalwCarriageError::EquivocationBondNotTheSigner);
    }
    if !crate::dns_finality::is_bond_active_at(accused_bond, pov_daa_score) {
        return Err(PalwCarriageError::EquivocationBondInactive);
    }
    let verdict = adjudicate_class_contradiction_v1(
        &carriage.certificate,
        chain_network_id,
        |digest, attestation: &crate::palw_slash::PalwExecutionAttestationV1| {
            verify_signature(
                &accused_bond.validator_pubkey,
                digest,
                &attestation.signature,
                crate::palw_slash::PALW_S_MLDSA87_ATTESTATION_CONTEXT,
            )
        },
    )
    .map_err(|e| PalwCarriageError::EquivocationNotProven(e.to_string()))?;
    match verdict.kind {
        PalwClassContradictionKindV1::ExecutorEquivocation { .. } => Ok(carriage.accused_bond_outpoint),
        PalwClassContradictionKindV1::ClassDivergence { .. } => Err(PalwCarriageError::EquivocationNotOneSigner),
    }
}

/// The acceptance-stage adjudication of an arithmetic conviction — the SECOND path that may cost
/// a bond, and the one ADR-0028 §6 makes Stage 2's prerequisite.
///
/// The check order is the security argument again, and it is the same shape as
/// [`adjudicate_equivocation_carriage_v1`]'s because the same two things must be established:
///
/// 1. **The record is the accused record**, so an upstream lookup bug cannot slash the wrong party.
/// 2. **The accused bond's own validator key is the attester**, so a genuine refutation cannot be
///    pointed at somebody else's bond.
/// 3. **The bond is active** at the point of view.
/// 4. **The attestation verifies** under that key — this is the authorship half.
/// 5. **The step refutation convicts** — the falsity half, delegated to
///    [`crate::palw_step_refute::check_execution_step_refutation_v1`], which recomputes ONE step
///    from opened tiles and compares exact bytes.
///
/// Shape admission has already established that both halves are about the same execution (same
/// job context, same logits root, same COMPOSITE root), so nothing here can convict a bond of a
/// lie told about a different job. The composite-root conjunct is repeated in this function
/// anyway: for a long time it did not exist at all, and matching only the job and the logits leg
/// left every other part of the refuted root as free filer input.
///
/// `Unadjudicable` is NOT a conviction and never slashes: it means this build's catalog cannot
/// decide the question, which is a fact about the class's coverage rather than about the accused
/// (ADR-0038 A4). That is the whole reason the catalog had to close for BASE-0 before this path
/// was worth having.
pub fn adjudicate_step_conviction_carriage_v1<F>(
    carriage: &PalwStepConvictionCarriageV1,
    accused_bond: &StakeBondRecord,
    pov_daa_score: u64,
    chain_network_id: &[u8],
    weights: &dyn crate::palw_step_refute::PalwWeightOracleV1,
    verify_signature: F,
) -> Result<TransactionOutpoint, PalwCarriageError>
where
    F: Fn(&[u8], &Hash, &[u8], &[u8]) -> bool,
{
    require_version(carriage.version)?;
    if accused_bond.bond_outpoint != carriage.accused_bond_outpoint {
        return Err(PalwCarriageError::EquivocationWrongBondRecord {
            resolved: accused_bond.bond_outpoint,
            accused: carriage.accused_bond_outpoint,
        });
    }
    if carriage.attestation.executor_id != accused_bond.validator_pubkey_hash {
        return Err(PalwCarriageError::EquivocationBondNotTheSigner);
    }
    // And the EXACT bond, not merely one of the signer's. `validator_pubkey_hash` is not unique, so
    // matching on it alone let a conviction name either of two bonds sharing a key — including the
    // one that had signed nothing about this execution. Since generation 3 the attestation names
    // the bond it is made by, so the accused is checkable rather than inferred.
    if carriage.attestation.bond_outpoint != accused_bond.bond_outpoint {
        return Err(PalwCarriageError::EquivocationBondNotTheSigner);
    }
    if !crate::dns_finality::is_bond_active_at(accused_bond, pov_daa_score) {
        return Err(PalwCarriageError::EquivocationBondInactive);
    }
    // Same rule as the equivocation adjudicator: the network identity comes from the CHAIN. Taking
    // it from the evidence's own job context let a filer choose it, so an attestation honestly
    // signed for devnet verified here and slashed a MAINNET bond (audit).
    if carriage.refutation.binding.job_context.network_id.as_slice() != chain_network_id {
        return Err(PalwCarriageError::EquivocationNotProven("the evidence names a different network than this chain".into()));
    }
    // Re-checked HERE and not left to admission alone: this function IS the slash decision, and
    // it is reachable from the weight path too. Admission running first is a property of the
    // current wiring, not of this function's contract, and a conviction whose signed composite
    // root is not the refuted one must never slash regardless of who called.
    if carriage.attestation.committed_root != carriage.refutation.binding.committed_execution_root {
        return Err(PalwCarriageError::EquivocationNotProven(
            "the attestation does not stand behind the refuted execution root".into(),
        ));
    }
    let message = carriage.attestation.message(chain_network_id);
    if !verify_signature(
        &accused_bond.validator_pubkey,
        &message,
        &carriage.attestation.signature,
        crate::palw_slash::PALW_S_MLDSA87_ATTESTATION_CONTEXT,
    ) {
        return Err(PalwCarriageError::EquivocationNotProven("attestation signature does not verify".into()));
    }
    // `Unadjudicable` is kept apart from every other failure here, and the distinction is the
    // whole of ADR-0038 I10. "This build's catalog cannot decide the step" is a fact about the
    // accused CLASS's coverage; "this evidence is junk" is a fact about the challenger. Flattening
    // both into one string — which this line used to do — discarded the first, so no verdict could
    // ever arm the class freeze the invariant requires.
    //
    // Neither convicts. The difference is what a caller may conclude afterwards, not who is
    // slashed: `Unadjudicable` slashes nobody AND freezes the class (I10), while an unproven
    // conviction slashes nobody and freezes nothing.
    crate::palw_step_refute::check_execution_step_refutation_v1(&carriage.refutation, weights).map_err(|e| match e {
        crate::palw_step_refute::PalwStepRefuteError::Unadjudicable => PalwCarriageError::StepUnadjudicable,
        other => PalwCarriageError::StepConvictionNotProven(other.to_string()),
    })?;
    Ok(carriage.accused_bond_outpoint)
}

/// Everything decidable from the bytes alone, per kind. See the module doc for what stateless
/// does NOT mean: a valid object can still be a lie; it cannot be *incoherent*.
pub fn validate_palw_carriage_v1(carriage: &PalwCarriageV1) -> Result<(), PalwCarriageError> {
    match carriage {
        PalwCarriageV1::Commitment(c) => validate_commitment_carriage(c),
        PalwCarriageV1::Receipt(r) => {
            require_version(r.version)?;
            r.receipt.validate_shape().map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
            Ok(())
        }
        PalwCarriageV1::BisectMove(m) => {
            require_version(m.version)?;
            // Shape only. Whether the move is LEGAL — right turn, right round, right midpoint,
            // within the round budget — is the ladder's own question and needs the session state,
            // which is chain state. Admission refuses only what is incoherent on its face: a
            // space no ladder could ever open.
            if let PalwBisectMoveBodyV1::Open { space_size, .. } = &m.body
                && !(2..=crate::palw_bisect::PALW_BISECT_MAX_SPACE).contains(space_size)
            {
                return Err(PalwCarriageError::BisectSpaceOutOfRange { got: *space_size });
            }
            Ok(())
        }
        PalwCarriageV1::StepConviction(c) => {
            require_version(c.version)?;
            // Shape only. Whether the step is actually wrong needs the kernel catalog and the
            // accused bond's key, both chain state; admission's job is to refuse the incoherent
            // so adjudication never sees a certificate whose two halves are about different
            // executions.
            c.attestation.validate_shape().map_err(|x| PalwCarriageError::Inner(x.to_string()))?;
            if c.attestation.job_context_hash != c.refutation.binding.job_context.context_hash() {
                return Err(PalwCarriageError::StepConvictionContextMismatch);
            }
            if c.attestation.full_logits_trace_root != c.refutation.binding.full_logits_trace_root {
                return Err(PalwCarriageError::StepConvictionRootMismatch);
            }
            // The COMPOSITE root, and this is the conjunct that makes the other two mean
            // something. A step refutation refutes `committed_execution_root`, whose other parts
            // — `step_leaf_count`, `step_merkle_root`, `checkpoint_count`, `checkpoint_merkle_root`,
            // `activation_leg_root`, `state_chunk_map_id`, `checkpoint_profile` — are the FILER's
            // input and are only ever checked against each other by `verify_binding`. Matching the
            // job context and the logits leg therefore did not establish that the accused stands
            // behind the refuted object: a filer could take any genuine attestation, rebuild a
            // self-consistent binding around the same job with a non-canonical `checkpoint_count`,
            // and collect a structural conviction against a bond that had done nothing wrong (the
            // shape pass returns a verdict from the binding alone, before any opening is read).
            // Since generation 2 the accused signs the composite root, so the tie is checkable.
            if c.attestation.committed_root != c.refutation.binding.committed_execution_root {
                return Err(PalwCarriageError::StepConvictionCommittedRootMismatch);
            }
            Ok(())
        }
        PalwCarriageV1::Equivocation(e) => {
            require_version(e.version)?;
            // Shape only, and deliberately NOT the contradiction itself: proving it needs the
            // accused bond's public key, which is chain state. Admission's job is to refuse the
            // incoherent, so that `adjudicate_equivocation_carriage_v1` is never handed a
            // certificate whose two attestations disagree about which job they are about.
            e.certificate.attestation_a.validate_shape().map_err(|x| PalwCarriageError::Inner(x.to_string()))?;
            e.certificate.attestation_b.validate_shape().map_err(|x| PalwCarriageError::Inner(x.to_string()))?;
            // One signer is what makes this kind slashable at all; two signers is class
            // divergence, which may only freeze and is not carried here.
            if e.certificate.attestation_a.executor_id != e.certificate.attestation_b.executor_id {
                return Err(PalwCarriageError::EquivocationNotOneSigner);
            }
            Ok(())
        }
        PalwCarriageV1::Attestation(a) => {
            require_version(a.version)?;
            a.attestation.validate_shape().map_err(|e| PalwCarriageError::Inner(e.to_string()))?;
            if a.attester_id != a.attestation.executor_id {
                return Err(PalwCarriageError::AttesterMismatch);
            }
            // The carriage names the filing bond and, since generation 3, so does the SIGNED
            // attestation. Two copies of one fact is the dual-source surface this family's own doc
            // warns about — checkable against nothing — unless they are required to agree. Here they
            // are, so the consumer may index on the carriage's copy while the signature covers the
            // same value. Without this, the signed outpoint would be decorative: a filer could sign
            // for bond A and carry bond B, and the payee is read from the carriage.
            if a.bond_outpoint != a.attestation.bond_outpoint {
                return Err(PalwCarriageError::AttestationBondMismatch);
            }
            // THE THIRD PAIR, and the one that was missed when the two above were written.
            //
            // The carriage names the commitment this attestation stands behind, and so does the
            // SIGNED attestation (`committed_root`, in the generation-3 preimage). The credit walk
            // joined on the CARRIAGE's copy, which is free filer input, and never read the signed
            // one — so an attacker could take any honest validator's published attestation, change
            // only this one unsigned field to point at its OWN fabricated commitment, and have it
            // credit: the signature still verifies because every field inside it is untouched.
            //
            // That minted `base(C)` for zero inference, with the attacker's bond never at risk
            // because it had committed nothing false. The same reasoning as the two checks above,
            // applied to the pair it was not applied to.
            if a.commitment_root != a.attestation.committed_root {
                return Err(PalwCarriageError::AttestationCommitmentMismatch);
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
                    if summary.version != crate::palw_slash::PALW_S_OBJECT_VERSION_V3 {
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
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_EQUIVOCATION {
        Some(PALW_CARRIAGE_KIND_EQUIVOCATION)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_STEP_CONVICTION {
        Some(PALW_CARRIAGE_KIND_STEP_CONVICTION)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_BISECT_MOVE {
        Some(PALW_CARRIAGE_KIND_BISECT_MOVE)
    } else if *subnetwork_id == SUBNETWORK_ID_PALW_RECEIPT {
        Some(PALW_CARRIAGE_KIND_RECEIPT)
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
        PALW_CARRIAGE_KIND_EQUIVOCATION => PalwCarriageV1::Equivocation(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_STEP_CONVICTION => PalwCarriageV1::StepConviction(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_BISECT_MOVE => PalwCarriageV1::BisectMove(borsh::from_slice(body).map_err(decode_err)?),
        PALW_CARRIAGE_KIND_RECEIPT => PalwCarriageV1::Receipt(borsh::from_slice(body).map_err(decode_err)?),
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
    /// **The chain block that accepted the carrier.**
    ///
    /// Without it a row cannot say which chain it belongs to, and a DAA score is not a chain
    /// identifier — two competing branches both have them. `PalwResolverInputV1::carriage` is
    /// specified as "records accepted on THE CHAIN BEING EVALUATED", so a reader that took the
    /// whole store would mix evidence from branches the node reorged away from into the weight of
    /// a block on the branch it kept, with nothing in the call looking wrong. This is the field
    /// that lets a reader ask reachability instead of guessing.
    ///
    /// It costs a stored-layout change, which `PALW_CARRIAGE_SCHEMA_VERSION` already turns into an
    /// automatic discard-and-resweep rather than an operator step.
    pub accepted_block: BlockHash,
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
    accepted_block: BlockHash,
) -> Vec<(TransactionId, PalwCarriageRecord)> {
    let mut out = Vec::new();
    for tx in txs {
        let Some(kind) = palw_carriage_tx_kind(&tx.subnetwork_id) else { continue };
        if validate_palw_carriage_stage1_tx(kind, &tx.payload, &tx.outputs).is_ok() {
            out.push((tx.id(), PalwCarriageRecord { kind, accepted_daa_score, accepted_block, body: tx.payload.clone() }));
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
    use crate::palw_slash::{PALW_S_ALL_DOMAINS, PALW_S_OBJECT_VERSION_V3};
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
                version: PALW_S_OBJECT_VERSION_V3,
                executor_id: h64(0xA2),
                job_context_hash: PalwJobContextV2::from_envelope(&test_envelope(), h64(0x37)).context_hash(),
                full_logits_trace_root: h64(0x71),
                committed_root: test_binding().committed_execution_root,
                // The SAME value the carriage names below: since generation 3 admission requires it.
                bond_outpoint: outpoint(0xB2, 1),
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
                version: PALW_S_OBJECT_VERSION_V3,
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

    /// **Audit B9: naming a commitment's root is not refuting it.**
    ///
    /// `refutes` was that comparison alone, so any well-formed carriage quoting a target's root
    /// voided that block's credit — no bond, no signature, no proof — and the carriage is relayable,
    /// so denying an honest producer its mint cost one transaction fee. Evidence must prove a fault
    /// against this exact commitment.
    ///
    /// The fixture below is precisely the fabricated case: it names `h64(0xE3)` as the committed
    /// trace root while carrying a summary that recomputes to something else. Before the fix it
    /// refuted; now it does not.
    #[test]
    fn evidence_that_proves_nothing_does_not_refute() {
        let PalwCarriedEvidenceV1::Summary(summary) = refutation().evidence else { panic!("fixture is a summary") };
        let named = summary.committed_trace_root;

        // It still NAMES the root — the join is unchanged.
        assert_eq!(named, h64(0xE3));
        // ...and the adjudicator refuses it, because the carried preimage does not recompute that
        // root. A filer without the target's real execution decomposition cannot get past this.
        assert!(
            crate::palw_slash::check_trace_summary_refutation_v1(&summary).is_err(),
            "the fixture must be the fabricated case for this test to mean anything"
        );

        let evidence = PalwCarriedEvidenceV1::Summary(summary);
        assert!(!evidence.refutes(&h64(0xE3), &h64(0xE3)), "unproven evidence must not void a block's credit");
        // And it certainly does not stand against a commitment it does not even name.
        assert!(!evidence.refutes(&h64(0xAA), &h64(0xAA)));

        // The SAME rule on the legs arm: a binding that names a root it cannot recompute, carrying
        // a `Shape` claim with no shape fault to find, proves nothing.
        let binding = test_binding();
        let named = binding.committed_execution_root;
        let legs = PalwLegsRefutationV1 { binding, evidence: crate::palw_legs::PalwLegsEvidenceV1::Shape };
        assert!(crate::palw_legs::check_legs_refutation_v1(&legs).is_err(), "the legs fixture must also be the fabricated case");
        assert!(
            !PalwCarriedEvidenceV1::Legs(legs).refutes(&named, &h64(0xAA)),
            "a legs record that names the right root but proves no fault must not void credit"
        );
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
        // Re-frozen 2026-08-17: PALW-S generation 3 (the attestation gained `committed_root`, then
        // `bond_outpoint`), so the attestation payload and every payload carrying a PALW-S object
        // version moved with it.
        let golden: [(&str, &str); 5] = [
            ("commitment", "2ef18007f17d928e80c6ecf94e6e9b71eabd24ba9b879f7045a18b72665fef14"),
            ("attestation", "50a5feacbf6b5ccd608e8e40977c14b2f3c863795edfba0669799a4d817d0f07"),
            ("opening-call", "aa0328847eae54b9fab887704631f020fed2e468d2906c564c32f2577001eea1"),
            ("opening-answer", "bb106d592e6d7ce811f40471497198d230cd6611106689b70bba180f6113bea7"),
            ("refutation", "284fb2fb9fcc2c95c49b8f575a6afb8b73b5bb2762ffd0fc47bded18452cd02d"),
        ];
        for (carriage, (name, expected)) in all_five().iter().zip(golden) {
            let got = payload_hash_hex(&encode_palw_carriage_v1(carriage));
            assert_eq!(got, expected, "{name} payload moved");
        }
    }

    /// **The copied-attestation mint.** All three of the carriage's duplicated facts must agree with
    /// the signed attestation, and the commitment root is the one that was missed.
    ///
    /// An attacker takes any honest validator's PUBLISHED attestation, changes only the carriage's
    /// `commitment_root` to point at its own fabricated commitment, and files it. Every field inside
    /// the signature is untouched, so the signature still verifies; the credit walk joined on the
    /// unsigned copy; and the attacker's own bond was never at risk because it had committed nothing
    /// false. That minted `base(C)` for zero inference.
    ///
    /// The fixture used to set the two roots EQUAL, which is exactly why nothing noticed — the same
    /// vacuous-fixture shape as a bare-v2 test where two distinct roots coincide.
    #[test]
    fn a_carried_commitment_root_the_signature_does_not_name_is_inadmissible() {
        let honest = attestation();
        assert_eq!(honest.commitment_root, honest.attestation.committed_root, "the fixture is a self-consistent filing");
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::Attestation(honest.clone())).is_ok());

        // The attack: repoint ONLY the unsigned carriage field at another commitment.
        let mut repointed = honest.clone();
        repointed.commitment_root = h64(0xAD);
        assert_eq!(
            repointed.attestation, honest.attestation,
            "the attack changes nothing inside the signature — that is what made it work"
        );
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Attestation(repointed)),
            Err(PalwCarriageError::AttestationCommitmentMismatch)
        );

        // Symmetric: editing the SIGNED side is refused too, so neither copy is the trusted one.
        let mut signed_elsewhere = honest;
        signed_elsewhere.attestation.committed_root = h64(0xAD);
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Attestation(signed_elsewhere)),
            Err(PalwCarriageError::AttestationCommitmentMismatch)
        );
    }

    /// The carriage's copy of the filing bond and the SIGNED one must agree.
    ///
    /// Two copies of one fact is the dual-source surface this family's own doc warns about. The
    /// consumer indexes on the carriage's copy — that is the payee — so a filer that could sign for
    /// bond A and carry bond B would make the signed outpoint decorative and the generation-3 fix
    /// cosmetic.
    #[test]
    fn a_carried_bond_that_the_signature_does_not_name_is_inadmissible() {
        let a = attestation();
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::Attestation(a.clone())).is_ok());

        let mut swapped = a.clone();
        swapped.bond_outpoint = outpoint(0xEE, 9);
        assert_eq!(validate_palw_carriage_v1(&PalwCarriageV1::Attestation(swapped)), Err(PalwCarriageError::AttestationBondMismatch));
        // Symmetrically: editing the SIGNED side is refused too, so neither copy is the trusted one.
        let mut signed_elsewhere = a;
        signed_elsewhere.attestation.bond_outpoint = outpoint(0xEE, 9);
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Attestation(signed_elsewhere)),
            Err(PalwCarriageError::AttestationBondMismatch)
        );
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
        // 0x09 is the bisection-move kind now; 0x0A is the first byte past the family.
        assert_eq!(decode_palw_carriage_v1(b"MPALW2\x0B"), Err(PalwCarriageError::UnknownKind(0x0B)));
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

    /// A minimal, shape-valid equivocation carriage for the band round-trip. Its adjudication
    /// is covered in `equivocation_tests`; here it only has to encode and route.
    fn equivocation_for_band() -> PalwEquivocationCarriageV1 {
        let ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-devnet".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x12),
            assignment_id: h64(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: h64(0x31),
            runtime_manifest_hash: h64(0x32),
            runtime_class_id: h64(0x33),
            shape_profile_id: h64(0x34),
            trace_scheme_id: crate::palw_v2::trace_scheme_id_v2(),
            cu_ruleset_id: h64(0x36),
            tokenizer_id: h64(0x37),
            prompt_token_ids_hash: h64(0x38),
            exact_decode_tokens: 16,
            declared_prefill_tokens: 8,
            max_context_tokens: 4_096,
        };
        // A bare-v2 shape: the committed object IS the logits root, so the two move together.
        let att = |root: Hash64| PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id: h64(0xE1),
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: outpoint(0xB1, 0),
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: outpoint(0xB1, 0),
            certificate: PalwClassContradictionCertificateV1 {
                version: PALW_S_OBJECT_VERSION_V3,
                attestation_a: att(h64(0x01)),
                attestation_b: att(h64(0x02)),
                job_context: ctx,
            },
        }
    }

    /// A shape-valid step conviction for the band round-trip; its adjudication is covered in
    /// `step_conviction_tests`.
    fn step_conviction_for_band() -> PalwStepConvictionCarriageV1 {
        let refutation = crate::palw_step_refute::tests::skeleton_refutation();
        PalwStepConvictionCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: outpoint(0xB1, 0),
            attestation: PalwExecutionAttestationV1 {
                version: PALW_S_OBJECT_VERSION_V3,
                executor_id: h64(0xE1),
                job_context_hash: refutation.binding.job_context.context_hash(),
                full_logits_trace_root: refutation.binding.full_logits_trace_root,
                committed_root: refutation.binding.committed_execution_root,
                bond_outpoint: outpoint(0xB1, 0),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
            refutation,
        }
    }

    /// A shape-valid ladder opening for the band round-trip.
    fn bisect_move_for_band() -> PalwBisectMoveCarriageV1 {
        PalwBisectMoveCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            challenger_bond_outpoint: outpoint(0xC1, 0),
            body: PalwBisectMoveBodyV1::Open {
                job_context_hash: h64(0x11),
                committed_root: h64(0x22),
                challenger_id: h64(0x33),
                responder_id: h64(0x44),
                responder_bond_outpoint: crate::tx::TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0xB1; 64]), 0),
                space: crate::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                space_size: 16,
            },
        }
    }

    /// A shape-valid receipt for the band round-trip.
    fn receipt_for_band() -> PalwReceiptCarriageV1 {
        PalwReceiptCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            receipt: crate::palw_receipt::PalwVerificationReceiptV1 {
                version: crate::palw_receipt::PALW_RECEIPT_VERSION_V1,
                target_block_hash: h64(0x71),
                target_commitment_root: h64(0x72),
                execution_class_id: h64(0x73),
                // A receipt with no samples is not a receipt: shape admission requires at least
                // one, because a verifier that opened nothing verified nothing.
                sample_coordinates: vec![crate::palw_receipt::PalwSampleCoordinateV1 {
                    token_index: 0,
                    layer_index: 0,
                    node_slot: 0,
                    unit_index: 0,
                }],
                observed_roots: vec![h64(0x74)],
                verdict: crate::palw_receipt::PalwReceiptVerdictV1::Match,
                verifier_bond_outpoint: outpoint(0xC2, 0),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
        }
    }

    fn all_band_members() -> Vec<(SubnetworkId, PalwCarriageV1)> {
        let mut out: Vec<(SubnetworkId, PalwCarriageV1)> = vec![
            (SUBNETWORK_ID_PALW_COMMITMENT, PalwCarriageV1::Commitment(commitment_composite())),
            (SUBNETWORK_ID_PALW_ATTESTATION, PalwCarriageV1::Attestation(attestation())),
            (SUBNETWORK_ID_PALW_OPENING_CALL, PalwCarriageV1::OpeningCall(opening_call())),
            (SUBNETWORK_ID_PALW_OPENING_ANSWER, PalwCarriageV1::OpeningAnswer(opening_answer())),
            (SUBNETWORK_ID_PALW_REFUTATION, PalwCarriageV1::Refutation(refutation())),
            (SUBNETWORK_ID_PALW_EVIDENCE_CHUNK, PalwCarriageV1::EvidenceChunk(evidence_chunk())),
            (SUBNETWORK_ID_PALW_EQUIVOCATION, PalwCarriageV1::Equivocation(equivocation_for_band())),
            (SUBNETWORK_ID_PALW_STEP_CONVICTION, PalwCarriageV1::StepConviction(step_conviction_for_band())),
            (SUBNETWORK_ID_PALW_BISECT_MOVE, PalwCarriageV1::BisectMove(bisect_move_for_band())),
            (SUBNETWORK_ID_PALW_RECEIPT, PalwCarriageV1::Receipt(receipt_for_band())),
        ];
        debug_assert_eq!(out.len(), 10);
        out.sort_by_key(|(_, c)| c.kind_byte());
        out
    }

    /// The Stage-1 contract, pinned end to end: each new subnetwork id routes to exactly its
    /// kind byte, and the Stage-1 payload IS the Stage-0 payload minus the 7-byte magic+kind
    /// prefix — so the goldens above cover both stages and the two decode paths cannot drift.
    #[test]
    fn stage1_ids_route_and_bodies_are_stage0_minus_the_envelope() {
        for (id, carriage) in all_band_members() {
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
            SubnetworkId::from_byte(0x4A),
        ] {
            assert_eq!(palw_carriage_tx_kind(&id), None);
        }
        // An unknown kind byte is an error, not a fallthrough.
        assert_eq!(decode_palw_stage1_body(0x0B, b"anything"), Err(PalwCarriageError::UnknownKind(0x0B)));
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
        let extracted = palw_carriage_records_from_accepted_txs(&txs, 4_242, Hash64::from_u64_word(0xB10C));
        assert_eq!(extracted.len(), 1, "exactly the valid Stage-1 carrier");
        assert_eq!(extracted[0].0, txs[0].id());
        assert_eq!(
            extracted[0].1,
            PalwCarriageRecord {
                kind: PALW_CARRIAGE_KIND_ATTESTATION,
                accepted_daa_score: 4_242,
                accepted_block: Hash64::from_u64_word(0xB10C),
                body: good_body
            }
        );
        // The record's body decodes back to the object admission validated — bytes verbatim.
        assert_eq!(
            decode_palw_stage1_body(extracted[0].1.kind, &extracted[0].1.body).unwrap(),
            PalwCarriageV1::Attestation(attestation())
        );
    }

    #[test]
    fn audit_bond_disposition_covers_the_three_e2_states_and_their_edges() {
        use PalwAuditBondDispositionV1::*;
        let (w, s) = (30, 30); // two-minute w_answer + prosecution slack
        // No answer: locked through the window AND the settlement slack, inclusive.
        assert_eq!(palw_audit_bond_disposition_v1(100, None, 100, w, s), Locked);
        assert_eq!(palw_audit_bond_disposition_v1(100, None, 160, w, s), Locked); // == deadline + slack
        assert_eq!(palw_audit_bond_disposition_v1(100, None, 161, w, s), CallerReturn);
        // An answer inside the window (deadline inclusive) commits the bond forever.
        assert_eq!(palw_audit_bond_disposition_v1(100, Some(130), 10_000, w, s), SlashFlowOnly);
        assert_eq!(palw_audit_bond_disposition_v1(100, Some(100), 101, w, s), SlashFlowOnly);
        // A LATE answer does not commit the caller's bond — the miner's offense had
        // already crystallized when the deadline passed.
        assert_eq!(palw_audit_bond_disposition_v1(100, Some(131), 140, w, s), Locked);
        assert_eq!(palw_audit_bond_disposition_v1(100, Some(131), 161, w, s), CallerReturn);
    }

    #[test]
    fn audit_bond_spend_gate_admits_only_caller_return() {
        let bond_outpoint = TransactionOutpoint::new(TransactionId::from_bytes([0xCA; 64]), 0);
        let spend = Transaction::new(
            0,
            vec![crate::tx::TransactionInput::new(bond_outpoint, vec![], 0, 0)],
            vec![],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let txs = vec![spend];
        let unanswered = |_: &TransactionOutpoint| Some((100u64, None));
        // Locked: refused, and the error names the spender and the bond.
        let err = palw_audit_bond_spend_gate(&txs, unanswered, 130, 30, 30, true).unwrap_err();
        assert_eq!(err, (txs[0].id(), bond_outpoint));
        // CallerReturn: admitted.
        assert!(palw_audit_bond_spend_gate(&txs, unanswered, 161, 30, 30, true).is_ok());
        // SlashFlowOnly: refused no matter how late — fail-closed until the Stage-2
        // slash-flow transaction shape exists.
        assert!(palw_audit_bond_spend_gate(&txs, |_: &TransactionOutpoint| Some((100, Some(120))), 10_000, 30, 30, true).is_err());
        // Not a recognized bond, or the gate not activated: pass untouched.
        assert!(palw_audit_bond_spend_gate(&txs, |_: &TransactionOutpoint| None, 130, 30, 30, true).is_ok());
        assert!(palw_audit_bond_spend_gate(&txs, unanswered, 130, 30, 30, false).is_ok());
    }

    #[test]
    fn the_audit_bond_slot_is_an_opening_calls_output_zero_and_nothing_else() {
        let call_with_bond = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(1_000, Default::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_OPENING_CALL,
            0,
            vec![],
        );
        assert_eq!(palw_audit_call_bond_outpoint(&call_with_bond), Some(TransactionOutpoint::new(call_with_bond.id(), 0)));
        // A call without outputs carries no bond (Stage-1 admission deliberately unchanged).
        let bare_call = Transaction::new(0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_OPENING_CALL, 0, vec![]);
        assert_eq!(palw_audit_call_bond_outpoint(&bare_call), None);
        // A non-call carriage or native transaction is never a bond slot.
        let native =
            Transaction::new(0, vec![], vec![TransactionOutput::new(1_000, Default::default())], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        assert_eq!(palw_audit_call_bond_outpoint(&native), None);
    }
}

#[cfg(test)]
mod equivocation_tests {
    use super::*;
    use crate::dns_finality::{BondStatus, StakeBondRecord};
    use crate::palw_slash::{PALW_S_MLDSA87_ATTESTATION_CONTEXT, PALW_S_OBJECT_VERSION_V3};
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
    use crate::tx::TransactionId;

    /// The chain identity every adjudication in these tests runs under.
    const NET: &[u8] = b"misaka-devnet";

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }

    /// A key is its own bytes; a signature is the key bytes followed by the digest. Deterministic,
    /// forgeable only by knowing the key — enough to exercise every branch without a keypair, and
    /// the real verifier is supplied by the pipeline.
    fn mock_key(signer: Hash64) -> Vec<u8> {
        signer.as_byte_slice().to_vec()
    }
    fn mock_sign(key: &[u8], digest: &Hash) -> Vec<u8> {
        let mut s = key.to_vec();
        s.extend_from_slice(digest.as_bytes().as_slice());
        s
    }
    fn mock_verify(key: &[u8], digest: &Hash, signature: &[u8], _context: &[u8]) -> bool {
        signature == mock_sign(key, digest).as_slice()
    }

    fn context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: NET.to_vec(),
            job_id: h(0x11),
            job_nullifier: h(0x12),
            assignment_id: h(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: h(0x31),
            runtime_manifest_hash: h(0x32),
            runtime_class_id: h(0x33),
            shape_profile_id: h(0x34),
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: h(0x36),
            tokenizer_id: h(0x37),
            prompt_token_ids_hash: h(0x38),
            exact_decode_tokens: 16,
            declared_prefill_tokens: 8,
            max_context_tokens: 4_096,
        }
    }

    fn attested(signer: Hash64, ctx: &PalwJobContextV2, root: Hash64) -> PalwExecutionAttestationV1 {
        let mut a = PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id: signer,
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_bytes(signer.as_bytes()),
                index: 0,
            },
            signature: vec![],
        };
        let digest = a.message(&ctx.network_id);
        a.signature = mock_sign(&mock_key(signer), &digest);
        a
    }

    fn bond(signer: Hash64, outpoint: TransactionOutpoint) -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: h(0x0A0A),
            validator_pubkey_hash: signer,
            validator_pubkey: mock_key(signer),
            amount: 20_000_00000000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 1_000,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    fn carriage(signer: Hash64, accused: TransactionOutpoint, root_a: Hash64, root_b: Hash64) -> PalwEquivocationCarriageV1 {
        let ctx = context();
        PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: accused,
            certificate: PalwClassContradictionCertificateV1 {
                version: PALW_S_OBJECT_VERSION_V3,
                attestation_a: attested(signer, &ctx, root_a),
                attestation_b: attested(signer, &ctx, root_b),
                job_context: ctx,
            },
        }
    }

    /// The honest path: one bonded signer, two roots, one job — the bond is slashable.
    #[test]
    fn a_proven_equivocation_slashes_the_accused_bond() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let c = carriage(signer, accused, h(0x01), h(0x02));
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::Equivocation(c.clone())).is_ok());
        assert_eq!(adjudicate_equivocation_carriage_v1(&c, &bond(signer, accused), 100, NET, mock_verify), Ok(accused));
    }

    /// **The attack this design exists to refuse: accusing somebody else's bond.**
    ///
    /// The certificate is genuine — a real signer really did equivocate — but the outpoint names
    /// a victim. Both signatures still verify under the SIGNER's key, so a check that only
    /// verified signatures would slash the victim. Binding the accused bond's own validator key
    /// to the equivocating signer is what refuses it.
    #[test]
    fn a_genuine_certificate_cannot_be_pointed_at_an_innocent_bond() {
        let (signer, victim_outpoint) = (h(0xE1), op(0xB9));
        let victim = bond(h(0x00CE), victim_outpoint);
        let c = carriage(signer, victim_outpoint, h(0x01), h(0x02));
        assert_eq!(
            adjudicate_equivocation_carriage_v1(&c, &victim, 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationBondNotTheSigner)
        );
    }

    /// A caller that resolved the wrong record is refused rather than trusted — an upstream
    /// lookup bug must not be able to become a slash of the wrong party.
    #[test]
    fn a_mismatched_bond_record_is_refused() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let c = carriage(signer, accused, h(0x01), h(0x02));
        let wrong = bond(signer, op(0xB2));
        assert_eq!(
            adjudicate_equivocation_carriage_v1(&c, &wrong, 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationWrongBondRecord { resolved: op(0xB2), accused })
        );
    }

    /// Forged signatures prove nothing, and neither does a certificate whose two attestations
    /// agree — the latter is not an offence at all.
    #[test]
    fn forgery_and_non_contradiction_are_both_refused() {
        let (signer, accused) = (h(0xE1), op(0xB1));

        let mut forged = carriage(signer, accused, h(0x01), h(0x02));
        forged.certificate.attestation_b.signature = vec![0xFF; 64];
        assert!(matches!(
            adjudicate_equivocation_carriage_v1(&forged, &bond(signer, accused), 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationNotProven(_))
        ));

        // Same root twice: the signer said one thing twice, which is not equivocation.
        let agreeing = carriage(signer, accused, h(0x01), h(0x01));
        assert!(matches!(
            adjudicate_equivocation_carriage_v1(&agreeing, &bond(signer, accused), 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationNotProven(_))
        ));
    }

    /// A bond that is not at risk cannot be taken: slashed already, or not yet active.
    #[test]
    fn an_inactive_bond_cannot_be_slashed() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let c = carriage(signer, accused, h(0x01), h(0x02));

        let mut already_slashed = bond(signer, accused);
        already_slashed.slashed_at_daa_score = Some(50);
        already_slashed.status = BondStatus::Slashed;
        assert_eq!(
            adjudicate_equivocation_carriage_v1(&c, &already_slashed, 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationBondInactive)
        );

        let mut not_yet = bond(signer, accused);
        not_yet.activation_daa_score = 10_000;
        not_yet.status = BondStatus::Pending;
        assert_eq!(
            adjudicate_equivocation_carriage_v1(&c, &not_yet, 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationBondInactive)
        );
    }

    /// Class divergence — two different signers — may freeze but never slash (ADR-0027 P1), so it
    /// is refused at admission and would be refused again at adjudication.
    #[test]
    fn class_divergence_is_not_carried_here_and_never_slashes() {
        let ctx = context();
        let divergent = PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: op(0xB1),
            certificate: PalwClassContradictionCertificateV1 {
                version: PALW_S_OBJECT_VERSION_V3,
                attestation_a: attested(h(0xE1), &ctx, h(0x01)),
                attestation_b: attested(h(0xE2), &ctx, h(0x02)),
                job_context: ctx,
            },
        };
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::Equivocation(divergent.clone())),
            Err(PalwCarriageError::EquivocationNotOneSigner)
        );
        assert_eq!(
            adjudicate_equivocation_carriage_v1(&divergent, &bond(h(0xE1), op(0xB1)), 100, NET, mock_verify),
            Err(PalwCarriageError::EquivocationBondNotTheSigner)
        );
    }

    /// The kind round-trips on its own subnetwork id, like every other band member.
    #[test]
    fn the_kind_routes_and_roundtrips() {
        let c = carriage(h(0xE1), op(0xB1), h(0x01), h(0x02));
        let obj = PalwCarriageV1::Equivocation(c);
        assert_eq!(obj.kind_byte(), PALW_CARRIAGE_KIND_EQUIVOCATION);
        assert_eq!(palw_carriage_tx_kind(&SUBNETWORK_ID_PALW_EQUIVOCATION), Some(PALW_CARRIAGE_KIND_EQUIVOCATION));
        let encoded = encode_palw_carriage_v1(&obj);
        let body = &encoded[7..];
        assert_eq!(decode_palw_stage1_body(PALW_CARRIAGE_KIND_EQUIVOCATION, body).unwrap(), obj);
        assert_eq!(PALW_S_MLDSA87_ATTESTATION_CONTEXT.is_empty(), false);
    }
}

#[cfg(test)]
mod step_conviction_tests {
    /// The chain identity every adjudication in this module runs under — it must equal the network
    /// the fixtures' own job context names, because a certificate from another network is refused.
    const NET: &[u8] = b"step-refute-test";

    use super::*;
    use crate::dns_finality::{BondStatus, StakeBondRecord};
    use crate::palw_slash::PALW_S_OBJECT_VERSION_V3;
    use crate::palw_step_refute::PalwWeightOracleV1;
    use crate::tx::TransactionId;

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }
    fn mock_key(signer: Hash64) -> Vec<u8> {
        signer.as_byte_slice().to_vec()
    }
    fn mock_sign(key: &[u8], digest: &Hash) -> Vec<u8> {
        let mut s = key.to_vec();
        s.extend_from_slice(digest.as_bytes().as_slice());
        s
    }
    fn mock_verify(key: &[u8], digest: &Hash, signature: &[u8], _context: &[u8]) -> bool {
        signature == mock_sign(key, digest).as_slice()
    }

    struct NoWeights;
    impl PalwWeightOracleV1 for NoWeights {
        fn operand_bytes(&self, _t: &str, _l: Option<u16>, _r: u32, _e: u32) -> Option<Vec<u8>> {
            None
        }
    }

    fn bond(signer: Hash64, outpoint: TransactionOutpoint) -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: h(0x0A0A),
            validator_pubkey_hash: signer,
            validator_pubkey: mock_key(signer),
            amount: 20_000_00000000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 1_000,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    /// A conviction carriage whose two halves agree about the execution, with a real signature.
    /// The refutation itself is a minimal skeleton: these tests exercise the AUTHORSHIP half and
    /// the gating, which is what this module owns — the falsity half is `palw_step_refute`'s and
    /// is tested there against honest and tampered executions.
    fn conviction(signer: Hash64, accused: TransactionOutpoint) -> PalwStepConvictionCarriageV1 {
        let refutation = crate::palw_step_refute::tests::skeleton_refutation();
        let mut attestation = PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id: signer,
            job_context_hash: refutation.binding.job_context.context_hash(),
            full_logits_trace_root: refutation.binding.full_logits_trace_root,
            committed_root: refutation.binding.committed_execution_root,
            bond_outpoint: accused,
            signature: vec![],
        };
        let network_id = refutation.binding.job_context.network_id.clone();
        attestation.signature = mock_sign(&mock_key(signer), &attestation.message(&network_id));
        PalwStepConvictionCarriageV1 { version: PALW_CARRIAGE_VERSION_V1, accused_bond_outpoint: accused, attestation, refutation }
    }

    /// Shape admission binds the two halves to ONE execution. A certificate that signs one job
    /// and refutes another proves nothing about the bond it accuses, and is refused at the door
    /// so adjudication never has to consider it.
    #[test]
    fn the_two_halves_must_be_about_the_same_execution() {
        let c = conviction(h(0xE1), op(0xB1));
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(c.clone())).is_ok());

        let mut wrong_context = c.clone();
        wrong_context.attestation.job_context_hash = h(0xDEAD);
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(wrong_context)),
            Err(PalwCarriageError::StepConvictionContextMismatch)
        );

        let mut wrong_root = c;
        wrong_root.attestation.full_logits_trace_root = h(0xDEAD);
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(wrong_root)),
            Err(PalwCarriageError::StepConvictionRootMismatch)
        );
    }

    /// The forgery the generation-2 `committed_root` closes, built end to end.
    ///
    /// Matching the job context and the logits leg was NOT enough. A step refutation refutes the
    /// COMPOSITE root, and every other part of it — here `checkpoint_count` — is the filer's own
    /// input that `verify_binding` only checks against the rest of the filer's input. So anyone
    /// could take a genuine attestation off the chain, rebuild a self-consistent binding around
    /// the same job with a non-canonical count, and slash a bond that had done nothing wrong.
    ///
    /// The test proves both halves of that claim: the forged refutation really does convict on its
    /// own (so nothing else was stopping it), and the conviction carrying it is refused because
    /// the accused never signed that composite root.
    #[test]
    fn a_forged_binding_around_a_genuine_attestation_cannot_slash() {
        let (signer, victim) = (h(0xE1), op(0xB1));
        let honest = conviction(signer, victim);
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(honest.clone())).is_ok());

        // Forge: same job, same logits leg, one tampered part, composite root recomputed so the
        // binding is internally consistent.
        let mut forged = honest.refutation.clone();
        forged.binding.checkpoint_count = honest.refutation.binding.checkpoint_count + 1;
        forged.binding.committed_execution_root = {
            // Recomputed exactly the way `verify_binding` does — leg roots first, then the
            // composite. A forger has every input it needs; that is the point.
            let ctx = forged.binding.job_context.context_hash();
            let profile = forged.binding.shape_profile.shape_profile_id();
            let step_root = crate::palw_step_leg::step_leg_root_v1(
                &ctx,
                &profile,
                forged.binding.step_leaf_count,
                &forged.binding.step_merkle_root,
            );
            let ckpt_root = crate::palw_step_leg::checkpoint_leg_root_v2(
                &ctx,
                &forged.binding.checkpoint_profile.profile_hash(),
                &forged.binding.state_chunk_map_id,
                forged.binding.job_context.exact_decode_tokens.saturating_sub(1),
                forged.binding.checkpoint_count,
                &forged.binding.checkpoint_merkle_root,
            );
            crate::palw_step_leg::execution_commitment_root_v2(
                &ctx,
                &forged.binding.full_logits_trace_root,
                &forged.binding.activation_leg_root,
                &ckpt_root,
                &step_root,
            )
        };
        assert_ne!(forged.binding.committed_execution_root, honest.refutation.binding.committed_execution_root);

        // Half one: the forgery convicts. The shape pass answers from the binding alone, before
        // any opening or weight is read, so no oracle and no honest material can save the victim.
        let verdict = crate::palw_step_refute::check_execution_step_refutation_v1(&forged, &NoWeights)
            .expect("a non-canonical checkpoint count is a structural fault");
        assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::CheckpointCountNotCanonical);

        // Half two: paired with the genuine attestation it is refused at the door, because the
        // signature does not stand behind the root being refuted.
        let attack = PalwStepConvictionCarriageV1 { refutation: forged.clone(), ..honest.clone() };
        assert_eq!(
            validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(attack.clone())),
            Err(PalwCarriageError::StepConvictionCommittedRootMismatch)
        );
        // And refused again by the adjudicator, which is the function that actually slashes and
        // must not rely on admission having run.
        let bond = bond(signer, victim);
        assert!(matches!(
            adjudicate_step_conviction_carriage_v1(&attack, &bond, 1_000, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::EquivocationNotProven(_))
        ));
        // Re-signing the forged root is not a way around it: the attacker does not hold the key,
        // and with the victim's key the composite root is one the victim genuinely stands behind.
        let mut resigned = attack;
        resigned.attestation.committed_root = forged.binding.committed_execution_root;
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::StepConviction(resigned.clone())).is_ok());
        assert!(
            matches!(
                adjudicate_step_conviction_carriage_v1(&resigned, &bond, 1_000, NET, &NoWeights, mock_verify),
                Err(PalwCarriageError::EquivocationNotProven(ref why)) if why.contains("signature")
            ),
            "the message covers committed_root, so editing it invalidates the signature"
        );
    }

    /// The authorship half, and the innocent-bond defence it exists for: a conviction pointed at
    /// a bond whose validator key is not the attester derives nothing, however genuine the
    /// refutation.
    #[test]
    fn a_conviction_cannot_be_pointed_at_an_innocent_bond() {
        let (signer, victim_outpoint) = (h(0xE1), op(0xB9));
        let c = conviction(signer, victim_outpoint);
        let victim = bond(h(0x00CE), victim_outpoint);
        assert_eq!(
            adjudicate_step_conviction_carriage_v1(&c, &victim, 100, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::EquivocationBondNotTheSigner)
        );

        // And not even one of the SIGNER'S OWN other bonds. `validator_pubkey_hash` is not unique,
        // so matching authorship on the key alone let a conviction name a sibling bond that had
        // signed nothing about this execution — the accused would be slashed for its neighbour's
        // fault. Since generation 3 the attestation names the bond it is made by, so the accused is
        // checkable. Both bonds here carry the signer's key; only the named one is accusable.
        let sibling_outpoint = op(0xB7);
        let sibling = bond(signer, sibling_outpoint);
        assert_ne!(sibling_outpoint, c.attestation.bond_outpoint);
        assert_eq!(sibling.validator_pubkey_hash, c.attestation.executor_id, "the sibling really does share the key");
        let mut pointed_at_sibling = c.clone();
        pointed_at_sibling.accused_bond_outpoint = sibling_outpoint;
        assert_eq!(
            adjudicate_step_conviction_carriage_v1(&pointed_at_sibling, &sibling, 100, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::EquivocationBondNotTheSigner),
            "a conviction may only name the bond its attestation was made by"
        );
        // The named bond is still accusable, so the tightening costs an honest prosecution nothing:
        // it reaches the falsity half and stops only at the missing oracle.
        assert!(matches!(
            adjudicate_step_conviction_carriage_v1(&c, &bond(signer, c.attestation.bond_outpoint), 100, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::StepConvictionNotProven(_))
        ));
    }

    /// A forged attestation is refused BEFORE the step is ever recomputed: authorship is checked
    /// first, so an unsigned accusation costs nobody a step evaluation.
    #[test]
    fn a_forged_attestation_is_refused_before_the_step_is_evaluated() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let mut forged = conviction(signer, accused);
        forged.attestation.signature = vec![0xFF; 64];
        assert!(matches!(
            adjudicate_step_conviction_carriage_v1(&forged, &bond(signer, accused), 100, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::EquivocationNotProven(_))
        ));
    }

    /// **`Unadjudicable` is not a conviction.** With no weight oracle the step cannot be
    /// recomputed, and the honest answer is that nothing was established — not that the accused
    /// is guilty. This is the hole ADR-0038 A4 closes by requiring catalog coverage, and the
    /// direction it must fail in.
    #[test]
    fn an_unadjudicable_step_convicts_nobody() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let c = conviction(signer, accused);
        let verdict = adjudicate_step_conviction_carriage_v1(&c, &bond(signer, accused), 100, NET, &NoWeights, mock_verify);
        assert!(matches!(verdict, Err(PalwCarriageError::StepConvictionNotProven(_))), "got {verdict:?}");
    }

    /// An inactive bond is not at risk, whatever the evidence says.
    #[test]
    fn an_inactive_bond_cannot_be_convicted() {
        let (signer, accused) = (h(0xE1), op(0xB1));
        let c = conviction(signer, accused);
        let mut slashed = bond(signer, accused);
        slashed.slashed_at_daa_score = Some(50);
        slashed.status = BondStatus::Slashed;
        assert_eq!(
            adjudicate_step_conviction_carriage_v1(&c, &slashed, 100, NET, &NoWeights, mock_verify),
            Err(PalwCarriageError::EquivocationBondInactive)
        );
    }

    /// The kind routes on its own subnetwork id and round-trips through both decode paths.
    #[test]
    fn the_kind_routes_and_roundtrips() {
        let obj = PalwCarriageV1::StepConviction(conviction(h(0xE1), op(0xB1)));
        assert_eq!(obj.kind_byte(), PALW_CARRIAGE_KIND_STEP_CONVICTION);
        assert_eq!(palw_carriage_tx_kind(&SUBNETWORK_ID_PALW_STEP_CONVICTION), Some(PALW_CARRIAGE_KIND_STEP_CONVICTION));
        let stage0 = encode_palw_carriage_v1(&obj);
        assert_eq!(decode_palw_carriage_v1(&stage0).unwrap(), Some(obj.clone()), "stage-0 path");
        assert_eq!(decode_palw_stage1_body(PALW_CARRIAGE_KIND_STEP_CONVICTION, &stage0[7..]).unwrap(), obj, "stage-1 path");
    }
}

#[cfg(test)]
mod bisect_move_tests {
    use super::*;
    use crate::palw_bisect::{
        PALW_BISECT_MAX_SPACE, PalwBisectDisclosureV1, PalwBisectLadderV1, PalwBisectSpaceV1, PalwBisectTurnV1, PalwBisectVerdictV1,
    };
    use crate::tx::TransactionId;

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }
    fn carriage(body: PalwBisectMoveBodyV1) -> PalwBisectMoveCarriageV1 {
        PalwBisectMoveCarriageV1 { version: PALW_CARRIAGE_VERSION_V1, challenger_bond_outpoint: op(0xC1), body }
    }
    fn open_body(space_size: u64) -> PalwBisectMoveBodyV1 {
        PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0x22),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            responder_bond_outpoint: crate::tx::TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0xB1; 64]), 0),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size,
        }
    }

    /// **The wire form cannot express a deadline, which is the point.**
    ///
    /// A carried deadline is what let the moving party set its opponent's clock to one DAA and
    /// win by expiry. The state machine now derives it from `accepted_daa + w_round`; this pins
    /// that the transport agrees, so the attack cannot come back through the wire while the
    /// machine stays honest.
    #[test]
    fn a_move_cannot_carry_a_deadline() {
        let disclosure = PalwBisectDisclosureV1 { version: 1, session_id: h(0x99), round: 0, midpoint: 8, mid_state: h(0x9) };
        let verdict = PalwBisectVerdictV1 { version: 1, session_id: h(0x99), round: 0, agree: true };
        // Borsh encodes every field, so a deadline would show up as extra bytes. These sizes are
        // the whole message: version + session + round + payload, and nothing else.
        assert_eq!(borsh::to_vec(&disclosure).unwrap().len(), 2 + 64 + 4 + 8 + 64, "disclosure carries no extra field");
        assert_eq!(borsh::to_vec(&verdict).unwrap().len(), 2 + 64 + 4 + 1, "verdict carries no extra field");
    }

    /// Admission refuses only what no ladder could ever open; legality is the session's question,
    /// because it needs state a stateless check does not have.
    #[test]
    fn admission_refuses_an_unopenable_space_and_nothing_else() {
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::BisectMove(carriage(open_body(16)))).is_ok());
        for bad in [0u64, 1, PALW_BISECT_MAX_SPACE + 1] {
            assert_eq!(
                validate_palw_carriage_v1(&PalwCarriageV1::BisectMove(carriage(open_body(bad)))),
                Err(PalwCarriageError::BisectSpaceOutOfRange { got: bad })
            );
        }
        // A disclosure for a session this node has never seen is SHAPE-valid: whether it is the
        // right turn at the right round is what the ladder decides, statefully.
        let stray = PalwBisectMoveBodyV1::Disclosure(PalwBisectDisclosureV1 {
            version: 1,
            session_id: h(0xDEAD),
            round: 7,
            midpoint: 3,
            mid_state: h(0x9),
        });
        assert!(validate_palw_carriage_v1(&PalwCarriageV1::BisectMove(carriage(stray))).is_ok());
    }

    /// A full game played through the carriage types: the moves that ride the chain drive the
    /// same machine, and the ladder converges on the divergent index.
    #[test]
    fn a_game_played_through_carriage_converges() {
        const W_ROUND: u64 = 30;
        let divergence = 5u64;
        let mut ladder = PalwBisectLadderV1::open(&h(0x11), &h(0x22), &h(0x33), &h(0x44), PalwBisectSpaceV1::StepLeaves, 16, 100, 200)
            .expect("a 16-wide space opens");
        let mut daa = 200u64;
        let mut moves = 0;
        while ladder.turn() != PalwBisectTurnV1::Terminal {
            let mid = ladder.expected_midpoint().expect("not terminal");
            let disclosure = PalwBisectDisclosureV1 {
                version: 1,
                session_id: ladder.session_id(),
                round: ladder.round(),
                midpoint: mid,
                mid_state: h(mid),
            };
            // Round-trip the move through the wire before applying it — a game whose transport
            // loses a field is a game two nodes play differently.
            let encoded =
                encode_palw_carriage_v1(&PalwCarriageV1::BisectMove(carriage(PalwBisectMoveBodyV1::Disclosure(disclosure.clone()))));
            let PalwCarriageV1::BisectMove(back) = decode_palw_stage1_body(PALW_CARRIAGE_KIND_BISECT_MOVE, &encoded[7..]).unwrap()
            else {
                panic!("kind must round-trip")
            };
            let PalwBisectMoveBodyV1::Disclosure(d) = back.body else { panic!("body must round-trip") };
            assert_eq!(d, disclosure);
            daa += 5;
            ladder.apply_disclosure(&d, daa, W_ROUND).unwrap();

            let verdict =
                PalwBisectVerdictV1 { version: 1, session_id: ladder.session_id(), round: ladder.round(), agree: divergence >= mid };
            daa += 5;
            ladder.apply_verdict(&verdict, daa, W_ROUND).unwrap();
            moves += 1;
            assert!(moves < 64, "a 16-wide space cannot need this many rungs");
        }
        assert_eq!(ladder.terminal_index(), Some(divergence));
    }

    /// The kind routes on its own subnetwork id and survives both decode paths.
    fn bond_record_for_authorship() -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0x11; 64]), 0),
            owner_pubkey_hash: Hash64::from_u64_word(1),
            validator_pubkey_hash: Hash64::from_u64_word(1),
            validator_pubkey: vec![1u8; 32],
            amount: 20_000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 100,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: crate::dns_finality::BondStatus::Active,
        }
    }

    /// **Audit P0-9 item 3**: a producer that withheld its execution still has authorship.
    ///
    /// The ladder exists for exactly that producer, and it has signed no attestation — so the only
    /// conviction object the step adjudicator accepts can never be built against it, and a terminal
    /// charging the challenger for not filing it would be fail-open. Its block commitment is the
    /// signed claim it does have.
    ///
    /// Each conjunct is dropped in turn, because each admits a different forgery.
    #[test]
    fn a_withheld_execution_is_authored_by_the_block_commitment() {
        use crate::palw_block_commitment::{PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT, PALW_BLOCK_COMMITMENT_VERSION_V1, PalwBlockCommitmentV1};
        let bond = bond_record_for_authorship();
        let root = Hash64::from_u64_word(0x7A);
        let authorship = PalwWithheldAuthorshipV1 {
            commitment: PalwBlockCommitmentV1 {
                version: PALW_BLOCK_COMMITMENT_VERSION_V1,
                execution_class_id: Hash64::from_u64_word(0xC1),
                executor_bond_outpoint: bond.bond_outpoint,
                trace_root: root,
                output_root: Hash64::from_u64_word(0),
                pwu_claim: 42,
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
            pre_pow_hash: Hash64::from_u64_word(0xB0),
            timestamp: 1_700_000_000,
            nonce: 7,
        };
        let ok = |_k: &[u8], _d: &Hash, s: &[u8], c: &[u8]| {
            s == vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN].as_slice() && c == PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT
        };
        assert!(authorship.establishes_authorship_v1(&bond, &root, b"net", 1_000, ok), "the honest case must hold");

        // The refuted root must be the one this commitment announced — otherwise an honest block's
        // signature authorises a conviction over a different execution.
        assert!(!authorship.establishes_authorship_v1(&bond, &Hash64::from_u64_word(0xFF), b"net", 1_000, ok));

        // The commitment must name THIS bond — otherwise the conviction slashes a party that signed
        // nothing about this execution.
        let mut other = bond.clone();
        other.bond_outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x99; 64]), 0);
        assert!(!authorship.establishes_authorship_v1(&other, &root, b"net", 1_000, ok));

        // The signature must verify under the BLOCK-COMMITMENT domain: a signature is evidence only
        // about the family it was made for.
        let wrong_domain = |_k: &[u8], _d: &Hash, _s: &[u8], c: &[u8]| c == b"another-domain".as_slice();
        assert!(!authorship.establishes_authorship_v1(&bond, &root, b"net", 1_000, wrong_domain));

        // And the attempt is inside the digest, so a signature over one attempt does not carry to
        // another — the network is likewise bound.
        assert!(!authorship.establishes_authorship_v1(&bond, &root, b"other-net", 1_000, |_k, _d, _s, _c| false));
    }

    #[test]
    fn the_kind_routes_and_roundtrips() {
        let obj = PalwCarriageV1::BisectMove(carriage(open_body(16)));
        assert_eq!(obj.kind_byte(), PALW_CARRIAGE_KIND_BISECT_MOVE);
        assert_eq!(palw_carriage_tx_kind(&SUBNETWORK_ID_PALW_BISECT_MOVE), Some(PALW_CARRIAGE_KIND_BISECT_MOVE));
        let stage0 = encode_palw_carriage_v1(&obj);
        assert_eq!(decode_palw_carriage_v1(&stage0).unwrap(), Some(obj.clone()));
        assert_eq!(decode_palw_stage1_body(PALW_CARRIAGE_KIND_BISECT_MOVE, &stage0[7..]).unwrap(), obj);
    }
}
