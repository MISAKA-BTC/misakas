pub use super::{
    bps::{Bps, TenBps},
    constants::consensus::*,
    genesis::{COMPUTE_REGISTRY_PALW_GENESIS, DEVNET_GENESIS, GENESIS, GenesisBlock, SIMNET_GENESIS, TESTNET_GENESIS, TESTNET11_GENESIS},
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
    /// kaspa-pq ADR-0039 PALW (audited-compute lane) fence. At/after this DAA score the header must be
    /// version `PALW_HEADER_VERSION` (v3), the algo-4 replica lane is live, and the ten PALW header
    /// fields carry ticket data; before it they MUST be zero (hash-invisible on pre-v3, so a non-zero
    /// value would be header malleability — enforced in `check_header_version`). `u64::MAX` ⇒ PALW never
    /// active on this net; a finite value ⇒ active. Mainnet, testnet-10, simnet and devnet use
    /// `u64::MAX`; the three PALW presets use `0` and are active from genesis. Mirrors the
    /// `evm_activation_daa_score` precedent.
    pub palw_activation_daa_score: u64,
    /// ADR-0040 P0-3 algo-4 acceptance control. When false, `check_pow_algo_id` rejects algo-4 before
    /// GHOSTDAG and header-stage store writes. The three PALW presets enable it; inactive presets do
    /// not. Presets 110 and 111 are bounded by their peer allowlist, while preset 200 uses the
    /// Header-v4 anti-spam accumulator.
    pub palw_algo4_accept: bool,
    /// ADR-MA (model-agnostic Compute Set registry): the activation fence for the Header-v5
    /// set/policy/plan commitments and the 0x40-0x44 registry transaction band. `u64::MAX` ⇒ the
    /// registry never activates on this net and every new code path is byte-identically inert
    /// (Header v5 rejected, registry txs rejected, no store writes). A finite value ⇒ v5 headers
    /// REQUIRED and registry txs live from that DAA score. Follows the
    /// `palw_activation_daa_score` precedent exactly; activation is a coordinated wire cutover
    /// (§25 — introduce the references BEFORE PALW main-net operation, or pay a hard fork later).
    pub palw_compute_registry_activation_daa_score: u64,
    /// ADR-0040 P1-13 archival-operation requirement. Presets without a supported pruning snapshot
    /// path set this flag so startup rejects pruned operation.
    pub palw_requires_archival: bool,
    /// ADR-0040 closed-network requirement. Startup requires an explicit peer allowlist when this flag
    /// is set.
    pub palw_requires_peer_allowlist: bool,
    /// kaspa-pq ADR-0039 PALW (§5.3/§28): fixed compute-credit scale applied to each unique blue
    /// algo-4 source (`ΔC = scale · calc_work(bits)`). This knob is deliberately independent of
    /// `palw_activation_daa_score`: Stage A can accept and measure the replica lane with `scale = 0`
    /// while leaving fork-choice work unchanged. Raising it is a consensus hard fork.
    pub palw_compute_work_scale: u64,
    /// kaspa-pq ADR-0039 PALW (§15.2): the active-nullifier retention window in DAA (≈ 120 s at 10 BPS
    /// = 1 200). Only consumed while PALW is active; a harmless unused value on non-PALW presets. The
    /// remaining PalwParams (lane BPS, epoch windows, audit params, `supported_profiles`) are built at
    /// runtime from `PalwParams::testnet_inert_default()` — they cannot live in a `const Params` (the
    /// `Vec` field), and are only read on a PALW-activated network.
    pub palw_nullifier_retention_daa: u64,
    /// kaspa-pq ADR-0039 PALW (§14.2): the PALW epoch length in DAA (≈ 10 s at 10 BPS = 100), used to
    /// map a header's DAA score to its PALW epoch for leaf/certificate activation checks. Unused while
    /// PALW is inactive.
    pub palw_epoch_length_daa: u64,
    /// kaspa-pq ADR-0039 PALW (§11.3): consecutive degraded epochs the DNS beacon tolerates before it
    /// halts algo-4 acceptance (`beacon_mode` grace window). Unused while PALW is inactive.
    pub palw_beacon_grace_epochs: u64,
    /// kaspa-pq ADR-0039 PALW (§11.2): the beacon commit-reveal quorum fraction `num/den` — the
    /// stake-weighted revealed tally must reach this fraction of committed stake for a Healthy seed
    /// advance (testnet 2/3). Unused while PALW is inactive.
    pub palw_beacon_quorum_num: u16,
    pub palw_beacon_quorum_den: u16,
    /// kaspa-pq **ADR-0040 P1-3 (CERT-01)** (§10.2): the batch-certificate AUDITOR quorum fraction
    /// `num/den` — the stake-weighted PASS tally over the certificate's voting bonds must reach this
    /// fraction for the certificate to be admitted (testnet 2/3). Distinct from the beacon quorum above:
    /// that one gates seed advance, this one gates whether a batch may become `Certified` at all.
    /// Mirrors `PalwParams::auditor_quorum_{num,den}`, lifted into `Params` because certificate
    /// admission runs in the virtual processor, which only sees `Params`.
    pub palw_audit_quorum_num: u16,
    pub palw_audit_quorum_den: u16,
    /// kaspa-pq **ADR-0040 §5.17.4 step 4 (AUTHSET-01)** — the SIZE of the beacon-selected auditor
    /// committee for a batch: the number of bonds the credential-aggregated weighted non-replacement
    /// sampler (SEL-01) draws as the eligible auditor slate, against which a certificate's
    /// `auditor_set_commitment` is re-derived and matched. Before this, only the quorum FRACTION
    /// (`palw_audit_quorum_{num,den}`) existed; a fraction cannot supply the committee's cardinality, so
    /// the AUTHSET-01 re-derivation had no way to know how many bonds `sample_auditors_by_score` should
    /// return. Lifted onto `Params` because the re-derivation runs at `verify_certificate_attestation`
    /// in the virtual processor, which only sees `Params`. Mirrors `PalwParams::auditor_count` (= 16).
    ///
    /// `Params` is not serialized, so this field does not change `LATEST_DB_VERSION` (§5.17.8).
    /// Activated presets require a non-zero value; otherwise the selected committee is empty and every
    /// vote is out of committee. `palw_activated_presets_bound_the_view` enforces this requirement.
    pub palw_audit_committee_size: u16,
    /// kaspa-pq **ADR-0040 §5.17.6 requirement (c) (SAMPLE-01)** — the number of the batch's on-chain
    /// leaves the audit round samples: the `sample_size` fed to [`crate::palw::palw_deterministic_sample`]
    /// to pick `beacon_selected_indices` over the batch's leaves, whose `receipt_da_root` values the
    /// re-derived `audit_sample_root` then commits to ([`crate::palw::palw_audit_sample_root`]). Before
    /// this, `audit_sample_root` was a producer-declared field with no re-derivation (SAMPLE-01); the
    /// SIZE of the sample had no home in `Params`, and the sampler at `verify_certificate_attestation`
    /// needs it. Lifted onto `Params` for the same reason as `palw_audit_committee_size` (the
    /// re-derivation runs in the virtual processor, which only sees `Params`).
    ///
    /// `Params` is not serialized, so this field does not change `LATEST_DB_VERSION` (§5.17.8).
    /// Activated presets require a non-zero value; a zero sample would reduce the derived root to the
    /// empty-vector constant. `palw_activated_presets_bound_the_view` enforces this requirement. The
    /// magnitude is calibrated at re-genesis.
    pub palw_audit_sample_size: u16,
    /// ADR-0045 D3-b (clauses 11–13) — the PCPB windows, in PALW epochs: `w` (challenge freshness),
    /// `k` (snapshot lag: the bond-weighted provider snapshot is fixed at `anchor − k`), `Δ` (post-
    /// commit offset: partner B is drawn from `R_{anchor + Δ}`, provably after the anchor). Mirror
    /// `PalwParams::{freshness_window_epochs, snapshot_lag_epochs, post_commit_delta_epochs}` — lifted
    /// onto `Params` because the leaf-chunk acceptance arm and `check_palw_ticket` see only `Params`
    /// (the `auditor_count → palw_audit_committee_size` pattern). `Params` is not serialized, so these
    /// do not move `LATEST_DB_VERSION`; activated presets require the `PalwParams` invariants
    /// (`k ≥ 1`, `Δ ≥ 1`, `w ≥ Δ`), enforced by `palw_activated_presets_bound_the_view`.
    pub palw_freshness_window_epochs: u64,
    pub palw_snapshot_lag_epochs: u64,
    pub palw_post_commit_delta_epochs: u64,
    /// kaspa-pq ADR-0039 PALW (§16.3): the per-lane difficulty params (window/target/min-samples/clamp
    /// + genesis lane bits). Drives the lane-aware retarget once PALW is active; the two lanes retarget
    /// independently so ticket supply and hash rate cannot manipulate each other's difficulty (§16.1).
    /// Inert placeholder (`testnet_default`, genesis bits 0) while PALW is inactive.
    pub palw_lane_difficulty: crate::palw::LaneDifficultyParams,
    /// PALW Header-v4 objective anti-spam parameters. `INERT` preserves the v3 layout on every
    /// existing preset. A non-inert value is valid only for a new public/value-network genesis.
    pub palw_spam: crate::palw_antispam::PalwSpamParams,
    /// kaspa-pq ADR-0039 PALW (§9.2/§9.3): the batch-admission bounds the mergeset-delta overlay-view
    /// builder enforces (max leaves / chunk size / registration lead / active + audit windows). Inert
    /// placeholder while PALW is inactive.
    pub palw_batch_admission: crate::palw::PalwBatchAdmissionParams,
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
    /// Review §11.2 — a domain-separated identity hash over every consensus-sensitive field of this
    /// live `Params` value, so two nodes can prove they run the SAME consensus rules (including any
    /// runtime override such as `--palw-enable-algo4`, which mutates `palw_algo4_accept` before the
    /// `Config` is shared) by comparing one value.
    ///
    /// Canonical form: an EXHAUSTIVE destructure (adding a `Params` field is a compile error here,
    /// never a silent hash gap) writes each field's derived `Debug` rendering — stable for a fixed
    /// struct definition and value — into a `field-name=value;` line protocol, then keyed
    /// BLAKE2b-512 (repo standard) over the bytes with a versioned domain. Excluded, deliberately:
    /// `dns_seeders` (peer discovery, not consensus) and `max_difficulty_target_f64` (a float
    /// derived from the already-hashed `max_difficulty_target`).
    ///
    /// This is an IDENTITY for comparison, not a wire format: the encoding is versioned by the
    /// domain string, and changing it (or any hashed field's Debug shape) legitimately changes the
    /// hash — which is exactly the alarm it exists to raise.
    #[must_use]
    pub fn consensus_identity_hash(&self) -> kaspa_hashes::Hash64 {
        use std::fmt::Write as _;
        let Params {
            dns_seeders: _,
            net,
            genesis,
            timestamp_deviation_tolerance,
            max_difficulty_target,
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
            pq_enforcement,
            pq_activation_daa_score,
            evm_activation_daa_score,
            palw_activation_daa_score,
            palw_algo4_accept,
            palw_compute_registry_activation_daa_score,
            palw_requires_archival,
            palw_requires_peer_allowlist,
            palw_compute_work_scale,
            palw_nullifier_retention_daa,
            palw_epoch_length_daa,
            palw_beacon_grace_epochs,
            palw_beacon_quorum_num,
            palw_beacon_quorum_den,
            palw_audit_quorum_num,
            palw_audit_quorum_den,
            palw_audit_committee_size,
            palw_audit_sample_size,
            palw_freshness_window_epochs,
            palw_snapshot_lag_epochs,
            palw_post_commit_delta_epochs,
            palw_lane_difficulty,
            palw_spam,
            palw_batch_admission,
            evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score,
        } = self;
        let mut canonical = String::with_capacity(4096);
        macro_rules! field {
            ($name:ident) => {
                let _ = write!(canonical, concat!(stringify!($name), "={:?};"), $name);
            };
        }
        field!(net);
        field!(genesis);
        field!(timestamp_deviation_tolerance);
        field!(max_difficulty_target);
        field!(past_median_time_window_size);
        field!(difficulty_window_size);
        field!(min_difficulty_window_size);
        field!(coinbase_payload_script_public_key_max_len);
        field!(max_coinbase_payload_len);
        field!(max_tx_inputs);
        field!(max_tx_outputs);
        field!(max_signature_script_len);
        field!(max_script_public_key_len);
        field!(mass_per_tx_byte);
        field!(mass_per_script_pub_key_byte);
        field!(mass_per_sig_op);
        field!(max_block_mass);
        field!(storage_mass_parameter);
        field!(deflationary_phase_daa_score);
        field!(pre_deflationary_phase_base_subsidy);
        field!(skip_proof_of_work);
        field!(max_block_level);
        field!(pruning_proof_m);
        field!(blockrate);
        field!(pre_crescendo_target_time_per_block);
        field!(crescendo_activation);
        field!(dns_params);
        field!(pow_blake2b_sha3_activation);
        field!(pq_enforcement);
        field!(pq_activation_daa_score);
        field!(evm_activation_daa_score);
        field!(palw_activation_daa_score);
        field!(palw_algo4_accept);
        field!(palw_compute_registry_activation_daa_score);
        field!(palw_requires_archival);
        field!(palw_requires_peer_allowlist);
        field!(palw_compute_work_scale);
        field!(palw_nullifier_retention_daa);
        field!(palw_epoch_length_daa);
        field!(palw_beacon_grace_epochs);
        field!(palw_beacon_quorum_num);
        field!(palw_beacon_quorum_den);
        field!(palw_audit_quorum_num);
        field!(palw_audit_quorum_den);
        field!(palw_audit_committee_size);
        field!(palw_audit_sample_size);
        field!(palw_freshness_window_epochs);
        field!(palw_snapshot_lag_epochs);
        field!(palw_post_commit_delta_epochs);
        field!(palw_lane_difficulty);
        field!(palw_spam);
        field!(palw_batch_admission);
        field!(evm_gas_pool_v2_activation_daa_score);
        field!(evm_f002_withdraw_cap_activation_daa_score);
        field!(evm_f003_mldsa_verify_activation_daa_score);
        field!(evm_typed_receipt_root_activation_daa_score);
        kaspa_hashes::blake2b_512_keyed(b"misaka-consensus-params-identity-v1", canonical.as_bytes())
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

    /// kaspa-pq ADR-0039 PALW: `true` when the audited-compute (algo-4) lane and Header-v3 are active at
    /// `daa_score`. Below the fence the header must be pre-v3 and its ten PALW fields must be zero.
    /// The default `u64::MAX` applies on mainnet / testnet-10 / simnet / devnet; the two PALW presets
    /// ship 0, so this returns `true` for every block there.
    #[inline]
    #[must_use]
    pub fn is_palw_active(&self, daa_score: u64) -> bool {
        daa_score >= self.palw_activation_daa_score
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
            palw_activation_daa_score: self.palw_activation_daa_score,
            palw_algo4_accept: self.palw_algo4_accept,
            palw_compute_registry_activation_daa_score: self.palw_compute_registry_activation_daa_score,
            palw_requires_archival: self.palw_requires_archival,
            palw_requires_peer_allowlist: self.palw_requires_peer_allowlist,
            palw_compute_work_scale: self.palw_compute_work_scale,
            palw_nullifier_retention_daa: self.palw_nullifier_retention_daa,
            palw_epoch_length_daa: self.palw_epoch_length_daa,
            palw_beacon_grace_epochs: self.palw_beacon_grace_epochs,
            palw_beacon_quorum_num: self.palw_beacon_quorum_num,
            palw_beacon_quorum_den: self.palw_beacon_quorum_den,
            palw_audit_quorum_num: self.palw_audit_quorum_num,
            palw_audit_quorum_den: self.palw_audit_quorum_den,
            palw_audit_committee_size: self.palw_audit_committee_size,
            palw_audit_sample_size: self.palw_audit_sample_size,
            palw_freshness_window_epochs: self.palw_freshness_window_epochs,
            palw_snapshot_lag_epochs: self.palw_snapshot_lag_epochs,
            palw_post_commit_delta_epochs: self.palw_post_commit_delta_epochs,
            palw_lane_difficulty: self.palw_lane_difficulty.clone(),
            palw_spam: self.palw_spam,
            palw_batch_admission: self.palw_batch_admission,
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
                // kaspa-pq ADR-0039: the PALW audited-compute testnet (`testnet-palw-10`).
                Some(110) => TESTNET_PALW_PARAMS,
                // kaspa-pq ADR-0048: the Header-v4 staging-mainnet PALW rehearsal net (`staging-mainnet-palw`).
                Some(200) => STAGING_MAINNET_PALW_PARAMS,
                // ADR-MA P14: the Header-v5 Compute Set registry rehearsal net (`compute-registry-palw`).
                Some(20) => COMPUTE_REGISTRY_PALW_PARAMS,
                // ADR-0045 D3-b: the PCPB dispatch rehearsal net (`pcpb-palw`) — the public PALW testnet.
                Some(21) => PCPB_PALW_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => match value.suffix {
                None => DEVNET_PARAMS,
                // kaspa-pq ADR-0039: the PALW audited-compute devnet (`devnet-palw`, `--devnet --netsuffix=111`).
                Some(111) => DEVNET_PALW_PARAMS,
                Some(x) => panic!("Devnet suffix {} is not supported", x),
            },
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
    // DNS-confirmed anchor must out-Work the canonical chain by > the EFFECTIVE emergency
    // work margin AND out-Stake it by > emergency_stake_margin (non-substitutability).
    //
    // 2026-08-01 testnet-20 bystander-wedge fix: the effective Work margin is
    // difficulty-denominated (`emergency_work_margin_for` = max_reorg_horizon_blocks × the
    // canonical tip's current per-block work + this ABSOLUTE ADDEND, kept ZERO on every
    // shipped preset). The old absolute-only margin (1_000_000 raw = "~2 devnet blocks")
    // was ~175× the entire work reachable inside the bounded ancestor walk at CPU-testnet
    // difficulty — honest bystanders 15× ahead on work with all attested stake stayed
    // DominanceViolation-wedged forever — while rounding to a fraction of ONE block (no
    // margin at all) at GPU difficulty. Work units are difficulty-scaled; only a
    // difficulty-denominated margin means the same thing on every net.
    //
    // StakeScore units ARE difficulty-independent, so the stake margin stays absolute — but
    // it is a bounded 15-epoch window on the production presets, so the margin must itself
    // fit inside that window. One full epoch preserves two-dimensional non-substitutability
    // while leaving an honest, attesting branch a reachable escape path from a stale fork.
    emergency_work_margin: BlueWorkType::ZERO,
    // One full-quality epoch. The previous 100-epoch margin exceeded the entire
    // 15-epoch StakeScore window and made emergency dominance unreachable.
    emergency_stake_margin: StakeScore(STAKE_SCORE_SCALE),
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
    // kaspa-pq DNS Dormancy Fence (design v0.1, §5.2 devnet/simnet) — functional core.
    // Inert (activation = u64::MAX): the eviction machinery is compiled but never
    // engaged, so devnet/simnet behavior is byte-identical. Small window/period for
    // fast tests (≈10 min window at the devnet epoch cadence); a full flip is instant
    // (limit = 100%). dns_v4_params_consistent() holds here.
    dormancy_activation_daa_score: u64::MAX,
    dormancy_window_epochs: 60,
    dormancy_evict_period_epochs: 10,
    dormancy_evict_limit_bps: 10_000,
    dormancy_revival_delay_epochs: 1,
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
    // 2026-08-01: absolute addend ZERO — the enforced Work margin is difficulty-denominated
    // (max_reorg_horizon_blocks × canonical-tip per-block work; see GENESIS_ACTIVE_DNS_PARAMS
    // and `emergency_work_margin_for`). The old 1_000_000 absolute was simultaneously a
    // permanent CPU-testnet bystander wedge and a no-op at real GPU difficulty.
    emergency_work_margin: BlueWorkType::ZERO,
    // One full-quality epoch: reachable inside the bounded 15-epoch score window
    // while preserving the work AND stake non-substitutability requirement.
    emergency_stake_margin: StakeScore(STAKE_SCORE_SCALE),
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
    // kaspa-pq DNS Dormancy Fence (design v0.1, §5.2 mainnet) — functional core.
    // Inert (activation = u64::MAX): compiled but never engaged, so mainnet + testnet
    // behavior is byte-identical. Window = 15 days: a validator that stops attesting
    // (no accepted attestation) for 15 days is moved to Dormant and dropped from the
    // finality denominator, then revived on its next accepted attestation. At the
    // ~10 s real-time epoch (attestation_epoch_length_blue_score = 100 blue_score/epoch
    // × 10 BPS), 15 d = 15 × 86_400 s / 10 s = 129_600 epochs. One eviction round per
    // day (8_640 epochs), rate-limited to 10 %/round of the active denominator. testnet
    // inherits these via `..PRODUCTION_DNS_PARAMS` (its epoch is also ~10 s, so the
    // window stays ≈ 15 d). dns_v4_params_consistent() holds: window·L = 12_960_000 ≥
    // unbonding (12_096_300) + max_reorg_horizon (300).
    dormancy_activation_daa_score: u64::MAX,
    dormancy_window_epochs: 129_600,
    dormancy_evict_period_epochs: 8_640,
    dormancy_evict_limit_bps: 1_000,
    dormancy_revival_delay_epochs: 1,
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
    // Testnet lowers min_active_stake and min_bond from production's 20M KAS to 10 KAS. Production's
    // `required_stake_depth = StakeScore(10 *
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

/// testnet-200 keeps the production validator-count, bond, and long-range
/// safety settings, but uses the reachable testnet WorkDepth/StakeDepth floors.
/// At the live CPU-mining difficulty the production value (1_000_000) is
/// unreachable: the anchor-relative window settles around 200-300 and PALW's
/// beacon gate therefore remains halted forever. This is intentionally narrower
/// than [`TESTNET_DNS_PARAMS`], which also lowers the staking economics.
///
/// `dns_params` is not a genesis-block input, so this coordinated testnet-200
/// upgrade preserves the existing genesis identity.
pub const STAGING_MAINNET_PALW_DNS_PARAMS: DnsParams = DnsParams {
    required_work_depth: Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]),
    // The rehearsal mesh has the production three-validator/20M-MSK economics,
    // but it must recover after planned validator restarts within a short-lived
    // PALW batch window. Use the same one-attested-epoch testnet floor while
    // retaining the production validator-count and stake-amount requirements.
    required_stake_depth: StakeScore(5000),
    ..PRODUCTION_DNS_PARAMS
};

/// testnet-200 rolling PALW batches need an active window at least as long as
/// the mandatory registration + audit lead (2 + 6 epochs). With the inherited
/// six-epoch window, a successor registered only after its predecessor becomes
/// active cannot activate before the predecessor expires. Sixteen epochs leaves
/// an eight-epoch overlap for automatic renewal while retaining all other
/// admission, bond, and bounded-view limits.
pub const STAGING_MAINNET_PALW_BATCH_ADMISSION: crate::palw::PalwBatchAdmissionParams =
    crate::palw::PalwBatchAdmissionParams { active_window_epochs: 16, ..crate::palw::PalwBatchAdmissionParams::INERT };

/// Public peer-discovery domains for the currently operated network.
///
/// These names intentionally belong to exactly one preset at a time. Reusing a
/// seed hostname across network identities makes a fresh node dial the right IP
/// on the wrong default port and obscures which chain the operator joined.
/// ADR-MA P14 migration (2026-07-30): the public PALW testnet moved from testnet-200 to
/// **testnet-20** (`compute-registry-palw`). testnet-200's replay halted because its DNS
/// finality thresholds were changed mid-chain without a DAA activation gate — a genesis-active
/// v5 re-genesis (testnet-20) is the clean recovery (the operator's "最も確実" option).
///
/// ADR-0045 D3-b migration (2026-08-01): the public PALW testnet moved again, testnet-20 →
/// **testnet-21** (`pcpb-palw`). D3-b's LeafV2 moved the leaf layout (LEAF_LEN 964 → 1189), the
/// chunk wire (v3-only) and therefore `leaf_hash → leaf_root → content_id() == batch_id` —
/// testnet-20's mined history is structurally unreplayable under the new rules, so the identity
/// tripwire's option (b) applies: re-genesis onto a new suffix. The public seed names now resolve
/// testnet-21; testnet-20 and testnet-200 keep NO seeders (deprecated, no discovery).
pub const TESTNET_21_DNS_SEEDERS: &[&str] = &["seeder1.misakascan.com", "seeder3.misakascan.com"];

pub const MAINNET_PARAMS: Params = Params {
    // Mainnet is defined but not launched. Never resolve the public testnet-200
    // seed names on mainnet's port.
    dns_seeders: &[],
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
    // table in `SUBSIDY_BY_MONTH_TABLE` (16.013224875B over 30 years at 1.4%/yr,
    // q = 0.986) applies from genesis, so `deflationary_phase_daa_score` is 0.
    // That makes `pre_deflationary_phase_base_subsidy` unused by
    // `calc_block_subsidy`; it is kept equal to the year-1 per-block subsidy at
    // 10 BPS (table[0].div_ceil(10) = 205_972_571 sompi = 2.05972571 MSK) so
    // callers reading it see the genesis rate.
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 205972571,
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
    palw_activation_daa_score: u64::MAX,
    palw_algo4_accept: false,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_requires_archival: false,
    palw_requires_peer_allowlist: false,
    palw_compute_work_scale: 0,
    palw_nullifier_retention_daa: 1_200, // ≈120 s @ 10 BPS (unused until PALW active)
    palw_epoch_length_daa: 100,          // ≈10 s @ 10 BPS
    palw_beacon_grace_epochs: 1,         // §11.3 grace (unused until PALW active)
    palw_beacon_quorum_num: 2,           // §11.2 beacon quorum 2/3 (unused until PALW active)
    palw_beacon_quorum_den: 3,
    palw_audit_quorum_num: 2, // ADR-0040 P1-3 §10.2 auditor quorum 2/3
    palw_audit_quorum_den: 3,
    palw_audit_committee_size: 16, // ADR-0040 §5.17.4 (AUTHSET-01) — mirrors PalwParams::auditor_count; inert
    palw_audit_sample_size: 16,    // ADR-0040 §5.17.6 (SAMPLE-01) — inert placeholder; magnitude is a re-genesis calibration
    palw_freshness_window_epochs: 6, // ADR-0045 D3-b — w (mirrors PalwParams::freshness_window_epochs)
    palw_snapshot_lag_epochs: 2,     // ADR-0045 D3-b — k
    palw_post_commit_delta_epochs: 2, // ADR-0045 D3-b — Δ
    palw_lane_difficulty: crate::palw::LaneDifficultyParams::INERT, // §16.3 (inert placeholder)
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    palw_batch_admission: crate::palw::PalwBatchAdmissionParams::INERT, // §9.2/§9.3 (inert placeholder)
    // gas-pool v2 ships inert on every network — a deploy sets a finite testnet score.
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
};

pub const TESTNET_PARAMS: Params = Params {
    // testnet-10 is retained as a compatibility preset, but its public mesh is
    // retired. Discovery moved to testnet-200; an explicitly configured legacy
    // peer can still be used for offline recovery.
    dns_seeders: &[],
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
    // table in `SUBSIDY_BY_MONTH_TABLE` (16.013224875B over 30 years at 1.4%/yr,
    // q = 0.986) applies from genesis, so `deflationary_phase_daa_score` is 0.
    // That makes `pre_deflationary_phase_base_subsidy` unused by
    // `calc_block_subsidy`; it is kept equal to the year-1 per-block subsidy at
    // 10 BPS (table[0].div_ceil(10) = 205_972_571 sompi = 2.05972571 MSK) so
    // callers reading it see the genesis rate.
    deflationary_phase_daa_score: 0,
    pre_deflationary_phase_base_subsidy: 205972571,
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
    palw_activation_daa_score: u64::MAX,
    palw_algo4_accept: false,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_requires_archival: false,
    palw_requires_peer_allowlist: false,
    palw_compute_work_scale: 0,
    palw_nullifier_retention_daa: 1_200, // ≈120 s @ 10 BPS (unused until PALW active)
    palw_epoch_length_daa: 100,          // ≈10 s @ 10 BPS
    palw_beacon_grace_epochs: 1,         // §11.3 grace (unused until PALW active)
    palw_beacon_quorum_num: 2,           // §11.2 beacon quorum 2/3 (unused until PALW active)
    palw_beacon_quorum_den: 3,
    palw_audit_quorum_num: 2, // ADR-0040 P1-3 §10.2 auditor quorum 2/3
    palw_audit_quorum_den: 3,
    palw_audit_committee_size: 16, // ADR-0040 §5.17.4 (AUTHSET-01) — mirrors PalwParams::auditor_count; inert
    palw_audit_sample_size: 16,    // ADR-0040 §5.17.6 (SAMPLE-01) — inert placeholder; magnitude is a re-genesis calibration
    palw_freshness_window_epochs: 6, // ADR-0045 D3-b — w (mirrors PalwParams::freshness_window_epochs)
    palw_snapshot_lag_epochs: 2,     // ADR-0045 D3-b — k
    palw_post_commit_delta_epochs: 2, // ADR-0045 D3-b — Δ
    palw_lane_difficulty: crate::palw::LaneDifficultyParams::INERT, // §16.3 (inert placeholder)
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    palw_batch_admission: crate::palw::PalwBatchAdmissionParams::INERT, // §9.2/§9.3 (inert placeholder)
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

/// kaspa-pq ADR-0039 PALW: the dedicated audited-compute testnet (`testnet-palw-10`, NetworkId
/// `testnet-110`). Inherits testnet-10's 10-BPS profile but with its OWN genesis + network id so PALW
/// measurements stay isolated from testnet-10 / testnet-40. PALW starts inert
/// (`palw_activation_daa_score = u64::MAX`, inherited) — the network runs the permanent algo-3 hash
/// floor at 10 BPS until a weight-0 activation re-genesis. Additive: no existing network is touched.
/// ADR-0039 — the activation-ready lane difficulty for the PALW-ACTIVE testnet (`testnet-palw-10`).
/// `genesis_hash_bits` MUST equal `TESTNET_PALW_GENESIS.bits` (the max-easy `0x207fffff` fast-start target
/// this activation re-genesis carries, so single-node algo-3 mining is fast) for §16.3
/// `is_consistent_for_activation`; `genesis_replica_bits` is likewise max-easy so the §14 clause-9
/// eligibility draw is winnable by grinding a couple of nullifiers.
pub const TESTNET_PALW_LANE_DIFFICULTY: crate::palw::LaneDifficultyParams = crate::palw::LaneDifficultyParams {
    genesis_hash_bits: 0x207fffff,
    genesis_replica_bits: 0x207fffff,
    ..crate::palw::LaneDifficultyParams::INERT
};

/// testnet-palw tunes the DNS anchor windows small (like devnet-palw) so a finality-buried v3 anchor
/// resolves on a short supporting chain; other DNS fields inherit [`TESTNET_DNS_PARAMS`]. Not a genesis
/// input (no re-genesis). Stays `dns_v3_params_consistent`.
pub const TESTNET_PALW_DNS_PARAMS: DnsParams = DnsParams {
    attestation_epoch_length_blue_score: 4,
    attestation_lag_blue_score: 2,
    attestation_anchor_backoff_blue_score: 1,
    ..TESTNET_DNS_PARAMS
};

/// kaspa-pq ADR-0039 PALW: the PALW-ACTIVE audited-compute testnet (`testnet-palw-10`, NetworkId
/// `testnet-110`). PALW (algo-4 proof-of-LLM) is ACTIVE from genesis (`palw_activation_daa_score = 0`).
/// Unlike devnet-palw this keeps **real** Layer-0 PoW for the algo-3 supporting lane (`skip_proof_of_work`
/// stays false) — the easy `0x1f7fffff` fast-start target + the pinned difficulty window make single-node
/// mining fast, and algo-4 headers are EXEMPT from the hash floor (their PoW is the k=2 replica match +
/// clause-9 eligibility draw; see `check_pow_and_calc_block_level`). EVM off so a non-evm kaspad build
/// runs it. Genesis hash is UNCHANGED from the inert testnet-palw (only params activate; none of these
/// fields is a genesis-block input).
pub const TESTNET_PALW_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Testnet, 110),
    genesis: crate::config::genesis::TESTNET_PALW_GENESIS,
    dns_seeders: &[],
    palw_activation_daa_score: 0,
    palw_algo4_accept: true,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_requires_archival: true,
    palw_requires_peer_allowlist: true,
    palw_lane_difficulty: TESTNET_PALW_LANE_DIFFICULTY,
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    // Stage A: algo-4 acceptance/measurement is independent from fork-choice credit.
    palw_compute_work_scale: 0,
    pow_blake2b_sha3_activation: ForkActivation::always(),
    evm_activation_daa_score: u64::MAX,
    // Never retarget away from the easy fast-start bits on the demo chain (keeps single-node mining fast).
    min_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    dns_params: Some(TESTNET_PALW_DNS_PARAMS),
    ..TESTNET_PARAMS
};

