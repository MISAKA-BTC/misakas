//! kaspa-pq Phase 11 (ADR-0010): in-process validator node service.
//!
//! Loads the ML-DSA-87 signing key (deriving the overlay `validator_id =
//! BLAKE2b-512(public_key)` and the P2PKH-ML-DSA funding address) and runs an async
//! heartbeat that, per epoch: evaluates eligibility (bond active),
//! and — when eligible — builds + signs a stake attestation, wraps it in a fee-funded
//! `StakeAttestationShard` transaction (funded from a UTXO at the validator's own
//! address), and, in `Active` mode, submits it via `flow_context`. A persistent
//! signed-epoch log (ADR-0011) guards against double-signing across restarts.
//!
//! The service is registered only when `--enable-validator` is set, so default node
//! behavior is unchanged; `Observer`/`Standby` modes never submit. The DNS overlay
//! reorg gate itself remains dormant until activated per-network.

use crate::compute::{ComputeConfig, ComputeInflight, ComputeRole, capability_expiry_to_declare};
use async_trait::async_trait;
use kaspa_addresses::Prefix;
use kaspa_consensus_core::dns_finality::{
    BondStatus, DNS_PAYLOAD_VERSION_V1, OpenComputeCommitment, PendingComputeVerdict, PrecommitLock, SignedEpochCheckOutcome,
    SignedEpochRecord, StakeAttestation, ValidatorAttestationTarget, ValidatorStatus, effective_bond_status, is_bond_active_at,
    signature_fingerprint, single_attestation_shard,
};
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionId, TransactionOutpoint, UtxoEntry};
use kaspa_consensus_core::vlt::{
    ComputeFraudKind, LlmJobSpec, ReplayProof, VerificationVerdict, VltParams, compute_receipt_hash, job_spec_id,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use kaspa_hashes::Hash64;
use kaspa_mining::{mempool::tx::Orphan, model::tx_query::TransactionQuery};
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_pq_validator_core::{
    ATTESTATION_TX_FEE_FLOOR_SOMPI, SignedEpochStore, ValidatorKey, load_validator_seed, parse_stake_bond_ref, select_funding,
};
use kaspa_rpc_core::model::GetValidatorStatusResponse;
use kaspa_rpc_service::service::ValidatorStatusProvider;
use kaspa_txscript::pay_to_address_script;
use kaspa_utxoindex::api::UtxoIndexProxy;
use misaka_palw::MatchProjection;
use std::{
    collections::HashSet,
    fmt,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

const VALIDATOR: &str = "validator-service";

/// Heartbeat cadence for the skeleton worker loop. Later slices replace this
/// fixed tick with epoch-boundary–driven attestation issuance.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// kaspa-pq DNS v3: max ready epochs to (re-)attest per heartbeat when catching up after
/// downtime. Bounds per-tick work + fees; a deeper backlog converges over several ticks.
const ATTESTATION_CATCH_UP_LIMIT: usize = 16;

/// Bounded paginated scan of the virtual UTXO set when locating a funding UTXO at the
/// validator's address. This is a full-set scan (NOT address-indexed); the utxoindex is
/// the production optimization. Caps keep a large UTXO set from stalling the heartbeat.
const FUNDING_SCAN_CHUNK_SIZE: usize = 1000;
const MAX_FUNDING_SCAN_CHUNKS: usize = 64;

/// How many pending audits to fetch per heartbeat. Only one is executed — a job is minutes of
/// inference — but a small batch lets the cycle skip past ones whose verdict is already in flight
/// without waiting a tick per skip.
const COMPUTE_PENDING_VERDICT_SCAN: usize = 8;

/// Payload sizes used to price compute-overlay transactions. Every compute payload is fixed-shape
/// apart from the commitment's job input, so these are exact rather than estimates: a 4627-byte
/// ML-DSA-87 signature plus the payload's own fields. Slight over-estimates only overpay the
/// relay minimum, which `relay_fee_for_compute_mass` already pads.
const MLDSA87_SIG_BYTES: usize = 4627;
/// version + validator_id + bond outpoint + 3 digests + expiry.
const COMPUTE_CAPABILITY_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 64 + 68 + 3 * 64 + 8 + 64;
/// version + job_id + executor_id + bond outpoint, before the job input is added.
const COMPUTE_COMMITMENT_BASE_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 64 + 64 + 68 + 64;
/// version + epoch + executor_id + bond outpoint + commitment tx id + spec + receipt.
const COMPUTE_CERTIFICATE_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 8 + 64 + 68 + 64 + 256 + 192 + 64;
/// version + certificate tx id + job_id + 2 receipt hashes + verifier_id + bond outpoint + verdict.
const COMPUTE_VERDICT_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 64 + 64 + 2 * 64 + 64 + 68 + 1 + 64;
/// MISAKA §5 round 2: version + validator_id + bond outpoint + epoch + target (hash, daa) + the
/// declared lock (epoch, hash).
const PRECOMMIT_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 64 + 68 + 8 + 64 + 8 + 8 + 64 + 64;
/// The challenge, sized for the `ForgedReceipt` kind (no contradiction proof attached).
const COMPUTE_CHALLENGE_PAYLOAD_BYTES: usize = MLDSA87_SIG_BYTES + 2 + 64 + 64 + 1 + 64 + 68 + 68 + 64 + 64 + 64;

/// Operating mode for the in-process validator service (ADR-0010, operational modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidatorMode {
    /// Sign and submit stake attestations when eligible (full validator).
    Active,
    /// Track eligibility and stay warm, but never sign/submit (hot spare for failover).
    Standby,
    /// Observe only — never sign, never submit (telemetry / dry-run). Default.
    #[default]
    Observer,
}

impl fmt::Display for ValidatorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValidatorMode::Active => "active",
            ValidatorMode::Standby => "standby",
            ValidatorMode::Observer => "observer",
        })
    }
}

impl FromStr for ValidatorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(ValidatorMode::Active),
            "standby" => Ok(ValidatorMode::Standby),
            "observer" => Ok(ValidatorMode::Observer),
            other => Err(format!("unknown validator mode '{other}' (expected one of: active, standby, observer)")),
        }
    }
}

/// Static validator configuration derived from CLI args (`--enable-validator` and friends).
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub mode: ValidatorMode,
    /// Path to the ML-DSA-87 signing seed file (64 hex chars = 32 bytes), if provided.
    pub key_path: Option<String>,
    /// Stake-bond outpoint backing this validator's attestations, as "txid:index", if provided.
    pub stake_bond: Option<String>,
    /// Path to the persistent equivocation-safety log (`validator-state.json`). When
    /// `None`, signing is disabled (the guard cannot be enforced without persistence).
    pub state_path: Option<PathBuf>,
    /// Network address prefix, used to render the validator's funding address for logs.
    pub address_prefix: Prefix,
    /// ADR-0009 Addendum A.3 network discriminator — the per-network genesis hash. The
    /// attestation path never needs it (consensus hands the service a pre-bound message), but the
    /// compute path signs its own messages, so it must bind the same discriminator consensus does.
    pub network_id: Vec<u8>,
    /// This network's VLT parameters, if the DNS overlay is configured for it.
    pub vlt: Option<VltParams>,
    /// MISAKA Verified LLM Token-Weighted BFT: the compute role's configuration.
    pub compute: ComputeConfig,
}

/// A point-in-time snapshot of the validator's operational status, produced by
/// [`ValidatorService::status`] (consumed by the `getValidatorStatus` RPC). Combines
/// service-local facts (mode, identity, signing history) with a fresh consensus read of
/// eligibility (bond + active-set membership).
#[derive(Clone, Debug)]
pub struct ValidatorStatusSnapshot {
    pub mode: ValidatorMode,
    /// `None` if no signing key is configured/loaded.
    pub validator_id: Option<Hash64>,
    /// The P2PKH-ML-DSA funding address (bech32), if a key is loaded.
    pub funding_address: Option<String>,
    /// Current epoch at the sink (`None` if the overlay is not configured for this network).
    pub epoch: Option<u64>,
    /// Effective bond status at the sink (`None` if no bond is configured/found).
    pub bond_status: Option<BondStatus>,
    /// Whether the validator is in the current epoch's active validator set.
    pub is_active_validator: bool,
    /// Highest epoch with a local signing record (the equivocation log).
    pub last_signed_epoch: Option<u64>,
    /// Coarse, RPC-stable status code (ADR-0010/0011).
    pub status: ValidatorStatus,
}

