use crate::{
    block::Block,
    header::{CompressedParents, Header},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::Transaction,
};
use kaspa_hashes::{Hash64, ZERO_HASH64};

/// The constants uniquely representing the genesis block.
///
/// PR-9.5c: `hash_merkle_root` widened to `crate::MerkleRoot`
/// (= `Hash64`). PR-9.5e: `hash` widened to [`crate::BlockHash`]
/// (= `Hash64`) — the block-identity flip from ADR-0008.
/// `utxo_commitment` is a 64-byte `Hash64` BLAKE2b-512 accumulator
/// commitment (ADR-0004 / design §12), not a block-hash identity.
#[derive(Clone, Debug)]
pub struct GenesisBlock {
    pub hash: crate::BlockHash,
    pub version: u16,
    pub hash_merkle_root: crate::MerkleRoot,
    // kaspa-pq (ADR-0004 / design §12): 64-byte BLAKE2b-512 UTXO-set commitment.
    pub utxo_commitment: Hash64,
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
        // ADR-0020: genesis is EVM-inert. `genesis.version` is `0` (< EVM_HEADER_VERSION),
        // so `new_finalized` defaults the EVM commitments (payload hash + execution root)
        // to zero and the preimage gate skips them — every existing genesis hash is
        // unchanged by the EVM lane.
        let mut header = Header::new_finalized(
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
        );
        // A public/value PALW deployment is a Header-v4 re-genesis. Its trusted root has no lane
        // history or jump pointers, but commits that exact empty accumulator so block 1 can seed a
        // fully authenticated fork-local transition. Existing v0 genesis headers stay byte-identical.
        if genesis.version >= crate::constants::PALW_ANTISPAM_HEADER_VERSION {
            header.palw_spam_accumulator_commitment =
                crate::palw_antispam::palw_spam_accumulator_commitment(genesis.daa_score, 0, 0, 0, None, None);
            header.finalize();
        }
        header
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
// `utxo_commitment` commits to the kaspa-pq genesis premine: a single 10B KAS main
// UTXO (re-genesis 2026-07-20), locked to a single-key ML-DSA-87 P2PKH (see
// `config::premine::misaka_premine_utxos`). It is the MuHash over that UTXO set. Per
// audit H-01 the 10B main wallet is network-dependent — mainnet uses the operator
// custody address, the test networks the operator's value-less test address — so the
// mainnet `utxo_commitment`/`hash` differ from testnet/devnet/simnet. The
// premine UTXO itself is imported into the UTXO store at consensus init
// (`consensus::utxo_set_override::set_initial_utxo_set`).
//
// `nonce` is left at 0. For mainnet / testnet (which validate PoW) the
// nonce will need to be mined against the kaspa-pq target before launch;
// for simnet / devnet `skip_proof_of_work` is true and the nonce is
// inert.

/// The genesis block of the block-DAG which serves as the public transaction ledger for kaspa-pq mainnet.
pub const GENESIS: GenesisBlock = GenesisBlock {
    // Computed by `gen_kaspa_pq_genesis_hashes` (see tests below). Carries the
    // ADR-0007 Phase-3 (BLAKE2b-512 ∥ SHA3-512) re-genesis coinbase marker "-bs3"
    // (see `coinbase_payload`), so this hash differs from the prior Argon2id-era
    // mainnet genesis — an un-wiped node trips the startup genesis-mismatch guard.
    hash: Hash64::from_bytes([
        0xe2, 0xe3, 0xa5, 0x61, 0xba, 0x88, 0x49, 0x86, 0x24, 0xb0, 0x15, 0xe7, 0x69, 0x23, 0x54, 0x6b, 0xc2, 0x62, 0x6a, 0xe2, 0x0b,
        0x55, 0x65, 0x3b, 0x26, 0x9b, 0x98, 0xfc, 0x7f, 0x48, 0x72, 0x8e, 0x64, 0x92, 0x24, 0x68, 0x8e, 0x22, 0x7f, 0x55, 0x65, 0x2d,
        0xe1, 0x2b, 0x42, 0xd7, 0x84, 0xde, 0xab, 0xd6, 0xe1, 0x16, 0x8c, 0x84, 0x8f, 0x5b, 0x08, 0x8b, 0xcb, 0xf1, 0xd9, 0xf1, 0xb0,
        0xfc,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x1c, 0xb7, 0x15, 0x4e, 0x4c, 0x7b, 0x48, 0x42, 0x70, 0x80, 0x7e, 0xe8, 0x2d, 0x27, 0x84, 0x36, 0xeb, 0x39, 0x57, 0xf5, 0x41,
        0xa2, 0x1e, 0xad, 0xf9, 0x49, 0x7d, 0x86, 0x78, 0x06, 0xbb, 0x0a, 0xf9, 0xdc, 0x9a, 0x02, 0x0a, 0x32, 0xc3, 0x96, 0xa8, 0x13,
        0x0c, 0x32, 0x59, 0x5e, 0xcd, 0xdf, 0x87, 0x77, 0xe0, 0x9c, 0xe2, 0xe2, 0x8a, 0x7a, 0xae, 0x12, 0x92, 0x34, 0xc8, 0xf9, 0x94,
        0x56,
    ]),
    // kaspa-pq (audit H-01): genesis commits to the 10B premine (a single main UTXO)
    // = MuHash over `misaka_premine_utxos(Mainnet)`. Mainnet's 10B main wallet is the
    // operator custody address (ceremony complete), so this commitment differs from the
    // test networks (whose 10B main wallet is the operator's value-less test address).
    utxo_commitment: Hash64::from_bytes([
        0x2c, 0xd0, 0x9b, 0xe6, 0xa0, 0x68, 0x3a, 0x12, 0x86, 0x8d, 0xb5, 0x22, 0x54, 0x21, 0xd1, 0xaf, 0xf9, 0xae, 0x9a, 0x85, 0x6b,
        0xb8, 0xc6, 0x15, 0x4b, 0x7a, 0xd9, 0x94, 0x91, 0x3d, 0xfc, 0x4d, 0x18, 0x50, 0x75, 0xf1, 0xdb, 0x51, 0xb0, 0x00, 0xcc, 0x61,
        0x69, 0x8e, 0xb3, 0xfd, 0x52, 0xbc, 0x13, 0x3c, 0x57, 0x3b, 0x95, 0x8a, 0xbb, 0x65, 0x52, 0x12, 0x75, 0xdd, 0xa4, 0x89, 0x6c,
        0xe3,
    ]),
    // 2025-05-28 00:00:00 UTC (= 1748390400000 ms) — kaspa-pq genesis reference timestamp (audit
    // M-06: comment now matches the value; the real mainnet launch timestamp is set at the
    // premine-ceremony re-genesis — see config/premine.rs MAINNET_PREMINE_CEREMONY_PENDING).
    timestamp: 1748390400000,
    // kaspa-pq Phase 3 (ADR-0007): PoW migrated to the compute-only BLAKE2b-512 ∥
    // SHA3-512 Layer-1 (`pow_blake2b_sha3_activation = always`, `algo_id = 3`),
    // superseding Phase-2 Argon2id for ~10^4× cheaper header verification. This is a
    // FAST hash (~10^4× higher hash-rate than Argon2id), so the inherited `0x1f7fffff`
    // genesis difficulty is intentionally EASY relative to real launch hash-rate: under
    // UN-throttled mining the DAA ramps live difficulty up to the hash-rate equilibrium
    // (D ≈ aggregate-H/s ÷ BPS) within the first ~MIN_DIFFICULTY_WINDOW, and it floors at
    // `max_difficulty_target`. Erring easy is safe (self-correcting, never stalls). The
    // launch op-team SHOULD pre-set this near equilibrium at the premine ceremony — measure
    // aggregate H/s with `pq-miner --bench-secs` on the launch hardware — to skip the
    // initial instamine ramp. Changing `bits` re-genesises `hash` below (recompute via
    // `gen_kaspa_pq_genesis_hashes`).
    bits: 0x1f7fffff,
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "misaka-mainnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x6d, 0x61, 0x69, 0x6e, 0x6e, 0x65, 0x74,
        // kaspa-pq Phase-3 re-genesis marker "-bs3" (BLAKE2b-512 ∥ SHA3-512, ADR-0007 Phase 3): bumps
        // the genesis hash so this chain is cryptographically distinct from the superseded Argon2id
        // chain — an un-wiped node hits the startup genesis-mismatch guard instead of silently resuming.
        0x2d, 0x62, 0x73, 0x33,
    ],
};

