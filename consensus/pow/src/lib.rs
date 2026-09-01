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
    palw_attempt_v2::{PALW_ATTEMPT_V2_L1_TAG_BYTES, PalwAttemptEnvelopeV2, challenge_v2, commitment_root_v2, l1_tag_v2},
    pow_layer0::{
        POW_ALGO_ID_ARGON2ID, POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_LLM,
        POW_ALGO_ID_PALW_OLLAMA, POW_ALGO_ID_PALW_RECEIPT_V3, POW_FINALIZER_BYTES, POW_L1_BLAKE2B_SHA3_OUT_BYTES,
        POW_L1_PALW_OLLAMA_OUT_BYTES, POW_L1_PALW_OUT_BYTES, POW_L1_TAG_MAX_BYTES, PowLayer0Error, argon2id_l1_tag_v1,
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
        Ok((passed, _)) if kaspa_consensus_core::pow_layer0::algo_id_derives_no_block_level(header.pow_algo_id) => (0, passed),
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
    /// The block's V2 attempt envelope, decoded once, when the header declares the committed-V2
    /// algo id (ADR-0042 Decision 3a, Unit A). This IS the wire carrier: `Header::palw_commitment`
    /// bytes on an algo-6 header are a `PAV2` envelope, and the algo-6 tag arm consumes
    /// `Expand(commitment_root)` from it INSTEAD of an inference — sound only because the same
    /// header's stateless admission recomputes the carried challenge from the header position and
    /// its stateful admission caps the bond's immature exposure. Neither check lives here; both
    /// are why the arm could not land alone. Decode failure is deferred to that arm
    /// (`PalwV2AttemptMissing`) so `new()` stays total — the pruning-proof path constructs states
    /// for peer-supplied headers before any shape gate ran.
    pub(crate) palw_attempt_v2: Option<PalwAttemptEnvelopeV2>,
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
        // **ADR-0066 Decision 1: the heartbeat lane is priced by a network constant, and its
        // `bits` are the GLOBAL expected bits like every other lane's.**
        //
        // Reading the price out of `bits` is what made the first implementation fatal: `bits` is
        // the field the difficulty window averages, so a window of heartbeat rows raised the
        // global demand to the lane's own price and no bonded block could re-enter the chain. A
        // fixed target has no feedback path — heartbeat rows enter the window as ordinary rows
        // carrying ordinary bits, and F1 and F3b are gone as quantities rather than as tunings.
        //
        // The substitution lives HERE, in the one place every PoW path goes through (ordinary
        // validation, the pruning proof, trusted import), so no caller can price the lane by
        // forgetting to.
        //
        // **ADR-0071 Decision 1 tried to extend this substitution to the ATTEMPT lane, and that
        // was wrong — see the ADR's own amendment.** The premise it served ("block-generation
        // weight must not depend on hash computation") was already satisfied: an algo-6 block's
        // blue work is the constant `PALW_ATTEMPT_BLUE_WORK_LOG2` (ADR-0068 Phase 1). Freezing the
        // TARGET as well took out something else entirely — the block interval — because
        // `retarget_over_span_v1` only ever redistributes cadence BETWEEN classes and says so:
        // "Block interval stays `DifficultyManager::calculate_difficulty_bits`'s job". On a
        // network where one class produces, that retarget is a deliberate no-op, so with `bits`
        // frozen nothing limited the rate at all. Measured on Relaunch 5: the floor produced ~50
        // blocks a minute against a 0.5/min target, flat for five minutes, and the public entry
        // node could never complete a review floor because it was IBD-ing continuously.
        let target_512 = if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1 {
            Uint512::MAX >> kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_WORK_LOG2
        } else {
            Uint512::from_compact_target_bits_512(header.bits)
        };
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
            // Same trust posture, other family, decoded per lane by the header's own declared
            // algorithm: the carriage magics are disjoint, so at most one of these could ever
            // succeed anyway — decoding only the declared one says which lane the header CLAIMS
            // to be in rather than discovering it. The algo-6 arm REQUIRES its envelope (an
            // algo-6 header without one has no work to check) and errors rather than expects,
            // because peer-supplied proof headers reach the finalizer before shape validation.
            palw_attempt_v2: (header.pow_algo_id == POW_ALGO_ID_PALW_COMMITTED_V2)
                .then(|| PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).ok())
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
    ///
    /// `pow_algo_id` is PEER-CONTROLLED, so this is a total function on it: an id this finalizer
    /// cannot verify returns `Err(UnknownAlgoId)` rather than assuming the caller filtered it.
    #[inline]
    fn calculate_l1_tag(&self, nonce: u64, buf: &mut [u8; POW_L1_TAG_MAX_BYTES]) -> Result<usize, PowLayer0Error> {
        match self.pow_algo_id {
            // ADR-0042 Decision 3a (algo_id = 6): the tag is `Expand(commitment_root_v2)` — a
            // cheap, total expansion of the carried attempt's identity, never an inference. The
            // work is priced by the attempt's life-cycle (bond, admission, panel, court), not by
            // this hash; what THIS arm guarantees is that a solved header attests exactly one
            // attempt at exactly this position:
            //
            // * the envelope must be present — an algo-6 header without one has no work to check;
            // * the carried `challenge` must equal the one this (pre_pow_hash, timestamp, nonce,
            //   class, bond) derives. The challenge is INSIDE the identity the root expands, so
            //   this equation is what forces a nonce move to be a new attempt (W2: one ticket,
            //   one inference). It is asked HERE and not only in `validate_stateless_v2` so that
            //   every path that computes PoW — the pruning-proof path included, which never
            //   reaches stateful admission — refuses an attempt re-mounted at another position.
            //
            // The recompute uses the CARRIED `network_domain`: the equation pins position/class/
            // bond to the attempt either way, cross-network replay is already refused by the
            // Layer-0 digest's own `network_id` binding, and domain-vs-network equality is
            // admission's stateless list (it needs the network's expected domain, which this
            // pure finalizer deliberately does not hold).
            //
            // A carriage that does not decode is not a failed tag, it is an unverifiable header:
            // the shape gate refuses it up-stack, and here a missing envelope is
            // `PalwV2AttemptMissing` rather than something silently tagged instead.
            //
            // This arm used to have a TWIN further down the match — same id, no challenge
            // recompute — which `match` made dead code. Dead in the safe direction (the strict
            // arm is first), but only by ordering: deleting the wrong one of two identical
            // patterns would have removed the position binding and left an attempt re-mountable
            // at any nonce. One arm, one rule.
            POW_ALGO_ID_PALW_COMMITTED_V2 => {
                let envelope = self.palw_attempt_v2.as_ref().ok_or(PowLayer0Error::PalwV2AttemptMissing)?;
                let attempt = &envelope.attempt;
                let expected = challenge_v2(
                    attempt.network_domain,
                    self.pre_pow_hash_64,
                    self.timestamp,
                    nonce,
                    attempt.class_id,
                    &attempt.executor_bond,
                );
                if attempt.challenge != expected {
                    return Err(PowLayer0Error::PalwV2ChallengeMismatch);
                }
                let tag = l1_tag_v2(commitment_root_v2(attempt));
                buf[..PALW_ATTEMPT_V2_L1_TAG_BYTES].copy_from_slice(&tag);
                Ok(PALW_ATTEMPT_V2_L1_TAG_BYTES)
            }
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
            // ADR-0066 Decision 1 (algo_id = 8): the heartbeat lane. Its tag IS algo-3's — the lane
            // is a self-verifying hash lane and wants exactly that function — and the two cannot
            // borrow each other's solutions anyway, because the Layer-0 finalizer binds
            // `pow_algo_id` into the digest. What differs is the TARGET (a network constant, set
            // in `new()`) and the acceptance (`Params::palw_heartbeat`), neither of which belongs
            // in a tag. Sharing the arm rather than copying it means the hash lane and the
            // heartbeat lane cannot drift into computing different things under one name.
            POW_ALGO_ID_BLAKE2B_SHA3 | kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1 => {
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
            // id, so these `expect`s are a constructor invariant — and they are only reachable now
            // because the arm is selected by an explicit id match instead of by falling through.
            POW_ALGO_ID_KHEAVYHASH => {
                let hasher = self.hasher.as_ref().expect("kHeavyHash StateLayer0 carries a PowHash");
                let matrix = self.matrix.as_ref().expect("kHeavyHash StateLayer0 carries a Matrix");
                let hash = hasher.clone().finalize_with_nonce(nonce);
                buf[..32].copy_from_slice(&matrix.heavy_hash(hash).as_bytes());
                Ok(32)
            }
            // Any other id is unverifiable by this finalizer, and it is peer-controlled input.
            //
            // The comment this replaces said any other id is "rejected up-stack by header
            // validation before PoW is ever computed". That is true of the ordinary header
            // pipeline and NOT true of the pruning-proof path, which computes the PoW of a
            // peer-supplied proof header before `check_algo_id` runs
            // (`pruning_proof/validate.rs`, `apply.rs`). Falling through to the kHeavyHash arm
            // there `expect`s a `None` that `new()` deliberately left empty for non-kHeavyHash
            // ids — a one-message remote panic, on hash-only mainnet as much as anywhere, with no
            // PALW required (mainnet-readiness audit P0-1).
            //
            // Total function instead: the caller maps this to a failed PoW and rejects the proof
            // (`calc_block_level_check_pow_layer0`'s `Err(_)` arm), which is the same verdict the
            // up-stack check would have produced, just without the crash.
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

    /// **The receipt lane skips the target comparison on purpose — this pins what makes that safe.**
    ///
    /// The ADR-0068 launch audit read `check_pow_layer0`'s algo-7 arm as an unpriced block lane and
    /// filed it as a launch blocker: one ML-DSA signature buys a header every node validates,
    /// stores and relays. The reading of the code was right and the verdict was wrong. ADR-0044
    /// Decision 6 stands and the operator confirmed it: a receipt block's work is a CERTIFIED
    /// QUANTUM, audited and paid for before the block exists, so the digest binds the header to
    /// that spend rather than pricing it — and comparing it to `bits` would be a filter its
    /// producer walks through for free while honest software stalls on it.
    ///
    /// This test exists because that is a decision an audit will keep rediscovering as a defect.
    /// What bounds the lane is elsewhere, in five places, and none of them is `bits`: the quantum
    /// lottery against the class's receipt target, the per-claim quanta cap and its spent set, the
    /// bond the spend is bound to, the use window, and the lane's **weightlessness**.
    ///
    /// **It is the last of those this file can check, and it is the one that matters here.** An
    /// unpriced lane that could buy chain position would be an attack; an unpriced lane that cannot
    /// is a receipt. So the skip stops being safe at the exact moment a receipt starts weighing
    /// something, and that is what fails here. (The arm itself needs a real spend envelope to
    /// reach — a digest cannot be computed without one — so it is exercised where envelopes are
    /// built, not here.)
    #[test]
    fn the_receipt_lanes_freedom_from_bits_rests_on_its_weightlessness() {
        // The lane the audit was worried about buys nothing: no fork-choice weight…
        assert!(
            kaspa_consensus_core::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_RECEIPT_V3),
            "a receipt buys no chain position — the moment it does, skipping the target becomes an attack"
        );
        // …and no pruning-proof hierarchy: a free digest must never be sold as structure.
        assert!(kaspa_consensus_core::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_RECEIPT_V3));

        // Asserted as a DIFFERENCE against the lane that IS priced by its target, because a test
        // that only checked the receipt side would pass just as well if every lane had gone
        // weightless — which would be a far worse defect than the one it is guarding.
        assert!(!kaspa_consensus_core::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(!kaspa_consensus_core::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_COMMITTED_V2));

        // And the target comparison is alive for the lanes that are priced by it: an impossible
        // target refuses, so the algo-7 arm is a lane-specific decision rather than a comparison
        // that quietly stopped working.
        let impossible = 0x0100_0001u32;
        let hashed = dummy_header_algo(impossible, 7, 1_700_000_000_000, POW_ALGO_ID_BLAKE2B_SHA3);
        let (admitted, _) =
            StateLayer0::new(&hashed, b"misaka-receipt-lane-test").check_pow_layer0(7).expect("the hash digest computes");
        assert!(!admitted, "a priced lane must still be refused by its own target");
    }

    /// **ADR-0066 Decision 1, at the one place that decides it: a heartbeat's target is the
    /// network constant and `header.bits` does not move it.**
    ///
    /// The integration test in the consensus crate runs with `skip_proof_of_work()`, so it can
    /// show that a heartbeat CARRIES the global bits but not that those bits are ignored when the
    /// work is checked. That is the half the withdrawn design got wrong — the price lived in the
    /// field the difficulty window averages — so it is asserted here directly.
    ///
    /// Two headers, identical but for `bits`: a trivially easy one and the hardest the compact
    /// encoding can state. Both must produce the SAME target, and it must be the constant.
    #[test]
    fn a_heartbeat_target_is_the_network_constant_whatever_bits_it_declares() {
        use kaspa_consensus_core::pow_layer0::{PALW_HEARTBEAT_WORK_LOG2, POW_ALGO_ID_HEARTBEAT_V1};

        let expected = Uint512::MAX >> PALW_HEARTBEAT_WORK_LOG2;
        let easy = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_HEARTBEAT_V1);
        let hard = dummy_header_algo(0x01010101, 0, 1_700_000_000, POW_ALGO_ID_HEARTBEAT_V1);
        assert_eq!(StateLayer0::new(&easy, b"testnet-10").target_512, expected, "trivial bits do not make the lane cheap");
        assert_eq!(StateLayer0::new(&hard, b"testnet-10").target_512, expected, "hard bits do not make it expensive either");

        // The neighbouring lane still reads its bits, so this is a substitution for ONE id and not
        // a change to how targets work.
        let sha3 = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_BLAKE2B_SHA3);
        assert_eq!(
            StateLayer0::new(&sha3, b"testnet-10").target_512,
            Uint512::from_compact_target_bits_512(0x207fffff),
            "every other lane is still priced by its own declared bits"
        );
        assert_ne!(StateLayer0::new(&sha3, b"testnet-10").target_512, expected);

        // And the price is real: the constant target is far harder than the trivial bits a V2
        // network runs at, which is what makes sibling flooding cost something.
        assert!(expected < Uint512::from_compact_target_bits_512(0x207fffff), "the lane is harder than the ambient target");
    }

    /// A peer-supplied header carrying an algo id this finalizer does not implement must be a
    /// REJECTED header, not a dead node.
    ///
    /// Mainnet-readiness audit P0-1, and it lands on a hash-only network. The ordinary header
    /// pipeline does run `check_algo_id` before the PoW, but the pruning-proof path does not
    /// (`pruning_proof/validate.rs` and `apply.rs` compute the PoW of a peer-supplied proof header
    /// first), so an id nobody implements reaches this finalizer straight off the wire. It used to
    /// fall through to the kHeavyHash arm and `expect` the `Some(PowHash)`/`Some(Matrix)` that
    /// `new()` leaves `None` for every non-kHeavyHash id: one message, any peer, node down.
    ///
    /// Asserted through `calc_block_level_check_pow_layer0` because that is the exact call the
    /// proof path makes — testing `calculate_l1_tag` alone would not prove the crash is
    /// unreachable from where it was actually reached.
    #[test]
    fn an_unknown_algo_id_is_a_rejected_header_not_a_panic() {
        // Every id this build does not implement, including the ones a later phase reserves.
        //
        // The list is smaller than on the hash-only lineage this test came from, and deliberately:
        // ids 4, 5, 6 and 7 ARE implemented here (PALW LLM, Ollama, committed-V2, receipt-V3), so
        // asserting they are unknown would assert the opposite of what this build does. **8 left
        // the list with ADR-0066 Decision 1** for the same reason — it is the heartbeat lane and
        // its tag arm is algo-3's. What the test is FOR is unchanged: the tag function must be
        // total on a peer-controlled id, and the ids below are the ones that reach the unknown arm.
        for algo_id in [0u8, 9, 10, 42, 200, u8::MAX] {
            let header = dummy_header_algo(0x207fffff, 1, 1_000_000, algo_id);

            let (level, passes) = calc_block_level_check_pow_layer0(&header, b"mainnet", 255);
            assert!(!passes, "algo_id {algo_id}: an unverifiable header must not pass PoW");
            assert_eq!(level, 0, "algo_id {algo_id}: an unverifiable header must not claim a block level");

            // And the finalizer says why, rather than assuming its caller filtered the id.
            let state = StateLayer0::new(&header, b"mainnet");
            let mut buf = [0u8; POW_L1_TAG_MAX_BYTES];
            assert_eq!(
                state.calculate_l1_tag(7, &mut buf),
                Err(PowLayer0Error::UnknownAlgoId(algo_id)),
                "algo_id {algo_id}: the tag function must be total on a peer-controlled id"
            );
        }
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
        let n = s.calculate_l1_tag(7, &mut buf).expect("algo_id 2 is a known id");
        let expect = argon2id_l1_tag_v1(s.pre_pow_hash_64, 7, b"testnet-10");
        assert_eq!(&buf[..n], expect.as_slice(), "algo_id=2 must compute the Argon2id L1 tag");

        // ...and differ from the kHeavyHash path for the same input.
        let kh = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_KHEAVYHASH);
        let s_kh = StateLayer0::new(&kh, b"testnet-10");
        let mut buf_kh = [0u8; POW_L1_TAG_MAX_BYTES];
        let n_kh = s_kh.calculate_l1_tag(7, &mut buf_kh).expect("algo_id 1 is a known id");
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
        let n = s.calculate_l1_tag(11, &mut buf).expect("algo_id 3 is a known id");
        assert_eq!(n, POW_L1_BLAKE2B_SHA3_OUT_BYTES, "BLAKE2b-SHA3 tag is 128 bytes");
        let expect = blake2b_sha3_l1_tag_v1(s.pre_pow_hash_64, 11, b"testnet-10");
        assert_eq!(&buf[..n], expect.as_slice(), "algo_id=3 must compute the BLAKE2b-SHA3 L1 tag");

        // ...and differ from the kHeavyHash path for the same input.
        let kh = dummy_header_algo(0x207fffff, 0, 1_700_000_000, POW_ALGO_ID_KHEAVYHASH);
        let s_kh = StateLayer0::new(&kh, b"testnet-10");
        let mut buf_kh = [0u8; POW_L1_TAG_MAX_BYTES];
        let n_kh = s_kh.calculate_l1_tag(11, &mut buf_kh).expect("algo_id 1 is a known id");
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
        // ADR-0044 Decision 6), and **8 left it with ADR-0066 Decision 1** (the heartbeat lane,
        // whose tag arm is algo-3's). An algo-6 header without an envelope is the NAMED error
        // `PalwV2AttemptMissing` (covered by `palw_v2_commitment_mutation_invalidates_pow`), and
        // both lanes are covered below by
        // `a_v2_lineage_header_without_its_carriage_is_a_failed_pow_not_a_panic`, which pins the
        // property this test actually exists for: a peer-controlled header must never panic the
        // finalizer, whatever it declares.
        for bad in [0u8, 9, 42, 128, 200, 255] {
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

    /// A V2 attempt whose challenge matches `(header position, nonce, class, bond)`, carried the
    /// way a real block carries it: in `Header::palw_commitment`, PAV2 wire form.
    fn v2_envelope_for(header: &Header, nonce: u64) -> PalwAttemptEnvelopeV2 {
        use kaspa_consensus_core::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN};
        let net = Hash64::from_u64_word(0x7E57_00D0);
        let bond = kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([3u8; 64]), 1);
        let class = Hash64::from_u64_word(0xC1A55);
        let pph = hashing::header::pre_pow_hash_64(header);
        let attempt = kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2 {
            version: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            network_domain: net,
            challenge: challenge_v2(net, pph, header.timestamp, nonce, class, &bond),
            class_id: class,
            executor_bond: bond,
            executor_pubkey: vec![7u8; STAKE_VALIDATOR_PUBKEY_LEN],
            operator_id: Hash64::from_u64_word(0x0E0),
            artifact_root: Hash64::from_u64_word(0xA7),
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x07),
            pwu: 4_242,
            trace_manifest_root: Hash64::from_u64_word(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: 1_000_000,
            execution_root: Hash64::from_u64_word(0x41),
        };
        PalwAttemptEnvelopeV2 { attempt, signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN] }
    }

    /// **The audit's P0-1 / C1 red test, by its registered name** (`docs/palw-rc-threat-model.md`):
    /// the finalizer arm, the wire carrier and this test land together, and together they make
    /// "mutating any one bit of the commitment fails the PoW" (ADR-0042 Decision 3a) a checked
    /// property instead of an intention.
    ///
    /// What "fails the PoW" means per bucket, deterministically:
    /// * **content fields** — the Layer-0 digest MOVES (the found solution attests only the found
    ///   attempt; at any real target a moved digest is a failed PoW with overwhelming probability,
    ///   and asserting movement instead of a target miss keeps the test flake-free);
    /// * **position/challenge fields, and the position itself** — the arm REFUSES outright
    ///   (`PalwV2ChallengeMismatch`): re-mounting an attempt at another (nonce, timestamp) or
    ///   under another class/bond is not a different digest, it is not a PoW at all;
    /// * **a missing or undecodable envelope** — `PalwV2AttemptMissing`, never a panic, because
    ///   the pruning-proof path reaches this code on peer input before any shape gate;
    /// * **the signature** — the digest does NOT move. The witness is deliberately outside the
    ///   priced identity (ADR-0042 Decision 3c); the raw-bytes block-identity rule for the field
    ///   is retained for now, so a third party who flips a signature bit produces a DIFFERENT
    ///   block id that dies alone at admission instead of poisoning the honest block's id — see
    ///   the threat-model register's Decision-3c note for why 3c-as-written must not land naively.
    #[test]
    fn palw_v2_commitment_mutation_invalidates_pow() {
        use kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2;

        const TS: u64 = 1_700_000_000;
        // A fixture, not a constant of the protocol: it must SOLVE the (very easy) target below,
        // and the digest moves whenever the envelope's encoding does — as it did when
        // `PALW_ATTEMPT_V2_VERSION` went 2 → 3, and again at 4 → 5. If this test starts failing on
        // "the solved header passes at its target", the envelope changed and this needs re-picking,
        // not the code under test.
        const NONCE: u64 = 3;
        const BITS: u32 = 0x207fffff;
        let network_id: &[u8] = b"simnet";

        let mut header = dummy_header_algo(BITS, NONCE, TS, POW_ALGO_ID_PALW_COMMITTED_V2);
        let base = v2_envelope_for(&header, NONCE);
        header.palw_commitment = base.encode_wire();

        let state = StateLayer0::new(&header, network_id);
        let digest0 = state.calculate_pow_layer0(NONCE).expect("a carried, position-consistent envelope computes a digest");
        assert!(state.check_pow_layer0(NONCE).unwrap().0, "the solved header passes at its target");

        let digest_for = |envelope: &PalwAttemptEnvelopeV2| {
            let mut h = dummy_header_algo(BITS, NONCE, TS, POW_ALGO_ID_PALW_COMMITTED_V2);
            h.palw_commitment = envelope.encode_wire();
            StateLayer0::new(&h, network_id).calculate_pow_layer0(NONCE)
        };

        // Exhaustive destructuring: adding a field to the attempt breaks THIS LINE until the new
        // field is placed in one of the two buckets below — the drift that re-opened P0-1 in
        // PR-06 (identity-visible, PoW-invisible fields) cannot recur silently.
        let PalwAttemptUnsignedV2 {
            version: _,
            network_domain: _,
            challenge: _,
            class_id: _,
            executor_bond: _,
            executor_pubkey: _,
            operator_id: _,
            artifact_root: _,
            trace_root: _,
            output_root: _,
            pwu: _,
            trace_manifest_root: _,
            trace_chunk_count: _,
            trace_retention_daa: _,
            execution_root: _,
        } = base.attempt.clone();

        // Bucket 1 — content fields: every one moves the digest, so the found solution does not
        // transfer to any sibling attempt.
        let content: Vec<(&str, Box<dyn Fn(&mut PalwAttemptUnsignedV2)>)> = vec![
            ("version", Box::new(|a| a.version = a.version.wrapping_add(1))),
            ("executor_pubkey", Box::new(|a| a.executor_pubkey[0] ^= 0xFF)),
            ("operator_id", Box::new(|a| a.operator_id = Hash64::from_u64_word(0x0FF1CE))),
            ("artifact_root", Box::new(|a| a.artifact_root = Hash64::from_u64_word(0xA27))),
            ("trace_root", Box::new(|a| a.trace_root = Hash64::from_u64_word(0xDEAD))),
            ("output_root", Box::new(|a| a.output_root = Hash64::from_u64_word(0xBEEF))),
            ("pwu", Box::new(|a| a.pwu += 1)),
            ("trace_manifest_root", Box::new(|a| a.trace_manifest_root = Hash64::from_u64_word(0x1AA1))),
            ("trace_chunk_count", Box::new(|a| a.trace_chunk_count += 1)),
            ("trace_retention_daa", Box::new(|a| a.trace_retention_daa += 1)),
            ("execution_root", Box::new(|a| a.execution_root = Hash64::from_u64_word(0xE7))),
        ];
        for (field, mutate) in content {
            let mut env = base.clone();
            mutate(&mut env.attempt);
            assert_ne!(env.attempt, base.attempt, "the {field} mutation must actually change the attempt");
            let digest = digest_for(&env).unwrap_or_else(|e| panic!("content field {field} must still compute, got {e}"));
            assert_ne!(digest, digest0, "mutating {field} left the Layer-0 digest unchanged — the solution transferred");
        }

        // Bucket 2 — challenge-equation fields: the arm itself refuses, on every path that
        // computes PoW, stateful admission reached or not.
        let positional: Vec<(&str, Box<dyn Fn(&mut PalwAttemptUnsignedV2)>)> = vec![
            ("network_domain", Box::new(|a| a.network_domain = Hash64::from_u64_word(0x9999))),
            ("challenge", Box::new(|a| a.challenge = Hash64::from_u64_word(0x1234))),
            ("class_id", Box::new(|a| a.class_id = Hash64::from_u64_word(0xC2))),
            (
                "executor_bond",
                Box::new(|a| a.executor_bond = kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([9u8; 64]), 2)),
            ),
        ];
        for (field, mutate) in positional {
            let mut env = base.clone();
            mutate(&mut env.attempt);
            assert_eq!(
                digest_for(&env),
                Err(PowLayer0Error::PalwV2ChallengeMismatch),
                "mutating {field} must be refused by the challenge equation, not left to a digest mismatch"
            );
        }

        // The position itself: the same envelope re-mounted at another nonce or timestamp is not
        // a cheaper try, it is not a PoW at all. (One envelope = one ticket, W2.)
        assert_eq!(state.calculate_pow_layer0(NONCE + 1), Err(PowLayer0Error::PalwV2ChallengeMismatch));
        let mut moved = dummy_header_algo(BITS, NONCE, TS + 1, POW_ALGO_ID_PALW_COMMITTED_V2);
        moved.palw_commitment = base.encode_wire();
        assert_eq!(StateLayer0::new(&moved, network_id).calculate_pow_layer0(NONCE), Err(PowLayer0Error::PalwV2ChallengeMismatch));

        // Presence: an algo-6 header with an empty, garbage or wrong-family carrier has no work
        // to check — a named error and a failed proof header, never a panic.
        for (what, bytes) in [
            ("empty", Vec::new()),
            ("garbage", vec![0xAB; 64]),
            ("wrong family (PBC1 magic)", {
                let mut b = b"PBC1".to_vec();
                b.extend_from_slice(&base.encode_wire()[4..]);
                b
            }),
        ] {
            let mut h = dummy_header_algo(BITS, NONCE, TS, POW_ALGO_ID_PALW_COMMITTED_V2);
            h.palw_commitment = bytes;
            assert_eq!(
                StateLayer0::new(&h, network_id).calculate_pow_layer0(NONCE),
                Err(PowLayer0Error::PalwV2AttemptMissing),
                "carrier case: {what}"
            );
            assert_eq!(
                calc_block_level_check_pow_layer0(&h, network_id, 100),
                (0, false),
                "carrier case {what} must be a failed PoW at level 0, never a panic"
            );
        }

        // The signature is a witness, not identity: flipping it moves NEITHER the digest (it is
        // outside `attempt_id`, Decision 3c) — so one inference cannot be re-priced by re-signing —
        // nor the PoW verdict. Its block-identity handling is the register's Decision-3c note.
        let mut resigned = base.clone();
        resigned.signature[0] ^= 0xFF;
        assert_eq!(
            digest_for(&resigned).expect("a re-signed envelope still computes"),
            digest0,
            "the signature must stay outside the priced identity"
        );

        // And the honest move: a NEW nonce with a re-derived envelope is a new ticket with a new
        // digest — the W2 cost model in one assertion.
        let mut fresh_header = dummy_header_algo(BITS, NONCE + 1, TS, POW_ALGO_ID_PALW_COMMITTED_V2);
        let fresh = v2_envelope_for(&fresh_header, NONCE + 1);
        fresh_header.palw_commitment = fresh.encode_wire();
        let fresh_digest = StateLayer0::new(&fresh_header, network_id)
            .calculate_pow_layer0(NONCE + 1)
            .expect("a re-derived envelope at the new position computes");
        assert_ne!(fresh_digest, digest0, "a new ticket is a new digest");
    }
}
