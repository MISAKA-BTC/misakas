//! PALW-S — the ADR-0027 slash objects: unilateral, objectively-checkable evidence.
//!
//! Normative sources: `docs/adr/0027-palw-slash-unilateral-fraud-proofs.md` (premises and
//! adjudication semantics), `docs/misaka-palw-slash-protocol-design-v0.1.md` as amended by that
//! ADR's §4 table, and `consensus/core/src/palw_v2.rs` for every reused preimage.
//!
//! # Scope and stage — read this before wiring anything to consensus
//!
//! This module is **Land**-stage code: types, hashing, and adjudication predicates only. Nothing
//! in consensus validation, fork choice, difficulty, emission, the header pipeline, or transaction
//! acceptance consumes it, and it must stay that way until the PALW-S staged activations (v0.1
//! §21 as restated in ADR-0027 §6) pass their own gates. **No slashing is enabled by this code
//! existing.**
//!
//! # The three premises, enforced by construction
//!
//! * **P1 — no BFT.** Every check in this module is decidable by a single party from the carried
//!   bytes plus the frozen v2 preimages. There is no quorum input anywhere: no vote count, no
//!   committee bitmap, no aggregate signature. The only signatures here are the *offender's own*
//!   (equivocation) or an honest divergence pair (class contradiction — which freezes, never
//!   slashes).
//! * **P2 — no challenge randomness.** Nothing here consumes a challenge seed, an anchor, or a
//!   sampled position. The refuted position is *named by the challenger*; the checker only
//!   verifies the naming against the commitment.
//! * **P3 — slash-terminal.** Every adjudication result is evidence for a bond movement (or a
//!   profile freeze). None of these objects can invalidate a block or re-order the DAG.
//!
//! # What is adjudicable at this stage
//!
//! The v2 trace commits ordered **logits** events. Without the activation/checkpoint legs and the
//! canonical reference evaluator (ADR-0027 §2), an arithmetic step refutation
//! (`ExecutionStepRefutationV1`) is not yet expressible — implementing it before its inputs are
//! pinned would freeze semantics that do not exist. What IS objectively checkable today, and is
//! implemented here:
//!
//! 1. [`PalwClassContradictionCertificateV1`] — ADR-0027 §5. Two signed attestations over the
//!    same `job_context_hash` with differing trace roots. Same signer ⇒ **executor equivocation**
//!    (an objective offense: two signatures that cannot both be true — the same rule
//!    [`crate::vlt::ComputeFraudKind::ContradictoryVerification`] already enforces one layer
//!    down). Different signers ⇒ **class divergence**: the determinism-class claim itself is
//!    refuted, which under P1 proves *the class* wrong but not *who* — so it may only freeze
//!    (fail-safe), never slash.
//! 2. [`PalwTraceSummaryRefutationV1`] — the committed root's own transparent preimage violates
//!    the pinned exact-decode schedule ([`crate::palw_v2::PalwTraceCommitmentV2::assemble`] would have refused to
//!    produce it). No opening needed; the whole preimage is carried.
//! 3. [`PalwTraceEventRefutationV1`] — a Merkle-opened event's preimage violates the pinned
//!    scheme: non-finite logits (the fail-closed rule an honest worker can never break), a logit
//!    count that is not the event's vocab, a vocab that is not the profile's, or a byte length
//!    that is not `4 × count`. One opening, one preimage, no model execution.
//!
//! Rejection is symmetric and explicit: a refutation that addresses the committed material and
//! finds it honest returns [`PalwSlashError::NoFaultFound`] — at acceptance stage that verdict is
//! what costs the challenger their bond (v0.1 §4.4), so the checker must never soften it.
//!
//! # Preimage reuse discipline
//!
//! Everything recomputed here — job context hash, outer trace root, summary rules — calls the
//! `palw_v2` public API. The two places this module must reproduce a `palw_v2` byte layout
//! instead of calling it (the Merkle leaf/node, whose per-level functions are not exposed, and
//! the logits-event header, whose producer rejects the very non-finite rows a refutation must
//! hash) share the *public domain constants* and are frozen against the originals by equivalence
//! tests (`opening_agrees_with_production_root_for_every_shape`,
//! `refutation_event_hash_matches_production_on_finite_rows`). If `palw_v2` ever changes those
//! layouts, this module fails its tests instead of silently forking the preimage.