pub const TESTNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xeb, 0xe5, 0xfa, 0x0a, 0x24, 0x9d, 0x3b, 0xf8, 0x4d, 0x21, 0xea, 0x6e, 0x45, 0xd8, 0x73, 0x62, 0xb6, 0x85, 0xb4, 0x9c, 0x4d,
        0x64, 0x4b, 0x8b, 0x73, 0xd8, 0x90, 0xf4, 0x1c, 0x8b, 0x4f, 0x50, 0xbf, 0xec, 0xdd, 0xc4, 0xe7, 0xa5, 0x34, 0x09, 0xaa, 0x5b,
        0xf2, 0xe3, 0x95, 0x66, 0x9a, 0xb8, 0x9f, 0x96, 0x1e, 0x45, 0x2e, 0x48, 0x8c, 0xec, 0xd1, 0x85, 0xc1, 0xe9, 0xb7, 0x92, 0x89,
        0x6b,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0xe1, 0x64, 0x8e, 0xe2, 0xbd, 0x65, 0x84, 0x8e, 0xff, 0x8b, 0x97, 0xa3, 0xcb, 0x7c, 0xcd, 0xcf, 0x68, 0x7d, 0xbc, 0x53, 0x92,
        0xa0, 0x7a, 0xed, 0x88, 0x1f, 0xd2, 0xef, 0x67, 0x0c, 0x93, 0xee, 0x44, 0x62, 0x46, 0x99, 0x56, 0xc3, 0xb3, 0x80, 0x9d, 0xa7,
        0x4d, 0xdb, 0xad, 0x0c, 0x6b, 0xbb, 0x3b, 0x17, 0x1e, 0x66, 0xbe, 0x0b, 0xa2, 0xe6, 0x95, 0x0f, 0xe2, 0x5b, 0x8e, 0xe2, 0x7b,
        0x84,
    ]),
    // kaspa-pq: genesis commits to the 10B premine (a single main UTXO) = MuHash over
    // `config::premine::misaka_premine_utxos()`. Test nets share one commitment (same
    // value-less operator test address); mainnet differs (operator custody address).
    utxo_commitment: Hash64::from_bytes([
        0xf6, 0x88, 0xf6, 0xb5, 0x28, 0x05, 0x4f, 0xc8, 0xef, 0x0c, 0x79, 0x31, 0xf6, 0xbc, 0x1c, 0x5b, 0x3f, 0x42, 0xe5, 0x40, 0xa1,
        0x71, 0x01, 0x4e, 0x10, 0xa5, 0x70, 0xec, 0x0c, 0xf2, 0x7e, 0x28, 0xeb, 0x39, 0x4f, 0xab, 0x92, 0xa6, 0xe7, 0x38, 0x85, 0xb7,
        0xa9, 0x29, 0x00, 0xfe, 0x92, 0x0c, 0xc3, 0xd1, 0xb7, 0xd2, 0x65, 0x11, 0xa9, 0xd6, 0x53, 0x38, 0x93, 0x76, 0xa0, 0x9f, 0x45,
        0x24,
    ]),
    timestamp: 1748390400000,
    // kaspa-pq Phase 3 (ADR-0007): genesis difficulty CALIBRATED to the measured BLAKE2b-512 ∥
    // SHA3-512 hash-rate (`pow_blake2b_sha3_activation = always`, `algo_id = 3`) so the chain
    // runs ~10 BPS with NO miner throttle from block 1. Measured 2026-06-12 on the quiesced
    // 4-host launch mesh via `pq-miner --bench-secs` (true grind cost: L1 tag + Layer-0
    // finalizer): .186 2.68M + .213 2.38M + .51 3.59M + .119 1.24M ≈ 9.89M H/s aggregate; the
    // 10-BPS DAA equilibrium is H/20 ≈ 495k, set to D ≈ 400k (≈80%, leaving CPU headroom for
    // the co-located node/validator/explorer — erring easy is self-correcting: the DAA ramps
    // up, floors at `max_difficulty_target`, no upper cap). The Argon2id-era `0x20018618`
    // (D ≈ 84, calibrated at 1681 H/s) is superseded. Mine UN-throttled (a throttle pins the
    // observed rate and starves the DAA). Changing miner topology? Re-bench and re-set near
    // H/20 (the first ~MIN_DIFFICULTY_WINDOW is fixed-difficulty).
    bits: 0x1e14f8b5,
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "misaka-testnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74,
        // kaspa-pq Phase-3 re-genesis marker "-bs3" (BLAKE2b-512 ∥ SHA3-512, ADR-0007 Phase 3): bumps
        // the genesis hash so this chain is cryptographically distinct from the superseded Argon2id
        // testnet — an un-wiped node hits the startup genesis-mismatch guard instead of silently resuming.
        0x2d, 0x62, 0x73, 0x33,
    ],
};

