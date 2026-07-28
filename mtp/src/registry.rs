//! Registration + claim (ADR-0027 §5, §6.4, §11-B). A participant binds a GitHub
//! handle to a `misakatest:` ML-DSA-87 key by signing a server-issued challenge;
//! at TGE the same key signs the mainnet receiving address (PQ-key continuity).

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_hashes::{Hash64, blake2b_512_address_payload};
use kaspa_txscript::verify_mldsa87_with_context;

use crate::{MTP_CLAIM_CONTEXT, MTP_REGISTER_CONTEXT};

/// ML-DSA-87 verification-key length.
pub const MLDSA87_PK_LEN: usize = 2592;
/// ML-DSA-87 signature length.
pub const MLDSA87_SIG_LEN: usize = 4627;

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum RegistrationError {
    #[error("address does not parse")]
    BadAddress,
    #[error("address prefix is not the expected testnet prefix")]
    WrongPrefix,
    #[error("address is not a v2 ML-DSA-87 P2PKH (64-byte payload)")]
    WrongVersion,
    #[error("pubkey is not {MLDSA87_PK_LEN} bytes")]
    BadPubkeyLen,
    #[error("pubkey does not hash to the address payload")]
    KeyAddressMismatch,
    #[error("signature does not verify over the challenge")]
    BadSignature,
}

/// A verified registration binding a GitHub handle to an ML-DSA-87 identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    pub github: String,
    pub address: String,
    pub pubkey: Vec<u8>,
}

/// Bind `pubkey` to `address_str`: the address must be a testnet v2 ML-DSA-87
/// P2PKH whose 64-byte payload equals `blake2b_512_address_payload(pubkey)`.
/// (The address payload is a HASH, not the pubkey — the full 2592-byte pubkey
/// must be transmitted separately, §4 note.)
fn bind_key_to_address(address_str: &str, pubkey: &[u8], expected_prefix: Prefix) -> Result<(), RegistrationError> {
    let addr = Address::try_from(address_str).map_err(|_| RegistrationError::BadAddress)?;
    if addr.prefix != expected_prefix {
        return Err(RegistrationError::WrongPrefix);
    }
    if addr.version != Version::PubKeyHashMlDsa87 {
        return Err(RegistrationError::WrongVersion);
    }
    if pubkey.len() != MLDSA87_PK_LEN {
        return Err(RegistrationError::BadPubkeyLen);
    }
    let addr_payload: [u8; 64] = (&addr.payload[..]).try_into().map_err(|_| RegistrationError::WrongVersion)?;
    if blake2b_512_address_payload(pubkey) != Hash64::from_bytes(addr_payload) {
        return Err(RegistrationError::KeyAddressMismatch);
    }
    Ok(())
}

/// The canonical Appendix-B registration challenge — the exact bytes a participant's key signs.
///
/// Deterministic in every field, so the issuing side and the verifying side recompute the same
/// message; changing network/github/address/nonce/issued_at flips the signature.
///
/// It lives HERE, beside [`verify_registration`], rather than in the service crate: the signer and
/// the verifier are now different programs (a participant's CLI, the operator's ingest), and a
/// challenge builder able to drift from its own verifier is a protocol split waiting to happen.
/// One definition, both sides.
pub fn registration_challenge(network: &str, github: &str, address: &str, nonce_hex: &str, issued_at_ms: u64) -> Vec<u8> {
    format!(
        "MISAKA-TESTNET-POINTS-REGISTRATION v1\nnetwork: {network}\ngithub: {github}\naddress: {address}\nnonce: {nonce_hex}\nissued_at: {issued_at_ms}"
    )
    .into_bytes()
}

/// Verify a registration: the address binds the pubkey, and the ML-DSA-87
/// signature verifies over the exact `challenge` bytes (the §11-B registration
/// message) under [`MTP_REGISTER_CONTEXT`]. A wrong length or bad signature is a
/// hard reject (never a panic — `verify_*` returns `Err` on malformed input).
pub fn verify_registration(
    github: &str,
    address_str: &str,
    pubkey: &[u8],
    challenge: &[u8],
    signature: &[u8],
    expected_prefix: Prefix,
) -> Result<Registration, RegistrationError> {
    bind_key_to_address(address_str, pubkey, expected_prefix)?;
    if !matches!(verify_mldsa87_with_context(pubkey, challenge, signature, MTP_REGISTER_CONTEXT), Ok(true)) {
        return Err(RegistrationError::BadSignature);
    }
    Ok(Registration { github: github.to_string(), address: address_str.to_string(), pubkey: pubkey.to_vec() })
}