use crate::palw_v2::{
    PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_DOMAIN_TRACE_MERKLE_LEAF, PALW_V2_DOMAIN_TRACE_MERKLE_NODE,
    PALW_V2_MAX_NETWORK_ID_BYTES, PALW_V2_MAX_TRACE_EVENTS, PalwJobContextV2, PalwLogitsDtypeV2, PalwTracePhaseV2, PalwTraceSummaryV2,
    full_logits_trace_root_v2, trace_scheme_id_v2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Versions, domains, caps
// ---------------------------------------------------------------------------------------------

/// Wire version of every PALW-S object in this module. **Generation 3** (2026-08-17): the execution
/// attestation gained `committed_root` (generation 2) and then `bond_outpoint` (generation 3), so
/// every object in the family that embeds or accompanies an attestation changed layout with it.
/// Older bytes are refused rather than misdecoded, and refusing them is the point rather than a
/// side effect: each retired generation signed strictly less than a claim needs to bind, and both
/// of the gaps were exploitable (see [`PalwExecutionAttestationV1::committed_root`] and
/// [`PalwExecutionAttestationV1::bond_outpoint`]).
pub const PALW_S_OBJECT_VERSION_V3: u16 = 3;

/// Keyed-BLAKE2b-256 domain of [`palw_execution_attestation_message_v3`]. The retired
/// `…/execution-attestation-message/v1` and `…/v2` strings must never be reused: a v1 signature is
/// a claim about the logits leg alone and a v2 signature says nothing about which bond stands
/// behind it, so reviving either string would let a narrower claim be replayed as a wider one.
pub const PALW_S_DOMAIN_ATTESTATION_MESSAGE_V3: &[u8] = b"misaka-palw/execution-attestation-message/v3";
/// ML-DSA-87 signing context for execution attestations (the registry resolves `executor_id` to
/// the public key; this module never resolves keys itself).
pub const PALW_S_MLDSA87_ATTESTATION_CONTEXT: &[u8] = b"misaka-palw/execution-attestation/mldsa87/v1";
/// Keyed-BLAKE2b-512 domain of the class-contradiction `evidence_id` (the v0.1 §24.1 `slash_id`
/// dedup key for this offense family).
pub const PALW_S_DOMAIN_CONTRADICTION_EVIDENCE_ID: &[u8] = b"misaka-palw/class-contradiction-evidence-id/v1";
/// Keyed-BLAKE2b-512 domain of structural-refutation `evidence_id`s.
pub const PALW_S_DOMAIN_REFUTATION_EVIDENCE_ID: &[u8] = b"misaka-palw/structural-refutation-evidence-id/v1";

/// Every domain this module introduces. The uniqueness test also checks these against
/// [`crate::palw_v2::PALW_V2_ALL_DOMAINS`] — one string reused across families is a preimage
/// bridge, exactly the class of bug the v2 domain-key incident was.
pub const PALW_S_ALL_DOMAINS: &[&[u8]] = &[
    PALW_S_DOMAIN_ATTESTATION_MESSAGE_V3,
    PALW_S_MLDSA87_ATTESTATION_CONTEXT,
    PALW_S_DOMAIN_CONTRADICTION_EVIDENCE_ID,
    PALW_S_DOMAIN_REFUTATION_EVIDENCE_ID,
];

/// Upper bound on a carried signature. ML-DSA-87 signatures are 4 627 bytes; anything past this
/// cap is rejected at shape level before any hashing or verification is attempted.
pub const PALW_S_MAX_SIGNATURE_BYTES: usize = 8192;

/// Upper bound on a carried logits row (bytes). The measured profile row is
/// `248 320 × 4 ≈ 0.99 MiB`; the cap leaves headroom for larger vocabs while keeping adversarial
/// allocations bounded.
pub const PALW_S_MAX_LOGITS_BYTES: usize = 16 * 1024 * 1024;

/// Maximum Merkle siblings an opening may carry: `ceil(log2(PALW_V2_MAX_TRACE_EVENTS))`.
pub const PALW_S_MAX_OPENING_SIBLINGS: usize = PALW_V2_MAX_TRACE_EVENTS.ilog2() as usize;

// ---------------------------------------------------------------------------------------------
// Errors — fail closed; every rejection is a variant, and the two rejection *meanings* are kept
// apart: malformed/unaddressed evidence (the object is not even about the committed material)
// versus NoFaultFound / NoContradiction (the material was addressed and found honest — the
// verdict that costs a challenger their bond at acceptance stage).
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwSlashError {
    #[error("unsupported palw-s object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("signature is empty or exceeds {max} bytes (got {got})")]
    SignatureSizeOutOfRange { got: usize, max: usize },
    #[error("job context is malformed: {0}")]
    ContextShape(&'static str),
    #[error("job context pins a different trace scheme than misaka-palw/full-logits-trace/v2")]
    SchemeMismatch,
    #[error(
        "the evidence names a different network than the chain adjudicating it — a foreign-network certificate may not slash a bond here"
    )]
    ForeignNetwork,
    #[error("attestation {which} does not bind the carried job context")]
    AttestationContextMismatch { which: &'static str },
    #[error("attestation {which} carries an invalid signature")]
    InvalidSignature { which: &'static str },
    #[error("the two attested roots are equal — nothing is contradicted")]
    NoContradiction,
    #[error("event_count {got} is outside 1..={max}")]
    EventCountOutOfRange { got: u32, max: usize },
    #[error("event_index {index} is not below event_count {count}")]
    EventIndexOutOfRange { index: u32, count: u32 },
    #[error("opening carries {got} siblings, exceeding the {max}-level cap")]
    OpeningTooDeep { got: usize, max: usize },
    #[error("opening path ended {missing} sibling(s) short of the root")]
    OpeningPathTooShort { missing: usize },
    #[error("opening path carries {extra} sibling(s) past the root")]
    OpeningPathTooLong { extra: usize },
    #[error("opening does not reproduce the carried ordered_event_commitment")]
    OpeningRootMismatch,
    #[error("carried preimage does not recompute the committed trace root")]
    CommittedRootMismatch,
    #[error("carried event preimage does not hash to the opened event hash")]
    EventPreimageMismatch,
    #[error("carried logits bytes exceed the {max}-byte cap (got {got})")]
    LogitsBytesTooLarge { got: usize, max: usize },
    #[error("the addressed material is honest under every pinned rule — refutation rejected")]
    NoFaultFound,
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The context-shape subset checkable without the envelope: version, network-id bounds, budgets,
/// and the scheme pin. (Prompt emptiness etc. are envelope-level facts an opaque hash cannot
/// witness.) Everything here must hold for ANY honestly-produced v2 context.
pub(crate) fn check_job_context_shape(ctx: &PalwJobContextV2) -> Result<(), PalwSlashError> {
    if ctx.version != PALW_TRACE_COMMITMENT_VERSION_V2 {
        return Err(PalwSlashError::ContextShape("version is not the v2 trace commitment version"));
    }
    if ctx.network_id.is_empty() || ctx.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES {
        return Err(PalwSlashError::ContextShape("network id is empty or over the cap"));
    }
    if ctx.exact_decode_tokens == 0 {
        return Err(PalwSlashError::ContextShape("exact_decode_tokens is zero"));
    }
    match ctx.declared_prefill_tokens.checked_add(ctx.exact_decode_tokens) {
        Some(total) if total <= ctx.max_context_tokens => {}
        _ => return Err(PalwSlashError::ContextShape("token budget overflows or exceeds max_context_tokens")),
    }
    // The two schemes an honestly-produced v2 context can pin: the float family's event tree,
    // and the model tiers' tiled selecting-row scheme. The pin used to admit only the first, so
    // every tiled-class binding failed its context check before a single leaf was read — the
    // whole step space of both model tiers, unprosecutable at the front door.
    if ctx.trace_scheme_id != trace_scheme_id_v2() && ctx.trace_scheme_id != crate::palw_step_refute::tiled_logits_scheme_id_v1() {
        return Err(PalwSlashError::SchemeMismatch);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Execution attestation and the class-contradiction certificate (ADR-0027 §5)
// ---------------------------------------------------------------------------------------------

/// A signed claim: "I, `executor_id`, executed the job identified by `job_context_hash` and my
/// canonical trace root is `full_logits_trace_root`."
///
/// The payload deliberately carries **no** class, manifest, model or network field: every one of
/// those is already bound inside `job_context_hash` (the v2 context preimage), and carrying a
/// second copy would create the checkable-against-nothing dual-source surface the v2 capability
/// object explicitly refuses (v2 design §16.1). The network is bound by the *message*, exactly
/// like the capability signature.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwExecutionAttestationV1 {
    /// = [`PALW_S_OBJECT_VERSION_V3`].
    pub version: u16,
    /// The operator identity the bond registry resolves to an ML-DSA-87 public key. Matching it
    /// to a live bond is acceptance-stage work, not this module's.
    pub executor_id: Hash64,
    /// [`PalwJobContextV2::context_hash`] of the executed job.
    pub job_context_hash: Hash64,
    /// [`full_logits_trace_root_v2`] the executor claims for that job.
    pub full_logits_trace_root: Hash64,
    /// The COMMITMENT ROOT this attestation stands behind: the composite execution root for a
    /// composite class (`execution_commitment_root_v1`/`_v2`), or `full_logits_trace_root` itself
    /// for a bare-v2 class, whose committed object IS the logits root.
    ///
    /// Added in generation 2 to close a live forgery on the step-conviction slash path. The signed
    /// preimage used to cover only `(network, executor, job_context, logits_root)`, while a step
    /// refutation refutes the COMPOSITE root — whose other parts (`step_leaf_count`,
    /// `step_merkle_root`, `checkpoint_count`, `checkpoint_merkle_root`, `activation_leg_root`,
    /// `state_chunk_map_id`, `checkpoint_profile`) are free filer input that `verify_binding` only
    /// checks against ITSELF. So anyone could take a genuine attestation, build a self-consistent
    /// `PalwStepBindingV2` around the same job and logits root but with a non-canonical
    /// `checkpoint_count`, and collect a structural conviction — the shape pass returns a verdict
    /// from the binding alone, before any opening is read — slashing a bond that had done nothing
    /// wrong. The v1 legs family never had this hole because it ties
    /// `binding.committed_execution_root` to the producer's signed `committed_root`; the v2 step
    /// leg had no signed anchor at all, because nothing in the tree announces a v2 composite root.
    ///
    /// With this field the tie exists in the object itself: any change to any part of the
    /// composite moves the root, and the root is now inside what the accused signed.
    pub committed_root: Hash64,
    /// The BOND this claim is made by — the payee when the claim earns a share, the accused when it
    /// convicts.
    ///
    /// Added in generation 3 to close a mint that the seat-based panel draw opened. A bond outpoint
    /// is the only unique validator identity (`validator_pubkey_hash` is not, and `dns_finality`
    /// says so), so once the draw seats two bonds of one key separately, a signer holding both can
    /// file the SAME generation-2 signature under each and collect two attester shares for one
    /// replay: the signed message named the key and the execution but never the bond, so nothing
    /// tied a signature to the seat that spent it. `q` is meant to buy `q` independent checks.
    ///
    /// It also tightens conviction. `adjudicate_step_conviction_carriage_v1` matched the accused
    /// bond by `validator_pubkey_hash`, so with two bonds under one key a conviction could name
    /// EITHER — including the one that had signed nothing. The accused must now be the exact bond
    /// the attestation names.
    ///
    /// The executor's own commitment carriage has bound its bond outpoint into its signed digest
    /// all along; the attestation not doing so was the asymmetry.
    pub bond_outpoint: crate::tx::TransactionOutpoint,
    /// ML-DSA-87 signature over [`palw_execution_attestation_message_v3`] under
    /// [`PALW_S_MLDSA87_ATTESTATION_CONTEXT`].
    pub signature: Vec<u8>,
}

impl PalwExecutionAttestationV1 {
    pub fn validate_shape(&self) -> Result<(), PalwSlashError> {
        if self.version != PALW_S_OBJECT_VERSION_V3 {
            return Err(PalwSlashError::UnsupportedVersion { got: self.version, expected: PALW_S_OBJECT_VERSION_V3 });
        }
        if self.signature.is_empty() || self.signature.len() > PALW_S_MAX_SIGNATURE_BYTES {
            return Err(PalwSlashError::SignatureSizeOutOfRange { got: self.signature.len(), max: PALW_S_MAX_SIGNATURE_BYTES });
        }
        Ok(())
    }

    /// The message this attestation's signature must cover, network-bound.
    pub fn message(&self, network_id: &[u8]) -> Hash {
        palw_execution_attestation_message_v3(
            network_id,
            self.executor_id,
            self.job_context_hash,
            self.full_logits_trace_root,
            self.committed_root,
            &self.bond_outpoint,
        )
    }
}

/// Keyed-BLAKE2b-256 signing message of an execution attestation. Layout mirrors
/// `palw_capability_message_v2`: length-prefixed network id, then fixed-width fields in struct
/// order. Golden-frozen in the tests.
///
/// v2 appended `committed_root`; v3 appends `bond_outpoint`. No earlier function survives: each
/// retired layout is one a claim could be replayed out of, so keeping it callable would keep the
/// hole reachable.
pub fn palw_execution_attestation_message_v3(
    network_id: &[u8],
    executor_id: Hash64,
    job_context_hash: Hash64,
    full_logits_trace_root: Hash64,
    committed_root: Hash64,
    bond_outpoint: &crate::tx::TransactionOutpoint,
) -> Hash {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).key(PALW_S_DOMAIN_ATTESTATION_MESSAGE_V3).to_state();
    hasher.update(&(network_id.len() as u32).to_le_bytes());
    hasher.update(network_id);
    hasher.update(executor_id.as_byte_slice());
    hasher.update(job_context_hash.as_byte_slice());
    hasher.update(full_logits_trace_root.as_byte_slice());
    hasher.update(committed_root.as_byte_slice());
    hasher.update(bond_outpoint.transaction_id.as_byte_slice());
    hasher.update(&bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash::from_bytes(out)
}

/// Two attestations over the same job context with different roots, plus the transparent context
/// they bind — carried in full so the adjudicator (and the freeze machinery) can *read* the
/// refuted class/manifest instead of trusting a side-channel copy of them.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClassContradictionCertificateV1 {
    /// = [`PALW_S_OBJECT_VERSION_V3`].
    pub version: u16,
    /// The full job context whose `context_hash()` both attestations must bind.
    pub job_context: PalwJobContextV2,
    pub attestation_a: PalwExecutionAttestationV1,
    pub attestation_b: PalwExecutionAttestationV1,
}

/// What a valid contradiction certificate proves. The split is the P1 line: equivocation has an
/// author and may slash; divergence has no identified author and may only freeze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwClassContradictionKindV1 {
    /// One signer, two roots, one job — two signatures that cannot both be true. The offender is
    /// `executor_id`; this is the PALW-S analogue of
    /// [`crate::vlt::ComputeFraudKind::ContradictoryVerification`].
    ExecutorEquivocation { executor_id: Hash64 },
    /// Two signers honestly (as far as this evidence goes) produced different roots for one job
    /// in one class: the class's determinism claim is refuted. Freeze target identities are read
    /// from the transparent context. **No slash** — the evidence does not identify a liar.
    ClassDivergence { runtime_class_id: Hash64, runtime_manifest_hash: Hash64, model_profile_id: Hash64 },
}

/// A finished adjudication: the kind, plus the order-independent dedup key for v0.1 §24.1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassContradictionVerdictV1 {
    pub kind: PalwClassContradictionKindV1,
    /// `keyed64(evidence domain, min(msg_a, msg_b) ‖ max(msg_a, msg_b))` — the same pair in
    /// either order is one piece of evidence and can finalize once.
    pub evidence_id: Hash64,
}

/// Adjudicates a class-contradiction certificate. `verify_signature` receives the network-bound
/// message and the attestation and must return true iff the signature verifies under the key the
/// registry holds for `attestation.executor_id` (with [`PALW_S_MLDSA87_ATTESTATION_CONTEXT`]).
/// Key resolution is deliberately outside this module: P1 requires the *evidence* to be
/// self-contained, and the key registry is chain state the caller owns.
///
/// Every rejection is fail-closed: a certificate that does not prove a contradiction proves
/// nothing, and nothing may act on it.
pub fn adjudicate_class_contradiction_v1<F>(
    certificate: &PalwClassContradictionCertificateV1,
    chain_network_id: &[u8],
    verify_signature: F,
) -> Result<PalwClassContradictionVerdictV1, PalwSlashError>
where
    F: Fn(&Hash, &PalwExecutionAttestationV1) -> bool,
{
    if certificate.version != PALW_S_OBJECT_VERSION_V3 {
        return Err(PalwSlashError::UnsupportedVersion { got: certificate.version, expected: PALW_S_OBJECT_VERSION_V3 });
    }
    certificate.attestation_a.validate_shape()?;
    certificate.attestation_b.validate_shape()?;
    check_job_context_shape(&certificate.job_context)?;

    let context_hash = certificate.job_context.context_hash();
    if certificate.attestation_a.job_context_hash != context_hash {
        return Err(PalwSlashError::AttestationContextMismatch { which: "a" });
    }
    if certificate.attestation_b.job_context_hash != context_hash {
        return Err(PalwSlashError::AttestationContextMismatch { which: "b" });
    }
    // EITHER root differing is a contradiction. Both are deterministic functions of one job
    // under one class, so two values for one job cannot both be true — and since generation 2 the
    // signer covers both, so a composite-only divergence is just as authored as a logits one.
    let same_logits = certificate.attestation_a.full_logits_trace_root == certificate.attestation_b.full_logits_trace_root;
    let same_committed = certificate.attestation_a.committed_root == certificate.attestation_b.committed_root;
    if same_logits && same_committed {
        return Err(PalwSlashError::NoContradiction);
    }

    // The network identity comes from the CHAIN, not from the certificate. Taking it from
    // `certificate.job_context.network_id` let the filer choose it: a devnet or testnet attestation,
    // honestly signed for that network, verified here and slashed a MAINNET bond, because the same
    // validator key is used across networks and the signing digest was being computed under
    // whichever network the evidence named (audit). Refusing a certificate whose own context
    // disagrees with the chain also means the message this adjudicates is the message the signer
    // actually produced on THIS network.
    if certificate.job_context.network_id.as_slice() != chain_network_id {
        return Err(PalwSlashError::ForeignNetwork);
    }
    let message_a = certificate.attestation_a.message(chain_network_id);
    let message_b = certificate.attestation_b.message(chain_network_id);
    if !verify_signature(&message_a, &certificate.attestation_a) {
        return Err(PalwSlashError::InvalidSignature { which: "a" });
    }
    if !verify_signature(&message_b, &certificate.attestation_b) {
        return Err(PalwSlashError::InvalidSignature { which: "b" });
    }

    let (lo, hi) = if message_a.as_bytes() <= message_b.as_bytes() { (message_a, message_b) } else { (message_b, message_a) };
    let evidence_id = keyed64(PALW_S_DOMAIN_CONTRADICTION_EVIDENCE_ID, &[&lo.as_bytes(), &hi.as_bytes()]);

    let kind = if certificate.attestation_a.executor_id == certificate.attestation_b.executor_id {
        PalwClassContradictionKindV1::ExecutorEquivocation { executor_id: certificate.attestation_a.executor_id }
    } else {
        PalwClassContradictionKindV1::ClassDivergence {
            runtime_class_id: certificate.job_context.runtime_class_id,
            runtime_manifest_hash: certificate.job_context.runtime_manifest_hash,
            model_profile_id: certificate.job_context.model_profile_id,
        }
    };
    Ok(PalwClassContradictionVerdictV1 { kind, evidence_id })
}

// ---------------------------------------------------------------------------------------------
// Merkle opening against the v2 trace-event commitment
// ---------------------------------------------------------------------------------------------

/// Membership proof of one event hash in a [`crate::palw_v2::trace_event_merkle_root_v2`] tree.
///
/// `siblings` are bottom-up and carry ONLY the levels at which the walked node is paired; the
/// levels at which it is the promoted odd node are derived from `(event_index, event_count)` and
/// consume nothing. The verifier derives the full shape itself — an opening cannot smuggle its
/// own promote/pair pattern, which is what makes the odd-promote construction proof-safe.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceEventOpeningV1 {
    pub event_index: u32,
    /// The (opaque) event hash whose membership is being proven.
    pub event_hash: Hash64,
    pub siblings: Vec<Hash64>,
}

