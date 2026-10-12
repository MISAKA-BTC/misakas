//! The checkpoint a remote miner pins, and the signed form an operator can hand out.
//!
//! A checkpoint is `(network, DAA score, block hash)` that the miner trusts *before* it talks to any node. Where it comes from is
//! the miner's configuration or a [`SignedCheckpointV1`] checked against keys the miner already holds. Stage D phase 1: this is
//! the whole trust root, and an update needs a new signed checkpoint — the client never advances it from what nodes say.

use crate::{finish, keyed, put_len};
use kaspa_hashes::Hash64;

pub const DOMAIN_CHECKPOINT: &[u8] = b"misaka-palw/remote/checkpoint/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub network_id: String,
    pub daa_score: u64,
    pub block_hash: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedCheckpointV1 {
    pub checkpoint: Checkpoint,
    /// The DAA score at which the issuer signed — a checkpoint is a statement about a moment, and an old one proves little.
    pub issued_at_daa: u64,
    /// Which of the miner's trusted keys signed (an index into the miner's own list, or any stable id the miner chose).
    pub key_id: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedCheckpointV1 {
    /// What the issuer signs: domain-separated, length-prefixed, total over every field but the signature.
    pub fn signing_digest(&self) -> Hash64 {
        let mut s = keyed(DOMAIN_CHECKPOINT);
        put_len(&mut s, self.checkpoint.network_id.as_bytes());
        s.update(&self.checkpoint.daa_score.to_le_bytes());
        s.update(self.checkpoint.block_hash.as_bytes().as_slice());
        s.update(&self.issued_at_daa.to_le_bytes());
        put_len(&mut s, &self.key_id);
        finish(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckpointError {
    #[error("the checkpoint's signer {0:?} is not among the keys this miner trusts")]
    UnknownSigner(Vec<u8>),
    #[error("the checkpoint's signature does not verify")]
    BadSignature,
    #[error("the checkpoint was issued at DAA {issued} and the chain is at {now}: older than the allowed {max_age} — a fresh signed checkpoint is required")]
    Stale { issued: u64, now: u64, max_age: u64 },
    #[error("the checkpoint is issued in the future of the chain view ({issued} > {now})")]
    FromTheFuture { issued: u64, now: u64 },
    #[error("the checkpoint is for network {got:?}, expected {expected:?}")]
    WrongNetwork { got: String, expected: String },
}

/// Verify a signed checkpoint. `verify(pubkey, message, signature)` is injected (ML-DSA-87 in the binaries), so this crate fixes the
/// freshness and ownership rules and not the primitive. `now_daa` is the quorum's conservative virtual DAA.
pub fn verify_signed_checkpoint(
    signed: &SignedCheckpointV1,
    trusted: &[(Vec<u8>, Vec<u8>)],
    expected_network: &str,
    now_daa: u64,
    max_age_daa: u64,
    verify: impl Fn(&[u8], &[u8], &[u8]) -> bool,
) -> Result<Checkpoint, CheckpointError> {
    if signed.checkpoint.network_id != expected_network {
        return Err(CheckpointError::WrongNetwork { got: signed.checkpoint.network_id.clone(), expected: expected_network.into() });
    }
    let (_, pubkey) =
        trusted.iter().find(|(id, _)| *id == signed.key_id).ok_or_else(|| CheckpointError::UnknownSigner(signed.key_id.clone()))?;
    if !verify(pubkey, signed.signing_digest().as_bytes().as_slice(), &signed.signature) {
        return Err(CheckpointError::BadSignature);
    }
    if signed.issued_at_daa > now_daa {
        return Err(CheckpointError::FromTheFuture { issued: signed.issued_at_daa, now: now_daa });
    }
    if now_daa - signed.issued_at_daa > max_age_daa {
        return Err(CheckpointError::Stale { issued: signed.issued_at_daa, now: now_daa, max_age: max_age_daa });
    }
    Ok(signed.checkpoint.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(daa: u64, issued: u64) -> SignedCheckpointV1 {
        SignedCheckpointV1 {
            checkpoint: Checkpoint { network_id: "testnet-12".into(), daa_score: daa, block_hash: Hash64::from_bytes([7; 64]) },
            issued_at_daa: issued,
            key_id: b"k1".to_vec(),
            signature: vec![1, 2, 3],
        }
    }
    /// A toy "signature": the digest's first bytes equal the signature field + the key is `pk`.
    fn toy(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> bool {
        pubkey == b"pk" && sig == [msg[0], msg[1], msg[2]]
    }
    fn sign(mut s: SignedCheckpointV1) -> SignedCheckpointV1 {
        let d = s.signing_digest();
        s.signature = d.as_bytes().as_slice()[..3].to_vec();
        s
    }
    const TRUSTED: fn() -> Vec<(Vec<u8>, Vec<u8>)> = || vec![(b"k1".to_vec(), b"pk".to_vec())];

    #[test]
    fn a_fresh_checkpoint_from_a_trusted_key_verifies() {
        let c = verify_signed_checkpoint(&sign(signed(5_000, 5_100)), &TRUSTED(), "testnet-12", 5_150, 500, toy).unwrap();
        assert_eq!(c.daa_score, 5_000);
    }

    #[test]
    fn stale_future_foreign_and_forged_checkpoints_are_refused_by_name() {
        let ok = sign(signed(5_000, 5_100));
        assert!(matches!(verify_signed_checkpoint(&ok, &TRUSTED(), "testnet-12", 9_000, 500, toy), Err(CheckpointError::Stale { .. })));
        assert!(matches!(verify_signed_checkpoint(&ok, &TRUSTED(), "testnet-12", 5_000, 500, toy), Err(CheckpointError::FromTheFuture { .. })));
        assert!(matches!(verify_signed_checkpoint(&ok, &TRUSTED(), "mainnet", 5_150, 500, toy), Err(CheckpointError::WrongNetwork { .. })));
        assert!(matches!(verify_signed_checkpoint(&ok, &[], "testnet-12", 5_150, 500, toy), Err(CheckpointError::UnknownSigner(_))));
        let mut forged = ok.clone();
        forged.checkpoint.daa_score = 4_000; // changes the digest the signature was made over
        assert!(matches!(verify_signed_checkpoint(&forged, &TRUSTED(), "testnet-12", 5_150, 500, toy), Err(CheckpointError::BadSignature)));
    }
}
