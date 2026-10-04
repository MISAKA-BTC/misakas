//! **The FALLBACK envelope** (ADR-0172 §5; algo 8 at or past `palw_accounting_v2`): what a reserve block carries in `palw_commitment` — the bond it answers
//! for, the ML-DSA-87 key and a signature over its own header position. Algo 8 below the fence is the heartbeat and carries NOTHING; from the fence on it is FALLBACK
//! and must carry exactly this, statelessly valid. Whether the key is the bond's, the bond eligible, the floor Idle and the slot unspent are the fold's (E3).
//!
//! **Validity is cheap, credit is bonded**: any key signs a valid FALLBACK (the clock stays permissionless); only an eligible bond's is credited.
//!
//! The signature binds the network, the pre-PoW hash (parents, DAA, merkle roots), the timestamp, the nonce and the bond. The nonce is signed for the round envelope's reason
//! (ADR-0125): the envelope is outside the PoW pre-image and inside the block identity, so an unsigned nonce would let anyone re-solve the header and re-announce the envelope
//! as another valid block. The ML-DSA context is the attempt envelope's (`PALW_ATTEMPT_V2_MLDSA87_CONTEXT`): a new context would change the ruleset's signature-context
//! set (ADR-0165 §5.1); the message domain below separates the meanings.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

pub const PALW_FALLBACK_CARRIAGE_MAGIC_V1: [u8; 4] = *b"PFB1";
pub const PALW_FALLBACK_ENVELOPE_VERSION_V1: u8 = 1;
/// The message domain (keyed BLAKE2b). The ML-DSA context is the attempt envelope's.
pub const PALW_FALLBACK_SIGNING_DOMAIN_V1: &[u8] = b"MISAKA-FALLBACK-V1";

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwFallbackEnvelopeV1 {
    pub version: u8,
    pub network_domain: Hash64,
    pub bond: PalwBondKeyV2,
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFallbackEnvelopeError {
    #[error("the FALLBACK carriage is undecodable: {0}")]
    Undecodable(&'static str),
    #[error("unsupported FALLBACK envelope version {got} (expected {expected})")]
    UnsupportedVersion { got: u8, expected: u8 },
    #[error("the FALLBACK key is {got} bytes, not the ML-DSA-87 {expected}")]
    PublicKeyLength { got: usize, expected: usize },
    #[error("the FALLBACK signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the FALLBACK envelope names another network")]
    NetworkDomainMismatch,
    #[error("the FALLBACK signature does not verify under the carried key")]
    SignatureInvalid,
}

/// `H(domain ‖ network ‖ pre-pow hash ‖ timestamp ‖ nonce ‖ bond)` — what the producer signs once the header is solved.
pub fn palw_fallback_signing_message_v1(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp_ms: u64,
    nonce: u64,
    bond: &PalwBondKeyV2,
) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FALLBACK_SIGNING_DOMAIN_V1).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp_ms.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **An operator FALLBACK's execution key — the seed it gives a claim it binds** (ADR-0172 §5.7, lane A): `H(domain ‖ network ‖ pre-pow hash ‖ bond ‖ nonce)`. Derived from the header
/// alone: invariant under the signature (outside it) and the timestamp, moved by the parents (inside the pre-PoW hash) and the nonce — every re-roll costs one `2^-24` puzzle, which is
/// MORE than the ~279 junk draws a BASE-0 operator binder cost (ADR-0152 / `palw_panel_anchor_execution_v1`). Never the block identity.
pub fn palw_fallback_execution_key_v1(network_domain: Hash64, pre_pow_hash: Hash64, bond: &PalwBondKeyV2, nonce: u64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA-FALLBACK-EXEC-V1").to_state();
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

impl PalwFallbackEnvelopeV1 {
    /// The header-carriage wire form: magic, then borsh.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_FALLBACK_CARRIAGE_MAGIC_V1.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Magic, borsh, and nothing after it.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwFallbackEnvelopeError> {
        let Some(body) = bytes.strip_prefix(&PALW_FALLBACK_CARRIAGE_MAGIC_V1) else {
            return Err(PalwFallbackEnvelopeError::Undecodable("payload does not start with the PFB1 magic"));
        };
        let mut slice = body;
        let decoded = <Self as borsh::BorshDeserialize>::deserialize(&mut slice).map_err(|_| PalwFallbackEnvelopeError::Undecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwFallbackEnvelopeError::Undecodable("trailing bytes"));
        }
        Ok(decoded)
    }

    /// Shape only: the version and the two ML-DSA-87 lengths.
    pub fn validate_shape(&self) -> Result<(), PalwFallbackEnvelopeError> {
        if self.version != PALW_FALLBACK_ENVELOPE_VERSION_V1 {
            return Err(PalwFallbackEnvelopeError::UnsupportedVersion { got: self.version, expected: PALW_FALLBACK_ENVELOPE_VERSION_V1 });
        }
        if self.pubkey.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN {
            return Err(PalwFallbackEnvelopeError::PublicKeyLength { got: self.pubkey.len(), expected: crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN });
        }
        if self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(PalwFallbackEnvelopeError::SignatureLength {
                got: self.signature.len(),
                expected: crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
            });
        }
        Ok(())
    }

    /// **The header stage's whole check**: shape, network, and the signature over the header position under the carried key.
    pub fn validate_stateless<V>(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp_ms: u64,
        nonce: u64,
        verify_mldsa87: V,
    ) -> Result<(), PalwFallbackEnvelopeError>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.validate_shape()?;
        if self.network_domain != network_domain {
            return Err(PalwFallbackEnvelopeError::NetworkDomainMismatch);
        }
        let message = palw_fallback_signing_message_v1(network_domain, pre_pow_hash, timestamp_ms, nonce, &self.bond);
        if !verify_mldsa87(&self.pubkey, message.as_byte_slice(), &self.signature, crate::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT) {
            return Err(PalwFallbackEnvelopeError::SignatureInvalid);
        }
        Ok(())
    }
}

