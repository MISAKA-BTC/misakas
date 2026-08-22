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
    // MISAKA Phase 4 (PALW): recomputed for the trivial-bits + "-palw"-marker re-genesis via
    // `gen_kaspa_pq_genesis_hashes` (the payload marker moves the merkle root too).
    hash: Hash64::from_bytes([
        0x47, 0x7f, 0x85, 0xfc, 0xa5, 0x16, 0x74, 0xf5, 0xf6, 0x58, 0x5d, 0xd6, 0xbe, 0xdb, 0x3e, 0x05, 0x5e, 0x4d, 0x89, 0xe9, 0x66,
        0x04, 0xd0, 0x39, 0xd6, 0xf6, 0x56, 0x34, 0xe3, 0x50, 0x65, 0x72, 0x0e, 0xca, 0x7d, 0x07, 0xc6, 0xdb, 0xfd, 0xf8, 0x50, 0x02,
        0xce, 0x04, 0xe5, 0xe1, 0xf9, 0x66, 0x59, 0x5a, 0xa9, 0xd8, 0x57, 0x5a, 0xd3, 0x18, 0x2b, 0xb2, 0xb5, 0xdf, 0x3a, 0x1f, 0x7d,
        0x1a,
    ]),
    version: 0,
    // PR-9.5g: recomputed (64-byte Hash64) via `gen_kaspa_pq_genesis_hashes`.
    hash_merkle_root: Hash64::from_bytes([
        0x9d, 0x49, 0xe1, 0x8b, 0xea, 0x8d, 0x0a, 0x47, 0x63, 0x28, 0x96, 0x83, 0x07, 0x93, 0xc9, 0x01, 0x68, 0xab, 0x45, 0xea, 0xcc,
        0x93, 0x49, 0xca, 0x66, 0x94, 0xf0, 0x43, 0x54, 0xf8, 0x02, 0xa8, 0x77, 0x9b, 0x13, 0xdb, 0x53, 0xcf, 0x20, 0xf4, 0xbd, 0x8b,
        0xbe, 0xdc, 0x07, 0xcf, 0x55, 0x2f, 0x83, 0x7d, 0xc6, 0xc1, 0x67, 0xb4, 0x7a, 0x98, 0x0e, 0xb1, 0x2f, 0xd9, 0x8b, 0xc4, 0x17,
        0x72,
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
    // MISAKA Phase 4 (PALW LLM PoW, ADR-0021): genesis difficulty is the easiest representable
    // target (~2^255, p ≈ 1/2 per attempt). One attempt = one full pinned-LLM inference
    // (~0.3-0.5/s per Apple-Silicon machine), so the Phase-3 hash-calibrated `0x1e14f8b5`
    // (D ≈ 400k against 9.89M H/s) is ~2^-43 per attempt — unreachable. From max target the
    // DAA walks difficulty onto the 10 s cadence within the min window; erring easy stays
    // self-correcting exactly as the Phase-3 note said.
    bits: 0x207fffff,
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
        // MISAKA Phase-4 re-genesis marker "-palw" (deterministic-LLM PoW, ADR-0021): same rationale —
        // the PALW chain is cryptographically distinct from the superseded BLAKE2b-SHA3 testnet.
        0x2d, 0x70, 0x61, 0x6c, 0x77,
    ],
};

