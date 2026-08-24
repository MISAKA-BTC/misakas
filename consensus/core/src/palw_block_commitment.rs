//! ADR-0038 Decision A: the block-carried PALW commitment — what a full node checks INSTEAD
//! of running the model.
//!
//! ## Why this is not a coinbase payload
//!
//! The commitment root is a function of the winning inference, which is a function of the
//! winning `nonce` — but the coinbase sits under `hash_merkle_root`, which sits under
//! `pre_pow_hash`, which is fixed BEFORE grinding. A coinbase-carried commitment is
//! therefore circular. The commitment must live in the post-PoW region of the header, like
//! `nonce` itself: **excluded from `pre_pow_hash`, included in the block hash** (the
//! Stage-1 header wiring adds that field; this module owns its payload, binding and shape).
//!
//! ## How admission stays runtime-free
//!
//! The current algo-4 verifier recomputes the 200-byte L1 tag by re-running inference —
//! the audited fatal coupling. Under ADR-0038 the header's claimed commitment supplies the
//! tag bytes ([`PalwBlockCommitmentV1::l1_tag_bytes`]), so admission is:
//!
//! ```text
//! finalizer(network, algo, pre_pow_hash, timestamp, bits, nonce, claimed_tag) < class_target
//! ```
//!
//! — a hash check. Whether the claimed tag honestly derives from `inference(challenge)`
//! is what assigned sampling ([`crate::palw_receipt`]) and the court decide, under the
//! weight ramp ([`crate::palw_weight`]): a fabricated tag passes admission and matures
//! never; fabricators pay bond slash (ADR-0038 New-risk 1).
//!
//! ## The challenge binding — why the tag is not a free grind
//!
//! The first draft of this module derived the tag from a root over `(class, bond, trace_root,
//! output_root, pwu_claim)` **only**, and claimed "honest miners still pay one full inference
//! per ticket (the lottery shape is unchanged)". That claim was false, and the gap is the
//! whole reason this section exists (mainnet-readiness re-audit 2026-08-17, blocker 3).
//!
//! None of those five fields depends on the `nonce`, while the Layer-0 finalizer mixes the
//! nonce separately. So a miner ran **one** inference, built **one** commitment, and then
//! ground `nonce` through the finalizer for free: one inference amortized across the whole
//! nonce space — a classic hash puzzle with a one-time setup cost, which is precisely the
//! property ADR-0038 exists to avoid. Worse, that grind is *invisible to sampling*: the
//! committed `trace_root` is a genuine inference, so a re-running verifier has nothing to
//! contradict — the commitment never said which attempt it belonged to.
//!
//! The fix is [`palw_block_challenge_v1`]: every tag derives from a challenge that binds the
//! exact attempt — network, `pre_pow_hash`, timestamp, **nonce**, class and executor bond.
//! Change any of them and the challenge changes, the root changes, the tag changes, and the
//! honest miner owes a new inference. Two properties follow, and they are *different in kind*:
//!
//! * **Cryptographic, and checkable by a node that never runs the model:** a commitment cannot
//!   be replayed onto another nonce, header, class or executor. The verifier recomputes the
//!   challenge from the header and the commitment's own fields; nothing else can produce that
//!   tag. This is what kills the amortization above, and it holds unconditionally.
//! * **Economic, and *not* checkable at admission:** that `trace_root` is the true
//!   `F(challenge, class)` rather than 32 random bytes. A grinder can still pick roots freely
//!   and win tickets at hash cost. What stops that is not this module — it is that the
//!   challenge is now *declared*, so an assigned verifier knows exactly which inference to
//!   re-run, finds the mismatch, and convicts. Optimistic verification cannot make this half
//!   cryptographic; it can only make it accountable, and the accountability requires the bond,
//!   the sampling and the court to actually exist.
//!
//! So: "one ticket costs one inference" is true for an honest miner and enforced *economically*
//! for everyone else. Stating it as an unconditional property of the construction — as the
//! first draft did — overstates it, and the overstatement is what hid the nonce grind.
//!
//! Consensus-inert until the ADR-0038 change set wires and activates together.

use crate::tx::TransactionOutpoint;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash, Hash64};
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Domains and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b domain of the executor's commitment signing digest.
pub const PALW_BLOCK_COMMITMENT_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/block-commitment-message/v1";

/// ML-DSA-87 signing context for a block commitment.
pub const PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/block-commitment/mldsa87/v1";

/// Keyed-BLAKE2b domain of the L1 tag expansion (commitment → 200 tag bytes).
pub const PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG: &[u8] = b"misaka-palw/block-commitment-l1-tag/v1";

/// Keyed-BLAKE2b domain of the per-attempt execution challenge — the model's input identity.
pub const PALW_BLOCK_COMMITMENT_DOMAIN_CHALLENGE: &[u8] = b"misaka-palw/block-challenge/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_BLOCK_COMMITMENT_ALL_DOMAINS: &[&[u8]] =
    &[PALW_BLOCK_COMMITMENT_DOMAIN_MESSAGE, PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG, PALW_BLOCK_COMMITMENT_DOMAIN_CHALLENGE];