pub const TESTNET11_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0x5f, 0x0a, 0xee, 0xea, 0x65, 0x6e, 0x75, 0x54, 0xad, 0xfd, 0x28, 0xfa, 0x68, 0xb3, 0x98, 0xc1, 0x0d, 0x4c, 0x84, 0x21, 0x9d,
        0x7f, 0x78, 0xcf, 0xb3, 0x03, 0xee, 0xc8, 0x7f, 0x7c, 0xc5, 0x00, 0xba, 0x45, 0x1f, 0xda, 0xf2, 0x86, 0x17, 0xdb, 0x86, 0xcf,
        0xcb, 0x36, 0xb0, 0x29, 0xf4, 0xa3, 0x0a, 0x60, 0x9d, 0x51, 0x98, 0xc5, 0xdc, 0xea, 0xfb, 0xdb, 0x03, 0x0e, 0x0a, 0xf3, 0xbb,
        0x4f,
    ]),
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x9f, 0xe2, 0xd9, 0xfe, 0xde, 0x32, 0x41, 0xd9, 0x7a, 0x11, 0x63, 0xf1, 0x17, 0x39, 0xae, 0x4c, 0x6e, 0x52, 0x22, 0x06, 0x37,
        0x10, 0xf2, 0xa4, 0x54, 0x20, 0xa4, 0xe2, 0x05, 0xad, 0xc4, 0x31, 0xbd, 0x0f, 0x65, 0x00, 0xe7, 0xd8, 0x47, 0xb9, 0x2e, 0x25,
        0xc1, 0x63, 0xc5, 0x89, 0x19, 0x5a, 0x11, 0xab, 0x78, 0x57, 0xfc, 0x3a, 0x41, 0xf3, 0xf4, 0x54, 0xc3, 0xff, 0xdc, 0x68, 0xda,
        0x2e,
    ]),
    bits: 0x1e0218de, // see `gen_testnet11_genesis` (= testnet target ×10 harder; rescaled after the D≈400k BLAKE2b-SHA3 calibration)
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "misaka-testnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74,
        11, 1,                                                                                                // TN11, kaspa-pq Relaunch 1
    ],
    ..TESTNET_GENESIS
};

/// kaspa-pq ADR-0039 PALW: the dedicated 10-BPS audited-compute testnet (`testnet-palw-10`). A distinct
/// coinbase payload tag gives it its own genesis hash so its measurements stay separate from testnet-10
/// / testnet-40. PALW starts inert (`palw_activation_daa_score = u64::MAX`) — the network runs the
/// permanent algo-3 hash floor at 10 BPS until a weight-0 activation. hash / hash_merkle_root are
/// recomputed by `gen_kaspa_pq_genesis_hashes` (placeholders below).
pub const TESTNET_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xe5, 0x59, 0x3d, 0x63, 0x53, 0x30, 0x3c, 0xc0, 0xf7, 0xba, 0x41, 0x50, 0xd2, 0xcb, 0x70, 0xbf, 0x1e, 0x53, 0x5c, 0xb2, 0x06,
        0xac, 0x1d, 0xc0, 0x7f, 0x6f, 0x85, 0xf4, 0x99, 0xd5, 0x81, 0x7e, 0x9d, 0x3b, 0x92, 0xa9, 0xae, 0xdb, 0x83, 0x12, 0x77, 0xc4,
        0x06, 0xa7, 0x04, 0x17, 0x61, 0x0d, 0xc9, 0x8c, 0x26, 0xce, 0xbd, 0x5e, 0x0a, 0xd1, 0x7b, 0x13, 0xc9, 0x0f, 0x91, 0x7a, 0x22,
        0xdc,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0x1a, 0xc8, 0x6a, 0x01, 0x62, 0xd2, 0xec, 0x78, 0x46, 0xd0, 0x62, 0xb5, 0xdd, 0x96, 0x57, 0xd0, 0xd3, 0xa0, 0xb7, 0xfe, 0x24,
        0x81, 0x63, 0xe2, 0x6d, 0xbe, 0x42, 0x81, 0xce, 0xe9, 0x0b, 0xb1, 0x42, 0xea, 0x6f, 0x47, 0x1b, 0xc8, 0x76, 0x8a, 0xff, 0x2a,
        0x46, 0x7b, 0x1b, 0x21, 0xff, 0x89, 0x69, 0x4d, 0xc5, 0x2c, 0xe9, 0xce, 0x75, 0xd4, 0xf0, 0x6c, 0xee, 0xe0, 0xc6, 0x54, 0x3f,
        0x99,
    ]),
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-palw-10"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x61, 0x6c, 0x77, 0x2d, 0x31, 0x30,
    ],
    // ADR-0039 activation re-genesis: max-easy fast-start target (== TESTNET_PALW_LANE_DIFFICULTY
    // genesis_hash_bits) so single-node algo-3 mining is fast; algo-4 is exempt from the hash floor.
    bits: 0x207fffff,
    ..TESTNET_GENESIS
};

