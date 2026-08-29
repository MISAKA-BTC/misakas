//! Signed epoch ledger (ADR-0027 §2, §4.3, §11-C). A ledger is a pure function of
//! the collected facts; it pins `rules_hash` + `inputs_hash` and is ML-DSA-87
//! signed so anyone can (a) recompute the same scores and (b) verify the operator
//! signed exactly those bytes.

use kaspa_hashes::{Hash64, blake2b_512_keyed};
use kaspa_pq_validator_core::ValidatorKey;
use kaspa_txscript::verify_mldsa87_with_context;
use serde::{Deserialize, Serialize};

use crate::MTP_LEDGER_CONTEXT;
use crate::rules::MilliPoints;

/// One identity's per-category score for an epoch, with evidence links (§11-C).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreRow {
    pub id: String,
    pub c1: MilliPoints,
    pub c2: MilliPoints,
    pub c3: MilliPoints,
    pub c4: MilliPoints,
    /// **C5 LLM mining** (ADR-0040 §16″). Appended last: the field order is the canonical signing
    /// preimage, so inserting it anywhere else would invalidate every previously signed ledger.
    ///
    /// `serde` default so historical (4-category) ledgers still deserialize, reading as `c5 = 0` — which
    /// is the truthful value for an epoch scored before the category existed.
    #[serde(default)]
    pub c5: MilliPoints,
    pub evidence: Vec<String>,
}

/// A published, signable epoch ledger (§11-C). Field order is the canonical
/// signing preimage; `scores` MUST be sorted by `id` (guaranteed by
/// [`crate::score_epoch`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochLedger {
    pub epoch: u64,
    pub range: [String; 2],
    pub network: String,
    /// hex of `Rules::rules_hash()`.
    pub rules_hash: String,
    /// hex of `blake2b_512_keyed(MTP_INPUTS_CONTEXT, canonical facts)`.
    pub inputs_hash: String,
    pub scores: Vec<ScoreRow>,
    /// hex ML-DSA-87 signature over [`Self::digest`]; `None` until signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig_mldsa87: Option<String>,
}

impl EpochLedger {
    /// The signing digest: `blake2b_512_keyed(MTP_LEDGER_CONTEXT, canonical JSON
    /// without the signature)`. Deterministic (fixed field order, sorted scores,
    /// integer-only values).
    pub fn digest(&self) -> Hash64 {
        let mut unsigned = self.clone();
        unsigned.sig_mldsa87 = None;
        let preimage = serde_json::to_vec(&unsigned).expect("ledger JSON is infallible");
        blake2b_512_keyed(MTP_LEDGER_CONTEXT, &preimage)
    }

    /// Sign the ledger in place with the operator key.
    pub fn sign(&mut self, key: &ValidatorKey) {
        let digest = self.digest().as_bytes();
        let sig = key.sign_with_context(&digest, MTP_LEDGER_CONTEXT);
        self.sig_mldsa87 = Some(hex(&sig));
    }

    /// Verify the ledger's signature against the operator `pubkey` (2592 bytes).
    pub fn verify(&self, pubkey: &[u8]) -> bool {
        let Some(sig_hex) = &self.sig_mldsa87 else { return false };
        let Some(sig) = unhex(sig_hex) else { return false };
        let digest = self.digest().as_bytes();
        matches!(verify_mldsa87_with_context(pubkey, &digest, &sig, MTP_LEDGER_CONTEXT), Ok(true))
    }

    /// Serialize as a single JSONL line (§11-C).
    pub fn to_jsonl(&self) -> String {
        serde_json::to_string(self).expect("ledger JSON is infallible")
    }
}

fn hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(bytes)
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> EpochLedger {
        EpochLedger {
            epoch: 12,
            range: ["2026-09-21T00:00:00Z".into(), "2026-09-28T00:00:00Z".into()],
            network: "testnet-10".into(),
            rules_hash: "aa".into(),
            inputs_hash: "bb".into(),
            scores: vec![ScoreRow {
                id: "gh:alice".into(),
                c1: 152_500,
                c2: 0,
                c3: 60_000,
                c4: 150_000,
                c5: 0,
                evidence: vec!["gh:misakas#241".into()],
            }],
            sig_mldsa87: None,
        }
    }

    #[test]
    fn sign_then_verify_round_trips_and_detects_tamper() {
        let key = ValidatorKey::from_seed([0x7a; 32]);
        let mut l = ledger();
        l.sign(&key);
        assert!(l.verify(key.public_key()), "operator signature must verify");

        // tamper a score → signature no longer matches the recomputed digest.
        let mut t = l.clone();
        t.scores[0].c1 += 1;
        assert!(!t.verify(key.public_key()), "any edit invalidates the signature");

        // a different operator key does not verify.
        let other = ValidatorKey::from_seed([0x7b; 32]);
        assert!(!l.verify(other.public_key()));
    }

    #[test]
    fn digest_is_signature_independent_and_deterministic() {
        let mut l = ledger();
        let d0 = l.digest();
        let key = ValidatorKey::from_seed([1; 32]);
        l.sign(&key);
        assert_eq!(d0, l.digest(), "digest excludes the signature field");
        // JSONL is stable.
        assert!(l.to_jsonl().contains("\"epoch\":12"));
    }
}
