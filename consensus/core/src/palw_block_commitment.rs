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
    /// Merkle root of the execution trace checkpoints (what samplers open).
    pub trace_root: Hash64,
    /// Merkle root of the output/token stream.
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
    pub fn validate_against_class_v1(
        &self,
        class_target: u128,
        pwu_per_inference: u64,
    ) -> Result<(), crate::palw_pwu::PalwPwuError> {
        crate::palw_pwu::check_pwu_claim_v1(self.pwu_claim, class_target, pwu_per_inference)
    }

    /// This commitment's challenge for a given attempt — [`palw_block_challenge_v1`] with the
    /// class and bond taken from `self`, so a caller cannot accidentally bind a trace to a
    /// class or executor other than the one it claims.
    pub fn challenge_for(&self, network_id: &[u8], pre_pow_hash: Hash64, timestamp: u64, nonce: u64) -> Hash64 {
        palw_block_challenge_v1(
            network_id,
            pre_pow_hash,
            timestamp,
            nonce,
            self.execution_class_id,
            &self.executor_bond_outpoint,
        )
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
        all.extend(crate::palw_slash::PALW_S_ALL_DOMAINS);
        all.extend(crate::palw_routing::PALW_ROUTING_ALL_DOMAINS);
        all.extend(crate::palw_registry::PALW_REGISTRY_ALL_DOMAINS);
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
        assert_eq!(c.validate_shape(), Err(PalwBlockCommitmentError::SignatureLength { got: 64, expected: STAKE_ATTESTATION_SIG_LEN }));
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
        assert_eq!(
            greedy.validate_against_class_v1(target, cost),
            Err(PalwPwuError::ClaimMismatch { claimed: u64::MAX, derived })
        );
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
        assert!(matches!(
            PalwBlockCommitmentV1::decode(&bytes[..bytes.len() - 1]),
            Err(PalwBlockCommitmentError::Undecodable { .. })
        ));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(PalwBlockCommitmentV1::decode(&trailing), Err(PalwBlockCommitmentError::TrailingBytes { got: 1 }));
    }
}