/// Derive the coarse [`ValidatorStatus`] from the validator's mode and its
/// consensus-derived eligibility facts. Without a key, or outside `Active` mode, the
/// validator never produces an attestation, so it maps to `DryRun`; `Active` walks the
/// bond → active-set → already-signed ladder.
fn derive_validator_status(
    mode: ValidatorMode,
    key_loaded: bool,
    bond_status: Option<BondStatus>,
    is_active_validator: bool,
    signed_this_epoch: bool,
) -> ValidatorStatus {
    if !key_loaded || mode != ValidatorMode::Active {
        return ValidatorStatus::DryRun;
    }
    match bond_status {
        None => ValidatorStatus::BondNotFound,
        Some(BondStatus::Pending) => ValidatorStatus::BondPending,
        Some(BondStatus::Unbonding) => ValidatorStatus::Unbonding,
        Some(BondStatus::Slashed) => ValidatorStatus::Slashed,
        Some(BondStatus::Active) => {
            if !is_active_validator {
                ValidatorStatus::ActiveIdle
            } else if signed_this_epoch {
                ValidatorStatus::SignedThisEpoch
            } else {
                ValidatorStatus::ActiveEligible
            }
        }
    }
}

/// In-process validator node service (skeleton).
/// In-memory funding-chain state for attestation submission. The node's utxoindex keeps listing a
/// just-spent funding UTXO as available until our tx is mined, so re-querying it re-selects an
/// outpoint our own in-flight tx already spent → "output … already spent … in the mempool". We
/// instead chain off the previous tx's change output (`pending_change`) and exclude outpoints we
/// have already spent in flight (`inflight_spent`, self-pruned to what the node still lists). See
/// [`kaspa_pq_validator_core::select_funding`]. Reset on restart (a fresh chain is reselected).
#[derive(Default)]
struct FundingChain {
    pending_change: Option<(TransactionOutpoint, UtxoEntry)>,
    inflight_spent: HashSet<TransactionOutpoint>,
    /// kaspa-pq DNS-v3 hardening (Fix B — port of the external validator's stall recovery, audit
    /// M-2): the tx id of the attestation that produced the current `pending_change` chain head.
    /// `None` when there is no in-flight chain. Used for a cheap per-txid mempool residency lookup
    /// (`MiningManagerProxy::has_transaction`) to detect whether the head mined/dropped, rather than
    /// re-scanning the whole funding address's UTXO set.
    chain_head_txid: Option<TransactionId>,
    /// kaspa-pq DNS-v3 hardening (Fix B): the epoch whose attestation produced the current
    /// `pending_change` chain head. `None` when there is no in-flight chain. Used to count distinct
    /// served epochs the head has gone unconfirmed (advance the stall counter at most once/epoch).
    chain_head_epoch: Option<u64>,
    /// kaspa-pq DNS-v3 hardening (Fix B): consecutive served epochs the funding-chain head has
    /// stayed in the mempool without confirming. Reset to 0 whenever the head leaves the mempool
    /// (mined or dropped) or the local pending chain is cleared. A present head is NOT abandoned:
    /// during congestion, re-funding from confirmed UTXOs creates parallel funding chains and
    /// amplifies the flood.
    stalled_epochs: u64,
}

impl FundingChain {
    /// Update stall bookkeeping for the current funding-chain head.
    ///
    /// Returns true when the caller should warn. A head that is still in the mempool is kept as the
    /// authoritative next funding UTXO; only the stall counter/logging advances. A head that is gone
    /// (mined or dropped) clears the stall counter, and the submit path handles any dropped-head
    /// failure by resetting the chain before selecting fresh confirmed funding.
    fn note_head_mempool_status(&mut self, latest_epoch: u64, head_unmined: bool) -> bool {
        if self.pending_change.is_none() {
            self.stalled_epochs = 0;
            return false;
        }

        if head_unmined {
            if self.chain_head_epoch != Some(latest_epoch) {
                self.stalled_epochs = self.stalled_epochs.saturating_add(1);
                self.chain_head_epoch = Some(latest_epoch);
            }
            self.stalled_epochs >= STALL_WARN_EPOCHS
        } else {
            self.stalled_epochs = 0;
            self.chain_head_epoch = None;
            false
        }
    }
}

/// kaspa-pq DNS-v3 hardening (Fix B): consecutive served epochs before warning that the
/// funding-chain head is still pending. The chain is kept while the head remains in mempool.
const STALL_WARN_EPOCHS: u64 = 3;

pub struct ValidatorService {
    config: ValidatorConfig,
    consensus_manager: Arc<ConsensusManager>,
    tick_service: Arc<TickService>,
    /// Used to submit attestation-shard transactions to the local mempool + p2p.
    flow_context: Arc<FlowContext>,
    /// Loaded signing key + derived identity. `None` until/unless a valid key is configured.
    key: Option<ValidatorKey>,
    /// Parsed stake-bond outpoint, if `--stake-bond` was provided and well-formed.
    bond_outpoint: Option<TransactionOutpoint>,
    /// Persistent equivocation-safety log. `None` (signing disabled) unless a key, bond,
    /// and state path are all present and the on-disk log belongs to this validator.
    signed_epochs: Mutex<Option<SignedEpochStore>>,
    /// Address-indexed UTXO lookup for funding (when `--utxoindex` is enabled); falls back
    /// to a bounded virtual-UTXO-set scan otherwise.
    utxoindex: Option<UtxoIndexProxy>,
    /// Mass-based fee (sompi) for the attestation-shard tx, computed once at startup.
    attestation_fee_sompi: u64,
    /// Network coinbase-maturity (blocks): a coinbase funding UTXO younger than this cannot be
    /// spent. Captured once at startup from the consensus params.
    coinbase_maturity: u64,
    /// Local funding chain so consecutive attestations (within a heartbeat's catch-up loop and
    /// across heartbeats) don't re-select a UTXO an in-flight tx already spent.
    funding_chain: Mutex<FundingChain>,
    /// MISAKA Verified LLM Token-Weighted BFT: the compute role, when this node can actually run
    /// the consensus-registered runtime. `None` leaves the attestation path exactly as it was.
    compute: Option<ComputeRole>,
    /// What the compute cycle has submitted and not yet seen on chain.
    compute_inflight: Mutex<ComputeInflight>,
    /// Mass calculator, kept for the compute path: unlike the attestation shard, a compute
    /// transaction's payload size varies (a commitment carries the job input), so its fee cannot
    /// be computed once at startup.
    mass_calculator: MassCalculator,
}

