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
// `utxo_commitment` commits to the kaspa-pq genesis premine: 40 vault UTXOs of
// 0.1B KAS each + one 9B main UTXO = 13B KAS (re-genesis 2026-06-17), each locked
// to a single-key ML-DSA-87 P2PKH (see `config::premine::misaka_premine_utxos`).
// It is the MuHash over that UTXO set. Per audit H-01 the 9B main wallet is
// network-dependent — mainnet uses the operator custody address, the test networks
// the Claude-managed test key (the 40 vault payloads are shared) — so the mainnet
// `utxo_commitment`/`hash` differ from testnet/devnet/simnet. The
// premine UTXOs themselves are imported into the UTXO store at consensus init
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
        0xb8, 0x57, 0x6a, 0xdb, 0xd8, 0x7d, 0x5f, 0xb7, 0x45, 0x48, 0xf2, 0xfe, 0x1a, 0x04, 0x48, 0x27, 0x1b, 0x83, 0xc2, 0x19, 0x46,
        0x26, 0x1a, 0xcb, 0x1c, 0x69, 0x7d, 0xb0, 0xbf, 0x50, 0xcd, 0x7b, 0x29, 0xa8, 0x89, 0x59, 0xe3, 0x8c, 0xaa, 0xfb, 0x76, 0x3f,
        0x38, 0xaf, 0x08, 0xdf, 0x30, 0x7f, 0xdf, 0x09, 0x31, 0x1e, 0x6c, 0xc4, 0x74, 0xf7, 0xe4, 0x04, 0xa4, 0x88, 0xc3, 0xf7, 0xb2,
        0x23,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x1c, 0xb7, 0x15, 0x4e, 0x4c, 0x7b, 0x48, 0x42, 0x70, 0x80, 0x7e, 0xe8, 0x2d, 0x27, 0x84, 0x36, 0xeb, 0x39, 0x57, 0xf5, 0x41,
        0xa2, 0x1e, 0xad, 0xf9, 0x49, 0x7d, 0x86, 0x78, 0x06, 0xbb, 0x0a, 0xf9, 0xdc, 0x9a, 0x02, 0x0a, 0x32, 0xc3, 0x96, 0xa8, 0x13,
        0x0c, 0x32, 0x59, 0x5e, 0xcd, 0xdf, 0x87, 0x77, 0xe0, 0x9c, 0xe2, 0xe2, 0x8a, 0x7a, 0xae, 0x12, 0x92, 0x34, 0xc8, 0xf9, 0x94,
        0x56,
    ]),
    // kaspa-pq (audit H-01): genesis commits to the 13B premine (40 vaults × 0.1B +
    // 1 main × 9B) = MuHash over `misaka_premine_utxos(Mainnet)`. Mainnet's 9B main
    // wallet is the operator custody address (ceremony complete), so this commitment
    // differs from the test networks (whose 9B main wallet is the Claude-managed test
    // key; the 40 vault payloads are shared across all nets).
    utxo_commitment: Hash64::from_bytes([
        0x98, 0x00, 0x15, 0x03, 0x21, 0xd8, 0x51, 0xb0, 0x29, 0xcd, 0x00, 0x52, 0x04, 0x61, 0x88, 0x80, 0xd3, 0x0f, 0xd1, 0x21, 0xc1,
        0xa5, 0x04, 0x88, 0xd9, 0x8a, 0xee, 0x89, 0xdf, 0x93, 0x14, 0x68, 0xd0, 0x05, 0xc2, 0x8d, 0x87, 0xf2, 0x99, 0x9f, 0x0e, 0xa0,
        0x82, 0xbe, 0xd9, 0xf8, 0xfb, 0x30, 0x3a, 0x22, 0x40, 0xc9, 0x41, 0x63, 0xed, 0x5f, 0x68, 0xaf, 0xd1, 0x2c, 0x80, 0x1e, 0xd2,
        0x69,
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
        0x7b, 0x19, 0xd8, 0x8a, 0x35, 0x0d, 0xd8, 0x3e, 0x44, 0x5b, 0x87, 0x30, 0xfb, 0x2c, 0x87, 0x42, 0x84, 0xb8, 0xb5, 0x3a, 0x9f,
        0x95, 0x5d, 0x7b, 0xf9, 0xcc, 0xae, 0x8a, 0xb9, 0xd1, 0xfe, 0xa1, 0xc5, 0x2e, 0x54, 0xc2, 0x85, 0x92, 0x27, 0x6c, 0x12, 0x16,
        0xa4, 0xc7, 0x9a, 0x56, 0x78, 0x84, 0x64, 0x5d, 0x03, 0x16, 0x1e, 0xc2, 0x14, 0xcf, 0xb5, 0x97, 0x2d, 0x7b, 0xc3, 0x2b, 0x05,
        0x74,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0xe1, 0x64, 0x8e, 0xe2, 0xbd, 0x65, 0x84, 0x8e, 0xff, 0x8b, 0x97, 0xa3, 0xcb, 0x7c, 0xcd, 0xcf, 0x68, 0x7d, 0xbc, 0x53, 0x92,
        0xa0, 0x7a, 0xed, 0x88, 0x1f, 0xd2, 0xef, 0x67, 0x0c, 0x93, 0xee, 0x44, 0x62, 0x46, 0x99, 0x56, 0xc3, 0xb3, 0x80, 0x9d, 0xa7,
        0x4d, 0xdb, 0xad, 0x0c, 0x6b, 0xbb, 0x3b, 0x17, 0x1e, 0x66, 0xbe, 0x0b, 0xa2, 0xe6, 0x95, 0x0f, 0xe2, 0x5b, 0x8e, 0xe2, 0x7b,
        0x84,
    ]),
    // kaspa-pq: genesis commits to the 13B premine (40 vaults × 0.1B + 1 main × 9B)
    // = MuHash over `config::premine::misaka_premine_utxos()`. Test nets share one
    // commitment (same Claude-managed 9B + shared vault payloads); mainnet differs.
    utxo_commitment: Hash64::from_bytes([
        0xd0, 0xe8, 0xb1, 0x14, 0x85, 0xe4, 0xfe, 0xa7, 0xad, 0xb0, 0x81, 0xd2, 0xeb, 0x83, 0xc6, 0xcc, 0xdb, 0x94, 0x80, 0x9a, 0x12,
        0xf4, 0x76, 0x97, 0x9e, 0x83, 0x29, 0xcb, 0xaa, 0xdb, 0x15, 0x96, 0xee, 0x71, 0xef, 0x05, 0x4d, 0x5e, 0x6e, 0xf5, 0x45, 0x54,
        0x10, 0x51, 0x10, 0x82, 0xe5, 0x0b, 0x5b, 0x1e, 0x3e, 0xd2, 0xa8, 0x0d, 0x27, 0x4b, 0x1f, 0xfa, 0xc2, 0x6f, 0x15, 0x71, 0xef,
        0xba,
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
        0x01, 0xbb, 0x12, 0x6f, 0x94, 0x18, 0x4f, 0x56, 0x72, 0xf1, 0x5d, 0x5b, 0x9e, 0x84, 0xf0, 0x69, 0x31, 0x4d, 0xd2, 0xdc, 0xc9,
        0xbd, 0x7e, 0x9f, 0x90, 0xdc, 0x19, 0x50, 0x44, 0x72, 0xf5, 0xf0, 0x79, 0xb6, 0x91, 0x58, 0x47, 0x13, 0x4f, 0x63, 0xb9, 0xb1,
        0x4b, 0xfd, 0xef, 0x17, 0x19, 0x6d, 0xfd, 0x23, 0x15, 0x2c, 0x42, 0x34, 0x48, 0x7f, 0xa3, 0xb4, 0x0c, 0x2d, 0xed, 0x9b, 0x2f,
        0x15,
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

pub const SIMNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0x62, 0xac, 0xff, 0xfe, 0xdc, 0xd4, 0x94, 0x6c, 0x2a, 0xfb, 0xe1, 0x6d, 0x1b, 0xe4, 0x92, 0x53, 0x97, 0x70, 0xfb, 0x2d, 0x5a,
        0x2d, 0x5f, 0xb5, 0x0b, 0x27, 0x6a, 0x6e, 0x4a, 0x52, 0xed, 0x1d, 0x15, 0x1e, 0x0e, 0x76, 0x44, 0xda, 0x9a, 0x60, 0x31, 0xcd,
        0x54, 0x1c, 0x46, 0x95, 0x0c, 0xf8, 0xb5, 0x86, 0xcf, 0x79, 0x90, 0x9a, 0xaf, 0x4b, 0x9a, 0xd3, 0xb0, 0x55, 0xa3, 0x57, 0x13,
        0xa6,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x94, 0x93, 0x6b, 0x83, 0x97, 0xe7, 0x1b, 0xf0, 0x26, 0xa0, 0x43, 0x70, 0xcc, 0x71, 0x7c, 0xf9, 0xe8, 0xf5, 0x56, 0x0f, 0x7c,
        0xf9, 0x57, 0x9d, 0xf6, 0xc5, 0x2d, 0x2c, 0x90, 0x15, 0x7a, 0x18, 0xd7, 0x2a, 0xf6, 0x58, 0x47, 0xd0, 0xaf, 0xc3, 0x65, 0x0a,
        0xe4, 0xca, 0x64, 0x28, 0x11, 0xcd, 0x62, 0x0b, 0x3e, 0x87, 0xdb, 0x14, 0x51, 0x30, 0x4b, 0x0f, 0x98, 0x97, 0x5f, 0x1a, 0xcf,
        0xc2,
    ]),
    // kaspa-pq: genesis commits to the 13B premine (40 vaults × 0.1B + 1 main × 9B)
    // = MuHash over `config::premine::misaka_premine_utxos()`. Test nets share one
    // commitment (same Claude-managed 9B + shared vault payloads); mainnet differs.
    utxo_commitment: Hash64::from_bytes([
        0xd0, 0xe8, 0xb1, 0x14, 0x85, 0xe4, 0xfe, 0xa7, 0xad, 0xb0, 0x81, 0xd2, 0xeb, 0x83, 0xc6, 0xcc, 0xdb, 0x94, 0x80, 0x9a, 0x12,
        0xf4, 0x76, 0x97, 0x9e, 0x83, 0x29, 0xcb, 0xaa, 0xdb, 0x15, 0x96, 0xee, 0x71, 0xef, 0x05, 0x4d, 0x5e, 0x6e, 0xf5, 0x45, 0x54,
        0x10, 0x51, 0x10, 0x82, 0xe5, 0x0b, 0x5b, 0x1e, 0x3e, 0xd2, 0xa8, 0x0d, 0x27, 0x4b, 0x1f, 0xfa, 0xc2, 0x6f, 0x15, 0x71, 0xef,
        0xba,
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
        0xc3, 0x1b, 0xfa, 0xe8, 0xfc, 0x7a, 0x45, 0x01, 0x0d, 0x6a, 0xab, 0x0e, 0x38, 0xeb, 0xdc, 0xce, 0x1e, 0x87, 0xb1, 0x44, 0x4e,
        0xb9, 0xaa, 0xc6, 0xed, 0x2d, 0x00, 0xde, 0x7f, 0x28, 0x6a, 0xa6, 0x78, 0x5e, 0xb8, 0x84, 0x16, 0xb7, 0xe7, 0xb3, 0xc4, 0xb4,
        0x67, 0x50, 0xc2, 0xae, 0x9e, 0x17, 0xbf, 0xa6, 0xe9, 0xf0, 0x31, 0x2a, 0x4d, 0xb8, 0x2e, 0xb5, 0x65, 0x2f, 0x17, 0x67, 0x4f,
        0x12,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0xd1, 0x96, 0x61, 0x33, 0xe5, 0x47, 0xbb, 0xcb, 0xba, 0x99, 0xe6, 0x39, 0x7d, 0x39, 0xde, 0x71, 0xea, 0xa9, 0x6f, 0xd9, 0x50,
        0x3a, 0x17, 0x67, 0xc3, 0x60, 0x1c, 0x4b, 0x63, 0x4e, 0x68, 0xe2, 0x19, 0x12, 0xf8, 0xff, 0x19, 0x63, 0x37, 0x99, 0x17, 0x68,
        0xc1, 0x70, 0xda, 0x86, 0x3a, 0xdb, 0x94, 0x86, 0xfc, 0x20, 0x48, 0xc0, 0xf0, 0x4b, 0xcf, 0xc6, 0x3f, 0xef, 0x15, 0x80, 0x31,
        0x3e,
    ]),
    // kaspa-pq: genesis commits to the 13B premine (40 vaults × 0.1B + 1 main × 9B)
    // = MuHash over `config::premine::misaka_premine_utxos()`. Test nets share one
    // commitment (same Claude-managed 9B + shared vault payloads); mainnet differs.
    utxo_commitment: Hash64::from_bytes([
        0xd0, 0xe8, 0xb1, 0x14, 0x85, 0xe4, 0xfe, 0xa7, 0xad, 0xb0, 0x81, 0xd2, 0xeb, 0x83, 0xc6, 0xcc, 0xdb, 0x94, 0x80, 0x9a, 0x12,
        0xf4, 0x76, 0x97, 0x9e, 0x83, 0x29, 0xcb, 0xaa, 0xdb, 0x15, 0x96, 0xee, 0x71, 0xef, 0x05, 0x4d, 0x5e, 0x6e, 0xf5, 0x45, 0x54,
        0x10, 0x51, 0x10, 0x82, 0xe5, 0x0b, 0x5b, 0x1e, 0x3e, 0xd2, 0xa8, 0x0d, 0x27, 0x4b, 0x1f, 0xfa, 0xc2, 0x6f, 0x15, 0x71, 0xef,
        0xba,
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

    // PR-9.5g: re-enabled with the `test_genesis_hashes` above; params
    // widened to the 64-byte Hash64 (block hash / merkle root).
    fn assert_hashes_eq(got: Hash64, expected: Hash64) {
        if got != expected {
            // Special hex print to ease changing the genesis hash according to the print if needed
            panic!("Got hash {:#04x?} while expecting {:#04x?}", got.as_bytes(), expected.as_bytes());
        }
    }
}
