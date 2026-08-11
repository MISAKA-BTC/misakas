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
        Self {
            target_time_per_block: 10_000,
            ghostdag_k: 4,
            past_median_time_sample_rate: 1,
            difficulty_sample_rate: 1,
            max_block_parents: 10,
            mergeset_size_limit: 180,
            merge_depth: 360,
            finality_depth: 4_320,
            pruning_depth: 10_800,
            coinbase_maturity: 10,
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

    /// MISAKA Phase 4 PoW: activation of the **PALW deterministic pinned-LLM** Layer-1
    /// (`POW_ALGO_ID_PALW_LLM = 4`). Past this DAA score every block header MUST declare
    /// `algo_id = 4` and its PoW digest is the Layer-0 finalizer over one deterministic
    /// Qwen3.5-2B inference transcript (see `pow_layer0::POW_ALGO_ID_PALW_LLM`); it supersedes
    /// BLAKE2b-SHA3 where both are active. `always()` ⇒ PALW from genesis (devnet — the
    /// 0.1-bps LLM-PoW network); `never()` ⇒ inert (mainnet/testnet/simnet until their own
    /// fork ADR). Genesis (the parentless trusted root) is exempt. Validating nodes need the
    /// pinned worker (`PALW_WORKER` + `MISAKA_PALW_GGUF`) or the explicit fixture mode
    /// (`MISAKA_PALW_POW_FIXTURE=1`).
    pub pow_palw_activation: ForkActivation,

    /// MISAKA Phase 4b PoW: activation of the **PALW-via-Ollama** Layer-1
    /// (`POW_ALGO_ID_PALW_OLLAMA = 5`), superseding every other algo where active. Same seed /
    /// prompt / grinding closure as Phase 4; the runtime is a host-local Ollama server running
    /// the pinned Qwen model (the runtime an Ubuntu VPS fleet operates), and the tag commits to
    /// the greedy response bytes + token counts (Ollama exposes no per-decode logits).
    /// `always()` ⇒ from genesis (testnet-10 — the public PALW network); `never()` elsewhere
    /// (devnet keeps the stronger algo-4 worker tag). Nodes need `MISAKA_PALW_OLLAMA_MODEL`
    /// (+ optional `MISAKA_PALW_OLLAMA_URL`) or the devnet-only fixture env.
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
    pub fn consensus_params_id(&self) -> Hash {
        // Exhaustive on purpose. If this stops compiling because `Params` gained a field, decide
        // whether that field changes block validity: if it does, hash it; if it does not, bind it
        // with a comment saying why, as below.
        let Params {
            // Excluded: where to find peers is not a rule about blocks.
            dns_seeders: _,
            net,
            genesis,
            timestamp_deviation_tolerance,
            max_difficulty_target,
            // Excluded: a lossy f64 view of `max_difficulty_target`, which is hashed above. Its bit
            // pattern is also not stable across the ways it can be computed.
            max_difficulty_target_f64: _,
            past_median_time_window_size,
            difficulty_window_size,
            min_difficulty_window_size,
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            mass_per_tx_byte,
            mass_per_script_pub_key_byte,
            mass_per_sig_op,
            max_block_mass,
            storage_mass_parameter,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            skip_proof_of_work,
            max_block_level,
            pruning_proof_m,
            blockrate,
            pre_crescendo_target_time_per_block,
            crescendo_activation,
            dns_params,
            pow_blake2b_sha3_activation,
            pow_palw_activation,
            pow_palw_ollama_activation,
            pq_enforcement,
            pq_activation_daa_score,
            evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score,
        } = self;

        let mut h = ConsensusParamsId::new();
        h.write(CONSENSUS_FINGERPRINT_DOMAIN_V1);

        h.write([net.network_type as u8]);
        h.write(net.suffix.unwrap_or(u32::MAX).to_le_bytes());
        h.write(genesis.hash.as_bytes());
        h.write(timestamp_deviation_tolerance.to_le_bytes());
        h.write(max_difficulty_target.to_le_bytes());
        h.write((*past_median_time_window_size as u64).to_le_bytes());
        h.write((*difficulty_window_size as u64).to_le_bytes());
        h.write((*min_difficulty_window_size as u64).to_le_bytes());
        h.write([*coinbase_payload_script_public_key_max_len]);
        h.write((*max_coinbase_payload_len as u64).to_le_bytes());
        h.write((*max_tx_inputs as u64).to_le_bytes());
        h.write((*max_tx_outputs as u64).to_le_bytes());
        h.write((*max_signature_script_len as u64).to_le_bytes());
        h.write((*max_script_public_key_len as u64).to_le_bytes());
        h.write(mass_per_tx_byte.to_le_bytes());
        h.write(mass_per_script_pub_key_byte.to_le_bytes());
        h.write(mass_per_sig_op.to_le_bytes());
        h.write(max_block_mass.to_le_bytes());
        h.write(storage_mass_parameter.to_le_bytes());
        h.write(deflationary_phase_daa_score.to_le_bytes());
        h.write(pre_deflationary_phase_base_subsidy.to_le_bytes());
        h.write([*skip_proof_of_work as u8]);
        h.write([*max_block_level]);
        h.write(pruning_proof_m.to_le_bytes());

        h.write(blockrate.target_time_per_block.to_le_bytes());
        h.write((blockrate.ghostdag_k as u64).to_le_bytes());
        h.write(blockrate.past_median_time_sample_rate.to_le_bytes());
        h.write(blockrate.difficulty_sample_rate.to_le_bytes());
        h.write([blockrate.max_block_parents]);
        h.write(blockrate.mergeset_size_limit.to_le_bytes());
        h.write(blockrate.merge_depth.to_le_bytes());
        h.write(blockrate.finality_depth.to_le_bytes());
        h.write(blockrate.pruning_depth.to_le_bytes());
        h.write(blockrate.coinbase_maturity.to_le_bytes());

        h.write(pre_crescendo_target_time_per_block.to_le_bytes());
        h.write(crescendo_activation.daa_score().to_le_bytes());

        // Length-prefixed: `None` and an empty encoding must not hash alike.
        match dns_params {
            Some(dns) => {
                let bytes = borsh::to_vec(dns).expect("DnsParams is borsh-serializable");
                h.write((bytes.len() as u64).to_le_bytes());
                h.write(&bytes);
            }
            None => h.write(u64::MAX.to_le_bytes()),
        };

        h.write(pow_blake2b_sha3_activation.daa_score().to_le_bytes());
        h.write(pow_palw_activation.daa_score().to_le_bytes());
        h.write(pow_palw_ollama_activation.daa_score().to_le_bytes());
        h.write([*pq_enforcement as u8]);
        h.write(pq_activation_daa_score.to_le_bytes());
        h.write(evm_activation_daa_score.to_le_bytes());
        h.write(evm_gas_pool_v2_activation_daa_score.to_le_bytes());
        h.write(evm_f002_withdraw_cap_activation_daa_score.to_le_bytes());
        h.write(evm_f003_mldsa_verify_activation_daa_score.to_le_bytes());
        h.write(evm_typed_receipt_root_activation_daa_score.to_le_bytes());

        h.finalize()
    }

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
            pow_palw_activation: self.pow_palw_activation,
            pow_palw_ollama_activation: self.pow_palw_ollama_activation,
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
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        })
    }
}

