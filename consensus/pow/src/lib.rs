// public for benchmarks
#[doc(hidden)]
pub mod matrix;
pub mod palw;
#[cfg(feature = "wasm32-sdk")]
pub mod wasm;
#[doc(hidden)]
pub mod xoshiro;

use std::cmp::max;

use crate::matrix::Matrix;
use kaspa_consensus_core::{
    BlockLevel, hashing,
    header::Header,
    pow_layer0::{
        POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_LLM, POW_FINALIZER_BYTES,
        POW_L1_BLAKE2B_SHA3_OUT_BYTES, POW_L1_PALW_OUT_BYTES, POW_L1_TAG_MAX_BYTES, PowLayer0Error, argon2id_l1_tag_v1,
        blake2b_sha3_l1_tag_v1, l1_seed32_for_kheavyhash_v1, pow_finalizer_blake2b_512,
    },
};
use kaspa_hashes::{Hash64, PowHash};
use kaspa_math::{Uint256, Uint512};

/// State is an intermediate data structure with pre-computed values to speed up mining.
pub struct State {
    pub(crate) matrix: Matrix,
    pub(crate) target: Uint256,
    // PRE_POW_HASH || TIME || 32 zero byte padding; without NONCE
    pub(crate) hasher: PowHash,
}

impl State {
    #[inline]
    pub fn new(header: &Header) -> Self {
        let target = Uint256::from_compact_target_bits(header.bits);
        // Zero out the time and nonce.
        let pre_pow_hash = hashing::header::hash_override_nonce_time(header, 0, 0);
        // PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
        let hasher = PowHash::new(pre_pow_hash, header.timestamp);
        let matrix = Matrix::generate(pre_pow_hash);

        Self { matrix, target, hasher }
    }

    #[inline]
    #[must_use]
    /// PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
    pub fn calculate_pow(&self, nonce: u64) -> Uint256 {
        // Hasher already contains PRE_POW_HASH || TIME || 32 zero byte padding; so only the NONCE is missing
        let hash = self.hasher.clone().finalize_with_nonce(nonce);
        let hash = self.matrix.heavy_hash(hash);
        Uint256::from_le_bytes(hash.as_bytes())
    }

    #[inline]
    #[must_use]
    pub fn check_pow(&self, nonce: u64) -> (bool, Uint256) {
        let pow = self.calculate_pow(nonce);
        // The pow hash must be less or equal than the claimed target.
        (pow <= self.target, pow)
    }
}

pub fn calc_block_level(header: &Header, max_block_level: BlockLevel) -> BlockLevel {
    let (block_level, _) = calc_block_level_check_pow(header, max_block_level);
    block_level
}

pub fn calc_block_level_check_pow(header: &Header, max_block_level: BlockLevel) -> (BlockLevel, bool) {
    if header.parents_by_level.is_empty() {
        return (max_block_level, true); // Genesis has the max block level
    }

    let state = State::new(header);
    let (passed, pow) = state.check_pow(header.nonce);
    let block_level = calc_level_from_pow(pow, max_block_level);
    (block_level, passed)
}

pub fn calc_level_from_pow(pow: Uint256, max_block_level: BlockLevel) -> BlockLevel {
    let signed_block_level = max_block_level as i64 - pow.bits() as i64;
    max(signed_block_level, 0) as BlockLevel
}

// ---------------------------------------------------------------------
// kaspa-pq PR-8.6: Layer 0 (BLAKE2b-512) block-level / PoW-check entry
// points used by consensus header & pruning-proof validation. These
// replace the legacy 32-byte `State`-based functions above on the
// kaspa-pq consensus path (ADR-0007 / ADR-0008).
// ---------------------------------------------------------------------

/// Block level from a 512-bit Layer 0 PoW value. The ADR-0007
/// difficulty lift (`target_512 = target_256 << 256`) means the top
/// 256 bits of the 512-bit pow carry the same difficulty information
/// as the legacy 256-bit pow, so the level is computed from that
/// projection — preserving the upstream level semantics exactly while
/// the acceptance test uses the full 512-bit comparison.
#[inline]
pub fn calc_level_from_pow_512(pow_512: Uint512, max_block_level: BlockLevel) -> BlockLevel {
    // `pow_512 >> 256` is at most 256 bits wide, so the conversion never truncates.
    let pow_256 = Uint256::try_from(pow_512 >> 256).unwrap_or(Uint256::ZERO);
    calc_level_from_pow(pow_256, max_block_level)
}

