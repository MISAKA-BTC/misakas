//! **Seam 1 — the beacon → `job_challenge` binding, and the salted output commitment.**
//!
//! This is the half the tree specifies but does not implement. Today `job_challenge` is a free
//! input to a scheduler-signed JobSpec: the CLI only refuses an all-zero value ("derive it from
//! chain randomness" is a doc comment, not a check), and the v1 salted `output_commitment` has
//! no production caller at all. ADR-0040 §537 states the intended derivation:
//!
//! ```text
//! job_challenge = H(network_id ‖ epoch_beacon ‖ scheduler_job_id
//!                   ‖ requester_credential ‖ request_commitment ‖ shape_id)
//! ```
//!
//! and §1357 notes the existing `execution_challenge` cannot be used as-is because it lacks
//! `scheduler_job_id` / `requester_credential` / `request_commitment`. [`derive_job_challenge`]
//! implements exactly the ADR shape, in this bridge's own domain (a bridge-issued challenge must
//! never be mistakable for a consensus value), with the epoch bound in alongside the seed so a
//! challenge cannot be replayed into a different epoch under a carried seed.
//!
//! **Why a LEASE, not a post-hoc derivation.** The desktop answer clock generates before it
//! commits. If the challenge were derived after generation, a provider could regenerate until it
//! liked the output and only then commit — grinding, exactly what the product doc's §16
//! "decide mint before you send" rule exists to prevent. So the bridge issues the challenge
//! FIRST, bound to `request_commitment` (the compiled prompt's own commitment), and later
//! refuses any submission whose prompt does not reproduce that commitment. The cost is one
//! LAN round-trip before generation — negligible against seconds of inference.
//!
//! **Byte-parity with the live receipt path.** The commitment over the answer is
//! [`misaka_palw::receipt_v3::output_commitment_v3`] — the function real receipts and
//! `MatchProjectionV2` already use, where `job_challenge` plays the salt role. So a leaf minted
//! from a bridge-coordinated job commits to its output under the same preimage a consensus
//! receipt does. (The dead v1 `output_commitment(salt, ids)` is deliberately NOT used.)

use kaspa_hashes::{HASH64_SIZE, Hash64, blake2b_512_keyed};
use misaka_palw::receipt_v3::output_commitment_v3;
use serde::{Deserialize, Serialize};

use crate::chain::BeaconFacts;

/// Keyed-BLAKE2b domain for bridge-issued job challenges.
///
/// HISTORY: this began as a bridge-local domain, deliberately disjoint from consensus. ADR-0045
/// D3-b then promoted the derivation INTO consensus byte-for-byte — domain string included —
/// as `kaspa_consensus_core::palw::PALW_JOB_CHALLENGE_DOMAIN`, because clause 11 re-derives the
/// leaf's `receipt_v3_job_challenge` and every already-issued lease had committed under THIS
/// domain. The two constants are now intentionally EQUAL and the parity is load-bearing:
/// `job_challenge_parity_with_consensus_is_pinned` below fails if either side drifts.
pub const BRIDGE_JOB_CHALLENGE_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/job-challenge";
/// Domain for the request commitment (the prompt's own binding).
pub const BRIDGE_REQUEST_COMMITMENT_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/request-commitment";

fn push_hash(buffer: &mut Vec<u8>, hash: &Hash64) {
    buffer.extend_from_slice(hash.as_byte_slice());
}

/// `request_commitment` over the exact prompt the provider will run. Binding the token ids (not
/// text) is what makes the later "did you run what you leased?" check exact.
pub fn request_commitment(prompt_token_ids: &[u32], max_new: u32, class_label: &[u8]) -> Hash64 {
    let mut preimage = Vec::with_capacity(8 + class_label.len() + 4 + prompt_token_ids.len() * 4);
    preimage.extend_from_slice(&(class_label.len() as u64).to_le_bytes());
    preimage.extend_from_slice(class_label);
    preimage.extend_from_slice(&max_new.to_le_bytes());
    preimage.extend_from_slice(&(prompt_token_ids.len() as u64).to_le_bytes());
    for id in prompt_token_ids {
        preimage.extend_from_slice(&id.to_le_bytes());
    }
    blake2b_512_keyed(BRIDGE_REQUEST_COMMITMENT_DOMAIN, &preimage)
}

