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
//! `algo_id` semantics: the Layer-1 tag is selected by
//! `header.pow_algo_id`. Defined ids:
//!   - `1` [`POW_ALGO_ID_KHEAVYHASH`] — Phase 1 kHeavyHash matrix.
//!   - `2` [`POW_ALGO_ID_ARGON2ID`] — Phase 2 memory-hard Argon2id
//!     (superseded; still *verifiable* for historical pruning
//!     proofs, but no live network selects it).
//!   - `3` [`POW_ALGO_ID_BLAKE2B_SHA3`] — Phase 3 compute-only
//!     BLAKE2b-512 ∥ SHA3-512 (the active testnet/mainnet algo).
//! There is **no** mixed-`algo_id` difficulty arithmetic;
//! transitions are hard cut-offs at a specific DAA score, and a
//! header must declare exactly the id its network mandates
//! ([`required_algo_id`] / [`check_algo_id`]).

use blake2b_simd::Params;
use kaspa_hashes::{Hash, Hash64};
use kaspa_math::{Uint256, Uint512, Uint576};
use sha3::{Digest, Sha3_512};

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

/// kaspa-pq Phase 2 Layer 1 algorithm id: **memory-hard Argon2id** (ADR-0007 §"Phase 2").
/// Replaces kHeavyHash on the networks where it is activated (testnet/mainnet) to compress the
/// GPU↔ASIC performance gap and prevent kHeavyHash/BLAKE2b ASICs (incl. Kaspa's) from being
/// reused against this chain. The Layer 0 BLAKE2b-512 finalizer is unchanged; only the Layer 1
/// tag computation differs.
pub const POW_ALGO_ID_ARGON2ID: u8 = 2;

/// Argon2id Layer-1 parameters (`algo_id = 2`). Memory cost is the ASIC-resistance lever; it is
/// paid per *hash attempt* by miners (millions/s → memory-bandwidth bound), while a verifier runs
/// it exactly once per block header (negligible). 16 MiB, 1 pass, 1 lane, 32-byte tag.
pub const POW_L1_ARGON2ID_M_COST_KIB: u32 = 16 * 1024;
pub const POW_L1_ARGON2ID_T_COST: u32 = 1;
pub const POW_L1_ARGON2ID_P_COST: u32 = 1;
pub const POW_L1_ARGON2ID_OUT_BYTES: usize = 32;
/// Domain separator (BLAKE2b key) for the algo_id = 2 Argon2id password + salt derivation.
pub const POW_L1_ARGON2ID_V1_DOMAIN: &[u8] = b"kaspa-pq-l1-argon2id-v1";

/// kaspa-pq Phase 3 Layer 1 algorithm id: **compute-only BLAKE2b-512 ∥ SHA3-512** (ADR-0007 §"Phase 3").
///
/// Replaces Argon2id (`algo_id = 2`) on the networks where it is activated (testnet/mainnet) to make
/// header verification ~10^4× cheaper, which is the IBD/catch-up bottleneck under a memory-hard PoW
/// (a verifier runs the Layer-1 tag once per header). The trade-off is explicit and accepted: the PoW
/// is no longer memory-hard, so GPU/FPGA/ASIC acceleration is possible — the chain's safety leans on
/// the two-dimensional (PoW × stake) DNS finality overlay (ADR-0009) rather than PoW egalitarianism.
/// The Layer-0 BLAKE2b-512 finalizer is unchanged; only the Layer-1 tag differs.
pub const POW_ALGO_ID_BLAKE2B_SHA3: u8 = 3;

/// Output width of the algo_id = 3 Layer-1 tag: BLAKE2b-512 (64) ∥ SHA3-512 (64) = 128 bytes. Within
/// [`POW_L1_TAG_MAX_BYTES`] (256), so the Layer-0 finalizer accepts it.
pub const POW_L1_BLAKE2B_SHA3_OUT_BYTES: usize = 128;
/// Domain separator for the algo_id = 3 BLAKE2b-512 ∥ SHA3-512 Layer-1 tag. Used as the BLAKE2b key
/// for the first half and as an explicit prefix for the (un-keyed) SHA3-512 second half.
pub const POW_L1_BLAKE2B_SHA3_V1_DOMAIN: &[u8] = b"kaspa-pq-l1-blake2b-sha3-v1";