/// kaspa-pq Layer 0 replacement for [`calc_block_level_check_pow`].
/// `network_id` is the per-network domain-separation tag fed to the
/// Layer 0 finalizer (see [`StateLayer0::new`]).
pub fn calc_block_level_check_pow_layer0(header: &Header, network_id: &[u8], max_block_level: BlockLevel) -> (BlockLevel, bool) {
    if header.parents_by_level.is_empty() {
        return (max_block_level, true); // Genesis has the max block level
    }

    let state = StateLayer0::new(header, network_id);
    match state.check_pow_layer0(header.nonce) {
        Ok((passed, pow_512)) => (calc_level_from_pow_512(pow_512, max_block_level), passed),
        // PALW (algo_id = 4) errors are ENVIRONMENTAL, never header-dependent (the prompt is a
        // fixed frame around the seed, far under the ceiling): a missing/broken worker means this
        // node cannot judge ANY header on a PALW network. Returning `false` here would silently
        // reject every valid block — stall the node, ban honest peers — so fail loud instead,
        // exactly like the VLT devnet fence panics a kaspad missing its runtime.
        Err(e @ (PowLayer0Error::PalwUnavailable(_) | PowLayer0Error::PalwWorkerFailed(_))) => {
            panic!("PALW PoW validation cannot run on this node: {e}")
        }
        // Remaining variants are finalizer-internal misuse, which cannot happen for a well-formed
        // header; treat as a failed PoW.
        Err(_) => (0, false),
    }
}

/// kaspa-pq Layer 0 replacement for [`calc_block_level`].
pub fn calc_block_level_layer0(header: &Header, network_id: &[u8], max_block_level: BlockLevel) -> BlockLevel {
    calc_block_level_check_pow_layer0(header, network_id, max_block_level).0
}

// ---------------------------------------------------------------------
// kaspa-pq PR-8.6: Layer 0 PoW verifier
// ---------------------------------------------------------------------

/// kaspa-pq Layer 0 PoW verifier state. Wraps the existing upstream
/// kHeavyHash machinery (Phase 1 `algo_id = 1`) inside the
/// BLAKE2b-512 Layer 0 finalizer (ADR-0007 + ADR-0008).
///
/// Construction:
///
///   1. Compute the 64-byte pre-PoW hash via
///      `hashing::header::pre_pow_hash_64` (BlockPrePowHash64 over
///      the header preimage with nonce/time zeroed).
///   2. Derive the 32-byte kHeavyHash v1 seed from the 64-byte
///      pre-PoW hash via `l1_seed32_for_kheavyhash_v1` — the
///      domain-separated bridge that lets the upstream kHeavyHash
///      take a 32-byte input even though kaspa-pq has widened
///      everything to 64 bytes.
///   3. Seed the existing `PowHash` and `Matrix` with that 32-byte
///      seed exactly the way the upstream `State::new` does.
///   4. Compute the 512-bit Layer 0 target via
///      `Uint512::from_compact_target_bits_512`.
///
/// `check_pow_layer0(nonce)`:
///
///   1. Run kHeavyHash to produce the 32-byte L1 tag (the same
///      computation the upstream `State::calculate_pow` does).
///   2. Feed everything (network_id, algo_id = 1, pre_pow_hash_64,
///      timestamp, bits, nonce, length-prefixed L1 tag) into
///      `pow_finalizer_blake2b_512`.
///   3. Compare `Uint512::from_le_bytes(pow_512)` against the
///      512-bit target.
pub struct StateLayer0 {
    /// `Some` ONLY for `POW_ALGO_ID_KHEAVYHASH` — the kHeavyHash L1 tag is the
    /// sole consumer. algo_id 2 (Argon2id) / 3 (BLAKE2b-SHA3) / 4 (PALW LLM)
    /// ignore it, so the expensive `Matrix::generate` (a 64×64 rank-64 search)
    /// is skipped for them (perf: it was being paid per header on the SHA3
    /// chain — IBD/proof bug).
    pub(crate) matrix: Option<Matrix>,
    pub(crate) target_512: Uint512,
    /// Cached so each `check_pow_layer0` call doesn't re-hash the
    /// header — the only varying input across nonce trials is the
    /// nonce itself (and, derived from it, the L1 tag).
    pub(crate) pre_pow_hash_64: Hash64,
    pub(crate) network_id: Vec<u8>,
    pub(crate) timestamp: u64,
    pub(crate) bits: u32,
    /// PR-9.5d: Layer 1 algorithm discriminator read from
    /// `header.pow_algo_id`. Fed into the Layer 0 finalizer so the
    /// PoW digest binds to the declared algorithm. Phase 1 admits
    /// only `POW_ALGO_ID_KHEAVYHASH`; rejection of any other value
    /// is the header-validation rule's job (consensus/src), not the
    /// finalizer's.
    pub(crate) pow_algo_id: u8,
    /// PRE_POW_HASH || TIME || 32 zero byte padding; without NONCE.
    /// Seeded with the derived `l1_seed32` (not the 64-byte pre-PoW
    /// hash) so the kHeavyHash interface stays 32-byte-input. `Some` only for
    /// `POW_ALGO_ID_KHEAVYHASH` (see `matrix`).
    pub(crate) hasher: Option<PowHash>,
}