impl ValidatorService {
    pub fn new(
        config: ValidatorConfig,
        consensus_manager: Arc<ConsensusManager>,
        tick_service: Arc<TickService>,
        flow_context: Arc<FlowContext>,
        mass_calculator: MassCalculator,
        utxoindex: Option<UtxoIndexProxy>,
        coinbase_maturity: u64,
    ) -> Self {
        // Validate configuration eagerly so misconfiguration surfaces at startup, not at first use.
        let key = match &config.key_path {
            Some(path) => match load_validator_seed(path) {
                Ok(seed) => {
                    let key = ValidatorKey::from_seed(seed);
                    info!("[{VALIDATOR}] loaded validator signing key from {path} (validator_id={})", key.validator_id);
                    info!(
                        "[{VALIDATOR}] funding address: {} — send UTXOs here to fund attestation-shard submission",
                        key.funding_address(config.address_prefix)
                    );
                    Some(key)
                }
                Err(err) => {
                    warn!("[{VALIDATOR}] {err} — validator will run without a signing key");
                    None
                }
            },
            None => None,
        };
        let bond_outpoint = match &config.stake_bond {
            Some(s) => match parse_stake_bond_ref(s) {
                Ok(outpoint) => Some(outpoint),
                Err(err) => {
                    warn!("[{VALIDATOR}] {err}");
                    None
                }
            },
            None => None,
        };
        // The equivocation-safety log requires a key (validator_id), a bond, and a path.
        // A load failure (e.g. a foreign state file) leaves it `None`, which disables signing.
        let signed_epochs = match (&key, bond_outpoint, &config.state_path) {
            (Some(key), Some(outpoint), Some(path)) => match SignedEpochStore::load_or_empty(path.clone(), key.validator_id, outpoint)
            {
                Ok(store) => {
                    info!("[{VALIDATOR}] equivocation-safety log {} ({} prior epoch(s))", path.display(), store.record_count());
                    Some(store)
                }
                Err(err) => {
                    warn!("[{VALIDATOR}] {err} — signing disabled until resolved");
                    None
                }
            },
            _ => None,
        };
        // The attestation-shard tx shape is fixed, so its mass-based fee is computed once.
        let attestation_fee_sompi = key
            .as_ref()
            .map_or(ATTESTATION_TX_FEE_FLOOR_SOMPI, |k| k.estimate_attestation_fee(&mass_calculator, config.address_prefix));
        // The compute role resolves (and logs) at startup so a misconfigured runtime surfaces
        // before the first job rather than at the first sortition.
        let compute = ComputeRole::new(&config.compute, config.vlt.as_ref());
        Self {
            config,
            consensus_manager,
            tick_service,
            flow_context,
            key,
            bond_outpoint,
            signed_epochs: Mutex::new(signed_epochs),
            utxoindex,
            attestation_fee_sompi,
            coinbase_maturity,
            funding_chain: Mutex::new(FundingChain::default()),
            compute,
            compute_inflight: Mutex::new(ComputeInflight::default()),
            mass_calculator,
        }
    }

    pub async fn worker(self: &Arc<ValidatorService>) {
        let validator_id = match &self.key {
            Some(key) => key.validator_id.to_string(),
            None => "none".to_string(),
        };
        info!(
            "[{VALIDATOR}] starting (mode={}, validator_id={}, stake-bond={})",
            self.config.mode,
            validator_id,
            self.config.stake_bond.as_deref().unwrap_or("none"),
        );
        if self.config.mode == ValidatorMode::Active && self.key.is_none() {
            warn!("[{VALIDATOR}] mode=active but no signing key is loaded; no attestations can be produced");
        }

        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await {
                break;
            }

            // Heartbeat: report the node tip, the validator's own bond status, and its
            // active-set membership for the current epoch. When eligible (bond active AND
            // in the active set) it also builds + signs the attestation for the sink and
            // verifies it locally — but does NOT gossip or submit it (the equivocation
            // guard and submission are later slices).
            let my_id = self.key.as_ref().map(|k| k.validator_id);
            let session = self.consensus_manager.consensus().session().await;
            let sink = session.async_get_sink_daa_score_timestamp().await;
            let dns = session.async_get_dns_confirmation().await;
            // The overlay reads return None on non-overlay networks too, so skip the
            // lookups there to avoid misleading status lines.
            let (bond, active_set, attestation_targets) = if dns.is_some() {
                let bond = match self.bond_outpoint {
                    Some(outpoint) => session.async_get_stake_bond(outpoint).await,
                    None => None,
                };
                let active_set = session.async_get_active_validator_set().await;
                // Eligible iff our bond is active AND our validator_id is in the active set.
                let eligible = match (&bond, &active_set, my_id) {
                    (Some(b), Some(c), Some(id)) => is_bond_active_at(b, sink.daa_score) && c.members.contains(&id),
                    _ => false,
                };
                // kaspa-pq DNS v3: sign the canonical lagged anchor(s). Once we have signed at
                // least one epoch, batch-sign every ready epoch SINCE then (catch-up after
                // downtime / when epoch_duration < heartbeat); on the first run just take the
                // latest ready target. `SignedEpochStore` dedups, so re-offered epochs are no-ops.
                let attestation_targets = match (eligible, self.bond_outpoint) {
                    (true, Some(outpoint)) => {
                        let last_signed = self.signed_epochs.lock().unwrap().as_ref().and_then(|s| s.last_signed_epoch());
                        match last_signed {
                            Some(e) => {
                                session.async_get_validator_attestation_targets(outpoint, e + 1, ATTESTATION_CATCH_UP_LIMIT).await
                            }
                            None => session.async_get_validator_attestation_target(outpoint).await.into_iter().collect(),
                        }
                    }
                    _ => Vec::new(),
                };
                (bond, active_set, attestation_targets)
            } else {
                (None, None, Vec::new())
            };
            drop(session);

            match dns {
                Some(conf) => {
                    let bond_status = match (self.bond_outpoint.is_some(), &bond) {
                        (false, _) => "unconfigured".to_string(),
                        (true, Some(b)) => {
                            format!("{:?}(active={})", effective_bond_status(b, sink.daa_score), is_bond_active_at(b, sink.daa_score))
                        }
                        (true, None) => "not-found".to_string(),
                    };
                    let active_set_status = match (&active_set, my_id) {
                        (Some(c), Some(id)) => format!(
                            "epoch={} is_active_validator={} (active_validators={})",
                            c.epoch,
                            c.members.contains(&id),
                            c.active_validator_count
                        ),
                        (Some(c), None) => {
                            format!("epoch={} no-signing-key (active_validators={})", c.epoch, c.active_validator_count)
                        }
                        (None, _) => "unavailable".to_string(),
                    };
                    info!(
                        "[{VALIDATOR}] heartbeat: mode={} sink_daa={} bond={} active_set=[{}] dns_overlay=configured (stage={:?}, dns_confirmed={})",
                        self.config.mode, sink.daa_score, bond_status, active_set_status, conf.rollout_stage, conf.dns_confirmed
                    );

                    // Eligible: fund + sign + (in Active mode) submit each ready epoch's
                    // attestation shard tx, under the per-epoch equivocation guard.
                    if let (Some(key), Some(outpoint)) = (&self.key, self.bond_outpoint) {
                        // kaspa-pq DNS-v3 hardening (Fix A — anchor-deep start-gate): skip any epoch
                        // whose canonical lagged anchor predates the bond's activation. The consensus
                        // §B.4 rule (attestation_reward_eligibility → active_bond_at(.., target_daa_score))
                        // makes ANY block including such a shard INVALID, so it would submit-OK but never
                        // mine and would stall the funding chain on a young chain (e.g. just after a
                        // re-genesis). Gate on the exact §B.4 condition.
                        let activation = bond.as_ref().map(|b| b.activation_daa_score).unwrap_or(u64::MAX);
                        // kaspa-pq DNS-v3 hardening (Fix B): ONCE per heartbeat (before the catch-up
                        // loop, which legitimately chains many epochs per tick), check whether the
                        // funding-chain head is still pending. If it is still in mempool, keep the
                        // pending chain and warn after STALL_WARN_EPOCHS; do not re-fund from a
                        // confirmed UTXO, because congestion-time re-funding creates parallel chains
                        // and amplifies the flood. Keyed on the latest served epoch so the counter
                        // advances at most once per wall-clock epoch.
                        if let Some(latest_epoch) = attestation_targets.iter().map(|t| t.epoch).max() {
                            self.recover_stalled_funding_chain(latest_epoch).await;
                        }
                        for target in &attestation_targets {
                            if target.target_daa_score < activation {
                                trace!(
                                    "[{VALIDATOR}] gating epoch {} target_daa={} < activation_daa={} (bond not anchor-deep yet)",
                                    target.epoch, target.target_daa_score, activation
                                );
                                continue;
                            }
                            self.try_attest(target, key, outpoint).await;
                        }

                        // MISAKA §5 round 2, immediately after the prevotes and before the
                        // compute cycle. Nothing is DNS-confirmed until the precommit round
                        // reaches quorum, so a validator that prevotes and then spends the tick
                        // on a multi-minute inference would leave finality stalled at round 1.
                        self.run_precommit_cycle(key, outpoint).await;

                        // MISAKA Verified LLM Token-Weighted BFT. After the attestations, because
                        // DNS finality must not wait behind a multi-minute inference: attesting is
                        // what keeps the overlay confirming, and it is cheap.
                        self.run_compute_cycle(key, outpoint).await;
                    }
                }
                None => {
                    trace!("[{VALIDATOR}] heartbeat: mode={} sink_daa={} dns_overlay=not-configured", self.config.mode, sink.daa_score)
                }
            }
        }