/// ADR-0039 P0 — the activation-ready lane-difficulty a single-node **devnet-palw** net carries: `INERT`
/// windows/rates + **real** genesis bits (max-easy `0x207fffff` so Layer-0 PoW grinds instantly on a
/// throwaway net; the replica lane easy so the §14 clause-9 eligibility draw is winnable by grinding a
/// couple of nullifiers). The devnet-palw genesis header MUST be built with `bits ==
/// DEVNET_PALW_GENESIS_BITS`, so the §16.3 re-genesis preflight (`is_consistent_for_activation`) holds —
/// unlike the E2E harness shortcut (`min_samples` above the windows), which never called that predicate.
pub const DEVNET_PALW_GENESIS_BITS: u32 = 0x207fffff;
pub const DEVNET_PALW_LANE_DIFFICULTY: crate::palw::LaneDifficultyParams = crate::palw::LaneDifficultyParams {
    genesis_hash_bits: DEVNET_PALW_GENESIS_BITS,
    genesis_replica_bits: DEVNET_PALW_GENESIS_BITS,
    ..crate::palw::LaneDifficultyParams::INERT
};

/// devnet-palw tunes the DNS anchor windows small so a finality-buried v3 anchor (the clause-6/9
/// checkpoint the algo-4 lane draws from) resolves within a short supporting chain — a running single-node
/// demo need not mine ~epoch-length blocks first. Other DNS fields inherit the shared
/// [`GENESIS_ACTIVE_DNS_PARAMS`]; stays `dns_v3_params_consistent`. Not a genesis input (no re-genesis).
pub const DEVNET_PALW_DNS_PARAMS: DnsParams = DnsParams {
    attestation_epoch_length_blue_score: 4,
    attestation_lag_blue_score: 2,
    attestation_anchor_backoff_blue_score: 1,
    ..GENESIS_ACTIVE_DNS_PARAMS
};