/// ADR-0040 §537, with the beacon epoch bound in.
///
/// `requester_credential` is the requesting provider's `owner_pubkey_hash` (the chain's own
/// credential identity, `validator_id_from_pubkey(owner_public_key)`), so a challenge is bound
/// to WHO leased it — another provider cannot pick up someone else's challenge and submit under
/// it.
#[allow(clippy::too_many_arguments)]
pub fn derive_job_challenge(
    network_id: u32,
    beacon_epoch: u64,
    beacon_seed: &Hash64,
    scheduler_job_id: &Hash64,
    requester_credential: &Hash64,
    request_commitment: &Hash64,
    shape_id: u16,
) -> Hash64 {
    let mut preimage = Vec::with_capacity(4 + 8 + HASH64_SIZE * 4 + 2);
    preimage.extend_from_slice(&network_id.to_le_bytes());
    preimage.extend_from_slice(&beacon_epoch.to_le_bytes());
    push_hash(&mut preimage, beacon_seed);
    push_hash(&mut preimage, scheduler_job_id);
    push_hash(&mut preimage, requester_credential);
    push_hash(&mut preimage, request_commitment);
    preimage.extend_from_slice(&shape_id.to_le_bytes());
    blake2b_512_keyed(BRIDGE_JOB_CHALLENGE_DOMAIN, &preimage)
}

/// A challenge the bridge issued and will honour. Persisted in the journal, so a restart cannot
/// forget an outstanding lease (which would let the holder re-lease and grind).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobLeaseV1 {
    pub scheduler_job_id_hex: String,
    pub job_challenge_hex: String,
    pub requester_credential_hex: String,
    pub request_commitment_hex: String,
    pub beacon_epoch: u64,
    pub beacon_seed_hex: String,
    pub shape_id: u16,
    pub network_id: u32,
    /// Sink epoch at issue; a lease expires so a stale beacon cannot be mined forever.
    pub issued_epoch: u64,
    pub expires_epoch: u64,
}

/// How many sink epochs a lease stays valid. Short: the point of the lease is that the challenge
/// exists before the answer, not that it lives long. Epoch length is 100 DAA (~10 s at 10 BPS),
/// so 6 epochs ≈ one minute — comfortably longer than a 35B answer, far shorter than a window
/// worth grinding.
pub const LEASE_EPOCHS: u64 = 6;

impl JobLeaseV1 {
    pub fn issue(
        network_id: u32,
        beacon: &BeaconFacts,
        scheduler_job_id: &Hash64,
        requester_credential: &Hash64,
        request_commitment: &Hash64,
        shape_id: u16,
    ) -> Result<Self, String> {
        let seed = beacon.seed()?;
        let challenge = derive_job_challenge(
            network_id,
            beacon.epoch,
            &seed,
            scheduler_job_id,
            requester_credential,
            request_commitment,
            shape_id,
        );
        if challenge == Hash64::default() {
            return Err("derived an all-zero job challenge — refusing (dispatch rejects it too)".into());
        }
        Ok(Self {
            scheduler_job_id_hex: crate::match_key::hash64_hex(scheduler_job_id),
            job_challenge_hex: crate::match_key::hash64_hex(&challenge),
            requester_credential_hex: crate::match_key::hash64_hex(requester_credential),
            request_commitment_hex: crate::match_key::hash64_hex(request_commitment),
            beacon_epoch: beacon.epoch,
            beacon_seed_hex: beacon.seed_hex.clone(),
            shape_id,
            network_id,
            issued_epoch: beacon.current_epoch,
            expires_epoch: beacon.current_epoch.saturating_add(LEASE_EPOCHS),
        })
    }

    pub fn job_challenge(&self) -> Result<Hash64, String> {
        crate::chain::parse_hash64(&self.job_challenge_hex)
    }

    pub fn is_expired_at(&self, current_epoch: u64) -> bool {
        current_epoch > self.expires_epoch
    }