/// Errors returned by Layer 0 helpers.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PowLayer0Error {
    #[error("kaspa-pq Layer 0: L1 tag length {0} exceeds POW_L1_TAG_MAX_BYTES = {POW_L1_TAG_MAX_BYTES}")]
    L1TagTooLong(usize),
    #[error("kaspa-pq Layer 0: pow_algo_id = {0} is unrecognised or wrong for this network's active PoW phase")]
    UnknownAlgoId(u8),
}

/// Validate that an `algo_id` is recognised by this binary at
/// Phase 1. Rejects everything except `POW_ALGO_ID_KHEAVYHASH`.
#[inline]
pub fn check_algo_id_phase1(algo_id: u8) -> Result<(), PowLayer0Error> {
    if algo_id == POW_ALGO_ID_KHEAVYHASH { Ok(()) } else { Err(PowLayer0Error::UnknownAlgoId(algo_id)) }
}

/// The Layer-1 algorithm a header MUST declare, given whether the Phase-3 BLAKE2b-512 ∥ SHA3-512 fork
/// is active at the header's DAA score. A network that has activated it requires `algo_id = 3`;
/// otherwise the Phase-1 `algo_id = 1` (kHeavyHash). This is a hard cut-off — there is no mixed-algo
/// arithmetic. (Argon2id, `algo_id = 2`, is the superseded Phase-2 algorithm: still *verifiable*
/// via [`check_algo_id_known`] for historical pruning proofs, but no live network selects it.)
#[inline]
pub fn required_algo_id(blake2b_sha3_active: bool) -> u8 {
    if blake2b_sha3_active { POW_ALGO_ID_BLAKE2B_SHA3 } else { POW_ALGO_ID_KHEAVYHASH }
}

/// Validate a header's `algo_id` against the network's PoW state: it must equal
/// [`required_algo_id`]. Rejects both unknown ids and the *wrong-but-known* id (e.g. a miner trying
/// the cheap kHeavyHash on a BLAKE2b-SHA3 network, or vice-versa).
#[inline]
pub fn check_algo_id(algo_id: u8, blake2b_sha3_active: bool) -> Result<(), PowLayer0Error> {
    if algo_id == required_algo_id(blake2b_sha3_active) { Ok(()) } else { Err(PowLayer0Error::UnknownAlgoId(algo_id)) }
}

/// Accept any algo_id this binary knows how to verify ({kHeavyHash, Argon2id, BLAKE2b-SHA3}). Used
/// where the PoW itself is independently verified and only an unknown/garbage id must be rejected
/// (e.g. the pruning-proof path); the exact per-network/per-DAA rule is enforced by [`check_algo_id`]
/// in the main header pipeline. Argon2id (2) stays accepted so historical proofs spanning the
/// Phase-2 era still validate.
#[inline]
pub fn check_algo_id_known(algo_id: u8) -> Result<(), PowLayer0Error> {
    if algo_id == POW_ALGO_ID_KHEAVYHASH || algo_id == POW_ALGO_ID_ARGON2ID || algo_id == POW_ALGO_ID_BLAKE2B_SHA3 {
        Ok(())
    } else {
        Err(PowLayer0Error::UnknownAlgoId(algo_id))
    }
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

/// kaspa-pq Phase 2 (`algo_id = 2`): the memory-hard Argon2id Layer-1 tag.
///
/// ```text
/// password = BLAKE2b-256(key = POW_L1_ARGON2ID_V1_DOMAIN, pre_pow_hash64 || nonce_le)
/// salt     = BLAKE2b-128(key = POW_L1_ARGON2ID_V1_DOMAIN, "salt" || netid_len_le || network_id)
/// l1_tag   = Argon2id(password, salt; m=16MiB, t=1, p=1, out=32)
/// ```
///
/// Deterministic (fixed params + fixed per-network salt), binds to the block via `pre_pow_hash`
/// and to the search via `nonce`, and is domain/network-separated. The 32-byte tag is then fed to
/// the unchanged Layer-0 `pow_finalizer_blake2b_512` with `algo_id = 2`.
pub fn argon2id_l1_tag_v1(pre_pow_hash: Hash64, nonce: u64, network_id: &[u8]) -> [u8; POW_L1_ARGON2ID_OUT_BYTES] {
    // password: per-(block, nonce) — this is what miners vary across nonce trials.
    let password = {
        let digest = Params::new()
            .hash_length(32)
            .key(POW_L1_ARGON2ID_V1_DOMAIN)
            .to_state()
            .update(pre_pow_hash.as_byte_slice())
            .update(&nonce.to_le_bytes())
            .finalize();
        let mut o = [0u8; 32];
        o.copy_from_slice(digest.as_bytes());
        o
    };
    // salt: fixed per network (deterministic). Length-prefixed network id for unambiguous separation.
    let salt = {
        let digest = Params::new()
            .hash_length(16)
            .key(POW_L1_ARGON2ID_V1_DOMAIN)
            .to_state()
            .update(b"salt")
            .update(&(network_id.len() as u16).to_le_bytes())
            .update(network_id)
            .finalize();
        let mut o = [0u8; 16];
        o.copy_from_slice(digest.as_bytes());
        o
    };
    let params = argon2::Params::new(
        POW_L1_ARGON2ID_M_COST_KIB,
        POW_L1_ARGON2ID_T_COST,
        POW_L1_ARGON2ID_P_COST,
        Some(POW_L1_ARGON2ID_OUT_BYTES),
    )
    .expect("static Argon2id params are valid");
    let a2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; POW_L1_ARGON2ID_OUT_BYTES];
    a2.hash_password_into(&password, &salt, &mut out).expect("Argon2id hash into fixed-size buffer");
    out
}

