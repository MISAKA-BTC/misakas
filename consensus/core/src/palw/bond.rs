//! Current PALW ML-DSA-87 output helpers.

use crate::tx::{ScriptPublicKey, ScriptVec};

/// Construct the standard ML-DSA-87 P2PKH script used by current PALW bond
/// and producer outputs.
pub fn p2pkh_mldsa87_spk(owner_payload: &[u8; 64]) -> ScriptPublicKey {
    // OP_DUP OP_BLAKE2B_512 OP_DATA_64 <payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87
    const OP_DUP: u8 = 0x76;
    const OP_BLAKE2B_512: u8 = 0xc4;
    const OP_DATA64: u8 = 0x40;
    const OP_EQUAL_VERIFY: u8 = 0x88;
    const OP_CHECKSIG_MLDSA87: u8 = 0xa6;

    let mut script = Vec::with_capacity(69);
    script.extend([OP_DUP, OP_BLAKE2B_512, OP_DATA64]);
    script.extend_from_slice(owner_payload);
    script.extend([OP_EQUAL_VERIFY, OP_CHECKSIG_MLDSA87]);
    ScriptPublicKey::new(0, ScriptVec::from_slice(&script))
}