/// Recomputes the Merkle root a valid opening implies. Mirrors the production layout of
/// [`crate::palw_v2::trace_event_merkle_root_v2`] (leaf = keyed64(leaf-domain, index_le ‖ event_hash); node =
/// keyed64(node-domain, left ‖ right); odd node promoted unchanged); the equivalence test freezes
/// the mirror against the original for every tree shape up to 17 leaves.
///
/// Returns the computed root — the CALLER compares it against the committed
/// `ordered_event_commitment`; this function has no opinion about what the right root is.
pub fn trace_event_opening_root_v1(event_count: u32, opening: &PalwTraceEventOpeningV1) -> Result<Hash64, PalwSlashError> {
    if event_count == 0 || event_count as usize > PALW_V2_MAX_TRACE_EVENTS {
        return Err(PalwSlashError::EventCountOutOfRange { got: event_count, max: PALW_V2_MAX_TRACE_EVENTS });
    }
    if opening.event_index >= event_count {
        return Err(PalwSlashError::EventIndexOutOfRange { index: opening.event_index, count: event_count });
    }
    if opening.siblings.len() > PALW_S_MAX_OPENING_SIBLINGS {
        return Err(PalwSlashError::OpeningTooDeep { got: opening.siblings.len(), max: PALW_S_MAX_OPENING_SIBLINGS });
    }

    let mut current = {
        let mut preimage = Vec::with_capacity(4 + 64);
        preimage.extend_from_slice(&opening.event_index.to_le_bytes());
        preimage.extend_from_slice(opening.event_hash.as_byte_slice());
        keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF, &[&preimage])
    };
    let mut position = opening.event_index as usize;
    let mut width = event_count as usize;
    let mut siblings = opening.siblings.iter();
    while width > 1 {
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            let Some(sibling) = siblings.next() else {
                return Err(PalwSlashError::OpeningPathTooShort { missing: 1 });
            };
            current = if position.is_multiple_of(2) {
                keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[current.as_byte_slice(), sibling.as_byte_slice()])
            } else {
                keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[sibling.as_byte_slice(), current.as_byte_slice()])
            };
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    let leftover = siblings.count();
    if leftover != 0 {
        return Err(PalwSlashError::OpeningPathTooLong { extra: leftover });
    }
    Ok(current)
}