/// The canonical execution challenge for one ticket attempt: **the thing the model is run on,
/// and the thing a verifier recomputes to know which inference to re-run.**
///
/// Binds, in this frozen order and each length-prefixed or fixed-width so no two distinct
/// inputs can share a preimage:
///
/// * `network_id` — no cross-network replay;
/// * `pre_pow_hash` — the parents, merkle root and bits this attempt is against;
/// * `timestamp`, `nonce` — **the attempt itself**; this is the binding whose absence made the
///   tag a free hash grind (see the module docs);
/// * `execution_class_id` — the difficulty domain, so a trace cannot be moved between classes;
/// * `executor_bond_outpoint` — the accountable identity, so a trace cannot be re-mined by
///   another executor (and so the slash target is inside what the work commits to).
///
/// `bits` is deliberately absent: it already sits inside `pre_pow_hash`, and re-mixing a value
/// twice buys nothing while adding a second place to get the ordering wrong.
pub fn palw_block_challenge_v1(
    network_id: &[u8],
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    execution_class_id: Hash64,
    executor_bond_outpoint: &TransactionOutpoint,
) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BLOCK_COMMITMENT_DOMAIN_CHALLENGE).to_state();
    state.update(&(network_id.len() as u32).to_le_bytes());
    state.update(network_id);
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(execution_class_id.as_byte_slice());
    state.update(executor_bond_outpoint.transaction_id.as_byte_slice());
    state.update(&executor_bond_outpoint.index.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Serialization magic of the carried payload (refuses foreign bytes before borsh runs).
pub const PALW_BLOCK_COMMITMENT_MAGIC: [u8; 4] = *b"PBC1";

/// ADR-0038 Decision A: the fence that lets a PALW header carry its commitment at all.
///
/// Until this is installed, `pow_layer0::check_palw_commitment_shape` requires the field to be
/// EMPTY on a PALW header — the `PalwCommitmentNotYetBound` refusal. That refusal is not a bug: a
/// hash-visible field nobody validates is worse than an absent one, so the field stayed closed
/// until something could check it. This opens it, and only from `activation_daa_score`.
///
/// **`None` on every shipped preset.** Installing it changes block validity — a header that was
/// valid with an empty commitment is not valid with a malformed one, and vice versa — so it is
/// part of the consensus fingerprint, written Some-only so a network that does not install one
/// fingerprints byte-identically to before this field existed.
///
/// This is Decision A's *first* clause only. The block still names no bond that anything checks
/// (`executor references an Active bond of an Active ExecutionClass`), the ticket is still not
/// compared against a class target, and the PWU is still not derived — all of which need chain
/// state this pure shape gate does not have. What it buys is the precondition those need: a block
/// that can say who mined it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBlockCommitmentParamsV1 {
    /// From this DAA score a PALW header MUST carry a well-formed PBC1 commitment; below it the
    /// field must still be empty, so the transition is a clean fork point rather than a window in
    /// which both shapes are legal.
    pub activation_daa_score: u64,
}

impl PalwBlockCommitmentParamsV1 {
    /// Whether a PALW header at `daa_score` must carry its commitment.
    #[inline]
    #[must_use]
    pub fn is_bound(&self, daa_score: u64) -> bool {
        daa_score >= self.activation_daa_score
    }
}

pub const PALW_BLOCK_COMMITMENT_VERSION_V1: u16 = 1;

