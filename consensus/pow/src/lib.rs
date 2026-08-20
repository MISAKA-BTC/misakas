// public for benchmarks
#[doc(hidden)]
pub mod matrix;
pub mod palw;
pub mod palw_admission;
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
        POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2,
        POW_ALGO_ID_PALW_LLM, POW_ALGO_ID_PALW_OLLAMA, POW_ALGO_ID_PALW_RECEIPT_V3, POW_FINALIZER_BYTES, POW_L1_BLAKE2B_SHA3_OUT_BYTES, POW_L1_PALW_OLLAMA_OUT_BYTES, POW_L1_PALW_OUT_BYTES, POW_L1_TAG_MAX_BYTES,
        PowLayer0Error, argon2id_l1_tag_v1, blake2b_sha3_l1_tag_v1, l1_seed32_for_kheavyhash_v1, pow_finalizer_blake2b_512,
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
    // Through the SHARED predicate, so that any gate which must run before this function can ask the
    // same question and cannot drift from it. Inlining `parents_by_level.is_empty()` here is what let
    // the pruning-proof gate exempt a header shape whose PoW still ran (see the predicate's docs).
    if kaspa_consensus_core::pow_layer0::pow_short_circuits_as_parentless_root(header) {
        return (max_block_level, true); // Genesis has the max block level
    }

    let state = StateLayer0::new(header, network_id);
    match state.check_pow_layer0(header.nonce) {
        // ADR-0044 Decision 6: a receipt header's digest is free to re-roll, so deriving a BLOCK
        // LEVEL from it would sell hierarchy position — the pruning-proof structure — for the
        // price of one signature. Receipt blocks sit at the base level; the level hierarchy is
        // built by the attempt lane, whose digests are inference-priced. (This is the same
        // reasoning as the `passed` arm above, applied to the other thing a digest buys.)
        Ok((passed, _)) if header.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3 => (0, passed),
        Ok((passed, pow_512)) => (calc_level_from_pow_512(pow_512, max_block_level), passed),
        // `PalwWorkerFailed` is a statement about THIS node: it has a registered model runtime
        // and the runtime broke, persistently — the driver's bounded retries absorb the transient
        // half (spawn failure, OOM kill, timeout under validation load; mainnet-readiness audit
        // B7, ADR-0036 Decision 4), so what reaches here is a runtime this node genuinely cannot
        // use. These errors are ENVIRONMENTAL, never header-dependent (the prompt is a fixed
        // frame around the seed, far under the ceiling): on a legacy PALW network, returning
        // `false` would silently reject every valid block — stall the node, ban honest peers,
        // fork it off alone — so a node that OPTED INTO the legacy lane still fails loud.
        //
        // A node that never registers a runtime — every V2 node and every hash-network node, per
        // ADR-0042 Decision 4 — cannot reach this arm: with nothing registered, the tag path
        // answers `PalwUnavailable` below before any runtime exists to fail. The panic is scoped
        // to the one deployment that asked for a model, precisely because it asked.
        Err(e @ PowLayer0Error::PalwWorkerFailed(_)) => {
            panic!("PALW PoW validation cannot run on this node: {e}")
        }
        // `PalwUnavailable` — no runtime registered in this process, or one that is configured
        // off this network's class — prices the header as failed PoW (ADR-0042 Decision 4, PR-02:
        // a full node without a model is the NORMAL case, not a fault). That verdict is correct
        // everywhere it can be reached: a network whose required-algo rule demands an
        // inference-priced id refuses to boot a kaspad without a verified runtime (the startup
        // rail), the algo gates reject such headers up-stack on every other network — including
        // the pruning-proof path, whose shape gate runs `check_algo_id` BEFORE this function —
        // and a library consumer that grinds without configuring a runtime mints nothing rather
        // than minting tags no peer accepts.
        //
        // Remaining variants are finalizer-internal misuse, which cannot happen for a well-formed
        // header; also a failed PoW.
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
    /// The block's PALW commitment, decoded once, when the header carries one.
    ///
    /// Present only where the commitment fence is open — `check_palw_commitment_shape` requires an
    /// EMPTY `palw_commitment` on every network whose fence is shut, so this is `None` everywhere
    /// today and the tag path below is byte-identical to before it existed.
    pub(crate) palw_commitment: Option<kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentV1>,
    /// ADR-0042 Decision 3 (Unit A): the V2 attempt envelope, decoded once, on an algo-6 header.
    /// The finalizer consumes `Expand(commitment_root)` from it INSTEAD of an inference — which
    /// is sound only because the same header's stateless admission recomputes the carried
    /// challenge from the header position and its stateful admission caps the bond's immature
    /// exposure. Neither check lives here; both are why the arm could not land alone.
    pub(crate) palw_attempt_v2: Option<kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2>,
    /// ADR-0044 Decision 6 (Unit B): the free-prompt spend envelope on an algo-7 header.
    pub(crate) palw_spend_v3: Option<kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3>,
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
            // Decoded, never trusted: a header whose bytes do not decode carries no commitment to
            // bind to, and admission refuses it separately. Silently binding nothing would be the
            // dangerous reading, so the tag only changes when a commitment is actually present.
            palw_commitment: kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentV1::decode(&header.palw_commitment).ok(),
            // Decoded per lane, by the header's own declared algorithm: the three carriage magics
            // are disjoint, so at most one of these could ever succeed anyway — decoding only the
            // declared one says which lane the header claims to be in rather than discovering it.
            palw_attempt_v2: (header.pow_algo_id == POW_ALGO_ID_PALW_COMMITTED_V2)
                .then(|| kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode(&header.palw_commitment).ok())
                .flatten(),
            palw_spend_v3: (header.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3)
                .then(|| kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment).ok())
                .flatten(),
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
    /// grind hot loop allocation-free (no per-nonce heap `Vec`).
    ///
    /// Two arms can fail: the PALW arms (they reach an external runtime) and the unknown-id arm,
    /// which returns `UnknownAlgoId` rather than `expect`ing the kHeavyHash state that
    /// `StateLayer0::new` never populated for it. Every *hash* arm is infallible.
    #[inline]
    fn calculate_l1_tag(&self, nonce: u64, buf: &mut [u8; POW_L1_TAG_MAX_BYTES]) -> Result<usize, PowLayer0Error> {
        match self.pow_algo_id {
            // Phase 4b (algo_id = 5): one deterministic Ollama inference over the same seed;
            // the tag commits to the greedy response bytes + counts. 72 bytes.
            POW_ALGO_ID_PALW_OLLAMA => {
                let tag = palw::palw_ollama_l1_tag(self.pre_pow_hash_64, self.timestamp, nonce, &self.network_id)?;
                buf[..POW_L1_PALW_OLLAMA_OUT_BYTES].copy_from_slice(&tag);
                Ok(POW_L1_PALW_OLLAMA_OUT_BYTES)
            }
            // Phase 4 (algo_id = 4): one deterministic pinned-LLM inference over the seed derived
            // from (network, pre_pow_hash, timestamp, nonce). 200 bytes.
            POW_ALGO_ID_PALW_LLM => {
                let tag = palw::palw_l1_tag(self.pre_pow_hash_64, self.timestamp, nonce, &self.network_id)?;
                // Audit P0-1: the block identity hash covers `palw_commitment` and every PoW-path
                // digest excludes it, so a miner who solved once could swap the trace root, the
                // output root or the executor bond and mint sibling blocks on the SAME PoW. Binding
                // the commitment into the tag closes that — one bit of it moves the root, the root
                // moves the tag, the tag moves the digest, and the PoW fails.
                //
                // The inference stays the work. `PalwBlockCommitmentV1::l1_tag_bytes` would REPLACE
                // it with a free CPU expansion, which is the W1 change and must not land before a
                // bond's immature exposure is capped in consensus (audit P0-10) — free tags plus
                // uncapped exposure is what makes fake-root grinding cheap.
                match self.palw_commitment.as_ref() {
                    Some(commitment) => {
                        let challenge = commitment.challenge_for(&self.network_id, self.pre_pow_hash_64, self.timestamp, nonce);
                        let bound = kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentV1::bind_l1_tag_v1(
                            &tag,
                            commitment.commitment_root(challenge),
                        );
                        buf[..bound.len()].copy_from_slice(&bound);
                        Ok(bound.len())
                    }
                    None => {
                        buf[..POW_L1_PALW_OUT_BYTES].copy_from_slice(&tag);
                        Ok(POW_L1_PALW_OUT_BYTES)
                    }
                }
            }
            // ADR-0042 Decision 3a (algo_id = 6, Unit A): the finalizer consumes an EXPANSION of
            // the attempt's commitment root instead of an inference. One new ticket still costs
            // one new inference, but that is enforced by the pieces around this arm, not by it:
            // the commitment root binds the challenge, the challenge binds the header position,
            // and stateless admission recomputes it — so a miner cannot re-announce one solved
            // position, and per-bond exposure (stateful) is what prices a FAKE trace root.
            //
            // A carriage that does not decode is not a failed tag, it is an unverifiable header:
            // the shape gate refuses it up-stack, and here it maps to `PalwCarriageMissing` rather
            // than silently tagging something else.
            POW_ALGO_ID_PALW_COMMITTED_V2 => {
                let envelope = self.palw_attempt_v2.as_ref().ok_or(PowLayer0Error::PalwCarriageMissing(self.pow_algo_id))?;
                let root = kaspa_consensus_core::palw_attempt_v2::commitment_root_v2(&envelope.attempt);
                let tag = kaspa_consensus_core::palw_attempt_v2::l1_tag_v2(root);
                buf[..tag.len()].copy_from_slice(&tag);
                Ok(tag.len())
            }
            // ADR-0044 Decision 6 (algo_id = 7, Unit B): `Expand(spend_id)`. The tag is IDENTITY
            // binding, not a lottery — see `check_pow_layer0`, which is where the difference is
            // load-bearing.
            POW_ALGO_ID_PALW_RECEIPT_V3 => {
                let envelope = self.palw_spend_v3.as_ref().ok_or(PowLayer0Error::PalwCarriageMissing(self.pow_algo_id))?;
                let id = kaspa_consensus_core::palw_freeprompt_v3::fp_spend_id_v3(&envelope.spend);
                let tag = kaspa_consensus_core::palw_freeprompt_v3::fp_spend_l1_tag_v3(id);
                buf[..tag.len()].copy_from_slice(&tag);
                Ok(tag.len())
            }
            // Phase 3 (algo_id = 3): compute-only BLAKE2b-512 ∥ SHA3-512 over (pre_pow_hash, nonce). 128 bytes.
            POW_ALGO_ID_BLAKE2B_SHA3 => {
                buf[..POW_L1_BLAKE2B_SHA3_OUT_BYTES].copy_from_slice(&blake2b_sha3_l1_tag_v1(
                    self.pre_pow_hash_64,
                    nonce,
                    &self.network_id,
                ));
                Ok(POW_L1_BLAKE2B_SHA3_OUT_BYTES)
            }
            // Phase 2 (algo_id = 2): memory-hard Argon2id over (pre_pow_hash, nonce). 32 bytes.
            POW_ALGO_ID_ARGON2ID => {
                buf[..32].copy_from_slice(&argon2id_l1_tag_v1(self.pre_pow_hash_64, nonce, &self.network_id));
                Ok(32)
            }
            // Phase 1 (algo_id = 1, kHeavyHash). `new()` populates hasher+matrix exactly for this
            // id, so these `expect`s are a constructor invariant — NOT peer-reachable, because the
            // arm is now selected by an explicit id match rather than by falling through.
            POW_ALGO_ID_KHEAVYHASH => {
                let hasher = self.hasher.as_ref().expect("kHeavyHash StateLayer0 carries a PowHash");
                let matrix = self.matrix.as_ref().expect("kHeavyHash StateLayer0 carries a Matrix");
                let hash = hasher.clone().finalize_with_nonce(nonce);
                buf[..32].copy_from_slice(&matrix.heavy_hash(hash).as_bytes());
                Ok(32)
            }
            // Any other id is unverifiable by this finalizer, and it is peer-controlled input.
            //
            // This MUST be a returned error, not an `expect()` on the (absent, `None`) kHeavyHash
            // state. The pruning-proof path computes PoW on peer-supplied proof headers BEFORE the
            // up-stack `check_algo_id` runs (`pruning_proof/validate.rs`), so an `expect` here is a
            // one-message remote panic on any network including hash-only mainnet — no PALW
            // required (mainnet-readiness audit P0-1). Total function: the caller maps this to a
            // failed PoW / rejected proof (`calc_block_level_check_pow_layer0`'s `Err(_)` arm).
            other => Err(PowLayer0Error::UnknownAlgoId(other)),
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
        if self.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3 {
            // ADR-0044 Decision 6: a receipt block's work is a CERTIFIED QUANTUM, already audited
            // and already paid for; this digest exists to bind the header to that spend, not to
            // price it. Comparing it to `bits` would be a filter its producer walks through for
            // free — nothing in a receipt header costs anything to re-roll — while honest
            // software stalled on it. The lottery is the quantum ticket against the class's
            // receipt target, in `check_palw_receipt_spend_admission_v3` item 5, and only there.
            //
            // Returning the digest unchanged keeps the caller's shape; what the caller must NOT
            // do with it is derive a block level (see `calc_block_level_check_pow_layer0`).
            return Ok((true, pow_512));
        }
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
    ///
    /// The network id is `devnet`, the `NetworkId` display form consensus passes down, because the
    /// fixture is honored on devnet ONLY — `kaspa_pow::palw::fixture_permitted_on`. This test used
    /// to say `kaspa-devnet`, which is not a network this codebase names anywhere; it passed
    /// because the fixture was selected by the variable alone.
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
        let s = StateLayer0::new(&h, b"devnet");

        // Dispatch: the verifier's internal L1 tag must equal the fixture tag for the seed the
        // verifier is contractually bound to derive — (network, pre_pow_hash, timestamp, nonce).
        let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
        let n = s.calculate_l1_tag(5, &mut buf).unwrap();
        assert_eq!(n, POW_L1_PALW_OUT_BYTES, "PALW tag is 200 bytes");
        let seed = palw_pow_seed_v1(s.pre_pow_hash_64, 1_700_000_000, 5, b"devnet");
        assert_eq!(&buf[..n], palw_fixture_l1_tag_v1(&seed).as_slice(), "algo_id=4 must compute the PALW fixture tag");

        // Determinism + nonce sensitivity of the full Layer-0 digest.
        let a = s.calculate_pow_layer0(5).unwrap();
        assert_eq!(a, s.calculate_pow_layer0(5).unwrap(), "PALW Layer-0 digest must be deterministic");
        assert_ne!(a, s.calculate_pow_layer0(6).unwrap(), "nonce must change the PALW digest");

        // Timestamp sensitivity THROUGH THE TAG: two headers differing only in timestamp share a
        // pre-PoW hash prefix computation but must produce different L1 tags (the seed binds the
        // timestamp) — this is what closes the free timestamp-grinding dimension.
        let h2 = dummy_header_algo(0x207fffff, 0, 1_700_000_001, POW_ALGO_ID_PALW_LLM);
        let s2 = StateLayer0::new(&h2, b"devnet");
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

    /// Without the fixture env and without a registered model runtime, judging a PALW header is
    /// `PalwUnavailable` — and `calc_block_level_check_pow_layer0` prices that as a failed PoW
    /// rather than a panic (ADR-0042 Decision 4: a full node without a model is the normal case).
    ///
    /// kaspa-pow cannot even LINK the crate that would answer (`no_model_runtime_edge.rs` pins
    /// the dependency graph), so unlike the pre-PR-02 version of this test, no developer
    /// environment — a stray `PALW_WORKER`, a live Ollama — can make a real runtime answer here.
    /// The outcome is exact, not conditional on the machine.
    #[test]
    fn layer0_palw_without_worker_is_unavailable_not_a_failed_pow() {
        use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_LLM;
        let _env = PALW_ENV_LOCK.lock().unwrap();
        let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_PALW_LLM);
        let s = StateLayer0::new(&h, b"simnet");
        match s.check_pow_layer0(0) {
            Err(PowLayer0Error::PalwUnavailable(msg)) => {
                assert!(msg.contains("no PALW model runtime"), "the error must name the missing runtime: {msg}")
            }
            other => panic!("expected PalwUnavailable, got {other:?}"),
        }
        // And the consensus wrapper's verdict on the same header: failed PoW, level 0 — never a
        // panic, never an accept.
        let (level, passed) = calc_block_level_check_pow_layer0(&h, b"simnet", 64);
        assert_eq!((level, passed), (0, false), "an unregistered runtime must price the header as failed PoW");
    }

    /// Mainnet-readiness audit **P0-1**: a header with an unrecognised `pow_algo_id` must never
    /// panic the finalizer.
    ///
    /// Before the fix, `calculate_l1_tag`'s catch-all arm served double duty (kHeavyHash *and*
    /// default) and `expect()`ed the `hasher`/`matrix` that `StateLayer0::new` leaves `None` for any
    /// non-kHeavyHash id — so a single peer-supplied pruning-proof header carrying e.g.
    /// `pow_algo_id = 7` crashed the node, on every network, PALW entirely uninvolved. The
    /// pruning-proof path computes PoW on peer headers before the up-stack `check_algo_id`, so the
    /// only safe shape is a returned error the consensus wrapper maps to a failed PoW.
    #[test]
    fn layer0_unknown_algo_id_is_error_not_panic() {
        // 0 and everything outside the implemented set is unknown; include the type extremes.
        //
        // 6 and 7 LEFT this list when Units A/B landed their tag arms (ADR-0042 Decision 3a,
        // ADR-0044 Decision 6). They are covered below by
        // `a_v2_lineage_header_without_its_carriage_is_a_failed_pow_not_a_panic`, which pins the
        // property this test actually exists for: a peer-controlled header must never panic the
        // finalizer, whatever it declares.
        for bad in [0u8, 8, 42, 128, 200, 255] {
            let h = dummy_header_algo(0x207fffff, 0, 1_700_000_000, bad);
            // Build the verifier on a hash-only network to prove no PALW machinery is needed to
            // trigger (or to survive) the crash.
            let s = StateLayer0::new(&h, b"mainnet");

            let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
            assert!(
                matches!(s.calculate_l1_tag(0, &mut buf), Err(PowLayer0Error::UnknownAlgoId(id)) if id == bad),
                "calculate_l1_tag must reject pow_algo_id={bad} with UnknownAlgoId, not panic",
            );
            assert!(
                matches!(s.calculate_pow_layer0(0), Err(PowLayer0Error::UnknownAlgoId(id)) if id == bad),
                "calculate_pow_layer0 must propagate UnknownAlgoId for pow_algo_id={bad}",
            );
            assert!(
                matches!(s.check_pow_layer0(0), Err(PowLayer0Error::UnknownAlgoId(id)) if id == bad),
                "check_pow_layer0 must propagate UnknownAlgoId for pow_algo_id={bad}",
            );

            // The consensus entry the pruning-proof path actually calls MUST NOT panic: it reports a
            // failed PoW at level 0 so the proof is rejected rather than the node crashing.
            let (level, passes) = calc_block_level_check_pow_layer0(&h, b"mainnet", 100);
            assert_eq!((level, passes), (0, false), "unknown algo id {bad} must be a failed PoW at level 0, never a panic");
        }
    }

    /// The predicate-drift regression, found by an adversarial verifier's proof-of-concept after the
    /// first pruning-proof gate shipped.
    ///
    /// `Header::direct_parents()` reads `parents_by_level[0]` and returns `&[]` when that run exists
    /// but is empty, whereas the PoW short-circuit asks `parents_by_level.is_empty()`. For
    /// `parents_by_level == [[]]` the two disagree: the gate called such a header "parentless" and
    /// skipped `check_algo_id`, while the finalizer still ran — so `algo_id = 4` reached the PALW arm
    /// on a worker-less node and panicked it, the exact P0-1 trigger (b) the gate exists to close.
    ///
    /// Both sides now ask [`pow_short_circuits_as_parentless_root`]. This pins the predicate itself,
    /// which is what the drift was about; the gate's use of it is covered in `kaspa-consensus`.
    #[test]
    fn the_parentless_predicate_does_not_drift_from_the_pow_short_circuit() {
        use kaspa_consensus_core::pow_layer0::pow_short_circuits_as_parentless_root;

        // `parents_by_level == [[]]`: a level-0 run that EXISTS and is EMPTY.
        let empty_level0 = Header::new_finalized(
            1,
            vec![vec![]].try_into().unwrap(),
            ZERO_HASH64,
            ZERO_HASH64,
            ZERO_HASH64,
            1_700_000_000,
            0x207fffff,
            0,
            POW_ALGO_ID_KHEAVYHASH,
            0,
            BlueWorkType::from_u64(0),
            0,
            ZERO_HASH64,
        );
        // The trap: `direct_parents()` calls this parentless...
        assert!(empty_level0.direct_parents().is_empty(), "direct_parents() reports parentless for [[]]");
        // ...but the PoW does NOT short-circuit, so a gate must NOT exempt it.
        assert!(
            !pow_short_circuits_as_parentless_root(&empty_level0),
            "[[]] must NOT be treated as a parentless root: its PoW runs, so gates must check it"
        );

        // A header with real parents: PoW runs, no exemption.
        let with_parents = dummy_header(0x207fffff, 0, 1_700_000_000);
        assert!(!with_parents.direct_parents().is_empty());
        assert!(!pow_short_circuits_as_parentless_root(&with_parents));

        // The genuine parentless root — no levels at all — is the one case that short-circuits, and
        // `calc_block_level_check_pow_layer0` must return max level without touching the finalizer
        // (proved here by using an algo id that would otherwise error).
        let genesis = Header::new_finalized(
            1,
            Vec::<Vec<kaspa_hashes::Hash64>>::new().try_into().unwrap(),
            ZERO_HASH64,
            ZERO_HASH64,
            ZERO_HASH64,
            1_700_000_000,
            0x207fffff,
            0,
            200, // an unknown id: reaching the finalizer would return UnknownAlgoId, not max level
            0,
            BlueWorkType::from_u64(0),
            0,
            ZERO_HASH64,
        );
        assert!(pow_short_circuits_as_parentless_root(&genesis));
        assert_eq!(
            calc_block_level_check_pow_layer0(&genesis, b"mainnet", 100),
            (100, true),
            "the true parentless root short-circuits to max level without running the finalizer"
        );
    }
}