/// kaspa-pq ADR-0048 — genesis for the Header-v4 **staging-mainnet** PALW rehearsal network
/// (`staging-mainnet-palw`, NetworkId `testnet-200`). The FIRST shipped v4 genesis:
/// `version = PALW_ANTISPAM_HEADER_VERSION`, so the canonical `From<&GenesisBlock> for Header`
/// conversion derives and binds the canonical EMPTY `palw_spam_accumulator_commitment` into the
/// genesis identity before finalizing (the one-way re-genesis boundary ADR-0041 defines and
/// staging rehearses; see `header_v4_regenesis_commits_the_canonical_empty_spam_accumulator`).
/// Distinct coinbase payload tag ("misaka-staging-palw") + fresh timestamp give it its own ledger,
/// cryptographically separate from every legacy identity. Real algo-3 PoW from block 1 at the
/// max-easy fast-start target (same rationale as TESTNET_PALW_GENESIS — the DAA ramps difficulty
/// to the live hash-rate; algo-4 headers are hash-floor-exempt structurally). hash /
/// hash_merkle_root minted by `gen_kaspa_pq_genesis_hashes`.
pub const STAGING_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xdd, 0xcc, 0x54, 0xb6, 0x57, 0xa1, 0x94, 0xb8, 0x97, 0x9d, 0x5f, 0x5d, 0x81, 0xee, 0xa0, 0xf7, 0x2d, 0x54, 0x1e, 0xbe, 0x52,
        0xd8, 0xd0, 0x40, 0xd4, 0xd8, 0xcf, 0x58, 0x3a, 0x4a, 0x38, 0x2b, 0x07, 0xef, 0xf8, 0xd0, 0xbf, 0x7d, 0x2e, 0x0a, 0x31, 0xc0,
        0x66, 0x7e, 0xff, 0x7e, 0xd2, 0x21, 0xf0, 0x12, 0x7b, 0x99, 0x50, 0xa5, 0x43, 0x63, 0x54, 0x46, 0x34, 0x0b, 0x32, 0x06, 0xed,
        0xb0,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0x6c, 0xdc, 0x75, 0x26, 0x13, 0x20, 0x1a, 0xd9, 0xaa, 0xf9, 0x81, 0x08, 0x34, 0x0b, 0x8d, 0x91, 0x23, 0x54, 0xf8, 0xec, 0xd7,
        0x61, 0x3c, 0x5e, 0x3d, 0xfd, 0xb3, 0x65, 0x7c, 0xfc, 0x33, 0x86, 0x3e, 0x15, 0xa4, 0x19, 0x7e, 0x58, 0x92, 0xfe, 0x60, 0x2b,
        0xa5, 0x48, 0x4f, 0x4f, 0xe6, 0x2a, 0x3d, 0xc0, 0x89, 0xf8, 0xf9, 0xa9, 0xe1, 0xd0, 0x65, 0xae, 0xa1, 0x8d, 0x2c, 0x52, 0x0f,
        0x08,
    ]),
    // ADR-0048 / ADR-0041: Header-v4 re-genesis — the version at which the conversion above binds
    // the anti-spam accumulator commitment into the genesis hash.
    version: crate::constants::PALW_ANTISPAM_HEADER_VERSION,
    // 2026-07-26 16:00:00 UTC (= 1785081600000 ms) — fresh staging re-genesis reference timestamp.
    //
    // Must be in the PAST at launch. The original value (2026-07-27 16:00:00 UTC) was authored
    // hours ahead of wall-clock, and a genesis in the future makes the network unminable until it
    // passes: block 1 inherits `past_median_time + 1` from genesis, so every template is rejected
    // with "the block timestamp is too far into the future" and the chain cannot leave block 0.
    // Re-minted here while the network still had zero blocks, so no ledger was invalidated.
    timestamp: 1785081600000,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-staging-palw"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x73, 0x74, 0x61, 0x67, 0x69, 0x6e, 0x67, 0x2d, 0x70, 0x61, 0x6c, 0x77,
    ],
    // ADR-0048: max-easy real-PoW fast-start target (== STAGING_PALW_LANE_DIFFICULTY genesis bits,
    // §16.3 `is_consistent_for_activation`); erring easy is self-correcting under the DAA.
    bits: 0x207fffff,
    ..TESTNET_GENESIS
};

