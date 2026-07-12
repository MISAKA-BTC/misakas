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
};
use kaspa_addresses::Prefix;
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

    /// kaspa-pq Phase 3 PoW (ADR-0007): activation of the compute-only **BLAKE2b-512 ∥ SHA3-512**
    /// Layer-1 (`POW_ALGO_ID_BLAKE2B_SHA3 = 3`), which supersedes the Phase-2 Argon2id to make header
    /// verification ~10^4× cheaper (the IBD/catch-up bottleneck). Past this DAA score every block
    /// header MUST declare `algo_id = 3`; before it, the Phase-1 kHeavyHash (`algo_id = 1`).
    /// `always()` ⇒ BLAKE2b-SHA3 from genesis (testnet/mainnet); `never()` ⇒ stay kHeavyHash
    /// (devnet/simnet keep fast local PoW). Genesis (the parentless trusted root) is exempt.
    pub pow_blake2b_sha3_activation: ForkActivation,

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

impl Params {
    /// kaspa-pq: `true` when PQ-only enforcement is active at `daa_score`.
    /// In `Consensus` mode this gates legacy secp256k1 signature opcodes,
    /// P2SH, and non-ML-DSA-87 script classes at the consensus and script-
    /// engine level. See ADR-0019 / docs/kaspa-pq-design-mldsa87.md.
    #[inline]
    #[must_use]
    pub fn is_pq_active(&self, daa_score: u64) -> bool {
        matches!(self.pq_enforcement, PqEnforcementMode::Consensus) && daa_score >= self.pq_activation_daa_score
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): `true` when the EVM
    /// execution lane is active at `daa_score` on this network. Below the fence
    /// (the default `u64::MAX` for non-EVM nets) the `evm_payload` must be empty.
    #[inline]
    #[must_use]
    pub fn is_evm_active(&self, daa_score: u64) -> bool {
        daa_score >= self.evm_activation_daa_score
    }

