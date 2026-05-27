use crate::{
    block::Block,
    header::{CompressedParents, Header},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::Transaction,
};
use kaspa_hashes::{Hash, ZERO_HASH};
use kaspa_muhash::EMPTY_MUHASH;

/// The constants uniquely representing the genesis block
#[derive(Clone, Debug)]
pub struct GenesisBlock {
    pub hash: Hash,
    pub version: u16,
    pub hash_merkle_root: Hash,
    pub utxo_commitment: Hash,
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub coinbase_payload: &'static [u8],
}

impl GenesisBlock {
    pub fn build_genesis_transactions(&self) -> Vec<Transaction> {
        vec![Transaction::new(0, Vec::new(), Vec::new(), 0, SUBNETWORK_ID_COINBASE, 0, self.coinbase_payload.to_vec())]
    }
}

impl From<&GenesisBlock> for Header {
    fn from(genesis: &GenesisBlock) -> Self {
        Header::new_finalized(
            genesis.version,
            CompressedParents::default(),
            genesis.hash_merkle_root,
            ZERO_HASH,
            genesis.utxo_commitment,
            genesis.timestamp,
            genesis.bits,
            genesis.nonce,
            genesis.daa_score,
            0.into(),
            0,
            ZERO_HASH,
        )
    }
}

impl From<&GenesisBlock> for Block {
    fn from(genesis: &GenesisBlock) -> Self {
        Block::new(genesis.into(), genesis.build_genesis_transactions())
    }
}

impl From<(&Header, &'static [u8])> for GenesisBlock {
    fn from((header, payload): (&Header, &'static [u8])) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            hash_merkle_root: header.hash_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            coinbase_payload: payload,
        }
    }
}

// kaspa-pq genesis blocks.
//
// All four genesis constants below are freshly minted for the kaspa-pq fork;
// they do **not** continue the mainline Kaspa ledger. The structural fields
// (bits, version) follow upstream conventions; the content fields (hash,
// hash_merkle_root, coinbase_payload, daa_score, timestamp) are kaspa-pq
// specific.
//
// Workflow for filling in `hash` and `hash_merkle_root`:
//   1. With `hash` / `hash_merkle_root` set to ZERO_HASH (the placeholders
//      below), run `cargo test -p kaspa-consensus-core --lib
//      config::genesis::tests::test_genesis_hashes -- --nocapture`.
//   2. The test panics in `assert_hashes_eq` printing the actual computed
//      hash bytes for each genesis variant.
//   3. Copy those values into the `hash:` and `hash_merkle_root:` fields
//      here, then re-run the test to confirm.
//
// `utxo_commitment` is left as `EMPTY_MUHASH` for Phase 2. It will be
// replaced with the empty-state finalization of LtHash16_1024 in Phase 3
// (see docs/adr/0003-lthash-utxo-accumulator.md and ADR-0004 for the
// 64-byte commitment design — for the PoC we keep the field 32 bytes).
//
// `nonce` is left at 0. For mainnet / testnet (which validate PoW) the
// nonce will need to be mined against the kaspa-pq target before launch;
// for simnet / devnet `skip_proof_of_work` is true and the nonce is
// inert.

