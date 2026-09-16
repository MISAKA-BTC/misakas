//! **The ML-DSA-87 primitives every lane shares**: the key and signature lengths, the P2PKH
//! script a 64-byte payload pays to, and a key's 64-byte id.
//!
//! They were first written for the DNS-finality overlay (`dns_finality`), and every PALW lane, the
//! coinbase, the premine and the EVM bridge came to read them from there. The overlay is retired;
//! PALW never depended on it, only on these four definitions, so they live here and the overlay
//! re-exports them under its old names. Every value is byte-identical to what it was: the lengths
//! are FIPS 204's, the script is ADR-0019 §8's, and the key id is the unkeyed BLAKE2b-512 of the
//! key — genesis premine outputs, bond payouts and registry ids all hash through them.

use crate::Hash64;
use crate::tx::{ScriptPublicKey, ScriptVec};
use blake2b_simd::Params as Blake2bParams;

/// 2592 bytes — an ML-DSA-87 verification key (FIPS 204), equal to `kaspa_txscript::MLDSA87_PK_LEN`.
pub const MLDSA87_PUBKEY_LEN: usize = 2592;

/// 4627 bytes — an ML-DSA-87 signature (FIPS 204), equal to `kaspa_txscript::MLDSA87_SIG_LEN`.
pub const MLDSA87_SIGNATURE_LEN: usize = 4627;

/// **The ML-DSA-87 P2PKH `ScriptPublicKey` paying `payload`.**
///
/// The 69-byte script is
/// `OpDup ‖ OpBlake2b512 ‖ OpData64 ‖ <payload64> ‖ OpEqualVerify ‖ OpCheckSigMlDsa87` at
/// `ScriptPublicKey` version 0 (ADR-0019 §8). The opcode bytes are literals because
/// `consensus-core` does not depend on `kaspa-txscript`; the output is byte-identical to
/// `kaspa_txscript::pay_to_address_script(&Address::new(_, Version::PubKeyHashMlDsa87, payload))`,
/// which a parity test in the `consensus` crate pins.
pub fn p2pkh_mldsa87_spk(payload: &[u8; 64]) -> ScriptPublicKey {
    const OP_DUP: u8 = 0x76;
    const OP_BLAKE2B_512: u8 = 0xc4;
    const OP_DATA64: u8 = 0x40;
    const OP_EQUAL_VERIFY: u8 = 0x88;
    const OP_CHECKSIG_MLDSA87: u8 = 0xa6;

    let mut script = Vec::with_capacity(69);
    script.push(OP_DUP);
    script.push(OP_BLAKE2B_512);
    script.push(OP_DATA64);
    script.extend_from_slice(payload);
    script.push(OP_EQUAL_VERIFY);
    script.push(OP_CHECKSIG_MLDSA87);
    ScriptPublicKey::new(0, ScriptVec::from_slice(&script))
}

/// **A key's 64-byte id**: the unkeyed BLAKE2b-512 of the verification key. Unkeyed because the
/// input is a fixed-length key, not a multi-field structure.
pub fn mldsa87_key_id(pubkey: &[u8]) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(Blake2bParams::new().hash_length(64).to_state().update(pubkey).finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlay's names are the same values, so nothing that hashed through them moves.
    #[test]
    fn the_overlay_names_are_the_same_values() {
        assert_eq!(crate::dns_finality::STAKE_VALIDATOR_PUBKEY_LEN, MLDSA87_PUBKEY_LEN);
        assert_eq!(crate::dns_finality::STAKE_ATTESTATION_SIG_LEN, MLDSA87_SIGNATURE_LEN);
        let payload = [0x5Au8; 64];
        assert_eq!(crate::dns_finality::p2pkh_mldsa87_spk(&payload), p2pkh_mldsa87_spk(&payload));
        assert_eq!(p2pkh_mldsa87_spk(&payload).script().len(), 69);
        assert_eq!(crate::dns_finality::validator_id_from_pubkey(&[7u8; 32]), mldsa87_key_id(&[7u8; 32]));
    }
}
