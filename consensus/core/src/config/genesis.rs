use crate::{
    block::Block,
    header::{CompressedParents, Header},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::Transaction,
};
use kaspa_hashes::{Hash, Hash64, ZERO_HASH64};
use kaspa_muhash::EMPTY_MUHASH;

/// PR-9.5e: non-zero placeholder for every genesis block `hash` until
/// PR-9.5g recomputes the real values. It MUST be non-zero: the real
/// genesis hash will be too, and a zero hash would alias
/// [`crate::blockhash::NONE`] (all-zero), tripping the `!= NONE`
/// reachability invariant in every DAG-building test. It is also
/// distinct from `blockhash::ORIGIN` (all-`0xfe`). All five genesis
/// variants may share this value: each test instantiates a single
/// network's consensus, so the genesis ids never coexist in one
/// store. PR-9.5g replaces each `hash` with the recomputed digest and
/// re-enables `config::genesis::tests::test_genesis_hashes`.
const GENESIS_HASH_PLACEHOLDER: Hash64 = Hash64::from_bytes([0x01u8; 64]);

/// The constants uniquely representing the genesis block.
///
/// PR-9.5c: `hash_merkle_root` widened to `crate::MerkleRoot`
/// (= `Hash64`). PR-9.5e: `hash` widened to [`crate::BlockHash`]
/// (= `Hash64`) — the block-identity flip from ADR-0008.
/// `utxo_commitment` stays 32-byte (`Hash`): it is an accumulator
/// commitment, not a block-hash identity.
#[derive(Clone, Debug)]
pub struct GenesisBlock {
    pub hash: crate::BlockHash,
    pub version: u16,
    pub hash_merkle_root: crate::MerkleRoot,
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
            // PR-9.5c: `accepted_id_merkle_root` widened to
            // Hash64; ZERO_HASH64 is the canonical empty value
            // for a genesis block (no accepted parents).
            ZERO_HASH64,
            genesis.utxo_commitment,
            genesis.timestamp,
            genesis.bits,
            genesis.nonce,
            // PR-9.5d: genesis runs the Phase 1 kHeavyHash algo.
            crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            genesis.daa_score,
            0.into(),
            0,
            // PR-9.5e: pruning_point is a block-hash identity (Hash64).
            ZERO_HASH64,
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
    // Computed by `gen_kaspa_pq_genesis_hashes` (see tests below). This is
    // the kaspa-pq Phase-3 value: the kaspa-pq Phase-2 commit had a different
    // value because `EMPTY_MUHASH` was the upstream multiplicative-MuHash
    // empty-state hash; switching the accumulator to LtHash16_1024 in Phase
    // 3 changed `EMPTY_MUHASH`, which in turn changed the block hash.
    hash: GENESIS_HASH_PLACEHOLDER, // PR-9.5e placeholder — regenerated in PR-9.5g
    version: 0,
    // PR-9.5c: 32-byte mainnet merkle root invalidated by the
    // Hash64 widening; placeholder until PR-9.5g regenerates all
    // 5 genesis hashes through `gen_kaspa_pq_genesis_hashes` (see
    // docs/hash64-migration-inventory.md §"Genesis values").
    hash_merkle_root: ZERO_HASH64,
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
    hash: GENESIS_HASH_PLACEHOLDER, // PR-9.5e placeholder — regenerated in PR-9.5g
    version: 0,
    // PR-9.5c: testnet merkle root invalidated; placeholder
    // until PR-9.5g regen.
    hash_merkle_root: ZERO_HASH64,
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
    hash: GENESIS_HASH_PLACEHOLDER, // PR-9.5e placeholder — regenerated in PR-9.5g
    // PR-9.5c: testnet11 merkle root invalidated; placeholder
    // until PR-9.5g regen.
    hash_merkle_root: ZERO_HASH64,
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
    hash: GENESIS_HASH_PLACEHOLDER, // PR-9.5e placeholder — regenerated in PR-9.5g
    version: 0,
    // PR-9.5c: simnet merkle root invalidated; placeholder
    // until PR-9.5g regen.
    hash_merkle_root: ZERO_HASH64,
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
    hash: GENESIS_HASH_PLACEHOLDER, // PR-9.5e placeholder — regenerated in PR-9.5g
    version: 0,
    // PR-9.5c: devnet merkle root invalidated; placeholder
    // until PR-9.5g regen.
    hash_merkle_root: ZERO_HASH64,
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

    // kaspa-pq Phase 9 PR-9.5c: the 5 genesis hash_merkle_root constants are now
    // ZERO_HASH64 placeholders, and `block.hash()` (still Hash32) vs
    // `calc_hash_merkle_root` (now Hash64) no longer share a type, so this
    // assertion test is excluded from compilation until PR-9.5g regenerates the
    // genesis constants via `gen_kaspa_pq_genesis_hashes` below and re-enables it.
    // See docs/hash64-migration-inventory.md §"Genesis values".
    #[cfg(any())]
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
                // PR-9.5c: accepted_id_merkle_root widened to Hash64.
                ZERO_HASH64,
                g.utxo_commitment,
                g.timestamp,
                g.bits,
                g.nonce,
                // PR-9.5d: Phase 1 kHeavyHash algo id.
                crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
                g.daa_score,
                0.into(),
                0,
                // PR-9.5e: pruning_point is a block-hash identity (Hash64).
                ZERO_HASH64,
            );

            // PR-9.5g uses this output: both `hash_merkle_root` and `hash` are
            // now 64-byte Hash64 values (PR-9.5e widened BlockHash). Paste each
            // into a `Hash64::from_bytes([...])` over the corresponding genesis
            // constant above.
            println!("{name}:");
            println!("    hash_merkle_root: Hash64::from_bytes({:#04x?}),", merkle.as_bytes());
            println!("    hash:             Hash64::from_bytes({:#04x?}),", header.hash.as_bytes());
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

    // Only used by the `#[cfg(any())]`-excluded `test_genesis_hashes`
    // above; excluded in lockstep until PR-9.5g re-enables that test.
    #[cfg(any())]
    fn assert_hashes_eq(got: Hash, expected: Hash) {
        if got != expected {
            // Special hex print to ease changing the genesis hash according to the print if needed
            panic!("Got hash {:#04x?} while expecting {:#04x?}", got.as_bytes(), expected.as_bytes());
        }
    }
}