/// Verify a TGE claim: the same registered `pubkey` signs the `claim` message
/// (§11-B: identity + mainnet_address + total_points_ack + nonce) under
/// [`MTP_CLAIM_CONTEXT`]. Returns true iff the signature is valid.
pub fn verify_claim(pubkey: &[u8], claim: &[u8], signature: &[u8]) -> bool {
    matches!(verify_mldsa87_with_context(pubkey, claim, signature, MTP_CLAIM_CONTEXT), Ok(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;

    // Derive a real misakatest address from a real ML-DSA-87 key.
    fn key_and_addr(seed: u8) -> (ValidatorKey, Vec<u8>, String) {
        let key = ValidatorKey::from_seed([seed; 32]);
        let pk = key.public_key().to_vec();
        let payload = blake2b_512_address_payload(&pk);
        let addr = Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &payload.as_bytes());
        (key, pk, addr.to_string())
    }

    #[test]
    fn valid_registration_round_trips() {
        let (key, pk, addr) = key_and_addr(0x21);
        let challenge = b"MISAKA-TESTNET-POINTS-REGISTRATION v1\ngithub: alice\nnonce: ab..";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        let reg = verify_registration("alice", &addr, &pk, challenge, &sig, Prefix::Testnet).unwrap();
        assert_eq!(reg.github, "alice");
        assert_eq!(reg.pubkey, pk);
    }

    #[test]
    fn tampered_challenge_or_wrong_key_is_rejected() {
        let (key, pk, addr) = key_and_addr(0x22);
        let challenge = b"MISAKA-TESTNET-POINTS-REGISTRATION v1\ngithub: bob";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        // tampered message
        assert_eq!(
            verify_registration("bob", &addr, &pk, b"different message", &sig, Prefix::Testnet),
            Err(RegistrationError::BadSignature)
        );
        // a different key whose address does not match
        let (_k2, pk2, _a2) = key_and_addr(0x33);
        assert_eq!(
            verify_registration("bob", &addr, &pk2, challenge, &sig, Prefix::Testnet),
            Err(RegistrationError::KeyAddressMismatch)
        );
        // wrong context (signed under claim, verified as register) fails
        let sig_claim = key.sign_with_context(challenge, MTP_CLAIM_CONTEXT);
        assert_eq!(
            verify_registration("bob", &addr, &pk, challenge, &sig_claim, Prefix::Testnet),
            Err(RegistrationError::BadSignature)
        );
    }

    #[test]
    fn claim_round_trips_and_rejects_tamper_and_wrong_context() {
        // §5 test 6 (ADR-0038 D8): the TGE claim path — the registered key signs the
        // claim message under MTP_CLAIM_CONTEXT. Closes verify_claim's zero coverage.
        let (key, pk, _addr) = key_and_addr(0x51);
        // §11-B claim message: identity + mainnet_address + total_points_ack + nonce.
        let claim = b"MISAKA-TESTNET-POINTS-CLAIM v1\nid: gh:alice\nmainnet: misaka:xyz\ntotal_points_ack: 1234000\nnonce: 00ff";
        let sig = key.sign_with_context(claim, MTP_CLAIM_CONTEXT);
        assert!(verify_claim(&pk, claim, &sig), "a correct claim signature must verify");

        // tampered message → reject.
        assert!(!verify_claim(&pk, b"MISAKA-TESTNET-POINTS-CLAIM v1\nid: gh:mallory", &sig));
        // a different key → reject.
        let (_k2, pk2, _a2) = key_and_addr(0x52);
        assert!(!verify_claim(&pk2, claim, &sig));
        // a signature made under the REGISTER context must NOT verify as a claim
        // (cross-context domain separation, D7).
        let sig_register = key.sign_with_context(claim, MTP_REGISTER_CONTEXT);
        assert!(!verify_claim(&pk, claim, &sig_register), "register-context sig is not a valid claim");
        // a malformed (short) signature is a hard reject, never a panic.
        assert!(!verify_claim(&pk, claim, &[0u8; 10]));
    }

    #[test]
    fn mainnet_address_prefix_is_rejected_for_testnet_registration() {
        let (key, pk, _) = key_and_addr(0x24);
        let payload = blake2b_512_address_payload(&pk);
        let mainnet_addr = Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &payload.as_bytes()).to_string();
        let challenge = b"x";
        let sig = key.sign_with_context(challenge, MTP_REGISTER_CONTEXT);
        assert_eq!(
            verify_registration("eve", &mainnet_addr, &pk, challenge, &sig, Prefix::Testnet),
            Err(RegistrationError::WrongPrefix)
        );
    }
}