// ---------------------------------------------------------------------------------------------
// Structural refutations of a committed trace root
// ---------------------------------------------------------------------------------------------

/// The pinned rule a structural refutation proves broken. Discriminants are wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTraceStructuralFaultV1 {
    /// `summary.event_count != context.exact_decode_tokens` — the exact-decode schedule mandates
    /// one event per canonical call and `D` calls total.
    EventCountNotExactDecode = 0,
    /// `summary.first_event_kind != Prefill`.
    FirstEventNotPrefill = 1,
    /// `summary.last_event_kind` disagrees with the event count (`Prefill` iff a single event).
    LastEventKindWrong = 2,
    /// Summary token counts disagree with the job context they are committed beside.
    SummaryTokenCountsDisagree = 3,
    /// The opened event's byte payload is not `4 × logits_count` bytes.
    EventBytesNotFourPerLogit = 4,
    /// The opened event declares `logits_count != n_vocab` — an honest event hashes exactly one
    /// full row.
    EventLogitsCountNotVocab = 5,
    /// The opened event's `n_vocab` is not the committed profile vocab (`summary.vocab_size`).
    EventVocabNotProfile = 6,
    /// The opened event contains a non-finite logit — the fail-closed rule (v2 design §8) an
    /// honest worker can never have committed past.
    EventNonFiniteLogit { logit_index: u32 } = 7,
}

impl PalwTraceStructuralFaultV1 {
    /// Stable `(code, argument)` encoding for evidence ids.
    fn evidence_words(self) -> (u8, u32) {
        match self {
            PalwTraceStructuralFaultV1::EventCountNotExactDecode => (0, 0),
            PalwTraceStructuralFaultV1::FirstEventNotPrefill => (1, 0),
            PalwTraceStructuralFaultV1::LastEventKindWrong => (2, 0),
            PalwTraceStructuralFaultV1::SummaryTokenCountsDisagree => (3, 0),
            PalwTraceStructuralFaultV1::EventBytesNotFourPerLogit => (4, 0),
            PalwTraceStructuralFaultV1::EventLogitsCountNotVocab => (5, 0),
            PalwTraceStructuralFaultV1::EventVocabNotProfile => (6, 0),
            PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index } => (7, logit_index),
        }
    }
}

/// Shared first stage of both refutation checkers: the carried transparent preimage must
/// recompute the committed root, or the refutation is about some other commitment and proves
/// nothing about this one.
fn verify_outer_binding(
    job_context: &PalwJobContextV2,
    summary: &PalwTraceSummaryV2,
    ordered_event_commitment: &Hash64,
    committed_trace_root: &Hash64,
) -> Result<Hash64, PalwSlashError> {
    check_job_context_shape(job_context)?;
    let context_hash = job_context.context_hash();
    let recomputed = full_logits_trace_root_v2(&context_hash, summary, ordered_event_commitment);
    if recomputed != *committed_trace_root {
        return Err(PalwSlashError::CommittedRootMismatch);
    }
    Ok(context_hash)
}

/// The summary-rule scan, in frozen order. Mirrors the rules
/// [`crate::palw_v2::PalwTraceCommitmentV2::assemble`] enforces on the honest path (minus the event-list length,
/// which only the producer can see); the equivalence test pins the mirror to `assemble`'s
/// behavior in both directions.
fn scan_summary_faults(context: &PalwJobContextV2, summary: &PalwTraceSummaryV2) -> Option<PalwTraceStructuralFaultV1> {
    if summary.event_count != context.exact_decode_tokens {
        return Some(PalwTraceStructuralFaultV1::EventCountNotExactDecode);
    }
    if summary.declared_prefill_tokens != context.declared_prefill_tokens || summary.exact_decode_tokens != context.exact_decode_tokens
    {
        return Some(PalwTraceStructuralFaultV1::SummaryTokenCountsDisagree);
    }
    if summary.first_event_kind != PalwTracePhaseV2::Prefill {
        return Some(PalwTraceStructuralFaultV1::FirstEventNotPrefill);
    }
    let expected_last = if summary.event_count == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode };
    if summary.last_event_kind != expected_last {
        return Some(PalwTraceStructuralFaultV1::LastEventKindWrong);
    }
    None
}

fn refutation_evidence_id(tag: &[u8], committed_trace_root: &Hash64, event_index: u32, fault: PalwTraceStructuralFaultV1) -> Hash64 {
    let (code, argument) = fault.evidence_words();
    let mut preimage = Vec::with_capacity(1 + 64 + 4 + 1 + 4);
    preimage.extend_from_slice(tag);
    preimage.extend_from_slice(committed_trace_root.as_byte_slice());
    preimage.extend_from_slice(&event_index.to_le_bytes());
    preimage.push(code);
    preimage.extend_from_slice(&argument.to_le_bytes());
    keyed64(PALW_S_DOMAIN_REFUTATION_EVIDENCE_ID, &[&preimage])
}

/// A finished structural adjudication: what was proven broken, and the §24.1 dedup key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStructuralRefutationVerdictV1 {
    pub fault: PalwTraceStructuralFaultV1,
    pub evidence_id: Hash64,
}

/// Refutation of a committed trace root by its own transparent metadata: the summary committed
/// under `committed_trace_root` violates the pinned exact-decode schedule. Needs no opening —
/// the outer preimage IS the evidence.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceSummaryRefutationV1 {
    /// = [`PALW_S_OBJECT_VERSION_V3`].
    pub version: u16,
    pub job_context: PalwJobContextV2,
    pub summary: PalwTraceSummaryV2,
    /// The Merkle root half of the outer preimage, carried opaquely.
    pub ordered_event_commitment: Hash64,
    /// The root the miner announced (signed elsewhere); the refutation must recompute exactly it.
    pub committed_trace_root: Hash64,
}

pub fn check_trace_summary_refutation_v1(
    refutation: &PalwTraceSummaryRefutationV1,
) -> Result<PalwStructuralRefutationVerdictV1, PalwSlashError> {
    if refutation.version != PALW_S_OBJECT_VERSION_V3 {
        return Err(PalwSlashError::UnsupportedVersion { got: refutation.version, expected: PALW_S_OBJECT_VERSION_V3 });
    }
    verify_outer_binding(
        &refutation.job_context,
        &refutation.summary,
        &refutation.ordered_event_commitment,
        &refutation.committed_trace_root,
    )?;
    match scan_summary_faults(&refutation.job_context, &refutation.summary) {
        Some(fault) => Ok(PalwStructuralRefutationVerdictV1 {
            fault,
            evidence_id: refutation_evidence_id(b"summary/", &refutation.committed_trace_root, 0, fault),
        }),
        None => Err(PalwSlashError::NoFaultFound),
    }
}

/// The claimed preimage of one committed logits event, carried raw. `logits_le_bytes` is the
/// exact byte payload that followed the event header into the hash — including, for a fraudulent
/// event, payloads an honest producer could never emit (non-finite rows, short rows, misaligned
/// tails). That is the point: [`crate::palw_v2::logits_event_hash_v2`] refuses to hash such rows,
/// so refutation needs [`refutation_event_hash_v1`], the layout-identical adjudication-side hash
/// with the producer-side guards removed. **Producers must never use it.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceEventPreimageV1 {
    pub phase: PalwTracePhaseV2,
    pub phase_step: u32,
    pub n_vocab: u32,
    pub logits_count: u32,
    pub logits_le_bytes: Vec<u8>,
}

/// Adjudication-side recomputation of a v2 logits-event hash from carried preimage parts.
/// Byte-layout-identical to [`crate::palw_v2::logits_event_hash_v2`] (frozen by the equivalence
/// test); performs NO validity checks — validity is what the fault scan judges afterwards.
pub fn refutation_event_hash_v1(job_context_hash: &Hash64, preimage: &PalwTraceEventPreimageV1) -> Hash64 {
    let mut header = Vec::with_capacity(64 + 1 + 4 + 4 + 1 + 4);
    header.extend_from_slice(job_context_hash.as_byte_slice());
    header.push(preimage.phase.wire_byte());
    header.extend_from_slice(&preimage.phase_step.to_le_bytes());
    header.extend_from_slice(&preimage.n_vocab.to_le_bytes());
    header.push(PalwLogitsDtypeV2::F32Le.wire_byte());
    header.extend_from_slice(&preimage.logits_count.to_le_bytes());
    keyed64(crate::palw_v2::PALW_V2_DOMAIN_LOGITS_EVENT, &[&header, &preimage.logits_le_bytes])
}

