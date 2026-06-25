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

use async_trait::async_trait;
use kaspa_addresses::Prefix;
use kaspa_consensus_core::dns_finality::{
    BondStatus, DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation, ValidatorAttestationTarget,
    ValidatorStatus, effective_bond_status, is_bond_active_at, signature_fingerprint, single_attestation_shard,
};
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, UtxoEntry};
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
use kaspa_mining::mempool::tx::Orphan;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_pq_validator_core::{
    ATTESTATION_TX_FEE_FLOOR_SOMPI, SignedEpochStore, ValidatorKey, load_validator_seed, parse_stake_bond_ref, select_funding,
};
use kaspa_rpc_core::model::GetValidatorStatusResponse;
use kaspa_rpc_service::service::ValidatorStatusProvider;
use kaspa_txscript::pay_to_address_script;
use kaspa_utxoindex::api::UtxoIndexProxy;
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
}

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
                        // re-genesis). Gate on the exact §B.4 condition. (The standalone validator also
                        // carries an epoch-counted stuck-chain recovery; that detector is not portable
                        // here because the catch-up loop legitimately chains many epochs per heartbeat —
                        // this start-gate removes the §B.4 stall mode that was the observed root cause.)
                        let activation = bond.as_ref().map(|b| b.activation_daa_score).unwrap_or(u64::MAX);
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
            // If our chain head has been mined (now in the node set), resync to the node view.
            if let Some((head, _)) = &chain.pending_change {
                if node_outpoints.contains(head) {
                    chain.pending_change = None;
                }
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
                        let change = UtxoEntry::new(
                            funding_entry.amount - fee,
                            funding_entry.script_public_key.clone(),
                            virtual_daa,
                            false,
                        );
                        chain.pending_change = Some((TransactionOutpoint::new(tx_id, 0), change));
                    }
                }
                Err(e) => {
                    warn!("[{VALIDATOR}] submit of attestation shard tx {tx_id} (epoch {}) failed: {e}", target.epoch);
                    // Drop the chain head so the next attempt reselects from the node view.
                    self.funding_chain.lock().unwrap().pending_change = None;
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
            let Ok(set) =
                utxoindex.clone().get_utxos_by_script_public_keys([funding_spk.clone()].into_iter().collect()).await
            else {
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
    use std::io::Write;

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
}
