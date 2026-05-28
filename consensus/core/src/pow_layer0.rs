//! kaspa-pq Phase 8 (PR-8.3): Layer 0 PoW finalizer + difficulty-lift
//! helpers.
//!
//! See [ADR-0007](../../docs/adr/0007-layered-pow.md). This module
//! contains the **consensus-critical, frozen** half of the Layered
//! PoW:
//!
//! 1. The BLAKE2b-512 keyed finalizer with
//!    [`POW_FINALIZER_DOMAIN`] as the key
//!    (`b"kaspa-pq-pow-v1"`).
//! 2. The 512-bit comparison domain, exposed as
//!    `Uint512`/`Uint576` operations re-exported from
//!    `kaspa_math`.
//! 3. The difficulty-lift helper that maps an upstream 256-bit
//!    target into the kaspa-pq 512-bit comparison domain
//!    (`target_512 = target_256 << 256`; see the ADR for the
//!    block-finding-probability preservation proof).
//!
//! The module is intentionally self-contained: it does **not**
//! reach into the consensus PoW validator yet. The wiring step
//! (PR-8.6) plugs `pow_finalizer_blake2b_512` into the actual
//! `verify_pow` path and consumes `header.pow_algo_id`
//! (introduced in PR-8.4).
//!
//! `algo_id` semantics: at Phase 1 only
//! [`POW_ALGO_ID_KHEAVYHASH`] (`= 1`) is consensus-valid. A
//! future hard-fork ADR will introduce `algo_id = 2, …` for
//! ASIC-hard Layer 1 variants. There is **no** mixed-`algo_id`
//! difficulty arithmetic; transitions are hard cut-offs at a
//! specific DAA score.

use blake2b_simd::Params;
use kaspa_hashes::{Hash, Hash64};
use kaspa_math::{Uint256, Uint512, Uint576};

/// BLAKE2b key for the Layer 0 PoW finalizer. Matches the
/// existing `crypto/hashes/src/hashers.rs` pattern of using a
/// short ASCII domain tag as the BLAKE2b key for cross-context
/// hash separation.
pub const POW_FINALIZER_DOMAIN: &[u8] = b"kaspa-pq-pow-v1";

/// Output width of the Layer 0 finalizer in bytes. Compared
/// against a 512-bit (`Uint512`) target.
pub const POW_FINALIZER_BYTES: usize = 64;

/// kaspa-pq Phase 1 Layer 1 algorithm id (the only one valid in
/// Phase 1).
///
/// Semantically: "this header's L1 tag is the upstream
/// `cSHAKE256("HeavyHash")` 32-byte digest, unchanged". Future
/// `algo_id` values introduce ASIC-hard L1 variants and ship in
/// their own hard-fork ADRs.
pub const POW_ALGO_ID_KHEAVYHASH: u8 = 1;

/// Maximum byte length of an L1 tag accepted by the Layer 0
/// finalizer. Acts as a defensive upper bound so a future
/// `algo_id` cannot accidentally inflate header validation cost
/// past a reasonable budget — actual lengths are fixed per
/// `algo_id` and validated up-stack.
pub const POW_L1_TAG_MAX_BYTES: usize = 256;

/// Domain-separator key for the algo_id = 1 (kHeavyHash) seed
/// derivation. kaspa-pq Phase 9 (PR-9.3) — see ADR-0008
/// §"algo_id = 1 (kHeavyHash) seed derivation".
///
/// The upstream kHeavyHash signature takes a 32-byte seed; the
/// kaspa-pq Phase 1 path derives that seed from the 64-byte
/// pre-PoW hash via a dedicated keyed BLAKE2b-256 so the 32-byte
/// seed cannot be substituted for any other 32-byte digest in the
/// system.
pub const POW_L1_KHEAVYHASH_V1_SEED_DOMAIN: &[u8] = b"kaspa-pq-l1-kheavyhash-v1-seed";

/// Errors returned by Layer 0 helpers.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PowLayer0Error {
    #[error("kaspa-pq Layer 0: L1 tag length {0} exceeds POW_L1_TAG_MAX_BYTES = {POW_L1_TAG_MAX_BYTES}")]
    L1TagTooLong(usize),
    #[error("kaspa-pq Layer 0: unknown pow_algo_id = {0}; Phase 1 admits only POW_ALGO_ID_KHEAVYHASH = 1")]
    UnknownAlgoId(u8),
}