/// ADR-MA P14 — genesis for the Header-v5 **Compute Set registry** rehearsal network
/// (`compute-registry-palw`, NetworkId `testnet-20`). The FIRST shipped v5 genesis:
/// `version = PALW_COMPUTE_SET_HEADER_VERSION`, so the header carries the three Compute Set
/// reference fields in its hash preimage from block 0 — all-zero at genesis (genesis references
/// no set; a real `compute_set_id` is content-derived and never zero), exactly the
/// `check_header_version` zero-guard shape for a non-PALW-lane v5 header. Inherits the staging
/// v4 spam-accumulator binding (version 5 ≥ 4 takes the same `From<&GenesisBlock>` branch).
/// Distinct coinbase payload tag ("misaka-compute-registry") + fresh timestamp give it its own
/// ledger. hash / hash_merkle_root minted by `gen_kaspa_pq_genesis_hashes`.
pub const COMPUTE_REGISTRY_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xec, 0x8b, 0x43, 0x28, 0x79, 0x56, 0xfc, 0x0d, 0x2e, 0x9a, 0xdb, 0x77, 0xcc, 0x83, 0x16, 0x51, 0xee, 0xac, 0x26, 0x58, 0x3c,
        0x08, 0xe3, 0x37, 0x2c, 0xb0, 0xc7, 0x0b, 0x3f, 0x13, 0x89, 0xbc, 0x02, 0x1a, 0x21, 0x72, 0x3a, 0xd0, 0x32, 0xba, 0xd0, 0xf3,
        0x11, 0x51, 0xd8, 0x10, 0x67, 0xde, 0x9f, 0x38, 0xed, 0x49, 0x8c, 0x7c, 0x1b, 0x3f, 0xaa, 0x44, 0x5c, 0x23, 0x12, 0x79, 0xd1,
        0xcc,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0x22, 0xde, 0x0f, 0x5e, 0x63, 0x62, 0x59, 0xea, 0xba, 0x6c, 0x39, 0x2b, 0x45, 0x63, 0x65, 0x82, 0x84, 0x47, 0x37, 0x66, 0xb2,
        0x5b, 0x3b, 0xf6, 0xf4, 0x76, 0xfe, 0x97, 0x30, 0x6b, 0xcc, 0x1e, 0xa5, 0xce, 0x33, 0xad, 0x4f, 0x6c, 0xc3, 0x2c, 0x01, 0x86,
        0xc0, 0x4a, 0x62, 0xc0, 0x6b, 0x76, 0xd5, 0x65, 0x8e, 0x66, 0xc6, 0xfb, 0x4f, 0xbb, 0x12, 0x77, 0x9b, 0x9b, 0xf5, 0xb4, 0xb6,
        0x79,
    ]),
    // ADR-MA: the Header-v5 re-genesis — Compute Set references live in the hash preimage from
    // block 0, so adding a model NEVER changes the header schema again (§13 «将来のモデル追加時に
    // header変更を発生させない»).
    version: crate::constants::PALW_COMPUTE_SET_HEADER_VERSION,
    // 2026-07-30 00:00:00 UTC (= 1785369600000 ms) — must be in the PAST at launch (the staging
    // future-timestamp lesson: a future genesis makes the network unminable until it passes).
    timestamp: 1785369600000,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-compute-registry"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x63, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x65, 0x2d, 0x72, 0x65, 0x67, 0x69, 0x73, 0x74, 0x72, 0x79,
    ],
    // Max-easy real-PoW fast-start target (the staging rationale — the DAA ramps to live rate).
    bits: 0x207fffff,
    ..TESTNET_GENESIS
};

/// ADR-0045 D3-b — genesis for the **PCPB dispatch** rehearsal network (`pcpb-palw`, NetworkId
/// `testnet-21`). The re-genesis the D3-b train requires: LeafV2 moved the leaf bincode layout
/// (LEAF_LEN 964 → 1189), leaf-chunk wire v3 refuses v2, and `leaf_hash → leaf_root →
/// content_id() == batch_id` all moved with it — testnet-20's mined history (v2 chunks, V1-era
/// leaves) is structurally unreplayable under the new rules, so the tripwire's option (b) applies:
/// a NEW suffix, never an in-place edit. Header schema is UNCHANGED by D3-b (the leaf is payload,
/// not header), so this stays a v5 genesis in the compute-registry shape: distinct coinbase tag
/// ("misaka-pcpb-palw") + fresh timestamp give it its own ledger. hash / hash_merkle_root minted
/// by `gen_kaspa_pq_genesis_hashes`.
pub const PCPB_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xbc, 0x06, 0x8f, 0xa4, 0x04, 0x36, 0xf4, 0x1d, 0x33, 0x53, 0xae, 0xb8, 0xaf, 0x6d, 0x97, 0x35, 0x08, 0x50, 0xc8, 0xfd, 0x11,
        0x3e, 0x16, 0xe7, 0x6c, 0x0c, 0xda, 0x61, 0x04, 0xa9, 0x22, 0xd5, 0xa8, 0x60, 0x08, 0x9e, 0xe7, 0xe5, 0x4b, 0xd9, 0xd9, 0xdc,
        0x86, 0xf7, 0xf3, 0x2c, 0xf0, 0x36, 0x9b, 0x72, 0xe9, 0xa4, 0x60, 0xe8, 0x3d, 0x71, 0xec, 0xe7, 0x13, 0x12, 0xe6, 0xfe, 0x63,
        0xdc,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0x50, 0x3a, 0x90, 0xcd, 0x93, 0x6c, 0x9b, 0xcf, 0xdb, 0x28, 0xa9, 0x87, 0x97, 0xeb, 0x86, 0xb3, 0xd6, 0x1c, 0x83, 0xde, 0xb7,
        0xe7, 0xd8, 0x08, 0xa2, 0xea, 0x8e, 0x3d, 0xa4, 0xdc, 0x5d, 0x31, 0x5a, 0xf2, 0x44, 0xfc, 0x2f, 0x2a, 0x7c, 0xf4, 0x65, 0x9a,
        0xd3, 0x55, 0x4f, 0xa2, 0xbd, 0x2f, 0x50, 0xe3, 0xc4, 0xe6, 0x18, 0x3a, 0xcb, 0xd8, 0x62, 0x64, 0x5f, 0xba, 0x7c, 0x58, 0x5b,
        0xb4,
    ]),
    // Header v5 (Compute Set references in the preimage from block 0) — inherited from the
    // compute-registry shape; D3-b touches the LEAF, never the header schema.
    version: crate::constants::PALW_COMPUTE_SET_HEADER_VERSION,
    // 2026-07-31 00:00:00 UTC (= 1785456000000 ms) — must be in the PAST at launch (the staging
    // future-timestamp lesson: a future genesis makes the network unminable until it passes).
    timestamp: 1785456000000,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-pcpb-palw"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x63, 0x70, 0x62, 0x2d, 0x70, 0x61, 0x6c, 0x77,
    ],
    // Max-easy real-PoW fast-start target (the staging rationale — the DAA ramps to live rate).
    bits: 0x207fffff,
    ..TESTNET_GENESIS
};