/// The PALW L1 tag width the Layer-0 finalizer consumes (matches the algo-4 tag width, so
/// the finalizer construction is unchanged by ADR-0038 — only the tag's SOURCE moves from
/// "re-run the model" to "the header's claim").
pub const PALW_BLOCK_COMMITMENT_L1_TAG_BYTES: usize = 200;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwBlockCommitmentError {
    #[error("unsupported block-commitment version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("pwu claim is zero — a block claiming no work is a hash block wearing the wrong algo id")]
    ZeroPwuClaim,
    #[error("payload does not start with the PBC1 magic")]
    BadMagic,
    /// ADR-0038 W8: the block names a bond that is not Active at the point it is judged.
    #[error("the executor bond {outpoint:?} is not an active bond at DAA {pov_daa_score} — no bond, no block (ADR-0038 W8)")]
    ExecutorBondNotActive { outpoint: TransactionOutpoint, pov_daa_score: u64 },
    #[error("payload failed to decode: {reason}")]
    Undecodable { reason: &'static str },
    #[error("payload carries {got} trailing bytes after the commitment")]
    TrailingBytes { got: usize },
}

// ---------------------------------------------------------------------------------------------
// The commitment
// ---------------------------------------------------------------------------------------------

/// The post-PoW header extension payload: everything a sampler, a refuter and the credit
/// path need to hold this block's work accountable — bound to the exact ticket attempt and
/// signed by the bonded executor (ADR-0038 Decision A: no bond, no block — W8).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBlockCommitmentV1 {
    /// = [`PALW_BLOCK_COMMITMENT_VERSION_V1`].
    pub version: u16,
    /// The difficulty domain this block mined under ([`crate::palw_class_daa`]).
    pub execution_class_id: Hash64,
    /// The executor's bond — accountable identity, slash target, and payee (I4, W8).
    pub executor_bond_outpoint: TransactionOutpoint,
    /// The execution trace's root — **what samplers open**, so it must be openable.
    ///
    /// The only legal source is the v2 projection's `full_logits_trace_root`
    /// ([`crate::palw_v2::full_logits_trace_root_v2`]), which wraps a real Merkle tree over the
    /// ordered event hashes; [`crate::palw_v2::trace_event_opening_v2`] is the opening a sampler
    /// gets. **Not** the layer-0 `gemm_trace_root`, which is one flat digest over concatenated
    /// events: a commitment carrying that would name a trace nobody can challenge, so every
    /// dispute over the block terminates `Unadjudicable` — rejected but unslashed, and under
    /// ADR-0038 I10 it freezes the class instead of holding the block to anything.
    pub trace_root: Hash64,
    /// The output/token stream's commitment — [`crate::palw_v2::output_commitment_v2`].
    ///
    /// **Flat, and deliberately so**, which this doc used to obscure by calling it a Merkle root
    /// beside a field that genuinely is one. Nothing opens it: the token ids are bounded by the
    /// job's `exact_decode_tokens`, so a dispute carries the stream whole rather than a path into
    /// it, and the stream is already tied to the trace because
    /// [`crate::palw_v2::output_token_ids_hash_v2`] is bound inside the trace root's summary. The
    /// asymmetry with `trace_root` is the size of what is committed — one full-logits row per
    /// token cannot be carried whole, and a token id list can.
    pub output_root: Hash64,
    /// The canonical PWU this block claims under its class's frozen derivation.
    ///
    /// **Exactly one value is legal**, and it is not chosen here: see
    /// [`crate::palw_pwu::check_pwu_claim_v1`], which requires equality with
    /// `expected_attempts(class_target) × pwu_per_inference(class)`. Both factors are facts —
    /// the class's own DAA target and its registered normative operation count — so this field
    /// is a restatement of chain state that the ticket root and the executor's signature bind,
    /// never a miner input. [`Self::validate_shape`] cannot enforce that (it has no class
    /// state); [`Self::validate_against_class_v1`] does.
    pub pwu_claim: u64,
    /// ML-DSA-87 over [`palw_block_commitment_message_v1`] under
    /// [`PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT`]. Verified statefully (the bond registry
    /// resolves the key); shape checks the length.
    pub signature: Vec<u8>,
}

impl PalwBlockCommitmentV1 {
    /// Stateless shape admission. Stateful questions (bond Active, class Active and equal to
    /// a registered domain, signature validity, pwu_claim equal to the class derivation) are
    /// consumer-entry checks.
    pub fn validate_shape(&self) -> Result<(), PalwBlockCommitmentError> {
        if self.version != PALW_BLOCK_COMMITMENT_VERSION_V1 {
            return Err(PalwBlockCommitmentError::UnsupportedVersion {
                got: self.version,
                expected: PALW_BLOCK_COMMITMENT_VERSION_V1,
            });
        }
        if self.pwu_claim == 0 {
            return Err(PalwBlockCommitmentError::ZeroPwuClaim);
        }
        let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        if self.signature.len() != expected {
            return Err(PalwBlockCommitmentError::SignatureLength { got: self.signature.len(), expected });
        }
        Ok(())
    }

    /// The stateful half of admission that [`Self::validate_shape`] cannot do: `pwu_claim` must
    /// EQUAL its derivation from the class's own DAA target and registered per-inference cost.
    ///
    /// Split from `validate_shape` rather than folded into it because the two have different
    /// inputs and different call sites: shape is a pure function of the payload and runs
    /// wherever bytes arrive, while this needs the class's chain state and therefore runs where
    /// that state is resolved. Keeping them apart is what stops the stateful check being
    /// quietly skipped by a caller that only had bytes — the shape function's own doc says its
    /// stateful questions are consumer-entry checks, and this is one of them, now callable.
    pub fn validate_against_class_v1(&self, class_target: u128, pwu_per_inference: u64) -> Result<(), crate::palw_pwu::PalwPwuError> {
        crate::palw_pwu::check_pwu_claim_v1(self.pwu_claim, class_target, pwu_per_inference)
    }