/// The genesis block of the block-DAG which serves as the public transaction ledger for kaspa-pq mainnet.
pub const GENESIS: GenesisBlock = GenesisBlock {
    // Computed by `gen_kaspa_pq_genesis_hashes` (see tests below).
    hash: Hash::from_bytes([
        0x31, 0x47, 0xfe, 0x86, 0x08, 0xf8, 0x7e, 0xd0, 0x7d, 0x9d, 0x83, 0x7e, 0x08, 0xee, 0x85, 0x81, 0xa2, 0x7c, 0xaf, 0x7e, 0x6a,
        0x03, 0xd6, 0x1e, 0x57, 0x56, 0x9a, 0x17, 0x2f, 0x71, 0x71, 0xef,
    ]),
    version: 0,
    // Computed by `gen_kaspa_pq_genesis_hashes` from the coinbase payload below.
    hash_merkle_root: Hash::from_bytes([
        0x00, 0x04, 0x05, 0x45, 0xe4, 0x22, 0x5f, 0xfa, 0x1f, 0xf2, 0x28, 0xa6, 0xde, 0x10, 0xd7, 0x85, 0x15, 0xbf, 0xf8, 0x71, 0xa3,
        0x2f, 0xe6, 0xe0, 0xa0, 0x98, 0x6e, 0x54, 0x19, 0xa8, 0xa1, 0x90,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    // 2026-05-28 00:00:00 UTC — kaspa-pq launch reference timestamp.
    timestamp: 1748390400000,
    // Difficulty target carried over from upstream Kaspa mainnet genesis; the
    // kaspa-pq launch op-team may want to revisit this once Phase 3/6 settle.
    bits: 486722099,
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "kaspapq-mainnet"
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x70, 0x71, 0x2d, 0x6d, 0x61, 0x69, 0x6e, 0x6e, 0x65, 0x74,
    ],
};

pub const TESTNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0xfc, 0xdc, 0x05, 0x2c, 0x59, 0xd4, 0xfb, 0xde, 0xff, 0x24, 0x05, 0x65, 0x1b, 0x82, 0x55, 0xb3, 0xa7, 0x7b, 0x54, 0x32, 0x2e,
        0xdb, 0xd3, 0x72, 0x44, 0x01, 0x02, 0xd6, 0xcf, 0x75, 0x58, 0x8e,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x42, 0xd6, 0x20, 0x64, 0xee, 0xe7, 0xe2, 0xb3, 0xc4, 0x30, 0x94, 0xe7, 0x49, 0x95, 0x58, 0x5d, 0xe0, 0x86, 0x59, 0xf7, 0xed,
        0xbb, 0xdc, 0x5c, 0x2b, 0x81, 0x3c, 0x40, 0x2e, 0x53, 0x63, 0x8d,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 1748390400000,
    bits: 0x1e7fffff,
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "kaspapq-testnet"
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x70, 0x71, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74,
    ],
};

pub const TESTNET11_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0x8b, 0x50, 0xa0, 0xbe, 0x62, 0x6d, 0x33, 0xd2, 0x2b, 0x44, 0x1a, 0x39, 0x8a, 0x4e, 0xfb, 0x7d, 0x6d, 0xaf, 0xad, 0xb8, 0xd0,
        0x73, 0x8b, 0x10, 0xa5, 0xf3, 0x54, 0x59, 0x4d, 0xb4, 0x55, 0x67,
    ]),
    hash_merkle_root: Hash::from_bytes([
        0x14, 0xf6, 0x9b, 0x7f, 0x6c, 0xa9, 0xa3, 0x3e, 0x30, 0xe2, 0x53, 0x2d, 0x81, 0x2c, 0x17, 0xa1, 0xe2, 0xde, 0x07, 0xda, 0x9d,
        0x4c, 0xa2, 0xa6, 0x07, 0x7c, 0x47, 0x51, 0xb0, 0xae, 0x69, 0xf6,
    ]),
    bits: 504155340, // see `gen_testnet11_genesis`
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "kaspapq-testnet"
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x70, 0x71, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74,
        11, 1,                                                                                                // TN11, kaspa-pq Relaunch 1
    ],
    ..TESTNET_GENESIS
};

pub const SIMNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0x15, 0x85, 0x90, 0x34, 0x31, 0x16, 0xe3, 0xd3, 0xf0, 0xa7, 0x54, 0xa2, 0x42, 0xeb, 0x6c, 0xad, 0x93, 0x63, 0x9b, 0x2e, 0x20,
        0x07, 0x04, 0xfb, 0x7e, 0xc1, 0x30, 0xdd, 0x56, 0x11, 0x05, 0x94,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0xa3, 0x52, 0xf7, 0xc5, 0x8b, 0x48, 0x94, 0x6a, 0x10, 0x90, 0x1f, 0x94, 0x95, 0x77, 0x7c, 0x23, 0x29, 0x86, 0x07, 0x00, 0xe2,
        0x05, 0x0d, 0xed, 0x53, 0xe2, 0x68, 0xad, 0xff, 0xc2, 0xf9, 0x69,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 1748390400000,
    bits: 0x207fffff,
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                           // Varint
        0x00,                                                                                           // OP-FALSE
        // "kaspapq-simnet"
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x70, 0x71, 0x2d, 0x73, 0x69, 0x6d, 0x6e, 0x65, 0x74,
    ],
};