/// **testnet-22 (`evm-genesis-palw`) — the 2026-08-02 re-genesis.**
///
/// Same Header-v5 compute-registry shape as testnet-21, re-cut for two reasons that could not be
/// applied to a live ledger:
///
///   * **EVM is genesis-active here.** testnet-21 activates ADR-0020's lane mid-chain at DAA
///     6,500,000, which is the harder configuration and was the right call for a ledger that already
///     had history. A fresh net does not need the fence: `evm_activation_daa_score = 0` means every
///     block from block 0 carries the two EVM header commitments and the lane is exercised from the
///     first transaction, which is what a testnet is for.
///   * **Every 2026-08-02 consensus fix is genesis-active**, so nothing on this net is "the old rule
///     until DAA N". testnet-21 had to fence the bug report #6 and static-audit fixes because it had
///     mined history produced under the pre-fix binaries; testnet-22 has none.
///
/// Distinct coinbase payload ("misaka-t22-evm-palw") ⇒ distinct genesis hash from every other net, so
/// a stale datadir cannot be mistaken for this one. hash/hash_merkle_root computed by
/// `gen_kaspa_pq_genesis_hashes`.
pub const EVM_GENESIS_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0x60, 0x5f, 0x9d, 0xe9, 0xc7, 0xc8, 0x32, 0x6d, 0xf0, 0x34, 0x37, 0x20, 0x1f, 0xed, 0xdb, 0x6f, 0x8e, 0x1b, 0xed, 0x31, 0x42,
        0xe7, 0x90, 0x78, 0xe5, 0x06, 0xb2, 0x07, 0xa0, 0x51, 0xd2, 0x7b, 0x66, 0x4a, 0xfd, 0x30, 0x4e, 0x7c, 0x2f, 0x37, 0xe6, 0x5a,
        0x7d, 0x5a, 0xbd, 0x4f, 0x90, 0xf5, 0x3f, 0x5f, 0xe8, 0xcd, 0xef, 0x99, 0x9a, 0xc5, 0xb1, 0xd3, 0xca, 0xf0, 0x39, 0x9e, 0x62,
        0xdb,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0xfe, 0xaf, 0x8b, 0xcd, 0xc4, 0x9c, 0x5b, 0x1b, 0x07, 0x8d, 0x9c, 0x30, 0xb9, 0x65, 0x86, 0x32, 0x70, 0x22, 0xf8, 0x51, 0x4d,
        0x27, 0xbf, 0xf3, 0x99, 0x9d, 0xdc, 0x38, 0x10, 0x66, 0xa2, 0x86, 0x7b, 0xb1, 0x9e, 0xd0, 0x8d, 0xb7, 0x60, 0xb0, 0xcc, 0x05,
        0x10, 0xcd, 0xca, 0x30, 0xcc, 0x91, 0x6c, 0x66, 0x6d, 0x6f, 0xad, 0xed, 0x9d, 0xd0, 0x04, 0xb1, 0xce, 0x56, 0xe7, 0xf2, 0xe9,
        0x45,
    ]),
    version: crate::constants::PALW_COMPUTE_SET_HEADER_VERSION,
    // 2026-08-02 00:00:00 UTC. Must be in the PAST at launch — a future genesis makes the network
    // unminable until it passes (the staging lesson, learned the expensive way).
    timestamp: 1785628800000,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-t22-evm-palw"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x74, 0x32, 0x32, 0x2d, 0x65, 0x76, 0x6d, 0x2d, 0x70, 0x61, 0x6c, 0x77,
    ],
    // Max-easy real-PoW fast start; the DAA ramps to the live rate.
    bits: 0x207fffff,
    ..TESTNET_GENESIS
};