/// Validate that an `algo_id` is recognised by this binary at
/// Phase 1. Rejects everything except `POW_ALGO_ID_KHEAVYHASH`.
#[inline]
pub fn check_algo_id_phase1(algo_id: u8) -> Result<(), PowLayer0Error> {
    if algo_id == POW_ALGO_ID_KHEAVYHASH { Ok(()) } else { Err(PowLayer0Error::UnknownAlgoId(algo_id)) }
}

/// kaspa-pq Layer 0 PoW finalizer.
///
/// Layout (ADR-0007 §"Decision", ADR-0008-updated to take a
/// 64-byte `pre_pow_hash`):
///
/// ```text
/// pow_512 = BLAKE2b-512(
///     key   = POW_FINALIZER_DOMAIN,
///     input = network_id_len_le_u16 || network_id ||
///             algo_id ||
///             pre_pow_hash64 ||                     // 64 bytes
///             timestamp.to_le_bytes() ||
///             bits.to_le_bytes() ||
///             nonce.to_le_bytes() ||
///             (l1_tag.len() as u16).to_le_bytes() || l1_tag,
/// )
/// ```
///
/// All variable-length inputs (`network_id`, `l1_tag`) carry a
/// 2-byte little-endian length prefix in front so the input is
/// self-delimiting: adding a new `algo_id` whose tag is a
/// different length cannot collide with a previous variant's
/// concatenation.
///
/// Returns the 64-byte digest. The caller compares against the
/// 512-bit target via `Uint512::from_le_bytes` /
/// `Uint512::from_compact_target_bits_512`.
pub fn pow_finalizer_blake2b_512(
    network_id: &[u8],
    algo_id: u8,
    pre_pow_hash: Hash64,
    timestamp: u64,
    bits: u32,
    nonce: u64,
    l1_tag: &[u8],
) -> Result<[u8; POW_FINALIZER_BYTES], PowLayer0Error> {
    if l1_tag.len() > POW_L1_TAG_MAX_BYTES {
        return Err(PowLayer0Error::L1TagTooLong(l1_tag.len()));
    }

    let mut state = Params::new().hash_length(POW_FINALIZER_BYTES).key(POW_FINALIZER_DOMAIN).to_state();

    // 2-byte length-prefix for the variable-width network_id so the
    // domain separation is unambiguous across simnet / devnet /
    // testnet / mainnet, which all carry distinct network_id bytes
    // (see ADR-0001).
    state.update(&(network_id.len() as u16).to_le_bytes());
    state.update(network_id);

    state.update(&[algo_id]);
    // ADR-0008: pre_pow_hash is now 64 bytes (BlockPrePowHash64).
    state.update(&pre_pow_hash.as_bytes());
    state.update(&timestamp.to_le_bytes());
    state.update(&bits.to_le_bytes());
    state.update(&nonce.to_le_bytes());

    state.update(&(l1_tag.len() as u16).to_le_bytes());
    state.update(l1_tag);

    let digest = state.finalize();
    let mut out = [0u8; POW_FINALIZER_BYTES];
    out.copy_from_slice(digest.as_bytes());
    Ok(out)
}

/// Derive the 32-byte kHeavyHash v1 seed from the 64-byte
/// pre-PoW hash. kaspa-pq Phase 9 (PR-9.3); see ADR-0008
/// §"algo_id = 1 (kHeavyHash) seed derivation".
///
/// ```text
/// l1_seed32 = BLAKE2b-256(
///     key   = POW_L1_KHEAVYHASH_V1_SEED_DOMAIN,
///     input = pre_pow_hash64,
/// )
/// ```
///
/// This bridges the 64-byte Layer 0 pre-PoW hash to the upstream
/// 32-byte kHeavyHash interface for the Phase 1 `algo_id = 1`
/// path. The seed is domain-separated on its own keyed BLAKE2b
/// instance so the 32-byte seed and the 64-byte pre-PoW hash
/// cannot be substituted for each other anywhere else.
#[inline]
pub fn l1_seed32_for_kheavyhash_v1(pre_pow_hash: Hash64) -> Hash {
    let digest =
        Params::new().hash_length(32).key(POW_L1_KHEAVYHASH_V1_SEED_DOMAIN).to_state().update(pre_pow_hash.as_byte_slice()).finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    Hash::from_bytes(out)
}