/// Refutation of a committed trace root through one Merkle-opened event whose preimage violates
/// a pinned per-event rule.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTraceEventRefutationV1 {
    /// = [`PALW_S_OBJECT_VERSION_V3`].
    pub version: u16,
    pub job_context: PalwJobContextV2,
    pub summary: PalwTraceSummaryV2,
    pub ordered_event_commitment: Hash64,
    pub committed_trace_root: Hash64,
    pub opening: PalwTraceEventOpeningV1,
    pub event_preimage: PalwTraceEventPreimageV1,
}

pub fn check_trace_event_refutation_v1(
    refutation: &PalwTraceEventRefutationV1,
) -> Result<PalwStructuralRefutationVerdictV1, PalwSlashError> {
    if refutation.version != PALW_S_OBJECT_VERSION_V3 {
        return Err(PalwSlashError::UnsupportedVersion { got: refutation.version, expected: PALW_S_OBJECT_VERSION_V3 });
    }
    if refutation.event_preimage.logits_le_bytes.len() > PALW_S_MAX_LOGITS_BYTES {
        return Err(PalwSlashError::LogitsBytesTooLarge {
            got: refutation.event_preimage.logits_le_bytes.len(),
            max: PALW_S_MAX_LOGITS_BYTES,
        });
    }
    let context_hash = verify_outer_binding(
        &refutation.job_context,
        &refutation.summary,
        &refutation.ordered_event_commitment,
        &refutation.committed_trace_root,
    )?;

    // The opening must land on the committed Merkle root under the committed event count…
    let computed_root = trace_event_opening_root_v1(refutation.summary.event_count, &refutation.opening)?;
    if computed_root != refutation.ordered_event_commitment {
        return Err(PalwSlashError::OpeningRootMismatch);
    }
    // …and the carried preimage must be the opened event, byte for byte.
    if refutation_event_hash_v1(&context_hash, &refutation.event_preimage) != refutation.opening.event_hash {
        return Err(PalwSlashError::EventPreimageMismatch);
    }

    // Fault scan, frozen order: encoding-level first, then identity-level, then value-level.
    let preimage = &refutation.event_preimage;
    let fault = if (preimage.logits_count as u64) * 4 != preimage.logits_le_bytes.len() as u64 {
        Some(PalwTraceStructuralFaultV1::EventBytesNotFourPerLogit)
    } else if preimage.logits_count != preimage.n_vocab {
        Some(PalwTraceStructuralFaultV1::EventLogitsCountNotVocab)
    } else if preimage.n_vocab != refutation.summary.vocab_size {
        Some(PalwTraceStructuralFaultV1::EventVocabNotProfile)
    } else {
        preimage
            .logits_le_bytes
            .chunks_exact(4)
            .position(|chunk| !f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).is_finite())
            .map(|logit_index| PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: logit_index as u32 })
    };

    match fault {
        Some(fault) => Ok(PalwStructuralRefutationVerdictV1 {
            fault,
            evidence_id: refutation_evidence_id(b"event/", &refutation.committed_trace_root, refutation.opening.event_index, fault),
        }),
        None => Err(PalwSlashError::NoFaultFound),
    }
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    /// The chain identity every adjudication in this module runs under — the same one the
    /// fixtures' job context names, because a certificate from another network is refused now.
    const NET: &[u8] = b"misaka-devnet";
    use super::*;
    use crate::palw_v2::{
        PALW_V2_ALL_DOMAINS, PalwStopReasonV2, PalwTraceCommitmentV2, logits_event_hash_v2, trace_event_merkle_root_v2,
    };
    use std::collections::HashMap;

    fn h64(seed: u8) -> Hash64 {
        Hash64::from_bytes([seed; 64])
    }

    fn test_context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-devnet".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x12),
            assignment_id: h64(0x13),
            execution_seed: [0x22; 32],
            model_profile_id: h64(0x31),
            runtime_manifest_hash: h64(0x32),
            runtime_class_id: h64(0x33),
            shape_profile_id: h64(0x34),
            trace_scheme_id: trace_scheme_id_v2(),
            cu_ruleset_id: h64(0x36),
            tokenizer_id: h64(0x37),
            prompt_token_ids_hash: h64(0x38),
            declared_prefill_tokens: 7,
            exact_decode_tokens: 3,
            max_context_tokens: 64,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Domains
    // -----------------------------------------------------------------------------------------

    #[test]
    fn palw_s_domains_are_unique_and_disjoint_from_v2() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_S_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate palw-s domain: {}", String::from_utf8_lossy(d));
            assert!(d.len() <= 64, "blake2b key cap exceeded: {}", String::from_utf8_lossy(d));
        }
        for d in PALW_V2_ALL_DOMAINS {
            assert!(!seen.contains(d), "palw-s reuses a v2 domain: {}", String::from_utf8_lossy(d));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Attestation message — golden-frozen
    // -----------------------------------------------------------------------------------------

    fn op(seed: u8, index: u32) -> crate::tx::TransactionOutpoint {
        crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([seed; 64]), index }
    }

    #[test]
    fn attestation_message_golden_vector() {
        let msg = palw_execution_attestation_message_v3(b"misaka-devnet", h64(0xA1), h64(0xB2), h64(0xC3), h64(0xD4), &op(0xE5, 3));
        // Re-frozen 2026-08-17 for generation 3 (`committed_root` then `bond_outpoint` appended,
        // new domain string each time). A change here is a signing-message layout change: new
        // object version, never an in-place edit. The retired vectors 9fb7e41e… (v1) and
        // 13fea3ce… (v2) go with their domains — see `PALW_S_DOMAIN_ATTESTATION_MESSAGE_V3` for
        // why those strings must not come back.
        assert_eq!(msg.to_string(), "47404fc927b21439a702b563541a1e68909bf99d2797365c796d5d949fa86a34");
    }

    #[test]
    fn attestation_message_binds_every_field() {
        let base = palw_execution_attestation_message_v3(b"net", h64(1), h64(2), h64(3), h64(4), &op(5, 0));
        assert_ne!(base, palw_execution_attestation_message_v3(b"neu", h64(1), h64(2), h64(3), h64(4), &op(5, 0)));
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(9), h64(2), h64(3), h64(4), &op(5, 0)));
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(1), h64(9), h64(3), h64(4), &op(5, 0)));
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(1), h64(2), h64(9), h64(4), &op(5, 0)));
        // The generation-2 field, bound like every other: without this the whole fix is cosmetic.
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(1), h64(2), h64(3), h64(9), &op(5, 0)));
        // The generation-3 field, and its INDEX as well as its transaction id — two outputs of one
        // funding transaction are two different bonds, so a signature for one must not verify for
        // the other.
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(1), h64(2), h64(3), h64(4), &op(9, 0)));
        assert_ne!(base, palw_execution_attestation_message_v3(b"net", h64(1), h64(2), h64(3), h64(4), &op(5, 1)));
    }

    // -----------------------------------------------------------------------------------------
    // Class contradiction
    // -----------------------------------------------------------------------------------------

    /// Mock signature scheme: signature = message bytes ‖ executor id bytes. The adjudicator only
    /// sees the closure verdict, so the mock exercises every code path the real ML-DSA-87 wiring
    /// will drive.
    fn mock_sign(message: &Hash, executor_id: &Hash64) -> Vec<u8> {
        let mut sig = message.as_bytes().to_vec();
        sig.extend_from_slice(executor_id.as_byte_slice());
        sig
    }

    fn mock_verify(message: &Hash, attestation: &PalwExecutionAttestationV1) -> bool {
        attestation.signature == mock_sign(message, &attestation.executor_id)
    }

    fn attested(executor_id: Hash64, context: &PalwJobContextV2, root: Hash64) -> PalwExecutionAttestationV1 {
        let mut attestation = PalwExecutionAttestationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            executor_id,
            job_context_hash: context.context_hash(),
            full_logits_trace_root: root,
            // Bare-v2 shape: the committed object IS the logits root.
            committed_root: root,
            // One bond per signer in these vectors; the equivocation rule is about the SIGNER, and a
            // signer with two bonds telling two stories is still one signer.
            bond_outpoint: op(executor_id.as_bytes()[0], 0),
            signature: vec![],
        };
        let message = attestation.message(&context.network_id);
        attestation.signature = mock_sign(&message, &executor_id);
        attestation
    }

    fn contradiction(a: PalwExecutionAttestationV1, b: PalwExecutionAttestationV1) -> PalwClassContradictionCertificateV1 {
        PalwClassContradictionCertificateV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            job_context: test_context(),
            attestation_a: a,
            attestation_b: b,
        }
    }

    #[test]
    fn same_signer_two_roots_is_equivocation() {
        let ctx = test_context();
        let cert = contradiction(attested(h64(0xE1), &ctx, h64(0x01)), attested(h64(0xE1), &ctx, h64(0x02)));
        let verdict = adjudicate_class_contradiction_v1(&cert, NET, mock_verify).unwrap();
        assert_eq!(verdict.kind, PalwClassContradictionKindV1::ExecutorEquivocation { executor_id: h64(0xE1) });
    }

    #[test]
    fn different_signers_two_roots_is_class_divergence_reading_identities_from_the_context() {
        let ctx = test_context();
        let cert = contradiction(attested(h64(0xE1), &ctx, h64(0x01)), attested(h64(0xE2), &ctx, h64(0x02)));
        let verdict = adjudicate_class_contradiction_v1(&cert, NET, mock_verify).unwrap();
        assert_eq!(
            verdict.kind,
            PalwClassContradictionKindV1::ClassDivergence {
                runtime_class_id: ctx.runtime_class_id,
                runtime_manifest_hash: ctx.runtime_manifest_hash,
                model_profile_id: ctx.model_profile_id,
            }
        );
    }

    #[test]
    fn equal_roots_contradict_nothing() {
        let ctx = test_context();
        let cert = contradiction(attested(h64(0xE1), &ctx, h64(0x01)), attested(h64(0xE2), &ctx, h64(0x01)));
        assert_eq!(adjudicate_class_contradiction_v1(&cert, NET, mock_verify), Err(PalwSlashError::NoContradiction));
    }

    #[test]
    fn evidence_id_is_order_independent() {
        let ctx = test_context();
        let a = attested(h64(0xE1), &ctx, h64(0x01));
        let b = attested(h64(0xE2), &ctx, h64(0x02));
        let v1 = adjudicate_class_contradiction_v1(&contradiction(a.clone(), b.clone()), NET, mock_verify).unwrap();
        let v2 = adjudicate_class_contradiction_v1(&contradiction(b, a), NET, mock_verify).unwrap();
        assert_eq!(v1.evidence_id, v2.evidence_id);
    }

    #[test]
    fn foreign_context_signature_or_shape_faults_are_all_rejected() {
        let ctx = test_context();
        let a = attested(h64(0xE1), &ctx, h64(0x01));
        let b = attested(h64(0xE2), &ctx, h64(0x02));

        // Attestation bound to a different context.
        let mut other_ctx = ctx.clone();
        other_ctx.job_id = h64(0x77);
        let foreign = attested(h64(0xE1), &other_ctx, h64(0x01));
        assert_eq!(
            adjudicate_class_contradiction_v1(&contradiction(foreign, b.clone()), NET, mock_verify),
            Err(PalwSlashError::AttestationContextMismatch { which: "a" })
        );

        // Tampered root after signing: the signature stops verifying.
        let mut tampered = a.clone();
        tampered.full_logits_trace_root = h64(0x55);
        tampered.job_context_hash = ctx.context_hash();
        assert_eq!(
            adjudicate_class_contradiction_v1(&contradiction(tampered, b.clone()), NET, mock_verify),
            Err(PalwSlashError::InvalidSignature { which: "a" })
        );

        // Wrong object version — and 1 specifically, the retired generation whose attestations
        // signed only the logits leg. Those bytes must be refused, not reinterpreted.
        let mut cert = contradiction(a.clone(), b.clone());
        cert.version = 1;
        assert!(matches!(adjudicate_class_contradiction_v1(&cert, NET, mock_verify), Err(PalwSlashError::UnsupportedVersion { .. })));

        // Oversized and empty signatures die at shape level.
        let mut oversized = a.clone();
        oversized.signature = vec![0u8; PALW_S_MAX_SIGNATURE_BYTES + 1];
        assert!(matches!(
            adjudicate_class_contradiction_v1(&contradiction(oversized, b.clone()), NET, mock_verify),
            Err(PalwSlashError::SignatureSizeOutOfRange { .. })
        ));
        let mut empty = a.clone();
        empty.signature = vec![];
        assert!(matches!(
            adjudicate_class_contradiction_v1(&contradiction(empty, b.clone()), NET, mock_verify),
            Err(PalwSlashError::SignatureSizeOutOfRange { .. })
        ));

        // Wrong-scheme context.
        let mut cert = contradiction(a, b);
        cert.job_context.trace_scheme_id = h64(0x99);
        assert_eq!(adjudicate_class_contradiction_v1(&cert, NET, mock_verify), Err(PalwSlashError::SchemeMismatch));
    }

    // -----------------------------------------------------------------------------------------
    // Merkle openings
    // -----------------------------------------------------------------------------------------

    /// Test-local tree builder that records every level, so sibling paths can be extracted and
    /// cross-checked against the production root.
    fn build_levels(event_hashes: &[Hash64]) -> Vec<Vec<Hash64>> {
        let leaves: Vec<Hash64> = event_hashes
            .iter()
            .enumerate()
            .map(|(i, ev)| {
                let mut preimage = Vec::new();
                preimage.extend_from_slice(&(i as u32).to_le_bytes());
                preimage.extend_from_slice(ev.as_byte_slice());
                keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_LEAF, &[&preimage])
            })
            .collect();
        let mut levels = vec![leaves];
        while levels.last().unwrap().len() > 1 {
            let previous = levels.last().unwrap();
            let mut next = Vec::new();
            for pair in previous.chunks(2) {
                match pair {
                    [left, right] => {
                        next.push(keyed64(PALW_V2_DOMAIN_TRACE_MERKLE_NODE, &[left.as_byte_slice(), right.as_byte_slice()]))
                    }
                    [odd] => next.push(*odd),
                    _ => unreachable!(),
                }
            }
            levels.push(next);
        }
        levels
    }

    fn opening_for(levels: &[Vec<Hash64>], event_hashes: &[Hash64], index: usize) -> PalwTraceEventOpeningV1 {
        let mut siblings = Vec::new();
        let mut position = index;
        for level in &levels[..levels.len() - 1] {
            let promoted = level.len() % 2 == 1 && position == level.len() - 1;
            if !promoted {
                siblings.push(level[position ^ 1]);
            }
            position /= 2;
        }
        PalwTraceEventOpeningV1 { event_index: index as u32, event_hash: event_hashes[index], siblings }
    }

    #[test]
    fn opening_agrees_with_production_root_for_every_shape() {
        for count in 1usize..=17 {
            let events: Vec<Hash64> = (0..count).map(|i| h64(0x40 + i as u8)).collect();
            let production_root = trace_event_merkle_root_v2(&events).unwrap();
            let levels = build_levels(&events);
            assert_eq!(levels.last().unwrap()[0], production_root, "mirror diverged from production at count {count}");
            for index in 0..count {
                let opening = opening_for(&levels, &events, index);
                let computed =
                    trace_event_opening_root_v1(count as u32, &opening).unwrap_or_else(|e| panic!("count {count} index {index}: {e}"));
                assert_eq!(computed, production_root, "count {count} index {index}");
            }
        }
    }

    #[test]
    fn opening_rejects_every_malformation() {
        let events: Vec<Hash64> = (0..5).map(|i| h64(0x60 + i as u8)).collect();
        let root = trace_event_merkle_root_v2(&events).unwrap();
        let levels = build_levels(&events);
        let good = opening_for(&levels, &events, 3);
        assert_eq!(trace_event_opening_root_v1(5, &good).unwrap(), root);

        // Tampered sibling: still folds, lands on a different root.
        let mut tampered = good.clone();
        tampered.siblings[0] = h64(0xEE);
        assert_ne!(trace_event_opening_root_v1(5, &tampered).unwrap(), root);

        // Same path claimed for a different index: shape may fold, root must differ.
        let mut wrong_index = good.clone();
        wrong_index.event_index = 2;
        if let Ok(other_root) = trace_event_opening_root_v1(5, &wrong_index) {
            assert_ne!(other_root, root);
        }

        // Missing / extra siblings.
        let mut short = good.clone();
        short.siblings.pop();
        assert!(matches!(trace_event_opening_root_v1(5, &short), Err(PalwSlashError::OpeningPathTooShort { .. })));
        let mut long = good.clone();
        long.siblings.push(h64(0xEF));
        assert!(matches!(trace_event_opening_root_v1(5, &long), Err(PalwSlashError::OpeningPathTooLong { .. })));

        // Wrong declared count: either the derived shape rejects the path, or the root moves.
        if let Ok(other_root) = trace_event_opening_root_v1(4, &good) {
            assert_ne!(other_root, root);
        }

        // Bounds.
        assert!(matches!(trace_event_opening_root_v1(0, &good), Err(PalwSlashError::EventCountOutOfRange { .. })));
        assert!(matches!(
            trace_event_opening_root_v1(PALW_V2_MAX_TRACE_EVENTS as u32 + 1, &good),
            Err(PalwSlashError::EventCountOutOfRange { .. })
        ));
        let mut out_of_range = good.clone();
        out_of_range.event_index = 5;
        assert!(matches!(trace_event_opening_root_v1(5, &out_of_range), Err(PalwSlashError::EventIndexOutOfRange { .. })));
        let mut too_deep = good;
        too_deep.siblings = vec![h64(0xED); PALW_S_MAX_OPENING_SIBLINGS + 1];
        assert!(matches!(trace_event_opening_root_v1(5, &too_deep), Err(PalwSlashError::OpeningTooDeep { .. })));
    }

    #[test]
    fn promoted_single_leaf_tree_opens_with_no_siblings() {
        let events = vec![h64(0x70)];
        let root = trace_event_merkle_root_v2(&events).unwrap();
        let opening = PalwTraceEventOpeningV1 { event_index: 0, event_hash: events[0], siblings: vec![] };
        assert_eq!(trace_event_opening_root_v1(1, &opening).unwrap(), root);
    }

    // -----------------------------------------------------------------------------------------
    // Event-hash equivalence: the adjudication-side hash IS the production hash on honest rows
    // -----------------------------------------------------------------------------------------

    #[test]
    fn refutation_event_hash_matches_production_on_finite_rows() {
        let context_hash = test_context().context_hash();
        let mut scratch = Vec::new();
        for (step, phase) in [(0u32, PalwTracePhaseV2::Prefill), (1, PalwTracePhaseV2::Decode), (2, PalwTracePhaseV2::Decode)] {
            let logits: Vec<f32> = (0..8).map(|i| (i as f32 - 3.5) * (step as f32 + 0.25)).collect();
            let production = logits_event_hash_v2(&context_hash, phase, step, step, 8, &logits, &mut scratch).unwrap();
            let preimage = PalwTraceEventPreimageV1 {
                phase,
                phase_step: step,
                n_vocab: 8,
                logits_count: 8,
                logits_le_bytes: logits.iter().flat_map(|l| l.to_le_bytes()).collect(),
            };
            assert_eq!(refutation_event_hash_v1(&context_hash, &preimage), production, "layout fork at step {step}");
        }
    }

    // -----------------------------------------------------------------------------------------
    // Structural refutations
    // -----------------------------------------------------------------------------------------

    /// An honest 3-event trace (D = 3: one prefill batch + two decode calls) with vocab 8,
    /// assembled through the production path.
    fn honest_commitment() -> (PalwJobContextV2, PalwTraceCommitmentV2, Vec<PalwTraceEventPreimageV1>) {
        let context = test_context();
        let context_hash = context.context_hash();
        let mut scratch = Vec::new();
        let mut event_hashes = Vec::new();
        let mut preimages = Vec::new();
        for (index, (phase, step)) in
            [(PalwTracePhaseV2::Prefill, 0u32), (PalwTracePhaseV2::Decode, 0), (PalwTracePhaseV2::Decode, 1)].iter().enumerate()
        {
            let logits: Vec<f32> = (0..8).map(|i| i as f32 + index as f32 * 0.5).collect();
            event_hashes.push(logits_event_hash_v2(&context_hash, *phase, *step, index as u32, 8, &logits, &mut scratch).unwrap());
            preimages.push(PalwTraceEventPreimageV1 {
                phase: *phase,
                phase_step: *step,
                n_vocab: 8,
                logits_count: 8,
                logits_le_bytes: logits.iter().flat_map(|l| l.to_le_bytes()).collect(),
            });
        }
        let summary = PalwTraceSummaryV2 {
            vocab_size: 8,
            logits_dtype: PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: context.declared_prefill_tokens,
            exact_decode_tokens: context.exact_decode_tokens,
            event_count: 3,
            first_event_kind: PalwTracePhaseV2::Prefill,
            last_event_kind: PalwTracePhaseV2::Decode,
            output_token_ids_hash: h64(0x51),
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        };
        let commitment = PalwTraceCommitmentV2::assemble(context.clone(), summary, event_hashes).unwrap();
        (context, commitment, preimages)
    }

    fn summary_refutation_of(
        context: &PalwJobContextV2,
        summary: PalwTraceSummaryV2,
        merkle_root: Hash64,
    ) -> PalwTraceSummaryRefutationV1 {
        let committed = full_logits_trace_root_v2(&context.context_hash(), &summary, &merkle_root);
        PalwTraceSummaryRefutationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            job_context: context.clone(),
            summary,
            ordered_event_commitment: merkle_root,
            committed_trace_root: committed,
        }
    }

    #[test]
    fn honest_summary_survives_refutation_and_costs_the_challenger() {
        let (context, commitment, _) = honest_commitment();
        let merkle_root = trace_event_merkle_root_v2(&commitment.ordered_event_hashes).unwrap();
        let refutation = summary_refutation_of(&context, commitment.summary.clone(), merkle_root);
        assert_eq!(refutation.committed_trace_root, commitment.full_logits_sequence_root);
        assert_eq!(check_trace_summary_refutation_v1(&refutation), Err(PalwSlashError::NoFaultFound));
    }

    #[test]
    fn summary_faults_mirror_assemble_rejections_exactly() {
        let (context, commitment, _) = honest_commitment();
        let merkle_root = trace_event_merkle_root_v2(&commitment.ordered_event_hashes).unwrap();
        let honest = &commitment.summary;

        let cases: Vec<(PalwTraceSummaryV2, PalwTraceStructuralFaultV1)> = vec![
            (PalwTraceSummaryV2 { event_count: 4, ..honest.clone() }, PalwTraceStructuralFaultV1::EventCountNotExactDecode),
            (
                PalwTraceSummaryV2 { declared_prefill_tokens: 9, ..honest.clone() },
                PalwTraceStructuralFaultV1::SummaryTokenCountsDisagree,
            ),
            (
                PalwTraceSummaryV2 { first_event_kind: PalwTracePhaseV2::Decode, ..honest.clone() },
                PalwTraceStructuralFaultV1::FirstEventNotPrefill,
            ),
            (
                PalwTraceSummaryV2 { last_event_kind: PalwTracePhaseV2::Prefill, ..honest.clone() },
                PalwTraceStructuralFaultV1::LastEventKindWrong,
            ),
        ];
        for (bad_summary, expected_fault) in cases {
            // The producer path must refuse this summary…
            assert!(
                PalwTraceCommitmentV2::assemble(context.clone(), bad_summary.clone(), commitment.ordered_event_hashes.clone())
                    .is_err(),
                "assemble accepted a summary the refutation logic calls fraudulent: {expected_fault:?}"
            );
            // …and the refutation path must convict a root that carries it anyway.
            let refutation = summary_refutation_of(&context, bad_summary, merkle_root);
            let verdict = check_trace_summary_refutation_v1(&refutation).unwrap();
            assert_eq!(verdict.fault, expected_fault);
        }
    }

    #[test]
    fn summary_refutation_must_address_the_committed_root() {
        let (context, commitment, _) = honest_commitment();
        let merkle_root = trace_event_merkle_root_v2(&commitment.ordered_event_hashes).unwrap();
        let mut refutation =
            summary_refutation_of(&context, PalwTraceSummaryV2 { event_count: 4, ..commitment.summary.clone() }, merkle_root);
        refutation.committed_trace_root = h64(0x99);
        assert_eq!(check_trace_summary_refutation_v1(&refutation), Err(PalwSlashError::CommittedRootMismatch));
    }

    fn event_refutation_of(
        context: &PalwJobContextV2,
        commitment: &PalwTraceCommitmentV2,
        index: usize,
        preimage: PalwTraceEventPreimageV1,
    ) -> PalwTraceEventRefutationV1 {
        let levels = build_levels(&commitment.ordered_event_hashes);
        PalwTraceEventRefutationV1 {
            version: PALW_S_OBJECT_VERSION_V3,
            job_context: context.clone(),
            summary: commitment.summary.clone(),
            ordered_event_commitment: trace_event_merkle_root_v2(&commitment.ordered_event_hashes).unwrap(),
            committed_trace_root: commitment.full_logits_sequence_root,
            opening: opening_for(&levels, &commitment.ordered_event_hashes, index),
            event_preimage: preimage,
        }
    }

    /// Builds a commitment whose event 1 has an adversarial preimage (crafted with the
    /// adjudication-side hash — the production hash refuses to produce it), wrapped in an
    /// otherwise-consistent summary. This is exactly the fraud shape only an opening can refute.
    fn commitment_with_bad_event(
        bad: PalwTraceEventPreimageV1,
    ) -> (PalwJobContextV2, PalwTraceCommitmentV2, PalwTraceEventPreimageV1) {
        let (context, honest, preimages) = honest_commitment();
        let context_hash = context.context_hash();
        let mut event_hashes = honest.ordered_event_hashes.clone();
        event_hashes[1] = refutation_event_hash_v1(&context_hash, &bad);
        let commitment = PalwTraceCommitmentV2::assemble(context.clone(), honest.summary.clone(), event_hashes).unwrap();
        drop(preimages);
        (context, commitment, bad)
    }

    #[test]
    fn non_finite_logit_is_refuted_at_its_index() {
        let mut bytes: Vec<u8> = (0..8).flat_map(|i| (i as f32).to_le_bytes()).collect();
        bytes[5 * 4..6 * 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let bad = PalwTraceEventPreimageV1 {
            phase: PalwTracePhaseV2::Decode,
            phase_step: 0,
            n_vocab: 8,
            logits_count: 8,
            logits_le_bytes: bytes,
        };
        let (context, commitment, bad) = commitment_with_bad_event(bad);
        let verdict = check_trace_event_refutation_v1(&event_refutation_of(&context, &commitment, 1, bad)).unwrap();
        assert_eq!(verdict.fault, PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: 5 });

        // Infinity is convicted by the same rule.
        let mut bytes: Vec<u8> = (0..8).flat_map(|i| (i as f32).to_le_bytes()).collect();
        bytes[0..4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
        let bad = PalwTraceEventPreimageV1 {
            phase: PalwTracePhaseV2::Decode,
            phase_step: 0,
            n_vocab: 8,
            logits_count: 8,
            logits_le_bytes: bytes,
        };
        let (context, commitment, bad) = commitment_with_bad_event(bad);
        let verdict = check_trace_event_refutation_v1(&event_refutation_of(&context, &commitment, 1, bad)).unwrap();
        assert_eq!(verdict.fault, PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: 0 });
    }

    #[test]
    fn short_row_wrong_vocab_and_misaligned_bytes_are_refuted() {
        // logits_count declared as vocab but only half the bytes present → misaligned.
        let bad = PalwTraceEventPreimageV1 {
            phase: PalwTracePhaseV2::Decode,
            phase_step: 0,
            n_vocab: 8,
            logits_count: 8,
            logits_le_bytes: (0..4).flat_map(|i| (i as f32).to_le_bytes()).collect(),
        };
        let (context, commitment, bad) = commitment_with_bad_event(bad);
        let verdict = check_trace_event_refutation_v1(&event_refutation_of(&context, &commitment, 1, bad)).unwrap();
        assert_eq!(verdict.fault, PalwTraceStructuralFaultV1::EventBytesNotFourPerLogit);

        // Consistent short row: count = 4, bytes = 16, but vocab says 8.
        let bad = PalwTraceEventPreimageV1 {
            phase: PalwTracePhaseV2::Decode,
            phase_step: 0,
            n_vocab: 8,
            logits_count: 4,
            logits_le_bytes: (0..4).flat_map(|i| (i as f32).to_le_bytes()).collect(),
        };
        let (context, commitment, bad) = commitment_with_bad_event(bad);
        let verdict = check_trace_event_refutation_v1(&event_refutation_of(&context, &commitment, 1, bad)).unwrap();
        assert_eq!(verdict.fault, PalwTraceStructuralFaultV1::EventLogitsCountNotVocab);

        // Internally consistent event, but its vocab is not the committed profile vocab.
        let bad = PalwTraceEventPreimageV1 {
            phase: PalwTracePhaseV2::Decode,
            phase_step: 0,
            n_vocab: 4,
            logits_count: 4,
            logits_le_bytes: (0..4).flat_map(|i| (i as f32).to_le_bytes()).collect(),
        };
        let (context, commitment, bad) = commitment_with_bad_event(bad);
        let verdict = check_trace_event_refutation_v1(&event_refutation_of(&context, &commitment, 1, bad)).unwrap();
        assert_eq!(verdict.fault, PalwTraceStructuralFaultV1::EventVocabNotProfile);
    }

    #[test]
    fn honest_event_survives_refutation_and_costs_the_challenger() {
        let (context, commitment, preimages) = honest_commitment();
        for (index, preimage) in preimages.into_iter().enumerate() {
            let refutation = event_refutation_of(&context, &commitment, index, preimage);
            assert_eq!(check_trace_event_refutation_v1(&refutation), Err(PalwSlashError::NoFaultFound), "index {index}");
        }
    }

    #[test]
    fn event_refutation_rejects_unaddressed_or_mismatched_evidence() {
        let (context, commitment, preimages) = honest_commitment();

        // Preimage that is not the opened event.
        let mut wrong_preimage = event_refutation_of(&context, &commitment, 1, preimages[1].clone());
        wrong_preimage.event_preimage.phase_step = 7;
        assert_eq!(check_trace_event_refutation_v1(&wrong_preimage), Err(PalwSlashError::EventPreimageMismatch));

        // Opening that does not land on the committed Merkle root.
        let mut wrong_opening = event_refutation_of(&context, &commitment, 1, preimages[1].clone());
        wrong_opening.opening.siblings[0] = h64(0xAB);
        assert_eq!(check_trace_event_refutation_v1(&wrong_opening), Err(PalwSlashError::OpeningRootMismatch));

        // Outer preimage that does not recompute the committed trace root.
        let mut wrong_root = event_refutation_of(&context, &commitment, 1, preimages[1].clone());
        wrong_root.committed_trace_root = h64(0xCD);
        assert_eq!(check_trace_event_refutation_v1(&wrong_root), Err(PalwSlashError::CommittedRootMismatch));

        // Oversized logits payload dies at shape level.
        let mut oversized = event_refutation_of(&context, &commitment, 1, preimages[1].clone());
        oversized.event_preimage.logits_le_bytes = vec![0u8; PALW_S_MAX_LOGITS_BYTES + 1];
        assert!(matches!(check_trace_event_refutation_v1(&oversized), Err(PalwSlashError::LogitsBytesTooLarge { .. })));
    }

    #[test]
    fn refutation_evidence_ids_separate_by_root_index_and_fault() {
        let root_a = h64(0x01);
        let root_b = h64(0x02);
        let fault_a = PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: 3 };
        let fault_b = PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: 4 };
        let mut ids = vec![
            refutation_evidence_id(b"event/", &root_a, 1, fault_a),
            refutation_evidence_id(b"event/", &root_b, 1, fault_a),
            refutation_evidence_id(b"event/", &root_a, 2, fault_a),
            refutation_evidence_id(b"event/", &root_a, 1, fault_b),
            refutation_evidence_id(b"summary/", &root_a, 0, PalwTraceStructuralFaultV1::EventCountNotExactDecode),
        ];
        ids.sort_by(|a, b| a.as_byte_slice().cmp(b.as_byte_slice()));
        ids.dedup();
        assert_eq!(ids.len(), 5, "evidence ids collided");
    }

    // -----------------------------------------------------------------------------------------
    // Wire stability
    // -----------------------------------------------------------------------------------------

    #[test]
    fn borsh_roundtrips_and_fault_discriminants_are_frozen() {
        let (context, commitment, preimages) = honest_commitment();
        let refutation = event_refutation_of(&context, &commitment, 1, preimages[1].clone());
        let bytes = borsh::to_vec(&refutation).unwrap();
        assert_eq!(PalwTraceEventRefutationV1::try_from_slice(&bytes).unwrap(), refutation);

        let ctx = test_context();
        let cert = contradiction(attested(h64(0xE1), &ctx, h64(0x01)), attested(h64(0xE2), &ctx, h64(0x02)));
        let bytes = borsh::to_vec(&cert).unwrap();
        assert_eq!(PalwClassContradictionCertificateV1::try_from_slice(&bytes).unwrap(), cert);

        // Discriminants are wire: byte 0 is the variant tag, and the data variant carries its
        // index little-endian after the tag.
        let frozen: Vec<(PalwTraceStructuralFaultV1, u8)> = vec![
            (PalwTraceStructuralFaultV1::EventCountNotExactDecode, 0),
            (PalwTraceStructuralFaultV1::FirstEventNotPrefill, 1),
            (PalwTraceStructuralFaultV1::LastEventKindWrong, 2),
            (PalwTraceStructuralFaultV1::SummaryTokenCountsDisagree, 3),
            (PalwTraceStructuralFaultV1::EventBytesNotFourPerLogit, 4),
            (PalwTraceStructuralFaultV1::EventLogitsCountNotVocab, 5),
            (PalwTraceStructuralFaultV1::EventVocabNotProfile, 6),
            (PalwTraceStructuralFaultV1::EventNonFiniteLogit { logit_index: 0x0102_0304 }, 7),
        ];
        for (fault, tag) in frozen {
            let bytes = borsh::to_vec(&fault).unwrap();
            assert_eq!(bytes[0], tag, "discriminant moved for {fault:?}");
            assert_eq!(PalwTraceStructuralFaultV1::try_from_slice(&bytes).unwrap(), fault);
        }
    }

    /// The adjudicator never trusts the closure with anything but the message: a verifier keyed
    /// by executor id (the realistic registry shape) drives the same paths.
    #[test]
    fn registry_shaped_verifier_works_through_the_closure() {
        let ctx = test_context();
        let a = attested(h64(0xE1), &ctx, h64(0x01));
        let b = attested(h64(0xE2), &ctx, h64(0x02));
        let mut registry: HashMap<Hash64, ()> = HashMap::new();
        registry.insert(h64(0xE1), ());
        registry.insert(h64(0xE2), ());
        let verdict = adjudicate_class_contradiction_v1(&contradiction(a, b), NET, |message, attestation| {
            registry.contains_key(&attestation.executor_id) && mock_verify(message, attestation)
        })
        .unwrap();
        assert!(matches!(verdict.kind, PalwClassContradictionKindV1::ClassDivergence { .. }));
    }
}