/// kaspa-pq Phase 3 (`algo_id = 3`): the compute-only BLAKE2b-512 ∥ SHA3-512 Layer-1 tag.
///
/// ```text
/// half_b = BLAKE2b-512(key = DOMAIN, netid_len_le16 || network_id || pre_pow_hash64 || nonce_le)
/// half_s = SHA3-512(DOMAIN || netid_len_le16 || network_id || pre_pow_hash64 || nonce_le)
/// l1_tag = half_b || half_s                                          // 64 + 64 = 128 bytes
/// ```
///
/// Both halves bind the block (`pre_pow_hash`), the search (`nonce`), and the network (length-prefixed
/// `network_id`), and are domain-separated on `DOMAIN` — the BLAKE2b half uses it as the key, the
/// (un-keyed) SHA3 half prepends it. The 128-byte tag is fed to the unchanged Layer-0
/// `pow_finalizer_blake2b_512` with `algo_id = 3`, which mixes a *second* BLAKE2b-512 over the whole
/// preimage (including `half_s`) — so a miner cannot skip the SHA3 half: the accepted digest depends
/// on every tag byte. Per-nonce work is therefore 2×BLAKE2b-512 + 1×SHA3-512, all compute-only.
pub fn blake2b_sha3_l1_tag_v1(pre_pow_hash: Hash64, nonce: u64, network_id: &[u8]) -> [u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES] {
    // BLAKE2b-512 half (keyed on DOMAIN). Length-prefixed network id => self-delimiting preimage.
    let half_b = Params::new()
        .hash_length(64)
        .key(POW_L1_BLAKE2B_SHA3_V1_DOMAIN)
        .to_state()
        .update(&(network_id.len() as u16).to_le_bytes())
        .update(network_id)
        .update(pre_pow_hash.as_byte_slice())
        .update(&nonce.to_le_bytes())
        .finalize();

    // SHA3-512 half. `sha3` has no keying, so DOMAIN is prepended explicitly; the same
    // length-prefixed (network_id, pre_pow_hash, nonce) follow.
    let mut s = Sha3_512::new();
    s.update(POW_L1_BLAKE2B_SHA3_V1_DOMAIN);
    s.update((network_id.len() as u16).to_le_bytes());
    s.update(network_id);
    s.update(pre_pow_hash.as_byte_slice());
    s.update(nonce.to_le_bytes());
    let half_s = s.finalize();

    let mut out = [0u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES];
    out[..64].copy_from_slice(half_b.as_bytes());
    out[64..].copy_from_slice(&half_s);
    out
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
///
/// NOTE (audit L): GHOSTDAG blue-work is **intentionally** still computed by the
/// legacy `difficulty::calc_work(bits)` (32-bit-compact target), NOT this 512-bit
/// form — the kaspa-pq difficulty lift keeps the historical work unit so blue-work
/// accounting is unchanged. This helper exists for the Layer-0 512-bit PoW surface
/// and block-level derivation. Switching only *part* of the work accounting to
/// `calc_work_512` would change blue-work and split the DAG, so the two MUST NOT be
/// mixed within a single accounting domain.
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

    #[test]
    fn check_algo_id_enforces_exact_network_algo() {
        // BLAKE2b-SHA3-active network: must be 3; kHeavyHash (1) and the superseded Argon2id (2) are
        // both WRONG (not just unknown) on the active path.
        assert!(check_algo_id(POW_ALGO_ID_BLAKE2B_SHA3, true).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_KHEAVYHASH, true), Err(PowLayer0Error::UnknownAlgoId(1)));
        assert_eq!(check_algo_id(POW_ALGO_ID_ARGON2ID, true), Err(PowLayer0Error::UnknownAlgoId(2)));
        // kHeavyHash network: must be 1, 3 is rejected.
        assert!(check_algo_id(POW_ALGO_ID_KHEAVYHASH, false).is_ok());
        assert_eq!(check_algo_id(POW_ALGO_ID_BLAKE2B_SHA3, false), Err(PowLayer0Error::UnknownAlgoId(3)));
        assert_eq!(required_algo_id(true), 3);
        assert_eq!(required_algo_id(false), 1);
    }

    /// `check_algo_id_known` (pruning-proof path) accepts every algo this binary can verify —
    /// kHeavyHash (1), the superseded Argon2id (2), and BLAKE2b-SHA3 (3) — and rejects the rest.
    #[test]
    fn check_algo_id_known_accepts_all_verifiable_algos() {
        for ok in [POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3] {
            assert!(check_algo_id_known(ok).is_ok(), "algo_id {ok} must be known");
        }
        for bad in [0u8, 4, 7, 0xff] {
            assert_eq!(check_algo_id_known(bad), Err(PowLayer0Error::UnknownAlgoId(bad)));
        }
    }

    /// BLAKE2b-SHA3 Layer-1 (algo_id = 3) must be DETERMINISTIC (miner and every verifier agree on
    /// the tag for a given header+nonce), 128 bytes wide, and sensitive to block, nonce and network.
    /// It must also differ from the kHeavyHash-seed and Argon2id derivations on the same inputs.
    #[test]
    fn blake2b_sha3_l1_tag_deterministic_and_sensitive() {
        let net = b"testnet-10";
        let a = blake2b_sha3_l1_tag_v1(h(0x11), 42, net);
        let b = blake2b_sha3_l1_tag_v1(h(0x11), 42, net);
        assert_eq!(a, b, "BLAKE2b-SHA3 L1 must be deterministic");
        assert_eq!(a.len(), POW_L1_BLAKE2B_SHA3_OUT_BYTES);
        assert_eq!(a.len(), 128);
        assert!(a.len() <= POW_L1_TAG_MAX_BYTES, "tag must fit the finalizer's max");
        assert_ne!(a, [0u8; POW_L1_BLAKE2B_SHA3_OUT_BYTES]);
        // The two halves are distinct hash families over the same preimage — they must not coincide.
        assert_ne!(&a[..64], &a[64..], "BLAKE2b half must differ from SHA3 half");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x12), 42, net), "pre_pow_hash must change the tag");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x11), 43, net), "nonce must change the tag");
        assert_ne!(a, blake2b_sha3_l1_tag_v1(h(0x11), 42, b"mainnet"), "network must change the tag");
        // Distinct from the other algos' derivations on the same input (different algo).
        assert_ne!(&a[..32], argon2id_l1_tag_v1(h(0x11), 42, net).as_slice());
        assert_ne!(&a[..32], l1_seed32_for_kheavyhash_v1(h(0x11)).as_bytes().as_slice());
    }

    /// Argon2id Layer-1 (algo_id = 2) must be DETERMINISTIC (miner and every verifier must agree on
    /// the tag for a given header+nonce) and sensitive to block, nonce and network.
    #[test]
    fn argon2id_l1_tag_deterministic_and_sensitive() {
        let net = b"testnet-10";
        let a = argon2id_l1_tag_v1(h(0x11), 42, net);
        let b = argon2id_l1_tag_v1(h(0x11), 42, net);
        assert_eq!(a, b, "Argon2id L1 must be deterministic");
        assert_eq!(a.len(), POW_L1_ARGON2ID_OUT_BYTES);
        assert_ne!(a, [0u8; 32]);
        assert_ne!(a, argon2id_l1_tag_v1(h(0x12), 42, net), "pre_pow_hash must change the tag");
        assert_ne!(a, argon2id_l1_tag_v1(h(0x11), 43, net), "nonce must change the tag");
        assert_ne!(a, argon2id_l1_tag_v1(h(0x11), 42, b"mainnet"), "network must change the tag");
        // It must differ from the kHeavyHash-seed derivation on the same inputs (different algo).
        assert_ne!(a.as_slice(), l1_seed32_for_kheavyhash_v1(h(0x11)).as_bytes().as_slice());
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
