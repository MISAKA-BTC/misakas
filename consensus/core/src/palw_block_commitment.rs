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
//! — a hash check. Whether the claimed tag honestly derives from `inference(seed(nonce))`
//! is what assigned sampling ([`crate::palw_receipt`]) and the court decide, under the
//! weight ramp ([`crate::palw_weight`]): a fabricated tag passes admission and matures
//! never. Honest miners still pay one full inference per ticket (the lottery shape is
//! unchanged); fabricators pay bond slash (ADR-0038 New-risk 1).
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

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_BLOCK_COMMITMENT_ALL_DOMAINS: &[&[u8]] =
    &[PALW_BLOCK_COMMITMENT_DOMAIN_MESSAGE, PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG];

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
    /// The canonical PWU this block claims under its class's frozen derivation
    /// ([`crate::palw_class_daa`] — static intra-class, never wall-clock).
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

    /// The commitment root: one digest over the class, bond, both Merkle roots and the pwu
    /// claim — what receipts cover ([`crate::palw_receipt`]'s `target_commitment_root`) and
    /// what the ticket's tag expands from. The signature is NOT inside (a root must be
    /// recomputable by a verifier who has not resolved the key yet).
    pub fn commitment_root(&self) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BLOCK_COMMITMENT_DOMAIN_L1_TAG).to_state();
        state.update(&[0u8]); // leaf discriminator: root preimage, not tag expansion
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
    /// a domain-keyed expansion of the commitment root (deterministic, admission-checkable
    /// by any CPU). Honest miners derive the root from a real trace; the expansion width
    /// keeps the finalizer call-shape identical to today's algo-4.
    pub fn l1_tag_bytes(&self) -> [u8; PALW_BLOCK_COMMITMENT_L1_TAG_BYTES] {
        let root = self.commitment_root();
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
    pub fn message(&self, network_id: &[u8], pre_pow_hash: Hash64, timestamp: u64, nonce: u64) -> Hash {
        let mut state = blake2b_simd::Params::new().hash_length(32).key(PALW_BLOCK_COMMITMENT_DOMAIN_MESSAGE).to_state();
        state.update(&(network_id.len() as u32).to_le_bytes());
        state.update(network_id);
        state.update(pre_pow_hash.as_byte_slice());
        state.update(&timestamp.to_le_bytes());
        state.update(&nonce.to_le_bytes());
        state.update(self.commitment_root().as_byte_slice());
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
        let base = commitment().commitment_root();
        let mut c = commitment();
        c.execution_class_id = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root());
        let mut c = commitment();
        c.executor_bond_outpoint = TransactionOutpoint::new(Hash64::from_u64_word(99), 0);
        assert_ne!(base, c.commitment_root());
        let mut c = commitment();
        c.trace_root = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root());
        let mut c = commitment();
        c.output_root = Hash64::from_u64_word(99);
        assert_ne!(base, c.commitment_root());
        let mut c = commitment();
        c.pwu_claim = 101;
        assert_ne!(base, c.commitment_root());
        let mut c = commitment();
        c.signature = vec![0x77; STAKE_ATTESTATION_SIG_LEN];
        assert_eq!(base, c.commitment_root());
    }

    /// The L1 tag expansion: full width, deterministic, moves with the root, and its 64-byte
    /// chunks are pairwise distinct (a repeated-block tag would collapse the finalizer's
    /// entropy).
    #[test]
    fn l1_tag_expansion_is_deterministic_and_root_bound() {
        let tag = commitment().l1_tag_bytes();
        assert_eq!(tag.len(), PALW_BLOCK_COMMITMENT_L1_TAG_BYTES);
        assert_eq!(tag, commitment().l1_tag_bytes());
        let mut c = commitment();
        c.trace_root = Hash64::from_u64_word(99);
        assert_ne!(tag, c.l1_tag_bytes());
        assert_ne!(tag[0..64], tag[64..128]);
        assert_ne!(tag[64..128], tag[128..192]);
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