    /// Re-derive the challenge from the lease's own recorded inputs. Any drift between what we
    /// stored and what the derivation produces is a bug or tampering, and is caught here rather
    /// than becoming an unverifiable commitment later.
    pub fn verify_self_consistent(&self) -> Result<(), String> {
        let recomputed = derive_job_challenge(
            self.network_id,
            self.beacon_epoch,
            &crate::chain::parse_hash64(&self.beacon_seed_hex)?,
            &crate::chain::parse_hash64(&self.scheduler_job_id_hex)?,
            &crate::chain::parse_hash64(&self.requester_credential_hex)?,
            &crate::chain::parse_hash64(&self.request_commitment_hex)?,
            self.shape_id,
        );
        if crate::match_key::hash64_hex(&recomputed) != self.job_challenge_hex {
            return Err("lease challenge does not re-derive from its own inputs".into());
        }
        Ok(())
    }

    /// The bound the submission must satisfy: same prompt, same requester, unexpired.
    pub fn accepts(
        &self,
        prompt_token_ids: &[u32],
        max_new: u32,
        class_label: &[u8],
        requester_credential: &Hash64,
        current_epoch: u64,
    ) -> Result<(), String> {
        if self.is_expired_at(current_epoch) {
            return Err(format!(
                "lease expired at epoch {} (now {current_epoch}) — lease a fresh challenge and re-run",
                self.expires_epoch
            ));
        }
        if self.requester_credential_hex != crate::match_key::hash64_hex(requester_credential) {
            return Err("lease belongs to a different requester credential".into());
        }
        let actual = request_commitment(prompt_token_ids, max_new, class_label);
        if crate::match_key::hash64_hex(&actual) != self.request_commitment_hex {
            return Err("submitted prompt does not match the leased request commitment".into());
        }
        Ok(())
    }
}