        trace!("[{VALIDATOR}] worker exiting");
    }

    /// On-demand snapshot of the validator's operational status, for the `getValidatorStatus`
    /// RPC. Combines local config/identity + the signing log with a fresh consensus read of
    /// bond + active-set eligibility.
    pub async fn status(&self) -> ValidatorStatusSnapshot {
        let validator_id = self.key.as_ref().map(|k| k.validator_id);
        let funding_address = self.key.as_ref().map(|k| k.funding_address(self.config.address_prefix).to_string());

        let session = self.consensus_manager.consensus().session().await;
        let active_set = session.async_get_active_validator_set().await;
        let bond = match self.bond_outpoint {
            Some(outpoint) => session.async_get_stake_bond(outpoint).await,
            None => None,
        };
        let sink_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
        drop(session);

        let epoch = active_set.as_ref().map(|c| c.epoch);
        let bond_status = bond.as_ref().map(|b| effective_bond_status(b, sink_daa));
        let is_active_validator = matches!((&active_set, validator_id), (Some(c), Some(id)) if c.members.contains(&id));
        let (last_signed_epoch, signed_this_epoch) = {
            let guard = self.signed_epochs.lock().unwrap();
            match guard.as_ref() {
                Some(s) => (s.last_signed_epoch(), epoch.map(|e| s.has_signed_epoch(e)).unwrap_or(false)),
                None => (None, false),
            }
        };
        let status =
            derive_validator_status(self.config.mode, self.key.is_some(), bond_status, is_active_validator, signed_this_epoch);

        ValidatorStatusSnapshot {
            mode: self.config.mode,
            validator_id,
            funding_address,
            epoch,
            bond_status,
            is_active_validator,
            last_signed_epoch,
            status,
        }
    }

    /// kaspa-pq DNS-v3 hardening (Fix B). Run once per heartbeat (NOT per epoch — the
    /// catch-up loop chains many epochs per tick).
    ///
    /// If the funding-chain head is still resident in the local mempool (unmined), advance the
    /// per-epoch stall counter at most once per distinct served `latest_epoch`. If it remains
    /// present, keep the pending chain and warn only; do not re-fund from a confirmed node UTXO,
    /// because congestion-time re-funding creates parallel chains and amplifies the mempool flood.
    /// If the head has left the mempool (mined or dropped), reset the counter.
    ///
    /// Behavioral note vs. the external validator: that sidecar's per-txid `get_mempool_entry`
    /// returns a tri-state (Present / Gone / Unknown-on-RPC-error). In-process there is no RPC, so
    /// `has_transaction` is a direct in-memory read with only Present / Gone — there is no transient
    /// error to suppress, so the Unknown "make no change" branch is intentionally absent.
    async fn recover_stalled_funding_chain(&self, latest_epoch: u64) {
        // Snapshot the head id under the lock; do the (await-ing) mempool read without holding it.
        let (has_head, head_txid) = {
            let chain = self.funding_chain.lock().unwrap();
            (chain.pending_change.is_some(), chain.chain_head_txid)
        };
        if !has_head {
            // No in-flight chain ⇒ nothing can be stalled.
            self.funding_chain.lock().unwrap().stalled_epochs = 0;
            return;
        }
        // Is the head still unmined? A cheap per-txid mempool lookup (TransactionsOnly, mirroring the
        // external validator's `get_mempool_entry(txid, false, false)`), never a full address scan.
        let head_unmined = match head_txid {
            Some(txid) => self.flow_context.mining_manager().clone().has_transaction(txid, TransactionQuery::TransactionsOnly).await,
            // A pending change with no recorded head id (e.g. carried over an upgrade): treat as mined
            // so we don't count a phantom stall; the next submit re-stamps the head id.
            None => false,
        };
        let mut chain = self.funding_chain.lock().unwrap();
        // Re-check under the lock: a concurrent path may have cleared the head (e.g. a submit failure
        // in try_attest) while we were awaiting the mempool read.
        if chain.pending_change.is_none() {
            chain.stalled_epochs = 0;
            return;
        }
        let should_warn = chain.note_head_mempool_status(latest_epoch, head_unmined);
        if should_warn {
            warn!(
                "[{VALIDATOR}] funding-chain head still in mempool for {} epochs (now epoch {latest_epoch}); keeping pending chain, not re-funding",
                chain.stalled_epochs
            );
        }
    }

    /// Async attestation cycle for an eligible epoch: discover a funding UTXO, build the
    /// guarded + signed shard transaction, and — in `Active` mode — submit it. No-ops
    /// cleanly when there is no funding UTXO or the equivocation guard blocks/skips.
    async fn try_attest(&self, target: &ValidatorAttestationTarget, key: &ValidatorKey, bond_outpoint: TransactionOutpoint) {
        let funding_spk = pay_to_address_script(&key.funding_address(self.config.address_prefix));
        let fee = self.attestation_fee_sompi;
        let candidates = self.find_funding_candidates(&funding_spk).await;
        let virtual_daa = self.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();

        // Select funding under the chain lock (NOT held across the await below). Prefer chaining off
        // our own unconfirmed change so we never re-select a UTXO the node's utxoindex still lists as
        // available but which an in-flight attestation tx of ours already spent ("already spent in
        // the mempool"). This matters most in the per-heartbeat catch-up loop, where several ready
        // epochs are attested before any of their txs are mined.
        let funding = {
            let mut chain = self.funding_chain.lock().unwrap();
            let node_outpoints: HashSet<TransactionOutpoint> = candidates.iter().map(|(op, _)| *op).collect();
            // Forget in-flight exclusions the node no longer lists (mined ⇒ safe to forget): self-heals.
            chain.inflight_spent.retain(|op| node_outpoints.contains(op));
            // If our chain head has been mined (now in the node set), resync to the node view and
            // clear the stall-tracking state (the head confirmed ⇒ not stalled).
            if let Some((head, _)) = &chain.pending_change
                && node_outpoints.contains(head)
            {
                chain.pending_change = None;
                chain.chain_head_txid = None;
                chain.chain_head_epoch = None;
                chain.stalled_epochs = 0;
            }
            select_funding(&chain.pending_change, &chain.inflight_spent, candidates, fee, virtual_daa, self.coinbase_maturity).ok()
        };

        let Some(tx) = self.guarded_build_funded(target, key, bond_outpoint, funding.clone(), fee) else {
            return;
        };
        let tx_id = tx.id();
        if self.config.mode == ValidatorMode::Active {
            // Same path the RPC `submitTransaction` uses: validate + insert to mempool, then broadcast.
            let session = self.consensus_manager.consensus().unguarded_session();
            match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
                Ok(()) => {
                    info!("[{VALIDATOR}] submitted attestation shard tx {tx_id} for epoch {}", target.epoch);
                    // Advance the funding chain: this tx's change output (index 0, back to self) funds
                    // the next ready epoch. The tx id excludes signature scripts, so it is stable.
                    if let Some((funding_outpoint, funding_entry)) = funding {
                        let mut chain = self.funding_chain.lock().unwrap();
                        chain.inflight_spent.insert(funding_outpoint);
                        let change =
                            UtxoEntry::new(funding_entry.amount - fee, funding_entry.script_public_key.clone(), virtual_daa, false);
                        chain.pending_change = Some((TransactionOutpoint::new(tx_id, 0), change));
                        // kaspa-pq DNS-v3 hardening (Fix B, audit M-2): record the head tx id (for the
                        // per-txid mempool confirmation lookup) and the epoch that produced it (so the
                        // stall counter advances once per unconfirmed epoch). A fresh head is, by
                        // definition, not yet stalled.
                        chain.chain_head_txid = Some(tx_id);
                        chain.chain_head_epoch = Some(target.epoch);
                        chain.stalled_epochs = 0;
                    }
                }
                Err(e) => {
                    warn!("[{VALIDATOR}] submit of attestation shard tx {tx_id} (epoch {}) failed: {e}", target.epoch);
                    // Drop the chain head (and its stall-tracking state) so the next attempt reselects
                    // from the node view. No new change output exists, so there is nothing to chain.
                    let mut chain = self.funding_chain.lock().unwrap();
                    chain.pending_change = None;
                    chain.chain_head_txid = None;
                    chain.chain_head_epoch = None;
                    chain.stalled_epochs = 0;
                }
            }
        } else {
            info!(
                "[{VALIDATOR}] built funded attestation shard tx {tx_id} for epoch {} — mode={} so NOT submitting",
                target.epoch, self.config.mode
            );
        }
    }

    /// List the UTXOs locked to `funding_spk` (the validator's own P2PKH-ML-DSA address). Prefers the
    /// address-indexed utxoindex lookup; falls back to a bounded virtual-UTXO-set scan when
    /// `--utxoindex` is not enabled. Returns them filtered ONLY by our own bond outpoint (see below);
    /// fee/maturity/in-flight filtering and the chain-head-vs-node choice are [`select_funding`]'s job.
    ///
    /// kaspa-pq (bond spend-gate hardening): EXCLUDE our own `bond_outpoint` from funding candidates.
    /// A StakeBond's output-0 is a normal owner-controlled UTXO whose stake-lock is enforced solely by
    /// the consensus bond spend-gate (ADR-0016 §D.2) — it is typically the LARGEST mature non-coinbase
    /// UTXO at the funding address, so `select_funding` (which picks max-by-amount) would otherwise
    /// select it. Building an attestation tx that spends a non-releasable bond gets the carrying block
    /// disqualified (`NonReleasableBondSpendInBlock`), so the tx is mempool-accepted but never mines —
    /// a validator self-wedge. The explicit unbond CLI path already excludes it
    /// (kaspa-pq-validator/src/main.rs); this mirrors that onto the attestation funding path.
    async fn find_funding_candidates(&self, funding_spk: &ScriptPublicKey) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        let bond_outpoint = self.bond_outpoint;
        if let Some(utxoindex) = &self.utxoindex {
            // Address-indexed: O(matches) instead of O(utxo-set). The utxoindex stores a
            // compact entry (no spk — it's the lookup key), so rebuild the full UtxoEntry.
            let Ok(set) = utxoindex.clone().get_utxos_by_script_public_keys([funding_spk.clone()].into_iter().collect()).await else {
                return Vec::new();
            };
            return set
                .into_values()
                .flatten()
                .filter(|(outpoint, _)| Some(*outpoint) != bond_outpoint)
                .map(|(outpoint, c)| (outpoint, UtxoEntry::new(c.amount, funding_spk.clone(), c.block_daa_score, c.is_coinbase)))
                .collect();
        }
        // Fallback: bounded paginated scan of the virtual UTXO set, collecting all of OUR outputs.
        let session = self.consensus_manager.consensus().session().await;
        let mut from: Option<TransactionOutpoint> = None;
        let mut candidates = Vec::new();
        for _ in 0..MAX_FUNDING_SCAN_CHUNKS {
            let chunk = session.async_get_virtual_utxos(from, FUNDING_SCAN_CHUNK_SIZE, from.is_some()).await;
            if chunk.is_empty() {
                break;
            }
            from = chunk.last().map(|(outpoint, _)| *outpoint);
            candidates.extend(
                chunk
                    .into_iter()
                    .filter(|(outpoint, entry)| &entry.script_public_key == funding_spk && Some(*outpoint) != bond_outpoint),
            );
        }
        candidates
    }

    // ---------------------------------------------------------------------
    // MISAKA Verified LLM Token-Weighted BFT: the compute cycle.
    // ---------------------------------------------------------------------

    /// One pass of the compute cycle, run from the heartbeat once the attestation work is done.
    ///
    /// Order is deliberate — capability, then verifier, then executor:
    ///
    /// * **Capability** first because it is cheap and everything else depends on it: a lapsed
    ///   declaration removes this node from committee draws entirely.
    /// * **Verifier** before executor because acceptance is refutation-dominant. A verifier that
    ///   goes quiet denies *other* validators their credit and can stall a job permanently; an
    ///   executor that goes quiet costs only itself, and only until the next tick.
    /// * **Executor** last, with whatever capacity is left.
    ///
    /// At most one *job* runs per pass. A job is a full LLM inference measured in minutes, so
    /// starting a second one would just queue work the next tick would have picked up anyway —
    /// while holding a stale view of the chain the whole time.
    /// MISAKA §5 round 2: lock on the oldest epoch whose prevote quorum this chain shows and that
    /// this validator has not locked yet.
    ///
    /// **One per tick, oldest first.** The lock a precommit declares must be the previous counted
    /// one, so the chain of locks is built a link at a time and in order; signing several at once
    /// would produce a batch whose later members declare locks the chain has not accepted yet, and
    /// the walk would drop everything after the first.
    ///
    /// The lock comes from `duty.held` — what the network can see this validator has published —
    /// and never from local state. A node restored from a backup, or resynced, would otherwise
    /// declare a lock the chain contradicts, which stops its precommits counting at best and, if
    /// its old lock came from another branch, is precisely the equivocation this round exists to
    /// make provable.
    async fn run_precommit_cycle(&self, key: &ValidatorKey, bond_outpoint: TransactionOutpoint) {
        let session = self.consensus_manager.consensus().session().await;
        let duty = session.async_get_precommit_duty(key.validator_id, bond_outpoint).await;
        drop(session);
        let Some(duty) = duty.filter(|d| d.round_active) else {
            trace!("[{VALIDATOR}] precommit: round 2 is not live at the sink; nothing to lock");
            return;
        };
        let Some(&(epoch, target_hash, target_daa_score)) = duty.due.first() else {
            trace!("[{VALIDATOR}] precommit: nothing due (held lock is epoch {})", duty.held.epoch);
            return;
        };
        let held = if duty.held == PrecommitLock::default() { None } else { Some(duty.held) };
        let precommit = match key.sign_precommit(&self.config.network_id, epoch, target_hash, target_daa_score, held, bond_outpoint) {
            Ok(p) => p,
            Err(e) => {
                warn!("[{VALIDATOR}] precommit: refusing to sign epoch {epoch}: {e}");
                return;
            }
        };
        let build = |funding_outpoint, funding: &UtxoEntry, fee| key.build_precommit_tx(&precommit, funding_outpoint, funding, fee);
        if self.build_and_submit_overlay_tx("precommit", PRECOMMIT_PAYLOAD_BYTES, false, build).await.is_some() {
            info!("[{VALIDATOR}] precommit: LOCKED epoch {epoch} on anchor {target_hash} (previous lock: epoch {})", duty.held.epoch);
        }
    }

    async fn run_compute_cycle(&self, key: &ValidatorKey, bond_outpoint: TransactionOutpoint) {
        let Some(role) = &self.compute else {
            return;
        };
        let Some(vlt) = &self.config.vlt else {
            return;
        };
        let session = self.consensus_manager.consensus().session().await;
        let status = session.async_get_compute_status(key.validator_id, bond_outpoint).await;
        drop(session);
        // Keyed on the SHADOW fence, not the weight fence. The soak between them is precisely the
        // interval in which this node has to commit, execute, audit and certify for real — that is
        // what fills `C_i(E)` — while finality is still stake-weighted. Waiting for the weight
        // fence would leave the credit table empty at the moment the vote moves onto it.
        let Some(status) = status.filter(|s| s.shadow_active) else {
            trace!("[{VALIDATOR}] compute: the compute overlay is not live at the sink; nothing to do");
            return;
        };
        if !status.vlt_active {
            trace!(
                "[{VALIDATOR}] compute: overlay live at DAA {} but voting weight is still bonded stake (soak) — producing credit anyway",
                status.sink_daa_score
            );
        }
        let now_daa = status.sink_daa_score;

        // (1) Keep the capability declaration live.
        let has_live_capability = status.capability_expiry_daa_score.is_some();
        if !self.compute_inflight.lock().unwrap().capability_recent(now_daa)
            && let Some(expiry) =
                capability_expiry_to_declare(status.capability_expiry_daa_score, now_daa, vlt.max_capability_validity_blocks)
        {
            self.submit_capability(key, bond_outpoint, role, expiry, now_daa).await;
            if !has_live_capability {
                // Nothing to do until the declaration is ACCEPTED: with none live this node is in
                // no committee draw, so there is nothing to audit, and a job committed now would
                // be one nobody in class has declared for. A *renewal* is different — the old
                // declaration still stands — so that case falls through.
                return;
            }
        }

        // (2) Audit whatever we were drawn onto. One job per pass, newest first.
        let session = self.consensus_manager.consensus().session().await;
        let pending = session.async_get_pending_compute_verdicts(key.validator_id, COMPUTE_PENDING_VERDICT_SCAN).await;
        drop(session);
        let pending_count = pending.len();
        for job in pending {
            if self.compute_inflight.lock().unwrap().verdict_recent(job.certificate_tx_id, now_daa) {
                continue;
            }
            info!(
                "[{VALIDATOR}] compute: sortitioned to audit certificate {} (job {}); replaying — {} pending",
                job.certificate_tx_id, job.job_id, pending_count
            );
            self.audit_one(key, bond_outpoint, role, &job, now_daa).await;
            return; // one job per pass
        }

        // (3) Originate work, if this node is configured to.
        let Some(prompt) = role.prompt.clone() else {
            return;
        };
        // An executor needs enough in-class peers to reach `min_verifier_confirmations`, or its
        // jobs cannot be verified and mint nothing however honestly they are run. Say so rather
        // than burning GPU time and fees on uncreditable work.
        if status.in_class_peer_count < vlt.min_verifier_confirmations as usize {
            trace!(
                "[{VALIDATOR}] compute: {} in-class peer(s) declared this profile but {} confirmations are required; not originating jobs",
                status.in_class_peer_count, vlt.min_verifier_confirmations
            );
            return;
        }

        // An expired commitment is dead weight: no certificate naming it can be credited, and it
        // stays visible for the whole credit window. It must not count as work in progress, or one
        // missed certification would stop this node originating jobs for as long as the window
        // lasts.
        let (live, expired): (Vec<_>, Vec<_>) = status.open_commitments.iter().partition(|c| !c.expired);
        if !expired.is_empty() {
            trace!(
                "[{VALIDATOR}] compute: {} commitment(s) aged past max_commitment_age_blocks and can no longer be certified",
                expired.len()
            );
        }

        // Certify the oldest commitment whose beacon has formed — oldest is also
        // closest-to-expiry.
        for open in &live {
            if !open.beacon_ready {
                trace!("[{VALIDATOR}] compute: commitment {} is waiting on beacon epoch {}", open.commitment_tx_id, open.beacon_epoch);
                continue;
            }
            self.certify_one(key, bond_outpoint, role, open, status.epoch).await;
            return; // one job per pass
        }

        // Nothing in progress — commit to a new job, unless one is already in flight.
        if live.is_empty() && !self.compute_inflight.lock().unwrap().commitment_recent(now_daa) {
            self.submit_commitment(key, bond_outpoint, role, &prompt, now_daa).await;
        }
    }

    /// Declare (or renew) this node's `(model, runtime, determinism class)` capability.
    async fn submit_capability(
        &self,
        key: &ValidatorKey,
        bond_outpoint: TransactionOutpoint,
        role: &ComputeRole,
        expiry_daa_score: u64,
        now_daa: u64,
    ) {
        let entry = role.entry;
        let build = |funding_outpoint, funding: &UtxoEntry, fee| {
            key.build_capability_tx(
                &self.config.network_id,
                bond_outpoint,
                entry.model_weights_hash,
                entry.runtime_hash,
                entry.runtime_class_id,
                expiry_daa_score,
                funding_outpoint,
                funding,
                fee,
            )
        };
        if self.build_and_submit_overlay_tx("capability", COMPUTE_CAPABILITY_PAYLOAD_BYTES, false, build).await.is_some() {
            self.compute_inflight.lock().unwrap().note_capability(now_daa);
            info!("[{VALIDATOR}] compute: declared capability for the registered profile, valid to DAA {expiry_daa_score}");
        }
    }

    /// Publish the phase-1 commitment for a new job: the job id, and the input a verifier will
    /// replay. Both must be on chain before the beacon that draws the committee exists.
    async fn submit_commitment(
        &self,
        key: &ValidatorKey,
        bond_outpoint: TransactionOutpoint,
        role: &ComputeRole,
        prompt: &[u8],
        now_daa: u64,
    ) {
        let job_id = job_spec_id(&role.job_spec(prompt));
        let build = |funding_outpoint, funding: &UtxoEntry, fee| {
            key.build_commitment_tx(&self.config.network_id, job_id, prompt.to_vec(), bond_outpoint, funding_outpoint, funding, fee)
        };
        let payload_bytes = COMPUTE_COMMITMENT_BASE_PAYLOAD_BYTES + prompt.len();
        if self.build_and_submit_overlay_tx("commitment", payload_bytes, false, build).await.is_some() {
            self.compute_inflight.lock().unwrap().note_commitment(now_daa);
            info!("[{VALIDATOR}] compute: committed to job {job_id} ({} byte input); awaiting the sortition beacon", prompt.len());
        }
    }

    /// Execute a job this node committed to and publish its certificate.
    ///
    /// The spec is re-derived from the input the commitment published rather than remembered, so
    /// a restart between committing and certifying loses nothing. Deriving a *different* spec
    /// would produce a certificate whose `job_id` does not match the commitment — refused by the
    /// credit walk — so the derivation is checked against the committed id before any work starts.
    async fn certify_one(
        &self,
        key: &ValidatorKey,
        bond_outpoint: TransactionOutpoint,
        role: &ComputeRole,
        open: &OpenComputeCommitment,
        epoch: u64,
    ) {
        let spec = role.job_spec(&open.input);
        if job_spec_id(&spec) != open.job_id {
            warn!(
                "[{VALIDATOR}] compute: commitment {} names job {} but this node's configuration now derives {}; \
                 the job cannot be certified (has the model table or --compute-max-tokens changed?)",
                open.commitment_tx_id,
                open.job_id,
                job_spec_id(&spec)
            );
            return;
        }
        let Some(projection) = self.run_job(role, &spec, open.input.clone(), false).await else {
            return;
        };
        let receipt = projection.to_compute_receipt();
        let commitment_tx_id = open.commitment_tx_id;
        let spec_for_build = spec.clone();
        let build = |funding_outpoint, funding: &UtxoEntry, fee| {
            key.build_certificate_tx(
                &self.config.network_id,
                epoch,
                commitment_tx_id,
                spec_for_build.clone(),
                receipt,
                bond_outpoint,
                funding_outpoint,
                funding,
                fee,
            )
        };
        if let Some(tx_id) = self.build_and_submit_overlay_tx("certificate", COMPUTE_CERTIFICATE_PAYLOAD_BYTES, false, build).await {
            info!(
                "[{VALIDATOR}] compute: certified job {} in epoch {epoch} as tx {tx_id} (R_j={}); awaiting verifier verdicts",
                open.job_id,
                compute_receipt_hash(&spec, &receipt)
            );
        }
    }

    /// Replay a peer's job and publish the verdict the comparison implies.
    ///
    /// The peer's own projection is never fed to the runtime — a replay audit that is handed the
    /// answer confirms anything. `sign_verifier_verdict` likewise derives the verdict from the two
    /// hashes rather than accepting one, so there is no path here that signs a judgement this
    /// node's own execution does not support.
    async fn audit_one(
        &self,
        key: &ValidatorKey,
        bond_outpoint: TransactionOutpoint,
        role: &ComputeRole,
        job: &PendingComputeVerdict,
        now_daa: u64,
    ) {
        // Guard the fee: a spec whose id is not the job id we were told to audit means the
        // certificate and its commitment disagree, which the credit walk would have refused.
        if job_spec_id(&job.spec) != job.job_id {
            warn!("[{VALIDATOR}] compute: certificate {} names a spec that is not its job id; skipping", job.certificate_tx_id);
            return;
        }
        let Some(projection) = self.run_job(role, &job.spec, job.input.clone(), true).await else {
            return;
        };
        // The proof is THIS node's own projection — the receipt it produced and the preimage of that
        // receipt's `trace_commitment`. Everything else about the verdict, including which way it
        // points, is derived from it, so there is no path here that reports a result this node's
        // execution does not support.
        let verdict = key.sign_verifier_verdict(
            &self.config.network_id,
            job.certificate_tx_id,
            job.job_id,
            job.executor_receipt_hash,
            ReplayProof { receipt: projection.to_compute_receipt(), residuals: projection.residuals() },
            bond_outpoint,
        );
        let replay_receipt_hash = verdict.replay_receipt_hash;
        let refuted = verdict.verdict == VerificationVerdict::Refuted;
        let build = |funding_outpoint, funding: &UtxoEntry, fee| key.build_verdict_tx(&verdict, funding_outpoint, funding, fee);
        if self.build_and_submit_overlay_tx("verdict", COMPUTE_VERDICT_PAYLOAD_BYTES, false, build).await.is_none() {
            return;
        }
        self.compute_inflight.lock().unwrap().note_verdict(job.certificate_tx_id, now_daa);
        if !refuted {
            info!("[{VALIDATOR}] compute: confirmed certificate {} — our replay reproduced R_j", job.certificate_tx_id);
            return;
        }
        warn!(
            "[{VALIDATOR}] compute: REFUTED certificate {} — executor claimed R_j={} but our replay produced {}",
            job.certificate_tx_id, job.executor_receipt_hash, replay_receipt_hash
        );
        // The refutation alone already denies the certificate its credit. A fraud proof adds the
        // reporter reward and slashes the executor, but stakes our own bond on the claim, so it is
        // opt-in — see `ComputeConfig::auto_challenge`.
        if !role.auto_challenge {
            info!(
                "[{VALIDATOR}] compute: not filing a fraud proof (--compute-auto-challenge is off); the refutation already blocks the credit"
            );
            return;
        }
        let reward_payload = key.reward_spk_payload();
        let build = |funding_outpoint, funding: &UtxoEntry, fee| {
            key.build_challenge_tx(
                &self.config.network_id,
                job.certificate_tx_id,
                job.job_id,
                ComputeFraudKind::ForgedReceipt,
                job.executor_receipt_hash,
                replay_receipt_hash,
                job.executor_bond_outpoint,
                Vec::new(),
                bond_outpoint,
                reward_payload,
                funding_outpoint,
                funding,
                fee,
            )
        };
        // Output-less, like slashing evidence: consensus mints the reporter reward at (tx_id, 0).
        if let Some(tx_id) = self.build_and_submit_overlay_tx("challenge", COMPUTE_CHALLENGE_PAYLOAD_BYTES, true, build).await {
            warn!("[{VALIDATOR}] compute: filed a ForgedReceipt fraud proof against {} as tx {tx_id}", job.certificate_tx_id);
        }
    }

    /// Run one job on a blocking thread, so a multi-minute inference does not stall the async
    /// runtime the rest of the node shares.
    ///
    /// `as_verifier` selects the runtime's independent-replica mode. Nothing about what the peer
    /// claimed is passed in — see [`ComputeRuntime::replay`].
    async fn run_job(&self, role: &ComputeRole, spec: &LlmJobSpec, input: Vec<u8>, as_verifier: bool) -> Option<MatchProjection> {
        let runtime = role.runtime();
        let spec = spec.clone();
        let started = std::time::Instant::now();
        let result =
            tokio::task::spawn_blocking(
                move || {
                    if as_verifier { runtime.replay(&spec, &input) } else { runtime.execute(&spec, &input) }
                },
            )
            .await;
        match result {
            Ok(Ok(projection)) => {
                info!("[{VALIDATOR}] compute: job finished in {:?}", started.elapsed());
                Some(projection)
            }
            Ok(Err(err)) => {
                warn!("[{VALIDATOR}] compute: the runtime failed after {:?}: {err}", started.elapsed());
                None
            }
            Err(err) => {
                warn!("[{VALIDATOR}] compute: the job task panicked or was cancelled: {err}");
                None
            }
        }
    }

    /// Fund, build and submit one compute-overlay transaction, reusing the attestation path's
    /// funding chain so a compute transaction and an attestation never select the same UTXO.
    ///
    /// Returns the submitted transaction's id, or `None` if funding, building or submission
    /// failed — every one of which is a retry-next-tick condition, not an error to escalate.
    async fn build_and_submit_overlay_tx<F>(
        &self,
        kind: &str,
        payload_bytes: usize,
        no_change: bool,
        build: F,
    ) -> Option<TransactionId>
    where
        F: FnOnce(TransactionOutpoint, &UtxoEntry, u64) -> Result<Transaction, String>,
    {
        let key = self.key.as_ref()?;
        if self.config.mode != ValidatorMode::Active {
            trace!("[{VALIDATOR}] compute: mode={} so not submitting the {kind} transaction", self.config.mode);
            return None;
        }
        let fee = key.estimate_overlay_fee(&self.mass_calculator, self.config.address_prefix, payload_bytes, no_change);
        let funding_spk = pay_to_address_script(&key.funding_address(self.config.address_prefix));
        let candidates = self.find_funding_candidates(&funding_spk).await;
        let virtual_daa = self.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();
        let funding = {
            let mut chain = self.funding_chain.lock().unwrap();
            let node_outpoints: HashSet<TransactionOutpoint> = candidates.iter().map(|(op, _)| *op).collect();
            chain.inflight_spent.retain(|op| node_outpoints.contains(op));
            if let Some((head, _)) = &chain.pending_change
                && node_outpoints.contains(head)
            {
                chain.pending_change = None;
                chain.chain_head_txid = None;
                chain.chain_head_epoch = None;
                chain.stalled_epochs = 0;
            }
            select_funding(&chain.pending_change, &chain.inflight_spent, candidates, fee, virtual_daa, self.coinbase_maturity).ok()
        };
        let Some((funding_outpoint, funding_entry)) = funding else {
            info!("[{VALIDATOR}] compute: no funding UTXO covering the {kind} fee of {fee} sompi; retrying next heartbeat");
            return None;
        };
        let tx = match build(funding_outpoint, &funding_entry, fee) {
            Ok(tx) => tx,
            Err(e) => {
                warn!("[{VALIDATOR}] compute: could not build the {kind} transaction: {e}");
                return None;
            }
        };
        let tx_id = tx.id();
        let session = self.consensus_manager.consensus().unguarded_session();
        match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
            Ok(()) => {
                let mut chain = self.funding_chain.lock().unwrap();
                chain.inflight_spent.insert(funding_outpoint);
                if no_change {
                    // An output-less transaction leaves nothing to chain from; the next selection
                    // falls back to the node's confirmed view.
                    chain.pending_change = None;
                    chain.chain_head_txid = None;
                } else {
                    let change =
                        UtxoEntry::new(funding_entry.amount - fee, funding_entry.script_public_key.clone(), virtual_daa, false);
                    chain.pending_change = Some((TransactionOutpoint::new(tx_id, 0), change));
                    chain.chain_head_txid = Some(tx_id);
                }
                chain.chain_head_epoch = None;
                chain.stalled_epochs = 0;
                Some(tx_id)
            }
            Err(e) => {
                warn!("[{VALIDATOR}] compute: submit of the {kind} transaction {tx_id} failed: {e}");
                let mut chain = self.funding_chain.lock().unwrap();
                chain.pending_change = None;
                chain.chain_head_txid = None;
                chain.chain_head_epoch = None;
                chain.stalled_epochs = 0;
                None
            }
        }
    }

    /// Equivocation-guarded build of the funded attestation shard tx (ADR-0011). Only on
    /// [`SignedEpochCheckOutcome::Allow`] does it sign the attestation, self-verify it,
    /// persist the signed-epoch record (before any submission), and return the funded
    /// transaction. Refuses on `Block` (would be slashable), skips on `AllowRebroadcast`
    /// (already signed this target this epoch), and returns `None` when no funding UTXO is
    /// available — so the next tick retries once funds arrive.
    fn guarded_build_funded(
        &self,
        target: &ValidatorAttestationTarget,
        key: &ValidatorKey,
        bond_outpoint: TransactionOutpoint,
        funding: Option<(TransactionOutpoint, UtxoEntry)>,
        fee: u64,
    ) -> Option<Transaction> {
        let mut guard = self.signed_epochs.lock().unwrap();
        let Some(store) = guard.as_mut() else {
            trace!("[{VALIDATOR}] eligible for epoch {} but no equivocation-safety log; not signing", target.epoch);
            return None;
        };
        // `signature_fingerprint` is not part of the equivocation predicate, so a
        // placeholder is fine for the pre-sign check; the stored record carries the real one.
        let candidate = SignedEpochRecord {
            epoch: target.epoch,
            target_hash: target.target_hash,
            target_daa_score: target.target_daa_score,
            signature_fingerprint: Hash64::from_bytes([0u8; 64]),
        };
        match store.check(&candidate) {
            SignedEpochCheckOutcome::Block => {
                warn!(
                    "[{VALIDATOR}] EQUIVOCATION BLOCKED: epoch {} already signed a different target; refusing to sign {}",
                    target.epoch, target.target_hash
                );
                None
            }
            SignedEpochCheckOutcome::AllowRebroadcast => {
                info!("[{VALIDATOR}] epoch {} already signed this target; rebroadcast-safe, not re-signing", target.epoch);
                None
            }
            SignedEpochCheckOutcome::Allow => {
                let Some((funding_outpoint, funding_entry)) = funding else {
                    info!(
                        "[{VALIDATOR}] eligible for epoch {} but no funding UTXO at the validator address; skipping (send funds to enable submission)",
                        target.epoch
                    );
                    return None;
                };
                // Sign the attestation, self-verify (never broadcast a bad sig), then build
                // the fee-funded shard tx around it.
                let digest = target.message.as_bytes();
                let signature = key.sign_attestation(&digest);
                if !key.verify_attestation(&digest, &signature) {
                    warn!("[{VALIDATOR}] self-verify of attestation signature failed for epoch {}; not submitting", target.epoch);
                    return None;
                }
                let attestation = StakeAttestation {
                    version: DNS_PAYLOAD_VERSION_V1,
                    validator_id: key.validator_id,
                    bond_outpoint,
                    epoch: target.epoch,
                    target_hash: target.target_hash,
                    target_daa_score: target.target_daa_score,
                    validator_set_commitment: target.validator_set_commitment,
                    signature: signature.to_vec(),
                };
                let shard = single_attestation_shard(attestation);
                let tx = match key.build_funded_shard_tx(&shard, funding_outpoint, &funding_entry, fee) {
                    Ok(tx) => tx,
                    Err(e) => {
                        warn!("[{VALIDATOR}] could not build funded attestation shard tx: {e}");
                        return None;
                    }
                };
                // Persist BEFORE submission. If the flush fails, do not advance — retrying
                // next tick is safe, but submitting without a durable record is not.
                let record = SignedEpochRecord { signature_fingerprint: signature_fingerprint(&signature), ..candidate };
                if let Err(e) = store.record_and_flush(record) {
                    warn!("[{VALIDATOR}] failed to persist signed-epoch record (not advancing): {e}");
                    return None;
                }
                Some(tx)
            }
        }
    }
}