impl StateLayer0 {
    /// `network_id` is the kaspa-pq `NetworkId::to_string` byte
    /// form (e.g. `b"mainnet"`, `b"testnet-10"`). It's a
    /// consensus-input field of the Layer 0 finalizer — different
    /// `network_id` bytes domain-separate the PoW per-network so a
    /// solved header from one kaspa-pq network can't be replayed
    /// on another.
    #[inline]
    pub fn new(header: &Header, network_id: &[u8]) -> Self {
        let pre_pow_hash_64 = hashing::header::pre_pow_hash_64(header);
        let target_512 = Uint512::from_compact_target_bits_512(header.bits);
        // Only the kHeavyHash L1 tag (algo_id = 1) consumes the PowHash + Matrix.
        // algo_id 2 (Argon2id) / 3 (BLAKE2b-SHA3) compute their tag directly from
        // (pre_pow_hash, nonce), so skip the expensive `Matrix::generate` for them
        // — it was wrongly paid for every header on the SHA3 chain, slowing IBD
        // header validation and pruning-proof checks. Consensus output unchanged
        // (calculate_l1_tag never reads matrix/hasher off the non-kHeavyHash arms).
        let (hasher, matrix) = if header.pow_algo_id == POW_ALGO_ID_KHEAVYHASH {
            let l1_seed32 = l1_seed32_for_kheavyhash_v1(pre_pow_hash_64);
            (Some(PowHash::new(l1_seed32, header.timestamp)), Some(Matrix::generate(l1_seed32)))
        } else {
            (None, None)
        };

        Self {
            matrix,
            target_512,
            pre_pow_hash_64,
            network_id: network_id.to_vec(),
            timestamp: header.timestamp,
            bits: header.bits,
            pow_algo_id: header.pow_algo_id,
            hasher,
        }
    }