/// **What the header stage asks of an algo-8 header's `palw_commitment`.** Below the fence (`fallback_armed = false`) a heartbeat carries nothing — byte for byte the
/// rule it always was. At or past it the commitment must be a well-formed FALLBACK envelope. `Ok(())` otherwise.
pub fn check_algo8_commitment_shape_v1(fallback_armed: bool, palw_commitment: &[u8]) -> Result<(), String> {
    if !fallback_armed {
        return if palw_commitment.is_empty() {
            Ok(())
        } else {
            Err(format!("a heartbeat header carries a {}-byte palw_commitment below palw_accounting_v2", palw_commitment.len()))
        };
    }
    if palw_commitment.len() > crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES {
        return Err(format!("a FALLBACK commitment of {} bytes is above the cap", palw_commitment.len()));
    }
    PalwFallbackEnvelopeV1::decode(palw_commitment).and_then(|e| e.validate_shape()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn envelope() -> PalwFallbackEnvelopeV1 {
        PalwFallbackEnvelopeV1 {
            version: 1,
            network_domain: Hash64::from_u64_word(7),
            bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 1)),
            pubkey: vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN],
            signature: vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    #[test]
    fn a_heartbeat_below_the_fence_carries_nothing_and_a_fallback_past_it_carries_exactly_an_envelope() {
        assert!(check_algo8_commitment_shape_v1(false, &[]).is_ok(), "the heartbeat, byte for byte");
        assert!(check_algo8_commitment_shape_v1(false, &envelope().encode()).is_err(), "below the fence an envelope is malleable bytes");
        assert!(check_algo8_commitment_shape_v1(true, &[]).is_err(), "a FALLBACK without an envelope is no FALLBACK");
        let bytes = envelope().encode();
        assert!(check_algo8_commitment_shape_v1(true, &bytes).is_ok());
        assert!(bytes.len() <= crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES, "{} bytes", bytes.len());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(check_algo8_commitment_shape_v1(true, &trailing).is_err());
        let mut short = envelope();
        short.signature.pop();
        assert!(check_algo8_commitment_shape_v1(true, &short.encode()).is_err());
        let mut v2 = envelope();
        v2.version = 2;
        assert!(check_algo8_commitment_shape_v1(true, &v2.encode()).is_err());
        assert_eq!(PalwFallbackEnvelopeV1::decode(&bytes).unwrap(), envelope());
    }

    #[test]
    fn the_signature_binds_the_network_the_position_the_nonce_and_the_bond() {
        let e = envelope();
        let base = palw_fallback_signing_message_v1(e.network_domain, Hash64::from_u64_word(1), 10, 5, &e.bond);
        assert_ne!(base, palw_fallback_signing_message_v1(Hash64::from_u64_word(8), Hash64::from_u64_word(1), 10, 5, &e.bond));
        assert_ne!(base, palw_fallback_signing_message_v1(e.network_domain, Hash64::from_u64_word(2), 10, 5, &e.bond));
        assert_ne!(base, palw_fallback_signing_message_v1(e.network_domain, Hash64::from_u64_word(1), 11, 5, &e.bond));
        assert_ne!(base, palw_fallback_signing_message_v1(e.network_domain, Hash64::from_u64_word(1), 10, 6, &e.bond), "the nonce is signed");
        let other = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 2));
        assert_ne!(base, palw_fallback_signing_message_v1(e.network_domain, Hash64::from_u64_word(1), 10, 5, &other));
        // The stateless check is the verifier's verdict on that message, after shape and network.
        let ok = |_: &[u8], m: &[u8], _: &[u8], _: &[u8]| m == base.as_byte_slice();
        assert!(e.validate_stateless(e.network_domain, Hash64::from_u64_word(1), 10, 5, ok).is_ok());
        assert_eq!(e.validate_stateless(e.network_domain, Hash64::from_u64_word(1), 10, 6, ok), Err(PalwFallbackEnvelopeError::SignatureInvalid));
        assert_eq!(
            e.validate_stateless(Hash64::from_u64_word(9), Hash64::from_u64_word(1), 10, 5, ok),
            Err(PalwFallbackEnvelopeError::NetworkDomainMismatch)
        );
    }
}