    /// ADR-0038 **W8**: the executor's bond must be an ACTIVE bond at the point the block is
    /// judged — "no bond, no block".
    ///
    /// The third stateful half, kept apart from [`Self::validate_shape`] and
    /// [`Self::validate_against_class_v1`] for the reason those two are apart: each needs a
    /// different input, so folding them together would let a caller who only had one of them
    /// satisfy the signature and skip the rest. A caller with bytes can call the first, a caller
    /// with class state the second, and a caller with a bond view this.
    ///
    /// Why it is load-bearing rather than decoration, in ADR-0038's own words: *"§New-risk 1 shows
    /// the design is unsound without it"*. The closure argument prices fabricated work at
    /// `−bond × P(conviction)`; with no bond the deterrent is zero and the sampled-verification
    /// regime the ADR replaces exhaustive checking with has nothing behind it. It is exactly the
    /// clause that lets a network stop re-running every inference, which is why it must land
    /// before, not after, that switch.
    ///
    /// Returns the bond so the caller can use it for the payee without a second lookup that could
    /// resolve differently — the B5 defect, in miniature.
    pub fn validate_executor_bond_v1<'a>(
        &self,
        bonds: &'a crate::dns_finality::ActiveBondView,
        pov_daa_score: u64,
    ) -> Result<&'a crate::dns_finality::StakeBondRecord, PalwBlockCommitmentError> {
        bonds
            .active_bond_at(&self.executor_bond_outpoint, pov_daa_score)
            .ok_or(PalwBlockCommitmentError::ExecutorBondNotActive { outpoint: self.executor_bond_outpoint, pov_daa_score })
    }

    /// This commitment's challenge for a given attempt — [`palw_block_challenge_v1`] with the
    /// class and bond taken from `self`, so a caller cannot accidentally bind a trace to a
    /// class or executor other than the one it claims.
    pub fn challenge_for(&self, network_id: &[u8], pre_pow_hash: Hash64, timestamp: u64, nonce: u64) -> Hash64 {
        palw_block_challenge_v1(network_id, pre_pow_hash, timestamp, nonce, self.execution_class_id, &self.executor_bond_outpoint)
    }

    /// The commitment root: one digest over **the challenge**, the class, the bond, both Merkle
    /// roots and the pwu claim — what receipts cover ([`crate::palw_receipt`]'s
    /// `target_commitment_root`) and what the ticket's tag expands from. The signature is NOT
    /// inside (a root must be recomputable by a verifier who has not resolved the key yet).
    ///
    /// The `challenge` parameter is the ADR-0038 blocker-3 fix and the reason this is not a
    /// no-argument method: a root — and therefore a ticket — is meaningless except relative to
    /// one attempt. Taking it by argument rather than storing it keeps a single source of truth
    /// (the header supplies the attempt; the commitment cannot disagree with itself) and makes
    /// "which inference does this claim?" unanswerable-by-omission at the type level.
    pub fn commitment_root(&self, challenge: Hash64) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG).to_state();
        state.update(&[0u8]); // leaf discriminator: root preimage, not tag expansion
        state.update(challenge.as_byte_slice());
        state.update(self.execution_class_id.as_byte_slice());
        state.update(self.executor_bond_outpoint.transaction_id.as_byte_slice());
        state.update(&self.executor_bond_outpoint.index.to_le_bytes());
        state.update(self.trace_root.as_byte_slice());
        state.update(self.output_root.as_byte_slice());
        state.update(&self.pwu_claim.to_le_bytes());
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// **Bind an attempt's L1 tag to this commitment** — the fix for "one PoW solution, unlimited
    /// distinct block identities" (external audit P0-1).
    ///
    /// The block identity hash covers `palw_commitment`; every PoW-path digest excludes it. So a
    /// miner who solves once can swap `trace_root`, `output_root` or `executor_bond_outpoint`, keep
    /// the same PoW, and mint sibling blocks without limit. Mixing the commitment root into the tag
    /// closes that: one bit of the commitment moves the root, the root moves the tag, the tag moves
    /// the finalizer digest, and the PoW fails.
    ///
    /// **This is NOT [`Self::l1_tag_bytes`], and the difference is the whole safety argument.**
    /// That function REPLACES the inference with an expansion of the commitment root — it is the W1
    /// change, and it makes producing a tag free. Free tags are safe only once a bond's immature
    /// exposure is capped in consensus (audit P0-10); until then, replacing the work is what turns
    /// fake-root grinding from expensive into cheap. This function keeps the inference as the work
    /// and only BINDS the commitment to it, so it can land on its own.
    ///
    /// Same width as the input, so the finalizer's call shape is unchanged.
    pub fn bind_l1_tag_v1(inference_tag: &[u8], commitment_root: Hash64) -> [u8; PALW_BLOCK_COMMITMENT_L1_TAG_BYTES] {
        let mut out = [0u8; PALW_BLOCK_COMMITMENT_L1_TAG_BYTES];
        for (chunk_index, chunk) in out.chunks_mut(64).enumerate() {
            let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG).to_state();
            // Leaf discriminator 2, distinct from `l1_tag_bytes`'s 1: the two must never produce the
            // same bytes for the same root, or a network could be moved between the bound-work and
            // bound-inference regimes without the digest noticing.
            state.update(&[2u8]);
            state.update(&(inference_tag.len() as u32).to_le_bytes());
            state.update(inference_tag);
            state.update(commitment_root.as_byte_slice());
            state.update(&(chunk_index as u32).to_le_bytes());
            chunk.copy_from_slice(&state.finalize().as_bytes()[..chunk.len()]);
        }
        out
    }

    /// The 200 tag bytes the Layer-0 finalizer consumes in place of the re-run inference:
    /// a domain-keyed expansion of the commitment root for **this attempt's challenge**
    /// (deterministic, admission-checkable by any CPU). The expansion width keeps the
    /// finalizer call-shape identical to today's algo-4.
    ///
    /// Because the challenge carries the nonce, a new ticket attempt is a new tag: the miner
    /// cannot reuse one inference across the nonce space. What this does NOT establish is that
    /// `trace_root` is the honest `F(challenge, class)` — see the module docs on the
    /// cryptographic/economic split.
    pub fn l1_tag_bytes(&self, challenge: Hash64) -> [u8; PALW_BLOCK_COMMITMENT_L1_TAG_BYTES] {
        let root = self.commitment_root(challenge);
        let mut out = [0u8; PALW_BLOCK_COMMITMENT_L1_TAG_BYTES];
        for (chunk_index, chunk) in out.chunks_mut(64).enumerate() {
            let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG).to_state();
            state.update(&[1u8]); // leaf discriminator: tag expansion
            state.update(root.as_byte_slice());
            state.update(&(chunk_index as u32).to_le_bytes());
            chunk.copy_from_slice(&state.finalize().as_bytes()[..chunk.len()]);
        }
        out
    }

    /// The digest this commitment's signature must cover: the payload fields AND the exact
    /// ticket attempt — so a signed commitment can never be replayed onto a different
    /// header, timestamp or nonce (the ADR-0038 non-transferability of W2, carried into
    /// the signature layer).
    ///
    /// The attempt now reaches this digest twice over: once directly, and once through the
    /// challenge inside `commitment_root`. That redundancy is deliberate — it was the *only*
    /// binding before blocker 3 was fixed, and it bound the half nobody checks (signatures are
    /// not verified at admission today) while the ticket, which everybody checks, bound
    /// nothing. Keeping both means the two layers cannot drift apart again.
    pub fn message(&self, network_id: &[u8], pre_pow_hash: Hash64, timestamp: u64, nonce: u64) -> Hash {
        let challenge = self.challenge_for(network_id, pre_pow_hash, timestamp, nonce);
        let mut state = blake2b_simd::Params::new().hash_length(32).key(PALW_BLOCK_COMMITMENT_DOMAIN_MESSAGE).to_state();
        state.update(&(network_id.len() as u32).to_le_bytes());
        state.update(network_id);
        state.update(pre_pow_hash.as_byte_slice());
        state.update(&timestamp.to_le_bytes());
        state.update(&nonce.to_le_bytes());
        state.update(self.commitment_root(challenge).as_byte_slice());
        let mut out = [0u8; 32];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash::from_bytes(out)
    }

    /// Encode with the PBC1 magic (the header-extension wire form).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_BLOCK_COMMITMENT_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Decode a header-extension payload: magic, then borsh, then an exact-length check
    /// (trailing bytes are refused — a payload is not a container).
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwBlockCommitmentError> {
        let Some(body) = bytes.strip_prefix(&PALW_BLOCK_COMMITMENT_MAGIC) else {
            return Err(PalwBlockCommitmentError::BadMagic);
        };
        let mut slice = body;
        let decoded = <Self as borsh::BorshDeserialize>::deserialize(&mut slice)
            .map_err(|_| PalwBlockCommitmentError::Undecodable { reason: "borsh body" })?;
        if !slice.is_empty() {
            return Err(PalwBlockCommitmentError::TrailingBytes { got: slice.len() });
        }
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;

    const NET: &[u8] = b"misaka-testnet-11";

    /// A fixed challenge for tests whose subject is the payload binding, not the attempt.
    fn ch() -> Hash64 {
        Hash64::from_u64_word(0xC0FFEE)
    }

    fn commitment() -> PalwBlockCommitmentV1 {
        PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_u64_word(1),
            executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(2), 3),
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            pwu_claim: 100,
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// Domains are unique against every other PALW family (incl. V3 job and receipt).
    #[test]
    fn domains_are_unique_across_all_palw_families() {
        let mut all: Vec<&[u8]> = Vec::new();
        all.extend(PALW_BLOCK_COMMITMENT_ALL_DOMAINS);
        all.push(PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT);
        all.extend(crate::palw_job_identity::PALW_JOB_ALL_DOMAINS);
        all.extend(crate::palw_receipt::PALW_RECEIPT_ALL_DOMAINS);
        all.extend(crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS);
        all.extend(crate::palw_job_panel::PALW_PANEL_ALL_DOMAINS);
        all.extend(crate::palw_slash::PALW_S_ALL_DOMAINS);
        all.extend(crate::palw_routing::PALW_ROUTING_ALL_DOMAINS);
        all.extend(crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS);
        all.extend(crate::palw_attempt_v2::PALW_ATTEMPT_V2_ALL_DOMAINS);
        all.extend(crate::palw_state_v2::PALW_STATE_V2_ALL_DOMAINS);
        all.extend(crate::palw_panel_v2::PALW_PANEL_V2_ALL_DOMAINS);
        all.extend(crate::palw_court_v2::PALW_COURT_V2_ALL_DOMAINS);
        all.extend(crate::palw_mode_v2::PALW_MODE_V2_ALL_DOMAINS);
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "domain collision: {:?}", String::from_utf8_lossy(a));
            }
        }
    }

    /// Shape admission: version drift, zero pwu, wrong signature length all refuse; the
    /// well-formed commitment admits.
    #[test]
    fn shape_admission_is_closed() {
        assert!(commitment().validate_shape().is_ok());
        let mut c = commitment();
        c.version = 2;
        assert_eq!(c.validate_shape(), Err(PalwBlockCommitmentError::UnsupportedVersion { got: 2, expected: 1 }));
        let mut c = commitment();
        c.pwu_claim = 0;
        assert_eq!(c.validate_shape(), Err(PalwBlockCommitmentError::ZeroPwuClaim));
        let mut c = commitment();
        c.signature = vec![0x5A; 64];
        assert_eq!(
            c.validate_shape(),
            Err(PalwBlockCommitmentError::SignatureLength { got: 64, expected: STAKE_ATTESTATION_SIG_LEN })
        );
    }

    /// The commitment root binds class, bond, both Merkle roots and the pwu claim — and NOT
    /// the signature (a verifier recomputes the root before resolving any key).
    #[test]
    fn commitment_root_binds_payload_not_signature() {
        let ch = ch();
        let base = commitment().commitment_root(ch);
        let mut c = commitment();
        c.execution_class_id = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root(ch));
        let mut c = commitment();
        c.executor_bond_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(99), 0);
        assert_ne!(base, c.commitment_root(ch));
        let mut c = commitment();
        c.trace_root = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root(ch));
        let mut c = commitment();
        c.output_root = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root(ch));
        let mut c = commitment();
        c.pwu_claim = 101;
        assert_ne!(base, c.commitment_root(ch));
        let mut c = commitment();
        c.signature = vec![0x77; STAKE_ATTESTATION_SIG_LEN];
        assert_eq!(base, c.commitment_root(ch));
    }

    /// The L1 tag expansion: full width, deterministic, moves with the root, and its 64-byte
    /// chunks are pairwise distinct (a repeated-block tag would collapse the finalizer's
    /// entropy).
    #[test]
    fn l1_tag_expansion_is_deterministic_and_root_bound() {
        let tag = commitment().l1_tag_bytes(ch());
        assert_eq!(tag.len(), PALW_BLOCK_COMMITMENT_L1_TAG_BYTES);
        assert_eq!(tag, commitment().l1_tag_bytes(ch()));
        let mut c = commitment();
        c.trace_root = Hash64::from_u64_word(99);
        assert_ne!(tag, c.l1_tag_bytes(ch()));
        assert_ne!(tag[0..64], tag[64..128]);
        assert_ne!(tag[64..128], tag[128..192]);
    }

    /// ADR-0038 blocker 3, pinned: **the ticket costs one inference per attempt.**
    ///
    /// The defect this replaces: the tag derived from payload fields only, none of which
    /// depends on the nonce, while the Layer-0 finalizer mixes the nonce separately. One
    /// inference therefore bought the entire nonce space — a hash puzzle with a setup cost —
    /// and sampling could not see it, because a genuine `trace_root` with no declared attempt
    /// has nothing to contradict.
    ///
    /// Each assertion below is one way the old construction leaked. If any of them regresses,
    /// the grind is back.
    #[test]
    fn ticket_binds_the_exact_attempt() {
        let c = commitment();
        let pph = Hash64::from_u64_word(10);
        let at = |ts, nonce| c.challenge_for(NET, pph, ts, nonce);
        let base = at(1_000, 42);

        // The nonce — the binding whose absence WAS the grind.
        assert_ne!(base, at(1_000, 43), "a new nonce must be a new challenge");
        assert_ne!(c.l1_tag_bytes(base), c.l1_tag_bytes(at(1_000, 43)), "...and therefore a new tag");
        // Timestamp, header, network: no replay across attempts or chains.
        assert_ne!(base, at(1_001, 42));
        assert_ne!(base, c.challenge_for(NET, Hash64::from_u64_word(11), 1_000, 42));
        assert_ne!(base, c.challenge_for(b"other-net", pph, 1_000, 42));
        // Class and executor ride the challenge via `challenge_for`, so one trace cannot be
        // re-mined in another difficulty domain or under another bond.
        let mut other_class = c.clone();
        other_class.execution_class_id = Hash64::from_u64_word(99);
        assert_ne!(base, other_class.challenge_for(NET, pph, 1_000, 42), "class must ride the challenge");
        let mut other_bond = c.clone();
        other_bond.executor_bond_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(99), 0);
        assert_ne!(base, other_bond.challenge_for(NET, pph, 1_000, 42), "executor must ride the challenge");

        // Determinism: one attempt, one tag — a verifier recomputing it lands byte-identically.
        assert_eq!(c.l1_tag_bytes(base), c.l1_tag_bytes(at(1_000, 42)));
    }

    /// Re-audit blocker 6, pinned at the commitment: **`pwu_claim` has exactly one legal value.**
    ///
    /// Shape admission accepts any non-zero claim — correctly, it has no class state — so the
    /// weight-inflation attack is refused here, at the stateful entry.
    #[test]
    fn pwu_claim_is_not_a_miner_input() {
        use crate::palw_pwu::{PalwPwuError, palw_pwu_v1};
        let (target, cost) = (u128::MAX / 1_000, 4_000u64);
        let derived = palw_pwu_v1(target, cost);

        let mut honest = commitment();
        honest.pwu_claim = derived;
        assert!(honest.validate_against_class_v1(target, cost).is_ok());

        // The fixture's own hand-typed 100 is not the derivation — a plausible-looking number
        // is still a rejection, which is the point of requiring equality rather than a bound.
        assert!(commitment().validate_against_class_v1(target, cost).is_err());

        // The attack: claim the maximum. Shape says fine; the class says no.
        let mut greedy = commitment();
        greedy.pwu_claim = u64::MAX;
        assert!(greedy.validate_shape().is_ok(), "shape cannot catch this — it has no class state");
        assert_eq!(greedy.validate_against_class_v1(target, cost), Err(PalwPwuError::ClaimMismatch { claimed: u64::MAX, derived }));
    }

    /// The signing digest binds the exact ticket attempt: pre_pow_hash, timestamp and nonce
    /// each move the message — a signed commitment cannot be replayed onto another header
    /// (W2 at the signature layer).
    #[test]
    fn message_binds_the_exact_ticket() {
        let c = commitment();
        let base = c.message(NET, Hash64::from_u64_word(10), 1_000, 42);
        assert_ne!(base, c.message(b"other-net", Hash64::from_u64_word(10), 1_000, 42));
        assert_ne!(base, c.message(NET, Hash64::from_u64_word(11), 1_000, 42));
        assert_ne!(base, c.message(NET, Hash64::from_u64_word(10), 1_001, 42));
        assert_ne!(base, c.message(NET, Hash64::from_u64_word(10), 1_000, 43));
        let mut m = c.clone();
        m.pwu_claim = 101;
        assert_ne!(base, m.message(NET, Hash64::from_u64_word(10), 1_000, 42));
    }

    /// Wire form: magic + borsh roundtrip; foreign magic, truncation and trailing bytes all
    /// refuse.
    #[test]
    fn wire_form_roundtrips_and_refuses_junk() {
        let c = commitment();
        let bytes = c.encode();
        assert_eq!(PalwBlockCommitmentV1::decode(&bytes).unwrap(), c);
        assert_eq!(PalwBlockCommitmentV1::decode(b"XYZ1junk"), Err(PalwBlockCommitmentError::BadMagic));
        assert!(matches!(PalwBlockCommitmentV1::decode(&bytes[..bytes.len() - 1]), Err(PalwBlockCommitmentError::Undecodable { .. })));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(PalwBlockCommitmentV1::decode(&trailing), Err(PalwBlockCommitmentError::TrailingBytes { got: 1 }));
    }
}