pub const DEVNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0x4a, 0xea, 0x43, 0xc0, 0x20, 0x88, 0x30, 0x5b, 0x87, 0x09, 0x66, 0x7d, 0x83, 0x43, 0x23, 0x17, 0x82, 0x47, 0xfc, 0xc3, 0x82,
        0x06, 0xe4, 0x0c, 0x4b, 0x67, 0xa9, 0xb6, 0xe3, 0x1c, 0xdd, 0x8d,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x88, 0xf9, 0xf9, 0xb4, 0xa2, 0x38, 0x7e, 0x3b, 0x73, 0xff, 0x75, 0xfb, 0xbc, 0x1f, 0xeb, 0x1d, 0x45, 0x6f, 0x85, 0x8e, 0x61,
        0x7b, 0xed, 0xc3, 0x53, 0x76, 0x7f, 0xd7, 0x63, 0x94, 0x98, 0x4a,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 1748390400000,
    bits: 0x1e21bc1c, // Bits with ~testnet-like difficulty for slow devnet start.
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                           // Varint
        0x00,                                                                                           // OP-FALSE
        // "kaspapq-devnet"
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x70, 0x71, 0x2d, 0x64, 0x65, 0x76, 0x6e, 0x65, 0x74,
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::bps::TenBps, merkle::calc_hash_merkle_root};

    #[test]
    fn test_genesis_hashes() {
        [GENESIS, TESTNET_GENESIS, TESTNET11_GENESIS, SIMNET_GENESIS, DEVNET_GENESIS].into_iter().for_each(|genesis| {
            let block: Block = (&genesis).into();
            assert_hashes_eq(calc_hash_merkle_root(block.transactions.iter()), block.header.hash_merkle_root);
            assert_hashes_eq(block.hash(), genesis.hash);
        });
    }

    /// Helper for the kaspa-pq Phase 2 workflow: compute and print the
    /// correct `hash` and `hash_merkle_root` for every kaspa-pq genesis
    /// constant, so they can be pasted into the `GENESIS` / `TESTNET_GENESIS`
    /// / `TESTNET11_GENESIS` / `SIMNET_GENESIS` / `DEVNET_GENESIS`
    /// declarations above.
    ///
    /// Run with:
    /// `cargo test -p kaspa-consensus-core --lib config::genesis::tests::gen_kaspa_pq_genesis_hashes -- --nocapture`
    #[test]
    fn gen_kaspa_pq_genesis_hashes() {
        for (name, g) in [
            ("GENESIS", &GENESIS),
            ("TESTNET_GENESIS", &TESTNET_GENESIS),
            ("TESTNET11_GENESIS", &TESTNET11_GENESIS),
            ("SIMNET_GENESIS", &SIMNET_GENESIS),
            ("DEVNET_GENESIS", &DEVNET_GENESIS),
        ] {
            // Compute the merkle root that the genesis *should* have, given
            // its coinbase payload. (`g.hash_merkle_root` is the placeholder
            // ZERO_HASH at this point.)
            let coinbase_txs = g.build_genesis_transactions();
            let merkle = calc_hash_merkle_root(coinbase_txs.iter());

            // Reconstruct the genesis header with that merkle root so we can
            // read off the block hash this genesis *should* have.
            let header = Header::new_finalized(
                g.version,
                CompressedParents::default(),
                merkle,
                ZERO_HASH,
                g.utxo_commitment,
                g.timestamp,
                g.bits,
                g.nonce,
                g.daa_score,
                0.into(),
                0,
                ZERO_HASH,
            );

            println!("{name}:");
            println!("    hash_merkle_root: Hash::from_bytes({:#04x?}),", merkle.as_bytes());
            println!("    hash:             Hash::from_bytes({:#04x?}),", header.hash.as_bytes());
        }
    }

    #[test]
    fn gen_testnet11_genesis() {
        let bps = TenBps::bps();
        let mut genesis = TESTNET_GENESIS;
        let target = kaspa_math::Uint256::from_compact_target_bits(genesis.bits);
        let scaled_target = target * bps / 100;
        let scaled_bits = scaled_target.compact_target_bits();
        genesis.bits = scaled_bits;
        if genesis.bits != TESTNET11_GENESIS.bits {
            panic!("Testnet 11: new bits: {}\nnew hash: {:#04x?}", scaled_bits, Block::from(&genesis).hash().as_bytes());
        }
    }

    fn assert_hashes_eq(got: Hash, expected: Hash) {
        if got != expected {
            // Special hex print to ease changing the genesis hash according to the print if needed
            panic!("Got hash {:#04x?} while expecting {:#04x?}", got.as_bytes(), expected.as_bytes());
        }
    }
}