/// ADR-0039 P0 — genesis for the single-node PALW-active devnet (`devnet-palw`, `--devnet --netsuffix=111`).
/// Carries `bits == DEVNET_PALW_GENESIS_BITS` (0x207fffff, max-easy) so §16.3's
/// `is_consistent_for_activation` holds and Layer-0 PoW grinds instantly. Distinct coinbase payload
/// ("misaka-devnet-palw") → distinct genesis hash from DEVNET_GENESIS. hash/hash_merkle_root recomputed by
/// `gen_kaspa_pq_genesis_hashes`.
pub const DEVNET_PALW_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xf7, 0x91, 0xa0, 0x64, 0x54, 0x13, 0x30, 0x6d, 0x21, 0xbf, 0x1f, 0x36, 0x85, 0x83, 0x33, 0x2a, 0x6b, 0xc4, 0xbf, 0x2e, 0x71,
        0xcc, 0x60, 0xeb, 0xf4, 0x0f, 0x3e, 0xf6, 0x1b, 0x24, 0x4d, 0x89, 0xa7, 0x38, 0x34, 0x3b, 0x48, 0x04, 0x90, 0xe0, 0xbe, 0xa5,
        0xdc, 0x8c, 0x22, 0x5b, 0xd6, 0xe0, 0x84, 0x83, 0x59, 0xb8, 0xdc, 0x00, 0x3e, 0xea, 0x1e, 0x01, 0xac, 0x48, 0x27, 0x1e, 0x6d,
        0xd0,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0x0c, 0x16, 0xdc, 0x11, 0x92, 0x7a, 0x95, 0xa8, 0x67, 0xf4, 0x1f, 0x53, 0x8a, 0x4c, 0x23, 0x42, 0xc6, 0xb2, 0x3a, 0x6a, 0xa8,
        0x4b, 0x87, 0x80, 0x5f, 0x65, 0xec, 0x10, 0x6a, 0x85, 0x87, 0xed, 0xfe, 0xdb, 0x4b, 0xe6, 0x23, 0xa1, 0x0a, 0xe1, 0x01, 0x96,
        0x01, 0xe7, 0x45, 0x77, 0x9e, 0xf7, 0xbe, 0x71, 0xd5, 0x07, 0xa8, 0x52, 0xb0, 0xb4, 0x31, 0x94, 0x88, 0x00, 0x55, 0x5e, 0x5c,
        0x78,
    ]),
    bits: 0x207fffff,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-devnet-palw"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x64, 0x65, 0x76, 0x6e, 0x65, 0x74, 0x2d, 0x70, 0x61, 0x6c, 0x77,
    ],
    ..DEVNET_GENESIS
};

pub const SIMNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xae, 0x58, 0xe3, 0x0b, 0xcd, 0x2b, 0xc2, 0xf8, 0x89, 0x48, 0x31, 0xb8, 0x6b, 0x19, 0xcf, 0x52, 0xaa, 0x5d, 0x28, 0xb5, 0x33,
        0xf1, 0x43, 0x46, 0x5f, 0xa9, 0xcc, 0xa9, 0x39, 0x1c, 0x6c, 0x1c, 0xbc, 0x1c, 0x1f, 0x17, 0x09, 0x74, 0x9b, 0x6a, 0x4a, 0x58,
        0x91, 0x83, 0x4b, 0xa1, 0x45, 0xa6, 0x42, 0x79, 0x0e, 0x9d, 0x38, 0x3c, 0x9f, 0xe7, 0xa8, 0x74, 0x24, 0x21, 0x1d, 0x68, 0xfb,
        0xa4,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x94, 0x93, 0x6b, 0x83, 0x97, 0xe7, 0x1b, 0xf0, 0x26, 0xa0, 0x43, 0x70, 0xcc, 0x71, 0x7c, 0xf9, 0xe8, 0xf5, 0x56, 0x0f, 0x7c,
        0xf9, 0x57, 0x9d, 0xf6, 0xc5, 0x2d, 0x2c, 0x90, 0x15, 0x7a, 0x18, 0xd7, 0x2a, 0xf6, 0x58, 0x47, 0xd0, 0xaf, 0xc3, 0x65, 0x0a,
        0xe4, 0xca, 0x64, 0x28, 0x11, 0xcd, 0x62, 0x0b, 0x3e, 0x87, 0xdb, 0x14, 0x51, 0x30, 0x4b, 0x0f, 0x98, 0x97, 0x5f, 0x1a, 0xcf,
        0xc2,
    ]),
    // kaspa-pq: genesis commits to the 10B premine (a single main UTXO) = MuHash over
    // `config::premine::misaka_premine_utxos()`. Test nets share one commitment (same
    // value-less operator test address); mainnet differs (operator custody address).
    utxo_commitment: Hash64::from_bytes([
        0xf6, 0x88, 0xf6, 0xb5, 0x28, 0x05, 0x4f, 0xc8, 0xef, 0x0c, 0x79, 0x31, 0xf6, 0xbc, 0x1c, 0x5b, 0x3f, 0x42, 0xe5, 0x40, 0xa1,
        0x71, 0x01, 0x4e, 0x10, 0xa5, 0x70, 0xec, 0x0c, 0xf2, 0x7e, 0x28, 0xeb, 0x39, 0x4f, 0xab, 0x92, 0xa6, 0xe7, 0x38, 0x85, 0xb7,
        0xa9, 0x29, 0x00, 0xfe, 0x92, 0x0c, 0xc3, 0xd1, 0xb7, 0xd2, 0x65, 0x11, 0xa9, 0xd6, 0x53, 0x38, 0x93, 0x76, 0xa0, 0x9f, 0x45,
        0x24,
    ]),
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
        // "misaka-simnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x73, 0x69, 0x6d, 0x6e, 0x65, 0x74,
    ],
};

