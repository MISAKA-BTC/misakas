//! kaspa-pq (misaka) genesis premine.
//!
//! A single 15B KAS UTXO locked to a **2-of-3 ML-DSA-65 P2SH multisig**, baked
//! into genesis on every network (same script everywhere). This is the genesis
//! half of the 30B final supply (the other 15B is mined over 20 years; see the
//! emission table in `consensus/src/processes/coinbase.rs`).
//!
//! The redeem script is `<2> <pk0> <pk1> <pk2> <3> OP_CHECKMULTISIGMLDSA65`
//! (built by `kaspa_txscript::standard::multisig::multisig_redeem_script_mldsa65`);
//! only its BLAKE2b-256 hash appears here, wrapped in the P2SH template
//! `OP_BLAKE2B OP_DATA32 <hash> OP_EQUAL`. The three signing keys (devnet) are in
//! the repo-root `misaka-devnet-multisig-keys.json` and the devnet multisig
//! address is
//! `misakadev:prcvn42vpqtzrmnz69agafljqudd74slx9slq3mh939ge8fmvazdstg70sh3t`.
//! Regenerate via the `gen_misaka_devnet_multisig` test in kaspa-txscript.

use crate::{
    constants::SOMPI_PER_KASPA,
    tx::{ScriptPublicKey, ScriptVec, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::Hash64;

/// Premine amount: 15B KAS.
pub const MISAKA_PREMINE_SOMPI: u64 = 15_000_000_000 * SOMPI_PER_KASPA;

/// P2SH `script_public_key` script bytes (version 0):
/// `OP_BLAKE2B(0xaa) OP_DATA32(0x20) <32-byte redeem-script hash> OP_EQUAL(0x87)`.
const MISAKA_PREMINE_P2SH_SCRIPT: [u8; 35] = [
    0xaa, 0x20, 0xf0, 0xc9, 0xd5, 0x4c, 0x08, 0x16, 0x21, 0xee, 0x62, 0xd1, 0x7a, 0x8e, 0xa7, 0xf2, 0x07, 0x1a, 0xdf, 0x56, 0x1f,
    0x31, 0x61, 0xf0, 0x47, 0x77, 0x2c, 0x4a, 0x8c, 0x9d, 0x3b, 0x67, 0x44, 0xd8, 0x87,
];

/// Deterministic sentinel txid for the single premine UTXO: ASCII "misaka-premine"
/// (14 bytes) zero-padded to the 64-byte `Hash64` width. Fixed because it feeds
/// the genesis `utxo_commitment`.
#[rustfmt::skip]
const MISAKA_PREMINE_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x65, // "misaka-premine"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// The canonical kaspa-pq genesis premine UTXO set: one 15B KAS multisig P2SH
/// UTXO, spendable from block 0 (`is_coinbase: false`, no maturity delay).
pub fn misaka_premine_utxos() -> UtxoCollection {
    let script_public_key = ScriptPublicKey::new(0, ScriptVec::from_slice(&MISAKA_PREMINE_P2SH_SCRIPT));
    let outpoint = TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index: 0 };
    let entry = UtxoEntry { amount: MISAKA_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false };
    UtxoCollection::from_iter([(outpoint, entry)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muhash::MuHashExtensions;
    use kaspa_muhash::MuHash;

    /// Prints the genesis `utxo_commitment` to hardcode in `genesis.rs`. Run:
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::print_premine_commitment -- --nocapture`
    #[test]
    fn print_premine_commitment() {
        let mut ms = MuHash::new();
        for (outpoint, entry) in misaka_premine_utxos() {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        println!("PREMINE_UTXO_COMMITMENT: Hash::from_bytes([{rust}])");
    }
}