    /// kaspa-pq EVM Lane: `true` when the gas-pool v2 executor (the liveness fix) is
    /// active at `daa_score`. Below the fence (the default `u64::MAX`) the v1 strict
    /// declared-gas prefix-take executes. See `evm_gas_pool_v2_activation_daa_score`.
    #[inline]
    #[must_use]
    pub fn is_evm_gas_pool_v2_active(&self, daa_score: u64) -> bool {
        daa_score >= self.evm_gas_pool_v2_activation_daa_score
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

    /// Returns the expected number of blocks per second
    #[inline]
    #[must_use]
    pub fn bps(&self) -> u64 {
        1000 / self.blockrate.target_time_per_block
    }

    /// Returns the expected number of blocks per second throughout history (currently represented as [`ForkedParam`]).
    /// Required permanently in order to calculate the subsidy month from the current DAA score.
    #[inline]
    #[must_use]
    pub fn bps_history(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            1000 / self.pre_crescendo_target_time_per_block,
            1000 / self.blockrate.target_time_per_block,
            self.crescendo_activation,
        )
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
            // kaspa-pq PoW algo activation is consensus-fixed, never runtime-overridable.
            pow_blake2b_sha3_activation: self.pow_blake2b_sha3_activation,
            // kaspa-pq: PQ enforcement is consensus-fixed, never runtime-overridable.
            pq_enforcement: self.pq_enforcement,
            pq_activation_daa_score: self.pq_activation_daa_score,
            // kaspa-pq EVM lane activation is consensus-fixed, never runtime-overridable.
            evm_activation_daa_score: self.evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score: self.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: self.evm_f002_withdraw_cap_activation_daa_score,
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

impl From<NetworkType> for Params {
    fn from(value: NetworkType) -> Self {
        match value {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => TESTNET_PARAMS,
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
    }
}

impl From<NetworkId> for Params {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
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
    emergency_stake_margin: StakeScore(100 * STAKE_SCORE_SCALE),
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
        // pool per finalized epoch. All inert via the v2 fence.
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
    // kaspa-pq bond spend-gate mergeset hardening: inert (u64::MAX) — the legacy own-body
    // spend-gate is the active protection; activation is a coordinated hard fork (see the field doc).
    bond_spend_gate_mergeset_activation_daa_score: u64::MAX,
    // kaspa-pq liveness-first DNS finality: attestation participation feeds StakeScore, rewards,
    // and health, but shipped networks do not make insufficient attestation stake a base-ledger
    // validity failure. Private/research networks can lower this fence when explicitly testing the
    // hard-inclusion anti-censorship rule.
    mandatory_attestation_inclusion_daa_score: u64::MAX,
    // Local finality-dependent producer/RPC policy: pause bridge/EVM payload production when the
    // DNS-confirmed anchor is older than this DAA distance. Not used for block validation.
    bridge_finality_max_staleness_daa_score: DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE,
};

/// Number of blocks in 14 days at the production 10 BPS block rate
/// (`14 d × 86_400 s/d × 10 blk/s`). Used for the unbonding window and the
/// equivocation-evidence window so a withdrawing validator stays slashable for
/// the whole 14-day exit.
pub const FOURTEEN_DAYS_BLOCKS_10BPS: u64 = 14 * 86_400 * 10; // 12_096_000

/// kaspa-pq production (mainnet + testnet) DNS-finality overlay params. Differs from the
/// shared [`GENESIS_ACTIVE_DNS_PARAMS`] (used by devnet/simnet) in the economically
/// load-bearing knobs:
///   * `min_active_stake_sompi = 20_000_000 KAS` — the network does not reach the `Active`
///     rollout stage until at least 20M KAS of stake is bonded (user decision 2026-06-01).
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
    // Production: the overlay reaches the Active stage once >= 20M KAS of stake is bonded.
    min_active_stake_sompi: 20_000_000 * SOMPI_PER_KASPA,
    // audit H-11 (Kaspa-diff): the DNS Active stage must NOT be drivable by a single key. A
    // multi-operator floor (3) is the mainnet default so finality does not hinge on one operator's
    // key/availability/honesty (the safety floor is BOTH the 20M-KAS `min_active_stake_sompi` AND
    // this validator COUNT). The FINAL value (3-5+), stake-concentration caps, and the
    // `required_work_depth` calibration to live difficulty are a mainnet-launch governance gate —
    // see the mainnet launch checklist; mainnet is not yet launched. (Testnet pins this back to 1
    // in TESTNET_DNS_PARAMS for the single-operator experimental mesh.)
    min_active_validators: 3,
    // Production: every individual validator must bond >= 20M KAS; a smaller StakeBond is
    // rejected at acceptance and can never attest (user decision 2026-06-01).
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
    emergency_stake_margin: StakeScore(100 * STAKE_SCORE_SCALE),
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
        // pool per finalized epoch. All inert via the v2 fence.
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
    // kaspa-pq bond spend-gate mergeset hardening: inert (u64::MAX) on mainnet+testnet — the legacy
    // own-body spend-gate stays the active protection until a coordinated activation (see field doc).
    bond_spend_gate_mergeset_activation_daa_score: u64::MAX,
    // kaspa-pq liveness-first DNS finality: keep attestation below the base-chain validity layer.
    // Missing or below-floor shards degrade StakeScore / DNS health and pause finality-dependent
    // flows, but miners can still advance the PoW/GHOSTDAG ledger while validators recover. Invalid
    // shards remain rejected by the normal eligibility/signature checks. Private/research forks can
    // lower this fence to test the hard-inclusion anti-censorship rule.
    mandatory_attestation_inclusion_daa_score: u64::MAX,
    // Local finality-dependent producer/RPC policy: pause bridge/EVM payload production when the
    // DNS-confirmed anchor is older than this DAA distance. Not used for block validation.
    bridge_finality_max_staleness_daa_score: DEFAULT_BRIDGE_FINALITY_MAX_STALENESS_DAA_SCORE,
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
pub const TESTNET_DNS_PARAMS: DnsParams = DnsParams {
    required_work_depth: Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]),
    min_bond_amount_sompi: 10 * SOMPI_PER_KASPA,
    min_active_stake_sompi: 10 * SOMPI_PER_KASPA,
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
    pow_blake2b_sha3_activation: ForkActivation::always(),
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020: EVM lane inert in P1 (no executor yet); the testnet value flips to
    // a finite activation score when the revm executor lands (P2+). u64::MAX = never.
    evm_activation_daa_score: u64::MAX,
    // gas-pool v2 ships inert on every network — a deploy sets a finite testnet score.
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

pub const TESTNET_PARAMS: Params = Params {
    // kaspa-pq testnet DNS seeders (MISAKA-operated, isolated per
    // docs/adr/0001-network-isolation.md). Same Kaspa-style auto-discovery as mainnet,
    // but nodes connect on the testnet-10 default P2P port (26211) — so the hosts
    // behind these records must also run a reachable testnet-10 node on 26211.
    dns_seeders: &[
        "seeder1.misakascan.com",
        "seeder2.misakascan.com",
        "seeder3.misakascan.com",
        "seeder4.misakascan.com",
        "seeder1.misakachain.com",
        "seeder2.misakachain.com",
        "seeder3.misakachain.com",
    ],
    net: NetworkId::with_suffix(NetworkType::Testnet, 10),
    genesis: TESTNET_GENESIS,
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
    max_block_level: 250,
    pruning_proof_m: 1000,

    blockrate: BlockrateParams::new::<10>(),

    // kaspa-pq: 10 BPS since genesis. This field only feeds the subsidy-month
    // calc (`bps_history`); setting it to 100ms keeps emission on the 10 BPS
    // schedule throughout, independent of the (legacy) crescendo activation score.
    pre_crescendo_target_time_per_block: 100,

    // 18:30 UTC, March 6, 2025
    crescendo_activation: ForkActivation::new(88_657_000),
    // kaspa-pq: TESTNET inherits mainnet's production overlay economics (14-day
    // unbonding/evidence window, PoS-v2 active, 2-D dominance reorg gate) but with
    // testnet-friendly thresholds (see TESTNET_DNS_PARAMS): a lowered
    // `required_work_depth` (100) so the 2-D DNS gate confirms at Argon2id's floored
    // CPU difficulty, and 10-KAS `min_bond`/`min_active_stake` so a single
    // premine-backed validator can drive finality. Not a genesis-block input, so the
    // genesis hash is unchanged.
    dns_params: Some(TESTNET_DNS_PARAMS),
    pow_blake2b_sha3_activation: ForkActivation::always(),
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
    pow_blake2b_sha3_activation: ForkActivation::never(),
    pq_enforcement: PqEnforcementMode::Consensus,
    pq_activation_daa_score: 0,
    // ADR-0020: EVM lane inert in P1 (no executor yet); the testnet value flips to
    // a finite activation score when the revm executor lands (P2+). u64::MAX = never.
    evm_activation_daa_score: u64::MAX,
    // gas-pool v2 ships inert on every network — a deploy sets a finite testnet score.
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

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
    difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
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

    blockrate: BlockrateParams::new::<10>(),

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
    // aggregation walk cheap on the ~10 bps devnet (amortized O(1) per block).
    dns_params: Some(GENESIS_ACTIVE_DNS_PARAMS),
    pow_blake2b_sha3_activation: ForkActivation::never(),
};