/// ADR-0039 P0 — the PALW-active single-node **devnet-palw** preset (`--devnet --netsuffix=111`).
/// PALW audited-compute lane (algo-4) is ACTIVE from genesis. Derived from [`DEVNET_PARAMS`] with the
/// activation recipe proven by the in-process E2E (`palw_algo4_real_inference_e2e`): PALW active, max-easy
/// genesis/replica bits, `skip_proof_of_work` (algo-4 pins `nonce == low64(nullifier)`, incompatible with a
/// real Layer-0 hash-floor), BLAKE2b-SHA3 algo-3 supporting blocks, and EVM OFF so a default (non-evm)
/// kaspad build runs it. Inherits `palw_epoch_length_daa = 100`, `palw_beacon_grace_epochs = 1`, and the
/// v3-consistent `GENESIS_ACTIVE_DNS_PARAMS` from DEVNET. `palw_compute_work_scale = 0` (Stage-A: accept +
/// measure, no fork-choice credit — single node has no competing chain).
pub const DEVNET_PALW_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Devnet, 111),
    genesis: crate::config::genesis::DEVNET_PALW_GENESIS,
    dns_seeders: &[],
    palw_activation_daa_score: 0,
    palw_algo4_accept: true,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    // Devnet permits normal pruning; archival nodes opt in when full history is required.
    palw_requires_archival: false,
    palw_requires_peer_allowlist: true,
    palw_lane_difficulty: DEVNET_PALW_LANE_DIFFICULTY,
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    palw_compute_work_scale: 0,
    skip_proof_of_work: true,
    pow_blake2b_sha3_activation: ForkActivation::always(),
    evm_activation_daa_score: u64::MAX,
    // Never retarget away from the max-easy genesis bits on the short demo chain.
    min_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    // Small DNS anchor windows so a finality-buried v3 anchor resolves on a short chain (Stage 5).
    dns_params: Some(DEVNET_PALW_DNS_PARAMS),
    // Devnet-scale retention. The inherited 10-BPS constants put pruning_depth at
    // 1,080,000 blue-score units — at this devnet's REAL ~1 block/s that is ~12 days
    // before the pruning point moves AT ALL, so "pruning enabled" would still mean
    // unbounded growth for any realistic soak. 7,200 / 21,600 (~2 h / ~6 h at 1 BPS)
    // bound the retained window to a few hundred MB while staying far above every
    // PALW validation window (DA retention 2,000; challenge deadline D+202; beacon
    // burial 100; batch admission ≈ 20 epochs = 2,000; the paid-work walk bound —
    // `palw_admission_windows_fit_the_pruning_depth` still asserts that relation).
    // The theoretical 10-BPS anticone lower bound does not bind a closed 1-BPS
    // two-node mesh with width-1 chains.
    blockrate: {
        let mut b = BlockrateParams::new::<10>();
        b.finality_depth = 7_200;
        b.pruning_depth = 21_600;
        b
    },
    ..DEVNET_PARAMS
};