#[cfg(test)]
mod w8_executor_bond_tests {
    use super::*;
    use crate::dns_finality::{ActiveBondView, StakeBondRecord};
    use crate::tx::TransactionOutpoint;

    fn bond(outpoint: TransactionOutpoint, activation: u64, unbond_request: Option<u64>) -> StakeBondRecord {
        StakeBondRecord {
            bond_outpoint: outpoint,
            validator_pubkey_hash: Hash64::from_u64_word(7),
            owner_pubkey_hash: Hash64::from_u64_word(8),
            version: 1,
            validator_pubkey: vec![7u8; 32],
            amount: 20_000,
            activation_daa_score: activation,
            created_daa_score: 0,
            unbonding_period_blocks: 100,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: unbond_request,
            slashed_at_daa_score: None,
            status: crate::dns_finality::BondStatus::Active,
        }
    }

    fn view(records: Vec<StakeBondRecord>) -> ActiveBondView {
        ActiveBondView::from_records(records.into_iter().map(|r| (r.bond_outpoint, r)))
    }

    fn commitment_naming(outpoint: TransactionOutpoint) -> PalwBlockCommitmentV1 {
        PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_u64_word(1),
            executor_bond_outpoint: outpoint,
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            pwu_claim: 100,
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// The happy path, and the reason the bond is RETURNED: the caller pays the bond that acted,
    /// without a second lookup that could resolve to a different record.
    #[test]
    fn an_active_bond_admits_and_hands_back_the_record_that_admitted_it() {
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(2), 3);
        let view = view(vec![bond(outpoint, 100, None)]);
        let found = commitment_naming(outpoint).validate_executor_bond_v1(&view, 500).expect("active at 500");
        assert_eq!(found.bond_outpoint, outpoint, "the record returned must be the one named");
    }