pub const DEVNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0xfb, 0xaa, 0x9f, 0xc8, 0x97, 0x36, 0x4b, 0x41, 0xa9, 0xb5, 0x34, 0xe3, 0x65, 0xa7, 0x3d, 0xc2, 0x69, 0xc0, 0xf4, 0x4f, 0x4f,
        0xf4, 0xc1, 0x0a, 0x87, 0x48, 0xed, 0x97, 0x47, 0xc9, 0x86, 0x81, 0x01, 0xe3, 0x39, 0x99, 0x27, 0xd2, 0x07, 0x9c, 0xc7, 0xc9,
        0xdd, 0xe3, 0x10, 0xc5, 0xd9, 0xec, 0x8f, 0xbf, 0x93, 0xbc, 0x75, 0x8b, 0x7a, 0x08, 0xa0, 0x92, 0xcb, 0xf8, 0x71, 0x49, 0x43,
        0xa1,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0xd1, 0x96, 0x61, 0x33, 0xe5, 0x47, 0xbb, 0xcb, 0xba, 0x99, 0xe6, 0x39, 0x7d, 0x39, 0xde, 0x71, 0xea, 0xa9, 0x6f, 0xd9, 0x50,
        0x3a, 0x17, 0x67, 0xc3, 0x60, 0x1c, 0x4b, 0x63, 0x4e, 0x68, 0xe2, 0x19, 0x12, 0xf8, 0xff, 0x19, 0x63, 0x37, 0x99, 0x17, 0x68,
        0xc1, 0x70, 0xda, 0x86, 0x3a, 0xdb, 0x94, 0x86, 0xfc, 0x20, 0x48, 0xc0, 0xf0, 0x4b, 0xcf, 0xc6, 0x3f, 0xef, 0x15, 0x80, 0x31,
        0x3e,
    ]),
    // kaspa-pq: genesis commits to the 10B premine (a single main UTXO) = MuHash over
    // `config::premine::misaka_premine_utxos()`. Test nets share one commitment (same
    // value-less operator test address); mainnet differs (operator custody address).
    utxo_commitment: Hash64::from_bytes([
        0xf6, 0x88, 0xf6, 0xb5, 0x28, 0x05, 0x4f, 0xc8, 0xef, 0x0c, 0x79, 0x31, 0xf6, 0xbc, 0x1c, 0x5b, 0x3f, 0x42, 0xe5, 0x40, 0xa1,
        0x71, 0x01, 0x4e, 0x10, 0xa5, 0x70, 0xec, 0x0c, 0xf2, 0x7e, 0x28, 0xeb, 0x39, 0x4f, 0xab, 0x92, 0xa6, 0xe7, 0x38, 0x85, 0xb7,
        0xa9, 0x29, 0x00, 0xfe, 0x92, 0x0c, 0xc3, 0xd1, 0xb7, 0xd2, 0x65, 0x11, 0xa9, 0xd6, 0x53, 0x38, 0x93, 0x76, 0xa0, 0x9f, 0x45,
        0x24,
    ]),
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
        // "misaka-devnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x64, 0x65, 0x76, 0x6e, 0x65, 0x74,
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::bps::TenBps, merkle::calc_hash_merkle_root};

    // PR-9.5g: re-enabled after recomputing the 5 genesis constants
    // (hash + hash_merkle_root) as 64-byte Hash64 via
    // `gen_kaspa_pq_genesis_hashes` below. Asserts each genesis block's
    // recomputed merkle root and block hash match the committed constants.
    #[test]
    fn test_genesis_hashes() {
        [
            GENESIS,
            TESTNET_GENESIS,
            TESTNET11_GENESIS,
            TESTNET_PALW_GENESIS,
            STAGING_PALW_GENESIS,
            COMPUTE_REGISTRY_PALW_GENESIS,
            PCPB_PALW_GENESIS,
            EVM_GENESIS_PALW_GENESIS,
            SIMNET_GENESIS,
            DEVNET_GENESIS,
        ]
        .into_iter()
        .for_each(|genesis| {
            let block: Block = (&genesis).into();
            assert_hashes_eq(calc_hash_merkle_root(block.transactions.iter()), block.header.hash_merkle_root);
            assert_hashes_eq(block.hash(), genesis.hash);
        });
    }

    #[test]
    fn header_v4_regenesis_commits_the_canonical_empty_spam_accumulator() {
        let mut genesis = SIMNET_GENESIS;
        genesis.version = crate::constants::PALW_ANTISPAM_HEADER_VERSION;
        genesis.hash = Hash64::default();
        let header: Header = (&genesis).into();
        assert_eq!(
            header.palw_spam_accumulator_commitment,
            crate::palw_antispam::palw_spam_accumulator_commitment(genesis.daa_score, 0, 0, 0, None, None)
        );
        assert_eq!(header.palw_spam_nonce, 0);

        let mut without_commitment = header.clone();
        without_commitment.palw_spam_accumulator_commitment = Hash64::default();
        without_commitment.finalize();
        assert_ne!(header.hash, without_commitment.hash, "the v4 genesis identity must bind its root accumulator");
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
            ("TESTNET_PALW_GENESIS", &TESTNET_PALW_GENESIS),
            ("STAGING_PALW_GENESIS", &STAGING_PALW_GENESIS),
            ("COMPUTE_REGISTRY_PALW_GENESIS", &COMPUTE_REGISTRY_PALW_GENESIS),
            ("PCPB_PALW_GENESIS", &PCPB_PALW_GENESIS),
            ("EVM_GENESIS_PALW_GENESIS", &EVM_GENESIS_PALW_GENESIS),
            ("SIMNET_GENESIS", &SIMNET_GENESIS),
            ("DEVNET_GENESIS", &DEVNET_GENESIS),
            ("DEVNET_PALW_GENESIS", &DEVNET_PALW_GENESIS),
        ] {
            // Compute the merkle root that the genesis *should* have, given
            // its coinbase payload. (`g.hash_merkle_root` is the placeholder
            // ZERO_HASH at this point.)
            let coinbase_txs = g.build_genesis_transactions();
            let merkle = calc_hash_merkle_root(coinbase_txs.iter());

            // Reconstruct through the canonical GenesisBlock conversion. This is deliberately not
            // an open-coded `Header::new_finalized`: a future Header-v4 re-genesis must also derive
            // and bind its canonical empty anti-spam accumulator before the printed hash is pinned.
            let mut candidate = g.clone();
            candidate.hash_merkle_root = merkle;
            let header: Header = (&candidate).into();

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

    // PR-9.5g: re-enabled with the `test_genesis_hashes` above; params
    // widened to the 64-byte Hash64 (block hash / merkle root).
    fn assert_hashes_eq(got: Hash64, expected: Hash64) {
        if got != expected {
            // Special hex print to ease changing the genesis hash according to the print if needed
            panic!("Got hash {:#04x?} while expecting {:#04x?}", got.as_bytes(), expected.as_bytes());
        }
    }
}