impl From<NetworkId> for Params {
    fn from(value: NetworkId) -> Self {
        with_registered_models(match value.network_type {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_PARAMS,
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
    // kaspa-pq bond spend-gate mergeset hardening: still inert (u64::MAX) on mainnet+testnet, and
    // that is now a SCHEDULING decision rather than a default (2026-08-11 audit P0).
    //
    // The hole is real and open here: the legacy own-body gate never sees a spend that arrives via
    // the MERGESET, so a validator can withdraw its locked collateral out from under an `Active`
    // bond — removing exactly the backing `λ·B_i` caps weight against and slashing threatens.
    // Devnet/simnet start with the acceptance-time skip at genesis (see GENESIS_ACTIVE_DNS_PARAMS);
    // a public network cannot, because activating it re-classifies transactions and is therefore a
    // coordinated hard fork with real history behind it.
    //
    // It must move in the SAME release as the VLT shadow fence — that fence turns on challenge
    // slashing, and slashing an unbacked bond is theatre. See
    // `docs/testnet10-vlt-shadow-fork-runbook.md`, which refuses to have its `H` chosen until
    // this and the forged-evidence gap close.
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
    // DNS-accelerated coinbase settlement: OFF on mainnet until testnet has soaked the policy
    // layer through at least one full anchor-live / anchor-dead cycle. See the field doc.
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
    // MISAKA Verified LLM Token-Weighted BFT (`vlt::VltParams`): the replacement of bonded capital
    // by verified useful compute as the source of voting power. Shipped DORMANT
    // (`vlt_activation_daa_score: u64::MAX`) on mainnet + testnet: activating it is a coordinated
    // hard fork and must not be scheduled until the active set can actually produce verified
    // compute, because with no VLT every `W_i(E)` is zero and no epoch reaches the `Q(E)` quorum.
    //
    // Note what is deliberately NOT changed by this: `min_bond_amount_sompi` and
    // `min_active_stake_sompi` stay at 20M KAS above. Under VLT weighting the bond stops being
    // voting power and becomes the participation requirement plus the slashable collateral that
    // caps convertible compute (`λ·B_i`) — the same 20M number, a different job.
    //
    // SCHEDULING THE FORK (ADR-0024 "Activation runbook", steps 3 and 4) is an edit to exactly
    // three fields of this struct, and nothing else in this preset:
    //
    //     vlt: VltParams {
    //         vlt_shadow_activation_daa_score: <H>,          // step 3 — the overlay starts running
    //         vlt_activation_daa_score: <H + vlt_credit_span()>,  // step 4 — the vote moves
    //         model_cost_table: ModelCostTable::palw_metal_registered(),
    //         ..VltParams::INERT
    //     },
    //
    // (`palw_metal_registered` carries BOTH pinned Metal profiles — Qwen3.5-2B palw-lite and the
    // 35B PALW — because the 2B one is what a fleet's verifier committees can actually afford to
    // fully replay; see `docs/testnet10-vlt-shadow-fork-runbook.md` for the step-3 flag-day
    // procedure, the hardware-class precondition, and why the fence and the table move in ONE
    // release.)
    //
    // Everything else here is already sized for `INERT`'s K = 96: the credit walk below covers
    // the span, and `unbonding_period_blocks` covers the §7 bound. Two tests hold that claim —
    // `public_presets_need_only_the_two_heights_and_the_model_to_fork` proves no other constant
    // has to move, and `shipped_presets_are_either_dormant_or_fully_forkable` fails the build if
    // a fence is moved without the rest of the edit. The model table is not optional: an empty
    // one credits every job zero, so the fork would cost a hard fork to discover it did nothing.
    vlt: VltParams::INERT,
    // MISAKA Compute Token Program (design v0.1 §10): inert everywhere — the TOK ledger and
    // emission do not exist until a per-network hard fork moves these fences (and freezes the
    // TBD R0/H schedule numbers the design deliberately leaves open).
    tkn: TokenParams::INERT,
    // Sized for `VltParams::INERT`'s K = 96 + delay 1 epochs at the 100-blue_score attestation
    // epoch length, plus the 300-block challenge window and a lag/grace margin. This is the walk
    // cost VLT weighting adds per recompute; it is paid only once the fence above is moved.
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
/// `u64::MAX` means "not scheduled", which is the shipped state. Setting it to a DAA height picks
/// up everything the fork needs at once, because the three fields below are derived from it
/// rather than edited independently:
///
/// * `vlt.vlt_shadow_activation_daa_score` — the overlay starts crediting, drawing committees,
///   paying the audit fee and slashing settled challenges;
/// * `bond_spend_gate_mergeset_activation_daa_score` — the mergeset spend gate closes, so the
///   collateral those slashes are aimed at can no longer be withdrawn out from under an Active
///   bond (2026-08-11 audit P0; it MUST move with the fence, never after it);
/// * `vlt.model_cost_table` — the registered profiles, without which every job mints zero and the
///   fork would cost a hard fork to discover it did nothing.
///
/// The weight fence (`vlt_activation_daa_score`) deliberately stays `u64::MAX`: moving the VOTE
/// is step 4, after the soak has measured what the weight is made of.
///
/// Choosing the height, the fleet-update procedure and the exit criteria are in
/// `docs/testnet10-vlt-shadow-fork-runbook.md`. The rule of thumb: current tip plus twice the
/// fleet's update window, and every validator/miner binary inside the fleet BEFORE it.
pub const TESTNET_VLT_SHADOW_FORK_DAA_SCORE: u64 = 30_200_000;

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
    // ── PALW 0.1-bps re-sizing of every block/DAA/blue-score-denominated window ─────────────────
    // PRODUCTION's windows are sized for 10 bps; inherited unchanged onto a 0.1-bps chain their
    // wall-clock stretches ×100 (the 14-day unbond becomes ~3.8 YEARS, the ~55-min dead-branch
    // auto-release becomes days). Every override below preserves the SECURITY semantics in wall
    // clock, with two deliberate deviations noted inline. Mainnet (10 bps) keeps PRODUCTION as-is.
    //
    //   field                                PRODUCTION @10bps        here @0.1bps       wall clock
    //   epoch_length_blocks                  100        (10 s)        10    (100 s)      ×10: 1-block
    //     epochs would be degenerate (every block an epoch boundary); 100 s epochs keep the
    //     attestation/snapshot cadence meaningful. attestation_* below move in lockstep.
    //   max_reorg_horizon_blocks             300        (30 s)        30    (300 s)      ×10: a
    //     3-block horizon is below plausible anticone depth even at k = 4; 30 blocks errs long.
    //   evidence_window_blocks               12_096_000 (14 d)        120_960 (14 d)     exact
    //   unbonding_period_blocks              +300                     120_990            U ≥ R+E ✓
    //   dns_gate_horizon_blocks              18_000     (30 min)      1_800  (5 h)       3×TTL kept
    //   dns_veto_ttl_daa_score               6_000      (10 min)      600    (100 min)   30× the
    //     healthy anchor-to-tip distance (lag 10 + epoch 10 = 20), the same headroom ratio the
    //     PRODUCTION comment derives (6_000 ≈ 30 × 200).
    //   attestation_epoch_length_blue_score  100                      10
    //   attestation_lag_blue_score           100                      10
    //   attestation_anchor_backoff_blue_score 20                      2
    //   stake_score_window_blue_score        1_500                    150   (= 15 epochs, the
    //     window-ceiling ratio the emergency_stake_margin calibration relies on)
    //   bridge_finality_max_staleness        1_500      (150 s)       15    (150 s)      exact
    //   coinbase_settlement_long_maturity    30_000     (~4.5 h)      3_000 (8.3 h)      > horizon
    //     + TTL with the same margin shape (3_000 > 1_800 + 600).
    //   reward_uniqueness_window_blocks      600 — INHERITED unchanged (block-count semantics; the
    //     scan bound is cheap and shrinking it toward the mergeset limit would weaken it).
    epoch_length_blocks: 10,
    max_reorg_horizon_blocks: 30,
    evidence_window_blocks: 120_960,
    unbonding_period_blocks: 120_990,
    dns_gate_horizon_blocks: 1_800,
    dns_veto_ttl_daa_score: 600,
    attestation_epoch_length_blue_score: 10,
    attestation_lag_blue_score: 10,
    attestation_anchor_backoff_blue_score: 2,
    stake_score_window_blue_score: 150,
    bridge_finality_max_staleness_daa_score: 15,
    // (superseded by the PALW table above; the historical rationale for inheriting 18_000/6_000
    // at 10 bps — measured ~110 DAA/min, ~2.5 h protection, ~55 min auto-release — is preserved
    // in the PRODUCTION_DNS_PARAMS comment.)
    // The preference soaks here first (mainnet ships it OFF): ½-work bound, arming only when
    // this chain's own anchor has been dead past the TTL above. See the field doc.
    stake_preference_max_work_deficit_multiplier: 2,
    // Settlement soaks here first too, POLICY layer only (the consensus layer is unwired until
    // the anchor gets its sequential per-chain-block view — see the field doc). 3_000 DAA >
    // gate horizon (1_800) + veto TTL (600) with margin: a contested fork should resolve
    // before either side's rewards go liquid through the fallback. At 0.1 bps this is ~8.3 h —
    // long enough to bite, short enough to tolerate while the validator set is being restored.
    coinbase_settlement_long_maturity_daa: 3_000,
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
    // PALW 0.1 bps: with `difficulty_sample_rate = 1` the window size IS the window duration in
    // blocks — 264 × 10 s ≈ the same 2 641 s DAA window the sampled 661-slot window models on
    // ≥1-bps networks (see the devnet preset for the full rationale).
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

    // PALW re-genesis: testnet-10 runs the 0.1-bps LLM-PoW blockrate — one block per 10 s,
    // sized so one deterministic Qwen3.5-2B inference (~1-3 s/attempt) is a meaningful
    // fraction of the block interval. See docs/testnet10-palw-rollout-runbook.md for the
    // coordinated public rollout this implies.
    blockrate: BlockrateParams::new_deci_bps(),

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
    pow_blake2b_sha3_activation: ForkActivation::always(),
    // PALW LLM PoW from genesis, in the OLLAMA flavor (algo_id = 5): the public testnet-10 IS
    // the 0.1-bps LLM-PoW network as of the "-palw" re-genesis
    // (docs/testnet10-palw-rollout-runbook.md), and its fleet is Ubuntu VPSes, which cannot run
    // the Metal-pinned worker — the runtime is a host-local Ollama serving the pinned Qwen
    // model (`MISAKA_PALW_OLLAMA_MODEL`, optional `MISAKA_PALW_OLLAMA_URL`). The stronger
    // worker-tag algo (4) stays devnet's; its activation here remains never() so required_algo_id
    // resolves to 5 alone.
    pow_palw_activation: ForkActivation::never(),
    pow_palw_ollama_activation: ForkActivation::always(),
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
    pow_blake2b_sha3_activation: ForkActivation::never(),
    // PALW LLM PoW from genesis: devnet IS the 0.1-bps LLM-PoW network on this branch. Every
    // post-genesis header declares algo_id = 4 and is validated by replaying one deterministic
    // pinned-Qwen3.5-2B inference (or the explicit `MISAKA_PALW_POW_FIXTURE=1` fixture). Devnet
    // deliberately keeps the WORKER flavor (full-logits `gemm_trace_root` binding) — the Ollama
    // flavor (5) is testnet's fleet-runtime concession.
    pow_palw_activation: ForkActivation::always(),
    pow_palw_ollama_activation: ForkActivation::never(),
};

#[cfg(test)]
mod consensus_params_id_tests {
    use super::*;

    /// The hand-evaluated 0.1-bps blockrate constants must match the same formulas `Bps<BPS>`
    /// encodes, evaluated at λ = 0.1 (`Bps` itself cannot express a sub-integer rate). Guards the
    /// spelled-out values in `new_deci_bps` against drift in the shared duration constants.
    #[test]
    fn deci_bps_constants_match_formulas() {
        use crate::config::{bps::calculate_ghostdag_k, constants::consensus::*};
        let b = BlockrateParams::new_deci_bps();
        let lambda = 0.1f64;
        assert_eq!(b.target_time_per_block, 10_000, "1000 / 0.1 ms");
        assert_eq!(b.ghostdag_k as u64, calculate_ghostdag_k(2.0 * NETWORK_DELAY_BOUND as f64 * lambda, GHOSTDAG_TAIL_DELTA));
        // Per-block sample rates floor at 1 (0.1 · interval < 1 for both windows).
        assert_eq!(b.past_median_time_sample_rate, 1);
        assert_eq!(b.difficulty_sample_rate, 1);
        // Formula floors: k/2 = 2 < 10 parents; 2k = 8 < 180 mergeset.
        assert_eq!(b.max_block_parents, 10);
        assert_eq!(b.mergeset_size_limit, 180);
        // Duration-scaled depths: λ · duration, exactly.
        assert_eq!(b.merge_depth as f64, lambda * MERGE_DEPTH_DURATION as f64);
        assert_eq!(b.finality_depth as f64, lambda * FINALITY_DURATION as f64);
        assert_eq!(b.coinbase_maturity as f64, lambda * COINBASE_MATURITY_SECONDS as f64);
        // Pruning: the prunality lower bound at these constants sits below the duration term,
        // so the duration term wins — recompute both sides to keep that claim honest.
        let lower_bound = b.finality_depth
            + b.merge_depth * 2
            + 4 * b.mergeset_size_limit * b.ghostdag_k as u64
            + 2 * b.ghostdag_k as u64
            + 2;
        assert!(lower_bound <= b.pruning_depth, "prunality lower bound {lower_bound} must not exceed pruning depth");
        assert_eq!(b.pruning_depth as f64, lambda * PRUNING_DURATION as f64);
        // The devnet preset actually runs these params with PALW active from genesis.
        assert_eq!(DEVNET_PARAMS.blockrate.target_time_per_block, 10_000);
        assert!(DEVNET_PARAMS.pow_palw_activation.is_active(0));
        // And the integer-bps view floors at 1 (the raw division is 0 — the reason emission
        // consumes `target_time_per_block_history` and sizing consumers get the floored view).
        assert_eq!(DEVNET_PARAMS.bps(), 1);
        assert_eq!(DEVNET_PARAMS.target_time_per_block_history().after(), 10_000);
    }

    #[test]
    fn a_different_rule_set_gets_a_different_fingerprint() {
        // The whole point of the handshake check: two nodes answering the same network name must
        // not be able to peer while disagreeing about block validity.
        let base = SIMNET_PARAMS;
        let mut tweaked = SIMNET_PARAMS;
        tweaked.ghostdag_k += 1;
        assert_ne!(base.consensus_params_id(), tweaked.consensus_params_id());
    }

    #[test]
    fn shipped_presets_have_pinned_fingerprints() {
        // Golden vectors. Any change to what goes into the fingerprint, or to how it is encoded,
        // breaks these — which is the point. It forces the change to be deliberate, and it lets an
        // operator tell whether two releases will peer before deploying one of them.
        //
        // If you are here because a preset legitimately changed: update the value, and understand
        // that nodes on the old build will no longer peer with this one. That is usually the
        // correct outcome. Make sure it is the intended one.
        //
        // Last moved when the Compute Token Program's `tkn: TokenParams` joined `DnsParams`
        // (design v0.1 §10, inert `u64::MAX` fences everywhere — same shape as the `vlt` and
        // settlement-fence appends before it: `dns_params` is hashed as its whole borsh encoding,
        // so every preset that carries an overlay moved at once). Deliberate: the same release
        // also admits the 0x30/0x31 token-op subnetworks, which older builds reject per-tx, so
        // shipping it is the next coordinated flag day, not a rolling update — exactly as it was
        // for the settlement/preference batch this note previously recorded.
        //
        // Report every preset rather than dying on the first. All four moved together on that
        // merge, and a first-failure assert showed one of them, which reads as a narrower change
        // than it was.
        //
        // PALW LLM PoW (this branch): all four moved again because `pow_palw_activation` entered
        // the hash (every preset writes its daa_score, so even the `never()` nets shift — same
        // mechanics as the TokenParams merge above). Devnet moved for three additional,
        // deliberate reasons: `always()` activation, the 0.1-bps `new_deci_bps` blockrate, and
        // the re-genesised trivial-bits genesis hash. Coordinated flag day, as before.
        //
        // And once more when `pow_palw_ollama_activation` entered the hash (the Phase-4b
        // Ollama-runtime algo, `always()` on testnet-10 — the fleet's runtime — and `never()`
        // elsewhere).
        let changed: Vec<String> = [
            ("mainnet", MAINNET_PARAMS, "9110ee1c8bedfc8cd0e32336a7adeeb2940752737e385d1c69b65aee662334c2"),
            // Moved again by the t10 PALW re-genesis ("-palw" marker + trivial bits + 0.1-bps
            // blockrate + palw activation + the wall-clock-preserving DnsParams re-sizing) —
            // see docs/testnet10-palw-rollout-runbook.md — and pinned MATERIALIZED (below) per
            // the 8208cd6 lesson, so the pre-merge values (`32cbf80f…` re-genesis-const /
            // `d07cb673…` shadow-materialized) were both superseded by this merge.
            ("testnet", TESTNET_PARAMS, "2d2258cc51a3b2216bab6d93b0aec2332322903e5e7414db15ad8112adced671"),
            ("simnet", SIMNET_PARAMS, "135e88c69a659d3cf4b5ce8275953c7597b2c67b03d2a74b3d0696c5d0b703fa"),
            ("devnet", DEVNET_PARAMS, "42cc6be92506a14654cb676184e1416796dec682b15e93cb9c639e8e0d77efa5"),
        ]
        .into_iter()
        .filter_map(|(name, params, expected)| {
            // MATERIALIZED, not the raw const. `with_registered_models` attaches the compute
            // profiles to a preset that has scheduled its VLT shadow fence, and it runs at every
            // `From<NetworkType/NetworkId> for Params` — so the const and the thing a node
            // actually runs are different values, and only the second one is what peers compare
            // at the handshake. Pinning the const would pin a number no node ever reports:
            // caught live, where a correctly-built release announced `5fabb683…` while this test
            // was green on `62e299b6…`.
            let actual = Params::from(params.net).consensus_params_id().to_string();
            (actual != expected).then(|| format!("  {name}: pinned {expected}, got {actual}"))
        })
        .collect();
        assert!(changed.is_empty(), "consensus fingerprint changed for {} preset(s):\n{}", changed.len(), changed.join("\n"));
    }

    #[test]
    fn the_encoding_cannot_be_confused_by_concatenation() {
        // Adjacent fields must not be able to trade digits. Moving a value from one field to the
        // next has to change the fingerprint, or two different rule sets could share one.
        let mut a = SIMNET_PARAMS;
        let mut b = SIMNET_PARAMS;
        a.max_tx_inputs = 10;
        a.max_tx_outputs = 20;
        b.max_tx_inputs = 1020;
        b.max_tx_outputs = 0;
        assert_ne!(a.consensus_params_id(), b.consensus_params_id());
    }

    #[test]
    fn an_absent_dns_overlay_is_not_an_empty_one() {
        // `None` and a zero-length encoding must differ, or "no overlay" and "an overlay that
        // happens to encode short" would look like the same rule set.
        let mut with = SIMNET_PARAMS;
        let mut without = SIMNET_PARAMS;
        with.dns_params = Some(GENESIS_ACTIVE_DNS_PARAMS);
        without.dns_params = None;
        assert_ne!(with.consensus_params_id(), without.consensus_params_id());
    }

    #[test]
    fn a_fork_activation_height_is_part_of_the_rules() {
        let base = TESTNET_PARAMS;
        let mut moved = TESTNET_PARAMS;
        moved.evm_activation_daa_score += 1;
        assert_ne!(base.consensus_params_id(), moved.consensus_params_id(), "a different activation schedule is a different rule set");
    }

    #[test]
    fn identical_params_agree() {
        assert_eq!(SIMNET_PARAMS.consensus_params_id(), SIMNET_PARAMS.clone().consensus_params_id());
    }

    #[test]
    fn every_shipped_preset_is_distinguishable() {
        let ids = [
            ("mainnet", MAINNET_PARAMS.consensus_params_id()),
            ("testnet", TESTNET_PARAMS.consensus_params_id()),
            ("simnet", SIMNET_PARAMS.consensus_params_id()),
            ("devnet", DEVNET_PARAMS.consensus_params_id()),
        ];
        for (i, (name_a, a)) in ids.iter().enumerate() {
            for (name_b, b) in ids.iter().skip(i + 1) {
                assert_ne!(a, b, "{name_a} and {name_b} must not share a fingerprint");
            }
        }
    }

    #[test]
    fn an_overlay_rule_change_is_caught() {
        // testnet-22's shape: a build whose overlay behaviour differs while everything visible at
        // handshake looks identical. The fingerprint covers the whole struct, so it is caught.
        let base = TESTNET_PARAMS;
        let mut tweaked = TESTNET_PARAMS;
        if let Some(dns) = tweaked.dns_params.as_mut() {
            dns.dns_activation_daa_score += 1;
        } else {
            tweaked.dns_params = Some(GENESIS_ACTIVE_DNS_PARAMS);
        }
        assert_ne!(base.consensus_params_id(), tweaked.consensus_params_id());
    }
}