pub const TESTNET11_GENESIS: GenesisBlock = GenesisBlock {
    // Re-genesis (2026-08-20, "Relaunch 2" + the public main-wallet move): recomputed via
    // `gen_kaspa_pq_genesis_hashes` for the community-allocation utxo_commitment below and the
    // bumped relaunch marker in the payload (which also moves the merkle root).
    hash: Hash64::from_bytes([
        0x8b, 0xbd, 0x3b, 0xe6, 0xeb, 0xa2, 0xf3, 0xa3, 0x90, 0x77, 0x5c, 0x06, 0xfa, 0x43, 0x94, 0xfe, 0xdd, 0x23, 0xf3, 0x25, 0x39,
        0x18, 0xe3, 0xa3, 0x22, 0xd3, 0x0a, 0x5a, 0x77, 0xfa, 0x73, 0x30, 0x91, 0x7a, 0x5a, 0x14, 0x2f, 0xa9, 0xcd, 0x06, 0x17, 0x71,
        0xdc, 0xeb, 0xa8, 0x1b, 0xaf, 0x6d, 0x83, 0xbf, 0x2d, 0xc8, 0x58, 0x0a, 0xb0, 0x3d, 0x24, 0xc3, 0x1e, 0x3a, 0x03, 0x7a, 0xac,
        0xff,
    ]),
    hash_merkle_root: Hash64::from_bytes([
        0xd8, 0xed, 0x27, 0x01, 0x0b, 0x9b, 0x2e, 0x88, 0x85, 0x83, 0x46, 0xbf, 0x4d, 0xc4, 0xe5, 0x6a, 0xd1, 0x26, 0xee, 0x28, 0x62,
        0x0b, 0xa9, 0xb3, 0x27, 0x76, 0x44, 0x5b, 0xd4, 0x83, 0xfd, 0xdf, 0x66, 0x31, 0x5a, 0x89, 0x03, 0x08, 0x1c, 0xaf, 0x35, 0xa3,
        0xa2, 0x7f, 0x3c, 0x0d, 0xad, 0x26, 0xc4, 0x49, 0xe1, 0xbf, 0x51, 0x3d, 0xa5, 0xe8, 0xa7, 0x87, 0x7d, 0x28, 0x74, 0x12, 0x92,
        0xb1,
    ]),
    // Public-relaunch genesis (2026-08-20): commits to the shared 13B premine PLUS the 347M MSK
    // community allocation (`config::premine::TESTNET11_COMMUNITY_ALLOCATIONS`, 9 UTXOs on the
    // "misaka-t11-community" sentinel txid) = MuHash over
    // `config::premine::genesis_premine_utxos_for(testnet-11)`. testnet-10 keeps the shared
    // test-net commitment above — its running chain does not move.
    utxo_commitment: Hash64::from_bytes([
        0xac, 0x07, 0xf8, 0xbd, 0xc1, 0x84, 0x04, 0x64, 0x37, 0x9f, 0xfc, 0xbf, 0xb2, 0xea, 0x5b, 0x65, 0x8a, 0xed, 0x7b, 0x0c, 0xa8,
        0x6d, 0x1d, 0xfc, 0x7a, 0xc2, 0x7b, 0x1c, 0x46, 0x5d, 0x5c, 0x9e, 0x12, 0xb9, 0xfa, 0x01, 0x59, 0x73, 0x9a, 0x8d, 0xae, 0xd0,
        0xc7, 0xdb, 0x99, 0x39, 0x22, 0x6e, 0xcc, 0x5a, 0x84, 0xd2, 0xa9, 0x78, 0x88, 0x66, 0x85, 0x61, 0xe4, 0x5a, 0xe2, 0x94, 0x76,
        0x11,
    ]),
    bits: 0x200ccccc, // see `gen_testnet11_genesis` (= testnet target ×10 harder; rescaled for the PALW trivial-bits re-genesis)
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                                                 // Varint
        0x00,                                                                                                 // OP-FALSE
        // "misaka-testnet"
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74,
        // TN11, kaspa-pq Relaunch 2: the community-allocation public relaunch (2026-08-20). The
        // bump makes this chain cryptographically distinct from the Relaunch-1 soak — an un-wiped
        // soak node hits the startup genesis-mismatch guard instead of silently resuming.
        11, 2,
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

/// **The PALW-RC network's genesis (ADR-0036 Decision 2, ADR-0042 PR-10).**
///
/// A NEW identity, not a re-flag of an existing one, and that is the decision's whole content:
/// mainnet cannot carry PALW (its cadence is 100 ms against PALW's frozen 120 s, and at 10 BPS its
/// `finality_depth` is orders of magnitude above either shipped `w_challenge` — two independent
/// refusals, measured in `the_shipped_mainnet_identity_cannot_carry_a_palw_schedule`), and
/// testnet-10/11 cannot either: a `ConsensusV2` network may activate no V1 PALW proof-of-work,
/// which both of them do.
///
/// Its premine is the standard 40 vaults plus a 9B main wallet at `PALW_PUBLIC_MAIN_ADDRESS` —
/// the operator's own address, the same one testnet-11 moved to, because this net is public and a
/// regenerable test key is not. It carries no community allocation: that list is testnet-11's.
/// Independently of the allocation, the marker in the coinbase payload moves the merkle root and
/// therefore the block hash, so this genesis is a different block from every other network's by
/// construction, not merely by balance.
///
/// The hash below is computed by `gen_kaspa_pq_genesis_hashes`, not typed.
pub const PALW_RC_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash64::from_bytes([
        0x28, 0xa4, 0x4a, 0x68, 0x0b, 0xe0, 0xfb, 0x35, 0xe6, 0xe2, 0x97, 0x89, 0xd7, 0x8e, 0xc3, 0x24, 0x1b, 0x4c, 0x06, 0x7f, 0x54,
        0xa4, 0xa7, 0x1e, 0xfc, 0x04, 0xbc, 0xe3, 0xa7, 0x8a, 0x05, 0x09, 0x9d, 0x74, 0x25, 0xd4, 0x85, 0x35, 0x1a, 0xe5, 0x57, 0x5e,
        0x5d, 0xf0, 0x4d, 0xbe, 0xb6, 0x12, 0xb3, 0xdc, 0xa0, 0x55, 0x81, 0x20, 0x0d, 0x9d, 0x58, 0x2f, 0xa1, 0x9d, 0x08, 0xf8, 0xbb,
        0x24,
    ]),
    version: 0,
    hash_merkle_root: Hash64::from_bytes([
        0x4f, 0xb0, 0xec, 0xf4, 0x36, 0x74, 0x65, 0x5f, 0x8f, 0x5c, 0x2f, 0xed, 0xc3, 0xb5, 0x5a, 0x38, 0xd3, 0x03, 0xf2, 0x39, 0x97,
        0x3f, 0x96, 0x89, 0x53, 0xc7, 0xd0, 0x71, 0x77, 0x0f, 0x1e, 0x91, 0x3a, 0xd5, 0x93, 0x35, 0x22, 0x2f, 0x4c, 0x92, 0x25, 0x40,
        0x7c, 0x0b, 0x37, 0xe5, 0x08, 0x38, 0xe6, 0xad, 0x09, 0x27, 0x3c, 0xa8, 0x26, 0x68, 0x8b, 0x19, 0x5a, 0x91, 0xc6, 0x05, 0xd1,
        0x97,
    ]),
    // The premine with the PUBLIC PALW main wallet (`PALW_PUBLIC_MAIN_ADDRESS`), no community
    // set — that list is testnet-11's. The 40 vault UTXOs are the same ones every network has;
    // the 9B main wallet differs, and since 2026-08-22 it is ALSO reduced by the six genesis-bond
    // fee floats carved out of it (`palw_rc_bond_fee_floats`) — without which a PALW network
    // cannot fund the first submission that releases any producer's escrow. Re-pinned with
    // `cargo test -p kaspa-consensus --lib repin::print -- --ignored --nocapture`; the M-07 guard
    // refuses to boot on a mismatch, which is how this change announced that it needed re-pinning.
    utxo_commitment: Hash64::from_bytes([
        0x4f, 0x6f, 0x4b, 0xd9, 0xc4, 0x0b, 0xa2, 0x34, 0x99, 0xfd, 0x90, 0x02, 0x26, 0x69, 0x9f, 0xfc, 0xf9, 0x1c, 0x7b, 0xa1, 0x5a,
        0x71, 0x9c, 0xd1, 0x9f, 0xfd, 0xe5, 0xbf, 0x45, 0x26, 0xc9, 0xbe, 0x62, 0x9d, 0x87, 0xe4, 0xb9, 0x6d, 0xe1, 0x1c, 0x73, 0xfe,
        0x45, 0x17, 0x44, 0xaa, 0x01, 0x62, 0x49, 0x7a, 0x68, 0x76, 0xaf, 0x35, 0xde, 0x96, 0x14, 0x65, 0xbd, 0x24, 0xec, 0x19, 0x4b,
        0x54,
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
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "misaka-palw-rc" — the marker that makes this a different block from every other
        // network's genesis, through the merkle root.
        0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x61, 0x6c, 0x77, 0x2d, 0x72, 0x63,
    ],
};

pub const DEVNET_GENESIS: GenesisBlock = GenesisBlock {
    // PALW LLM PoW: recomputed for the 0x207fffff genesis bits (re-genesis of devnet) via
    // `gen_kaspa_pq_genesis_hashes`; the merkle root is bits-independent and unchanged.
    hash: Hash64::from_bytes([
        0x55, 0xc0, 0x31, 0xa4, 0x52, 0xeb, 0xa0, 0xa1, 0xba, 0xa0, 0x57, 0x6c, 0x24, 0x01, 0x73, 0xcb, 0x15, 0x4f, 0xcc, 0x34, 0x35,
        0x97, 0x33, 0x13, 0x4b, 0x4b, 0x20, 0x37, 0x30, 0x6a, 0x1f, 0xe2, 0x5c, 0x78, 0xc9, 0xa0, 0xe4, 0x9c, 0x10, 0x45, 0x67, 0xd6,
        0x94, 0x94, 0x7f, 0x01, 0x5a, 0x74, 0xdd, 0x1b, 0xbe, 0x49, 0x4d, 0x9f, 0xd6, 0x2a, 0x61, 0xd9, 0x50, 0x91, 0x53, 0x33, 0x29,
        0x6b,
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
    // PALW LLM PoW (0.1 bps): start at the easiest representable target (~2^255, p ≈ 1/2 per
    // attempt). One attempt = one full pinned-LLM inference (~0.3/s per machine), so the old
    // hash-miner start bits (0x1e21bc1c ≈ 2^-43 per attempt) would never find a block; from here
    // the DAA walks difficulty onto the 10 s cadence within the min window.
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
            ("PALW_RC_GENESIS", &PALW_RC_GENESIS),
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
        // Divide before multiplying: since the PALW re-genesis the testnet target is the easiest
        // representable (~2^255), and `target * bps` overflows Uint256. The floor-first form
        // keeps the "×10 harder than testnet" relation to compact-bits precision.
        let scaled_target = target / 100 * bps;
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
