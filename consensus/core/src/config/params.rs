pub use super::{
    bps::{Bps, TenBps},
    constants::consensus::*,
    genesis::{DEVNET_GENESIS, GENESIS, GenesisBlock, SIMNET_GENESIS, TESTNET_GENESIS, TESTNET11_GENESIS},
};
use crate::{
    BlockLevel, BlueWorkType, KType,
    constants::{SOMPI_PER_KASPA, STORAGE_MASS_PARAMETER},
    dns_finality::{
        DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE, DnsParams, DnsReorgMode, FeeSplitParams, MAX_ATTESTATIONS_PER_SHARD,
        RewardParams, STAKE_SCORE_SCALE, StakeScore,
    },
    network::{NetworkId, NetworkType},
    token::TokenParams,
    vlt::VltParams,
};
/// Domain separator for [`Params::consensus_params_id`]. Versioned so a future encoding change is
/// a deliberate, visible break rather than a silent one.
const CONSENSUS_FINGERPRINT_DOMAIN_V1: &[u8] = b"misaka/consensus-fingerprint/v1";

use kaspa_addresses::Prefix;
use kaspa_hashes::{ConsensusParamsId, Hash};
use kaspa_math::{Uint256, Uint576};
use serde::{Deserialize, Serialize};
use std::{
    cmp::min,
    ops::{Deref, DerefMut},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkActivation(u64);

impl ForkActivation {
    const NEVER: u64 = u64::MAX;
    const ALWAYS: u64 = 0;

    pub const fn new(daa_score: u64) -> Self {
        Self(daa_score)
    }

    pub const fn never() -> Self {
        Self(Self::NEVER)
    }

    pub const fn always() -> Self {
        Self(Self::ALWAYS)
    }

    /// Returns the actual DAA score triggering the activation. Should be used only
    /// for cases where the explicit value is required for computations (e.g., coinbase subsidy).
    /// Otherwise, **activation checks should always go through `self.is_active(..)`**
    pub fn daa_score(self) -> u64 {
        self.0
    }

    pub fn is_active(self, current_daa_score: u64) -> bool {
        current_daa_score >= self.0
    }

    /// Checks if the fork was "recently" activated, i.e., in the time frame of the provided range.
    /// This function returns false for forks that were always active, since they were never activated.
    pub fn is_within_range_from_activation(self, current_daa_score: u64, range: u64) -> bool {
        self != Self::always() && self.is_active(current_daa_score) && current_daa_score < self.0 + range
    }

    /// Checks if the fork is expected to be activated "soon", i.e., in the time frame of the provided range.
    /// Returns the distance from activation if so, or `None` otherwise.  
    pub fn is_within_range_before_activation(self, current_daa_score: u64, range: u64) -> Option<u64> {
        if !self.is_active(current_daa_score) && current_daa_score + range > self.0 { Some(self.0 - current_daa_score) } else { None }
    }
}

/// A consensus parameter which depends on forking activation
#[derive(Clone, Copy, Debug)]
pub struct ForkedParam<T: Copy> {
    pre: T,
    post: T,
    activation: ForkActivation,
}

impl<T: Copy> ForkedParam<T> {
    const fn new(pre: T, post: T, activation: ForkActivation) -> Self {
        Self { pre, post, activation }
    }

    pub const fn new_const(val: T) -> Self {
        Self { pre: val, post: val, activation: ForkActivation::never() }
    }

    pub fn activation(&self) -> ForkActivation {
        self.activation
    }

    pub fn get(&self, daa_score: u64) -> T {
        if self.activation.is_active(daa_score) { self.post } else { self.pre }
    }

    /// Returns the value before activation (=pre unless activation = always)
    pub fn before(&self) -> T {
        match self.activation.0 {
            ForkActivation::ALWAYS => self.post,
            _ => self.pre,
        }
    }

    /// Returns the permanent long-term value after activation (=post unless the activation is never scheduled)
    pub fn after(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            _ => self.post,
        }
    }

    /// Maps the ForkedParam<T> to a new ForkedParam<U> by applying a map function on both pre and post
    pub fn map<U: Copy, F: Fn(T) -> U>(&self, f: F) -> ForkedParam<U> {
        ForkedParam::new(f(self.pre), f(self.post), self.activation)
    }
}

impl<T: Copy + Ord> ForkedParam<T> {
    /// Returns the min of `pre` and `post` values. Useful for non-consensus initializations
    /// which require knowledge of the value bounds.
    ///
    /// Note that if activation is not scheduled (set to never) then pre is always returned,
    /// and if activation is set to always (since inception), post will be returned.
    pub fn lower_bound(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            ForkActivation::ALWAYS => self.post,
            _ => self.pre.min(self.post),
        }
    }

    /// Returns the max of `pre` and `post` values. Useful for non-consensus initializations
    /// which require knowledge of the value bounds.
    ///
    /// Note that if activation is not scheduled (set to never) then pre is always returned,
    /// and if activation is set to always (since inception), post will be returned.
    pub fn upper_bound(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            ForkActivation::ALWAYS => self.post,
            _ => self.pre.max(self.post),
        }
    }
}

/// Blockrate-related consensus params.
/// Grouped together under a single struct because they are logically related and
/// in order to easily support **future BPS acceleration hardforks** (by simply adding
/// a forked instance of blockrate params to the main [`Params`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockrateParams {
    pub target_time_per_block: u64, // (milliseconds)
    pub ghostdag_k: KType,
    pub past_median_time_sample_rate: u64,
    pub difficulty_sample_rate: u64,
    pub max_block_parents: u8,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
    pub finality_depth: u64,
    pub pruning_depth: u64,
    pub coinbase_maturity: u64,
}

impl BlockrateParams {
    pub const fn new<const BPS: u64>() -> Self {
        Self {
            target_time_per_block: Bps::<BPS>::target_time_per_block(),
            ghostdag_k: Bps::<BPS>::ghostdag_k(),
            past_median_time_sample_rate: Bps::<BPS>::past_median_time_sample_rate(),
            difficulty_sample_rate: Bps::<BPS>::difficulty_adjustment_sample_rate(),
            max_block_parents: Bps::<BPS>::max_block_parents(),
            mergeset_size_limit: Bps::<BPS>::mergeset_size_limit(),
            merge_depth: Bps::<BPS>::merge_depth_bound(),
            finality_depth: Bps::<BPS>::finality_depth(),
            pruning_depth: Bps::<BPS>::pruning_depth(),
            coinbase_maturity: Bps::<BPS>::coinbase_maturity(),
        }
    }

    pub const fn increase_max_block_parents(mut self, max_block_parents: u8) -> Self {
        if self.max_block_parents < max_block_parents {
            self.max_block_parents = max_block_parents;
        }
        self
    }

    /// Blockrate params for the **0.1-bps** (one block per 10 seconds) PALW LLM-PoW network.
    ///
    /// `Bps<const BPS: u64>` cannot express a sub-integer rate, so the same formulas are spelled
    /// out here evaluated at λ = 0.1, with every per-block sample rate floored at 1
    /// (a rate below one block is "sample every block"):
    ///
    /// * `target_time_per_block` = 1000 / 0.1 = 10_000 ms.
    /// * `ghostdag_k` = `calculate_ghostdag_k(2·NETWORK_DELAY_BOUND·0.1 = 1.0, GHOSTDAG_TAIL_DELTA)`
    ///   = 4 (asserted against the real function in `deci_bps_constants_match_formulas`).
    /// * `past_median_time_sample_rate` = max(1, ⌊0.1 · PAST_MEDIAN_TIME_SAMPLE_INTERVAL⌋) = 1;
    ///   the 27-sample median window then spans 270 s ≈ the 264 s deviation window it models.
    /// * `difficulty_sample_rate` = max(1, ⌊0.1 · DIFFICULTY_WINDOW_SAMPLE_INTERVAL⌋) = 1.
    /// * `max_block_parents` = 10, `mergeset_size_limit` = 180 — both formula floors (k/2 = 2 and
    ///   2k = 8 are far below them).
    /// * `merge_depth` = 0.1 · MERGE_DEPTH_DURATION = 360, `finality_depth` = 0.1 ·
    ///   FINALITY_DURATION = 4_320, `pruning_depth` = 0.1 · PRUNING_DURATION = 10_800 (the
    ///   prunality lower bound at these values is 7_930, below the duration term).
    /// * `coinbase_maturity` = 0.1 · COINBASE_MATURITY_SECONDS = 10.
    ///
    /// Wall-clock durations (12 h finality, 30 h pruning, 100 s maturity) are exactly the
    /// 10-bps network's — only the block-unit denominators shrink 100×.
    pub const fn new_deci_bps() -> Self {
        Self::new_seconds_per_block(10, 4)
    }

    /// Blockrate params for the **120 s/block** PALW public testnet (decided 2026-08-12 from
    /// fleet measurements; see docs/history/testnet10-palw-rollout-runbook.md §"Why 120 s").
    ///
    /// The block interval on a network whose PoW is an LLM inference is not a UX preference: a
    /// validator replays every OTHER miner's winning attempt, so its steady-state load is
    /// `(M-1)/M · r / T` for per-header replay cost `r`. Measured on the fleet (2B/F16, N = 16
    /// decode tokens): r ≈ 12 s on an EPYC core, ≈ 26 s on the slowest host (Broadwell). T = 120 s
    /// puts the slowest honest host at ≈ 15 %. And because the MODEL can be replaced by an
    /// algo-id fork on the same chain while the BLOCK RATE cannot, the interval is chosen to also
    /// absorb the Qwen3.6-35B-A3B runtime (r ≈ 30-60 s on ≥ 32 GB hosts ⇒ ≈ 17-33 %). At the
    /// earlier 10 s the slowest host sat at 400 % — permanently behind, on any model.
    ///
    /// `ghostdag_k = 1` is `calculate_ghostdag_k(2 · NETWORK_DELAY_BOUND · (1/120), δ)`, asserted
    /// against the real function in `two_minute_bps_constants_match_formulas`.
    pub const fn new_two_minute_bps() -> Self {
        Self::new_seconds_per_block(120, 1)
    }

    /// Shared constructor for sub-1-bps networks: the same formulas `Bps<BPS>` encodes, evaluated
    /// at λ = 1/`secs` (which `Bps` cannot express — its const generic is an integer bps).
    ///
    /// Per-block sample rates floor at 1 (a rate below one block means "sample every block"), and
    /// `max_block_parents` / `mergeset_size_limit` sit at the formula floors for every k this is
    /// used with (k/2 < 10, 2k < 180). `ghostdag_k` is a parameter because `calculate_ghostdag_k`
    /// is not a const fn; each caller's value is asserted against the real formula in tests.
    pub const fn new_seconds_per_block(secs: u64, ghostdag_k: KType) -> Self {
        use crate::config::constants::consensus::{
            COINBASE_MATURITY_SECONDS, FINALITY_DURATION, MERGE_DEPTH_DURATION, PRUNING_DURATION,
        };
        let mergeset_size_limit: u64 = 180;
        let finality_depth = FINALITY_DURATION / secs;
        let merge_depth = MERGE_DEPTH_DURATION / secs;
        // The prunality lower bound, exactly as `Bps::pruning_depth` decomposes it.
        let lower_bound = finality_depth + merge_depth * 2 + 4 * mergeset_size_limit * ghostdag_k as u64 + 2 * ghostdag_k as u64 + 2;
        let duration_term = PRUNING_DURATION / secs;
        let pruning_depth = if lower_bound > duration_term { lower_bound } else { duration_term };
        // Ceiling division: maturity must never round DOWN below its wall-clock intent (at
        // 120 s/block, 100 s of maturity is one block, not zero).
        let coinbase_maturity = COINBASE_MATURITY_SECONDS.div_ceil(secs);
        Self {
            target_time_per_block: secs * 1000,
            ghostdag_k,
            past_median_time_sample_rate: 1,
            difficulty_sample_rate: 1,
            max_block_parents: 10,
            mergeset_size_limit,
            merge_depth,
            finality_depth,
            pruning_depth,
            coinbase_maturity,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverrideParams {
    /// Timestamp deviation tolerance (in seconds)
    pub timestamp_deviation_tolerance: Option<u64>,

    /// Size of the sampled block window that is used to calculate the past median time of each block
    pub past_median_time_window_size: Option<usize>,

    /// Size of the sampled block window that is used to calculate the required difficulty of each block
    pub difficulty_window_size: Option<usize>,

    /// The minimum size a difficulty window (full or sampled) must have to trigger a DAA calculation
    pub min_difficulty_window_size: Option<usize>,

    pub coinbase_payload_script_public_key_max_len: Option<u8>,
    pub max_coinbase_payload_len: Option<usize>,

    pub max_tx_inputs: Option<usize>,
    pub max_tx_outputs: Option<usize>,
    pub max_signature_script_len: Option<usize>,
    pub max_script_public_key_len: Option<usize>,
    pub mass_per_tx_byte: Option<u64>,
    pub mass_per_script_pub_key_byte: Option<u64>,
    pub mass_per_sig_op: Option<u64>,
    pub max_block_mass: Option<u64>,

    /// The parameter for scaling inverse KAS value to mass units (KIP-0009)
    pub storage_mass_parameter: Option<u64>,

    /// DAA score after which the pre-deflationary period switches to the deflationary period
    pub deflationary_phase_daa_score: Option<u64>,

    pub pre_deflationary_phase_base_subsidy: Option<u64>,
    pub skip_proof_of_work: Option<bool>,
    pub max_block_level: Option<BlockLevel>,
    pub pruning_proof_m: Option<u64>,

    /// Blockrate-related params
    pub blockrate: Option<BlockrateParams>,

    /// Target time per block prior to the crescendo hardfork (in milliseconds)
    pub pre_crescendo_target_time_per_block: Option<u64>,

    /// Crescendo activation DAA score
    pub crescendo_activation: Option<ForkActivation>,
}

impl From<Params> for OverrideParams {
    fn from(p: Params) -> Self {
        Self {
            timestamp_deviation_tolerance: Some(p.timestamp_deviation_tolerance),
            pre_crescendo_target_time_per_block: Some(p.pre_crescendo_target_time_per_block),
            difficulty_window_size: Some(p.difficulty_window_size),
            past_median_time_window_size: Some(p.past_median_time_window_size),
            min_difficulty_window_size: Some(p.min_difficulty_window_size),
            coinbase_payload_script_public_key_max_len: Some(p.coinbase_payload_script_public_key_max_len),
            max_coinbase_payload_len: Some(p.max_coinbase_payload_len),
            max_tx_inputs: Some(p.max_tx_inputs),
            max_tx_outputs: Some(p.max_tx_outputs),
            max_signature_script_len: Some(p.max_signature_script_len),
            max_script_public_key_len: Some(p.max_script_public_key_len),
            mass_per_tx_byte: Some(p.mass_per_tx_byte),
            mass_per_script_pub_key_byte: Some(p.mass_per_script_pub_key_byte),
            mass_per_sig_op: Some(p.mass_per_sig_op),
            max_block_mass: Some(p.max_block_mass),
            storage_mass_parameter: Some(p.storage_mass_parameter),
            deflationary_phase_daa_score: Some(p.deflationary_phase_daa_score),
            pre_deflationary_phase_base_subsidy: Some(p.pre_deflationary_phase_base_subsidy),
            skip_proof_of_work: Some(p.skip_proof_of_work),
            max_block_level: Some(p.max_block_level),
            pruning_proof_m: Some(p.pruning_proof_m),
            blockrate: Some(p.blockrate),
            crescendo_activation: Some(p.crescendo_activation),
        }
    }
}

/// kaspa-pq PQ-only enforcement mode (ADR-0019 / docs/kaspa-pq-design-mldsa87.md).
/// Selects whether legacy secp256k1 signature paths are merely non-standard
/// (mempool) or hard consensus failures. Every kaspa-pq network uses `Consensus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PqEnforcementMode {
    /// Upstream-compatible: no PQ restriction. Test / legacy-compat only.
    Disabled,
    /// Mempool + wallet reject legacy, but consensus still accepts. Migration
    /// testing only; never valid for a launched network.
    PolicyOnly,
    /// Block validation + script engine enforce ML-DSA-87-only. kaspa-pq default.
    Consensus,
}

/// **ADR-0065 D1's parameter: a fence and the window it turns on.**
///
/// Two fields rather than a bare `ForkActivation` because the rule needs a duration, and a
/// duration must not travel through the fence visitor (see the field's doc on [`Params`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBondMaturityV1 {
    /// When the rule comes into force.
    pub activation: ForkActivation,
    /// How long a bond must have been registered before it may be drawn for a seat, in DAA.
    pub window_daa: u64,
}

// ---- lane maturity (post-launch, 2026-09-26): ADR-0065 D1 from the post-launch fence -------------
/// **ADR-0065 D1's DAA window in force at `anchor_daa`, on both of its fences** — `maturity`'s window
/// where `maturity`'s own activation OR lane maturity's `early` fence ([`Params::palw_bond_maturity_early`])
/// is active at the anchor, else `None`.
///
/// One OR over ONE window: from `maturity`'s own height the answer is `maturity`'s alone whether or
/// not `early` is armed (no double counting, no gap at the hand-over), and below `early` it is
/// `None`, byte for byte the released rule. `early` without `maturity` answers nothing —
/// `validate_palw_v2` refuses that pairing, and the resolver does not invent a window for it.
/// The processor's `palw_bond_maturity_at` is this function over its copies of the two fields, so
/// the validator, the assembler and the registry warning resolve one window.
pub fn palw_bond_maturity_window_in_force_v1(
    maturity: Option<PalwBondMaturityV1>,
    early: Option<ForkActivation>,
    anchor_daa: u64,
) -> Option<u64> {
    let maturity = maturity?;
    let in_force = maturity.activation.is_active(anchor_daa) || early.is_some_and(|fence| fence.is_active(anchor_daa));
    in_force.then_some(maturity.window_daa)
}
// ---- end lane maturity ---------------------------------------------------------------------------

/// **ADR-0066 Decision 1's parameter: the heartbeat lane, and the fixed price it is held to.**
///
/// The price is a network CONSTANT and never `header.bits`. That is the whole decision: the first
/// implementation put the lane's difficulty in the field the global difficulty window averages, so
/// a window of heartbeat rows demanded work 33,554,432 and no bonded block could re-enter it — a
/// heartbeat-only chain recoverable only by re-mint, which is the self-feeding refusal ADR-0060
/// exists to abolish, reintroduced by its own remedy.
///
/// `work_log2` sits beside the fence for the same reason `PalwBondMaturityV1::window_daa` does, and
/// carries the same hazard: the fence visitor must not see it, so two builds scheduling the lane at
/// one height with different prices share an identity. Arming it is a coordinated change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHeartbeatV1 {
    /// When the lane opens.
    pub activation: ForkActivation,
    /// The lane's price as a work exponent: a heartbeat header must satisfy
    /// `Uint256::MAX >> work_log2`, independent of the `bits` it carries.
    pub work_log2: u32,
    /// **F3a's bound (ADR-0068 Phase 1): how many heartbeat blocks one mergeset may hold**,
    /// selected parent included. The slot rule bounds the chain and the price bounds the header
    /// rate; this is the missing third bound — the DAG's — placed where `mergeset_size_limit`
    /// already lives. Like `work_log2` it is a value beside the fence: hashed into
    /// `consensus_params_id`, invisible to the fence visitor, and locked to
    /// [`crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET`] at `validate_palw_v2` so the fence
    /// can only declare a bound this binary enforces.
    pub max_per_mergeset: u64,
}

/// **ADR-0125: the execution lane — round blocks beside the chain, admitted by permit.**
///
/// Past `activation` a `ConsensusV2` network accepts algo-10 round blocks
/// ([`crate::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1`]). A round block has exactly one non-round parent
/// (its anchor, which is its selected parent), is always red, is never a selected parent and never
/// counts in the DAA score, so every window, retarget and depth of the chain reads what it would
/// read without the lane. A chain block that merges one accepts its transactions when the block's
/// permit — drawn for its one-second round from the span's schedule of finalized attempts — is
/// found in the chain block's parent state, and pays its fees to the permit holder's payout.
///
/// The values beside the activation are the lane's shape, and like every companion value they are
/// hashed into `consensus_params_id` and invisible to the fence visitor: arming the lane is a
/// coordinated change. **Widening is a stage table** (§7.2): the lane opens at `permits_per_round`
/// and each used slot of `widenings` names a later height and a wider round. Each widening's height
/// is a fence of its own (visited, scheduled, named to the fork-id gate); its width is a companion
/// value. A span's width is the stage in force at the DAA score the span opens with, so every rule
/// that reads a width — the draw, the permit index, the per-round mergeset bound — reads one number
/// for a whole span, and a block validates at the width its span was produced at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecutionLaneV1 {
    /// When the lane opens.
    pub activation: ForkActivation,
    /// Permits per one-second round when the lane opens — the lane's first blocks per second. At
    /// most [`crate::palw_execution_lane_v1::PALW_EXEC_MAX_PERMITS_PER_ROUND_V1`].
    pub permits_per_round: u16,
    /// The later stages, in height order; unused slots are [`PalwExecWideningV1::NONE`] and follow
    /// every used one.
    pub widenings: [PalwExecWideningV1; crate::palw_execution_lane_v1::PALW_EXEC_MAX_WIDENINGS_V1],
    /// The most round blocks one mergeset may hold. Counted apart from `mergeset_size_limit`,
    /// which keeps bounding the chain's own blocks exactly as before.
    pub max_per_mergeset: u64,
    /// The DAA span of one schedule. The attempts finalized in span `s` become span `s + 2`'s
    /// participants at the first chain block of `s + 1`, and span `s + 2`'s schedule at its own first
    /// chain block, seeded by the latest attempt-carrying chain block of `s + 1` (ADR-0130) — so a
    /// span's producers are public for one span, not before. A round block is judged by the schedule
    /// of its anchor's span, and a merging block accepts round blocks anchored in its own span or the
    /// one before.
    pub schedule_span_daa: u64,
    /// **A later, shorter schedule span** (testnet-11: 5 DAA → 1 DAA at
    /// [`PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA`]). Unused is [`PalwExecSpanShortV1::NONE`]. The
    /// two-span seed delay (ADR-0130 Decision 4) is kept: participants are fixed, then a future
    /// attempt-carrying anchor seeds them. Only the wall-clock of one span shrinks. Historical
    /// folds below this height still read [`Self::schedule_span_daa`].
    pub short_span: PalwExecSpanShortV1,
}

/// **One later shortening of the execution lane's schedule span.** Height is a fence (fork-id
/// visible); the new span length is a companion value. `NONE` is unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecSpanShortV1 {
    pub activation: ForkActivation,
    pub schedule_span_daa: u64,
}

/// **One widening of the execution lane** (ADR-0125 §7.2): from the first span that opens at or past
/// `activation`, a round holds `permits_per_round` permits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecWideningV1 {
    pub activation: ForkActivation,
    pub permits_per_round: u16,
}

impl PalwExecWideningV1 {
    /// An unused slot.
    pub const NONE: Self = Self { activation: ForkActivation::never(), permits_per_round: 0 };

    pub fn is_used(&self) -> bool {
        self.activation != ForkActivation::never()
    }
}

impl PalwExecSpanShortV1 {
    /// No shortening scheduled.
    pub const NONE: Self = Self { activation: ForkActivation::never(), schedule_span_daa: 0 };

    pub fn is_used(&self) -> bool {
        self.activation != ForkActivation::never()
    }
}

impl PalwExecutionLaneV1 {
    /// No widening scheduled.
    pub const NO_WIDENINGS: [PalwExecWideningV1; crate::palw_execution_lane_v1::PALW_EXEC_MAX_WIDENINGS_V1] =
        [PalwExecWideningV1::NONE; crate::palw_execution_lane_v1::PALW_EXEC_MAX_WIDENINGS_V1];

    /// The DAA score span `span` opens with under the lane's opening span length.
    pub fn span_opening_daa(&self, span: u64) -> u64 {
        span.saturating_mul(self.schedule_span_daa.max(1))
    }

    /// The schedule span in force at `daa_score` — the opening length, or the shortened length
    /// past [`Self::short_span`].
    pub fn schedule_span_daa_at(&self, daa_score: u64) -> u64 {
        if self.short_span.is_used() && self.short_span.activation.is_active(daa_score) {
            self.short_span.schedule_span_daa.max(1)
        } else {
            self.schedule_span_daa.max(1)
        }
    }

    /// **The width of span `span`**: the widest stage in force at the DAA score the span opens with.
    /// Stages only widen and are in height order, so this is the last used stage at or below it.
    pub fn width_of_span(&self, span: u64) -> u16 {
        self.width_of_span_len(span, self.schedule_span_daa)
    }

    /// [`Self::width_of_span`] under an explicit span length — the shortened numbering past
    /// [`Self::short_span`].
    pub fn width_of_span_len(&self, span: u64, span_daa: u64) -> u16 {
        let opening = span.saturating_mul(span_daa.max(1));
        self.widenings
            .iter()
            .filter(|stage| stage.is_used() && stage.activation.is_active(opening))
            .map(|stage| stage.permits_per_round)
            .fold(self.permits_per_round, u16::max)
    }

    /// The width of the span `daa_score` falls in.
    pub fn width_at_daa(&self, daa_score: u64) -> u16 {
        let span_daa = self.schedule_span_daa_at(daa_score);
        self.width_of_span_len(crate::palw_execution_lane_v1::palw_execution_span_v1(daa_score, span_daa), span_daa)
    }

    /// The widest round any stage of this lane configures.
    pub fn widest(&self) -> u16 {
        self.widenings
            .iter()
            .filter(|stage| stage.is_used())
            .map(|stage| stage.permits_per_round)
            .fold(self.permits_per_round, u16::max)
    }
}

/// **ADR-0126: the validator carve drops to a fifth, and the tenth it gives up is escrowed for PALW.**
///
/// Past `activation`, wherever the overlay's FULL split is in force ([`DnsParams::reward_fee_split`]
/// at the block's DAA returns `reward_params.fee_split`), the split's `subsidy_validator_bps` is
/// this fence's. The worker base — the split's primary — takes the remainder, as
/// [`crate::dns_finality::split_block_subsidy`] always has it, so the four parts still sum to the
/// subsidy exactly. The bootstrap split and both fee splits are untouched, and so is every block
/// below the height. [`palw_overlay_fee_split_at_v1`] is the one reader of the lowered split.
///
/// A PALW claim escrows `⌊subsidy × worker_carve_permille / 1000⌋` of the block that carried its
/// attempt, and the coinbase withholds exactly that for the block — both resolved at **the attempt
/// block's own DAA score**, so the carve the fold records and the carve the coinbase withholds stay
/// one number (ADR-0042 Decision 10, B-1).
///
/// The two numbers are companion values: hashed into `consensus_params_id`, reported beside the
/// height in `consensus_schedule_id`, and invisible to the fence visitor, like the execution lane's
/// shape. Answered only on a `ConsensusV2` network that runs an overlay
/// ([`Params::palw_overlay_carve_fence`]); [`Params::validate_palw_v2`] refuses one that cannot fit
/// (Decision 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOverlayCarveV1 {
    /// When the lowered split and the grown escrow take effect.
    pub activation: ForkActivation,
    /// The validator pool's share of a block subsidy past the fence, in basis points. At most the
    /// network's full-split share: the fence only lowers it.
    pub subsidy_validator_bps: u16,
    /// A claim's escrow past the fence, in permille of the subsidy of the block that carried its
    /// attempt. At most the worker base the lowered split leaves.
    pub worker_carve_permille: u16,
}

/// **ADR-0132 Protocol Upgrade C: the economic payout** — a `Final` claim of a model class is paid
/// `min(escrow, attempted × rate)` on the economics it snapshotted at acceptance, its panel the share
/// derived from verification against producer compute, and a class over the cap-utilization ceiling
/// is not activatable (see `palw_economic_payout_v1`). Every number is the network's, the same for
/// every class; `Some`-only in the fingerprint; `None` on every shipped preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicPayoutV1 {
    pub activation: ForkActivation,
    /// Sompi paid per 10⁹ MAC-equivalents of attempted compute. Non-zero.
    pub rate_sompi_per_giga: u64,
    /// `α` in permille: how a unit of verification compute is valued against the producer's.
    pub panel_share_alpha_permille: u32,
    /// The panel share's bounds, in permille of the priced reward: `min ≤ max ≤ 1000`.
    pub panel_share_min_permille: u16,
    pub panel_share_max_permille: u16,
    /// A class whose uncapped reward exceeds this fraction of the escrow is not activatable. `≤ 1000`.
    pub cap_utilization_max_permille: u16,
}

/// **A public, immutable source URI on new model registrations.** `None` on every preset;
/// hashed Some-only. Prefix policy is applied at arming; the dormant chain stores no prefixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPublicModelSourceRuleV1 {
    pub activation: ForkActivation,
}

impl PalwEconomicPayoutV1 {
    /// The fold's reading of this fence at a block whose compact `bits` are `block_bits`.
    pub fn fold_v1(&self, block_bits: u32) -> crate::palw_economic_payout_v1::PalwEconomicPayoutFoldV1 {
        crate::palw_economic_payout_v1::PalwEconomicPayoutFoldV1 {
            rate_sompi_per_giga: self.rate_sompi_per_giga,
            panel_share_alpha_permille: self.panel_share_alpha_permille,
            panel_share_min_permille: self.panel_share_min_permille,
            panel_share_max_permille: self.panel_share_max_permille,
            cap_utilization_max_permille: self.cap_utilization_max_permille,
            block_bits,
        }
    }
}

impl PalwOverlayCarveV1 {
    /// `split` with this fence's validator share. The worker base is the primary and is
    /// documentary — [`crate::dns_finality::split_block_subsidy`] hands it the remainder whatever
    /// it says — so it is restated as that remainder, the number `validate_palw_v2` fits the carve
    /// against.
    pub fn lowered(&self, split: &FeeSplitParams) -> FeeSplitParams {
        FeeSplitParams {
            subsidy_worker_base_bps: 10_000u16
                .saturating_sub(split.subsidy_worker_inclusion_bps)
                .saturating_sub(self.subsidy_validator_bps)
                .saturating_sub(split.subsidy_service_bps),
            subsidy_validator_bps: self.subsidy_validator_bps,
            ..split.clone()
        }
    }

    /// The escrow carve as the reward module's checked type. `None` only for a permille past the
    /// whole subsidy, which `validate_palw_v2` refuses before a node starts.
    pub fn escrow_carve(&self) -> Option<crate::palw_reward_v2::PalwRewardParamsV2> {
        crate::palw_reward_v2::PalwRewardParamsV2::new(self.worker_carve_permille).ok()
    }
}

/// **The overlay's reward split at `daa_score`** (ADR-0126 Decision 2): `reward_fee_split`, with the
/// full split lowered where `carve` is active. `None` wherever `reward_fee_split` is `None` (below
/// the overlay's activation). Every reader of the split — the coinbase carve and
/// `coinbase_validator_pool` on the build and the validation path, and through the pool the quality
/// sub-pool and the audit fee — reads this, so a template and the block it becomes cannot disagree
/// about it (SA-3).
pub fn palw_overlay_fee_split_at_v1(dns: &DnsParams, carve: Option<PalwOverlayCarveV1>, daa_score: u64) -> Option<FeeSplitParams> {
    let split = dns.reward_fee_split(daa_score)?;
    // `reward_fee_split` answers with a reference into `dns`, so identity with the full split is
    // exactly "Stage 3 is in force at this score" — without a second copy of the staging rule.
    let full = std::ptr::eq(split, &dns.reward_params.fee_split);
    Some(match carve.filter(|carve| full && carve.activation.is_active(daa_score)) {
        Some(carve) => carve.lowered(split),
        None => split.clone(),
    })
}

/// **The carve a claim escrows** (ADR-0126 Decision 3): the fence's where it is active at the LOWER of
/// two DAA scores — the block that carried the attempt, and the block whose coinbase pays that
/// block's reward — and `None`, the bundle's own `worker_carve_permille`, below it.
///
/// The escrow is withheld from the worker base of the split the PAYING block uses
/// ([`palw_overlay_fee_split_at_v1`] at the payer's score), so a carve resolved past the fence has to
/// meet a lowered split. For a block's own attempt the payer is its selected-chain child, always
/// later, and the lower score is the attempt block's. For merged work the order is the DAG's, not a
/// rule: nothing makes a merged block's DAA score lower than its merger's, and a carve resolved at
/// the merged block's score alone would escrow 72 % against a 62 % base — the coinbase withholds
/// what it can (`saturating_sub`) and the claim later releases the whole 72 %, minting the rest on
/// top of the schedule. At the lower score a lowered carve only ever meets a lowered split, so
/// `escrow ≤ worker base` holds by construction.
pub fn palw_overlay_escrow_carve_at_v1(
    carve: Option<PalwOverlayCarveV1>,
    attempt_daa_score: u64,
    paying_daa_score: u64,
) -> Option<crate::palw_reward_v2::PalwRewardParamsV2> {
    let daa_score = attempt_daa_score.min(paying_daa_score);
    carve.filter(|carve| carve.activation.is_active(daa_score)).and_then(|carve| carve.escrow_carve())
}

/// **ADR-0130: a seat's exposure is floored at a multiple of the most it can be paid on the claim.**
///
/// Past `activation` a seat drawn onto a claim's panel reserves
/// [`crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1`]: the larger of ADR-0124's
/// `3 × claim.reserved` and `⌊reward_multiple_permille × max_seat_reward / 1000⌋`, where
/// `max_seat_reward` is the pool share of the claim's escrow divided by the seats the panel is drawn
/// with. A bond is drawn only while its free collateral covers that amount, and the amount reserved
/// is written into the claim's duty row, so the release and the dissent slash read what binding
/// took. On testnet-11's numbers past DAA 6,001 a seat is paid up to ~128 MSK a claim and reserved
/// ~0.40 MSK; at `λ = 2` it reserves ~256 MSK.
///
/// The draw resolves the multiple at the claim's ANCHOR (with every other rule that decides a panel,
/// `PalwSeatEconomyV1::reward_multiple_permille`), the fold at the binding block's DAA
/// (`PalwTransitionExtrasV1::panel_reward_multiple_permille`) — the same block for every binding the
/// chain derives at its anchor.
///
/// `reward_multiple_permille` is a companion value: hashed into `consensus_params_id`, reported
/// beside the height in `consensus_schedule_id`, invisible to the fence visitor. Answered only on a
/// `ConsensusV2` network ([`Params::palw_panel_exposure_floor_fence`]), and refused by
/// [`Params::validate_palw_v2`] without the panel economy it floors, below that economy's height, at
/// zero, and past [`crate::palw_panel_economy_v1::PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1`]. Armed on
/// testnet-12 alone, at `λ = 5` (the user's 2026-09-25 mainnet value, from ADR-0130 D1's 5–10);
/// `None` on testnet-11, devnet and mainnet, and not yet stated by the mainnet card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelExposureFloorV1 {
    /// When a seat's reservation starts being floored.
    pub activation: ForkActivation,
    /// `λ`, in permille of the most a seat can be paid on the claim: `2_000` reserves twice it.
    pub reward_multiple_permille: u32,
}

/// **ADR-0152-adjacent: the Activation Pool and the listing rules it rides on** (user decision
/// 2026-09-25; the adversarial review's §4, R1 and R2) — see
/// [`crate::palw_activation_pool_v1`].
///
/// One fence arms three rules: R1 (silence reclamation spares a class that cannot produce and a
/// genesis row), R2 (the registry's span step skips Dormant and Frozen classes and audits a
/// `Candidate` only at its own staggered span) and the pool (a per-class side map funded by
/// sink-bound top-ups, paying a once-per-operator preparation reward at the audit and an activation
/// bonus at `Probation → ActiveLimited`). `terms` is the companion value: hashed into
/// `consensus_params_id`, reported beside the height in `consensus_schedule_id`, invisible to the
/// fence visitor.
///
/// Genesis-only — R1 and R2 change rules every existing class is stepped by, and a crossing would
/// need a drill that crosses it — and refused by [`Params::validate_palw_v2`] without
/// `palw_model_registry`, `palw_admission_independence` and `palw_audit_2026_09_23` armed at
/// genesis beside it, or with terms that cannot run. `Some(0)` on testnet-12 only; `None` on every
/// other preset, and hashed Some-only everywhere, so every other network's ids are byte-identical
/// to a build without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwActivationPoolParamsV1 {
    /// When R1, R2 and the pool take effect — genesis or never.
    pub activation: ForkActivation,
    /// The pool's numbers ([`crate::palw_activation_pool_v1::PALW_ACTIVATION_POOL_TERMS_V1`] on
    /// testnet-12).
    pub terms: crate::palw_activation_pool_v1::PalwActivationPoolTermsV1,
}

/// **The readiness-V2 horizon** (user decision 2026-09-25, readiness capacity option (a)) — how many
/// execution spans a `SeatReadinessProvedV2` row stands past `Params::palw_readiness_v2`, where every
/// network without this fence keeps the eight of
/// [`crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1`].
///
/// A seat counts as READY for a class only while its latest possession proof is fresh, and a V2
/// proof is ~48.9 KB (~195,508 transient mass, two a block at one block a DAA): at eight spans the
/// integration owner's network model kept about 6.4 of testnet-12's sixteen rows fresh on empty
/// blocks and 2.7–4.2 in a DA storm, against the ten that keep both genesis model classes out of
/// HELD. The user chose the longer horizon, with the node's re-prove cadence (half of it) scaled to
/// it; `max_age_spans` is the companion value, hashed into `consensus_params_id` and reported
/// beside the height in `consensus_schedule_id`, invisible to the fence visitor.
///
/// Genesis-only — a crossing would count one row ready and not ready either side of one height,
/// with no drill that crosses it — and refused by [`Params::validate_palw_v2`] off ConsensusV2,
/// above genesis, without `palw_model_registry` and `palw_readiness_v2` armed at genesis beside it,
/// with a horizon outside `[8, 30]` spans
/// ([`crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_MAX_V1`]), or with the V2
/// bundle's mirror unsynced. `Some(always, 24)` on testnet-12 only; `None` on every other preset,
/// and hashed Some-only everywhere, so every other network's ids are byte-identical to a build
/// without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwReadinessV2MaxAgeParamsV1 {
    /// When the horizon takes effect — genesis or never.
    pub activation: ForkActivation,
    /// How long a V2 row stands, in execution spans
    /// ([`crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1`] on testnet-12).
    pub max_age_spans: u32,
}

/// The Activation Pool's terms, in declaration order, into a fingerprint (`consensus_params_id`
/// and `consensus_schedule_id` write the same bytes).
fn palw_activation_pool_terms_write_v1(h: &mut ConsensusParamsId, terms: &crate::palw_activation_pool_v1::PalwActivationPoolTermsV1) {
    h.write(terms.prep_base_sompi.to_le_bytes());
    h.write(terms.prep_share_permille.to_le_bytes());
    h.write(terms.bonus_share_permille.to_le_bytes());
    h.write(terms.ramp_daa.to_le_bytes());
    h.write(terms.prep_payee_cap.to_le_bytes());
    h.write(terms.bonus_payee_cap.to_le_bytes());
    h.write(terms.min_topup_sompi.to_le_bytes());
    h.write(terms.bonus_cap_sompi.to_le_bytes());
}

/// **ADR-0066 Decision 3's parameter (finding F2), closed by ADR-0068 Phase 1: the attempt lane's
/// fork-choice work leaves `calc_work(header.bits)`.**
///
/// On `ConsensusV2` the hash target is not the throttle, so `calc_work` prices a bonded block at
/// 2 — parity with two sibling heartbeats at ε = 1. Under this fence an attempt-lane (algo-6)
/// block's blue work is the constant `1 << work_log2` instead, so a bonded block outweighs a
/// million heartbeats and hash buys nothing back. A constant and not the claimed pwu: the claim
/// is only verified against class state on the selected chain, and GHOSTDAG has only the header —
/// see [`crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2`] for the whole argument.
///
/// `work_log2` sits beside the fence exactly as [`PalwHeartbeatV1::work_log2`] does, and carries
/// the same hazard and the same lock: hashed into `consensus_params_id`, never visited by the
/// fence visitor, refused at start if it names a value this binary does not implement. Arming it
/// moves `header.blue_work` on every attempt block from the fence height on, so it is a
/// coordinated change — every operator schedules the same height or the chain splits the moment
/// the fence fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAttemptWorkV1 {
    /// When attempt-lane blue work switches to the constant.
    pub activation: ForkActivation,
    /// The attempt lane's work as an exponent: an algo-6 block contributes `1 << work_log2` to
    /// blue work, independent of the `bits` it carries.
    pub work_log2: u32,
    /// **ADR-0071 Decision 2: how many nonces one execution buys, as an exponent.**
    ///
    /// The lane's third price, and the one that was not a number anywhere in consensus at all. The
    /// job anchor deliberately excluded the nonce, so a single inference served an unbounded sweep
    /// and the only thing making it four million rather than 2^64 was `kaspad`'s own
    /// `NONCES_PER_TEMPLATE` — a node-local constant, which bounds honest producers and nobody
    /// else. With the bucket in the anchor the bound is a rule, and a rule two nodes must agree on
    /// belongs in the id they compare.
    ///
    /// Same lock as its two siblings: refused at start unless it names
    /// [`crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2`], the value the anchor actually
    /// buckets by.
    pub ticket_bucket_log2: u32,
}

/// **ADR-0128 Decision 8: DNS validators vote BFT by bonded stake, and that vote decides the stake
/// reorg gate.**
///
/// Past `activation` (resolved at the sink's DAA score) an epoch's canonical lagged anchor is
/// DNS-final when bonded stake above two thirds of the epoch's counted set has both attested it
/// and precommitted it under a lock the chain can check, the precommit binding the counted set
/// through its snapshot commitment ([`crate::dns_bft_v1`]). `DnsState.last_dns_confirmed_anchor`
/// then follows the newest DNS-final anchor instead of the StakeScore depth rule, and
/// `dns_reorg_outcome` refuses every candidate that abandons it — before the PALW comparator,
/// for a reorg and an extension alike — until it goes stale under `dns_veto_ttl_daa_score`.
/// Below the fence, and on every network that leaves it `None` (every shipped preset), nothing
/// changes.
///
/// The three numbers beside the height are the counted set's leak (ADR-0128 Decision 3), which
/// is this fence's own and replaces [`PalwInactivityLeakV1`]:
///
/// * a bond is leaked at an epoch whose anchor is at DAA `a` when `a − last ≥ t_leak_daa`, `last`
///   being the anchor DAA of its youngest attestation accepted at or below `a` with an anchor at
///   least `reentry_final_depth_daa` below `a`, or — with no such attestation in the evidence
///   window — the later of the bond's activation and the window's lower edge;
/// * nothing is leaked where leaking would leave fewer than `min_retained_validators` distinct
///   validators.
///
/// **Values beside a height, so the height is visited and the values are not** — the
/// [`PalwBondMaturityV1::window_daa`] rule for the D1 reason: the identity visitor normalises
/// what it visits, and a normalised duration would let two builds leaking at different silences
/// fingerprint identically. All four reach `consensus_params_id` and `consensus_schedule_id`
/// Some-only; a scheduled height collapses out of the identity with its values.
///
/// Refused at start by [`Params::validate_palw_v2`] where the network runs no overlay, where
/// `t_leak_daa` is zero, where the re-entry depth is not below it, where the floor is under four
/// validators, and where the pruning depth is shorter than the walk a sink's evaluation reads
/// ([`crate::dns_bft_v1::dns_bft_walk_blue_score_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DnsBftGateV1 {
    /// When the BFT confirmation and the gate that follows it come into force, judged at the
    /// sink's DAA score (and, in the gate, at the incumbent sink's).
    pub activation: ForkActivation,
    /// How long a bond must have been silent before the counted set leaks it, in DAA. testnet-11's
    /// planned value is 5,040: seven days at the chain's 120-second cadence.
    pub t_leak_daa: u64,
    /// How deep an attestation's anchor must be buried below an epoch's anchor before it counts as
    /// that epoch's re-entry evidence, in DAA (ADR-0066 SA-2's hysteresis). Strictly below
    /// `t_leak_daa`, or nobody who leaks ever comes back.
    pub reentry_final_depth_daa: u64,
    /// The fewest distinct validators leaking may leave in a counted set; below it nothing is
    /// leaked and finality waits. At least four — the smallest set in which one fault is tolerated.
    pub min_retained_validators: u32,
}

/// **ADR-0066 Decision 4's parameter: the inactivity leak, and how long silence must last.**
///
/// **The leak is removed.** No code reads this value any more and [`Params::validate_palw_v2`]
/// refuses a network that sets it; the type stays because `Params::palw_inactivity_leak` does
/// (hashed only when set, so an unset preset's ids are what they were). The leak that exists is
/// [`DnsBftGateV1`]'s (ADR-0128), which reads an evidence window every synced node holds.
///
/// It replaced `DnsParams.inactivity_leak_daa`, which could not be armed at all. That field is
/// inside `DnsParams`, which `consensus_params_id` hashes as one raw borsh blob while
/// `for_each_fence` deliberately does not visit it — both halves individually correct, and together
/// meaning that moving the constant off `u64::MAX` moves `consensus_identity_id` and disconnects
/// the first upgrading operator from every un-upgraded peer immediately. The leak could only ever
/// be turned on by a flag day. Here it is a fence like any other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwInactivityLeakV1 {
    /// When the leak comes into force.
    pub activation: ForkActivation,
    /// How long a validator must have been silent before its weight begins to leak, in DAA.
    pub t_leak_daa: u64,
    /// **ADR-0066 SA-2's hysteresis, as a number: how deep an attestation must be buried before
    /// it counts as re-entry evidence, in DAA.**
    ///
    /// Exclusion follows `t_leak_daa` of silence. Re-inclusion needs an attestation that is itself
    /// FINAL, and this is what "final" costs: an anchor newer than `sink_daa - this` does not move
    /// a validator's baseline. Without it, a validator flapping around the threshold re-enters the
    /// quorum on a block of its own choosing, which is a quorum swing an attacker times rather
    /// than one the chain observes.
    ///
    /// Was required to be strictly below `t_leak_daa`, or the re-entry window is empty and nobody
    /// who leaks ever comes back. [`Params::validate_palw_v2`] now refuses any leak at all.
    ///
    /// Beside the fence and therefore in `consensus_params_id` — and, since the SA-4 amendment,
    /// in `consensus_schedule_id` as well, for the same reason `t_leak_daa` is: two builds that
    /// disagree about it re-admit different validators, and the schedule id is what the
    /// `flow_context` warning prints when it keeps a peer whose params id differs. Deliberately
    /// NOT in `consensus_identity_id` — see [`Params::consensus_schedule_id`] for why gating on it
    /// would make arming the leak the flag day the top-level fence exists to abolish.
    pub reentry_final_depth_daa: u64,
}

/// **ADR-0073 SA-1's parameter: how many attempt blocks the free-prompt beacon folds.**
///
/// The single-block beacon costs one inference to RE-ROLL and only one block's subsidy to
/// WITHHOLD: a producer whose own block would be the beacon can drop it when the draw disfavours
/// its pending claims. Past `activation` the beacon is the fold of the first `k` attempt blocks at
/// or after the slot, so a producer holding attempt share `p` chooses the draw with probability
/// `p^k` rather than `p`. `None` on every shipped preset — with the fence off the derivation is
/// byte-identical to the pre-SA-1 rule, which is what `with_the_fence_off_the_beacon_is_the_block_itself`
/// pins.
///
/// **`k` sits beside the fence and is deliberately NOT visited by `for_each_fence`** — it is a
/// WIDTH, and the identity visitor normalises every value it visits to `0`/`u64::MAX`, so a
/// visited width would make two builds folding different numbers of blocks fingerprint identically
/// at every height. It is hashed raw into `consensus_params_id` (and into `consensus_schedule_id`,
/// so a mismatch can be reported) instead. That is exactly `PalwBondMaturityV1::window_daa`'s
/// shape and it carries the same hazard: while the fence is only SCHEDULED the identity path
/// collapses the whole option, so two builds scheduling SA-1 at one height with different `k`
/// share an identity, peer with only the params-id warning, and draw different beacons the moment
/// it fires. **Arming this fence is therefore a coordinated change**, and `validate_palw_v2`
/// refuses a `k` below [`crate::palw_fp_beacon_v3::PALW_BEACON_FOLD_MIN_K_V1`] so at least the
/// amendment's floor cannot be undercut by configuration.
///
/// **Why not inside the V2 bundle.** `k` decides a draw, so the natural home is
/// `PalwFreePromptParamsV3`, where the V2 handshake would compare it through `palw_ruleset_id_v2`
/// and two builds could not disagree at all. Putting it there moves that id — and the network
/// fingerprint with it — for every preset carrying a bundle, i.e. a re-mint on a running testnet,
/// for a rule that is dormant everywhere. It belongs in the bundle at the re-mint that arms it;
/// until then it is a top-level fence like ADR-0065 D1, for D1's reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBeaconFoldV1 {
    /// When the fold comes into force. Resolved at the DRAW'S OWN SLOT rather than at the walking
    /// block's height: the slot is a function of the claim (`final_daa + receipt_maturity_daa`),
    /// so a producer reading spendable quanta at the tip and a validator checking a spend deep in
    /// the past resolve the same width for the same claim. A height-of-the-walker resolution would
    /// let those two disagree about a fold whose inputs are identical.
    pub activation: ForkActivation,
    /// How many attempt-class chain blocks at or after the slot are folded into the beacon.
    /// At or above [`crate::palw_fp_beacon_v3::PALW_BEACON_FOLD_MIN_K_V1`].
    pub k: u8,
}

/// Consensus parameters. Contains settings and configurations which are consensus-sensitive.
/// Changing one of these on a network node would exclude and prevent it from reaching consensus
/// with the other unmodified nodes.
#[derive(Clone, Debug)]
pub struct Params {
    pub dns_seeders: &'static [&'static str],
    pub net: NetworkId,
    pub genesis: GenesisBlock,

    /// Timestamp deviation tolerance (in seconds)
    pub timestamp_deviation_tolerance: u64,

    /// Defines the highest allowed proof of work difficulty value for a block as a [`Uint256`]
    pub max_difficulty_target: Uint256,

    /// Highest allowed proof of work difficulty as a floating number
    pub max_difficulty_target_f64: f64,

    /// Size of the sampled block window that is used to calculate the past median time of each block
    pub past_median_time_window_size: usize,

    /// Size of the sampled block window that is used to calculate the required difficulty of each block
    pub difficulty_window_size: usize,

    /// The minimum size a difficulty window must have to trigger a DAA calculation
    pub min_difficulty_window_size: usize,

    pub coinbase_payload_script_public_key_max_len: u8,
    pub max_coinbase_payload_len: usize,

    pub max_tx_inputs: usize,
    pub max_tx_outputs: usize,
    pub max_signature_script_len: usize,
    pub max_script_public_key_len: usize,

    pub mass_per_tx_byte: u64,
    pub mass_per_script_pub_key_byte: u64,
    pub mass_per_sig_op: u64,
    pub max_block_mass: u64,

    /// The parameter for scaling inverse KAS value to mass units (KIP-0009)
    pub storage_mass_parameter: u64,

    /// DAA score after which the pre-deflationary period switches to the deflationary period
    pub deflationary_phase_daa_score: u64,

    pub pre_deflationary_phase_base_subsidy: u64,
    pub skip_proof_of_work: bool,
    pub max_block_level: BlockLevel,
    pub pruning_proof_m: u64,

    /// Blockrate-related params
    pub blockrate: BlockrateParams,

    /// Target time per block prior to the crescendo hardfork (in milliseconds).
    /// Required permanently in order to calculate the subsidy month from the current DAA score
    pub pre_crescendo_target_time_per_block: u64,

    /// Crescendo activation DAA score
    pub crescendo_activation: ForkActivation,

    /// kaspa-pq Phase 10 (ADR-0009): DNS finality overlay parameters, or
    /// `None` when the overlay is not configured for this network. `None`
    /// on every current network — the overlay's consensus effects
    /// (bond population, reorg gate) are guarded by `dns_params.is_some()`
    /// and are therefore fully inert until a network opts in.
    pub dns_params: Option<DnsParams>,

    /// ADR-0033 (B14): the PALW credit gate's fence — the one registered class this network
    /// credits and the §4e chain facts, or `None` when the gate is dormant. `None` on every
    /// shipped network (the type cannot even be built in a `const`, so a literal here can
    /// only ever be `None`); activation is a deliberate runtime opt-in whose preconditions
    /// ADR-0033 lists. Everything the gate mints is validated against this same fence, so a
    /// node without it rejects any coinbase claiming PALW credit.
    pub palw_credit: Option<crate::palw_credit::PalwCreditParamsV1>,
    /// ADR-0039 W4′: the PALW fork-choice fence. **`None` on every shipped preset** — that is the
    /// whole of today's dormancy guarantee, and it is a fact about the presets, not a structural
    /// one. [`Params::validate_palw_v1`] does NOT refuse `Some` outright: it refuses `Some` only
    /// when `palw_credit` is `None` (no registered class to weigh blocks against). Setting it is
    /// therefore possible for anyone constructing `Params` directly, and setting it changes the
    /// P2P fingerprint ([`Params::consensus_fingerprint`]) while changing no tip ordering — both
    /// seam sites still pass `None` for the weights, so today it partitions the network without
    /// altering fork choice. `params_do_not_set_the_palw_fork_choice_fence` asserts the preset
    /// fact; nothing asserts more, because nothing more is true.
    pub palw_fork_choice: Option<crate::palw_chain_weight::PalwChainWeightParamsV1>,
    /// ADR-0038 Decision A, first clause: the fence that lets a PALW header carry its commitment.
    ///
    /// `None` on every shipped preset, and `None` is today's rule exactly — a PALW header's
    /// `palw_commitment` must be EMPTY. Installing it makes the field required and validated from
    /// `activation_daa_score`, which is why it is part of the fingerprint (Some-only, so a network
    /// that installs none fingerprints byte-identically to before this field existed).
    /// ADR-0028 §3 / ADR-0038 Decision H: the class's registered windows — `delta_bind`, `w_replay`,
    /// `w_answer`, `w_round`, `w_challenge`, `prosecution_slack`, `q`.
    ///
    /// `None` on every shipped preset. Installing it is what makes the panel draw, the duty
    /// deadline and the challenge window configurable rather than derived from a placeholder, and
    /// it is validated with [`crate::palw_schedule::PalwScheduleParamsV1::validate_for_value_network_v1`]
    /// — so **a network that installs a schedule and is not on the frozen 120-second cadence is
    /// refused at construction** rather than discovering it when no node can finish a join.
    pub palw_schedule: Option<crate::palw_schedule::PalwScheduleParamsV1>,
    /// ADR-0038 Decision B: the weight ramp's own two numbers — the receipt quorum that licenses
    /// work and ρ_r, the fraction of pwu a licensed-but-immature block carries.
    ///
    /// `None` on every shipped preset. Separate from [`Self::palw_fork_choice`] because they are
    /// different questions: this decides when a block MATURES, that decides how much an immature
    /// block is worth to tip selection. A network could sensibly change one without the other.
    pub palw_ramp: Option<crate::palw_weight::PalwWeightParamsV1>,
    pub palw_block_commitment: Option<crate::palw_block_commitment::PalwBlockCommitmentParamsV1>,

    /// **ADR-0064 — the bond becomes usable in the block that registers it.** `None` on every
    /// shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past this fence, admission item 1's bond lookup — for the block's OWN attempt only —
    /// resolves against `bonds(parent state) ∪ {BondRegistered in this block's own mergeset}`, so a
    /// bond becomes usable by the block that ACCEPTS its registration instead of by that block's
    /// child. The state machine already accepts such a block (`apply_palw_transition_v4` applies
    /// objects at step 3 and own work at step 4); only the pipeline pre-check, reading state at the
    /// selected parent, refuses. Removing that disagreement is what this fence does.
    ///
    /// **It does NOT recover a chain whose producers have all stopped**, and the ADR carries the
    /// correction: a block's own body is never in its own mergeset (a block's transactions are
    /// accepted by a later block), and a V2 network accepts only the attempt and receipt lanes,
    /// both of which require standing the newcomer does not have — so its registration cannot reach
    /// any block body in the first place. This buys a joining producer one chain block on a LIVE
    /// chain. Trustless recovery from a total stop remains open.
    ///
    /// **TOP LEVEL on purpose, and it must stay here.** Top-level fences are normalised by
    /// [`Self::consensus_identity_id`], so two builds that disagree only about a future height keep
    /// one identity and stay peers. A fence inside the V2 bundle goes through `palw_ruleset_id_v2`,
    /// which is a bare hash over the whole bundle with no normalisation — arming it there would
    /// partition the network on deploy day.
    pub palw_bootstrap_activation: Option<ForkActivation>,

    /// **ADR-0065 D4 — an `Unavailable` receipt stops convicting the producer.** `None` on every
    /// shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past this fence an `Unavailable` verdict counts toward NEITHER quorum and is never charged
    /// as a dissent — the treatment `Incapable` already gets. A claim whose panel therefore
    /// concludes nothing falls to the redraw-then-`ReceiptTimeout` path, which voids the escrow and
    /// slashes nobody, instead of `ProducerDefaulted`, which calls `void_and_slash` and takes
    /// `claim.reserved` from the producer's bond.
    ///
    /// **Why the verdict cannot be repaired instead.** A conviction ought to require proof the
    /// producer was reached and refused, and no such proof is constructible here: the material pull
    /// carries no seat identity, nonce or signature, the serve carries no producer signature, and
    /// neither side records anything. So the seat cannot tell *the producer withheld* from *the
    /// network did not deliver*, and on testnet-11 roughly a third of every remote seat's verdicts
    /// were the second wearing the first's clothes.
    ///
    /// **What the obligation-naming gate is and is not.** An earlier version of this note said that
    /// gate — `palw_panel_v2`'s `chunk_index` / `requested_daa` checks — "is satisfied by two
    /// constants", and that is false. Only `chunk_index` is a constant (`0`); `requested_daa` is a
    /// computed timestamp, and it had teeth: it was refusing every redrawn panel's abstention,
    /// which is why the seat service needed fixing to floor that timestamp at the CURRENT panel's
    /// `bound_daa`. The gate was working and its producer was wrong. The gate is dropped past this
    /// fence for a different and narrower reason — an abstention accuses nobody, so refusing a
    /// whole receipt set over one seat's malformed accusation kills claims that three other seats
    /// verified — and anyone weighing whether to re-tighten it should weigh that, not a claim that
    /// it never checked anything.
    ///
    /// **TOP LEVEL for the reason on `palw_bootstrap_activation`**, and here it is load-bearing
    /// rather than tidy: this rule must reach a LIVE network, and a fence inside the V2 bundle
    /// moves `palw_ruleset_id_v2`, which `for_each_fence` never descends into — so every old/new
    /// pair would fail the handshake outright instead of peering with a warning.
    pub palw_unavailable_abstains: Option<ForkActivation>,

    /// **ADR-0065 D1 — a bond must be earned before it may judge.** `None` on every shipped
    /// preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past `activation`, `derive_panel_v2` draws a seat only from a bond registered at or before
    /// `anchor_daa - window_daa`. `registered_daa` has been written by the fold since the ruleset
    /// existed and read by no gate; this is the rule it was evidently recorded for, so it adds no
    /// state.
    ///
    /// **The window is a DURATION and is therefore NEVER visited by `for_each_fence`** — see
    /// `inactivity_leak_daa`, which carries the same warning. `consensus_identity_id` normalises
    /// every visited value to `0` or `u64::MAX`, so a visited duration would make two builds
    /// shipping different windows fingerprint identically at EVERY height. It is hashed raw into
    /// `consensus_params_id` instead, and only the `activation` beside it is a fence.
    ///
    /// **What that does NOT buy, and it is a deployment hazard worth knowing about.** While the
    /// fence is merely scheduled, `consensus_identity_id` rewrites its height to `never()` and
    /// `normalize_values_a_scheduled_fence_drags_with_it` then collapses the whole option — so two
    /// builds scheduling D1 at one height with DIFFERENT windows share an identity, peer with only
    /// the params-id warning, and seat different panels the moment it fires. That is this file's
    /// standing rule for a value inert until its fence fires (audit3 H1) and the VLT cost table has
    /// the same property; it is not special to this field. It does mean **arming this fence is a
    /// coordinated change**, and the warning in `flow_context` is the whole of the defence.
    /// `the_bond_maturity_window_is_in_the_fingerprint_and_not_in_the_fence_visitor` pins both
    /// halves so neither can move by accident.
    pub palw_bond_maturity: Option<PalwBondMaturityV1>,

    /// **ADR-0065 D2 — a deep reorg may not rest on bonds the fork minted for itself.** `None` on
    /// every shipped preset, so the behaviour is byte-identical to not having the field.
    ///
    /// D2 was restated as unimplementable inside the state fold: `safe_frontier` is written by a
    /// pure single-chain transition whose result is hashed into `state_root`, so a value that
    /// depended on a fork point would depend on which competing branch a node holds, and two nodes
    /// would compute different roots for one block. It lives at the COMPARISON instead — the
    /// deep-reorg gate, the one site holding both tips inside one consensus instance.
    ///
    /// **A bare fence with no companion value, deliberately.** The threshold is derived from the
    /// panel's own `seat_count`/`quorum` rather than configured, because a value that sits beside a
    /// fence but is not itself a fence is normalised out of [`Self::consensus_identity_id`] — two
    /// builds scheduling the rule at one height with different thresholds would share an identity,
    /// peer, and disagree the moment it fires. `palw_bond_maturity` carries exactly that hazard and
    /// its doc records that arming it is therefore a coordinated change; this has nothing to
    /// coordinate.
    pub palw_frontier_provenance: Option<ForkActivation>,

    // ─── lane: rcore/f1-forkchoice-attacks — the deep-reorg strict-economic-win fence ───
    /// **A deep reorg must be earned on ECONOMICS, or the incumbent is kept.** `None` on every
    /// shipped preset — dormant on testnet-12, armed only in this lane's tests.
    ///
    /// Below the fence `dns_reorg_outcome` allows a deep reorg when
    /// `compare_palw_candidates_v1(challenger, incumbent)` is `Greater`, and that comparator's LAST
    /// key is the candidate hash — so a private branch that ties the incumbent on all three
    /// economic keys (safe-frontier blue score, safe weight, live total) wins the reorg on hash
    /// alone. The fork-choice attack probes on this branch measure exactly that: with no `Final`
    /// or immature claim on either side both candidates read `{frontier 0, safe 0, live 0}` and
    /// `decide_deep_reorg_v2` Allows, so a heartbeat / junk-attempt private branch double-spends
    /// inside the finality depth by grinding a higher-hash tip. Past the fence a reorg is allowed
    /// on a STRICT economic win and refused on a strict economic loss, and an all-economic tie keeps
    /// the incumbent — the confirmed history — **unless the reorg is SHALLOW** (lane
    /// rcore/f1-strictwin-tie): a tie whose incumbent side (the incumbent's selected chain above its
    /// common chain ancestor with the challenger) spans at most
    /// `palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1` = 2 DAA ticks is decided by GHOSTDAG's
    /// own order, blue work then hash (`palw_fork_authority_v2::palw_reorg_strict_economic_win_v1`;
    /// the processor reads the depth and the order, `palw_reorg_shallow_ghostdag_win_v1`). Keeping
    /// the incumbent on EVERY tie was a rule about arrival order: the gate judges every non-extension
    /// sink move, one-block sibling switches included, so two honest nodes racing a slot kept the
    /// siblings they saw first and their virtual chains diverged over one DAG (measured through the
    /// pipeline, rcore/cap-weight 3aa4abec4: split at the first exchange in 8/8 runs). The price,
    /// stated: a tying branch heavier on blue work can reverse a payment with at most two ticks (~4
    /// min) of confirmation however releases are chained (a shallow tie never LOWERS the sink's DAA,
    /// so the window cannot be walked back — without that two chained releases reversed three ticks,
    /// measured); deeper, the tie keeps the
    /// incumbent. The rule is keyed as before (read at the INCUMBENT's DAA), reads no clock and touches no
    /// live-weight rule (so it is independent of the two unconfirmed synthesis choices), and it fires
    /// only on actual reorgs, so forward progress
    /// on GHOSTDAG blue work is byte-identical below and above it. A **bare fence with no companion
    /// value**, the [`Self::palw_frontier_provenance`] rule for its reason.
    pub palw_reorg_strict_economic_win: Option<ForkActivation>,
    // ─── lane: rcore/hf-pptake2 — the pruning-proof / IBD adoption strict-economic-win fence ───
    /// **A pruning-proof / IBD adoption must be earned on ECONOMICS, or the incumbent is kept.**
    /// `None` on every shipped preset — dormant on testnet-12, armed only in this lane's tests.
    ///
    /// The IBD-commit sibling of [`Self::palw_reorg_strict_economic_win`]. A permissionless peer can
    /// serve a node doing pruning-proof IBD a headers-only fork of unbonded / lottery-losing attempt
    /// headers: each costs ~one ML-DSA signature (no bond, no lottery win, no inference; the attempt
    /// lane's Layer-0 target is ambient, so its PoW is trivial) yet carries the 2²⁰ attempt constant,
    /// because `palw_lane_blue_work_v1` is a pure function of the header, blind to the bond and the
    /// lottery. So the proof outweighs the honest chain on raw accumulated blue work and
    /// `compare_proofs_inner` adopts it. Below this fence the staging-commit gate then compares the
    /// two PALW orders with `decide_ibd_commit_v2`, whose comparator's LAST key is the candidate
    /// hash — and a chain that matured no work reads `{frontier 0, safe 0, live 0}`, tying an honest
    /// chain whose own first floor claim is still short of `Final`, so the attacker takes the history
    /// over by grinding a higher-hash tip. Past the fence `validate_staging_palw_order` commits only
    /// on a STRICT economic win (`palw_ibd_commit_strict_economic_v1`); an all-economic tie keeps the
    /// incumbent — the confirmed history — which is strictly more conservative, reads no clock and
    /// touches no live-weight rule, and can only ever convert a `Commit` into a `KeepIncumbent`, so an
    /// honestly-superior chain still syncs. Read at the INCUMBENT's DAA — a challenger's own score is
    /// attacker-chosen. A **bare fence with no companion value**, the [`Self::palw_frontier_provenance`]
    /// rule for its reason.
    pub palw_pruning_proof_strict_economic_win: Option<ForkActivation>,
    // ─── lane: rcore/cap-weight — ADR-0160 F-W, the per-bond weight cap (J-1) ───
    /// **ADR-0160 F-W: claims × N is not fork power × N.** `None` on every shipped preset — a
    /// testnet-12-only capacity fence ([`PALW_T12_CAPACITY_FENCES_V1`]), dormant until the capacity
    /// release's own flag day arms it (never with the DAA-750 list).
    ///
    /// Below the fence every non-terminal claim adds its whole stored `immature_contribution`
    /// (β·pwu) to `bounded_immature`, so 100 claims weigh 100× and one 2M claim weighs 555,611
    /// floor claims — before any panel has looked at it. Past it, for an ATTEMPT claim whose
    /// `accepted_daa` is at or past the height (`crate::palw_weight_cap_v1`):
    /// * the claim's weight is STAGED by its lifecycle — Created 0, Anchored 10‰, Licensed full
    ///   (S2 250‰), Final full into `safe_weight`, void 0 — and its full weight is `min(raw, C7
    ///   ceiling)` (ADR-0160 D-3);
    /// * a bond's provisional weight is capped at `⌊C/6,500 MSK⌋` floor-claim weights, so a 13k
    ///   bond's live claims weigh at most two floor claims whatever their number or class;
    /// * the claim's weight reservation (`claim.reserved`) is `min(w, R_budget − held)`, the bond's
    ///   budget `⌊C/6,500 MSK⌋ × w_floor`.
    ///
    /// Keyed on the claim's `accepted_daa` (the ADR-0145 precedent), so a claim accepted below the
    /// height keeps today's accounting for its whole life. Requires `palw_rcore_plus`,
    /// `palw_reorg_strict_economic_win` (staged weight makes economic ties common; without
    /// strict-win a tie would fall to the hash) and `palw_operator_anchor` (lane A) at or below it.
    /// The operator anchor is what keeps a private branch's post-fork claims at Created (weight 0):
    /// staged weight moves the weight a claim buys from its acceptance to its bind and licence, and a
    /// private branch that may anchor its own panels (the P0-10 seed-grinding hole the anchor stops)
    /// self-licenses up to `Σ W_cap` of its bonds' caps that the public branch cannot match — its
    /// own post-fork claims weigh 0 until bound, and its pre-fork licences are importable — so the
    /// strict-win gate would ALLOW the deep reorg (ADR-0160 §8 A1). Until the structural P0-10 fix
    /// retires the stopgap, F-W does not arm without it. The fold reads it through the V2 bundle's
    /// mirror (`Self::sync_palw_capacity_weight_cap`). A bare fence.
    ///
    /// **Fork choice past it is strict-win's own rule, and only that** (rcore/cap-s1, stage 1 of the
    /// capacity programme: ONE shallow-tie rule). The lane (rcore/cap-weight 3aa4abec4) carried its own
    /// copy of the shallow-tie rule behind this fence; the DAA-750 release shipped the same rule as
    /// `palw_reorg_strict_economic_win`'s (rcore/f1-strictwin-tie, merged at 19e732712) WITH the
    /// no-DAA-lowering check (a shallow tie never moves the sink to a lower DAA, so chained tie releases
    /// stay within two ticks), and cap-s1 dropped the copy: past F-W the deep-reorg gate runs
    /// `palw_fork_authority_v2::palw_reorg_strict_economic_win_v1` with the processor's
    /// `palw_reorg_shallow_ghostdag_win_v1`, exactly as below it. Staged weight makes all-economic ties
    /// COMMON (a `Created` claim weighs 0, a bond at its cap gains nothing), and the lane's own analysis
    /// found the tie ratchet worse under F-W without the no-DAA-lowering check — so **F-W is never
    /// armable without it**: [`Self::validate_palw_capacity_weight_cap_v1`] refuses F-W unless strict-win
    /// is armed at or below it, and refuses it on a build whose strict-win rule lacks the check
    /// (`palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1`).
    pub palw_capacity_weight_cap: Option<ForkActivation>,

    /// **ADR-0066 Decision 1 — the heartbeat lane.** `None` on every shipped preset.
    ///
    /// Replaces `palw_heartbeat_v1::PALW_HEARTBEAT_LANE_ENABLED`, a `const bool` that changed block
    /// validity and moved no fingerprint: code-only masquerading as a rule, which is a silent fork.
    /// Two builds compiled from the same tree with different values of that constant were peers
    /// that disagreed about which blocks are valid, and nothing in the handshake could say so.
    pub palw_heartbeat: Option<PalwHeartbeatV1>,

    /// **ADR-0066 Decision 3 (finding F2), closed by ADR-0068 Phase 1 — attempt-lane blue work
    /// becomes a constant.** `None` on every shipped preset.
    ///
    /// Fenced separately from `palw_heartbeat` because it moves `header.blue_work` on every
    /// attempt block whether or not the heartbeat lane is armed — but the two are meant to arm
    /// together: ε is only "near-weightless" once the other side of the comparison is real.
    pub palw_attempt_work: Option<PalwAttemptWorkV1>,

    /// **ADR-0072 §3 option (b) / the security amendment — the attempt lane's execution-priced
    /// rule, arriving by activation instead of by re-genesis.** `None` on every shipped preset, so
    /// the behaviour is byte-identical to not having the field at all.
    ///
    /// ADR-0072's rule went live on testnet-11 inside Relaunch 5's re-genesis, which is the one
    /// rollout mainnet may not have (2026-08-27 doctrine: consensus changes ship by activation,
    /// never by re-genesis). Armed, this fence says: **below it the attempt lane is
    /// [`crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2`] carrying the pre-ADR-0072 envelope
    /// version, and at or above it the lane is
    /// [`crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3`] carrying the current one** — two lanes, one
    /// binary.
    ///
    /// **What that buys, and what it does not, measured rather than asserted.** Post-fence blocks
    /// are produced and validated: the template builder declares algo-9, the shipped producer
    /// builds on it, the relay path validates its envelope and its signature, GHOSTDAG prices it at
    /// the attempt lane's ε, and the free-prompt beacon walk still finds it. Pre-fence history is a
    /// different matter, and this field must not be read as claiming it works. The LEGACY arm is
    /// version-fenced on the two peer-facing shape gates and on the relay path's stateless check,
    /// but two things behind them are still compiled in at the current rule: the stateful admission
    /// (`palw_admission_v2::check_palw_attempt_admission_full_v2*` → `validate_stateless_v2`), and —
    /// decisively — the pre-ADR-0072 lottery arithmetic itself, which was deleted at Relaunch 5's
    /// re-genesis and is not restored here. A legacy-arm block therefore cannot pass PoW under this
    /// binary whatever its envelope says.
    ///
    /// So this fence is safe to arm **at genesis** (`ForkActivation::always()`, where `LegacyArm` is
    /// unreachable and `consensus_identity_id` separates armed from un-armed builds on its own),
    /// and arming it at a FUTURE height on a chain with real pre-ADR-0072 history additionally
    /// requires that arithmetic back byte for byte, as §3 option (b) says.
    ///
    /// **`None` is not "off"; it is "un-fenced".** An un-armed network runs the current rule at
    /// every height on algo-6 — exactly what testnet-11 and devnet run today, and why arming this
    /// field is the only thing that changes any of their behaviour. That asymmetry is deliberate:
    /// the fence exists for a chain that has pre-ADR-0072 history to keep, and a chain re-minted
    /// on the new rule has none.
    ///
    /// **A bare fence with no companion value** (the [`Self::palw_frontier_provenance`] rule and
    /// its reason): everything the new lane needs beside the height — the per-class target seed
    /// ([`crate::palw_class_daa::attempt_target_seed_v1`], the same call a post-genesis admission
    /// makes) and the lane-filtered window's first `bits` (a constraint on a filter nobody has
    /// written; no difficulty window in this tree reads `pow_algo_id`) — is DERIVED at the fence
    /// from state the chain already holds (SA-1), never configured. A value sitting beside a fence
    /// is normalised out of
    /// [`Self::consensus_identity_id`], so two builds scheduling this at one height with different
    /// companion values would share an identity, peer, and then disagree the moment it fires.
    ///
    /// **Arming it is a coordinated flag day and the fork-id handshake is what makes it survivable**
    /// (SA-2): see [`crate::fork_id_v1`]. Without it the un-upgraded minority keeps extending the
    /// algo-6 arm past the fence, refuses the majority's algo-9 blocks, and neither side's
    /// handshake says anything is wrong.
    pub palw_attempt_activation: Option<ForkActivation>,

    /// **ADR-0128 — the DNS validators' BFT vote by bonded stake, and the stake reorg gate that
    /// follows it.** `None` on every shipped preset.
    ///
    /// A top-level fence with three values beside it; see [`DnsBftGateV1`]. Read through
    /// [`Self::dns_bft_gate_fence`], which answers only where the network runs an overlay.
    pub dns_bft_gate: Option<DnsBftGateV1>,

    /// **ADR-0066 Decision 4 — the inactivity leak.** `None` on every shipped preset.
    ///
    /// **Removed:** nothing reads it, and [`Self::validate_palw_v2`] refuses `Some`. The field
    /// stays for its Some-only hashing into the consensus fingerprints; the leak lives in
    /// [`Self::dns_bft_gate`] (ADR-0128).
    ///
    /// Replaces `DnsParams.inactivity_leak_daa`, which is retired at `u64::MAX` permanently rather
    /// than reused: see [`PalwInactivityLeakV1`] for why editing that constant could not be an
    /// activation.
    pub palw_inactivity_leak: Option<PalwInactivityLeakV1>,

    /// **ADR-0073 SA-1 — the free-prompt beacon folds `k` attempt blocks.** `None` on every
    /// shipped preset, so the derivation is byte-identical to not having the field at all.
    ///
    /// TOP LEVEL rather than inside the V2 bundle, and `k` rides beside its fence rather than
    /// inside `for_each_fence` — both choices, and the hazards they carry, are argued at
    /// [`PalwBeaconFoldV1`].
    pub palw_beacon_fold: Option<PalwBeaconFoldV1>,
    /// **ADR-0071 security amendment (SA-1..SA-4) — a capability declaration is bounded, priced
    /// and proven.** `None` on every shipped preset, so the behaviour is byte-identical to not
    /// having the field at all.
    ///
    /// Past `activation` four rules arm together, because each is unsound without the others:
    ///
    /// * **SA-1** `capable_classes` may name at most
    ///   [`crate::palw_state_v2::PALW_MAX_CAPABLE_CLASSES`] classes, and a declaration REPLACES
    ///   the set rather than growing it. The set is hashed into `state_root`, so an unbounded
    ///   declaration is a state-growth lever priced only by transaction mass.
    /// * **SA-2** each declared class reserves
    ///   [`crate::palw_state_v2::PALW_CAPABILITY_EXPOSURE_SOMPI`] on the bond's exposure — the
    ///   ADR-0056 Decision 3 shape: reserved, never burned, released when the class is dropped by
    ///   a replacing declaration or the bond retires.
    /// * **SA-3** a seat is drawn for class `C` only after a POSITIVE chain fact — an accepted
    ///   attempt or free-prompt claim on `C` naming that bond — unless `C` is a genesis class.
    ///   Silence is still never judged: a bond that declared and never produced is simply not
    ///   drawn, not charged and not convicted (ADR-0065 D4 stands).
    /// * **SA-4** a declaration is refused for a `Retiring` bond.
    ///
    /// **A bare fence with no companion value, deliberately** — the bound and the price are
    /// constants in `palw_state_v2`, so there is no duration beside this height for
    /// [`Self::consensus_identity_id`] to normalise away (the `palw_bond_maturity` hazard).
    ///
    /// **TOP LEVEL, for the `palw_unavailable_abstains` reason**: this rule must be able to reach
    /// a LIVE network, and a fence inside the V2 bundle moves `palw_ruleset_id_v2`, which
    /// `for_each_fence` never descends into — so every old/new pair would fail the handshake
    /// outright instead of peering with a warning.
    ///
    /// **The liveness trap it must not walk into.** SA-3 narrows the panel draw, and a short draw
    /// is not a smaller panel — it is `InsufficientEligibleBonds`, so the claim never binds and
    /// voids at `BindTimeout` with its escrow burned. The shipped genesis registry has zero slack
    /// by construction, and what keeps it drawable past this fence is the genesis-class exemption:
    /// `the_shipped_registry_still_draws_a_full_panel_with_the_capability_fence_armed` is the
    /// assertion, not a deployment note.
    pub palw_capability_bound: Option<ForkActivation>,
    /// **ADR-0134 — the compute overlay retires at a height.** Past it a block carrying a
    /// compute-certificate, -challenge, -capability, -commitment or -verdict transaction (the VLT's
    /// five subnetworks, ADR-0024) is refused; below it those transactions are accepted and do
    /// nothing, as they always have — their stateful machinery (the committee beacon, the credit
    /// walk, the audit fee, the challenge slash) is gone, and no shipped chain ever carried one.
    /// `None` = never. Hashed `Some`-only; the fork-id gate names it.
    pub palw_compute_overlay_retired: Option<ForkActivation>,
    /// **ADR-0135 Protocol Upgrade A — the permissionless model registry.** Past it a class walks
    /// the lifecycle (`PalwModelLifecycleV1`) on chain-visible facts, its profile is derived from its
    /// registration's graph (`palw_model_registry_v1`), seats judge a class only with a possession
    /// proof (`SeatReadinessProved`), a class the panel cannot serve is `HELD` alone, and the class
    /// shares are written from admission at every span boundary. `None` = never, on every shipped
    /// preset. Hashed `Some`-only; the fork-id gate names it. Needs `palw_panel_economy` and
    /// `palw_execution_lane` at or below its height (`validate_palw_v2`).
    pub palw_model_registry: Option<ForkActivation>,

    /// **ADR-0132 Protocol Upgrade C: the economic payout** — see [`PalwEconomicPayoutV1`]. Resolved
    /// at each block's own DAA score: a claim accepted past it snapshots its economics and is paid
    /// at the rate; the registry reads the cap ceiling at its boundaries. `None` on every shipped
    /// preset.
    pub palw_economic_payout: Option<PalwEconomicPayoutV1>,
    /// **ADR-0137 — the work target.** Past it a model class's ticket is `MAX · min(1, CCU_m / W₀)`
    /// with `W₀ = escrow / rate_max` off the block's own subsidy and `palw_economic_payout`'s
    /// rate; the class shares, the model classes' targets and the epoch budgets are no longer
    /// read; the registry's rows, proofs and lifecycle gate stay, and one network-wide
    /// verification budget replaces the per-class in-flight cap. `None` on every shipped preset;
    /// hashed Some-only; refused without `palw_model_registry` and `palw_economic_payout` at or
    /// below its height. Never set an activation height before the devnet drill has passed.
    pub palw_work_target: Option<ForkActivation>,
    /// **ADR-0132 S — the single lottery.** One forward runs two draws today, the class ticket and
    /// the Layer-0 digest against `bits`, and only the first is credited: testnet-11's measured
    /// producer threw away 79 % of the forwards it had already run. Past this fence a PALW attempt
    /// header's digest is admitted unconditionally, the class ticket is the whole lottery, the
    /// header derives no block level, and its row does not price the difficulty window. `Some` on
    /// testnet-11 at [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] with the anchor clock (one setter); `None`
    /// elsewhere; hashed `Some`-only; refused without `palw_work_target` at or below its height —
    /// with the network draw gone the work target is the only thing left holding the cadence
    /// (ADR-0137 §22 finding 1).
    pub palw_single_lottery: Option<ForkActivation>,
    /// **ADR-0132 §7.6 — the short challenge window, as a fence.** `6fdf6ba7` (2026-09-14) shortened
    /// a licensed claim's challenge window from `window_challenge` (1,200 DAA) to 120 DAA past a
    /// bare height "kept outside the serialized ruleset so the existing fingerprint remains
    /// unchanged" — a consensus rule no fork id or fingerprint could see, and a build without it
    /// would have diverged silently past that height. It is a fence: `Some` on testnet-11 at the
    /// 6,001 flag day beside the model economy it exists for (a Final in days, not weeks, at
    /// testnet-11's pace), `None` elsewhere; hashed Some-only; in the schedule and the fork id like
    /// every fence; mirrored into the V2 bundle's state params
    /// (`PalwStateParamsV2::short_challenge_window_from_daa`) by
    /// [`Params::set_palw_short_challenge_window`], and `validate_palw_v2` refuses a bundle whose
    /// copy disagrees. Set it through the setter, never by assignment.
    pub palw_short_challenge_window: Option<ForkActivation>,
    /// **ADR-0133 Verification V2 (S1, segmented replay) — the protocol.** Past this fence a
    /// segment-scoped receipt set (`ReceiptLicensedV2`) licenses a claim by coverage: the V1 quorum
    /// and every segment of the anchor's cut attested `Valid` twice; the V1 object keeps licensing
    /// by quorum, since a full receipt is a full attestation. `Some(6,100)` on testnet-11, its own
    /// height a hundred DAA past the 6,001 flag day; `None` elsewhere; hashed Some-only; refused
    /// without `palw_model_registry` at or below it. Operator's order (2026-09-18): V1 → S1 → S3 if
    /// needed → S2 if speed wins; no zero-knowledge proof.
    pub palw_verification_v2: Option<ForkActivation>,
    /// **ADR-0133 S3 — post-execution random layer/position sampling.** Past this fence a partial
    /// seat may attest its V2 mask by recomputing the bind-drawn `(layer, position)` sites rather
    /// than the whole assigned segments. Not a licence of its own. `Some` on testnet-11 at
    /// [`PALW_RC_VERIFICATION_S3_FENCE_DAA`]; `None` elsewhere; hashed Some-only; refused without
    /// `palw_verification_v2` at or below it.
    pub palw_verification_s3: Option<ForkActivation>,
    /// **ADR-0133 S2 — optimistic licence from the full-replay seat.** Past this fence a claim may
    /// license on that seat's `Valid` alone (`OptimisticLicensed`); every other seat audits (S3 if
    /// armed, S1 segments otherwise) and files the court if the sample disagrees. `Some` on
    /// testnet-11 at [`PALW_RC_VERIFICATION_S2_FENCE_DAA`]; `None` elsewhere; hashed Some-only;
    /// refused without `palw_verification_v2` at or below it, and without S3 at or below it when
    /// both are armed.
    pub palw_verification_s2: Option<ForkActivation>,
    /// **ADR-0133 §11.2 — readiness V2: a possession proof over the whole artifact.** Past this
    /// fence a seat proves possession with `SeatReadinessProvedV2` — the sixteen leaves the
    /// (class, bond, span) challenge draws from the whole inventory, in one multiproof, signed over
    /// the leaves it opened — the V1 one-leaf object is refused, and only a V2 row counts a seat
    /// ready (for eight spans, not thirty). `Some(6,100)` on testnet-11, the same day Verification
    /// V2 takes; `None` elsewhere; hashed Some-only; refused without `palw_model_registry` at or
    /// below it. H-6 of the 2026-09-18 audit, which the 6,001 bundle deliberately left as V1.
    pub palw_readiness_v2: Option<ForkActivation>,
    /// **ADR-0138 — the DAA score is the anchor's clock.** Past this fence a block advances the DAA
    /// score iff `bits` priced it: the attempt lane (unpriced from the single lottery), the receipt
    /// lane and the heartbeat lane join the round lane outside the clock, while staying merged, paid
    /// and folded. Without it the work target's expectation — 900 model blocks per 1,000-DAA epoch —
    /// is stated in a clock the model lane itself advances, and every DAA-denominated window
    /// (`t_leak_daa`, the receipt and challenge windows, a bond's withdrawal) runs ten times faster
    /// than it was sized (`docs/palw-daa-clock-audit-2026-09-18.md`). `Some` on testnet-11 at
    /// [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] with the single lottery (one setter) — a height past the
    /// live tip so already-committed `daaScore` values are not recomputed; `None` elsewhere; hashed
    /// Some-only.
    pub palw_anchor_clock: Option<ForkActivation>,
    /// **ADR-0142: the consensus clock cursor.** Past it a heartbeat is admissible iff its timestamp
    /// is at or after the cursor, and only a heartbeat moves the cursor — so a chain producing
    /// faster than the interval can no longer suppress the lane. Below it the lane keeps
    /// ADR-0066 Decision 2's selected-parent slot rule, byte for byte.
    ///
    /// **`validate_palw_v2` refuses `palw_anchor_clock` without this at or below its height.** The
    /// anchor clock leaves the heartbeat as the only lane that can advance the score, and under the
    /// old rule an attempt block postponed the next beat — measured on the drill as a DAA frozen at
    /// its own flag day with zero beats minted. Arming one without the other is that failure.
    /// `Some` on testnet-11 at [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] with the anchor clock; `None`
    /// elsewhere; hashed Some-only.
    pub palw_clock_cursor: Option<ForkActivation>,
    /// **The clock floor (the 2026-09-24 heartbeat audit, H3 and H5; ADR-0142's amendment §9).**
    /// Past it the cursor bounds the clock from BELOW as well as above:
    ///
    /// * **H3 — a heartbeat stamped before its slot is invalid.** Past the cursor a beat was
    ///   admissible at any timestamp and earned a DAA only at or past the cursor, so every beat
    ///   minted between two slots was a valid block that ticked nothing — 89% of testnet-12's.
    ///   And a heartbeat CHAIN over the per-mergeset bound (F5's exemption) must be paced by its
    ///   own timestamps: at most two beats a slot, which is all an honest chain can hold.
    /// * **H5 — a block that steps the clock on a beat's grant is stamped at or past the cursor,
    ///   and the reference is the EARLIEST of the tied steps.** Below this fence the step could
    ///   carry any timestamp above the median, so with the 132 s future tolerance a beat stamped
    ///   `ref + 120 s` was granted at once and the step stamped "now" became the next reference —
    ///   nothing floored the spacing; and a future-stamped sibling step tied on blue score won the
    ///   reference on its hash and pushed the next slot back by up to 132 s.
    ///
    /// Refines the cursor, so `validate_palw_v2` refuses it without `palw_clock_cursor` armed at or
    /// below its height. `Some(0)` on testnet-12 only; `None` elsewhere; hashed Some-only.
    pub palw_clock_floor: Option<ForkActivation>,
    /// **The lead cap (the 2026-09-25 mainnet-values review, HIGH; the user's option (a)).** Past
    /// it a header that moves the heartbeat clock — every heartbeat, and the block that steps the
    /// clock on a beat's grant ([`crate::palw_clock_cursor_v1::PalwClockStepV1::lead_capped`]) — is
    /// refused while stamped more than
    /// [`PALW_CLOCK_LEAD_CAP_MS`](crate::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS) (132 s) past
    /// the RECEIVING node's clock. Every other block keeps `timestamp_deviation_tolerance`.
    ///
    /// Why: the floor spaces clock-moving stamps one interval apart but nothing spaced them in wall
    /// time, so at 1,620 s one producer minted `⌊T / I⌋ + 1` = 14 DAA at once and every readiness row
    /// lapsed. Under the cap a burst is 2 steps. A local-clock rule, like the tolerance: the refusal
    /// is not cached as invalid and the header is admitted once wall time catches up. An attacker who
    /// holds the past-median time more than 132 s ahead stalls the DAA clock — steps must be stamped
    /// above the median — until wall time catches up; DAA-denominated windows do not shrink, so no
    /// row lapses (`palw_clock_cursor_v1`'s cap and `t12_clock_floor`'s tests say how). A beat
    /// granted below such a median (withheld through the push, or relayed late) would make every
    /// template a step past the cap — no lane could build — so virtual does not merge it until wall
    /// time catches up (`VirtualStateProcessor::palw_lead_cap_virtual_parents`, a local policy like
    /// the merge-depth filter): every lane keeps building and only the clock waits.
    ///
    /// Armed from genesis wherever the tolerance is 1,620 s — testnet-12 and a mainnet card — and
    /// `None` elsewhere; hashed Some-only, so every other preset fingerprints byte-identically to a
    /// build without it. Independent of the floor: on a network without the anchor clock
    /// (`palw_anchor_clock` — a mainnet card) nothing is ever `granted` and the cap bounds heartbeats
    /// alone.
    pub palw_clock_lead_cap: Option<ForkActivation>,
    /// **ADR-0152 v2, stage F2: a false `Valid` is judged by one adjudicator bound to the claim's
    /// committed root.** Past it the V1 `PanelFalseValid` is refused by name — its
    /// `job_id == claim_id` check is a fixed point no real block-lane or free-prompt claim can
    /// satisfy, and the V1 licence path lets a hand-signed V2 receipt clear it for the state-bound
    /// kinds — and `PanelFalseValidV2` is admitted in its place: the receipt the chain licensed on
    /// (full or segmented, under the chain's own domain), the contradiction pinned to the claim's
    /// `execution_root`, and the seat liable only for the segments its mask attested
    /// ([`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`]). An
    /// execution-proving conviction before `Final` voids the claim `CourtFraud` and slashes its
    /// executor; after `Final` it reverses the `Final`.
    ///
    /// Genesis-only, and refused without `palw_objective_offence`, `palw_audit_2026_09_23`,
    /// `palw_verification_v2` and `palw_economic_safety` armed at or below it: the rule is a
    /// conviction route of the ledger the first arms, and it reads the locks, liability rows and
    /// forfeiture the other three define. `Some(0)` on testnet-12 only; `None` elsewhere; hashed
    /// Some-only, so every other preset fingerprints byte-identically to a build without it.
    pub palw_offence_attribution: Option<ForkActivation>,
    /// **ADR-0152 v3.1 (R-core+): a bond is standing stake, a claim reserves until its licence, and
    /// a reward vests until its conviction window closes.** The v22 skeleton declares the fence and
    /// every field, enum slot, delta entry and rooted map its rules write — and none of the rules:
    /// every S, vesting, M3 and M4 writer is absent, so arming it changes no fold behaviour yet, only
    /// the encodings the v22 layout moves on every network and the ids below.
    ///
    /// Genesis-only, and refused unless every prerequisite the ADR lists (§6 "Prerequisites") is
    /// armed at or below it — `palw_offence_attribution`, `palw_admission_independence`,
    /// `palw_audit_2026_09_23`, `palw_economic_safety`, `palw_objective_offence`,
    /// `palw_panel_economy`, `palw_panel_exposure_floor`, `palw_unavailable_abstains`,
    /// `palw_clock_floor` and `palw_clock_cursor`, `palw_verification_v2`, `palw_da_court` and
    /// `palw_operator_id_unique` — with `palw_settled_anchor_depth` set, `palw_shard_licensing` unset,
    /// the bundle's context root `COMPLETE_V5`, and the bundle's three mirrors equal to the params
    /// (`sync_palw_rcore_plus`). `Some(0)` on testnet-12 only; `None` elsewhere; hashed Some-only
    /// (the `palw_clock_floor` pattern), so every other preset's schedule and ruleset ids are
    /// byte-identical to a build without it.
    pub palw_rcore_plus: Option<ForkActivation>,
    /// **ADR-0152 v3.1 C7's list: the classes R-core+ holds to Final beside the window rule** (the
    /// 2M hold, SR-1 condition 4). Testnet-12: the 2M genesis row's class id. Non-empty only with
    /// `palw_rcore_plus` armed, and a subset of testnet-12's genesis held rows; hashed into the
    /// params id only when non-empty, so every other preset's fingerprint is unchanged.
    ///
    /// A `&'static` slice, not the S spec's `Vec`: `Params` holds no heap field, and the const
    /// presets built by struct update (`..TESTNET_PARAMS`) cannot carry one (a `Vec` gives the type
    /// drop glue a const cannot evaluate). The bundle's mirror is the `Vec` the spec names.
    pub palw_rcore_conservative_classes: &'static [crate::Hash64],
    // ---- lane V02 (post-launch, 2026-09-26): a resolved claim's lock is carried by the whole collateral ----
    /// **Lane V02: a resolved claim's lock leaves the 500‰ work ceiling** (the 2026-09-25 sweep's
    /// V02, HIGH; the user's option (a)). Past it, at a gate's own DAA, the slashable locks a bond holds
    /// on claims that are no longer live (`Final`, voided or retired — the locks
    /// `persist_panel_liability` re-dates to `F + window_court`) are counted against the bond's WHOLE
    /// posted collateral and not against the `fp_max_exposure_ratio_permille` ceiling that gates new
    /// work: the work gate's room is `max(launch, min(ceiling − (committed − resolved), C − R − committed
    /// − accuser))` with `launch` the unsplit room and `R = 4 × min_collateral` the accuser reserve the
    /// relief never spends (52,000 MSK on testnet-12; the V02 review's HIGH — without it binds spent
    /// A-6's free half below one FinalRow DA session, court or held dissection)
    /// ([`crate::palw_state_v2::palw_rcore_gate_room_split_of_v1`],
    /// [`crate::palw_state_v2::palw_bond_accuser_reserve_v1`]). Past it the work readers outside the
    /// fold (the draw's filter, admission item 8, the producer's facts) also read the fold's accuser
    /// ledger, held dissections at their charge. The lock itself, its clocks, its liability row, its
    /// slash and the bond's exit gate are untouched, and `committed + accuser ≤ C` holds at every gate
    /// as before; the accuser gate is unchanged, and relief-admitted work leaves it at least `R`.
    ///
    /// Why: below it an honest `Valid` seat's post-`Final` lock (≈ 160–240 MSK per Final on a
    /// testnet-12 floor claim) sits in the 500‰ budget for `window_court` (3,000 DAA), so at 2–4 floor
    /// claims per DAA every seat's bind room closes within about 600–1,700 DAA of the first Final and
    /// no panel can bind (claims void at the anchor slot, escrow burned).
    ///
    /// Keyed on the evaluating block's DAA (the fold's `ctx.daa_score`, the draw's binding block,
    /// admission's block, the producer's candidate DAA); the rule classifies every lock by its claim's
    /// phase at that DAA, so a lock written below the height is read past it exactly as one written
    /// past it. Refused by `validate_palw_v2` off ConsensusV2, without `palw_rcore_plus` (the one
    /// ledger it splits) armed at or below it, or with the bundle's mirror unsynced
    /// (`sync_palw_final_lock_full_collateral`, which `sync_palw_rcore_plus` also calls). Dormant
    /// (`None`) on every shipped preset, testnet-12 included, until the operator arms the common
    /// post-launch height; hashed Some-only in every writer with the `never()` collapse.
    pub palw_final_lock_full_collateral: Option<ForkActivation>,
    // ---- lane V02 (post-launch, 2026-09-26): the post-Final honest lock life is shortened ----------
    /// **Lane V02: a resolved `Valid` seat's lock lives `F + 1,000` DAA, not `F + window_court`** (the
    /// 2026-09-25 sweep's V02, HIGH; the user's decision 2026-09-26 "both" — this shortening AND V02
    /// option (a), the whole-collateral relief on rcore/f1-v02-lock-budget, are independent lanes).
    /// Past it, at the dating block's own DAA, a `Valid` seat's slashable lock is stamped with a life
    /// of [`crate::palw_state_v2::PALW_FINAL_LOCK_LIFE_DAA_V1`] (1,000, never longer than
    /// `window_court`) in place of `window_court` (3,000 on testnet-12) — at the licence
    /// ([`crate::palw_state_v2`]'s `lock_valid_seat*`, `lock_counted_seat_v1`) and at the `Final`
    /// re-date (`persist_panel_liability`). The lock's `is_live` clock is what the withdrawal gate
    /// ([`crate::palw_state_v2::palw_bond_backs_live_duty_v2`]), `slashable_available` and the seat's
    /// duty backing read, so a seat's capital returns to the 500‰ work ceiling `window_court − 1,000`
    /// DAA sooner — roughly tripling seat-capital throughput on that ceiling.
    ///
    /// **Only the lock's `expiry_daa` moves.** The liability RECORD
    /// ([`crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1`]), the vesting rows that hold the
    /// executor's escrow `E`, the court/DA windows and the obligation-pruning slack all keep
    /// `window_court`, so the conviction path (court, DA default, held dissection, equivocation) still
    /// FIRES through `F + window_court` and the fraudulent mint stays recoverable.
    ///
    /// **Past the height the lock also stops being SLASHABLE when it stops being COMMITTED** (the
    /// user's decision 2026-09-26, closing the per-collateral double-commit the review found: at
    /// `F + 1,000` `slashable_available` released the lock's collateral for new work while both
    /// false-`Valid` conviction routes still took the lock ROW, which stays present to
    /// `F + window_court` and prunes only with the liability record). A conviction in a block past the
    /// height takes a seat's lock only while
    /// [`crate::palw_state_v2::palw_false_valid_lock_slashable_v1`] holds — the per-lock term of
    /// `palw_bond_committed_v1`, both clocks at the escaped depth — keyed on the CONVICTING block's DAA
    /// as the life is keyed on the dating block's. An expired lock is left alone (no S4, no reporter
    /// extraction from it); the rest of the conviction runs unchanged — the `Final` reversed, `E`
    /// burned from its vesting row on the `window_court` clock, the producer's S3. So free + reserved
    /// ≤ posted at every DAA, no uncovered MINT opens, and the seat-side deterrent of a resolved
    /// `Valid` ends at `F + 1,000` (plus the second clock) by design. Below the height every
    /// conviction reads the row exactly as before.
    ///
    /// Keyed on the DATING block's DAA, so a lock stamped below the height keeps the long life when
    /// read past it (the stored `expiry_daa` is what `is_live` reads) and one stamped past it gets the
    /// short life — deterministic across the fence. Refused by `validate_palw_v2` off ConsensusV2,
    /// without `palw_rcore_plus` (whose vesting rows cover `E`) armed at or below it, or with the
    /// bundle's mirror unsynced (`sync_palw_final_lock_life`, which `sync_palw_rcore_plus` also calls).
    /// Dormant (`None`) on every shipped preset, testnet-12 included, until the operator arms the
    /// common post-launch height; hashed Some-only in every writer with the `never()` collapse.
    pub palw_final_lock_life: Option<ForkActivation>,
    // ---- end lane V02 -------------------------------------------------------------------------------
    // ---- lane F2-lock (post-launch, 2026-09-27): the F + 1,000 seat-lock life, applied retroactively ----
    /// **Lane F2-lock: the `F + 1,000` seat-lock life of [`Self::palw_final_lock_life`] reaches the
    /// locks that fence left long** (the user's decision of 2026-09-27; live testnet-12 at DAA 816:
    /// every genesis seat still carried 959–1,583 `Valid` locks dated `F + window_court` by `Final`s
    /// below DAA 750, the first of them expiring at DAA ≈ 3,142, while each seat takes ~2 new locks a
    /// DAA — so the draw's room (`palw_rcore_gate_room_v1`'s `C − R − committed`) closes again around
    /// DAA 1,350–1,575 and ADR-0152 SW-10's eligible-stake floor refuses every draw, voiding the
    /// claims and burning their escrow, as it did at DAA 600–749).
    ///
    /// The lane V02 fence dates a lock at its OWN dating block and re-dates it at `Final` extend-only
    /// (`persist_panel_liability`), so a lock stamped `F + window_court` below that fence keeps it.
    /// Past this one, at height `H`, both halves:
    ///
    /// 1. **The crossing block** — the first chain block whose DAA is at least `H`
    ///    ([`crate::palw_state_v2::palw_final_lock_life_retro_crossing_v1`]) — re-dates, once, every
    ///    seat lock whose claim is an honest `Final` to `min(expiry, max(F + 1,000, H))`, `F` being
    ///    the claim's `Final` DAA as the rooted state records it
    ///    ([`crate::palw_state_v2::palw_lock_final_daa_retro_v1`]); every other field of the lock,
    ///    `settled_at_final` included, is kept, so the second clock still applies. Each rewrite is the
    ///    lock's own `SlashableLock` delta entry in that block, so a reorg across `H` restores the old
    ///    expiries, and every reader of the lock (the room gates, the withdrawal gate, the sweep, the
    ///    slash, the RPC's `bondRoomSompi`) reads the one stored expiry on both sides of the crossing.
    /// 2. **A `Final` at `F ≥ H` re-dates its seat locks to exactly `F + 1,000`**, not extend-only,
    ///    so a claim licensed below DAA 750 (its lock stamped `L + window_court`) that finalizes past
    ///    `H` gets `F + 1,000` too.
    ///
    /// Neither touches a lock of a claim with data-availability history (DA-5's re-keys hold its
    /// locks behind a session's end, which the rooted state does not keep once the session closes),
    /// of a claim voided or of a `Final` a conviction reversed, or of a claim not yet `Final`. The
    /// liability RECORD, its `window_court` expiry and every producer-side clock (the vesting row's
    /// `E` clawback to `F + window_court`) are untouched; a lock always ends at or before its record,
    /// so the sweep's group prune is unchanged. Refused by `validate_palw_v2` off ConsensusV2, without
    /// `palw_final_lock_life` armed at or below it (whose rule — a lock stops being slashable when it
    /// stops being committed — is what keeps a released lock from being taken twice), or with the
    /// bundle's mirror (`sync_palw_final_lock_life_retro`, which `sync_palw_rcore_plus` also calls)
    /// unsynced. Dormant (`None`) on every shipped preset, testnet-12 included, until testnet-12's next
    /// flag day arms it; hashed Some-only in every writer with the `never()` collapse.
    pub palw_final_lock_life_retro: Option<ForkActivation>,
    // ---- end lane F2-lock ---------------------------------------------------------------------------
    /// **ADR-0152 §4-quater: class-derived verification deadlines.** Past it a claim's
    /// compute-bearing deadline `D(c)` is its class's, in the registry's own units
    /// ([`crate::palw_class_verify_deadline_v1`]): the receipt window is `max(window_receipt, D(c))`
    /// with a registry span counted as 5 DAA (V1, V3 — testnet-12 counted it as its 1-DAA lane span,
    /// a fifth of the window); a class whose deadline the chain cannot state without a measurement
    /// (NM: `D > window_receipt`, or held with `n_ctx > 8,192`) takes no claim on any lane until a
    /// measured row names it (V2 — the 2M row is closed at launch, U-D1); `Final` is floored at the
    /// verification horizon `H(c) = bound + D(c) + 1` through one helper at every site that arms it
    /// (V4); a `Valid` lock taken at licence lives to `H + window_court` (V5); a long-D claim's DA
    /// pause moves `H` (V6); and a long-D class owes the panel room until Final (K-1).
    ///
    /// Genesis-only, and refused unless `palw_class_receipt_window` is armed at or below it (the rule
    /// replaces §11.3's product), `palw_fp_derived_work` is too (a free-prompt claim's `D` reads
    /// its class's published work profile, which that fence makes exist before the first such claim),
    /// and `palw_short_challenge_window` is too (long-D is measured against its 120). **Its Some-ness
    /// sets the pruning depth (P-1)**: the claim lattice past it is the D_cap one
    /// (`palw_v2_claim_lattice_daa_v1`, 74,920 on testnet-12), chosen at genesis because a pruning
    /// point can never move backward.
    /// Independent of `palw_offence_attribution`. `Some(0)` on testnet-12 only; `None` elsewhere;
    /// hashed Some-only, so every other preset's ids are byte-identical to a build without it. The
    /// bundle's mirror is made by `sync_palw_class_verify_deadline`.
    pub palw_class_verify_deadline: Option<ForkActivation>,
    /// **ADR-0152 §4-quater's measured rows** (ADR-0153's flag day): the classes whose deadline a
    /// measurement states. Empty on every preset — so at launch every NM class, the 2M row among
    /// them, is refused by name. Non-empty only with `palw_class_verify_deadline` armed; each row
    /// validated on its own ([`crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1::validate`]),
    /// one per class, active no earlier than `palw_offence_attribution` (a class opens only when its
    /// lies are attributable, U-D8); hashed into the params id only when non-empty. A `&'static`
    /// slice for the reason `palw_rcore_conservative_classes` is one.
    pub palw_class_verify_rows: &'static [crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1],
    // ---- lane bind-deadlock (post-launch, 2026-09-26): an attempt at its ceiling still anchors ----
    /// **Lane bind-deadlock (post-launch fence, 2026-09-26; HIGH): an attempt block whose bond is at
    /// its exposure ceiling may still be the anchor — the BINDER — of the claims due at it.**
    ///
    /// Past `palw_rcore_plus` a claim binds its panel only in its own anchor block (ADR-0152 SW-8),
    /// the anchor is the first attempt block at or past `bind_base + anchor_delay` (heartbeats never
    /// anchor), and a chain block whose own attempt fails admission item 8 (`ExposureCeilingExceeded`
    /// against its parent state) is `StatusDisqualifiedFromChain`. So when every attempt-capable
    /// producer stands at its ceiling no anchor can exist: `Provisional` claims hold that very room
    /// until the `BindTimeout` backstop voids them, unpaid, and the cycle can repeat (audit-lifecycle
    /// T12-052, measured in `t12_bind_deadlock`). Past this fence — keyed on the candidate chain
    /// block's own DAA — such a block is not disqualified when it is the anchor of at least one claim
    /// (the processor's `palw_sw8_anchor_for`: a block that anchors, and a claim `Provisional` in its
    /// parent with its slot at or below the highest slot the block anchors) — past the operator-anchor
    /// fence (`palw_operator_anchor`, lane A) only when the block's OWN attempt is an operator's, since
    /// a block that anchors there by merely merging an operator attempt is one a heartbeat can stand in
    /// for — and it passes every other item of the full admission (signature, pins, pwu, class
    /// lottery, identity) with item 8 waived. The fold then treats its attempt exactly as
    /// finding 17's step-4 skip already does (`AttemptExposureCeiling`): no claim, no reservation, the
    /// worker carve withheld (`palw_v2_skipped_own_attempt_carve`) — while its derived bindings bind
    /// every claim due at it. A second binder with nothing left to bind is disqualified as before, so
    /// binders are bounded by the anchor events claims create.
    ///
    /// Refused by `validate_palw_v2` off ConsensusV2 and without `palw_rcore_plus` at or below it
    /// (below R-core+ a heartbeat anchors, so there is no deadlock to break). Dormant (`None`) on every
    /// shipped preset, testnet-12 included, until the post-launch release arms it; hashed Some-only and
    /// collapsed from `Some(never())`, so a build that carries the field fingerprints and peers exactly
    /// as one that does not until the fence is armed.
    pub palw_anchor_at_ceiling: Option<ForkActivation>,
    // ---- end lane bind-deadlock ---------------------------------------------------------------
    // ---- lane accept-order (post-launch, 2026-09-26): a merging block applies its mergeset parents-first ----
    /// **A merging chain block applies its mergeset parents first** (post-launch, 2026-09-26; the
    /// 2026-09-26 IBD root-cause audit's consensus finding).
    ///
    /// Every round block hanging from one anchor carries that anchor's blue work (ADR-0125: a round
    /// block is never blue and never a selected parent, and one whose parents are all round blocks
    /// takes their anchor as its own), so the consensus order — ascending `(blue_work, hash)` — puts
    /// a tied lane in HASH order, a child ahead of its parent. The merging block accepted the lane in
    /// that order: a carrier spending its parent block's output was skipped (testnet-12's merging
    /// block `42285c86…` at DAA 316 accepted three carriers only from later duplicates), a double
    /// spend inside a lane was won by the hash, and a transaction's fee went to whichever round block
    /// sorted first. Past this fence — keyed on the MERGING block's own DAA score — every consumer that
    /// applies the mergeset (UTXO acceptance and with it the accepted-id merkle root, the PALW fold's
    /// carried objects and merged works, the round lane's fee rows, and the EVM lane's
    /// `AcceptedEvmTxs`) reads the consensus order made parents-first by a stable reorder over the
    /// merged blocks' header parents
    /// ([`crate::topological_order::stable_topological_order`]): the identity wherever the consensus
    /// order already is parents-first, so only blocks of a tied lane can move. From the same fence a
    /// round block's own body may not carry an EVM payload (keyed on the round block's DAA score).
    ///
    /// Refused by `validate_palw_v2` off ConsensusV2 and without the execution lane
    /// (`palw_execution_lane`) opening at or below it: there are no round blocks to order. Dormant
    /// (`None`) on every shipped preset, testnet-12 included, until the post-launch release arms it;
    /// hashed Some-only and collapsed from `Some(never())`, so a build that carries the field
    /// fingerprints and peers exactly as one that does not until the fence is armed.
    pub palw_lane_accept_parents_first: Option<ForkActivation>,
    // ---- end lane accept-order ------------------------------------------------------------------
    /// **ADR-0152-adjacent: the Activation Pool, R1 and R2** (user decision 2026-09-25) — see
    /// [`PalwActivationPoolParamsV1`]. `Some` at genesis on testnet-12 only; hashed Some-only.
    pub palw_activation_pool: Option<PalwActivationPoolParamsV1>,
    /// **The readiness-V2 horizon** (user decision 2026-09-25, readiness capacity option (a)) — see
    /// [`PalwReadinessV2MaxAgeParamsV1`]. `Some` at genesis on testnet-12 only (24 spans); hashed
    /// Some-only. The bundle's mirror is made by `sync_palw_readiness_v2_max_age_spans`.
    pub palw_readiness_v2_max_age_spans: Option<PalwReadinessV2MaxAgeParamsV1>,
    // ---- lane F1: registry resilience (the 2026-09-25 pre-launch sweep's V03 and V05) ----
    /// **The registry-resilience fence** (the 2026-09-25 sweep's V03 and V05, the user's decisions of
    /// that evening): testnet-12's post-launch consensus change for the rules that AMPLIFY a readiness
    /// dip into burned escrow and a lost probation. Past it:
    ///
    /// * **V03(1): a no-capable-panel at the anchor slot re-anchors instead of voiding.** A claim its
    ///   anchor block did not bind, whose class's ready seats could not fill a panel on the state the
    ///   draw read (the anchor block's pre-object base, fixed before anything the anchor producer
    ///   chooses to carry), is re-based on that block and retried at its next anchor slot; only the
    ///   bind window's backstop, `accepted_daa + window_bind`, voids it `NoCapablePanel`. Keyed on the
    ///   anchor block's own DAA. ([`crate::palw_state_v2::palw_ncp_retry_is_due_v1`].)
    /// * **V03(2): a class held for readiness keeps its probation progress** and returns to it.
    /// * **V05: a probation resets only on failed probes of two distinct producer bonds.**
    ///   V03(2) and V05 are keyed on the FIRST DAA of the execution span a probe is counted in, which
    ///   is the span a step judges, so no span is judged by one rule with probes noted under the other
    ///   ([`crate::palw_state_v2::PalwStateParamsV2::registry_resilience_governs_span_v1`]).
    ///
    /// Below it every rule is the shipped one, byte for byte. `None` on every preset — testnet-12
    /// included, until the operator arms the common post-launch height (a height no other fence uses,
    /// so the fork id names it) — and hashed Some-only in every writer, so every shipped id is
    /// byte-identical to a build without the field; `Some(never())` collapses to `None` in the
    /// identity. `validate_palw_v2` refuses it off ConsensusV2, below `palw_rcore_plus` or
    /// `palw_model_registry`, or with the bundle's mirror (`sync_palw_registry_resilience`) unsynced.
    pub palw_registry_resilience: Option<ForkActivation>,
    // ---- lane F2 (post-launch, 2026-09-27): a draw the eligible population refuses re-anchors ----
    /// **Lane F2: a claim whose stake draw its anchor block refuses for ELIGIBILITY re-anchors at its
    /// next slot instead of voiding** (the user's decision of 2026-09-27, "void の代わりに次の slot で
    /// anchor し直す"; public testnet-12 voided 172 claims between DAA ~624 and 749 — 168 on SW-10's
    /// eligible-stake floor, 4 on "not enough eligible bonds for a panel" — destroying 550,545.60 MSK of
    /// escrowed worker reward, while every class could still seat a panel).
    ///
    /// Past it — keyed on the ANCHOR block's own DAA, like V03(1) — a claim its anchor block did not bind
    /// is re-based on that block and retried at its next anchor slot, exactly as lane F1's V03(1)
    /// retries a `NoCapablePanel` claim (the same state: `Provisional`, `rebound_daa` the anchor block's
    /// DAA, no panel record; the same backstop `accepted_daa + window_bind`; the same obligation kept),
    /// iff its stake draw refuses for eligibility at EVERY seed on the anchor block's pre-object base —
    /// SW-10's floor (`InsufficientEligibleStake`), fewer eligible operators than seats after the load
    /// filters (`InsufficientEligibleBonds`), or no eligible outsider (`NoOutsider`), with the draw's
    /// own inputs at the claim's anchor fact
    /// ([`crate::palw_panel_v2::PalwStakeDrawCensusV1`], [`crate::palw_state_v2::palw_sw8_thin_draws_v1`]).
    /// The state the draw read is fixed before anything the anchor producer carries, and a verdict
    /// that holds at every seed leaves no panel anyone could have seen and discarded, so no free
    /// re-draw exists. Every other refusal still voids in the anchor block, and a claim that has held
    /// a panel never retries.
    ///
    /// An entry of [`PALW_T12_POST_LAUNCH_FENCES_V2`], testnet-12's next flag day. Refused by
    /// `validate_palw_v2` off ConsensusV2, without `palw_rcore_plus` (the stake draw and SW-8's
    /// anchor-block void it narrows) armed at or below it, or with the bundle's mirror
    /// (`sync_palw_floor_refusal_retry`) unsynced. Dormant (`None`) on every shipped preset, testnet-12
    /// included, until the next flag day arms it; hashed Some-only in every writer with the `never()`
    /// collapse, so a build that carries the field fingerprints and peers exactly as one that does not
    /// until the fence is armed.
    pub palw_floor_refusal_retry: Option<ForkActivation>,
    // ---- end lane F2 ------------------------------------------------------------------------------
    /// **ADR-0143: an artifact root has one owner on the chain.** Past it the chain keeps a rooted
    /// index `artifact_root -> (class_id, line_id, version)`, every entrance where a root enters
    /// state refuses one that is already owned, and the positional lookups that answered "which
    /// line does this root belong to" by whichever `line_id` sorted first in a `BTreeMap` retire.
    ///
    /// ADR-0088's competition is NOT restricted: a different root on the same class, from any bond,
    /// stays permissionless. Only the exact root collides, and the chain never looks inside an
    /// artifact to judge similarity.
    ///
    /// Independent of the 6,301 bundle on purpose — one failure domain each. `None` on every
    /// shipped preset; hashed Some-only, so a build with it unset fingerprints byte-identically to
    /// a build without the field.
    pub palw_artifact_root_ownership: Option<ForkActivation>,
    /// **One operator identity, one bond** (the adversarial audit of 2026-09-19). `operator_id` is
    /// the unit of panel dedup and of the executor exclusion, and nothing in this tree ever verifies
    /// a signature under the `operator_pubkey` it is derived from — so it is a declaration, and a
    /// registrant could name an identity somebody else holds. Past this fence a second registration
    /// of an identity already on the chain is refused by name.
    ///
    /// **And it proves possession.** The registration past this fence carries TWO signatures — the
    /// bond's, then the operator key's over
    /// [`crate::palw_state_v2::PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT`] — so a second identity
    /// costs a second key rather than a second declaration. An ML-DSA-87 signature is fixed length,
    /// so the split is unambiguous and the field that already exists carries both: no new object,
    /// no new state. The two halves arm together because either alone leaves the gap: uniqueness
    /// without possession is a race, possession without uniqueness lets one key hold many bonds.
    ///
    /// `None` on every preset; hashed Some-only.
    pub palw_operator_id_unique: Option<ForkActivation>,
    /// **ADR-0144 §9: an objective offence debits the accused PALW bond.** Past it a verified
    /// `ObjectiveOffence` is consumed into the ledger and a false-Valid signer is slashable.
    /// `None` on mainnet and every other preset. Testnet-11 schedules it at
    /// [`PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA`] as one bundle: lock at Valid, Final keeps
    /// liability, withdraw-before-expiry is refused, PanelFalseValid debits the lock, and
    /// eligibility is `available >= required(claim)` (`gain/3+1`). Hashed Some-only.
    pub palw_objective_offence: Option<ForkActivation>,
    /// **ADR-0133 §7: the seat gate asks possession only.** Past it `required_ready_seats` is the
    /// panel plus the spare — what a claim's verification actually needs held — and the capacity
    /// question it used to swallow is answered by `max_inflight_claims` and
    /// `admission_claims_per_span_milli` against the seats that exist. `None` on mainnet and every
    /// other preset. Testnet-11 schedules it at [`PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA`].
    /// Hashed Some-only.
    pub palw_seat_gate_possession: Option<ForkActivation>,
    /// **ADR-0133 §11.3: the receipt deadline a class's own window derives.** Past it a bound
    /// panel is judged against `max(window_receipt, verification_window_spans × span_daa)` — the
    /// network's floor for a class that already fits, its own window for one that does not.
    /// `None` on mainnet and every other preset. Testnet-11 schedules it at
    /// [`PALW_RC_CLASS_RECEIPT_WINDOW_FENCE_DAA`]. Hashed Some-only.
    pub palw_class_receipt_window: Option<ForkActivation>,
    /// **Spend-once execution-round quanta** (1 CanonicalWork → N unique 1-second permits). Past
    /// this fence a span's schedule mints tickets instead of drawing the ADR-0125 credit lottery.
    /// `None` on mainnet. Testnet-11 schedules it at [`PALW_RC_EXECUTION_QUANTA_FENCE_DAA`]. Hashed
    /// Some-only so a build that has not reached the height still peers with one that has it in
    /// the future schedule.
    pub palw_execution_quanta: Option<ForkActivation>,
    /// A public source URI on new model registrations. testnet-12 sets it from genesis (`palw_t12_base_params`); `None` on the
    /// other shipped presets; hashed Some-only.
    pub palw_public_model_source_required: Option<PalwPublicModelSourceRuleV1>,
    /// **ADR-0145 §1–§5: a claim's work is DERIVED, not declared** (the 2026-09-19 reward audit,
    /// finding F1 — CRITICAL and live). `claim.pwu` is `expected_attempts(class_target) ×
    /// pwu_per_inference`, and `pwu_per_inference` is the step-leaf count of a canonical job the
    /// class REGISTRANT writes and signs. That cancelled correctly while a producer ran the job it
    /// declared; since `palw_prefill_draw` armed at DAA 4,000 an attempt runs
    /// `exact_decode_tokens = 1` and the price still counts every declared decode call, so the
    /// cancellation fails by the ratio between the two — measured at 7.8× the fork-choice weight
    /// for identical arithmetic, 24,572× per unit of arithmetic at the admissible extreme.
    ///
    /// Past this fence the weight, exposure and work-price paths read
    /// [`crate::palw_canonical_work_v1`]'s derivation of the job the draw actually executes, and
    /// the registrant's number is economically inert. Below it every one of those paths is
    /// byte-identical, which `a_dormant_canonical_work_fence_moves_no_number` pins.
    ///
    /// **It is keyed on the CLAIM's `accepted_daa`, not on the block's.** `safe_weight` is a
    /// running total priced once at a claim's `Final` and re-derived at two later chain points
    /// (`retire_claim` and `assert_internal_consistency_v3`); a block-keyed fence would price one
    /// claim under two bases and the state would refuse itself.
    ///
    /// `None` on every preset; hashed Some-only. **Arming it changes the UNIT of `safe_weight` from
    /// step leaves to MAC-equivalents**, so it needs a drill that crosses it before any height is
    /// chosen — ADR-0144 §6 item 0: the design is proven first and the date after.
    /// **ADR-0151's economic-safety bundle, as ONE fence** (the operator, 2026-09-22/23).
    ///
    /// Five rules that are unsound apart and correct together, so they may not arm at five heights:
    ///
    /// * an execution quantum matures before it may be spent
    ///   ([`crate::palw_economic_safety_v1::palw_exec_quantum_matures_at_v1`]);
    /// * a convicted Final's unused quanta are forfeit at every stage that can still hold one;
    /// * `palw_claim_extra_economic_rights_v1` prices the rights a lie can realize before a
    ///   conviction could take them, instead of returning zero;
    /// * a Valid seat's lock carries a tenth of margin rather than one sompi;
    /// * a permit records the quantum it came from, so the rights are traceable to their Final.
    ///
    /// Arming the pricing without the maturity would collateralise rights that are already
    /// recoverable; arming the maturity without the pricing would freeze honest permits and still
    /// leave the residual free; arming the forfeiture without the lineage would have nothing to
    /// forfeit against. Hashed Some-only. `None` on every preset but testnet-12, which arms it from
    /// genesis.
    pub palw_economic_safety: Option<ForkActivation>,
    pub palw_canonical_work: Option<ForkActivation>,
    /// **A class is not judged by its own registrant** (the 2026-09-19 reward audit's F3, ADR-0145
    /// invariants I3 and I4). `palw_panel_eligible_bonds_v2` draws only from bonds that declared
    /// capability — or, under the registry, proved possession — for the class, and both are things
    /// the registrant can do for itself, so a stranger's class is normally judged by the party that
    /// registered it. Past this fence four rules hold at once, because each alone leaves the door:
    ///
    /// * a panel of a registered class must seat at least one bond the registrant does not hold;
    /// * the `Valid` receipts that license a claim must name at least one such seat, so the QUORUM
    ///   and not merely the room is independent;
    /// * a registered class opens `Candidate` — existence, no claims, no weight — and leaves it
    ///   only once an independent seat is ready for it, so admission is earned rather than granted
    ///   by registering (ADR-0145 §7);
    /// * a class that does not admit claims does not enter the work-price unit or take a seated
    ///   class target, so one registration cannot reprice or dilute any other class (I4).
    ///
    /// **Independence is the executor exclusion's own identity triple — bond, `operator_id`,
    /// pubkey — plus the PAYEE.** The triple alone would be a fence that never fires: when a
    /// registrant produces its own class's claims, the executor exclusion has already dropped its
    /// bond, operator and key from the draw, so every seat passes. `payout_payload` is where a
    /// bond's rewards are paid and is already the key the panel payout queue groups by, so reading
    /// it here asks an existing fact a second question rather than inventing an ownership notion.
    ///
    /// It is a price, not a proof: a registrant with several keys paid to several addresses is
    /// several parties on this chain, because the chain cannot see otherwise. **And it has a
    /// deployment precondition a drill must establish before any network arms it** — seats that
    /// share one payout address are one party here, so a fleet paying its seats into one address
    /// would license nothing for a registered class.
    ///
    /// `None` on every preset; hashed Some-only, so a build with it unset fingerprints
    /// byte-identically to a build without the field.
    pub palw_admission_independence: Option<ForkActivation>,
    /// **ADR-0077 Phase B — the court prices the checkpoint, not the context.** `None` on every
    /// shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past this fence, Decisions 10–14 and SA-4 are the rules: the ladder's top is
    /// [`crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`] (`2^32`) rather than
    /// `PALW_STEP_MAX_LEAVES` (`2^22`); a class that registers a state chunk map is priced over its
    /// checkpoint interval rather than over its whole context and is refused the genesis-anchored
    /// long form; registration requires the canonical job's footprint to be at least `n_ctx / 8`;
    /// and the court's move clock is DERIVED from the window and the ladder
    /// ([`crate::palw_context_ladder::palw_court_turn_deadline_v1`]) instead of typed.
    ///
    /// **A bare fence with no companion value, deliberately** — the `palw_frontier_provenance`
    /// rule, for its reason. Every number Phase B installs is a `PalwConsensusParamsV2` field and
    /// therefore already inside `palw_ruleset_id_v2`, which is one atomic commitment to the whole
    /// bundle; a value sitting beside this fence but not inside it would be normalised out of
    /// [`Self::consensus_identity_id`], and two builds scheduling Phase B at one height with
    /// different ladders would share an identity, peer, and disagree the moment it fired.
    ///
    /// Which is also why arming it on a running testnet-11 is a RE-MINT rather than a schedule:
    /// the bundle it selects has a different ruleset id, so the two are different networks by
    /// construction. The fence exists so that fact is stated in the type rather than discovered in
    /// a diff.
    pub palw_context_ladder: Option<ForkActivation>,

    /// **ADR-0077 Decision 16 (P-16): may a free-prompt claim keep its prompt off chain?**
    ///
    /// `PanelDa` (privacy mode 2) lets the commitment transaction carry `prompt_token_ids_hash`
    /// and NO ids; the ids travel with the capture the executor already serves its panel. Dormant
    /// (`None`) on every shipped preset, so every network this build ships refuses mode 2 by name
    /// — the mode is not a default a network acquires by upgrading.
    ///
    /// **Two questions, one field, and they are asked at different times.**
    ///
    /// * [`Self::palw_panel_da_at`] — *is the rule in force at this height?* — is what the
    ///   extraction walk asks. It decides whether a mode-2 commitment becomes a claim, which is
    ///   the EFFECT, and effects are what a height fences.
    /// * [`Self::palw_panel_da_admissible`] — *does this build's ruleset carry the rule at all?*
    ///   — is what transaction admission asks, because isolation validation holds no DAA score
    ///   and never will (its own contract is that it is context-free). This is the shape the
    ///   0x30/0x31 token band already ships under: admitting the bytes is part of a coordinated
    ///   release, and the fence governs the effect.
    ///
    /// The pair is ordered on purpose: `admissible` ⊇ `at`, so the door is never stricter than
    /// the walk and a carrier the walk would credit can always get in — the asymmetry
    /// `validate_palw_fp_commitment_tx` exists to preserve.
    ///
    /// **A bare fence with no companion value, deliberately** — the mode number, the disclosure
    /// sentence and the payload rule are constants in `palw_freeprompt_v3`/`palw_panel_da_v1`, so
    /// there is no duration beside this height for [`Self::consensus_identity_id`] to normalise
    /// away (the `palw_bond_maturity` hazard).
    ///
    /// **TOP LEVEL rather than in the V2 bundle**, for the `palw_unavailable_abstains` reason:
    /// a fence inside the bundle moves `palw_ruleset_id_v2`, which `for_each_fence` never
    /// descends into, so every old/new pair would fail the handshake outright instead of peering
    /// with a warning until the height. ADR-0077 P-16 calls the arming "its own ruleset move";
    /// a scheduled top-level fence is that move without the flag day.
    ///
    /// **What arming does NOT buy** (ADR-0077 SA-5): it is a LICENCE, not a punishment. Until
    /// ADR-0062's data-availability court lands, a producer that withholds the ids reaches
    /// abstention (ADR-0065 D4) → no quorum → one redraw → `ReceiptTimeout` void, and nobody is
    /// slashed. Arming this fence must therefore never be read as arming an enforcement.
    pub palw_panel_da: Option<ForkActivation>,
    /// **ADR-0075 security amendment SA-1/SA-2 — the permissionless lane pays rent.** `None` on
    /// every shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// ADR-0075 Decision 1 made certification objects unsigned and permissionless on the argument
    /// that "the transaction fee is the rent". It was not: nothing on the accepting path ever
    /// looked at what a carrier paid, so both prices were the mempool's flat relay floor.
    ///
    /// * **SA-1.** A chunk group reserves one of `PALW_OBJECT_CHUNK_MAX_GROUPS` slots in the state
    ///   root for `PALW_OBJECT_CHUNK_TTL_DAA` (~5.5 days at 120 s), and a ONE-BYTE chunk declaring
    ///   `count = 8` reserves as much of it as a full one. Past this fence a chunk that OPENS a
    ///   group must be carried by a transaction paying
    ///   [`crate::palw_state_v2::palw_object_chunk_group_rent_v1`] — the carriage price of the room
    ///   it reserves rather than of the bytes it sent.
    /// * **SA-2.** A `FamilyCertified` costs every validator up to
    ///   `PALW_CERTIFICATION_MAX_VECTORS` court re-executions whether it passes or is refused, and
    ///   the vectors are counted from a field the submitter writes. Past this fence an object whose
    ///   carrier paid less than [`crate::palw_state_v2::palw_certification_min_fee_v1`] of its
    ///   vector count is dropped BEFORE it is graded and before it consumes one of the block's two
    ///   grading slots.
    ///
    /// **TOP LEVEL for the reason on `palw_unavailable_abstains`**: this rule must be able to reach
    /// a LIVE network, and a fence inside the V2 bundle moves `palw_ruleset_id_v2`, which
    /// `for_each_fence` never descends into — so every old/new pair would fail the handshake
    /// outright instead of peering with a warning.
    ///
    /// A bare fence with no companion value, deliberately: both prices are derived from constants
    /// this binary enforces, so there is nothing beside the fence to normalise away (the trap
    /// `palw_bond_maturity`'s window documents).
    pub palw_certification_rent: Option<ForkActivation>,
    /// **ADR-0069 Decision 7 — an uncertified class's blocks weigh nothing.** `None` on every
    /// shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past this fence a block whose execution class holds a GRANTED SHARE of zero at that block's
    /// own acceptance point contributes `pwu = 0` to both `safe` and `live`
    /// ([`crate::palw_chain_weight::chain_weights_v1`]), whatever ramp stage it has reached. The
    /// block is otherwise untouched: it still advances DAA, it is still paid its budgeted subsidy
    /// (ADR-0039's "admissible for liveness"), and its claim runs the same lattice. Weight is what
    /// certification buys.
    ///
    /// **Why share is the predicate and not a family lookup.** Admission enforces the converse:
    /// `palw_class_admission_v2::verify_class_admission_v3` returns `NotEndToEndCertified` when a
    /// class asks for `share > 0` and no end-to-end certified family covers its reachable kernels,
    /// so `share > 0 ⇒ certified` by construction. That is the same proxy ADR-0069's own Gate 3-3
    /// uses, and it is the only one available in the weight path — the class state carries no
    /// family and the shape profile is not reachable there.
    ///
    /// **What it closes.** Registration is permissionless and an uncertified family registers at
    /// share 0; a post-genesis entrant's `initial_target` is pinned to the base class's LIVE target
    /// (≈ 12,663 expected draws on testnet-11 5e); `pwu_per_inference` is admitted up to
    /// `PALW_STEP_MAX_LEAVES = 2²²`; a share-0 class still gets the one-block epoch budget floor
    /// (`derive_epoch_budgets_v2`); and an uncertified family cannot be convicted, so each draw
    /// costs one hash rather than one inference. One fabricated block per epoch therefore carried
    /// ≈ 8× the honest network's whole-epoch weight into `safe(C)` — the weight the IBD and
    /// deep-reorg gates read.
    ///
    /// **What else arming carries, because one switch has to leave a state its own check accepts.**
    /// `PalwChainStateV2::retire_claim` also folds a retiring FREE-PROMPT claim's spent quanta into
    /// `retired_safe_weight` past this fence. That is not part of Decision 7 — the free-prompt lane
    /// is not priced by share in either direction — it is the repair of a pre-existing hole: the
    /// spend added `pwu / quanta` to `safe_weight`, the retirement dropped the claim without
    /// booking it, and the stranded weight made `assert_internal_consistency_v2` refuse the node's
    /// OWN durable tip on the next restart (`load_tip` → `CarriageInconsistent`), and every peer
    /// importing that pruning-point snapshot with it. It rides this fence rather than a second one
    /// because `retired_safe_weight` is hashed into `palw_state_root`, so the repair moves a root
    /// and cannot be unfenced — and because a network that arms Decision 7 without it would fail
    /// the very bound Decision 7 installs. **Dormant, that hole is still open**: it is reachable on
    /// any ruleset with `claim_retirement_daa > 0` (the shipped RC sets it to `WINDOW_COURT`, 3000)
    /// as soon as one free-prompt quantum is spent on a claim that later retires.
    ///
    /// **A bare fence with no companion value**, for `palw_frontier_provenance`'s reason: the
    /// predicate is the share the chain already records, so there is no threshold to configure and
    /// nothing that could be normalised out of [`Self::consensus_identity_id`] and then disagree.
    ///
    /// **TOP LEVEL for the reason on `palw_bootstrap_activation`**: this is a fork-choice rule that
    /// has to reach a live network by rolling deploy, and a fence inside the V2 bundle moves
    /// `palw_ruleset_id_v2` — which `for_each_fence` never descends into — so every old/new pair
    /// would fail the handshake outright instead of peering with a warning.
    ///
    /// **GENESIS-ONLY, and [`Self::validate_palw_v2`] refuses anything else.** The paragraph above
    /// is why the fence is placed where it is; it is NOT permission to schedule one. Arming closes
    /// the free-prompt stranding going FORWARD and repairs nothing behind it: the stranded weight
    /// sits in `safe_weight` with its claim already dropped, so `safe_ceiling` is below it and the
    /// armed bound refuses exactly as the dormant equality did; the fold carries both scalars past
    /// the activation unchanged; and a node holding such a tip cannot start, so it never reaches
    /// the activation height at all. Measured in all three positions by
    /// `arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight`. A chain that has
    /// already retired a spent free-prompt claim below this fence is repaired by a re-mint or not
    /// at all — so the only arming that is a remedy rather than a second failure is one at the
    /// genesis of a fresh relaunch, and the validator makes that the only arming that starts.
    pub palw_uncertified_weightless: Option<ForkActivation>,
    /// **ADR-0062 as amended (SA-1…SA-6) — the data-availability court.** `None` on every shipped
    /// preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Past this fence a claim's data obligation becomes prosecutable by ARITHMETIC instead of by
    /// a vote: [`crate::palw_state_v2::PalwConsensusObjectV2::DefaultAccused`] opens a singular,
    /// bonded accusation naming ONE trace index, the claim moves to
    /// [`crate::palw_state_v2::PalwClaimPhaseV2::DefaultDisputed`], and
    /// [`crate::palw_state_v2::PalwConsensusObjectV2::MaterialDisclosed`] refutes it with a Merkle
    /// opening against the claim's own pinned trace root — checked by hash arithmetic, never by
    /// running a model. Silence past `W_disclose` confirms the default and takes `claim.reserved`.
    ///
    /// **What it does NOT do, and this is the whole reason it exists.** The `ProducerDefaulted`
    /// quorum verdict it was written around is unreachable on a D4-armed preset
    /// (`palw_unavailable_abstains`, armed from genesis on testnet-11 since Relaunch 5): the arm
    /// returns `ProducerDefaultRetired` before it reads a single receipt. So the HARM — a captured
    /// panel taking a producer's bond on a vote — is already prevented, and what this fence adds is
    /// the missing positive half: a default that can take a bond because somebody proved it.
    ///
    /// **TOP LEVEL for the reason on `palw_bootstrap_activation`**, and load-bearing here for the
    /// reason on `palw_unavailable_abstains`: this rule has to be able to reach a LIVE network, and
    /// a fence inside the V2 bundle moves `palw_ruleset_id_v2`, which `for_each_fence` never
    /// descends into — every old/new pair would fail the handshake outright instead of peering.
    ///
    /// **Arming it is a coordinated re-mint, not a rolling upgrade.** Past the fence a claim record
    /// can hold `DefaultDisputed` and a block can carry two object variants an older build cannot
    /// decode, so the state schema — not merely the rules — moves.
    /// [`crate::palw_state_v2::PALW_STATE_V2_VERSION`] must move in the SAME release that arms
    /// this on any network, and the pins move with it. It stays where it is while the fence is
    /// `None` everywhere, because a rule that cannot fire changes no state.
    ///
    /// **DO NOT ARM THIS WITHOUT A DISCLOSURE RESPONDER IN THE FIELD.** The consensus half is
    /// complete — the objects, the phase, the arithmetic, the sweep — and nothing in this tree
    /// CONSTRUCTS a `MaterialDisclosed`. A window that convicts on silence is only sound when the
    /// silent party could have spoken, and audit3 H4 already found this exact shape once: the
    /// court's opening rung convicted producers of classes for which no responder existed, so
    /// accusing was a guaranteed win and honest execution was punished. Armed with no responder,
    /// every accusation succeeds and every producer is slashable for the price of one bond —
    /// strictly worse than the captured-panel defect this ADR was written to close. The producer
    /// side (a panel/producer service that answers an open accusation from the capture it already
    /// holds, as `palw_panel.rs` answers `CourtDisclosed`) is the precondition for arming.
    pub palw_da_court: Option<ForkActivation>,
    /// **ADR-0084 U-08 — the court walks a refutation at the RULESET's step ladder, not the
    /// default 2^22.** `None` on every shipped preset, so a build carrying this field and one
    /// predating it are one network. Before the fence a close proof against a class whose
    /// step space exceeds `PALW_STEP_LEG_MAX_LEAVES` is refused at the leg (`LeafCountOutOfRange`)
    /// — the court cannot adjudicate a graph-v5 close at all; the panel's node-local verdicts
    /// (ADR-0086) already walk the wider ladder. Past the fence the walk is bounded by
    /// `PalwCourtParamsV2::max_step_leaf_count()` — the SAME ladder the class registration was
    /// judged against — and a close proof that was once an error becomes a verdict, which is why
    /// this is a fence and not a bug fix: nodes on either side would grade the same proof
    /// differently. Read it through `palw_court_ladder_fence` only.
    pub palw_court_ladder: Option<ForkActivation>,
    /// **ADR-0072 Decision 8 on the FREE-PROMPT lane: the DA obligation is the chain's, not the
    /// producer's** (mainnet audit, 2026-09-06, M-3). `None` on every shipped preset, so a build
    /// carrying this field and one predating it are one network.
    ///
    /// Decision 8 pinned the three DA fields on the attempt lane by equality
    /// (`palw_admission_v2::check_palw_attempt_da_pins_v1`, and the header stage above through
    /// [`Self::palw_attempt_header_pins`]) and the free-prompt lane never got it. Two of the three
    /// consequences are real:
    ///
    /// * `trace_retention_daa` is stored verbatim and is the ONLY bound on when a `DefaultAccused`
    ///   may be filed (`palw_state_v2`'s `DaOutsideRetention` arm). A producer writing `0` is
    ///   permanently immune to the DA court at zero cost — and writing `0` is **weakly dominant
    ///   for an honest producer too**, because it also immunises against griefing accusations that
    ///   pause the claim, so the field drifts to zero network-wide with no adversary at all and the
    ///   court a card arms becomes decorative.
    /// * `trace_chunk_count` is bounded only by `!= 0` while it decides
    ///   `palw_da_max_accusable_rows_v1`. It is honest at `1` for every run inside
    ///   `PALW_FP_TRACE_CHUNK_EVENTS_V3`, so this half is inert at shipped budgets and is pinned
    ///   for the reason Decision 8 gives rather than for a live consequence.
    ///
    /// **Why a DERIVATION and not an equality, unlike the attempt lane.** An attempt rides its
    /// block's header and knows that block's DAA score, so Decision 8 can ask for equality. A
    /// free-prompt commitment rides an ordinary transaction built before anyone knows which block
    /// will accept it, so an equality would be unsatisfiable. Past this fence the transition WRITES
    /// the obligation — `accepted_daa + palw_min_trace_retention_daa_v1` — onto the claim record
    /// and the carried value decides nothing. Same decision, at the only layer holding the input.
    ///
    /// **`trace_manifest_root` is deliberately NOT in scope.** On this lane it is
    /// `fp_trace_manifest_root_v3` over the per-chunk EVENT digests, which the chain does not hold
    /// and cannot recompute — and nothing in consensus reads the field on either lane, because the
    /// extraction walk does not carry it onto `PalwConsensusObjectV2` at all. Pinning it would mean
    /// carrying it there and giving the DA court something to bind a disclosure to, which is a
    /// `PALW_STATE_V2_VERSION` move belonging to the record's owner. Recorded, and pinned by test
    /// (`the_free_prompt_lanes_trace_manifest_root_is_carried_by_nothing`) so the gap can neither
    /// widen nor close silently.
    ///
    /// **A bare fence with no companion value, deliberately** — the [`Self::palw_context_ladder`]
    /// rule for its reason: the retention length is `palw_min_trace_retention_daa_v1` of the
    /// bundle's own windows and the chunk size is a module constant, so there is no duration beside
    /// this height for [`Self::consensus_identity_id`] to normalise away (the `palw_bond_maturity`
    /// hazard).
    ///
    /// **TOP LEVEL rather than in the V2 bundle**, for the `palw_unavailable_abstains` reason: a
    /// fence inside the bundle moves `palw_ruleset_id_v2`, which `for_each_fence` never descends
    /// into, so every old/new pair would fail the handshake outright instead of peering.
    pub palw_fp_da_pins: Option<ForkActivation>,

    /// **ADR-0018 §E's payout paths obey their own stated bounds** (mainnet audit, 2026-09-06 —
    /// H-2, H-3, M-1).
    ///
    /// Five rules, ONE fence, because they are one decision surface: §E states every one of them
    /// and the code stated none. Every one changes the bytes `validate_coinbase_transaction`
    /// compares by exact hash, so splitting them across fences would only make four extra ways to
    /// arm half a rule.
    ///
    /// 1. `validator_quality_bonus_outputs` stops at the pool — the whole-output cap its sibling
    ///    `validator_participation_reward_outputs` has carried since the 112-block halt, and §E's
    ///    own "Unspent → rollover … never the whole pool".
    /// 2. …and refuses an epoch whose `Σ included stake` exceeds its `expected_stake`. That is the
    ///    precondition the function's own doc ASSERTS and nothing enforced;
    ///    `epoch_meets_quality_floor` fails OPEN in the same direction, because a fraction above 1
    ///    is above any floor.
    /// 3. …and emits at most [`crate::palw_state_v2::PALW_V2_MAX_DEFERRED_VALIDATOR_PAYOUTS`]
    ///    outputs per BLOCK, across every epoch that block finalizes — the fan-out is
    ///    (epochs) × (included), not (included).
    /// 4. The reserve drip emits at most [`crate::palw_state_v2::PALW_V2_MAX_RESERVE_DRIP_PAYOUTS`]
    ///    per block, and spends `reserve_drip_per_epoch_cap_sompi` per BLOCK rather than per
    ///    crossing: a rate limiter whose rate must not depend on how many thresholds one wide
    ///    mergeset happens to cross.
    /// 5. The coinbase isolation cap counts every kind the builder appends
    ///    ([`crate::palw_state_v2::PALW_V2_COINBASE_EXTRA_OUTPUTS_BOUNDED`]) instead of three of
    ///    six.
    ///
    /// Rules 1-4 TIGHTEN what a coinbase may contain; rule 5 RELAXES the size guard ahead of it.
    /// `None` on testnet-11, devnet and simnet — every coinbase they build stays byte-identical,
    /// and a `None` writes nothing into `consensus_params_id` or `consensus_schedule_id`, so their
    /// fingerprints do not move. `always()` on a card, which has no history to fork.
    ///
    /// **TOP LEVEL rather than on `DnsParams`**, for the `palw_fp_da_pins` reason and one more:
    /// [`Self::consensus_params_id`] hashes `dns_params` as one length-prefixed borsh blob, so a
    /// new `DnsParams` field would move every preset's fingerprint, testnet-11's included.
    ///
    /// Read through [`Self::palw_validator_payout_bounds_fence`] only.
    pub palw_validator_payout_bounds: Option<ForkActivation>,

    /// **MSK-26A (2026-09 pre-freeze security review): the UTXO side-effect of a DNS slash obeys the
    /// same genuineness rule the bond-registry mutation already does.**
    ///
    /// The bond-REGISTRY path (`dns_bond_mutations_from_txs`) filters a chain block's accepted
    /// slashing / precommit evidence through `proved_slash_targets`, which re-verifies both
    /// attestation signatures against the accused bond's registered validator key — so evidence
    /// riding in a MERGE-blue block (which the own-body `check_slashing_evidence_genuine` block rule
    /// never sees) can no longer flip a bond to `Slashed`. But the UTXO SIDE-EFFECT path
    /// (`apply_slashing_side_effects` -> `resolve_slashing_side_effects`) removes the accused bond's
    /// locked output-0 UTXO and mints the reporter reward WITHOUT that filter and without any
    /// signature check, so a forged evidence merged into a block burns an honest validator's stake
    /// and pays its author a reward while the registry still reads the bond `Active`. Past this
    /// fence, `apply_slashing_side_effects` keeps only the effects whose bond is `proved_slash_targets`
    /// — the same signatures, freshness and status the registry path checks — so a slash reaches the
    /// UTXO set only on evidence the accused validator actually signed.
    ///
    /// **A bare fence with no companion value**, and `None` on every shipped preset — a `None` writes
    /// nothing into `consensus_params_id` / `consensus_schedule_id`, so the fingerprint does not move
    /// and the behaviour is byte-identical to not having the field at all. testnet-12 arms it at the
    /// post-launch fence; a card arms it from genesis (no history to fork). Read through
    /// [`Self::palw_slashing_evidence_utxo_genuine_at`] only.
    pub palw_slashing_evidence_utxo_genuine: Option<ForkActivation>,

    /// **ADR-0045 Decision 2's boundary sentence, made real — the crossing block derives its own
    /// epoch's budget from the parent state** (mainnet audit 2026-09-06, M-2). `None` on every
    /// shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// Dormant, admission reads the stored snapshot and refuses anything else by name: at every
    /// epoch boundary the stored table is stamped with the CLOSED epoch while `ctx.daa_score` is in
    /// the new one, so `EpochBudgetUnspecified` refuses every non-floor attempt the boundary block
    /// carries or merges. The chain block is disqualified and — because `sink_search_algorithm`
    /// discards it rather than deferring it — permanently orphaned, and the claims, escrow and
    /// worker carves of every non-floor attempt it merged are gone. The floor, the heartbeat lane
    /// and the receipt lane are exempt, so this is liveness and fairness, never a halt.
    ///
    /// Past the fence, admission calls [`crate::palw_state_v2::palw_epoch_budgets_for_v2`] for the
    /// block's own epoch — the same pure function the fold's `ensure_epoch_budgets` calls — which is
    /// the ADR's sentence verbatim: "for the crossing block itself (whose apply has not happened
    /// yet) it derives from the parent state".
    ///
    /// **A bare fence with no companion value**, the [`Self::palw_frontier_provenance`] rule for its
    /// reason: every number the derivation uses (`epoch_length`, `budget_tolerance_permille`) is
    /// already a `PalwStateParamsV2` field and therefore already inside `palw_ruleset_id_v2`.
    ///
    /// **TOP LEVEL rather than in the V2 bundle**, the [`Self::palw_unavailable_abstains`] reason:
    /// this rule has to be able to reach a LIVE network, and a fence inside the bundle moves
    /// `palw_ruleset_id_v2`, which `for_each_fence` never descends into — every old/new pair would
    /// fail the handshake outright instead of peering with a warning until the height.
    ///
    /// **The residual, stated rather than hidden.** The ADR asserts the parent-state derivation and
    /// the stored snapshot "cannot disagree". On this tree they can: `apply_class_share_growth`
    /// (step 2c) and `apply_class_reclamation` (step 2d) run before `ensure_epoch_budgets` (step
    /// 3b), so with `class_growth_permille != 0` the fold's table is derived from a share table the
    /// pre-check has not seen. The disagreement is bounded — one block's budget, at one boundary,
    /// for one class — and is pinned by
    /// `the_boundary_derivation_and_the_folds_snapshot_can_differ_when_growth_is_on`.
    pub palw_epoch_boundary_budget: Option<ForkActivation>,

    /// **ADR-0123: a spent class budget borrows the epoch slots nobody else is filling.**
    ///
    /// The census budget (ADR-0045 Decision 2) caps each non-floor class at its share of an
    /// epoch, and the floor's exemption is what is supposed to keep the chain moving past a cap:
    /// "the floor can always produce, so DAA always advances". That is true of what the floor is
    /// PERMITTED to do and false of what it DOES on a network where nobody wins its ticket — on
    /// testnet-11 on 2026-09-12 a class exhausted 367 of 367 at DAA ~3,761, the only other active
    /// producer drew once every eight minutes, and the chain fell from ~55 to ~4 blocks an hour
    /// for the ~199 DAA left before the budget refilled.
    ///
    /// Past this fence a class that has spent its budget may keep producing into the slots of the
    /// epoch that have ELAPSED and that the other classes have NOT filled — pro rata by DAA, so it
    /// can never take a slot early, and measured against the epoch's length rather than the sum of
    /// the budgets, so truncation cannot leave a last slot nobody may fill. See
    /// [`crate::palw_state_v2::palw_epoch_budget_release_v1`]. `None` on every shipped preset.
    pub palw_epoch_budget_release: Option<ForkActivation>,

    /// **ADR-0124 Decisions 1–5: the panel economy.** Past this fence a panel's seats are paid
    /// out of the claim's own reward (20 % of it, one fixed share per drawn seat, to the seats
    /// whose `Valid` receipts the chain credited inside the receipt window; the unpaid shares go
    /// to the panel reserve, never to the producer), every drawn seat reserves three times the
    /// claim's exposure on its bond for the claim's life and loses exactly that for contradicting
    /// its panel's quorum, a bond is drawn only while it holds ten producer floors and its free
    /// collateral covers the reservation, the draw is one ticket per eligible bond, and a seat
    /// may carry its own `Valid` receipt after the licence until the receipt deadline. Resolved at
    /// the block's DAA for the fold and at the claim's ANCHOR for the draw (the panel is a pure
    /// function of the claim). No new issuance: the split is of the escrow the accepting block
    /// already withheld. `None` on every shipped preset; a mainnet card states it from genesis.
    pub palw_panel_economy: Option<ForkActivation>,

    /// **ADR-0124 Decision 6: a claim is paid for the compute it certifies.** Past this fence a
    /// `Final` claim of a model class is named the fraction of its escrow that its class's
    /// canonical inference (`pwu_per_inference`, the pwu its exposure is priced on) is of the
    /// heaviest `Active`, weight-bearing model class's at the paying block, and the rest is never
    /// named — never minted, exactly as a voided escrow is. The liveness floor is not a model and
    /// is paid at the schedule. Resolved at the block's DAA. `None` on every shipped preset; a
    /// mainnet card states it from genesis.
    pub palw_work_priced_reward: Option<ForkActivation>,

    /// **ADR-0125: the execution lane** — see [`PalwExecutionLaneV1`]. Resolved at a round block's
    /// own DAA score for its admission, and at the merging block's for everything the chain does
    /// with it. `None` on every shipped preset.
    pub palw_execution_lane: Option<PalwExecutionLaneV1>,

    /// **ADR-0126: the validator carve drops to a fifth** — see [`PalwOverlayCarveV1`]. The split is
    /// resolved at each block's own DAA score, the escrow at the lower of the DAA scores of the block
    /// that carried the attempt and the block that pays it ([`palw_overlay_escrow_carve_at_v1`]).
    /// `None` on every shipped preset.
    pub palw_overlay_carve: Option<PalwOverlayCarveV1>,

    /// RFC-0012 coordinated retirement and settlement policy. Unassigned on every shipped network.
    pub palw_dns_retirement: Option<crate::palw_native_settlement_v1::PalwDnsRetirementV1>,

    /// RFC-0010 permissionless Panel policy. Dormant; every activation is refused until its beacon and release gates pass.
    pub palw_permissionless_panel_v1: Option<crate::palw_permissionless_panel_v1::PalwPermissionlessPanelV1>,
    /// RFC06 complete-population/100k experiment. No shipped preset assigns it; activation is refused.
    pub palw_permissionless_panel_complete_v1: Option<crate::palw_permissionless_panel_v1::PalwPermissionlessPanelCompleteV1>,
    /// Native retirement population compaction with permanent work tombstones; dormant and unapproved.
    pub palw_permissionless_panel_retired_v1: Option<crate::palw_permissionless_panel_v1::PalwPermissionlessPanelRetiredV1>,

    /// **ADR-0130: the seat exposure floor** — see [`PalwPanelExposureFloorV1`]. Resolved at the
    /// claim's anchor for the draw and at the binding block's DAA for the reservation, which the
    /// duty row stores. `None` on every shipped preset and on every card.
    pub palw_panel_exposure_floor: Option<PalwPanelExposureFloorV1>,

    /// **ADR-0044 Decision 9's two advertised caps, enforced** (mainnet audit 2026-09-06, L-2).
    /// `None` on every shipped preset, so the behaviour is byte-identical to not having the field.
    ///
    /// `PalwFreePromptParamsV3::max_prompt_tokens` and `max_decode_tokens` are borsh-hashed into
    /// `palw_ruleset_id_v2` — every node advertises them in its fingerprint — and no code read
    /// them: `validate_v3` never took the params struct, and the surviving bounds were transaction
    /// mass and the step ladder. Past this fence the ruleset-aware validation entry point carries
    /// them and a job above either is refused by name.
    ///
    /// **Dormant on testnet-11 and devnet, and that is the whole reason it is a fence.** Both have
    /// run since genesis with these numbers unread; enforcing them retroactively would refuse jobs
    /// those chains already accepted. Arming them there is a scheduled height and the operator's
    /// call. A card is born with the caps it advertises meaning something.
    ///
    /// **A bare fence with no companion value** — the two numbers are already
    /// `PalwFreePromptParamsV3` fields and therefore already inside `palw_ruleset_id_v2`.
    pub palw_fp_ruleset_caps: Option<ForkActivation>,

    /// **ADR-0087 Decision 6 — the model market is a consensus rule armed by activation.** `None`
    /// on every shipped preset: below it `ModelBuy`/`ModelSell` are refused by name at acceptance
    /// and no market exists; past it the fold opens a class's market on its first buy. The
    /// fingerprint moves only where this is set. Read through `palw_model_market_fence` only.
    pub palw_model_market: Option<ForkActivation>,
    /// **ADR-0088 Decision 11 — the model registry is a consensus rule armed by activation.**
    /// `None` on every shipped preset: below it the ten registry objects are refused by name at
    /// acceptance, a class has one root and no claim is attributed; past it every class's
    /// founding line exists and its developer publishes. Independent of `palw_model_market`.
    /// The fingerprint moves only where this is set. Read through `palw_model_lines_fence` only.
    pub palw_model_lines: Option<ForkActivation>,
    /// ADR-0175: independent immutable-model registration fence. Absent on shipped networks.
    /// Past it model content, identity and verification bindings cannot be replaced; evaluation
    /// may select a separately registered candidate but cannot move an existing lineage head.
    pub palw_model_immutable_v1: Option<ForkActivation>,
    /// Independent G14 rule activation; absent preserves historical consensus.
    pub palw_class_semantics_v1: Option<ForkActivation>,
    /// Independent G14 rule activation; absent preserves historical consensus.
    pub palw_court_commitment_scope_v1: Option<ForkActivation>,
    /// Prospective staged proof contract. Shipping presets remain dormant; old claims keep
    /// their original court, collateral and material horizons.
    pub palw_staged_public_court_v1: Option<ForkActivation>,
    /// **ADR-0095 §4.11 as corrected: the membership needs its OWN fence.**
    ///
    /// The first draft put it under `palw_model_lines`, which is wrong on any chain where the
    /// registry is already live: ADR-0095 writes two new collections that enter the state root
    /// (the declarations and the tenure clocks), so switching it on with the registry's fence
    /// would move the root under a running network. It is a consensus change and gets an
    /// activation of its own, like every other one.
    pub palw_model_benefits: Option<ForkActivation>,
    /// **ADR-0114 — the owner's leg becomes five percent, by activation.** `None` on every shipped
    /// preset: below it every join and leave is split 5 % burned / 1 % to the line's owner (ADR-0087
    /// Decision 4), past it 5 % / 5 % (`PalwModelFeesV1::V2`). It changes what the fold writes into
    /// the market rows and the owner's payouts, so it is a consensus change on a live chain and
    /// arrives at a height — a node on a build without it forks off there. Meaningless without the
    /// market, so it is read through `palw_model_leg_v2_fence` only, which folds that in.
    pub palw_model_leg_v2: Option<ForkActivation>,
    /// **ADR-0120 — the least seed is one million MSK, by activation.** `None` on every shipped
    /// preset: below it a line's market opens once the MSK paid into it reaches
    /// `PALW_MODEL_SEED_MIN_SOMPI_V1` (100,000 MSK, ADR-0090 and ADR-0094's pledges), past it
    /// `PALW_MODEL_SEED_MIN_SOMPI_V2` (1,000,000 MSK). It decides which payment opens a market —
    /// what the fold writes into the rows — so it is a consensus change on a live chain and arrives
    /// at a height; a node on a build without it forks off there. Meaningless without the market,
    /// so it is read through `palw_model_seed_v2_fence` only, which folds that in.
    pub palw_model_seed_v2: Option<ForkActivation>,
    /// **ADR-0162 — the pair opens on a virtual reserve, by activation.** `None` on every shipped
    /// preset. Past it every registered line's market is open from the line's creation (a line older
    /// than the fence: from the fence) on `PALW_MODEL_MARKET_VIRTUAL_SOMPI_V2` — 10,000,000 MSK that
    /// prices and is never paid — with no seed required: ADR-0120's least seed is zero, a pledge still
    /// collecting ADR-0094's floor opens with the pledge as its seed, and a seed is optional depth,
    /// taken only before the line's first trade. Buys (and the reward's buyback) need the class
    /// `Active` in status and lifecycle; sells drain as before. A market opened before the fence keeps
    /// its curve. It changes what the fold writes into the rows, so it is a consensus change and
    /// arrives at a height. Meaningless without the market, so it is read through
    /// `palw_model_virtual_v1_fence` only, which folds that in. **Some-only everywhere** — the params
    /// id, the schedule id and `for_each_fence` (lane sink's shape) — so no preset's fingerprint, fork
    /// id or schedule moves while it is `None`, and its `Some(never())` collapses to `None`.
    pub palw_model_virtual_v1: Option<ForkActivation>,
    /// **ADR-0089 Decision 9 — the market's EVM face is a consensus rule armed by activation.**
    /// `None` on every shipped preset: below it the four system addresses and the facades are
    /// empty accounts, the writer accepts nothing and the transition takes an empty action list;
    /// past it the EVM reads the fold and its actions are applied after the block. Refused by
    /// `validate_palw_v2` unless `palw_model_market` is armed at or before it and the EVM lane is
    /// active. Read through `palw_model_evm_fence` only.
    pub palw_model_evm: Option<ForkActivation>,
    // ---- lane sink (post-launch, 2026-09-26): a model sink is block-valid only bound ------------
    /// **Lane sink: the model sink binding** (the 2026-09-25 Position review's #1, post-launch fence).
    /// In a block whose DAA score is at or past it, every `OP_RETURN "MSKMDL01" <line>` output must
    /// be the one its lifecycle carrier's `ModelBuy` or `ModelSeed` names — its index, its line, its
    /// value — on a carrier that pays a P2PKH-ML-DSA-87 output a refused buy or seed can be paid back
    /// to ([`crate::palw_model_market_v1::palw_model_sink_binding_refusal_v1`]); a transaction with
    /// any other sink is invalid in the header context (`TxRuleError::ModelSinkUnbound`), and so is a
    /// block carrying one. Below it a sink no object binds is a valid output whose MSK leaves
    /// circulation with nothing recorded (no reserve, no burn row, no P-B1 refund) — the rule
    /// testnet-12 launched with, byte for byte.
    ///
    /// Keyed on the containing block's own DAA score (the header context already threads it for
    /// ADR-0087 Decision 6), so reorg, IBD and a pruning-proof sync agree; the mempool and the
    /// template ask it at the virtual's DAA. Refused by `validate_palw_v2` off ConsensusV2, without
    /// `palw_model_market` declared (there is no sink form to bind) and without
    /// `palw_audit_2026_09_23` (the P-B1 refund route its payee rule serves) armed at or below it.
    /// Dormant (`None`) on every shipped preset, testnet-12 included, until the operator arms the
    /// common post-launch height; hashed Some-only in every writer with the `never()` collapse, so a
    /// build that carries the field fingerprints and peers exactly as one that does not until it is
    /// armed. The node refuses such a sink as mempool standardness wherever the market is declared,
    /// with or without this fence.
    pub palw_model_sink_bound: Option<ForkActivation>,
    // ---- end lane sink ---------------------------------------------------------------------------
    // ---- ADR-0160 lane verify (capacity, post-launch, later than DAA 500): F-B and F-R -------------
    /// **ADR-0160 F-B: the batch licence** (lane verify V1, testnet-12 only, post-launch). In a block
    /// whose DAA score is at or past it a lifecycle carrier may carry
    /// `PalwConsensusObjectV2::ReceiptLicensedBatchV1` — one ML-DSA-87 signature per seat over a
    /// window root of that seat's V3 receipts ([`crate::palw_batch_licence_v1`]) and, per claim, each
    /// seat's receipt as a leaf and a Merkle path. Each entry licenses exactly as a
    /// `ReceiptLicensedV2` of the same receipts would (the same acceptance check, the same fold arm,
    /// ADR-0160 V-I1), and each leaf binds the claim's panel anchor, so a receipt cannot be imported
    /// across forks. Below it the object is refused by name by the acceptance layer and by the fold,
    /// and a `Windowed` false-Valid receipt is refused too: every block validates as under
    /// `0e8ec984e`.
    ///
    /// Keyed on the carrying block's DAA score. Refused by `validate_palw_v2` off ConsensusV2 and
    /// without `palw_rcore_plus` and `palw_verification_v2` armed at or below it (the funnel it feeds
    /// is R-core+'s coverage door). Mirrored for the fold by [`Self::sync_palw_capacity_verify`].
    /// Dormant (`None`) on every shipped preset, testnet-12 included; hashed Some-only in every writer
    /// with the `never()` collapse.
    pub palw_capacity_batch_licence: Option<ForkActivation>,
    /// **ADR-0160 F-R: room v2** (lane verify V2, testnet-12 only, post-launch). Past it, at the block's
    /// DAA score:
    ///
    /// * a model class's panel capacity counts what verification actually costs — `k = 2`
    ///   attestations per segment (Verification V2's coverage) instead of five full replays, at the
    ///   measured replay speed ([`crate::palw_verify_capacity_v1::PALW_CAPACITY_REPLAY_SPEED_PERMILLE_V1`])
    ///   instead of the 2.4e12 reference ([`crate::palw_verify_capacity_v1::palw_panel_capacity_v2`]);
    /// * T-2(a)'s per-bond `⌈c/2⌉` is replaced by the stake-proportional cap
    ///   ([`crate::palw_verify_capacity_v1::palw_bond_room_cap_v1`]: the whole room uncontended, a
    ///   guaranteed `⌊room · C_b / ΣC⌋` under contention plus the rounding slack first come — a split
    ///   gains at most the slack);
    /// * the floor (the base class, ungated below it) is gated by the seats' capital
    ///   ([`crate::palw_verify_capacity_v1::palw_floor_room_v1`], J-6), with the same share.
    ///
    /// Refused by `validate_palw_v2` off ConsensusV2 and without `palw_rcore_plus`,
    /// `palw_audit_2026_09_23` and `palw_offence_attribution` armed at or below it (ADR-0160 §6.1
    /// also requires lane liab's `palw_capacity_aggregate_liability`, which `rcore/cap-int` adds to
    /// this check when the two lanes merge). Mirrored for the fold by
    /// [`Self::sync_palw_capacity_verify`]. Dormant (`None`) on every shipped preset; hashed Some-only
    /// with the `never()` collapse.
    pub palw_capacity_verify_room: Option<ForkActivation>,
    // ---- end ADR-0160 lane verify -------------------------------------------------------------------
    // ---- ADR-0160 stage 2 (rcore/cap-s1): lane Q the audit door, lane S the issuance slots ---------
    /// **ADR-0160 F-Q: the audit door** (lane Q, testnet-12 only; ADR-0160 v3 §5.9, D-23). Past it a
    /// CREDITED claim — one whose escrow slot a ramp step's credit cut below its reward — reaches `Final`
    /// only once `k_aud` distinct members of its audit pool (the operator bonds that are neither its
    /// producer nor a seat of its panel, nor frozen, nor excluded) have posted a receipt that its roots
    /// reproduce (`PalwConsensusObjectV2::AuditReceiptBatchV1`, tag 60); `k_aud` is 1 below ρ 250 and 2
    /// from it ([`crate::palw_audit_door_v1`]). An unaudited credited claim waits — never voided, burned
    /// or charged for it. Refused by `validate_palw_v2` off ConsensusV2 and without
    /// `palw_capacity_aggregate_liability` (F-L), `palw_capacity_batch_licence` (F-B) and
    /// `palw_operator_anchor` (whose operator bonds are the pool) at or below it; and F-L's value is
    /// refused where a CREDITED step starts below F-Q. Mirrored for the fold by
    /// [`Self::sync_palw_capacity_stage2`]. Dormant (`None`) on every shipped preset; hashed Some-only
    /// with the `never()` collapse.
    pub palw_capacity_audit_door: Option<ForkActivation>,
    /// **ADR-0160 F-S: the issuance slots** (lane S, testnet-12 only; ADR-0160 v3 §6). Past it a bond's
    /// attempt is admitted only while it holds fewer than `N_out = u·ρ` issuance slots and its token
    /// bucket (depth `max(4, ⌈u·ρ/25⌉)`, refill `u·ρ/20` a DAA) holds a token
    /// ([`crate::palw_issuance_slots_v1`]) — DoS and queue bounds, never the safety. Refused by
    /// `validate_palw_v2` off ConsensusV2 and without `palw_capacity_weight_cap` (F-W),
    /// `palw_capacity_aggregate_liability` (F-L) and `palw_capacity_escrow_at_licence` (F-E) at or
    /// below it. Mirrored by [`Self::sync_palw_capacity_stage2`]; dormant on every shipped preset;
    /// hashed Some-only with the `never()` collapse.
    pub palw_capacity_issuance_slots: Option<ForkActivation>,
    // ---- end ADR-0160 stage 2 ----------------------------------------------------------------------
    /// **ADR-0160 F-N: the network level and the work-conserving fair share** (lane N, testnet-12 only;
    /// rcore/cap-s1 stage 4; ADR-0160 v3 §7). Past it an attempt is admitted only while the network has a
    /// free unit under `L_net = min(L_seat, L_carry, L_anchor)` and either its bond holds less than its
    /// share of `L_net` among the bonds that registered unmet demand, or what those bonds are owed leaves a
    /// unit ([`crate::palw_network_room_v1`]); a refused bond registers its demand for `H_L` DAA. Refused by
    /// `validate_palw_v2` off ConsensusV2 and without `palw_capacity_verify_room` (F-R),
    /// `palw_capacity_issuance_slots` (F-S) and `palw_operator_anchor` (whose attempts `ā_op` counts) at or
    /// below it. Mirrored by [`Self::sync_palw_capacity_stage2`]; dormant on every shipped preset; hashed
    /// Some-only with the `never()` collapse.
    pub palw_capacity_network_room: Option<ForkActivation>,
    /// **int-11: F-N's verification term** (`palw_capacity_network_verify`; lane P's finding on the live backlog, 2026-10-01).
    /// `L_net = min(L_seat, L_carry, L_anchor)` bounds the unlicensed queue by capital and carriage, not by what the
    /// seats verify, so a collapsed verification supply lets claims wait past the receipt window and slashes honest
    /// producers. Past this fence `L_net` is also capped by the static `L_ver = ⌊1.5 licences/DAA × (window_receipt −
    /// anchor_delay) / 2⌋` — 435 at the shipped 600 / 20 ([`crate::palw_network_room_v1::palw_network_verify_level_v1`]);
    /// never binds at the healthy supply. A constant: the adaptive form is stage 6. Refused by `validate_palw_v2` off
    /// ConsensusV2 and without `palw_capacity_network_room` at or below it. Mirrored by
    /// [`Self::sync_palw_capacity_network_verify`]; dormant on every shipped preset and in every flag-day list (int-11's names
    /// its height); hashed Some-only with the `never()` collapse.
    pub palw_capacity_network_verify: Option<ForkActivation>,
    /// **Panel-liveness lane PL, part C (ADR-0166): verifier unavailability is not producer fraud.** Past this fence a
    /// claim whose receipt window closed a SECOND time with no concluding object (and an S2 licence a second panel did
    /// not back by replay) expires `Voided { PanelUnavailable }` instead of being charged: the producer's reservation
    /// and bond lock return, nothing is slashed, no strike is written, the seats get nothing and the fee is not
    /// refunded. The producer is slashed only for a proven producer fault (a court loss, an invalid commitment, an
    /// unanswered DA demand). Refused by `validate_palw_v2` off ConsensusV2 and without `palw_audit_2026_09_23` at or
    /// below it. Mirrored by [`Self::sync_palw_panel_unavailable_expiry`]; testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped
    /// presets; hashed Some-only with the `never()` collapse.
    pub palw_panel_unavailable_expiry: Option<ForkActivation>,
    /// **Panel-liveness lane PL, part D (ADR-0166): a fast switch to a fresh panel.** Past this fence the FIRST panel of a
    /// claim has `T_fast = clamp(W_r(c) / 20, 30, W_r(c) / 2)` DAA from its bind (30 DAA at the shipped 600-DAA window) to
    /// conclude, not the whole receipt window: a silent panel is replaced by the redraw — a fresh panel, anchored on the
    /// sweep, which the existing machinery already deals — at the fast deadline, and the redraw gets the whole window
    /// (`T_hard`), after which the claim ends as part C says. Nobody is charged by the switch (a first timeout never was).
    /// Refused by `validate_palw_v2` off ConsensusV2 and without `palw_panel_unavailable_expiry` and `palw_rcore_plus` at or
    /// below it. Mirrored by [`Self::sync_palw_panel_fast_switch`]; dormant on every shipped preset; hashed Some-only with
    /// the `never()` collapse.
    pub palw_panel_fast_switch: Option<ForkActivation>,
    /// **Panel-liveness lane PL, part E (ADR-0166): positive liveness in panel ASSIGNMENT only.** Past this fence the
    /// stake-weighted draw weighs an operator by its stake weight times an availability factor (0.5x new or inactive,
    /// 1.0x normal, 1.2x long high availability) derived from on-chain SUCCESS only — credited receipts filed before
    /// their deadline over a rolling window. Security weight, slashing and fork weight stay stake-only. Refused by
    /// `validate_palw_v2` off ConsensusV2 and without `palw_rcore_plus` at or below it. Mirrored by
    /// [`Self::sync_palw_seat_availability`]; dormant on every shipped preset; hashed Some-only with the `never()`
    /// collapse.
    pub palw_seat_availability: Option<ForkActivation>,
    /// **ADR-0164 F-EM: the per-DAA PALW reward budget** (stage 5's emission redesign; `palw_capacity_emission_budget`). Past it an
    /// attempt claim's acceptance charges its block's whole carve to a rooted ledger keyed by the accepting block's DAA and is
    /// refused (non-fatal, the carve burned as a skipped attempt's) beyond
    /// [`crate::palw_capacity_s567_v1::PALW_EMISSION_BLOCKS_PER_DAA_V1`] carves a DAA — so emission per DAA does not depend on the
    /// number of claims or on ρ. Refused by `validate_palw_v2` off ConsensusV2 and without F-L (`palw_capacity_aggregate_liability`)
    /// and F-E (`palw_capacity_escrow_at_licence`) at or below it. Mirrored by [`Self::sync_palw_capacity_s567`]; dormant on every
    /// shipped preset; hashed Some-only with the `never()` collapse.
    pub palw_capacity_emission_budget: Option<ForkActivation>,
    /// **ADR-0164 F-M1: riders** (stage 5's multi-claim carriage, D-12; `palw_capacity_multi_claim`). Past it a lead attempt claim
    /// may take up to 64 rider attempts of its own bond in one `AttemptRidersV1` object (tag 95): each rider is its own claim
    /// (panel, licence, slot, share, room, audit), paid out of the lead's carve — Σ escrow of a block's claims is the block's carve.
    /// Refused without F-EM, F-B (`palw_capacity_batch_licence`), F-N (`palw_capacity_network_room`), F-S, F-W and F-Q at or below
    /// it. Mirrored by [`Self::sync_palw_capacity_s567`]; dormant on every shipped preset; hashed Some-only with the `never()` collapse.
    pub palw_capacity_multi_claim: Option<ForkActivation>,
    /// **ADR-0164 F-K: the lower-only ρ breaker** (stage 6; `palw_capacity_rho_breaker`). Past it one rooted row counts, per aligned
    /// 1,000-DAA epoch, what the chain itself shows of the ramp's health — receipt-deadline overruns, panel backlog, slash
    /// shortfalls, false audits, audit service — and at the end of the first block of each epoch steps the issuance tier down one
    /// rung (to ρ = 1 on a severe signal) and back up one rung only after two clean epochs; the tier is `min(schedule ρ, breaker
    /// level)`, never above the flag days' schedule. Refused without F-L, F-S and F-Q at or below it. Mirrored by
    /// [`Self::sync_palw_capacity_s567`]; dormant on every shipped preset; hashed Some-only with the `never()` collapse.
    pub palw_capacity_rho_breaker: Option<ForkActivation>,
    /// **ADR-0075 Decision 14 — only a chunk that can complete a group may spend the block's
    /// certification cap.** `None` on every shipped preset, so the behaviour is byte-identical to
    /// not having the field.
    ///
    /// The acceptance rehearsal counts a chunk against `PALW_CERTIFICATION_MAX_PER_BLOCK` when it
    /// would complete its group, and it asked only `count == 1` for a group that does not exist
    /// yet. So an `ObjectChunk { index: 5, count: 1 }` — sixty bytes the transition refuses on its
    /// face as `ChunkIndexOutOfRange` — was charged as a grading, and two of them per block starve
    /// every genuine `FamilyCertified` in that block for two ordinary fees.
    ///
    /// **A bare fence with no companion value.** The rule it arms is the transition's own
    /// `index < count`, not a configured quantity, so there is nothing beside it that could be
    /// normalised out of [`Self::consensus_identity_id`] and disagreed about silently — the hazard
    /// `palw_bond_maturity` documents. Fenced rather than applied outright because it changes which
    /// objects a block accepts, and therefore the state root: an upgraded node and an un-upgraded
    /// one would compute two roots for one block, which is the shape this whole file exists to
    /// refuse.
    pub palw_chunk_cap_charge: Option<ForkActivation>,

    /// **ADR-0081 Decision 3 — is a job's `prompt_token_ids_hash` a tiled Merkle root?** `None` on
    /// every shipped preset, so the behaviour is byte-identical to not having the field at all.
    ///
    /// The shipped commitment is [`crate::palw_v2::prompt_token_ids_hash_v2`], a FLAT digest over
    /// the whole prompt. A flat digest cannot be opened, so the court that adjudicates an embedding
    /// gather — which reads exactly ONE id, `prompt_ids[position]` — has to be handed the whole
    /// list to authenticate it, and `derive_court_cost_v1` charges `n_ctx x 4` bytes on EVERY node
    /// of the graph for the privilege. 2 KiB per node at `n_ctx` 512 against an 80 KiB carrier;
    /// 128 KiB at 32,768, where nothing fits. Past this fence the slot holds
    /// [`crate::palw_prompt_ids_v1::prompt_token_ids_root_v1`] and a refutation carries one tile
    /// plus its path: 472 bytes at 512, 856 at 32,768.
    ///
    /// **Arming it is a RE-MINT, not a schedule** — the `palw_context_ladder` sentence, and here it
    /// is stronger than a convention. The hash is a field of
    /// [`crate::palw_freeprompt_v3::PalwFreePromptJobV3`], so it is inside `fp_job_id_v3` and
    /// therefore inside every claim id, every spend id and every golden vector over a job or a
    /// claim. Two builds that armed this at one height would agree about the fence and disagree
    /// about what every job on the network is called, which is not a fork a height can stage.
    ///
    /// **And the re-mint is NOT confined to this crate.** `prompt_token_ids_hash_v2` is called
    /// from three crates outside consensus-core, in production paths this fence does not reach,
    /// and every one of them would keep spelling the flat form while the court expected the other:
    ///
    /// * it is WRITTEN into a job by `misaka-palw-base0`'s `fp_worker::run_one_job_v1`,
    ///   `produce::base0_rc_job_v1`, `qwen36_backend::job_for_anchor`, the `qwen25_a16_backend`
    ///   job builder and `e2e_drill::fp_drill_job_v1` — a producer left on the flat form mints
    ///   jobs the armed court cannot adjudicate;
    /// * it is CHECKED against a carried prompt by `kaspad`'s `palw_panel::fp_prompt_for_job`,
    ///   `kaspa-pq-validator-core`'s `palw_fp_sign_gate::signable_claim_id`, and
    ///   `misaka-palw-base0`'s `fp_interval::base0_open_fp_interval_v1` /
    ///   `base0_verify_fp_interval_opening_v1` and the three backends'
    ///   `refutation_with_prompt` — a checker left on the flat form refuses honest material.
    ///
    /// Three more production sites call `check_execution_step_refutation_v1` and would have to
    /// thread the opening: `operand_openings_for` in `misaka-palw-base0`'s `backend`,
    /// `qwen36_backend` and `qwen25_a16_backend`. None of this is reachable from a `Params`, which
    /// is exactly why it is written down here: the fence is the only thing an operator sets, and
    /// the surface it does not cover is the part that fails silently.
    ///
    /// **A bare fence with no companion value, deliberately** — the `palw_context_ladder` rule for
    /// its reason: the tile width, the domains and the opening's shape are constants in
    /// `palw_prompt_ids_v1`, so there is no duration beside this height for
    /// [`Self::consensus_identity_id`] to normalise away (the `palw_bond_maturity` hazard).
    ///
    /// **TOP LEVEL rather than in the V2 bundle**, for the `palw_unavailable_abstains` reason: a
    /// fence inside the bundle moves `palw_ruleset_id_v2`, which `for_each_fence` never descends
    /// into, so every old/new pair would fail the handshake outright.
    ///
    /// **ARMING IT IS REFUSED BY [`Self::validate_palw_v2`] ON THIS BUILD** (audit D M-2). The
    /// list above is not a plan, it is the surface: not one of those writers or checkers reads
    /// this fence today. `palw_fp_prompt_ids_admit_v1` — "the one check that comes before every
    /// other" — re-derives `prompt_token_ids_hash_v2`, the FLAT digest, unconditionally, and
    /// `palw_prompt_ids_form_at` has no production caller at all. So arming this would move
    /// `consensus_identity_id` and nothing else, while the admission price it buys
    /// (`prompt_ids_close_bytes_v1(MerkleV1, ..)`) is smaller than the close the network still
    /// carries by the `n_ctx x 4` term — under-counting a ceiling, which is the direction that
    /// admits a class whose disputes nobody can carry. The refusal lifts when the commitment
    /// moves with the fence, which is the re-mint this doc describes.
    pub palw_prompt_ids_merkle: Option<ForkActivation>,

    /// **ADR-0082 Decision 3's fence: the k-ary court and the history dissection.** Active, the
    /// court's dissection arity is the value DERIVED from the ruleset's window, deadline, ladder
    /// and the widest registered history (never a companion value carried here, for the
    /// `palw_bond_maturity` reason), and a fused attention leaf may be refuted by dissection
    /// (`palw_attn_dissect`). Dormant, the court is the shipped binary ladder and a graph-v5
    /// row is refused at admission. A bare fence, top level, `None` on every shipped preset —
    /// the `palw_prompt_ids_merkle` shape for its reasons.
    ///
    /// **Arming it is a RE-MINT of the ruleset's `dissection_arity`, and
    /// [`Self::validate_palw_v2`] refuses anything else** (audit D H-1). The derivation
    /// OVERWRITES `PalwCourtParamsV2::dissection_arity` at activation, so a bundle that still
    /// carries the binary 2 while the derivation answers 4 states one rule twice: the admission
    /// gate prices a fused row for the court that will play it, the frozen field is what
    /// `palw_ruleset_id_v2` hashes, and a chain in which they differ admits classes at a price its
    /// challengers cannot pay. The assembly refuses that configuration, so the bundle must carry
    /// the arity it derives.
    ///
    /// **And a fused row in the GENESIS set requires it.** Genesis registrations are verified
    /// against the committed catalog and never run `verify_class_admission_*`, so
    /// `FusedAttentionNeedsTheKaryCourt` has no door on that route; `validate_palw_v2` refuses a
    /// ruleset whose genesis set registers a graph-v5 class while this fence is dormant.
    pub palw_kary_court: Option<ForkActivation>,

    /// Activate model-specific finite court windows. After this fence registration derives the
    /// minimum complete dispute duration from each model's court shape; there is no shared DAA
    /// ceiling such as 6,000. Overflow or a non-finite court shape is refused.
    pub palw_model_court_window: Option<ForkActivation>,

    /// **ADR-0082 Decision 2's clock does not convict for a move no party in the field can make**
    /// (mainnet audit 2026-09-06, C-2/H-5). `None` on every shipped preset, so the fold is
    /// byte-identical to not having the field at all.
    ///
    /// §10.6 gave the fused terminal a clock because "silence no longer wins challenger-side", on
    /// the premise that the responder owes a `CourtAttnRootClaimed` it can file. No binary in this
    /// tree constructs that object outside a test module, and a converged ladder has `round() > 0`
    /// by construction, so the opening-rung mercy beside it cannot reach the terminal: every fused
    /// session that runs its rung clock out ends in `void_and_slash(CourtFraud)` against the
    /// accused, whether it is honest or guilty. A verdict that is independent of the facts is not
    /// the trial Decision 2 designed; it is a funded, evidence-free way to burn a dense-tier
    /// producer's collateral and its escrowed reward.
    ///
    /// Past this fence the fused terminal's silence routes to `rearm_after_unanswered_opening` —
    /// this fold's own third direction, which ends the session, convicts nobody and fines nobody —
    /// instead of to the void. NOTHING ELSE MOVES: the fused remap stands, the rung clock stands,
    /// every other silence still convicts the party that owes the move, and the challenger-side
    /// backstop is untouched.
    ///
    /// **This exemption is a placeholder for a responder and must be retired the day one ships**,
    /// by activation on a fresh card or a scheduled height on a live chain — never by a silent
    /// edit. `no_shipped_binary_files_a_court_attn_root_claimed` is the pin that goes red on that
    /// day.
    ///
    /// **TOP LEVEL for the reason on `palw_unavailable_abstains`**: it has to be able to reach a
    /// live network, and a fence inside the V2 bundle moves `palw_ruleset_id_v2`, which
    /// `for_each_fence` never descends into.
    pub palw_court_responder_coverage: Option<ForkActivation>,

    /// **ADR-0082 Decisions 10 and 11's fence: the free-prompt lane's numerator and its sampler.**
    ///
    /// Active, two rules move together because they are one ruleset move (ADR-0082 §4,
    /// "Identity"): a free-prompt claim's quanta are earned by its DECODE leaves — the prefill of
    /// a user-chosen prompt is priced at zero (Decision 10) — and the committed decode token is
    /// the seeded argmax of [`crate::palw_decode_select_v2`] rather than the plain one (Decision
    /// 11). Dormant, the transition prices whole leaves exactly as it does today and a job whose
    /// sampling fields are not the greedy defaults is refused BY NAME.
    ///
    /// They ride ONE fence deliberately. Decision 10's numerator floors to `ZeroQuanta` on the
    /// shipped two-decode-token rows, so it is armed with the graph-v5 rows and not before; and a
    /// sampler is what makes a long answer worth producing, so a network that arms one without
    /// the other has either a lane that earns nothing or a lane that repeats. Two fences would
    /// let an operator reach both of those states by configuration.
    ///
    /// A bare fence, top level, `None` on every shipped preset — the `palw_kary_court` shape
    /// directly above, for its reasons.
    ///
    /// **RFC-0001 §A (FP Job V4, 2026-09-27): the same fence also says "V4 from here".** Below it
    /// every new free-prompt job is V3 (greedy); from it every new job is V4
    /// (`palw_decode_pipeline_v4`: repeat / frequency / presence penalties, `logit_bias`, stop token
    /// sequences, Decision 11's key), and a V3 claim accepted before it is verified to the end by the
    /// V3 verifier. One fence, because the three are one ruleset move (RFC-0001 §3).
    ///
    /// **Armable on this build** — the audit D M-1 refusal is lifted: the transition credits decode
    /// work over the class's derived graph and every engine selects through `PalwFpDecoderV1`.
    /// `validate_palw_v2` still refuses it off ConsensusV2, below `palw_fp_derived_work`, or with the
    /// bundle's mirror out of step ([`Self::validate_palw_fp_decode_rules_v1`]). Arm it at a flag day
    /// through [`PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1`], which sets the fence and its mirror.
    pub palw_fp_decode_rules: Option<ForkActivation>,

    /// **ADR-0145 §5/§6: the free-prompt lane's work is DERIVED, and a cached prefix is not paid
    /// twice.** The 2026-09-19 reward audit's F2, which is CRITICAL for the lane ADR-0144 makes the
    /// product.
    ///
    /// Dormant — every shipped preset — `work_leaves` is the executor's own field and the
    /// acceptance walk never recomputes it; the repo's own test multiplied it by ten and asserted
    /// the walk still passed. Active, three things move together because they are one accounting
    /// move:
    ///
    /// * the extraction walk carries the run's PROMPT LENGTH onto the consensus object, which it
    ///   already holds and used to drop — without it the transition has one of the two execution
    ///   facts a derivation needs;
    /// * the transition recomputes `work_leaves` from the class's graph and those two facts
    ///   (`palw_freeprompt_v3::fp_derive_work_v1`) and REFUSES a commitment that disagrees. Not a
    ///   silent correction: a silent correction is a wire format nobody reads (ADR-0145 I1);
    /// * the credited leaves are NEW WORK ONLY — a prompt that extends a prompt this bond has
    ///   already been paid for on this class is credited for the extension, not for the prefix
    ///   (ADR-0145 §6).
    ///
    /// **What arming it requires, and why it is not the `palw_fp_decode_rules` shape.** The
    /// derivation needs the class's graph, and the chain did not keep one: a registration's
    /// carriage and a lane certification both CARRY a profile and neither was stored. Past this
    /// fence a free-prompt lane certification writes the profile into the rooted
    /// `fp_work_profiles` index, and a class already certified may carry a second
    /// `ClassLaneCertified` for the sole purpose of publishing it — permissionless, signature-free
    /// and checkable, because a class id IS its profile's id. A commitment of a class with no
    /// profile row is refused BY NAME and the refusal says what to carry. That is the difference
    /// from `palw_fp_decode_rules` above, which arms a numerator whose inputs no object can ever
    /// supply: this one is fail-closed and openable by anyone, and the drill that crosses the fence
    /// publishes the profiles before the first commitment.
    ///
    /// A bare fence, top level, `None` on every shipped preset; hashed Some-only, so a build with
    /// it unset fingerprints byte-identically to a build without the field.
    pub palw_fp_derived_work: Option<ForkActivation>,

    /// **ADR-0096 Decisions 6–8's fence: a free-prompt job may carry a DECODE CONSTRAINT, and the
    /// committed token is the argmax over the lanes the constraint admits.**
    ///
    /// Active, four things move together because they are one ruleset move (ADR-0096 Decision
    /// 8): the free-prompt job gains `constraint_id` (version 6 of `PalwFreePromptJobV3`, so
    /// every version-5 job stays exactly what it is); the selection rule becomes the ADMITTED
    /// argmax — `decode_token_select_v2` restricted to the lanes a pinned byte-level automaton
    /// admits at the position, which at `constraint_id = 0` is `decode_token_select_v2` byte for
    /// byte and at a greedy temperature is `base0_decode_token_select_v1` byte for byte; the
    /// court's tiled decode refutation gains the automaton state and a one-disclosure "not
    /// admitted" arm; and the answer is rendered up to the first committed end-of-generation id
    /// (`render_answer_v2`, ADR-0096 Decision 6). Dormant, a job carries no constraint field, the
    /// shipped rendering stands, and an entrance that was asked for a format serves it ADVISORY
    /// and says so (`misaka.format.enforcement`, ADR-0096 Decision 3).
    ///
    /// It does NOT require `palw_fp_decode_rules`: a greedy decode under a constraint is a
    /// complete rule, and ADR-0096 Decision 7 composes with ADR-0082 Decision 11 (mask, then
    /// key) rather than depending on it.
    ///
    /// A bare fence, top level, `None` on every shipped preset — the `palw_fp_decode_rules`
    /// shape directly above, for its reasons.
    ///
    /// **BUILT (RFC-0001 §2.5, lane U, 2026-10-03).** testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped presets. Armed, a free-prompt job may be FP job version 6 carrying a decode constraint
    /// ([`crate::palw_fp_constraint_job_v1`]): the committed token is the argmax over the lanes the constraint admits, the
    /// seat replays it through the same mask, and `palw_fp_constraint_v2` may ride it. The court's third arm is on the wire
    /// (`PalwCourtVerdictProofV2::DecodeTokenConstrained`, tag 17) and the direct offence contradictions are tags 14/15 (those
    /// need the dormant `palw_legacy_public_filer_v1`); the node does not yet generate the direct proof end to end
    /// (docs/audit/g14-review-2026-10-10/constrained-direct-proof-status.md). [`Self::validate_palw_v2`] refuses it off ConsensusV2.
    pub palw_fp_decode_constraint: Option<ForkActivation>,

    /// **ADR-0099 Decision 5's fence: a court opened at a NAMED leaf, decided in one move.**
    ///
    /// Active, a bonded accuser — a seat of the claim's panel holding one shard of the class, or
    /// any active bond — files `PalwShardCourtAccusationV1` (`palw_shard_court_v1`): the leaf it
    /// names, the refutation it already holds (the leaf's committed output and inputs opened
    /// against the claim's roots), and the artifact rows opened against the class's registered
    /// root. The chain adjudicates it with the court's own terminal check
    /// (`check_execution_step_refutation_capped_v1`) at the ruleset's ladder: a fault voids the
    /// claim and slashes its executor, a refutation that proves none slashes the accuser, and a
    /// fused attention leaf is answered `NeedsDissection` (ADR-0082's protocol, ADR-0093's
    /// responder). No ladder, no responder, no clock — nothing is asked of the accused. It is the
    /// half of ADR-0077 Decision 8 that a seat holding one shard can play, and without it no
    /// party on a sharded class can convict anyone.
    ///
    /// A bare fence, top level, `None` on every shipped preset — the `palw_fp_decode_constraint`
    /// shape directly above, for its reasons.
    ///
    /// **Built (ADR-0100):** the accusation is `PalwConsensusObjectV2::ShardCourtAccused`, the
    /// acceptance rule is the processor's arm, the fold is `palw_state_v2`'s, the seat files one
    /// from its own capture check, and the signing context is in
    /// `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`. **What [`Self::validate_palw_v2`] still refuses
    /// is arming it over a bundle that does not commit to that set** — the ruleset move ADR-0099
    /// §3 named: a network that wants the court states V3 at genesis through
    /// `palw_signature_contexts_v2`, and no shipped preset does.
    pub palw_shard_court: Option<ForkActivation>,

    /// **ADR-0100 Decision 4's fence: a sharded class is licensed per shard.**
    ///
    /// Active, a class's registrant may declare a shard plan (`ClassShardPlanDeclared`), a bond
    /// may declare which shards of a class it holds (`BondShardsDeclared`), a claim on a class
    /// with a plan draws a stratified panel — `shard_count × seat_count` seats, `quorum` a shard,
    /// shard-major — and licenses by parts (`ShardReceiptLicensed`), one carrier a shard at any
    /// shard count, in the block that lands its last shard.
    ///
    /// A bare fence, top level, `None` on every shipped preset, Some-only in both fingerprints.
    /// Its state enters the root and the carriage only once written, so arming it moves nothing a
    /// dormant chain roots. [`Self::validate_palw_v2`] refuses arming it (a) over a bundle whose
    /// committed signing contexts are not the COMPLETE_V3 set, because the plan and the bond's
    /// shard list are verified under contexts only V3 names, and (b) without
    /// [`Self::palw_shard_court`] armed at or below the same height: a panel of seats that each
    /// hold a shard must be able to convict with what a shard seat holds (ADR-0098 Decision 5).
    pub palw_shard_licensing: Option<ForkActivation>,

    /// **ADR-0102 — the court adjudicates the embedding lift per TOKEN.**
    ///
    /// Past this fence a class may reach `KDESC_A16_REQUANTIZE_BY_TOKEN` (graph-v6: the hybrid
    /// whose lift is the position's token's row of `embed_lift.a16`, which is what every
    /// calibrated hybrid artifact carries); before it the admission gate refuses such a class by
    /// name (`TokenLiftNeedsItsFence`). The kernel sits in the adjudicator's FENCED table, outside
    /// `court_catalog_root`, so shipping it moves no preset's fingerprint — this fence is what
    /// enters a network's identity, and only where it is armed. [`Self::validate_palw_v2`] refuses
    /// a genesis that registers such a class while the fence is dormant at genesis: genesis rows
    /// are verified against the committed catalog, not through the admission gate, so that door
    /// needs the same refusal. `None` on every preset.
    pub palw_token_lift: Option<ForkActivation>,

    /// **Kimi K3 family fence.** Past it a registration may reach the fenced Kimi kernels
    /// (`verify_class_admission_v9`'s `kimi_family`). Weight still needs a certified family.
    /// `None` on every shipped preset; hashed Some-only. Genesis that registers a Kimi class is
    /// refused unless this fence is armed from genesis.
    pub palw_kimi_k3: Option<ForkActivation>,

    /// **RFC-0002 Phase F — PALW-TIR v1: a class may be a Canonical Tensor IR program.**
    ///
    /// Past it a registration may carry a `TirProgramV1` (admission v10) and the IR court arms
    /// are admissible; before it every IR object is dropped by name with the block standing. The
    /// value carries the network's `prim_set_id`, the IR court version and the IR ceilings
    /// ([`crate::palw_tir_v1::PalwTirFenceV1`]). `None` on every preset; hashed Some-only in both
    /// fingerprints with the value, the activation alone visited by `for_each_fence`, and the whole
    /// option collapsed from `Some(never())` by the identity's normaliser — the `palw_heartbeat`
    /// shape. [`Self::validate_palw_tir_v1`] holds its refusals.
    pub palw_tir_v1: Option<crate::palw_tir_v1::PalwTirFenceV1>,

    /// **RFC-0003 — generative model classes: a class may be a PIPELINE of PALW-TIR version-2
    /// programs** (image, embedding, audio, video; spec 04b §15).
    ///
    /// The value carries the program version, the primitive set, the rand and output sets, the
    /// generative court version and per-profile ceilings ([`crate::palw_gen_v1::PalwGenFenceV1`]).
    /// testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped presets; hashed Some-only in both
    /// fingerprints with the value, the activation alone visited by `for_each_fence`, and the whole
    /// option collapsed from `Some(never())` — `palw_tir_v1`'s shape. [`Self::validate_palw_gen_v1`]
    /// holds its refusals.
    pub palw_gen_v1: Option<crate::palw_gen_v1::PalwGenFenceV1>,

    /// **FP Job V5: a free-prompt job with image inputs** (RFC-0003 §II.2.1, PALW-GEN-13; RFC-0001's
    /// lane) — the job of a vision-language class, which embeds FP Job V4 unchanged
    /// ([`crate::palw_fp_job_v5`]). Past this height a V5 job may be admitted for a text class with
    /// image slots; below it, and on every network that leaves it `None`, a V5 job is refused by
    /// name and FP Job V4 is exactly what it was.
    ///
    /// testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped presets; hashed Some-only in both
    /// fingerprints, the whole option collapsed from `Some(never())`. Requires `palw_gen_v1` (a
    /// vision-language class is a generative class) and `palw_fp_decode_rules` (V5 embeds V4) in
    /// force at or below it ([`Self::validate_palw_fp_job_v5_v1`]).
    pub palw_fp_job_v5: Option<ForkActivation>,

    /// **RFC-0003 PALW-GEN-20 by the range twin** ([`crate::palw_gen_range_twin_v1`]): past this height the pipeline admission sizes a
    /// generative class's closes with the range twin (`palw_tir_close_range_v1`, `palw_tir_fence2`'s twin) instead of the element
    /// twin — the same bounds, far fewer steps of the same work cap — so a real-size image stage (a vision tower) is sized rather than
    /// refused at the cap. Below it, and on every network that leaves it `None`, the element twin sizes exactly as before.
    ///
    /// Dormant: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only in both fingerprints, the whole option
    /// collapsed from `Some(never())`. Requires `palw_gen_v1` in force at or below it ([`Self::validate_palw_gen_range_twin_v1`]).
    pub palw_gen_range_twin_v1: Option<ForkActivation>,

    /// **The task-head profile** ([`crate::palw_task_heads_v1`], HFX 2026-10-08): past this height a generative class may be a task
    /// head (profile tag 6, `PalwGenProfileOffersV1::Head`, `PalwGenBodyV1::Head`) — a classifier, a token or span head, a masked-LM head
    /// or a vision head as a complete task. Below it, and on every network that leaves it `None`, an object carrying a `Head` variant is
    /// read exactly as the int-12 build reads its bytes: not decodable (the module doc's A-2 table).
    ///
    /// Dormant: `None` on every preset and in no flag-day list; hashed Some-only in both fingerprints with its value, the activation
    /// alone visited by `for_each_fence`, the whole option collapsed from `Some(never())`. **This build refuses arming it**
    /// ([`Self::validate_palw_task_heads_v1`]); the full-activation release will require `palw_gen_v1` in force at or below it
    /// ([`Self::palw_task_heads_v1_arming_preconditions`]).
    pub palw_task_heads_v1: Option<crate::palw_task_heads_v1::PalwTaskHeadsFenceV1>,

    /// **RFC-0011 §15.7: the probabilistic-constraint route's fence** ([`crate::palw_probabilistic_constraints_v1`]): the separately
    /// named fence under which kernel-bound classes (ADR-0172; `misaka-palw-kernel`'s versioned descriptors, `VerificationPlanV1`,
    /// constraint receipts) would be registered and their claims finalized. **Dormant with no activation height**: `None` on every
    /// preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from `Some(never())`, and refused when armed
    /// ([`Self::validate_palw_probabilistic_constraints_v1`]) — the RFC assigns none, and shadow comparison precedes any proposal.
    pub palw_probabilistic_constraints_v1: Option<ForkActivation>,

    /// **ADR-0178: rule E, the fork choice that heals a partition on bonded participation** ([`crate::palw_fork_choice_rule_e_v1`]):
    /// past it the sink search weighs every valid tip, the deep-reorg gate and the IBD commit compare the two tips' EXCLUSIVE pasts
    /// (participation of bonds registered in both first, once the fork is `PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1` deep; then the
    /// economic keys over those pasts), and the relay fetches below the merge-depth root. **Dormant**: `None` on every preset and in no
    /// flag-day list, hashed Some-only in both fingerprints, collapsed from `Some(never())`, refused when armed by
    /// [`Self::validate_palw_fork_choice_rule_e_v1`] — which also refuses `palw_dns_retirement_v1` unless rule E is armed at or below it.
    pub palw_fork_choice_rule_e_v1: Option<ForkActivation>,
    /// **RFC-0004 Part II: typed-root computation specifications on the kernel route** ([`crate::palw_typed_roots_v1`]): the separately
    /// named fence under which the route's ledger schedules the typed-roots extension `K2-TR-v1` (Memory, Retrieval, Composite classes;
    /// `misaka-palw-kernel::spec`). **Dormant with no activation height**: `None` on every preset and in no flag-day list, hashed
    /// Some-only in both fingerprints, collapsed from `Some(never())`, and refused when armed ([`Self::validate_palw_typed_roots_v1`])
    /// until the single full-activation release names a height for it with the others.
    pub palw_typed_roots_v1: Option<ForkActivation>,
    /// **RFC-0009 L2: the fork-choice commitment's fence** ([`crate::palw_fork_choice_commitment_v1`]): a post-state whose point stands
    /// at or past it is committed in headers as `BLAKE2b(fork-choice leaf ‖ ADR-0043 root)`, so a light client opens the comparator's
    /// inputs in 258 bytes. **Dormant with no activation height**: `None` on every preset and in no flag-day list, hashed Some-only in
    /// both fingerprints, collapsed from `Some(never())`, and refused when armed ([`Self::validate_palw_fork_choice_commitment_v1`]).
    pub palw_fork_choice_commitment_v1: Option<ForkActivation>,
    /// **ECON's M\*-49 verifier pay and the held default share** ([`crate::palw_verifier_pay_v1`], G14R, readiness §3f): past this
    /// height the kernel route's jobs escrow a per-check fee paid to drawn verifiers first out of the unchanged 49 %, and a pre-Final
    /// default's demander share is held to the liability horizon. **Dormant**: `None` on every preset, Some-only hashed, collapsed from
    /// `Some(never())`, refused when armed ([`Self::validate_palw_verifier_pay_v1`]).
    pub palw_verifier_pay_v1: Option<ForkActivation>,
    /// **RFC-0002 P0: protected candidate refutations** ([`crate::palw_model_refutation_seal_v4`], lane RFC02-T): past this height
    /// a refutation of a candidate statement (model artifact binding v2, model vector v2, flat model vector v3) is admitted only as
    /// the reveal of the signer's own mature seal (kernel inner 36 / 37), and the frozen unsealed refutations (inner 23 / 27 / 32)
    /// are refused by name. **Dormant**: `None` on every preset, Some-only hashed, collapsed from `Some(never())`, refused when armed
    /// ([`Self::validate_palw_model_refutation_seal_v1`]).
    pub palw_model_refutation_seal_v1: Option<ForkActivation>,
    /// Per-attempt source reservation, inner 40 (codex RFC-7; renumbered from 21 at the merge). Independent, None on every preset, Some-only fingerprints.
    /// Every real activation is refused until funded fees, timing/resource bounds and source security approval are complete.
    pub palw_beacon_source_opt_in_v1: Option<crate::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1>,
    /// Prepaid signed source fees (inner 61/62, codex used 22/23; renumbered at the merge), independent monetary fence; None on all presets and never armable.
    pub palw_beacon_source_fee_v1: Option<crate::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1>,

    /// **RFC-0015: Panel=0 (`OptimisticPublicVerification`) on the kernel route** ([`crate::palw_panel_free_v1`]): the separately named
    /// fence under which a network admits classes registered under the Panel-free verification mode (a fixed public challenge window,
    /// objective fraud proofs, producer reservations; `misaka-palw-kernel`'s `OpvPolicyV1`). **Dormant with no activation height**: `None`
    /// on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from `Some(never())`, and refused when
    /// armed ([`Self::validate_palw_panel_free_v1`]) until RFC-0015 §13.3's `ACTIVATION_ALLOWED` is evidenced (G14 on a real node, the
    /// new lifecycle / collateral review, a coordinated schedule).
    pub palw_panel_free_v1: Option<crate::palw_panel_free_v1::PalwPanelFreeFenceV1>,

    /// **RFC-0009 G-EXPIRY / G-RULESET: the signed class-registration envelope** (tag 108, [`crate::palw_onboarding_v1`]). Past this
    /// height a class registration may arrive wrapped in an object whose signature covers a last-valid DAA and the ruleset's
    /// `consensus_params_id`; the acceptance walk drops the envelope past its expiry or on another ruleset, and replaces it by the
    /// registration it wraps. **Dormant, with no activation height**: `None` on every preset and in no testnet-12 flag-day list,
    /// hashed Some-only in both fingerprints, collapsed from `Some(never())`, refused when armed
    /// ([`Self::validate_palw_signed_registration_v1`]) with the kernel route it ships beside.
    pub palw_signed_registration_v1: Option<ForkActivation>,
    /// **RFC-0006 per-segment pricing** (agent SHARD, [`crate::palw_tir_shard_segment_v2`], `docs/design/palw/shard-rfc6-10.md` §4):
    /// past this height a claim drawn per shard prices each seat's lock and pay by `max(work share, resident share)` of its cells
    /// (the resident share counts the K/V history up to the segment's end), keyed on the claim's panel bind. **Dormant, with no
    /// activation height**: `None` on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from
    /// `Some(never())`, refused when armed ([`Self::validate_palw_tir_shard_segment_v2`]) until the full-activation release names
    /// its height; needs `palw_tir_shard_v1` in force at or below it.
    pub palw_tir_shard_segment_v2: Option<ForkActivation>,

    /// **Lane DA16 (RFC-0009 §4.2, RFC-0014 §16): the provider court** ([`crate::palw_provider_court_v1`]). Past this height bonded
    /// providers lease a subject's public material (an artifact under both roots of its binding, a kernel claim's positions), anyone
    /// challenges one unit on chain, an unanswered challenge slashes the provider, and a claim's producer may move its DA responsibility
    /// to its leases (tags 150–153; rows in the kernel route's aux tables 43–45). **Dormant, with no activation height**: `None` on every
    /// preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from `Some(never())`, refused when armed
    /// ([`Self::validate_palw_provider_court_v1`]) with the kernel route it ships beside.
    pub palw_provider_court_v1: Option<ForkActivation>,
    /// **ADR-0176: the bond budget** ([`crate::palw_bond_budget_v1`], lane BUDGET): past this height every reward-bearing PALW claim
    /// reserves `(Q, B, R, F)` against its producer bond at acceptance, inside a window `W` keyed on acceptance that a reservation leaves
    /// only at `accepted_daa + W`, and every reward block, payout and Final consumes from that reservation and never beyond it. The
    /// value carries the versioned policy (W, the rho -> Q/B/R/F rates, the caps) — POLICY, none approved. **Dormant, with no
    /// activation height**: `None` on every preset and in no flag-day list, hashed Some-only (value included) in both fingerprints,
    /// collapsed from `Some(never())`, refused when armed ([`Self::validate_palw_bond_budget_v1`]).
    pub palw_bond_budget_v1: Option<crate::palw_bond_budget_v1::PalwBondBudgetFenceV1>,
    /// RFC08 monetary schema v2: authenticated reporter debt and legacy cash
    /// quarantine. Separate dormant fence; requires the budget at or below it.
    pub palw_reporter_income_v2: Option<ForkActivation>,
    /// Root-exact physical monetary payment/material retention, schema v3.
    /// Separate dormant fence; requires budget and reporter schemas at/below it.
    pub palw_material_income_v3: Option<ForkActivation>,
    /// Prospective producer-signed absolute Panel service lifetime. Dormant.
    pub palw_panel_lifetime_v2: Option<crate::palw_panel_lifetime_v2::PalwPanelLifetimeFenceV2>,
    /// Signed Claim V2 seed/checker/lifetime contract; dormant without scoped review.
    pub palw_claim_challenge_v2: Option<crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2>,
    /// Explicit signed payment-relative material covenants; dormant without cost approval.
    pub palw_monetary_material_v1: Option<crate::palw_monetary_material_v1::PalwMonetaryMaterialFenceV1>,
    /// **RFC07 R7-ECON: the optional verification-attestation gate on the model allocation** ([`crate::palw_econ_contracts_v1`]):
    /// inner 67 / aux 125. Dormant; Some-only; needs `palw_model_bond_allocation_v1` at or below it; refused when armed.
    pub palw_model_verification_gate_v1: Option<crate::palw_econ_contracts_v1::PalwModelVerificationGateFenceV1>,
    /// **RFC08 R8-WINDOW: the consent-signed dependency horizon extension**: inner 68 / aux 126. Dormant; Some-only; refused when armed.
    pub palw_horizon_extension_v1: Option<crate::palw_econ_contracts_v1::PalwHorizonExtensionFenceV1>,
    /// **RFC08 R8-ECON: the worker/poster minimum-price contract**: inner 69 / aux 127. Dormant; Some-only; refused when armed.
    pub palw_minimum_price_v1: Option<crate::palw_econ_contracts_v1::PalwMinimumPriceFenceV1>,
    /// **N3 (round 3): the Work Beacon's independent bootstrap — BootstrapExact source rounds** ([`crate::palw_beacon_bootstrap_v1`]):
    /// inner 73 / aux 130. Dormant; Some-only; needs `palw_probabilistic_constraints_v1` at or below it; refused when armed (no
    /// approved profile). A source-only record: no reward, block right or Final weight.
    pub palw_beacon_bootstrap_v1: Option<crate::palw_beacon_bootstrap_v1::PalwBeaconBootstrapFenceV1>,
    /// Immutable paid native consumer service/backstop/material contract. Dormant.
    pub palw_paid_consumer_lifecycle_v1: Option<crate::palw_bond_budget_v1::PalwPaidConsumerLifecycleFenceV1>,
    /// **ADR-0177 D3–D5: the model-bond allocation** ([`crate::palw_bond_budget_v1`]): past this height a bond's signed capital
    /// assignment (tag 140) counts toward `S_m`, and a claim's reward reservation is also bounded by its model's share
    /// `R_PALW · f(S_m) / Σ f(S_j)` of the realized PALW carve. Needs `palw_bond_budget_v1` at or below it. **Dormant**, as the budget
    /// fence: `None` everywhere, Some-only hashed with its policy, collapsed from `Some(never())`, refused when armed
    /// ([`Self::validate_palw_model_bond_allocation_v1`]).
    pub palw_model_bond_allocation_v1: Option<crate::palw_bond_budget_v1::PalwModelBondAllocationFenceV1>,

    /// **Lane LG14-B (RFC-0014 §4–§5 on the legacy V2 Panel route, user decision GAP-80): public descent and the held/fused DA
    /// units** ([`crate::palw_legacy_held_da_v2`]). Past this height any bond may demand an interior node of a V2 claim's step or
    /// checkpoint tree (its frontier and opening), or the committed half of one leaf's refutation (`KernelWitness`), and may convict
    /// a non-fused step by recomputing its leaf hash from its committed inputs and its own model copy (tags 157–159). **Dormant, with
    /// no activation height**: `None` on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from
    /// `Some(never())`, refused when armed ([`Self::validate_palw_legacy_held_da_v2`]).
    pub palw_legacy_held_da_v2: Option<ForkActivation>,
    /// **Lane INTF (ADR-0032's 2026-10-10 amendment): the PALW reporter share, 10% → 49%** ([`crate::palw_reporter_share_v2`]).
    /// Below this height R-1's reward and DA-6's refuted cost are 1,000 bps (`PALW_RCORE_REPORTER_REWARD_BPS_V1`, the share every
    /// conviction and session on every shipped network was priced at); at or past it 4,900 bps (`PALW_RCORE_REPORTER_REWARD_BPS_V2`).
    /// R-1 is fixed at the conviction's close, DA-6 at the session's open, so no past amount is re-read. **Dormant, with no
    /// activation height**: `None` on every preset and in no flag-day list, hashed Some-only (with the 4,900 bps beside it) in both
    /// fingerprints, collapsed from `Some(never())`, refused when armed ([`Self::validate_palw_reporter_share_v2`]) until the
    /// full-activation release names its height; needs `palw_rcore_plus` in force at or below it.
    pub palw_reporter_share_v2: Option<ForkActivation>,

    /// **Lane LG14-A (RFC-0014 §6–§7 on the legacy V2 Panel route): the public filer** ([`crate::palw_legacy_public_filer_v1`]). Past
    /// this height any Active bond may reserve a pursuit of a live V2 claim (tag 154, a deposit on its free half), which holds every
    /// end of the claim — `Final`, the bind / receipt / redraw / unavailable / replay timeouts, a `Final` claim's retirement — until the
    /// claim's own hard deadline (`trace_retention_daa − W_disclose`); a reserver's DA sessions ride its reservation's budget, never
    /// DA-8's shared non-seat budget; and a direct proof (kinds 3, 4, 7) lands over another bond's open court. **Dormant, with no
    /// activation height**: `None` on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from
    /// `Some(never())`, refused when armed ([`Self::validate_palw_legacy_public_filer_v1`]) until the full-activation release names its
    /// height; needs `palw_rcore_plus` and `palw_offence_attribution` in force at or below it.
    pub palw_legacy_public_filer_v1: Option<ForkActivation>,
    /// **palw_court_model_scope_v1** — ADR-0177 D2 / RFC-0014 §16.4 (register D2): past it no court route compels model bytes. **Dormant, with no activation height**: `None` on every preset and in no flag-day list, hashed Some-only
    /// in both fingerprints, collapsed from `Some(never())`, refused when armed ([`Self::validate_palw_2026_10_12_ruling_fences_v1`])
    /// until its rule is built and verified (user ruling 2026-10-12, docs/design/palw/mainnet-spec-register-2026-10-12.md).
    pub palw_court_model_scope_v1: Option<ForkActivation>,
    /// **palw_public_evidence_reward_v1** — register N4: past it a claim whose evidence cannot be published earns no new reward or weight. **Dormant, with no activation height**: `None` on every preset and in no flag-day list, hashed Some-only
    /// in both fingerprints, collapsed from `Some(never())`, refused when armed ([`Self::validate_palw_2026_10_12_ruling_fences_v1`])
    /// until its rule is built and verified (user ruling 2026-10-12, docs/design/palw/mainnet-spec-register-2026-10-12.md).
    pub palw_public_evidence_reward_v1: Option<ForkActivation>,
    /// **palw_claim_hard_deadline_v1** — register X3: past it a claim's dispute hard deadline is fixed at acceptance and never extended. **Dormant, with no activation height**: `None` on every preset and in no flag-day list, hashed Some-only
    /// in both fingerprints, collapsed from `Some(never())`, refused when armed ([`Self::validate_palw_2026_10_12_ruling_fences_v1`])
    /// until its rule is built and verified (user ruling 2026-10-12, docs/design/palw/mainnet-spec-register-2026-10-12.md).
    /// The value carries X3's policy (`T_prepare`, `T_observe`, `T_resolve` and the `B_*` start-budget terms with no existing
    /// source — [`crate::palw_ruling_fences_v1`]), hashed Some-only by its id beside the height; no value is approved.
    pub palw_claim_hard_deadline_v1: Option<crate::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1>,

    /// **RFC-0001 §2.6 stage 2b: inherited prefix leaves** ([`crate::palw_fp_prefix_v1`]). Past this height a free-prompt claim may be
    /// FP job version 12: a prefix-state job whose first `k` prefill positions' step leaves are bound to a JOB-INDEPENDENT prefix
    /// context ([`crate::palw_v2::PalwJobContextV2::prefix_context_hash_v1`], context version 3), so every claim over the same prefix
    /// commits byte-identical leaves there — a seat holds them once, and a court opens them against any claim that committed them.
    ///
    /// testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped presets; hashed Some-only. Requires `palw_fp_prefix_state` in force at
    /// or below it ([`Self::validate_palw_fp_prefix_inherit_v1`]).
    pub palw_fp_prefix_inherit: Option<ForkActivation>,

    /// **RFC-0001 §2.6 stage 2: the prefix-STATE receipt** (ADR-0145 §6). Past this height a free-prompt claim may be FP job
    /// version 11: a V4 job whose tail names the KV prefix state it consumed ([`crate::palw_fp_prefix_v1`]), credited for the new
    /// positions only. Below it, and on every network that leaves it `None`, such a job is refused by name.
    ///
    /// testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there); `None` on the other shipped presets; hashed Some-only in both fingerprints, the
    /// whole option collapsed from `Some(never())`. Requires `palw_fp_derived_work` (the price reads the state) and `palw_fp_decode_rules` (V11
    /// embeds V4) in force at or below it ([`Self::validate_palw_fp_prefix_state_v1`]).
    pub palw_fp_prefix_state: Option<ForkActivation>,

    /// **RFC-0001 §2.9: the job's `tokenizer_id` must be the class's listed tokenizer** ([`crate::palw_fp_tokenizer_v1`]). Past this
    /// height a free-prompt commitment on a class whose registry listing carries a tokenizer commitment (an IR class: its record's
    /// `tokenizer_id`) is skipped by name unless its job names that tokenizer; a class that lists none is unconstrained, as before.
    ///
    /// Dormant: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only. Requires `palw_tir_v1` (the listing) and
    /// `palw_fp_decode_rules` in force at or below it ([`Self::validate_palw_fp_tokenizer_match_v1`]).
    pub palw_fp_tokenizer_match: Option<ForkActivation>,

    /// **RFC-0001 §2.5: the decode constraint's second subset** ([`crate::palw_fp_constraint_v2`]) — discriminated `anyOf`/`oneOf`,
    /// non-recursive `$ref`, numeric ranges, with its own automaton bounds. Past this height a constraint of automaton version 2 may be
    /// carried; below it only version 1.
    ///
    /// Dormant: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only. Requires `palw_fp_decode_constraint` (the
    /// carrier of every constraint) and `palw_fp_decode_rules` in force at or below it ([`Self::validate_palw_fp_constraint_v2_v1`]) —
    /// and `palw_fp_decode_constraint` is itself unarmable by a build without ADR-0096 Decisions 6-8, so this one is too.
    pub palw_fp_constraint_v2: Option<ForkActivation>,

    /// **RFC-0001 §2.10: an adapter class listing** ([`crate::palw_adapter_class_v1`], ADR-0163) — a derived class, the registered
    /// parent plus an adapter section, listed on its own and not through an improvement line. Past this height an
    /// `AdapterClassListed` object records the composite reference of an IR class registered over that root.
    ///
    /// Dormant: `None` on every preset and in no testnet-12 flag-day list; hashed Some-only. Requires `palw_tir_v1` and
    /// `palw_improvement_v1` (the composite machinery: possession, openings) in force at or below it
    /// ([`Self::validate_palw_adapter_class_v1_v1`]).
    pub palw_adapter_class_v1: Option<ForkActivation>,

    /// **RFC-0002 Phase F, the second IR fence** (`crate::palw_tir_fence2_v1`): the consensus half of
    /// the fixes after the DAA-2,000 release — ref2's H7 (the `TopK` row of the box demand and the
    /// value bound `V`), the `Select`-arm work credit, and the IR data-availability units
    /// `TirStepLeaf { index }` and `TirStepNode { level, index }`. A bare height; `None` on every preset (testnet-12 included) until a
    /// later flag day arms it. Hashed Some-only in every writer with the `never()` collapse, so a build
    /// that carries the field fingerprints and peers exactly as one that does not. Refused by
    /// [`Self::validate_palw_tir_fence2`] off ConsensusV2, without `palw_tir_v1` in force at or below
    /// it, or with the bundle's mirror unsynced.
    pub palw_tir_fence2: Option<ForkActivation>,

    /// **RFC-0006 — layer-sharded panels for IR classes** (`crate::palw_tir_shard_v1`): past this height a
    /// class's registrant may declare a layer-shard plan (`TirShardPlanDeclared`), the chain draws a panel per
    /// shard with an outsider per shard, a seat attests cells (`ReceiptV4`), the recount runs over cells, a
    /// claim licenses by parts (`TirShardReceiptLicensed`), `TirStepRun` is a DA unit and the class room is the
    /// binding shard's. A bare height; `None` on every preset except **testnet-12, which arms it at the int-11/12
    /// flag day (DAA 5,300, `PALW_T12_INT11_FENCES_V1`)** — adopted 2026-10-03. Hashed Some-only in every writer with the
    /// `never()` collapse. Refused by [`Self::validate_palw_tir_shard_v1`] off ConsensusV2, without `palw_tir_v1`,
    /// `palw_tir_fence2`, `palw_verification_v2`, `palw_rcore_plus` and `palw_admission_independence` in force at
    /// or below it, or with the bundle's mirror unsynced.
    pub palw_tir_shard_v1: Option<ForkActivation>,
    /// **ADR-0165 (the Useful Work Transition), A″: the base floor is the idle-only bonded fallback**
    /// (`crate::palw_real_share_v1`). Past it a `PALW-BASE-0` attempt is accepted only while the rooted floor
    /// state — Idle, Probe or Normal, moved by `palw_floor_step_v1` from the REAL attempts the fold fully accepted
    /// (`floor_idle_slots` 20, `probe_slots` 8, `probe_cooldown_slots` 20) — is Idle, and is refused by name
    /// (`FloorNotIdle`; no claim, no reward, no PALW weight) otherwise. It does NOT make a floor block invalid in
    /// GHOSTDAG (ADR-0165 §00.2). A bare height; `None` on every preset until the DAA-5,300 flag day arms it. Hashed
    /// Some-only (with the three constants) in every writer with the `never()` collapse. Refused by
    /// [`Self::validate_palw_useful_work_v1`] off ConsensusV2, without `palw_model_registry` at or below it, or with the
    /// bundle's mirror unsynced.
    pub palw_floor_reserve_v1: Option<ForkActivation>,

    /// **ADR-0165, B: an attempt-lane block is a clock tick source beside the heartbeat.** The DAA still
    /// advances once per slot (the cursor's rule); the heartbeat miner waits a grace for an attempt
    /// before it mints. A bare height; `None` on every preset until the DAA-5,300 flag day arms it.
    /// Refused by [`Self::validate_palw_useful_work_v1`] without the anchor clock, the single lottery,
    /// the cursor, the floor and the lead cap at or below it.
    pub palw_real_clock_tick_v1: Option<ForkActivation>,

    /// **ADR-0170 (lane anchor/window): the seed anchor is a window, not a span**
    /// (`crate::palw_anchor_window_v1`). Past it a chain block's MERGED admitted attempts (blue or red) record the
    /// execution lane's seed anchor beside its own attempt, the anchor is not cleared at span boundaries, and ADR-0147's
    /// admission jury and the schedule seeding read the latest anchor within `S − 24 … S − 1` (the jury's population cut at
    /// the anchor's span). Without it the reserve (`palw_floor_reserve_v1`) leaves 2–3 % of spans anchored and a Candidate's
    /// audit seats in 2.7 % of its periods. A bare height; its companion values (the window, M1) are hashed with it
    /// ([`crate::palw_anchor_window_v1::palw_anchor_window_value_v1`]). `None` on every preset until the DAA-5,300 flag day
    /// arms it. Hashed Some-only in every writer with the `never()` collapse. Refused by
    /// [`Self::validate_palw_anchor_window_v1`] off ConsensusV2, without the execution lane, the economic-safety bundle, the
    /// admission jury or the model registry at or below it, or with the bundle's mirror unsynced.
    pub palw_anchor_window_v1: Option<ForkActivation>,

    /// **RFC-0009 stage C (lane R9): public receipt redemption** (`crate::palw_receipt_v4`). Past it a free-prompt receipt block may carry a
    /// `PFS4` spend: the executor's position-free redemption authorization (signed once, any time before the fence's use window) plus an
    /// eligible BUILDER's own signature, so a block builder other than the claim's executor spends a winning quantum, and the receipt
    /// block's subsidy-derived worker reward is split in the coinbase — the miner leg to the executor bond's registered payout, a
    /// pre-authorized fee (at most [`crate::palw_receipt_v4::PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS`]) to the builder. V3 spends decode and
    /// admit exactly as before. A bare height; the cap is a companion value hashed with it
    /// ([`crate::palw_receipt_v4::palw_receipt_spend_v4_value_v1`]). `None` on every preset (not in the DAA-5,300 release). Hashed Some-only
    /// in every writer with the `never()` collapse. Refused by [`Self::validate_palw_receipt_spend_v4`] off ConsensusV2 or without
    /// `palw_audit_2026_09_11` and `palw_audit_2026_09_23` at or below it.
    pub palw_receipt_spend_v4: Option<ForkActivation>,

    /// **RFC-0009 stage B: the provider challenge court** (`crate::palw_evidence_court_v1`). Past it a claim may be made accountable to its evidence
    /// PROVIDERS instead of its producer: providers file storage receipts from their own bonds, anyone may challenge one chunk with a deadline, and
    /// silence past it charges that provider once (a claim every provider has defaulted on lapses, which is not the miner's fraud). A bare height;
    /// the response window is a companion value hashed with it ([`crate::palw_evidence_court_v1::palw_evidence_court_value_v1`]). `None` on every
    /// preset, and **no fold reads it yet** — the machine is pure and tested, the chain integration is the next step. Hashed Some-only in every
    /// writer with the `never()` collapse. Refused by [`Self::validate_palw_evidence_court_v1`] off ConsensusV2 or without `palw_da_court` at or
    /// below it.
    pub palw_evidence_court_v1: Option<ForkActivation>,
    /// **Lane PA: the 2026-10-04 audit's consensus fixes** (`crate::palw_audit_1004_v1`) — P-F1, P-F4, B-F3, B-F4, C-F1, S-1/B-F5, RF-1,
    /// RF-3, RF-4, G-3, G-2, P-F5, C-F4 — behind ONE dormant fence. A bare height; its companion value (the beacon depth) is hashed
    /// with it. `None` on EVERY preset (the lead sets the height after the DAA-5,300 release). Hashed Some-only in every writer with
    /// the `never()` collapse. Refused by [`Self::validate_palw_audit_1004_v1`] off ConsensusV2, without the economic-safety bundle
    /// or the model registry at or below it, or with the bundle's mirror unsynced.
    pub palw_audit_1004_v1: Option<ForkActivation>,

    /// **RFC-0004 — the Model Improvement Protocol** (`crate::palw_improve_v1`): a line may be governed
    /// by consensus-native improvement epochs — hard cases, datasets, a candidate registry, evaluation
    /// jobs, promotion and rewards (spec 17). The value carries the scoring library's and the sign
    /// table's ids, the protocol version and the network's ceilings
    /// ([`crate::palw_improve_v1::PalwImprovementFenceV1`]). Dormant: `None` on every preset and in no
    /// testnet-12 flag-day list; hashed Some-only in both fingerprints with the value, the activation
    /// alone visited by `for_each_fence`, the whole option collapsed from `Some(never())` —
    /// `palw_gen_v1`'s shape. [`Self::validate_palw_improvement_v1`] holds its refusals, among them
    /// arming without `palw_tir_v1`, `palw_gen_v1` and `palw_kary_court` in force at or below it.
    pub palw_improvement_v1: Option<crate::palw_improve_v1::PalwImprovementFenceV1>,
    /// **RFC-0003 §I.4.7 / spec 04b §15.15.6 (decision 22): the held leaf challenge**
    /// ([`crate::palw_held_close_v1`]) — one generic held-regime object that opens a `Terminal` session at a
    /// named leaf and rides the court's own close group, so a lie at a leaf whose close exceeds one carrier
    /// (up to the carried cap, 32 × 100,000 B) is convictable on a chain that plays no bisection, for IR and
    /// pipeline classes alike. A bare height; `None` on every preset (testnet-12 included) until a later flag
    /// day arms it. Hashed Some-only in every writer with the `never()` collapse, so a build without the
    /// field and a dormant network fingerprint alike; mirrored on the V2 bundle's state params
    /// (`held_close_chunks_from_daa`, written by [`Self::sync_palw_held_close_chunks_v1`]) because the fold
    /// holds only the bundle. [`Self::validate_palw_held_close_chunks_v1`] refuses it off ConsensusV2 and
    /// without `palw_tir_v1` and `palw_held_context` in force at or below it. Tag 62's live semantics
    /// below the height are untouched.
    pub palw_held_close_chunks_v1: Option<ForkActivation>,
    /// **RFC-0002 Part II §II.7.5 Proposal A — one seating rule for every class kind**
    /// ([`crate::palw_class_seating_fence_v1`]; `palw_class_seating_v1.rs` is the predicate): past it a claim of any class kind
    /// (an IR or legacy attempt class, a generative class, a composite class) is admitted only if the class is *seated* for its
    /// executor — `seat_count` distinct ready operators besides the executor, and `independent_floor` of them independent of the
    /// registrant and the executor and of the network's base population — and the lifecycle leaves `Prefetching`/`Held` only on
    /// the same count. The value carries `independent_floor`. testnet-12 arms it at DAA 5,300 through `PALW_T12_INT11_FENCES_V1` (corrected 2026-10-12; it was not dormant there) (floor 3); `None` on the other shipped presets; hashed
    /// Some-only in both fingerprints with its value, only the activations visited by `for_each_fence`, the whole option collapsed
    /// from `Some(never())` — `palw_gen_v1`'s shape. Mirrored on the V2 bundle's state params
    /// (`PalwStateParamsV2::class_seating`, [`Self::sync_palw_class_seating`]); [`Self::validate_palw_class_seating`] holds its
    /// refusals.
    pub palw_class_seating: Option<crate::palw_class_seating_fence_v1::PalwClassSeatingFenceV1>,
    /// **RFC-0002 Phase H (mainnet): the IR is the admission path for new classes** ([`crate::palw_tir_only_v1`]) — past it a post-genesis
    /// registration of a legacy-family class (an admission carriage of hand-written kernels) is refused by name. A bare height; `None` on
    /// every preset and in no flag-day list: implemented and tested, **armed nowhere**. Hashed Some-only, mirrored on the bundle
    /// (`tir_only_from_daa`, [`Self::sync_palw_tir_only_v1`]); [`Self::validate_palw_tir_only_v1`] refuses arming without `palw_tir_v1`.
    pub palw_tir_only_v1: Option<ForkActivation>,

    /// **RFC-0007 Part I (spec 18): verification vertices, licence by tally, equivocation evidence and `Held` leaves**
    /// ([`crate::palw_vertex_v1`]) — a seat signs one vertex per round of its verdicts, the fold tallies the leaves and licenses a claim
    /// when its panel's `Valid` leaves reach quorum, two vertices of one round are slashed, and DA attestations ride as `Held` leaves.
    /// A bare height; `None` on every preset (testnet-12 included) until a later flag day arms it. Hashed Some-only in every writer with
    /// the `never()` collapse, so a build without the field and a dormant network fingerprint alike; mirrored on the V2 bundle's state
    /// params (`vertex_from_daa`, written by [`Self::sync_palw_verification_vertex_v1`]) because the fold holds only the bundle.
    /// [`Self::validate_palw_verification_vertex_v1`] refuses it off ConsensusV2 and without its prerequisites in force at or below it.
    /// Claims whose panels bound below the height license on the receipt path until they are licensed or void (spec 18 §18.6).
    pub palw_verification_vertex_v1: Option<ForkActivation>,

    /// **RFC-0008 v2 (EXEC lane, `docs/design/palw/rfc-0008-implementation-spec.md`): the unified EXEC payload** ([`crate::palw_exec_v2`],
    /// [`crate::palw_work_slice_v2`]) — one versioned envelope (`PXE2`) carrying `EXEC_TX` or `EXEC_SLICE`, the root/slice ledgers, and
    /// the weightless closure carriage (EXEC is never a selected parent and adds no raw work, PALW weight, blue score, DAA tick or
    /// k-anticone term). A bare height; `None` on every preset, testnet-12 included; Some-only in every hash with the `never()`
    /// collapse, so a build without the field and a dormant network fingerprint alike. Mirrored on the V2 bundle's state params
    /// (`exec_v2_from_daa`, written by [`Self::sync_palw_exec_payload_v2`]) because the fold holds only the bundle.
    /// [`Self::validate_palw_exec_payload_v2`] **refuses any armed height** until the spec's section 9 gates pass
    /// ([`crate::palw_exec_v2::PALW_EXEC_PAYLOAD_V2_ARMABLE`]); its structural prerequisites are enforced beside that refusal so the
    /// day the gates pass is a one-constant change with every other rule already pinned.
    pub palw_exec_payload_v2: Option<ForkActivation>,

    /// **RFC-0007 Part II (spec 18): the witness root in the trace manifest** ([`crate::palw_mesh_v1`]) — a class registered at or past the height pins a witness profile; its attempts commit `1 + witness_chunks` trace chunks under the v2 manifest root, so an `Unavailable` verdict can name a witness chunk. A bare height; `None` on every preset until a later flag day arms it; Some-only in every hash with the `never()` collapse; mirrored on the V2 bundle (`witness_manifest_from_daa`).
    pub palw_witness_manifest_v1: Option<ForkActivation>,

    /// **RFC-0007 Part IV.1 (spec 18): the global audit mesh** ([`crate::palw_mesh_v1`]) — post-commit audit draws over all bonded seats, `Audited` leaves, `TrapCommitted` / `TrapRevealed`, audit pay. A bare height; `None` on every preset; needs `palw_verification_vertex_v1`; mirrored on the V2 bundle (`audit_mesh_from_daa`).
    pub palw_audit_mesh_v1: Option<ForkActivation>,

    /// **RFC-0007 Part IV.2 (spec 18): staged onboarding (capped mode)** ([`crate::palw_mesh_v1`]) — the `Capped` lifecycle state, the weight cap, provisional rewards and the re-verification window. A bare height; `None` on every preset; needs `palw_audit_mesh_v1`; mirrored on the V2 bundle (`capped_from_daa`).
    pub palw_capped_onboarding_v1: Option<ForkActivation>,

    /// **ADR-0093 Decision 6 — admission refuses a fused class the court cannot dissect.**
    ///
    /// A dissection tries ONE head's softmax, so a fused output tile wider than a head (or not a
    /// divisor of it) is two heads' — `palw_attn_dispute_site_v2` refuses it at dispute time
    /// (`FusedTileStraddlesHeads`) and no responder can answer it. Admission has always checked
    /// the query half of that shape and never the output half, so such a class (the hybrid
    /// graph-v5 at 256-lane heads) is admitted and then untriable. Past this fence the gate
    /// refuses it by name (`FusedTileStraddlesHeads`); before it, nothing changes — a chain may
    /// already hold such a class, which is why this is a fence and not a fix. A genesis row is
    /// refused likewise when the fence is armed from genesis (genesis rows do not pass the gate).
    /// `None` on every preset.
    pub palw_fused_dissectable: Option<ForkActivation>,

    /// **ADR-0093 Decision 8 — the root claim carries the anchor the bottom will need.**
    ///
    /// A dissection's bottom on a class that checkpoints every position opens the checkpoint its
    /// step anchors at, and the path to that leaf runs through the ACCUSED's later checkpoints —
    /// which a producer whose execution follows its own lie commits differently from any honest
    /// run, and which its capture need not carry (a dense capture keeps none; an over-cap capture
    /// is never pulled). Past this fence the responder files `CourtAttnRootClaimedAnchored` — the
    /// root claim with that checkpoint opened against its own leg — and the plain root claim is
    /// refused at a site whose bottom needs an anchor; the anchor is verified at filing, so the
    /// challenger's bottom reads it off the chain. Before it, nothing moves: the anchored object is
    /// refused by name and the plain one opens the phase as it always did. Needs `palw_kary_court`
    /// armed at or below it. `None` on every preset: arming it refuses a root claim from every
    /// build before this one, so it waits for the network's upgrade.
    pub palw_attn_anchored_root: Option<ForkActivation>,

    /// **ADR-0103 — the context is held off the chain, and the chain carries a root, an opening and
    /// a logarithm.**
    ///
    /// Active, the chain's per-claim work stops growing with the context:
    ///
    /// * **Decision 1** — no bisection is played: `CourtOpened` is refused, every conviction is a
    ///   one-move object (`ShardCourtAccused`, and `CheckpointAccused` for a checkpoint chunk that
    ///   is not the cache-write rows it claims to hold), and the leaf ladder costs the clock nothing.
    /// * **Decision 3** — a class may register the held map (`tiled_kv_state_chunk_map_id_v4`, or its
    ///   hybrid composition): the same chunks under a tree whose index does not move, bounded by
    ///   depth rather than count. A class registering it is under the regime; the admission gate
    ///   refuses it while this fence is dormant.
    /// * **Decision 4** — a data-availability accusation may name a prompt tile, a state chunk or a
    ///   step range (`DefaultAccusedHeld` / `MaterialDisclosedHeld`).
    /// * **Decision 5** — a `ShardCourtAccused` at a fused attention leaf opens the dissection at that
    ///   leaf, and the arity is derived with the ladder's rounds at zero.
    /// * **Decisions 6 and 8** — a registration is judged by `verify_class_admission_v8`: the
    ///   per-position budget for a held class, and no chain wall of the class may be linear in the
    ///   context (`LinearInTheContext`).
    ///
    /// A bare fence, top level, `None` on every shipped preset, Some-only in the fingerprint, the
    /// schedule id and the fork id. [`Self::validate_palw_v2`] refuses arming it (a) over a bundle
    /// that does not commit to the COMPLETE_V4 signing contexts, (b) without `palw_kary_court` and
    /// `palw_shard_court` armed at or below its height, and (c) on a network whose prompt ids are
    /// flat — a held class's ids never ride, and a flat digest cannot be opened one tile at a time.
    pub palw_held_context: Option<ForkActivation>,
    /// **The 2026-09-11 pre-mainnet audit's consensus fixes, gated to one coordinated activation.**
    ///
    /// The fence for the audit's SHALLOW findings — the corrections that are self-contained edits to
    /// the state-transition or the acceptance filter: A-1 (the acceptance filter folds the fold's
    /// step 3, closing the activation-crossing halt), B-5 (a certified quantum is paid once), AC-SLOT
    /// (a fold-failing court move does not spend the block slot), and A-2 (an undecodable lifecycle
    /// payload is tolerated at isolation). Below the fence every one reproduces the pre-fix behavior
    /// exactly, so a build carrying this fence and one without it fold every block before the height
    /// identically; at the height the set activates together. Scheduled on testnet-11 at
    /// [`PALW_RC_AUDIT_FENCE_DAA`] (a live chain gets its fixes at a height, ADR-0083 path (a)),
    /// `always()` on a mainnet card (correct from genesis), `None` on every other preset. It moves
    /// the fingerprint, so the fleet upgrades to the fenced build before the height — the coordinated
    /// flag day the operator chose.
    ///
    /// The audit's DEEP findings (B-1 merged-work escrow, C-01 fused-terminal canonical count, B-4
    /// pre_pow-inclusive execution dedup, and the C-02/C-03/C-08/BC-SYBIL court-economics cluster)
    /// ride a SEPARATE fence, [`Self::palw_audit_2026_09_11_deep`], at a later height — they are
    /// larger changes that were not ready for this fence's flag day, and splitting them keeps this
    /// day's build stable while the deep set is finished. A build with the deep set and one without
    /// carry DIFFERENT `palw_audit_2026_09_11_deep` schedules, so they diverge VISIBLY at the deep
    /// height (the fork-id gate names it) rather than silently at THIS height — which is why the deep
    /// set must not ride this fence.
    pub palw_audit_2026_09_11: Option<ForkActivation>,

    /// **ADR-0117 — a draw is one forward.**
    ///
    /// Past this fence the attempt lane's job is the class's canonical job without its decode
    /// calls: the same anchor-derived prompt and prefill, `exact_decode_tokens = 1`, so the one
    /// generated token is chosen from the last prefill position's logits and the ticket — a
    /// function of the attempt, whose trace root commits that logits row and that token (ADR-0072)
    /// — is decided by one pass over the prompt (`palw_attempt_v2::palw_attempt_job_v1`, the ONE
    /// spelling the producer that runs it and the seat that replays it both call). The class, its
    /// id, its certification, its pwu and the free-prompt lane are unchanged: the canonical job is
    /// still what a class is registered, drilled and priced by, and a prefill-only job is its
    /// prefix, which every court already adjudicates.
    ///
    /// A bare fence, top level, `None` on every shipped preset, Some-only in the fingerprint, the
    /// schedule id and the fork id. Nothing at block acceptance reads it — the job is enforced by
    /// the seats that replay it (ADR-0084 Decision 7) — which is exactly why it must be a fence:
    /// a seat on the old rule replays the two-token job, finds a root it did not compute, and
    /// voids an honest claim, so the network must switch at one height.
    pub palw_prefill_draw: Option<ForkActivation>,

    /// The fence for the audit's DEEP findings — the corrections that are protocol-level changes:
    /// B-1 (merged work is escrowed symmetrically to own work), C-01 (the fused dissection binds a
    /// canonical `step_leaf_count`), B-4 (one inference is one block, deduped on the pre_pow-inclusive
    /// execution commitment), and the court/panel economics cluster (C-02 stake-weighted draw, C-03
    /// wall-clock court charging, C-08 forced disclosure, BC-SYBIL seat stake-at-risk). A single
    /// fence for the six, at its own later height (the operator's THIRD flag day), because they were
    /// not ready when [`Self::palw_audit_2026_09_11`] shipped and holding that day for them would
    /// have stranded the shallow set. Below the fence every deep fix reproduces the pre-fix behavior
    /// exactly. Scheduled on testnet-11 at [`PALW_RC_AUDIT_DEEP_FENCE_DAA`], `always()` on a mainnet
    /// card, `None` on every other preset. Distinct from the shallow fence so a build with the deep
    /// set and one without advertise different schedules and diverge visibly at this height.
    pub palw_audit_2026_09_11_deep: Option<ForkActivation>,

    /// **The 2026-09-23 economic audit's fence — the unit, replay and admission fixes that audit
    /// proved with running tests, armed together.** `Some(0)` on testnet-12 (which arms every rule
    /// from genesis), `None` on every other preset, so a build carrying these fixes fingerprints
    /// byte-identically to one without them on testnet-11 and mainnet — the fixes are rule changes
    /// inside armed rules, and a rule change with no fence is a silent fork of every node that has
    /// not upgraded. Past it, and only past it:
    ///
    /// * the panel seat lock prices a claim's fork weight in the COLLATERAL unit its reservation was
    ///   written in ([`crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim`]) — below it
    ///   the raw derived pwu met a collateral-unit price and the 2M row's lock was 119.19x any
    ///   genesis bond (C-3);
    /// * an attempt claim carries its execution key as its `work_id`, so one execution is one claim
    ///   ACROSS chain blocks and not only within one transition (C-1);
    /// * the execution lane's credit is the same collateral-unit quantity, so the quantum constant
    ///   divides the unit it was calibrated in, and a span mints at most
    ///   [`crate::palw_execution_quanta_v1::PALW_EXEC_MAX_QUANTA_PER_SPAN_V1`] tickets (C-2);
    /// * a `PanelBound` whose seats cannot post the lock binds nothing instead of disqualifying the
    ///   block that carried it (C-3, the liveness half);
    /// * a non-fused attention class is admitted only if the geometry its price is computed from
    ///   fits the query row its graph actually reads (C-4).
    pub palw_audit_2026_09_23: Option<ForkActivation>,

    /// **The second clock: how many PALW anchors must settle before an ECONOMIC deadline may
    /// elapse** (2026-09-23 heartbeat audit). A heartbeat block advances the DAA at 2^24 hashes
    /// and no bond, and every economic deadline on the chain was a raw DAA count — a seat that
    /// signed a false `Final` walked out after 10,500 heartbeats (350 wall-clock hours, zero
    /// anchors, zero inference). `Some(depth)` makes four of those deadlines require BOTH clocks:
    /// the DAA window as before AND `depth` further `Final` attempt claims settled since the event:
    ///
    /// * a Valid seat's slash liability (`PalwSlashableLockV1::is_live_v2`);
    /// * a retiring bond's withdrawal delay (`palw_bond_collateral_is_locked_v3`);
    /// * ADR-0065 D1's bond maturity before judging (`palw_bond_maturity_window_v2`);
    /// * the mempool's long coinbase maturity (`coinbase_spend_settled`, policy layer).
    ///
    /// Operational timers — bind, receipt, challenge, court moves, replay expiry — stay on the DAA
    /// clock: a chain with no producer must still be able to time out and clean up, which is what
    /// the heartbeat lane exists for. `None` is the DAA-only rule on every deadline. Hashed
    /// Some-only, so a build that carries the field fingerprints identically to one that does not
    /// on every preset that leaves it `None`. Read by the fold through
    /// `PalwTransitionExtrasV1::settled_anchor_depth`; only meaningful past
    /// [`Self::palw_audit_2026_09_23`], which carries the state field it counts against.
    pub palw_settled_anchor_depth: Option<u64>,
    /// **How often a `Candidate` class meets its ADR-0147 admission jury, in DAA** (the user's
    /// decision, 2026-09-23: 100 DAA is the standard). `None` is ADR-0147's own period — one epoch
    /// (`palw_admission_audit_period_spans_v1`), byte for byte. A permissionlessly registered model
    /// used to wait a whole epoch (1,000 DAA on testnet-12, a day and more at the heartbeat clock)
    /// for its first jury; the jury still needs a majority of network-drawn operators proving
    /// possession, so a shorter period gives a registrant more draws, never an easier one. Hashed
    /// Some-only; a zero collapses to `None`.
    pub palw_admission_audit_period_daa: Option<u64>,
    /// **ADR-0151's execution-quantum maturity, in DAA, where a network STATES it** (user decision
    /// 2026-09-25: testnet-12's is the challenge window it applies, 120, not the lattice's 1,200).
    ///
    /// `None` is [`crate::palw_economic_safety_v1::palw_exec_quantum_maturity_daa_v1`]'s rule — the
    /// lattice challenge window, byte for byte. testnet-12 licenses every claim under ADR-0132 §7.6's
    /// short window from genesis, so the unshortened 1,200 put its first round permit ~1,342 DAA (~45 h)
    /// after launch; stated at [`crate::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1`], a Final's
    /// rights wait 120 DAA. Read through [`Self::palw_exec_quantum_maturity_v1`] into
    /// `PalwEconomicSafetyFoldV1::maturity_daa`, the one value the snapshot delay and the lock's price
    /// both read — so the residual the shorter maturity leaves (2,880 DAA of gap instead of 1,800) is
    /// priced, never free: on testnet-12's capped mint it prices the same 65,537 quanta either way.
    ///
    /// A value, not a fence: it rides `palw_economic_safety`'s height and means nothing without it,
    /// so `validate_palw_v2` refuses it without that fence, past `window_court`, and at the lattice
    /// window (the `None` rule spelled a second way would fingerprint apart from itself); and
    /// `consensus_identity_id` drops it with a scheduled fence. Hashed Some-only, so every preset
    /// that leaves it `None` — all but testnet-12 — fingerprints byte-identically to a build without
    /// the field.
    pub palw_exec_quantum_maturity_daa: Option<u64>,

    // ---- lane F1 (post-launch, 2026-09-25): the panel seed is the anchor attempt's execution ----
    /// **Lane F1, the panel seed (post-launch fence, 2026-09-25; `wf_72c1a397-e23`, CRITICAL).**
    /// For a claim whose anchor block's DAA score is at or past this fence, the panel draw, the
    /// segment assignment and the S3 sample are keyed on
    /// `H("misaka-palw/panel-v2/draw-seed/v1" ‖ the anchor ATTEMPT's execution_commitment_v3 ‖ claim)`
    /// ([`crate::palw_panel_v2::palw_panel_draw_seed_v1`]) instead of the anchor block's identity hash.
    /// Below it the seed is the anchor block, byte for byte the rule testnet-12 launched with.
    ///
    /// Why: the identity covers the attempt's ML-DSA signature, the timestamp and the nonce inside its
    /// 2^22 bucket, none of which the class lottery prices, so one lottery win let the anchor
    /// producer redraw the panel privately for a re-signature. Past the fence every identity one win
    /// can take draws one panel; another panel is another lottery win (which, while audit P0-10 is
    /// open, is still cheap — docs/t12-panel-seed-2026-09-25.md says what this does and does not
    /// close).
    ///
    /// Keyed on the ANCHOR's DAA score: a fact every node reads off the anchor header the processor's
    /// one anchor walk already finds, so reorg, IBD and a pruning-proof sync agree. Refused by
    /// `validate_palw_v2` without `palw_rcore_plus` armed at or below it (only attempt blocks anchor
    /// there, so every anchor has an execution commitment). Dormant (`None`) on every shipped preset,
    /// testnet-12 included, until an operator arms it at an independent post-launch height; hashed
    /// Some-only with the seed rule's domain beside the height, and collapsed from `Some(never())`
    /// with the other fences, so a build that carries the field fingerprints and peers exactly as
    /// one that does not until the fence is armed.
    pub palw_panel_seed_execution: Option<ForkActivation>,
    // ---- end lane F1 ---------------------------------------------------------------------------
    // ---- lane maturity (post-launch, 2026-09-26): ADR-0065 D1 from the post-launch fence ---------
    /// **Lane maturity — ADR-0065 D1's seat maturity, brought forward to a post-launch fence**
    /// (2026-09-26, the lifecycle audit's correction to the panel-seed note: "a seat bond registered
    /// before DAA 1,000 is drawn onto floor panels within minutes — registered at DAA 13, on a panel
    /// at bind DAA 21").
    ///
    /// testnet-12 arms [`Self::palw_bond_maturity`] at its own window (1,000 DAA), because
    /// `validate_palw_v2` refuses it any earlier. So for its first 1,000 DAA no window applies at
    /// all, and a bond registered after launch is drawable the moment its registration is
    /// accepted. For a claim whose ANCHOR block's DAA score is at or past this fence, the draw
    /// applies D1 exactly as `palw_bond_maturity` would — `palw_bond_maturity`'s own window, both
    /// clocks (`palw_bond_maturity_window_v2`), one subtraction (`palw_seat_maturity_floor_v1`) — so:
    ///
    /// * a NON-genesis bond is drawable only once `anchor_daa ≥ registered_daa + window`. A bond
    ///   registered BEFORE the fence gets no grandfathering: its window counts from its own
    ///   registration DAA, as it would under `palw_bond_maturity` itself, so it leaves the draw at
    ///   the fence and returns at `registered_daa + window` (a bond registered at DAA 13 returns at
    ///   anchor 1,013). Counting from the fence instead would make the answer depend on when the
    ///   fence was armed and would disagree with `palw_bond_maturity` past its own height — a
    ///   second rule, where this is the first rule starting earlier;
    /// * the genesis bonds (`registered_daa = genesis.daa_score = 0`) stay drawable at every
    ///   anchor: the floor is `anchor_daa - window` saturating at 0, and `0 > 0` is false. That is
    ///   why this fence may sit inside the window `palw_bond_maturity`'s own guard refuses — the
    ///   guard's "every genesis bond would be immature" holds only for a genesis above DAA 0, and
    ///   [`Self::validate_palw_bond_maturity_early_v1`] refuses exactly that case;
    /// * from `palw_bond_maturity`'s own height on, the two answer the same window for the same
    ///   anchor ([`palw_bond_maturity_window_in_force_v1`] is one OR over one window), so there is
    ///   no double counting and no gap at 1,000: past it the draw is byte-for-byte the released rule.
    ///
    /// Keyed on the ANCHOR's DAA like `palw_bond_maturity` (the panel is a pure function of the
    /// claim, so the validator, the assembler and the registry warning resolve one window). A
    /// claim anchored below the fence keeps the released draw even if it binds past it; a panel
    /// already bound keeps its seats. Refused by `validate_palw_v2` without `palw_bond_maturity`
    /// scheduled (its window is the only window). Dormant (`None`) on every shipped preset,
    /// testnet-12 included, until an operator arms it at the one post-launch height (after
    /// `palw_t12_arm_every_rule_from_genesis`'s pass 2, which would zero it); hashed Some-only and
    /// collapsed from `Some(never())`, so a build that carries the field fingerprints and peers
    /// exactly as one that does not until the fence is armed.
    ///
    /// **Lane maturity-ext (user decision 2026-09-26: "under the same DAA-500 fence").** The same
    /// fence also applies D1 wherever the MODEL REGISTRY counts a bond: from it a non-genesis bond is
    /// neither a ready seat toward a class's `ready ≥ k` thresholds (the lifecycle's ready count, the
    /// panel room, `NoCapablePanel`, the Activation Pool's (a)) nor in an ADR-0147 admission jury's
    /// population until `registered_daa + window` — the draw's own floor (both clocks), asked at the
    /// EVALUATING block's DAA ([`Self::palw_bond_maturity_registry_fold_at`],
    /// `PalwModelRegistryFoldV1::bond_maturity`). One field rather than two because it is one rule:
    /// "a bond counts once D1 says it is mature", with one window and one exemption (genesis bonds),
    /// and two fences could be armed apart — a jury or a ready count built of seats the draw refuses
    /// (or the reverse), the second answer to one question the registry's one-predicate rule exists
    /// to forbid. Keyed on this fence alone, not on `palw_bond_maturity`'s own height: the release
    /// never applied D1 to the registry, so unarmed testnet-12 keeps its registry past 1,000 too.
    pub palw_bond_maturity_early: Option<ForkActivation>,
    // ---- end lane maturity -------------------------------------------------------------------
    // ---- lane A (post-launch, 2026-09-26): only an operator's attempt anchors a panel ----------
    /// **Lane A, the operator-anchored panel (testnet-12's post-launch stopgap for the panel-seed
    /// CRITICAL, user decision 2026-09-26).** Past `activation` — resolved at the candidate anchor
    /// block's OWN DAA score, the key lane F1 resolves the seed rule at — a block may anchor a claim's
    /// panel only if it is an attempt produced by one of `operators` (its envelope names the bond as
    /// executor under the bond's genesis-registered key): the anchor is the first such attempt on the
    /// selected chain at or past the claim's slot, and a slot none reaches before the bind window
    /// lapses voids `BindTimeout` at the backstop (S0). See [`crate::palw_operator_anchor_v1`].
    ///
    /// Why: while audit P0-10 is open a lottery win is ~279 junk BLAKE2b draws, so whoever may produce
    /// the anchor re-rolls the panel of any claim by drawing fresh wins (lane F1 prices a re-roll at a
    /// win, not an inference). Past this fence the attacker cannot hold the anchor position; the
    /// operator's future execution commitment is not its to pick or predict, so a claim id fixed before
    /// the slot buys one fair draw. The cost is operator trust in the draw until an inference-bound
    /// ticket exists.
    ///
    /// The seed (lane F1's execution-commitment seed, which lane A requires at or below itself) is read
    /// off the EARLIEST operator attempt at or past the slot in the anchor's past, so an operator
    /// attempt displaced from the selected chain by a heavier sibling still keys the draw.
    ///
    /// `operators` is sorted, distinct and a subset of the genesis registry (`validate_palw_v2`); on
    /// testnet-12 it is all eight genesis bonds ([`crate::palw_operator_anchor_v1::PalwOperatorAnchorV1::of_genesis_bonds_v1`]).
    /// Refused without `palw_rcore_plus` and `palw_panel_seed_execution` at or below the activation. Dormant (`None`) on every
    /// shipped preset, testnet-12 included; hashed Some-only (the height, the rule's domain and the
    /// operator outpoints) and collapsed from `Some(never())` with the other fences, so a build that
    /// carries the field fingerprints and peers exactly as one that does not until it is armed.
    pub palw_operator_anchor: Option<crate::palw_operator_anchor_v1::PalwOperatorAnchorV1>,
    // ---- end lane A ----------------------------------------------------------------------------
    // ---- ADR-0160 lane liab (post-launch, after the DAA-500 release): aggregate bond liability --
    /// **ADR-0160 F-L — aggregate bond liability (testnet-12 only; the capacity redesign's lane liab,
    /// user design 2026-09-26: "per-claim担保からaggregate bond liabilityへ").** Past `activation`:
    ///
    /// * a conviction's funnel (`close_conviction_v1`), resolved at the CONVICTION's DAA, forfeits the
    ///   convicted bond WHOLE on an intent-class offence (DA default, `CourtFraud`, contradictions 9,
    ///   10, 11, 13): its posted collateral, every vesting row it is payee of that has not moved, and
    ///   every live claim (voided `AggregateForfeit`); every conviction freezes the bond (rooted
    ///   `bond_freezes`): no attempt, no draw, no backed `Valid`, no exit, no maturity — final after an
    ///   intent-class conviction, lifted `window_court` after the last conviction otherwise;
    /// * the seat side, resolved at the CLAIM's `accepted_daa`, is re-priced by the step's ramp factor
    ///   ρ: the bind duty `⌈λ/ρ⌉`/`⌈lock_2/ρ⌉` (AS-1), the `Valid` lock `max(1, ⌈lock/ρ⌉)` once the
    ///   step's credited attribution reaches 250‰ (AS-2).
    ///
    /// The value carries its own schedule (`steps`: `{ from_daa, rho, q_credit_permille }`, the first
    /// at the fence, strictly increasing — the D1 rule: a later flag day appends a step), and is
    /// hashed WHOLE, Some-only; every step's height is on the schedule the fork id reads; it collapses
    /// from `Some(never())` with the other fences. Refused by `validate_palw_v2` without
    /// `palw_rcore_plus` and `palw_offence_attribution` at or below it, off ConsensusV2, with a value
    /// [`crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::refusal_v1`] refuses, or with the
    /// bundle's mirror (`sync_palw_capacity_liability`) unsynced. Dormant (`None`) on every shipped
    /// preset, testnet-12 included — not part of the DAA-500 release.
    pub palw_capacity_aggregate_liability: Option<crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1>,
    // ---- end ADR-0160 lane liab ------------------------------------------------------------------
    /// **ADR-0083 Decision 1 — the difficulty window counts only rows priced by `bits`.**
    ///
    /// Past this fence `calculate_difficulty_bits` sizes its expected duration by the rows in the
    /// window whose lane compares a digest against `header.bits`
    /// ([`crate::pow_layer0::algo_id_is_priced_by_bits`]); heartbeat rows (ADR-0066 Decision 1's
    /// constant-target lane) still carry the global bits into the average and still bound the
    /// window's span, but no longer count as blocks the retarget must slow down. A window with no
    /// priced row answers `max_difficulty_target` — the V2 doctrine ADR-0066 states ("a V2 network
    /// runs at MAX because the class lottery, not the hash target, is its throttle") — instead of
    /// the selected parent's bits, which would leave a chain whose attempt lanes died pinned at the
    /// bits that killed them.
    ///
    /// **Why.** On testnet-11 Relaunch 5f (2026-09-04) five heartbeat emitters on the two-minute
    /// cadence put ~3.2 rows per 122 s slot into the 264-row window; the retarget read the chain as
    /// three times too fast and tightened `bits` from `0x207fffff` (p = 0.5) to `0x1f65bd04`
    /// (p = 1.5e-3) in 826 blocks, halving every ~50 slots, while the heartbeats — priced by the
    /// constant 2²⁴ — never noticed. Only the bits-priced lanes paid: the floor's chance per draw
    /// fell to 7.9e-5 × 1.5e-3, no attempt block exists after DAA 226, and the chain could not
    /// recover, because `heartbeat_interval_ms` backs off only after a BONDED parent. That is
    /// ADR-0066's F1 ("the lane prices the bonded lane off its own chain, permanently") re-entering
    /// through the row COUNT after Decision 1 took the lane's PRICE out of `bits`.
    ///
    /// **TOP LEVEL for the reason on `palw_unavailable_abstains`**: this rule has to reach a LIVE
    /// network — the one it was measured on — so old and new builds must peer until the height.
    /// A bare fence with no companion value: the predicate is a constant of this binary.
    ///
    /// Before the fence the arithmetic is byte-for-byte the legacy retarget; the fence changes one
    /// factor (the row count the expected duration multiplies) and one answer (MAX when that count
    /// is zero). Arming it on a network whose `bits` already tightened restores MAX at the fence's
    /// first block, because its window holds no priced row.
    pub palw_difficulty_priced_rows: Option<ForkActivation>,

    /// **ADR-0083 Decision 1, the half its own text states and its predicate forgot: a receipt row
    /// is not a priced row** (mainnet audit, 2026-09-05).
    ///
    /// The decision says the window counts "only rows priced by `bits`". Its predicate excluded the
    /// heartbeat lane alone, and the receipt lane (7) is not priced either — `check_pow_layer0`
    /// admits every receipt digest unconditionally (ADR-0044 Decision 6: the work is the certified
    /// quantum the block spends). Counted as priced, a receipt row measures how fast quanta are
    /// spent, not work: a producer holding a stock of certified quanta tightens every attempt
    /// lane's `bits` off its own chain, which is ADR-0066's F1 through the lane the doctrine
    /// forgot. Past this fence the retarget uses
    /// [`crate::pow_layer0::algo_id_is_priced_by_bits_v2`]; before it, byte for byte, the rule
    /// every shipped block was judged under.
    ///
    /// **TOP LEVEL, bare, for `palw_difficulty_priced_rows`' reasons**, and it requires that fence:
    /// `validate_palw_v2` refuses it armed on a preset whose first fence is dormant or later. A
    /// fresh chain arms it from genesis (a carded mainnet does); testnet-11 reaches it as a
    /// scheduled height, ADR-0083's path (a), which is the operator's call.
    pub palw_receipt_rows_unpriced: Option<ForkActivation>,

    /// **ADR-0072 Decision 8, asked where the draw is actually spent** (mainnet audit, 2026-09-06
    /// — C-1).
    ///
    /// D8's rule is "every field inside the priced bytes is pinned, or it is the challenge", and
    /// D8's own evidence is a HEADER-STAGE fact: sweeping one unpinned field over one execution
    /// "gave 4,096 distinct tickets and 4,096 distinct Layer-0 tags". A Layer-0 tag under `bits` is
    /// a valid block, a relayed header, and — by ADR-0083 Decision 1, since
    /// `algo_id_is_priced_by_bits_v2` says the attempt lane is priced — a counted difficulty row.
    /// The pins that close D8 (`palw_admission_v2::check_palw_attempt_da_pins_v1`) are asked only
    /// at the composed admission entry point, which runs in the virtual processor and only for a
    /// candidate chain block; an attempt no bond backs is `StatusDisqualifiedFromChain` there,
    /// which is not an invalidity and does not remove the row. Both of that function's inputs are a
    /// network CONSTANT and the header's own DAA score, so the pin was always decidable on the
    /// relay path and simply was not asked there.
    ///
    /// Past this fence, and only on a `ConsensusV2` network, `palw_carriage_stateless_v1` asks the
    /// three DA pins beside the ML-DSA signature, and refuses a nonce whose bucket is above
    /// [`crate::palw_attempt_v2::PALW_ATTEMPT_MAX_NONCE_BUCKET_V1`].
    ///
    /// **What this does NOT do, stated here so nobody reads the fence as the repair.** It closes
    /// three free fields and 18 of the 42 bits of per-template anchor space. It does not make an
    /// attempt row cost an execution. `trace_root`, `output_root` and `execution_root` are
    /// classified `Pin::ExecutionReplay` — pinned by a panel that never convenes for an unadmitted
    /// attempt — and `hashing::header::pre_pow_hash_64` carries `utxo_commitment`,
    /// `accepted_id_merkle_root` and `palw_state_root`, none of which any header-stage rule
    /// constrains and all of which are checked only at the UTXO stage. C-1 stays open behind this
    /// fence; its size is pinned by
    /// `an_unadmitted_attempt_header_is_still_a_priced_difficulty_row`.
    ///
    /// **TOP LEVEL and bare, for [`Self::palw_difficulty_priced_rows`]' reasons**: it decides which
    /// headers are valid, so old and new builds must peer until the height, and there is no
    /// companion value a normalisation could hide (the pins' derivation is the bundle's own state
    /// params). `None` on every shipped preset — testnet-11 and devnet keep validating every header
    /// they already hold and their fingerprints do not move. A carded mainnet states it from
    /// genesis, where it is free: no history was judged under the looser rule.
    pub palw_attempt_header_pins: Option<ForkActivation>,

    /// **ADR-0042 Decision 11's signature-context set, complete** (mainnet audit 2026-09-06, M-8).
    ///
    /// Armed, the bundle's `signature_contexts_root` is
    /// [`crate::palw_mode_v2::palw_v2_signature_contexts_root_v2`] — every ML-DSA-87 context the
    /// acceptance layer verifies under, nineteen of them. Dormant, it is the frozen nine testnet-11
    /// and devnet committed to, ten of which the acceptance layer verifies without the ruleset id
    /// naming: bond capability, DA accusation, DA disclosure, the court's close declaration and
    /// both attention moves, the free-prompt commitment and spend, the carriage commitment and the
    /// execution attestation. Two builds that spell any of those differently share an id, and a
    /// refused object is SKIPPED while the block stands — so they split the registry with no block
    /// ever rejected and nothing in either log saying so.
    ///
    /// **Bare, TOP LEVEL and GENESIS-ONLY, for [`Self::palw_prompt_ids_merkle`]'s reasons.** A
    /// fence inside the bundle moves `palw_ruleset_id_v2`, which `for_each_fence` never descends
    /// into, so every old/new pair would fail the handshake outright. `validate_palw_v2` refuses
    /// any height but `always()`: the set is what every signature ON the network means, and no
    /// verifier of a signature holds the object's block height.
    ///
    /// **`None` on every shipped preset, and it must stay that way on testnet-11 and devnet.**
    /// Arming it moves that network's `consensus_params_id`, which is a re-mint of a live chain.
    /// A carded mainnet states it from genesis (`mainnet_card_base_v1`), which is the designed
    /// path for a network with no history.
    pub palw_signature_contexts_v2: Option<ForkActivation>,

    /// **ADR-0105 — a heartbeat never turns a bonded block red.** `None` on every shipped preset,
    /// so the behaviour is byte-identical to not having the field at all.
    ///
    /// Measured on testnet-11 on 2026-09-10 (11:41Z-13:20Z): once a heartbeat was the selected
    /// parent, every bonded block landed ~17 minutes after the template it was drawn on, with
    /// eight 120-second heartbeats in its anticone. At `ghostdag_k = 1` that anticone made it RED,
    /// so its 2^20 of fork-choice work never entered any block's blue work, the DNS anchor could
    /// never be buried `required_work_depth` deep, and the EVM bridge stayed paused for as long as
    /// any heartbeat miner ran. The heartbeats that did it weigh ε = 1 each.
    ///
    /// Past this fence GHOSTDAG colors a non-heartbeat mergeset candidate against the non-heartbeat
    /// blues ALONE: a heartbeat in its anticone is not counted, its anticone size is not consulted,
    /// and neither enlarges the other's count. A heartbeat candidate is still colored against every
    /// blue (it yields to bonded blocks, as it always did). Keyed on the CANDIDATE's own DAA score,
    /// which is fixed before any block that merges it is colored — so the rule is decidable
    /// without the new block's own GHOSTDAG output, the shape `palw_lane_blue_work_v1` already has.
    ///
    /// **What it does NOT do**: it does not make the stale bonded block the selected parent. The
    /// heartbeat that merges it carries its work, so the chain's weight and the DNS work depth are
    /// right, but the heartbeat lane keeps the chain at its recovery cadence while any heartbeat
    /// miner runs. Handing the chain back to the bonded lane is the heartbeat miner's node-side
    /// yield (ADR-0105 Decision 2), because a consensus rule that held the heartbeat slot for a
    /// merged bonded block would let a producer whose blocks can never become the sink hold the
    /// clock — ADR-0060 §1's wedge, re-entering through the lane that ended it.
    ///
    /// **A bare fence with no companion value**, the [`Self::palw_frontier_provenance`] rule for
    /// its reason: the classes are algorithm ids and the lane's own fence, so there is nothing
    /// beside this height for [`Self::consensus_identity_id`] to normalise away.
    ///
    /// **TOP LEVEL for the reason on `palw_unavailable_abstains`**: it is a fork-choice rule that
    /// has to be able to reach a LIVE network by activation, and a fence inside the V2 bundle moves
    /// `palw_ruleset_id_v2`, which `for_each_fence` never descends into. Arming it on testnet-11
    /// is a flag day — past the height the blue set, blue work and blue score of every block whose
    /// mergeset holds a bonded block beside a heartbeat anticone change — and `validate_palw_v2`
    /// refuses it on a network whose heartbeat lane is not armed. Read it through
    /// [`Self::palw_heartbeat_transparent_fence`] only.
    pub palw_heartbeat_transparent: Option<ForkActivation>,

    // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
    /// **ADR-0105 §11 — heartbeat transparency stops at the merging block's own selected chain.**
    /// `None` on every shipped preset, testnet-12 included, so no fingerprint, identity, schedule or
    /// fork id moves on any network until a post-launch flag day arms it at a height.
    ///
    /// Past this fence a non-heartbeat mergeset candidate is heartbeat-transparent
    /// ([`Self::palw_heartbeat_transparent`]) only when its selected parent lies on the merging
    /// block's own selected chain, at most `merge_depth` blue score below the merging block's
    /// selected parent — a draw that landed late on the chain it was drawn on, the block ADR-0105 was
    /// written for. Any other non-heartbeat candidate is colored classically, against every blue, the
    /// other branch's heartbeats included. Keyed, like the transparency fence, on the CANDIDATE's own
    /// DAA score, which is fixed before any block that merges it is colored: a candidate below the
    /// height is colored by the shipped rule in every block that ever merges it, byte for byte.
    ///
    /// Why (the 2026-09-25 heartbeat double-spend verifier, probes g2/h1/f/h2): under the shipped rule
    /// a bondless heartbeat miner's private branch merges the public chain's attempts BLUE — its own
    /// beats are invisible to them — so every public 2^20 counts on both branches and the private one
    /// out-weighs the public one by its own ε: X reorged away inside the merge depth, with no bond.
    ///
    /// Refines the transparency fence, so `validate_palw_v2` refuses it unless that fence is in force
    /// at or below its height. Hashed Some-only in every writer. Read it through
    /// [`Self::palw_heartbeat_transparent_same_chain_fence`] only.
    pub palw_heartbeat_transparent_same_chain: Option<ForkActivation>,
    // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
    // ---- ADR-0160 lane escrow (post-launch, rcore/cap-escrow) ----
    /// **ADR-0160 F-E — the escrow is the reward itself: `E` leaves the bond.** `None` on every shipped
    /// preset, testnet-12 included, so no fingerprint, identity, schedule or fork id moves until a
    /// post-launch flag day arms it at a height.
    ///
    /// Past this fence — keyed on a claim's own `accepted_daa`, so a claim accepted below it keeps
    /// `w + E` for its whole life — the escrow slot of a claim's bond commitment holds `m_c`
    /// (`crate::palw_escrow_funding_v2::palw_escrow_term_v2`: `E` for C7, without a credit or below
    /// `q_seat` = 250‰; otherwise `max(m*, ⌈E/ρ⌉)` with `m*` priced against the conviction floor of
    /// the claim's `accepted_daa` — `palw_monetary_prelicense_risk_v2`) instead of the withheld reward
    /// `E`, which stays withheld to `Final` plus the vesting row's maturity and is burned on a void or a
    /// conviction; a seat bound to an attributable attempt reserves its eligibility `max(duty_bind,
    /// lock_2)` — its duty wherever no credit cut it (`palw_escrow_bind_reserves_the_lock_v1`); and an
    /// attempt claim voided for a reason nobody is convicted under keeps its commitment for
    /// `window_receipt` past the void (E-4, the withdrawal principle). Mirrored into the V2 bundle by
    /// [`Self::sync_palw_capacity_escrow`].
    ///
    /// `validate_palw_v2` refuses it off ConsensusV2; without `palw_audit_2026_09_23` (the escrow slot
    /// itself), `palw_rcore_plus` (SR-1's ledger) and `palw_offence_attribution` (a void closes its
    /// courts) at or below it; with the bundle's mirror unsynced; and where a retiring network would
    /// retire a claim before its obligation hold ends. ADR-0160 also requires F-W
    /// (`palw_capacity_weight_cap`) and F-L (`palw_capacity_aggregate_liability`) at or below it — an
    /// `m` below `E` is safe only with J-1's cap and the aggregate freeze; those fields land with their
    /// lanes and the two refusals are wired on `rcore/cap-int`. Hashed Some-only in every writer.
    pub palw_capacity_escrow_at_licence: Option<ForkActivation>,
    // ---- end ADR-0160 lane escrow ----
    /// **ADR-0107: a class's cadence share grows only on work that reached `Final`.**
    ///
    /// The growth rule (ADR-0054, `derive_class_share_growth_v1`) reads the closed epoch's
    /// `produced_blocks`, which the transition increments when an attempt is ACCEPTED — the claim
    /// still `Provisional` — and which a later void never gives back ("voiding releases exposure,
    /// never production": correct for the epoch BUDGET, whose job is to stop a re-roll). The same
    /// counter as the growth SIGNAL means blocks nobody ever verified, and claims that void, earn a
    /// class permille exactly as certified work does. Past this fence growth also requires that the
    /// class's attempt claims which reached `Final` during the closed epoch number at least that
    /// epoch's budget; decay, the budget and the retarget keep reading `produced_blocks`.
    ///
    /// **Readable only while the epoch's finals are still in the state**, so `validate_palw_v2`
    /// refuses arming it unless `claim_retirement_daa` is 0 (claims never retire) or longer than an
    /// epoch — a claim finalized at the start of the closed epoch must still be there at its end.
    ///
    /// **A bare fence, TOP LEVEL**, for the `palw_fp_da_pins` reasons: no companion value for the
    /// identity to normalise away, and a fence inside the bundle would move `palw_ruleset_id_v2`
    /// and refuse every old/new pair at the handshake. `None` on every shipped preset; read it
    /// through [`Self::palw_share_growth_final_fence`] only.
    pub palw_share_growth_final: Option<ForkActivation>,

    /// **P0a — the GDN key-head count is derived, and the held map that reads it spells it**
    /// (`docs/design/palw/tir/phase-f-integration.md` §3.3). Past this fence a registration may
    /// carry a profile of version 3 (`crate::palw_step::PALW_STEP_OBJECT_VERSION_V3`: `k_heads =
    /// width(GatedDeltaNet.ref0) / gdn_head_k_dim`) on the held composition v5
    /// (`crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v5`, whose recurrence half gathers a
    /// conv window row of `2 · k_heads · k + v_heads · v` lanes); before it the admission gate
    /// refuses such a class by name (`GdnKeyHeadsNeedsItsFence`) — which a build without this field
    /// also does, in `validate_shape`. Past it (hygiene) the gate also refuses a V1/V2 profile whose
    /// derived key-head count is not its value-head count on a recurrence map written over one head
    /// count (`GdnMapAssumesEqualHeads`): such a class can never produce a checkpoint.
    ///
    /// No kernel is added, so `court_catalog_root` does not move. **A bare fence, top level**:
    /// `None` on every shipped preset, Some-only in both fingerprints, collapsed from
    /// `Some(never())` in the identity's normaliser; [`Self::validate_palw_v2`] refuses a genesis
    /// that registers a version-3 profile unless it is armed from genesis. Read it through
    /// [`Self::palw_gdn_key_heads_fence`] only. testnet-12 schedules it in
    /// [`PALW_T12_POST_LAUNCH_FENCES_V4`], whose height is the user's to set.
    pub palw_gdn_key_heads: Option<ForkActivation>,

    /// ADR-0042 Decision 1 (PR-10): the ONE PALW switch on the V2 lineage. `Disabled` on every
    /// shipped preset. A network is in exactly one mode; `ConsensusV2` carries the whole atomic
    /// ruleset and is validated at construction ([`Params::validate_palw_v2`]) — including the
    /// rule that a V2 network may not set ANY of the five V1 fences above and may not activate
    /// any V1 PALW PoW: there is no path that runs half of two lineages. The five `Option`
    /// fences above remain for the V1/legacy lineage only and stay `None` forever on V2
    /// networks; they retire with the V1 substrate, not before.
    pub palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode,

    /// kaspa-pq Phase 3 PoW (ADR-0007): activation of the compute-only **BLAKE2b-512 ∥ SHA3-512**
    /// Layer-1 (`POW_ALGO_ID_BLAKE2B_SHA3 = 3`), which supersedes the Phase-2 Argon2id to make header
    /// verification ~10^4× cheaper (the IBD/catch-up bottleneck). Past this DAA score every block
    /// header MUST declare `algo_id = 3`; before it, the Phase-1 kHeavyHash (`algo_id = 1`).
    /// `always()` ⇒ BLAKE2b-SHA3 from genesis (testnet/mainnet); `never()` ⇒ stay kHeavyHash
    /// (devnet/simnet keep fast local PoW). Genesis (the parentless trusted root) is exempt.
    pub pow_blake2b_sha3_activation: ForkActivation,

    /// Reserved legacy algo-4 activation. Never armed by current network presets;
    /// the inference PoW implementation is removed. Retained as fingerprint input.
    pub pow_palw_activation: ForkActivation,

    /// Reserved legacy algo-5 activation, retained as fingerprint input only.
    pub pow_palw_ollama_activation: ForkActivation,

    /// kaspa-pq: PQ-only enforcement mode for this network (ADR-0019 /
    /// docs/kaspa-pq-design-mldsa87.md). `Consensus` on every kaspa-pq net.
    pub pq_enforcement: PqEnforcementMode,

    /// DAA score at/after which `PqEnforcementMode::Consensus` takes effect.
    /// `0` on kaspa-pq nets (PQ-only from genesis).
    pub pq_activation_daa_score: u64,

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): DAA score at/after which the
    /// EVM execution lane is active on this network. Past this score, a block
    /// header must be version `>= EVM_HEADER_VERSION` and may carry a non-empty
    /// `evm_payload`; before it, the `evm_payload` must be empty (see
    /// `body_validation_in_isolation::check_evm_payload`). `u64::MAX` ⇒ EVM never
    /// active on this net (mainnet/devnet/simnet for now); a finite value (or
    /// `0` for genesis-active) ⇒ active. Mirrors the `pos_v2_activation_daa_score`
    /// / `pq_activation_daa_score` fence precedent.
    pub evm_activation_daa_score: u64,
    /// kaspa-pq EVM Lane gas-pool v2 fence. Below this DAA score the executor uses
    /// the v1 strict declared-gas prefix-take (one over-cap declared gas_limit, or a
    /// re-included already-accepted tx, blocks every later tx in the block). At/above
    /// it the executor switches to the Ethereum-style sequential gas pool: declared
    /// gas only gates admission to the pool, the pool is debited by ACTUAL gas used,
    /// acceptance-skipped (class-2) txs consume nothing, and a non-fitting tx is
    /// skipped WITHOUT blocking later (smaller) txs — the EVM-lane liveness fix.
    /// CHANGES execution results ⇒ activation-gated (consensus fork). `u64::MAX` ⇒
    /// inert (every net until a deploy sets a finite score). Mirrors the
    /// `evm_activation_daa_score` fence precedent.
    pub evm_gas_pool_v2_activation_daa_score: u64,

    /// Audit M-03: DAA score at/after which the F002 withdrawal cap is enforced —
    /// a tx whose withdrawals would push an accepting block over
    /// `MAX_WITHDRAWALS_PER_EVM_BLOCK` is a class-2 skip. `u64::MAX` ⇒ inert
    /// (withdrawals uncapped, execution byte-identical). A consensus rule, so it
    /// is activation-fenced like the gas-pool-v2 / evm-activation precedents;
    /// activating it is a coordinated deploy.
    pub evm_f002_withdraw_cap_activation_daa_score: u64,

    /// **The bridge ledger** (2026-09-24): DAA score at/after which the EVM lane's committed
    /// `evm_total_native_balance` is the L1's CHECKED ledger of the EVM supply it backed.
    ///
    /// The bridge burns on one side and mints on the other; nothing locks. What bounds the UTXOs
    /// the lane can materialize is therefore only the executor's own correctness: a balance that
    /// appears in the EVM without a deposit (an interpreter or intercept defect) withdraws like any
    /// other, and the accumulator that should have noticed saturates instead of refusing. Past
    /// this fence it refuses: deposits and market sell credits raise the ledger; basefee burns,
    /// F002 withdrawals and burned (filled) market escrows lower it; a user tx that would drive it below zero is a
    /// class-2 skip. That is the ceiling a lock-and-mint bridge gets from its pool, without a pool.
    ///
    /// A market escrow is the one draw that leaves in two steps: the tx that parks it in the writer
    /// RESERVES it out of that block's headroom (so the tx can still be skipped), and the committed
    /// ledger falls only when the child's settlement burns a filled escrow; a refused one is refunded
    /// inside the EVM and never leaves it.
    ///
    /// `u64::MAX` ⇒ inert (the accumulator saturates, execution byte-identical). Hashed into the
    /// fingerprint only when finite, so every network that leaves it inert fingerprints as before.
    pub evm_bridge_ledger_activation_daa_score: u64,

    /// PREA v1.1 §9 / P0-1: DAA score at/after which the F003 `MLDSA87_VERIFY`
    /// precompile (`MISAKA_MLDSA_VERIFY_PRECOMPILE`) is REGISTERED. `u64::MAX` ⇒
    /// inert (handler not registered, a call to `0x…F003` behaves as a call to an
    /// empty account — byte-identical execution, genesis/state-root unchanged). A
    /// consensus rule (enabling a precompile changes execution), so activation-
    /// fenced like the gas-pool-v2 / f002-withdraw-cap / evm-activation precedents;
    /// activating it is a coordinated deploy with a frozen `F003_VERIFY_GAS` + caps.
    pub evm_f003_mldsa_verify_activation_daa_score: u64,

    /// §12 Phase-7: DAA score at/after which the EVM lane commits the exact
    /// Ethereum EIP-2718 TYPED receipt root (`roots::receipts_root_v2`) in
    /// `EvmExecutionHeader.receipts_root`. `u64::MAX` ⇒ inert: the v1 borsh-MPT
    /// receipts root (`roots::receipts_root`) is committed, byte-for-byte
    /// unchanged. The committed `receipts_root` feeds the EVM commitment, so the
    /// switch is a CONSENSUS FORK — activation-fenced like the gas-pool-v2 /
    /// f002-withdraw-cap / f003 precedents and frozen at activation. Receipt logs
    /// and the aggregate `logs_bloom` are unaffected; only the root ENCODING changes.
    pub evm_typed_receipt_root_activation_daa_score: u64,
}

/// **ADR-0062 SA-6: what the data-availability court adds to a claim's lattice.**
///
/// Zero unless `palw_da_court` is installed, which is what makes every shipped preset's depths and
/// bounds byte-identical to a build without this ADR at all.
///
/// Past the fence it is the amendment's literal `accuse + disclose`, which is deliberately the
/// LOOSE bound. The tight one is zero: an accusation is refused unless the whole disclose window
/// fits inside the claim's own `trace_retention_daa` (`bind + receipt + challenge + court` from
/// acceptance, pinned by ADR-0072 Decision 8), so a DA session always ends at or before a point the
/// claim's normal longest path — `2·(bind + receipt) + challenge + court`, the redrawn one — has
/// already passed. Pinning the loose form anyway costs a dormant network nothing and means an
/// operator arming this fence is asked for the horizon the ADR asks for, not the one this
/// implementation happens to need.
/// **The pruning horizon a V2 ruleset needs, as a pure function of its own windows** — the ONE
/// derivation `with_palw_v2_depths` applies and [`Params::consensus_identity_id`] re-applies with
/// the data-availability term absent (ADR-0062 SA-6; testnet-11's DA-court flag day, 2026-09-06).
///
/// Two bounds and a rounding: the anticone-finalization lower bound, the claim lattice (two
/// bind+receipt pairs — a claim is revived once — plus the challenge and court windows, plus the
/// DA court's `accuse + disclose` where its fence is in force), and the `% finality_depth` offset
/// the pruning-sample walk needs. The inherited floor is deliberately NOT part of it: every V2
/// preset's depth is decided by these two bounds, which
/// `the_v2_pruning_depth_is_the_derivation_and_the_inherited_floor_never_binds` proves, and a floor
/// that bound would leave the identity's normalisation unable to reproduce the pre-fence value.
///
/// **ADR-0152 §4-quater P-1:** past `palw_class_verify_deadline` the lattice is the class-verify
/// one ([`palw_v2_claim_lattice_daa_v1`]) — keyed, like the DA term, on the fence's Some-ness, so no
/// network's horizon depends on a height.
pub fn palw_v2_pruning_depth_v1(
    blockrate: &BlockrateParams,
    bundle: &crate::palw_mode_v2::PalwConsensusParamsV2,
    da_court: Option<ForkActivation>,
    class_verify_deadline: Option<ForkActivation>,
) -> u64 {
    palw_v2_pruning_depth_for_lattice_v1(blockrate, bundle, palw_v2_claim_lattice_daa_v1(bundle, da_court, class_verify_deadline))
}

/// **The claim lattice: the longest stretch past a claim's acceptance that a judgement of it can be
/// anchored on** — the ONE lattice the pruning depth is derived from ([`palw_v2_pruning_depth_v1`])
/// and `validate_palw_v2` (K18) holds the depth to, so the two cannot disagree.
///
/// * Below `palw_class_verify_deadline` (every network but testnet-12): two bind+receipt pairs (a
///   claim is revived once), the challenge and court windows, and the DA court's `accuse + disclose`
///   where its fence is set ([`palw_v2_da_court_lattice_daa`]) — the rule as it stood, byte for byte.
/// * **Past it (ADR-0152 §4-quater P-1, U-D3)**: the receipt window is `max(window_receipt, D_cap)`
///   (a measured class's claims are priced up to D_cap, and their receipt window is then D_cap), and
///   the DA term is the long-D pause bound — the span of `R_eff` at D_cap
///   ([`palw_class_verify_reff_span_daa_v1`]; DA-8 unchanged: a session fits `R_eff`), where the DA
///   court is set. On testnet-12's windows: `2(600 + 16,000) + 1,200 + 3,000 + 37,520 = 74,920`.
///
/// Keyed on the fence's Some-ness (a `never()` is absence), exactly as the DA term is keyed on the DA
/// court's, so a ruleset's horizon is a genesis fact: a pruning point can never move backward (K37),
/// so a depth for D_cap can only be chosen at genesis — which is why the fence is genesis-only.
pub fn palw_v2_claim_lattice_daa_v1(
    bundle: &crate::palw_mode_v2::PalwConsensusParamsV2,
    da_court: Option<ForkActivation>,
    class_verify_deadline: Option<ForkActivation>,
) -> u64 {
    let state = &bundle.state;
    if class_verify_deadline.filter(|f| *f != ForkActivation::never()).is_none() {
        return 2 * (state.window_bind() + state.window_receipt())
            + state.window_challenge()
            + state.window_court()
            + palw_v2_da_court_lattice_daa(da_court, state);
    }
    let receipt = state.window_receipt().max(crate::palw_class_verify_deadline_v1::PALW_CLASS_VERIFY_CAP_DAA_V1);
    let pause = if da_court.is_some() { palw_class_verify_reff_span_daa_v1(state) } else { 0 };
    2 * (state.window_bind() + receipt) + state.window_challenge() + state.window_court() + pause
}

/// **The span of `R_eff` at D_cap** (ADR-0152 §4-quater.8): `2(W_bind + W_r) + max(wc, 1) + W_court +
/// W_disclose` with `W_r = max(window_receipt, D_cap)` and `wc` the challenge window a licence gets —
/// the longest a long-D claim's retention (and so its DA sessions, DA-8) can run past acceptance.
/// Past `palw_class_verify_deadline` every licence gets the short window
/// (`PALW_SHORT_CHALLENGE_WINDOW_DAA_V1`, 120): `validate_palw_v2` refuses the fence without
/// `palw_short_challenge_window` at or below it, since long-D is measured against that same 120. Read
/// as the constant rather than off the bundle's mirror, so the depth `with_palw_v2_depths` derives
/// before the mirrors are synced is the depth every later reader re-derives. 37,520 on testnet-12's
/// windows.
pub fn palw_class_verify_reff_span_daa_v1(state: &crate::palw_state_v2::PalwStateParamsV2) -> u64 {
    let receipt = state.window_receipt().max(crate::palw_class_verify_deadline_v1::PALW_CLASS_VERIFY_CAP_DAA_V1);
    2 * (state.window_bind() + receipt)
        + crate::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1
        + state.window_court()
        + crate::palw_state_v2::palw_da_disclose_window_daa_v1(state)
}

/// [`palw_v2_pruning_depth_v1`]'s depth for a given claim `lattice`: the anticone-finalization lower
/// bound or the lattice, whichever is longer, rounded off the finality boundary the pruning-sample
/// walk needs. The one rounding, shared so a depth sized for a longer lattice (E-11) is laid out the
/// way every V2 preset's is.
pub fn palw_v2_pruning_depth_for_lattice_v1(
    blockrate: &BlockrateParams,
    bundle: &crate::palw_mode_v2::PalwConsensusParamsV2,
    lattice: u64,
) -> u64 {
    let k = blockrate.ghostdag_k as u64;
    let finality_depth = bundle.state.window_challenge() / 2;
    let lower_bound = finality_depth + blockrate.merge_depth * 2 + 4 * blockrate.mergeset_size_limit * k + 2 * k + 2;
    let mut depth = lower_bound.max(lattice);
    let m = depth % finality_depth;
    if m <= k {
        depth += k + 1 - m;
    } else if m >= finality_depth - k {
        depth += (finality_depth - m) + k + 1;
    }
    depth
}

pub fn palw_v2_da_court_lattice_daa(fence: Option<ForkActivation>, state: &crate::palw_state_v2::PalwStateParamsV2) -> u64 {
    if fence.is_none() {
        return 0;
    }
    crate::palw_state_v2::palw_da_accuse_window_daa_v1(state)
        .saturating_add(crate::palw_state_v2::palw_da_disclose_window_daa_v1(state))
}

/// **How long a retiring bond's collateral stays locked at `daa_score`** (ADR-0062; mainnet audit
/// 2026-09-05, "should fix" `palw_mode_v2.rs:938`).
///
/// The bundle's own `withdrawal_delay_daa` outlasts the ARITHMETIC court's longest path — that is
/// the interlock `PalwConsensusParamsV2::validate_ruleset_shape` proves. The data-availability
/// court pauses a claim for `accuse + disclose` on top of that path, so past its fence the delay in
/// force is the bundle's plus that lattice: a bond that could retire inside the DA windows would
/// take its collateral back before its withholding stopped being provable.
///
/// **Resolved at a height, not rewritten into the bundle.** The delay is a bundle field, inside
/// `palw_ruleset_id_v2`, inside every fingerprint and identity — raising it there would make an
/// upgraded node a different network from a node still on the previous build, and testnet-11 is a
/// live chain whose fleet upgrades one host at a time. Resolving it here keeps the ruleset
/// byte-identical and moves only what the fence moves, when the fence moves it.
pub fn palw_v2_bond_withdrawal_delay_at_v1(
    bundle: &crate::palw_mode_v2::PalwConsensusParamsV2,
    fence: Option<ForkActivation>,
    daa_score: u64,
) -> u64 {
    let armed = fence.is_some_and(|f| f.is_active(daa_score));
    bundle.bond.withdrawal_delay_daa().saturating_add(if armed { palw_v2_da_court_lattice_daa(fence, &bundle.state) } else { 0 })
}

mod activation;
mod identity;
mod mirror;
mod validation;

impl Params {
    /// Everything about this network's PALW fence that must be true before a node runs it.
    ///
    /// **This function exists because nothing called the checks.** `PalwClassRegistrationV1::
    /// validate`, `PalwScheduleParamsV1::validate` and `stage2_eligible` were all implemented and
    /// well tested, and all three had zero non-test callers (re-audit 2026-08-17): installing
    /// `palw_credit = Some(..)` ran none of them. A fence could therefore ship windows exceeding
    /// the pruning horizon, a ceiling its own measurement does not derive, or a registration
    /// whose routing keys do not match its class tag — every one of which the code already knew
    /// how to refuse, and none of which anything asked it about.
    ///
    /// `Ok(())` when the fence is `None`: a network with no PALW credit params has nothing to
    /// check, which is every shipped preset today.
    /// ADR-0042 Decision 1's config gate (PR-10): the mode is atomic, or the network cannot be
    /// built.
    ///
    /// * `Disabled` / `LegacyTn11`: nothing new — the V1 gate below judges the V1 fences.
    /// * `ConsensusV2`: the bundle's startup invariants must hold, AND the params set must be
    ///   pure V2 — every V1 fence `None`, every V1 PALW PoW activation `never()`. A node that
    ///   accepted a mixed set would run half of two rulesets, which is the five-fences defect
    ///   reborn with an enum in front of it.
    /// **`finality_depth` and `pruning_depth` for a V2 network, derived from its own bundle.**
    ///
    /// Split out of `palw_v2_params_on_base` because a second caller assembling a V2 params set by
    /// hand restated only part of it and then failed the lattice bound — the same "one field of a
    /// derived set" shape as [`Self::with_two_minute_cadence`], one level down. Call it after the
    /// cadence and before installing the bundle.
    pub fn with_palw_v2_depths(mut self, bundle: &crate::palw_mode_v2::PalwConsensusParamsV2) -> Self {
        // Sized against the bundle's own challenge window, which is what the schedule rule compares:
        // `finality_depth < w_challenge`. Half the window, so the inequality holds with margin rather
        // than by one.
        self.blockrate.finality_depth = bundle.state.window_challenge() / 2;
        // The horizon, from the ONE derivation the identity's normalisation re-applies. Raised,
        // never lowered: an inherited depth wider than the derivation stays (no preset's is —
        // `the_v2_pruning_depth_is_the_derivation_and_the_inherited_floor_never_binds`).
        self.blockrate.pruning_depth = self.blockrate.pruning_depth.max(palw_v2_pruning_depth_v1(
            &self.blockrate,
            bundle,
            self.palw_da_court,
            self.palw_class_verify_deadline,
        ));
        self
    }

    /// **Cadence and depths together, reading the bundle out of the params themselves.**
    ///
    /// The form a caller wants once `palw_consensus_mode` is already installed. The assembly uses
    /// the two halves separately because it derives the depths before it has anywhere to put the
    /// bundle; every OTHER caller sets the mode first, and every other caller that hand-rolled a
    /// V2 params set got the cadence wrong — which is what this exists to make hard.
    pub fn with_palw_v2_cadence(self) -> Self {
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.clone()),
            _ => None,
        };
        let out = self.with_two_minute_cadence();
        let mut out = match bundle {
            Some(bundle) => out.with_palw_v2_depths(&bundle),
            None => out,
        };
        // **And the mint, because after the audit's genesis gate a V2 ruleset that is not minted at
        // the ambient target cannot exist** (mainnet audit, 2026-09-05).
        //
        // This is the "make this params set V2-shaped" helper, and every caller is a V2 fixture
        // that asserts its own bundle is "a ruleset a node would really boot on". Over the mainnet
        // base — which is what those fixtures use — it was not: `MAINNET_GENESIS`' `bits` is the
        // hash lineage's `0x1f7fffff`, 256x above a V2 network's ambient maximum, so 49 pipeline
        // fixtures were rehearsing a network no carding ceremony could legally mint. Minting them
        // correctly is the fix; loosening the gate to admit them would be shaping the rule to
        // agree with the fixture.
        if matches!(out.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            out.genesis.bits = out.max_difficulty_target.compact_target_bits();
        }
        out
    }

    /// **Every field that means "how fast this chain runs", set together.**
    ///
    /// A PALW V2 network runs one block per 120 s (ADR-0038 Decision H). That is not one field:
    /// `ghostdag_k`, `mergeset_size_limit`, `merge_depth`, `coinbase_maturity` and the DNS windows
    /// are all counts derived from the same rate, and the DNS windows are block counts that state a
    /// duration only in company with a cadence. Writing `target_time_per_block` alone — which is
    /// what every assembly path here used to do — leaves a chain running a 10 bps DAG and 10 bps
    /// windows on a 120 s clock. The visible end of that is `bond_spend_gate`, which enforces
    /// `unbonding_period_blocks` in consensus: inherited from a 10 bps base, the "14 day" period is
    /// about 46 years, so validator collateral is confiscated by a unit mismatch.
    ///
    /// Three groups move, and nothing else does:
    ///
    /// * **The DAG parameters**, from `BlockrateParams::new_two_minute_bps()` — the one place they
    ///   are derived together from lambda = 1/120. A base that deliberately widened its DAG keeps
    ///   that width, because `max_block_parents` is a topology choice and not a clock reading.
    /// * **Crescendo**, forced active. Mainnet activates it at DAA 110 165 000 and runs
    ///   `pre_crescendo_target_time_per_block` below that, so a V2 mainnet would have produced
    ///   10 bps headers under 120 s-denominated consensus windows for its whole pre-fork life.
    /// * **The DNS windows**, via [`DnsParams::at_two_minute_cadence`].
    ///
    /// Callers that raise `finality_depth` / `pruning_depth` afterwards must keep doing so: this
    /// sets the canonical pair, and the claim lattice needs more.
    ///
    /// **This is the exact identity on a base already sized for 120 s** (the RC's is), which is
    /// what lets a live network's assembly call it without moving `consensus_params_id`.
    pub fn with_two_minute_cadence(mut self) -> Self {
        let widened_parents = self.blockrate.max_block_parents;
        self.blockrate = BlockrateParams::new_two_minute_bps();
        if widened_parents > self.blockrate.max_block_parents {
            self.blockrate = self.blockrate.increase_max_block_parents(widened_parents);
        }
        self.crescendo_activation = ForkActivation::always();
        if let Some(dns) = self.dns_params.take() {
            self.dns_params = Some(dns.at_two_minute_cadence());
        }
        self
    }

    /// **The rules whose change invalidates history — everything the fingerprint covers EXCEPT the
    /// activation schedule.**
    ///
    /// Two builds that agree on every rule and differ only in WHEN a future rule turns on produce
    /// the same identity. That is the whole point (audit M1-6): `consensus_params_id` hashes the raw
    /// score of every fence, and the handshake refuses any peer whose value differs — so publishing
    /// a build that schedules a fence at a coordinated future height H disconnects the first
    /// upgrading operator from every un-upgraded peer IMMEDIATELY, and two disjoint meshes build two
    /// chains for the whole rollout. Nothing about H is involved; the split starts at deploy. That
    /// is what made the standing "no re-genesis, ship as an activation" policy unusable on a
    /// value-bearing network.
    ///
    /// Implemented by normalising the fences and re-using the same writer, so there is exactly one
    /// description of what the fingerprint covers. Peers still diverge at H — that is a consensus
    /// question and fork choice answers it — but they stay peers until then, which is the only way
    /// a scheduled upgrade can propagate at all.
    ///
    /// **What "normalised" means, and why it is not a single sentinel** (re-audit R-1). Collapsing
    /// every fence to `never` erased the difference between *scheduled* and *already in force*, and
    /// those are not the same fact. Two builds that disagree about a FUTURE height agree about every
    /// block either can produce today. Two builds that disagree about a fence that is active NOW
    /// disagree about blocks right now — and that is not hypothetical, it is exactly what audit
    /// M1-1 did: it moved `bond_spend_gate_mergeset_activation_daa_score` from `u64::MAX` to `0`,
    /// both of which used to normalise to the same value. A node on the old build and a node on the
    /// new one produced the SAME identity, so the handshake kept them — and they disagree from
    /// block 1 about whether a merge-blue bond spend is valid. A refusal became a silent fork.
    ///
    /// So the identity preserves the one thing a peer can check without a shared clock: whether a
    /// fence is **active at genesis**. `0` stays `0`; every other height, `u64::MAX` included,
    /// collapses to `never`. Genesis is the only reference point two nodes agree on regardless of
    /// how far either has synced, which is what makes this comparable at handshake time — a
    /// tip-relative predicate would refuse every node that is still doing an IBD.
    ///
    /// The residual, stated rather than hidden: two builds that both schedule a fence in the past,
    /// at DIFFERENT non-zero heights, still produce one identity and can disagree about history
    /// between those heights. Closing that needs a shared height in the handshake, which this
    /// message does not carry.
    pub fn consensus_identity_id(&self) -> Hash {
        let mut normalized = self.clone();
        // Active at genesis stays distinguishable; anything else is "not yet".
        normalized.for_each_fence(&mut |score| *score = if *score == 0 { 0 } else { u64::MAX });
        normalized.normalize_values_a_scheduled_fence_drags_with_it();
        // **ADR-0066 SA-4 lands in [`Self::consensus_schedule_id`], not here** — see the note
        // beside the values that method hashes. A scheduled fence's companion VALUE is reported, never gated:
        // gating on it is what makes arming a flag day, and this file records a live rollout that
        // depended on it not being one.
        normalized.consensus_params_id()
    }

    /// **Is the heartbeat lane open at `daa_score`?** (ADR-0066 Decision 1.)
    ///
    /// Two conditions, both necessary. The fence must be in force — the lane changes block
    /// validity, so it is an activation like any other. And the network must be `ConsensusV2`:
    /// the lane exists to keep a PALW network's clock running when its bonded producers have
    /// stopped, and on a hash network there is nothing for it to rescue and `algo_id = 8` is
    /// simply an id nobody mines.
    ///
    /// One question, one answer, one call site per rule — replacing the triple gate
    /// (`id == 3 && ConsensusV2 && const`) that made a rule reachable by two of its three
    /// conditions.
    pub fn palw_heartbeat_lane_open_at(&self, daa_score: u64) -> bool {
        self.palw_heartbeat_lane_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// The heartbeat lane's fence **with the mode condition already folded in** — `Some` only on a
    /// `ConsensusV2` network that has armed it.
    ///
    /// This is what the processors store. They see one `Option<ForkActivation>` and ask it
    /// `is_active(daa_score)`, so the mode half cannot be forgotten at one of the five sites that
    /// need it — which is precisely how the old triple gate went wrong: `pre_ghostdag_validation`
    /// checked all three conditions, `utxo_validation` checked `palw_state_params_v2.is_some()`,
    /// and the body processor checked a bool named for the mode. Three spellings of one question.
    pub fn palw_heartbeat_lane_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_heartbeat) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(h)) => Some(h.activation),
            _ => None,
        }
    }

    /// **F3a's width bound with the mode condition folded in** (ADR-0068 Phase 1) — the same
    /// activation the lane fence answers, because the bound arms WITH the lane: a mergeset can
    /// only hold heartbeats once the lane exists, and a lane must never exist without its bound.
    /// The processors store this one `Option<ForkActivation>` and read the bound itself from
    /// [`crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET`], which `validate_palw_v2` proved
    /// equal to the fence's declared value.
    pub fn palw_heartbeat_width_fence(&self) -> Option<ForkActivation> {
        self.palw_heartbeat_lane_fence()
    }

    /// **ADR-0105's coloring fence with the mode AND the lane folded in** — `Some` only on a
    /// `ConsensusV2` network whose heartbeat lane is armed and which has armed this fence.
    ///
    /// The rule is about how a heartbeat counts in a bonded block's anticone, so without the lane
    /// there is no heartbeat for it to be about; folding the lane in means GHOSTDAG never reads a
    /// header for a rule that cannot apply. `validate_palw_v2` refuses the configuration outright,
    /// so on any node that starts this is `palw_heartbeat_transparent` itself.
    pub fn palw_heartbeat_transparent_fence(&self) -> Option<ForkActivation> {
        match (self.palw_heartbeat_lane_fence(), self.palw_heartbeat_transparent) {
            (Some(lane), Some(fence)) if lane != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
    /// **ADR-0105 §11's same-chain restriction with the mode, the lane AND the transparency fence
    /// folded in** — `Some` only where [`Self::palw_heartbeat_transparent_fence`] is. GHOSTDAG carries
    /// it inside `HeartbeatTransparency`, so the restriction is never read where the rule it narrows
    /// is not.
    pub fn palw_heartbeat_transparent_same_chain_fence(&self) -> Option<ForkActivation> {
        self.palw_heartbeat_transparent_fence().and(self.palw_heartbeat_transparent_same_chain)
    }
    // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----

    // ---- ADR-0160 lane escrow (post-launch, rcore/cap-escrow): the mirror and what F-E refuses ----

    /// **ADR-0160 F-E, resolved**: `Some` only on a ConsensusV2 network that arms it (and never for a
    /// `never()`).
    pub fn palw_capacity_escrow_at_licence_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_escrow_at_licence) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end ADR-0160 lane escrow ----

    /// **ADR-0107's share-growth rule, read in ONE place** — `Some` only on a `ConsensusV2`
    /// network that has armed it, for [`Self::palw_fp_da_pins_fence`]'s reason. The transition
    /// requires Final work for growth past this fence and reads accepted blocks before it.
    pub fn palw_share_growth_final_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_share_growth_final) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **P0a's fence, resolved**: `Some` only on a `ConsensusV2` network that armed it. The ONE place
    /// "may a class derive its GDN key-head count" is decided.
    pub fn palw_gdn_key_heads_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_gdn_key_heads) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **Is the attempt lane's constant work in force at `daa_score`?** (ADR-0066 Decision 3 /
    /// finding F2, closed by ADR-0068 Phase 1.)
    pub fn palw_attempt_work_open_at(&self, daa_score: u64) -> bool {
        self.palw_attempt_work_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// The attempt-work fence **with the mode condition already folded in** — `Some` only on a
    /// `ConsensusV2` network that has armed it, for the same one-question-one-answer reason as
    /// [`Self::palw_heartbeat_lane_fence`]: algo-6 exists only under `ConsensusV2`, so on any
    /// other network there is no block this rule could price.
    pub fn palw_attempt_work_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_attempt_work) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(w)) => Some(w.activation),
            _ => None,
        }
    }

    /// **Is ADR-0071's capability bound in force at `daa_score`?** (SA-1..SA-4.)
    pub fn palw_capability_bound_at(&self, daa_score: u64) -> bool {
        self.palw_capability_bound_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0134: whether the compute overlay's five subnetworks are refused at `daa_score`.
    pub fn palw_compute_overlay_retired_at(&self, daa_score: u64) -> bool {
        self.palw_compute_overlay_retired.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0135: whether the permissionless model registry governs classes at `daa_score`.
    pub fn palw_model_registry_at(&self, daa_score: u64) -> bool {
        self.palw_model_registry.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0137: whether the work target is in force at `daa_score`.
    pub fn palw_work_target_at(&self, daa_score: u64) -> bool {
        self.palw_work_target.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0143: whether an artifact root has one recorded owner at `daa_score`.
    pub fn palw_artifact_root_ownership_at(&self, daa_score: u64) -> bool {
        self.palw_artifact_root_ownership.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0145: whether a claim's work is derived rather than declared at `daa_score`. Callers on
    /// the accounting path resolve it at the CLAIM's `accepted_daa`, not at the block's — see the
    /// field's own doc.
    pub fn palw_canonical_work_at(&self, daa_score: u64) -> bool {
        self.palw_canonical_work.is_some_and(|f| f.is_active(daa_score))
    }

    /// **ADR-0145: the height the canonical-work basis comes into force, if it ever does.** The
    /// accounting path needs the HEIGHT rather than a yes/no at the block, because a claim must be
    /// priced under one basis at its `Final`, at its retirement and at every re-derivation in
    /// between. `None` — every shipped preset — is the declared basis, unchanged.
    pub fn palw_canonical_work_daa(&self) -> Option<u64> {
        self.palw_canonical_work.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// The 2026-09-19 audit: whether one operator identity backs one bond at `daa_score`.
    pub fn palw_operator_id_unique_at(&self, daa_score: u64) -> bool {
        self.palw_operator_id_unique.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0144 §9: whether a verified objective offence debits PalwBond at `daa_score`.
    pub fn palw_objective_offence_at(&self, daa_score: u64) -> bool {
        self.palw_objective_offence.is_some_and(|f| f.is_active(daa_score))
    }

    /// The HEIGHT of the objective-offence fence, if scheduled. `None` on every shipped preset.
    pub fn palw_objective_offence_daa(&self) -> Option<u64> {
        self.palw_objective_offence.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// ADR-0133 §7: whether the seat gate asks possession alone at `daa_score`.
    pub fn palw_seat_gate_possession_at(&self, daa_score: u64) -> bool {
        self.palw_seat_gate_possession.is_some_and(|f| f.is_active(daa_score))
    }

    /// The HEIGHT of the possession seat-gate fence, if scheduled. `None` on mainnet.
    pub fn palw_seat_gate_possession_daa(&self) -> Option<u64> {
        self.palw_seat_gate_possession.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// ADR-0133 §11.3: whether a class's receipt deadline is its own derived window at `daa_score`.
    pub fn palw_class_receipt_window_at(&self, daa_score: u64) -> bool {
        self.palw_class_receipt_window.is_some_and(|f| f.is_active(daa_score))
    }

    /// The HEIGHT of the class receipt-window fence, if scheduled. `None` on mainnet.
    pub fn palw_class_receipt_window_daa(&self) -> Option<u64> {
        self.palw_class_receipt_window.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// Whether a span's schedule mints spend-once execution quanta at `daa_score`.
    pub fn palw_execution_quanta_at(&self, daa_score: u64) -> bool {
        self.palw_execution_quanta.is_some_and(|f| f.is_active(daa_score))
    }

    /// The HEIGHT of the execution-quanta fence, if scheduled.
    pub fn palw_execution_quanta_daa(&self) -> Option<u64> {
        self.palw_execution_quanta.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// ADR-0147 (the 2026-09-19 audit's F3, ADR-0145 I3/I4): whether the block-level rules of
    /// independent admission apply at `daa_score` — a registration opens `Candidate`, leaves it
    /// only on a jury the network drew, asks for no share, and stays out of the work-price unit.
    pub fn palw_admission_independence_at(&self, daa_score: u64) -> bool {
        self.palw_admission_independence.is_some_and(|f| f.is_active(daa_score))
    }

    /// **ADR-0147: the HEIGHT of independent admission, if it is ever armed.** The claim-level rule
    /// — a bought class's claim sits an outsider drawn from the network and licenses only on its
    /// `Valid` — compares this against the CLAIM's own `accepted_daa`, never against a block, so a
    /// claim gets one answer at its draw, its binding and every licensing path. `None` — every
    /// shipped preset — is the draw and the licence exactly as they were.
    pub fn palw_admission_independence_daa(&self) -> Option<u64> {
        self.palw_admission_independence.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// ADR-0132 S: whether the single lottery is in force at `daa_score`.
    pub fn palw_single_lottery_at(&self, daa_score: u64) -> bool {
        self.palw_single_lottery.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0132 §7.6: whether the short challenge window is in force at `daa_score`.
    pub fn palw_short_challenge_window_at(&self, daa_score: u64) -> bool {
        self.palw_short_challenge_window.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0133 Verification V2: whether segment-scoped licensing is in force at `daa_score`.
    pub fn palw_verification_v2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_v2.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0133 S3: whether post-execution layer/position sampling is in force at `daa_score`.
    pub fn palw_verification_s3_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s3.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0133 S2: whether the full-replay seat's `Valid` alone may license at `daa_score`.
    pub fn palw_verification_s2_at(&self, daa_score: u64) -> bool {
        self.palw_verification_s2.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0133 §11.2: whether readiness V2 (the whole-artifact possession proof) is in force.
    pub fn palw_readiness_v2_at(&self, daa_score: u64) -> bool {
        self.palw_readiness_v2.is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0138: whether the DAA score counts only `bits`-priced blocks at `daa_score`.
    pub fn palw_anchor_clock_at(&self, daa_score: u64) -> bool {
        self.palw_anchor_clock.is_some_and(|f| f.is_active(daa_score))
    }

    /// **ADR-0132 S and ADR-0138 are one decision: the day the attempt lane leaves `bits` it leaves
    /// the DAA clock.** Arm or clear both here; `validate_palw_v2` refuses the pair at two heights
    /// and the clock without the lottery. (A preset literal may still spell them apart — the
    /// flag-day table and `validate` catch a literal that does.)
    pub fn set_palw_single_lottery(&mut self, at: Option<ForkActivation>) {
        self.palw_single_lottery = at;
        self.palw_anchor_clock = at;
    }

    // ---- lane V02 (post-launch, 2026-09-26): a resolved claim's lock is carried by the whole collateral ----

    /// **Lane V02's fence** ([`Self::palw_final_lock_full_collateral`]), `never()` read as absence.
    /// `None` on every shipped preset until the operator arms it.
    pub fn palw_final_lock_full_collateral_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_final_lock_full_collateral) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end lane V02 (option a) ----------------------------------------------------------------------

    // ---- lane V02 (post-launch, 2026-09-26): the post-Final honest lock life is shortened ----------

    /// **Lane V02's fence** ([`Self::palw_final_lock_life`]), `never()` read as absence, folded with
    /// the mode. `None` on every shipped preset until the operator arms it.
    pub fn palw_final_lock_life_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_final_lock_life) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end lane V02 -------------------------------------------------------------------------------

    // ---- lane F2-lock (post-launch, 2026-09-27): the F + 1,000 seat-lock life, applied retroactively ----

    /// **Lane F2-lock's fence** ([`Self::palw_final_lock_life_retro`]), `never()` read as absence,
    /// folded with the mode. `None` on every shipped preset until testnet-12's next flag day arms it.
    pub fn palw_final_lock_life_retro_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_final_lock_life_retro) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end lane F2-lock ---------------------------------------------------------------------------

    /// **ADR-0152 §4-ter (A-held): the held classes no honest party can dissect inside a turn** —
    /// [`crate::palw_state_v2::palw_held_unanswerable_classes_of_v1`] over the bundle's genesis rows
    /// where `palw_offence_attribution` is armed, empty everywhere else (every preset but testnet-12).
    pub fn palw_held_unanswerable_classes_v1(&self) -> Vec<crate::Hash64> {
        let armed = self.palw_offence_attribution.is_some_and(|f| f != ForkActivation::never());
        match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) if armed => {
                crate::palw_state_v2::palw_held_unanswerable_classes_of_v1(&bundle.genesis_objects)
            }
            _ => Vec::new(),
        }
    }

    /// **ADR-0152 §4-ter N4: are this node's backends held-aware?** — `true` exactly where
    /// [`Self::palw_held_unanswerable_classes_v1`] lists (a ConsensusV2 network that arms
    /// `palw_offence_attribution`), so a node's `supports_dissection` and the chain's mercy list read
    /// one predicate (`palw_held_class_unanswerable_v1`, a pure function of the class's profile: the
    /// context bound, a recurrent layer, a compute turn past the cap); `false` on every other network,
    /// whose backends are then byte for byte what they were. Handed to every backend a node resolves
    /// (`PalwExecutionBackendV1::set_held_answerability_v1`). Node-side only: nothing hashes it.
    pub fn palw_held_answerability_v1(&self) -> bool {
        self.palw_offence_attribution.is_some_and(|f| f != ForkActivation::never())
            && matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
    }

    /// **The maturity ADR-0151's fold serves and prices, in DAA** — `PalwEconomicSafetyFoldV1::
    /// maturity_daa`, filled by the processor from this and nothing else.
    ///
    /// The stated [`Self::palw_exec_quantum_maturity_daa`] where there is one (testnet-12: 120, the
    /// challenge window it applies — user decision 2026-09-25); else the `None` rule,
    /// [`crate::palw_economic_safety_v1::palw_exec_quantum_maturity_daa_v1`] over the bundle's lattice
    /// windows (1,200 on the RC windows); `0` on a network with no V2 bundle, where no fold reads it.
    /// Only read where `palw_economic_safety` is in force.
    pub fn palw_exec_quantum_maturity_v1(&self) -> u64 {
        if let Some(maturity) = self.palw_exec_quantum_maturity_daa {
            return maturity;
        }
        match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                crate::palw_economic_safety_v1::palw_exec_quantum_maturity_daa_v1(
                    bundle.state.window_challenge(),
                    bundle.state.window_court(),
                )
            }
            _ => 0,
        }
    }

    // ---- lane maturity (post-launch, 2026-09-26): ADR-0065 D1 from the post-launch fence ---------

    /// **Lane maturity's fence** ([`Self::palw_bond_maturity_early`]), resolved off a ConsensusV2
    /// ruleset; `never()` is dormant.
    pub fn palw_bond_maturity_early_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_bond_maturity_early) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }

    /// **ADR-0065 D1's window in force for a claim anchored at `anchor_daa`** — the DAA window alone,
    /// before the processor widens it by the second clock (`palw_bond_maturity_window_v2`). `None`
    /// where neither `palw_bond_maturity` nor lane maturity's fence is active at the anchor. What the
    /// processor's `palw_bond_maturity_at` answers, through the same free function.
    pub fn palw_bond_maturity_window_at(&self, anchor_daa: u64) -> Option<u64> {
        palw_bond_maturity_window_in_force_v1(self.palw_bond_maturity, self.palw_bond_maturity_early_fence(), anchor_daa)
    }

    /// **Lane maturity-ext (post-launch, 2026-09-26): ADR-0065 D1 on the model registry at `daa`** —
    /// what the processor's `palw_model_registry_fold_at` carries as
    /// `PalwModelRegistryFoldV1::bond_maturity`, through the same free function
    /// ([`crate::palw_model_registry_v1::palw_bond_maturity_fold_v1`]): `Some` from this fence on
    /// (its window `palw_bond_maturity`'s, the second clock's raw depth at `daa`), `None` below it and
    /// wherever it is dormant — every shipped preset. Unlike [`Self::palw_bond_maturity_window_at`]
    /// it is NOT in force from `palw_bond_maturity`'s own height alone: the release never applied D1
    /// to the ready count or the admission jury, so an unarmed testnet-12 keeps that registry past
    /// 1,000 byte for byte.
    pub fn palw_bond_maturity_registry_fold_at(&self, daa: u64) -> Option<crate::palw_model_registry_v1::PalwBondMaturityFoldV1> {
        let depth = if self.palw_audit_2026_09_23_active_at(daa) { self.palw_settled_anchor_depth } else { None };
        crate::palw_model_registry_v1::palw_bond_maturity_fold_v1(
            self.palw_bond_maturity,
            self.palw_bond_maturity_early_fence(),
            depth,
            daa,
        )
    }
    // ---- end lane maturity -------------------------------------------------------------------

    pub fn set_palw_short_challenge_window(&mut self, at: Option<ForkActivation>) {
        self.palw_short_challenge_window = at;
        let from_daa = at.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_short_challenge_window_from_daa(from_daa);
        }
    }

    pub fn set_palw_class_receipt_window(&mut self, at: Option<ForkActivation>) {
        self.palw_class_receipt_window = at;
        let from_daa = at.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            let span = self.palw_execution_lane.map_or(1, |lane| lane.schedule_span_daa.max(1));
            bundle.state = match from_daa {
                Some(daa) => bundle.state.clone().with_class_receipt_window(daa, span).expect("the span is floored at one"),
                None => bundle.state.clone().without_class_receipt_window(),
            };
        }
    }

    /// ADR-0132 Upgrade C: the economic payout where it can mean something — a `ConsensusV2`
    /// network (the claims it prices are V2 claims). What the processors store.
    pub fn palw_economic_payout_fence(&self) -> Option<PalwEconomicPayoutV1> {
        match (&self.palw_consensus_mode, self.palw_economic_payout) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(payout)) => Some(payout),
            _ => None,
        }
    }

    /// ADR-0132 Upgrade C: the payout in force at `daa_score`, or `None`.
    pub fn palw_economic_payout_at(&self, daa_score: u64) -> Option<PalwEconomicPayoutV1> {
        self.palw_economic_payout_fence().filter(|payout| payout.activation.is_active(daa_score))
    }

    /// The capability-bound fence **with the mode condition already folded in** — `Some` only on a
    /// `ConsensusV2` network that has armed it, for the same one-question-one-answer reason as
    /// [`Self::palw_heartbeat_lane_fence`]: `capable_classes`, the claim lane and the panel draw
    /// exist only under `ConsensusV2`, so on any other network there is nothing this rule bounds.
    pub fn palw_capability_bound_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capability_bound) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **Is ADR-0077 Decision 16's `PanelDa` in force at `daa_score`?** The extraction walk's
    /// question: a mode-2 commitment becomes a `FreePromptCommitted` claim only past this height.
    pub fn palw_panel_da_at(&self, daa_score: u64) -> bool {
        self.palw_panel_da_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// The `PanelDa` fence **with the mode condition already folded in** — `Some` only on a
    /// `ConsensusV2` network that has armed it, for the same one-question-one-answer reason as
    /// [`Self::palw_capability_bound_fence`]: the free-prompt lane exists only under
    /// `ConsensusV2`, so on any other network there is no commitment this rule could admit.
    pub fn palw_panel_da_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_panel_da) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **Does this build's ruleset carry `PanelDa` at all?** — transaction admission's question,
    /// and the only one it can ask: `validate_tx_in_isolation` is context-free by contract and
    /// holds no DAA score, so it cannot ask whether a height has passed.
    ///
    /// Height-free, and therefore strictly WEAKER than [`Self::palw_panel_da_at`] at every height
    /// — which is the direction the free-prompt door is required to fail in. A network that has
    /// scheduled the fence admits the mode-2 SHAPE into a block from the moment it ships that
    /// preset, and the claim it would become still waits for the height; a network that has not
    /// scheduled it refuses the shape at the door, exactly as every build does today.
    ///
    /// Reading `is_some()` rather than a height is the 0x30/0x31 token band's shape: "admitting
    /// the ids is part of the coordinated release... what the DAA fence governs is the *effect*".
    pub fn palw_panel_da_admissible(&self) -> bool {
        self.palw_panel_da_fence().is_some()
    }

    /// **Which form a job's `prompt_token_ids_hash` takes at `daa_score`** (ADR-0081 Decision 3).
    ///
    /// [`crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat`] everywhere this build ships, and
    /// the ONE place the answer is decided — a caller that read the fence and switched on it
    /// itself would be a second spelling of a consensus rule, which is how a court comes to
    /// recompute a commitment differently from the producer that made it.
    ///
    /// The mode condition is folded in for [`Self::palw_panel_da_fence`]'s reason: the free-prompt
    /// lane exists only under `ConsensusV2`, so on any other network there is no job whose prompt
    /// commitment this rule could re-form.
    /// **It has no production caller, and the fence it reads cannot be armed on this build**
    /// (audit D M-2). Every writer and every checker of a `prompt_token_ids_hash` spells the flat
    /// form — see `palw_fp_prompt_ids_admit_v1` — so this answers `Flat` on every reachable
    /// configuration, and `validate_palw_v2` refuses a ruleset that arms `palw_prompt_ids_merkle`
    /// rather than letting the two halves disagree. It stays because it is where the answer will
    /// be decided once the commitment moves with the fence, and because the admission gate
    /// already takes the form as an argument (`PalwKaryCourtV1::prompt_ids_form`).
    pub fn palw_prompt_ids_form_at(&self, daa_score: u64) -> crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        match self.palw_prompt_ids_merkle_fence() {
            Some(fence) if fence.is_active(daa_score) => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            _ => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        }
    }

    /// **The network's prompt-commitment form, height-free** — the one every writer and reader of
    /// a `prompt_token_ids_hash` takes (ADR-0081 Decision 3; private-prompts design, 2026-09-05).
    /// `validate_palw_v2` refuses `palw_prompt_ids_merkle` at any height but genesis, so the
    /// height-indexed [`Self::palw_prompt_ids_form_at`] answers the same at every height and this
    /// is that answer, spelled once: a decoder holds bytes, a tx door holds no DAA score, and a
    /// worker holds only the network it was started for, and all three must agree with the court.
    pub fn palw_prompt_ids_form_v1(&self) -> crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        self.palw_prompt_ids_form_at(0)
    }

    /// ADR-0084 U-08's fence with the mode condition folded in — `Some` only on a `ConsensusV2`
    /// network that has armed it. The ONE place the court's step ladder is decided: a court
    /// adjudication takes its `step_ladder` from `palw_court_step_ladder_at` and never from the
    /// raw field.
    pub fn palw_court_ladder_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_court_ladder) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **ADR-0072 Decision 8 on the free-prompt lane, read in ONE place** — `Some` only on a
    /// `ConsensusV2` network that has armed it, for [`Self::palw_court_ladder_fence`]'s reason. The
    /// transition derives the claim's retention obligation past this fence and takes the producer's
    /// number before it.
    pub fn palw_fp_da_pins_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_da_pins) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **ADR-0018 §E's payout bounds at `daa_score`**, mode folded in — the ONE place this is
    /// resolved, for [`Self::palw_fp_da_pins_fence`]'s reason and one that is sharper here: the
    /// coinbase CONSTRUCTION path and the VALIDATION path must get the same answer for the same
    /// block, or every node computes a different coinbase and the chain stops.
    pub fn palw_validator_payout_bounds_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_validator_payout_bounds) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **Does this build's ruleset carry the payout bounds at all?** — the coinbase isolation
    /// guard's question, and the only one it can ask: `check_coinbase_in_isolation` is context-free
    /// by contract and holds no DAA score.
    ///
    /// Height-free, and therefore strictly WIDER than the height-indexed answer at every height —
    /// which is the direction a SIZE guard must fail in, and the opposite of the free-prompt door's
    /// direction. A guard that is too wide declines to reject a coinbase the exact-hash rule then
    /// rejects anyway; one that is too narrow refuses a coinbase this node itself built, which is
    /// the 112-block halt this rule exists to prevent.
    pub fn palw_validator_payout_bounds_declared(&self) -> bool {
        self.palw_validator_payout_bounds_fence().is_some()
    }

    /// **MSK-26A: is the slashing-evidence UTXO genuineness rule armed at `daa_score`?** — past it
    /// the DNS slashing UTXO side-effect (`apply_slashing_side_effects`) removes an accused bond's
    /// stake only for evidence proved genuine (`proved_slash_targets`), closing the merge-blue
    /// forgery that burns an honest bond's stake. `None`/dormant on every shipped preset, so the
    /// side-effect is byte-identical to before the field existed. The construction and validation
    /// paths read this one function, so both compute one UTXO commitment for the same block.
    pub fn palw_slashing_evidence_utxo_genuine_at(&self, daa_score: u64) -> bool {
        self.palw_slashing_evidence_utxo_genuine.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// ADR-0087 Decision 6's fence with the mode condition folded in — `Some` only on a
    /// `ConsensusV2` network that has armed it. The ONE place the model market is decided.
    pub fn palw_model_market_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_model_market) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0088 Decision 11's fence with the mode condition folded in — `Some` only on a
    /// `ConsensusV2` network that has armed it. The ONE place the model registry is decided.
    pub fn palw_model_lines_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_model_lines) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    pub fn palw_class_semantics_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_class_semantics_v1) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }

    pub fn palw_court_commitment_scope_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_court_commitment_scope_v1) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }

    pub fn palw_staged_public_court_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_staged_public_court_v1) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }

    pub fn palw_model_immutable_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_model_immutable_v1) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0095's fence, with the mode condition and the registry dependency folded in. A
    /// membership over a registry that does not exist is meaningless, so this is `Some` only where
    /// the registry is armed too — the ONE place ADR-0095 is decided.
    pub fn palw_model_benefits_fence(&self) -> Option<ForkActivation> {
        match (self.palw_model_lines_fence(), self.palw_model_benefits) {
            (Some(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0114's fence with the market dependency folded in: a fee schedule for a market that does
    /// not exist is meaningless, so this is `Some` only where the market is armed too — the ONE
    /// place ADR-0114 is decided.
    pub fn palw_model_leg_v2_fence(&self) -> Option<ForkActivation> {
        match (self.palw_model_market_fence(), self.palw_model_leg_v2) {
            (Some(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// The fee schedule a move at `daa_score` is quoted under (ADR-0114).
    pub fn palw_model_fees_at(&self, daa_score: u64) -> crate::palw_model_market_v1::PalwModelFeesV1 {
        crate::palw_model_market_v1::PalwModelFeesV1::at(self.palw_model_leg_v2_active_at(daa_score))
    }

    /// ADR-0120's fence with the market dependency folded in: a floor for a market that does not
    /// exist is meaningless, so this is `Some` only where the market is armed too — the ONE place
    /// ADR-0120 is decided.
    pub fn palw_model_seed_v2_fence(&self) -> Option<ForkActivation> {
        match (self.palw_model_market_fence(), self.palw_model_seed_v2) {
            (Some(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// The least seed that opens a market at `daa_score` (ADR-0090, ADR-0120), in sompi — zero past
    /// ADR-0162's fence, where every line's market is open from its creation and a seed is optional.
    pub fn palw_model_seed_min_sompi_at(&self, daa_score: u64) -> u64 {
        crate::palw_model_market_v1::palw_model_seed_min_sompi_v2(
            self.palw_model_seed_v2_active_at(daa_score),
            self.palw_model_virtual_v1_active_at(daa_score),
        )
    }

    /// ADR-0162's fence with the market dependency folded in: a virtual reserve for a market that does
    /// not exist is meaningless, so this is `Some` only where the market is armed too — the ONE place
    /// ADR-0162 is decided. It takes precedence over `palw_model_seed_v2` wherever both are in force.
    pub fn palw_model_virtual_v1_fence(&self) -> Option<ForkActivation> {
        match (self.palw_model_market_fence(), self.palw_model_virtual_v1) {
            (Some(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }

    /// ADR-0162, as the fold reads it (`PalwTransitionExtrasV1::model_virtual_v1`): the height the
    /// fence came into force at, where it is in force at `daa_score`; `None` below it.
    pub fn palw_model_virtual_v1_from_at(&self, daa_score: u64) -> Option<u64> {
        self.palw_model_virtual_v1_fence().filter(|fence| fence.is_active(daa_score)).map(|fence| fence.daa_score())
    }

    /// ADR-0089 Decision 9's fence with the mode condition folded in. The ONE place the market's
    /// EVM face is decided.
    pub fn palw_model_evm_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_model_evm) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    // ---- lane cap-weight (ADR-0160 F-W): the per-bond weight cap (J-1) -------------------------

    /// **ADR-0160 F-W** ([`Self::palw_capacity_weight_cap`]), resolved off a ConsensusV2 ruleset with
    /// `never()` read as absence. `None` on every shipped preset until the capacity release arms it.
    pub fn palw_capacity_weight_cap_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_weight_cap) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end lane cap-weight -----------------------------------------------------------------

    // ---- lane sink (post-launch, 2026-09-26): a model sink is block-valid only bound ------------

    /// **Lane sink's fence** ([`Self::palw_model_sink_bound`]), resolved off a ConsensusV2 ruleset with
    /// `never()` read as absence. `None` on every shipped preset until the operator arms it.
    pub fn palw_model_sink_bound_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_model_sink_bound) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end lane sink ---------------------------------------------------------------------------

    // ---- ADR-0160 lane verify (capacity, post-launch): F-B the batch licence, F-R room v2 ----------

    /// **F-B's fence** ([`Self::palw_capacity_batch_licence`]), resolved off a ConsensusV2 ruleset with
    /// `never()` read as absence. `None` on every shipped preset.
    pub fn palw_capacity_batch_licence_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_batch_licence) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **F-R's fence** ([`Self::palw_capacity_verify_room`]), resolved like F-B's.
    pub fn palw_capacity_verify_room_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_verify_room) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **F-Q's fence** ([`Self::palw_capacity_audit_door`]), resolved like F-B's.
    pub fn palw_capacity_audit_door_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_audit_door) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **F-N's fence** ([`Self::palw_capacity_network_room`]), resolved like F-B's.
    pub fn palw_capacity_network_room_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_network_room) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **ADR-0164 F-M1: are riders accepted at `daa`?** (The ruleset's field, resolved off ConsensusV2 — what a producer's own decision
    /// turns on; the fold reads the bundle's mirror.)
    pub fn palw_capacity_riders_active_at_v1(&self, daa: u64) -> bool {
        matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
            && self.palw_capacity_multi_claim.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.is_active(daa))
    }

    /// **The verification term's fence** ([`Self::palw_capacity_network_verify`]), resolved like F-N's.
    pub fn palw_capacity_network_verify_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_network_verify) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **The part-C fence** ([`Self::palw_panel_unavailable_expiry`]), resolved like F-N's verification term.
    pub fn palw_panel_unavailable_expiry_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_panel_unavailable_expiry) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **The part-D fence** ([`Self::palw_panel_fast_switch`]), resolved like F-N's verification term.
    pub fn palw_panel_fast_switch_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_panel_fast_switch) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **The part-E fence** ([`Self::palw_seat_availability`]), resolved like F-N's verification term.
    pub fn palw_seat_availability_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_seat_availability) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **F-S's fence** ([`Self::palw_capacity_issuance_slots`]), resolved like F-B's.
    pub fn palw_capacity_issuance_slots_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_capacity_issuance_slots) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }
    // ---- end ADR-0160 lane verify -------------------------------------------------------------------

    /// ADR-0089: the three fences the executor reads, resolved at one DAA.
    pub fn palw_evm_market_fences_at(&self, daa_score: u64) -> crate::evm::model_market::PalwEvmMarketFencesV1 {
        crate::evm::model_market::PalwEvmMarketFencesV1 {
            market_active: self.palw_model_market_active_at(daa_score),
            lines_active: self.palw_model_lines_active_at(daa_score),
            evm_active: self.palw_model_evm_active_at(daa_score),
            leg_v2_active: self.palw_model_leg_v2_active_at(daa_score),
            seed_v2_active: self.palw_model_seed_v2_active_at(daa_score),
            audit_2026_09_23_active: self.palw_audit_2026_09_23_active_at(daa_score),
            virtual_v1_from: self.palw_model_virtual_v1_from_at(daa_score),
        }
    }

    /// ADR-0082 Decision 3's fence with the mode condition already folded in — `Some` only on a
    /// `ConsensusV2` network that has armed it. The ONE place the k-ary court is decided; a court
    /// or an admission gate switches on this and never on the raw field.
    pub fn palw_kary_court_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_kary_court) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// Is the fused terminal's responder-coverage rule in force at `daa_score`? `false` on every
    /// shipped preset. The ONE place it is decided; the fold and the sync walk both switch on this
    /// and never on the raw field.
    pub fn palw_court_responder_coverage_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_court_responder_coverage) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0082 Decisions 10/11's fence with the mode condition already folded in — `Some` only on
    /// a `ConsensusV2` network that has armed it. The ONE place the free-prompt lane's numerator
    /// and its sampler are decided; the transition, the admission gate and the gateway switch on
    /// this and never on the raw field.
    pub fn palw_fp_decode_rules_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_decode_rules) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **ADR-0145 §5/§6's fence with the mode condition already folded in** — `Some` only on a
    /// `ConsensusV2` network that has armed it. The ONE place derived free-prompt work is decided;
    /// the extraction walk and the transition switch on this and never on the raw field.
    ///
    /// The mode condition is not decoration: the derivation reads a class's graph out of chain
    /// state, and a network not running `ConsensusV2` has no class state to read.
    pub fn palw_fp_derived_work_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_derived_work) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0096 Decisions 6–8's fence with the mode condition already folded in — `Some` only on
    /// a `ConsensusV2` network that has armed it. The ONE place a decode constraint is decided;
    /// the transition, the court and the entrance switch on this and never on the raw field.
    pub fn palw_fp_decode_constraint_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_decode_constraint) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0099 Decision 5's fence with the mode condition already folded in — `Some` only on a
    /// `ConsensusV2` network that has armed it. The ONE place the shard court is decided.
    pub fn palw_shard_court_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_shard_court) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0100 Decision 4's fence, resolved: `Some` only on a `ConsensusV2` network that armed
    /// it. The ONE place per-shard licensing is decided.
    pub fn palw_shard_licensing_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_shard_licensing) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0124 Decisions 1–5: the panel economy's fence, meaningful only on a `ConsensusV2`
    /// network (a panel economy for a lane that does not exist is meaningless).
    pub fn palw_panel_economy_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_panel_economy) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0124 Decision 6: the work price's fence, meaningful only on a `ConsensusV2` network.
    pub fn palw_work_priced_reward_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_work_priced_reward) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0125: the execution lane with the mode folded in — a round lane beside a chain that has no
    /// finalized attempts to schedule it from is meaningless, so only `ConsensusV2` answers.
    pub fn palw_execution_lane_fence(&self) -> Option<PalwExecutionLaneV1> {
        match (&self.palw_consensus_mode, self.palw_execution_lane) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(lane)) => Some(lane),
            _ => None,
        }
    }

    /// The lane's shape where it is open at `daa_score`.
    pub fn palw_execution_lane_at(&self, daa_score: u64) -> Option<PalwExecutionLaneV1> {
        self.palw_execution_lane_fence().filter(|lane| lane.activation.is_active(daa_score))
    }

    /// ADR-0126: the overlay carve where it can mean something — a `ConsensusV2` network (the escrow
    /// it grows is a V2 claim's) that runs an overlay (the split it lowers is the overlay's).
    pub fn palw_overlay_carve_fence(&self) -> Option<PalwOverlayCarveV1> {
        match (&self.palw_consensus_mode, &self.dns_params, self.palw_overlay_carve) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(_), Some(carve)) => Some(carve),
            _ => None,
        }
    }

    /// RFC-0012: evaluated at the authoritative context (incumbent for fork choice,
    /// accepting block for transactions/rewards, canonical sink for service duties).
    pub fn palw_dns_retired_at(&self, daa_score: u64) -> bool {
        self.palw_dns_retirement.is_some_and(|r| r.activation.is_active(daa_score))
    }

    /// ADR-0130: the seat exposure floor, meaningful only on a `ConsensusV2` network (the seats it
    /// floors are a V2 panel's) — what the processors store.
    pub fn palw_panel_exposure_floor_fence(&self) -> Option<PalwPanelExposureFloorV1> {
        match (&self.palw_consensus_mode, self.palw_panel_exposure_floor) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(floor)) => Some(floor),
            _ => None,
        }
    }

    /// ADR-0130: the reward multiple in force at `daa_score`, in permille — `0` (no floor) where the
    /// floor is dormant or absent. The fourth argument of
    /// [`crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1`].
    pub fn palw_panel_reward_multiple_permille_at(&self, daa_score: u64) -> u32 {
        self.palw_panel_exposure_floor_fence()
            .filter(|floor| floor.activation.is_active(daa_score))
            .map_or(0, |floor| floor.reward_multiple_permille)
    }

    /// **The numbers the draw needs past the panel-economy fence**, from the bundle's own state
    /// params: ADR-0124's panel floor (ten producer floors) and the exposure ceiling every
    /// reservation shares, and ADR-0130's reward multiple where its own fence is in force at the same
    /// score (`0` while it is not). `None` where the economy is dormant at `daa_score` or no bundle
    /// exists.
    pub fn palw_seat_economy_at(&self, daa_score: u64) -> Option<crate::palw_panel_economy_v1::PalwSeatEconomyV1> {
        if !self.palw_panel_economy_active_at(daa_score) {
            return None;
        }
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return None };
        Some(crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
            panel_floor_sompi: crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(bundle.state.min_collateral_sompi()),
            max_exposure_ratio_permille: bundle.state.fp_max_exposure_ratio_permille(),
            // ADR-0130: the floor where it is in force at the same score.
            reward_multiple_permille: self.palw_panel_reward_multiple_permille_at(daa_score),
        })
    }

    /// ADR-0102's fence, resolved: `Some` only on a `ConsensusV2` network that armed it. The ONE
    /// place "may a class reach the per-token lift kernel" is decided.
    pub fn palw_token_lift_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_token_lift) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// Kimi K3 family fence, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_kimi_k3_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_kimi_k3) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    pub fn palw_kimi_k3_at(&self, daa_score: u64) -> bool {
        self.palw_kimi_k3_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// ADR-0093 Decision 6's fence, resolved: `Some` only on a `ConsensusV2` network that armed it.
    /// The ONE place "must a fused class be dissectable to be admitted" is decided.
    pub fn palw_fused_dissectable_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fused_dissectable) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0093 Decision 8's fence, resolved: `Some` only on a `ConsensusV2` network that armed it.
    /// The ONE place "must a root claim carry its anchor" is decided.
    pub fn palw_attn_anchored_root_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_attn_anchored_root) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0103's fence, resolved: `Some` only on a `ConsensusV2` network that armed it. The ONE
    /// place "is the context held off the chain here" is decided.
    pub fn palw_held_context_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_held_context) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// The audit fence, resolved off a ConsensusV2 ruleset (the only mode any audit fix runs on).
    pub fn palw_audit_2026_09_11_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_audit_2026_09_11) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// The audit DEEP fence (B-1/C-01/B-4/court cluster), resolved off a ConsensusV2 ruleset.
    pub fn palw_audit_2026_09_11_deep_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_audit_2026_09_11_deep) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// The 2026-09-23 economic audit's fence, resolved off a ConsensusV2 ruleset.
    pub fn palw_audit_2026_09_23_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_audit_2026_09_23) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0152 v2 F2's offence-attribution fence, resolved off a ConsensusV2 ruleset.
    pub fn palw_offence_attribution_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_offence_attribution) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0117's fence, resolved: `Some` only on a `ConsensusV2` network that armed it. The ONE
    /// place "is an attempt's job its class's canonical job without the decode calls" is decided.
    pub fn palw_prefill_draw_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_prefill_draw) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// ADR-0152 R-core+'s fence, resolved off a ConsensusV2 ruleset.
    pub fn palw_rcore_plus_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_rcore_plus) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    // ---- lane F1 (post-launch, 2026-09-25): the panel seed is the anchor attempt's execution ----

    /// **Lane F1's fence** ([`Self::palw_panel_seed_execution`]), resolved off a ConsensusV2 ruleset.
    pub fn palw_panel_seed_execution_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_panel_seed_execution) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }
    // ---- end lane F1 ------------------------------------------------------------------------

    // ---- lane A (post-launch, 2026-09-26): only an operator's attempt anchors a panel -----------

    /// **Lane A's rule** ([`Self::palw_operator_anchor`]) resolved off a ConsensusV2 ruleset: the
    /// fence and each operator bond's genesis-registered key — what the processor's anchor predicate
    /// reads. `None` where the fence is absent, `never()`, or the network is not ConsensusV2.
    pub fn palw_operator_anchor_rule_v1(&self) -> Option<crate::palw_operator_anchor_v1::PalwOperatorAnchorRuleV1> {
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return None };
        let value = self.palw_operator_anchor.as_ref().filter(|rule| rule.activation != ForkActivation::never())?;
        Some(value.rule_v1(&bundle.genesis_objects))
    }

    /// **Lane A armed at `activation` over every genesis bond** — testnet-12's post-launch value (its
    /// genesis registers exactly the operator's eight cards). `None` off ConsensusV2.
    pub fn palw_operator_anchor_of_genesis_bonds_v1(
        &self,
        activation: ForkActivation,
    ) -> Option<crate::palw_operator_anchor_v1::PalwOperatorAnchorV1> {
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return None };
        Some(crate::palw_operator_anchor_v1::PalwOperatorAnchorV1::of_genesis_bonds_v1(activation, &bundle.genesis_objects))
    }
    // ---- end lane A ------------------------------------------------------------------------

    // ---- ADR-0160 lane liab (F-L): aggregate bond liability — the mirror and what the fence refuses ----

    /// **F-L's step in force at `daa`** (ADR-0160 §4.5): `None` where the fence is absent, `never()`,
    /// not yet active at `daa`, or the network is not ConsensusV2 — the one accessor the escrow and
    /// shadow lanes read beside the fold's mirror (`PalwStateParamsV2::capacity_step_at`).
    pub fn palw_capacity_step_at_v1(&self, daa: u64) -> Option<crate::palw_aggregate_liability_v1::PalwCapacityStepV1> {
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return None;
        }
        self.palw_capacity_aggregate_liability
            .as_ref()
            .and_then(|value| crate::palw_aggregate_liability_v1::palw_capacity_step_at_v1(value, daa))
    }
    // ---- end ADR-0160 lane liab --------------------------------------------------------------

    /// ADR-0152 §4-quater's class-verify-deadline fence, resolved off a ConsensusV2 ruleset.
    pub fn palw_class_verify_deadline_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_class_verify_deadline) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    // ---- lane bind-deadlock (post-launch, 2026-09-26): an attempt at its ceiling still anchors ----

    /// **Lane bind-deadlock's fence** ([`Self::palw_anchor_at_ceiling`]), resolved off a ConsensusV2
    /// ruleset.
    pub fn palw_anchor_at_ceiling_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_anchor_at_ceiling) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }
    // ---- end lane bind-deadlock ----------------------------------------------------------------

    // ---- lane accept-order (post-launch, 2026-09-26): the mergeset is applied parents-first ----

    /// **Lane accept-order's fence** ([`Self::palw_lane_accept_parents_first`]), resolved off a
    /// ConsensusV2 ruleset with the execution lane configured — the one place it is decided; the
    /// virtual processor and the body processor read this and never the raw field.
    pub fn palw_lane_accept_parents_first_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_execution_lane, self.palw_lane_accept_parents_first) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(_), Some(f)) if f != ForkActivation::never() => Some(f),
            _ => None,
        }
    }
    // ---- end lane accept-order -----------------------------------------------------------------

    /// **ADR-0152-adjacent: the Activation Pool's fence** (R1, R2 and the pool), resolved off a
    /// ConsensusV2 ruleset — the ONE place it is decided; the fold's extras, the transaction
    /// validator's sink rule and the mempool read this and never the raw field.
    pub fn palw_activation_pool_fence(&self) -> Option<PalwActivationPoolParamsV1> {
        match (&self.palw_consensus_mode, self.palw_activation_pool) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(pool)) if pool.activation != ForkActivation::never() => {
                Some(pool)
            }
            _ => None,
        }
    }

    /// The pool's terms where R1, R2 and the pool are in force at `daa_score`
    /// (`PalwTransitionExtrasV1::activation_pool`), else `None` — every preset but testnet-12.
    pub fn palw_activation_pool_at(&self, daa_score: u64) -> Option<crate::palw_activation_pool_v1::PalwActivationPoolTermsV1> {
        self.palw_activation_pool_fence().filter(|pool| pool.activation.is_active(daa_score)).map(|pool| pool.terms)
    }

    /// **The readiness-V2 horizon's fence** (user decision 2026-09-25, readiness capacity option
    /// (a)), resolved off a ConsensusV2 ruleset — `Some` on testnet-12 alone.
    pub fn palw_readiness_v2_max_age_spans_fence(&self) -> Option<PalwReadinessV2MaxAgeParamsV1> {
        match (&self.palw_consensus_mode, self.palw_readiness_v2_max_age_spans) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(horizon))
                if horizon.activation != ForkActivation::never() =>
            {
                Some(horizon)
            }
            _ => None,
        }
    }

    /// **How long a readiness-V2 row stands on this network, in spans**: the fence's horizon where it
    /// is armed (genesis-only, so at every DAA), else the default eight
    /// ([`crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1`]). The fold reads the
    /// bundle's mirror (`PalwStateParamsV2::readiness_v2_max_age_spans_v1`), which `validate_palw_v2`
    /// holds equal to this.
    pub fn palw_readiness_v2_max_age_spans_v1(&self) -> u32 {
        self.palw_readiness_v2_max_age_spans_fence()
            .map(|horizon| horizon.max_age_spans)
            .unwrap_or(crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1)
    }

    /// ADR-0081 Decision 3's fence with the mode condition already folded in — `Some` only on a
    /// `ConsensusV2` network that has armed it.
    pub fn palw_prompt_ids_merkle_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_prompt_ids_merkle) {
            (crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_), Some(f)) => Some(f),
            _ => None,
        }
    }

    /// **Is ADR-0072's execution-priced attempt lane in force at `daa_score`?**
    ///
    /// `None` — every shipped preset — means "un-fenced", and an un-fenced network runs the
    /// execution-priced rule at every height (that is what testnet-11 and devnet do today, having
    /// been re-minted onto it). Only an ARMED network has a height at which the answer is `false`,
    /// and below that height its attempt lane is the pre-ADR-0072 one. See the field's doc.
    pub fn palw_attempt_execution_priced_at(&self, daa_score: u64) -> bool {
        match self.palw_attempt_activation {
            None => true,
            Some(fence) => fence.is_active(daa_score),
        }
    }

    /// **Which attempt lane is open at `daa_score`** (ADR-0072 SA-3/SA-4) — the ONE place the
    /// fence is turned into the pair of facts that decide an attempt header's admissibility: which
    /// algo id carries the lane, and which envelope version it admits.
    ///
    /// One resolver rather than two lookups, because the defect §3's Decision 7 analysis found is
    /// precisely the two drifting apart: an activation that opened a new id while leaving the
    /// version check compiled in refuses the history it is asked to sync.
    pub fn palw_attempt_lane_at(&self, daa_score: u64) -> crate::pow_layer0::PalwAttemptLaneV1 {
        crate::pow_layer0::PalwAttemptLaneV1::from_fence(self.palw_attempt_activation.map(|fence| fence.is_active(daa_score)))
    }

    /// ADR-0128: the BFT gate, where the network runs an overlay at all — a vote of no validators
    /// decides nothing. What the processors store.
    pub fn dns_bft_gate_fence(&self) -> Option<DnsBftGateV1> {
        self.dns_params.as_ref().and(self.dns_bft_gate)
    }

    /// ADR-0128: the BFT gate where it is in force at `daa_score` — the sink's, when a sink's
    /// confirmation is evaluated, and the incumbent sink's, when the reorg gate is.
    pub fn dns_bft_gate_at(&self, daa_score: u64) -> Option<DnsBftGateV1> {
        self.dns_bft_gate_fence().filter(|gate| gate.activation.is_active(daa_score))
    }

    /// **Every scheduled fence height this build carries, sorted and deduped** (ADR-0072 SA-2).
    ///
    /// Derived from [`Self::for_each_fence`] and nothing else, so it can never drift from the
    /// identity and schedule ids — and so a fence added to `Params` tomorrow joins the fork id the
    /// moment it is classified there, without a second exhaustive list to forget.
    ///
    /// Two heights are deliberately excluded, and neither exclusion loses a rule:
    ///
    /// * `0` — a fence active at genesis is not a schedule, it is a rule about block 1, and
    ///   [`Self::consensus_identity_id`] already refuses a peer that disagrees about one (re-audit
    ///   R-1). Including it here would say two builds "cross" it at some height, which is false.
    /// * `u64::MAX` — `never()`, and the `None` sentinel `for_each_fence` writes for an absent
    ///   Some-only fence. Neither is a height any chain reaches.
    ///
    /// What is left is exactly the set of heights at which THIS build changes the rules, which is
    /// what a peer needs to be able to say it has or has not crossed.
    pub fn fence_schedule_v1(&self) -> Vec<u64> {
        let mut scores: Vec<u64> = Vec::new();
        // `for_each_fence` needs `&mut self` because the identity visitor writes through it; this
        // visitor only reads, so the clone comes back unchanged and is dropped.
        let mut probe = self.clone();
        probe.for_each_fence(&mut |score| scores.push(*score));
        scores.retain(|&score| score != 0 && score != u64::MAX);
        scores.sort_unstable();
        scores.dedup();
        scores
    }

    /// A fingerprint of the consensus rules this node runs, for the P2P handshake.
    ///
    /// Two nodes that answer the same network name but disagree here cannot reach consensus — this
    /// struct's own contract says so — and nothing else stops them from peering, syncing from each
    /// other and forking. testnet-22 forked exactly this way: an older build computing different
    /// overlay commitments, indistinguishable at handshake from a correct one.
    ///
    /// Two properties, both needed, and easy to get only one of:
    ///
    /// **Canonical.** The encoding below is fixed, versioned and domain-separated. An earlier cut
    /// hashed `format!("{self:?}")`, which is not an encoding at all: `Debug` output can change
    /// with a field rename, a library update, or a formatting change in some nested type, and two
    /// nodes running identical rules would then refuse to peer. Worse in the other direction, it
    /// silently covers whatever `Debug` happens to print and nothing else.
    ///
    /// **Complete.** The destructuring below is exhaustive — no `..` — so adding a field to
    /// `Params` fails to compile until somebody decides whether it belongs here. That is the point:
    /// the failure mode of a hand-maintained list is two nodes that believe they agree and do not,
    /// and a compile error is a much better way to find that out than a fork.
    ///
    /// Fields deliberately excluded are named with a reason at the destructure. Everything else is
    /// written in declaration order, integers little-endian, with lengths where a value is
    /// variable, so no two distinct parameter sets can collide by concatenation.
    ///
    /// Not an identifier to persist, publish, or compare across versions — a value for separating
    /// nodes at handshake, and nothing more.
    /// **ADR-0150: what each fenced ruleset's gate says on THIS network** — absent, scheduled for a
    /// height above genesis, or in force now. The identity hashes the revisions of the rulesets in
    /// force and leaves the scheduled ones out, exactly as it does with their heights, so a fleet
    /// can still roll a build out before the height its new rule takes effect at.
    pub fn palw_rule_manifest_gates_v1(&self) -> crate::palw_rule_manifest_v1::PalwRuleManifestGatesV1 {
        use crate::palw_rule_manifest_v1::{PalwGateStateV1, PalwRuleManifestGatesV1};
        let state = |fence: Option<ForkActivation>| match fence {
            None => PalwGateStateV1::Absent,
            Some(activation) if activation == ForkActivation::never() => PalwGateStateV1::Absent,
            Some(activation) if activation.daa_score() == 0 => PalwGateStateV1::InForce,
            Some(_) => PalwGateStateV1::Scheduled,
        };
        PalwRuleManifestGatesV1 {
            readiness_v2: state(self.palw_readiness_v2),
            verification_v2: state(self.palw_verification_v2),
            model_registry: state(self.palw_model_registry),
            economic_payout: state(self.palw_economic_payout.map(|payout| payout.activation)),
            work_target: state(self.palw_work_target),
            canonical_work: state(self.palw_canonical_work),
            admission_independence: state(self.palw_admission_independence),
            fp_derived_work: state(self.palw_fp_derived_work),
            anchor_clock: state(self.palw_anchor_clock),
            da_court: state(self.palw_da_court),
        }
    }

    /// ADR-0038 Decision A: whether a PALW header at `daa_score` must carry its commitment.
    ///
    /// `false` on every shipped network, which is the pre-ADR rule: the field must be empty. The
    /// answer is a property of the NETWORK and the header's own DAA, never of the node, so every
    /// consumer of `check_palw_commitment_shape` routes through here rather than deciding.
    #[inline]
    #[must_use]
    pub fn is_palw_commitment_bound(&self, daa_score: u64) -> bool {
        self.palw_block_commitment.is_some_and(|fence| fence.is_bound(daa_score))
    }
    /// Returns the past median time sample rate
    #[inline]
    #[must_use]
    pub fn past_median_time_sample_rate(&self) -> u64 {
        self.blockrate.past_median_time_sample_rate
    }

    /// Returns the difficulty sample rate
    #[inline]
    #[must_use]
    pub fn difficulty_sample_rate(&self) -> u64 {
        self.blockrate.difficulty_sample_rate
    }

    /// Returns the target time per block
    #[inline]
    #[must_use]
    pub fn target_time_per_block(&self) -> u64 {
        self.blockrate.target_time_per_block
    }

    /// Returns the expected number of blocks per second, **floored at 1**.
    ///
    /// Every remaining consumer uses this as a sizing/bounds heuristic (relay-flow counts, log
    /// chunk limits, orphan-pool ranges, an IBD warn threshold, a gRPC throughput hint), where 1
    /// is the correct degenerate value on a sub-1-bps network — the un-floored integer division
    /// is 0 at 0.1 bps (10_000 ms blocks) and 0-sized bounds panic (`chunks_timeout`). Exact
    /// rate arithmetic must use [`Params::target_time_per_block_history`] instead (as the
    /// coinbase emission schedule does).
    #[inline]
    #[must_use]
    pub fn bps(&self) -> u64 {
        (1000 / self.blockrate.target_time_per_block).max(1)
    }

    /// Returns the expected number of blocks per second throughout history (currently represented as [`ForkedParam`]).
    /// Required permanently in order to calculate the subsidy month from the current DAA score.
    #[inline]
    #[must_use]
    pub fn bps_history(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            (1000 / self.pre_crescendo_target_time_per_block).max(1),
            (1000 / self.blockrate.target_time_per_block).max(1),
            self.crescendo_activation,
        )
    }

    /// Target time per block throughout history, in milliseconds. The sub-integer-bps-safe
    /// counterpart of [`Params::bps_history`]: at 0.1 bps (`target_time_per_block = 10_000`)
    /// integer `bps` truncates to 0, so every rate-scaled computation (notably the coinbase
    /// emission schedule) consumes THIS and scales by `ttpb / 1000` instead of dividing by bps.
    pub fn target_time_per_block_history(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.pre_crescendo_target_time_per_block, self.blockrate.target_time_per_block, self.crescendo_activation)
    }

    pub fn ghostdag_k(&self) -> KType {
        self.blockrate.ghostdag_k
    }

    pub fn max_block_parents(&self) -> u8 {
        self.blockrate.max_block_parents
    }

    pub fn mergeset_size_limit(&self) -> u64 {
        self.blockrate.mergeset_size_limit
    }

    pub fn merge_depth(&self) -> u64 {
        self.blockrate.merge_depth
    }

    pub fn finality_depth(&self) -> u64 {
        self.blockrate.finality_depth
    }

    pub fn pruning_depth(&self) -> u64 {
        self.blockrate.pruning_depth
    }

    pub fn coinbase_maturity(&self) -> u64 {
        self.blockrate.coinbase_maturity
    }

    /// ADR-0018 coinbase settlement's long-maturity fallback, `0` on networks without the
    /// feature. **A coinbase spend clears TWO gates** — the classic maturity floor above and
    /// this one — and a spender that asks only the floor offers the node money it will refuse
    /// (on testnet-11 the floor is 1 and this is 600, so that was every coinbase younger than
    /// 600 DAA). See [`Self::coinbase_spend_maturity`] for the number a spend selector wants.
    pub fn coinbase_settlement_long_maturity_daa(&self) -> u64 {
        self.dns_params.as_ref().map_or(0, |d| d.coinbase_settlement_long_maturity_daa)
    }

    /// **The age at which a coinbase is spendable without an anchor** — the larger of the two
    /// gates. A selector using this is conservative by construction: a confirmed DNS anchor can
    /// legalise a younger coinbase, but not consulting the anchor only delays a candidate, never
    /// selects an illegal one. Issue #81 is what asking the floor alone does: the validator kept
    /// choosing a 427-DAA-old coinbase ("mature" by the floor of 1) while the node refused it for
    /// 600, and attestation stopped for exactly the remaining maturity window.
    ///
    /// **It is the mempool's whole rule, bar the DNS-final acceleration** (the user's Mainnet
    /// Decision A, 2026-09-24): a coinbase's spendability is DAA-based maturity only. The
    /// settled-anchor second clock the 2026-09-23 heartbeat audit had put on the mempool's coinbase
    /// policy is gone, so a validator or a wallet that selects by this number no longer offers the
    /// node a coinbase it holds back for anchors (the sweep's finding: the validator looked at the
    /// 600 DAA and nothing else, while the mempool waited on anchors too).
    pub fn coinbase_spend_maturity(&self) -> u64 {
        self.coinbase_maturity().max(self.coinbase_settlement_long_maturity_daa())
    }

    pub fn finality_duration_in_milliseconds(&self) -> u64 {
        self.blockrate.target_time_per_block * self.blockrate.finality_depth
    }

    pub fn difficulty_window_duration_in_block_units(&self) -> u64 {
        self.blockrate.difficulty_sample_rate * self.difficulty_window_size as u64
    }

    pub fn expected_difficulty_window_duration_in_milliseconds(&self) -> u64 {
        self.blockrate.target_time_per_block * self.blockrate.difficulty_sample_rate * self.difficulty_window_size as u64
    }

    /// Returns the depth at which the anticone of a chain block is final (i.e., is a permanently closed set).
    /// Based on the analysis at <https://github.com/kaspanet/docs/blob/main/Reference/prunality/Prunality.pdf>
    /// and on the decomposition of merge depth (rule R-I therein) from finality depth (φ)
    pub fn anticone_finalization_depth(&self) -> u64 {
        let anticone_finalization_depth = self.blockrate.finality_depth
            + self.blockrate.merge_depth
            + 4 * self.blockrate.mergeset_size_limit * self.blockrate.ghostdag_k as u64
            + 2 * self.blockrate.ghostdag_k as u64
            + 2;

        // In mainnet it's guaranteed that `self.pruning_depth` is greater
        // than `anticone_finalization_depth`, but for some tests we use
        // a smaller (unsafe) pruning depth, so we return the minimum of
        // the two to avoid a situation where a block can be pruned and
        // not finalized.
        min(self.blockrate.pruning_depth, anticone_finalization_depth)
    }

    pub fn network_name(&self) -> String {
        self.net.to_prefixed()
    }

    pub fn prefix(&self) -> Prefix {
        self.net.into()
    }

    pub fn default_p2p_port(&self) -> u16 {
        self.net.default_p2p_port()
    }

    pub fn default_rpc_port(&self) -> u16 {
        self.net.default_rpc_port()
    }

    pub fn override_params(self, overrides: OverrideParams) -> Self {
        Self {
            dns_seeders: self.dns_seeders,
            net: self.net,
            genesis: self.genesis.clone(),

            timestamp_deviation_tolerance: overrides.timestamp_deviation_tolerance.unwrap_or(self.timestamp_deviation_tolerance),

            max_difficulty_target: self.max_difficulty_target,
            max_difficulty_target_f64: self.max_difficulty_target_f64,

            difficulty_window_size: overrides.difficulty_window_size.unwrap_or(self.difficulty_window_size),
            past_median_time_window_size: overrides.past_median_time_window_size.unwrap_or(self.past_median_time_window_size),
            min_difficulty_window_size: overrides.min_difficulty_window_size.unwrap_or(self.min_difficulty_window_size),

            coinbase_payload_script_public_key_max_len: overrides
                .coinbase_payload_script_public_key_max_len
                .unwrap_or(self.coinbase_payload_script_public_key_max_len),

            max_coinbase_payload_len: overrides.max_coinbase_payload_len.unwrap_or(self.max_coinbase_payload_len),

            max_tx_inputs: overrides.max_tx_inputs.unwrap_or(self.max_tx_inputs),
            max_tx_outputs: overrides.max_tx_outputs.unwrap_or(self.max_tx_outputs),
            max_signature_script_len: overrides.max_signature_script_len.unwrap_or(self.max_signature_script_len),
            max_script_public_key_len: overrides.max_script_public_key_len.unwrap_or(self.max_script_public_key_len),
            mass_per_tx_byte: overrides.mass_per_tx_byte.unwrap_or(self.mass_per_tx_byte),
            mass_per_script_pub_key_byte: overrides.mass_per_script_pub_key_byte.unwrap_or(self.mass_per_script_pub_key_byte),
            mass_per_sig_op: overrides.mass_per_sig_op.unwrap_or(self.mass_per_sig_op),
            max_block_mass: overrides.max_block_mass.unwrap_or(self.max_block_mass),

            storage_mass_parameter: overrides.storage_mass_parameter.unwrap_or(self.storage_mass_parameter),

            deflationary_phase_daa_score: overrides.deflationary_phase_daa_score.unwrap_or(self.deflationary_phase_daa_score),

            pre_deflationary_phase_base_subsidy: overrides
                .pre_deflationary_phase_base_subsidy
                .unwrap_or(self.pre_deflationary_phase_base_subsidy),

            skip_proof_of_work: overrides.skip_proof_of_work.unwrap_or(self.skip_proof_of_work),

            max_block_level: overrides.max_block_level.unwrap_or(self.max_block_level),

            pruning_proof_m: overrides.pruning_proof_m.unwrap_or(self.pruning_proof_m),

            blockrate: overrides.blockrate.clone().unwrap_or(self.blockrate.clone()),

            pre_crescendo_target_time_per_block: overrides
                .pre_crescendo_target_time_per_block
                .unwrap_or(self.pre_crescendo_target_time_per_block),

            crescendo_activation: overrides.crescendo_activation.unwrap_or(self.crescendo_activation),

            // kaspa-pq DNS overlay params are not CLI-overridable; carried as-is.
            dns_params: self.dns_params,
            palw_credit: self.palw_credit,
            palw_fork_choice: self.palw_fork_choice,
            palw_schedule: self.palw_schedule,
            palw_ramp: self.palw_ramp,
            palw_block_commitment: self.palw_block_commitment,
            palw_bootstrap_activation: self.palw_bootstrap_activation,
            palw_unavailable_abstains: self.palw_unavailable_abstains,
            palw_bond_maturity: self.palw_bond_maturity,
            // Lane maturity (post-launch, 2026-09-26): CARRIED beside `palw_bond_maturity`, whose window
            // it reads — an override that dropped the prerequisite then fails `validate_palw_v2` by name.
            palw_bond_maturity_early: self.palw_bond_maturity_early,
            palw_frontier_provenance: self.palw_frontier_provenance,
            palw_reorg_strict_economic_win: self.palw_reorg_strict_economic_win, // lane: rcore/f1-forkchoice-attacks
            palw_pruning_proof_strict_economic_win: self.palw_pruning_proof_strict_economic_win, // lane: rcore/hf-pptake2
            palw_capacity_weight_cap: self.palw_capacity_weight_cap,             // lane: rcore/cap-weight (ADR-0160 F-W)
            palw_heartbeat: self.palw_heartbeat,
            palw_attempt_work: self.palw_attempt_work,
            palw_attempt_activation: self.palw_attempt_activation,
            dns_bft_gate: self.dns_bft_gate,
            palw_inactivity_leak: self.palw_inactivity_leak,
            palw_beacon_fold: self.palw_beacon_fold,
            palw_capability_bound: self.palw_capability_bound,
            palw_compute_overlay_retired: self.palw_compute_overlay_retired,
            palw_model_registry: self.palw_model_registry,
            palw_economic_payout: self.palw_economic_payout,
            palw_work_target: self.palw_work_target,
            palw_single_lottery: self.palw_single_lottery,
            palw_short_challenge_window: self.palw_short_challenge_window,
            palw_verification_v2: self.palw_verification_v2,
            palw_verification_s3: self.palw_verification_s3,
            palw_verification_s2: self.palw_verification_s2,
            palw_readiness_v2: self.palw_readiness_v2,
            palw_anchor_clock: self.palw_anchor_clock,
            palw_clock_cursor: None,
            // Rides with the cursor it refines: an override that drops the cursor drops the floor,
            // or `validate_palw_v2` would refuse the result.
            palw_clock_floor: None,
            // The lead cap: CARRIED — it has no prerequisite an override could drop, and an
            // overridden testnet-12 keeps the bound its 1,620 s tolerance needs.
            palw_clock_lead_cap: self.palw_clock_lead_cap,
            // Rides with `palw_economic_safety`, which an override drops below: armed without it,
            // `validate_palw_v2` would refuse the result.
            palw_offence_attribution: None,
            // ADR-0152 R-core+ (S-SPEC §5): CARRIED, like the audit fence, never reset — an
            // overridden testnet-12 then fails `validate_palw_v2` on the prerequisites the override
            // dropped, instead of silently disarming R-core+.
            palw_rcore_plus: self.palw_rcore_plus,
            // Lane F1 (the panel seed, post-launch): CARRIED beside its one prerequisite, which is
            // carried too.
            palw_panel_seed_execution: self.palw_panel_seed_execution,
            // Lane A (the operator anchor, post-launch): CARRIED beside its one prerequisite.
            palw_operator_anchor: self.palw_operator_anchor.clone(),
            // ADR-0160 lane liab (F-L): CARRIED beside its two prerequisites, which are carried too.
            palw_capacity_aggregate_liability: self.palw_capacity_aggregate_liability.clone(),
            palw_rcore_conservative_classes: self.palw_rcore_conservative_classes,
            // Lane V02 (post-launch): CARRIED with R-core+, whose ledger it splits, and with the bundle
            // whose mirror it matches.
            palw_final_lock_full_collateral: self.palw_final_lock_full_collateral,
            // Lane V02 (post-launch): CARRIED with R-core+, whose vesting rows cover E while a lock is
            // released early, and with the bundle whose mirror it matches.
            palw_final_lock_life: self.palw_final_lock_life,
            // Lane F2-lock (post-launch): CARRIED with lane V02's lock life, its one prerequisite, and with
            // the bundle whose mirror it matches.
            palw_final_lock_life_retro: self.palw_final_lock_life_retro,
            // ADR-0152 §4-quater: CARRIED like R-core+, never reset — an overridden testnet-12 then fails
            // `validate_palw_v2` on a prerequisite the override dropped instead of silently reopening
            // the 2M row.
            palw_class_verify_deadline: self.palw_class_verify_deadline,
            palw_class_verify_rows: self.palw_class_verify_rows,
            // Lane bind-deadlock (post-launch): CARRIED beside its one prerequisite, which is carried too.
            palw_anchor_at_ceiling: self.palw_anchor_at_ceiling,
            // Lane accept-order (post-launch): CARRIED beside the execution lane it orders, which is carried too.
            palw_lane_accept_parents_first: self.palw_lane_accept_parents_first,
            // ADR-0152-adjacent (Activation Pool): CARRIED for R-core+'s reason — an overridden
            // testnet-12 fails `validate_palw_v2` on the prerequisite the override dropped
            // (`palw_admission_independence`) instead of silently disarming R1, R2 and the pool.
            palw_activation_pool: self.palw_activation_pool,
            // The readiness-V2 horizon (user decision 2026-09-25): CARRIED for the pool's reason — an
            // overridden testnet-12 keeps the horizon its rows are counted by, and fails
            // `validate_palw_v2` on a prerequisite the override dropped rather than silently
            // counting them by eight spans.
            palw_readiness_v2_max_age_spans: self.palw_readiness_v2_max_age_spans,
            // Lane F1: CARRIED with the bundle whose mirror it matches.
            palw_registry_resilience: self.palw_registry_resilience,
            // Lane F2 (post-launch): CARRIED with the bundle whose mirror it matches, beside its one
            // prerequisite (R-core+), which is carried too.
            palw_floor_refusal_retry: self.palw_floor_refusal_retry,
            palw_artifact_root_ownership: None,
            palw_operator_id_unique: None,
            palw_objective_offence: self.palw_objective_offence,
            palw_seat_gate_possession: self.palw_seat_gate_possession,
            palw_class_receipt_window: self.palw_class_receipt_window,
            palw_execution_quanta: self.palw_execution_quanta,
            palw_public_model_source_required: None,
            palw_economic_safety: None,
            palw_canonical_work: None,
            palw_admission_independence: None,
            palw_context_ladder: self.palw_context_ladder,
            palw_panel_da: self.palw_panel_da,
            palw_certification_rent: self.palw_certification_rent,
            palw_uncertified_weightless: self.palw_uncertified_weightless,
            palw_da_court: self.palw_da_court,
            palw_court_ladder: self.palw_court_ladder,
            palw_model_market: self.palw_model_market,
            palw_model_lines: self.palw_model_lines,
            palw_model_immutable_v1: self.palw_model_immutable_v1,
            palw_class_semantics_v1: self.palw_class_semantics_v1,
            palw_court_commitment_scope_v1: self.palw_court_commitment_scope_v1,
            palw_staged_public_court_v1: self.palw_staged_public_court_v1,
            palw_model_benefits: self.palw_model_benefits,
            palw_model_leg_v2: self.palw_model_leg_v2,
            palw_model_seed_v2: self.palw_model_seed_v2,
            palw_model_virtual_v1: self.palw_model_virtual_v1,
            palw_model_evm: self.palw_model_evm,
            // Lane sink (the model sink binding, post-launch): CARRIED with the market it binds and
            // the audit fence it needs, both carried too.
            palw_model_sink_bound: self.palw_model_sink_bound,
            // ADR-0160 lane verify (F-B, F-R): CARRIED with the R-core+, audit and attribution fences
            // they need (carried too); the bundle's mirrors ride the carried bundle.
            palw_capacity_batch_licence: self.palw_capacity_batch_licence,
            palw_capacity_verify_room: self.palw_capacity_verify_room,
            palw_capacity_audit_door: self.palw_capacity_audit_door,
            palw_capacity_issuance_slots: self.palw_capacity_issuance_slots,
            palw_capacity_network_room: self.palw_capacity_network_room,
            palw_capacity_network_verify: self.palw_capacity_network_verify,
            palw_panel_unavailable_expiry: self.palw_panel_unavailable_expiry,
            palw_panel_fast_switch: self.palw_panel_fast_switch,
            palw_seat_availability: self.palw_seat_availability,
            palw_capacity_emission_budget: self.palw_capacity_emission_budget,
            palw_capacity_multi_claim: self.palw_capacity_multi_claim,
            palw_capacity_rho_breaker: self.palw_capacity_rho_breaker,
            palw_chunk_cap_charge: self.palw_chunk_cap_charge,
            palw_prompt_ids_merkle: self.palw_prompt_ids_merkle,
            palw_kary_court: self.palw_kary_court,
            palw_model_court_window: self.palw_model_court_window,
            palw_court_responder_coverage: self.palw_court_responder_coverage,
            palw_fp_decode_rules: self.palw_fp_decode_rules,
            palw_fp_derived_work: self.palw_fp_derived_work,
            palw_fp_decode_constraint: self.palw_fp_decode_constraint,
            palw_shard_court: self.palw_shard_court,
            palw_shard_licensing: self.palw_shard_licensing,
            palw_token_lift: self.palw_token_lift,
            palw_kimi_k3: self.palw_kimi_k3,
            palw_tir_v1: self.palw_tir_v1,
            palw_gen_v1: self.palw_gen_v1,
            palw_fp_job_v5: self.palw_fp_job_v5,
            palw_gen_range_twin_v1: self.palw_gen_range_twin_v1,
            palw_task_heads_v1: self.palw_task_heads_v1,
            palw_probabilistic_constraints_v1: self.palw_probabilistic_constraints_v1,
            palw_fork_choice_rule_e_v1: self.palw_fork_choice_rule_e_v1,
            palw_typed_roots_v1: self.palw_typed_roots_v1,
            palw_fork_choice_commitment_v1: self.palw_fork_choice_commitment_v1,
            palw_verifier_pay_v1: self.palw_verifier_pay_v1,
            palw_model_refutation_seal_v1: self.palw_model_refutation_seal_v1,
            palw_beacon_source_opt_in_v1: self.palw_beacon_source_opt_in_v1,
            palw_beacon_source_fee_v1: self.palw_beacon_source_fee_v1,
            palw_panel_free_v1: self.palw_panel_free_v1.clone(),
            palw_signed_registration_v1: self.palw_signed_registration_v1,
            palw_tir_shard_segment_v2: self.palw_tir_shard_segment_v2,
            palw_provider_court_v1: self.palw_provider_court_v1,
            palw_legacy_held_da_v2: self.palw_legacy_held_da_v2,
            palw_bond_budget_v1: self.palw_bond_budget_v1.clone(),
            palw_reporter_income_v2: self.palw_reporter_income_v2,
            palw_material_income_v3: self.palw_material_income_v3,
            palw_panel_lifetime_v2: self.palw_panel_lifetime_v2,
            palw_claim_challenge_v2: self.palw_claim_challenge_v2,
            palw_monetary_material_v1: self.palw_monetary_material_v1.clone(),
            palw_model_verification_gate_v1: self.palw_model_verification_gate_v1,
            palw_horizon_extension_v1: self.palw_horizon_extension_v1,
            palw_minimum_price_v1: self.palw_minimum_price_v1.clone(),
            palw_beacon_bootstrap_v1: self.palw_beacon_bootstrap_v1,
            palw_paid_consumer_lifecycle_v1: self.palw_paid_consumer_lifecycle_v1.clone(),
            palw_model_bond_allocation_v1: self.palw_model_bond_allocation_v1.clone(),
            palw_reporter_share_v2: self.palw_reporter_share_v2,
            palw_legacy_public_filer_v1: self.palw_legacy_public_filer_v1,
            palw_court_model_scope_v1: self.palw_court_model_scope_v1,
            palw_public_evidence_reward_v1: self.palw_public_evidence_reward_v1,
            palw_claim_hard_deadline_v1: self.palw_claim_hard_deadline_v1,
            palw_fp_prefix_inherit: self.palw_fp_prefix_inherit,
            palw_fp_prefix_state: self.palw_fp_prefix_state,
            palw_fp_tokenizer_match: self.palw_fp_tokenizer_match,
            palw_fp_constraint_v2: self.palw_fp_constraint_v2,
            palw_adapter_class_v1: self.palw_adapter_class_v1,
            palw_tir_fence2: self.palw_tir_fence2,
            palw_tir_shard_v1: self.palw_tir_shard_v1,
            palw_floor_reserve_v1: self.palw_floor_reserve_v1,
            palw_real_clock_tick_v1: self.palw_real_clock_tick_v1,
            palw_anchor_window_v1: self.palw_anchor_window_v1,
            palw_receipt_spend_v4: self.palw_receipt_spend_v4,
            palw_evidence_court_v1: self.palw_evidence_court_v1,
            palw_audit_1004_v1: self.palw_audit_1004_v1,
            palw_improvement_v1: self.palw_improvement_v1,
            palw_held_close_chunks_v1: self.palw_held_close_chunks_v1,
            palw_verification_vertex_v1: self.palw_verification_vertex_v1,
            palw_exec_payload_v2: self.palw_exec_payload_v2,
            palw_witness_manifest_v1: self.palw_witness_manifest_v1,
            palw_audit_mesh_v1: self.palw_audit_mesh_v1,
            palw_capped_onboarding_v1: self.palw_capped_onboarding_v1,
            palw_class_seating: self.palw_class_seating,
            palw_tir_only_v1: self.palw_tir_only_v1,
            palw_fused_dissectable: self.palw_fused_dissectable,
            palw_attn_anchored_root: self.palw_attn_anchored_root,
            palw_held_context: self.palw_held_context,
            palw_prefill_draw: self.palw_prefill_draw,
            palw_audit_2026_09_11: self.palw_audit_2026_09_11,
            palw_audit_2026_09_11_deep: self.palw_audit_2026_09_11_deep,
            palw_audit_2026_09_23: self.palw_audit_2026_09_23,
            palw_settled_anchor_depth: self.palw_settled_anchor_depth,
            palw_admission_audit_period_daa: self.palw_admission_audit_period_daa,
            // Rides with `palw_economic_safety`, which an override drops below: stated without it,
            // `validate_palw_v2` would refuse the result.
            palw_exec_quantum_maturity_daa: None,
            palw_difficulty_priced_rows: self.palw_difficulty_priced_rows,
            palw_receipt_rows_unpriced: self.palw_receipt_rows_unpriced,
            palw_attempt_header_pins: self.palw_attempt_header_pins,
            palw_signature_contexts_v2: self.palw_signature_contexts_v2,
            palw_fp_da_pins: self.palw_fp_da_pins,
            palw_validator_payout_bounds: self.palw_validator_payout_bounds,
            palw_slashing_evidence_utxo_genuine: self.palw_slashing_evidence_utxo_genuine,
            palw_epoch_boundary_budget: self.palw_epoch_boundary_budget,
            palw_epoch_budget_release: self.palw_epoch_budget_release,
            palw_panel_economy: self.palw_panel_economy,
            palw_work_priced_reward: self.palw_work_priced_reward,
            palw_execution_lane: self.palw_execution_lane,
            palw_overlay_carve: self.palw_overlay_carve,
            palw_dns_retirement: self.palw_dns_retirement,
            palw_permissionless_panel_v1: self.palw_permissionless_panel_v1,
            palw_permissionless_panel_complete_v1: self.palw_permissionless_panel_complete_v1,
            palw_permissionless_panel_retired_v1: self.palw_permissionless_panel_retired_v1,
            palw_panel_exposure_floor: self.palw_panel_exposure_floor,
            palw_fp_ruleset_caps: self.palw_fp_ruleset_caps,
            palw_heartbeat_transparent: self.palw_heartbeat_transparent,
            // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency)
            palw_heartbeat_transparent_same_chain: self.palw_heartbeat_transparent_same_chain,
            // ADR-0160 lane escrow (post-launch)
            palw_capacity_escrow_at_licence: self.palw_capacity_escrow_at_licence,
            palw_share_growth_final: self.palw_share_growth_final,
            // P0a: CARRIED — a class registered past it must stay admissible on an overridden ruleset.
            palw_gdn_key_heads: self.palw_gdn_key_heads,
            palw_consensus_mode: self.palw_consensus_mode.clone(),
            // kaspa-pq PoW algo activation is consensus-fixed, never runtime-overridable.
            pow_blake2b_sha3_activation: self.pow_blake2b_sha3_activation,
            pow_palw_activation: self.pow_palw_activation,
            pow_palw_ollama_activation: self.pow_palw_ollama_activation,
            // kaspa-pq: PQ enforcement is consensus-fixed, never runtime-overridable.
            pq_enforcement: self.pq_enforcement,
            pq_activation_daa_score: self.pq_activation_daa_score,
            // kaspa-pq EVM lane activation is consensus-fixed, never runtime-overridable.
            evm_activation_daa_score: self.evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score: self.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: self.evm_f002_withdraw_cap_activation_daa_score,
            evm_bridge_ledger_activation_daa_score: self.evm_bridge_ledger_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score: self.evm_f003_mldsa_verify_activation_daa_score,
            // §12 Phase-7: consensus-fixed (the receipts-root encoding is consensus), never overridable.
            evm_typed_receipt_root_activation_daa_score: self.evm_typed_receipt_root_activation_daa_score,
        }
    }
}

impl Deref for Params {
    type Target = BlockrateParams;

    fn deref(&self) -> &Self::Target {
        &self.blockrate
    }
}

impl DerefMut for Params {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.blockrate
    }
}

/// Install the registered compute profiles into a preset that has SCHEDULED its VLT shadow fence
/// (ADR-0024 step 3).
///
/// The table cannot live in the `const` preset: its entries are keyed BLAKE2b digests of the
/// pinned artifact strings, which no `const fn` can produce. It therefore has to be attached
/// where the preset is materialized — and it has to be attached HERE, at the `From` impls every
/// consumer passes through, rather than in one binary's argument parsing. A node built on
/// `kaspad`'s CLI is not the only thing that reads a preset: simpa, the integration harnesses and
/// any embedder read them too, and a scheduled fence over an empty table is a coordinated hard
/// fork in which every job normalizes to zero VLT — the one mistake
/// `shipped_presets_are_either_dormant_or_fully_forkable` exists to make impossible.
///
/// A dormant preset (`u64::MAX`) is left untouched, so every shipped network is byte-identical to
/// before this function existed.
fn with_registered_models(mut params: Params) -> Params {
    if let Some(dns) = params.dns_params.as_mut()
        && dns.vlt.vlt_shadow_activation_daa_score != u64::MAX
        && dns.vlt.model_cost_table.len == 0
    {
        dns.vlt.model_cost_table = crate::vlt::ModelCostTable::palw_metal_registered();
    }
    params
}

impl From<NetworkType> for Params {
    fn from(value: NetworkType) -> Self {
        with_registered_models(match value {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => TESTNET_PARAMS,
            // The BUNDLED form (ADR-0068 Phase 1 drill network), so the two `From` impls cannot
            // disagree about what devnet is — `DEVNET_PARAMS` alone is the bundle-free base.
            NetworkType::Devnet => devnet_shipped_params(),
            NetworkType::Simnet => SIMNET_PARAMS,
        })
    }
}

impl From<NetworkId> for Params {
    fn from(value: NetworkId) -> Self {
        with_registered_models(match value.network_type {
            NetworkType::Mainnet => mainnet_shipped_params(),
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET_PARAMS,
                // **The PALW-RC network** (was suffix 12; see `palw_rc_base_params`). The old
                // `TESTNET11_PARAMS` was the algo-4 `LegacyTn11` lane, which was never published
                // and is not running anywhere — kept below as a constant so a node asked for it by
                // name still gets a coherent answer rather than a panic, but no suffix routes here.
                Some(11) => palw_rc_shipped_params(),
                // The PALW-RC network (ADR-0036 Decision 2 / ADR-0042 PR-10). Its bundle-free
                // BASE identity: the bundle is a function of genesis artifacts no `From<NetworkId>`
                // holds, so it is installed on top (`palw_rc_params`) and its absence is visible
                // in `consensus_params_id` rather than silent. Before this arm existed, naming the
                // suffix `palw_rc_params` itself stamps aborted the process.
                // 12 is retired into 11. Left as an explicit refusal rather than a silent
                // alias: a node still configured for 12 should be told, not quietly moved.
                // **testnet-12 — the 2026-09-22 regenesis.** It was retired into 11 (and this arm
                // was an explicit refusal) because 12 and 13 had been minted for internal relaunches
                // nobody could join; it is a network again because this time it replaces 11 on the
                // public fleet. See `palw_t12_shipped_params`.
                Some(12) => palw_t12_shipped_params(),
                Some(x) => panic!("Testnet suffix {} is not supported (this build knows 10, 11 and 12)", x),
                None => panic!("Testnet suffix not provided"),
            },
            // The ADR-0068 drill ruleset — see `devnet_shipped_params`.
            NetworkType::Devnet => devnet_shipped_params(),
            NetworkType::Simnet => SIMNET_PARAMS,
        })
    }
}

/// kaspa-pq overlay activation — shared by ALL FOUR networks (user decision 2026-06-01).
///
/// The DNS-finality PoS overlay (ADR-0009/0017/0018) is **genesis-active everywhere**:
/// `dns_activation_daa_score: 0` so the two-stage confirmation model (PoW WorkScore +
/// validator StakeScore, each cleared against its `required_*_depth` threshold) is live
/// from block 0, and `full_reward_split_daa_score: 0` so the Stage-3 reward split applies
/// immediately. The rollout still advances Bootstrap→Active only once a real bond exists
/// (`min_active_validators: 1`, `min_active_stake_sompi: 0`), so an unbonded chain runs on
/// pure PoW/GHOSTDAG and Active (with the reorg gate) engages the moment a validator bonds
/// + attests. `reorg_mode: TwoDimensionalDominance` (ADR-0009/0018 §H mainnet spec, applied
/// to all nets per user request 2026-06-01): once an anchor is DNS-confirmed, a candidate
/// that exits the confirmed prefix is accepted ONLY if it **strictly beats** the canonical
/// chain on BOTH accumulated `WorkScore` AND `StakeScore` since their common ancestor, each
/// by its emergency margin (`emergency_work_margin` / `emergency_stake_margin`) — the
/// "non-substitutability" rule: a PoW-only surplus cannot buy past a PoS deficit and vice
/// versa. This replaces the prior PoC `HardCheckpoint` (which rejected ANY confirmed-prefix
/// exit — a loud testing convenience, not real DNS finality). NOTE: `dns_params` is NOT a
/// genesis-block input (genesis.rs never reads it), so every net stays `Some(..)` with the
/// genesis hashes unchanged.
pub const GENESIS_ACTIVE_DNS_PARAMS: DnsParams = DnsParams {
    dns_activation_daa_score: 0,
    min_active_stake_sompi: 0,
    min_active_validators: 1,
    // devnet/simnet: no per-bond minimum (any positive bond is accepted).
    min_bond_amount_sompi: 0,
    epoch_length_blocks: 100,
    required_work_depth: BlueWorkType::ZERO,
    required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
    // ADR-0018 §H two-dimensional dominance margins. A deep reorg that abandons a
    // DNS-confirmed anchor must out-Work the canonical chain by > emergency_work_margin
    // AND out-Stake it by > emergency_stake_margin (non-substitutability). The work margin
    // is a fixed ~2-blocks-of-devnet-work buffer (1_000_000; one BlueWorkType u64 limb);
    // on higher-difficulty nets it is a proportionally tighter — but always strict —
    // positive buffer. The stake margin is 1× the required_stake_depth unit.
    // BlueWorkType is a type alias for Uint576 (9 little-endian u64 limbs); construct via the
    // real struct name (the alias is not a tuple-struct ctor). Low limb = 1_000_000.
    emergency_work_margin: Uint576([1_000_000, 0, 0, 0, 0, 0, 0, 0, 0]),
    // CALIBRATION (incident 2026-07-19 §2-4 — was `100 * STAKE_SCORE_SCALE` = 1e11). StakeScore is
    // WINDOWED, not cumulative: its ceiling is `stake_score_window_blue_score /
    // attestation_epoch_length_blue_score` epochs × SCALE = 15 × 1e9 = 1.5e10 here. A margin of
    // 1e11 therefore sat ABOVE the attainable maximum, so `DominanceSatisfied` was unreachable on
    // any network state and TwoDimensionalDominance silently degenerated into HardCheckpoint —
    // i.e. the documented escape valve did not exist. 3 epochs of full participation keeps a real
    // margin while leaving 12 epochs of headroom inside the window. Enforced by
    // `presets_emergency_stake_margin_is_attainable`.
    emergency_stake_margin: StakeScore(3 * STAKE_SCORE_SCALE),
    max_reorg_horizon_blocks: 300,
    evidence_window_blocks: 300,
    unbonding_period_blocks: 700, // > max_reorg_horizon + evidence_window
    max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
    max_attestation_shard_mass: 50_200,
    reward_uniqueness_window_blocks: 600,
    stake_event_quality_floor_bps: 6000,
    degraded_stake_quality_epochs: 4,
    stake_censorship_floor_bps: 1000,
    reward_params: RewardParams {
        per_attestation_reward_sompi: 100_000_000,
        slashing_reporter_reward_bps: 1000,
        max_validator_inflation_per_block_sompi: 100_000_000 * MAX_ATTESTATIONS_PER_SHARD as u64,
        // ADR-0018 "本格版" (PoS-v2): 70/30 participation/quality split. INERT until
        // `pos_v2_activation_daa_score` — below the v2 fence the reward path forces the full pool
        // into participation (effective bps 10_000), so this is byte-identical on every net today.
        validator_participation_bps: 7000,
        validator_quality_bonus_bps: 3000,
        quality_gate_bonus_sompi: 0,
        worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
        fee_split: FeeSplitParams {
            // kaspa-pq: validator subsidy share raised 25% → 30% (re-genesis 同便).
            // worker_base absorbs the 5pt; inclusion 8% kept. miner stays majority
            // (70% worker = 62% base + 8% inclusion), validator 30%. Strengthens the
            // stake-finality incentive (2-D DNS reorg defense) without inflating supply.
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        },
        fee_split_bootstrap: FeeSplitParams {
            subsidy_worker_base_bps: 8200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 1000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        },
        // ADR-0018 "本格版" (PoS-v2) 4-way slashing split: reporter 10% (slashing_reporter_reward_bps)
        // / reserve 40% / victim 40% / burn 10%. INERT until pos_v2_activation — the slashing path
        // forces reserve/victim to 0 below the fence, degenerating to the byte-identical pre-v2 2-way
        // (reporter + burn). Calibratable economic defaults. The reserve **drip** (Phase 4) releases
        // at most `reserve_drip_per_epoch_cap_sompi` from the security reserve into the participation
        // pool per BLOCK past `palw_validator_payout_bounds`, and per finalized EPOCH below it. The
        // field name still says "per_epoch" and the unit past the fence is the block:
        // `at_two_minute_cadence` set `epoch_length_blocks` to 2 without moving this number, so a
        // block that merges 180 crosses ~90 thresholds and released ~90 caps (mainnet audit,
        // 2026-09-06, M-1). Renaming the field is fingerprint-neutral (borsh encodes positions, not
        // names) but it is a rename of an economic parameter and is left to the operator. All inert
        // via the v2 fence.
        security_reserve_bps: 4000,
        victim_epoch_pool_bps: 4000,
        reserve_drip_per_epoch_cap_sompi: 1000 * SOMPI_PER_KASPA,
    },
    reorg_mode: DnsReorgMode::TwoDimensionalDominance,
    full_reward_split_daa_score: 0,
    // PoS-v2 "本格版" economics master fence — dormant on devnet/simnet (this
    // GENESIS_ACTIVE preset); mainnet/testnet activate it from block 0 (PRODUCTION). No re-genesis.
    pos_v2_activation_daa_score: u64::MAX,
    // kaspa-pq DNS v3 (Canonical Lagged Anchor): blue_score-coordinated attestation epochs.
    // devnet/simnet use small windows for fast finality in tests. blue_score ≈ height at low DAG
    // parallelism, so these mirror the legacy epoch_length_blocks=100 cadence. Calibratable.
    attestation_epoch_length_blue_score: 100,
    attestation_lag_blue_score: 40,
    attestation_anchor_backoff_blue_score: 10,
    stake_score_window_blue_score: 1500,
    // ADR-0018 §F bridge wiring: deposit-lock txs' fees are finality-class (validator-primary
    // split) from genesis — doubly gated on the net's EVM activation, so it is LIVE on devnet
    // (EVM-active) and enforced-inert on simnet (EVM u64::MAX ⇒ identical splits even if a
    // lock output appears). NOT a genesis-block input.
    finality_fee_activation_daa_score: 0,
    // kaspa-pq bond spend-gate mergeset hardening: GENESIS-ACTIVE here (2026-08-11 audit P0).
    //
    // The own-body gate it replaces only ever saw a block's own transactions, so a spend of a
    // non-releasable bond's locked output-0 riding in a MERGE-BLUE block was accepted — the
    // collateral could be withdrawn while the bond still read `Active`, which is precisely the
    // backing `W_i(E) = min{C_i(E), λ·B_i(E)}` and every slashing rule assume exists. The
    // acceptance-time SKIP (`BondSpendFilter`) has been implemented and dormant behind this
    // fence; on a network that starts with it there is no history to fork, so it starts at 0
    // and the legacy own-body gate stands down (see `verify_expected_utxo_state`).
    bond_spend_gate_mergeset_activation_daa_score: 0,
    // kaspa-pq liveness-first DNS finality: attestation participation feeds StakeScore, rewards,
    // and health, but shipped networks do not make insufficient attestation stake a base-ledger
    // validity failure. Private/research networks can lower this fence when explicitly testing the
    // hard-inclusion anti-censorship rule.
    mandatory_attestation_inclusion_daa_score: u64::MAX,
    // Local finality-dependent producer/RPC policy: pause bridge/EVM payload production when the
    // DNS-confirmed anchor is older than this DAA distance. Not used for block validation.
    bridge_finality_max_staleness_daa_score: DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE,
    // Partition-liveness override (incident 2026-08-03 §8): release the stake veto for a candidate
    // that out-Works canonical since the common ancestor by >4x — i.e. only for an adversary
    // holding >80% of hashpower for the whole fork. Guarantees a partitioned minority branch
    // rejoins the work-dominant chain instead of wedging forever, which a strictly absolute stake
    // veto cannot do (no node can locally tell that it is the minority side of a partition).
    emergency_work_override_multiplier: 4,
    // Escape-from-a-dead-branch sink preference (incident #8 family): ON at the ½-work bound on
    // every test network so the soak and the regression harness exercise it. See the field doc.
    stake_preference_max_work_deficit_multiplier: 2,
    // DNS-accelerated coinbase settlement: OFF on dev/sim so the existing fixtures (which mine
    // and spend without attestation flow) keep their semantics; dedicated fixtures opt in with
    // custom DnsParams. See the field doc for the phase discipline.
    coinbase_settlement_long_maturity_daa: 0,
    // Consensus enforcement fence: NEVER in this build — the per-chain-block anchor fold is not
    // wired; the fence exists so the fold-carrying build announces itself via the fingerprint.
    coinbase_settlement_consensus_activation_daa_score: u64::MAX,
    // Unbond-authorization mergeset hardening (incident 2026-08-07): GENESIS-ACTIVE on every
    // preset, so a new network never has to pick — and remember — a per-net activation score.
    //
    // Residual on testnet-10 only: its chain already carries one unauthorized request (DAA
    // 28_059_617) whose stamp is persisted in the two operator nodes' bond stores. The bond store
    // is incremental derived state (ADR-0009 A.4), so those nodes keep the stamp while a node
    // synced from genesis under this rule will not derive it — a one-bond (1M of 24M MSK)
    // difference in the active-stake denominator. Both sides clear every threshold by a wide
    // margin, and the divergence is erased for good the first time the two nodes are resynced.
    // Genesis-active was chosen deliberately over a fence: the fence only papers over that single
    // historical stamp, at the cost of every future net inheriting a magic number.
    unbond_authz_mergeset_activation_daa_score: 0,
    // ADR-0060 Decision 4: leak OFF on devnet/simnet — harnesses and drills assume a frozen
    // validator set, and a drill that idles past a leak boundary should not watch its quorum
    // denominator move under it.
    inactivity_leak_daa: u64::MAX,
    // MISAKA Verified LLM Token-Weighted BFT: dormant. Devnet/simnet keep bonded-stake weight, so
    // the existing fast-finality test fixtures are unaffected. See `vlt::VltParams::INERT`.
    vlt: VltParams::INERT,
    // MISAKA Compute Token Program (design v0.1 §10): inert everywhere — the TOK ledger and
    // emission do not exist until a per-network hard fork moves these fences (and freezes the
    // TBD R0/H schedule numbers the design deliberately leaves open).
    tkn: TokenParams::INERT,
    vlt_credit_window_blue_score: 0,
    // Veto reach + release, devnet/simnet flavour. `0` ⇒ the gate horizon tracks
    // `max_reorg_horizon_blocks`, which the DAG fixtures tune directly (several of them raise it so
    // a from-genesis fork stays gate-eligible), and `u64::MAX` ⇒ no TTL. Both keep the strict
    // 2-D rule exactly as the simulation tests assert it — notably the 51%-PoW-attack test, whose
    // whole point is that a stake-less heavier branch is refused for as long as it is presented.
    // The production presets below carry the calibrated values; a dev net that wants to exercise
    // them sets them explicitly.
    dns_gate_horizon_blocks: 0,
    dns_veto_ttl_daa_score: u64::MAX,
    min_anchor_attesters: 1,
};

/// Number of blocks in 14 days at the production 10 BPS block rate
/// (`14 d × 86_400 s/d × 10 blk/s`). Used for the unbonding window and the
/// equivocation-evidence window so a withdrawing validator stays slashable for
/// the whole 14-day exit.
pub const FOURTEEN_DAYS_BLOCKS_10BPS: u64 = 14 * 86_400 * 10; // 12_096_000

/// kaspa-pq production (mainnet + testnet) DNS-finality overlay params. Differs from the
/// shared [`GENESIS_ACTIVE_DNS_PARAMS`] (used by devnet/simnet) in the economically
/// load-bearing knobs:
///   * `min_active_stake_sompi = 120,000,000 MSK` (6 validators x a 20M MSK bond, user decision
///     2026-09-25) — the network does not reach the `Active` rollout stage until that much stake
///     is bonded by at least that many validators.
///   * `unbonding_period_blocks = 14 days` (+ the reorg horizon, to keep the ADR-0009
///     §"Long-range bound" invariant `U ≥ R + E`). A withdrawal request only releases the
///     locked stake after this window; the stake stays slashable the entire time.
///   * `evidence_window_blocks = 14 days` — equivocation evidence remains acceptable for the
///     full unbonding window, so a validator that double-signs and then immediately requests
///     unbond can still be slashed at any point before the stake is released (the user's
///     "slash during the unbonding period" requirement).
/// Genesis-active (`dns_activation_daa_score: 0`) and `TwoDimensionalDominance` like devnet;
/// `dns_params` is NOT a genesis-block input, so adopting this leaves genesis hashes unchanged.
pub const PRODUCTION_DNS_PARAMS: DnsParams = DnsParams {
    dns_activation_daa_score: 0,
    // Production: the overlay reaches the Active stage once this much stake is bonded.
    //
    // **20M -> 600M (2026-08-30), then -> 120,000 with the bond floor below, then -> 120,000,000
    // (2026-09-25).** The two gates are ANDed, so the stake gate must state their product or it is
    // a dead constant reading as a second, weaker floor. It moves whenever either half does, which
    // `the_active_stake_gate_is_the_product_of_the_two_floors` holds: 6 x 20,000,000 MSK.
    min_active_stake_sompi: 6 * 20_000_000 * SOMPI_PER_KASPA,
    // audit H-11 (Kaspa-diff): the DNS Active stage must NOT be drivable by a single key. A
    // multi-operator floor is the mainnet default so finality does not hinge on one operator's
    // key/availability/honesty (the safety floor is BOTH the `min_active_stake_sompi` AND this
    // validator COUNT). (Testnet pins this back to 1 in TESTNET_DNS_PARAMS for the
    // single-operator experimental mesh.)
    //
    // **3 -> 12 (2026-08-30 bond-economics pass).** Priced at the market rate the token actually
    // trades at, the count is the term that carries this floor and the stake is not. Corrupting a
    // 2/3 quorum costs `ceil(2n/3) x min_bond`, and at 3 seats that is two bonds — 100M MSK,
    // which is 0.8 % of the supply's value. The same 50M bond over 12 seats needs eight, 400M
    // MSK, 3.1 %: a 4x improvement bought with no extra capital per validator, only more of them.
    // Raising the bond alone cannot reach the same place — 4x the bond over 3 seats prices out
    // every operator who is not the treasury, and concentration is the risk this floor exists to
    // prevent. 24 is the next step once that many distinct operators are actually seated; going
    // there before they exist would stall the overlay at `Bootstrapping` instead of securing it.
    //
    // **12 -> 6 (user decision 2026-09-25: testnet-12 runs mainnet's numbers, and mainnet's are the
    // ones the user set on 2026-09-24 — "DNS validator は最低 6 台・最低 20M MSK").** With the bond
    // at 20M below, corrupting a 2/3 quorum is `ceil(2·6/3) = 4` bonds = 80M MSK, 1,000x the
    // 80,000 MSK the 12 x 10,000 set priced it at.
    min_active_validators: 6,
    // Production: every individual validator must bond >= 20,000,000 MSK; a smaller StakeBond is
    // rejected at acceptance and can never attest.
    //
    // **50M -> 10,000 MSK (2026-08-30, operator decision): ONE floor for what it costs to be a
    // bonded participant on this network.** A PALW genesis seat posts 10,000 MSK (ADR-0061) and a
    // DNS validator posted 50M, so the same word — "bond" — named two collaterals five thousand
    // apart, and an operator reading either number had no way to know which one applied to them.
    // The registration registry now has a single floor, and the two registries agree.
    //
    // **What it costs, stated rather than buried.** The quorum-corruption cost is
    // `ceil(2n/3) x min_bond`; at 12 seats that is 8 bonds, so it falls from 400M MSK to
    // 80,000 MSK. The count, not the per-bond amount, is now the whole of that floor — which is
    // what `min_active_validators`' own note already argued is the term that carries it, but the
    // note argued that while the bond was 5,000x larger, so the conclusion is doing more work now
    // than it was asked to. **Before mainnet launches, this figure should be re-derived against
    // the settlement value a finality vote can reverse**; ADR-0061's 10,000 was sized for a PALW
    // seat's slashable exposure, not for reversing a confirmed anchor, and the two are different
    // questions that now share an answer.
    //
    // **10,000 -> 20,000,000 MSK (user decision 2026-09-25, the 2026-09-24 mainnet assumption).**
    // That re-derivation is what separated the two floors again: a PALW seat's floor stays the
    // 13,000 MSK producer floor (`PALW_MAINNET_MIN_COLLATERAL_SOMPI`), and a DNS validator — whose
    // vote can reverse a confirmed anchor — bonds 20M. testnet-12 runs this set unchanged
    // (`PALW_T12_DNS_PARAMS`).
    min_bond_amount_sompi: 20_000_000 * SOMPI_PER_KASPA,
    epoch_length_blocks: 100,
    // audit H-02 (true WorkDepth, Option A): a DNS-confirmed anchor must be buried by at least this
    // much ACCUMULATED blue work SINCE it became the canonical lagged anchor (anchor-relative
    // WorkDepth, computed in `update_dns_state`), so confirmation is genuinely two-dimensional —
    // it requires BOTH `WorkDepth ≥ required_work_depth` AND `StakeDepth ≥ required_stake_depth`.
    // This closes the "stake confirms a shallow-PoW anchor" corner (a stake-side adversary can no
    // longer fast-finalize an anchor with little PoW behind it). CALIBRATION FLOOR (operator knob,
    // like `emergency_work_margin`): set so the work term is satisfied WELL BEFORE the stake window
    // at the launch difficulty (stake stays the liveness bottleneck) yet non-trivial; tune to the
    // live difficulty before mainnet. Devnet/simnet (`GENESIS_ACTIVE_DNS_PARAMS`) keep `ZERO`
    // (stake-only) for fast tests + fast bring-up.
    required_work_depth: Uint576([1_000_000, 0, 0, 0, 0, 0, 0, 0, 0]),
    required_stake_depth: StakeScore(10 * STAKE_SCORE_SCALE),
    emergency_work_margin: Uint576([1_000_000, 0, 0, 0, 0, 0, 0, 0, 0]),
    // CALIBRATION (incident 2026-07-19 §2-4) — see GENESIS_ACTIVE_DNS_PARAMS. The old
    // `100 * STAKE_SCORE_SCALE` exceeded the 15-epoch window ceiling (1.5e10), making the
    // two-dimensional escape path dead code. Inherited by TESTNET_DNS_PARAMS.
    emergency_stake_margin: StakeScore(3 * STAKE_SCORE_SCALE),
    max_reorg_horizon_blocks: 300,
    // 14 days; equivocation stays slashable for the whole exit window.
    evidence_window_blocks: FOURTEEN_DAYS_BLOCKS_10BPS,
    // 14-day unbonding + the reorg horizon so `U ≥ R + E` (ADR-0009 §"Long-range bound").
    unbonding_period_blocks: FOURTEEN_DAYS_BLOCKS_10BPS + 300,
    max_attestations_per_block: MAX_ATTESTATIONS_PER_SHARD as u16,
    max_attestation_shard_mass: 50_200,
    reward_uniqueness_window_blocks: 600,
    stake_event_quality_floor_bps: 6000,
    degraded_stake_quality_epochs: 4,
    stake_censorship_floor_bps: 1000,
    reward_params: RewardParams {
        per_attestation_reward_sompi: 100_000_000,
        slashing_reporter_reward_bps: 1000,
        max_validator_inflation_per_block_sompi: 100_000_000 * MAX_ATTESTATIONS_PER_SHARD as u64,
        // ADR-0018 "本格版" (PoS-v2): 70/30 participation/quality split. INERT until
        // `pos_v2_activation_daa_score` — below the v2 fence the reward path forces the full pool
        // into participation (effective bps 10_000), so this is byte-identical on every net today.
        validator_participation_bps: 7000,
        validator_quality_bonus_bps: 3000,
        quality_gate_bonus_sompi: 0,
        worker_urgency_multiplier_scaled: STAKE_SCORE_SCALE as u64,
        fee_split: FeeSplitParams {
            // kaspa-pq: validator subsidy share raised 25% → 30% (re-genesis 同便).
            // worker_base absorbs the 5pt; inclusion 8% kept. miner stays majority
            // (70% worker = 62% base + 8% inclusion), validator 30%. Strengthens the
            // stake-finality incentive (2-D DNS reorg defense) without inflating supply.
            subsidy_worker_base_bps: 6200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 3000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        },
        fee_split_bootstrap: FeeSplitParams {
            subsidy_worker_base_bps: 8200,
            subsidy_worker_inclusion_bps: 800,
            subsidy_validator_bps: 1000,
            subsidy_service_bps: 0,
            normal_fee_worker_bps: 9000,
            normal_fee_validator_bps: 1000,
            normal_fee_service_bps: 0,
            finality_fee_validator_bps: 7500,
            finality_fee_worker_bps: 2500,
            finality_fee_service_bps: 0,
        },
        // ADR-0018 "本格版" (PoS-v2) 4-way slashing split: reporter 10% (slashing_reporter_reward_bps)
        // / reserve 40% / victim 40% / burn 10%. INERT until pos_v2_activation — the slashing path
        // forces reserve/victim to 0 below the fence, degenerating to the byte-identical pre-v2 2-way
        // (reporter + burn). Calibratable economic defaults. The reserve **drip** (Phase 4) releases
        // at most `reserve_drip_per_epoch_cap_sompi` from the security reserve into the participation
        // pool per BLOCK past `palw_validator_payout_bounds`, and per finalized EPOCH below it. The
        // field name still says "per_epoch" and the unit past the fence is the block:
        // `at_two_minute_cadence` set `epoch_length_blocks` to 2 without moving this number, so a
        // block that merges 180 crosses ~90 thresholds and released ~90 caps (mainnet audit,
        // 2026-09-06, M-1). Renaming the field is fingerprint-neutral (borsh encodes positions, not
        // names) but it is a rename of an economic parameter and is left to the operator. All inert
        // via the v2 fence.
        security_reserve_bps: 4000,
        victim_epoch_pool_bps: 4000,
        reserve_drip_per_epoch_cap_sompi: 1000 * SOMPI_PER_KASPA,
    },
    reorg_mode: DnsReorgMode::TwoDimensionalDominance,
    full_reward_split_daa_score: 0,
    // PoS-v2 "本格版" economics master fence. ACTIVE from genesis (0) on mainnet +
    // testnet (this PRODUCTION preset): the §E participation/quality split, 4-way
    // slashing (reporter/reserve/victim/burn) + victim compensation, and the
    // security-reserve drip all run from block 1. devnet + simnet keep
    // GENESIS_ACTIVE_DNS_PARAMS's fence (`u64::MAX`), so v2 stays dormant there.
    // Not a genesis-block input, so the genesis hash is unchanged; the existing
    // pre-v2 chains are invalid under the new PQ-only/mass rules and need a
    // re-genesis regardless, which this activation rides along with.
    pos_v2_activation_daa_score: 0,
    // kaspa-pq DNS v3 (Canonical Lagged Anchor): blue_score-coordinated attestation epochs.
    // mainnet/testnet use larger lag/backoff than devnet for selected-chain convergence margin.
    // stake_score_window_blue_score must cover required_stake_depth (10 epochs) + lag + grace.
    attestation_epoch_length_blue_score: 100,
    attestation_lag_blue_score: 100,
    attestation_anchor_backoff_blue_score: 20,
    stake_score_window_blue_score: 1500,
    // ADR-0018 §F bridge wiring: deposit-lock txs' fees are finality-class (validator-primary
    // 75/25 split — bridge txs are where EVM-lane value depends on the validators' finalized
    // head) from genesis — doubly gated on the net's EVM activation, so it is LIVE on testnet
    // (EVM-active) and enforced-inert on mainnet until its EVM lane activates (a lock output
    // alone cannot reroute fees there). NOT a genesis-block input; the classification change
    // rides the ADR-0007 Phase-3 re-genesis (BlockRewardData/VirtualState store-format change).
    finality_fee_activation_daa_score: 0,
    // kaspa-pq bond spend-gate mergeset hardening: **ACTIVE FROM GENESIS on mainnet**, because
    // mainnet has no history and this preset's own overlay does not either (audit M1-1, 2026-08-28).
    //
    // The hole this closes: the legacy own-body gate never sees a spend that arrives via the
    // MERGESET, so a validator could withdraw its locked collateral out from under an `Active`
    // bond — removing exactly the backing `λ·B_i` caps weight against and slashing threatens. The
    // block carrying such a spend is disqualified as a CHAIN block, but it stays in the DAG, is
    // merged blue later, and its transactions are then accepted with no bond check at all
    // (`utxo_validation.rs:352-359`: with the fence at `u64::MAX` and PALW disabled, `bond_filter`
    // is `None`). The record survives untouched — `BondMutation` is only ever derived from
    // DNS-subnetwork transactions — so the bond keeps counting toward `min_active_stake_sompi` and
    // the attestation quorum while its collateral is a free UTXO.
    //
    // It sat at `u64::MAX` here because activating it on a network WITH history re-classifies
    // transactions and is therefore a coordinated hard fork. That reasoning is sound and does not
    // apply to a network that has not launched: this preset ships `dns_activation_daa_score: 0`, so
    // on mainnet the overlay and its spend gate now start together, which is the devnet/simnet
    // posture (`GENESIS_ACTIVE_DNS_PARAMS`) for exactly the same reason — "on a network that starts
    // with it there is no history to fork".
    //
    // **Testnet is deliberately not changed by this line.** `TESTNET_DNS_PARAMS` overrides the
    // field with `TESTNET_VLT_SHADOW_FORK_DAA_SCORE`, so a live testnet's fingerprint — and
    // therefore its handshake — is untouched; scheduling the fence on a network with history is
    // still that network's problem, and M1-6 is why it cannot be rolled out gradually today.
    //
    // ────────────────────────────────────────────────────────────────────────────────────────
    // **PRECONDITION, and it is not a fact this repository carries** (re-audit A-1).
    //
    // Both this `0` and the re-pinned mainnet entry in `shipped_presets_have_pinned_fingerprints`
    // are legal ONLY IF mainnet has not launched. The whole justification above — "a network that
    // has not launched has no history to fork" — rests on it, and the only written statement of it
    // anywhere in this tree is one sentence in `docs/palw-mainnet-audit-2026-08-28.md`. Two nearby
    // signals point the other way and are worth reading before trusting it: `genesis.rs` records
    // that this genesis differs from "the prior Argon2id-era mainnet genesis", and the 9B custody
    // ceremony is described there as complete.
    //
    // If mainnet IS live, this line is a hard fork of a value-bearing chain and must instead be a
    // scheduled fence — which needs `consensus_identity_id`'s genesis-active rule (re-audit R-1) to
    // be understood exactly: a fence moved from `u64::MAX` to a FUTURE height keeps the identity and
    // rolls out; moved to `0` it does not, and cannot.
    //
    // CONFIRM OUT OF BAND BEFORE CUTTING A RELEASE, and record the answer here.
    // ────────────────────────────────────────────────────────────────────────────────────────
    bond_spend_gate_mergeset_activation_daa_score: 0,
    // kaspa-pq liveness-first DNS finality: keep attestation below the base-chain validity layer.
    // Missing or below-floor shards degrade StakeScore / DNS health and pause finality-dependent
    // flows, but miners can still advance the PoW/GHOSTDAG ledger while validators recover. Invalid
    // shards remain rejected by the normal eligibility/signature checks. Private/research forks can
    // lower this fence to test the hard-inclusion anti-censorship rule.
    mandatory_attestation_inclusion_daa_score: u64::MAX,
    // Local finality-dependent producer/RPC policy: pause bridge/EVM payload production when the
    // DNS-confirmed anchor is older than this DAA distance. Not used for block validation.
    bridge_finality_max_staleness_daa_score: DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE,
    // Partition-liveness override (incident 2026-08-03 §8) — see GENESIS_ACTIVE_DNS_PARAMS. Kept
    // identical here (and therefore inherited by TESTNET_DNS_PARAMS via `..PRODUCTION_DNS_PARAMS`)
    // because the deadlock is structural, not threshold-dependent: it is reachable on any network
    // whose `reorg_mode` is TwoDimensionalDominance, which is every current preset.
    emergency_work_override_multiplier: 4,
    // Escape-from-a-dead-branch sink preference (incident #8 family): OFF on mainnet until the
    // partition/IBD scenario matrix (genesis IBD, reconnection, stale-anchor recovery,
    // validator-outage recovery, Active↔Stale boundary, overwhelming-work reorg) has been run by
    // the regression harness. Testnet overrides to 2 and soaks it first. See the field doc.
    stake_preference_max_work_deficit_multiplier: 0,
    // DNS-accelerated coinbase settlement: **600 DAA, the user's Mainnet Decision A (`1827850e`;
    // made mainnet's 2026-09-25)** — a coinbase is spendable by DAA maturity alone, at this long
    // fallback, or earlier once a DNS-confirmed anchor passes its block. It was 0 here "until
    // testnet has soaked the policy layer"; testnet-10/11 ran 600 and testnet-12 is that soak.
    //
    // The count is sized for the CARDED cadence, which is the only one a mainnet launches at
    // (`with_two_minute_cadence` converts the windows and leaves this): 600 > the 360-block gate
    // horizon + the 120-DAA veto TTL, the field's sizing rule. On the bundle-free 10 bps preset it
    // is inert — its base `coinbase_maturity` is 1,000 blocks, above 600, and
    // `coinbase_spend_settled` never undercuts the base floor — so that preset's spend rule is
    // exactly what 0 gave it.
    coinbase_settlement_long_maturity_daa: 600,
    // Consensus enforcement fence: NEVER in this build — the per-chain-block anchor fold is not
    // wired; the fence exists so the fold-carrying build announces itself via the fingerprint.
    coinbase_settlement_consensus_activation_daa_score: u64::MAX,
    // Unbond-authorization mergeset hardening (incident 2026-08-07): GENESIS-ACTIVE on every
    // preset, so a new network never has to pick — and remember — a per-net activation score.
    //
    // Residual on testnet-10 only: its chain already carries one unauthorized request (DAA
    // 28_059_617) whose stamp is persisted in the two operator nodes' bond stores. The bond store
    // is incremental derived state (ADR-0009 A.4), so those nodes keep the stamp while a node
    // synced from genesis under this rule will not derive it — a one-bond (1M of 24M MSK)
    // difference in the active-stake denominator. Both sides clear every threshold by a wide
    // margin, and the divergence is erased for good the first time the two nodes are resynced.
    // Genesis-active was chosen deliberately over a fence: the fence only papers over that single
    // historical stamp, at the cost of every future net inheriting a magic number.
    unbond_authz_mergeset_activation_daa_score: 0,
    // ADR-0060 Decision 4's inactivity leak is removed; this retired constant has no reader and
    // stays `u64::MAX` because `DnsParams` is hashed whole into `consensus_params_id`.
    inactivity_leak_daa: u64::MAX,
    // MISAKA Verified LLM Token-Weighted BFT (`vlt::VltParams`): dormant on mainnet. Its weight
    // fence — the switch of voting power from bonded stake to verified compute — is removed, and
    // `validate_palw_v2` refuses it anywhere but `u64::MAX`; a network that runs the compute
    // overlay moves only the shadow fence and registers a model table (see
    // `shipped_presets_are_either_dormant_or_fully_forkable`).
    vlt: VltParams::INERT,
    // MISAKA Compute Token Program (design v0.1 §10): inert everywhere — the TOK ledger and
    // emission do not exist until a per-network hard fork moves these fences (and freezes the
    // TBD R0/H schedule numbers the design deliberately leaves open).
    tkn: TokenParams::INERT,
    // Sized for `VltParams::INERT`'s K = 96 + delay 1 epochs at the 100-blue_score attestation
    // epoch length, plus the 300-block challenge window and a lag/grace margin. The compute-overlay
    // walk pays it only once the shadow fence above is moved.
    vlt_credit_window_blue_score: 10_400,
    // ---- DNS-veto reach and its release paths (calibrated together; see the field docs) ----
    //
    // The 2026-08-03 §8 fix made the gate ABSTAIN past `max_reorg_horizon_blocks` instead of
    // rejecting outright, which is what un-wedged testnet-22 — but it also pinned the veto's whole
    // reach to that horizon, and at 10 BPS 300 blocks is THIRTY SECONDS. Every fork older than
    // that was settled by PoW alone, so "DNS finality" protected a 30-second window. These three
    // knobs restore a meaningful reach while keeping a bounded, layered release, so reach is no
    // longer bought with liveness:
    //
    //   * reach:   the gate now judges any fork that would rewind up to 18_000 of this node's own
    //              chain blocks (≈30 min at the nominal 10 BPS).
    //   * release 1 (immediate): `emergency_work_override_multiplier` — >4x work since the common
    //                 ancestor, i.e. only an adversary sustaining >80% of hashpower.
    //   * release 2: `dns_veto_ttl_daa_score` — my own chain advanced 6_000 DAA past my confirmed
    //                 anchor with no new confirmation ⇒ the branch I am defending has lost its
    //                 validators (the testnet-20 dead-branch wedge shape). The healthy
    //                 anchor-to-tip distance is `lag + epoch ≈ 200`, so 6_000 is ~30x headroom and
    //                 never fires on a chain that is still confirming.
    //   * release 3: divergence past the gate horizon ⇒ abstain, as today.
    //
    // Both numbers are denominated in blocks/DAA, NOT wall clock: the wall-clock figures above
    // assume the nominal 10 BPS, and a net mining below it (the live testnet mesh runs ~2 BPS at
    // floored Argon2id CPU difficulty) gets proportionally MORE history protected and a
    // proportionally longer release — ~2.5 h of rewind protection and ~55 min to auto-release.
    // That is the right direction for both: protection scales with the chain, and a slow net's
    // attestation hiccups do not trip the TTL. Retune only against a measured block rate.
    //
    // Worst case a *both-sides-alive* partition (t22's shape, where neither TTL fires) now holds
    // for up to the horizon instead of 30 s — bounded, and t22 itself released via layer 1 (branch
    // A out-worked branch B by ~27x). The old failure mode — unbounded — is gone in every layer.
    dns_gate_horizon_blocks: 18_000,
    dns_veto_ttl_daa_score: 6_000,
    // Mainnet floor is 3 active validators (audit H-11), so requiring 2 DISTINCT credited
    // attesters in an anchor's own epoch keeps the veto un-armable by a single signer while
    // tolerating one validator being down. TESTNET overrides this to 1 (single-operator mesh).
    min_anchor_attesters: 2,
};

/// kaspa-pq Phase 2 (ADR-0007): testnet DNS params = [`PRODUCTION_DNS_PARAMS`] with a lowered
/// `required_work_depth`. Argon2id's memory-hard PoW (CPU hash-rate ~hundreds H/s) drives the
/// testnet difficulty all the way to `max_difficulty_target`, so the anchor-relative WorkDepth
/// settles at a tiny floor (~200-300 at the live 3-CPU-miner difficulty) and the kHeavyHash-era
/// 1_000_000 floor is unreachable — `dnsConfirmed` would never flip even though stake is
/// confirmed (`StakeDepth ≥ required_stake_depth`). Lower the testnet work floor to a token value
/// the Argon2id chain reliably exceeds so the 2-D gate confirms on stake; the work dimension is
/// near-trivial at floored CPU difficulty (stake is the real finality). Mainnet keeps PRODUCTION's
/// 1_000_000 (the operator tunes it to the live mainnet difficulty at launch — see the field
/// comment in PRODUCTION_DNS_PARAMS). NOT a genesis-block input, so the genesis hash is unchanged.
///
/// Also lowers the staking thresholds so testers can actually run a validator: at the testnet
/// block subsidy (~3.7 MSK/block) the mainnet `min_bond_amount_sompi`/`min_active_stake_sompi` of
/// 20M KAS would need ~26 days of CPU mining (or a premine grant) to fund, and the coinbase
/// arrives as ~3.7-MSK fragments. Lowering both to 10 KAS lets a tester mine for a few seconds
/// and bond (the `bond` CLI aggregates several mature coinbase UTXOs — see `build_funded_stake_bond_tx_multi`).
/// Mainnet keeps the 20M-KAS floors. None of these are genesis-block inputs.
/// ADR-0024 step 3 (the VLT SHADOW fork) — the ONE constant a release cut has to choose.
///
/// **GENESIS-ACTIVE (`0`) since the PALW re-genesis** (user decision 2026-08-12). The height
/// 30_200_000 this constant used to carry was measured against the SUPERSEDED 1-bps chain's live
/// tip; on the re-genesised chain, which restarts at DAA 0 and advances one block per 120 s, that
/// height is ~115 years away — the fence, and with it the bond spend gate the 2026-08-11 audit
/// called a P0, would have been silently inert forever. Since the chain is being rebuilt anyway
/// there is no fleet to coordinate ahead of a future height: the fork ships in the genesis rules,
/// which is strictly safer than any scheduled height (nothing to miss, nothing to race).
///
/// `u64::MAX` still means "not scheduled". Setting it to a DAA height picks up everything the
/// fork needs at once, because the three fields below are derived from it rather than edited
/// independently:
///
/// * `vlt.vlt_shadow_activation_daa_score` — the overlay starts crediting, drawing committees,
///   paying the audit fee and slashing settled challenges;
/// * `bond_spend_gate_mergeset_activation_daa_score` — the mergeset spend gate closes, so the
///   collateral those slashes are aimed at can no longer be withdrawn out from under an Active
///   bond (2026-08-11 audit P0; it MUST move with the fence, never after it);
/// * `vlt.model_cost_table` — the registered profiles, without which every job mints zero and the
///   fork would cost a hard fork to discover it did nothing.
///
/// The weight fence (`vlt_activation_daa_score`) stays `u64::MAX`: VLT voting weight is removed
/// and `Params::validate_palw_v2` refuses any other value. Genesis-active step 3 means the overlay
/// draws committees, pays the audit fee and slashes settled challenges from block 1 — compute
/// weight never decides finality.
///
/// Choosing the height, the fleet-update procedure and the exit criteria are in
/// `docs/testnet10-vlt-shadow-fork-runbook.md`. The rule of thumb: current tip plus twice the
/// fleet's update window, and every validator/miner binary inside the fleet BEFORE it.
pub const TESTNET_VLT_SHADOW_FORK_DAA_SCORE: u64 = 0;

/// **testnet-12's DNS-finality set: mainnet's own** (user decisions 2026-09-24 and 2026-09-25).
///
/// [`PRODUCTION_DNS_PARAMS`] — the mainnet set — through [`DnsParams::at_two_minute_cadence`], the
/// same conversion that gives testnet-11 its 120 s windows and a carded mainnet its own (so, e.g.,
/// `unbonding_period_blocks` is 14 days at t12's 120 s blocks plus the reorg horizon: 10,083 blocks,
/// not 14 days of 10 bps blocks), with ONE substitution:
///
/// * **`required_work_depth` = testnet-11's**, the one testnet-only value the chain needs to stay
///   coherent: production's is a 10 bps kHeavyHash blue-work depth that a PALW chain at the easiest
///   header target would take years to accumulate, so DNS confirmation could never flip. (The same
///   retune is owed on the mainnet side before a card launches.)
///
/// Everything else is production's, and since 2026-09-25 production carries the numbers this set
/// used to override: **6 validators, a 20,000,000 MSK bond, 120,000,000 MSK of active stake**
/// (validators x bond) and **Decision A's 600-DAA coinbase long maturity**. So the set testnet-12
/// runs did not move when those overrides were dropped — only mainnet's did. Also production's:
/// `required_stake_depth` (ten epochs at full participation), `min_anchor_attesters` = 2, the reward
/// params, the stake preference off, and the compute overlay inert (`VltParams::INERT`; ADR-0134
/// retired it). `validate_palw_v2` and the v3 anchor invariants accept the set (pinned in
/// `the_testnet_12_dns_set_is_mainnet_assumed_and_nobody_elses_moved`).
pub const PALW_T12_DNS_PARAMS: DnsParams = {
    let mut dns = PRODUCTION_DNS_PARAMS.at_two_minute_cadence();
    dns.required_work_depth = TESTNET_DNS_PARAMS.required_work_depth;
    dns
};

// SCHEDULED 2026-08-11. Live tip measured at 29_981_862 (`/info/blockdag` on the public
// explorer, cross-checked by a P2P handshake with the fleet). t10 runs at 1 bps, so the margin is
// ~2.5 days — twice the end-to-end duration of the 2026-08-10 flag day, so an operator who starts
// the rollout when this release lands still finishes with a day to spare.
//
// RE-CHECK BEFORE CUTTING. This number is only as good as the tip it was measured against: if the
// release slips past ~2026-08-13 the margin is gone and H must be recomputed, because a fence
// that arrives with the fleet half-updated forks the un-updated half at the first audit-fee
// coinbase. `docs/testnet10-vlt-shadow-fork-runbook.md` has the procedure and the five-minute
// staleness check.
//
// The model table rides along automatically: `with_registered_models` attaches the registered
// profiles at every `From<NetworkType/NetworkId> for Params`, so a scheduled fence can never
// reach a consumer over an empty table (which would be a coordinated hard fork crediting every
// job zero — see `shipped_presets_are_either_dormant_or_fully_forkable`).
//
// The WEIGHT fence stays dormant. This release only starts the overlay running and policing;
// moving the vote is step 4, after the soak has measured what the weight is made of.

pub const TESTNET_DNS_PARAMS: DnsParams = DnsParams {
    required_work_depth: Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]),
    min_bond_amount_sompi: 10 * SOMPI_PER_KASPA,
    min_active_stake_sompi: 10 * SOMPI_PER_KASPA,
    // Dormant here for the same reason as PRODUCTION — see the comment there. (The testnet
    // window is 30 blue score, so the implemented rule would have leaked on ~1 h of silence
    // against a declared 7 days.)
    inactivity_leak_daa: u64::MAX,
    // Experimental single-operator testnet mesh: pin the validator-count floor to 1 (mainnet's
    // PRODUCTION floor is 3, audit H-11). This is the live testnet's intended config; do NOT raise
    // it here without re-provisioning multiple testnet validators.
    min_active_validators: 1,
    // kaspa-pq audit fix (M-2 comment correction): TESTNET lowers min_active_stake / min_bond from
    // PRODUCTION's 20M KAS to 10 KAS. PRODUCTION's `required_stake_depth = StakeScore(10 *
    // STAKE_SCORE_SCALE)` (= 10 epochs at full participation, since StakeScore accrues exactly
    // STAKE_SCORE_SCALE = 1_000_000_000 units per fully-participated epoch) is calibrated for the
    // 20M-KAS-scale active set; left inherited it makes `StakeDepth >= required_stake_depth`
    // effectively unreachable for a 10-KAS testnet validator, so `dns_confirmed` could never flip.
    //
    // `StakeScore(5000)` is a DELIBERATELY LOW testnet threshold, NOT "~10 epochs of stake": with
    // STAKE_SCORE_SCALE = 1_000_000_000, 5000 units is ~5e-6 of a single fully-participated epoch's
    // accrual, so even a tiny validator clears the stake dimension of the 2-D finality gate within
    // its FIRST attested epoch (`required_stake_depth_epochs = ceil(5000 / STAKE_SCORE_SCALE) = 1`).
    // The intent is fast confirmation on a low-stake experimental mesh, not to mirror PRODUCTION's
    // 10-epoch burial. NOT a genesis input (dns_params).
    required_stake_depth: StakeScore(5000),
    // The mesh runs a SINGLE validator by design (`min_active_validators: 1` above), so PRODUCTION's
    // 2-distinct-attester floor would mean no anchor ever confirms here — DNS finality would be
    // silently off, not merely weaker. Keep the original "≥1 credited attestation in the anchor's
    // own epoch" guard. Raise this in lockstep with `min_active_validators`, never before.
    min_anchor_attesters: 1,
    // ── PALW 120 s/block re-sizing of every block/DAA/blue-score-denominated window ────────────
    // PRODUCTION's windows are sized for 10 bps (100 ms blocks); inherited unchanged onto a
    // 120 s/block chain their wall clock stretches ×1200 (the 14-day unbond would be ~46 YEARS).
    // Every override below preserves the SECURITY semantics in wall clock, with the deviations
    // noted inline. Mainnet (10 bps) keeps PRODUCTION as-is.
    //
    //   field                                 PRODUCTION @10bps      here @120 s     wall clock
    //   epoch_length_blocks                   100        (10 s)      2   (4 min)     see note A
    //   max_reorg_horizon_blocks              300        (30 s)      3   (6 min)     see note B
    //   evidence_window_blocks                12_096_000 (14 d)      10_080 (14 d)   exact
    //   unbonding_period_blocks               +300                   10_083          U ≥ R+E ✓
    //   dns_gate_horizon_blocks               18_000     (30 min)    360 (12 h)      3×TTL kept
    //   dns_veto_ttl_daa_score                6_000      (10 min)    120 (4 h)       30× the
    //     healthy anchor-to-tip distance (lag 2 + epoch 2 = 4), the same headroom ratio the
    //     PRODUCTION comment derives (6_000 ≈ 30 × 200).
    //   attestation_epoch_length_blue_score   100                    2
    //   attestation_lag_blue_score            100                    2
    //   attestation_anchor_backoff_blue_score 20                     1               floor
    //   stake_score_window_blue_score         1_500                  30  (= 15 epochs, the
    //     window-ceiling ratio the emergency_stake_margin calibration relies on)
    //   bridge_finality_max_staleness         1_500      (150 s)     2   (4 min)     ≥ 150 s
    //   coinbase_settlement_long_maturity     30_000     (~4.5 h)    600 (20 h)      > horizon
    //     + TTL with the same margin shape (600 > 360 + 120).
    //   reward_uniqueness_window_blocks       600 — INHERITED unchanged (block-count semantics;
    //     the scan bound is cheap and shrinking it toward the mergeset limit would weaken it).
    //
    // Note A: the ×1200 scaling would put epoch_length at 0 blocks, which is meaningless — 2 is
    // the floor at which an epoch still contains a boundary and an interior. It is 4 min of wall
    // clock (vs PRODUCTION's 10 s): epochs are now a coarse unit on this chain, which is the
    // honest consequence of 120 s blocks, not a tuning choice.
    // Note B: likewise floored. 3 blocks is 6 min — far longer in wall clock than PRODUCTION's
    // 30 s, and comfortably above any plausible anticone depth at k = 1.
    epoch_length_blocks: 2,
    max_reorg_horizon_blocks: 3,
    evidence_window_blocks: 10_080,
    unbonding_period_blocks: 10_083,
    dns_gate_horizon_blocks: 360,
    dns_veto_ttl_daa_score: 120,
    attestation_epoch_length_blue_score: 2,
    attestation_lag_blue_score: 2,
    attestation_anchor_backoff_blue_score: 1,
    stake_score_window_blue_score: 30,
    bridge_finality_max_staleness_daa_score: 2,
    // (superseded by the PALW table above; the historical rationale for inheriting 18_000/6_000
    // at 10 bps — measured ~110 DAA/min, ~2.5 h protection, ~55 min auto-release — is preserved
    // in the PRODUCTION_DNS_PARAMS comment.)
    //
    // **And the TTL is counted on a clock the wedge can stop** (carried from `origin/main`,
    // 2026-08-13/14). The inherited 6_000 was calibrated against "~110 DAA/min of the live mesh",
    // which reads as ~55 min of release time — but that assumes the chain keeps moving, and the
    // wedge this TTL exists to release is exactly the state where it does not. Measured on the
    // live t10 fleet: a confirmed anchor fell off the selected chain, every candidate failed the
    // includes test, the gate refused every heavier candidate, and virtual advanced 3 DAA per
    // ~17 min. At THAT rate 6_000 takes ~12 days, not 55 min. The clock is
    // `canonical_daa - anchor_daa` on this node's OWN chain, so the release cannot fire because
    // the thing it releases is what freezes its input.
    //
    // 120 survives that geometry, which is why the table above stands rather than being replaced
    // by main's 2_000: at the frozen 120 s cadence 120 DAA is 4 h of healthy time, and even at the
    // wedged 3-DAA-per-17-min rate it is ~11 h — a release within half a day, which is what main's
    // constant was reaching for. 2_000 was chosen for a 10 bps chain and would be ~2.8 days here.
    // The honest fix is still to stop counting the TTL on a stoppable clock; that is a behaviour
    // change to `dns_reorg_outcome` and belongs in its own release.
    // The preference soaks here first (mainnet ships it OFF): ½-work bound, arming only when
    // this chain's own anchor has been dead past the TTL above. See the field doc.
    stake_preference_max_work_deficit_multiplier: 2,
    // Settlement soaks here first too, POLICY layer only (the consensus layer is unwired until
    // the anchor gets its sequential per-chain-block view — see the field doc). 600 DAA > gate
    // horizon (360) + veto TTL (120) with margin: a contested fork should resolve before either
    // side's rewards go liquid through the fallback. At 120 s/block this is ~20 h — long enough
    // to bite, short enough to tolerate while the validator set is being restored.
    coinbase_settlement_long_maturity_daa: 600,
    // Consensus enforcement fence: NEVER in this build — the per-chain-block anchor fold is not
    // wired; the fence exists so the fold-carrying build announces itself via the fingerprint.
    coinbase_settlement_consensus_activation_daa_score: u64::MAX,
    // ADR-0024 step 3, driven by the ONE release constant above. Both move together by
    // construction: the fence turns on challenge slashing, and the spend gate is what keeps the
    // collateral those slashes aim at from being withdrawn through the mergeset first
    // (2026-08-11 audit P0). Shipped `u64::MAX` = not scheduled.
    bond_spend_gate_mergeset_activation_daa_score: TESTNET_VLT_SHADOW_FORK_DAA_SCORE,
    vlt: VltParams {
        vlt_shadow_activation_daa_score: TESTNET_VLT_SHADOW_FORK_DAA_SCORE,
        // The VOTE does not move in this fork — that is step 4, after the soak.
        vlt_activation_daa_score: u64::MAX,
        ..VltParams::INERT
    },
    ..PRODUCTION_DNS_PARAMS
};

pub const MAINNET_PARAMS: Params = Params {
    // kaspa-pq mainnet DNS seeders (isolated from upstream Kaspa per
    // docs/adr/0001-network-isolation.md — these are MISAKA-operated only). A node
    // resolves each hostname's A/AAAA records to a list of peer IPs and randomly
    // selects among them (Kaspa-style auto-discovery), connecting on the mainnet
    // default P2P port (26111). The hosts behind these records must run a reachable
    // mainnet node on 26111. `addnode` flags still augment this list.
    dns_seeders: &[
        "seeder1.misakascan.com",
        "seeder2.misakascan.com",
        "seeder3.misakascan.com",
        "seeder4.misakascan.com",
        "seeder1.misakachain.com",
        "seeder2.misakachain.com",
        "seeder3.misakachain.com",
    ],
    net: NetworkId::new(NetworkType::Mainnet),
    genesis: GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    past_median_time_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // Limit the cost of calculating compute/transient/storage masses
    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    // Transient mass enforces a limit of 125Kb, however script engine max scripts size is 16Kb so there's no point in surpassing that.
    max_signature_script_len: 16_384,
    // Compute mass enforces a limit of ~45.5Kb, however script engine max scripts size is 16Kb so there's no point in surpassing that.
    // Note that storage mass will kick in and gradually penalize also for lower lengths (generalized KIP-0009, plurality will be high).
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    // kaspa-pq Phase 7 (ML-DSA-87 verify recalibration; supersedes the
    // Phase-6 ML-DSA-87 numbers). Measured on Apple Silicon arm64 via
    // `crypto/txscript/benches/bench.rs` (ml_dsa_87::verify):
    //   Schnorr verify (secp256k1):              12.74 µs
    //   ML-DSA-87 verify (default, NEON/AVX2):   63.88 µs  (5.01× ratio)
    //   ML-DSA-87 verify (libcrux portable):     76.52 µs  (6.01× ratio — slowest)
    //
    // Per `docs/adr/0005-mass-policy.md` §"Calibration formula" the
    // value is calibrated against the slowest variant so that no-SIMD
    // low-end reference platforms remain safely budgeted:
    //   1000 (upstream) × 6.01 (slowest ratio) × 1.59 (safety) = 9548 → 10_000.
    mass_per_sig_op: 10000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    // kaspa-pq emission: there is no flat pre-deflationary phase — the decay
    // table in `SUBSIDY_BY_MONTH_TABLE` (15B over 20 years at 5%/yr) applies from
    // genesis, so `deflationary_phase_daa_score` is 0. That makes
    // `pre_deflationary_phase_base_subsidy` unused by `calc_block_subsidy`; it is
    // kept equal to the year-1 per-block subsidy at 10 BPS (table[0].div_ceil(10)
    // = 370_468_345 sompi ≈ 3.70468 KAS) so callers reading it see the genesis rate.
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 370468345,
    skip_proof_of_work: false,
    max_block_level: 225,
    pruning_proof_m: 1000,

    blockrate: BlockrateParams::new::<10>(),

    // kaspa-pq: 10 BPS since genesis. This field only feeds the subsidy-month
    // calc (`bps_history`); setting it to 100ms keeps emission on the 10 BPS
    // schedule throughout, independent of the (legacy) crescendo activation score.
    pre_crescendo_target_time_per_block: 100,

    // Roughly 2025-05-05 1500 UTC
    crescendo_activation: ForkActivation::new(110_165_000),
    // kaspa-pq: MAINNET uses the production overlay params — 20M-KAS min active stake + 14-day
    // unbonding/evidence window (slashable through the whole exit). See PRODUCTION_DNS_PARAMS.
    // Not a genesis-block input, so the genesis hash is unchanged.
    dns_params: Some(PRODUCTION_DNS_PARAMS),
    palw_credit: None,
    palw_fork_choice: None,
    palw_schedule: None,
    palw_ramp: None,
    palw_block_commitment: None,
    palw_bootstrap_activation: None,
    palw_unavailable_abstains: None,
    palw_bond_maturity: None,
    palw_bond_maturity_early: None,
    palw_frontier_provenance: None,
    palw_reorg_strict_economic_win: None,         // lane: rcore/f1-forkchoice-attacks
    palw_pruning_proof_strict_economic_win: None, // lane: rcore/hf-pptake2
    palw_capacity_weight_cap: None,               // lane: rcore/cap-weight (ADR-0160 F-W)
    palw_heartbeat: None,
    palw_attempt_work: None,
    // ADR-0072's activation fence ships dormant: this preset runs the execution-priced rule at
    // every height on algo-6, exactly as it does today, and arming the fence is the only thing
    // that would change a block.
    palw_attempt_activation: None,
    dns_bft_gate: None,
    palw_inactivity_leak: None,
    palw_beacon_fold: None,
    palw_capability_bound: None,
    palw_compute_overlay_retired: None,
    palw_model_registry: None,
    palw_economic_payout: None,
    palw_work_target: None,
    palw_single_lottery: None,
    palw_short_challenge_window: None,
    palw_verification_v2: None,
    palw_verification_s3: None,
    palw_verification_s2: None,
    palw_readiness_v2: None,
    palw_anchor_clock: None,
    palw_clock_cursor: None,
    palw_clock_floor: None,
    palw_clock_lead_cap: None,
    palw_offence_attribution: None,
    palw_rcore_plus: None,
    palw_panel_seed_execution: None,
    palw_operator_anchor: None,
    palw_capacity_aggregate_liability: None,
    palw_rcore_conservative_classes: &[],
    palw_final_lock_full_collateral: None,
    palw_final_lock_life: None,
    palw_final_lock_life_retro: None,
    palw_class_verify_deadline: None,
    palw_class_verify_rows: &[],
    palw_anchor_at_ceiling: None,
    palw_lane_accept_parents_first: None,
    palw_activation_pool: None,
    palw_readiness_v2_max_age_spans: None,
    palw_registry_resilience: None,
    palw_floor_refusal_retry: None,
    palw_artifact_root_ownership: None,
    palw_operator_id_unique: None,
    palw_objective_offence: None,
    palw_seat_gate_possession: None,
    palw_class_receipt_window: None,
    palw_execution_quanta: None,
    palw_public_model_source_required: None,
    palw_economic_safety: None,
    palw_canonical_work: None,
    palw_admission_independence: None,
    palw_context_ladder: None,
    // ADR-0077 Decision 16: `PanelDa` is dormant. A prompt that stays off chain is a mode a
    // network arms on purpose, never one it acquires by upgrading.
    palw_panel_da: None,
    palw_certification_rent: None,
    // ADR-0069 Decision 7: dormant. Arming it is a per-network activation decision.
    palw_uncertified_weightless: None,
    palw_da_court: None,
    palw_court_ladder: None,
    palw_model_market: None,
    palw_model_lines: None,
    palw_model_immutable_v1: None,
    palw_class_semantics_v1: None,
    palw_court_commitment_scope_v1: None,
    palw_staged_public_court_v1: None,
    palw_model_benefits: None,
    palw_model_leg_v2: None,
    palw_model_seed_v2: None,
    // ADR-0162: dormant on every preset.
    palw_model_virtual_v1: None,
    palw_model_evm: None,
    palw_model_sink_bound: None,
    palw_capacity_batch_licence: None,
    palw_capacity_verify_room: None,
    palw_capacity_audit_door: None,
    palw_capacity_issuance_slots: None,
    palw_capacity_network_room: None,
    palw_capacity_network_verify: None,
    palw_panel_unavailable_expiry: None,
    palw_panel_fast_switch: None,
    palw_seat_availability: None,
    palw_capacity_emission_budget: None,
    palw_capacity_multi_claim: None,
    palw_capacity_rho_breaker: None,
    palw_chunk_cap_charge: None,
    palw_prompt_ids_merkle: None,
    palw_kary_court: None,
    palw_model_court_window: None,
    palw_court_responder_coverage: None,
    palw_fp_decode_rules: None,
    palw_fp_derived_work: None,
    palw_fp_decode_constraint: None,
    palw_shard_court: None,
    palw_shard_licensing: None,
    palw_token_lift: None,
    palw_tir_v1: None,
    palw_gen_v1: None,
    palw_fp_job_v5: None,
    palw_gen_range_twin_v1: None,
    palw_task_heads_v1: None,
    palw_probabilistic_constraints_v1: None,
    palw_fork_choice_rule_e_v1: None,
    palw_typed_roots_v1: None,
    palw_fork_choice_commitment_v1: None,
    palw_verifier_pay_v1: None,
    palw_model_refutation_seal_v1: None,
    palw_beacon_source_opt_in_v1: None,
    palw_beacon_source_fee_v1: None,
    palw_panel_free_v1: None,
    palw_signed_registration_v1: None,
    palw_tir_shard_segment_v2: None,
    palw_provider_court_v1: None,
    palw_legacy_held_da_v2: None,
    palw_bond_budget_v1: None,
    palw_reporter_income_v2: None,
    palw_material_income_v3: None,
    palw_panel_lifetime_v2: None,
    palw_claim_challenge_v2: None,
    palw_monetary_material_v1: None,
    palw_model_verification_gate_v1: None,
    palw_horizon_extension_v1: None,
    palw_minimum_price_v1: None,
    palw_beacon_bootstrap_v1: None,
    palw_paid_consumer_lifecycle_v1: None,
    palw_model_bond_allocation_v1: None,
    palw_reporter_share_v2: None,
    palw_legacy_public_filer_v1: None,
    palw_court_model_scope_v1: None,
    palw_public_evidence_reward_v1: None,
    palw_claim_hard_deadline_v1: None,
    palw_fp_prefix_inherit: None,
    palw_fp_prefix_state: None,
    palw_fp_tokenizer_match: None,
    palw_fp_constraint_v2: None,
    palw_adapter_class_v1: None,
    palw_tir_fence2: None,
    palw_tir_shard_v1: None,
    palw_floor_reserve_v1: None,
    palw_real_clock_tick_v1: None,
    palw_anchor_window_v1: None,
    palw_receipt_spend_v4: None,
    palw_evidence_court_v1: None,
    palw_audit_1004_v1: None,
    palw_improvement_v1: None,
    palw_held_close_chunks_v1: None,
    palw_verification_vertex_v1: None,
    palw_exec_payload_v2: None,
    palw_witness_manifest_v1: None,
    palw_audit_mesh_v1: None,
    palw_capped_onboarding_v1: None,
    palw_class_seating: None,
    palw_tir_only_v1: None,
    palw_kimi_k3: None,
    palw_fused_dissectable: None,
    palw_attn_anchored_root: None,
    palw_held_context: None,
    palw_prefill_draw: None,
    palw_audit_2026_09_11: None,
    palw_audit_2026_09_11_deep: None,
    palw_audit_2026_09_23: None,
    palw_settled_anchor_depth: None,
    palw_admission_audit_period_daa: None,
    palw_exec_quantum_maturity_daa: None,
    palw_difficulty_priced_rows: None,
    palw_receipt_rows_unpriced: None,
    palw_attempt_header_pins: None,
    palw_signature_contexts_v2: None,
    palw_fp_da_pins: None,
    palw_validator_payout_bounds: None,
    palw_slashing_evidence_utxo_genuine: None,
    palw_epoch_boundary_budget: None,
    palw_epoch_budget_release: None,
    palw_panel_economy: None,
    palw_work_priced_reward: None,
    palw_execution_lane: None,
    palw_overlay_carve: None,
    palw_dns_retirement: None,
    palw_permissionless_panel_v1: None,
    palw_permissionless_panel_complete_v1: None,
    palw_permissionless_panel_retired_v1: None,
    palw_panel_exposure_floor: None,
    palw_fp_ruleset_caps: None,
    // ADR-0105: dormant on every shipped preset (see the field's doc).
    palw_heartbeat_transparent: None,
    // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency): dormant on every preset.
    palw_heartbeat_transparent_same_chain: None,
    palw_capacity_escrow_at_licence: None,
    // ADR-0107: dormant on every shipped preset (see the field's doc).
    palw_share_growth_final: None,
    palw_gdn_key_heads: None,
    palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode::Disabled,
    pow_blake2b_sha3_activation: ForkActivation::always(),
    // PALW LLM PoW: inert on mainnet until its own fork ADR schedules it.
    pow_palw_activation: ForkActivation::never(),
    pow_palw_ollama_activation: ForkActivation::never(),
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020: EVM lane inert in P1 (no executor yet); the testnet value flips to
    // a finite activation score when the revm executor lands (P2+). u64::MAX = never.
    evm_activation_daa_score: u64::MAX,
    // gas-pool v2 ships inert on every network — a deploy sets a finite testnet score.
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_bridge_ledger_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

pub const TESTNET_PARAMS: Params = Params {
    // kaspa-pq testnet DNS seeders (MISAKA-operated, isolated per
    // docs/adr/0001-network-isolation.md). Same Kaspa-style auto-discovery as mainnet,
    // but nodes connect on the testnet-10 default P2P port (26211) — so the hosts
    // behind these records must also run a reachable testnet-10 node on 26211.
    //
    // Not a consensus input: `consensus_params_id` excludes `dns_seeders` ("where to find
    // peers is not a rule about blocks"), so adding a record here is NOT a flag day and a
    // node carrying it peers with one that does not.
    //
    // Third zone (`misakastake.com`) added 2026-08-10, on purpose: `misakascan.com` and
    // `misakachain.com` both delegate to the same two hosts that back the fleet's own nodes,
    // so a bootstrap has been single-operator AND single-pair since launch. seeder1 here is
    // delegated to host C (5.104.81.23) — the third machine, previously build-only — which
    // makes the discovery path survive the loss of either original host.
    dns_seeders: &[
        "seeder1.misakascan.com",
        "seeder2.misakascan.com",
        "seeder3.misakascan.com",
        "seeder4.misakascan.com",
        "seeder1.misakachain.com",
        "seeder2.misakachain.com",
        "seeder3.misakachain.com",
        "seeder1.misakastake.com",
    ],
    net: NetworkId::with_suffix(NetworkType::Testnet, 10),
    genesis: TESTNET_GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    past_median_time_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    // PALW 120 s/block: with `difficulty_sample_rate = 1` (every block sampled) the window size
    // IS the window duration in blocks. 264 × 120 s ≈ 8.8 h of difficulty memory — longer in
    // wall clock than the 2 641 s the shared constant models at ≥1 bps, and kept deliberately:
    // the miner set is a provisioned fleet rather than an open hashrate market, so the ±6 %
    // statistical stability of 264 samples is worth more than fast response. The 150-block
    // `min_difficulty_window_size` still gives a ~5 h fixed-difficulty launch window.
    difficulty_window_size: 264,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // Limit the cost of calculating compute/transient/storage masses
    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    // Transient mass enforces a limit of 125Kb, however script engine max scripts size is 16Kb so there's no point in surpassing that.
    max_signature_script_len: 16_384,
    // Compute mass enforces a limit of ~45.5Kb, however script engine max scripts size is 16Kb so there's no point in surpassing that.
    // Note that storage mass will kick in and gradually penalize also for lower lengths (generalized KIP-0009, plurality will be high).
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    // kaspa-pq Phase 7 (ML-DSA-87 verify recalibration; supersedes the
    // Phase-6 ML-DSA-87 numbers). Measured on Apple Silicon arm64 via
    // `crypto/txscript/benches/bench.rs` (ml_dsa_87::verify):
    //   Schnorr verify (secp256k1):              12.74 µs
    //   ML-DSA-87 verify (default, NEON/AVX2):   63.88 µs  (5.01× ratio)
    //   ML-DSA-87 verify (libcrux portable):     76.52 µs  (6.01× ratio — slowest)
    //
    // Per `docs/adr/0005-mass-policy.md` §"Calibration formula" the
    // value is calibrated against the slowest variant so that no-SIMD
    // low-end reference platforms remain safely budgeted:
    //   1000 (upstream) × 6.01 (slowest ratio) × 1.59 (safety) = 9548 → 10_000.
    mass_per_sig_op: 10000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    // kaspa-pq emission: there is no flat pre-deflationary phase — the decay
    // table in `SUBSIDY_BY_MONTH_TABLE` (15B over 20 years at 5%/yr) applies from
    // genesis, so `deflationary_phase_daa_score` is 0. That makes
    // `pre_deflationary_phase_base_subsidy` unused by `calc_block_subsidy`; it is
    // kept equal to the year-1 per-block subsidy at 10 BPS (table[0].div_ceil(10)
    // = 370_468_345 sompi ≈ 3.70468 KAS) so callers reading it see the genesis rate.
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 370468345,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,

    // PALW public testnet: ONE BLOCK PER 120 SECONDS. Sized from the fleet's measured per-header
    // replay cost so the slowest honest validator stays near 15 % steady-state load today and
    // still fits the future 35B-A3B algo fork (the model can fork on this chain; the block rate
    // cannot). See `new_two_minute_bps` and the runbook's §"Why 120 s".
    blockrate: BlockrateParams::new_two_minute_bps(),

    // kaspa-pq: this field only feeds the subsidy-month calc's pre-activation arm, which the
    // always-active crescendo below makes unreachable on this chain.
    pre_crescendo_target_time_per_block: 100,

    // PALW re-genesis: the chain restarts at DAA 0 with a single 10 s/block era, so the emission
    // schedule is on the post-activation (0.1-bps) arm from genesis — the historical score
    // 88_657_000 belonged to the superseded pre-PALW chain and would have kept emission on the
    // 10-bps table for the first ~28 years of the new chain.
    crescendo_activation: ForkActivation::always(),
    // kaspa-pq: TESTNET inherits mainnet's production overlay economics (14-day
    // unbonding/evidence window, PoS-v2 active, 2-D dominance reorg gate) but with
    // testnet-friendly thresholds (see TESTNET_DNS_PARAMS): a lowered
    // `required_work_depth` (100) so the 2-D DNS gate confirms at Argon2id's floored
    // CPU difficulty, and 10-KAS `min_bond`/`min_active_stake` so a single
    // premine-backed validator can drive finality. Not a genesis-block input, so the
    // genesis hash is unchanged.
    dns_params: Some(TESTNET_DNS_PARAMS),
    palw_credit: None,
    palw_fork_choice: None,
    palw_schedule: None,
    palw_ramp: None,
    palw_block_commitment: None,
    palw_bootstrap_activation: None,
    palw_unavailable_abstains: None,
    palw_bond_maturity: None,
    palw_bond_maturity_early: None,
    palw_frontier_provenance: None,
    palw_reorg_strict_economic_win: None,         // lane: rcore/f1-forkchoice-attacks
    palw_pruning_proof_strict_economic_win: None, // lane: rcore/hf-pptake2
    palw_capacity_weight_cap: None,               // lane: rcore/cap-weight (ADR-0160 F-W)
    palw_heartbeat: None,
    palw_attempt_work: None,
    // ADR-0072's activation fence ships dormant: this preset runs the execution-priced rule at
    // every height on algo-6, exactly as it does today, and arming the fence is the only thing
    // that would change a block.
    palw_attempt_activation: None,
    dns_bft_gate: None,
    palw_inactivity_leak: None,
    palw_beacon_fold: None,
    palw_capability_bound: None,
    palw_compute_overlay_retired: None,
    palw_model_registry: None,
    palw_economic_payout: None,
    palw_work_target: None,
    palw_single_lottery: None,
    palw_short_challenge_window: None,
    palw_verification_v2: None,
    palw_verification_s3: None,
    palw_verification_s2: None,
    palw_readiness_v2: None,
    palw_anchor_clock: None,
    palw_clock_cursor: None,
    palw_clock_floor: None,
    palw_clock_lead_cap: None,
    palw_offence_attribution: None,
    palw_rcore_plus: None,
    palw_panel_seed_execution: None,
    palw_operator_anchor: None,
    palw_capacity_aggregate_liability: None,
    palw_rcore_conservative_classes: &[],
    palw_final_lock_full_collateral: None,
    palw_final_lock_life: None,
    palw_final_lock_life_retro: None,
    palw_class_verify_deadline: None,
    palw_class_verify_rows: &[],
    palw_anchor_at_ceiling: None,
    palw_lane_accept_parents_first: None,
    palw_activation_pool: None,
    palw_readiness_v2_max_age_spans: None,
    palw_registry_resilience: None,
    palw_floor_refusal_retry: None,
    palw_artifact_root_ownership: None,
    palw_operator_id_unique: None,
    palw_objective_offence: None,
    palw_seat_gate_possession: None,
    palw_class_receipt_window: None,
    palw_execution_quanta: None,
    palw_public_model_source_required: None,
    palw_economic_safety: None,
    palw_canonical_work: None,
    palw_admission_independence: None,
    palw_context_ladder: None,
    // ADR-0077 Decision 16: `PanelDa` is dormant. A prompt that stays off chain is a mode a
    // network arms on purpose, never one it acquires by upgrading.
    palw_panel_da: None,
    palw_certification_rent: None,
    // ADR-0069 Decision 7: dormant. Arming it is a per-network activation decision.
    palw_uncertified_weightless: None,
    palw_da_court: None,
    palw_court_ladder: None,
    palw_model_market: None,
    palw_model_lines: None,
    palw_model_immutable_v1: None,
    palw_class_semantics_v1: None,
    palw_court_commitment_scope_v1: None,
    palw_staged_public_court_v1: None,
    palw_model_benefits: None,
    palw_model_leg_v2: None,
    palw_model_seed_v2: None,
    // ADR-0162: dormant on every preset.
    palw_model_virtual_v1: None,
    palw_model_evm: None,
    palw_model_sink_bound: None,
    palw_capacity_batch_licence: None,
    palw_capacity_verify_room: None,
    palw_capacity_audit_door: None,
    palw_capacity_issuance_slots: None,
    palw_capacity_network_room: None,
    palw_capacity_network_verify: None,
    palw_panel_unavailable_expiry: None,
    palw_panel_fast_switch: None,
    palw_seat_availability: None,
    palw_capacity_emission_budget: None,
    palw_capacity_multi_claim: None,
    palw_capacity_rho_breaker: None,
    palw_chunk_cap_charge: None,
    palw_prompt_ids_merkle: None,
    palw_kary_court: None,
    palw_model_court_window: None,
    palw_court_responder_coverage: None,
    palw_fp_decode_rules: None,
    palw_fp_derived_work: None,
    palw_fp_decode_constraint: None,
    palw_shard_court: None,
    palw_shard_licensing: None,
    palw_token_lift: None,
    palw_tir_v1: None,
    palw_gen_v1: None,
    palw_fp_job_v5: None,
    palw_gen_range_twin_v1: None,
    palw_task_heads_v1: None,
    palw_probabilistic_constraints_v1: None,
    palw_fork_choice_rule_e_v1: None,
    palw_typed_roots_v1: None,
    palw_fork_choice_commitment_v1: None,
    palw_verifier_pay_v1: None,
    palw_model_refutation_seal_v1: None,
    palw_beacon_source_opt_in_v1: None,
    palw_beacon_source_fee_v1: None,
    palw_panel_free_v1: None,
    palw_signed_registration_v1: None,
    palw_tir_shard_segment_v2: None,
    palw_provider_court_v1: None,
    palw_legacy_held_da_v2: None,
    palw_bond_budget_v1: None,
    palw_reporter_income_v2: None,
    palw_material_income_v3: None,
    palw_panel_lifetime_v2: None,
    palw_claim_challenge_v2: None,
    palw_monetary_material_v1: None,
    palw_model_verification_gate_v1: None,
    palw_horizon_extension_v1: None,
    palw_minimum_price_v1: None,
    palw_beacon_bootstrap_v1: None,
    palw_paid_consumer_lifecycle_v1: None,
    palw_model_bond_allocation_v1: None,
    palw_reporter_share_v2: None,
    palw_legacy_public_filer_v1: None,
    palw_court_model_scope_v1: None,
    palw_public_evidence_reward_v1: None,
    palw_claim_hard_deadline_v1: None,
    palw_fp_prefix_inherit: None,
    palw_fp_prefix_state: None,
    palw_fp_tokenizer_match: None,
    palw_fp_constraint_v2: None,
    palw_adapter_class_v1: None,
    palw_tir_fence2: None,
    palw_tir_shard_v1: None,
    palw_floor_reserve_v1: None,
    palw_real_clock_tick_v1: None,
    palw_anchor_window_v1: None,
    palw_receipt_spend_v4: None,
    palw_evidence_court_v1: None,
    palw_audit_1004_v1: None,
    palw_improvement_v1: None,
    palw_held_close_chunks_v1: None,
    palw_verification_vertex_v1: None,
    palw_exec_payload_v2: None,
    palw_witness_manifest_v1: None,
    palw_audit_mesh_v1: None,
    palw_capped_onboarding_v1: None,
    palw_class_seating: None,
    palw_tir_only_v1: None,
    palw_kimi_k3: None,
    palw_fused_dissectable: None,
    palw_attn_anchored_root: None,
    palw_held_context: None,
    palw_prefill_draw: None,
    palw_audit_2026_09_11: None,
    palw_audit_2026_09_11_deep: None,
    palw_audit_2026_09_23: None,
    palw_settled_anchor_depth: None,
    palw_admission_audit_period_daa: None,
    palw_exec_quantum_maturity_daa: None,
    palw_difficulty_priced_rows: None,
    palw_receipt_rows_unpriced: None,
    palw_attempt_header_pins: None,
    palw_signature_contexts_v2: None,
    palw_fp_da_pins: None,
    palw_validator_payout_bounds: None,
    palw_slashing_evidence_utxo_genuine: None,
    palw_epoch_boundary_budget: None,
    palw_epoch_budget_release: None,
    palw_panel_economy: None,
    palw_work_priced_reward: None,
    palw_execution_lane: None,
    palw_overlay_carve: None,
    palw_dns_retirement: None,
    palw_permissionless_panel_v1: None,
    palw_permissionless_panel_complete_v1: None,
    palw_permissionless_panel_retired_v1: None,
    palw_panel_exposure_floor: None,
    palw_fp_ruleset_caps: None,
    // ADR-0105: dormant on every shipped preset (see the field's doc).
    palw_heartbeat_transparent: None,
    // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency): dormant on every preset.
    palw_heartbeat_transparent_same_chain: None,
    palw_capacity_escrow_at_licence: None,
    // ADR-0107: dormant on every shipped preset (see the field's doc).
    palw_share_growth_final: None,
    palw_gdn_key_heads: None,
    palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode::Disabled,
    pow_blake2b_sha3_activation: ForkActivation::always(),
    // PALW LLM PoW: DISABLED on the public preset (2026-08-12). The Ollama flavor (algo_id = 5)
    // that shipped here is FORGEABLE WITHOUT RUNNING THE MODEL and must not be on a public
    // network for one more commit.
    //
    // Measured, on the pinned blob: the canonical prompt makes the model emit ONE constant
    // 16-token continuation for every seed (27/27 seeds, 22 of them uniform random). The tag is
    // therefore a 64-byte constant, a `prompt_eval_count` drawn from ~10 values, and a constant
    // 16 — so an attacker guesses it with ~31 BLAKE2b hashes and zero inference, at ~38 k
    // nonces/s in single-threaded Python, while an honest miner pays 12-26 s per attempt. The
    // verifier replays the real model, gets the same constant, and ACCEPTS the forgery: no
    // consensus rule is broken, the work is simply not there. Confirmable without any runtime —
    // the first 64 bytes of the calibration constant ARE the digest of that boilerplate string.
    //
    // The cause is structural, not a tuning error: the tag can only commit to the OUTPUT TEXT,
    // and a small model at temperature 0 collapses to a handful of attractors (measured
    // min-entropy ~3.1 bits over 60 seeds even with a high-entropy seed-derived prompt), while
    // Ollama exposes no per-token logprobs to bind instead. The replacement therefore binds the
    // LOGITS via the worker's `gemm_trace_root` and lands as `algo_id = 2` — see ADR-0021.
    //
    // Until that ships, testnet falls back to the Phase-3 BLAKE2b-SHA3 PoW below, which is
    // sound. This line is the safety switch; the redesign is the fix.
    pow_palw_activation: ForkActivation::never(),
    pow_palw_ollama_activation: ForkActivation::never(),
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020 (O13 activation): EVM lane GENESIS-ACTIVE on testnet — every
    // post-genesis header is v2 carrying the two EVM commitments, so the public
    // testnet exercises the full lane (relay / deposit-claim / withdraw bridge /
    // receipts) alongside Argon2id PoW + the PoS-finality overlay. NOT a
    // genesis-block input (genesis hash unchanged), but the version fork-gate
    // invalidates every v1 block => a barrier re-genesis of the testnet mesh, and
    // testnet kaspad MUST be built `--features evm` (a non-evm build refuses
    // evm-active blocks by design). Mainnet/simnet stay u64::MAX-inert.
    evm_activation_daa_score: 0,
    // EVM is genesis-active here; the gas-pool v2 executor (Ethereum/geth-style
    // sequential gas pool — a tx skipped over-cap no longer starves later/smaller
    // txs) activates at this testnet DAA. This is a consensus fork: every mesh node
    // MUST run a v2-capable `--features evm` binary BEFORE this score, or the EVM
    // state commitment splits. Set 2026-06-21 to ~90 min ahead of the live virtual
    // DAA (~2.102M) to cover the rolling mesh swap. Mainnet/simnet/devnet stay inert.
    evm_gas_pool_v2_activation_daa_score: 2_125_000,
    // M-03 withdrawal cap: inert (u64::MAX) — its activation is a separate coordinated deploy.
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_bridge_ledger_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

pub const SIMNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Simnet),
    genesis: SIMNET_GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    past_median_time_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,

    // kaspa-pq emission: decay table applies from genesis (see MAINNET_PARAMS).
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 370468345,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    max_signature_script_len: 16_384,
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    // kaspa-pq Phase 7 (ML-DSA-87 verify recalibration; supersedes the
    // Phase-6 ML-DSA-87 numbers). Measured on Apple Silicon arm64 via
    // `crypto/txscript/benches/bench.rs` (ml_dsa_87::verify):
    //   Schnorr verify (secp256k1):              12.74 µs
    //   ML-DSA-87 verify (default, NEON/AVX2):   63.88 µs  (5.01× ratio)
    //   ML-DSA-87 verify (libcrux portable):     76.52 µs  (6.01× ratio — slowest)
    //
    // Per `docs/adr/0005-mass-policy.md` §"Calibration formula" the
    // value is calibrated against the slowest variant so that no-SIMD
    // low-end reference platforms remain safely budgeted:
    //   1000 (upstream) × 6.01 (slowest ratio) × 1.59 (safety) = 9548 → 10_000.
    mass_per_sig_op: 10000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    skip_proof_of_work: true, // For simnet only, PoW can be simulated by default
    max_block_level: 250,
    pruning_proof_m: PRUNING_PROOF_M,

    // For simnet, we deviate from default 10BPS configuration and allow at least 64 parents in order to support mempool benchmarks out of the box
    blockrate: BlockrateParams::new::<10>().increase_max_block_parents(64),

    pre_crescendo_target_time_per_block: TenBps::target_time_per_block(),

    crescendo_activation: ForkActivation::always(),
    // kaspa-pq: DNS-finality PoS overlay genesis-active on every network (see
    // GENESIS_ACTIVE_DNS_PARAMS). Not a genesis-block input, so the genesis hash is unchanged.
    dns_params: Some(GENESIS_ACTIVE_DNS_PARAMS),
    palw_credit: None,
    palw_fork_choice: None,
    palw_schedule: None,
    palw_ramp: None,
    palw_block_commitment: None,
    palw_bootstrap_activation: None,
    palw_unavailable_abstains: None,
    palw_bond_maturity: None,
    palw_bond_maturity_early: None,
    palw_frontier_provenance: None,
    palw_reorg_strict_economic_win: None,         // lane: rcore/f1-forkchoice-attacks
    palw_pruning_proof_strict_economic_win: None, // lane: rcore/hf-pptake2
    palw_capacity_weight_cap: None,               // lane: rcore/cap-weight (ADR-0160 F-W)
    palw_heartbeat: None,
    palw_attempt_work: None,
    // ADR-0072's activation fence ships dormant: this preset runs the execution-priced rule at
    // every height on algo-6, exactly as it does today, and arming the fence is the only thing
    // that would change a block.
    palw_attempt_activation: None,
    dns_bft_gate: None,
    palw_inactivity_leak: None,
    palw_beacon_fold: None,
    palw_capability_bound: None,
    palw_compute_overlay_retired: None,
    palw_model_registry: None,
    palw_economic_payout: None,
    palw_work_target: None,
    palw_single_lottery: None,
    palw_short_challenge_window: None,
    palw_verification_v2: None,
    palw_verification_s3: None,
    palw_verification_s2: None,
    palw_readiness_v2: None,
    palw_anchor_clock: None,
    palw_clock_cursor: None,
    palw_clock_floor: None,
    palw_clock_lead_cap: None,
    palw_offence_attribution: None,
    palw_rcore_plus: None,
    palw_panel_seed_execution: None,
    palw_operator_anchor: None,
    palw_capacity_aggregate_liability: None,
    palw_rcore_conservative_classes: &[],
    palw_final_lock_full_collateral: None,
    palw_final_lock_life: None,
    palw_final_lock_life_retro: None,
    palw_class_verify_deadline: None,
    palw_class_verify_rows: &[],
    palw_anchor_at_ceiling: None,
    palw_lane_accept_parents_first: None,
    palw_activation_pool: None,
    palw_readiness_v2_max_age_spans: None,
    palw_registry_resilience: None,
    palw_floor_refusal_retry: None,
    palw_artifact_root_ownership: None,
    palw_operator_id_unique: None,
    palw_objective_offence: None,
    palw_seat_gate_possession: None,
    palw_class_receipt_window: None,
    palw_execution_quanta: None,
    palw_public_model_source_required: None,
    palw_economic_safety: None,
    palw_canonical_work: None,
    palw_admission_independence: None,
    palw_context_ladder: None,
    // ADR-0077 Decision 16: `PanelDa` is dormant. A prompt that stays off chain is a mode a
    // network arms on purpose, never one it acquires by upgrading.
    palw_panel_da: None,
    palw_certification_rent: None,
    // ADR-0069 Decision 7: dormant. Arming it is a per-network activation decision.
    palw_uncertified_weightless: None,
    palw_da_court: None,
    palw_court_ladder: None,
    palw_model_market: None,
    palw_model_lines: None,
    palw_model_immutable_v1: None,
    palw_class_semantics_v1: None,
    palw_court_commitment_scope_v1: None,
    palw_staged_public_court_v1: None,
    palw_model_benefits: None,
    palw_model_leg_v2: None,
    palw_model_seed_v2: None,
    // ADR-0162: dormant on every preset.
    palw_model_virtual_v1: None,
    palw_model_evm: None,
    palw_model_sink_bound: None,
    palw_capacity_batch_licence: None,
    palw_capacity_verify_room: None,
    palw_capacity_audit_door: None,
    palw_capacity_issuance_slots: None,
    palw_capacity_network_room: None,
    palw_capacity_network_verify: None,
    palw_panel_unavailable_expiry: None,
    palw_panel_fast_switch: None,
    palw_seat_availability: None,
    palw_capacity_emission_budget: None,
    palw_capacity_multi_claim: None,
    palw_capacity_rho_breaker: None,
    palw_chunk_cap_charge: None,
    palw_prompt_ids_merkle: None,
    palw_kary_court: None,
    palw_model_court_window: None,
    palw_court_responder_coverage: None,
    palw_fp_decode_rules: None,
    palw_fp_derived_work: None,
    palw_fp_decode_constraint: None,
    palw_shard_court: None,
    palw_shard_licensing: None,
    palw_token_lift: None,
    palw_tir_v1: None,
    palw_gen_v1: None,
    palw_fp_job_v5: None,
    palw_gen_range_twin_v1: None,
    palw_task_heads_v1: None,
    palw_probabilistic_constraints_v1: None,
    palw_fork_choice_rule_e_v1: None,
    palw_typed_roots_v1: None,
    palw_fork_choice_commitment_v1: None,
    palw_verifier_pay_v1: None,
    palw_model_refutation_seal_v1: None,
    palw_beacon_source_opt_in_v1: None,
    palw_beacon_source_fee_v1: None,
    palw_panel_free_v1: None,
    palw_signed_registration_v1: None,
    palw_tir_shard_segment_v2: None,
    palw_provider_court_v1: None,
    palw_legacy_held_da_v2: None,
    palw_bond_budget_v1: None,
    palw_reporter_income_v2: None,
    palw_material_income_v3: None,
    palw_panel_lifetime_v2: None,
    palw_claim_challenge_v2: None,
    palw_monetary_material_v1: None,
    palw_model_verification_gate_v1: None,
    palw_horizon_extension_v1: None,
    palw_minimum_price_v1: None,
    palw_beacon_bootstrap_v1: None,
    palw_paid_consumer_lifecycle_v1: None,
    palw_model_bond_allocation_v1: None,
    palw_reporter_share_v2: None,
    palw_legacy_public_filer_v1: None,
    palw_court_model_scope_v1: None,
    palw_public_evidence_reward_v1: None,
    palw_claim_hard_deadline_v1: None,
    palw_fp_prefix_inherit: None,
    palw_fp_prefix_state: None,
    palw_fp_tokenizer_match: None,
    palw_fp_constraint_v2: None,
    palw_adapter_class_v1: None,
    palw_tir_fence2: None,
    palw_tir_shard_v1: None,
    palw_floor_reserve_v1: None,
    palw_real_clock_tick_v1: None,
    palw_anchor_window_v1: None,
    palw_receipt_spend_v4: None,
    palw_evidence_court_v1: None,
    palw_audit_1004_v1: None,
    palw_improvement_v1: None,
    palw_held_close_chunks_v1: None,
    palw_verification_vertex_v1: None,
    palw_exec_payload_v2: None,
    palw_witness_manifest_v1: None,
    palw_audit_mesh_v1: None,
    palw_capped_onboarding_v1: None,
    palw_class_seating: None,
    palw_tir_only_v1: None,
    palw_kimi_k3: None,
    palw_fused_dissectable: None,
    palw_attn_anchored_root: None,
    palw_held_context: None,
    palw_prefill_draw: None,
    palw_audit_2026_09_11: None,
    palw_audit_2026_09_11_deep: None,
    palw_audit_2026_09_23: None,
    palw_settled_anchor_depth: None,
    palw_admission_audit_period_daa: None,
    palw_exec_quantum_maturity_daa: None,
    palw_difficulty_priced_rows: None,
    palw_receipt_rows_unpriced: None,
    palw_attempt_header_pins: None,
    palw_signature_contexts_v2: None,
    palw_fp_da_pins: None,
    palw_validator_payout_bounds: None,
    palw_slashing_evidence_utxo_genuine: None,
    palw_epoch_boundary_budget: None,
    palw_epoch_budget_release: None,
    palw_panel_economy: None,
    palw_work_priced_reward: None,
    palw_execution_lane: None,
    palw_overlay_carve: None,
    palw_dns_retirement: None,
    palw_permissionless_panel_v1: None,
    palw_permissionless_panel_complete_v1: None,
    palw_permissionless_panel_retired_v1: None,
    palw_panel_exposure_floor: None,
    palw_fp_ruleset_caps: None,
    // ADR-0105: dormant on every shipped preset (see the field's doc).
    palw_heartbeat_transparent: None,
    // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency): dormant on every preset.
    palw_heartbeat_transparent_same_chain: None,
    palw_capacity_escrow_at_licence: None,
    // ADR-0107: dormant on every shipped preset (see the field's doc).
    palw_share_growth_final: None,
    palw_gdn_key_heads: None,
    palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode::Disabled,
    pow_blake2b_sha3_activation: ForkActivation::never(),
    // PALW LLM PoW: simnet keeps instant local kHeavyHash (simulation/tests must not need a model).
    pow_palw_activation: ForkActivation::never(),
    pow_palw_ollama_activation: ForkActivation::never(),
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020: EVM lane inert in P1 (no executor yet); the testnet value flips to
    // a finite activation score when the revm executor lands (P2+). u64::MAX = never.
    evm_activation_daa_score: u64::MAX,
    // gas-pool v2 ships inert on every network — a deploy sets a finite testnet score.
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_bridge_ledger_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

/// MISAKA PALW staging net (`testnet-11`): the public-testnet SHAPE — the same 120 s
/// blockrate, windows and overlay params as `TESTNET_PARAMS` — with PALW LLM PoW
/// (`algo_id = 4`, the worker/full-logits flavor) ACTIVE from genesis and the hash lane
/// off, on the pre-scaled `TESTNET11_GENESIS` bits (`0x200ccccc` ≈ 10× the trivial
/// target: near the x86 fleet's expected converged difficulty, so the fixed-difficulty
/// launch window mints at roughly the target cadence instead of bursting — the gate-3
/// finding this preset applies).
///
/// This is the public PALW staging net. Its fingerprint/genesis differ from both live t10
/// and devnet, so it handshake-rejects strangers by construction. Peer discovery is kept
/// outside consensus and uses the project's DNS seeders; operators must not bake a resolved
/// seeder IP into a permanent configuration because the answer set is operational and rotates.
pub const TESTNET11_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Testnet, 11),
    genesis: TESTNET11_GENESIS,
    dns_seeders: &["seeder1.misakascan.com", "seeder2.misakascan.com", "seeder3.misakascan.com", "seeder4.misakascan.com"],
    // PALW-only lane, devnet's shape: every post-genesis header declares algo 4 and is
    // validated by replaying one pinned inference. The Ollama flavor stays off — the
    // fleet's calibrated class is the WORKER (x86 CPU) class, measured 4/4 in gate 2.
    pow_blake2b_sha3_activation: ForkActivation::never(),
    pow_palw_activation: ForkActivation::always(),
    pow_palw_ollama_activation: ForkActivation::never(),
    // ADR-0042 Decision 1: t11 is the LEGACY PALW lineage, and since the Relaunch-2 re-genesis
    // (2026-08-20) it says so in the mode enum — the fingerprint already moved for the community
    // allocation, so marking the mode rode the same flag day for free. `LegacyTn11` changes no
    // rule (the legacy knobs above carry the values); it makes the lineage a handshake fact.
    palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode::LegacyTn11,
    ..TESTNET_PARAMS
};

/// **The PALW-RC network (ADR-0036 Decision 2, ADR-0042 PR-10) — a FUNCTION, not a `const`.**
///
/// A `ConsensusV2` bundle carries a catalog root, a court shape and a genesis registration list,
/// and its construction can FAIL (`validate`); a `const` cannot hold a fallible construction, and
/// faking one with `unwrap()` in a static would turn a bad ruleset into a panic at first use
/// instead of an error at the call site. So the RC network is built and checked, and a caller
/// holding an `Ok` holds params a node will start on.
///
/// **What it is:** testnet suffix 12, at PALW's frozen 120 s cadence, `ConsensusV2` from genesis,
/// with no V1 PALW proof-of-work (a V2 network may activate none — which is why this could not be
/// a re-flag of testnet-10 or -11) and its own genesis block.
///
/// **What it is NOT: mainnet.** ADR-0042 says window sizes, the share table, ρ_r, k, bonds and
/// `base(C)` are soak outputs, filled in "after Chaosnet + RC, in a later ADR". This carries the
/// free-prompt devnet bundle's numbers, which are honest devnet numbers and are not calibrated
/// for a network holding value. Shipping mainnet numbers today would mean inventing them.
///
/// The `finality_depth` is the one thing this must fix beyond the cadence: PALW's schedule
/// requires `finality_depth < w_challenge`, and testnet's 10-BPS depth fails it by orders of
/// magnitude. It is the second of the two independent refusals
/// `the_shipped_mainnet_identity_cannot_carry_a_palw_schedule` measures.
/// **The RC network from the ONE thing code cannot mint, plus the operator's own identities.**
///
/// `palw_rc_params` takes nine arguments because a bundle needs nine facts. Eight of them are
/// derivable — the class id is its graph's `shape_profile_id`, the catalog root is over a catalog
/// counted from that graph, `pwu_per_inference` is that count, and the court catalog root is this
/// build's own adjudication table. Passing them separately is nine places a number could be
/// chosen twice, and ADR-0042 Decision 11's whole claim is that RC and mainnet agree by hash.
///
/// So this derives all eight and takes only what it cannot: the BASE-0 artifact root (the int8
/// weights, the requantization parameters and the pinned sin/cos table, which somebody produces)
/// and the genesis bond's identities.
///
/// **The genesis bond set is a list for a reason.** `derive_panel_v2` excludes the executor's own
/// bond, its operator and its key, so a registry with one bond can never seat a panel and every
/// claim on such a network would void at `BindTimeout`. A network that carries weight registers
/// its initial validator set here so the FIRST panel can seat. Bonds can also enter through a
/// transaction — `palw_bond_registration_binds_its_carrier_v2` is the collateral lock that makes
/// that safe — but a network still needs enough of them at genesis to seat a panel before anyone
/// has had a chance to register one.
pub fn palw_rc_params_from_artifacts(
    base0_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    palw_v2_params_from_artifacts_on_base(palw_rc_base_params(), base0_artifact_root, genesis_bonds)
}

/// **[`palw_rc_params_from_artifacts`] over an arbitrary base identity.**
///
/// The whole point of routing every network through one function is that the genesis GATE runs on
/// each of them. `verify_palw_genesis_v2` needs the catalog PREIMAGE, which only this entry point
/// derives, and it resolves each bond's collateral against `genesis_premine_utxos_for(params.net)`
/// — THIS network's own premine. So a base whose genesis does not actually hold the collateral its
/// cards declare is refused here, at assembly, rather than by the first node to boot it.
pub fn palw_v2_params_from_artifacts_on_base(
    base: Params,
    base0_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    let genesis_utxos = crate::config::premine::genesis_premine_utxos_for(base.net);
    palw_v2_params_from_artifacts_on_base_with_utxos(base, base0_artifact_root, genesis_bonds, genesis_utxos)
}

/// [`palw_v2_params_from_artifacts_on_base`] against a SUPPLIED genesis UTXO set.
///
/// The genesis gate resolves each bond's declared collateral against the network's own premine, so
/// on every shipped path that set is `genesis_premine_utxos_for(net)` and this seam is invisible.
/// It exists so a network whose premine does not carry bond collateral YET — mainnet, whose card
/// list is empty — can still be assembled and gated in a test, against a premine built the way its
/// real one would be. Without that, the mainnet wiring could only be checked by shipping it.
pub fn palw_v2_params_from_artifacts_on_base_with_utxos(
    base: Params,
    base0_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
    genesis_utxos: crate::utxo::utxo_collection::UtxoCollection,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    let (profile, catalog) = crate::palw_base0_profile::palw_rc_base0_registration_v1(base0_artifact_root)
        .map_err(|_| crate::palw_mode_v2::PalwModeV2Error::Invalid("BASE-0's registration does not derive"))?;
    let class_id = profile.shape_profile_id();
    let entry = catalog.entries().first().expect("the RC catalog has its one class");
    let mut params = palw_v2_params_on_base(
        base,
        class_id,
        catalog.root(),
        crate::palw_catalog_coverage::palw_court_catalog_root_v1(),
        entry.canonical_step_leaf_count,
        base0_artifact_root,
        genesis_bonds,
    )?;
    // **The floor is seeded by the same rule as any other class** (ADR-0076), and a network that
    // registers ONLY the floor is the case the rule exists to protect. Its class target cannot set
    // cadence — with one class the retarget is a deliberate no-op and the interval is `bits`'
    // job — so the seed's whole job here is to be an ANCHOR: mainnet mints floor-only and every
    // model class it will ever hold arrives afterwards, by registration. A floor left at half the
    // ticket space leaves a factor of two above it, and no class dearer per unit of share than the
    // floor could ever be seeded into the room that is not there. Seeded, the floor sits at
    // `MAX/278` and the room above it is the 278× a model tier needs.
    {
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
            unreachable!("palw_v2_params_on_base installs a ConsensusV2 bundle or returns Err");
        };
        palw_seed_attempt_targets_v1(bundle)?;
    }
    // **The genesis gate, on the path that can actually run it.**
    //
    // `verify_palw_genesis_v2` is the boot gate for a `ConsensusV2` artifact, and it needs the
    // catalog PREIMAGE — which is exactly what this entry point derives and `palw_rc_params`
    // (which takes only a root) cannot. So it ran nowhere outside its own tests, and the checks it
    // holds — the catalog is the committed one, every registration agrees with it, and now that a
    // registered bond's collateral is money the genesis UTXO set really holds (audit C-08) —
    // gated nothing that ships.
    //
    // The outpoint resolver is THIS network's genesis set, so "the bond holds its collateral" is
    // a statement about the same UTXO commitment the genesis block carries, not about a set
    // supplied alongside it.
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        unreachable!("palw_v2_params_on_base installs a ConsensusV2 bundle or returns Err");
    };
    crate::palw_genesis_v2::verify_palw_genesis_v2_with_clock_v1(
        bundle,
        &catalog,
        &bundle.genesis_objects,
        |outpoint| genesis_utxos.get(outpoint).map(|entry| entry.amount),
        // ADR-0151 D3 — read from THIS network's fences, never assumed.
        palw_clock_advances_without_a_claim_v1(&params),
    )?;
    // **The last transform a booting node applies, applied here too.**
    //
    // `From<NetworkId> for Params` wraps its whole match in `with_registered_models`, so a node's
    // params — and therefore its `consensus_params_id` — carry the model cost table. This function
    // is documented as "the call a node makes at boot" and every caller trusts that: the genesis
    // TOOL printed an id no node would ever log (`c4a381f6…` against the node's `9d0cc709…`), which
    // an operator comparing the two reads as a fleet mismatch. Idempotent by construction — the
    // wrapper fills the table only when it is empty — so the outer application stays a no-op.
    Ok(with_registered_models(params))
}

/// **The RC network with the Qwen3.6 class registered from genesis** — the two-class form of
/// [`palw_rc_params_from_artifacts`], for a network minted after the class's weights exist.
///
/// The base assembly runs first and unchanged, so the floor's registration, the bond gates and
/// the panel gate are exactly the shipped ones. Then the second class is added the way the
/// transition itself defines a later entrant: registered at the bundle's OWN minimum grantable
/// share (one permille, donated from the floor by largest-remainder — the transition's write, not
/// this function's), with the floor's slash value and initial target, because a class that chose
/// its own economics would be choosing its weight. The catalog becomes two entries and its root —
/// which is inside the ruleset id — moves, so this is a DIFFERENT network identity from the
/// one-class RC, as it must be: a node that can adjudicate two classes and one that knows of one
/// would otherwise wear one name.
///
/// The genesis gate then re-runs over the extended object list against the two-entry catalog:
/// every check the one-class network passes, the two-class network must pass again, including the
/// bind-window arithmetic that sizes the bonds against the FLOOR's per-claim reservation (the
/// entrant's pwu is higher, so its claims reserve more and fit fewer concurrently — a bond that
/// carries the floor's window still carries a thinner window of Qwen3.6 claims, which is the
/// correct consequence of a bigger job, not a gate failure).
pub fn palw_rc_params_with_qwen36(
    base0_artifact_root: crate::Hash64,
    qwen36_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    palw_rc_params_with_classes(base0_artifact_root, qwen36_artifact_root, None, genesis_bonds)
}

/// The per-inference work a class registration declares, or zero for anything else — the input the
/// genesis bond sizing takes its maximum over.
fn genesis_pwu_of(object: &crate::palw_state_v2::PalwConsensusObjectV2) -> u64 {
    match object {
        crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } => match pwu_rule {
            crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
            crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
        },
        _ => 0,
    }
}

/// **The cadence the hybrid tier holds when the network starts** (permille of the share table).
///
/// Not the grant floor: see the note inside `palw_rc_params_with_classes`. Two hundred permille is
/// one block in five, which is a tier a public network can actually watch — and it is small enough
/// that a tier nobody runs costs the chain nothing, because the retarget's census renormalizes over
/// the classes that PRODUCED (audit H1) and ADR-0054 decays a silent class back into the floor.
///
/// **This is a bootstrap, not a ceiling.** ADR-0054 lets any class earn cadence afterwards by
/// filling its budget, so a tier funded at the grant floor is not stuck there — it is 23 closed
/// epochs from 200‰ (1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 15, 18, 22, 27, 33, 41, 51, 63, 78, 97, 121,
/// 151, 188, 235), which at testnet-11's measured ~30 h epoch is about four weeks of uninterrupted
/// production. What genesis funding buys is that month, and a public network that shows what it is
/// on its first day instead of in its second month.
///
/// Changing it changes `palw_ruleset_id_v2` — the genesis object list is inside the bundle — so it
/// is a mint-time decision. Nothing raises a share by fiat afterwards; production does.
///
/// # Zeroed by the ADR-0068 launch audit, and restored when the family earned it back
///
/// The audit found this line handing 489‰ + 489‰ — 97.8% of cadence — to two families nothing
/// could convict: both backends took the trait defaults for `bisect_prefix_state` and
/// `refutation_for_index`, so a dispute over one of their claims could not leave round 0 whichever
/// party was honest, and `Qwen36Backend`'s own module header said so and drew ADR-0039's
/// conclusion. "The block was paid for by actual inference" was an unenforced sentence, so the
/// share went to zero — with the reason recorded that weight would return by ACTIVATION once the
/// family certified end-to-end (ADR-0069 Decision 6).
///
/// **It certified.** The step spaces adjudicate (ADR-0070: every leaf swept rather than sampled),
/// the family is in this build's `court_e2e_root`, and the admission gate would refuse a nonzero
/// share here if it were not — which is what makes restoring this a consequence rather than a
/// decision. The funding argument in the paragraphs above was always correct about a class this
/// chain can prosecute; this is now one.
pub const PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE: u16 = 489;

/// **The dense tier's, at genesis.** Equal to the hybrid's on purpose: the two differ enormously in
/// what they cost to run, and nothing about that difference is knowable at mint time. ADR-0054 is
/// what separates them afterwards, out of what they each produce.
///
/// **490/490 since ADR-0068 Phase 2** (was 200/200): with the heartbeat clock armed from genesis,
/// the floor retires to its 20‰ reserve and the model tiers hold the table — the LLM-primary
/// economy the ADR is for. Blocks and issuance follow this table (ADR-0045: budgets are blocks,
/// the subsidy carve is class-blind), so this line IS the "LLM ≈ 98% of issuance" decision.
///
/// # Zeroed by the launch audit, and restored on the same terms as the hybrid
///
/// The dense tier was zeroed for the hybrid's reason and was always closer to the exit: the A16
/// capture bridge already existed, and what was missing was the checkpoint leg and the two rung
/// methods. Those landed (ADR-0070), the family certifies end-to-end, and the admission gate
/// enforces the rest — a nonzero share here is refused unless this build's `court_e2e_root`
/// carries the family, so this line cannot claim more than the certificate supports.
///
/// Weight follows the certificate, not the intent. It did when the number was zero and it does now.
///
/// # And since ADR-0082 this number funds the graph-v5 512 row, whose family is NOT yet certified
///
/// The paragraph above is about the graph-v2 row and is now half true of the row this share funds.
/// The dense slot holds ADR-0082's fused 512 row (5f card §2), and the RC's pinned certified set
/// gives `PALW-QWEN25-A16` the kernel set of the graph-v2 profile — which does not reach
/// `KDESC_A16_ATTN_FUSED`. So `verify_class_admission_v5` refuses this share by name
/// (`NotEndToEndCertified { share: 489 }`), measured by
/// `misaka/palw/base0/tests/a16_root_probe.rs::the_genesis_registered_row_is_admissible_under_the_armed_rc_court`.
///
/// A GENESIS registration does not pass through that gate — it is verified against the committed
/// catalog — so this number is granted and the class holds the cadence. What it does not hold is
/// weight: `palw_uncertified_weightless` is the rule that reads the certificate at production time.
/// Closing it is ADR-0082 stream I's court drill over a fused class plus the
/// `PALW_RC_COURT_E2E_ROOT_BYTES` re-pin that records it — the 5f card §6 calls that drill "the
/// gate, and admission is not". Until then this line allocates cadence to a class that cannot yet
/// convert it into weight, which is a launch fact and not a silent one.
pub const PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE: u16 = 489;

/// The same assembly, with the A16 dense class when its root is pinned. `None` is the two-class
/// network exactly as before — an unpinned class is absent, never a placeholder.
/// **[`palw_v2_params_with_classes_on_base`] on the RC's own base identity.**
///
/// The RC spelling, kept because every caller of it means the RC. The base is a parameter one
/// level down for the reason every other assembly takes one: mainnet is a different base and the
/// SAME economy, and a class-registering path that could only be spelled for the RC is a path
/// mainnet does not have (mainnet audit — `mainnet_shipped_params` assembled through the
/// class-FREE builder while its own comment described the ADR-0068 share table, so a carded
/// mainnet would have been born a floor-only network with the model tiers' 980‰ nowhere to go).
pub fn palw_rc_params_with_classes(
    base0_artifact_root: crate::Hash64,
    qwen36_artifact_root: crate::Hash64,
    qwen25_a16_artifact_root: Option<crate::Hash64>,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    palw_v2_params_with_classes_on_base(
        palw_rc_base_params(),
        base0_artifact_root,
        qwen36_artifact_root,
        qwen25_a16_artifact_root,
        genesis_bonds,
    )
}

/// The floor plus the model tiers, over an arbitrary base identity.
pub fn palw_v2_params_with_classes_on_base(
    base: Params,
    base0_artifact_root: crate::Hash64,
    qwen36_artifact_root: crate::Hash64,
    qwen25_a16_artifact_root: Option<crate::Hash64>,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    let genesis_utxos = crate::config::premine::genesis_premine_utxos_for(base.net);
    palw_v2_params_with_classes_on_base_with_utxos(
        base,
        base0_artifact_root,
        qwen36_artifact_root,
        qwen25_a16_artifact_root,
        genesis_bonds,
        genesis_utxos,
    )
}

/// **[`palw_v2_params_with_classes_on_base`] against a SUPPLIED genesis UTXO set** — the same
/// seam, for the same reason, as `palw_v2_params_from_artifacts_on_base_with_utxos` above it: a
/// network whose premine does not carry its card's bond collateral YET (mainnet, whose
/// `PALW_MAINNET_GENESIS_BONDS` is empty) can still be assembled and gated in a test, against a
/// premine built the way its real one would be. Without it the class-registering card — the one a
/// real mainnet mint runs — could only be checked by shipping it (mainnet audit 2026-09-06, M-7).
pub fn palw_v2_params_with_classes_on_base_with_utxos(
    base: Params,
    base0_artifact_root: crate::Hash64,
    qwen36_artifact_root: crate::Hash64,
    qwen25_a16_artifact_root: Option<crate::Hash64>,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
    genesis_utxos: crate::utxo::utxo_collection::UtxoCollection,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    palw_v2_params_with_class_rows_v1(
        base,
        base0_artifact_root,
        genesis_bonds,
        genesis_utxos,
        PalwGenesisClassRowsV1::Rc { qwen36_artifact_root, qwen25_a16_artifact_root },
    )
}

/// **Which ROWS of the two model families a genesis registers**, and the artifact root each binds.
///
/// A class is its graph AND its context (`n_ctx` is inside `shape_profile_id`), so "register Qwen2.5"
/// is not a complete instruction: the width and the map have to be named. Two sets ship.
///
/// The variants are not interchangeable and the difference is measured, not stylistic: a row under
/// the graph-v5 / graph-v3 maps is bounded by `n_ctx × layer_count ≤ PALW_STEP_MAX_ENUMERATION`, so
/// the dense family stops deriving above `n_ctx` 524,288 and the hybrid above 131,072. A row under
/// the HELD map (ADR-0103 Decision 6) is bounded per POSITION instead, which is the only reason a
/// 2M row exists at all.
///
/// **Each variant carries its own roots.** The roots used to be positional arguments beside the row
/// set, and a held set with no hybrid row would have had to accept a hybrid root it never reads — an
/// argument nothing reads is one no test can catch being wrong, and a wrong root in this card has
/// already shut a tier twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenesisClassRowsV1<'a> {
    /// **testnet-11's set**: the hybrid's graph-v3 row at its frozen `n_ctx` 8 and the dense
    /// graph-v5 row at 512. Live chain facts — a registered class id can never be re-pointed — so
    /// this variant is frozen with them.
    Rc { qwen36_artifact_root: crate::Hash64, qwen25_a16_artifact_root: Option<crate::Hash64> },
    /// **A held set**: graph-v7 rows (the held map), each at its own width over its own artifact,
    /// registered in the order given. testnet-12 ships [`PALW_T12_GENESIS_HELD_ROWS`].
    ///
    /// A held row needs `palw_held_context` armed, and a hybrid row also needs `palw_token_lift`;
    /// both are armed from genesis on the one preset that uses this, so the genesis gate is what
    /// proves the pairing rather than a comment.
    Held(&'a [PalwGenesisHeldRowV1]),
}

/// **Which family a held genesis row belongs to** — the two families whose graph-v7 rows derive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenesisHeldFamilyV1 {
    /// Qwen2.5-1.5B A16, `qwen25_a16_held_registration_v1`.
    DenseA16,
    /// Qwen3.6-35B-A3B, `qwen36_held_registration_v1`.
    HybridQwen36,
}

/// **One held row a genesis registers: the family, the width, and the root it binds.**
///
/// The root travels WITH its row because a second width of one family is a second artifact, not a
/// second pointer at the first one's file: a row needs a rotary table covering its whole context,
/// so each width is its own conversion with its own inventory root, and ADR-0143 gives every root
/// exactly one owner on chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenesisHeldRowV1 {
    pub family: PalwGenesisHeldFamilyV1,
    pub n_ctx: u32,
    pub artifact_root: crate::Hash64,
}

/// [`palw_v2_params_with_classes_on_base_with_utxos`] with the row set named. Everything except how a
/// row is BUILT is shared: the floor check over every donation, the collateral and capability
/// re-derivation over the registered tiers, the catalog's two orders, the per-class attempt-target
/// seeding and both runs of the genesis gate. A second copy of any of that would be a second place for
/// it to drift, which is the defect this file's own history records.
pub fn palw_v2_params_with_class_rows_v1(
    base: Params,
    base0_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
    genesis_utxos: crate::utxo::utxo_collection::UtxoCollection,
    rows: PalwGenesisClassRowsV1<'_>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    use crate::palw_mode_v2::PalwModeV2Error as E;
    let invalid = |what: &'static str| E::Invalid(what);

    let mut params =
        palw_v2_params_from_artifacts_on_base_with_utxos(base, base0_artifact_root, genesis_bonds, genesis_utxos.clone())?;
    // **A held row set mints the held REGIME first, and the order is forced.**
    //
    // `palw_held_context_mint_v1` is the one spelling of "this lattice runs ADR-0103" — the V4
    // signing-context set, trace format 4 with the tiled prompt ids, the regime's prompt cap, the
    // `2^40` step ladder and the arity its own court derives. Two things below read the bundle it
    // rewrites, so it cannot run afterwards:
    //
    // * a held row's `max_step_leaf_count` and `canonical_step_leaf_count` are counted against
    //   `bundle.court.max_step_leaf_count()`. Registered under the RC's `2^26` and then minted to
    //   `2^40`, every one of those counts would be a cap rather than a count — the class would be
    //   priced at a ceiling it does not reach;
    // * `prompt_ids_form` below is read off the trace format the mint sets.
    //
    // It also arms, from genesis, the fences the regime cannot run without (`palw_prompt_ids_merkle`,
    // `palw_signature_contexts_v2`, `palw_kary_court`, `palw_shard_court`, `palw_panel_da`,
    // `palw_held_context`, `palw_da_court`, `palw_fp_da_pins`) — every one of which
    // `palw_t12_arm_every_rule_from_genesis` has already armed, so this is the same decision
    // arriving through the function that owns it rather than a second one.
    if let PalwGenesisClassRowsV1::Held(held) = rows {
        if held.is_empty() {
            return Err(invalid("a held genesis set registers no model row"));
        }
        // **The close budget is DERIVED from the rows this genesis registers** (ADR-0082
        // Decision 6), before the mint, because the mint carries the court's ceilings forward and
        // the rows are what those ceilings have to pay for.
        //
        // `DEFAULT_MAX_CLOSE_CHUNKS` is 27, sized for the RC's graph-v5 row at 512. A held row at
        // 2,097,152 opens a wider terminal, and a ruleset that registered it against the RC's
        // ceiling would fail its own boot gate with "a registered class's terminal opening exceeds
        // the ceiling this ruleset pays for" — which is the honest failure and not the intended
        // one. `palw_widest_close_over_the_ladder_v1` is the derivation that answers it, over THIS
        // set's families and THIS set's widths, and it returns the binding node rather than a bare
        // maximum so the number has a reason attached.
        //
        // Priced under the court the mint is about to install: the Merkle prompt-id form (trace
        // format 4) and the ladder the regime freezes. Getting that pre-image wrong cannot go
        // unnoticed — `verify_against_catalog` re-derives every registered row's cost against the
        // court that actually shipped, and refuses the genesis if the two disagree.
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
            unreachable!("palw_v2_params_from_artifacts_on_base_with_utxos installs a ConsensusV2 bundle or returns Err");
        };
        let ladder = crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
        let court_preimage = crate::palw_class_admission_v2::PalwKaryCourtV1 {
            dissection_arity: bundle.court.dissection_arity(),
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            window_court_daa: bundle.state.window_court(),
        };
        // **Each row at ITS OWN family and width, not the cross product.** `palw_widest_close_over_the_ladder_v1`
        // prices `families × rows`, which is the right shape for "how wide could this ladder ever go";
        // it is the wrong one for "what does THIS genesis register". Asked for both families at both
        // widths it answers with the hybrid at 2,097,152 — a row nobody registers — and sizes the
        // close budget, the court window and the pruning horizon for it.
        let widest = held
            .iter()
            .filter_map(|row| {
                let family = match row.family {
                    PalwGenesisHeldFamilyV1::DenseA16 => &crate::palw_context_ladder::PALW_LADDER_FAMILIES_V7[0..1],
                    PalwGenesisHeldFamilyV1::HybridQwen36 => &crate::palw_context_ladder::PALW_LADDER_FAMILIES_V7[1..2],
                };
                crate::palw_context_ladder::palw_widest_close_over_the_ladder_v1(family, &[row.n_ctx], Some(court_preimage), ladder)
            })
            .map(|(_, _, binding)| binding.close_bytes)
            .max()
            .ok_or(invalid("no held row of this genesis set prices at all: the set registers nothing prosecutable"))?;
        let close_bytes = widest.max(crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);
        let court = bundle.court;
        bundle.court = crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(
            court.max_step_leaf_count(),
            court.turn_deadline_daa(),
            court.terminal_rounds(),
            close_bytes,
            court.max_terminal_macs(),
            court.max_operand_count(),
        )
        .map_err(|_| invalid("the held set's derived close budget is not a legal court ceiling"))?
        .with_dissection_arity(court.dissection_arity())
        .map_err(|_| invalid("the held set's court refuses its own arity"))?;
        params = palw_held_context_mint_v1(params, ladder)?;
        // **And the arity the FINAL class set derives, frozen before those rows are priced.**
        //
        // A held court's arity is a function of the widest registered attention site
        // (`palw_court_arity_held_v1` over `palw_attn_widest_registered_site_v2`), and the mint has
        // only the floor to look at: it answers 2. Register the 2M row afterwards and the same
        // derivation answers 4 — so the rows would be admitted priced for a binary court and
        // prosecuted in a 4-ary one, which is precisely what `validate_palw_v2` refuses ("the gate
        // that admits a class would price it for one court and the court that plays its dispute
        // would deal another"). Under-priced by `k/2` in disclosure bytes, on the very close ceiling
        // they were admitted under.
        //
        // So the site is taken over the profiles this genesis is ABOUT to register as well as the
        // ones it already has, the arity is derived once, and the rows below are priced at it. The
        // fixed point converges in one step because a profile does not depend on the arity.
        let row_profiles = held
            .iter()
            .map(|row| match row.family {
                PalwGenesisHeldFamilyV1::DenseA16 => crate::palw_context_ladder::palw_a16_context_row_profile_v7(row.n_ctx)
                    .map_err(|_| invalid("a held dense row does not derive at the width this genesis registers")),
                PalwGenesisHeldFamilyV1::HybridQwen36 => crate::palw_context_ladder::palw_qwen36_context_row_profile_v7(row.n_ctx)
                    .map_err(|_| invalid("a held hybrid row does not derive at the width this genesis registers")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
            unreachable!("the mint returns a ConsensusV2 ruleset or an Err");
        };
        let (registered_history, registered_lanes) = crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle);
        let (rows_history, rows_lanes) = crate::palw_court_v2::palw_attn_widest_site_of_profiles_v2(row_profiles.iter());
        let arity = crate::palw_mode_v2::palw_court_arity_held_v1(
            bundle.state.window_court(),
            bundle.court.turn_deadline_daa(),
            registered_history.max(rows_history),
            crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4,
            bundle.court.terminal_rounds(),
            registered_lanes.max(rows_lanes),
            bundle.court.max_close_chunks(),
        )
        .ok_or(invalid(
            "no dissection arity fits this genesis set's court window: the widest row it registers cannot be tried inside it",
        ))?;
        bundle.court = bundle.court.with_dissection_arity(arity).map_err(|_| invalid("the derived held arity is not a legal one"))?;
        let bundle = bundle.clone();
        params = params.with_palw_v2_depths(&bundle);
    }
    // ADR-0151 D3, read before the bundle is borrowed: does this chain's clock need a claim?
    let clock_advances_without_a_claim = palw_clock_advances_without_a_claim_v1(&params);
    // ADR-0151 D1, same reason: the escrow a claim carries is the cash half of its fraud gain.
    // **The subsidy the FIRST block really pays** (2026-09-23 audit U2, wired by option A): every
    // ConsensusV2 preset sets `deflationary_phase_daa_score = 0`, so no block is ever pre-deflationary
    // and `pre_deflationary_phase_base_subsidy` (370,468,345 sompi) is a number no block pays; the
    // genesis-era subsidy is `palw_genesis_block_subsidy_sompi` — 444,562,014,000 sompi on testnet-12,
    // 1,200x. It moves in ONE change with the floor concurrency the card sizes for
    // (`PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1`), the runtime's escrow reservation and the premine pin:
    // any one of them alone either under-states the fraud a seat backs or does not assemble.
    let base_subsidy_and_carve =
        (palw_genesis_block_subsidy_sompi(&params), params.palw_overlay_carve.map(|c| c.worker_carve_permille));
    // **Which prompt-id form the dense row is PRICED at, read before the bundle is borrowed.**
    //
    // ADR-0082 Decision 5's fence (`palw_prompt_ids_merkle`) is `None` on every shipped preset, so
    // this is `Flat` — the more expensive of the two forms (82,080 bytes of ids against 80,504),
    // which is the safe direction for a ceiling. It is read from the network's own fence rather
    // than chosen here, because a class priced under a form its ruleset does not run is the
    // "figure measured under one configuration" defect ADR-0082 §1 opens with.
    let prompt_ids_form = params.palw_prompt_ids_form_at(0);
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
        unreachable!("palw_rc_params_from_artifacts installs a ConsensusV2 bundle or returns Err");
    };

    // The floor's economics, read off the base assembly's own genesis object rather than restated:
    // a second spelling of the slash value would be a second place for it to drift.
    let (slash, target) = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { slash_value_per_pwu, initial_target, .. } => {
                Some((*slash_value_per_pwu, *initial_target))
            }
            _ => None,
        })
        .ok_or(invalid("the base assembly carries no class registration"))?;

    // **What each class is given at genesis, and why it is not the grant floor.**
    //
    // A post-genesis entrant takes `min_grantable_share_permille` and may take nothing else
    // (ADR-0049 Decision H), and ADR-0054's growth rule is what lets it earn more afterwards. That
    // is the right shape for a stranger arriving on a running chain. It is the wrong shape for a
    // class the network is being MINTED around: the grant floor is defined to make its holder's
    // expectation exactly one block per epoch, so a genesis that hands the model tiers 1‰ ships a
    // network whose two model classes may produce one block each per epoch — a public network that
    // is, in practice, the floor with two demonstrations attached.
    //
    // Genesis is the one moment cadence can be allocated rather than earned, because there is no
    // incumbent to take it from: the floor holds the whole table by construction and every permille
    // any class ever holds came from it. So the model tiers are funded here, and ADR-0054 decides
    // what happens next in both directions — a tier that fills its budget grows, one that goes
    // quiet decays back into the floor.
    let floor_reserve = bundle.state.base_class_reserve_permille();
    // **A network whose work target is armed from genesis declares NO per-model share** (the
    // operator's decision of 2026-09-22, ADR-0137's own conclusion carried into the mint).
    //
    // ADR-0137 made a share a RESULT: past `Params::palw_work_target` a model class's draw is
    // `MAX · min(1, CCU_m / W₀)` (`palw_effective_class_target_v1` — "the class target is not read
    // at all"), `apply_class_share_growth` returns before it grows or decays anything, the registry
    // "moves no share, refreshes no budget and seats no price", and the panel's capacity is one
    // network-wide replay budget rather than a per-class allocation (D5, `panel_room_v1`). On such a
    // network the 489‰ the RC card hands each tier is a number nothing reads — and a number nothing
    // reads is worse than no number, because it looks like a policy. It also breaks ADR-0144's
    // "registering a model must not change an unrelated model's economics": a declared share funds
    // an entrant by SCALING every incumbent, so the eleventh model would move the other ten.
    //
    // So both tiers register at the bundle's own `min_grantable_share_permille` — the floor a
    // post-genesis entrant takes (ADR-0049 Decision H) — and the liveness floor holds the whole
    // residual, which is exactly what ADR-0137 says it is: "the floor is the residual and is never
    // priced here". Block rights come from verified work and nothing else:
    //
    //     Ready panel → claim verified → Final → CanonicalWork → execution quanta → global permit
    //
    // with the anti-Sybil conditions armed beside it on this preset — `palw_canonical_work`
    // (ADR-0145 F1: a claim's work is DERIVED, so padding, copies and idle context buy nothing),
    // `palw_admission_independence` (F3: a class is not judged by its own registrant, so registering
    // a swarm of light models and seating them yourself is not a route to block rights), and the
    // execution lane's ONE permit a round, which is the scarcity every model shares.
    let no_declared_share = matches!(rows, PalwGenesisClassRowsV1::Held(_));
    let min_share = bundle.state.min_grantable_share_permille();
    // **Every model row this card registers, as the rest of the card reads it**: the profile the
    // collateral prices a draw on, the catalog entry, the genesis-form registration, and the
    // canonical `(prefill, decode)` one draw is priced at. The RC set and a held set differ in HOW a
    // row is built and what share it declares; from here on they are one list, so the collateral,
    // the capability grant, the catalog and the registration order each have one spelling.
    struct GenesisTierV1 {
        profile: crate::palw_step::PalwShapeProfileV3,
        entry: crate::palw_mode_v2::PalwClassCatalogEntryV2,
        object: crate::palw_state_v2::PalwConsensusObjectV2,
        canonical: (u32, u32),
    }
    let (tiers, shares): (Vec<GenesisTierV1>, Vec<u16>) = match rows {
        PalwGenesisClassRowsV1::Rc { qwen36_artifact_root, qwen25_a16_artifact_root } => {
            let dense_share = qwen25_a16_artifact_root.map(|_| PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE).unwrap_or(0);
            // **The hybrid asks for more than it keeps, because the dense tier dilutes it.**
            //
            // `granted_share_table_v2` funds an entrant by scaling every incumbent to `1000 - share`, and
            // the dense tier registers second — so whatever the hybrid holds when the dense tier arrives is
            // multiplied by `(1000 - dense) / 1000`. Declaring the intended figure would land the hybrid
            // BELOW it, silently, and the two tiers would not be the equals this card means them to be.
            // Inverting the dilution here is what makes the constants above describe the table the chain
            // actually starts with, which a test asserts by folding the real transition over these objects.
            let hybrid_share =
                u16::try_from((u32::from(PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE) * 1000).div_ceil(1000 - u32::from(dense_share)))
                    .map_err(|_| invalid("the hybrid's genesis share does not fit a permille"))?;
            // **`_v3`, and the version number is not the obvious one.** The hybrid's corrected row is
            // `graph-v3` — the v2 projection over the eps-corrected geometry — because `graph-v2`'s
            // spelling reached t11 first and a registered name cannot be re-pointed at a different id.
            // Registering `qwen36_profile_v2` instead would clear every other check and mint a THIRD
            // class id that no node dispatches, which is the shape this line has to get right because
            // nothing downstream can tell it apart: the admission gate's weight rule is scoped to the
            // FAMILY, and the family is certified either way.
            let (profile, entry, object) =
                crate::palw_qwen36_profile::qwen36_registration_v3(qwen36_artifact_root, hybrid_share, slash, target)
                    .map_err(|_| invalid("the Qwen3.6 registration does not derive"))?;
            let mut tiers = vec![GenesisTierV1 { profile, entry, object, canonical: crate::palw_qwen36_profile::QWEN36_RC_CANONICAL }];
            let mut shares = vec![hybrid_share];
            // The A16 dense class, when its artifact root is pinned. It registers last, so what it
            // declares is what it keeps — no dilution follows it.
            //
            // **GENESIS DECISION (8e): the dense slot holds ADR-0082's `graph-v5` 512 row, and the
            // `graph-v2` `n_ctx`-16 row it replaces is not registered.** The 5f genesis card §2 registers
            // "dense A16 **512 row** — the only wide row registered", and §6 states why the narrow one
            // cannot stand in for it: at `n_ctx` 16 the decode budget is 7 tokens against grammar floors
            // of 38 / 60 / 104, so the demonstration the launch is about cannot be expressed. A class is
            // its graph, so this is a NEW class id and not a repair — `qwen25_a16_registration_v2` and
            // the row it builds stay exactly where they are for the networks that already registered them.
            //
            // **GENESIS DECISION (8e): the share is the dense tier's existing
            // `PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE`, unchanged.** One class leaves the slot and
            // another takes it, so the cadence table the chain starts with is the one the card's
            // arithmetic was written against, and the hybrid's dilution inversion above is unmoved.
            if let Some(root) = qwen25_a16_artifact_root {
                let (profile, entry, object) = crate::palw_qwen25_profile::qwen25_a16_graph_v5_registration_v1(
                    root,
                    dense_share,
                    slash,
                    target,
                    bundle,
                    prompt_ids_form,
                    // **Whose bond the registration is charged to: nobody's.** A fused row must carry
                    // its graph (`PalwClassStateV2::fused_attention` is readable from nowhere else) and
                    // the carriage has a `registrant_bond` field, but a genesis registration has no
                    // registrant — the network itself decided it, and ADR-0056 Decision 3 says it pays
                    // nothing. Naming a real registry row instead would charge that operator a
                    // reservation it never asked for AND make the class stop looking like a genesis
                    // class to ADR-0071 SA-3's exemption, which with the capability fence armed leaves
                    // no bond able to judge it. See `palw_genesis_registrant_bond_v1`.
                    crate::palw_state_v2::palw_genesis_registrant_bond_v1(),
                )
                .map_err(|_| invalid("the Qwen2.5 A16 graph-v5 registration does not derive"))?;
                tiers.push(GenesisTierV1 { profile, entry, object, canonical: crate::palw_qwen25_profile::QWEN25_A16_CANONICAL });
                shares.push(dense_share);
            }
            (tiers, shares)
        }
        // **Every held row takes the grant floor and nothing else.** The dilution inversion above
        // exists to make a DECLARED figure land on an INTENDED realized one; with no intended
        // allocation there is nothing to invert — the minimum is the minimum however the tiers dilute
        // each other, and the floor absorbs the difference.
        //
        // Each row carries a CARRIAGE, so it is built against the bundle it joins (the ladder, the
        // arity, the court window), and its registrant is the genesis's own, for the reason given on
        // the RC dense row above.
        PalwGenesisClassRowsV1::Held(held) => {
            let tiers = held
                .iter()
                .map(|row| {
                    let (registered, canonical) = match row.family {
                        PalwGenesisHeldFamilyV1::DenseA16 => (
                            crate::palw_qwen25_profile::qwen25_a16_held_registration_v1(
                                row.artifact_root,
                                row.n_ctx,
                                min_share,
                                slash,
                                target,
                                bundle,
                                prompt_ids_form,
                                crate::palw_state_v2::palw_genesis_registrant_bond_v1(),
                            )
                            .map_err(|_| invalid("a held Qwen2.5 A16 row does not derive")),
                            crate::palw_qwen25_profile::qwen25_a16_held_canonical_v1(row.n_ctx),
                        ),
                        PalwGenesisHeldFamilyV1::HybridQwen36 => (
                            crate::palw_qwen36_profile::qwen36_held_registration_v1(
                                row.artifact_root,
                                row.n_ctx,
                                min_share,
                                slash,
                                target,
                                bundle,
                                prompt_ids_form,
                                crate::palw_state_v2::palw_genesis_registrant_bond_v1(),
                            )
                            .map_err(|_| invalid("a held Qwen3.6 row does not derive")),
                            crate::palw_qwen36_profile::qwen36_held_canonical_v1(row.n_ctx),
                        ),
                    };
                    let (profile, entry, object) = registered?;
                    Ok(GenesisTierV1 { profile, entry, object, canonical })
                })
                .collect::<Result<Vec<_>, crate::palw_mode_v2::PalwModeV2Error>>()?;
            let shares = vec![min_share; tiers.len()];
            (tiers, shares)
        }
    };
    // The floor after every donation, in the same arithmetic the transition applies (each entrant
    // scales every incumbent to `1000 - share`). A genesis that starts the floor below the reserve
    // ADR-0054 would never let a growing class cross is a genesis in a state its own rules could not
    // have produced.
    let floor_after = shares.iter().fold(1000u32, |floor, share| floor * (1000 - u32::from(*share)) / 1000);
    if floor_after < u32::from(floor_reserve) {
        return Err(invalid("the genesis class shares would start the liveness floor below its own reserve"));
    }

    // **The registry has to be able to CARRY the tiers this card funds.**
    //
    // A claim reserves `pwu × slash_value` against its bond until it resolves, and the bond
    // collateral the base assembly declared was derived from the FLOOR's counted work. A model
    // tier's claims are hundreds of times dearer (Qwen3.6's canonical inference counts 348× the
    // floor's), so a registry sized for the floor admits a couple of dozen of them and then holds:
    // `the bond's exposure ceiling leaves no room for another claim`, measured at fifteen blocks
    // against a two-hundred-block allowance. Allocating cadence a bond cannot carry is allocating
    // nothing, so the collateral is re-derived here from the DEAREST class the card registers, by
    // the same rule the base assembly used for the floor.
    let dearest_pwu_per_inference = tiers
        .iter()
        .map(|tier| genesis_pwu_of(&tier.object))
        .chain(bundle.genesis_objects.iter().map(genesis_pwu_of))
        .max()
        .unwrap_or(0);
    // **The declaration is what the runtime ceiling reads, so it must be the money that is
    // really there** — `max(derived minimum, the genesis output's own amount)`. Declaring only
    // the derived minimum left ADR-0061's re-sized carve invisible to consensus: the exposure
    // ceiling is `declared × max_exposure_ratio`, so the surplus sitting in the outpoint bought
    // exactly one extra concurrent claim, and the ADR's "≈3× margin" existed on paper only. The
    // held amount is the honest figure and C-08 (`held ≥ declared`) still passes by construction.
    // **Sized per CLASS on a held card, on the dearest class alone on the RC's.**
    //
    // `palw_v2_collateral_for_claim_lifetime_v1` prices `max(per-claim) × (MAX_CLAIM_EXPOSURE_DAA + 1)`
    // = ×7,201, because a bond really can accumulate that many claims before the first finalizes.
    // That concurrency is the FLOOR's; a model class is refused past its in-flight cap, which the
    // held 2M row derives as one. Measured on this card: the collapse asks 38,889,673.34 MSK a seat
    // and the honest sum asks about 21,700 — the floor at genuine full concurrency is 11.10 MSK and
    // one 2M claim is 5,400.59. A seat that costs more than the whole community allocation is not a
    // safety margin, it is a closed network.
    //
    // The RC path keeps the collapse untouched: testnet-11's registry is a live chain fact, and at
    // `n_ctx` 512 the two derivations differ by little enough that re-sizing it would move a shipped
    // genesis to save collateral nobody is short of.
    let carried_collateral = if no_declared_share {
        let floor_pwu = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                    class_id,
                    pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
                    ..
                } if *class_id == bundle.base_class_id => Some(*pwu_per_inference),
                _ => None,
            })
            .max()
            .ok_or(invalid("the base assembly carries no floor registration to size collateral against"))?;
        // **Each row in BOTH units** (ADR-0151 D1, corrected 2026-09-23). The reservation the
        // runtime writes is the row's derived work for one draw renormalised by the floor's
        // leaves-per-MAC-eq, not its own declared leaves — the two differ by 44.25× on the held 2M
        // row, which is exactly the factor that wedged the first t12 fleet's producers shut.
        // A free function, not a closure: it names no error type, so the caller keeps using its own
        // `invalid` and this stays readable next to the two call shapes around it.
        fn derived_per_draw(profile: &crate::palw_step::PalwShapeProfileV3, canonical: (u32, u32)) -> Option<u128> {
            let descriptor =
                crate::palw_canonical_work_v1::PalwCanonicalClassDescriptorV1::of(profile, crate::Hash64::default()).ok()?;
            let job = crate::palw_base0_profile::rc_job_context(profile, canonical.0, canonical.1);
            Some(crate::palw_canonical_work_v1::palw_canonical_draw_work_v1(&descriptor, &job, true).ok()?.provisional_scalar_v1())
        }
        // The floor's profile is rebuilt here rather than threaded in: it is a pure function of
        // `PALW_RC_BASE0_GEOMETRY`, and the base assembly that first built it is a different call.
        let floor_profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .map_err(|_| invalid("the floor profile does not derive for the collateral basis"))?;
        let floor_row = crate::palw_fp_devnet_v3::PalwCollateralRowV1 {
            declared_leaves: floor_pwu,
            derived_per_draw: derived_per_draw(&floor_profile, crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL)
                .ok_or(invalid("the floor's canonical draw does not derive"))?,
        };
        let model_rows = tiers
            .iter()
            .map(|tier| {
                Ok(crate::palw_fp_devnet_v3::PalwCollateralRowV1 {
                    declared_leaves: genesis_pwu_of(&tier.object),
                    derived_per_draw: derived_per_draw(&tier.profile, tier.canonical)
                        .ok_or(invalid("a model row's canonical draw does not derive"))?,
                })
            })
            .collect::<Result<Vec<_>, crate::palw_mode_v2::PalwModeV2Error>>()?;
        let bind_window_liveness = (!clock_advances_without_a_claim).then(|| bundle.state.window_bind());
        // **The escrow a genesis-era claim carries**, which is the cash half of its fraud gain: the
        // pre-deflationary subsidy through the overlay's worker carve. Read from the preset rather
        // than assumed, because a card with a different carve authorizes a different lie.
        let escrowed_reward = escrow_for_a_genesis_claim_v1(&base_subsidy_and_carve);
        crate::palw_fp_devnet_v3::palw_v2_collateral_for_class_set_v1(
            floor_row,
            &model_rows,
            escrowed_reward,
            bind_window_liveness,
            crate::palw_fp_devnet_v3::PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1,
        )
        .max(crate::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI)
    } else {
        // What this network's premine carves a seat (`genesis_bond_collateral_for`): testnet-11's
        // 10,000 MSK byte for byte, and a mainnet card's producer floor (13,000 MSK).
        crate::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(dearest_pwu_per_inference)
            .max(crate::config::premine::genesis_bond_collateral_for(params.net))
    };
    // **And the registry has to be able to JUDGE the tiers this card funds** (ADR-0071
    // Decision 3).
    //
    // Same shape as the collateral re-derivation directly above, one field over: the base assembly
    // declared each genesis bond capable of the FLOOR, because the floor was the only class it
    // registered. A panel seat is drawn only for classes its bond declared, and the genesis
    // registry has zero slack by construction — `seat_count + 1` bonds with the executor excluded
    // — so tiers registered here without extending the declarations would be tiers whose every
    // claim fails to seat a panel and voids at `BindTimeout`. Funding cadence to a class no seat
    // will judge is funding nothing, exactly as funding a bond that cannot carry the claim is.
    //
    // A genesis operator runs what the genesis says it runs; a stranger joining later declares for
    // itself with `BondCapabilityDeclared`. This is the one moment capability can be assigned
    // rather than claimed, for the same reason cadence can be.
    let tier_class_ids: Vec<crate::Hash64> = tiers.iter().map(|tier| tier.entry.class_id).collect();
    for genesis_object in bundle.genesis_objects.iter_mut() {
        if let crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered { collateral, capable_classes, .. } = genesis_object {
            if *collateral < carried_collateral {
                *collateral = carried_collateral;
            }
            capable_classes.extend(tier_class_ids.iter().copied());
        }
    }

    // The catalog. Two DIFFERENT orders, on purpose: the CATALOG is a commitment and
    // sorts ascending by class id (its canonical form — two assemblers of one class set must
    // produce one root), while the REGISTRATION LIST keeps the floor first, because the genesis
    // gate and the transition both require the base class to be the first registration. Conflating
    // the two orders is exactly what the catalog's own validation refuses.
    let (base_profile, base_catalog) = crate::palw_base0_profile::palw_rc_base0_registration_v1(base0_artifact_root)
        .map_err(|_| invalid("BASE-0's registration does not derive"))?;
    let base_entry = base_catalog.entries().first().ok_or(invalid("the RC catalog lost its floor"))?.clone();
    debug_assert_eq!(base_profile.shape_profile_id(), bundle.base_class_id, "the floor the base assembly registered");
    let mut entries = vec![base_entry];
    entries.extend(tiers.iter().map(|tier| tier.entry.clone()));
    entries.sort_by(|a, b| a.class_id.cmp(&b.class_id));
    let catalog =
        crate::palw_mode_v2::PalwClassCatalogV2::new(entries).map_err(|_| invalid("the registered catalog is not well-formed"))?;
    bundle.class_catalog_root = catalog.root();
    bundle.genesis_objects.extend(tiers.into_iter().map(|tier| tier.object));

    // **Every class's attempt lane is seeded at the retarget's equilibrium** (ADR-0076).
    //
    // Until this call all three rows carried the SAME target, because the model rows were built
    // from `(slash, target)` read off the floor's registration a hundred lines up — one number
    // meant "the floor's economics", and the tiers inherited the second half of it along with the
    // first. The two halves are not alike: `slash_value_per_pwu` is per-network by rule (one slash
    // value for the network, because it sets weight-per-collateral), while a target is per CLASS
    // by construction, and sharing it hands the cadence table to whichever class draws fastest.
    //
    // Measured on the network this line changes: Relaunch 5d ran one hour with the floor at 249
    // blocks and QWEN36 and A16 at zero, against a table allotting them 489‰ each — and no rule
    // in the chain could repair it, because `retarget_over_span_v1` skips a class that produced
    // nothing and `converge_idle_target_v1` moves an idle class only toward a price it is already
    // paying. A shared seed is therefore not a slow start; it is a permanent one.
    palw_seed_attempt_targets_v1(bundle)?;

    // **A genesis that registers a FUSED row arms the court that can try it** (ADR-0082
    // Decision 3; 5f genesis card §1, "ARM, `ForkActivation::always()`").
    //
    // This is not a preference and it is not a schedule. `validate_palw_v2` refuses to assemble a
    // ruleset whose genesis set registers a graph-v5 class while `palw_kary_court` is dormant —
    // "a court with no dissection cannot try that leaf, so every dispute of that class would end
    // unprosecuted at the price it was admitted for" — so on this path the two are one decision
    // and the only question is where it is spelled. It is spelled HERE, as a function of the class
    // set, rather than as a literal on a preset: a preset that registered the row and forgot the
    // fence would be a binary that panics at startup, and a preset that armed the fence without
    // registering the row would be ADR-0065 D1's mistake (a rule with nothing shipping to obey
    // it). Deriving it from the set is what makes both impossible.
    //
    // `always()` and never a height: a chain that acquired a dissection court mid-life would be
    // two courts wearing one ruleset id, and the arity the court derives is inside
    // `palw_ruleset_id_v2`. Genesis is the only moment this can be armed.
    //
    // Networks with no fused row in their genesis set are untouched — the field stays `None` and
    // the build fingerprints byte-identically to one without it.
    let registers_a_fused_row = match &params.palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
            crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle).0 > 0
        }
        _ => false,
    };
    if registers_a_fused_row && params.palw_kary_court.is_none() {
        return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
            "the genesis set registers a fused-attention row and the preset leaves palw_kary_court dormant: the \
             court that can try that row must be STATED on the preset (ForkActivation::always()) — a fence armed \
             by derivation alone would be an arming nobody decided",
        ));
    }

    // ADR-0152 §4-ter: the answerability mirror is a function of the genesis rows this function
    // just installed, so it is re-mirrored here, after them — the base's own sync saw the base's rows.
    params.sync_palw_held_answerability();
    // Both gates again, from scratch, over the network actually being shipped.
    params.validate_palw_v2()?;
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        unreachable!("still the bundle installed above");
    };
    // The SUPPLIED set, so both gate runs — the base assembly's and this one's — resolve a bond's
    // collateral against one premine. Reading `genesis_premine_utxos_for` a second time here would
    // make the two halves of one card answer to two different UTXO sets.
    crate::palw_genesis_v2::verify_palw_genesis_v2_with_clock_v1(
        bundle,
        &catalog,
        &bundle.genesis_objects,
        |outpoint| genesis_utxos.get(outpoint).map(|entry| entry.amount),
        // ADR-0151 D3 — read from THIS network's fences, never assumed.
        clock_advances_without_a_claim,
    )?;
    Ok(params)
}

/// **The attempt-lane seed for every class a bundle registers** (ADR-0076).
///
/// `MAX · share · pwu_per_inference / PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1` —
/// [`crate::palw_class_daa::attempt_target_seed_v1`] is the arithmetic and its doc is the
/// derivation. This function is only the part that needs the bundle: **which share** each class
/// actually holds.
///
/// The declared `share_permille` on a registration is NOT that number. A grant funds an entrant by
/// scaling every incumbent, so the floor declares 1000‰ and holds 22‰ once the tiers register, and
/// the hybrid declares 957‰ to hold 489‰ after the dense tier dilutes it. Pricing against the
/// declared figures would be a 23× error on the floor alone. So the objects are FOLDED — through
/// `apply_palw_transition_v2`, the same call `assemble_palw_rc_identity_v2` uses to prove a
/// registration list applies — and the resulting table is what the seeds are computed from. A
/// second, hand-rolled spelling of the dilution here would be a second place for it to drift, and
/// this file already carries one such repeat (`floor_after`) whose only job is a sanity check.
///
/// Folding twice is deliberate and cheap: this fold reads shares from objects whose targets are
/// about to change, and target is not an input to the share table, so the table this reads is the
/// table the chain will hold.
fn palw_seed_attempt_targets_v1(
    bundle: &mut crate::palw_mode_v2::PalwConsensusParamsV2,
) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, apply_palw_transition_v2, palw_max_exposure_pwu_of_rule_v1,
    };
    let invalid = crate::palw_mode_v2::PalwModeV2Error::Invalid;
    // The genesis block's own context. Only the share table is read out of the fold, and no
    // grant's arithmetic depends on the block, its DAA or its subsidy — so these are the genesis
    // values rather than a placeholder that would have to be explained.
    let ctx = PalwBlockContextV2 { block: Default::default(), daa_score: 0, blue_score: 0, subsidy: 0 };
    let (state, _delta) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &bundle.state, &ctx, &bundle.genesis_objects, None)
        .map_err(|_| invalid("the genesis registrations do not apply, so no share table can be read from them"))?;

    let mut rows: Vec<(usize, u16, u64)> = Vec::new();
    for (index, object) in bundle.genesis_objects.iter().enumerate() {
        let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, .. } = object else {
            continue;
        };
        let share = state
            .class_share_permille(class_id)
            .ok_or(invalid("a registered class holds no share in the table its own registration produced"))?;
        rows.push((index, share, palw_max_exposure_pwu_of_rule_v1(pwu_rule)));
    }
    if rows.is_empty() {
        return Err(invalid("a ConsensusV2 bundle registers at least the base class"));
    }
    for (index, share, pwu) in rows {
        let seed = crate::palw_class_daa::attempt_target_seed_v1(share, pwu);
        let PalwConsensusObjectV2::ClassRegistered { initial_target, .. } = &mut bundle.genesis_objects[index] else {
            unreachable!("the index was taken from a ClassRegistered row of this same list");
        };
        *initial_target = seed;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The PALW-RC (testnet-12) genesis card — road-map Gate 4
// ---------------------------------------------------------------------------------------------

/// **`artifact_root` for the RC's liveness floor — derived, not hosted.**
///
/// The road map called the genesis artifact "the one input code cannot mint". Half of that stopped
/// being true: `palw_base0_profile`'s own doc says "BASE-0 has no file: it is a specification", and
/// a specification's artifact can be PRODUCED by a rule instead of downloaded. It is —
/// `misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1` derives every weight byte from one
/// pinned seed and takes the Merkle root over the canonical inventory — so this constant is
/// re-derivable by anyone, in any language, rather than being a 4.5 MiB blob every participant has
/// to be handed a correct copy of.
///
/// `the_pinned_rc_artifact_root_is_the_one_the_floor_derives` (in `misaka-palw-base0`, which is the
/// only crate that can hold both sides) fails if either side moves — and it did: the floor's
/// `vocab_size` and `n_ctx` are chosen by the court's cost ceiling now (ADR-0049 Decision C), so
/// every weight byte the seed produces moved with them.
pub const PALW_RC_GENESIS_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([
    0xbc, 0xf2, 0xd9, 0xeb, 0x73, 0x57, 0xbd, 0x6c, 0x26, 0x7d, 0xf2, 0xdf, 0x65, 0x88, 0x39, 0x3c, 0xa7, 0x1c, 0x67, 0xd7, 0xc8,
    0x02, 0x90, 0x3c, 0xa7, 0x03, 0x19, 0x48, 0x30, 0x3c, 0x79, 0x3d, 0xcb, 0x78, 0xbf, 0xe2, 0x64, 0x88, 0xf5, 0x2d, 0x03, 0x93,
    0xbe, 0x08, 0xe0, 0xcc, 0x07, 0x77, 0xb0, 0x80, 0xe2, 0xdc, 0xe9, 0x35, 0x5d, 0x35, 0x76, 0x03, 0x6b, 0x73, 0x45, 0x45, 0xb8,
    0xdf,
]);

/// **The Qwen3.6 class's artifact root — the one fact about it code cannot mint.**
///
/// `Qwen36ArtifactV1::artifact_root()` over the converted `.palwq36`: the shape, every parameter
/// table, the rotary table and every weight byte, each under a length-prefixed tagged name. A node
/// that computes a different root for its file is holding different weights, whatever the file is
/// called — which is why the chain names the ROOT and never a path or a filename.
///
/// Measured on the artifact converted from
/// `Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf` (33.27 GiB of `int8` codes over 40 layers,
/// GGUF SHA-256 `1dc49461…`, default flags). The conversion is deterministic — see
/// `docs/palw-public-testnet-classes-runbook.md` for the recipe — so an operator may either
/// rebuild this file from the published GGUF or download it and let their node verify the root;
/// both routes end at this constant.
///
/// **Re-pinned 2026-08-27.** The first pin (`c970d693…`) named an artifact that could be
/// DOWNLOADED but not REBUILT. The original file survives on HuggingFace as
/// `palw-runtime/qwen36-35b-a3b.palwq36` (SHA-256 `5fe1d15c…`; its computed root equals the
/// old pin — verified after this note first claimed the file was lost, which was wrong). What
/// was true, and what forced the re-pin, is the other half: no conversion from the pinned GGUF
/// reproduces it. The pin commit's own converter, HEAD's converter on arm64/macOS and HEAD's
/// converter on x86-64/Linux all produce ONE root under default flags, and it is not that one —
/// while the SAME converter reproduces the dense tier's pin from its public checkpoint exactly.
/// Whatever produced `c970d693…` (a different flag set, an edited tree, or a different input)
/// was never written down, so the runbook's "rebuild it yourself" route was unsatisfiable. This
/// constant is now the value the published converter actually produces, and no converter change
/// may ever chase a pin; the pin follows the converter.
///
/// **Unset (all zero) means the class is not on the network.** `palw_rc_shipped_params` then
/// assembles the one-class RC exactly as before, so a build with no Qwen3.6 root is the shipped
/// floor-only network rather than a broken two-class one.
pub const PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([
    0xf4, 0xaa, 0xd4, 0xfd, 0x54, 0x39, 0x28, 0xeb, 0x2d, 0x3a, 0x73, 0x75, 0x55, 0xb0, 0x9d, 0xa9, 0xbf, 0x68, 0x5f, 0xc5, 0x15,
    0xc0, 0xf8, 0xd4, 0x52, 0x09, 0x88, 0xef, 0xcf, 0xfa, 0xcf, 0x08, 0x13, 0xd1, 0xb7, 0x27, 0x53, 0x7f, 0x0d, 0x03, 0xd3, 0x49,
    0x25, 0x3a, 0xa1, 0x1e, 0xf4, 0x27, 0xe4, 0x04, 0x7c, 0x21, 0x66, 0xb6, 0x9f, 0xd7, 0xed, 0xb4, 0x6a, 0x4a, 0x99, 0x84, 0xb3,
    0x68,
]);

/// **The Qwen2.5-1.5B A16 class's artifact root — the A16 OPERAND-INVENTORY root, not the file's
/// digest.**
///
/// The `.palwart` is the one `qwen25-convert --a16` produces from `Qwen/Qwen2.5-1.5B-Instruct`
/// (sha256 `a8c4e53e…`, 1,795,427,276 bytes). Its `Base0ArtifactV1::artifact_digest()` is
/// `c00faa48…`, and **that is what this constant used to hold, and it was the wrong root form.**
/// The row the chain registers is the court-capable `graph-v2` row, and
/// `CanonicalClassV1::artifact_root` derives that row's root as
/// `a16_inventory_v1(artifact, profile).root()` — because an arithmetic close's openings prove
/// against the registered root, and nothing can be opened against a flat digest. The producer's
/// resolver compares the inventory root of every holding against this constant, so with the digest
/// pinned here seat2 refused its own artifact nine times at Relaunch 5 ("holds no artifact whose
/// registered root form is c00faa48…") and the dense tier produced zero blocks. Measured
/// 2026-09-02 with `misaka/palw/base0/tests/a16_root_probe.rs` over the deployed file:
/// `artifact_digest = c00faa48…`, `inventory_root = 1a7457f1…`. The comment on
/// `CanonicalClassV1::artifact_root` names this exact failure ("deciding it here a second time is
/// the two-mappings defect"); this constant was the second mapping.
///
/// Re-pinning it moves `palw_ruleset_id_v2` and therefore `consensus_params_id`; the genesis hash
/// does not move.
///
/// The dense tier the network registers is W8A16, not the floor's W8A8: static PTQ of this
/// checkpoint into a seven-bit activation stream degenerates (ADR-0053), while on A16 the same
/// weights score FAITHFUL against their own float reference (top-1 45/57, top-5 56/57, rank
/// correlation 0.893, per-layer stream cosine 0.98–1.00) and answer "The capital of France is
/// Paris." at 33 tokens per second. A class is its graph, so those are two classes and only one of
/// them is worth a network's cadence.
///
/// Zero means the class is absent — the two-class network exactly as before, never a placeholder.
pub const PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([
    0x1a, 0x74, 0x57, 0xf1, 0x00, 0xd9, 0xfb, 0x0f, 0x34, 0x06, 0xd8, 0x82, 0xb4, 0xb5, 0xbc, 0xd7, 0xe2, 0xeb, 0xcc, 0xcd, 0x54,
    0xed, 0xc5, 0x26, 0x8a, 0x08, 0xc3, 0xa8, 0x5b, 0xc6, 0xc8, 0xd3, 0xad, 0xac, 0xdf, 0x34, 0x5c, 0xde, 0x3c, 0xb7, 0x2f, 0xfe,
    0x8e, 0xd7, 0xfe, 0x7a, 0x2f, 0x72, 0x9d, 0x10, 0xf0, 0x08, 0x21, 0xf9, 0x4b, 0x1e, 0x85, 0x62, 0xe4, 0xe2, 0x17, 0xb7, 0x27,
    0x08,
]);

/// **The genesis root of ADR-0082's graph-v5 dense row — the same value, and that is the
/// measurement rather than a shortcut.**
///
/// The 5f card registers the dense tier as the graph-v5 512 row
/// ([`crate::palw_qwen25_profile::qwen25_a16_graph_v5_registration_v1`]), which is a court-capable
/// row and therefore pins an INVENTORY root, not a container digest
/// (`misaka_palw_base0::classes::CanonicalClassV1::artifact_root`). Two things could have moved it
/// and neither does:
///
/// * **the graph.** Decision 1 fuses four attention nodes into one; it moves no tensor, so the
///   operand inventory the root is taken over is the same set in the same order.
/// * **the tokenizer binding.** The file the cut ships is the re-bound conversion, whose digest is
///   `158314b5…` against the deployed file's `c00faa48…` — binding writes the commitment field, and
///   the commitment field is not an operand.
///
/// **Measured 2026-09-03 with `misaka/palw/base0/tests/a16_root_probe.rs`
/// (`print_a16_v5_root_forms`) over BOTH files under `palw_a16_context_row_profile_v5(512)`:**
///
/// | file | `artifact_digest()` | `a16_inventory_v1(.., v5).root()` |
/// |---|---|---|
/// | `/Users/wata/Downloads/qwen25-1.5b-a16.palwart` (deployed) | `c00faa48…` | `1a7457f1…` |
/// | `instruct-bound.palwart` (the bound file the cut ships) | `158314b5…` | `1a7457f1…` |
///
/// So this is an ALIAS and not a second byte array: the two rows are one file's one inventory, and
/// a second spelling of a value that is equal by measurement is the two-mappings defect this
/// family has already paid for once. If a future conversion ever moves the operand set, the probe
/// above is what says so — and then this becomes its own constant, with its own measurement.
pub const PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT: crate::Hash64 = PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT;

/// **The 2M Qwen2.5 artifact's root — the weights testnet-12's dense row is registered over.**
///
/// NOT [`PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT`], and the difference is the point. ADR-0103's
/// graph-v7 is "the same artifact under the held map", which is true of the GRAPH and false of this
/// artifact: a 2M row needs a rotary table that covers 2M positions, so the fleet re-converted the
/// model (`qwen25-convert --a16 --n-ctx 2097152`, 2.87 GB against the 512-span artifact's 1.80 GB)
/// and a re-conversion is a different inventory and therefore a different root.
///
/// **The 2M Qwen2.5 class's operand-inventory root, read out of a committed measurement.**
///
/// NOT a byte array a human pasted in, and that is the whole content of this constant. The previous
/// two spellings of this value were hand-pinned flat `artifact_digest()`s — on testnet-11, recorded
/// in [`PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT`]'s own comment, and then again here on testnet-12,
/// where all four seats logged `holds no artifact whose registered root form is b5baca63…` from
/// genesis and the dense tier produced zero blocks over a byte-identical artifact.
///
/// A registration must pin the root an arithmetic close's openings prove against, which for a
/// court-capable A16 row is `a16_inventory_v1(artifact, profile).root()` and never the file's digest.
/// Deriving it needs the artifact, which a compiling binary does not have — so the artifact's
/// `.palwmanifest`, 660 bytes written by `palw-class manifest` from the deployed file, is committed
/// beside this card and parsed at compile time. There is no second copy of the value to disagree
/// with the first; changing what this genesis registers means re-running the CLI and committing its
/// output.
///
/// Measured on the deployed file (169.58.39.220, 2026-09-23): digest `b5baca63…`, inventory root
/// `f63af2c4…`, over the `graph-v7@2097152` profile whose class id is `74c67e63…`. The root a
/// reader can check is in `config/class-manifests/qwen25-1.5b-a16-2m.palwmanifest`; the pairing of
/// this positional read to that class id is asserted by
/// `t12_genesis_reads_its_root_from_the_committed_manifest`, and any host holding the artifact can
/// re-derive the whole manifest with `palw-class manifest --check`.
///
/// **ADR-0103's graph-v7 is "the same artifact under the held map", which is true of the GRAPH and
/// false of this artifact.** A 2M row needs a rotary table covering 2M positions, so the fleet
/// re-converted the model (`qwen25-convert --a16 --n-ctx 2097152`, 2.87 GB against the 512-span
/// artifact's 1.80 GB), and a re-conversion is a different inventory and therefore a different root.
pub const PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT: crate::Hash64 =
    crate::config::class_manifest_const_v1::inventory_root_of_class(
        crate::config::class_manifest_const_v1::QWEN25_A16_2M_MANIFEST_V1,
        1,
    );

/// **The one scheduled height on testnet-12: ADR-0065 D1's bond-maturity window.**
///
/// 1,000 DAA — the value this tree's own fence sweep uses — and the fence is armed AT it rather than
/// at genesis, because a window cannot have elapsed at block zero. See
/// `palw_t12_arm_every_rule_from_genesis`.
pub const PALW_T12_BOND_MATURITY_WINDOW_DAA: u64 = 1_000;

/// **testnet-12's wide dense row: the held graph-v7 row at ADR-0103's widest context.**
pub const PALW_T12_DENSE_N_CTX: u32 = crate::palw_base0_a16::A16_MAX_ATTN_HISTORY_HELD_V1 as u32;

/// **testnet-12's narrow dense row: the held graph-v7 row at 8,192** (operator decision 2026-09-23).
///
/// The row the fleet can actually produce at. The 2M row is the ladder's widest, and on a fleet host
/// one attempt at it is days of compute and one ready seat of it is ≈ 12 GiB — seven of those (the
/// panel's ready quorum) do not fit the 23 GiB hosts the fleet has. 8,192 is a rung of the same
/// dense ladder (`palw_a16_context_row_profile_v7`), over its own conversion of the same weights.
pub const PALW_T12_NARROW_DENSE_N_CTX: u32 = 8_192;

/// **The hybrid width the fleet's existing Qwen3.6 artifact covers: 512 — NOT a genesis row.**
///
/// testnet-12 registered the held hybrid row at this width until 2026-09-23, when the operator took
/// it out of the genesis set: the graph-v7 held map gathers the GDN convolution over
/// `(2·k_dim + v_dim)·heads` channels, which is the engine's layout only when the key and value head
/// counts agree, and Qwen3.6-35B-A3B's do not (16 against 32) — so every held hybrid attempt fails at
/// prefill and a genesis hybrid row would be a class nobody can produce for. A hybrid row is a
/// registration TRANSACTION on this network (`palw_model_registry`, `palw_context_ladder` and
/// `palw_held_context` are armed from DAA 0), which is the route it takes once the map is corrected.
/// The width stays named because the audit's measurements of the hybrid row read it.
pub const PALW_T12_HYBRID_N_CTX: u32 = 512;

/// **The root testnet-12's narrow dense row registers** — the operand-inventory root of the fleet's
/// `qwen25-1.5b-a16-8k.palwart` (`qwen25-convert --a16 --n-ctx 8192` over the same Qwen2.5-1.5B
/// source as the 2M artifact), read out of its committed sidecar exactly as the 2M row's is.
///
/// A separate conversion and not the 2M file: the 2M artifact's rotary table covers 8,192 positions
/// too, but its root is the 2M row's, and ADR-0143 gives a root one owner.
pub const PALW_T12_GENESIS_QWEN25_A16_8K_ARTIFACT_ROOT: crate::Hash64 =
    crate::config::class_manifest_const_v1::inventory_root_of_class(
        crate::config::class_manifest_const_v1::QWEN25_A16_8K_MANIFEST_V1,
        PALW_T12_QWEN25_A16_8K_MANIFEST_OCCURRENCE,
    );

/// Which row of the 8k sidecar is the `graph-v7@8192` class (1-based, like every occurrence here). The
/// sidecar lists every class the file pairs with, sorted by class id — one, for this file;
/// `t12_genesis_roots_are_all_read_from_committed_manifests` holds this positional read to the class
/// the card registers.
const PALW_T12_QWEN25_A16_8K_MANIFEST_OCCURRENCE: usize = 1;

/// **testnet-12's genesis rows** (operator decision 2026-09-23): the dense family at 8,192 and at
/// 2,097,152, each over its own artifact, and no hybrid row (see [`PALW_T12_HYBRID_N_CTX`]).
///
/// The 2M row is second so that it keeps the registration slot — and therefore the realized share —
/// it had when the hybrid row sat where the 8k row now does.
pub const PALW_T12_GENESIS_HELD_ROWS: [PalwGenesisHeldRowV1; 2] = [
    PalwGenesisHeldRowV1 {
        family: PalwGenesisHeldFamilyV1::DenseA16,
        n_ctx: PALW_T12_NARROW_DENSE_N_CTX,
        artifact_root: PALW_T12_GENESIS_QWEN25_A16_8K_ARTIFACT_ROOT,
    },
    PalwGenesisHeldRowV1 {
        family: PalwGenesisHeldFamilyV1::DenseA16,
        n_ctx: PALW_T12_DENSE_N_CTX,
        artifact_root: PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT,
    },
];

/// **The class ids of testnet-12's genesis held rows** ([`PALW_T12_GENESIS_HELD_ROWS`]): each row's
/// graph-v7 profile id at its own width — the id its registration binds (`class_id =
/// profile.shape_profile_id()`). What ADR-0152's C7 list must be a subset of.
pub fn palw_t12_genesis_held_class_ids_v1() -> Vec<crate::Hash64> {
    PALW_T12_GENESIS_HELD_ROWS
        .iter()
        .filter_map(|row| {
            let profile = match row.family {
                PalwGenesisHeldFamilyV1::DenseA16 => crate::palw_context_ladder::palw_a16_context_row_profile_v7(row.n_ctx),
                PalwGenesisHeldFamilyV1::HybridQwen36 => crate::palw_context_ladder::palw_qwen36_context_row_profile_v7(row.n_ctx),
            };
            profile.ok().map(|p| p.shape_profile_id())
        })
        .collect()
}

/// **ADR-0152 C7 on testnet-12: `[the 2M class id]`**, pinned — the dense genesis row at
/// [`PALW_T12_DENSE_N_CTX`], held to Final (U1: "2M held to Final"; the 8k row releases per SR-1).
/// A literal because `Params` is built in `const` presets; `the_t12_c7_list_is_the_2m_row` holds it
/// equal to [`palw_t12_rcore_conservative_classes_v1`]'s derivation, so a moved profile id fails by
/// name instead of pointing C7 at nothing.
/// `74c67e63…902f7a`, the 2M row's class id at this build's graph-v7 profile.
pub const PALW_T12_RCORE_CONSERVATIVE_CLASSES: [crate::Hash64; 1] = [crate::Hash64::from_bytes(PALW_T12_2M_CLASS_ID_BYTES)];
const PALW_T12_2M_CLASS_ID_BYTES: [u8; 64] = [
    0x74, 0xc6, 0x7e, 0x63, 0xd9, 0xc0, 0x3d, 0xaa, 0x05, 0x88, 0x0c, 0x5d, 0x8a, 0x47, 0xb3, 0x54, 0xca, 0x20, 0xe9, 0x52, 0xb1,
    0xa2, 0xd4, 0x9c, 0x10, 0x7a, 0xbe, 0x14, 0xf8, 0x90, 0xa9, 0xc5, 0x07, 0x90, 0x37, 0x1b, 0xb7, 0x15, 0xc7, 0xce, 0xa3, 0x3a,
    0xe8, 0xac, 0x92, 0x13, 0xa3, 0xa6, 0x3d, 0xa4, 0x09, 0x07, 0x0c, 0xb2, 0xc9, 0x8b, 0x8e, 0x86, 0x15, 0x98, 0xdb, 0x90, 0x2f,
    0x7a,
];

/// ADR-0152 C7 on testnet-12, derived: the 2M row's class id (its graph-v7 profile id at
/// [`PALW_T12_DENSE_N_CTX`]).
pub fn palw_t12_rcore_conservative_classes_v1() -> Vec<crate::Hash64> {
    PALW_T12_GENESIS_HELD_ROWS
        .iter()
        .filter(|row| row.family == PalwGenesisHeldFamilyV1::DenseA16 && row.n_ctx == PALW_T12_DENSE_N_CTX)
        .filter_map(|row| crate::palw_context_ladder::palw_a16_context_row_profile_v7(row.n_ctx).ok().map(|p| p.shape_profile_id()))
        .collect()
}

pub fn palw_rc_qwen25_a16_is_registered() -> bool {
    PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT != crate::Hash64::from_bytes([0u8; 64])
}

/// Is the Qwen3.6 class part of this build's network?
pub fn palw_rc_qwen36_is_registered() -> bool {
    PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT != crate::Hash64::from_bytes([0u8; 64])
}

/// **The genesis bond — the three facts code cannot mint, and does not pretend to.**
///
/// Which premine output backs the bond, and the ML-DSA-87 keys that sign under it. An empty
/// verification key is the UNSET state and it is the shipped one: a binary cannot invent an
/// identity, and a placeholder key would be an identity nobody holds the secret for — which is
/// strictly worse than no network, because it looks like one.
///
/// `palw-rc-genesis` turns an operator's values into this block. Until they are pasted,
/// `Params::from(testnet-12)` yields the bundle-free base identity and `kaspad` says so at startup.
///
/// **It is a REGISTRY, not a bond, and that is the correction that matters.** The shipped card
/// carried exactly one bond; `derive_panel_v2` excludes a claim's own executor by bond, by operator
/// AND by key and seats one bond per operator, so a 5-seat panel needs SIX distinct operators. With
/// one bond no claim could ever be licensed: every one would void at `BindTimeout`, `safe_weight`
/// would stay zero, and each block's escrowed worker carve would be burned.
///
/// **Two sentences that used to stand here are no longer true, and both were favourable**
/// (mainnet audit, 2026-09-05). This said "`BondRegistered` may not ride a transaction, so a
/// registry too small has no later repair" and "`verify_palw_genesis_v2` now refuses such a
/// registry outright". A `BondRegistered` DOES ride a transaction now
/// (`palw_lifecycle_objects_v2`), and Gate 2 was retired by ADR-0061 — `palw_genesis_v2.rs` says
/// so in its own text: "a genesis may seat zero". So an under-seated card is not refused; it
/// assembles, boots, and runs as a bootstrap phase in which blocks flow on the heartbeat lane,
/// strangers register bonds as transactions, and licensing begins once `seat_count + 1` distinct
/// operators exist.
///
/// That is a designed transitional state on a drill network and a **priced** one on a network with
/// value: until it ends, every claim voids at `BindTimeout` and its escrowed worker reward is
/// destroyed, one block at a time, announced per block by the runtime ("voided claims holding N
/// sompi … destroyed, not paid"). Sizing this list at or above `palw_v2_min_genesis_bonds_v1()`
/// before a mainnet mint is therefore an operator decision with a running cost, not a rule the
/// code enforces.
#[derive(Clone, Copy)]
pub struct PalwRcGenesisBondCard {
    /// Which premine output backs this bond — its collateral outpoint, at `premine_outpoint`
    /// index `0..cards` (a collateral output carved from the main wallet since the 10B-cap
    /// re-genesis 2026-08-30). It is LOCKED while the bond is not retired.
    pub premine_index: u32,
    /// The ML-DSA-87 verification key that signs attempts under this bond.
    pub bond_pubkey: &'static [u8],
    /// The operator identity key. **One seat is one operator**, so every row here must carry a
    /// DIFFERENT one — a registry of clones seats nobody however long it is.
    pub operator_pubkey: &'static [u8],
    /// The 64-byte P2PKH-ML-DSA-87 owner payload this bond's matured rewards are paid to.
    pub payout_payload: [u8; 64],
}

// **The testnet-12 genesis registry, minted 2026-08-22.** Six bonds over premine collateral
// outputs 0..=5 (carved from the main wallet since the 10B-cap re-genesis 2026-08-30), each
// with a DISTINCT operator key: `derive_panel_v2` excludes a claim's own executor by
// bond, by operator and by key and seats one bond per operator, so a 5-seat panel needs six.
// (This used to add "`BondRegistered` may not ride a transaction, so a registry too small has no
// later repair". It does ride one now — see the card struct's doc, mainnet audit 2026-09-05 — so
// the repair exists and what a short registry costs is burned escrow until it arrives.)
//
// Every key was generated on the host that holds it (`kaspa-pq-validator keygen`, 0600) and only
// the public row travelled; each payout is that bond key's own address, so a matured reward is
// spendable by the seed that signs for it. Assembled and gate-checked by
// `palw-rc-genesis --rows`, which runs `palw_rc_params_from_artifacts` — the same call a node
// makes at boot.
//
//   palw_ruleset_id     9bec83e7…757a066d
//   consensus_params_id c4a381f66d80c0a6afd01c43ad06ddff1b941e07d74836d259781e605bfe553c
//
// Every node must ship this table byte-for-byte: it is inside `consensus_params_id`, so a node
// that ships a different one is refused at the handshake rather than forking.
/// **Mainnet's PALW genesis bond registry — empty until it is real.**
///
/// The switch that turns PALW on for mainnet is this list plus
/// [`PALW_MAINNET_GENESIS_ARTIFACT_ROOT`]. While it is empty, `genesis_premine_utxos_for` carves
/// nothing, `mainnet_shipped_params` returns `MAINNET_PARAMS` unchanged, and mainnet is the
/// hash-only network it is today — byte-identical, fingerprint included.
///
/// **Why it is empty rather than filled with placeholders.** Every card carries an ML-DSA-87 key
/// generated on the host that will hold it; only the public row travels. A fabricated key is a
/// seat nobody can ever sign a receipt for — a permanently silent juror on a registry that,
/// today, has no spare seat to absorb one. So this cannot be populated from inside the tree: it
/// needs operators, and populating it re-mints mainnet's genesis (the collateral outputs are part
/// of the premine, so the genesis UTXO commitment moves with them).
///
/// It must hold at least [`crate::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1`] rows if
/// ADR-0065 D1 is ever to be armed here — `validate_palw_v2` refuses a smaller one — and never
/// fewer than `palw_v2_min_genesis_bonds_v1`, which `verify_palw_genesis_v2` refuses.
pub const PALW_MAINNET_GENESIS_BONDS: &[PalwRcGenesisBondCard] = &[];

/// Mainnet's BASE-0 artifact root. Zero means "no ruleset to assemble", exactly as it does for the
/// RC card, and `mainnet_shipped_params` falls back to the hash-only `MAINNET_PARAMS`.
pub const PALW_MAINNET_GENESIS_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([0u8; 64]);

/// **Mainnet's hybrid tier, as a pinned artifact root.** Zero means the tier is absent, exactly as
/// zero means "no card" above — an unpinned class is not a placeholder.
///
/// Mainnet had no spelling for this at all until the mainnet audit, and its absence was not
/// visible from `mainnet_shipped_params`: that function assembled through the class-FREE builder
/// while its own comment described ADR-0068's table as "{floor ≈ 20‰, model classes ≈ 980‰}". A
/// carded mainnet would therefore have been born a floor-only network — the LLM lane the ADR is
/// about would not have existed on it, and the 980‰ would have had nowhere to go. The share table
/// is still a pin-time decision; what changed is that there is now a pin to make.
pub const PALW_MAINNET_QWEN36_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([0u8; 64]);

/// Mainnet's dense tier. Optional in the same sense the RC's is: `None` is the two-class network.
pub const PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT: crate::Hash64 = crate::Hash64::from_bytes([0u8; 64]);

/// Is mainnet's hybrid tier pinned? The model tiers ride the class-registering assembly only when
/// it is; otherwise mainnet assembles the floor alone, which is what it does today.
pub fn palw_mainnet_qwen36_is_registered() -> bool {
    PALW_MAINNET_QWEN36_ARTIFACT_ROOT != crate::Hash64::from_bytes([0u8; 64])
}

/// …and the dense tier, on the same terms.
pub fn palw_mainnet_qwen25_a16_is_registered() -> bool {
    PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT != crate::Hash64::from_bytes([0u8; 64])
}

mod rc_genesis_bonds;
pub use rc_genesis_bonds::*;

/// **testnet-12's eighth genesis card, re-keyed 2026-09-23.** Card 7 of [`PALW_RC_GENESIS_BONDS`] has
/// no key on any fleet host, so on testnet-12 it would be a registered seat nobody can sign for — a
/// permanently silent juror on a registry whose class lifecycle needs seven ready seats. The
/// operator's decision: a new bond key and a new operator key, generated on 5.104.81.23 with
/// `misaka key gen` (`/etc/misaka/t12/t12-bond-7.key`, `t12-operator-7.key`, mode 0600, never
/// copied off the host) and turned into this row there by `palw-rc-genesis --emit-row`, which reads
/// the seeds and prints only public values. The payout is the bond key's own address
/// (`misakatest:qt8swf72…`), so the fee float the premine carves for this card (index 48) is
/// spendable by the key that signs under it, like every other card's.
///
/// Same premine index (7), so the collateral outpoint is unchanged; testnet-11 keeps its card.
pub const PALW_T12_GENESIS_BOND_7: PalwRcGenesisBondCard = PalwRcGenesisBondCard {
    premine_index: 7,
    bond_pubkey: &[
        0xd7, 0x87, 0x50, 0xbb, 0x18, 0xb2, 0x54, 0xb9, 0xd5, 0xd2, 0x80, 0x8d, 0x82, 0x24, 0x21, 0xcf, 0x60, 0x25, 0xf1, 0x55, 0x0c,
        0xa3, 0x14, 0x1f, 0x7b, 0x57, 0x9a, 0x2e, 0xcb, 0x99, 0x26, 0x44, 0xac, 0xec, 0x73, 0x54, 0xc0, 0x48, 0x85, 0x39, 0x23, 0x14,
        0xa6, 0x8f, 0x94, 0x75, 0x45, 0x4b, 0x6d, 0x53, 0x54, 0x59, 0xcc, 0x68, 0x3a, 0xb2, 0xc7, 0xa1, 0x7c, 0xa3, 0xdb, 0x4d, 0x08,
        0x5f, 0x99, 0x91, 0x1f, 0xc7, 0xcf, 0x52, 0x35, 0xe5, 0x6c, 0x4d, 0xe1, 0x45, 0xcd, 0x6f, 0xb9, 0x0e, 0x9b, 0x0b, 0xb2, 0x69,
        0x82, 0xd6, 0xaf, 0x1c, 0xf1, 0x24, 0xd1, 0x8b, 0xd5, 0x4e, 0x1c, 0x19, 0x29, 0x33, 0x19, 0x48, 0xc4, 0x08, 0x06, 0xf6, 0x7e,
        0x42, 0x18, 0x33, 0xda, 0x4d, 0xa6, 0x97, 0xc1, 0x32, 0x2c, 0xac, 0x5b, 0xdd, 0x79, 0xa2, 0x6b, 0x69, 0x28, 0xb6, 0x3f, 0x06,
        0x77, 0xb4, 0x0c, 0xbe, 0xae, 0xfb, 0x27, 0xfa, 0xdd, 0xcb, 0x74, 0xc6, 0xd9, 0x89, 0x27, 0x55, 0xd8, 0x03, 0x7e, 0xa8, 0xeb,
        0xd8, 0x2a, 0x9b, 0x33, 0x94, 0x1f, 0xa8, 0xb6, 0x80, 0x1d, 0x1f, 0x80, 0x91, 0x75, 0xe9, 0x8f, 0x26, 0x23, 0x75, 0xa1, 0xdc,
        0xd2, 0x35, 0x35, 0xbb, 0x51, 0xc8, 0xfb, 0x9c, 0x7d, 0x00, 0x1d, 0x68, 0x35, 0x52, 0xaf, 0x16, 0xcb, 0xfd, 0xa6, 0x23, 0xd4,
        0x4f, 0xc7, 0xfc, 0x3b, 0x00, 0xba, 0xdd, 0xfe, 0x46, 0xa4, 0x24, 0xa0, 0xa6, 0xed, 0x0b, 0xc2, 0xff, 0x32, 0xe4, 0x85, 0x46,
        0xee, 0x60, 0xae, 0x7e, 0x74, 0xac, 0x10, 0xdc, 0x0e, 0xbc, 0x61, 0x03, 0xda, 0xf6, 0xc8, 0x8a, 0xaa, 0xfd, 0xc7, 0x83, 0xbe,
        0xa5, 0xbe, 0x97, 0xa8, 0xfe, 0x3d, 0x6a, 0x25, 0x25, 0x39, 0x41, 0x1d, 0xf3, 0xb1, 0xb5, 0x29, 0x7f, 0xaf, 0xe2, 0xb6, 0x42,
        0x5c, 0x23, 0x89, 0xce, 0xa3, 0x32, 0x2b, 0x5e, 0xf5, 0x40, 0xfc, 0x48, 0xc4, 0x0b, 0x92, 0x08, 0xd0, 0x93, 0x9f, 0xc3, 0x7b,
        0x77, 0x32, 0xbf, 0xbc, 0x9b, 0x7f, 0x20, 0xc9, 0xeb, 0x22, 0x53, 0x6f, 0x39, 0xbd, 0x8c, 0x3a, 0x8a, 0x5a, 0x6d, 0xaa, 0x25,
        0x40, 0xa7, 0x76, 0x26, 0x17, 0x12, 0xcc, 0x70, 0xe5, 0xb3, 0xf4, 0xdd, 0x8d, 0x1b, 0x62, 0xa7, 0x41, 0x1a, 0xe4, 0xf7, 0xb6,
        0xa9, 0xb0, 0x70, 0xbf, 0x36, 0x2e, 0x52, 0xf8, 0x19, 0x1a, 0xb4, 0x10, 0xcb, 0xe8, 0xb3, 0x75, 0xb9, 0xa1, 0x66, 0xfb, 0x37,
        0x97, 0x16, 0x4d, 0x94, 0x48, 0x0a, 0x35, 0x48, 0xbe, 0xbb, 0x2e, 0x04, 0x34, 0x42, 0x36, 0xf8, 0xd4, 0xc3, 0x50, 0x72, 0x39,
        0x7a, 0xee, 0xe9, 0x0c, 0x83, 0x1d, 0xfd, 0x66, 0xec, 0x63, 0xe9, 0xab, 0x3c, 0x1e, 0x7a, 0x23, 0x05, 0x46, 0xcc, 0x32, 0x51,
        0xa0, 0x2c, 0xce, 0x54, 0xb2, 0x9a, 0x41, 0x05, 0x45, 0xf1, 0xc5, 0x02, 0x34, 0xd8, 0x49, 0xc3, 0xf5, 0x12, 0x16, 0x04, 0x29,
        0xa3, 0xad, 0xa5, 0x4f, 0x40, 0xec, 0x1a, 0xf0, 0x00, 0xa4, 0x00, 0x5e, 0x9c, 0x72, 0x90, 0x43, 0x1f, 0x08, 0xef, 0x86, 0x5c,
        0x1d, 0x43, 0xb8, 0xa2, 0xbe, 0x92, 0x4f, 0x61, 0xb9, 0x0c, 0xde, 0x80, 0x48, 0x9b, 0xa8, 0x76, 0x8b, 0x73, 0x95, 0x8f, 0xeb,
        0x15, 0x90, 0x76, 0xf5, 0x0e, 0x1d, 0xaa, 0xf0, 0x89, 0x48, 0xe1, 0x1e, 0x65, 0xef, 0x0a, 0x70, 0x54, 0xd1, 0x24, 0xb9, 0x78,
        0xe4, 0x35, 0xdb, 0xea, 0x96, 0x43, 0xef, 0x05, 0xcf, 0x8d, 0xad, 0x69, 0x9b, 0xdc, 0xc2, 0x52, 0xc7, 0xa5, 0xc0, 0xb5, 0x6b,
        0xcd, 0x3c, 0xa6, 0x0f, 0x51, 0x92, 0x09, 0x31, 0x72, 0x2f, 0x55, 0x9c, 0x0a, 0x3b, 0x16, 0x39, 0x13, 0x5a, 0xff, 0x6a, 0xdb,
        0xe0, 0xf3, 0x8b, 0x60, 0x05, 0x1d, 0x3e, 0x4c, 0x6f, 0xcc, 0xb6, 0xfd, 0x3b, 0x32, 0xe2, 0x2b, 0xa8, 0x86, 0xb9, 0x83, 0xdd,
        0x5a, 0x6e, 0x9c, 0x6d, 0xbb, 0x10, 0xb3, 0x50, 0x76, 0x84, 0xe8, 0x69, 0xa5, 0x92, 0xfc, 0x61, 0xb3, 0xe5, 0x75, 0xc1, 0x16,
        0x2f, 0xa9, 0x2c, 0xc3, 0x41, 0xfc, 0x4b, 0x0b, 0x6d, 0x38, 0xae, 0x1b, 0xdc, 0x8a, 0x88, 0xe0, 0x87, 0x89, 0x49, 0xd6, 0xf5,
        0x08, 0x7e, 0x0f, 0xf7, 0x47, 0xec, 0xa6, 0x66, 0x99, 0xd2, 0x37, 0xe5, 0xc0, 0x22, 0xa2, 0x1f, 0x5b, 0x76, 0xa6, 0x33, 0xfc,
        0x99, 0x62, 0xd2, 0x97, 0x96, 0xea, 0x6e, 0xc3, 0xfe, 0x43, 0x76, 0x97, 0x7e, 0x31, 0x1a, 0x13, 0xb7, 0xb6, 0xb9, 0x8f, 0xc6,
        0x6e, 0xfb, 0x4e, 0xf8, 0xd6, 0xd4, 0x68, 0xec, 0xe1, 0xc7, 0x23, 0x9e, 0xb9, 0x40, 0xdd, 0x33, 0xd2, 0x5c, 0xc6, 0x17, 0x84,
        0xa8, 0x29, 0xaf, 0xad, 0x81, 0x25, 0x50, 0x45, 0x18, 0x85, 0x6b, 0x97, 0xfd, 0xb4, 0x0d, 0x2f, 0x80, 0x4c, 0x25, 0xf8, 0xfc,
        0x0a, 0x5d, 0xbd, 0x25, 0x3c, 0x7b, 0x15, 0x3f, 0x67, 0x0c, 0x3b, 0x64, 0x2f, 0xfd, 0x7e, 0xec, 0x8f, 0x7f, 0x08, 0xf4, 0x4c,
        0xca, 0xf3, 0xe7, 0xad, 0x30, 0xec, 0xf8, 0x13, 0x5e, 0x50, 0x61, 0x72, 0xfe, 0xa4, 0xd5, 0x2c, 0x80, 0xca, 0xff, 0xb2, 0x82,
        0x23, 0x1b, 0x90, 0xe1, 0xe3, 0xcd, 0x8a, 0xaf, 0xb4, 0xc7, 0xad, 0xc9, 0xe2, 0x9d, 0xfc, 0x71, 0x4a, 0xe8, 0x54, 0x6b, 0x20,
        0x5f, 0xf1, 0x0c, 0xb9, 0xcb, 0x26, 0x0d, 0x9b, 0x77, 0xbb, 0x2d, 0x16, 0xaa, 0x0a, 0xb1, 0xf6, 0x0b, 0x96, 0x69, 0x4a, 0xd6,
        0x65, 0xc7, 0x1e, 0x69, 0x2f, 0xd4, 0x65, 0x27, 0xc9, 0xa7, 0xb6, 0x8f, 0x3c, 0xa8, 0x25, 0x2b, 0xaa, 0x66, 0x3c, 0x01, 0xc0,
        0x27, 0x64, 0x1d, 0xe0, 0x21, 0x9f, 0xda, 0xe1, 0xc8, 0x1e, 0x8e, 0x6a, 0xe9, 0x01, 0x1c, 0xb3, 0xeb, 0xae, 0x88, 0xba, 0x9d,
        0xdb, 0xdc, 0xec, 0x93, 0xba, 0x7f, 0x09, 0x3b, 0x42, 0xeb, 0x34, 0x4f, 0x3e, 0xfd, 0x14, 0xfe, 0xb6, 0xc4, 0x9c, 0x5c, 0xc3,
        0xbb, 0x16, 0x80, 0x4c, 0x3f, 0xc9, 0x29, 0xff, 0x07, 0x37, 0x0a, 0x39, 0x72, 0x98, 0x1b, 0x8a, 0x02, 0x76, 0x7d, 0xda, 0x9f,
        0x36, 0xf3, 0x4e, 0x91, 0x3c, 0x0b, 0x03, 0xbd, 0x2b, 0x37, 0x51, 0xbe, 0xc7, 0xd2, 0x22, 0x11, 0x18, 0xf8, 0xaf, 0x03, 0x79,
        0xc9, 0xba, 0x89, 0xa9, 0xb4, 0x0c, 0x99, 0xcf, 0xc3, 0x53, 0x59, 0x2e, 0x31, 0xc1, 0xf3, 0xba, 0xe9, 0x62, 0xf9, 0x7c, 0x9c,
        0x5d, 0x89, 0xd4, 0x0e, 0xcf, 0xc0, 0x0b, 0x65, 0xa0, 0xe1, 0x2c, 0x07, 0x61, 0x3e, 0x43, 0x78, 0x62, 0xdc, 0x71, 0x65, 0x87,
        0xb0, 0x33, 0x72, 0xc9, 0x49, 0x84, 0x6d, 0x73, 0x26, 0x0f, 0x13, 0x09, 0x68, 0xab, 0x19, 0x3b, 0x37, 0xaf, 0x69, 0x30, 0x4c,
        0xf7, 0x94, 0x97, 0x69, 0x01, 0x51, 0x11, 0x36, 0x5b, 0x48, 0xfb, 0x4d, 0xc7, 0x72, 0x16, 0x51, 0x26, 0x36, 0xab, 0xe2, 0x89,
        0x42, 0x9e, 0x6c, 0x9e, 0xbc, 0x45, 0x5f, 0x99, 0x9c, 0x9f, 0xf8, 0x3b, 0xca, 0xfe, 0x86, 0x0e, 0x75, 0x04, 0x41, 0x31, 0x7c,
        0x7c, 0x61, 0x1a, 0xe3, 0xa1, 0x49, 0x14, 0xd1, 0x8e, 0x92, 0x37, 0x33, 0xab, 0x9e, 0x2a, 0x0f, 0xa6, 0xfc, 0xa4, 0xfc, 0x41,
        0x55, 0x3e, 0x49, 0xee, 0xd8, 0x41, 0xfc, 0xca, 0x72, 0x70, 0x93, 0xe6, 0xdd, 0x33, 0x46, 0x9f, 0x72, 0x42, 0xeb, 0x9a, 0xbe,
        0x92, 0xc4, 0xce, 0xd6, 0xab, 0x17, 0x65, 0xd7, 0x62, 0xa9, 0xfb, 0x89, 0xff, 0x34, 0x14, 0xe3, 0xde, 0x40, 0x40, 0xa9, 0xc9,
        0x2f, 0x19, 0x75, 0x72, 0x96, 0x96, 0x26, 0xa6, 0xf6, 0xb9, 0x94, 0xda, 0x7c, 0x4b, 0x57, 0x5b, 0x93, 0xed, 0xf4, 0xf5, 0xbe,
        0x4d, 0x72, 0x18, 0x8f, 0xc2, 0xa1, 0x60, 0xa4, 0xe3, 0x0b, 0x01, 0x98, 0x02, 0xd3, 0xa8, 0xc0, 0xd7, 0x0a, 0x8b, 0x6c, 0xf0,
        0x66, 0x4f, 0x12, 0x5d, 0x9a, 0x18, 0xc4, 0xb5, 0x08, 0xda, 0x79, 0x7d, 0x69, 0x29, 0xd0, 0x08, 0x6b, 0xfe, 0x38, 0xfe, 0x96,
        0x78, 0xbb, 0x47, 0xb0, 0x3a, 0xb9, 0x6a, 0xd9, 0x00, 0x0a, 0xc2, 0x32, 0x34, 0x00, 0x8c, 0xda, 0x5f, 0xbd, 0xc8, 0x27, 0x4d,
        0xcb, 0xd8, 0x7c, 0xe8, 0x8c, 0xb3, 0xbf, 0x4d, 0xc2, 0xcc, 0x9a, 0xcd, 0x17, 0x05, 0xf9, 0x07, 0x3f, 0xf5, 0xca, 0x5b, 0x52,
        0x3a, 0x92, 0xff, 0x18, 0xd2, 0xf8, 0xa2, 0x0c, 0x7b, 0x79, 0xca, 0x78, 0xef, 0xe9, 0x79, 0xc1, 0xf2, 0x7d, 0x26, 0x49, 0xec,
        0x76, 0x50, 0xa2, 0xa9, 0x3b, 0x87, 0xab, 0x17, 0x9c, 0x28, 0x25, 0x29, 0xaa, 0x31, 0x0f, 0xdd, 0xb7, 0xc5, 0x40, 0x7f, 0x6d,
        0xb5, 0x04, 0xea, 0xa7, 0xe7, 0x40, 0x47, 0xa4, 0xb1, 0x69, 0xb5, 0xf6, 0x1a, 0xbb, 0xd4, 0x67, 0xda, 0x00, 0x31, 0x42, 0xce,
        0x65, 0x99, 0xc6, 0x33, 0x63, 0x3a, 0x6e, 0xce, 0xce, 0xbc, 0x43, 0x2f, 0x33, 0x08, 0x65, 0x56, 0x2d, 0xfc, 0xd0, 0x60, 0x85,
        0x54, 0x1d, 0x84, 0xca, 0xe4, 0x80, 0xc0, 0xd7, 0x70, 0xa6, 0xc7, 0x14, 0x16, 0x70, 0x91, 0x85, 0xff, 0x1b, 0xab, 0x97, 0x93,
        0x9b, 0x36, 0xd2, 0xc5, 0xfb, 0x4f, 0x4a, 0x8b, 0x11, 0xc6, 0x35, 0xa8, 0x6a, 0x0b, 0x15, 0x2b, 0xd5, 0x68, 0xd7, 0x93, 0xe9,
        0x5a, 0xd0, 0x1b, 0xde, 0x07, 0x2d, 0x34, 0x29, 0x83, 0x83, 0xde, 0x40, 0x4b, 0x64, 0xba, 0x13, 0x1d, 0xf2, 0xf8, 0xe4, 0xae,
        0xd8, 0x33, 0x87, 0xb2, 0x68, 0xf1, 0x4e, 0x87, 0xf2, 0x19, 0xa7, 0x9a, 0x27, 0xd2, 0x73, 0xb4, 0xa5, 0xa3, 0xd1, 0x08, 0xf1,
        0xd4, 0x8b, 0xce, 0xc6, 0x06, 0x74, 0x36, 0x75, 0x2a, 0x17, 0xa7, 0x00, 0xe2, 0xc2, 0x9c, 0x6a, 0x72, 0x9f, 0xc7, 0x6f, 0x1d,
        0x5a, 0xc9, 0x45, 0x7a, 0x3b, 0x83, 0x01, 0xb5, 0xe5, 0x70, 0x05, 0x3c, 0xf8, 0x86, 0xa3, 0xe1, 0xcf, 0xca, 0x90, 0x31, 0x22,
        0x9f, 0x10, 0x14, 0xb3, 0x7d, 0x80, 0x41, 0x2c, 0x34, 0x1c, 0x4b, 0xc1, 0x3f, 0xe2, 0xa8, 0xa3, 0xf1, 0xb2, 0x49, 0x91, 0xda,
        0x41, 0xd1, 0x34, 0x00, 0xda, 0x12, 0x24, 0x92, 0x0b, 0x6d, 0xc9, 0x30, 0xc6, 0x21, 0x07, 0xeb, 0xdb, 0x60, 0xcd, 0xbb, 0xc7,
        0x83, 0x3f, 0xd4, 0x0e, 0x01, 0x2e, 0x9d, 0xcb, 0xe1, 0x3a, 0x77, 0xae, 0x8d, 0x9f, 0x78, 0x92, 0x01, 0xba, 0x23, 0x30, 0x4d,
        0x30, 0x4f, 0x06, 0xe2, 0x37, 0x7c, 0x35, 0xd3, 0x84, 0x20, 0x0a, 0x15, 0x2e, 0x84, 0x80, 0xb6, 0xde, 0xa5, 0x51, 0x09, 0x26,
        0xde, 0x00, 0xde, 0xaa, 0x51, 0x86, 0xed, 0x73, 0x39, 0x4a, 0xca, 0x79, 0x51, 0x78, 0xfa, 0x46, 0x85, 0x7b, 0xac, 0xc8, 0x65,
        0x7b, 0x61, 0xa4, 0xbc, 0xdb, 0xa8, 0x90, 0x16, 0x87, 0x00, 0x5b, 0x77, 0xa1, 0x71, 0xf8, 0x36, 0xb6, 0xda, 0xd7, 0x2c, 0xdf,
        0x35, 0xdb, 0xa7, 0xfb, 0xd2, 0x08, 0xfe, 0x4e, 0xd4, 0x03, 0x6d, 0x69, 0x57, 0xd2, 0x09, 0x0c, 0xf5, 0xa8, 0xbb, 0x89, 0x57,
        0x10, 0x02, 0xe8, 0x77, 0x9b, 0x23, 0x44, 0xec, 0x5f, 0xf1, 0x71, 0x4b, 0x44, 0x44, 0x56, 0x01, 0x29, 0xab, 0x14, 0xbb, 0x7d,
        0x85, 0xd8, 0x13, 0x64, 0xc9, 0x3a, 0xea, 0x7b, 0xa1, 0xdc, 0x42, 0xef, 0x7e, 0xfc, 0x6b, 0x96, 0x4c, 0x9a, 0xd6, 0xb7, 0xff,
        0xf6, 0x33, 0xda, 0x5c, 0x30, 0xc2, 0x04, 0x24, 0x74, 0xd8, 0xc8, 0xd4, 0x10, 0x13, 0xf7, 0x98, 0x25, 0x51, 0xed, 0xa5, 0xd5,
        0x82, 0x67, 0xf1, 0x02, 0xb1, 0x80, 0x5d, 0x8e, 0xfd, 0x6b, 0x40, 0x5e, 0x06, 0x84, 0x77, 0x89, 0xd9, 0x5d, 0x8f, 0x8a, 0x32,
        0xa1, 0xdd, 0xc6, 0xd0, 0x92, 0x0c, 0x3b, 0xc1, 0xb7, 0xcf, 0xcd, 0x19, 0xe0, 0xfa, 0x2e, 0xaa, 0x2c, 0x96, 0x32, 0x4d, 0x27,
        0xce, 0xa8, 0x5e, 0x35, 0xa3, 0x01, 0x97, 0x79, 0x9a, 0x4d, 0x8e, 0x53, 0x91, 0x18, 0x51, 0x62, 0x01, 0xbf, 0xaa, 0x77, 0xee,
        0xb7, 0xa2, 0x93, 0xe9, 0x19, 0xb8, 0x41, 0x1a, 0xb1, 0x50, 0x61, 0x5e, 0xa8, 0x87, 0x6a, 0x45, 0xf4, 0xe2, 0x9d, 0xde, 0xd6,
        0x0f, 0x98, 0x38, 0xd5, 0x04, 0x4c, 0x70, 0x09, 0xca, 0x05, 0xf4, 0x3b, 0x09, 0xd5, 0x31, 0x1d, 0x57, 0xab, 0x02, 0xc5, 0x1e,
        0xf9, 0x72, 0x30, 0x0f, 0xd2, 0x15, 0xa2, 0x56, 0x6e, 0xf1, 0x14, 0x78, 0xa0, 0x57, 0x7c, 0x0e, 0xce, 0xcf, 0x4a, 0x2e, 0xa2,
        0x95, 0x4c, 0xdd, 0xe2, 0x22, 0xbe, 0xa9, 0x52, 0x99, 0xe0, 0x74, 0xc1, 0xa2, 0xbf, 0x72, 0x6e, 0xbe, 0x37, 0xa4, 0x37, 0x0d,
        0x2c, 0xcd, 0xa2, 0xf5, 0x18, 0x2f, 0x1d, 0x5e, 0xa4, 0xfc, 0xf6, 0xa0, 0xef, 0xa6, 0x71, 0xf0, 0xdb, 0xf1, 0x3d, 0x3a, 0xe8,
        0x8e, 0xc5, 0xc8, 0x72, 0x2d, 0x0c, 0xfc, 0xd2, 0x69, 0x77, 0xdc, 0x15, 0x43, 0x1a, 0x65, 0xae, 0xca, 0x76, 0x0f, 0x16, 0x8e,
        0x49, 0xfe, 0x32, 0xd5, 0x2b, 0x00, 0x15, 0x6e, 0xed, 0x69, 0x48, 0x7d, 0x4d, 0xbb, 0x08, 0x33, 0x2d, 0xa6, 0x96, 0xbe, 0x38,
        0x78, 0x12, 0xab, 0x31, 0xca, 0xf4, 0x4a, 0x4e, 0xc3, 0xa3, 0x0f, 0x0c, 0x36, 0x30, 0x9f, 0xc9, 0xb7, 0x2f, 0xb8, 0xaa, 0x78,
        0x3b, 0x5d, 0xd9, 0x66, 0x17, 0x18, 0x9c, 0x50, 0xa9, 0xf5, 0xec, 0x64, 0xf3, 0xd6, 0x96, 0x6b, 0x14, 0xad, 0x6b, 0x2b, 0x73,
        0xd9, 0x17, 0xf0, 0x9d, 0x99, 0x8e, 0x67, 0x0f, 0xad, 0xf9, 0x52, 0xc6, 0xc2, 0x8e, 0xdc, 0x9c, 0xa9, 0x67, 0x0a, 0xa3, 0xbf,
        0x80, 0x65, 0x8e, 0x46, 0x92, 0x56, 0x83, 0xfc, 0x61, 0x2e, 0x5d, 0x48, 0x3f, 0x1d, 0x75, 0x77, 0x13, 0xdd, 0xfb, 0x96, 0xe7,
        0xf8, 0x60, 0x03, 0x23, 0x74, 0x71, 0x72, 0x7b, 0x78, 0xad, 0x1a, 0x05, 0xf5, 0xc5, 0xff, 0x73, 0x7a, 0x41, 0x95, 0x50, 0xdd,
        0xdd, 0x83, 0x50, 0x44, 0xd3, 0x2c, 0xd8, 0x68, 0xd3, 0xac, 0x56, 0x18, 0xa5, 0xbb, 0x92, 0xb6, 0xb8, 0xc5, 0x10, 0x9f, 0x95,
        0x73, 0x17, 0xdf, 0x78, 0x61, 0xbf, 0xf2, 0x0a, 0x2b, 0x68, 0xe3, 0xfd, 0x2a, 0x56, 0x81, 0xf0, 0xea, 0x74, 0x98, 0xaf, 0x72,
        0xf4, 0x8c, 0xba, 0xa0, 0xce, 0x4d, 0x25, 0x6d, 0xea, 0x7f, 0xb7, 0x77, 0x6b, 0xe6, 0x8b, 0x50, 0x98, 0x90, 0x3a, 0x4e, 0x9e,
        0x82, 0xfc, 0xa3, 0x4d, 0xbe, 0x02, 0xc7, 0x37, 0x01, 0x36, 0x0b, 0x0a, 0x34, 0x70, 0x38, 0x50, 0x03, 0x26, 0x29, 0x20, 0xf3,
        0x06, 0x9d, 0x98, 0x6b, 0xd5, 0x29, 0x0b, 0xf2, 0x0c, 0x6f, 0xa5, 0x58, 0xd8, 0x29, 0x06, 0x24, 0xa5, 0xae, 0xff, 0x4d, 0xf0,
        0xed, 0xeb, 0x67, 0x9a, 0xfa, 0x28, 0x25, 0x9e, 0xd9, 0x7f, 0xb6, 0x3a, 0x73, 0xc3, 0xc9, 0xb7, 0x84, 0xe2, 0x9a, 0x1a, 0x35,
        0x3d, 0x84, 0x42, 0xca, 0x00, 0xc3, 0xde, 0x63, 0xd5, 0xc5, 0xa5, 0x00, 0xfd, 0x4a, 0xbd, 0x51, 0xc5, 0x29, 0xb4, 0xb3, 0x0a,
        0x17, 0xdf, 0xed, 0xf6, 0x67, 0xa5, 0x0f, 0xb2, 0xec, 0x5b, 0xa4, 0xa3, 0x1a, 0xce, 0x87, 0x6a, 0x4b, 0x3a, 0x93, 0x2b, 0x83,
        0xb0, 0x41, 0x0b, 0xf3, 0xcc, 0x9e, 0x6d, 0x7c, 0xdf, 0xd0, 0x7f, 0xdb, 0x2c, 0x28, 0xf8, 0xbd, 0xef, 0x55, 0xd6, 0x0b, 0xb8,
        0x67, 0xc1, 0xf0, 0xd7, 0xed, 0xa0, 0x58, 0x5d, 0x17, 0x54, 0x39, 0x10, 0x6f, 0xf7, 0x2d, 0x5f, 0x3f, 0xaa, 0xcb, 0x4b, 0x72,
        0x6e, 0xae, 0x40, 0x88, 0x99, 0xd6, 0xf3, 0x47, 0xa0, 0xe3, 0x77, 0xca, 0x72, 0x6f, 0x60, 0x9d, 0x65, 0x90, 0x99, 0xb3, 0xa1,
        0xac, 0x2e, 0x69, 0xdd, 0x35, 0xdb, 0xda, 0xed, 0x0f, 0xec, 0x8a, 0x99, 0xa1, 0xe0, 0xb7, 0xf0, 0xc9, 0x29, 0xd6, 0x22, 0x24,
        0x22, 0x28, 0x0d, 0x5c, 0x35, 0x37, 0xa8, 0xce, 0x10, 0x01, 0xd6, 0x84, 0x3c, 0x1e, 0x7b, 0x8d, 0x2c, 0xd3, 0x06, 0x26, 0xed,
        0x69, 0x75, 0xb3, 0x6e, 0xb5, 0xdd, 0xb5, 0x0a, 0x5c, 0x20, 0xd0, 0x40, 0xe9, 0x26, 0x96, 0xb4, 0x0e, 0x94, 0x9b, 0x88, 0x9a,
        0x0e, 0xb8, 0xcd, 0xd0, 0xc6, 0x48, 0xc0, 0x77, 0x65, 0x53, 0x70, 0x2c, 0x69, 0x1b, 0xd4, 0xf5, 0xc4, 0x61, 0xc7, 0xb8, 0xd3,
        0x62, 0x0e, 0x1d, 0x45, 0x53, 0x2f, 0x47, 0x1d, 0x15, 0x8b, 0x37, 0x25, 0x55, 0xcd, 0x53, 0xb3, 0xe4, 0x07, 0xcf, 0xa4, 0xe1,
        0x5f, 0x1d, 0xd2, 0x7d, 0x81, 0x66, 0xcb, 0x5a, 0x61, 0x88, 0x71, 0xc2, 0xa7, 0xce, 0x4b, 0x45, 0xf4, 0x1d, 0x7c, 0xac, 0x27,
        0x50, 0xda, 0xac, 0x5e, 0x2d, 0xc4, 0x0f, 0x21, 0xeb, 0x4f, 0xfa, 0xc2, 0x1f, 0x58, 0xfe, 0xd5, 0xf0, 0x51, 0x90, 0x5c, 0xdf,
        0x62, 0xad, 0xdd, 0x05, 0x97, 0x3d, 0x54, 0xdb, 0xdb, 0xe9, 0xcb, 0x69, 0x4b, 0xfc, 0x38, 0x38, 0x6d, 0xd3, 0x08, 0x56, 0xea,
        0x99, 0x03, 0x0a, 0x46, 0x35, 0x50, 0xf1, 0x11, 0x4a, 0x08, 0x84, 0xfd, 0x4e, 0x63, 0x79, 0xd0, 0x8e, 0x61, 0x7a, 0x21, 0x84,
        0x4d, 0x04, 0x1a, 0x80, 0xf9, 0x58, 0x31, 0x7e, 0xd0, 0xda, 0x17, 0x45, 0x9d, 0x95, 0x64, 0x58, 0xf6, 0x6f, 0x4d, 0x20, 0xff,
        0x01, 0xa3, 0xb7, 0x9f, 0x2a, 0x60, 0xf5, 0x96, 0x06, 0x14, 0x18, 0x22, 0xe6, 0x09, 0xd3, 0x32, 0x0d, 0x1e, 0x86, 0xf4, 0xce,
        0xb1, 0x44, 0x61, 0x60, 0x5e, 0xd9, 0x79, 0xe2, 0x59, 0x2c, 0x7b, 0x46, 0x38, 0xab, 0xaf, 0x82, 0xda, 0xa0, 0xb9, 0x70, 0x9b,
        0xfc, 0x9b, 0xe5, 0x35, 0x69, 0xdc, 0x8e, 0x65, 0x14, 0x9e, 0x60, 0x96, 0x89, 0x72, 0xb7, 0x44, 0xb4, 0x6d, 0x90, 0x75, 0xf7,
        0xd6, 0x91, 0xcf, 0x7c, 0x53, 0x2f, 0xd8, 0x4a, 0x1f, 0xca, 0x17, 0x30, 0x5e, 0xb7, 0xf2, 0x1e, 0x55, 0x52, 0xbe, 0xe0, 0x46,
        0xf1, 0xdd, 0x16, 0x94, 0x32, 0x6e, 0xa4, 0xf8, 0x98, 0x3a, 0xa5, 0xf4, 0x69, 0x7f, 0x0f, 0x48, 0xe0, 0x1f, 0x4b, 0x0a, 0xf2,
        0x1d, 0xc1, 0xa5, 0x95, 0x37, 0xb2, 0x89, 0x62, 0xd3, 0x05, 0x63, 0xd3, 0xc7, 0x1b, 0x11, 0x0b, 0xc6, 0x52, 0xea, 0x19, 0xef,
        0x3d, 0x3f, 0x67, 0xe3, 0xaf, 0x56, 0xee, 0x49, 0xfc, 0xa2, 0x51, 0xef, 0x64, 0xbe, 0xf4, 0x33, 0x86, 0x04, 0xfc, 0x54, 0xc9,
        0xac, 0x04, 0x0c, 0xf5, 0xf7, 0xc9, 0x7f, 0x63, 0x3d, 0xa7, 0xa1, 0xc8, 0x1e, 0x03, 0x9d, 0x29, 0x16, 0xd1, 0x26, 0x57, 0xa4,
        0x81, 0x5e, 0x20, 0x14, 0x08, 0x97, 0xbb, 0x8a, 0xcc, 0x99, 0xe7, 0x04, 0x24, 0x3f, 0x10, 0x58, 0x93, 0xb8, 0x17, 0x8c, 0x3c,
        0xfc, 0xaa, 0x6d, 0xfc, 0xd7, 0x73, 0x9b, 0xd8, 0x95, 0xbf, 0x53, 0xa0, 0x74, 0xc3, 0x79, 0xa9, 0x3f, 0xb7, 0x34, 0x29, 0x7b,
        0x49, 0x28, 0xbe, 0xf4, 0x97, 0x65, 0xf9, 0x28, 0x4d, 0x68, 0x45, 0x54, 0xf6, 0x34, 0xe1, 0x04, 0x4e, 0x31, 0xe3, 0x35, 0xea,
        0xc6, 0xdb, 0x67, 0x84, 0x01, 0xc1, 0x8e, 0x58, 0x1f, 0x5f, 0xd0, 0x02, 0x64, 0xca, 0x7d, 0xff, 0x8b, 0x03, 0x2f, 0x60, 0x15,
        0xb5, 0x0c, 0xad, 0x2f, 0x9d, 0x1e, 0x74, 0x52, 0xb1, 0x46, 0xde, 0xe0, 0x1d, 0x42, 0xf3, 0x12, 0x2f, 0x56, 0x7f, 0xc2, 0x55,
        0xf3, 0x0d, 0x41, 0xc8, 0x5c, 0xb1, 0x17, 0xe4, 0xb8, 0x8d, 0xce, 0x32, 0xf9, 0x67, 0x86, 0xec, 0x0c, 0x2e, 0x96, 0x56, 0xba,
        0xdd, 0xad, 0xa5, 0x5e, 0x51, 0x2d, 0xf3, 0x05, 0xfb,
    ],
    operator_pubkey: &[
        0x45, 0x6c, 0x12, 0x63, 0xd4, 0x6d, 0xb2, 0x3a, 0xe8, 0xa0, 0x8e, 0x42, 0x6e, 0x8b, 0x14, 0x32, 0x6e, 0x1e, 0x61, 0xdf, 0xe8,
        0xba, 0xc7, 0x37, 0x61, 0xd7, 0x0d, 0xf1, 0xad, 0x3c, 0x36, 0x70, 0xea, 0xee, 0xb0, 0x8d, 0x76, 0xe8, 0xfd, 0x4b, 0x70, 0x4c,
        0xe4, 0xe9, 0x45, 0x2f, 0x7c, 0x99, 0x2b, 0xa9, 0x43, 0xe8, 0xaa, 0x43, 0x53, 0x7b, 0x02, 0x60, 0xfe, 0xce, 0xa2, 0xea, 0xc8,
        0x1c, 0xaa, 0xa5, 0x8c, 0xa4, 0x8e, 0x7a, 0xac, 0xa9, 0xfc, 0x7d, 0x5c, 0x13, 0x54, 0x95, 0x9b, 0xb4, 0x8d, 0xdd, 0xf0, 0x58,
        0x5e, 0xc3, 0x64, 0xbe, 0xe2, 0xae, 0xa4, 0x8e, 0xf1, 0xfd, 0x5b, 0xf5, 0x95, 0xcb, 0x79, 0xd0, 0x5c, 0xb5, 0x32, 0x79, 0xd2,
        0x73, 0xb9, 0x19, 0x8d, 0x26, 0xab, 0xe6, 0x66, 0x6e, 0x49, 0xea, 0xde, 0x64, 0xc7, 0xc0, 0xf6, 0xe3, 0xf7, 0xfa, 0xa3, 0x54,
        0xa2, 0x48, 0x31, 0x8a, 0xb0, 0xf6, 0x78, 0x76, 0xd3, 0xa5, 0xbc, 0x75, 0x14, 0xf6, 0xc7, 0x92, 0x3b, 0xe1, 0xd3, 0xe4, 0x70,
        0x61, 0x70, 0x90, 0x26, 0xe4, 0x02, 0x33, 0xf1, 0x8b, 0x13, 0x98, 0x31, 0x32, 0x32, 0x40, 0xbb, 0xb2, 0xe6, 0x2b, 0xf5, 0x81,
        0xb4, 0x49, 0x64, 0x2a, 0xc5, 0xdc, 0xfb, 0x09, 0xfe, 0x29, 0xff, 0x35, 0x41, 0xb0, 0x61, 0xd2, 0x31, 0xa5, 0xa7, 0x59, 0xc8,
        0x3c, 0xe3, 0xcc, 0x36, 0xe9, 0xc9, 0x42, 0x62, 0xf7, 0x08, 0xac, 0xee, 0x56, 0x00, 0xc6, 0x4a, 0x15, 0xb9, 0xe0, 0xb9, 0x36,
        0x00, 0x1e, 0x01, 0x64, 0xcc, 0xea, 0x7f, 0x4d, 0x9d, 0x50, 0x70, 0x27, 0x32, 0xf2, 0x24, 0x28, 0x8e, 0x76, 0x39, 0x20, 0x62,
        0x68, 0x3d, 0xea, 0xa3, 0x9e, 0xc0, 0xbe, 0xa0, 0xb8, 0x5f, 0x34, 0xc6, 0xe2, 0xc1, 0x2f, 0xaf, 0xb2, 0x20, 0x22, 0x46, 0x2b,
        0xa8, 0x80, 0xe9, 0xfe, 0x89, 0x5a, 0x0e, 0xab, 0x1a, 0xcd, 0x5f, 0xc4, 0x8b, 0x01, 0x1c, 0xcb, 0xb3, 0x88, 0x9d, 0x71, 0x22,
        0xe7, 0xae, 0x80, 0x9a, 0x0c, 0x00, 0x57, 0x1a, 0x56, 0xfb, 0xcc, 0xdc, 0xb1, 0xa4, 0xbc, 0x5b, 0x80, 0x77, 0xf5, 0x03, 0xf6,
        0x60, 0x57, 0x95, 0xb5, 0xb5, 0x11, 0x8b, 0x6f, 0x91, 0x43, 0xc7, 0x9a, 0xd2, 0x7c, 0x55, 0xdb, 0x27, 0xb7, 0xae, 0x90, 0x86,
        0x87, 0x63, 0xf3, 0x2f, 0xe4, 0xb0, 0x85, 0xb6, 0x3f, 0x9e, 0x76, 0xbc, 0x1b, 0x73, 0x73, 0x33, 0xbb, 0xef, 0x82, 0x55, 0x5c,
        0x52, 0x45, 0x19, 0xd4, 0x50, 0x65, 0x1d, 0xcf, 0xa7, 0xb3, 0xd8, 0xcc, 0xe4, 0xfb, 0x61, 0xb6, 0xd8, 0xc9, 0x22, 0xc0, 0x91,
        0x8b, 0xc9, 0x90, 0x64, 0x14, 0x64, 0xd2, 0xa1, 0x2e, 0xc7, 0x57, 0x23, 0x64, 0xf9, 0xbd, 0x4e, 0x16, 0xc2, 0x52, 0xb1, 0x76,
        0x42, 0xfe, 0xbe, 0xd5, 0x28, 0xce, 0xa0, 0x13, 0x36, 0x15, 0xf7, 0xd4, 0x0b, 0xb7, 0x43, 0xe8, 0x22, 0x19, 0x33, 0xc2, 0x1f,
        0x6f, 0x3a, 0x67, 0xd9, 0x1d, 0xca, 0x08, 0x39, 0x21, 0xcd, 0xc1, 0x44, 0x8a, 0x7d, 0x44, 0x3f, 0x3b, 0x81, 0x30, 0xb3, 0x3e,
        0xe7, 0x3b, 0x28, 0xd1, 0xdc, 0x91, 0x00, 0x1b, 0xdc, 0xc1, 0xdf, 0xf2, 0x1c, 0xfe, 0xd9, 0x1c, 0x59, 0x8d, 0xf3, 0x31, 0xe7,
        0x0f, 0xf6, 0x82, 0x72, 0xb3, 0xd6, 0xd7, 0x8f, 0xc9, 0xad, 0xa0, 0x0a, 0x4d, 0xb8, 0xae, 0x3c, 0x1f, 0x80, 0xc1, 0x9b, 0xc3,
        0x46, 0xfa, 0x09, 0x03, 0x47, 0xbc, 0x50, 0x0b, 0x9e, 0x1b, 0xee, 0x0e, 0x68, 0x4f, 0xca, 0xb5, 0x21, 0x9a, 0x3a, 0x70, 0x1f,
        0x8e, 0xad, 0xec, 0x42, 0x63, 0xdb, 0x0e, 0xcd, 0x5a, 0x81, 0xf8, 0x10, 0x1e, 0x9f, 0xda, 0x1f, 0x88, 0x5d, 0x19, 0x34, 0xb5,
        0x38, 0x63, 0x1a, 0x10, 0x4b, 0xff, 0xda, 0x99, 0xda, 0xf4, 0x14, 0x99, 0xae, 0x93, 0xb0, 0x55, 0x9b, 0x19, 0x0b, 0x66, 0x4b,
        0x65, 0xaa, 0x5a, 0xae, 0x44, 0xb8, 0x7a, 0xdc, 0xe3, 0xac, 0x53, 0x7e, 0x28, 0xd1, 0xd9, 0x45, 0x8d, 0x95, 0x06, 0x4c, 0xd9,
        0x69, 0x3a, 0xdd, 0x30, 0xa8, 0x1d, 0x5f, 0xf9, 0x1c, 0x25, 0xac, 0x3a, 0x6a, 0x15, 0x2f, 0x4b, 0xdf, 0xa4, 0x71, 0xd8, 0x1d,
        0x9e, 0x12, 0x3a, 0x35, 0x7d, 0x6c, 0x27, 0x23, 0x20, 0x65, 0x03, 0x25, 0xbb, 0x07, 0x34, 0x60, 0xb9, 0xad, 0x7d, 0x8a, 0xac,
        0x48, 0xc4, 0xff, 0x14, 0xca, 0x69, 0x2a, 0x48, 0x0b, 0xc5, 0x25, 0x3c, 0xf7, 0x39, 0x50, 0x6b, 0x55, 0xf3, 0xbc, 0xf4, 0x71,
        0x91, 0x3d, 0x95, 0xf0, 0x48, 0x23, 0x0a, 0xed, 0xde, 0x35, 0x9b, 0x70, 0x16, 0x9b, 0xee, 0x65, 0x1e, 0x04, 0x66, 0x1c, 0x14,
        0x76, 0x79, 0xfd, 0x25, 0xe1, 0x78, 0xf4, 0x89, 0x4f, 0x78, 0x12, 0xe0, 0x7b, 0xf8, 0xe6, 0xbe, 0x14, 0xe7, 0x24, 0x1a, 0xf0,
        0x71, 0x06, 0x60, 0xd6, 0xf4, 0xc6, 0x37, 0xcb, 0x2e, 0x33, 0x17, 0xc6, 0xca, 0x81, 0x57, 0xe3, 0x24, 0x61, 0x38, 0xfe, 0xd3,
        0xbf, 0x57, 0xff, 0x42, 0x27, 0x4f, 0x02, 0x82, 0x2e, 0x9e, 0x39, 0x12, 0xcd, 0x19, 0xbd, 0x2e, 0x70, 0x4c, 0x76, 0x32, 0x92,
        0xe9, 0x69, 0x95, 0x78, 0x95, 0x2a, 0xa4, 0x7f, 0x19, 0x28, 0x47, 0x53, 0xd8, 0x70, 0xb5, 0x1a, 0xf4, 0xd6, 0xab, 0x29, 0x32,
        0x19, 0xd2, 0xd2, 0x28, 0x6a, 0x9f, 0x18, 0xb5, 0x04, 0xdf, 0xc6, 0xe0, 0xa8, 0x92, 0x82, 0xa9, 0x08, 0x34, 0xcd, 0x7a, 0x83,
        0xcf, 0x47, 0xfc, 0xc6, 0x12, 0x73, 0x48, 0xc2, 0x84, 0x41, 0xff, 0xbf, 0x65, 0xf9, 0xd2, 0xe4, 0x30, 0x5a, 0x97, 0xde, 0x9e,
        0x8b, 0x99, 0x2b, 0x01, 0x8a, 0xac, 0xcc, 0x2a, 0x2a, 0xa2, 0x88, 0x3a, 0x6b, 0xa1, 0xc0, 0xdc, 0xc1, 0xfa, 0xd5, 0xe3, 0xf2,
        0x3c, 0x94, 0xc2, 0x4a, 0xb4, 0x4c, 0xd4, 0x74, 0x5b, 0x93, 0x27, 0xa9, 0xf4, 0x25, 0xf2, 0xa8, 0xf0, 0xee, 0xee, 0xe9, 0x9f,
        0x52, 0x14, 0x38, 0x4a, 0xea, 0x14, 0x24, 0xa1, 0xc9, 0x6d, 0x77, 0xb0, 0xc4, 0xe0, 0x15, 0x75, 0xb4, 0x79, 0x26, 0xfa, 0x32,
        0x31, 0x5b, 0x4f, 0x37, 0xd7, 0x33, 0x8f, 0x03, 0xee, 0xc5, 0xec, 0x28, 0x2c, 0x05, 0xcf, 0x30, 0x55, 0x18, 0x77, 0x12, 0x04,
        0x8b, 0x85, 0x40, 0x34, 0x28, 0xb4, 0x8c, 0x71, 0x73, 0xf3, 0x55, 0x90, 0x20, 0x51, 0xb3, 0xd8, 0x14, 0xf4, 0xd9, 0x33, 0x66,
        0x1d, 0x88, 0x32, 0x82, 0x8c, 0x34, 0x6a, 0xac, 0x85, 0xf6, 0xaa, 0xc7, 0xf4, 0x30, 0x22, 0x2f, 0xba, 0xac, 0xd4, 0x52, 0xbd,
        0x9d, 0xbd, 0x06, 0xaa, 0x79, 0x34, 0x4e, 0x36, 0x1e, 0xd7, 0x75, 0x2b, 0xf3, 0xd4, 0xd6, 0x36, 0x53, 0x9b, 0x1f, 0x44, 0xe5,
        0x2e, 0x95, 0x69, 0x27, 0x69, 0xba, 0x82, 0x87, 0xd9, 0x39, 0x9f, 0x2d, 0x15, 0x9e, 0xc5, 0x8b, 0x9c, 0x81, 0xa8, 0xb8, 0x93,
        0x56, 0x0f, 0x3d, 0x1c, 0x32, 0xd6, 0xb2, 0xc7, 0x87, 0xa5, 0xac, 0xaa, 0xe5, 0xcd, 0x67, 0x21, 0xc4, 0xe4, 0x10, 0x22, 0xe0,
        0x32, 0xbf, 0x8d, 0xdb, 0x6b, 0x2d, 0x0b, 0xae, 0x94, 0xa6, 0x2e, 0x61, 0xe6, 0x57, 0x79, 0xae, 0xe8, 0x73, 0xf0, 0x2e, 0x02,
        0xed, 0x81, 0x15, 0x31, 0x07, 0x12, 0x98, 0x28, 0xa8, 0x55, 0x3e, 0x95, 0x44, 0x0e, 0xea, 0xfe, 0xfa, 0x8b, 0x95, 0xe4, 0xa9,
        0x17, 0x8c, 0x44, 0xad, 0x92, 0x21, 0x40, 0xea, 0x1c, 0x76, 0x3d, 0xa6, 0x85, 0xcc, 0xab, 0x2e, 0x84, 0xac, 0x3a, 0xaf, 0x69,
        0x8c, 0x6d, 0x24, 0x27, 0xac, 0x2b, 0x5e, 0xb3, 0x4e, 0x57, 0xc7, 0x68, 0x01, 0xfb, 0x42, 0x27, 0x1b, 0xfb, 0x1b, 0xda, 0x1a,
        0x50, 0x7c, 0x33, 0xcf, 0xf7, 0x90, 0xb8, 0x51, 0x57, 0x9d, 0x1a, 0xe1, 0xa6, 0x09, 0xf9, 0x9a, 0x55, 0xf7, 0x1e, 0xde, 0x59,
        0xfb, 0x13, 0x34, 0xfb, 0x70, 0x97, 0x77, 0xd6, 0xa6, 0xd5, 0x74, 0xdf, 0x7f, 0xdf, 0x8e, 0xef, 0xdf, 0x0d, 0x86, 0xcf, 0xcd,
        0x6f, 0xee, 0x7b, 0x76, 0x13, 0x08, 0xa2, 0x7d, 0xec, 0x09, 0xf0, 0x51, 0xb0, 0x0d, 0xcf, 0x37, 0x36, 0xfc, 0x5f, 0x95, 0xbd,
        0xd7, 0x6b, 0x88, 0x6c, 0x92, 0x9d, 0x08, 0xfb, 0x20, 0xd9, 0xa1, 0xc0, 0xd5, 0x09, 0x53, 0x5c, 0x47, 0xb3, 0x65, 0xf5, 0xda,
        0x68, 0x97, 0x80, 0x93, 0x35, 0x8d, 0xa4, 0x4b, 0x99, 0x91, 0xf3, 0x8c, 0x72, 0x26, 0x6b, 0x88, 0xf7, 0x3e, 0x5b, 0x41, 0xac,
        0x3c, 0x26, 0xb2, 0xd5, 0x7d, 0xe6, 0x82, 0x17, 0xcd, 0x5d, 0x6e, 0x6c, 0xed, 0x44, 0x91, 0x1d, 0xdc, 0xf8, 0x85, 0xe4, 0x4b,
        0x8e, 0x36, 0x04, 0x37, 0x58, 0x93, 0x99, 0x0a, 0xaa, 0x13, 0x0b, 0x90, 0x1a, 0x35, 0x57, 0x99, 0x17, 0xc2, 0x80, 0x96, 0x87,
        0x85, 0x52, 0xa7, 0x87, 0x09, 0x2d, 0x85, 0xad, 0x11, 0x9b, 0x33, 0xd8, 0x87, 0x52, 0xe5, 0x05, 0xd6, 0xaf, 0x4d, 0xa5, 0x9e,
        0x69, 0xcf, 0xe7, 0x08, 0x4a, 0xcc, 0x7a, 0x04, 0x31, 0x15, 0xdc, 0x34, 0xce, 0x3a, 0x78, 0xb2, 0x27, 0xf7, 0x40, 0x84, 0xc9,
        0x7f, 0x76, 0xa4, 0x7f, 0x86, 0x95, 0x2d, 0x4c, 0x8f, 0xa7, 0xca, 0xe8, 0x2d, 0x73, 0x76, 0x0b, 0x93, 0x02, 0x54, 0xb8, 0xe3,
        0xd5, 0x73, 0x7e, 0x28, 0x33, 0xb4, 0xea, 0x1f, 0xae, 0xb1, 0x6e, 0xbf, 0x63, 0xcb, 0xe5, 0xb8, 0x21, 0xfc, 0xb5, 0xeb, 0xfe,
        0x55, 0x68, 0x10, 0xe7, 0x14, 0x80, 0xcd, 0x3d, 0xae, 0xd4, 0x49, 0x63, 0x4e, 0x94, 0x38, 0x90, 0x94, 0x45, 0x10, 0x2a, 0x76,
        0xd5, 0xf3, 0xa3, 0xb5, 0x8f, 0x7a, 0xd8, 0x9c, 0x85, 0xd8, 0x46, 0x51, 0x1f, 0xbf, 0x98, 0xb9, 0x65, 0xf6, 0x66, 0xfd, 0x89,
        0x39, 0x2a, 0xda, 0x7c, 0x2d, 0xad, 0x77, 0x07, 0x03, 0xff, 0xef, 0xeb, 0x36, 0x39, 0xb5, 0x00, 0x56, 0x5b, 0x92, 0xd3, 0x61,
        0xbe, 0xc4, 0xb7, 0x9c, 0xd4, 0x44, 0x85, 0x75, 0x2c, 0xb4, 0x2c, 0xb5, 0x6d, 0x86, 0x7c, 0xdc, 0xa6, 0xc3, 0x94, 0x21, 0xc0,
        0x2e, 0x76, 0x0c, 0xd9, 0x43, 0xe1, 0x07, 0x8c, 0x07, 0x96, 0x39, 0xcc, 0x14, 0x11, 0x31, 0x8a, 0xb4, 0xb7, 0x36, 0xce, 0x65,
        0x35, 0x51, 0xc0, 0xd9, 0xb6, 0x83, 0xd4, 0x7f, 0xb9, 0x6d, 0x7a, 0xc8, 0xb2, 0x8e, 0xa9, 0xfb, 0x81, 0x09, 0x31, 0x03, 0x98,
        0x24, 0x6d, 0x0e, 0xce, 0xd6, 0x69, 0x08, 0x4f, 0xbb, 0x1f, 0x55, 0x36, 0xa2, 0x6d, 0x87, 0xd9, 0xbf, 0xeb, 0x4d, 0x86, 0xb8,
        0x04, 0x49, 0x93, 0xf8, 0xdd, 0xbf, 0x1b, 0x31, 0x08, 0x8d, 0xed, 0x1c, 0xc5, 0xd2, 0x01, 0x78, 0xde, 0x95, 0x08, 0xb0, 0xa3,
        0x9c, 0x4d, 0xac, 0xac, 0x25, 0x03, 0x2e, 0x38, 0x85, 0x7c, 0xf9, 0x72, 0x16, 0xd3, 0x85, 0xc4, 0x0b, 0x60, 0xbc, 0x9e, 0xb5,
        0xac, 0xbc, 0xd8, 0xd7, 0xbd, 0x62, 0x8b, 0x7f, 0x47, 0x57, 0xd9, 0x0f, 0xc3, 0x20, 0xbe, 0x1a, 0xee, 0x09, 0xa6, 0x4c, 0xb7,
        0x22, 0x5f, 0x26, 0xe3, 0x46, 0xfb, 0x07, 0x54, 0xd4, 0x95, 0xab, 0x35, 0x9b, 0x2a, 0x4a, 0xf6, 0x7a, 0x99, 0xec, 0x1d, 0x57,
        0x30, 0x8a, 0x11, 0x87, 0x72, 0xe9, 0x1c, 0xa9, 0x74, 0xbf, 0x36, 0x66, 0xf9, 0x0d, 0x8c, 0xea, 0x87, 0x9d, 0x59, 0x29, 0x1b,
        0x08, 0x04, 0x93, 0x57, 0xab, 0xc6, 0xef, 0xb1, 0x06, 0x79, 0x31, 0x2e, 0xae, 0x52, 0x03, 0x66, 0x08, 0xc1, 0xfb, 0xc2, 0xe0,
        0x93, 0x44, 0xf7, 0xb6, 0x35, 0x19, 0x87, 0xa0, 0x2e, 0x99, 0xa4, 0x02, 0x7e, 0xd8, 0x3b, 0xc4, 0x6c, 0xf6, 0xf3, 0xb8, 0x33,
        0x1e, 0x1d, 0xf0, 0x35, 0x06, 0x9a, 0x04, 0x29, 0x55, 0xfb, 0x38, 0x7b, 0xee, 0x07, 0xe2, 0xf5, 0xf2, 0xda, 0x3d, 0x07, 0xb9,
        0xa2, 0x90, 0x97, 0x2f, 0xe2, 0xe4, 0xf8, 0x16, 0x22, 0xa4, 0x8b, 0x24, 0x0e, 0x29, 0xe3, 0x04, 0xab, 0x9e, 0x70, 0x2a, 0xf5,
        0xc4, 0xba, 0x58, 0x2a, 0x00, 0x5b, 0xc2, 0xca, 0xf3, 0xd7, 0xbf, 0xeb, 0xe1, 0xf2, 0xe0, 0x5a, 0x0f, 0x26, 0x0b, 0xb4, 0x03,
        0x5a, 0xc1, 0x06, 0x53, 0x17, 0x82, 0x38, 0x25, 0x56, 0x5c, 0xf7, 0x49, 0x8c, 0x8c, 0x5e, 0xfd, 0xd0, 0xb5, 0x57, 0x11, 0x33,
        0xaf, 0xa4, 0x9c, 0x33, 0x41, 0xfe, 0xd8, 0x68, 0xd5, 0xd9, 0xfa, 0xf3, 0x88, 0x5f, 0x81, 0x03, 0xc7, 0x0e, 0x89, 0x46, 0xcc,
        0xc1, 0x05, 0x74, 0xa9, 0xb1, 0x80, 0x95, 0xd3, 0x11, 0xf2, 0x24, 0x9e, 0xd2, 0xa6, 0x3a, 0x58, 0x16, 0x92, 0x79, 0x59, 0x8f,
        0x64, 0x8d, 0x11, 0xc0, 0x26, 0xe7, 0x92, 0x69, 0xff, 0x48, 0xd8, 0xe0, 0x07, 0xfe, 0x41, 0x78, 0xab, 0x96, 0xdb, 0x78, 0x4c,
        0x75, 0x77, 0xa3, 0xf2, 0x53, 0x8b, 0x68, 0xed, 0x83, 0xe3, 0x9e, 0xf4, 0xa8, 0x9e, 0xe2, 0xe4, 0xcb, 0x73, 0x83, 0xbb, 0x57,
        0x80, 0x56, 0x4c, 0x5b, 0x5a, 0x26, 0x52, 0xe8, 0x77, 0xe6, 0xc6, 0xdc, 0x3b, 0x3f, 0x99, 0xe1, 0x63, 0xd7, 0xef, 0x46, 0x87,
        0xc9, 0x2c, 0x10, 0xb5, 0xa2, 0x9f, 0x37, 0x8b, 0x56, 0xda, 0x00, 0x01, 0xdd, 0xa0, 0x53, 0x93, 0x5f, 0x7c, 0x32, 0xf6, 0x43,
        0x11, 0xff, 0x33, 0x30, 0xe3, 0x78, 0x2f, 0xca, 0x39, 0xef, 0xf0, 0x64, 0x01, 0xc3, 0x44, 0x52, 0xa7, 0x45, 0xb1, 0x58, 0x3c,
        0x48, 0x16, 0x2f, 0xbc, 0x58, 0x8d, 0xb2, 0x46, 0xc8, 0xd7, 0xad, 0x7e, 0x63, 0xc0, 0x7e, 0xdd, 0x39, 0xa2, 0x7e, 0x6a, 0xd8,
        0xe1, 0xaf, 0x23, 0xcc, 0x9e, 0xb0, 0xc4, 0x89, 0xdb, 0xda, 0xc6, 0x58, 0xc6, 0x69, 0x11, 0x31, 0xe6, 0xd9, 0x2c, 0x1d, 0xb1,
        0x90, 0x60, 0xd1, 0x35, 0x8f, 0x66, 0xd1, 0xb5, 0xc8, 0x9f, 0x1c, 0x83, 0x52, 0xaf, 0xa4, 0x63, 0x5b, 0x30, 0xa6, 0xa1, 0x68,
        0xbf, 0xac, 0x60, 0x46, 0xd3, 0x69, 0x54, 0xea, 0x3c, 0x5d, 0xdc, 0x33, 0xef, 0x86, 0xd4, 0xb2, 0xff, 0x03, 0x37, 0x5b, 0x52,
        0x31, 0x52, 0x0b, 0x05, 0x6a, 0x09, 0xf6, 0xaf, 0xcc, 0x78, 0xf0, 0x89, 0xe8, 0x48, 0xea, 0x8b, 0x11, 0x63, 0x5a, 0x3c, 0x1f,
        0x50, 0x56, 0x77, 0x78, 0x3a, 0xce, 0x4f, 0x35, 0x89, 0x25, 0x39, 0x62, 0xb8, 0xca, 0xbe, 0x3c, 0x1f, 0x14, 0x69, 0x5e, 0x2b,
        0x5c, 0x7d, 0xd3, 0xd4, 0x94, 0x21, 0xa6, 0xac, 0xf0, 0x89, 0x07, 0x83, 0xab, 0x42, 0x0c, 0x2d, 0xa6, 0x12, 0x83, 0x11, 0x0d,
        0x2b, 0x1e, 0x3f, 0xe1, 0x53, 0xe4, 0xac, 0xe6, 0x1e, 0x41, 0x31, 0x79, 0x19, 0xd7, 0xf4, 0x7c, 0xef, 0xc7, 0xdc, 0x9a, 0x61,
        0xc4, 0x71, 0x5d, 0xce, 0x2a, 0x7b, 0x0b, 0xc8, 0x64, 0xc2, 0x43, 0xe6, 0xdc, 0x4d, 0x53, 0x7d, 0x21, 0x29, 0xed, 0xfe, 0xfc,
        0x07, 0xcc, 0x0f, 0x76, 0x85, 0x95, 0xa5, 0x40, 0x84, 0x37, 0xd3, 0xce, 0xfe, 0x1e, 0xcf, 0xec, 0xa7, 0xc9, 0x57, 0x17, 0x1d,
        0xfb, 0x74, 0xcc, 0x99, 0xea, 0x81, 0xbf, 0xb2, 0x3e, 0xb7, 0xab, 0xd7, 0x46, 0x7d, 0x21, 0x5c, 0xc2, 0xe2, 0xa8, 0x24, 0xfe,
        0xa3, 0xa4, 0x31, 0x64, 0xf4, 0x46, 0x81, 0x3a, 0x06, 0x83, 0xef, 0x12, 0x22, 0xb1, 0xca, 0x35, 0x45, 0x66, 0x11, 0x73, 0x47,
        0x78, 0xfa, 0xb9, 0x25, 0x90, 0x86, 0x33, 0x75, 0x3f, 0x13, 0xdb, 0x06, 0xd0, 0x2e, 0x70, 0x17, 0xe2, 0xbd, 0x85, 0x71, 0xf5,
        0xce, 0x41, 0xa5, 0x98, 0x1d, 0xcf, 0x2e, 0x2b, 0x7a, 0x9b, 0x9f, 0xad, 0xbf, 0xc2, 0xf0, 0xb3, 0xc2, 0x16, 0xc1, 0x44, 0x12,
        0x30, 0xf1, 0x8c, 0x29, 0x55, 0x10, 0xbb, 0x1d, 0x18, 0x9e, 0x31, 0x74, 0x6b, 0x48, 0xce, 0x82, 0xa5, 0x1d, 0xce, 0xda, 0x09,
        0x3c, 0x84, 0x69, 0x6f, 0xd3, 0x68, 0x88, 0xeb, 0x76, 0x6f, 0x60, 0x46, 0xb7, 0x8b, 0xd4, 0xf1, 0x94, 0x39, 0x68, 0x20, 0xc7,
        0x07, 0x96, 0xce, 0xee, 0xd7, 0x65, 0x52, 0x66, 0x79, 0xab, 0x4d, 0x4c, 0xe4, 0x88, 0xf9, 0xc2, 0x94, 0xd9, 0x10, 0xd6, 0x7b,
        0xd5, 0x9d, 0x02, 0x17, 0x23, 0xe3, 0x1d, 0x4e, 0xbf, 0x42, 0x9f, 0xa7, 0xf3, 0x4a, 0x20, 0x85, 0x49, 0x1f, 0xba, 0xbd, 0x2b,
        0xfb, 0x2b, 0x6c, 0x61, 0x8d, 0x59, 0x9e, 0x04, 0xe5, 0x0d, 0x54, 0x31, 0x93, 0x5f, 0xa8, 0xde, 0x28, 0x1b, 0x73, 0xb2, 0x00,
        0xe9, 0x6a, 0xf0, 0xed, 0x4d, 0xda, 0x67, 0x3d, 0xda, 0x2a, 0xe4, 0x0d, 0x1d, 0xcc, 0xcd, 0x4e, 0x79, 0x35, 0xa0, 0xbb, 0xa6,
        0xc0, 0x36, 0xd2, 0x76, 0x0e, 0xc4, 0x6c, 0x01, 0x1a, 0xd0, 0xea, 0x80, 0x2e, 0x8c, 0x73, 0x17, 0xad, 0xb8, 0xbe, 0x25, 0xf7,
        0x13, 0xd2, 0xe1, 0xbf, 0x54, 0x23, 0xc5, 0xe4, 0x7e, 0xb0, 0xc3, 0x5b, 0x0a, 0x40, 0xfd, 0x4f, 0x1e, 0x2e, 0x48, 0xe1, 0x09,
        0xe0, 0x14, 0x54, 0x18, 0x6c, 0xb4, 0x33, 0x42, 0xcc, 0xe1, 0x90, 0x0e, 0x7a, 0x23, 0x4b, 0xad, 0xf5, 0xc6, 0x74, 0xaa, 0x59,
        0x76, 0xae, 0x4e, 0xd1, 0xc5, 0x5d, 0xc5, 0xc5, 0xa8, 0xe0, 0xd2, 0xdc, 0x16, 0xbf, 0x2f, 0x2b, 0x14, 0x17, 0xf4, 0x92, 0x2a,
        0xee, 0x9f, 0x21, 0xe6, 0x17, 0x61, 0x99, 0x7e, 0xf2, 0x0f, 0x53, 0x46, 0xc8, 0xea, 0x75, 0x40, 0x9c, 0xe6, 0x91, 0x9e, 0x9f,
        0xeb, 0xd9, 0xcf, 0x39, 0xfc, 0xa9, 0xc2, 0x84, 0x49, 0xe4, 0x4a, 0x5e, 0xa5, 0xe2, 0x05, 0x4e, 0x08, 0x14, 0x94, 0x1a, 0x81,
        0x75, 0x37, 0x61, 0xbe, 0x5e, 0x09, 0x58, 0x11, 0x47, 0xb4, 0x51, 0xaf, 0x48, 0xaa, 0x50, 0xdd, 0x90, 0x7a, 0x19, 0x9d, 0xc5,
        0x30, 0x39, 0x81, 0x3d, 0xfb, 0x3a, 0xb8, 0x6a, 0x34, 0x01, 0x9d, 0xf4, 0xda, 0x88, 0x70, 0xef, 0x91, 0x7f, 0x28, 0x41, 0xe9,
        0xec, 0x3b, 0xb2, 0x29, 0x31, 0xcb, 0xa0, 0xa3, 0xbc, 0x5e, 0x5a, 0x61, 0x89, 0xa2, 0xdb, 0xf7, 0x92, 0xa4, 0x4c, 0x3f, 0x7c,
        0x13, 0x28, 0x28, 0x60, 0xf8, 0x0d, 0x7a, 0x99, 0x89, 0x9b, 0x3b, 0x7d, 0x4e, 0xbc, 0x1f, 0x6f, 0x24, 0x76, 0xcb, 0x07, 0xe2,
        0xcb, 0xca, 0x3a, 0x15, 0x1d, 0x4d, 0x5a, 0x41, 0xc3, 0xd5, 0xf5, 0x99, 0x9d, 0x1f, 0x35, 0x85, 0xd5, 0xdf, 0x69, 0xd9, 0x8b,
        0xdd, 0x44, 0x40, 0x2f, 0xc6, 0xc8, 0x37, 0x16, 0x23, 0x0b, 0x1b, 0xed, 0x79, 0x8a, 0x4f, 0xf1, 0x4d, 0x2b, 0x78, 0xea, 0x80,
        0x71, 0x65, 0xba, 0x75, 0x6b, 0xac, 0xe6, 0x8a, 0xdc, 0x91, 0x36, 0xef, 0x59, 0x1a, 0x5e, 0x16, 0xde, 0x6d, 0x08, 0x31, 0xe7,
        0x8d, 0x0e, 0x45, 0x8f, 0xc5, 0xec, 0x92, 0xbc, 0x5b, 0x77, 0x27, 0xab, 0xf9, 0xc3, 0xe0, 0x90, 0x83, 0x24, 0x0c, 0x26, 0x3f,
        0x4f, 0x00, 0x63, 0x5d, 0x7e, 0x10, 0x6a, 0xa6, 0x4a, 0xe3, 0x9d, 0xb7, 0xbc, 0xf0, 0x36, 0xd8, 0xc8, 0x36, 0x2b, 0xbf, 0x09,
        0x0e, 0x5e, 0xe6, 0x44, 0xce, 0xac, 0xad, 0x1b, 0x1d, 0x61, 0xb3, 0xe9, 0x5b, 0x56, 0xac, 0xfa, 0x13, 0x1d, 0x3f, 0x4b, 0x03,
        0xd3, 0x7e, 0x84, 0x5f, 0xbd, 0x29, 0xbb, 0x1d, 0x38, 0x4c, 0x4d, 0x0d, 0xf0, 0x97, 0xc5, 0xb3, 0x72, 0xf3, 0x14, 0x1b, 0xda,
        0x03, 0xf4, 0x17, 0x47, 0x34, 0xa1, 0xea, 0x36, 0xda, 0x6c, 0x92, 0x4d, 0x72, 0xd0, 0xec, 0x34, 0xe8, 0x07, 0xcf, 0x74, 0xe3,
        0x32, 0x44, 0xec, 0xf5, 0x68, 0x21, 0x0e, 0x1b, 0xe3,
    ],
    payout_payload: [
        0xcf, 0x07, 0x27, 0xca, 0x2e, 0xe0, 0x9a, 0xf2, 0x33, 0x05, 0x26, 0x81, 0xdd, 0x59, 0x1d, 0xc9, 0x6f, 0xc2, 0x96, 0xf7, 0x25,
        0x7e, 0xd5, 0x93, 0x2d, 0x02, 0x0d, 0x2d, 0xad, 0x29, 0x5b, 0xc8, 0x66, 0xb7, 0x09, 0x67, 0x5e, 0xab, 0xff, 0x34, 0xc1, 0xf4,
        0x44, 0x16, 0xb8, 0x44, 0x49, 0xda, 0x7b, 0x68, 0xb1, 0x26, 0xae, 0x34, 0x4a, 0x65, 0xc0, 0x01, 0x0a, 0x35, 0x99, 0x76, 0x99,
        0x63,
    ],
};

/// **testnet-12's genesis bond registry**: testnet-11's cards 0–6 as they are (the operators keep
/// their keys and their collateral outpoints) and [`PALW_T12_GENESIS_BOND_7`] in the eighth slot.
/// The card assembly and the premine both read THIS table for testnet-12, so the seat the card
/// registers and the float the premine pays are the same key's.
pub const PALW_T12_GENESIS_BONDS: &[PalwRcGenesisBondCard] = &[
    PALW_RC_GENESIS_BONDS[0],
    PALW_RC_GENESIS_BONDS[1],
    PALW_RC_GENESIS_BONDS[2],
    PALW_RC_GENESIS_BONDS[3],
    PALW_RC_GENESIS_BONDS[4],
    PALW_RC_GENESIS_BONDS[5],
    PALW_RC_GENESIS_BONDS[6],
    PALW_T12_GENESIS_BOND_7,
];

/// Is the genesis card filled in?
///
/// **This no longer decides whether the network gets its ruleset** (ADR-0061 Decision 1, and the
/// audit that followed it). It used to: an empty card returned the bundle-free base identity, so
/// the moment an operator emptied the registry to mint the zero-seat genesis the ADR describes,
/// `Params::from(testnet-11)` quietly handed back a HASH-ONLY chain — no PALW lanes, no heartbeat
/// lane, no court, a 10B premine and nothing to spend it on. A whole fleet built from that binary
/// agrees with itself and peers happily, so the one warning that fires ("peers will refuse the
/// handshake") is false exactly when it matters. A zero-seat network is a V2 network whose
/// producers have not arrived yet, which is the ADR's whole sentence.
pub fn palw_rc_genesis_card_is_set() -> bool {
    !PALW_RC_GENESIS_BONDS.is_empty()
}

/// **The RC network as it ships: the base identity, plus the bundle when the card is set.**
///
/// This is what `Params::from(NetworkId testnet-12)` returns, so a node's ruleset is a property of
/// its BINARY rather than of a file it was pointed at — which is the only shape that keeps
/// ADR-0042 Decision 11's promise ("it reads the RC's canonical ruleset bytes rather than a human
/// re-typing parameters"). A per-node config file would make every operator's ruleset a local
/// decision, and the handshake would be the first place anyone found out.
/// **Mainnet as it ships: hash-only today, `ConsensusV2` the moment its genesis card is set.**
///
/// The same shape `palw_rc_shipped_params` has, and for the same reason — a node's ruleset is a
/// property of its BINARY, not of a file it was pointed at. With
/// [`PALW_MAINNET_GENESIS_ARTIFACT_ROOT`] zero this returns `MAINNET_PARAMS` untouched, so mainnet
/// is byte-identical to before this function existed, fingerprint included.
///
/// Turning PALW on for mainnet is therefore two constants and a re-mint: the artifact root, and a
/// [`PALW_MAINNET_GENESIS_BONDS`] registry of real operator keys — whose collateral outputs become
/// part of the premine, which is why it moves the genesis. Everything between those constants and
/// a running V2 mainnet is the code below, and it is exercised by
/// `a_mainnet_equivalent_genesis_actually_enables_palw`.
/// **The two halves of a mainnet genesis card have to arrive together** (mainnet audit,
/// 2026-09-05).
///
/// [`PALW_MAINNET_GENESIS_ARTIFACT_ROOT`] decides whether mainnet gets a RULESET;
/// [`PALW_MAINNET_GENESIS_BONDS`] decides whether its premine carves BOND COLLATERAL
/// (`genesis_premine_utxos_for` branches on the registry alone). Nothing cross-checked them, so a
/// half-filled card minted a genesis holding collateral and fee floats at outpoints that a
/// `Disabled` mainnet never reads — 10B of cap spent on seats no rule seats — and the only thing
/// that noticed was the genesis-hash assert at node start, whose remedy ("re-pin the genesis")
/// turns the mistake into a hash-only mainnet with seat money inside it.
///
/// A predicate rather than an inline `assert!` so both arms are reachable from a test: the failing
/// arm of a `const`-driven check is otherwise unexecutable until the day it fires for real.
fn mainnet_card_coherence_v1(artifact_root_is_set: bool, bond_count: usize) -> Result<(), String> {
    if !artifact_root_is_set && bond_count > 0 {
        return Err(format!(
            "the mainnet genesis card lists {bond_count} bond(s) while PALW_MAINNET_GENESIS_ARTIFACT_ROOT is unset: \
             the premine would carve their collateral and fee floats out of the 10B cap for a network whose ruleset \
             seats nobody, and no PALW rule would ever read those outpoints. A card is both constants or neither."
        ));
    }
    Ok(())
}

pub fn mainnet_shipped_params() -> Params {
    if PALW_MAINNET_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        // **A card is two constants and they are read by two different gates** (mainnet audit,
        // 2026-09-05). The ruleset keys off the artifact root — this branch — while the PREMINE
        // keys off the bond registry alone (`genesis_premine_utxos_for`, which returns
        // `bonded_genesis_utxos` for mainnet the moment `PALW_MAINNET_GENESIS_BONDS` is non-empty).
        // Half a card therefore mints a genesis that carves collateral and fee floats for seats,
        // and then returns a `Disabled` mainnet that seats nobody: the money is minted, locked at
        // outpoints no rule ever reads, and the only thing that notices is the genesis-hash assert
        // in `set_genesis_utxo_commitment_from_config`, whose message tells the operator to re-pin
        // the genesis — which converts the mistake into a hash-only mainnet with seat money in it.
        // Refuse here instead, naming the constant that is missing.
        if let Err(e) = mainnet_card_coherence_v1(false, PALW_MAINNET_GENESIS_BONDS.len()) {
            panic!("{e}");
        }
        return MAINNET_PARAMS;
    }
    let bonds: Vec<_> = PALW_MAINNET_GENESIS_BONDS
        .iter()
        .map(|c| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint(c.premine_index)),
            pubkey: c.bond_pubkey.to_vec(),
            operator_pubkey: c.operator_pubkey.to_vec(),
            payout_payload: crate::Hash64::from_bytes(c.payout_payload),
        })
        .collect();
    // A card that is set and does not assemble is a binary that would boot a network its own
    // genesis gate refuses; failing at startup with the gate's own message is the honest outcome.
    //
    // ADR-0068 Phase 2: a V2 mainnet is born with its clock armed — the heartbeat lane, its
    // width bound and the attempt-work constant, from genesis, exactly as the testnet rehearses.
    // The floor's 20‰ reserve rides the shared bundle builder, and the class share table is a
    // pin-time decision whose ADR-0068 figure is {floor ≈ 20‰, model classes ≈ 980‰}.
    // **Through the class-registering assembly when the model tiers are pinned** (mainnet audit).
    // This used to call `palw_v2_params_from_artifacts_on_base` unconditionally — the builder that
    // takes only the FLOOR's artifact root — so a carded mainnet came out floor-only however the
    // comment above described it. The two paths differ in exactly one thing, which is whether the
    // model tiers exist, and that is the whole of ADR-0068 on this network.
    // **ADR-0082 Decision 3, STATED on mainnet's base by the card that pins the row it tries**
    // (mainnet audit, 2026-09-05). `palw_v2_params_with_classes_on_base` refuses a genesis set that
    // registers a fused-attention (graph-v5) row while `palw_kary_court` is dormant, and
    // `validate_palw_v2` refuses it again — so with `MAINNET_PARAMS`' `palw_kary_court: None` a
    // carded mainnet that pinned the dense tier did not ship a court it could not run: it refused
    // to BOOT, and the mainnet card had no way to say the word. It is stated here rather than
    // derived from the assembled bundle for the reason `palw_rc_base_params` gives — a fence armed
    // by derivation alone is an arming nobody decided — and the operator's statement is the act of
    // pinning `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT`, which is the dense slot the RC fills with
    // ADR-0082's graph-v5@512 row. A card that instead pins a fused row through some other slot
    // still meets the assembly's refusal, by name, which is the fail-closed direction.
    mainnet_card_params_v1(
        PALW_MAINNET_GENESIS_ARTIFACT_ROOT,
        palw_mainnet_qwen36_is_registered().then_some(PALW_MAINNET_QWEN36_ARTIFACT_ROOT),
        palw_mainnet_qwen25_a16_is_registered().then_some(PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT),
        bonds,
        crate::config::premine::genesis_premine_utxos_for(MAINNET_PARAMS.net),
    )
    .unwrap_or_else(|e| panic!("the pinned mainnet PALW genesis card does not assemble: {e}"))
}

/// **The mainnet card assembly, with its constants as ARGUMENTS** (mainnet audit 2026-09-06, M-7).
///
/// `mainnet_shipped_params` is a wrapper that reads five `const`s and unwraps. Everything between
/// those two facts — which base the card states its fences on, which of the two assemblies runs,
/// the certification re-derivation and the Phase-1 arming — used to live inside a function no test
/// could call with a card in place, because the card is compile-time. So the assembly a mainnet
/// mint actually runs had ZERO test coverage: it panicked on the ambient-target gate and nothing in
/// the tree noticed, while the runbook's named verification command
/// (`shipped_presets_have_pinned_fingerprints`) pins the bundle-FREE `MAINNET_PARAMS` and stays
/// green with a filled card.
///
/// Splitting the constants out is the whole of the fix on the test axis:
/// `every_assembled_v2_ruleset_is_minted_at_its_ambient_target` now mints a card from the RC's own
/// artifact roots and a bonded premine and asserts it assembles. Nothing about the assembly changed.
fn mainnet_card_params_v1(
    artifact_root: crate::Hash64,
    qwen36_artifact_root: Option<crate::Hash64>,
    qwen25_a16_artifact_root: Option<crate::Hash64>,
    bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
    genesis_utxos: crate::utxo::utxo_collection::UtxoCollection,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    let base = mainnet_card_base_v1(MAINNET_PARAMS, qwen25_a16_artifact_root.is_some());
    let assembled = match qwen36_artifact_root {
        Some(hybrid) => palw_v2_params_with_classes_on_base_with_utxos(
            base,
            artifact_root,
            hybrid,
            qwen25_a16_artifact_root,
            bonds,
            genesis_utxos,
        )?,
        None => palw_v2_params_from_artifacts_on_base_with_utxos(base, artifact_root, bonds, genesis_utxos)?,
    };
    Ok(palw_rc_arm_phase1(mainnet_certify_registered_classes_v1(assembled)))
}

/// **The fences a mainnet card STATES on its base — in one place** (mainnet audit, 2026-09-05).
///
/// Stated rather than derived, for the reason `palw_rc_base_params` gives: a fence armed by a
/// derivation alone is an arming nobody decided. The RC states its set on its own base; a card
/// states its set here, and the tests compare the two as sets so the next fence added cannot be
/// stated on one shipped network and forgotten on the other.
///
/// * `palw_kary_court` (ADR-0082 D3) — when the card pins the dense tier, whose graph-v5 row only a
///   dissection court can try. The operator's statement is the act of pinning
///   `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT`; a fused row pinned through another slot still meets the
///   assembly's refusal by name, which is the fail-closed direction.
/// * `palw_context_ladder` (ADR-0077 Phase B) — the class-admission ladder rules: a class that
///   registers a state chunk map is priced over its checkpoint interval, and the court's move clock
///   is derived from the window and the ladder rather than typed.
/// * `palw_court_ladder` (ADR-0084 U-08) — the court's refutation walkers open against the ladder
///   the RULESET froze instead of the executor's `2^22`, so a class is prosecuted at the ladder it
///   was admitted at. Free on a fresh chain: no history was judged under the other ladder.
///
///   **These are two fences and they were one until the DA-court and model-market branches
///   merged** (mainnet audit 2026-09-06, M-15). One branch closed U-08 on `palw_context_ladder`,
///   the other minted `palw_court_ladder` for it, and the merge kept both fields with
///   `palw_court_step_ladder_at` — the processor's only reader of `palw_refutation_leaf_cap_v2` —
///   reading the second. A card that armed only the first admitted the graph-v5 dense row at `2^26`
///   and prosecuted it at `2^22`: U-08, re-opened on the one network the fence exists for. Both are
///   stated here, and `the_processor_resolves_the_refutation_ladder_from_palw_court_ladder_alone`
///   binds the second to its reader.
/// * `palw_difficulty_priced_rows` and `palw_receipt_rows_unpriced` (ADR-0083 D1, both halves) — the
///   window counts by lane, and a receipt row is not a priced row. The first is also what
///   `palw_rc_arm_phase1` would add; stated here because the assembly validates before it runs.
/// * `palw_attempt_header_pins` (ADR-0072 D8) — the three DA pins and ADR-0071 D2's bucket ceiling
///   are asked on the relay path, not only at the composed admission entry point a disqualified
///   block never reaches.
/// * `palw_validator_payout_bounds` (ADR-0018 §E; mainnet audit 2026-09-06 H-2/H-3/M-1) — the §E
///   quality bonus stops at its pool and refuses an epoch it cannot price, the deferred fan-out and
///   the reserve drip are bounded per BLOCK rather than per epoch crossing, and the coinbase's own
///   size guard counts every kind the builder appends. A live chain reaches these as a flag day;
///   a card has no coinbase history to fork.
/// * `palw_certification_rent` (ADR-0075 SA-1/SA-2) and `palw_chunk_cap_charge` (ADR-0075 D14) —
///   without them two ~60-byte `ObjectChunk`s a block spend the certification grading slots before
///   the object is validated, and eight unsigned chunks squat every pending-chunk slot: a
///   block-cheap way to keep an honest class weightless for the life of the chain.
/// * `palw_epoch_boundary_budget` (ADR-0045 D2; mainnet audit 2026-09-06, M-2) — the block that
///   crosses an epoch boundary derives its own epoch's budget from the parent state, which is the
///   ADR's own sentence. Dormant, every non-floor attempt at a boundary is `EpochBudgetUnspecified`
///   and the crossing block is permanently orphaned with its mergeset's claims.
/// * `palw_fp_ruleset_caps` (ADR-0044 D9; mainnet audit 2026-09-06, L-2) — the free-prompt lane
///   enforces the two caps its ruleset advertises. Dormant, `max_prompt_tokens` and
///   `max_decode_tokens` are in every node's fingerprint and read by nothing.
/// * `palw_capability_bound` (ADR-0071 SA-1/SA-2/SA-4) — a bond's `capable_classes` set is bounded
///   at `PALW_MAX_CAPABLE_CLASSES` and priced at `PALW_CAPABILITY_EXPOSURE_SOMPI` against its own
///   collateral, at BOTH writers (`BondRegistered` and `BondCapabilityDeclared`). Dormant it is a
///   free, mass-priced, per-bond-lifetime state-growth lever and the SA-2 price is never charged —
///   and the 2026-09-05 pass recorded the rule as closed while its fix sat behind a fence no preset
///   armed and no card stated (mainnet audit 2026-09-06, M-14).
///
///   **The liveness trap it must not walk into, and the assertion that says it does not.** SA-3
///   narrows the panel draw, and a short draw is not a smaller panel — it is
///   `InsufficientEligibleBonds`, so the claim never binds and voids at `BindTimeout` with its
///   escrow burned. What keeps a card drawable past the fence is the genesis-class exemption plus
///   the fact that this assembly extends every genesis bond's `capable_classes` with the tiers it
///   registers. `a_carded_registry_still_draws_a_full_panel_with_the_capability_fence_armed` is the
///   assertion, over the card's own assembly, not a deployment note.
///
/// **What a card does NOT get from this function, and the checklist item it owes.** Every fence
/// here is `always()`, i.e. DAA score 0, and `fork_id_gate_fences_v1` excludes 0 by design — a
/// fence in force at block 1 is `consensus_identity_id`'s business and is never "crossed". So a
/// carded mainnet's fork-id gate is `Unfenced` from genesis, correctly, and it BECOMES armed the
/// moment the operator schedules any fence at a height. That is now automatic (the gate list is
/// derived from [`Params::palw_fences_v1`]), where it used to require a per-fence edit to
/// `fork_id_v1::fork_id_gate_fences_v1` that nobody's checklist named — see mainnet audit
/// 2026-09-06, M-6, and `docs/mainnet-palw-certification-runbook.md` §0.
///
/// testnet-11 arms none of these from genesis; each is a scheduled height or a re-mint there,
/// which is the operator's call and not this function's. Devnet arms the context ladder.
fn mainnet_card_base_v1(mut base: Params, dense_tier_pinned: bool) -> Params {
    if dense_tier_pinned {
        base.palw_kary_court = Some(ForkActivation::always());
        // **AC-D8 (mainnet audit 2026-09-11): a dense card that arms the k-ary court arms
        // ADR-0093's D6 and D8 with it, from genesis.** The k-ary court alone leaves two holes on
        // the fused tier: `palw_fused_dissectable` (D6) is what refuses a registrant's fused class
        // whose output tile spans two heads — a class no dissection can try, so an honest responder
        // is convicted by silence — and `palw_attn_anchored_root` (D8) is what lets move 1 carry
        // its anchor checkpoint, the one path a challenger cannot rebuild once a forger's execution
        // followed its lie. Both are free to state from genesis (no shipped row fails D6, and
        // `validate_palw_v2` only needs `palw_kary_court` at or below the anchored-root height,
        // which `always()` satisfies). Residual (audit C-04): D8 as designed still cannot bottom a
        // forger who lied at a mid-layer fused tile, because the per-position anchor carries that
        // forger's own later-layer state — a court-completeness gap the dense tier keeps until D8
        // carries the sibling chunk hashes, tracked against ADR-0093.
        base.palw_fused_dissectable = Some(ForkActivation::always());
        base.palw_attn_anchored_root = Some(ForkActivation::always());
    }
    base.palw_capability_bound = Some(ForkActivation::always());
    // ADR-0134: a card never carries the VLT compute overlay — its five subnetworks are refused from genesis.
    base.palw_compute_overlay_retired = Some(ForkActivation::always());
    base.palw_context_ladder = Some(ForkActivation::always());
    base.palw_court_ladder = Some(ForkActivation::always());
    // **The future-time tolerance the median-time window actually models** (mainnet audit
    // 2026-09-06, L-1).
    //
    // `past_median_time_window_size` is 27 samples, defined as `(2·T − 1)/10` for `T = 132 s` — a
    // 263-second window at the hash lineage's 10-second sample interval. At the V2 cadence the
    // sample interval is one 120-second block, so the same 27 samples span 54 minutes and the
    // declared 132 s describes nothing: the observable band is a ~26-minute backdating window
    // (Bitcoin-like) and a ~26-minute lag on lock-time finality, while the node's future-time bound
    // is still 132 seconds. Two numbers, one clock, and they disagree by 12.3x.
    //
    // The WINDOW is not the thing to move — `AVERAGE_FRAME_SIZE` is 11, so a window short enough to
    // model 132 s is a median any producer holding two of three recent blocks sets outright, which
    // is a worse property than a wide band. The tolerance is the mis-sized quantity, and it is
    // derived here from the cadence rather than typed.
    //
    // **A card only.** `timestamp_deviation_tolerance` is inside `consensus_params_id`, and
    // testnet-11 and devnet are live chains running the inconsistent pair — correcting them is a
    // fingerprint move and therefore the operator's call and a coordinated flag day.
    // `the_v2_median_time_window_and_the_timestamp_tolerance_describe_one_clock` pins that gap so it
    // can neither close nor widen in silence. Carding mainnet is a re-mint, so this is a value the
    // ceremony chooses — the same argument the ambient-target mint rests on.
    base.timestamp_deviation_tolerance = palw_v2_timestamp_deviation_tolerance_v1(
        base.past_median_time_window_size,
        BlockrateParams::new_two_minute_bps().past_median_time_sample_rate,
        BlockrateParams::new_two_minute_bps().target_time_per_block,
    );
    // **The lead cap rides with that tolerance** (the 2026-09-25 mainnet-values review, HIGH): a
    // header that moves the heartbeat clock keeps the 132 s bound, so the tolerance above is never
    // a burst of clock ticks (`Params::palw_clock_lead_cap`). Stated as testnet-12 states it.
    base.palw_clock_lead_cap = Some(ForkActivation::always());
    // **ADR-0045 Decision 2's boundary sentence, in force from block one** (mainnet audit
    // 2026-09-06, M-2). Free on a fresh chain: no epoch has closed, so no history was judged under
    // the other reading, and a card is the one network that can be born obeying the ADR the tree
    // already carries. testnet-11 reaches it by a scheduled height or not at all — it is a live
    // chain with orphaned boundary blocks in its past, and a node on the new rule would accept
    // history a node on the old one refuses.
    base.palw_epoch_boundary_budget = Some(ForkActivation::always());
    // **ADR-0044 Decision 9's two caps, enforced from block one** (mainnet audit 2026-09-06, L-2).
    // `max_prompt_tokens` and `max_decode_tokens` are already hashed into `palw_ruleset_id_v2`, so
    // every node advertises them; past this fence the lane's validation is given the struct it was
    // never given and refuses a job above either by name. testnet-11 and devnet have run since
    // genesis with them unread, so arming it there would refuse jobs those chains have accepted.
    base.palw_fp_ruleset_caps = Some(ForkActivation::always());
    // Both halves of ADR-0083 Decision 1, stated together: the assembly validates before
    // `palw_rc_arm_phase1` runs, and the second half is refused on a base whose first is dormant.
    base.palw_difficulty_priced_rows = Some(ForkActivation::always());
    base.palw_receipt_rows_unpriced = Some(ForkActivation::always());
    // **ADR-0072 Decision 8 at the header stage** (mainnet audit, 2026-09-06 — C-1). A card is a
    // fresh chain: no history was judged under the rule where the DA pins ran only in the virtual
    // processor, so stating it from genesis costs nothing, and the three fields D8 names are pinned
    // on the path where a draw becomes a `bits`-priced difficulty row. It is a partial answer to
    // C-1 and the field's own doc says which part; it is not a reason to consider C-1 closed.
    base.palw_attempt_header_pins = Some(ForkActivation::always());
    base.palw_certification_rent = Some(ForkActivation::always());
    base.palw_chunk_cap_charge = Some(ForkActivation::always());
    // **ADR-0042 Decision 11's complete signature-context set, from block one** (mainnet audit
    // 2026-09-06, M-8). Ten contexts the acceptance layer verifies under are outside the frozen
    // set testnet-11 committed to, and on a card the DA and court ones are consensus-live from
    // genesis. A fresh network is the only place this can be stated — arming it on a running chain
    // re-mints that chain's identity — which is exactly what a card is for. The assembly
    // (`palw_v2_params_on_base`) writes the matching root into the bundle, and `validate_palw_v2`
    // refuses either half alone.
    //
    // Deliberately NOT in `palw_rc_arm_phase1`: that function runs for testnet-11 too, and
    // testnet-11 is live.
    base.palw_signature_contexts_v2 = Some(ForkActivation::always());
    // **The private-prompt set, armed from block one** (private-prompts design, 2026-09-05):
    //
    // * `palw_prompt_ids_merkle` (ADR-0081 D3) — a job's `prompt_token_ids_hash` is a tiled Merkle
    //   root, so the court can be shown ONE tile of a hidden prompt (`ArithmeticOpened`) instead of
    //   the whole list. Genesis-only by `validate_palw_v2`; the assembly writes trace format 4.
    // * `palw_panel_da` (ADR-0077 D16) — a commitment may carry no prompt at all (mode 2); the ids
    //   reach the drawn seats over the authenticated pull and never a block, a relay or an
    //   unauthenticated serve.
    // * `palw_da_court` (ADR-0062) — the accusation/disclosure court, arm-able now that a seat
    //   answers accusations (`palw_da_duties_v2`, `PalwBackend::disclose_trace_event`) and the
    //   disclosure is verified against the roots a claim actually carries. A fresh card has no
    //   history, so ADR-0062's "move `PALW_STATE_V2_VERSION` in the release that arms it" does
    //   not apply; the coupling is pinned by test for presets that do.
    base.palw_prompt_ids_merkle = Some(ForkActivation::always());
    base.palw_panel_da = Some(ForkActivation::always());
    base.palw_da_court = Some(ForkActivation::always());
    // **A court is only armable over a lane whose obligation the chain defines** (ADR-0072
    // Decision 8; mainnet audit 2026-09-06 M-3). `palw_da_court` above convicts a producer that
    // will not open an accused event inside `claim.trace_retention_daa` — and on the free-prompt
    // lane that number is written by the accused, so a producer that writes `0` is immune and an
    // honest one is weakly better off writing `0` too. Past this fence the transition derives the
    // obligation from the accepting block's DAA, exactly as the attempt lane's admission pins it.
    // Armed together with the court, from block one, because a fresh card has no history for
    // either to be retroactive about.
    base.palw_fp_da_pins = Some(ForkActivation::always());
    // **ADR-0018 §E's payout bounds** (mainnet audit, 2026-09-06 — H-2, H-3, M-1). A card is the
    // only network that can take these from block one: they change the coinbase in both directions
    // — four tightenings of what the builder emits and one relaxation of the size guard ahead of
    // it — so a live chain reaches them as a flag day and that is its operator's call. testnet-11
    // leaves them dormant and every coinbase it builds is byte-identical to today's.
    base.palw_validator_payout_bounds = Some(ForkActivation::always());
    // **B-2 (mainnet audit 2026-09-11): a carded mainnet counts only Final work in the share
    // census, from genesis** (ADR-0107). Without it, `produced_blocks` is written at Provisional
    // and never decremented on void, so fake or unfinalised blocks grow a class's cadence share
    // (observed 1‰→51‰ in 16 epochs). Armed here rather than derived: a card is where a mainnet's
    // fences are STATED, and this one has no reason to wait. testnet-11 leaves it dormant — arming
    // it there is a flag day, and the census it changes is the same one two live chains committed.
    base.palw_share_growth_final = Some(ForkActivation::always());
    // The 2026-09-11 audit's consensus fixes are correct from a mainnet card's genesis.
    base.palw_audit_2026_09_11 = Some(ForkActivation::always());
    // The deep audit findings (B-1/C-01/B-4/court cluster) likewise — correct from a card's genesis.
    base.palw_audit_2026_09_11_deep = Some(ForkActivation::always());
    // **ADR-0124, stated from a card's genesis.** A mainnet is born with its panel paid out of the
    // claim's own reward, its seats holding exposure, its seat floor at ten producer floors and
    // its claims priced by the compute they certify: the economics the operator asked for, with
    // no history judged under the other reading. testnet-11 reaches both by a scheduled height
    // (a flag day, `PALW_RC_PANEL_ECONOMY_FENCE_DAA`) or not at all — a live chain whose seats
    // have never held exposure cannot start holding it retroactively.
    base.palw_panel_economy = Some(ForkActivation::always());
    base.palw_work_priced_reward = Some(ForkActivation::always());
    // **ADR-0126's carve, stated from a card's genesis** (user decision 2026-09-25: mainnet takes
    // the numbers testnet-12 runs). The validator pool's subsidy share drops from the overlay's full
    // split, 30%, to 20%, and the tenth it gives up is escrowed for PALW: a claim's escrow is 720‰
    // of its block's subsidy, not the bundle's 620‰ — 3,200.85 MSK a claim at a 120 s block, the G
    // every ADR-0152 tier, the seat lock and the genesis-seat sizing are built on. The worker base
    // the lowered split leaves is 72%, which the carve fills exactly (ADR-0126 Decision 4, checked
    // by `validate_palw_v2`). testnet-11 reaches it by its 6,001 flag day; testnet-12 from genesis.
    base.palw_overlay_carve =
        Some(PalwOverlayCarveV1 { activation: ForkActivation::always(), subsidy_validator_bps: 2_000, worker_carve_permille: 720 });
    base
}

/// **A card certifies the free-prompt lane of the classes it REGISTERS** (mainnet audit,
/// 2026-09-05; ADR-0074 D6, ADR-0075 D6). The shared assembly installs
/// `palw_rc_fp_certified_class_ids_v1`, a set computed from a typed list of profiles that names the
/// graph-v2 dense projection while the card registers the graph-v5 row — so the RC's set names a
/// class its card does not register and omits the one it does. testnet-11 keeps that set (its
/// bundle is a live identity, closable only by an on-chain `ClassLaneCertified`); a card
/// re-derives it from the objects it actually assembled.
fn mainnet_certify_registered_classes_v1(mut params: Params) -> Params {
    if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode {
        let derived = crate::palw_e2e_adjudicability::palw_fp_certified_class_ids_of_registered_v1(bundle);
        bundle.state = bundle.state.clone().with_fp_certified_classes(derived);
    }
    params
}

/// **ADR-0068 Phase 1: the DAA height at which testnet-11's clock arms.** Chosen 2026-09-01
/// with the live tip at DAA 1,746: +3,254 covers a serialized fleet rollout (~14 min of mesh
/// downtime per node restart, measured) several times over. Until this height an armed and an
/// un-armed build are PEERS — the schedule is normalised out of `consensus_identity_id` — and
/// from it, every node must run a build that carries these fences or it refuses every new block
/// (the attempt lane's declared blue work changes). One height for both fences: ε is only
/// near-weightless once the other side of the comparison is real.
pub const PALW_RC_PHASE1_FENCE_DAA: u64 = 5_000;

/// **testnet-11's second flag day: the DA court, `PanelDa` and the model market** (2026-09-06).
///
/// Chosen at DAA 1,619 against a measured 6.2 DAA/h over the previous six hours (9.2 over the last
/// one): 281 DAA is between 12 and 45 hours of lead over a build, a three-host rsync and eight unit
/// restarts. Every seat must run a build carrying these fences BEFORE this height or it forks off
/// at it — the identity is unchanged until then (`consensus_identity_id` normalises a scheduled
/// fence, and since this release the pruning horizon it drags with it), so an un-upgraded seat
/// peers and warns rather than partitioning early, which is what makes a rolling upgrade possible.
pub const PALW_RC_DA_COURT_FENCE_DAA: u64 = 1_900;

/// **testnet-11's fourth flag day: the membership** (ADR-0095).
///
/// It gets a height of its own rather than riding the registry's, and the reason is not stylistic.
/// ADR-0095 writes two collections that enter the state root — the declarations and the tenure
/// clocks — so switching it on with `palw_model_lines`, which passed at DAA 1,900 and is long
/// live, would move the root under a running network the moment the code shipped. A consensus
/// change gets an activation, and this is one.
///
/// Chosen at DAA 2,011 against the recent rate: 389 DAA of lead, which is days rather than hours
/// for a build, a three-host rsync and the seat restarts. Every seat must run a build carrying
/// this fence BEFORE this height or it forks off at it.
pub const PALW_RC_MODEL_BENEFITS_FENCE_DAA: u64 = 2_400;

/// **testnet-11's flag day for the five-percent owner leg** (ADR-0114).
///
/// Chosen by the operator on 2026-09-11 at DAA ≈3,399, with the chain advancing 4–5 DAA an hour: about
/// a day of lead for the build, the fleet's rolling restart and the outside operators' own upgrades.
/// Below it every join and leave is split 5 % burned / 1 % to the owner; at and past it, 5 % / 5 %.
/// Every seat must run a build carrying this fence BEFORE this height or it forks off at it — the DAA
/// 2,400 fence was crossed unannounced once and the outside nodes left at ≈2,241; announce this one.
pub const PALW_RC_MODEL_LEG_V2_FENCE_DAA: u64 = 3_500;
/// **testnet-11's flag day for the one-million-MSK least seed** (ADR-0120).
///
/// Chosen by the operator on 2026-09-12 to ship with the 7,000 release (held context, the audit's deep
/// fixes) at a height of its OWN: `fork_id_v1` digests fired heights, not fence sets, so a fence that
/// shared 6,000 would let a build without it peer through the gate and part silently there. Below it a
/// market opens at 100,000 MSK paid in; at and past it, at 1,000,000 MSK. A line whose pledges are
/// still short at the height keeps them — every sompi is already locked in its sink — and opens when
/// they reach the new floor.
pub const PALW_RC_MODEL_SEED_V2_FENCE_DAA: u64 = 6_900;
/// **testnet-11's SECOND flag day for the 2026-09-11 pre-mainnet audit fixes** (`palw_audit_2026_09_11`).
/// A live chain gets the fixes at a height, not a re-genesis (ADR-0083 path (a)). Only this fence
/// arms on testnet-11; the audit's share-census fence (`palw_share_growth_final`, B-2/ADR-0107) and
/// dense-court fences (`palw_fused_dissectable` / `palw_attn_anchored_root`, AC-D8) are mainnet-card
/// corrections and stay dormant on testnet-11 until the operator decides otherwise.
///
/// **It MUST be a height no other schedule entry uses, and NOT 3,500.** ADR-0114's owner-leg flag
/// day already schedules 3,500 (fleet fingerprint `02c7282b`, schedule id `8934a9b1`,
/// 1150/1900/2150/2400/3500/2125000). `fork_id_v1` digests the FIRED HEIGHTS, not the fence set —
/// so a build that adds a DIFFERENT fence at 3,500 advertises the SAME fork id as the 02c7282b
/// fleet, stays connected through the gate, and then silently diverges past 3,500. A fresh height
/// here adds a new schedule entry the gate can see, so a not-yet-upgraded node is refused FROM this
/// height rather than diverging at it. **Provisional** — set with deployment margin over the tip
/// at release time (t11 advances ~4–5 DAA/h; leave external operators room to upgrade), and rebased
/// onto the fleet's own base (ADR-0114's `42438178` lineage = origin/main 8b6661a9 + 5bed4405).
///
/// **ADR-0117's `palw_prefill_draw` shares this height, deliberately** (operator, 2026-09-11: one
/// rollout for both). The rule above is about adding a fence at a height an EARLIER build already
/// schedules; these two were scheduled together and reach main in one merge, so no released build
/// carries one without the other. The blind spot is exact all the same: a build with the audit
/// fence and not the draw fence (`a8f00497` alone, fingerprint `09efd285…`) advertises this build's
/// fork id and diverges silently at 4,000 — it must never run on testnet-11.
pub const PALW_RC_AUDIT_FENCE_DAA: u64 = 4_000;

/// **testnet-11's held-regime flag day: DAA 6,000** (ADR-0118; the regime is ADR-0103's).
///
/// Chosen by the operator on 2026-09-12: the deep-audit flag day's height
/// (`palw_audit_2026_09_11_deep`), so the regime, ADR-0119's class ladder and the deep fixes ride
/// ONE release and the fleet re-deploys once. [`palw_arm_held_regime_at_v1`] arms the three fences
/// the regime takes together.
///
/// **Every fence at one height ships in one build.** `fork_id_v1` digests the FIRED HEIGHTS, not
/// the fence set, so a node carrying some of a height's fences and not the others advertises the
/// same fork id and diverges silently past it — where a height of its own is a schedule entry the
/// gate refuses a stale node at. So the height is one no other entry uses, or one whose other
/// fences ship in the same release: not 3,500 (ADR-0114, deployed), not 4,000 (the audit and
/// `palw_prefill_draw`, released without the regime). **6,000 is shared with the deep-audit fence,
/// so no build may carry one of the two without the other**: every node must run the release that
/// carries both before 6,000.
///
/// **Moved from 7,000 to 6,000 on 2026-09-17** at the operator's instruction ("時間がかかりすぎる"),
/// with the tip at DAA 5,798 and the fleet still on the 7,000 release (`13520042`, `ae1d6162…`),
/// which never reaches its height: 6,000 is a height that release does not schedule, so the fork-id
/// gate refuses it from 6,000 and the whole fleet must carry this build before then. ADR-0120's
/// 6,900 stays where that release put it and now follows this height.
pub const PALW_RC_HELD_FENCE_DAA: Option<u64> = Some(7_100);

/// **The held regime on a network minted before it, at one height** (ADR-0118 Decision 6).
///
/// `palw_held_context` requires, at or below its height, the k-ary court, the data-availability
/// court, `PanelDa`, the one-move court (the regime refuses the bisection, so something else must
/// already convict) and the retention pins (a demand must fit inside a retention the chain derived)
/// — `Params::validate_palw_v2` refuses it otherwise. testnet-11 armed the first three from
/// genesis and 1,900; the last two it never armed, so they take the regime's own height, and the
/// held fence past genesis commits the V4 signature contexts the one-move court's accusation
/// signs under (Decision 1) and leaves every class but a held one its flat prompt ids (Decision 3).
pub fn palw_arm_held_regime_at_v1(params: &mut Params, daa: u64) {
    let at = Some(ForkActivation::new(daa));
    params.palw_held_context = at;
    params.palw_shard_court = at;
    params.palw_fp_da_pins = at;
}

/// **testnet-11's THIRD flag day: the 2026-09-11 audit's DEEP findings** (`palw_audit_2026_09_11_deep`
/// — B-1 merged-work escrow, C-01 fused-terminal canonical count, B-4 pre_pow-inclusive execution
/// dedup, and the C-02/C-03/C-08/BC-SYBIL court-economics cluster). A single fence for the six, at a
/// height AFTER [`PALW_RC_AUDIT_FENCE_DAA`], because the deep set was not finished when the shallow
/// fence's flag day shipped and holding that day for it would have stranded the ready shallow fixes.
///
/// **It MUST be a height no RELEASED schedule entry uses, and greater than 4,000.** Same reason the
/// shallow fence is not 3,500: `fork_id_v1` digests the FIRED HEIGHTS, so a build that carries the
/// deep set and one that does not MUST advertise different schedules — a distinct height is what the
/// gate sees, so a not-yet-upgraded node is refused FROM this height rather than diverging silently.
/// The held regime's flag day ([`PALW_RC_HELD_FENCE_DAA`], ADR-0118) shares the height by the
/// operator's choice, which is legal only because no release carries one of the two without the
/// other: the 7,000 release carried both at 7,000, this build carries both at 6,000 (moved
/// 2026-09-17, unreached), and every earlier release carries neither.
/// Putting the deep set on the 4,000 fence instead would make a deep build and a shallow build share
/// a fingerprint and a schedule yet fold differently at 4,000 — a silent split. **Provisional** —
/// set with deployment margin over the tip when the deep set ships, and announced as its own flag
/// day. 7,000 from 2026-09-12; 6,000 from 2026-09-17 (the operator: the wait was too long).
pub const PALW_RC_AUDIT_DEEP_FENCE_DAA: u64 = 7_100;

/// **testnet-12's second clock: settled anchors an economic deadline needs beside its DAA window.**
///
/// Thirty `Final` attempt claims. Not derived — a drill's starting point, chosen so that (a) it is
/// far more than a colluding quorum can produce alone (each anchor is a won class draw AND a
/// licensed panel, so a quorum that also runs producers pays a full inference per anchor and still
/// needs the other seats' locks), and (b) at the cadence the fleet showed on testnet-11 (about one
/// `Final` per 10 DAA when producing) it sits well inside the shortest economic window it gates
/// (`window_court` = 3,000 DAA), so an honest seat's liability expires on the DAA clock as before.
/// A network with no producer at all leaves every economic deadline open — which is the point: on
/// heartbeat-only history nothing is secured, so nothing economic may conclude. Retune from the
/// drill's measured anchor cadence, never from a calendar.
pub const PALW_T12_SETTLED_ANCHOR_DEPTH: u64 = 30;

/// **The standard ADR-0147 admission-audit period, in DAA** (user decision, 2026-09-23): a
/// `Candidate` class meets its jury every 100 DAA rather than once per epoch.
pub const PALW_ADMISSION_AUDIT_PERIOD_DAA_STANDARD: u64 = 100;

/// **testnet-11's third flag day: the refutation ladder** (ADR-0084 U-08, the 2026-09-06 audit's
/// H-4, ADR-0092 §9 step 2).
///
/// **It has a height of its own, and the reason is measured rather than stylistic.** The ladder
/// first rode [`PALW_RC_DA_COURT_FENCE_DAA`], and the fork-id gate cannot see that: `fork_id_v1`
/// digests the genesis hash and the FIRED HEIGHTS (`fired_fences_digest_v1`), never the fence set,
/// so a build carrying the ladder at 1,900 and one without it advertise the identical fork id at
/// every DAA, peer, and then disagree about which closes are valid past the height. Measured:
/// same fork id at 1,899, at 1,900 and at 2,500, same schedule, same identity. Adding a rule to a
/// height the schedule already names is invisible to the gate; adding a height is not.
///
/// 2,150 is 1,900 plus 250 DAA — about 50 hours at the fleet's measured 4.98 DAA/h, the same shape
/// of lead 1,900 itself was chosen with, and it lands after the second flag day rather than beside
/// it so an operator upgrades once per height rather than once for two rules at one height.
pub const PALW_RC_COURT_LADDER_FENCE_DAA: u64 = 2_150;

/// **testnet-11's flag day of 2026-09-17: ADR-0124, ADR-0125, ADR-0126 (revised), ADR-0128 and
/// ADR-0130 M1/M2, at one height** (the operator's height and scope: "t11 で有効にする高さは DAA 7000で"
/// → 7,001 that morning; moved with the held regime's height the same day — "DAA7001に設定されてるのを
/// 6001に変更して　時間がかかりすぎる" — to 6,001, with the execution lane at one block a second).
///
/// **One past the held regime's height, and that is the rule rather than a preference.** `fork_id_v1`
/// digests the FIRED HEIGHTS, deduplicated: these fences at 6,000 would leave this build advertising
/// the schedule a build carrying only the held regime at 6,000 advertises, the two would peer, and
/// they would disagree past it about what a panel is paid, which blocks the lane admits, what the
/// validators' carve is and which reorgs the stake gate refuses. 6,001 is a height no released
/// schedule names, so a node without the set is refused from it
/// (`a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id`, ADR-0120's 6,900 for the same
/// reason). Every node must run a build carrying it before the height — and before 6,000, the
/// moved height the deployed release is refused on first.
pub const PALW_RC_PALW_UPGRADE_FENCE_DAA: u64 = 7_101;

/// **ADR-0143's height on testnet-11: an artifact root gets one owner at DAA 6,702** — the operator's
/// call (2026-09-19) to ship it with the 6,300 flag day rather than behind its own later one.
///
/// **6,702 and not 6,300, for the reason [`PALW_RC_PALW_UPGRADE_FENCE_DAA`] is 6,301 and not 6,000.**
/// `fork_id_v1` digests the FIRED HEIGHTS, deduplicated, so a fence at a height this schedule
/// already names is INVISIBLE to the handshake: a build carrying the 6,300 set without this fence
/// would advertise the identical schedule, the two would peer, and they would disagree from 6,300
/// about which line owns an artifact root — a silent fork with no warning, which is the one failure
/// the fork id exists to turn into a named refusal. 6,702 is a height no released schedule names, so
/// a node without it is refused from there
/// (`a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id`).
///
/// **It is one release with the 6,300 day and one operator decision**; only the height is its own,
/// and the two DAA of separation are what make the difference checkable rather than silent.
pub const PALW_RC_ARTIFACT_ROOT_OWNERSHIP_FENCE_DAA: u64 = 7_102;

/// **ADR-0130 latency: the execution lane's schedule span shortens 5 DAA → 1 DAA at DAA 7,300.**
///
/// The 7,101 lane keeps `schedule_span_daa = 5` for every fold below this height. Past it, one span
/// is one DAA (~120 s), so Final → snapshot → future seed → permit is two spans (~4 min) instead of
/// ~20. The two-span seed delay itself is kept (ADR-0130 Decision 4). 7,300 is a height no released
/// schedule names and is one before ADR-0134's retirement at 7,301, so the fork-id gate can see it.
pub const PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA: u64 = 7_300;

/// **ADR-0134's height on testnet-11: the compute overlay retires at DAA 6,201** — its own height,
/// two hundred past the 6,001 flag day (7,201 past 7,001 until the flag day moved on 2026-09-17; the
/// two hundred is kept), so the fork-id gate separates a build that carries the retirement from one
/// that does not, and a failure at either height names its cause (the operator: the beacon
/// retirement is a different failure domain from the liveness and economics fences).
pub const PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA: u64 = 7_301;

/// **ADR-0133 Verification V2 (S1) — testnet-11 arms it at DAA 6,100**, its own height a hundred DAA
/// past the 6,001 flag day and a hundred before ADR-0134's retirement: the licence rule generalises
/// (segment-scoped receipts count by coverage; a full receipt is V1's), so a build without it is
/// refused there and a fleet that crossed 6,001 has its own day to cross this one. The operator's
/// order (2026-09-18): V1 full replay → S1 segmented replay → S3 if needed → S2 if speed wins; no
/// zero-knowledge proof is used or planned.
pub const PALW_RC_VERIFICATION_V2_FENCE_DAA: u64 = 7_200;

/// **ADR-0133 S3 on testnet-11.** Layer/position sampling, after S1 is live, before S2. 8,600 is
/// past the lock ledger (8,500) and unnamed on any released schedule, so already-committed
/// licences stay coverage-shaped until the operator crosses it. Mainnet stays `None`.
pub const PALW_RC_VERIFICATION_S3_FENCE_DAA: u64 = 8_600;

/// **ADR-0133 S2 on testnet-11.** Optimistic licence from the full-replay seat, after S3. 8,700
/// is past S3 and unnamed on any released schedule. Mainnet stays `None`.
pub const PALW_RC_VERIFICATION_S2_FENCE_DAA: u64 = 8_700;

/// **ADR-0138 + ADR-0142 — the DAA clock and its cursor, on testnet-11 at DAA 7,780.**
///
/// The 6,001 / 7,101 flag day left both dormant: past the anchor clock only a heartbeat can
/// advance the score, and under the selected-parent slot rule an attempt block postponed the next
/// beat (the registry drill froze at its own flag day). ADR-0142's cursor is in this tree; the
/// live chain's DAA is still vanilla GHOSTDAG, so a heartbeat that merges *N* parallel attempt
/// tips jumps `+N` (measured 2026-09-20: DAA 7,210 for an hour of QWEN siblings, then heartbeat
/// `df80394b` at 7,219 with nine parents).
///
/// The old slot rule treats a bonded PALW parent as "the chain is producing" and holds the
/// heartbeat for an hour. On a PALW-only network that is a misread: a BASE-0 (or any attempt)
/// win advances one DAA and then the class lottery goes silent, so nothing produces the next
/// block either (measured 2026-09-21: DAA 7,680 then again 7,732 BASE-0 tip, ~45 minutes of no
/// blocks, next beat an hour later). 7,900 was past the first stall and unnamed, but the chain
/// has not reached it: every BASE-0 win before the fence restarts the hour. 7,780 is past the
/// live tip (~7,732) and unnamed on any released schedule, so already-committed `daaScore`
/// values stay valid and the fork-id gate can see the fence. The single lottery arms with the
/// clock (one setter); the cursor arms at the same height. A fleet still advertising 7,900
/// must roll before 7,780.
pub const PALW_RC_ANCHOR_CLOCK_FENCE_DAA: u64 = 7_780;

/// **Spend-once execution-round quanta on the BPS1 lane.** Past this height a verified Attempt
/// Final mints `ceil(CanonicalWork / 100_000)` unique 1-second permits instead of drawing the
/// ADR-0125 credit lottery. 7,800 is ~40 DAA past the live tip (~7,740) and 20 DAA past the
/// clock, so already-committed schedules stay lottery-shaped and the fork-id gate can see the
/// fence. Mainnet stays `None`.
pub const PALW_RC_EXECUTION_QUANTA_FENCE_DAA: u64 = 7_800;

/// **ADR-0133 §7: `required_ready_seats` asks possession, and capacity answers elsewhere.**
/// Past this height the seat gate is the panel plus the spare, and a class whose replay outruns
/// its fleet is admitted rarely (one claim in flight) instead of refused. The shipped rule
/// returned the larger of possession and "seats to sustain one claim a span", which for the held
/// 2M Qwen2.5 row — canonical job `n_ctx / 8 = 262,143` prefill by ADR-0103 Decision 14, 1,998
/// reference seat-spans a replay — asked 9,992 ready seats of a seven-bond network and closed
/// `Prefetching` for good. 8,100 is past the clock (7,780) and the live tip (~7,861) and unnamed
/// on any released schedule, so the fork-id gate can see the fence and every row already written
/// keeps the profile it was derived with. Mainnet stays `None`.
pub const PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA: u64 = 8_100;

/// **ADR-0133 §11.3: a class's receipt deadline is the window its own graph derives.** The
/// network's `window_receipt` is one number for every class, and §11.3 already refuses to stretch
/// it — a class whose derived window does not fit is `overloaded` and goes to `Held`. Past this
/// height the deadline a bound panel is judged against is
/// `max(window_receipt, verification_window_spans × span_daa)`: the network's floor for every
/// class that already fits, and its own window for one that does not, so the 2M Qwen2.5 row
/// (1,399 spans × 10 DAA against a 600-DAA receipt window) can leave `Probation` without the
/// global deadline being weakened for anybody. Never SHORTER than `window_receipt`, so no claim
/// alive under the old rule has its window tightened by the fence. 8,160 is past
/// [`PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA`] — the seat gate has to open before a class can
/// reach the deadline this widens — and unnamed on any released schedule. Mainnet stays `None`.
pub const PALW_RC_CLASS_RECEIPT_WINDOW_FENCE_DAA: u64 = 8_160;

/// **ADR-0144 §9: the live lock ledger, as one future bundle.** Past this height a Valid
/// receipt locks `required = palw_max_fraud_gain_v1(claim)/3+1`, Final keeps a liability,
/// BondRetire while locked is refused, and a PanelFalseValid debit spends the lock once.
/// 8,500 is past the clock (7,780) and unnamed on any released schedule. Genesis 10k MSK
/// covers one dense-row seat (~997 MSK); the 400k sompi registry floor is unchanged.
/// Mainnet stays `None`.
pub const PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA: u64 = 8_500;

/// **The build before the 6,001 flag day**, for the tests that reconstruct an earlier release: the
/// fences [`PALW_RC_PALW_UPGRADE_FENCE_DAA`] schedules, cleared (the held regime and the deep audit
/// stay at THIS build's 6,000; [`palw_rc_deployed_7000_release_for_tests`] is the fleet's build).
#[cfg(test)]
pub(crate) fn palw_rc_clear_flag_day_6001_for_tests(params: &mut Params) {
    // ADR-0134's retirement at 6,201 came after the flag day: the 7,000 release carries neither.
    params.palw_compute_overlay_retired = None;
    params.palw_panel_economy = None;
    params.palw_work_priced_reward = None;
    params.palw_execution_lane = None;
    params.palw_overlay_carve = None;
    params.dns_bft_gate = None;
    // ADR-0135 Upgrade A, ADR-0132 Upgrade C and ADR-0137 ride the same flag day.
    params.palw_model_registry = None;
    params.palw_verification_v2 = None;
    params.palw_verification_s3 = None;
    params.palw_verification_s2 = None;
    params.palw_readiness_v2 = None;
    params.palw_economic_payout = None;
    params.palw_work_target = None;
    params.set_palw_single_lottery(None);
    params.set_palw_short_challenge_window(None);
    params.palw_verification_v2 = None;
    params.palw_verification_s3 = None;
    params.palw_verification_s2 = None;
    params.palw_readiness_v2 = None;
    params.palw_anchor_clock = None;
    params.palw_clock_cursor = None;
    // ADR-0143's own day is two past the flag day and later than every release this reconstructs.
    params.palw_artifact_root_ownership = None;
    params.palw_canonical_work = None;
    params.palw_objective_offence = None;
    params.palw_seat_gate_possession = None;
    params.palw_class_receipt_window = None;
    params.palw_execution_quanta = None;
}

/// **The fleet's release as deployed (`13520042`, fingerprint `ae1d6162…`)**: no flag-day set, no
/// retirement, and the held regime with the audit's deep fence at the 7,000 that release scheduled —
/// the peer this build must keep below 6,000 and refuse from it.
#[cfg(test)]
pub(crate) fn palw_rc_deployed_7000_release_for_tests(params: &mut Params) {
    palw_rc_clear_flag_day_6001_for_tests(params);
    palw_arm_held_regime_at_v1(params, 7_000);
    params.palw_audit_2026_09_11_deep = Some(ForkActivation::new(7_000));
}

/// Arm the ADR-0068 Phase 1 fences on an assembled PALW-RC ruleset: the heartbeat lane (with
/// its width bound) and the attempt-work constant, both at [`PALW_RC_PHASE1_FENCE_DAA`]. The
/// values are the binary's own constants by construction, so the `validate_palw_v2` locks hold
/// by definition — asserted anyway, because "by definition" is how two sources drift.
fn palw_rc_arm_phase1(mut params: Params) -> Params {
    // ADR-0068 Phase 2: armed FROM GENESIS. Phase 1 scheduled the fences at DAA 5,000 on the
    // running Relaunch-4 chain (a scheduled fence keeps the identity, so the fleet could roll);
    // this branch is the Relaunch-5 identity, whose chain begins with the clock already open —
    // "genesis から armed" is the mainnet preset's rule and the testnet rehearses what mainnet
    // ships. A fresh chain's first block may be a heartbeat (the drill's zero-bond devnet
    // bootstrapped exactly that way).
    params.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    });
    params.palw_attempt_work = Some(PalwAttemptWorkV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    });
    // **ADR-0065 Decision 4, armed — the launch audit's other unattended fence.**
    //
    // Dormant, an `Unavailable` quorum needs no proof of anything: a seat that did not RECEIVE a
    // producer's material signs the same verdict as a seat the producer withheld from, and three
    // of them void the claim, slash the producer's bond and slash the honest seats that signed
    // `Valid`. Nothing in that sentence requires an attacker. This tree's own measurement, recorded
    // beside the panel parameters, is that roughly a third of remote seats' verdicts on testnet-11
    // were transport rather than withholding, which puts a quorum of three within ordinary reach —
    // the audit's arithmetic was about one claim in twelve.
    //
    // Armed, the `ProducerUnavailable` quorum becomes unreachable and such a claim falls to a
    // redraw and then a receipt timeout, which voids WITHOUT slashing. That is the correct verdict
    // for "the network did not deliver": it is not a finding about anybody's conduct, and a rule
    // that destroys an honest participant's collateral for the network's own packet loss is worse
    // than no rule.
    //
    // Armed here rather than scheduled because Relaunch 5 is a new genesis — there is no running
    // chain whose identity a fence would have to preserve, so the honest form is the one that is
    // true from block one.
    params.palw_unavailable_abstains = Some(ForkActivation::always());
    // **The rest of the set a shipped V2 network arms — spelled HERE because it was spelled
    // nowhere a carded mainnet passes through** (mainnet audit, 2026-09-05).
    //
    // ADR-0069 Decision 7 and ADR-0083 Decision 1 were stated as literals on `palw_rc_base_params`
    // and on `DEVNET_PARAMS`: two bases, neither of which mainnet uses. `mainnet_shipped_params`
    // assembles over `MAINNET_PARAMS` — where every PALW fence is `None` — and then called this
    // function, which armed three fences and stopped. `docs/adr/README.md` says a carded mainnet
    // "assembles the RC's ruleset with the same fences armed from genesis"; it did not. It was born
    //
    //  * with the heartbeat lane armed (above) and `palw_difficulty_priced_rows` dormant, which is
    //    exactly the state ADR-0083 exists to escape. Measured on testnet-11 Relaunch 5f: five
    //    heartbeat emitters on the two-minute cadence tightened `bits` from p = 0.5 to p = 1.5e-3
    //    in 826 blocks and no bonded lane could win again. Mainnet's `difficulty_window_size` is
    //    661 against the RC's 264, so its integration is 2.5x slower and the wedge deeper;
    //  * and with `palw_uncertified_weightless` dormant — which `validate_palw_v2` refuses to arm
    //    above genesis ("it is genesis-only, or dormant"), so an uncertified class's pwu would
    //    enter that chain's fork-choice weight PERMANENTLY, and the free-prompt retirement hole
    //    whose end state is `load_tip -> CarriageInconsistent` would stay open for its whole life.
    //
    // Every write below is **only if the preset left it dormant**, which is what makes this
    // function safe to route every shipped assembly through: it can add a fence a base forgot, and
    // it can never move one a base decided. testnet-11 states `palw_difficulty_priced_rows` at DAA
    // 1150 — it is a live chain and ADR-0083 had to reach it as a flag day rather than as a
    // genesis — and keeps that value here. A fresh chain takes `always()`, which is free: its
    // window holds no priced row at block 1 either way.
    if params.palw_uncertified_weightless.is_none() {
        params.palw_uncertified_weightless = Some(ForkActivation::always());
    }
    if params.palw_difficulty_priced_rows.is_none() {
        params.palw_difficulty_priced_rows = Some(ForkActivation::always());
    }
    if let Err(e) = params.validate_palw_v2() {
        panic!("the armed PALW-RC ruleset does not validate: {e}");
    }
    params
}

/// **ADR-0103: the held regime, minted on a shipped lattice** — the one spelling the generator,
/// the tests and the devnet flag share, so "the network this ADR describes" is a function and not
/// four hand-copied mutations.
///
/// From genesis: the COMPLETE_V4 signing-context set (`palw_signature_contexts_v2`), trace format 4
/// and the tiled prompt ids (`palw_prompt_ids_merkle`), the k-ary court, the one-move court,
/// `PanelDa`, the data-availability court and the free-prompt pins (ADR-0111 Decision 5), and
/// `palw_held_context` itself — every precondition `validate_palw_v2` names — with
/// the court's ladder minted at `ladder` (Decision 1: at the carrier's budget, since no round is
/// played). Every other number is the base lattice's. `Err` is `validate_palw_v2`'s refusal,
/// verbatim: a mint this function cannot assemble is not one a node would start on.
///
/// Not a preset and never a default: nothing ships it, and every shipped fingerprint is unmoved by
/// its existence (the fence stays `None` on every preset).
pub fn palw_held_context_mint_v1(mut params: Params, ladder: u64) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    // **ADR-0119 Decision 1: no deeper than the regime's own ladder.** Under the regime every fused
    // leaf of every class is tried by a dissection whose session is a bisection ladder over the
    // claim's ladder, and a bisection ladder refuses a space past `2^40` — so a network ladder past
    // it minted a court that could not open for any fused class. (ADR-0103 D1's `2^48` example.)
    if ladder > crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1 {
        return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
            "the held regime's ladder is at most 2^40: a dissection's session is a bisection ladder over the claim's ladder, \
             and no bisection ladder opens past that (ADR-0119 Decision 1)",
        ));
    }
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
        return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
            "the held regime is a ConsensusV2 ruleset; this base carries no V2 bundle",
        ));
    };
    // ADR-0152 IMPL-7: a network that arms R-core+ commits to V5, which is V4 plus R-core+'s
    // contexts — the held regime's three are in both.
    bundle.signature_contexts_root = if params.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never()) {
        crate::palw_mode_v2::palw_v2_signature_contexts_root_v5()
    } else {
        crate::palw_mode_v2::palw_v2_signature_contexts_root_v4()
    };
    bundle.trace_format_version = crate::palw_mode_v2::PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS;
    let court = bundle.court;
    bundle.court = crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(
        ladder,
        court.turn_deadline_daa(),
        court.terminal_rounds(),
        court.max_close_bytes(),
        court.max_terminal_macs(),
        court.max_operand_count(),
    )?
    .with_dissection_arity(court.dissection_arity())?;
    // The seventh wall (ADR-0103 §10): the advertised prompt cap is the regime's, not the frame's.
    bundle.freeprompt = bundle
        .freeprompt
        .clone()
        .with_held_prompt_cap_v1(crate::palw_freeprompt_v3::PALW_FP_HELD_MAX_PROMPT_TOKENS_V1)
        .map_err(|_| crate::palw_mode_v2::PalwModeV2Error::Invalid("the held prompt cap is not a legal free-prompt cap"))?;
    params.palw_prompt_ids_merkle = Some(ForkActivation::always());
    params.palw_signature_contexts_v2 = Some(ForkActivation::always());
    params.palw_kary_court = Some(ForkActivation::always());
    params.palw_shard_court = Some(ForkActivation::always());
    params.palw_panel_da = Some(ForkActivation::always());
    params.palw_held_context = Some(ForkActivation::always());
    // ADR-0111 Decision 5: the data-availability court and the free-prompt pins, from genesis. A
    // seat that holds no capture demands a leaf's evidence through the court, and the pins make the
    // retention that demand must fit inside a chain fact rather than the producer's own number.
    params.palw_da_court = Some(ForkActivation::always());
    params.palw_fp_da_pins = Some(ForkActivation::always());
    // ADR-0082 Decision 3: the frozen arity must be the one the held court derives from genesis.
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
        unreachable!("matched above")
    };
    let derived = crate::palw_court_v2::palw_court_params_held_at_v2(bundle, true, true).map_err(|_| {
        crate::palw_mode_v2::PalwModeV2Error::Invalid("no dissection arity fits this lattice's court window with the ladder at zero")
    })?;
    bundle.court = bundle.court.with_dissection_arity(derived.dissection_arity())?;
    // The court widens the pruning horizon to hold its phase: re-derived from the bundle, raised and
    // never lowered, exactly as the shipped assembly derives it.
    let bundle = bundle.clone();
    params = params.with_palw_v2_depths(&bundle);
    params.validate_palw_v2()?;
    Ok(params)
}

pub fn palw_rc_shipped_params() -> Params {
    // The artifact root is the one thing code cannot mint. Without it there is no class to
    // register and therefore no ruleset to assemble — that, and only that, is what falls back to
    // the bundle-free base. An empty BOND registry does not: see `palw_rc_genesis_card_is_set`.
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return palw_rc_base_params();
    }
    let bonds: Vec<_> = PALW_RC_GENESIS_BONDS
        .iter()
        .map(|c| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint(c.premine_index)),
            pubkey: c.bond_pubkey.to_vec(),
            operator_pubkey: c.operator_pubkey.to_vec(),
            payout_payload: crate::Hash64::from_bytes(c.payout_payload),
        })
        .collect();
    // **Two classes when the Qwen3.6 root is pinned, one when it is not.** The branch is on a
    // CONSTANT, not on what the node happens to hold: a network's class set is part of its ruleset
    // id, so a node that decided this from a local file would fingerprint differently from its
    // peers and the two would refuse each other — which is the fail-safe direction but the wrong
    // question. Whether this node can SERVE the class is a separate matter, answered by the
    // backend registry at the moment a claim names it (`--palw-class-artifact`); a node without
    // the weights still validates the chain, it simply cannot produce for that class.
    if palw_rc_qwen36_is_registered() {
        // The dense slot is ADR-0082's graph-v5 512 row (5f card §2); its root is the same
        // inventory root by measurement — see `PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT`.
        let dense = palw_rc_qwen25_a16_is_registered().then_some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT);
        return palw_rc_arm_phase1(
            palw_rc_params_with_classes(PALW_RC_GENESIS_ARTIFACT_ROOT, PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT, dense, bonds)
                .unwrap_or_else(|e| panic!("the pinned PALW-RC genesis card does not assemble: {e}")),
        );
    }
    palw_rc_arm_phase1(
        palw_rc_params_from_artifacts(PALW_RC_GENESIS_ARTIFACT_ROOT, bonds)
            // A card that is set and does not assemble is a binary that would boot a network its own
            // genesis gate refuses. Failing at startup with the gate's own message is the only honest
            // outcome; silently falling back to the bundle-free base would put a node on a chain it
            // cannot join and tell it nothing.
            .unwrap_or_else(|e| panic!("the pinned PALW-RC genesis card does not assemble: {e}")),
    )
}

/// **testnet-12 as it ships: the fences at DAA 0 and the two HELD dense rows.**
///
/// [`palw_rc_shipped_params`]'s shape over [`palw_t12_base_params`], with one substitution and one
/// consequence.
///
/// The substitution is the row set ([`PALW_T12_GENESIS_HELD_ROWS`]): the dense family's graph-v7 row
/// at [`PALW_T12_NARROW_DENSE_N_CTX`] over [`PALW_T12_GENESIS_QWEN25_A16_8K_ARTIFACT_ROOT`] — the row
/// a fleet host can produce at — and at [`PALW_T12_DENSE_N_CTX`] over
/// [`PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT`] (class `74c67e63…`, ADR-0103's widest). No hybrid
/// row: the held map is wrong for Qwen3.6's geometry (see [`PALW_T12_HYBRID_N_CTX`]). Both rows
/// declare the grant floor and nothing more — the work target is armed from DAA 0, so block rights
/// come from verified work, not from a declared share.
///
/// The consequence is that this preset MUST arm `palw_held_context` from genesis, and it does — not as
/// a special case but because [`palw_t12_arm_every_rule_from_genesis`] arms every rule this binary
/// knows. The admission gate refuses a held row while the held fence is dormant, so a preset that got
/// it wrong fails its own genesis gate at startup rather than booting a network whose classes nobody
/// can produce for.
///
/// **Every other width in 512…2M is registrable from block one**, by transaction, with no fence to
/// wait for: `palw_context_ladder` (the court prices the checkpoint, not the context),
/// `palw_model_registry` (ADR-0135's permissionless registry), `palw_seat_gate_possession` and
/// `palw_class_receipt_window` are all in force at DAA 0. Only the rows the operator put in the
/// genesis are in the genesis BLOCK.
pub fn palw_t12_shipped_params() -> Params {
    palw_t12_public_params_over_v1(palw_t12_base_params())
}

/// **testnet-12 as it LAUNCHED** (`0e8ec984e`; params `b8564b88…`, identity `5de80e64…`, schedule
/// `93da24cc…`): [`palw_t12_shipped_params`] with every fence of [`PALW_T12_POST_LAUNCH_FENCES_V1`] set
/// back to dormant, each through its own `set` (mirrors included). The ruleset a node that has not
/// taken the post-launch release still runs — byte for byte, which
/// `post_launch_fence_arming_tests` pins to the launch release's three ids — and the baseline the
/// post-launch lanes' tests arm ONE fence over, so each lane's test keeps judging its own fence
/// against the build it replaces.
pub fn palw_t12_launch_params_v1() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    // The second flag day's list first (its fences ride the DAA-750 ones' prerequisites), then the
    // DAA-750 list: testnet-12 as it launched carries neither.
    for fence in PALW_T12_POST_LAUNCH_FENCES_V4
        .iter()
        .chain(PALW_T12_DECODE_RULES_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1)
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V3)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V2)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V1)
    {
        (fence.set)(&mut params, None);
    }
    params
}

/// [`palw_t12_shipped_params`]'s assembly over a given base — public testnet-12's genesis registry and
/// premine on `base`. The preset passes [`palw_t12_base_params`] itself; a test passes it with a
/// post-launch fence set after pass 2, to prove the assembly carries what was set there.
fn palw_t12_public_params_over_v1(base: Params) -> Params {
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return base;
    }
    // **Each card's bond is named on testnet-12's OWN premine txid** (`premine_outpoint_for`; user
    // decision 2026-09-24, replay separation): the same indices as testnet-11's cards, on a txid
    // no private chain that shared the sentinel ever minted.
    let bonds: Vec<_> = PALW_T12_GENESIS_BONDS
        .iter()
        .map(|c| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint_for(base.net, c.premine_index)),
            pubkey: c.bond_pubkey.to_vec(),
            operator_pubkey: c.operator_pubkey.to_vec(),
            payout_payload: crate::Hash64::from_bytes(c.payout_payload),
        })
        .collect();
    let genesis_utxos = crate::config::premine::genesis_premine_utxos_for(base.net);
    palw_t12_params_with_registry_v1(base, bonds, genesis_utxos)
}

/// **A testnet-12 drill's params: the shipping ruleset on the drill's genesis** (ADR-0152 §8.2,
/// `phase2-plan.md` §1.8, P2-12). What `Params::from(testnet-12)` returns, with exactly three
/// substitutions, each the salt's (`config::drill`):
///
/// * the genesis block — the drill marker, the drill premine's `utxo_commitment`, the hash over both
///   ([`crate::config::drill::palw_t12_drill_genesis_block_v1`]);
/// * the genesis bond registry — public testnet-12's seats at the same premine indices, named on
///   the drill's premine txid and re-keyed with drill-only keys, so nothing a drill seat signs is
///   signed by a card key;
/// * `dns_seeders` emptied: a drill finds its peers by explicit address only, never through public
///   testnet-12's seeders (not a consensus input; `consensus_params_id` excludes it).
///
/// Every rule — every fence, window, class row, panel shape and the R-core+ bundle — is assembled by
/// the same [`palw_t12_params_with_registry_v1`] the public preset takes, so a drill drills the rules
/// that ship. The ids move only through the genesis and the registry: `consensus_params_id` and
/// `consensus_identity_id` hash both, and the handshake refuses on the genesis hash before either.
/// **Never called by `Params::from`**: only kaspad's `--palw-drill-genesis-salt` reaches it, and it
/// installs the salt on the node's `Config` in the same place, which the start-up guard checks.
pub fn palw_t12_drill_params_v1(salt: &crate::config::drill::PalwDrillSaltV1) -> Params {
    let mut base = palw_t12_base_params();
    base.genesis = crate::config::drill::palw_t12_drill_genesis_block_v1(salt);
    base.dns_seeders = &[];
    if PALW_RC_GENESIS_ARTIFACT_ROOT == crate::Hash64::from_bytes([0u8; 64]) {
        return with_registered_models(base);
    }
    let bonds = crate::config::drill::palw_t12_drill_genesis_bonds_v1(salt);
    let genesis_utxos = crate::config::drill::palw_t12_drill_genesis_utxos_v1(salt);
    // `Params::from(NetworkId)` wraps every preset in `with_registered_models`; the drill takes the
    // same wrapper so the two differ in the three fields above and nowhere else.
    with_registered_models(palw_t12_params_with_registry_v1(base, bonds, genesis_utxos))
}

/// **One post-launch fence of testnet-12's post-launch release** (the user's decision of 2026-09-26:
/// one fence height, [`PALW_T12_POST_LAUNCH_FENCE_DAA`], carrying every CRITICAL/HIGH fix found after
/// launch). An entry of [`PALW_T12_POST_LAUNCH_FENCES_V1`].
#[derive(Clone, Copy)]
pub struct PalwPostLaunchFenceV1 {
    /// The fence's name, exactly as [`Params::palw_fences_v1`] spells it (a test holds every entry to
    /// that list, so a misspelt or non-fence name fails the build).
    pub name: &'static str,
    /// **Sets the fence on an ASSEMBLED ruleset** — the field and every mirror of it the fold reads
    /// (a V2 bundle copy, say), and nothing else: `Some(at)` arms or moves it, `None` leaves it
    /// dormant. The drill's move ([`crate::config::drill::palw_drill_post_launch_fences_at_v1`]) and
    /// its tests call this and nothing else, so a lane that adds a fence to the release writes how to
    /// set it once, here.
    pub set: fn(&mut Params, Option<ForkActivation>),
}

impl std::fmt::Debug for PalwPostLaunchFenceV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// **The fences testnet-12's post-launch release arms at one post-launch height**
/// ([`PALW_T12_POST_LAUNCH_FENCE_DAA`], DAA 750 — the user's decision of 2026-09-26) — the ONE list of
/// them. Each lane that puts a fix behind the
/// release's fence adds its entry here and nowhere else (int-4 phase 2b: twelve entries — F1's three,
/// fork choice, maturity, sink, lane A, V02's two lock fences, bind-deadlock, MSK-26A's slash and
/// pptake2's IBD commit):
///
/// * the drill's `--palw-drill-fence-at=<DAA>` moves exactly these to a low height on a salted drill
///   chain, so a drill crosses the release's flag day with the shipping binary
///   ([`crate::config::drill::palw_drill_post_launch_fences_at_v1`]; the memory rule "a flag day
///   needs a drill that crosses it");
/// * the release arms exactly these at its height, through `set` so every mirror follows, on the
///   ASSEMBLED ruleset — in [`palw_t12_params_with_registry_v1`] after the assembly and before its
///   `validate_palw_v2`, the one body the public preset and every drill share. Never inside
///   [`palw_t12_base_params`] before its pass 2 (which zeroes every fence height it visits: a genesis
///   rule, a re-mint), and never at 1,000 (`palw_bond_maturity`'s height, which `validate_palw_v2`
///   refuses for `palw_bond_maturity_early` and the fork id cannot see).
///
/// **Lane A's entry needs the assembled ruleset**: its operators are the bonds the genesis REGISTRY
/// names, which a bundle-less base does not carry yet, so on one it panics rather than leave the
/// CRITICAL's stopgap silently dormant. Every other entry is a bare height or mirrors itself, and
/// the registry-resilience mirror is also re-made by the assembly
/// ([`palw_v2_params_with_class_rows_v1`]), so those may be set after pass 2 as well. With every
/// entry at one height, `validate_palw_v2`'s prerequisites among them hold (lane A over F1's seed at
/// or below it; the rest over fences testnet-12 arms at 0) — `drill.rs` and the processor's combined
/// crossing (`t12_post_launch_fences_combined`) test exactly that.
///
/// **Armed on testnet-12 at [`PALW_T12_POST_LAUNCH_FENCE_DAA`]** by
/// [`palw_t12_arm_post_launch_fences_v1`] (so on every drill ruleset too, which
/// `--palw-drill-fence-at` then MOVES), and dormant (`None`) on every other preset.
///
/// **Shipped; it does not grow again.** A fix for the NEXT flag day goes in
/// [`PALW_T12_POST_LAUNCH_FENCES_V2`] (lane F2, 2026-09-27).
pub const PALW_T12_POST_LAUNCH_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    // Lane F1, registry resilience (V03/V05): the field and the V2 bundle's mirror the fold reads.
    PalwPostLaunchFenceV1 {
        name: "palw_registry_resilience",
        set: |params, at| {
            params.palw_registry_resilience = at;
            params.sync_palw_registry_resilience();
        },
    },
    // Lane F1, the panel seed is the anchor attempt's execution commitment (CRITICAL): a bare height.
    PalwPostLaunchFenceV1 { name: "palw_panel_seed_execution", set: |params, at| params.palw_panel_seed_execution = at },
    // F1, heartbeat transparency stops at the merging block's own chain (CRITICAL): a bare height.
    PalwPostLaunchFenceV1 {
        name: "palw_heartbeat_transparent_same_chain",
        set: |params, at| params.palw_heartbeat_transparent_same_chain = at,
    },
    // Fork choice (HIGH): a deep reorg needs a strict economic win, a tie keeps the incumbent — unless
    // it is at most two DAA ticks deep on the incumbent's side, where GHOSTDAG's order decides it so
    // honest slot races converge (rcore/f1-strictwin-tie): a bare height.
    PalwPostLaunchFenceV1 { name: "palw_reorg_strict_economic_win", set: |params, at| params.palw_reorg_strict_economic_win = at },
    // Lane maturity (HIGH): ADR-0065 D1 before 1,000 for non-genesis bonds: a bare height, below
    // `palw_bond_maturity`'s 1,000 (never at it).
    PalwPostLaunchFenceV1 { name: "palw_bond_maturity_early", set: |params, at| params.palw_bond_maturity_early = at },
    // Lane sink (Position #1): a model sink is block-valid only bound: a bare height.
    PalwPostLaunchFenceV1 { name: "palw_model_sink_bound", set: |params, at| params.palw_model_sink_bound = at },
    // Lane A, the operator-anchored panel (the panel-seed CRITICAL's stopgap): the height over every
    // bond the genesis registers (testnet-12's eight operator cards), testnet-12's armed value. Needs an
    // assembled ConsensusV2 ruleset (see the list's doc); requires F1's seed at or below it.
    PalwPostLaunchFenceV1 {
        name: "palw_operator_anchor",
        set: |params, at| {
            let rule = at.map(|at| {
                params.palw_operator_anchor_of_genesis_bonds_v1(at).expect(
                    "palw_operator_anchor is set on an ASSEMBLED ConsensusV2 ruleset: its operators are the genesis registry's \
                     bonds (arm the post-launch list in palw_t12_params_with_registry_v1, after the assembly)",
                )
            });
            params.palw_operator_anchor = rule;
        },
    },
    // Lane V02 option (a) (HIGH): a resolved claim's lock is carried by the whole collateral with the
    // 4-floor accuser reserve: the field and the V2 bundle's mirror the fold reads.
    PalwPostLaunchFenceV1 {
        name: "palw_final_lock_full_collateral",
        set: |params, at| {
            params.palw_final_lock_full_collateral = at;
            params.sync_palw_final_lock_full_collateral();
        },
    },
    // Lane V02 lock life (HIGH): a resolved Valid seat's lock lives F + 1,000 and stops being slashable
    // when it stops being committed: the field and the V2 bundle's mirror the fold reads. Independent
    // of the entry above; both compose at one height (each is its own mirror on the state params).
    PalwPostLaunchFenceV1 {
        name: "palw_final_lock_life",
        set: |params, at| {
            params.palw_final_lock_life = at;
            params.sync_palw_final_lock_life();
        },
    },
    // Lane bind-deadlock (HIGH): an operator's own attempt at its bond's exposure ceiling still anchors
    // the claims due at it: a bare height, at or above lane A's (`palw_operator_anchor`) — its binder
    // reads lane A's anchor rule past that fence, and the release sets both to one height.
    PalwPostLaunchFenceV1 { name: "palw_anchor_at_ceiling", set: |params, at| params.palw_anchor_at_ceiling = at },
    // MSK-26A (audit Critical, hf-slash): the UTXO side-effect of a DNS slash obeys the genuineness
    // rule the registry path does: a bare height.
    PalwPostLaunchFenceV1 {
        name: "palw_slashing_evidence_utxo_genuine",
        set: |params, at| params.palw_slashing_evidence_utxo_genuine = at,
    },
    // hf-pptake2 (audit Critical): a pruning-proof / IBD staging commit needs a strict economic win,
    // read at the incumbent's DAA: a bare height.
    PalwPostLaunchFenceV1 {
        name: "palw_pruning_proof_strict_economic_win",
        set: |params, at| params.palw_pruning_proof_strict_economic_win = at,
    },
    // Lane accept-order (the 2026-09-26 IBD audit's consensus finding): a merging block applies a tied
    // round lane parents-first, and a round block carries no EVM payload: a bare height, over the
    // execution lane (testnet-12 opens it at genesis).
    PalwPostLaunchFenceV1 { name: "palw_lane_accept_parents_first", set: |params, at| params.palw_lane_accept_parents_first = at },
];

/// **testnet-12's post-launch fence height: DAA 750** (the user's decision of 2026-09-26, with the live
/// chain at DAA ~457): every entry of [`PALW_T12_POST_LAUNCH_FENCES_V1`] arms here. Not 500 (the height
/// first planned), and never 1,000 — `palw_bond_maturity`'s height, where a second fence would be
/// invisible to the fork id (`validate_palw_v2` refuses `palw_bond_maturity_early` there). A node
/// that has not upgraded keeps peering below this height and is refused by an upgraded one from it.
pub const PALW_T12_POST_LAUNCH_FENCE_DAA: u64 = 750;

/// **ADR-0160's capacity fences — the claim-capacity programme's own list, never the DAA-750 release's**
/// (rcore/cap-s1, stage 1 of the user's staged-safety plan of 2026-09-26): the stage-1 PROOF list, F-L at
/// ρ = 1 ([`crate::palw_aggregate_liability_v1::PALW_T12_CAPACITY_STEPS_V1`]) — what the stage gates and the
/// drill ([`crate::config::drill::palw_drill_capacity_fences_at_v1`]) arm. Every entry is a
/// testnet-12-only fence that ships DORMANT — `None` on every preset, testnet-12 included, so the
/// release's params id and schedule id do not move. The capacity FLAG DAY takes
/// [`PALW_T12_CAPACITY_RHO10_FENCES_V1`] instead (the same entries, F-L at the fixed ρ = 10), one entry at a
/// time into its own flag-day list. Kept apart from [`PALW_T12_POST_LAUNCH_FENCES_V1`] and
/// [`PALW_T12_POST_LAUNCH_FENCES_V2`] on purpose: those are armed on every testnet-12 ruleset (public and
/// drill), so an entry there would be ARMED on the live chain.
///
/// Each entry's `set` writes the field and every mirror of it the fold reads, exactly as the post-launch
/// entries do, so a test, a drill and the eventual release arm a fence in one place. The capacity fences'
/// prerequisites include post-launch fences (F-W needs `palw_reorg_strict_economic_win` and lane A's
/// `palw_operator_anchor` at or below it), so they are armed at or above [`PALW_T12_POST_LAUNCH_FENCE_DAA`]
/// on the ASSEMBLED ruleset.
pub const PALW_T12_CAPACITY_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    PALW_T12_CAPACITY_WEIGHT_CAP_V1,
    PALW_T12_CAPACITY_ESCROW_AT_LICENCE_V1,
    PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO1_V1,
    PALW_T12_CAPACITY_BATCH_LICENCE_V1,
    PALW_T12_CAPACITY_VERIFY_ROOM_V1,
    PALW_T12_CAPACITY_AUDIT_DOOR_V1,
    PALW_T12_CAPACITY_ISSUANCE_SLOTS_V1,
    PALW_T12_CAPACITY_NETWORK_ROOM_V1,
];

/// **ADR-0160 stage 3 (rcore/cap-s1): the ρ = 10 capacity flag day, entry by entry** — the safe ×10
/// architecture of stages 1, 2 and 4 with stage 3's FIXED ρ: J-1 (F-W), the escrow at licence (F-E), the
/// aggregate liability at ρ = 10, credited (F-L,
/// [`crate::palw_aggregate_liability_v1::PALW_T12_CAPACITY_STEPS_RHO10_V1`]), the batch licence and the
/// verify room (F-B, F-R), the audit door (F-Q), the issuance slots (F-S) and the network's fair share
/// (F-N). **Each is a named constant, armable by a one-entry addition to a future flag-day list** (every
/// entry at that list's height, through its own `set`, mirrors included). The eight arm TOGETHER:
/// `validate_palw_v2` refuses a list that leaves out an entry another one needs (F-L's credited step
/// needs F-Q, F-Q needs F-L and F-B, F-E needs F-W and F-L, F-S needs F-W, F-L and F-E, F-N needs F-R
/// and F-S). Nothing here is on any shipped list: testnet-12 ships every capacity fence dormant.
///
/// The ready ρ 25 / ρ 100 variants are later flag days' entries, each its own fence value
/// ([`PALW_T12_CAPACITY_RHO25_STEP_2_V1`], then [`PALW_T12_CAPACITY_RHO100_STEP_3_V1`]; or
/// [`PALW_T12_CAPACITY_RHO100_STEP_2_V1`] straight after ρ = 10); later stages' fences (riders, the
/// breaker) are theirs.
pub const PALW_T12_CAPACITY_RHO10_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    PALW_T12_CAPACITY_WEIGHT_CAP_V1,
    PALW_T12_CAPACITY_ESCROW_AT_LICENCE_V1,
    PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1,
    PALW_T12_CAPACITY_BATCH_LICENCE_V1,
    PALW_T12_CAPACITY_VERIFY_ROOM_V1,
    PALW_T12_CAPACITY_AUDIT_DOOR_V1,
    PALW_T12_CAPACITY_ISSUANCE_SLOTS_V1,
    PALW_T12_CAPACITY_NETWORK_ROOM_V1,
];

/// ADR-0160 F-W (lane cap-weight): staged claim weight, the per-bond weight cap (J-1) and the capped
/// reservation — the field and the V2 bundle's mirror the fold reads. Requires R-core+, strict-win (whose
/// shallow-tie rule it uses, with the no-DAA-lowering check) and lane A at or below it.
pub const PALW_T12_CAPACITY_WEIGHT_CAP_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_weight_cap",
    set: |params, at| {
        params.palw_capacity_weight_cap = at;
        params.sync_palw_capacity_weight_cap();
    },
};

/// ADR-0160 F-E (lane cap-escrow): the escrow funding point — past it the bond holds `m_c` in a claim's
/// escrow slot instead of `E` (m_c = E without a credit: at ρ = 1 the slot is today's), and an unconvicted
/// void keeps the obligation for h_obl. The field and the V2 bundle's mirror the fold reads.
pub const PALW_T12_CAPACITY_ESCROW_AT_LICENCE_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_escrow_at_licence",
    set: |params, at| {
        params.palw_capacity_escrow_at_licence = at;
        params.sync_palw_capacity_escrow();
    },
};

/// ADR-0160 F-L (lane cap-liab) as stage 1 proves it: aggregate bond liability on testnet-12's ρ = 1
/// schedule from the height (`PALW_T12_CAPACITY_STEPS_V1`: nothing credited) and the V2 bundle's mirror the
/// fold reads. Requires `palw_rcore_plus` and `palw_offence_attribution` at or below it (testnet-12 arms
/// both at genesis).
pub const PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO1_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability",
    set: |params, at| {
        params.palw_capacity_aggregate_liability = at.map(crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::t12_at_v1);
        params.sync_palw_capacity_liability();
    },
};

/// **ADR-0160 stage 3: F-L at the FIXED ρ = 10, credited** — the one entry the ρ = 10 flag day takes in
/// place of [`PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO1_V1`]. Moving it keeps a later flag day's appended
/// steps (a ready ρ 25 / ρ 100 entry): only the fence's own first step moves, and `validate_palw_v2`
/// refuses a move above one of them.
pub const PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability",
    set: |params, at| {
        let later: Vec<_> =
            params.palw_capacity_aggregate_liability.take().map(|old| old.steps.into_iter().skip(1).collect()).unwrap_or_default();
        params.palw_capacity_aggregate_liability = at.map(|at| {
            let mut value = crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::of_schedule_v1(
                at,
                crate::palw_aggregate_liability_v1::PALW_T12_CAPACITY_STEPS_RHO10_V1,
            );
            value.steps.extend(later);
            value
        });
        params.sync_palw_capacity_liability();
    },
};

/// ADR-0160 F-B (lane cap-verify): the batch licence (a licence list riding one object, a `Windowed`
/// kind-3 receipt) — a bare height with the fold's mirrors.
pub const PALW_T12_CAPACITY_BATCH_LICENCE_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_batch_licence",
    set: |params, at| {
        params.palw_capacity_batch_licence = at;
        params.sync_palw_capacity_verify();
    },
};

/// ADR-0160 F-R (lane cap-verify): room v2 (the measured k = 2 class room under ρ × the shipped room, the
/// stake-proportional share for T-2(a), the floor room J-6) — a bare height with the fold's mirrors.
pub const PALW_T12_CAPACITY_VERIFY_ROOM_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_verify_room",
    set: |params, at| {
        params.palw_capacity_verify_room = at;
        params.sync_palw_capacity_verify();
    },
};

/// ADR-0160 stage 2: F-Q the audit door — a bare height with the fold's mirror (which carries lane A's
/// operator bonds, its pool).
pub const PALW_T12_CAPACITY_AUDIT_DOOR_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_audit_door",
    set: |params, at| {
        params.palw_capacity_audit_door = at;
        params.sync_palw_capacity_stage2();
    },
};

/// ADR-0160 stage 2: F-S the issuance slots (with stage 4's burst: one DAA's refill) — a bare height with
/// the fold's mirror.
pub const PALW_T12_CAPACITY_ISSUANCE_SLOTS_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_issuance_slots",
    set: |params, at| {
        params.palw_capacity_issuance_slots = at;
        params.sync_palw_capacity_stage2();
    },
};

/// ADR-0160 stage 4: F-N the network level and the work-conserving fair share — a bare height with the
/// fold's mirror.
pub const PALW_T12_CAPACITY_NETWORK_ROOM_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_network_room",
    set: |params, at| {
        params.palw_capacity_network_room = at;
        params.sync_palw_capacity_stage2();
    },
};

/// **int-11: F-N's static verification term as a flag-day entry** (lane P's `L_ver`, `palw_capacity_network_verify`): the
/// network level capped by what the seats can verify, past its height. In no list and at no height on any preset — the
/// flag day that arms it names its height; a drill moves it with `--palw-drill-capacity-network-verify-at`.
pub const PALW_T12_CAPACITY_NETWORK_VERIFY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_network_verify",
    set: |params, at| {
        params.palw_capacity_network_verify = at;
        params.sync_palw_capacity_network_verify();
    },
};

/// The verification term's one-entry list (a flag-day list takes it in one line; the drill's flag moves it).
pub const PALW_T12_CAPACITY_NETWORK_VERIFY_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_CAPACITY_NETWORK_VERIFY_V1];

/// **Lane PL part C as a flag-day entry** (`palw_panel_unavailable_expiry`, ADR-0166). In no list and at no height on any preset — the
/// flag day that arms it names its height; a drill moves the whole panel-liveness list with
/// `--palw-drill-panel-liveness-at`.
pub const PALW_T12_PANEL_UNAVAILABLE_EXPIRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_panel_unavailable_expiry",
    set: |params, at| {
        params.palw_panel_unavailable_expiry = at;
        params.sync_palw_panel_unavailable_expiry();
    },
};

/// **Lane PL part D as a flag-day entry** (`palw_panel_fast_switch`, ADR-0166). In no list and at no height on any preset — the
/// flag day that arms it names its height; a drill moves the whole panel-liveness list with
/// `--palw-drill-panel-liveness-at`.
pub const PALW_T12_PANEL_FAST_SWITCH_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_panel_fast_switch",
    set: |params, at| {
        params.palw_panel_fast_switch = at;
        params.sync_palw_panel_fast_switch();
    },
};

/// **Lane PL part E as a flag-day entry** (`palw_seat_availability`, ADR-0166). In no list and at no height on any preset — the
/// flag day that arms it names its height; a drill moves the whole panel-liveness list with
/// `--palw-drill-panel-liveness-at`.
pub const PALW_T12_SEAT_AVAILABILITY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_seat_availability",
    set: |params, at| {
        params.palw_seat_availability = at;
        params.sync_palw_seat_availability();
    },
};

/// **Lane PL's flag-day list (ADR-0166): parts C, D and E in the order their prerequisites need.** Dormant on every preset: the
/// flag day that arms it (lane INT's DAA-5,300 list) names its height; a drill's `--palw-drill-panel-liveness-at` moves it.
pub const PALW_T12_PANEL_LIVENESS_FENCES_V1: &[PalwPostLaunchFenceV1] =
    &[PALW_T12_PANEL_UNAVAILABLE_EXPIRY_V1, PALW_T12_PANEL_FAST_SWITCH_V1, PALW_T12_SEAT_AVAILABILITY_V1];

/// **ADR-0160 stage 3: the ready ρ = 25 variant — F-L's second step** (`…_step_2`), at its own flag day's
/// height above the ρ = 10 flag day's. Its own fence value, never dynamic: the fork id names the step's
/// height, so a node that did not append it is refused from there.
pub const PALW_T12_CAPACITY_RHO25_STEP_2_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability_step_2",
    set: |params, at| palw_t12_capacity_step_set_v1(params, 2, at, 25),
};

/// **ADR-0160 stage 3: the ready ρ = 100 variant after ρ = 25 — F-L's third step** (`…_step_3`).
pub const PALW_T12_CAPACITY_RHO100_STEP_3_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability_step_3",
    set: |params, at| palw_t12_capacity_step_set_v1(params, 3, at, 100),
};

/// **ADR-0160 stage 3: the ready ρ = 100 variant straight after ρ = 10 — F-L's second step** (`…_step_2`).
pub const PALW_T12_CAPACITY_RHO100_STEP_2_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability_step_2",
    set: |params, at| palw_t12_capacity_step_set_v1(params, 2, at, 100),
};

/// **ADR-0164 stage 7: the ready ρ = 250 step — F-L's fourth step** (`…_step_4`; `k_aud` = 2 from here, ADR-0160 D-23). Armed after
/// ρ = 100's third; `validate_palw_v2` refuses it without F-EM, F-M1 and F-K at or below its height.
pub const PALW_T12_CAPACITY_RHO250_STEP_4_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability_step_4",
    set: |params, at| palw_t12_capacity_step_set_v1(params, 4, at, 250),
};

/// **ADR-0164 stage 7: the ready ρ = 1000 step — F-L's fifth step** (`…_step_5`), after ρ = 250. The per-bond ceiling of ADR-0160
/// §9.1's last row (2,030 concurrent claims on 13,000 MSK); the network's queue stays bounded by F-N's `L_ver` (ADR-0164 §9).
pub const PALW_T12_CAPACITY_RHO1000_STEP_5_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_aggregate_liability_step_5",
    set: |params, at| palw_t12_capacity_step_set_v1(params, 5, at, 1_000),
};

/// **ADR-0164 stage 5: F-EM, the per-DAA PALW reward budget** — a flag-day entry (`palw_capacity_emission_budget`): past its
/// height an attempt claim's acceptance charges its block's carve to a DAA-keyed ledger, 16 carves a DAA at most. The
/// field and the V2 bundle's mirror in one place.
pub const PALW_T12_CAPACITY_EMISSION_BUDGET_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_emission_budget",
    set: |params, at| {
        params.palw_capacity_emission_budget = at;
        params.sync_palw_capacity_s567();
    },
};

/// **ADR-0164 stage 5: F-M1, riders** (`palw_capacity_multi_claim`, object tag 95): a lead takes up to 64 riders paid out of its
/// own carve.
pub const PALW_T12_CAPACITY_MULTI_CLAIM_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_multi_claim",
    set: |params, at| {
        params.palw_capacity_multi_claim = at;
        params.sync_palw_capacity_s567();
    },
};

/// **ADR-0164 stage 6: F-K, the lower-only ρ breaker** (`palw_capacity_rho_breaker`).
pub const PALW_T12_CAPACITY_RHO_BREAKER_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capacity_rho_breaker",
    set: |params, at| {
        params.palw_capacity_rho_breaker = at;
        params.sync_palw_capacity_s567();
    },
};

/// A ready ρ variant's `set`: step `slot` of F-L at `at`, then the mirror. **Arming it on a ruleset whose
/// F-L does not carry the earlier steps panics** — the flag day that appends a step must come after the
/// ones it builds on, and a silently dropped step would leave a scheduled ρ unarmed; `None` (or
/// `never()`) always succeeds.
fn palw_t12_capacity_step_set_v1(params: &mut Params, slot: usize, at: Option<ForkActivation>, rho: u32) {
    let carried = params
        .palw_capacity_aggregate_liability
        .as_ref()
        .filter(|value| value.activation != ForkActivation::never())
        .map_or(0, |value| value.steps.len());
    if at.is_some_and(|at| at != ForkActivation::never()) && carried < slot - 1 {
        panic!(
            "palw_capacity_aggregate_liability_step_{slot} (ρ = {rho}) is armed on a ruleset whose F-L carries {carried} step(s): \
             arm the flag days it builds on first"
        );
    }
    crate::palw_aggregate_liability_v1::palw_capacity_step_append_v1(params.palw_capacity_aggregate_liability.as_mut(), slot, at, rho);
    params.sync_palw_capacity_liability();
}

/// **Arm (or, with `None`, disarm) every capacity fence at one height** — each entry of
/// [`PALW_T12_CAPACITY_FENCES_V1`] through its own `set`, on an ASSEMBLED testnet-12 ruleset, then
/// `validate_palw_v2`. What a capacity flag day will call; today only tests and the drill do (every
/// preset leaves the list dormant). `Err` names the refusal (a height below a prerequisite, say).
pub fn palw_t12_arm_capacity_fences_v1(params: &mut Params, at: Option<ForkActivation>) -> Result<(), String> {
    for fence in PALW_T12_CAPACITY_FENCES_V1 {
        (fence.set)(params, at);
    }
    params.validate_palw_v2().map_err(|e| format!("the capacity fences at {at:?} do not validate: {e}"))
}

/// **testnet-12 as shipped with every capacity fence armed at `at`** (`at ≥`
/// [`PALW_T12_POST_LAUNCH_FENCE_DAA`], the prerequisites' height) — the "armed" side of stage 1's twin
/// comparison, whose other side is [`palw_t12_shipped_params`] itself. Panics on a height the
/// validation refuses: a fixture, not a node path.
pub fn palw_t12_capacity_params_v1(at: u64) -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_capacity_fences_v1(&mut params, Some(ForkActivation::new(at))).unwrap_or_else(|e| panic!("{e}"));
    params
}

/// **The post-launch release, armed** — every entry of [`PALW_T12_POST_LAUNCH_FENCES_V1`] at
/// [`PALW_T12_POST_LAUNCH_FENCE_DAA`], each through its own `set` so every mirror follows (the V2
/// bundle's registry-resilience and V02 copies, lane A's operator list over the genesis registry).
/// Called on the ASSEMBLED ruleset in [`palw_t12_params_with_registry_v1`], after the assembly and
/// before its `validate_palw_v2` — the one body public testnet-12 and every drill share.
fn palw_t12_arm_post_launch_fences_v1(params: &mut Params) {
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(params, Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA)));
    }
}

/// **The fences testnet-12's SECOND post-launch flag day arms at one height** (the user's decision of
/// 2026-09-27: the seats saturate again past the DAA-750 release, so the retry that keeps a refused
/// draw's escrow alive ships in a flag day of its own) — the ONE list of them, the DAA-750 list's
/// successor. [`PALW_T12_POST_LAUNCH_FENCES_V1`] is the shipped DAA-750 set and never grows again; a
/// lane that puts a fix behind the next flag day adds its entry HERE, one `PalwPostLaunchFenceV1`
/// line (a bare height, or the field and its mirror through the field's own `sync_*`), and nothing
/// else moves:
///
/// * the release arms exactly these at [`PALW_T12_POST_LAUNCH_FENCE_V2_DAA`] in
///   [`palw_t12_params_with_registry_v1`], after the DAA-750 list and before `validate_palw_v2`
///   ([`palw_t12_arm_post_launch_fences_v2`]) — so on every drill ruleset too;
/// * a drill's `--palw-drill-fence2-at` moves exactly these to a low height on a salted drill chain
///   ([`crate::config::drill::palw_drill_post_launch_fences_v2_at_v1`]), so a drill already past its
///   DAA-750 crossing crosses this flag day with the shipping binary;
/// * [`palw_t12_launch_params_v1`] sets them back to dormant with the DAA-750 list (testnet-12 as it
///   launched), and [`palw_t12_release_v1_params`] sets back these alone (the DAA-750 release as the
///   fleet runs it — the baseline each of this list's lanes judges its own fence against).
///
/// Every entry is hashed Some-only with the `never()` collapse, so while the height is `None` the
/// list is dormant and every shipped id — testnet-12's `dbbc9104…` among them — is the DAA-750
/// release's, byte for byte.
///
/// Entries (lane · fence):
/// * lane F2 · `palw_floor_refusal_retry` — a stake draw refused for eligibility at every seed on
///   the anchor block's pre-object base re-anchors at the next slot instead of voiding.
/// * lane F2-lock · `palw_final_lock_life_retro` — at the crossing block every Valid seat lock of a
///   `Final` claim re-dates to `min(expiry, max(F + 1,000, H))`, and past it a `Final` dates its seat
///   locks at `F + 1,000` (its prerequisite `palw_final_lock_life`, DAA 750, is below this height).
pub const PALW_T12_POST_LAUNCH_FENCES_V2: &[PalwPostLaunchFenceV1] = &[
    // Lane F2 (the floor-refusal retry): the field and the V2 bundle's mirror the fold reads.
    PalwPostLaunchFenceV1 {
        name: "palw_floor_refusal_retry",
        set: |params, at| {
            params.palw_floor_refusal_retry = at;
            params.sync_palw_floor_refusal_retry();
        },
    },
    // Lane F2-lock (the retroactive seat-lock life): the field and the V2 bundle's mirror.
    PalwPostLaunchFenceV1 {
        name: "palw_final_lock_life_retro",
        set: |params, at| {
            params.palw_final_lock_life_retro = at;
            params.sync_palw_final_lock_life_retro();
        },
    },
];

/// **FP Job V4's flag-day entry** (RFC-0001 §A.4, lane fp-sampler, 2026-09-27): ADR-0082 D10 + D11
/// + "only V4 FP jobs from here", one fence (`palw_fp_decode_rules`) and its bundle mirror, set
/// through its own `set` like every entry of the post-launch lists. **Dormant** — it rides its own
/// flag-day list, [`PALW_T12_DECODE_RULES_FENCES_V1`], whose height is `None` (RFC-0003's FP Job V5
/// and RFC-0004's improvement fence both require it in force at or below them), so every shipped id is
/// unchanged. Arming it is the one line [`PALW_T12_DECODE_RULES_DAA`] and a re-pin of testnet-12
/// (`scripts/release/t12-repin.sh --shipping`). Its own prerequisite `palw_fp_derived_work` is armed at
/// testnet-12's genesis, so `validate_palw_v2` holds at any height.
pub const PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_fp_decode_rules",
    set: |params, at| {
        params.palw_fp_decode_rules = at;
        params.sync_palw_fp_decode_rules();
    },
};

/// **testnet-12's second post-launch flag day: DAA 1,300** (the user's decision of 2026-09-27, with the
/// live chain at DAA ~853: the seats' room runs out again at DAA ~1,350–1,575, so both fences of
/// [`PALW_T12_POST_LAUNCH_FENCES_V2`] arm before that). A height no other fence uses — not 750 and
/// not 1,000, where the fork id could not tell the flag days apart. A node on the DAA-750 release
/// keeps peering below this height and is refused by an upgraded one from it.
pub const PALW_T12_POST_LAUNCH_FENCE_V2_DAA: Option<u64> = Some(1_300);

/// **The second flag day, armed** — every entry of [`PALW_T12_POST_LAUNCH_FENCES_V2`] at
/// [`PALW_T12_POST_LAUNCH_FENCE_V2_DAA`] through its own `set`, on the ASSEMBLED ruleset, after the
/// DAA-750 list; a no-op while the height is `None`.
fn palw_t12_arm_post_launch_fences_v2(params: &mut Params) {
    let Some(at) = PALW_T12_POST_LAUNCH_FENCE_V2_DAA else { return };
    for fence in PALW_T12_POST_LAUNCH_FENCES_V2 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's THIRD post-launch flag day: the capacity architecture at a fixed ρ = 10** (ADR-0160 stages
/// 1, 2, 4 and stage 3's fixed ρ; the user's decision of 2026-09-27). Exactly
/// [`PALW_T12_CAPACITY_RHO10_FENCES_V1`] — all eight entries arm TOGETHER (`validate_palw_v2` refuses a list
/// that leaves out any but F-N, and F-N rides anyway: it is the fair-share property). ρ 25 / ρ 100 and the
/// FP Job V4 fence are later flag days' entries.
pub const PALW_T12_POST_LAUNCH_FENCES_V3: &[PalwPostLaunchFenceV1] = PALW_T12_CAPACITY_RHO10_FENCES_V1;

/// **testnet-12's third post-launch flag day: DAA 1,700** (the user's decision of 2026-09-28). It was 1,500
/// (2026-09-27, the live chain at DAA ~1,100), but that release was never rolled out: the fleet stayed on the
/// DAA-1,300 release and the chain passed 1,500 on its rules (DAA ~1,565 at 17:15 JST), so the package moves to
/// 1,700 (~22:55 JST at ~2.5 min/DAA). A height no other fence uses — not 750, 1,000 or 1,300, and not the
/// P0a list's (`PALW_T12_POST_LAUNCH_FENCE_V4_DAA`, still `None`).
pub const PALW_T12_POST_LAUNCH_FENCE_V3_DAA: Option<u64> = Some(1_700);

/// **The third flag day, armed** — every entry of [`PALW_T12_POST_LAUNCH_FENCES_V3`] at
/// [`PALW_T12_POST_LAUNCH_FENCE_V3_DAA`] through its own `set`, on the ASSEMBLED ruleset, after the DAA-750
/// and DAA-1,300 lists (F-W's prerequisites are the DAA-750 list's; F-Q reads lane A's operators).
fn palw_t12_arm_post_launch_fences_v3(params: &mut Params) {
    let Some(at) = PALW_T12_POST_LAUNCH_FENCE_V3_DAA else { return };
    for fence in PALW_T12_POST_LAUNCH_FENCES_V3 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's fourth post-launch flag day: the live GDN key-head fix (P0a)** —
/// `docs/design/palw/tir/phase-f-integration.md` §3.3, "an entry in the first flag-day list after DAA
/// 1,500 at an unused height". Past it a class may register a version-3 profile on the held composition
/// v5, which is what lets the fleet's Qwen3.6 artifact (16 key / 32 value heads) capture a recurrence
/// checkpoint at all: under graph-v7 every held hybrid attempt fails at prefill.
///
/// Entries (lane · fence):
/// * P0a · `palw_gdn_key_heads` — profile version 3 and the v5 map admitted; a V1/V2 profile whose
///   derived key-head count is not its value-head count refused on a recurrence map (hygiene).
pub const PALW_T12_POST_LAUNCH_FENCES_V4: &[PalwPostLaunchFenceV1] = &[PALW_GDN_KEY_HEADS_ENTRY_V1];

/// P0a's one entry: a bare height — the gate reads it at the registering block, and nothing is mirrored. Named so the int-12 flag
/// day's list ([`PALW_T12_INT11_FENCES_V1`]) arms the same entry the fourth list names.
pub const PALW_GDN_KEY_HEADS_ENTRY_V1: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_gdn_key_heads", set: |params, at| params.palw_gdn_key_heads = at };

/// **The fourth flag day's height — `None` until the user sets it** (Phase F §5 decision 9: "its
/// height"). While `None` the list is dormant on every ruleset, the shipped one included, and a drill
/// crosses it with `--palw-drill-fence4-at`. It must be a height no other fence uses (not 750, 1,000,
/// 1,300 or 1,500): the fork id names heights, not fences.
pub const PALW_T12_POST_LAUNCH_FENCE_V4_DAA: Option<u64> = None;

/// **The fourth flag day, armed** — every entry of [`PALW_T12_POST_LAUNCH_FENCES_V4`] at
/// [`PALW_T12_POST_LAUNCH_FENCE_V4_DAA`] through its own `set`, on the ASSEMBLED ruleset, after the
/// first three lists; a no-op while the height is `None`.
fn palw_t12_arm_post_launch_fences_v4(params: &mut Params) {
    let Some(at) = PALW_T12_POST_LAUNCH_FENCE_V4_DAA else { return };
    for fence in PALW_T12_POST_LAUNCH_FENCES_V4 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's IR flag day: RFC-0002 PALW-TIR v1** (the user's decision of 2026-09-28) — `palw_tir_v1`
/// alone: this build's primitive set and IR court version at testnet-12's measured IR ceilings
/// ([`crate::palw_tir_v1::PalwTirFenceV1::testnet12_v1`]: `max_cone_work` 2^16, 88,000 program bytes).
/// Its prerequisites are in force from genesis on testnet-12 (`validate_palw_tir_v1`: A-2 declared, the
/// k-ary court and R-core+ armed at or below it).
///
/// **The P0a / C5 fences stay DORMANT and off this list** (the user's direction: hybrids go the IR way).
/// Named apart from the P0a list's `PALW_T12_POST_LAUNCH_FENCES_V4` (dormant, not on this line), so the
/// two cannot collide when both lines are integrated.
pub const PALW_T12_TIR_FLAG_DAY_FENCES_V1: &[PalwPostLaunchFenceV1] = &[crate::palw_tir_v1::PALW_T12_TIR_V1_ENTRY];

/// **The IR flag day's height: DAA 2,000** (~11:10 JST 2026-09-29 at 2.43 min/DAA; the user's decision of
/// 2026-09-28). A height no other fence uses — not 750, 1,000, 1,300 or 1,700.
pub const PALW_T12_TIR_FLAG_DAY_DAA: Option<u64> = Some(2_000);

/// **The IR flag day, armed** — every entry of [`PALW_T12_TIR_FLAG_DAY_FENCES_V1`] at
/// [`PALW_T12_TIR_FLAG_DAY_DAA`] through its own `set` (which writes the bundle's mirror), on the ASSEMBLED
/// ruleset, after the third list; a no-op while the height is `None`.
fn palw_t12_arm_tir_flag_day_v1(params: &mut Params) {
    let Some(at) = PALW_T12_TIR_FLAG_DAY_DAA else { return };
    for fence in PALW_T12_TIR_FLAG_DAY_FENCES_V1 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's DAA-3,600 flag day** (the user's decision of 2026-10-01: one release, one drill, one
/// rollout — `rcore/int-10`; the court window stays dormant on testnet-12, the coordinator's decision of the
/// same day, so the list is `palw_tir_fence2` ALONE) — `palw_tir_fence2` ([`crate::palw_tir_fence2_v1`]: ref2's
/// H7 `TopK` row, the `Select`-arm work credit, the IR DA units `TirStepLeaf`, `TirStepNode` and
/// `TirRowNode`, the per-class DA ladder past the fence, the seat-reach (2^30) admission cap). Its
/// prerequisite is the IR flag day's `palw_tir_v1` at or below it (`validate_palw_tir_fence2`).
/// `--palw-drill-tir2-at` moves this list alone.
pub const PALW_T12_TIR_FENCE2_FENCES_V1: &[PalwPostLaunchFenceV1] = &[crate::palw_tir_fence2_v1::PALW_T12_TIR_FENCE2_ENTRY];

/// **The DAA-3,600 flag day's height** (~17:00 JST 2026-10-02 at 2.9 min/DAA; the user's decision of
/// 2026-10-01). A height no other fence uses — not 750, 1,000, 1,300, 1,700 or 2,000: the fork id names
/// heights, not fences. An activation height, not a maximum model window.
pub const PALW_T12_TIR_FENCE2_DAA: Option<u64> = Some(3_600);

/// **The DAA-3,600 flag day, armed** — every entry of [`PALW_T12_TIR_FENCE2_FENCES_V1`] at
/// [`PALW_T12_TIR_FENCE2_DAA`] through its own `set` (which writes the bundle's mirror), on the ASSEMBLED
/// ruleset, after the IR flag day; a no-op while the height is `None`.
fn palw_t12_arm_tir_fence2_v1(params: &mut Params) {
    let Some(at) = PALW_T12_TIR_FENCE2_DAA else { return };
    for fence in PALW_T12_TIR_FENCE2_FENCES_V1 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's decode-rules flag day — `palw_fp_decode_rules` alone** (ADR-0082 D10 seed/temperature
/// and D11 repeat penalty, FP Job V4 only from there; [`PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1`]). It
/// is the prerequisite RFC-0003's FP Job V5 and RFC-0004's improvement fence name — an evaluation's
/// generated text is a decode under these rules — so it arms at or below either. Its own list so the
/// release that arms it names one height for it; a drill crosses it with `--palw-drill-decode-rules-at`.
pub const PALW_T12_DECODE_RULES_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1];

/// **The decode-rules flag day's height — `None` until the user sets it.** While `None` the list is
/// dormant on every ruleset, the shipped one included, and every shipped id is unchanged. It must be a
/// height no other fence uses (not 750, 1,000, 1,300, 1,700, 2,000 or the second IR flag day's): the fork
/// id names heights, not fences.
pub const PALW_T12_DECODE_RULES_DAA: Option<u64> = None;

/// **The decode-rules flag day, armed** — every entry of [`PALW_T12_DECODE_RULES_FENCES_V1`] at
/// [`PALW_T12_DECODE_RULES_DAA`] through its own `set` (which writes the bundle's mirror), on the
/// ASSEMBLED ruleset, after the IR flag days; a no-op while the height is `None`.
fn palw_t12_arm_decode_rules_v1(params: &mut Params) {
    let Some(at) = PALW_T12_DECODE_RULES_DAA else { return };
    for fence in PALW_T12_DECODE_RULES_FENCES_V1 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **The model-specific finite court-window fence's entry** (`palw_model_court_window`, Codex's
/// `codex/t12-court-window`). After it, registration derives each fused-attention or IR class's court
/// window from its own court shape — the minimum complete dispute duration, with no shared DAA ceiling —
/// and stores it in `class_court_windows`; every class registered below it keeps the network window.
pub const PALW_T12_MODEL_COURT_WINDOW_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_model_court_window", set: |params, at| params.palw_model_court_window = at };

/// The per-model court window alone — what `--palw-drill-model-court-at` moves. **Armed on testnet-12 only by the int-13 flag day's list
/// ([`PALW_T12_INT13_FENCES_V1`], at [`PALW_T12_INT13_DAA`], which is `None` — the list waits for the full-activation release; this list's own
/// height [`PALW_T12_MODEL_COURT_WINDOW_DAA`] is `None`)**, and a no-op for every
/// verdict there: testnet-12 charges every class the held clock
/// (`palw_attn_court_admits_row_held_v1` — `held.armed` is the network's regime, from genesis), under which the
/// window the fence derives is the network window for every admissible class and no admission verdict moves
/// (`consensus/core/tests/palw_t12_court_window_changes_no_admission.rs`). The code, this list and the drill
/// flag stay for a ruleset that charges the ladder clock, and the baselines below clear it (through this list and through the int-13
/// list) so each keeps its pinned ids.
pub const PALW_T12_MODEL_COURT_WINDOW_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_MODEL_COURT_WINDOW_ENTRY];

/// **The model court window's OWN height — `None`** (the coordinator's decision of 2026-10-01: no flag day of its own). Testnet-12 arms the
/// window with the int-13 list at [`PALW_T12_INT13_DAA`] instead (the coordinator's brief of 2026-10-08). See
/// [`PALW_T12_MODEL_COURT_WINDOW_FENCES_V1`]. It must be a height no other fence uses if it is ever set.
pub const PALW_T12_MODEL_COURT_WINDOW_DAA: Option<u64> = None;

/// **The model court window, armed** — every entry of [`PALW_T12_MODEL_COURT_WINDOW_FENCES_V1`] at
/// [`PALW_T12_MODEL_COURT_WINDOW_DAA`]; a no-op while the height is `None`, which it is (the int-13 list arms the entry).
fn palw_t12_arm_model_court_window_v1(params: &mut Params) {
    let Some(at) = PALW_T12_MODEL_COURT_WINDOW_DAA else { return };
    for fence in PALW_T12_MODEL_COURT_WINDOW_FENCES_V1 {
        (fence.set)(params, Some(ForkActivation::new(at)));
    }
}

/// **testnet-12's int-11 flag day** (the user's decision of 2026-10-01, "100倍にして", and the coordinator's of 2026-10-02): RFC-0003's
/// generative lane, RFC-0004's model improvement and ADR-0160's capacity ramp to ρ = 25, at ONE height,
/// [`PALW_T12_INT11_FLAG_DAY_DAA`] (H), through each entry's own `set` (mirrors included) — the ONE list of what arms there, in
/// the order the entries' prerequisites need:
///
/// * `palw_fp_decode_rules` — ADR-0082 D10/D11, the sampler's seed, temperature and repeat penalty: the prerequisite of FP
///   Job V5 and of an evaluation's decode (spec 17 [A6-1]);
/// * `palw_gen_v1` — RFC-0003 at testnet-12's provisional ceilings ([`crate::palw_gen_v1::PALW_T12_GEN_CEILINGS_V1`]);
/// * `palw_fp_job_v5` — the free-prompt job's version-8..10 tail: V5, the tensor claim;
/// * `palw_held_close_chunks_v1` — the held leaf challenge (object tag 90);
/// * `palw_improvement_v1` — RFC-0004 at [`crate::palw_improve_v1::PALW_T12_IMPROVE_CEILINGS_V1`] (the pipeline-claim data
///   availability rides with it);
/// * `palw_capacity_network_verify` — F-N's static verification term, `L_ver` = 435 at the shipped 600 / 20 ("at or before"
///   ρ = 25's height: at it);
/// * `palw_capacity_aggregate_liability_step_2` — ρ = 25, F-L's second step, from H.
///
/// ρ = 100, F-L's third step, is its own entry at H + 95 ([`PALW_T12_INT11_RHO100_FENCES_V1`] at [`PALW_T12_INT11_RHO100_DAA`]).
/// **Not on this list, by decision:** `palw_model_court_window` (armed nowhere), `palw_model_virtual_v1` (ADR-0162, the user has not
/// decided), the P0a list. Lane F's class-seating fence joins the list when it lands.
///
/// * a drill's `--palw-drill-int11-at=<DAA>` moves exactly these to a low height H' (ρ = 100 to H' + 95) on a salted drill chain, so a
///   drill rehearses the real combined crossing besides the per-fence flags of rows 12–17 of `docs/design/palw/int-11-drill-flags.md`;
/// * every entry is hashed Some-only with the `never()` collapse, so while the height is `None` the list is dormant and every
///   shipped id is the DAA-3,600 release's ([`palw_t12_release_v5_params`]), byte for byte.
pub const PALW_T12_INT11_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1,
    crate::palw_gen_v1::PALW_DRILL_GEN_V1_ENTRY,
    crate::palw_fp_job_v5::PALW_DRILL_FP_V5_ENTRY,
    crate::palw_held_close_v1::PALW_HELD_CLOSE_CHUNKS_ENTRY_V1,
    crate::palw_improve_v1::PALW_DRILL_IMPROVE_V1_ENTRY,
    // ---- int-12 (2026-10-03): every RFC but 0005 rides the same height, in prerequisite order ----
    // RFC-0006: layer-sharded panels (needs palw_tir_v1 / palw_tir_fence2 from the DAA-3,600 list, palw_rcore_plus, palw_verification_v2).
    crate::palw_tir_shard_v1::PALW_DRILL_TIR_SHARD_ENTRY,
    // RFC-0007: Part I vertex, then Part II witness manifest and Part IV.1 audit mesh (both need the vertex), then IV.2 capped onboarding.
    crate::palw_vertex_v1::PALW_VERTEX_ENTRY_V1,
    crate::palw_mesh_v1::PALW_WITNESS_MANIFEST_ENTRY_V1,
    crate::palw_mesh_v1::PALW_AUDIT_MESH_ENTRY_V1,
    crate::palw_mesh_v1::PALW_CAPPED_ONBOARDING_ENTRY_V1,
    // RFC-0001: the decode constraint, its second subset (needs the first), the prefix-state receipt and its inheritance (needs the
    // receipt), the tokenizer-match rule, the adapter class listing (needs palw_improvement_v1).
    crate::palw_fp_constraint_job_v1::PALW_DRILL_FP_DECODE_CONSTRAINT_ENTRY,
    crate::palw_fp_constraint_v2::PALW_DRILL_FP_CONSTRAINT_V2_ENTRY,
    crate::palw_fp_prefix_v1::PALW_DRILL_FP_PREFIX_STATE_ENTRY,
    crate::palw_fp_prefix_v1::PALW_DRILL_FP_PREFIX_INHERIT_ENTRY,
    crate::palw_fp_tokenizer_v1::PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY,
    crate::palw_adapter_class_v1::PALW_DRILL_ADAPTER_CLASS_V1_ENTRY,
    // RFC-0002 rest: the class-seating fence and P0a's GDN key heads (`palw_tir_only_v1` is a MAINNET step and is NOT here).
    crate::palw_class_seating_fence_v1::PALW_CLASS_SEATING_ENTRY_V1,
    PALW_GDN_KEY_HEADS_ENTRY_V1,
    // Lane PL (ADR-0166, part of the Useful Work Transition): verifier silence stops slashing an honest producer; standby seats;
    // the availability factor in panel assignment.
    PALW_T12_PANEL_UNAVAILABLE_EXPIRY_V1,
    PALW_T12_PANEL_FAST_SWITCH_V1,
    PALW_T12_SEAT_AVAILABILITY_V1,
    // Lane RS (ADR-0165): the base floor becomes a reserve, real-model attempts advance the clock.
    crate::palw_real_share_v1::PALW_T12_FLOOR_RESERVE_ENTRY,
    crate::palw_real_share_v1::PALW_T12_REAL_CLOCK_TICK_ENTRY,
    // Lane anchor/window (ADR-0170, the user's decision of 2026-10-03): the seed anchor is a window — a merged admitted attempt anchors, the
    // anchor survives span boundaries, and the admission jury and the schedule seeding read the latest anchor of `S − 24 … S − 1`. Without
    // it the reserve above leaves 2–3 % of spans anchored and a Candidate's audit seats in 2.7 % of its periods. Needs the execution lane,
    // the economic-safety bundle, the admission jury and the registry, all in force from genesis on testnet-12.
    crate::palw_anchor_window_v1::PALW_T12_ANCHOR_WINDOW_ENTRY,
    PALW_T12_CAPACITY_NETWORK_VERIFY_V1,
    // ADR-0164 (the user's decision of 2026-10-03: the ×1000 capacity change ships in this release): stage 5's budget and riders and
    // stage 6's breaker arm at H with ρ = 25; the ρ = 250 and ρ = 1000 steps are their own entries at H + 190 and H + 285.
    PALW_T12_CAPACITY_EMISSION_BUDGET_V1,
    PALW_T12_CAPACITY_MULTI_CLAIM_V1,
    PALW_T12_CAPACITY_RHO_BREAKER_V1,
    PALW_T12_CAPACITY_RHO25_STEP_2_V1,
];

/// **The int-11 flag day's second height: ρ = 100** — F-L's third step, [`PALW_T12_INT11_RHO100_OFFSET_DAA`] (95) DAA after H (the
/// user's decision of 2026-10-01), in an entry of its own: the fork id names the step's height.
pub const PALW_T12_INT11_RHO100_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_CAPACITY_RHO100_STEP_3_V1];

/// **The int-11 flag day's third height: ρ = 250** — F-L's fourth step ([`PALW_T12_CAPACITY_RHO250_STEP_4_V1`], `k_aud` = 2 from it),
/// [`PALW_T12_INT11_RHO250_OFFSET_DAA`] (190) DAA after H (the user's decision of 2026-10-03), in an entry of its own: the fork id
/// names the step's height.
pub const PALW_T12_INT11_RHO250_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_CAPACITY_RHO250_STEP_4_V1];

/// **The int-11 flag day's fourth height: ρ = 1000** — F-L's fifth step ([`PALW_T12_CAPACITY_RHO1000_STEP_5_V1`]),
/// [`PALW_T12_INT11_RHO1000_OFFSET_DAA`] (285) DAA after H.
pub const PALW_T12_INT11_RHO1000_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_CAPACITY_RHO1000_STEP_5_V1];

/// ρ = 250 arms this many DAA after H (the user's decision of 2026-10-03: 5,490 at H = 5,300).
pub const PALW_T12_INT11_RHO250_OFFSET_DAA: u64 = 190;
/// ρ = 1000 arms this many DAA after H (5,585 at H = 5,300).
pub const PALW_T12_INT11_RHO1000_OFFSET_DAA: u64 = 285;

/// **The int-11 flag day's height H — DAA 5,300 (tentative: the user names the final one)**, ONE named constant that the preset, a
/// drill's `--palw-drill-int11-at` offsets and the deploy kit's copy (`contrib/t12-deploy-kit/fleet.env.example`, pinned to this
/// value by `t12_deploy_kit_constants`) share. A height no other fence uses — not 750, 1,000, 1,300, 1,700, 2,000 or 3,600: the fork id
/// names heights, not fences. `None` is the dormant release.
pub const PALW_T12_INT11_FLAG_DAY_DAA: Option<u64> = Some(5_300);

/// ρ = 100 arms this many DAA after H (the user's decision of 2026-10-01).
pub const PALW_T12_INT11_RHO100_OFFSET_DAA: u64 = 95;

/// **ρ = 100's height, H + 95** — `None` while H is.
pub const PALW_T12_INT11_RHO100_DAA: Option<u64> = match PALW_T12_INT11_FLAG_DAY_DAA {
    Some(at) => Some(at + PALW_T12_INT11_RHO100_OFFSET_DAA),
    None => None,
};

/// **ρ = 250's height, H + 190** — `None` while H is.
pub const PALW_T12_INT11_RHO250_DAA: Option<u64> = match PALW_T12_INT11_FLAG_DAY_DAA {
    Some(at) => Some(at + PALW_T12_INT11_RHO250_OFFSET_DAA),
    None => None,
};

/// **ρ = 1000's height, H + 285** — `None` while H is.
pub const PALW_T12_INT11_RHO1000_DAA: Option<u64> = match PALW_T12_INT11_FLAG_DAY_DAA {
    Some(at) => Some(at + PALW_T12_INT11_RHO1000_OFFSET_DAA),
    None => None,
};

/// **The int-11 flag day, armed** — every entry of [`PALW_T12_INT11_FENCES_V1`] at [`PALW_T12_INT11_FLAG_DAY_DAA`] and ρ = 100's entry at
/// [`PALW_T12_INT11_RHO100_DAA`], each through its own `set`, on the ASSEMBLED ruleset, after the DAA-3,600 flag day whose
/// `palw_tir_fence2` an evaluation pipeline needs below it; a no-op while H is `None`.
fn palw_t12_arm_int11_flag_day_v1(params: &mut Params) {
    palw_t12_arm_int11_flag_day_at_v1(params, PALW_T12_INT11_FLAG_DAY_DAA);
}

/// **The int-11 flag day at `at`** (`None`: dormant) — the list at `at`, ρ = 100 at `at + 95`; the one body the preset, a drill and the
/// tests share. Dormant is set in the reverse order, so a step is never the one a later step stands on — **and the int-13 list, which
/// stands on this one (`palw_gen_range_twin_v1` needs `palw_gen_v1`), is set back to dormant first** (arming leaves it alone).
pub fn palw_t12_arm_int11_flag_day_at_v1(params: &mut Params, at: Option<u64>) {
    match at {
        Some(at) => {
            for fence in PALW_T12_INT11_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at)));
            }
            for fence in PALW_T12_INT11_RHO100_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at.saturating_add(PALW_T12_INT11_RHO100_OFFSET_DAA))));
            }
            for fence in PALW_T12_INT11_RHO250_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at.saturating_add(PALW_T12_INT11_RHO250_OFFSET_DAA))));
            }
            for fence in PALW_T12_INT11_RHO1000_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at.saturating_add(PALW_T12_INT11_RHO1000_OFFSET_DAA))));
            }
        }
        None => {
            // **The lists that stand on this one go first** (the reverse order, one level up): the int-13 list's range twin needs this
            // list's `palw_gen_v1` at or below it, so a ruleset with this list dormant and that one armed does not validate. Setting
            // the int-11 list back to dormant therefore sets the int-13 list back with it — the int-10 baseline is both dormant.
            palw_t12_arm_int13_flag_day_at_v1(params, None);
            for fence in PALW_T12_INT11_RHO1000_FENCES_V1
                .iter()
                .chain(PALW_T12_INT11_RHO250_FENCES_V1)
                .chain(PALW_T12_INT11_RHO100_FENCES_V1)
                .chain(PALW_T12_INT11_FENCES_V1.iter().rev())
            {
                (fence.set)(params, None);
            }
        }
    }
}

/// **testnet-12's int-13 list — the code-change-only fences that wait on a height** (written for the coordinator's brief of 2026-10-08, a
/// DAA-9,000 flag day that the user then CANCELLED the same day: the list waits for the full-activation release) — the ONE list of what
/// arms at [`PALW_T12_INT13_DAA`] (`None`: dormant), through each entry's own `set`
/// (mirrors included), in the order the entries' prerequisites need. **Tier 1 only: every entry's prerequisites are already in force on
/// testnet-12 (genesis rules, or the int-11 list at 5,300 for the range twin), so the list is a code change and nothing else:**
///
/// * `palw_audit_1004_v1` — the 2026-10-04 audit's consensus fixes (needs the model registry and the economic-safety bundle);
/// * `palw_gen_range_twin_v1` — RFC-0003 PALW-GEN-20 by the range twin: the pipeline admission sizes a generative class's closes with
///   the range twin, the same read sets and bounds with far fewer steps of the same cap (needs `palw_gen_v1`, armed at 5,300);
/// * `palw_model_court_window` — the per-model finite court window, which on testnet-12 (the held clock) moves no admission verdict
///   (`palw_t12_court_window_changes_no_admission`); it names its own height so the ruleset states what it runs;
/// * `palw_receipt_spend_v4` — RFC-0009 stage C, public receipt redemption (needs `palw_audit_2026_09_11` and `palw_audit_2026_09_23`).
///
/// **Not on this list, by decision:** `palw_tir_only_v1` and `palw_model_virtual_v1` (the user has not decided) and the lanes that are
/// not ready (`palw_dns_retirement_v1`, `palw_exec_payload_v2`, `palw_permissionless_panel_v1`, `palw_probabilistic_constraints_v1`,
/// `palw_panel_free_v1`) — each joins by one line here plus the re-pin, when it passes its gates **before the code freeze**.
/// **Once the int-13 build is cut, this list is frozen** (C4 round 3, F-C4R3-04): a fence that joins DAA 9,000 after an int-13 binary
/// is deployed is invisible to the fork id (`fork_id_v1` fires it with the others at the same height), so the two builds would stay
/// peers past 9,000 while disagreeing about blocks. A later fence takes a fresh height on a new list.
///
/// * a drill's `--palw-drill-int13-at=<DAA>` moves exactly these to a low height on a salted drill chain;
/// * every entry is hashed Some-only with the `never()` collapse, so while the height is `None` the list is dormant and every shipped
///   id is the int-12 release's ([`palw_t12_release_v6_params`]), byte for byte.
pub const PALW_T12_INT13_FENCES_V1: &[PalwPostLaunchFenceV1] = &[
    crate::palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY,
    crate::palw_gen_range_twin_v1::PALW_DRILL_GEN_RANGE_TWIN_ENTRY,
    PALW_T12_MODEL_COURT_WINDOW_ENTRY,
    crate::palw_receipt_v4::PALW_T12_RECEIPT_SPEND_V4_ENTRY,
];

/// **The int-13 list's height — `None`** (it was DAA 9,000 in the coordinator's brief of 2026-10-08, until the user cancelled that flag day), ONE named constant that the preset, a drill's
/// `--palw-drill-int13-at` and the deploy kit's copy (`contrib/t12-deploy-kit/fleet.env.example`, pinned to this value by
/// `t12_deploy_kit_constants`) share. A height no other fence uses — not 750, 1,000, 1,300, 1,700, 2,000, 3,600, 5,300, 5,395, 5,490 or
/// 5,585: the fork id names heights, not fences. `None` is the dormant release.
///
/// **`None` — there is no DAA-9,000 flag day on testnet-12** (the user's decision of 2026-10-08, ~20:00). The four fences of this list wait
/// for ONE future release — the full-activation release, which also enables RFC-0008, RFC-0010, RFC-0011's K2 route, RFC-0012, RFC-0014 and
/// RFC-0015 once all RFC implementation is complete — and that release takes a fresh height then. Until it does, the shipped testnet-12
/// ruleset arms the list nowhere and every shipped id is the int-12 release's, byte for byte. The list, [`palw_t12_arm_int13_flag_day_at_v1`]
/// and the drill flag `--palw-drill-int13-at` stay: they are how the list is tested and how it will be scheduled.
pub const PALW_T12_INT13_DAA: Option<u64> = None;

/// **The int-13 flag day, armed** — every entry of [`PALW_T12_INT13_FENCES_V1`] at [`PALW_T12_INT13_DAA`], each through its own `set`,
/// on the ASSEMBLED ruleset, after the int-11 flag day whose `palw_gen_v1` the range twin needs below it; a no-op while the height is
/// `None`.
fn palw_t12_arm_int13_flag_day_v1(params: &mut Params) {
    palw_t12_arm_int13_flag_day_at_v1(params, PALW_T12_INT13_DAA);
}

/// **The int-13 flag day at `at`** (`None`: dormant) — the list at `at`; the one body the preset, a drill and the tests share. Dormant is
/// set in the reverse order, so an entry is never the one a later entry stands on.
pub fn palw_t12_arm_int13_flag_day_at_v1(params: &mut Params, at: Option<u64>) {
    match at {
        Some(at) => {
            for fence in PALW_T12_INT13_FENCES_V1 {
                (fence.set)(params, Some(ForkActivation::new(at)));
            }
        }
        None => {
            for fence in PALW_T12_INT13_FENCES_V1.iter().rev() {
                (fence.set)(params, None);
            }
        }
    }
}

/// **testnet-12 as its int-12 release (the DAA-5,300 flag day, `rcore/int-12`) ships it** — [`palw_t12_shipped_params`] with the int-13
/// flag day's list set back to dormant: the ruleset a node that has not taken the int-13 release runs (the fleet's today, params
/// `5ee7fd8e…`, schedule `1678e073…`), and the baseline the int-13 flag day is judged against (the fork-id comparison, the "only this
/// list moved it" twin).
pub fn palw_t12_release_v6_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    params
}

/// **testnet-12 as its DAA-3,600 release (int-10) ships it** — [`palw_t12_shipped_params`] with the int-11 flag day (its list and ρ = 100's
/// entry) and the int-13 flag day set back to dormant: the ruleset a node that has not taken the int-11 release runs, int-10's, and the
/// baseline the int-11 flag day is judged against (the fork-id comparison, the "only this list moved it" twin).
pub fn palw_t12_release_v5_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    params
}

/// **testnet-12 as its DAA-2,000 release (int-8, `4ca695b98`) ships it** — [`palw_t12_shipped_params`] with
/// the DAA-3,600 flag day's list (and the dormant model court window's and decode rules') set back to dormant: the ruleset a
/// node that has not taken the DAA-3,600 release runs (the fleet's int-8 ids, params `3db42ea6…`), and the
/// baseline the DAA-3,600 flag day is judged against (the fork-id comparison, the "only this fence moved it"
/// twin).
pub fn palw_t12_release_v4_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    for fence in PALW_T12_POST_LAUNCH_FENCES_V4
        .iter()
        .chain(PALW_T12_DECODE_RULES_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1)
    {
        (fence.set)(&mut params, None);
    }
    params
}

/// **testnet-12 as its DAA-1,700 release (int-7, `7aba8dd57`) ships it** — [`palw_t12_shipped_params`] with
/// the IR flag day's list (and the DAA-3,600 flag day's, which builds on it) set back to dormant: the ruleset a
/// node that has not taken the IR release runs (int-7's, params `770fb822…`, schedule `9410712f…`), and the
/// baseline the IR flag day is judged against.
pub fn palw_t12_release_v3_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    for fence in PALW_T12_DECODE_RULES_FENCES_V1
        .iter()
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1)
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
    {
        (fence.set)(&mut params, None);
    }
    params
}

/// **testnet-12 as its DAA-1,300 release (`587cab2b0`) ships it** — [`palw_t12_shipped_params`] with every
/// fence of [`PALW_T12_POST_LAUNCH_FENCES_V3`] (and the IR flag day's) set back to dormant: the ruleset a node
/// that has not taken the third flag day's release runs, and the baseline the capacity package is judged
/// against.
pub fn palw_t12_release_v2_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    for fence in PALW_T12_POST_LAUNCH_FENCES_V4
        .iter()
        .chain(PALW_T12_DECODE_RULES_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1)
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V3)
    {
        (fence.set)(&mut params, None);
    }
    params
}

/// **testnet-12 as its DAA-750 post-launch release ships it** — [`palw_t12_shipped_params`] with every
/// fence of [`PALW_T12_POST_LAUNCH_FENCES_V2`] set back to dormant through its own `set`: the
/// ruleset a node that has not taken the second flag day's release runs, and the baseline that
/// list's lanes arm ONE fence over. Equal to the shipped ruleset while the list's height is `None`.
pub fn palw_t12_release_v1_params() -> Params {
    let mut params = palw_t12_shipped_params();
    palw_t12_arm_int13_flag_day_at_v1(&mut params, None);
    palw_t12_arm_int11_flag_day_at_v1(&mut params, None);
    for fence in PALW_T12_POST_LAUNCH_FENCES_V4
        .iter()
        .chain(PALW_T12_DECODE_RULES_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(PALW_T12_MODEL_COURT_WINDOW_FENCES_V1)
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V3)
        .chain(PALW_T12_POST_LAUNCH_FENCES_V2)
    {
        (fence.set)(&mut params, None);
    }
    params
}

/// **testnet-12's assembly over a given genesis registry** — the one body [`palw_t12_shipped_params`]
/// and [`palw_t12_drill_params_v1`] share, so a drill cannot run a ruleset the network does not.
fn palw_t12_params_with_registry_v1(
    base: Params,
    bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
    genesis_utxos: crate::utxo::utxo_collection::UtxoCollection,
) -> Params {
    let params = palw_v2_params_with_class_rows_v1(
        base,
        PALW_RC_GENESIS_ARTIFACT_ROOT,
        bonds,
        genesis_utxos,
        PalwGenesisClassRowsV1::Held(&PALW_T12_GENESIS_HELD_ROWS),
    )
    // A card that is set and does not assemble is a binary that would boot a network its own genesis
    // gate refuses; failing at startup with the gate's own message is the only honest outcome.
    .unwrap_or_else(|e| panic!("the testnet-12 genesis card does not assemble: {e}"));
    // **A card certifies the free-prompt lane of the classes it REGISTERS** — the mainnet rule
    // (`mainnet_certify_registered_classes_v1`), applied to this card too (2026-09-23). Without it the
    // shared assembly installed testnet-11's set, which names the graph-v3 hybrid and the graph-v2
    // dense rows this card does not register and omits the two held rows it does: the genesis hybrid
    // and dense classes had no free-prompt lane at all. testnet-11 keeps its set because its bundle
    // is a live identity; this one is minted fresh, so it derives the set from its own objects —
    // which is only non-empty for the held rows because the families that cover them
    // (`PALW-QWEN25-A16-V5`, `PALW-QWEN36-V6`) are drilled on both lanes.
    let mut params = mainnet_certify_registered_classes_v1(params);
    // **The post-launch release** (int-4, the user's decision of 2026-09-26): every fence of
    // `PALW_T12_POST_LAUNCH_FENCES_V1` at DAA 750, on the assembled ruleset (lane A's operators are
    // the registry this body just installed) and before the validation below, which holds their
    // prerequisites among themselves (lane A over F1's seed, bind-deadlock over lane A).
    palw_t12_arm_post_launch_fences_v1(&mut params);
    // **The second post-launch flag day** (lane F2 and its successors, 2026-09-27): every fence of
    // `PALW_T12_POST_LAUNCH_FENCES_V2` at `PALW_T12_POST_LAUNCH_FENCE_V2_DAA` — dormant while that height
    // is `None` — on the same assembled ruleset, after the DAA-750 list whose fences it builds on.
    palw_t12_arm_post_launch_fences_v2(&mut params);
    // **The third post-launch flag day** (the capacity architecture at ρ = 10, 2026-09-27): every fence of
    // `PALW_T12_POST_LAUNCH_FENCES_V3` at `PALW_T12_POST_LAUNCH_FENCE_V3_DAA`, after the first two lists.
    palw_t12_arm_post_launch_fences_v3(&mut params);
    // **The IR flag day** (RFC-0002 PALW-TIR v1, 2026-09-28): `palw_tir_v1` at `PALW_T12_TIR_FLAG_DAY_DAA`,
    // after the third list, on the same assembled ruleset (its prerequisites are genesis rules here).
    palw_t12_arm_tir_flag_day_v1(&mut params);
    // **The DAA-3,600 flag day** (`rcore/int-10`, 2026-10-01): `palw_tir_fence2` (RFC-0002 Phase F's second IR
    // fence) ALONE at `PALW_T12_TIR_FENCE2_DAA`, after the IR flag day whose `palw_tir_v1` it needs below it.
    palw_t12_arm_tir_fence2_v1(&mut params);
    // The decode-rules flag day (ADR-0082 D10/D11; RFC-0003/0004's prerequisite): dormant while its height is `None`.
    palw_t12_arm_decode_rules_v1(&mut params);
    // **The model court window is armed nowhere** (`PALW_T12_MODEL_COURT_WINDOW_DAA` is `None`; the coordinator's
    // decision of 2026-10-01): testnet-12 charges the held clock, under which the fence moves no verdict.
    palw_t12_arm_model_court_window_v1(&mut params);
    // **The int-11 flag day** (RFC-0003, RFC-0004 and the capacity ramp to ρ = 25 / ρ = 100, 2026-10-02): `PALW_T12_INT11_FENCES_V1` at
    // `PALW_T12_INT11_FLAG_DAY_DAA` and ρ = 100 at H + 95, after the DAA-3,600 flag day whose `palw_tir_fence2` it needs below it.
    palw_t12_arm_int11_flag_day_v1(&mut params);
    // **The fourth post-launch flag day** (P0a, the live GDN key-head fix): every fence of
    // `PALW_T12_POST_LAUNCH_FENCES_V4` at `PALW_T12_POST_LAUNCH_FENCE_V4_DAA` — dormant while that height
    // is `None` — after the first three lists and every other flag day's.
    palw_t12_arm_post_launch_fences_v4(&mut params);
    // **The int-13 flag day** (the coordinator's brief of 2026-10-08): `PALW_T12_INT13_FENCES_V1` — the code-change-only fences, tier 1 — at
    // `PALW_T12_INT13_DAA`, after the int-11 flag day whose `palw_gen_v1` the range twin needs below it; dormant while the height is `None`.
    palw_t12_arm_int13_flag_day_v1(&mut params);
    // **`palw_rc_arm_phase1` is NOT called here, and that is deliberate.** Its whole body is
    // "arm this if the preset left it dormant", and `palw_t12_arm_every_rule_from_genesis` has
    // already armed everything — so routing through it could only either change nothing or
    // overwrite a decision this preset made. The two locks it exists to assert are asserted below
    // over the assembled ruleset, which is the network actually being shipped.
    if let Err(e) = params.validate_palw_v2() {
        panic!("the testnet-12 ruleset does not validate: {e}");
    }
    params
}

/// **Devnet as it ships on this branch: the ADR-0068 Phase 1 drill network — `ConsensusV2`
/// with a ZERO-row genesis bond registry.**
///
/// The same shape `palw_rc_shipped_params` and `mainnet_shipped_params` have — a node's ruleset
/// is a property of its BINARY — assembled over the devnet base. Two deliberate differences from
/// the RC card:
///
/// * **Zero genesis bonds (ADR-0061).** A registry may be empty: bonds ride ordinary
///   transactions, and the heartbeat lane (armed on this preset from genesis) produces the blocks
///   they ride. A fresh devnet therefore boots as a heartbeat-only chain, a newcomer registers a
///   bond ON that chain (`--palw-register-bond`), and only then does the bonded attempt lane
///   start — which is exactly the bootstrap story the Phase 1 drill exists to prove on real
///   processes. No premine is carved (no collateral outputs, no fee floats), so the devnet
///   genesis block — hash, UTXO commitment, everything — is byte-identical to the base's and no
///   re-mint rides this change.
/// * **The floor's artifact root is the SAME derived BASE-0 root the RC pins**
///   ([`PALW_RC_GENESIS_ARTIFACT_ROOT`]): the root is a function of the BASE-0 specification
///   (every weight byte derives from one pinned seed), not of the network, so devnet re-uses the
///   derivation rather than pinning a second copy of the same value.
///
/// The assembly imposes the frozen 120 s cadence and re-derives finality/pruning depths
/// (`palw_v2_params_on_base`), runs the genesis gate (`verify_palw_genesis_v2` — which since
/// ADR-0061 admits the empty registry), and `validate_palw_v2` proves the armed heartbeat and
/// attempt-work fences declare exactly the constants this binary enforces.
///
/// **The class set is testnet-11's** (ADR-0082 devnet drill, 2026-09-03): the floor, the QWEN36
/// hybrid and the graph-v5@512 dense row, through the same assembly `palw_rc_shipped_params` uses,
/// from the same pinned roots. It was the floor alone, which made the dense row a POST-genesis
/// registration on devnet — and a class registered mid-epoch is not budgeted until the next
/// boundary (`EPOCH_LENGTH` 1,000 DAA, ~10 h at this host's cadence), so every drill stopped at
/// the commitment with "this class's epoch budget is already spent" and nothing past stage 5 was
/// ever rehearsed. Worse, the drill was then asking a network the cut chain is not: on a
/// floor-only devnet the row's admission went through the post-genesis gate under devnet's armed
/// `palw_context_ladder`, while on t11 the same row is a genesis object and post-genesis
/// registrations meet a dormant ladder — the difference that hid `PricedForADifferentCourt`
/// (97bdbf75) from every green drill. With the set mirrored, the drill rehearses the chain we
/// ship, and the class is budgeted from DAA 0.
///
/// The stated loss: no drill anywhere now exercises post-genesis registration END TO END on a
/// live chain (panel → chain → certification → budget → production); the admission DECISION is
/// covered by `palw_class_admission_v2`'s route-agreement test, `tests/palw_rc_admission_shape.rs`
/// and the SDK's preflight tests. This moves the devnet fingerprint (a named move; the t11 one
/// does not change — this function never touches the RC preset). The devnet genesis BLOCK is
/// unchanged: the class objects live in the ruleset bundle, not in the block.
pub fn devnet_shipped_params() -> Params {
    let dense = palw_rc_qwen25_a16_is_registered().then_some(PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT);
    if palw_rc_qwen36_is_registered() {
        palw_v2_params_with_classes_on_base(
            DEVNET_PARAMS,
            PALW_RC_GENESIS_ARTIFACT_ROOT,
            PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
            dense,
            palw_devnet_genesis_bonds_v1(),
        )
        .unwrap_or_else(|e| panic!("the devnet drill ruleset (testnet-11's class set on the devnet base) does not assemble: {e}"))
    } else {
        palw_v2_params_from_artifacts_on_base(DEVNET_PARAMS, PALW_RC_GENESIS_ARTIFACT_ROOT, palw_devnet_genesis_bonds_v1())
            .unwrap_or_else(|e| panic!("the devnet ADR-0068 drill ruleset does not assemble: {e}"))
    }
}

/// **Devnet's genesis bond registry, from public seeds** (`palw_devnet_genesis_bond_seed_v1`):
/// six seats, one per seed, each with its own operator identity and a fee float at its own
/// address, so a developer can run a producing, seating PALW devnet from genesis — the rehearsal
/// chain ADR-0075 §7 asks for before any certification object goes near mainnet. Value-less:
/// the seeds are in the source.
pub fn palw_devnet_genesis_bonds_v1() -> Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1> {
    crate::config::premine::palw_devnet_genesis_bond_keys_v1()
        .iter()
        .map(|(n, payload, vk)| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint(*n)),
            pubkey: vk.clone(),
            operator_pubkey: format!("misaka-devnet-operator-{n}").into_bytes(),
            payout_payload: crate::Hash64::from_bytes(*payload),
        })
        .collect()
}

/// **The PALW-RC network's identity WITHOUT a ruleset bundle — what `NetworkId` testnet-12 maps
/// to, and the base every bundled variant is built from.**
///
/// `Params::from(NetworkId)` used to `panic!` on every testnet suffix but 10 and 11, and
/// testnet-12 is the suffix `palw_rc_params` stamps into its own `net`. So the RC network could
/// be assembled in a test and could not be constructed by a node: `kaspad` builds its params with
/// `let params: Params = network.into()`, and every tool beside it — the wallet, the CLI, the
/// validator — does the same. A network whose identity aborts the process that names it is not a
/// network anyone can run.
///
/// What this returns is the RC IDENTITY: the RC genesis, the frozen 120 s cadence, no V1 PALW
/// proof-of-work, no inherited seeders. What it deliberately does NOT carry is the `ConsensusV2`
/// bundle, because a bundle is a function of genesis ARTIFACTS (the BASE-0 artifact root, the
/// catalog preimage, the genesis bond and its keys) that no `From<NetworkId>` impl holds. That is
/// also why every shipped preset stays bundle-free, which
/// `assembling_an_identity_installs_no_fence_on_any_shipped_preset` asserts.
///
/// **A node on this base is not on the RC ruleset, and it finds out at the handshake rather than
/// at consensus.** The bundle is inside `consensus_params_id`, so an artifact-less node
/// fingerprints differently from a bundled one and the two refuse each other — the fail-safe
/// direction. `kaspad` also says so at startup rather than leaving the operator to infer it from
/// a peerless node.
pub fn palw_rc_base_params() -> Params {
    let mut params = TESTNET_PARAMS.clone();
    // **testnet-11, and the suffix stops climbing here.**
    //
    // 12 and 13 were minted for internal relaunches and neither was ever a network anyone could
    // join; a suffix that increments per rebuild is a changelog wearing a network's name. Nothing
    // was running on 11 when this moved (measured across all four hosts), and `main_address_for`
    // already maps 11 and 12 to the same public main wallet — so the premine, and therefore the
    // genesis, is unchanged by the move. What changes is the name the handshake carries.
    params.net = NetworkId::with_suffix(NetworkType::Testnet, 11);
    params.genesis = crate::config::genesis::PALW_RC_GENESIS;
    // **The names are delegated, so they can be shipped.**
    //
    // This was empty on the reasoning that a new network's discovery names have to be names
    // somebody owns and delegates — an operational step, not a code one. They are: the
    // `misakascan.com` zone delegates `seederN.misakascan.com` NS to `ns-seederN.misakascan.com`,
    // each an A record for a host running `misaka-dnsseeder`. Only two of the four are ours to
    // reconfigure (seeder1 on 160.16.131.119, seeder3 on 95.111.236.186); the other two answer
    // from hosts this fleet does not administer, and a seeder still pointed at another network
    // returns nothing rather than returning wrong peers, which is the failure direction to want.
    //
    // **A seeder that answers nothing looks exactly like a name that does not resolve.** That is
    // what made this hard to read: `dig seeder1.misakascan.com A` was empty for a delegated name
    // whose nameserver was up, because the seeder had no node to read peers from. Test a seeder by
    // querying its delegated nameserver directly, never by resolving the name and inferring.
    //
    // `dns_seeders` is deliberately outside `consensus_params_id` — where to find peers is not a
    // rule about blocks — so this is a plain edit and not a flag day, and a node that reaches none
    // of them still bootstraps with `--addpeer`.
    params.dns_seeders = &["seeder1.misakascan.com", "seeder2.misakascan.com", "seeder3.misakascan.com", "seeder4.misakascan.com"];
    // A `ConsensusV2` network activates no V1 PALW proof-of-work and installs no V1 fence —
    // `validate_palw_v2` refuses both, and that refusal is why this is a new identity.
    params.pow_blake2b_sha3_activation = ForkActivation::never();
    params.pow_palw_activation = ForkActivation::never();
    params.pow_palw_ollama_activation = ForkActivation::never();
    // **The two genesis fences the 5f cut arms, STATED on the preset** so the card's table is the
    // operator's map of what this network arms — a fence armed only by a derivation somewhere
    // else would make that table false, and would be an arming nobody decided (it would fire the
    // day an unrelated constant was filled in). `always()` is the only legal value for each (a
    // chain that acquired either rule mid-life would be two rulesets wearing one id).
    //
    // * `palw_uncertified_weightless` — ADR-0069 Decision 7: READ on the production path (the
    //   virtual processor resolves it per point and hands it to the fold; the sync walker carries
    //   the same fence), and genesis is its only opportunity (`validate_palw_v2` refuses it above
    //   genesis). Dormant, an uncertified class's pwu enters safe weight.
    // * `palw_kary_court` — ADR-0082 Decision 3: the class assembly below DERIVES that a genesis
    //   set registering a fused row needs the dissection court and refuses a preset that
    //   disagrees — belt and braces: the preset says it, the derivation enforces it.
    //
    // NOT `palw_context_ladder` — and no longer for the reason this comment used to give ("no
    // accessor reads that fence to gate anything"). It gates two things now: the class ladder
    // rules at post-genesis admission (`palw_class_ladder_rules_for_court_v1`) and, since the
    // 2026-09-05 mainnet audit, the leaf ladder the court's refutation walkers open against
    // (`palw_refutation_leaf_cap_v2`, ADR-0084 U-08). Dormant here, the 512 row this genesis
    // registers is admitted at `2^26` and prosecuted at `2^22`, i.e. not at all. It stays dormant
    // on this preset because testnet-11 is a LIVE chain with closes judged under the constant:
    // arming it is a scheduled height (ADR-0083's path (a)) or a re-mint, and either is the
    // operator's call. A carded mainnet and devnet arm it from genesis.
    params.palw_uncertified_weightless = Some(ForkActivation::always());
    params.palw_kary_court = Some(ForkActivation::always());
    // **ADR-0083 Decision 1, SCHEDULED on the live chain — testnet-11 Relaunch 5f's flag day.**
    //
    // Measured 2026-09-04 (5f card §10b/§10e): five heartbeat emitters on the two-minute cadence
    // tightened `bits` from p = 0.5 to p = 1.5e-3 in 826 blocks and no attempt lane could win.
    // The operator chose path (a) of the ADR's §5: a fence at a future height rather than a
    // re-mint. Genesis is unchanged; `consensus_params_id` moves (the Some-only write below), the
    // fence-normalised identity does not, so a seat on the previous build peers with a warning
    // until the height and forks off at it — every seat restarts on this build BEFORE DAA 1150.
    // Chosen at DAA 894 (02:50Z) with ~93 DAA/h: ~2.7 h of lead over a 15-minute build and a
    // six-seat restart. At the height the window holds no priced row (last attempt block at DAA
    // 226), so the first fenced block carries `max_difficulty_target`.
    params.palw_difficulty_priced_rows = Some(ForkActivation::new(1150));
    // **The second flag day: the data-availability court, private prompts, and the model market —
    // SCHEDULED, on the live chain** (2026-09-06; ADR-0062, ADR-0077 D16, ADR-0087/0088/0089/0090).
    //
    // ADR-0083's path (a) again, for the same reason: testnet-11 carries value and a history, so a
    // rule that is not in force today arrives at a height rather than at a re-genesis. What each
    // fence turns on at [`PALW_RC_DA_COURT_FENCE_DAA`]:
    //
    // * `palw_da_court` — ADR-0062's accusation/disclosure court, arm-able now that a seat answers
    //   accusations and a disclosure is verified against the roots a claim actually carries. Two
    //   values ride with it: a retiring bond stays locked for the court's own windows
    //   (`palw_v2_bond_withdrawal_delay_at_v1`), and the pruning horizon widens to hold the phase
    //   (`palw_v2_pruning_depth_v1`) — the horizon is normalised out of the identity while the
    //   fence is future, which is what lets the fleet roll one host at a time.
    // * `palw_panel_da` — ADR-0077 Decision 16's private prompt: a commitment may carry no prompt
    //   on chain, and the ids reach only the drawn seats over the authenticated pull.
    // * `palw_model_market`, `palw_model_lines`, `palw_model_evm` — ADR-0087/0088/0089's model
    //   positions, the model registry, and the market's EVM window and hand (ADR-0090 §7 item 6's
    //   release, which says exactly these three lines).
    //
    // `palw_prompt_ids_merkle` is deliberately NOT here: the prompt-commitment form is genesis-only
    // (`validate_palw_v2` refuses any other height), because no reader of a `prompt_token_ids_hash`
    // holds a job's anchor height. testnet-11 keeps the flat digest; a card mints with the Merkle one.
    params.palw_da_court = Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA));
    params.palw_panel_da = Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA));
    params.palw_model_market = Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA));
    params.palw_model_lines = Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA));
    params.palw_model_benefits = Some(ForkActivation::new(PALW_RC_MODEL_BENEFITS_FENCE_DAA));
    params.palw_model_leg_v2 = Some(ForkActivation::new(PALW_RC_MODEL_LEG_V2_FENCE_DAA));
    params.palw_model_seed_v2 = Some(ForkActivation::new(PALW_RC_MODEL_SEED_V2_FENCE_DAA));
    // **The 2026-09-11 audit flag day.** The audit's state-transition and acceptance-filter fixes
    // (A-1, A-2, B-5, AC-SLOT, and the deep court/panel findings) activate together on testnet-11
    // at one height, behind this ONE fence. The share-census fence (`palw_share_growth_final`,
    // B-2/ADR-0107) and the dense-court fences (`palw_fused_dissectable` / `palw_attn_anchored_root`,
    // AC-D8) are deliberately NOT armed here: they are mainnet-card corrections, dormant on
    // testnet-11 "until a flag day says otherwise" (`the_share_growth_final_fence_is_dormant_everywhere`,
    // and the ADR-0107 note that the merged-payout question is still the operator's to make).
    params.palw_audit_2026_09_11 = Some(ForkActivation::new(PALW_RC_AUDIT_FENCE_DAA));
    // **ADR-0117's one-forward draw rides the same flag day** (operator, 2026-09-11: one rollout for
    // both). From 4,000 an attempt's job is its class's canonical job without the decode calls. The
    // shared height has a price, named on `PALW_RC_AUDIT_FENCE_DAA`: the two ship only together.
    params.palw_prefill_draw = Some(ForkActivation::new(PALW_RC_AUDIT_FENCE_DAA));
    // **The held regime (ADR-0103) at its flag day, DAA 6,000** (ADR-0118; the operator's height,
    // shared with the deep-audit fence). Past it no bisection opens, a held class may register,
    // and the one-move court and the retention pins are in force.
    if let Some(daa) = PALW_RC_HELD_FENCE_DAA {
        palw_arm_held_regime_at_v1(&mut params, daa);
    }
    // **The 2026-09-11 audit's THIRD flag day** — the deep findings (B-1/C-01/B-4/court cluster) at
    // their own later height, distinct from the shallow fence so a deep build and a shallow build
    // diverge visibly at this height rather than silently at 4,000.
    params.palw_audit_2026_09_11_deep = Some(ForkActivation::new(PALW_RC_AUDIT_DEEP_FENCE_DAA));
    // **ADR-0143 is NOT armed, by the adversarial audit of 2026-09-19.** The rule and its index ship
    // here and do nothing, because the audit found two defects in the fence as written and both are
    // in the index it installs:
    //
    // * `artifact_owners` is a state-rooted map with no cap, no eviction and no TTL, while every
    //   neighbouring registry table has one. One `ModelVersionPublished` per row, at a transaction
    //   fee, buys a row every node re-hashes on every block and every new node downloads with the
    //   pruning carriage, for ever. The row of a version the 64-row history has already evicted
    //   outlives the version.
    // * uniqueness was written GLOBAL, across classes. That is wrong on its face: `class_id` is
    //   `PalwShapeProfile::shape_profile_id()`, which hashes `n_ctx`, `n_batch`, `n_ubatch` and the
    //   runtime flags, so one artifact legitimately carries several classes — `@512` and `@2048` are
    //   two class ids over one root. Global keying refuses the second, and lets a stranger reserve a
    //   public model's root for one line founding before its owner ever registers.
    //
    // The remedy is a `(class_id, root)` key and a bound on the rows, and it needs its own drill
    // crossing its own fence (the launch runbook's §5c). Until then the constant stands, unused, so
    // the height is not silently re-decided when it is armed.
    // params.palw_artifact_root_ownership = Some(ForkActivation::new(PALW_RC_ARTIFACT_ROOT_OWNERSHIP_FENCE_DAA));
    // **The 6,001 flag day** ([`PALW_RC_PALW_UPGRADE_FENCE_DAA`]), one release for all of it:
    //
    // * ADR-0124 — a panel is paid 20 % of its claim's escrow, a drawn seat holds three times the
    //   claim's exposure, and a claim is paid for the compute it certifies;
    // * ADR-0125 — the execution lane at one permit a round, one execution block a second, no widening
    //   scheduled (the next stage is its own height); with ADR-0130's scheduler (one parity an
    //   operator, participants fixed a span before a future anchor seeds them) and spans of 5 DAA, ten
    //   minutes at 120 s, so a span's producers are public ten minutes ahead rather than an hour.
    //   Past [`PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA`] (7,300) the same scheduler runs at 1 DAA a
    //   span — two future spans, ~4 min — without rewriting the 7,101 folds;
    // * ADR-0126 (revised) — the validator pool falls from 30 % to 20 % of a block's subsidy and the
    //   tenth is escrowed for the block's PALW claim (72 % instead of 62 %);
    // * ADR-0128 — validators vote BFT by bonded stake and the DNS-final anchor decides the stake reorg
    //   gate; a bond silent for 5,040 DAA (seven days at 120 s) leaks, re-entry waits for an attestation
    //   buried 200 DAA, and no leak takes the set below four validators.
    //
    // ADR-0127 and ADR-0129 add reads and pins, no rule.
    let flag_day_6001 = ForkActivation::new(PALW_RC_PALW_UPGRADE_FENCE_DAA);
    params.palw_panel_economy = Some(flag_day_6001);
    params.palw_work_priced_reward = Some(flag_day_6001);
    params.palw_execution_lane = Some(PalwExecutionLaneV1 {
        activation: flag_day_6001,
        permits_per_round: 1,
        widenings: PalwExecutionLaneV1::NO_WIDENINGS,
        max_per_mergeset: 600,
        schedule_span_daa: 5,
        short_span: PalwExecSpanShortV1 {
            activation: ForkActivation::new(PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA),
            schedule_span_daa: 1,
        },
    });
    params.palw_overlay_carve = Some(PalwOverlayCarveV1 {
        activation: flag_day_6001,
        subsidy_validator_bps: 2_000,
        worker_carve_permille: PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1,
    });
    // **ADR-0135 Protocol Upgrade A and ADR-0132 Protocol Upgrade C, on the same flag day** (the
    // operator's decision, after the devnet drill): the permissionless model registry governs every
    // class the node can describe from 6,001 (rows open at the first span boundary past it, the
    // seats' proofs count after one readiness age), and a model-class claim accepted from 6,001 is
    // paid `min(escrow, attempted × rate)` with its panel at the share its verification compute
    // earns. The rate is calibrated from the shadow (ADR-0132 §7): the heaviest live class, the
    // Qwen2.5 A16 row at 1.495 class draws × 2.0 network draws × 83.1 G MAC-eq, sits at ~70 % of the
    // 3,200.85 MSK escrow — under the 80 % ceiling with room for a retarget — and the hybrid at
    // ~10 %. `α = 0.1`: a five-seat full replay lands the panel share at 14 % (dense) and 20 % (hybrid).
    params.palw_model_registry = Some(flag_day_6001);
    params.palw_economic_payout = Some(PalwEconomicPayoutV1 {
        activation: flag_day_6001,
        rate_sompi_per_giga: 900_000_000,
        panel_share_alpha_permille: 100,
        panel_share_min_permille: 100,
        panel_share_max_permille: 300,
        cap_utilization_max_permille: 800,
    });
    // **ADR-0137 rides the SAME height, and it has to** (the simulation's fifth finding): the
    // registry's shares-from-admission hand the model classes 980 ‰ of a cadence they cannot fill,
    // the floor enters the competing census the first block a model produces, and its class DAA
    // hardens the floor toward its 20 ‰ — a chain at a tenth of its cadence. Armed one height apart,
    // the network would run that configuration for the gap. Armed together, no share, no model class
    // target and no epoch budget is read from 6,001: a model class draws against
    // `MAX · min(1, CCU / W₀)` with `W₀ = escrow / rate` off the block's own subsidy (306.25 G MAC-eq
    // at the rate above), the registry keeps its rows, proofs and lifecycle gate, and one
    // network-wide verification budget replaces the per-class in-flight cap. The reward is
    // unchanged: the payout's `min(escrow, attempted × rate)` is what a block buys at `W₀`.
    params.palw_work_target = Some(flag_day_6001);
    // **ADR-0132 S, ADR-0138 and ADR-0142 ride their own later day** ([`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`]).
    //
    // They arm together — one setter for the lottery and the clock, the cursor at the same height
    // — because `validate_palw_v2` refuses the clock without the cursor and the pair at two
    // heights. They are not on 7,101: that day already passed on the live chain, and arming the
    // clock behind a committed tip would recompute `daaScore` for headers the fleet already
    // stores. [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`] is past the tip and unnamed on any released
    // schedule, so the fork-id gate can see it. See
    // `docs/adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md`.
    //
    // The work target above does not depend on them: the dependency runs the other way
    // (`palw_single_lottery` needs `palw_work_target` at or below it, never the reverse), so the
    // class economy arrived on the flag day as designed and the lottery joins it here.
    let clock_day = ForkActivation::new(PALW_RC_ANCHOR_CLOCK_FENCE_DAA);
    params.palw_clock_cursor = Some(clock_day);
    params.set_palw_single_lottery(Some(clock_day));
    // ADR-0144 §9: the lock ledger, Final liability, and PanelFalseValid debit — one future
    // height, after the clock, so already-committed Valid receipts are not retroactively locked.
    params.palw_objective_offence = Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA));
    // ADR-0133 §7: the seat gate asks possession, and capacity answers through the admission rate
    // — one future height, after the clock, so every profile already folded keeps its derivation.
    params.palw_seat_gate_possession = Some(ForkActivation::new(PALW_RC_SEAT_GATE_POSSESSION_FENCE_DAA));
    // ADR-0133 §11.3: the receipt deadline a class's own window derives — sixty DAA after the seat
    // gate, because the gate has to open before a class can reach the deadline this widens. The
    // setter, not the field: the bundle carries the lane's span beside the height, and the lane
    // above is already configured.
    params.set_palw_class_receipt_window(Some(ForkActivation::new(PALW_RC_CLASS_RECEIPT_WINDOW_FENCE_DAA)));
    params.palw_execution_quanta = Some(ForkActivation::new(PALW_RC_EXECUTION_QUANTA_FENCE_DAA));
    // ADR-0132 §7.6: the short challenge window, on the 6,001 / 7,101 flag day — explicit, in the
    // schedule and the fork id, mirrored into the bundle by the setter.
    params.set_palw_short_challenge_window(Some(flag_day_6001));
    // ADR-0133 Verification V2: its own day, a hundred DAA past the flag day.
    params.palw_verification_v2 = Some(ForkActivation::new(PALW_RC_VERIFICATION_V2_FENCE_DAA));
    // ADR-0133 S3 then S2, after the lock ledger, unnamed on any released schedule.
    params.palw_verification_s3 = Some(ForkActivation::new(PALW_RC_VERIFICATION_S3_FENCE_DAA));
    params.palw_verification_s2 = Some(ForkActivation::new(PALW_RC_VERIFICATION_S2_FENCE_DAA));
    // ADR-0133 §11.2: readiness V2 takes the same day — the possession proof the 6,001 bundle left
    // as V1 (H-6 of the 2026-09-18 audit) becomes a whole-artifact multiproof here.
    params.palw_readiness_v2 = Some(ForkActivation::new(PALW_RC_VERIFICATION_V2_FENCE_DAA));
    params.dns_bft_gate =
        Some(DnsBftGateV1 { activation: flag_day_6001, t_leak_daa: 5_040, reentry_final_depth_daa: 200, min_retained_validators: 4 });
    // **ADR-0134 — the compute overlay's committee beacon retires at its own height** (2026-09-17):
    // past `PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA` the VLT's five compute subnetworks are refused;
    // below it they are accepted and do nothing, as on every block this chain has carried (none
    // carried one). Its own height, not the flag day's, so the two builds and the two failures are
    // told apart.
    params.palw_compute_overlay_retired = Some(ForkActivation::new(PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA));
    params.palw_model_evm = Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA));
    params.palw_court_ladder = Some(ForkActivation::new(PALW_RC_COURT_LADDER_FENCE_DAA));
    // **The EVM lane is ON from DAA 0, inherited from `TESTNET_PARAMS` and kept deliberately.**
    //
    // It was briefly turned off here on the reasoning that `MAINNET_PARAMS` never activates the
    // lane and ADR-0042 calls this the mainnet-CANDIDATE ruleset. That reasoning is now overruled
    // by an operator decision: testnet-11 carries the lane, and the RC is meant to be the network
    // t11's traffic moves onto — an RC without it would be a testnet the existing lane's users
    // cannot follow, which is a worse mismatch than the one with mainnet.
    //
    // It is not free, and the cost is a build requirement rather than a rule:
    // `build_block_template` cannot construct a valid template when the lane is active and the
    // binary lacks `--features evm`. **Every node that builds blocks on this network must be an
    // evm build.** `kaspad` refuses at STARTUP with that message rather than dying at its first
    // template — see `daemon.rs` — because a node that boots, syncs, and then cannot produce is
    // the most expensive way to learn this.
    //
    // The whole cadence in one spelling; a no-op on this base, which is the point of asserting it
    // here rather than trusting `TESTNET_PARAMS` to keep carrying it.
    params.with_two_minute_cadence()
}

// =================================================================================================
// testnet-12 — the 2026-09-22 regenesis of testnet-11, with every rule in force from DAA 0
// =================================================================================================

// **testnet-12's base identity: testnet-11's, at a new genesis, with every fence at DAA 0.**
//
// The operator's instruction for this regenesis was one sentence — "過去に fence をつけて DAA から
// 有効にしていたもの、将来なるものを genesis から有効にすること" — and a regenesis is the one
// moment it can be obeyed literally. On a LIVE chain a rule arrives at a height because the blocks
// below that height were judged under the old rule and cannot be re-judged (ADR-0083 path (a)); a
// chain with no blocks below has nothing to protect, so the honest form of every one of those rules
// is `always()`. That is what [`palw_t12_arm_every_rule_from_genesis`] does, and doing it through
// `for_each_fence` rather than by hand is what keeps it true of fences added after today.
//
// What is NOT inherited from t11:
// * the genesis block ([`crate::config::genesis::PALW_T12_GENESIS`]) and with it the premine —
//   the 858M t12 community table on its own sentinel txid;
// * every scheduled height. t11's flag days (1,150 / 1,900 / 2,150 / 2,400 / 3,500 / 4,000 / 6,900
//   / 7,100 / 7,101 / 7,200 / 7,300 / 7,301 / 7,780 / 7,800 / 8,100 / 8,160 / 8,500 / 8,600 /
//   8,700) are gone, not moved: there is no height at which this chain changes its mind.
//
// What IS inherited, deliberately: the eight genesis bond cards (the operators hold those keys
// already), the frozen 120 s cadence, the P2P port (26311 — see `NetworkId::default_p2p_port`),
// the DNS seeder names, and the EVM lane.

/// **ADR-0151 D3: can this network advance its consensus clock without admitting a claim?**
///
/// The question the genesis collateral rule was really asking, asked directly. `verify_palw_genesis_v2`
/// used to demand that every genesis bond be able to hold `window_bind` of the dearest claim at once,
/// because of a cycle:
///
/// ```text
/// collateral exhausted -> no claim admitted -> no block produced -> DAA frozen
///   -> no BindTimeout -> collateral never released
/// ```
///
/// It bought the way out of that cycle with capital: 1,620,178 MSK a seat on the testnet-12 card,
/// against a reachable liability of 21,630. **Capital is the wrong instrument** — it makes the initial
/// operators the only parties who can afford a seat, and a genesis seat and a later seat obey
/// different economics for the same duty. The cycle should be cut where it closes, which is the
/// clock.
///
/// It is cut when two things hold, and this function is those two things:
///
/// * **the heartbeat lane is armed from genesis.** A heartbeat carries no claim and reserves no
///   exposure — it is minted against a constant target, `PALW_HEARTBEAT_MAX_PER_MERGESET` a mergeset —
///   so a bond whose ceiling is completely full can still mint one;
/// * **no lane this network can produce is priced by `bits`.** On a `ConsensusV2` network every V1
///   proof-of-work activation is `never()` (algos 1–5 are unreachable), and past the single lottery
///   `algo_id_is_priced_by_bits_v3` takes the attempt, receipt, execution and round lanes out of the
///   pricing. So `priced == 0` on every mergeset — and ADR-0138 §3b's stand-in rule
///   (`SampledDifficultyManager::calc_daa_score_and_mergeset_non_daa_blocks`: `stand_in = priced == 0
///   && heartbeats > 0`) makes exactly one heartbeat tick the clock in the missing anchor's place,
///   at most once per wall-clock interval under ADR-0142's cursor.
///
/// Together: **the DAA score advances every heartbeat interval regardless of what any bond can
/// afford.** Deadlines fire, `BindTimeout` releases exposure, and the chain cannot wedge on
/// collateral. Both clauses are height-independent — the second is about activations that are
/// `never()`, the first about a fence armed at 0 — so the answer cannot become false later.
///
/// Read by the genesis gate ([`crate::palw_genesis_v2::verify_palw_genesis_v2_with_clock_v1`]) and by
/// the collateral derivation, which drops its liveness term when this is true.
pub fn palw_clock_advances_without_a_claim_v1(params: &Params) -> bool {
    // A heartbeat must be mintable from block one.
    let heartbeat_from_genesis =
        params.palw_heartbeat.is_some_and(|h| h.activation != ForkActivation::never() && h.activation.is_active(0));
    if !heartbeat_from_genesis {
        return false;
    }
    // And no lane whose digest `bits` prices may be producible, or the beat is not the stand-in and
    // the clock belongs to a lane a bond has to pay to enter.
    let never = |a: ForkActivation| a == ForkActivation::never();
    never(params.pow_blake2b_sha3_activation) && never(params.pow_palw_activation) && never(params.pow_palw_ollama_activation)
}

/// **The escrow one genesis-era claim carries** — the cash half of `palw_max_fraud_gain_v1`
/// (ADR-0151 D1). The pre-deflationary base subsidy through the overlay's worker carve, which is
/// what `worker_carve_v2` computes for a block; stated as its own function so the collateral
/// derivation and a reader are looking at one spelling.
/// **The block subsidy at genesis, as `calc_block_subsidy(0)` pays it** — `kaspa-consensus`'s
/// coinbase manager is not reachable from this crate, so the two inputs it reads are restated here:
/// the pre-deflationary figure while `deflationary_phase_daa_score > 0`, and otherwise the
/// emission table's month-0 base scaled to the block time, `ceil(3,704,683,450 x ttpb_ms / 1000)`.
/// `consensus/src/processes/coinbase.rs` pins the equality in a test so the two spellings cannot
/// drift apart silently.
pub fn palw_genesis_block_subsidy_sompi(params: &Params) -> u64 {
    if params.deflationary_phase_daa_score > 0 {
        return params.pre_deflationary_phase_base_subsidy;
    }
    (PALW_SUBSIDY_MONTH0_BASE_SOMPI as u128 * params.target_time_per_block_history().after() as u128)
        .div_ceil(1000)
        .min(u64::MAX as u128) as u64
}

/// `SUBSIDY_BY_MONTH_TABLE[0]` of `consensus/src/processes/coinbase.rs` — the month-0 emission per
/// second of block time, before the block-time scaling. Pinned here for the same reason the genesis
/// collateral is: the derivation lives in a crate this one cannot call.
pub const PALW_SUBSIDY_MONTH0_BASE_SOMPI: u64 = 3_704_683_450;

/// **What testnet-12's first block pays** — [`palw_genesis_block_subsidy_sompi`] at its 120,000 ms
/// block time, pinned so the card's escrow input is a number an operator can read.
pub const PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

/// **The overlay's worker carve, 720 ‰ of the subsidy** (ADR-0126): testnet-11's flag-day value,
/// carried to testnet-12 by its base card. One spelling for the preset and for the figures derived
/// from what a claim escrows.
pub const PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1: u16 = 720;

/// **`E`: the escrow a testnet-12 genesis-era claim of the heaviest class carries** — its first
/// block's subsidy through the overlay's worker carve, `escrow_for_a_genesis_claim_v1`'s arithmetic
/// (3,200.85 MSK). The price of work pays the heaviest weight-bearing class the escrow whole, so
/// this is the most one claim pays. Pinned against the card by
/// `palw_t12_genesis_claim_escrow_is_the_cards`.
pub const PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI: u64 =
    PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI / 1_000 * PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1 as u64;

fn escrow_for_a_genesis_claim_v1((subsidy, carve_permille): &(u64, Option<u16>)) -> u64 {
    let permille = carve_permille.unwrap_or(0) as u64;
    subsidy / 1_000 * permille
}

pub fn palw_t12_base_params() -> Params {
    let mut params = palw_rc_base_params();
    params.net = NetworkId::with_suffix(NetworkType::Testnet, 12);
    params.genesis = crate::config::genesis::PALW_T12_GENESIS;
    // Mainnet-assumed DNS-finality validators (user decision 2026-09-24) — before the arming walk,
    // exactly where testnet-11's table sat, so the walk treats both the same.
    params.dns_params = Some(PALW_T12_DNS_PARAMS);
    // **Mainnet's future-time tolerance, derived as a card derives it** (user decision 2026-09-25:
    // testnet-12 runs mainnet's numbers; mainnet audit 2026-09-06 L-1). The 27-sample median window
    // at one 120 s block a sample models 1,620 s, not the hash lineage's 132 s, and testnet-12 is a
    // re-mint, so it takes the value a card takes rather than the pinned gap testnet-11 and devnet
    // keep. **The effect, stated:** testnet-12's heartbeat clock is paced by header stamps, and a
    // stamp is bounded above only by this tolerance, so a producer that stamps every beat at its
    // slot can run the DAA clock up to 1,620 s — about 13 slots — ahead of wall time (about 1 slot at
    // 132 s). The lead is a one-time offset in STAMPS (the clock floor still spaces two stamps at
    // least one interval apart, `t12_a_producer_runs_the_clock_at_most_the_drift_budget_ahead`), but
    // not in wall time: those ticks can be minted at once, so one producer can advance the DAA by
    // `⌊T / I⌋ + 1` = 14 with no other block in between (2 at 132 s) — every DAA-denominated window
    // is shortened by that much wall time, and a burst that outlasts a readiness row's remaining age
    // lapses the row with no proof able to land (the 2026-09-25 mainnet-values review, HIGH; measured
    // on the lifecycle fold by `t12_a_run_ahead_burst_outlasts_a_readiness_row`). **Closed by the lead
    // cap** (the user's option (a), 2026-09-25): `palw_clock_lead_cap`, armed below, admits a header
    // that moves the clock at most 132 s past the receiving node's clock, so a burst is 2 ticks
    // again and every other block keeps this tolerance
    // (`t12_the_lead_cap_bounds_a_run_ahead_burst_to_two_ticks`).
    params.timestamp_deviation_tolerance = palw_v2_timestamp_deviation_tolerance_v1(
        params.past_median_time_window_size,
        BlockrateParams::new_two_minute_bps().past_median_time_sample_rate,
        BlockrateParams::new_two_minute_bps().target_time_per_block,
    );
    // **Mainnet's block-level ceiling, 225** (user decision 2026-09-25). No block on this network
    // derives a level above 0 whichever ceiling it runs — the single lottery gives an attempt header
    // none, and heartbeat, receipt and round blocks derive none — so the ceiling sets only the genesis
    // level and the number of (genesis-only) pruning-proof levels above 0, and with them the proof's
    // header budget, `(225 + 1) x 2 x m` = 452,000 slots (502,000 at 250). Headers are unchanged: a
    // header lists parents only up to the first genesis-only level. **Owed at either ceiling, not
    // caused by this one:** with every block at level 0 the level-0 proof is the whole history below
    // the pruning point, so that budget, not the level pyramid, is what eventually bounds a proof —
    // about 314 days of two-block heartbeat slots below the pruning point at 225 (348 at 250). And
    // the pruning point itself does not leave genesis until the first claim is `Final` (the PALW
    // safe frontier caps it). Both measured at 225 on a moved pruning point — built, validated by a
    // node at genesis, applied by a staging node — by
    // `t12_a_moved_pruning_point_s_proof_builds_validates_and_applies_at_225`.
    params.max_block_level = MAINNET_PARAMS.max_block_level;
    palw_t12_arm_every_rule_from_genesis(&mut params);
    params
}

/// **Arm every rule this binary knows at DAA 0, and say which ones cannot be armed and why.**
///
/// Two passes, in this order and for this reason:
///
/// 1. **Install the dormant fences.** A `None` fence is absent, and `for_each_fence` visits absence
///    through a sentinel it cannot write back — so a walk alone can move a height and can never
///    turn a rule on. The struct-bearing ones also need a CONFIGURATION beside the height, and a
///    configuration is a decision: each one below is the value this tree's own fence sweep
///    (`fork_id_v1`'s table) uses, not a number invented here.
/// 2. **Walk every fence to 0.** `for_each_fence` is exhaustive over `Params` by destructuring, so
///    a fence added to the struct tomorrow is armed by this function without anybody remembering to
///    come back — which is the only way a list of 80 heights stays true. `never()` is preserved
///    (score `u64::MAX` round-trips to `never()`), which is what keeps the three V1 proof-of-work
///    activations off: a `ConsensusV2` network may activate none of them and `validate_palw_v2`
///    refuses a build that does.
///
/// **The Layer-0 activations are saved and restored around the walk** rather than left to the
/// `never()` round-trip alone, because `crescendo_activation`, `pq_activation_daa_score` and
/// `evm_activation_daa_score` are already 0 and the five EVM feature fences are set BELOW by name.
/// Restoring them explicitly means this function's effect on the EVM lane is a list one can read,
/// not a side effect of a generic walk.
///
/// **Three PALW fences stay dormant, and none of them is an oversight:**
///
/// * `palw_inactivity_leak` — **`validate_palw_v2` refuses `Some`.** ADR-0066 Decision 4's leak was
///   replaced by `dns_bft_gate`'s (ADR-0128, armed below from 0); the field survives only so its
///   Some-only hashing keeps the fingerprints it is already in. Arming it would fail the build.
/// * `palw_frontier_provenance` — ADR-0065 D2 is **restated as unimplementable** inside the state
///   fold: `safe_frontier` is written by a pure single-chain transition whose result is hashed into
///   `state_root`, so a value that depended on the fork point would depend on which branch a node
///   happened to walk. There is no rule here to arm.
/// * `palw_beacon_fold` — ADR-0073 SA-1 folds `k` attempt blocks into the **free-prompt beacon**,
///   and this tree's own finding is that PALW block production has no beacon (the attempt IS the
///   execution anchor). Arming a fold over a randomness source nothing reads would put a `k` inside
///   the fingerprint to no effect.
///
/// Anything else `None` on testnet-11 is armed here, including the three the operator decided on
/// 2026-09-22: `palw_context_ladder` (the 512 rows — see [`palw_t12_shipped_params`]),
/// `palw_artifact_root_ownership` (ADR-0143, with the 2026-09-19 audit's `(class_id, root)` keying,
/// which is what lets one artifact carry `@512` and `@2048`), and `palw_panel_exposure_floor`
/// (ADR-0130's λ, 5 since the user's 2026-09-25 mainnet-values decision, affordable here because the
/// collateral is re-derived from the registered classes rather than left at the 10,000 MSK carve).
pub fn palw_t12_arm_every_rule_from_genesis(params: &mut Params) {
    let at = ForkActivation::always();

    // ---- pass 0: the ADR-0068 Phase 2 lanes ----------------------------------------------------
    // The same two `palw_rc_arm_phase1` installs, with the same constants (the binary's own, so the
    // `validate_palw_v2` locks hold by construction). They are NOT inherited from
    // `palw_rc_base_params`, which leaves both `None` — the RC preset gets them from `arm_phase1`,
    // and this preset deliberately does not route through that function (see this module's
    // `palw_t12_shipped_params`). Stated here because `palw_heartbeat_transparent` below is refused
    // on a network whose heartbeat lane is dormant: a chain cannot hold a rule about how heartbeats
    // count in an anticone when it has no heartbeats.
    params.palw_heartbeat = Some(PalwHeartbeatV1 {
        activation: at,
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    });
    params.palw_attempt_work = Some(PalwAttemptWorkV1 {
        activation: at,
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    });

    // ---- pass 1: the fences testnet-11 leaves dormant ------------------------------------------
    // ADR-0064: a bond is usable in the block that accepts its registration.
    params.palw_bootstrap_activation = Some(at);
    // **The one testnet-11 genesis fence this preset had missed** (found 2026-09-23 by the ledger
    // `t12_arms_every_fence_t11_armed`, which now exists so it cannot be missed again). Armed at 0 on
    // testnet-11 since Relaunch 5 — an Unavailable seat abstains from fork choice — but `None` in
    // `palw_rc_base_params`, and pass 2's `for_each_fence` zeroing has no `u64` to visit inside a
    // `None`. A rule the live network has run from genesis, silently dormant on its successor, is
    // precisely the regression the operator asked to be checked for.
    params.palw_unavailable_abstains = Some(at);
    // **ADR-0065 D1 — the ONE rule this preset cannot arm at DAA 0, and the reason is the rule
    // itself.** A bond may judge only if it was registered `window_daa` before the anchor, and at
    // genesis every bond was registered at DAA 0: `validate_palw_v2` refuses the pair in those words
    // ("armed before its own window has elapsed since genesis — every genesis bond would be immature
    // at once and no panel could be drawn"). A maturity window is not a statement a chain can make
    // about its own first block.
    //
    // So it is armed at the earliest height where it MEANS anything — its own window — rather than
    // dropped. From DAA 1,000 the eight genesis bonds are mature by a margin and every bond
    // registered afterwards waits its window, which is the whole content of the decision. This is
    // the only scheduled height on this network, and it is scheduled by arithmetic rather than by
    // an operator's calendar.
    params.palw_bond_maturity = Some(PalwBondMaturityV1 {
        activation: ForkActivation::new(PALW_T12_BOND_MATURITY_WINDOW_DAA),
        window_daa: PALW_T12_BOND_MATURITY_WINDOW_DAA,
    });
    // ADR-0071 SA-1..SA-4: a capability declaration is bounded, priced and proven. Safe from genesis
    // because the genesis registrations name `palw_genesis_registrant_bond_v1()` and therefore keep
    // SA-3's exemption — the reason that sentinel exists rather than a real registry row.
    params.palw_capability_bound = Some(at);
    // ADR-0075 SA-1/SA-2: the permissionless certification lane pays rent.
    params.palw_certification_rent = Some(at);
    // ADR-0075 D14: only a chunk that can complete a group spends the block's certification cap.
    params.palw_chunk_cap_charge = Some(at);
    // ADR-0077 Phase B Decisions 10–14 / SA-4: the court prices the checkpoint, not the context.
    // **This is the fence the 512 rows need** (`PALW_CONTEXT_LADDER_ROWS`), so it is the one the
    // operator's "qwen25 / qwen36 を ctx512 から" asks for.
    params.palw_context_ladder = Some(at);
    // ADR-0081 D3: a job's prompt ids are a tiled Merkle root, so the court that reads ONE id can
    // open it. Genesis-only by rule (`validate_palw_v2` refuses it above genesis) — a regenesis is
    // the only occasion this rule has.
    params.palw_prompt_ids_merkle = Some(at);
    // ADR-0042 D11: the COMPLETE nineteen-context signature set, not the frozen nine.
    params.palw_signature_contexts_v2 = Some(at);
    // ADR-0082 D2 (C-2/H-5): the fused terminal's clock does not convict for a move nobody can make.
    params.palw_court_responder_coverage = Some(at);
    // **ADR-0082 D10/D11 (with FP Job V4, RFC-0001 §A) is not armed at genesis**: this build carries
    // both rules, and they arm at a post-launch flag day through
    // `PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1` (dormant on this branch; see its TODO) — a live
    // chain changes its rules by activation, never by regenesis.
    // params.palw_fp_decode_rules = Some(at);
    // **ADR-0096 D6-D8 stays dormant because THIS BUILD cannot carry it**, in `validate_palw_v2`'s
    // own words: consensus-core has no constraint automaton and no version-6 free-prompt job, the
    // court has no v3 refutation arm, and no engine masks a decode. A missing implementation, not a
    // decision a regenesis gets to make.
    // params.palw_fp_decode_constraint = Some(at);
    // ADR-0044 D9 (L-2): the two advertised free-prompt caps are enforced, not merely advertised.
    params.palw_fp_ruleset_caps = Some(at);
    // ADR-0093 D6/D8: admission refuses a fused class the court cannot dissect, and a root claim
    // carries the anchor the dissection's bottom needs.
    params.palw_fused_dissectable = Some(at);
    params.palw_attn_anchored_root = Some(at);
    // **ADR-0100 D4's per-shard licensing stays DORMANT, and `validate_palw_v2` is why**: it refuses
    // it beside `palw_admission_independence` — "a stratified panel seats no outsider, so a bought
    // class licensed by parts would be judged by its own population alone" (ADR-0147). The two
    // cannot both be armed, and of the pair the independence rule is the one this chain needs: it is
    // the 2026-09-19 reward audit's F3, CRITICAL and live, while no class in this genesis declares a
    // shard plan and none can be registered without one. Arming the capability and losing the
    // correctness rule would be trading a fix for a feature nobody uses.
    // params.palw_shard_licensing = Some(at);
    // ADR-0102: the court adjudicates the embedding lift per token (graph-v6's fenced kernel).
    params.palw_token_lift = Some(at);
    // The Kimi K3 family's fenced kernels. **Genesis-only in effect**: a genesis that registers a
    // Kimi class is refused unless this is armed from genesis, and arming it later cannot rescue
    // one. This genesis registers none, so what it buys is that a future K3 registration is a
    // transaction rather than a third regenesis.
    params.palw_kimi_k3 = Some(at);
    // ADR-0083 D1's other half (mainnet audit 2026-09-05): a receipt row is not a priced row.
    params.palw_receipt_rows_unpriced = Some(at);
    // ADR-0072 D8 (C-1): every field inside the priced bytes is pinned at the HEADER stage.
    params.palw_attempt_header_pins = Some(at);
    // ADR-0105: a heartbeat never turns a bonded block red — the measured `ghostdag_k = 1` trap.
    params.palw_heartbeat_transparent = Some(at);
    // ADR-0107: a class's cadence share grows only on work that reached `Final`.
    params.palw_share_growth_final = Some(at);
    // ADR-0018 §E (H-2/H-3/M-1): the validator payout paths obey their own stated bounds.
    params.palw_validator_payout_bounds = Some(at);
    // ADR-0045 D2 (M-2): the crossing block derives its own epoch's budget from the parent state.
    params.palw_epoch_boundary_budget = Some(at);
    // ADR-0123: a spent class budget borrows the epoch slots nobody else is filling — the rule that
    // stops the measured 14× stall (t11, 2026-09-12: one seat's 367/367 budget and no floor exit).
    params.palw_epoch_budget_release = Some(at);
    // ADR-0130: the seat exposure floor, **λ = 5** — a seat reserves five times the most it can be
    // paid (user decision 2026-09-25: testnet-12 runs mainnet's numbers, and ADR-0130 D1's mainnet
    // statement is "λ = 5–10 under consideration"; the user took 5). Not the fence sweep's 2
    // (`fork_id_v1`'s table), which prices a hypothetical arming, not this network. Under R-core+ it
    // is L-4's λ-term, `5 × (E × 200‰ / 5 seats)` = 640.17 MSK a claim (256.07 at λ = 2): it now
    // binds the floor AND the 8k duty (the 8k row bound on `lock_2` at λ = 2), just under the
    // `⌊(w + E) / 5⌋` cap, and the 2M duty stays the cap, 12,588.76.
    // The genesis seats do not re-derive: option A sizes the claims a seat PRODUCES
    // (`palw_v2_collateral_for_class_set_v1` reads no λ), and a seat's judging duty is released at
    // the claim's Final or void.
    params.palw_panel_exposure_floor = Some(PalwPanelExposureFloorV1 { activation: at, reward_multiple_permille: 5_000 });
    // ADR-0143 with the 2026-09-19 audit's remedy applied: an artifact root has one owner per CLASS,
    // and the index is bounded. Keyed `(class_id, root)` — global keying would refuse the second of
    // `@512` and `@2048` over one artifact, which is exactly the ladder this net registers.
    params.palw_artifact_root_ownership = Some(at);
    // The 2026-09-19 adversarial audit: one operator identity, one bond.
    params.palw_operator_id_unique = Some(at);
    // A public source URI on new model registrations.
    params.palw_public_model_source_required = Some(PalwPublicModelSourceRuleV1 { activation: at });
    // ADR-0145 §1–§5 (reward audit F1, CRITICAL): a claim's work is DERIVED, not declared.
    params.palw_canonical_work = Some(at);
    // ADR-0145 / F3 (I3, I4): a class is not judged by its own registrant.
    params.palw_admission_independence = Some(at);
    // ADR-0145 §5/§6 (F2, CRITICAL): the free-prompt lane's work is derived, and a cached prefix is
    // not paid twice.
    params.palw_fp_derived_work = Some(at);
    // **ADR-0151's economic-safety bundle** — the five rules that make a fraudulent Final's rights
    // recoverable and price what is not. Armed here as ONE fence because each is unsound alone: the
    // pricing without the maturity collateralises rights that are already recoverable, the maturity
    // without the pricing freezes honest permits and still leaves the residual free, and the
    // forfeiture without the lineage has nothing to forfeit against.
    //
    // Without it a held-2M Final's 270,029 execution permits are worth nothing to
    // `palw_max_fraud_gain_v1`, and three colluding seats out-value the lie by TWO SOMPI — so one
    // economic actor holding a claim's whole 3-of-5 quorum is profitable, which is the last thing
    // `palw_admission_independence` leaves open ("it is a price, not a proof").
    params.palw_economic_safety = Some(at);
    // **The 2026-09-23 economic audit's fixes** (`Params::palw_audit_2026_09_23`): the seat lock's
    // unit, the cross-block execution replay, the execution-credit unit and quanta bound, the inert
    // ineligible PanelBound, and the non-fused attention-geometry admission check. Every one is a
    // rule change inside a rule this network already arms, so on this network it is armed with them.
    params.palw_audit_2026_09_23 = Some(at);
    // **The second clock** (`Params::palw_settled_anchor_depth`): an economic deadline on this
    // network elapses only after its DAA window AND this many further settled anchors. The value is
    // the drill's starting point, not a derivation — see the constant's own note.
    params.palw_settled_anchor_depth = Some(PALW_T12_SETTLED_ANCHOR_DEPTH);
    // **ADR-0147's admission jury every 100 DAA** (`Params::palw_admission_audit_period_daa`), the
    // standard the user set on 2026-09-23, instead of once per 1,000-DAA epoch.
    params.palw_admission_audit_period_daa = Some(PALW_ADMISSION_AUDIT_PERIOD_DAA_STANDARD);
    // **ADR-0151's maturity is the challenge window this network APPLIES** (user decision
    // 2026-09-25, `Params::palw_exec_quantum_maturity_daa`): every licence here gets ADR-0132 §7.6's
    // short window from genesis (testnet-11's flag day, moved to 0 by pass 2's walk), so a Final's
    // execution rights wait those 120 DAA — not the unshortened 1,200 that put the first round permit
    // ~45 h after launch. The longer convictable gap it leaves is priced (on this network's capped
    // mint, at the same 65,537 quanta as before), so no collateral moves.
    params.palw_exec_quantum_maturity_daa = Some(crate::palw_state_v2::PALW_SHORT_CHALLENGE_WINDOW_DAA_V1);
    // **The clock floor** (`Params::palw_clock_floor`, the 2026-09-24 heartbeat audit's H3 and H5):
    // a heartbeat stamped before its slot is invalid, a heartbeat chain is paced by its own stamps,
    // a step is stamped at or past the cursor, and the reference is the earliest tied step. The
    // cursor it refines is armed by pass 2 (testnet-11 schedules it; the walk moves it to 0).
    params.palw_clock_floor = Some(at);
    // **The lead cap** (`Params::palw_clock_lead_cap`, the 2026-09-25 mainnet-values review's HIGH):
    // a heartbeat or a clock step is admitted at most 132 s past the receiving node's clock, so the
    // 1,620 s tolerance this network runs cannot be spent as a 14-DAA burst.
    params.palw_clock_lead_cap = Some(at);
    // **ADR-0152 v2 F2** (`Params::palw_offence_attribution`): a false `Valid` is judged by one
    // adjudicator bound to the claim's committed root, on the receipt form the chain licensed, and
    // the V1 payload whose job-id check no real claim can pass is refused. The objective-offence
    // ledger and Verification V2 it rides on are testnet-11's heights, moved to 0 by pass 2.
    params.palw_offence_attribution = Some(at);
    // **ADR-0152 v3.1 R-core+** (`Params::palw_rcore_plus`), by name: every prerequisite it is
    // refused without is armed at 0 by this pass or by pass 2's walk, and C7 is the 2M row (U1).
    // The v22 skeleton lands the fence with every writer dormant, so arming it moves testnet-12's
    // encodings and ids and no fold behaviour. The bundle's mirrors are re-made by
    // `sync_palw_rcore_plus` where the bundle is assembled.
    params.palw_rcore_plus = Some(at);
    params.palw_rcore_conservative_classes = &PALW_T12_RCORE_CONSERVATIVE_CLASSES;
    // **ADR-0152 §4-quater** (`Params::palw_class_verify_deadline`), by name: a claim's verification
    // deadline is its class's, in the registry's units (a registry span is 5 DAA, not this lane's
    // 1-DAA span); `Final` waits for it; and a class whose deadline needs a measurement — the 2M row —
    // takes no claim until a flag day installs a measured row (U-D1). The rows are empty at genesis.
    // Its prerequisites (§11.3's receipt window, the derived free-prompt work) are armed at 0 by pass
    // 2's walk. The bundle's mirror is re-made by `sync_palw_class_verify_deadline`.
    params.palw_class_verify_deadline = Some(at);
    // **ADR-0152-adjacent: the Activation Pool, R1 and R2** (`Params::palw_activation_pool`, user
    // decision 2026-09-25): a listing is not reclaimed for the network's absence, the registry's
    // step audits a `Candidate` only at its own staggered span and skips Dormant and Frozen classes,
    // and a per-class pool pays preparation and activation out of sink-bound top-ups. Genesis-only;
    // its three prerequisites are armed above or by pass 2's walk. The terms are the user's
    // illustrative scale, to be tuned.
    params.palw_activation_pool =
        Some(PalwActivationPoolParamsV1 { activation: at, terms: crate::palw_activation_pool_v1::PALW_ACTIVATION_POOL_TERMS_V1 });
    // **The readiness-V2 horizon** (`Params::palw_readiness_v2_max_age_spans`, user decision
    // 2026-09-25, readiness capacity option (a)): a possession row stands twenty-four spans (24 DAA)
    // instead of eight, so two proofs a block keep both genesis model classes' seats counted, and the
    // node re-proves at half of it. Genesis-only; its prerequisites (`palw_model_registry`,
    // `palw_readiness_v2`) are armed at 0 by pass 2's walk. The bundle's mirror is re-made by
    // `sync_palw_readiness_v2_max_age_spans`.
    params.palw_readiness_v2_max_age_spans = Some(PalwReadinessV2MaxAgeParamsV1 {
        activation: at,
        max_age_spans: crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1,
    });
    // **ADR-0065 D4 — an `Unavailable` receipt convicts nobody.** Armed on testnet-11 from its
    // first block and left `None` here by omission, which made this the ONE rule of testnet-11's
    // that the regenesis did not carry: three seats that merely failed to RECEIVE a claim's
    // material voided it as `ProducerDefaulted` and slashed the producer. Armed now, beside the
    // admission check above that closes the pricing lever the abstention used to mask.
    params.palw_unavailable_abstains = Some(at);

    // ---- pass 2: every fence's height to 0, `never()` preserved ---------------------------------
    // The Layer-0 / EVM activations are this walk's only exceptions, restored by name below.
    let layer0 = (
        params.crescendo_activation,
        params.pow_blake2b_sha3_activation,
        params.pow_palw_activation,
        params.pow_palw_ollama_activation,
        params.pq_activation_daa_score,
        params.evm_activation_daa_score,
    );
    let bond_maturity = params.palw_bond_maturity;
    params.for_each_fence(&mut |score| {
        if *score != u64::MAX {
            *score = 0;
        }
    });
    // The walk zeroes every height, and this is the one height that may not be zero (see the
    // ADR-0065 D1 note above). Restored rather than special-cased inside the visitor, because the
    // visitor's value is that it knows nothing about individual fences.
    params.palw_bond_maturity = bond_maturity;
    params.crescendo_activation = layer0.0;
    // **A `ConsensusV2` network activates no V1 PALW proof-of-work.** These are `never()` on the RC
    // base and the walk preserves `never()`, so restoring them is a belt-and-braces restatement of
    // a refusal `validate_palw_v2` also enforces — the one place where saying it twice is cheap.
    params.pow_blake2b_sha3_activation = layer0.1;
    params.pow_palw_activation = layer0.2;
    params.pow_palw_ollama_activation = layer0.3;
    params.pq_activation_daa_score = layer0.4;
    params.evm_activation_daa_score = layer0.5;

    // **The five EVM feature fences, armed by NAME rather than by the walk.** Four of them are
    // `u64::MAX` on testnet-11 (inert, never scheduled anywhere) and the walk preserves that, so
    // "every future rule from genesis" has to say them out loud:
    //
    // * gas pool v2 — the EVM-lane liveness fix (t11 schedules it at 2,125,000): declared gas gates
    //   admission, the pool is debited by ACTUAL gas, and a non-fitting tx is skipped without
    //   blocking the smaller ones behind it;
    // * F002 (audit M-03) — a tx whose withdrawals would push a block over
    //   `MAX_WITHDRAWALS_PER_EVM_BLOCK` is a class-2 skip;
    // * the bridge ledger (2026-09-24) — `evm_total_native_balance` is the checked ledger of the EVM
    //   supply the L1 backed, and a tx whose draw it cannot cover is a class-2 skip;
    // * F003 (PREA v1.1 §9) — the `MLDSA87_VERIFY` precompile is registered;
    // * typed receipt root (§12 Phase-7) — the lane commits the EIP-2718 typed receipt root.
    //
    // Each changes execution results, which is why each is fenced at all; a chain with no execution
    // below DAA 0 is the one place they are free.
    // **The execution lane opens at its SHORT span, because there is nothing to shorten.**
    //
    // The RC lane opens at a 5-DAA schedule span and narrows to 1 at a later height; the walk above
    // zeroed both, and `validate_palw_v2` refuses that — "the short span must fire after the lane
    // opens" — correctly, since two rules at one height are one rule wearing two names. A chain
    // born today simply opens at the span the later fence was heading for: one permit a round, one
    // span a DAA, no widening scheduled. `PalwExecSpanShortV1::NONE` is the "no second height"
    // spelling, not a disabled feature.
    //
    // This is the global scarcity every model shares, and it is the reason removing the per-model
    // share does not raise the block rate: block rights are `verified work / execution quantum`,
    // and the lane's one permit a round is what bounds how many of them become blocks. Registering
    // ten models cannot widen it.
    if let Some(lane) = params.palw_execution_lane.as_mut() {
        lane.schedule_span_daa = lane.short_span.schedule_span_daa.max(1);
        lane.short_span = PalwExecSpanShortV1::NONE;
    }
    params.evm_gas_pool_v2_activation_daa_score = 0;
    params.evm_f002_withdraw_cap_activation_daa_score = 0;
    params.evm_bridge_ledger_activation_daa_score = 0;
    params.evm_f003_mldsa_verify_activation_daa_score = 0;
    params.evm_typed_receipt_root_activation_daa_score = 0;

    // ---- the mirrored setters, re-applied so the bundle cannot carry a stale height -------------
    // `set_palw_*` write a field AND the V2 bundle's copy of it; the walk wrote only the field. On
    // this bundle-less base the mirror is a no-op today, and stating it is what keeps that true if
    // this function is ever called on an assembled ruleset.
    let single_lottery = params.palw_single_lottery;
    params.set_palw_single_lottery(single_lottery);
    let short_challenge = params.palw_short_challenge_window;
    params.set_palw_short_challenge_window(short_challenge);
    let receipt_window = params.palw_class_receipt_window;
    params.set_palw_class_receipt_window(receipt_window);

    // Three fences stay dormant by the reasoning in this function's doc block; asserted rather than
    // left implicit, because "we meant to leave that one" and "we forgot that one" look identical in
    // a diff.
    debug_assert!(params.palw_inactivity_leak.is_none(), "validate_palw_v2 refuses a network that sets the retired leak");
    debug_assert!(params.palw_frontier_provenance.is_none(), "ADR-0065 D2 is unimplementable inside the state fold");
    debug_assert!(params.palw_beacon_fold.is_none(), "PALW block production has no beacon to fold");
    debug_assert!(params.palw_shard_licensing.is_none(), "validate_palw_v2 refuses it beside palw_admission_independence");
    debug_assert!(
        params.palw_fp_decode_rules.is_none(),
        "ADR-0082 D10/D11 (and FP Job V4) are not a genesis rule here: they arm at a flag day through PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1"
    );
    debug_assert!(params.palw_fp_decode_constraint.is_none(), "this build carries no constraint automaton (ADR-0096 D6-D8)");
}

pub fn palw_rc_params(
    base_class_id: crate::Hash64,
    class_catalog_root: crate::Hash64,
    court_catalog_root: crate::Hash64,
    genesis_pwu_per_inference: u64,
    genesis_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    palw_v2_params_on_base(
        palw_rc_base_params(),
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        genesis_pwu_per_inference,
        genesis_artifact_root,
        genesis_bonds,
    )
}

/// **[`palw_rc_params`] over an arbitrary base identity** — the same assembly, for a network that
/// is not the RC.
///
/// Everything `palw_rc_params` did after choosing its base was already generic: the finality depth
/// is derived from the BUNDLE's challenge window, the pruning depth from that finality depth and
/// the claim lattice, and `validate_palw_v2` judges the result. Only the base was hard-coded. So
/// enabling PALW on another network is choosing a different `base`, not writing a second assembly
/// — which matters because a second assembly is a second set of derived depths, and those are
/// exactly the numbers whose drift audit M2-20 was about.
///
/// The base supplies the identity (net, genesis, cadence, the DNS overlay, every activation
/// score); this supplies the ruleset. A caller that passed a base whose PALW fences disagree with
/// a V2 mode gets the refusal from `validate_palw_v2` at the bottom rather than a running node.
#[allow(clippy::too_many_arguments)]
/// **The difficulty window a V2 network runs, in ONE place.**
///
/// 264 samples at `difficulty_sample_rate = 1` and 120 s a block is ~8.8 h of difficulty memory —
/// the number `TESTNET_PARAMS` states and gives its reasons for (a provisioned producer fleet
/// rather than an open hashrate market, so 264 samples' +-6 % stability beats fast response), and
/// the number devnet runs. It is a function of the cadence, so it belongs beside the cadence and
/// not on each preset: mainnet's own 661 is sized in SECONDS at 10 bps and becomes 22 h under this
/// cadence, which is how a carded mainnet acquired a difficulty memory nobody chose.
pub const PALW_V2_DIFFICULTY_WINDOW_SIZE: usize = 264;

/// The floor beneath [`PALW_V2_DIFFICULTY_WINDOW_SIZE`] — the fixed-difficulty launch window a
/// fresh V2 chain runs before it has samples to retarget from (~5 h at this cadence).
pub const PALW_V2_MIN_DIFFICULTY_WINDOW_SIZE: usize = MIN_DIFFICULTY_WINDOW_SIZE;

/// **The future-time tolerance a past-median-time window of this shape MODELS** (mainnet audit
/// 2026-09-06, L-1).
///
/// `MEDIAN_TIME_SAMPLED_WINDOW_SIZE` is defined as `(2·T − 1) / PAST_MEDIAN_TIME_SAMPLE_INTERVAL`
/// rounded up — the window is sized to span twice the tolerance, at whatever wall-clock interval
/// separates two samples. This inverts that: given a window, a sample rate and a cadence, the `T`
/// the window is a window FOR.
///
/// The sample interval on a hash-lineage network is `PAST_MEDIAN_TIME_SAMPLE_INTERVAL` = 10 s. On a
/// V2 network `past_median_time_sample_rate` is 1 and a block is 120 s, so the same 27 samples span
/// 54 minutes and model a tolerance of 1,620 s — 12.3x the 132 s every preset declares. The window
/// is NOT the quantity to shrink: `AVERAGE_FRAME_SIZE` is 11, so a three-sample window is a median
/// any producer holding two of three recent blocks sets outright, which trades a wide backdating
/// band for a settable one.
///
/// Read at exactly one place in consensus (`pre_ghostdag_validation`'s
/// `unix_now() + timestamp_deviation_tolerance * 1000`), so what this corrects is the future-time
/// bound a V2 node applies, not the median rule.
pub fn palw_v2_timestamp_deviation_tolerance_v1(window_samples: usize, sample_rate: u64, target_time_per_block_ms: u64) -> u64 {
    (window_samples as u64) * sample_rate * (target_time_per_block_ms / 1000) / 2
}

pub fn palw_v2_params_on_base(
    base: Params,
    base_class_id: crate::Hash64,
    class_catalog_root: crate::Hash64,
    court_catalog_root: crate::Hash64,
    genesis_pwu_per_inference: u64,
    genesis_artifact_root: crate::Hash64,
    genesis_bonds: Vec<crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1>,
) -> Result<Params, crate::palw_mode_v2::PalwModeV2Error> {
    // **The devnet preset runs the lattice in minutes** (ADR-0077 Decision 7); every other base
    // runs testnet-11's windows. A preset property, not a flag: a node's windows are inside its
    // ruleset id, and a drill that has to reach `Final` and spend a quantum in one session needs
    // windows a multi-node chain can cross in a session.
    let windows = if base.net.network_type == crate::network::NetworkType::Devnet {
        &crate::palw_fp_devnet_v3::PALW_DEVNET_WINDOWS_V1
    } else {
        &crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1
    };
    // **ADR-0124 §9 / the operator's mainnet numbers: a card's producer floor is 13,000 MSK**, so
    // Decision 4's seat floor is 130,000 MSK. testnet-12 runs the mainnet-assumed bonds too (user
    // decision 2026-09-24). Chosen by network like the windows above: testnet-11 and devnet keep the
    // policy floor, and their bundles and fingerprints do not move.
    let min_collateral = if base.net.network_type == crate::network::NetworkType::Mainnet
        || (base.net.network_type == crate::network::NetworkType::Testnet && base.net.suffix == Some(12))
    {
        crate::palw_fp_devnet_v3::PALW_MAINNET_MIN_COLLATERAL_SOMPI
    } else {
        crate::palw_fp_devnet_v3::PALW_POLICY_MIN_COLLATERAL_SOMPI
    };
    let mut bundle = crate::palw_fp_devnet_v3::palw_fp_bundle_with_windows_and_floor_v3(
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        genesis_pwu_per_inference,
        genesis_artifact_root,
        genesis_bonds,
        windows,
        min_collateral,
    )?;
    // **A base that arms the Merkle prompt commitment names it in its ruleset id** (ADR-0081
    // Decision 3): `validate_palw_v2` at the bottom of this function refuses the fence without the
    // format and the format without the fence, so the two are written from one fact here.
    if base.palw_prompt_ids_merkle.is_some_and(|a| a != ForkActivation::never()) {
        bundle.trace_format_version = crate::palw_mode_v2::PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS;
    }
    // **And the signature-context set, from the same one fact and for the same reason** (ADR-0042
    // Decision 11; mainnet audit 2026-09-06, M-8). This is the ONE assembly both testnet-11 and a
    // carded mainnet pass through, so it is the one place where "which set did this network commit
    // to" can be answered once. A base that does not arm the fence keeps the frozen root the
    // bundle builder already wrote — which is what leaves testnet-11's and devnet's ruleset ids
    // exactly where they are.
    if base.palw_signature_contexts_v2.is_some_and(|a| a != ForkActivation::never()) {
        // **Which complete set — decided by what this network's OWN fences will need.**
        //
        // The root is frozen at genesis and lives inside `palw_ruleset_id_v2`, so it has to cover
        // every context the acceptance layer will ever verify under on this chain, not just the ones
        // it verifies under today. `validate_palw_v2` states both requirements by name: the one-move
        // court's accusation context needs at least COMPLETE_V3, and the held regime needs
        // COMPLETE_V4. Writing V2 unconditionally made "arm the shard court" and "arm the signature
        // set" two fences that refuse each other, which is the shape of a rule with nothing able to
        // obey it.
        //
        // Read from the fence rather than from the height: a fence scheduled for DAA 8,000 still
        // means this chain will verify that context, and the root cannot be re-minted at 8,000.
        let ever = |f: Option<ForkActivation>| f.is_some_and(|a| a != ForkActivation::never());
        bundle.signature_contexts_root = if ever(base.palw_rcore_plus) {
            // ADR-0152 IMPL-7: R-core+'s contexts are only in V5, and V5 is V4 plus them.
            crate::palw_mode_v2::palw_v2_signature_contexts_root_v5()
        } else if ever(base.palw_held_context) {
            crate::palw_mode_v2::palw_v2_signature_contexts_root_v4()
        } else if ever(base.palw_shard_court) {
            crate::palw_mode_v2::palw_v2_signature_contexts_root_v3()
        } else {
            crate::palw_mode_v2::palw_v2_signature_contexts_root_v2()
        };
    }
    // The bond's withdrawal delay is NOT rewritten here where the DA court is armed: it is resolved
    // at each block by `palw_v2_bond_withdrawal_delay_at_v1`, so the ruleset id (and therefore every
    // fingerprint and identity) stays what it would be without the fence. See that function for why
    // a live chain's fleet needs it that way.
    // **The free-prompt lane bears weight only on a certified class** (ADR-0074 Decision 6): a
    // shipped preset always carries the drilled set, so the gate is never absent on a network
    // that carries value. The test bundles stay ungated on purpose.
    bundle.state = bundle.state.with_fp_certified_classes(crate::palw_e2e_adjudicability::palw_rc_fp_certified_class_ids_v1());
    // The identity is the base's, in ONE place: `Params::from(testnet-12)` and this must not be
    // able to disagree about which genesis, cadence or activation set the RC network has — a
    // node whose bundled and bundle-less forms differ in anything but the bundle would be two
    // networks wearing one name.
    let mut params = base;
    // **The frozen cadence is part of the RULESET, so the assembly imposes it rather than hoping
    // the base already had it** (ADR-0038 Decision H). Every window inside the bundle is
    // DAA-denominated and the cadence is not in `palw_ruleset_id_v2`, so two networks could share
    // a ruleset id and run different rules in wall-clock terms; `validate_palw_v2` refuses that at
    // the bottom of this function. The RC base already carries this value, so this is a no-op
    // there — it exists for every OTHER base, where forgetting it is a startup panic rather than a
    // wrong number.
    //
    // For mainnet it is not a small change and should not read as one: mainnet runs 10 bps, and a
    // V2 network runs one block per 120 s. Enabling PALW on mainnet slows its cadence by that
    // factor, which is a consequence of the consensus and not a knob beside it.
    // **One spelling of "this network runs at 120 s", because there turned out to be two
    // assembly paths and only one of them had it** (mainnet audit item 6). See the method.
    params = params.with_two_minute_cadence();
    // **And the DAG shape that cadence implies, for the same reason and in the same place**
    // (mainnet audit, 2026-09-05).
    //
    // `with_two_minute_cadence` moves `blockrate` and leaves two quantities that are FUNCTIONS of
    // the cadence behind, so a V2 network assembled over a 10-bps base ran a shape nobody chose:
    //
    // * `difficulty_window_size`. Mainnet's 661 is `DIFFICULTY_WINDOW_DURATION / SAMPLE_INTERVAL`
    //   = 2641 s / 4, sized as ~264 SECONDS of difficulty memory at 10 bps. The cadence change
    //   sets `difficulty_sample_rate` to 1 and `target_time_per_block` to 120 s, at which the same
    //   661 is 661 x 120 s ~= 22 HOURS — a 300x change in the thing the constant was sized to be,
    //   arrived at by leaving it alone. testnet-11 and devnet both run 264 (~8.8 h), chosen for
    //   this cadence and stated as such beside `TESTNET_PARAMS`.
    // * `max_block_parents`. `with_two_minute_cadence` preserves a WIDENED parent count on purpose
    //   — a caller that raised it meant to — but MAINNET_PARAMS' 16 is `Bps::<10>`'s default and
    //   not a decision, so a carded mainnet ran 16 parents at `ghostdag_k = 1` where both networks
    //   that have actually run V2 run 10.
    //
    // Imposed here rather than on the presets for the reason the cadence itself is: this is the
    // one place a V2 ruleset is assembled, and a quantity restated per preset is a quantity that
    // drifts on the preset nobody re-read. A no-op on testnet-11 and devnet, which already carry
    // both values; the only ruleset it moves is a carded mainnet's, which has no chain to move.
    {
        let v2 = BlockrateParams::new_two_minute_bps();
        params.blockrate.max_block_parents = v2.max_block_parents;
        params.difficulty_window_size = PALW_V2_DIFFICULTY_WINDOW_SIZE;
        params.min_difficulty_window_size = PALW_V2_MIN_DIFFICULTY_WINDOW_SIZE;
    }
    // **And the MINT, because a V2 ruleset that is not minted at the ambient target cannot exist**
    // (mainnet audit 2026-09-06, M-7; the gate is `validate_palw_v2` at the bottom of this
    // function).
    //
    // ADR-0066: "a V2 network runs at MAX because the class lottery, not the hash target, is its
    // throttle", and ADR-0083 makes the retarget answer `max_difficulty_target` for a window with
    // no bits-priced row. Both PALW presets already mint there (`PALW_RC_GENESIS` and
    // `DEVNET_GENESIS` carry `bits: 0x207fffff`, which is exactly
    // `MAX_DIFFICULTY_TARGET.compact_target_bits()`), so this line is the identity on every
    // network that has ever run V2 and moves nothing: no genesis hash, no `consensus_params_id`,
    // no fingerprint.
    //
    // The one ruleset it moves is a carded mainnet's, and there it is the whole point. Carding is
    // a re-mint — the seats' collateral enters the premine — so `bits` is a value the ceremony
    // chooses, and left at `MAINNET_GENESIS`' hash-lineage `0x1f7fffff` it was 256x harder than a
    // V2 network's ambient maximum: the chain would be born needing 256x the inferences per block
    // for the whole `min_difficulty_window_size` launch window before one retarget could answer.
    // Until this line the card could not be assembled AT ALL — `mainnet_shipped_params` panicked
    // on this very gate — and the only write of the value in the tree was a `#[cfg(test)]` helper,
    // so every test over the mainnet base was rehearsing a mint production could not perform.
    //
    // Imposed HERE, beside the cadence and the windows, for the reason they are: this is the one
    // place a V2 ruleset is assembled, and a quantity restated per caller is a quantity that
    // drifts on the caller nobody re-read.
    //
    // **The ceremony consequence, stated because nothing runs a document:** the card's genesis
    // hash moves with this value, so `GENESIS.bits`, `GENESIS.hash` and `GENESIS.utxo_commitment`
    // in `config/genesis.rs` must be re-pinned in the same commit that fills the card, or
    // `set_genesis_utxo_commitment_from_config`'s M-07 assert refuses to boot. See
    // `repin::print_repinned_mainnet_card_genesis`.
    params.genesis.bits = params.max_difficulty_target.compact_target_bits();
    // **The depths a V2 network needs, derived from its own bundle in ONE place.** Restated by
    // hand anywhere else, they drift — which is how a params set assembled outside this function
    // ended up failing the very lattice bound this derivation exists to satisfy.
    params = params.with_palw_v2_depths(&bundle);
    // **A network that mints the HELD regime from genesis freezes the arity that regime derives**
    // (ADR-0082 Decision 3 / ADR-0103 Decision 5).
    //
    // `validate_palw_v2` refuses an armed `palw_kary_court` whose frozen `dissection_arity` is not
    // the one the ruleset derives — "the gate that admits a class would price it for one court and
    // the court that plays its dispute would deal another". Under the held regime no session plays
    // the leaf ladder, so the derivation runs with the ladder's rounds at zero and generally
    // answers something other than the binary arity every preset literal carries. Without this the
    // base assembly refuses the very ruleset `palw_held_context_mint_v1` is about to complete, and
    // the refusal names a re-mint that is already in progress.
    //
    // **Inert on every network that does not mint the regime at genesis**, which is every network
    // that exists: testnet-11 arms `palw_held_context` at a height, so `is_active(0)` is false and
    // this branch never runs; devnet, simnet and mainnet leave the fence dormant. No fingerprint
    // moves.
    if params.palw_held_context.is_some_and(|f| f != ForkActivation::never() && f.is_active(0)) {
        let derived = crate::palw_court_v2::palw_court_params_held_at_v2(&bundle, true, true).map_err(|_| {
            crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "the held regime is minted from genesis and no dissection arity fits this ruleset's court window with the                  ladder at zero (ADR-0103 Decision 5)",
            )
        })?;
        bundle.court = bundle.court.with_dissection_arity(derived.dissection_arity())?;
    }
    params.palw_consensus_mode = crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    // ADR-0132 §7.6: a base whose fences were armed before it had a bundle (the shipped preset
    // arms on `LegacyTn11` and assembles here) mirrors the short window's height into the bundle
    // now — the one place a V2 bundle is assembled, so the one place the copy can be re-made.
    params.sync_palw_short_challenge_window();
    // ADR-0133 §11.3, for the same reason and in the same place: the height and the lane's span
    // are re-mirrored onto the bundle this function just built.
    params.sync_palw_class_receipt_window();
    // Option A (2026-09-23 audit U2), for the same reason and in the same place: the height from which
    // a claim's escrow is reserved on its bond is `palw_audit_2026_09_23`'s, mirrored.
    params.sync_palw_escrow_backed_exposure();
    // ADR-0160 lane escrow, for the same reason and in the same place: F-E's height, mirrored onto the
    // bundle this function built (a base that set the fence before the bundle existed follows).
    params.sync_palw_capacity_escrow();
    // ADR-0152 R-core+, for the same reason and in the same place: the fence's height, the
    // processor's withdrawal delay and C7's list, mirrored onto the bundle this function built.
    params.sync_palw_rcore_plus();
    // ADR-0152 §4-quater, for the same reason and in the same place: the fence's height and its
    // measured rows, mirrored onto the bundle this function built.
    params.sync_palw_class_verify_deadline();
    // The readiness-V2 horizon (user decision 2026-09-25), for the same reason and in the same place:
    // its spans, mirrored onto the bundle this function built.
    params.sync_palw_readiness_v2_max_age_spans();
    // Lane F1's registry-resilience height (V03/V05), for the same reason and in the same place: a base
    // that set the fence before it had a bundle (the post-launch release arming its list after
    // `palw_t12_base_params`'s pass 2) had its mirror written into no bundle, and `validate_palw_v2`
    // below refused the ruleset — the int-4 phase-1 audit's LOW. Re-mirrored here, it follows.
    params.sync_palw_registry_resilience();
    // Lane cap-weight (ADR-0160 F-W), for the same reason and in the same place.
    params.sync_palw_capacity_weight_cap();
    // ADR-0160 lane liab (F-L), for the registry-resilience mirror's reason and in the same place: a
    // base that set the fence before it had a bundle has its value re-mirrored onto the one just built.
    params.sync_palw_capacity_liability();
    // ADR-0160 lane verify (F-B, F-R), for the registry-resilience reason: a base that set either
    // fence before it had a bundle has its mirrors written here.
    params.sync_palw_capacity_verify();
    // Lane F2's floor-refusal-retry height, for the same reason and in the same place.
    params.sync_palw_floor_refusal_retry();
    // RFC-0002 Phase F's IR fence, for the same reason and in the same place.
    params.sync_palw_tir_v1();
    // RFC-0003's generative fence, likewise.
    params.sync_palw_gen_v1();
    // RFC-0002 Phase F's second IR fence, likewise.
    params.sync_palw_tir_fence2();
    // ADR-0165's reserve fence, likewise.
    params.sync_palw_floor_reserve_v1();
    // ADR-0170's seed anchor window, likewise.
    params.sync_palw_anchor_window_v1();
    // Lane PA's audit-1004 fence, likewise.
    params.sync_palw_audit_1004_v1();
    // RFC-0004's improvement fence, likewise.
    params.sync_palw_improvement_v1();
    // RFC-0003's held leaf challenge, likewise.
    params.sync_palw_held_close_chunks_v1();
    // RFC-0003's generative range twin, likewise.
    params.sync_palw_gen_range_twin_v1();
    // The task-head profile, likewise.
    params.sync_palw_task_heads_v1();
    // RFC-0007's verification vertex, likewise.
    params.sync_palw_verification_vertex_v1();
    // RFC-0008 v2's EXEC payload, likewise.
    params.sync_palw_exec_payload_v2();
    // RFC-0007 Parts II and IV, likewise.
    params.sync_palw_witness_manifest_v1();
    params.sync_palw_audit_mesh_v1();
    params.sync_palw_capped_onboarding_v1();
    // RFC-0006's layer-sharded panels, likewise.
    params.sync_palw_tir_shard_v1();
    // RFC-0006's per-segment pricing, likewise.
    params.sync_palw_tir_shard_segment_v2();
    // ADR-0032's amended reporter share (lane INTF), likewise.
    params.sync_palw_reporter_share_v2();
    // The legacy route's public filer (lane LG14-A), likewise.
    params.sync_palw_legacy_public_filer_v1();
    // The 2026-10-12 rulings' fences (X3, N4, D2), likewise.
    params.sync_palw_2026_10_12_ruling_fences_v1();
    // RFC-0001 §2.10's adapter class listing, likewise.
    params.sync_palw_adapter_class_v1();
    // ADR-0096's decode constraint, likewise.
    params.sync_palw_fp_decode_constraint_v1();
    // RFC-0002 Part II's class-seating fence, likewise.
    params.sync_palw_class_seating();
    // RFC-0002 Phase H's IR-only fence, likewise.
    params.sync_palw_tir_only_v1();
    params.validate_palw_v2()?;
    Ok(params)
}

pub const DEVNET_PARAMS: Params = Params {
    // kaspa-pq: PQ-only enforcement from genesis (ADR-0019).
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020 activation prep (O13 sandbox stage): EVM lane GENESIS-ACTIVE on
    // devnet — every post-genesis header is v2 with the two EVM commitments,
    // so the live mesh exercises the full lane (relay e2e / C4 / C5 / Y10).
    // NOT a genesis-block input (genesis hash unchanged), but the version
    // fork-gate invalidates every v1 block => barrier re-genesis of the mesh,
    // and devnet kaspad MUST be built `--features evm` (a non-evm build
    // refuses evm-active blocks by design). Mainnet/testnet/simnet stay
    // u64::MAX-inert until the O13/O9 decision.
    evm_activation_daa_score: 0,
    // EVM is genesis-active here, but the gas-pool v2 executor stays inert until a
    // deploy sets a finite activation score (consensus fork — see params docs).
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_bridge_ledger_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
    // kaspa-pq: devnet now uses the same MISAKA DNS seeders as mainnet/testnet for automatic
    // peer discovery (devnet default P2P port is 26611, matching the live mesh — see
    // NetworkId::default_p2p_port). Nodes launched WITHOUT `--nodnsseed` resolve these to find
    // peers; the seeders' A records (160.16.131.119 / 95.111.236.186) run devnet nodes on 26611.
    // dns_seeders is NOT a genesis-block input, so the genesis hash is unchanged (no re-genesis).
    dns_seeders: &[
        "seeder1.misakascan.com",
        "seeder2.misakascan.com",
        "seeder3.misakascan.com",
        "seeder4.misakascan.com",
        "seeder1.misakachain.com",
        "seeder2.misakachain.com",
        "seeder3.misakachain.com",
    ],
    net: NetworkId::new(NetworkType::Devnet),
    genesis: DEVNET_GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    past_median_time_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    // PALW 0.1 bps: with `difficulty_sample_rate = 1` (every block sampled) the window size IS the
    // window duration in blocks. 264 blocks × 10 s ≈ the same 2 641 s DAA window duration the
    // sampled 661-slot window models on ≥1-bps networks; keeping 661 here would slow difficulty
    // response to ~1.8 h of chain time for no extra fidelity.
    difficulty_window_size: 264,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    max_signature_script_len: 16_384,
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    // kaspa-pq Phase 7 (ML-DSA-87 verify recalibration; supersedes the
    // Phase-6 ML-DSA-87 numbers). Measured on Apple Silicon arm64 via
    // `crypto/txscript/benches/bench.rs` (ml_dsa_87::verify):
    //   Schnorr verify (secp256k1):              12.74 µs
    //   ML-DSA-87 verify (default, NEON/AVX2):   63.88 µs  (5.01× ratio)
    //   ML-DSA-87 verify (libcrux portable):     76.52 µs  (6.01× ratio — slowest)
    //
    // Per `docs/adr/0005-mass-policy.md` §"Calibration formula" the
    // value is calibrated against the slowest variant so that no-SIMD
    // low-end reference platforms remain safely budgeted:
    //   1000 (upstream) × 6.01 (slowest ratio) × 1.59 (safety) = 9548 → 10_000.
    mass_per_sig_op: 10000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    // kaspa-pq emission: decay table applies from genesis (see MAINNET_PARAMS).
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 370468345,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,

    // PALW LLM PoW runs at 0.1 bps — one block per 10 s, sized so one deterministic
    // Qwen3.5-2B inference (~1-3 s/attempt) is a meaningful fraction of the block interval.
    blockrate: BlockrateParams::new_deci_bps(),

    pre_crescendo_target_time_per_block: 100,

    crescendo_activation: ForkActivation::always(),
    // kaspa-pq DNS-finality PoS overlay — GENESIS-ACTIVE on devnet (see GENESIS_ACTIVE_DNS_PARAMS:
    // `dns_activation_daa_score = 0`, so the rollout reaches `Active` once stake bonds and the
    // TwoDimensionalDominance reorg gate engages — NOT visibility-only). Devnet shares simnet's
    // fully-active config, and the same shape as mainnet/testnet's PRODUCTION_DNS_PARAMS minus the
    // 20M-KAS stake/bond minimums and 14-day evidence/unbonding windows. Full Stage-3 reward split
    // from genesis (`full_reward_split_daa_score = 0`); the PoS-v2 "本格版" economics stay fenced
    // (`pos_v2_activation_daa_score = u64::MAX`). The small epoch/window (epoch 100, reorg/evidence
    // 300, unbond 700, reward 600 — consistent with U ≥ R+E) keep the PR-10.11-throttled StakeScore
    // aggregation walk cheap on the devnet (amortized O(1) per block). NOTE (PALW 0.1 bps): these
    // windows are BLOCK counts, so at one block per 10 s their wall-clock stretches 100× vs the
    // old 10-bps devnet (epoch 100 blocks ≈ 17 min, unbond 700 ≈ ~2 h) — the U ≥ R+E shape and
    // every consensus invariant are unchanged, but VLT harness timings must budget for it.
    dns_params: Some(GENESIS_ACTIVE_DNS_PARAMS),
    palw_credit: None,
    palw_fork_choice: None,
    palw_schedule: None,
    palw_ramp: None,
    palw_block_commitment: None,
    palw_bootstrap_activation: None,
    palw_unavailable_abstains: None,
    palw_bond_maturity: None,
    palw_bond_maturity_early: None,
    palw_frontier_provenance: None,
    palw_reorg_strict_economic_win: None,         // lane: rcore/f1-forkchoice-attacks
    palw_pruning_proof_strict_economic_win: None, // lane: rcore/hf-pptake2
    palw_capacity_weight_cap: None,               // lane: rcore/cap-weight (ADR-0160 F-W)
    // **ADR-0068 Phase 1, armed on the drill network and nowhere else.** Devnet is the network
    // these fences exist to be drilled on: the heartbeat lane (fixed 2^24-hash price, width-bounded
    // mergesets) and the attempt lane's constant blue work, both live from genesis. Every other
    // preset ships None — arming them is a per-network activation decision, and this preset IS
    // that decision for the local drill. The values are locked to the pow_layer0 constants by
    // `validate_palw_v2`, so the fence can only declare what this binary enforces.
    palw_heartbeat: Some(PalwHeartbeatV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2,
        max_per_mergeset: crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET,
    }),
    palw_attempt_work: Some(PalwAttemptWorkV1 {
        activation: ForkActivation::always(),
        work_log2: crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2,
        ticket_bucket_log2: crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2,
    }),
    // ADR-0072 ships dormant here too: the drill devnet was re-minted onto the execution-priced
    // rule, so it has no pre-ADR-0072 history for a fence to protect.
    palw_attempt_activation: None,
    dns_bft_gate: None,
    palw_inactivity_leak: None,
    palw_beacon_fold: None,
    palw_capability_bound: None,
    palw_compute_overlay_retired: None,
    palw_model_registry: None,
    palw_economic_payout: None,
    palw_work_target: None,
    palw_single_lottery: None,
    palw_short_challenge_window: None,
    palw_verification_v2: None,
    palw_verification_s3: None,
    palw_verification_s2: None,
    palw_readiness_v2: None,
    palw_anchor_clock: None,
    palw_clock_cursor: None,
    palw_clock_floor: None,
    palw_clock_lead_cap: None,
    palw_offence_attribution: None,
    palw_rcore_plus: None,
    palw_panel_seed_execution: None,
    palw_operator_anchor: None,
    palw_capacity_aggregate_liability: None,
    palw_rcore_conservative_classes: &[],
    palw_final_lock_full_collateral: None,
    palw_final_lock_life: None,
    palw_final_lock_life_retro: None,
    palw_class_verify_deadline: None,
    palw_class_verify_rows: &[],
    palw_anchor_at_ceiling: None,
    palw_lane_accept_parents_first: None,
    palw_activation_pool: None,
    palw_readiness_v2_max_age_spans: None,
    palw_registry_resilience: None,
    palw_floor_refusal_retry: None,
    palw_artifact_root_ownership: None,
    palw_operator_id_unique: None,
    palw_objective_offence: None,
    palw_seat_gate_possession: None,
    palw_class_receipt_window: None,
    palw_execution_quanta: None,
    palw_public_model_source_required: None,
    palw_economic_safety: None,
    palw_canonical_work: None,
    palw_admission_independence: None,
    // **ARMED on devnet at genesis** — the testnet-11 5f genesis card §1's set, rehearsed here
    // first. Without the ladder the registered class cannot price a wide row, and the ladder is
    // the whole point of registering the graph-v5 512 row (`misaka_palw_base0::classes`).
    //
    // **Arming the ladder is THREE moves, not two** (card §1; corrected by the mainnet audit of
    // 2026-09-06, M-15): this field, the bundle's `PalwCourtParamsV2::max_step_leaf_count`, and —
    // since the DA-court/model-market merge minted a second fence for it — `palw_court_ladder`
    // below, which is what the processor's `palw_court_step_ladder_at` actually reads. The second
    // is already `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` = 2^26 on this base (2^22 admits n_ctx 39).
    // The THIRD is dormant here and this is a KNOWN GAP, not an oversight: devnet is a shipped
    // preset with a pinned fingerprint, so arming it is a re-mint and the operator's call. Until
    // then devnet admits the graph-v5 row at 2^26 and its court walks it at 2^22 — pinned by
    // `the_refutation_ladder_is_the_rulesets_only_past_the_court_ladder_fence`, which asserts the
    // gap so it can neither close nor widen in silence.
    palw_context_ladder: Some(ForkActivation::always()),
    // ADR-0077 Decision 16: `PanelDa` is dormant. A prompt that stays off chain is a mode a
    // network arms on purpose, never one it acquires by upgrading.
    palw_panel_da: None,
    palw_certification_rent: None,
    // **ADR-0069 Decision 7, ARMED at genesis** — and `always()` is the only legal value:
    // `validate_palw_v2` refuses this fence at any other height, because the rule it carries
    // decides what an uncertified class WEIGHS and a chain cannot change that opinion about
    // history it already accepted. Genesis is the only moment it can be armed.
    palw_uncertified_weightless: Some(ForkActivation::always()),
    palw_da_court: None,
    // ADR-0084 U-08: dormant, and see the note on `palw_context_ladder` above for why devnet
    // carries one half of the ladder and not the other.
    palw_court_ladder: None,
    palw_model_market: None,
    palw_model_lines: None,
    palw_model_immutable_v1: None,
    palw_class_semantics_v1: None,
    palw_court_commitment_scope_v1: None,
    palw_staged_public_court_v1: None,
    palw_model_benefits: None,
    palw_model_leg_v2: None,
    palw_model_seed_v2: None,
    // ADR-0162: dormant on every preset.
    palw_model_virtual_v1: None,
    palw_model_evm: None,
    palw_model_sink_bound: None,
    palw_capacity_batch_licence: None,
    palw_capacity_verify_room: None,
    palw_capacity_audit_door: None,
    palw_capacity_issuance_slots: None,
    palw_capacity_network_room: None,
    palw_capacity_network_verify: None,
    palw_panel_unavailable_expiry: None,
    palw_panel_fast_switch: None,
    palw_seat_availability: None,
    palw_capacity_emission_budget: None,
    palw_capacity_multi_claim: None,
    palw_capacity_rho_breaker: None,
    palw_chunk_cap_charge: None,
    // ADR-0082 Decision 5: NOT armed. At the registered 512 row the flat prompt ids are 82,080
    // bytes against a one-carrier budget of 83,333, so the Merkle form buys nothing and arming it
    // would move every free-prompt job id. It becomes REQUIRED above about n_ctx 1,024, which is
    // the day a wider row registers — not before.
    palw_prompt_ids_merkle: None,
    // **ADR-0082 Decision 3, ARMED at genesis.** A bare fence: no companion value, no bundle
    // field. The preset's `dissection_arity` stays 2 and the fence overrides it with
    // `palw_court_arity_v1` of this ruleset's own quantities. Without it the graph-v5 row is
    // admitted and UNPROSECUTABLE — `verify_class_admission_v5` refuses a fused profile under a
    // dormant fence by name (`FusedAttentionNeedsTheKaryCourt`), which is the guard; the property
    // is that a dispute reaches a verdict, and only the court drill asserts that.
    palw_kary_court: Some(ForkActivation::always()),
    palw_model_court_window: None,
    // **NOT armed on devnet, deliberately** (mainnet audit 2026-09-06, C-2/H-5): devnet is the
    // drill network for the dissection court over a fused row, and the exemption would change what
    // its fold does at a terminal. It is the network the responder will be built against.
    palw_court_responder_coverage: None,
    // ADR-0082 stream H's (decode leaves earn, seeded argmax): NOT armed. Not a prosecutability
    // condition and not on the acceptance path, and its transition arm still refuses by name
    // (`FreePromptDecodeLeavesUnavailable`) for want of the profile-derived split quantities on
    // `PalwClassStateV2`. A fence armed without a shipping thing to obey it is the ADR-0065 D1
    // mistake.
    palw_fp_decode_rules: None,
    palw_fp_derived_work: None,
    palw_fp_decode_constraint: None,
    palw_shard_court: None,
    palw_shard_licensing: None,
    palw_token_lift: None,
    palw_tir_v1: None,
    palw_gen_v1: None,
    palw_fp_job_v5: None,
    palw_gen_range_twin_v1: None,
    palw_task_heads_v1: None,
    palw_probabilistic_constraints_v1: None,
    palw_fork_choice_rule_e_v1: None,
    palw_typed_roots_v1: None,
    palw_fork_choice_commitment_v1: None,
    palw_verifier_pay_v1: None,
    palw_model_refutation_seal_v1: None,
    palw_beacon_source_opt_in_v1: None,
    palw_beacon_source_fee_v1: None,
    palw_panel_free_v1: None,
    palw_signed_registration_v1: None,
    palw_tir_shard_segment_v2: None,
    palw_provider_court_v1: None,
    palw_legacy_held_da_v2: None,
    palw_bond_budget_v1: None,
    palw_reporter_income_v2: None,
    palw_material_income_v3: None,
    palw_panel_lifetime_v2: None,
    palw_claim_challenge_v2: None,
    palw_monetary_material_v1: None,
    palw_model_verification_gate_v1: None,
    palw_horizon_extension_v1: None,
    palw_minimum_price_v1: None,
    palw_beacon_bootstrap_v1: None,
    palw_paid_consumer_lifecycle_v1: None,
    palw_model_bond_allocation_v1: None,
    palw_reporter_share_v2: None,
    palw_legacy_public_filer_v1: None,
    palw_court_model_scope_v1: None,
    palw_public_evidence_reward_v1: None,
    palw_claim_hard_deadline_v1: None,
    palw_fp_prefix_inherit: None,
    palw_fp_prefix_state: None,
    palw_fp_tokenizer_match: None,
    palw_fp_constraint_v2: None,
    palw_adapter_class_v1: None,
    palw_tir_fence2: None,
    palw_tir_shard_v1: None,
    palw_floor_reserve_v1: None,
    palw_real_clock_tick_v1: None,
    palw_anchor_window_v1: None,
    palw_receipt_spend_v4: None,
    palw_evidence_court_v1: None,
    palw_audit_1004_v1: None,
    palw_improvement_v1: None,
    palw_held_close_chunks_v1: None,
    palw_verification_vertex_v1: None,
    palw_exec_payload_v2: None,
    palw_witness_manifest_v1: None,
    palw_audit_mesh_v1: None,
    palw_capped_onboarding_v1: None,
    palw_class_seating: None,
    palw_tir_only_v1: None,
    palw_kimi_k3: None,
    palw_fused_dissectable: None,
    palw_attn_anchored_root: None,
    palw_held_context: None,
    palw_prefill_draw: None,
    palw_audit_2026_09_11: None,
    palw_audit_2026_09_11_deep: None,
    palw_audit_2026_09_23: None,
    palw_settled_anchor_depth: None,
    palw_admission_audit_period_daa: None,
    palw_exec_quantum_maturity_daa: None,
    palw_difficulty_priced_rows: None,
    palw_receipt_rows_unpriced: None,
    palw_attempt_header_pins: None,
    palw_signature_contexts_v2: None,
    palw_fp_da_pins: None,
    palw_validator_payout_bounds: None,
    palw_slashing_evidence_utxo_genuine: None,
    palw_epoch_boundary_budget: None,
    palw_epoch_budget_release: None,
    palw_panel_economy: None,
    palw_work_priced_reward: None,
    palw_execution_lane: None,
    palw_overlay_carve: None,
    palw_dns_retirement: None,
    palw_permissionless_panel_v1: None,
    palw_permissionless_panel_complete_v1: None,
    palw_permissionless_panel_retired_v1: None,
    palw_panel_exposure_floor: None,
    palw_fp_ruleset_caps: None,
    // ADR-0105: dormant on every shipped preset (see the field's doc).
    palw_heartbeat_transparent: None,
    // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency): dormant on every preset.
    palw_heartbeat_transparent_same_chain: None,
    palw_capacity_escrow_at_licence: None,
    // ADR-0107: dormant on every shipped preset (see the field's doc).
    palw_share_growth_final: None,
    palw_gdn_key_heads: None,
    palw_consensus_mode: crate::palw_mode_v2::PalwConsensusMode::Disabled,
    pow_blake2b_sha3_activation: ForkActivation::never(),
    // **Devnet is the ADR-0068 drill network on this branch: ConsensusV2, so no V1 PALW
    // proof-of-work.** `validate_palw_v2` refuses a V1 PoW fence
    // beside a V2 ruleset, and `devnet_shipped_params` installs the V2 bundle on this base.
    // The const alone is therefore the BASE identity (hash-only), exactly the shape
    // `palw_rc_base_params` has for testnet-11; what a node actually runs is
    // `Params::from(Devnet)` = `devnet_shipped_params()`, the bundled form.
    pow_palw_activation: ForkActivation::never(),
    pow_palw_ollama_activation: ForkActivation::never(),
};

#[cfg(test)]
mod consensus_params_id_tests;

#[cfg(test)]
mod fingerprint_probe {
    use super::*;

    /// The probe the deploy card asks for: does the fingerprint a NODE announces equal the one
    /// this crate pins? The daemon takes `NetworkId` → `Params` (`daemon.rs:410`) and nothing in
    /// its builder chain mutates `params`, so these must be the same value — and if they are not,
    /// the pin describes a configuration no node runs.
    #[test]
    fn the_pinned_testnet_fingerprint_is_the_one_a_node_announces() {
        let from_const = TESTNET_PARAMS.consensus_params_id().to_string();
        let via_preset_net = Params::from(TESTNET_PARAMS.net).consensus_params_id().to_string();
        let as_the_daemon_builds_it = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10)).consensus_params_id().to_string();
        println!("const                 : {from_const}");
        println!("Params::from(net)     : {via_preset_net}");
        println!("Params::from(id{{10}})  : {as_the_daemon_builds_it}");
        println!("TESTNET_PARAMS.net    : {:?}", TESTNET_PARAMS.net);
        assert_eq!(
            via_preset_net, as_the_daemon_builds_it,
            "the preset's own NetworkId must reach the same match arm the daemon's does"
        );
    }

    /// **The PALW-RC network can be NAMED — the daemon's own path used to abort on it.**
    ///
    /// `palw_rc_params` stamps `NetworkId::with_suffix(Testnet, 12)` into its own `net`, and
    /// `From<NetworkId> for Params` panicked on every testnet suffix but 10 and 11. So the RC
    /// identity could be assembled inside a test and could not be constructed by the process that
    /// runs it: `kaspad` does `let params: Params = network.into()` before anything else, and the
    /// wallet, the CLI and the validator all take the same route.
    ///
    /// What the arm returns is the identity, NOT the ruleset — the bundle is a function of genesis
    /// artifacts no `From` impl holds. The last assertion is the one that makes that safe.
    #[test]
    fn the_palw_rc_network_id_builds_params_instead_of_aborting() {
        let id = NetworkId::with_suffix(NetworkType::Testnet, 11);
        let base = Params::from(id);

        assert_eq!(base.net, id, "the params a node builds name the network it asked for");
        assert_eq!(base.genesis.hash, crate::config::genesis::PALW_RC_GENESIS.hash, "the RC genesis, not testnet-10's");
        assert_eq!(
            base.blockrate.target_time_per_block,
            crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS,
            "the frozen PALW cadence — the first of the two reasons this is a new identity"
        );
        for (name, activation) in [
            ("blake2b_sha3", base.pow_blake2b_sha3_activation),
            ("palw v1", base.pow_palw_activation),
            ("palw ollama", base.pow_palw_ollama_activation),
        ] {
            assert!(!activation.is_active(u64::MAX - 1), "{name} must never activate on a V2 identity");
        }
        // **The seeder names are SHARED with mainnet, testnet-10 and devnet, and that is not a
        // mistake to fix.** This assertion used to demand the list be empty, on the reasoning that
        // inheriting another network's seeders would look configured and find nobody. The names
        // are inherited — there are only four delegated `misakascan.com` seeder records and every
        // network ships them — so what actually keeps that safe is the port, not the name: a
        // record hands out an IP, and each network dials it on ITS OWN default P2P port. A
        // testnet-11 peer answers on 26311 and is simply not there on 26111/26211/26611, and if it
        // were, `consensus_params_id` refuses the handshake. Shared name, disjoint port, and the
        // failure direction is "finds nobody", never "peers with the wrong chain".
        //
        // So the property worth asserting is the disjoint port, plus that every name shipped here
        // is one the project already delegates rather than a new name nobody owns.
        assert!(!base.dns_seeders.is_empty(), "a public network ships discovery names");
        let delegated: std::collections::HashSet<&str> =
            MAINNET_PARAMS.dns_seeders.iter().chain(TESTNET_PARAMS.dns_seeders.iter()).copied().collect();
        for name in base.dns_seeders {
            assert!(delegated.contains(name), "{name} is not one of the delegated records — nothing would resolve it");
        }
        for other in [NetworkType::Mainnet, NetworkType::Devnet] {
            assert_ne!(
                NetworkId::new(other).default_p2p_port(),
                id.default_p2p_port(),
                "{other:?} shares these seeder names, so its port must differ or a record would cross networks"
            );
        }
        assert_ne!(
            NetworkId::with_suffix(NetworkType::Testnet, 10).default_p2p_port(),
            id.default_p2p_port(),
            "testnet-10 shares these seeder names, so its port must differ or a record would cross networks"
        );
        assert!(base.palw_credit.is_none(), "and no V1 fence, which `validate_palw_v2` would refuse anyway");
        // **Since the card was minted (2026-08-22) `Params::from` DOES carry the bundle**, because
        // `palw_rc_shipped_params` installs it once `PALW_RC_GENESIS_BONDS` is non-empty. This
        // assertion used to demand the opposite, and it was right for a bundle-free build: the
        // identity (genesis, cadence, activations, seeders) is what the probe measures, and that
        // half is unchanged either way. Asserted as the CARD's state rather than as an absence, so
        // it keeps meaning something in both worlds.
        assert_eq!(
            matches!(base.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)),
            palw_rc_genesis_card_is_set(),
            "the bundle rides `Params::from` exactly when the shipped card is filled in"
        );

        // The bundled form is the SAME identity plus the ruleset: one source for genesis, cadence
        // and activations, so the two forms cannot come to disagree about which network they are.
        let bundled = palw_rc_params_from_artifacts(
            crate::Hash64::from_u64_word(0xA7),
            // A registry sized to seat the panel and to outlast the bind window, because the
            // genesis gate refuses anything less — the probe measures the identity, not the card.
            (0..crate::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1() as u32)
                .map(|i| crate::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
                    bond: crate::palw_state_v2::PalwBondKeyV2(crate::config::premine::premine_outpoint(i)),
                    pubkey: vec![7u8.wrapping_add(i as u8); 32],
                    operator_pubkey: vec![21u8, i as u8, 0, 0, 0, 0, 0, 0],
                    payout_payload: crate::Hash64::from_u64_word(0x9A11 + i as u64),
                })
                .collect(),
        )
        .expect("the RC artifacts assemble");
        assert_eq!(bundled.net, base.net);
        assert_eq!(bundled.genesis.hash, base.genesis.hash);
        assert_eq!(bundled.blockrate.target_time_per_block, base.blockrate.target_time_per_block);
        assert_eq!(bundled.dns_seeders, base.dns_seeders);

        // **The fail-safe.** The bundle is inside `consensus_params_id`, so a node that boots this
        // identity WITHOUT the artifacts fingerprints differently and is refused at the handshake
        // — it cannot silently join the RC network on a ruleset that has no classes, no bonds and
        // no court. Making the arm exist must not make that mistake quiet.
        assert_ne!(
            base.consensus_params_id(),
            bundled.consensus_params_id(),
            "an artifact-less node must be a DIFFERENT ruleset, visibly, at the handshake"
        );
    }
}

/// **ADR-0125 §7.2: the lane's stage table.** A span keeps the width in force at the DAA score it
/// opens with; widenings are heights after the opening, in order, each wider than the last; and a
/// widening's height is scheduled like any fence while its width is part of the rules.
#[cfg(test)]
mod palw_execution_lane_stage_tests {
    use super::*;

    fn lane(widenings: &[(u64, u16)]) -> PalwExecutionLaneV1 {
        let mut table = PalwExecutionLaneV1::NO_WIDENINGS;
        for (slot, (height, width)) in widenings.iter().enumerate() {
            table[slot] = PalwExecWideningV1 { activation: ForkActivation::new(*height), permits_per_round: *width };
        }
        PalwExecutionLaneV1 {
            activation: ForkActivation::new(500),
            permits_per_round: 1,
            widenings: table,
            max_per_mergeset: 600,
            schedule_span_daa: 100,
            short_span: PalwExecSpanShortV1::NONE,
        }
    }

    fn armed(lane: PalwExecutionLaneV1) -> Params {
        let mut params = palw_rc_shipped_params();
        params.palw_execution_lane = Some(lane);
        params
    }

    #[test]
    fn a_span_keeps_the_width_in_force_where_it_opens() {
        let lane = lane(&[(1_000, 2), (2_050, 5), (3_000, 10)]);
        assert_eq!(lane.width_of_span(0), 1);
        assert_eq!(lane.width_of_span(9), 1, "span 9 opens at 900, before the first widening");
        assert_eq!(lane.width_of_span(10), 2, "span 10 opens at 1,000, exactly the widening's height");
        assert_eq!(lane.width_of_span(20), 2, "span 20 opens at 2,000 — the widening at 2,050 falls inside it");
        assert_eq!(lane.width_of_span(21), 5, "and the span after it is the first to widen");
        assert_eq!(lane.width_of_span(30), 10);
        assert_eq!(lane.width_of_span(u64::MAX), 10, "no overflow at the far end");
        assert_eq!(lane.width_at_daa(2_099), 2, "a DAA score reads its span's width, not the stage in force at it");
        assert_eq!(lane.widest(), 10);
        armed(lane).validate_palw_v2().expect("a widening table in order is runnable");
        armed(self::lane(&[])).validate_palw_v2().expect("a lane with no widening is runnable");
    }

    #[test]
    fn an_unrunnable_stage_table_refuses_to_start() {
        let refused = |table: &[(u64, u16)], why: &str| {
            assert!(armed(lane(table)).validate_palw_v2().is_err(), "{why}");
        };
        refused(&[(400, 2)], "a widening before the lane opens");
        refused(&[(500, 2)], "a widening at the opening height");
        refused(&[(1_000, 3), (900, 5)], "widenings out of height order");
        refused(&[(1_000, 3), (1_000, 5)], "two widenings at one height");
        refused(&[(1_000, 1)], "a widening that does not widen");
        refused(&[(1_000, 3), (2_000, 2)], "a narrowing");
        refused(&[(1_000, 11)], "a round wider than the binary implements");
        let mut gap = lane(&[(1_000, 2), (2_000, 3)]);
        gap.widenings[0] = PalwExecWideningV1::NONE;
        assert!(armed(gap).validate_palw_v2().is_err(), "a used slot after an unused one");
        let mut tight = lane(&[(1_000, 10)]);
        tight.max_per_mergeset = 9;
        assert!(armed(tight).validate_palw_v2().is_err(), "a mergeset bound below the widest round");
    }

    #[test]
    fn a_widening_is_a_scheduled_fence_and_its_width_is_reported_with_it() {
        let base = armed(lane(&[(1_000, 2)]));
        let later = armed(lane(&[(1_100, 2)]));
        let wider = armed(lane(&[(1_000, 3)]));
        let none = armed(lane(&[]));
        assert_ne!(base.consensus_params_id(), none.consensus_params_id(), "a widening reaches the fingerprint");
        assert_ne!(base.consensus_params_id(), later.consensus_params_id(), "its height reaches the fingerprint");
        assert_ne!(base.consensus_params_id(), wider.consensus_params_id(), "so does its width");
        assert_ne!(base.consensus_schedule_id(), later.consensus_schedule_id(), "the schedule names the height");
        assert_ne!(base.consensus_schedule_id(), wider.consensus_schedule_id(), "and reports the width");
        // Scheduling a widening must not partition the fleet at deploy: a height that has not fired
        // leaves the identity, and its width leaves with it.
        assert_eq!(base.consensus_identity_id(), later.consensus_identity_id());
        assert_eq!(base.consensus_identity_id(), wider.consensus_identity_id());
        assert_eq!(base.consensus_identity_id(), none.consensus_identity_id());
        assert!(base.fence_schedule_v1().contains(&1_000), "the fork-id schedule names the widening's height");
        assert!(
            base.palw_fences_v1()
                .iter()
                .any(|(name, fence)| *name == "palw_execution_lane_widening_1" && *fence == Some(ForkActivation::new(1_000))),
            "and so does the named fence list the gate reads"
        );
        assert!(
            none.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_execution_lane_widening_1" && fence.is_none()),
            "an unused slot names no height"
        );
    }

    #[test]
    fn a_short_span_is_a_scheduled_fence_and_keeps_the_identity() {
        let opening = lane(&[]);
        let shortened = PalwExecutionLaneV1 {
            short_span: PalwExecSpanShortV1 { activation: ForkActivation::new(1_000), schedule_span_daa: 1 },
            ..opening
        };
        let later = PalwExecutionLaneV1 {
            short_span: PalwExecSpanShortV1 { activation: ForkActivation::new(1_100), schedule_span_daa: 1 },
            ..opening
        };
        let longer = PalwExecutionLaneV1 {
            short_span: PalwExecSpanShortV1 { activation: ForkActivation::new(1_000), schedule_span_daa: 2 },
            ..opening
        };
        let base = armed(shortened);
        assert_eq!(shortened.schedule_span_daa_at(999), 100);
        assert_eq!(shortened.schedule_span_daa_at(1_000), 1);
        assert_ne!(base.consensus_params_id(), armed(opening).consensus_params_id(), "the shortening reaches the fingerprint");
        assert_ne!(base.consensus_params_id(), armed(later).consensus_params_id(), "its height reaches the fingerprint");
        assert_ne!(base.consensus_params_id(), armed(longer).consensus_params_id(), "so does its length");
        assert_ne!(base.consensus_schedule_id(), armed(later).consensus_schedule_id(), "the schedule names the height");
        assert_eq!(base.consensus_identity_id(), armed(opening).consensus_identity_id(), "a scheduled shortening leaves the identity");
        assert_eq!(base.consensus_identity_id(), armed(later).consensus_identity_id());
        assert!(base.fence_schedule_v1().contains(&1_000));
        assert!(
            base.palw_fences_v1()
                .iter()
                .any(|(name, fence)| *name == "palw_execution_lane_span_short" && *fence == Some(ForkActivation::new(1_000)))
        );
        armed(shortened).validate_palw_v2().expect("a shorter span after the opening is runnable");
        let mut too_wide = shortened;
        too_wide.short_span.schedule_span_daa = 100;
        assert!(armed(too_wide).validate_palw_v2().is_err(), "not a shortening");
        let mut too_early = shortened;
        too_early.short_span.activation = ForkActivation::new(500);
        assert!(armed(too_early).validate_palw_v2().is_err(), "at the opening height");
    }
}

/// **ADR-0126: the validator carve drops to a fifth.** The split one reader resolves, testnet-11's
/// own numbers through it, the four refusals and the fence's place in the three ids.
#[cfg(test)]
mod palw_overlay_carve_tests;

#[cfg(test)]
mod palw_model_registry_fence_tests;

#[cfg(test)]
mod adr0142_release_probe {
    /// **What the shipped preset actually schedules at or past the boundary**, printed so a release
    /// is checked against the build rather than against a page. Named as a probe, not a rule: it
    /// asserts the DAA-clock trio (ADR-0138 / ADR-0142) is scheduled at
    /// [`PALW_RC_ANCHOR_CLOCK_FENCE_DAA`], and prints the rest.
    #[test]
    fn the_flag_day_carries_what_the_announcement_says_it_carries() {
        let p = super::palw_rc_shipped_params();
        let mut rows: Vec<(&str, u64)> = p
            .palw_fences_v1()
            .into_iter()
            .filter_map(|(n, f)| f.filter(|a| *a != super::ForkActivation::never()).map(|a| (n, a.daa_score())))
            .filter(|(_, h)| *h >= super::PALW_RC_AUDIT_DEEP_FENCE_DAA)
            .collect();
        rows.sort_by_key(|(_, h)| *h);
        for (n, h) in &rows {
            println!("PROBE {h:>6}  {n}");
        }
        for (held, height) in [
            ("palw_single_lottery", super::PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
            ("palw_anchor_clock", super::PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
            ("palw_clock_cursor", super::PALW_RC_ANCHOR_CLOCK_FENCE_DAA),
            ("palw_execution_quanta", super::PALW_RC_EXECUTION_QUANTA_FENCE_DAA),
            ("palw_objective_offence", super::PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA),
        ] {
            assert_eq!(rows.iter().find(|(n, _)| *n == held).map(|(_, h)| *h), Some(height), "{held} is scheduled at {height}");
        }
        assert!(rows.iter().any(|(n, _)| *n == "palw_work_target"), "the work target keeps the day");
        assert!(rows.iter().any(|(n, _)| *n == "palw_model_registry"), "and so does the registry");
    }
}

/// **testnet-12's post-launch release, armed** (int-4; the user's decision of 2026-09-26): every entry of
/// [`PALW_T12_POST_LAUNCH_FENCES_V1`] at [`PALW_T12_POST_LAUNCH_FENCE_DAA`] (750) on the shipped ruleset,
/// and set back it is the launch release to the id.
#[cfg(test)]
mod post_launch_fence_arming_tests;