/// ADR-0048 — the activation-ready lane difficulty the staging-mainnet rehearsal net carries.
/// Mirrors [`TESTNET_PALW_LANE_DIFFICULTY`] (INERT windows/rates + max-easy genesis bits on both
/// lanes) as its own constant so the ADR-0046 L1/L2 staging re-measurement can recalibrate it
/// without touching testnet-palw-110. `genesis_hash_bits` MUST equal `STAGING_PALW_GENESIS.bits`
/// (§16.3 `is_consistent_for_activation`); the replica lane is likewise max-easy so the §14
/// clause-9 eligibility draw is winnable from a cold start.
pub const STAGING_PALW_LANE_DIFFICULTY: crate::palw::LaneDifficultyParams = crate::palw::LaneDifficultyParams {
    genesis_hash_bits: 0x207fffff,
    genesis_replica_bits: 0x207fffff,
    ..crate::palw::LaneDifficultyParams::INERT
};

/// ADR-0048 — the Header-v4 **staging-mainnet** PALW rehearsal preset (`staging-mainnet-palw`,
/// NetworkId `testnet-200`, `--testnet --netsuffix=200`). Header-v4 is a one-way re-genesis
/// boundary, so the final mainnet identity (ADR-0041) is preceded by this SAME-SHAPE rehearsal
/// network; success means its frozen params/genesis shape is copied verbatim into the future
/// `MAINNET_PALW_*` (values only, no new design).
///
/// The ADR-0041 mainnet shape, on an independent identity:
///   * **v4 genesis** ([`crate::config::genesis::STAGING_PALW_GENESIS`]) — the spam-accumulator
///     commitment is bound into the genesis hash; PALW is genesis-active
///     (`palw_activation_daa_score = 0`).
///   * **NON-inert `palw_spam`** — the FIRST preset to ship it. `PUBLIC_REGENESIS_CANDIDATE` is a
///     deliberate starting point; ADR-0046 L1 staging measurements recalibrate the magnitude.
///     Satisfies the HeaderProcessor v4 deployment fence (structurally valid + genesis-active +
///     `genesis.version == 4`).
///   * `palw_algo4_accept = true` and `palw_compute_work_scale = 0` (weight-0 start).
///   * **Real PoW** (`skip_proof_of_work = false`) in the testnet-palw shape: algo-3 blocks grind
///     the real Layer-0 hash floor from the max-easy fast-start target, algo-4 blocks are
///     hash-floor-exempt structurally (`check_pow_and_calc_block_level`) — no devnet skip-pow.
///   * **Full-scale depths** via `..MAINNET_PARAMS`: finality 432_000 / pruning 1_080_000 — the
///     mainnet 想定値, deliberately NOT shrunk (縮小しない実物大), so the pruning first-pass and
///     warm-up-window exercises measure the real thing.
///   * `palw_requires_archival = false` and `palw_requires_peer_allowlist = false`.
///   * ADR-0043 note: G6 sibling flooding is bounded by the amended allocation-policy fix, which is
///     network-independent — there is no per-preset knob here.
///
/// Everything not listed inherits MAINNET_PARAMS (production DNS overlay economics, 10-BPS
/// blockrate, PALW audit/committee/epoch values — identical across mainnet/testnet presets today).
pub const STAGING_MAINNET_PALW_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Testnet, 200),
    genesis: crate::config::genesis::STAGING_PALW_GENESIS,
    // DEPRECATED (2026-07-30): superseded by testnet-20 (compute-registry-palw). Kept compilable
    // for any node still holding its ledger, but no longer publicly seeded — its replay is unsafe
    // (mid-chain DNS-threshold change, no DAA gate). Migrate to `--testnet --netsuffix=20`.
    dns_seeders: &[],
    palw_activation_daa_score: 0,
    palw_algo4_accept: true,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_compute_work_scale: 0,
    palw_spam: crate::palw_antispam::PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE,
    skip_proof_of_work: false,
    palw_requires_archival: false,
    // The staging network accepts unlisted peers for public testnet validation.
    palw_requires_peer_allowlist: false,
    palw_lane_difficulty: STAGING_PALW_LANE_DIFFICULTY,
    palw_batch_admission: STAGING_MAINNET_PALW_BATCH_ADMISSION,
    dns_params: Some(STAGING_MAINNET_PALW_DNS_PARAMS),
    ..MAINNET_PARAMS
};

/// ADR-MA P14 — the **Compute Set registry rehearsal** network (`compute-registry-palw`,
/// NetworkId `testnet-20`, `--testnet --netsuffix=20`). The FIRST registry-active preset: the
/// staging-mainnet (testnet-200) shape with the ADR-MA fence OPEN from genesis —
///   * **v5 genesis** ([`crate::config::genesis::COMPUTE_REGISTRY_PALW_GENESIS`]): the three
///     Compute Set references enter the hash preimage at block 0 (all-zero — genesis names no
///     set), so model registration NEVER changes the header schema again (§13).
///   * `palw_compute_registry_activation_daa_score = 0` — band 0x40-0x44 admits from genesis,
///     Header v5 is the only admitted schema, per-set difficulty and the GHOSTDAG credit seam
///     are live paths.
///   * DNS seeders: the PUBLIC seed names (2026-07-30 migration — this preset superseded
///     testnet-200 as the public net; the field below is authoritative). It began as a
///     closed two-host rehearsal dialling peers explicitly (`--addpeer`).
///   * Everything else inherits the staging shape verbatim (real PoW, non-inert anti-spam,
///     full-scale depths): the rehearsal exercises the registry, not a new economics surface.
pub const COMPUTE_REGISTRY_PALW_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Testnet, 20),
    genesis: COMPUTE_REGISTRY_PALW_GENESIS,
    // DEPRECATED (2026-08-01): superseded by testnet-21 (pcpb-palw). ADR-0045 D3-b moved the leaf
    // format (LeafV2 964→1189, chunk wire v3-only, batch ids re-derived), so this net's mined
    // history cannot replay under the new rules. Kept compilable for any node still holding its
    // ledger, but no longer publicly seeded. Migrate with `--testnet --netsuffix=21` on a fresh
    // datadir.
    dns_seeders: &[],
    // ADR-MA: the registry fence — OPEN from genesis (this net rehearses the Compute Set registry
    // as well as the mainnet shape it inherits from STAGING).
    palw_compute_registry_activation_daa_score: 0,
    dns_params: Some(COMPUTE_REGISTRY_DNS_PARAMS),
    ..STAGING_MAINNET_PALW_PARAMS
};

/// ADR-0045 D3-b — the **PCPB dispatch** rehearsal network (`pcpb-palw`, NetworkId `testnet-21`,
/// `--testnet --netsuffix=21`). The compute-registry shape carried through the D3-b re-genesis
/// train — the first preset whose ledger is minted entirely under the LeafV2 rules:
///   * **LeafV2 + chunk v3 + clauses 11/12/13 live from block 0**: every stored leaf passed the
///     acceptance-time challenge re-derivation and dispatch-evidence re-run, and every algo-4
///     mint re-checks the clause-13 binding. There is no pre-PCPB history to carry.
///   * **PCPB windows are consensus params** (`palw_freshness_window_epochs` w /
///     `palw_snapshot_lag_epochs` k / `palw_post_commit_delta_epochs` Δ) — they are part of this
///     net's identity hash, which is why D3-b could not land on testnet-20 in place (the identity
///     tripwire fired, and its option (b) — re-genesis onto a new suffix — is this preset).
///   * **v5 genesis** ([`crate::config::genesis::PCPB_PALW_GENESIS`]): D3-b changes the leaf
///     payload, never the header schema, so the header stays Header-v5 with the registry fence
///     open from genesis.
///   * Mint floor (design memo §10.2): the first mintable `registered_epoch` is `k + Δ` (= 4 with
///     the shipped windows) — producers must not register batches before epoch 4; the early
///     epochs are structurally algo-4-empty, which is fail-closed, not a fault.
///   * DNS seeders: the PUBLIC seed names (2026-08-01 migration — this preset supersedes
///     testnet-20 as the public net; the field below is authoritative).
pub const PCPB_PALW_PARAMS: Params = Params {
    net: NetworkId::with_suffix(NetworkType::Testnet, 21),
    genesis: crate::config::genesis::PCPB_PALW_GENESIS,
    dns_seeders: TESTNET_21_DNS_SEEDERS,
    ..COMPUTE_REGISTRY_PALW_PARAMS
};