// service trait implementation for the validator service
impl AsyncService for ValidatorService {
    fn ident(self: Arc<Self>) -> &'static str {
        VALIDATOR
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", VALIDATOR);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", VALIDATOR);
            Ok(())
        })
    }
}

// kaspa-pq Phase 11 (ADR-0010): bridge the validator service's status to the RPC layer
// (`getValidatorStatus`). `RpcCoreService` holds this as `Option<Arc<dyn …>>` to avoid a
// crate cycle (rpc-service must not depend on kaspad).
#[async_trait]
impl ValidatorStatusProvider for ValidatorService {
    async fn rpc_validator_status(&self) -> GetValidatorStatusResponse {
        let s = self.status().await;
        GetValidatorStatusResponse {
            enabled: true,
            mode: s.mode.to_string(),
            has_key: s.validator_id.is_some(),
            validator_id: s.validator_id.map(|id| id.to_string()).unwrap_or_default(),
            funding_address: s.funding_address.unwrap_or_default(),
            overlay_configured: s.epoch.is_some(),
            epoch: s.epoch.unwrap_or(0),
            bond_status: match s.bond_status {
                Some(BondStatus::Pending) => "pending",
                Some(BondStatus::Active) => "active",
                Some(BondStatus::Unbonding) => "unbonding",
                Some(BondStatus::Slashed) => "slashed",
                None => "none",
            }
            .to_string(),
            is_active_validator: s.is_active_validator,
            has_signed_epoch: s.epoch.is_some() && s.last_signed_epoch == s.epoch,
            last_signed_epoch: s.last_signed_epoch.unwrap_or(0),
            status: s.status as u32,
            status_label: format!("{:?}", s.status),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_pending_change() -> (TransactionOutpoint, UtxoEntry) {
        (TransactionOutpoint::default(), UtxoEntry::new(1_000, ScriptPublicKey::from_vec(0, vec![]), 0, false))
    }

    #[test]
    fn validator_mode_parsing_roundtrip() {
        for (s, m) in [("active", ValidatorMode::Active), ("standby", ValidatorMode::Standby), ("observer", ValidatorMode::Observer)] {
            assert_eq!(ValidatorMode::from_str(s).unwrap(), m);
            assert_eq!(m.to_string(), s);
        }
        // Case-insensitive and trimmed.
        assert_eq!(ValidatorMode::from_str("  ACTIVE ").unwrap(), ValidatorMode::Active);
        assert!(ValidatorMode::from_str("bogus").is_err());
        // Default is the safe observer mode.
        assert_eq!(ValidatorMode::default(), ValidatorMode::Observer);
    }

    #[test]
    fn derive_validator_status_ladder() {
        use ValidatorStatus::*;
        // Without a key, or outside Active mode → DryRun regardless of eligibility.
        assert_eq!(derive_validator_status(ValidatorMode::Observer, true, Some(BondStatus::Active), true, false), DryRun);
        assert_eq!(derive_validator_status(ValidatorMode::Standby, true, Some(BondStatus::Active), true, false), DryRun);
        assert_eq!(derive_validator_status(ValidatorMode::Active, false, Some(BondStatus::Active), true, false), DryRun);
        // Active mode walks the bond → active-set → already-signed ladder.
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, None, false, false), BondNotFound);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Pending), false, false), BondPending);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Unbonding), false, false), Unbonding);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Slashed), false, false), Slashed);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Active), false, false), ActiveIdle);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Active), true, false), ActiveEligible);
        assert_eq!(derive_validator_status(ValidatorMode::Active, true, Some(BondStatus::Active), true, true), SignedThisEpoch);
    }

    #[test]
    fn funding_chain_keeps_pending_head_when_mempool_resident_past_warn_threshold() {
        let mut chain = FundingChain {
            pending_change: Some(dummy_pending_change()),
            chain_head_txid: Some(TransactionId::default()),
            chain_head_epoch: Some(10),
            ..FundingChain::default()
        };

        assert!(!chain.note_head_mempool_status(11, true));
        assert!(!chain.note_head_mempool_status(12, true));
        assert!(chain.note_head_mempool_status(13, true));
        assert!(chain.note_head_mempool_status(13, true));

        assert!(chain.pending_change.is_some());
        assert_eq!(chain.chain_head_txid, Some(TransactionId::default()));
        assert_eq!(chain.chain_head_epoch, Some(13));
        assert_eq!(chain.stalled_epochs, STALL_WARN_EPOCHS);
    }

    #[test]
    fn funding_chain_gone_head_resets_stall_without_clearing_pending_chain() {
        let mut chain = FundingChain {
            pending_change: Some(dummy_pending_change()),
            chain_head_txid: Some(TransactionId::default()),
            chain_head_epoch: Some(12),
            stalled_epochs: 9,
            ..FundingChain::default()
        };

        assert!(!chain.note_head_mempool_status(13, false));

        assert!(chain.pending_change.is_some());
        assert_eq!(chain.chain_head_txid, Some(TransactionId::default()));
        assert_eq!(chain.chain_head_epoch, None);
        assert_eq!(chain.stalled_epochs, 0);
    }
}