    /// Compute the Layer-1 tag for `nonce` into `buf`, returning its length. The tag width varies
    /// by `pow_algo_id` (kHeavyHash/Argon2id = 32 bytes, BLAKE2b-SHA3 = 128, PALW = 200), so the
    /// caller passes a max-width stack buffer and reads back `&buf[..len]` — this keeps the miner
    /// grind hot loop allocation-free (no per-nonce heap `Vec`). Only the PALW arm can fail (it
    /// reaches an external runtime); every hash arm is infallible.
    #[inline]
    fn calculate_l1_tag(&self, nonce: u64, buf: &mut [u8; POW_L1_TAG_MAX_BYTES]) -> Result<usize, PowLayer0Error> {
        match self.pow_algo_id {
            // Phase 4 (algo_id = 4): one deterministic pinned-LLM inference over the seed derived
            // from (network, pre_pow_hash, timestamp, nonce). 200 bytes.
            POW_ALGO_ID_PALW_LLM => {
                let tag = palw::palw_l1_tag(self.pre_pow_hash_64, self.timestamp, nonce, &self.network_id)?;
                buf[..POW_L1_PALW_OUT_BYTES].copy_from_slice(&tag);
                Ok(POW_L1_PALW_OUT_BYTES)
            }
            // Phase 3 (algo_id = 3): compute-only BLAKE2b-512 ∥ SHA3-512 over (pre_pow_hash, nonce). 128 bytes.
            POW_ALGO_ID_BLAKE2B_SHA3 => {
                buf[..POW_L1_BLAKE2B_SHA3_OUT_BYTES]
                    .copy_from_slice(&blake2b_sha3_l1_tag_v1(self.pre_pow_hash_64, nonce, &self.network_id));
                Ok(POW_L1_BLAKE2B_SHA3_OUT_BYTES)
            }
            // Phase 2 (algo_id = 2): memory-hard Argon2id over (pre_pow_hash, nonce). 32 bytes.
            POW_ALGO_ID_ARGON2ID => {
                buf[..32].copy_from_slice(&argon2id_l1_tag_v1(self.pre_pow_hash_64, nonce, &self.network_id));
                Ok(32)
            }
            // Phase 1 (algo_id = 1, kHeavyHash) and the default. Any other id is rejected up-stack
            // by header validation (`check_algo_id`) before PoW is ever computed.
            _ => {
                // kHeavyHash (algo_id = 1). new() populates hasher+matrix exactly for this id;
                // any other id is rejected up-stack by header validation before PoW runs.
                let hasher = self.hasher.as_ref().expect("kHeavyHash StateLayer0 carries a PowHash");
                let matrix = self.matrix.as_ref().expect("kHeavyHash StateLayer0 carries a Matrix");
                let hash = hasher.clone().finalize_with_nonce(nonce);
                buf[..32].copy_from_slice(&matrix.heavy_hash(hash).as_bytes());
                Ok(32)
            }
        }
    }

    /// Compute the full Layer 0 PoW digest for the given nonce.
    /// 64 bytes (BLAKE2b-512 output).
    #[inline]
    pub fn calculate_pow_layer0(&self, nonce: u64) -> Result<[u8; POW_FINALIZER_BYTES], PowLayer0Error> {
        let mut tag_buf = [0u8; POW_L1_TAG_MAX_BYTES];
        let tag_len = self.calculate_l1_tag(nonce, &mut tag_buf)?;
        pow_finalizer_blake2b_512(
            &self.network_id,
            // PR-9.5d: bind the digest to the header's declared
            // algo id rather than a hardcoded constant.
            self.pow_algo_id,
            self.pre_pow_hash_64,
            self.timestamp,
            self.bits,
            nonce,
            &tag_buf[..tag_len],
        )
    }

    /// Full Layer 0 verifier: produces the 64-byte digest and
    /// compares against the 512-bit target. Returns
    /// `(passes, pow_512_value)`. The `pow_512_value` is exposed
    /// for difficulty / block-level computations that look at the
    /// number of leading zero bits.
    #[inline]
    pub fn check_pow_layer0(&self, nonce: u64) -> Result<(bool, Uint512), PowLayer0Error> {
        let digest = self.calculate_pow_layer0(nonce)?;
        let pow_512 = Uint512::from_le_bytes(digest);
        Ok((pow_512 <= self.target_512, pow_512))
    }
}

#[cfg(test)]
mod tests_pq {
    use super::*;
    use kaspa_consensus_core::{BlueWorkType, header::Header, pow_layer0::POW_ALGO_ID_KHEAVYHASH};
    use kaspa_hashes::ZERO_HASH64;

    /// Serializes the two PALW tests that read/write the process-global fixture env var —
    /// without this, the fixture test's `set_var` can land inside the no-worker test's window
    /// between its guard check and its assertion.
    static PALW_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn dummy_header(bits: u32, nonce: u64, timestamp: u64) -> Header {
        dummy_header_algo(bits, nonce, timestamp, POW_ALGO_ID_KHEAVYHASH)
    }