    /// "No bond, no block" — a block naming a bond this chain point does not hold is refused, not
    /// waved through with a warning. ADR-0038: "the design is unsound without it".
    #[test]
    fn a_block_naming_no_bond_at_all_is_refused() {
        let view = view(vec![]);
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(2), 3);
        assert!(matches!(
            commitment_naming(outpoint).validate_executor_bond_v1(&view, 500),
            Err(PalwBlockCommitmentError::ExecutorBondNotActive { .. })
        ));
    }

    /// A bond that exists but is not ACTIVE at this point is not a bond for this block: before its
    /// activation it has not been posted yet, and once unbonding has started it is on its way out.
    /// Either way the deterrent the clause prices work against is absent.
    #[test]
    fn a_bond_outside_its_active_window_does_not_admit() {
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(2), 3);
        let commitment = commitment_naming(outpoint);

        let not_yet = view(vec![bond(outpoint, 1_000, None)]);
        assert!(commitment.validate_executor_bond_v1(&not_yet, 999).is_err(), "before activation");
        assert!(commitment.validate_executor_bond_v1(&not_yet, 1_000).is_ok(), "at activation");

        let leaving = view(vec![bond(outpoint, 100, Some(400))]);
        assert!(leaving.active_bond_at(&outpoint, 500).is_none(), "the view's own rule must agree");
        assert!(commitment.validate_executor_bond_v1(&leaving, 500).is_err(), "unbonding has started");
    }

    /// The bond a block names is the one checked — naming someone else's active bond does not
    /// borrow their accountability.
    #[test]
    fn a_different_active_bond_does_not_stand_in() {
        let mine = TransactionOutpoint::new(Hash64::from_u64_word(2), 3);
        let theirs = TransactionOutpoint::new(Hash64::from_u64_word(9), 0);
        let view = view(vec![bond(theirs, 100, None)]);
        assert!(matches!(
            commitment_naming(mine).validate_executor_bond_v1(&view, 500),
            Err(PalwBlockCommitmentError::ExecutorBondNotActive { .. })
        ));
    }
}