/// ADR-MA P14 rehearsal DNS economics: the staging (production-scale) shape with the
/// validator-ENTRY floors at the testnet scale. The rehearsal validates the REGISTRY —
/// §17.3's validator quorum needs real, mining-fundable stake bonds within minutes of a fresh
/// re-genesis, and 20M-MSK floors on a ~2-MSK-coinbase chain would take ~10M blocks to fund
/// (the TESTNET_DNS_PARAMS rationale verbatim). testnet-200 keeps rehearsing the production
/// economics; this net rehearses the Compute Set machinery.
pub const COMPUTE_REGISTRY_DNS_PARAMS: DnsParams = DnsParams {
    min_bond_amount_sompi: 10 * SOMPI_PER_KASPA,
    min_active_stake_sompi: 10 * SOMPI_PER_KASPA,
    // The two-host rehearsal runs one validator per host; the quorum meaning stays 2/3 by stake.
    min_active_validators: 1,
    ..STAGING_MAINNET_PALW_DNS_PARAMS
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
    pre_deflationary_phase_base_subsidy: 205972571,
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
    palw_activation_daa_score: u64::MAX,
    palw_algo4_accept: false,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_requires_archival: false,
    palw_requires_peer_allowlist: false,
    palw_compute_work_scale: 0,
    palw_nullifier_retention_daa: 1_200, // ≈120 s @ 10 BPS (unused until PALW active)
    palw_epoch_length_daa: 100,          // ≈10 s @ 10 BPS
    palw_beacon_grace_epochs: 1,         // §11.3 grace (unused until PALW active)
    palw_beacon_quorum_num: 2,           // §11.2 beacon quorum 2/3 (unused until PALW active)
    palw_beacon_quorum_den: 3,
    palw_audit_quorum_num: 2, // ADR-0040 P1-3 §10.2 auditor quorum 2/3
    palw_audit_quorum_den: 3,
    palw_audit_committee_size: 16, // ADR-0040 §5.17.4 (AUTHSET-01) — mirrors PalwParams::auditor_count; inert
    palw_audit_sample_size: 16,    // ADR-0040 §5.17.6 (SAMPLE-01) — inert placeholder; magnitude is a re-genesis calibration
    palw_freshness_window_epochs: 6, // ADR-0045 D3-b — w (mirrors PalwParams::freshness_window_epochs)
    palw_snapshot_lag_epochs: 2,     // ADR-0045 D3-b — k
    palw_post_commit_delta_epochs: 2, // ADR-0045 D3-b — Δ
    palw_lane_difficulty: crate::palw::LaneDifficultyParams::INERT, // §16.3 (inert placeholder)
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    palw_batch_admission: crate::palw::PalwBatchAdmissionParams::INERT, // §9.2/§9.3 (inert placeholder)
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
    palw_activation_daa_score: u64::MAX,
    palw_algo4_accept: false,
    // ADR-MA Compute Set registry: not yet activated on any shipped preset (Header v5 + 0x40 band inert).
    palw_compute_registry_activation_daa_score: u64::MAX,
    palw_requires_archival: false,
    palw_requires_peer_allowlist: false,
    palw_compute_work_scale: 0,
    palw_nullifier_retention_daa: 1_200, // ≈120 s @ 10 BPS (unused until PALW active)
    palw_epoch_length_daa: 100,          // ≈10 s @ 10 BPS
    palw_beacon_grace_epochs: 1,         // §11.3 grace (unused until PALW active)
    palw_beacon_quorum_num: 2,           // §11.2 beacon quorum 2/3 (unused until PALW active)
    palw_beacon_quorum_den: 3,
    palw_audit_quorum_num: 2, // ADR-0040 P1-3 §10.2 auditor quorum 2/3
    palw_audit_quorum_den: 3,
    palw_audit_committee_size: 16, // ADR-0040 §5.17.4 (AUTHSET-01) — mirrors PalwParams::auditor_count; inert
    palw_audit_sample_size: 16,    // ADR-0040 §5.17.6 (SAMPLE-01) — inert placeholder; magnitude is a re-genesis calibration
    palw_freshness_window_epochs: 6, // ADR-0045 D3-b — w (mirrors PalwParams::freshness_window_epochs)
    palw_snapshot_lag_epochs: 2,     // ADR-0045 D3-b — k
    palw_post_commit_delta_epochs: 2, // ADR-0045 D3-b — Δ
    palw_lane_difficulty: crate::palw::LaneDifficultyParams::INERT, // §16.3 (inert placeholder)
    palw_spam: crate::palw_antispam::PalwSpamParams::INERT,
    palw_batch_admission: crate::palw::PalwBatchAdmissionParams::INERT, // §9.2/§9.3 (inert placeholder)
    // EVM is genesis-active here, but the gas-pool v2 executor stays inert until a
    // deploy sets a finite activation score (consensus fork — see params docs).
    evm_gas_pool_v2_activation_daa_score: u64::MAX,
    evm_f002_withdraw_cap_activation_daa_score: u64::MAX,
    evm_f003_mldsa_verify_activation_daa_score: u64::MAX,
    evm_typed_receipt_root_activation_daa_score: u64::MAX,
    // The former shared devnet is retired. Public discovery belongs exclusively
    // to testnet-200.
    dns_seeders: &[],
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
    pre_deflationary_phase_base_subsidy: 205972571,
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

#[cfg(test)]
mod consensus_identity_hash_tests {
    use super::*;

    /// §11.2 — the identity hash is deterministic, distinguishes presets, reacts to the runtime
    /// algo4 override (the whole point: two nodes disagreeing on `--palw-enable-algo4` must show
    /// different identities), and ignores the deliberately-excluded non-consensus fields.
    #[test]
    fn identity_hash_is_deterministic_sensitive_and_excludes_non_consensus() {
        let base = Params::from(NetworkId::with_suffix(NetworkType::Devnet, 111));
        assert_eq!(base.consensus_identity_hash(), base.consensus_identity_hash(), "deterministic on one value");
        assert_eq!(base.consensus_identity_hash(), base.clone().consensus_identity_hash(), "clone-stable");

        // Distinguishes presets (different genesis/params).
        let mainnet = Params::from(NetworkId::new(NetworkType::Mainnet));
        assert_ne!(base.consensus_identity_hash(), mainnet.consensus_identity_hash(), "distinguishes presets");

        // Reacts to the runtime acceptance override (--palw-enable-algo4 mutates this field).
        let mut flipped = base.clone();
        flipped.palw_algo4_accept = !flipped.palw_algo4_accept;
        assert_ne!(base.consensus_identity_hash(), flipped.consensus_identity_hash(), "algo4 override changes identity");

        // Excluded fields do NOT move the hash: dns_seeders (discovery) and the derived f64.
        let mut seeders = base.clone();
        seeders.dns_seeders = &["example.invalid"];
        assert_eq!(base.consensus_identity_hash(), seeders.consensus_identity_hash(), "dns_seeders excluded");
        let mut f64_only = base.clone();
        f64_only.max_difficulty_target_f64 = 12345.0;
        assert_eq!(base.consensus_identity_hash(), f64_only.consensus_identity_hash(), "derived f64 excluded");
    }
}

#[cfg(test)]
mod palw_network_tests {
    use super::*;

    /// Regression for the 2026-07-19 permanent virtual-sink wedge: the former
    /// 100-epoch emergency margin could never fit inside the 15-epoch StakeScore
    /// window, making `DominanceSatisfied` unreachable regardless of honest stake.
    #[test]
    fn dns_emergency_stake_margin_is_reachable_inside_the_window() {
        for (name, dns) in [("genesis-active", GENESIS_ACTIVE_DNS_PARAMS), ("production", PRODUCTION_DNS_PARAMS)] {
            let complete_epochs = dns.stake_score_window_blue_score / dns.attestation_epoch_length_blue_score.max(1);
            let maximum_window_score = complete_epochs as u128 * STAKE_SCORE_SCALE;
            assert!(complete_epochs > 1, "{name}: fixture must contain multiple StakeScore epochs");
            assert_eq!(dns.emergency_stake_margin, StakeScore(STAKE_SCORE_SCALE), "{name}: escape margin is one full epoch");
            assert!(
                dns.emergency_stake_margin.0 < maximum_window_score,
                "{name}: emergency stake margin {} must be reachable inside max window score {maximum_window_score}",
                dns.emergency_stake_margin.0
            );
        }
    }

    /// Regression for the 2026-08-01 testnet-20 permanent bystander wedge — the Work-side
    /// sibling of the stake-margin tripwire above. The margin the reorg gate enforces is
    /// difficulty-denominated (`emergency_work_margin_for`: `max_reorg_horizon_blocks` × the
    /// canonical tip's per-block work), which fits inside the bounded common-ancestor walk
    /// (`max(horizon, stake window) ≥ horizon`) at ANY difficulty. An absolute constant
    /// cannot: the old 1_000_000 raw was ~175× the whole walk's work at testnet-20's CPU
    /// difficulty, so honest bystanders (candidate_work=5726 vs canonical_work=362, all
    /// attested stake) held their stale forks forever. The per-preset field is therefore only
    /// an OPTIONAL ABSOLUTE ADDEND and must stay ZERO on every shipped preset — any nonzero
    /// value re-arms the wedge on whichever net's difficulty makes it unreachable.
    #[test]
    fn dns_emergency_work_margin_absolute_addend_stays_zero() {
        for (name, dns) in [
            ("genesis-active", GENESIS_ACTIVE_DNS_PARAMS),
            ("production", PRODUCTION_DNS_PARAMS),
            ("testnet", TESTNET_DNS_PARAMS),
            ("staging-mainnet-palw (testnet-200)", STAGING_MAINNET_PALW_DNS_PARAMS),
            ("testnet-palw (testnet-110)", TESTNET_PALW_DNS_PARAMS),
            ("devnet-palw (devnet-111)", DEVNET_PALW_DNS_PARAMS),
            ("compute-registry (testnet-20, deprecated / testnet-21 pcpb-palw)", COMPUTE_REGISTRY_DNS_PARAMS),
        ] {
            assert_eq!(
                dns.emergency_work_margin,
                BlueWorkType::ZERO,
                "{name}: the absolute Work-margin addend must stay ZERO — work units are \
                 difficulty-scaled, so any absolute constant re-arms the bystander wedge"
            );
            assert!(
                dns.max_reorg_horizon_blocks > 0,
                "{name}: max_reorg_horizon_blocks must be positive so the difficulty-denominated \
                 emergency Work margin stays a real (non-trivial) dominance requirement"
            );
        }
    }

    /// Header-v4 is deliberately re-genesis-only. No LEGACY identity may silently acquire its
    /// serialization, stamp cost, or accumulator database merely because the implementation lands.
    /// ADR-0048 ships the ONE deliberate exception: `staging-mainnet-palw` (`testnet-200`) IS a
    /// Header-v4 re-genesis — the same-shape rehearsal of the ADR-0041 mainnet identity — so it is
    /// asserted on the OTHER side of the boundary: non-inert spam params that satisfy the v4
    /// deployment fence trio the `HeaderProcessor` constructor enforces (mirrored from
    /// `consensus/src/pipeline/header_processor/processor.rs`), not inertness.
    #[test]
    fn palw_header_v4_antispam_is_inert_on_every_shipped_preset_except_the_staging_regenesis() {
        for (name, p) in [
            ("mainnet", MAINNET_PARAMS),
            ("testnet-10", TESTNET_PARAMS),
            ("testnet-palw-110", TESTNET_PALW_PARAMS),
            ("devnet-palw-111", DEVNET_PALW_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
        ] {
            assert!(p.palw_spam.is_inert(), "{name} must remain Header-v4-inert");
            assert!(
                p.genesis.version < crate::constants::PALW_ANTISPAM_HEADER_VERSION,
                "{name} must not reuse or exceed the public/value re-genesis schema"
            );
        }

        // ADR-0048: the staging re-genesis is the single shipped v4 / non-inert preset, and it must
        // satisfy the construction fence a node applies before processing any header — a non-inert
        // `palw_spam` requires a structurally valid, PALW-genesis-active, version-4 genesis.
        let staging = STAGING_MAINNET_PALW_PARAMS;
        assert!(!staging.palw_spam.is_inert(), "staging-mainnet-palw must ship NON-inert v4 anti-spam params");
        assert_eq!(
            staging.palw_spam,
            crate::palw_antispam::PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE,
            "staging ships the candidate calibration until the ADR-0046 L1 re-measurement"
        );
        assert!(staging.palw_spam.is_structurally_valid(), "v4 fence (1/3): structural validity");
        assert!(
            staging.palw_activation_daa_score <= staging.genesis.daa_score,
            "v4 fence (2/3): PALW active at (or before) the genesis DAA score"
        );
        assert_eq!(
            staging.genesis.version,
            crate::constants::PALW_ANTISPAM_HEADER_VERSION,
            "v4 fence (3/3): the genesis itself carries the Header-v4 schema"
        );

        let candidate = crate::palw_antispam::PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE;
        assert!(candidate.is_structurally_valid());
        assert!(candidate.base_stamp_bits > 0);
        assert!(candidate.max_stamp_bits >= candidate.base_stamp_bits);
        assert!(crate::constants::PALW_ANTISPAM_HEADER_VERSION > crate::constants::PALW_HEADER_VERSION);
    }

    /// ADR-0039: the PALW audited-compute testnet (`testnet-110`) selects TESTNET_PALW_PARAMS with its
    /// OWN genesis, a distinct network id, the inherited 10-BPS profile, and PALW inert (weight-0 start).
    #[test]
    fn testnet_palw_network_selection() {
        let net = NetworkId::with_suffix(NetworkType::Testnet, 110);
        let p: Params = net.into();
        assert_eq!(p.net, net);
        assert_eq!(p.net.suffix, Some(110));
        // distinct genesis from testnet-10 (separate ledger / measurements).
        assert_eq!(p.genesis.hash, crate::config::genesis::TESTNET_PALW_GENESIS.hash);
        assert_ne!(p.genesis.hash, TESTNET_PARAMS.genesis.hash);
        // inherits the 10-BPS testnet profile.
        assert_eq!(p.bps(), TESTNET_PARAMS.bps());
        // ADR-0039: testnet-palw is now PALW-ACTIVE (proof-of-LLM on testnet) — algo-4 from genesis.
        assert!(p.is_palw_active(0), "testnet-palw is PALW-active from genesis");
        assert_eq!(p.palw_activation_daa_score, 0);
        assert_eq!(p.palw_compute_work_scale, 0, "Stage-A PALW compute credit stays weight zero");
        // Keeps REAL Layer-0 PoW for the algo-3 supporting lane (no skip_proof_of_work crutch); algo-4 is
        // exempt from the hash floor in `check_pow_and_calc_block_level` (its PoW is the k=2 match + draw).
        assert!(!p.skip_proof_of_work, "testnet-palw uses real algo-3 PoW; algo-4 is exempt in the pipeline");
        assert!(p.pow_blake2b_sha3_activation.is_active(0), "algo-3 supporting blocks are v3 BLAKE2b-SHA3");
        assert_eq!(p.evm_activation_daa_score, u64::MAX, "EVM off so a non-evm kaspad build runs testnet-palw");
        assert_eq!(p.genesis.bits, TESTNET_PALW_LANE_DIFFICULTY.genesis_hash_bits, "§16.3 genesis-bits invariant");
        assert!(TESTNET_PALW_LANE_DIFFICULTY.is_consistent_for_activation(p.genesis.bits));
        assert!(p.dns_params.unwrap().dns_v3_params_consistent(), "tuned testnet-palw DNS params stay v3-consistent");
        // testnet-10 (suffix 10) stays PALW-inert (only testnet-palw activates).
        let t10: Params = NetworkId::with_suffix(NetworkType::Testnet, 10).into();
        assert_eq!(t10.palw_activation_daa_score, u64::MAX);
        assert!(!t10.is_palw_active(0));
    }

    /// ADR-MA P14 / ADR-0045 D3-b: `--testnet --netsuffix=20` selects the Header-v5 Compute Set
    /// registry rehearsal preset. DEPRECATED (2026-08-01): superseded as the public net by
    /// testnet-21 (`pcpb-palw`) — D3-b moved the leaf format (LeafV2 964→1189, chunk wire
    /// v3-only, batch ids re-derived), so this net's mined history cannot replay under the new
    /// rules, and its 2026-08-01 fork family additionally left probabilistic dead-branch anchor
    /// traps for fresh syncs (the bystander-wedge incident). Kept compilable for any node still
    /// holding its ledger; the live tripwire (identity pin + threshold pins) moved to
    /// `pcpb_palw_network_selection`, which guards the successor.
    #[test]
    fn compute_registry_palw_network_selection() {
        let net = NetworkId::with_suffix(NetworkType::Testnet, 20);
        let p: Params = net.into();
        assert_eq!(p.net, net);
        assert_eq!(p.net.suffix, Some(20));
        // Its OWN v5 genesis — a ledger distinct from staging and every legacy identity.
        assert_eq!(p.genesis.hash, crate::config::genesis::COMPUTE_REGISTRY_PALW_GENESIS.hash);
        assert_ne!(p.genesis.hash, STAGING_MAINNET_PALW_PARAMS.genesis.hash);
        assert_ne!(p.genesis.hash, TESTNET_PARAMS.genesis.hash);
        assert_eq!(p.genesis.version, crate::constants::PALW_COMPUTE_SET_HEADER_VERSION, "Header-v5 re-genesis");
        // The registry fence stayed OPEN from genesis (the shape testnet-21 inherits).
        assert_eq!(p.palw_compute_registry_activation_daa_score, 0);
        assert!(p.is_palw_active(0));
        assert_eq!(p.palw_activation_daa_score, 0);
        assert!(p.palw_algo4_accept);
        assert!(!p.skip_proof_of_work);
        // 2026-08-01 migration: deprecated nets must not be publicly discovered — the public seed
        // names resolve testnet-21 now.
        assert!(p.dns_seeders.is_empty(), "deprecated testnet-20 must not be publicly seeded after the testnet-21 migration");
        // The historical thresholds stay what the (now-frozen) ledger was mined under.
        let dns = p.dns_params.clone().unwrap();
        assert_eq!(dns.required_work_depth, Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!(dns.required_stake_depth, StakeScore(5000));
    }

    /// ADR-0045 D3-b: `--testnet --netsuffix=21` selects the PCPB dispatch rehearsal preset
    /// (`pcpb-palw`) — the compute-registry shape carried through the D3-b re-genesis train. The
    /// first net whose entire ledger is minted under the LeafV2 rules: clauses 11/12 gate every
    /// leaf at acceptance, clause 13 re-checks the binding at mint, and the PCPB windows (w/k/Δ)
    /// are consensus params from block 0. THE live public net — the tripwire lives here now.
    #[test]
    fn pcpb_palw_network_selection() {
        let net = NetworkId::with_suffix(NetworkType::Testnet, 21);
        let p: Params = net.into();
        assert_eq!(p.net, net);
        assert_eq!(p.net.suffix, Some(21));
        // Its OWN v5 genesis — a ledger distinct from testnet-20, staging, and every legacy identity.
        assert_eq!(p.genesis.hash, crate::config::genesis::PCPB_PALW_GENESIS.hash);
        assert_ne!(p.genesis.hash, COMPUTE_REGISTRY_PALW_PARAMS.genesis.hash);
        assert_ne!(p.genesis.hash, STAGING_MAINNET_PALW_PARAMS.genesis.hash);
        assert_ne!(p.genesis.hash, TESTNET_PARAMS.genesis.hash);
        // D3-b changes the LEAF payload, never the header schema — still a Header-v5 genesis.
        assert_eq!(p.genesis.version, crate::constants::PALW_COMPUTE_SET_HEADER_VERSION, "Header-v5 re-genesis (leaf moved, not the header)");
        // Inherited compute-registry shape: fence open, PALW active, acceptance released, real PoW.
        assert_eq!(p.palw_compute_registry_activation_daa_score, 0);
        assert!(p.is_palw_active(0));
        assert_eq!(p.palw_activation_daa_score, 0);
        assert!(p.palw_algo4_accept);
        assert!(!p.skip_proof_of_work);
        // 2026-08-01 migration: testnet-21 is the PUBLIC PALW testnet (superseding testnet-20),
        // so it carries the public seeders.
        assert_eq!(p.dns_seeders, TESTNET_21_DNS_SEEDERS);
        // ADR-0045 D3-b — the PCPB windows are part of this net's consensus identity (they are
        // exactly why D3-b could not land on testnet-20 in place). w ≥ Δ keeps the freshness
        // window non-empty; the first mintable registered_epoch is k + Δ (= 4): the early epochs
        // are structurally algo-4-empty, which is fail-closed, not a fault (design memo §10.2).
        assert_eq!(p.palw_freshness_window_epochs, 6, "w — changing this is a re-genesis-class identity move");
        assert_eq!(p.palw_snapshot_lag_epochs, 2, "k — changing this is a re-genesis-class identity move");
        assert_eq!(p.palw_post_commit_delta_epochs, 2, "Δ — changing this is a re-genesis-class identity move");
        assert!(p.palw_freshness_window_epochs >= p.palw_post_commit_delta_epochs, "w ≥ Δ (non-empty freshness window)");
        // TRIPWIRE (2026-07-30 testnet-200 halt): the DNS finality thresholds drive the beacon-seed
        // provenance, so changing them on THIS live public net breaks IBD replay of pre-change
        // history (see DnsParams::required_work_depth). If you must change them, RE-GENESIS onto a
        // new suffix (as 200→20 and 20→21 did) and update these pins — do not edit them on a
        // running net.
        let dns = p.dns_params.clone().unwrap();
        assert_eq!(dns.required_work_depth, Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]), "changing this on the live public net breaks IBD replay — re-genesis instead");
        assert_eq!(dns.required_stake_depth, StakeScore(5000), "changing this on the live public net breaks IBD replay — re-genesis instead");
        // 2026-08-01 bystander-wedge lesson, carried into the successor net from birth: the
        // emergency Work margin is difficulty-denominated (`emergency_work_margin_for`) and the
        // per-preset absolute addend must stay ZERO — a nonzero absolute re-arms the permanent
        // dead-branch wedge at whichever difficulty makes it unreachable.
        assert_eq!(dns.emergency_work_margin, BlueWorkType::ZERO, "the Work-margin addend must stay zero (bystander-wedge regression)");
        // TRIPWIRE, widened (2026-07-31): the two threshold pins above name only the fields the
        // testnet-200 halt happened to travel through. The hazard class is larger than those two —
        // the v4 `palw_beacon_seed` recurrence also consumes `palw_epoch_length_daa`, the beacon
        // grace and quorum params, `max_block_mass`, the genesis hash, the D3-b PCPB windows, and
        // TEN more `DnsParams` fields (attestation epoch/lag, the activation and min-active gates,
        // the stake-quality floors, the mandatory-inclusion score, the shard-mass cap). Changing
        // ANY of them on a live network re-derives seeds (or re-scopes clause windows) for
        // already-mined history, so the whole consensus surface is pinned at once via
        // `consensus_identity_hash` (it covers every field except `dns_seeders` and the derived
        // f64 — see `consensus_identity_hash_tests`).
        //
        // Tripped this assert? That is the tripwire working, not a stale test. testnet-21 is a LIVE
        // public network, so pick one and say which in the commit message:
        //   (a) the change is behind a FUTURE DAA activation score — replay of pre-fence history is
        //       byte-identical, so it is safe: update the pin below;
        //   (b) the change is unconditional — it silently invalidates mined history. RE-GENESIS onto
        //       a new suffix (as 200→20 and 20→21 did) and pin the new preset instead. Never edit
        //       in place.
        //   (c) the change touches ONLY fork-choice policy — a field read exclusively by
        //       `dns_reorg_allows` at live sink-selection time (today: the emergency reorg margins),
        //       never by block validity, `is_dns_confirmed`/anchor progression, or the beacon-seed
        //       recurrence. Mined history replays byte-identical and a mixed-version mesh cannot
        //       split on validity (the gate only decides which sink a node HOLDS, a per-node choice
        //       that already differs across live nodes), so an unconditional edit is safe: update
        //       the pin and name the exemption in the commit. Verify the read-site claim (grep the
        //       field) before invoking this clause — if the field also feeds any replayed decision,
        //       it is class (a)/(b), not (c). Precedent: the 2026-08-01 bystander-wedge fix
        //       (`emergency_work_margin` absolute → difficulty-denominated; wedged bystanders
        //       un-wedge on upgrade, healthy nodes' gates never engage, so no flag day needed).
        // Operators can compare this exact value across nodes: it is `consensusParamsHash` in
        // getInfo, and kaspad logs it at startup.
        assert_eq!(
            p.consensus_identity_hash().to_string(),
            "1efdbdaba9953c4f815a8ff5e44d46f04761f5dad92629c8dc45ccd875a1b34e2ddb3cd6319e809df4c2309b48ae236bcd35626a3c428a02382f1f15d7f83c45",
            "the LIVE public net's consensus params changed — DAA-gate it and re-pin, or re-genesis onto a new suffix"
        );
        // Every preset OUTSIDE the compute-registry lineage keeps the fence closed.
        for other in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET_PALW_PARAMS, STAGING_MAINNET_PALW_PARAMS, DEVNET_PARAMS, DEVNET_PALW_PARAMS, SIMNET_PARAMS]
        {
            assert_eq!(other.palw_compute_registry_activation_daa_score, u64::MAX);
        }
    }

    /// ADR-0048: `--testnet --netsuffix=200` selects the Header-v4 staging-mainnet PALW rehearsal
    /// preset — the ADR-0041 mainnet shape on an independent identity: v4 genesis (the anti-spam
    /// accumulator commitment bound into the genesis hash), genesis-active PALW, NON-inert spam
    /// params, algo-4 acceptance released, real PoW, and full-scale (un-shrunk) finality/pruning
    /// depths so the staging exercises measure the real thing.
    #[test]
    fn staging_mainnet_palw_network_selection() {
        let net = NetworkId::with_suffix(NetworkType::Testnet, 200);
        let p: Params = net.into();
        assert_eq!(p.net, net);
        assert_eq!(p.net.suffix, Some(200));
        // Its OWN v4 genesis — a ledger distinct from every legacy identity.
        assert_eq!(p.genesis.hash, crate::config::genesis::STAGING_PALW_GENESIS.hash);
        assert_ne!(p.genesis.hash, MAINNET_PARAMS.genesis.hash);
        assert_ne!(p.genesis.hash, TESTNET_PARAMS.genesis.hash);
        assert_ne!(p.genesis.hash, TESTNET_PALW_PARAMS.genesis.hash);
        assert_eq!(p.genesis.version, crate::constants::PALW_ANTISPAM_HEADER_VERSION, "Header-v4 re-genesis");
        // ADR-0041 shape: PALW genesis-active, acceptance RELEASED, weight-0 start.
        assert!(p.is_palw_active(0), "staging-mainnet-palw is PALW-active from genesis");
        assert_eq!(p.palw_activation_daa_score, 0);
        assert!(p.palw_algo4_accept, "the acceptance flip is released on staging (ADR-0040 §7.1.1)");
        // Acceptance without closure needs the accumulator to be the bound instead: A1 opened this
        // net to unlisted peers, and algo-4 is exempt from the Layer-0 hash floor, so `palw_spam`
        // below is the only thing left standing between a free header and the header stage.
        assert!(!p.palw_requires_peer_allowlist, "A1 opened staging to unlisted peers");
        assert!(!p.palw_spam.is_inert(), "an OPEN accepting preset must carry a non-inert anti-spam accumulator");
        // Still Stage-A on fork-choice credit, and deliberately so. `ΔC = scale · calc_work(bits)`
        // on a hash-floor-exempt lane would let anyone reachable here accumulate work for free —
        // chain takeover, strictly worse than the header spam the accumulator bounds.
        assert_eq!(p.palw_compute_work_scale, 0, "Stage-A PALW compute credit stays weight zero");
        // The FIRST non-inert anti-spam preset (ADR-0046 recalibrates the magnitude on staging).
        assert!(!p.palw_spam.is_inert());
        assert!(p.palw_spam.is_structurally_valid());
        // Real PoW in the testnet-palw shape: algo-3 grinds the real hash floor from the max-easy
        // fast-start target; algo-4's hash-floor exemption is structural, not a param.
        assert!(!p.skip_proof_of_work, "staging rehearses real PoW — no devnet skip-pow crutch");
        assert!(p.pow_blake2b_sha3_activation.is_active(0), "algo-3 supporting blocks are v3 BLAKE2b-SHA3");
        assert_eq!(p.evm_activation_daa_score, u64::MAX, "EVM off so a non-evm kaspad build runs staging");
        assert_eq!(p.genesis.bits, STAGING_PALW_LANE_DIFFICULTY.genesis_hash_bits, "§16.3 genesis-bits invariant");
        assert!(STAGING_PALW_LANE_DIFFICULTY.is_consistent_for_activation(p.genesis.bits));
        // ADR-0048: full-scale depths — the mainnet 想定値, deliberately NOT shrunk.
        assert_eq!(p.finality_depth(), 432_000, "finality depth stays at the mainnet full-scale value");
        assert_eq!(p.pruning_depth(), 1_080_000, "pruning depth stays at the mainnet full-scale value");
        assert_eq!(p.finality_depth(), MAINNET_PARAMS.blockrate.finality_depth);
        assert_eq!(p.pruning_depth(), MAINNET_PARAMS.blockrate.pruning_depth);
        assert_eq!(p.bps(), MAINNET_PARAMS.bps(), "inherits the 10-BPS mainnet profile");
        // ADR-0042 改訂 A1 (2026-07-28): the allowlist gate is RELEASED for this testnet — the fence
        // it depended on was lifted and verification moved onto the running network. Pruned
        // operation remains the default posture.
        assert!(!p.palw_requires_peer_allowlist, "A1 opened the staging rehearsal net to unlisted peers");
        assert!(!p.palw_requires_archival);
        // 2026-07-30 migration: testnet-200 is deprecated and no longer publicly seeded; the public
        // seeds resolve testnet-20 (compute-registry-palw).
        assert!(p.dns_seeders.is_empty(), "deprecated testnet-200 must not be publicly discovered after the testnet-20 migration");
        assert_eq!(
            PCPB_PALW_PARAMS.dns_seeders, TESTNET_21_DNS_SEEDERS,
            "testnet-21 is the public PALW testnet after the 2026-08-01 migration"
        );
        assert!(
            COMPUTE_REGISTRY_PALW_PARAMS.dns_seeders.is_empty(),
            "deprecated testnet-20 must not be publicly discovered after the testnet-21 migration"
        );
        assert!(TESTNET_PARAMS.dns_seeders.is_empty(), "retired testnet-10 must not resolve public seeds on port 26211");
        assert!(MAINNET_PARAMS.dns_seeders.is_empty(), "unlaunched mainnet must not reuse testnet seed names");
        assert!(DEVNET_PARAMS.dns_seeders.is_empty(), "retired devnet must not reuse testnet seed names");
        // Keeps production validator economics, but uses reachable testnet WorkDepth/StakeDepth
        // floors so the PALW beacon gate can recover within a rehearsal batch window.
        let dns = p.dns_params.unwrap();
        assert_eq!(dns.required_work_depth, Uint576([100, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!(dns.min_active_validators, PRODUCTION_DNS_PARAMS.min_active_validators);
        assert_eq!(dns.min_active_stake_sompi, PRODUCTION_DNS_PARAMS.min_active_stake_sompi);
        assert_eq!(dns.min_bond_amount_sompi, PRODUCTION_DNS_PARAMS.min_bond_amount_sompi);
        assert_eq!(dns.required_stake_depth, StakeScore(5000));
        assert!(dns.dns_v3_params_consistent(), "staging DNS params stay v3-consistent");
        assert_eq!(p.palw_batch_admission.registration_lead_epochs, 2);
        assert_eq!(p.palw_batch_admission.audit_window_epochs, 6);
        assert_eq!(p.palw_batch_admission.active_window_epochs, 16);
        assert!(
            p.palw_batch_admission.active_window_epochs
                >= p.palw_batch_admission.registration_lead_epochs.saturating_add(p.palw_batch_admission.audit_window_epochs),
            "staging active window must permit a successor registered at activation to overlap"
        );
    }

    /// ADR-0040 P1-5: validates the persisted `PalwBatchViewV1` bound on every activated preset.
    /// All shipped presets must pass, while an activated preset with `max_view_batches = 0` must fail.
    #[test]
    fn palw_activated_presets_bound_the_view() {
        let presets: [(&str, Params); 7] = [
            ("mainnet", MAINNET_PARAMS),
            ("testnet-10", TESTNET_PARAMS),
            ("testnet-palw-110", TESTNET_PALW_PARAMS),
            ("devnet-palw-111", DEVNET_PALW_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
            ("staging-mainnet-palw", STAGING_MAINNET_PALW_PARAMS),
        ];
        for (name, p) in presets.iter() {
            // Acceptance is no longer withheld everywhere — ADR-0040 P0-3 is released on the PALW
            // presets — but it tracks activation exactly, so neither half can drift.
            assert_eq!(p.palw_algo4_accept, p.is_palw_active(0), "{name}: algo-4 acceptance differs from PALW activation");
            // DOS-01: algo-4 is hash-floor exempt, so an accepting preset needs some other bound —
            // reachability (closed) or a non-inert anti-spam accumulator (open).
            if p.palw_algo4_accept {
                assert!(
                    p.palw_requires_peer_allowlist || !p.palw_spam.is_inert(),
                    "{name} accepts algo-4 while open AND without an anti-spam accumulator (DOS-01)"
                );
            }
            assert!(
                p.palw_batch_admission.is_consistent_for_activation(),
                "{name}: batch-admission params must bound the per-block-persisted view"
            );
            if p.palw_activation_daa_score != u64::MAX {
                assert_ne!(p.palw_batch_admission.max_view_batches, 0, "{name} activates PALW with an UNBOUNDED view");
                // ADR-0040 §5.17.4 (AUTHSET-01) — the committee size must not be vacuously zero on an
                // activated preset. A zero committee would select no auditors, making every vote
                // out-of-committee.
                assert_ne!(p.palw_audit_committee_size, 0, "{name} activates PALW with an EMPTY auditor committee");
                // ADR-0040 §5.17.6 (SAMPLE-01) — the sample size must not be vacuously zero on an
                // activated preset, for the same reason as the committee size: a zero sample makes the
                // re-derived `audit_sample_root` a fixed empty-vector constant, so any certificate
                // declaring that constant would pass SAMPLE-01 vacuously — enforcement without a property.
                assert_ne!(p.palw_audit_sample_size, 0, "{name} activates PALW with a ZERO audit sample size");
                // ADR-0045 D3-b — the PCPB windows must satisfy the PalwParams invariants on an
                // activated preset: `k ≥ 1` and `Δ ≥ 1` (a zero lag/offset would resolve the snapshot
                // or draw beacon AT the anchor epoch — known material, grindable B selection), and
                // `w ≥ Δ` (or the freshness window `anchor+Δ ≤ registered ≤ issued+w` is empty and
                // every honest leaf is rejected — the P1-7 failure shape).
                assert!(p.palw_snapshot_lag_epochs >= 1, "{name} activates PALW with a ZERO snapshot lag (k)");
                assert!(p.palw_post_commit_delta_epochs >= 1, "{name} activates PALW with a ZERO post-commit offset (Δ)");
                assert!(
                    p.palw_freshness_window_epochs >= p.palw_post_commit_delta_epochs,
                    "{name} activates PALW with w < Δ — the freshness window is empty and every honest leaf dies"
                );
            }
        }
        // Exactly three presets activate PALW (ADR-0048 added the staging re-genesis); the other four
        // stay inert. Pins the activation surface so a new activated preset cannot appear without
        // passing through this test.
        let activated: Vec<&str> = presets.iter().filter(|(_, p)| p.palw_activation_daa_score != u64::MAX).map(|(n, _)| *n).collect();
        assert_eq!(activated, vec!["testnet-palw-110", "devnet-palw-111", "staging-mainnet-palw"]);

        // REJECT: an activated preset whose cap has been zeroed by a params edit must fail the
        // preflight. This is what makes the `max_view_batches` doc claim true rather than paper.
        let mut broken = TESTNET_PALW_PARAMS;
        broken.palw_batch_admission.max_view_batches = 0;
        assert_eq!(broken.palw_activation_daa_score, 0, "the fixture must be an ACTIVATED preset");
        assert!(
            !broken.palw_batch_admission.is_consistent_for_activation(),
            "max_view_batches = 0 on an activated preset must be rejected"
        );

        // ADR-0040 §5.17.4 (AUTHSET-01) — the same discipline for the auditor committee size. A params
        // edit that zeroes it on an ACTIVATED preset must be caught by this preflight, exactly as the
        // loop assertion above would fire. Constructed here as a standalone REJECT fixture so the
        // invariant is pinned independently of the loop's iteration over the shipped presets.
        let mut broken_committee = TESTNET_PALW_PARAMS;
        broken_committee.palw_audit_committee_size = 0;
        assert_eq!(broken_committee.palw_activation_daa_score, 0, "the fixture must be an ACTIVATED preset");
        assert!(
            broken_committee.palw_activation_daa_score != u64::MAX && broken_committee.palw_audit_committee_size == 0,
            "committee_size = 0 on an activated preset must be a rejectable state"
        );

        // ADR-0040 §5.17.6 (SAMPLE-01) — the same discipline for the audit sample size.
        let mut broken_sample = TESTNET_PALW_PARAMS;
        broken_sample.palw_audit_sample_size = 0;
        assert_eq!(broken_sample.palw_activation_daa_score, 0, "the fixture must be an ACTIVATED preset");
        assert!(
            broken_sample.palw_activation_daa_score != u64::MAX && broken_sample.palw_audit_sample_size == 0,
            "sample_size = 0 on an activated preset must be a rejectable state"
        );

        // ADR-0040 ECON-03 — the same discipline for the anti-split floor. Without a minimum,
        // splitting a bond is free: 100 splits buy 100 credentials at no capital cost, so any
        // per-credential property is Sybil-defeatable. This REJECT fixture is what keeps
        // `min_provider_bond_sompi`'s non-zero-ness enforced rather than merely documented — delete
        // the `min_provider_bond_sompi != 0` clause from `is_consistent_for_activation` and this
        // assertion fails.
        let mut no_floor = TESTNET_PALW_PARAMS;
        no_floor.palw_batch_admission.min_provider_bond_sompi = 0;
        assert_eq!(no_floor.palw_activation_daa_score, 0, "the fixture must be an ACTIVATED preset");
        assert!(
            !no_floor.palw_batch_admission.is_consistent_for_activation(),
            "min_provider_bond_sompi = 0 on an activated preset must be rejected — a free bond split is a Sybil hole"
        );
        // And the shipped floor is actually positive, so the clause above is not vacuously satisfied.
        assert_ne!(TESTNET_PALW_PARAMS.palw_batch_admission.min_provider_bond_sompi, 0);
    }

    /// kaspa-pq **ADR-0040 §5.15.13 (gate G16)** — the paid-work walk must stay ABOVE the pruning point.
    ///
    /// The reward-coordinate duplicate-work walk reads a per-block column family that the pruning
    /// processor deletes. Those two are only compatible if the walk's reach is strictly shorter than
    /// the pruning depth: otherwise a live block's window would extend into rows that no longer exist
    /// and the rule would evaluate against a truncated paid set — a WRONG reward set on a node that
    /// has pruned, and agreement with a from-genesis node would be luck.
    ///
    /// This relation is a parameter fact, so it is checked where both parameters live. It is checked
    /// on EVERY preset, not just the activated ones: an inert preset that later flips its fence must
    /// not discover the relation is already broken.
    ///
    /// Limitation: this only bounds the walk on a node with a complete chain below it. A pruned-IBD
    /// joiner has no rows below its pruning point, so for the first `paid_work_walk_bound_daa` of DAA
    /// above it the walk returns
    /// a short prefix regardless of this relation. Closing that band needs the paid set to ride the
    /// pruning-point snapshot, and that snapshot's borsh encoding is the preimage of
    /// `Header::overlay_commitment_root` — so it is a header-commitment change, not a wiring change.
    /// It is an activation blocker for G16 and is recorded as such.
    #[test]
    fn palw_paid_work_walk_stays_above_the_pruning_point() {
        for (name, p) in [
            ("mainnet", MAINNET_PARAMS),
            ("testnet-10", TESTNET_PARAMS),
            ("testnet-palw-110", TESTNET_PALW_PARAMS),
            ("devnet-palw-111", DEVNET_PALW_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
            ("staging-mainnet-palw", STAGING_MAINNET_PALW_PARAMS),
        ] {
            let walk = p.palw_batch_admission.paid_work_walk_bound_daa(p.palw_epoch_length_daa);
            let pruning_depth = p.pruning_depth();
            assert!(walk > 0, "{name}: a zero walk bound would make the G16 rule vacuous");
            assert!(
                walk < pruning_depth,
                "{name}: the G16 paid-work walk reaches {walk} DAA but pruning deletes the rows it reads at \
                 depth {pruning_depth}. Shorten the batch-admission windows or the rule must stop pruning."
            );
        }
    }

    #[test]
    fn devnet_palw_activation_config_is_consistent() {
        // ADR-0039 P0 skeleton: the activation config a running devnet-palw single-node net will carry
        // must pass the §16.3 re-genesis preflight (`is_consistent_for_activation`) — the E2E harness
        // bypassed it. This pins "activation is one config + genesis away", not a code change.
        assert!(
            DEVNET_PALW_LANE_DIFFICULTY.is_consistent_for_activation(DEVNET_PALW_GENESIS_BITS),
            "devnet-palw lane difficulty must pass §16.3 is_consistent_for_activation"
        );
        // Activation flip: palw_activation_daa_score = 0 ⇒ PALW-active from genesis (vs u64::MAX inert base).
        let mut p = SIMNET_PARAMS;
        p.palw_activation_daa_score = 0;
        p.palw_lane_difficulty = DEVNET_PALW_LANE_DIFFICULTY;
        assert!(p.is_palw_active(0), "devnet-palw must be PALW-active from daa 0");
        assert!(!SIMNET_PARAMS.is_palw_active(0), "base simnet stays inert (regression guard)");
        // Non-zero genesis bits are mandatory (0 is the inert placeholder that fails the preflight).
        assert!(DEVNET_PALW_LANE_DIFFICULTY.genesis_hash_bits != 0);
        assert!(!crate::palw::LaneDifficultyParams::INERT.is_consistent_for_activation(DEVNET_PALW_GENESIS_BITS));
    }

    #[test]
    fn devnet_palw_preset_selected_and_active() {
        // ADR-0039 P0: `--devnet --netsuffix=111` resolves to the PALW-active devnet-palw preset, live.
        let p = Params::from(NetworkId::with_suffix(NetworkType::Devnet, 111));
        assert_eq!(p.net, NetworkId::with_suffix(NetworkType::Devnet, 111));
        assert!(p.is_palw_active(0), "devnet-palw is PALW-active from genesis");
        assert_eq!(p.palw_activation_daa_score, 0);
        assert!(p.skip_proof_of_work, "algo-4 pins the nonce; the preset must skip the Layer-0 hash floor");
        assert!(p.pow_blake2b_sha3_activation.is_active(0), "algo-3 supporting blocks are v3 BLAKE2b-SHA3");
        assert_eq!(p.evm_activation_daa_score, u64::MAX, "EVM off so a non-evm kaspad build runs devnet-palw");
        assert_eq!(p.genesis.hash, crate::config::genesis::DEVNET_PALW_GENESIS.hash);
        assert_eq!(p.genesis.bits, DEVNET_PALW_GENESIS_BITS, "genesis bits must equal the §16.3 invariant");
        assert!(DEVNET_PALW_LANE_DIFFICULTY.is_consistent_for_activation(p.genesis.bits));
        assert!(p.dns_params.unwrap().dns_v3_params_consistent(), "inherited DNS params stay v3-consistent");
        // Plain `--devnet` (no suffix) is unchanged and PALW-inert.
        let d = Params::from(NetworkId::new(NetworkType::Devnet));
        assert_eq!(d.palw_activation_daa_score, u64::MAX);
        assert!(!d.is_palw_active(0));
        assert_ne!(d.genesis.hash, p.genesis.hash, "devnet-palw has a distinct genesis");
    }

    /// §5.4 audit guard (follow-up to the `accepted_txs_of_chain_block` tolerance,
    /// commit 878eb42). That helper returns an EMPTY tx set for a chain block whose
    /// acceptance data / mergeset bodies are pruned. That is correct only where
    /// every node agrees the data is gone — the consensus pruning point. Of its
    /// callers, all but one walk a RECENT reorg path (`chain_path.removed/added`,
    /// the resolve-virtual split walk), which cannot cross the pruning point and so
    /// never sees a pruned block. The exception is the StakeScore aggregation, which
    /// walks back by DEPTH — `stake_score_window_blue_score` in blue score from the
    /// tip. If that window could reach past the retained region, two nodes with
    /// slightly different pruning points would resolve DIFFERENT accepted-tx sets
    /// (one derives the attestations, the other gets the tolerant empty) and compute
    /// different StakeScores — a finality split. Acceptance data is retained down to
    /// `pruning_depth`, so the window must stay strictly inside it. It does today by
    /// a wide margin (1500 vs a pruning depth in the 10^5+ range); this pins that so
    /// a future param change cannot silently erase it.
    #[test]
    fn the_stakescore_window_stays_inside_the_retained_region() {
        for (name, params) in [
            ("mainnet", MAINNET_PARAMS),
            ("testnet", TESTNET_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
            ("staging-mainnet-palw", STAGING_MAINNET_PALW_PARAMS),
        ] {
            let Some(dns) = params.dns_params.as_ref() else { continue };
            let window = dns.stake_score_window_blue_score;
            let pruning_depth = params.pruning_depth();
            assert!(
                window < pruning_depth,
                "{name}: stake_score_window_blue_score {window} must be < pruning_depth {pruning_depth}, else the \
                 tolerant accepted_txs_of_chain_block walk can reach pruned acceptance data and split the StakeScore \
                 across nodes with different pruning points (§5.4)"
            );
        }
    }

    /// **ADR-0046 Decision §3 — drift alarm for `docs/econ-parameters-frozen.md`.**
    ///
    /// The ledger records every shipped economic constant as a (value, evidence, re-calibration
    /// trigger) triple and labels anything without measurement evidence 未較正 rather than frozen.
    /// That labelling is only true while the ledger's numbers and the presets' numbers agree.
    ///
    /// Nothing else pins the MAGNITUDES. `is_consistent_for_activation` and
    /// `palw_activated_presets_bound_the_view` assert non-zero-ness only, so `10 MSK -> 1 MSK` or
    /// `6 -> 1` epochs passes every existing test while silently invalidating the κ ≈ 12 derivation
    /// the ledger publishes. This closes that one gap.
    ///
    /// This is an ALARM, not a new rule: it adds no constraint the protocol does not already have,
    /// and it does not claim any of these values is calibrated. Changing a value here is fine —
    /// changing it without updating the ledger in the same commit is not.
    ///
    /// Read on the staging-mainnet preset because that is the one carrying these values into a
    /// public rehearsal, and because it reaches them by INHERITANCE (`..MAINNET_PARAMS` ->
    /// `PalwBatchAdmissionParams::INERT`) — which is exactly how the vacuous leaf floor propagates.
    /// ADR-0042 R1: `bind_chain_derived_paid_work_attribution` rebuilds the paid-work row set by
    /// walking selected parents from the pruning point, and refuses the import if any header or
    /// ghostdag entry along that walk is missing. That refusal is correct but useless if it fires on
    /// honest imports, so the walk MUST fit inside what the IBD trusted-data package delivers.
    ///
    /// Trusted data carries the preset's sampled difficulty window around the pruning point, header
    /// AND ghostdag per block. The walk needs `paid_work_walk_bound_daa`. If someone lengthens the
    /// batch life or shortens that network's difficulty window past each other, the binding starts
    /// rejecting honest peers and IBD breaks — loudly here instead.
    #[test]
    fn trusted_data_covers_the_paid_work_walk() {
        let p = STAGING_MAINNET_PALW_PARAMS;
        let walk = p.palw_batch_admission.paid_work_walk_bound_daa(p.palw_epoch_length_daa);
        // Do not compare against `DIFFICULTY_WINDOW_DURATION` directly: that constant is seconds,
        // while this walk is DAA score. At 10 BPS each four-second sample spans 40 DAA.
        let trusted_window = p.difficulty_window_duration_in_block_units();
        assert!(
            walk <= trusted_window,
            "the paid-work walk ({walk} DAA) must fit inside the trusted-data DAA window \
             ({trusted_window} DAA), or the R1 chain binding refuses honest imports"
        );
        // Pin the current safety margin; these values are structural bounds rather than calibration.
        assert_eq!(walk, 2700, "paid-work walk bound moved — re-check the trusted-data margin");
        assert_eq!(trusted_window, 26_440, "testnet-200 trusted-data window moved — re-check the R1 margin");
    }

    #[test]
    fn shipped_economic_constants_match_the_frozen_ledger() {
        const LEDGER: &str = "changed — update docs/econ-parameters-frozen.md (ADR-0046 Decision §3: value + evidence + \
                              re-calibration trigger) in the SAME commit";
        let p = STAGING_MAINNET_PALW_PARAMS;

        // Ledger E1 — 暫定 (NOT frozen). κ = 10 MSK / 79,299,440 sompi-per-leaf live payout ≈ 12.6.
        // Slash is total forfeit of bond output-0, so re-pricing this re-prices the slash too.
        assert_eq!(
            p.palw_batch_admission.min_provider_bond_sompi,
            10 * crate::constants::SOMPI_PER_KASPA,
            "min_provider_bond_sompi {LEDGER}"
        );

        // Ledger E2 — 未較正 and VACUOUS. Deliberately NOT asserted non-zero by
        // `is_consistent_for_activation` (see the DELIBERATE OMISSION note in palw.rs); staging
        // inherits the zero through `..MAINNET_PARAMS`, so the rehearsal runs with no leaf floor.
        assert_eq!(p.palw_batch_admission.min_leaf_bond_sompi, 0, "min_leaf_bond_sompi {LEDGER}");

        // Ledger E3 — 未較正. 6 epochs matches `audit_window_epochs`; the magnitude is unmeasured.
        assert_eq!(p.palw_batch_admission.provider_unbond_floor_epochs, 6, "provider_unbond_floor_epochs {LEDGER}");

        // Ledger E4 — 未較正. L3 stays weight-0 until the ADR-0045 fraud wiring exists.
        assert_eq!(p.palw_compute_work_scale, 0, "palw_compute_work_scale {LEDGER}");
    }
}