/// The salted output commitment, byte-identical to the live receipt-v3 path.
pub fn salted_output_commitment(output_token_ids: &[u32], job_challenge: &Hash64) -> Hash64 {
    output_commitment_v3(output_token_ids, job_challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_key::hash64_hex;

    fn beacon(epoch: u64, seed_byte: u8) -> BeaconFacts {
        BeaconFacts {
            epoch,
            seed_hex: format!("{seed_byte:02x}").repeat(64),
            anchor_hash_hex: "cd".repeat(64),
            anchor_daa_score: epoch * 100,
            observed_daa_score: epoch * 100 + 300,
            current_epoch: epoch + 3,
        }
    }

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    #[test]
    fn challenge_depends_on_every_adr_input() {
        let base = derive_job_challenge(1, 10, &h(1), &h(2), &h(3), &h(4), 5);
        assert_ne!(base, derive_job_challenge(2, 10, &h(1), &h(2), &h(3), &h(4), 5), "network_id");
        assert_ne!(base, derive_job_challenge(1, 11, &h(1), &h(2), &h(3), &h(4), 5), "beacon epoch");
        assert_ne!(base, derive_job_challenge(1, 10, &h(9), &h(2), &h(3), &h(4), 5), "beacon seed");
        assert_ne!(base, derive_job_challenge(1, 10, &h(1), &h(9), &h(3), &h(4), 5), "scheduler job id");
        assert_ne!(base, derive_job_challenge(1, 10, &h(1), &h(2), &h(9), &h(4), 5), "requester credential");
        assert_ne!(base, derive_job_challenge(1, 10, &h(1), &h(2), &h(3), &h(9), 5), "request commitment");
        assert_ne!(base, derive_job_challenge(1, 10, &h(1), &h(2), &h(3), &h(4), 6), "shape id");
        // Deterministic.
        assert_eq!(base, derive_job_challenge(1, 10, &h(1), &h(2), &h(3), &h(4), 5));
    }

    #[test]
    fn lease_binds_the_prompt_and_the_requester() {
        let lease = JobLeaseV1::issue(7, &beacon(10, 0xab), &h(1), &h(2), &request_commitment(&[1, 2, 3], 256, b"cls"), 1).unwrap();
        lease.verify_self_consistent().unwrap();

        // The leased prompt is accepted.
        lease.accepts(&[1, 2, 3], 256, b"cls", &h(2), 13).unwrap();
        // A different prompt, generation bound, class, or requester is not.
        assert!(lease.accepts(&[1, 2, 4], 256, b"cls", &h(2), 13).is_err(), "different prompt");
        assert!(lease.accepts(&[1, 2, 3], 512, b"cls", &h(2), 13).is_err(), "different max_new");
        assert!(lease.accepts(&[1, 2, 3], 256, b"other", &h(2), 13).is_err(), "different class");
        assert!(lease.accepts(&[1, 2, 3], 256, b"cls", &h(9), 13).is_err(), "different requester");
    }

    #[test]
    fn lease_expires() {
        let lease = JobLeaseV1::issue(7, &beacon(10, 0xab), &h(1), &h(2), &request_commitment(&[1], 8, b"c"), 1).unwrap();
        assert_eq!(lease.issued_epoch, 13);
        assert_eq!(lease.expires_epoch, 13 + LEASE_EPOCHS);
        assert!(!lease.is_expired_at(19));
        assert!(lease.is_expired_at(20));
        assert!(lease.accepts(&[1], 8, b"c", &h(2), 20).is_err());
    }

    /// The anti-grinding property, stated as a test: a provider that re-runs the SAME leased job
    /// gets the same challenge (so the same commitment domain), while a provider that wants a
    /// different challenge must change something the lease binds — and then its old lease no
    /// longer accepts the submission.
    #[test]
    fn a_second_lease_cannot_be_used_for_the_first_prompt() {
        let commitment_a = request_commitment(&[1, 2, 3], 256, b"cls");
        let commitment_b = request_commitment(&[4, 5, 6], 256, b"cls");
        let lease_a = JobLeaseV1::issue(7, &beacon(10, 0xab), &h(1), &h(2), &commitment_a, 1).unwrap();
        let lease_b = JobLeaseV1::issue(7, &beacon(10, 0xab), &h(8), &h(2), &commitment_b, 1).unwrap();
        assert_ne!(lease_a.job_challenge_hex, lease_b.job_challenge_hex);
        assert!(lease_b.accepts(&[1, 2, 3], 256, b"cls", &h(2), 13).is_err());
    }

    /// ADR-0045 D3-b — the seam pin: the bridge derivation and the consensus clause-11 derivation
    /// are the SAME function, byte for byte. If this breaks, every outstanding lease stops
    /// resolving on-chain (the leaf's committed challenge no longer re-derives), and the failure
    /// would otherwise surface only as silent acceptance-arm rejections.
    #[test]
    fn job_challenge_parity_with_consensus_is_pinned() {
        assert_eq!(
            BRIDGE_JOB_CHALLENGE_DOMAIN,
            kaspa_consensus_core::palw::PALW_JOB_CHALLENGE_DOMAIN,
            "domain strings must stay equal (D3-b promoted the bridge domain into consensus)"
        );
        let ours = derive_job_challenge(7, 10, &h(1), &h(2), &h(3), &h(4), 5);
        let consensus = kaspa_consensus_core::palw::palw_job_challenge(7, 10, &h(1), &h(2), &h(3), &h(4), 5);
        assert_eq!(ours, consensus, "preimage layouts must stay byte-identical");
    }

    #[test]
    fn output_commitment_is_the_live_receipt_v3_function() {
        let challenge = h(3);
        let tokens = vec![10u32, 20, 30];
        // Byte-parity assertion: our helper IS output_commitment_v3, not a re-implementation.
        assert_eq!(salted_output_commitment(&tokens, &challenge), output_commitment_v3(&tokens, &challenge));
        // The challenge really salts it — same tokens under a different challenge differ.
        assert_ne!(salted_output_commitment(&tokens, &challenge), salted_output_commitment(&tokens, &h(4)));
        // …and the token vector is length-bound (no extension collision).
        assert_ne!(
            hash64_hex(&salted_output_commitment(&[10, 20], &challenge)),
            hash64_hex(&salted_output_commitment(&tokens, &challenge))
        );
    }
}