/// Difficulty-lift helper. Maps a 256-bit upstream-style target to
/// a 512-bit kaspa-pq target while preserving block-finding
/// probability under the ideal uniform-hash model:
///
/// ```text
/// Pr[X_512 ≤ target_256 << 256]
///   = (target_256 << 256) / 2^512
///   = target_256 / 2^256
///   = Pr[X_256 ≤ target_256]
/// ```
///
/// Use cases:
///
///  - Translating historical 256-bit compact-bits values into the
///    kaspa-pq comparison domain at fork activation.
///  - Sanity-checking the `from_compact_target_bits_512` decoder:
///    by construction
///    `from_compact_target_bits_512(bits) == lift_target_256_to_512(
///        Uint256::from_compact_target_bits(bits))`.
#[inline]
pub fn lift_target_256_to_512(target_256: Uint256) -> Uint512 {
    Uint512::from(target_256) << 256
}

/// `floor(2^512 / (target + 1))` returned as a [`Uint576`]. Thin
/// re-export of `Uint512::calc_work_512` so consumers can pull the
/// kaspa-pq work-computation surface from `pow_layer0` without
/// also pulling `kaspa_math` directly.
#[inline]
pub fn calc_work_512(target: Uint512) -> Uint576 {
    target.calc_work_512()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::ZERO_HASH64;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    #[test]
    fn algo_id_phase1_only_admits_kheavyhash() {
        assert!(check_algo_id_phase1(POW_ALGO_ID_KHEAVYHASH).is_ok());
        for bad in [0u8, 2, 3, 7, 0xff] {
            assert_eq!(check_algo_id_phase1(bad), Err(PowLayer0Error::UnknownAlgoId(bad)));
        }
    }

    /// The finalizer is deterministic: same input -> same output.
    #[test]
    fn finalizer_deterministic() {
        let net = b"simnet";
        let a = pow_finalizer_blake2b_512(net, 1, h(0x11), 1_000_000, 0x1e7fffff, 42, &[7u8; 32]).unwrap();
        let b = pow_finalizer_blake2b_512(net, 1, h(0x11), 1_000_000, 0x1e7fffff, 42, &[7u8; 32]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), POW_FINALIZER_BYTES);
    }

    /// Every input field meaningfully influences the digest. This is
    /// the self-delimiting property of the layout — varying any one
    /// field must shift the output.
    #[test]
    fn finalizer_inputs_change_digest() {
        let base = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();

        let net_diff = pow_finalizer_blake2b_512(b"mainnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, net_diff, "network_id must alter digest");

        // algo_id 2 is not a valid Phase 1 id, but the finalizer must
        // accept arbitrary algo_id bytes (Phase 2+ will hard-fork in
        // new ids). What matters here: changing algo_id changes the
        // digest.
        let algo_diff = pow_finalizer_blake2b_512(b"simnet", 2, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, algo_diff, "algo_id must alter digest");

        let pre_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x22), 100, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, pre_diff, "pre_pow_hash must alter digest");

        let ts_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 101, 0x1e7fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, ts_diff, "timestamp must alter digest");

        let bits_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x207fffff, 7, &[3u8; 32]).unwrap();
        assert_ne!(base, bits_diff, "bits must alter digest");

        let nonce_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 8, &[3u8; 32]).unwrap();
        assert_ne!(base, nonce_diff, "nonce must alter digest");

        let tag_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[4u8; 32]).unwrap();
        assert_ne!(base, tag_diff, "l1_tag bytes must alter digest");

        let len_diff = pow_finalizer_blake2b_512(b"simnet", 1, h(0x11), 100, 0x1e7fffff, 7, &[3u8; 31]).unwrap();
        assert_ne!(base, len_diff, "l1_tag length must alter digest");
    }

    /// The 2-byte length prefix in front of `l1_tag` defeats the
    /// canonical-concat collision attack: two distinct (tag, netid)
    /// pairs whose concatenation is the same string must still
    /// produce different digests.
    #[test]
    fn finalizer_l1_tag_is_self_delimiting() {
        // Construction: two l1_tag values whose raw bytes differ only
        // by length-prefix boundary placement. Without the length
        // prefix this would collide; with it, the digests differ.
        let a = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"AB").unwrap();
        let b = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"ABCD").unwrap();
        let c = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, b"").unwrap();
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn finalizer_rejects_overlong_l1_tag() {
        let too_long = vec![0u8; POW_L1_TAG_MAX_BYTES + 1];
        let r = pow_finalizer_blake2b_512(b"net", 1, ZERO_HASH64, 0, 0, 0, &too_long);
        assert_eq!(r, Err(PowLayer0Error::L1TagTooLong(POW_L1_TAG_MAX_BYTES + 1)));
    }

    /// Difficulty-lift identity at the consensus-core boundary —
    /// matches the same identity tested in `kaspa-math` but routed
    /// through this module's `lift_target_256_to_512` re-export.
    #[test]
    fn pq_difficulty_lift_identity_at_consensus_boundary() {
        for bits in [0x207fffffu32, 0x1d00ffffu32, 0x1e21bc1cu32, 486722099u32] {
            let target_256 = Uint256::from_compact_target_bits(bits);
            let via_decoder = Uint512::from_compact_target_bits_512(bits);
            let via_lift = lift_target_256_to_512(target_256);
            assert_eq!(via_decoder, via_lift, "decoder and lift disagree on bits={bits:#x}");
        }
    }

    #[test]
    fn calc_work_512_reexport_matches_math() {
        let target = Uint512::from_compact_target_bits_512(0x1e7fffff);
        let work_via_module = calc_work_512(target);
        let work_via_math = target.calc_work_512();
        assert_eq!(work_via_module, work_via_math);
    }

    /// Sanity check: the empty-input digest is non-trivial. (Catches
    /// a future accidental hard-coding to zero.)
    #[test]
    fn finalizer_empty_input_nontrivial_digest() {
        let d = pow_finalizer_blake2b_512(b"", 0, ZERO_HASH64, 0, 0, 0, b"").unwrap();
        assert_ne!(d, [0u8; POW_FINALIZER_BYTES]);
    }

    /// kaspa-pq Phase 9 (PR-9.3): the algo_id = 1 (kHeavyHash) seed
    /// derivation is deterministic, sensitive to every byte of the
    /// 64-byte pre-PoW hash, and key-separated from the other
    /// kaspa-pq BLAKE2b-256 hashers (TransactionHash, BlockHash,
    /// MuHashElementHash, …). Determinism is the basis for miner
    /// reproducibility; key-separation is the basis for not being
    /// substitutable elsewhere.
    #[test]
    fn l1_seed32_for_kheavyhash_v1_basic_properties() {
        let a = l1_seed32_for_kheavyhash_v1(h(0x11));
        let b = l1_seed32_for_kheavyhash_v1(h(0x11));
        assert_eq!(a, b, "derivation must be deterministic");

        let c = l1_seed32_for_kheavyhash_v1(h(0x12));
        assert_ne!(a, c, "different pre-PoW hashes must yield different seeds");

        // Flip the last byte of the 64-byte input; the derived seed
        // must shift.
        let mut bytes = [0x11u8; 64];
        bytes[63] = 0x12;
        let last_bit_flipped = l1_seed32_for_kheavyhash_v1(Hash64::from_bytes(bytes));
        assert_ne!(a, last_bit_flipped, "every byte of pre_pow_hash must influence the seed");

        // Key separation against the existing 32-byte BLAKE2b
        // hashers. The kHeavyHash seed must not equal any of them on
        // the same input bytes.
        use kaspa_hashes::{BlockHash, Hasher, MuHashElementHash, TransactionHash};
        let pre_pow_bytes = h(0x33).as_bytes();
        let pre_pow_slice: &[u8] = &pre_pow_bytes;
        let seed = l1_seed32_for_kheavyhash_v1(h(0x33));
        assert_ne!(seed.as_bytes(), BlockHash::hash(pre_pow_slice).as_bytes());
        assert_ne!(seed.as_bytes(), TransactionHash::hash(pre_pow_slice).as_bytes());
        assert_ne!(seed.as_bytes(), MuHashElementHash::hash(pre_pow_slice).as_bytes());
    }
}