#[cfg(test)]
mod decision_a_gate_tests {
    use super::*;
    use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
    use crate::pow_layer0::{POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_LLM, PowLayer0Error, check_palw_commitment_shape};

    fn a_commitment() -> PalwBlockCommitmentV1 {
        PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_u64_word(1),
            executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(2), 3),
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            pwu_claim: 100,
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// Unfenced is the pre-ADR rule, unchanged: a PALW header's commitment must be empty.
    #[test]
    fn without_the_fence_a_palw_header_must_still_carry_nothing() {
        assert!(check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &[], false).is_ok());
        assert!(matches!(
            check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &a_commitment().encode(), false),
            Err(PowLayer0Error::PalwCommitmentNotYetBound { .. })
        ));
    }

    /// Fenced: the commitment is required, and must be a commitment.
    #[test]
    fn with_the_fence_a_palw_header_must_carry_a_well_formed_commitment() {
        assert!(check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &a_commitment().encode(), true).is_ok());
        // Empty is now a refusal, reported as "not a commitment" rather than a length complaint.
        assert!(matches!(
            check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &[], true),
            Err(PowLayer0Error::PalwCommitmentMalformed { .. })
        ));
        // Right length, wrong content: the magic is what the decoder checks first.
        let mut junk = a_commitment().encode();
        junk[0] ^= 0xff;
        assert!(matches!(
            check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &junk, true),
            Err(PowLayer0Error::PalwCommitmentMalformed { .. })
        ));
        // Decodes but is not a valid commitment: shape is checked too, not just the encoding.
        let mut zero_pwu = a_commitment();
        zero_pwu.pwu_claim = 0;
        assert!(matches!(
            check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &zero_pwu.encode(), true),
            Err(PowLayer0Error::PalwCommitmentMalformed { .. })
        ));
    }

    /// The non-PALW arm never relaxes: there the field is hash-INVISIBLE, so a non-empty one is
    /// block-hash malleability whatever any fence says.
    #[test]
    fn the_non_palw_arm_is_not_fenced() {
        for bound in [false, true] {
            assert!(check_palw_commitment_shape(POW_ALGO_ID_KHEAVYHASH, &[], bound).is_ok());
            assert!(matches!(
                check_palw_commitment_shape(POW_ALGO_ID_KHEAVYHASH, &a_commitment().encode(), bound),
                Err(PowLayer0Error::NonPalwHeaderCarriesPalwCommitment { .. })
            ));
        }
    }

    /// The cap is reported before the binding rule, in both directions — an operator whose payload
    /// is oversized should be told that, not that it failed to decode.
    #[test]
    fn the_size_cap_outranks_the_binding_rule() {
        let oversized = vec![0xAA; crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES + 1];
        for bound in [false, true] {
            assert!(matches!(
                check_palw_commitment_shape(POW_ALGO_ID_PALW_LLM, &oversized, bound),
                Err(PowLayer0Error::PalwCommitmentTooLong { .. })
            ));
        }
    }

    #[test]
    fn the_fence_binds_at_its_own_daa_score_and_not_before() {
        let fence = PalwBlockCommitmentParamsV1 { activation_daa_score: 500 };
        assert!(!fence.is_bound(0));
        assert!(!fence.is_bound(499));
        assert!(fence.is_bound(500));
        assert!(fence.is_bound(u64::MAX));
    }
}