    fn dummy_header_algo(bits: u32, nonce: u64, timestamp: u64, pow_algo_id: u8) -> Header {
        Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            // PR-9.5c: merkle roots are Hash64; PR-9.5d: pow_algo_id added.
            ZERO_HASH64, // hash_merkle_root
            ZERO_HASH64, // accepted_id_merkle_root
            ZERO_HASH64, // kaspa-pq (ADR-0004 / design §12): utxo_commitment (Hash64)
            timestamp,
            bits,
            nonce,
            pow_algo_id,
            0, // daa_score
            BlueWorkType::from_u64(0),
            0,
            ZERO_HASH64, // PR-9.5e: pruning_point is a block hash (Hash64)
        )
    }

    /// The Layer 0 verifier produces a deterministic 64-byte digest
    /// for a given (header, nonce, network_id) triple.
    #[test]
    fn layer0_calculate_pow_is_deterministic() {
        let header = dummy_header(0x207fffff, 0, 1_700_000_000);
        let s = StateLayer0::new(&header, b"simnet");
        let a = s.calculate_pow_layer0(42).unwrap();
        let b = s.calculate_pow_layer0(42).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), POW_FINALIZER_BYTES);
    }

    /// Changing nonce changes the Layer 0 digest. Changing
    /// network_id changes the Layer 0 digest. Both are properties
    /// of the ADR-0007 finalizer layout that the wiring must
    /// preserve.
    #[test]
    fn layer0_inputs_change_digest() {
        let header = dummy_header(0x207fffff, 0, 1_700_000_000);
        let s_simnet = StateLayer0::new(&header, b"simnet");
        let s_mainnet = StateLayer0::new(&header, b"mainnet");

        let nonce_a = s_simnet.calculate_pow_layer0(42).unwrap();
        let nonce_b = s_simnet.calculate_pow_layer0(43).unwrap();
        assert_ne!(nonce_a, nonce_b);

        let net_a = s_simnet.calculate_pow_layer0(42).unwrap();
        let net_b = s_mainnet.calculate_pow_layer0(42).unwrap();
        assert_ne!(net_a, net_b);
    }

    /// The easiest representable target accepts a large fraction of
    /// digests; a max-difficulty target rejects essentially all.
    #[test]
    fn layer0_check_pow_easy_target_passes_hard_target_rejects() {
        // The easiest compact target 0x207fffff decodes to
        // target_256 ≈ 2^255, which the ADR-0007 difficulty lift
        // (`target_512 = target_256 << 256`) maps to target_512 ≈
        // 2^511 — i.e. ≈50% of uniform 512-bit digests pass *per
        // nonce*, NOT every digest. So scan a small nonce window and
        // require at least one acceptance; P(all 64 reject) ≈ 2^-64.
        let easy = dummy_header(0x207fffff, 0, 1_700_000_000);
        let s_easy = StateLayer0::new(&easy, b"simnet");
        let any_pass = (0u64..64).any(|n| s_easy.check_pow_layer0(n).unwrap().0);
        assert!(any_pass, "easiest target must accept at least one nonce in a small scan");

        // bits = 0x01010000 -> target_256 = 1, lifted to target_512 =
        // 1 << 256 = 2^256. A digest lands below only if its top 256
        // bits are all zero (P ≈ 2^-256), so this rejects in practice.
        let hard = dummy_header(0x01010000, 0, 1_700_000_000);
        let s_hard = StateLayer0::new(&hard, b"simnet");
        let (pass, _) = s_hard.check_pow_layer0(0).unwrap();
        assert!(!pass, "trivially-hard target must reject");
    }

    /// PR-9.5d Phase 2 (ADR-0007): the Layer-0 verifier dispatches its
    /// swappable Layer-1 tag on `header.pow_algo_id`. An `algo_id = 2`
    /// header is validated with memory-hard Argon2id (NOT kHeavyHash):
    /// the verifier's internal tag matches the standalone
    /// `argon2id_l1_tag_v1`, differs from the kHeavyHash path, and a
    /// nonce at the easiest target is accepted by `check_pow_layer0`.
    /// This is the consensus-side proof that a re-genesised Argon2id
    /// chain's blocks validate end-to-end.
    #[test]
    fn layer0_dispatches_argon2id_for_algo_id_2() {
        use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_ARGON2ID, argon2id_l1_tag_v1};
        let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_ARGON2ID);
        let s = StateLayer0::new(&h, b"testnet-10");

        // Dispatch: the verifier's internal L1 tag must equal the
        // standalone Argon2id tag for the same (pre_pow_hash, nonce).
        let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
        let n = s.calculate_l1_tag(7, &mut buf).unwrap();
        let expect = argon2id_l1_tag_v1(s.pre_pow_hash_64, 7, b"testnet-10");
        assert_eq!(&buf[..n], expect.as_slice(), "algo_id=2 must compute the Argon2id L1 tag");

        // ...and differ from the kHeavyHash path for the same input.
        let kh = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_KHEAVYHASH);
        let s_kh = StateLayer0::new(&kh, b"testnet-10");
        let mut buf_kh = [0u8; POW_L1_TAG_MAX_BYTES];
        let n_kh = s_kh.calculate_l1_tag(7, &mut buf_kh).unwrap();
        assert_ne!(&buf_kh[..n_kh], &buf[..n], "kHeavyHash and Argon2id tags must differ");

        // Acceptance: the easiest target accepts at least one Argon2id
        // nonce in a small scan (P(all 64 reject) ≈ 2^-64).
        let any_pass = (0u64..64).any(|n| s.check_pow_layer0(n).unwrap().0);
        assert!(any_pass, "easiest target must accept an Argon2id nonce");
    }

    /// kaspa-pq Phase 3 (ADR-0007): the Layer-0 verifier dispatches its swappable Layer-1 tag on
    /// `header.pow_algo_id`. An `algo_id = 3` header is validated with the compute-only BLAKE2b-512 ∥
    /// SHA3-512 tag: the verifier's internal 128-byte tag matches the standalone
    /// `blake2b_sha3_l1_tag_v1`, differs from the kHeavyHash path, and a nonce at the easiest target
    /// is accepted by `check_pow_layer0`. This is the consensus-side proof that a re-genesised
    /// BLAKE2b-SHA3 chain's blocks validate end-to-end.
    #[test]
    fn layer0_dispatches_blake2b_sha3_for_algo_id_3() {
        use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_BLAKE2B_SHA3, POW_L1_BLAKE2B_SHA3_OUT_BYTES, blake2b_sha3_l1_tag_v1};
        let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_BLAKE2B_SHA3);
        let s = StateLayer0::new(&h, b"testnet-10");

        // Dispatch: the verifier's internal L1 tag must equal the standalone BLAKE2b-SHA3 tag (128 bytes).
        let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
        let n = s.calculate_l1_tag(11, &mut buf).unwrap();
        assert_eq!(n, POW_L1_BLAKE2B_SHA3_OUT_BYTES, "BLAKE2b-SHA3 tag is 128 bytes");
        let expect = blake2b_sha3_l1_tag_v1(s.pre_pow_hash_64, 11, b"testnet-10");
        assert_eq!(&buf[..n], expect.as_slice(), "algo_id=3 must compute the BLAKE2b-SHA3 L1 tag");

        // ...and differ from the kHeavyHash path for the same input.
        let kh = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_KHEAVYHASH);
        let s_kh = StateLayer0::new(&kh, b"testnet-10");
        let mut buf_kh = [0u8; POW_L1_TAG_MAX_BYTES];
        let n_kh = s_kh.calculate_l1_tag(11, &mut buf_kh).unwrap();
        assert_ne!(&buf_kh[..n_kh], &buf[..n], "kHeavyHash and BLAKE2b-SHA3 tags must differ");

        // Acceptance: the easiest target accepts at least one BLAKE2b-SHA3 nonce in a small scan.
        let any_pass = (0u64..64).any(|n| s.check_pow_layer0(n).unwrap().0);
        assert!(any_pass, "easiest target must accept a BLAKE2b-SHA3 nonce");
    }

    /// PALW LLM PoW (algo_id = 4), fixture mode: the Layer-0 verifier dispatches to the PALW
    /// Layer-1 tag; with `MISAKA_PALW_POW_FIXTURE=1` the tag is the in-process fixture derivation
    /// over the seed. Asserts dispatch (tag == fixture-of-seed), determinism, timestamp
    /// sensitivity (the grinding-closure property: a re-stamped header re-pays the tag), and
    /// easy-target acceptance. Env var is process-global; nothing else in this test binary
    /// computes algo-4 tags, and the prior value is restored on exit.
    #[test]
    fn layer0_dispatches_palw_fixture_for_algo_id_4() {
        use kaspa_consensus_core::pow_layer0::{
            POW_ALGO_ID_PALW_LLM, POW_L1_PALW_OUT_BYTES, palw_fixture_l1_tag_v1, palw_pow_seed_v1,
        };
        const KEY: &str = "MISAKA_PALW_POW_FIXTURE";
        let _env = PALW_ENV_LOCK.lock().unwrap();
        let prev = std::env::var(KEY).ok();
        unsafe { std::env::set_var(KEY, "1") };

        let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_PALW_LLM);
        let s = StateLayer0::new(&h, b"kaspa-devnet");

        // Dispatch: the verifier's internal L1 tag must equal the fixture tag for the seed the
        // verifier is contractually bound to derive — (network, pre_pow_hash, timestamp, nonce).
        let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
        let n = s.calculate_l1_tag(5, &mut buf).unwrap();
        assert_eq!(n, POW_L1_PALW_OUT_BYTES, "PALW tag is 200 bytes");
        let seed = palw_pow_seed_v1(s.pre_pow_hash_64, 1_700_000_000, 5, b"kaspa-devnet");
        assert_eq!(&buf[..n], palw_fixture_l1_tag_v1(&seed).as_slice(), "algo_id=4 must compute the PALW fixture tag");

        // Determinism + nonce sensitivity of the full Layer-0 digest.
        let a = s.calculate_pow_layer0(5).unwrap();
        assert_eq!(a, s.calculate_pow_layer0(5).unwrap(), "PALW Layer-0 digest must be deterministic");
        assert_ne!(a, s.calculate_pow_layer0(6).unwrap(), "nonce must change the PALW digest");

        // Timestamp sensitivity THROUGH THE TAG: two headers differing only in timestamp share a
        // pre-PoW hash prefix computation but must produce different L1 tags (the seed binds the
        // timestamp) — this is what closes the free timestamp-grinding dimension.
        let h2 = dummy_header_algo(0x207fffff, 0, 1_700_000_001, POW_ALGO_ID_PALW_LLM);
        let s2 = StateLayer0::new(&h2, b"kaspa-devnet");
        let mut buf2 = [0u8; POW_L1_TAG_MAX_BYTES];
        let n2 = s2.calculate_l1_tag(5, &mut buf2).unwrap();
        assert_ne!(&buf2[..n2], &buf[..n], "timestamp must change the PALW L1 tag itself, not just the finalizer input");

        // Acceptance: the easiest target accepts at least one PALW nonce in a small scan.
        let any_pass = (0u64..64).any(|n| s.check_pow_layer0(n).unwrap().0);
        assert!(any_pass, "easiest target must accept a PALW nonce");

        match prev {
            Some(v) => unsafe { std::env::set_var(KEY, v) },
            None => unsafe { std::env::remove_var(KEY) },
        }
    }

    /// Without the fixture env and without a worker configured, judging a PALW header is a node
    /// configuration error: `check_pow_layer0` reports `PalwUnavailable` (and the consensus-side
    /// `calc_block_level_check_pow_layer0` escalates that to a panic rather than mis-labelling
    /// valid blocks as bad PoW).
    #[test]
    fn layer0_palw_without_worker_is_unavailable_not_a_failed_pow() {
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_LLM;
        let _env = PALW_ENV_LOCK.lock().unwrap();
        // Only meaningful when the surrounding environment does not configure PALW.
        if std::env::var("MISAKA_PALW_POW_FIXTURE").as_deref() == Ok("1") || std::env::var("PALW_WORKER").is_ok() {
            return;
        }
        let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_PALW_LLM);
        let s = StateLayer0::new(&h, b"kaspa-devnet");
        match s.check_pow_layer0(0) {
            Err(PowLayer0Error::PalwUnavailable(msg)) => {
                assert!(msg.contains("PALW_WORKER"), "the error must tell the operator which knob to set: {msg}")
            }
            other => panic!("expected PalwUnavailable, got {other:?}"),
        }
    }
}
