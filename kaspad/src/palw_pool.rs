//! **The pool service: this node's chain, lent to miners that have none** (`misaka-palw-pool`).
//!
//! `palw_producer.rs` says what this is for in its own opening: *"Third-party mining over RPC
//! needs those facts on the wire, which is a protocol change and a separate piece of work."* This
//! is the node side of that work. The producer runs eight steps in one loop; the pool runs the
//! five that need a node and lets a remote miner run the three that need a key.
//!
//! # Why it is a node service and not a separate daemon talking RPC
//!
//! Three of the five things it does have no RPC at all, and two of them could not sensibly have
//! one. `broadcast_palw_material` is a P2P gossip on the flow context; block submission wants the
//! same path the producer uses; and a template must be built at the chain point its facts were
//! read at. A sidecar would have to re-expose all three and would then be a second producer
//! implementation — which is the shape of defect this repository has written down five times.
//!
//! # What this service will and will not do on a miner's behalf
//!
//! It **will** build that miner a template paying that miner, read the chain facts, retain the
//! material the miner sends and gossip it to the panel, and submit the block. Those are the
//! obligations that need a node, and a pooled miner cannot discharge them.
//!
//! It **will not** hold a key, sign an attempt, or take a cut of the coinbase. The coinbase pays
//! the miner's own address, because the template is built around `MinerData` derived from the
//! address that miner authenticated with. An operator who wants a fee takes it out of band; this
//! service has no mechanism for one, deliberately — a pool that could redirect the coinbase is a
//! pool a miner would have to trust about the one thing it can otherwise verify for itself.

use std::sync::Arc;

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use misaka_palw_pool::server::{PoolChainV1, PoolStateV1, PreparedJobV1};
use misaka_palw_pool::session::{BondStandingV1, MinerIdentityV1};

pub const PALW_POOL: &str = "palw-pool";

#[derive(Clone, Debug)]
pub struct PalwPoolConfig {
    /// Where to listen, e.g. `0.0.0.0:26350`.
    pub listen: String,
    /// The class this pool serves. The daemon passes the bundle's `base_class_id` — the floor —
    /// because it is the one class every miner can resolve with no download.
    pub class_id: Hash64,
    pub court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    pub network_id: String,
    pub address_prefix: kaspa_addresses::Prefix,
    /// Where miners' material is kept for as long as their attempts promised. The pool's half of
    /// the bargain: the miner computed it, and this node is the only one of the two with a mouth.
    pub retention_dir: std::path::PathBuf,
    pub max_miners: usize,
    /// Honoured exactly as the producer honours it — see `PalwProducerConfig`.
    pub enable_unsynced_mining: bool,
}

pub struct PalwPoolService {
    config: PalwPoolConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl PalwPoolService {
    pub fn new(
        config: PalwPoolConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        let (shutdown, _) = tokio::sync::watch::channel(false);
        Self { config, consensus_manager, mining_manager, flow_context, shutdown }
    }

    pub async fn worker(self: &Arc<Self>) {
        let listener = match tokio::net::TcpListener::bind(&self.config.listen).await {
            Ok(l) => l,
            Err(e) => {
                warn!("[{PALW_POOL}] cannot listen on {}: {e} — the pool is not running", self.config.listen);
                return;
            }
        };
        info!(
            "[{PALW_POOL}] listening on {} for class {} (up to {} miners; each needs its own registered bond)",
            self.config.listen, self.config.class_id, self.config.max_miners
        );

        // **A pool holds captures, so a pool has to answer for them.**
        //
        // Since the pull transport a seat that needs a claim's material ASKS for it, and only a
        // node with a registered resolver answers — the panel registers one because it owns a
        // retention directory. A pool owns one too: its miners upload the material they cannot
        // serve themselves, and it is written here. Without this line a pool-only node keeps
        // every miner's bytes on disk and is silent when asked for them, so the claim gathers no
        // quorum, voids at its receipt deadline, and the miner is never paid for work it really
        // did. The panel registers the same closure over the same directory, so a node running
        // both is unaffected by whichever registers last.
        self.flow_context
            .palw_gossip()
            .set_material_resolver(crate::palw_producer::palw_material_resolver_v1(self.config.retention_dir.clone()));
        info!("[{PALW_POOL}] answering material pulls from {}", self.config.retention_dir.display());

        // **And a pool that writes into a retention directory has to sweep it** (audit M2-22).
        // Retention grew monotonically on the consensus volume until the producer learned to
        // prune; a pool-only node has no producer loop, so this is where that sweep lives for it.
        let pruner = {
            let retention = self.config.retention_dir.clone();
            let mut shutdown = self.shutdown.subscribe();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                        _ = shutdown.changed() => break,
                    }
                    let removed = crate::palw_producer::palw_prune_retained_v1(&retention);
                    if removed > 0 {
                        info!("[{PALW_POOL}] pruned {removed} retained material file(s) past the lattice horizon");
                    }
                }
            })
        };

        let state = Arc::new(tokio::sync::Mutex::new(PoolStateV1::new()));
        misaka_palw_pool::server::serve_v1(
            self.clone() as Arc<dyn PoolChainV1>,
            listener,
            state,
            self.config.max_miners,
            self.shutdown.subscribe(),
        )
        .await;
        pruner.abort();
    }

    /// The retained-material path, shared with the producer so the two cannot disagree about
    /// what a claim's file is called.
    fn retain(&self, attempt_id: Hash64, material: &[u8]) -> Result<Vec<u8>, String> {
        std::fs::create_dir_all(&self.config.retention_dir)
            .map_err(|e| format!("cannot create the retention directory {}: {e}", self.config.retention_dir.display()))?;
        let path = crate::palw_producer::palw_retained_material_path(&self.config.retention_dir, &attempt_id);
        let tmp = path.with_extension("material.partial");
        std::fs::write(&tmp, material).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("cannot publish {}: {e}", path.display()))?;
        Ok(material.to_vec())
    }
}

#[async_trait::async_trait]
impl PoolChainV1 for PalwPoolService {
    fn network_id(&self) -> String {
        self.config.network_id.clone()
    }

    fn address_prefix(&self) -> kaspa_addresses::Prefix {
        self.config.address_prefix
    }

    fn job_for_class_id(&self) -> Hash64 {
        self.config.class_id
    }

    async fn class_facts(&self) -> (Hash64, Hash64, Vec<u8>, bool) {
        let session = self.consensus_manager.consensus().unguarded_session();
        let facts = session.palw_producer_facts_v2(self.config.class_id, None);
        let (artifact_root, is_base) = facts.map(|f| (f.artifact_root, f.is_base_class)).unwrap_or((Hash64::default(), false));
        (self.config.class_id, artifact_root, borsh::to_vec(&self.config.court).unwrap_or_default(), is_base)
    }

    async fn bond_standing(&self, class_id: Hash64, bond: TransactionOutpoint) -> Result<BondStandingV1, String> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let facts = session.palw_producer_facts_v2(class_id, Some(bond)).ok_or("this network is not running PALW ConsensusV2")?;
        Ok(match &facts.bond {
            None => BondStandingV1 { known: false, registered_pubkey: Vec::new(), not_ready_reason: String::new() },
            Some(bond_facts) => BondStandingV1 {
                known: true,
                registered_pubkey: bond_facts.registered_pubkey.clone(),
                // `ready_to_produce` compares the local key too, and the pool has none — so the
                // bond's OWN preconditions are asked here and the key is compared by the gate,
                // against the same `registered_pubkey` this carries.
                not_ready_reason: facts
                    .ready_to_produce(&bond_facts.registered_pubkey)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default(),
            },
        })
    }

    async fn job_for(&self, identity: MinerIdentityV1) -> Result<PreparedJobV1, String> {
        {
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                return Err("this node is still syncing".into());
            }
            if !self.config.enable_unsynced_mining && !self.flow_context.should_mine(&session).await {
                return Err("this node is not in a state to mine (peers, sync or chain participation)".into());
            }
            let facts = session
                .palw_producer_facts_v2(self.config.class_id, Some(identity.bond))
                .ok_or("this network is not running PALW ConsensusV2")?;
            let bond_facts = facts.bond.as_ref().ok_or("the chain no longer holds this miner's bond")?;
            facts.ready_to_produce(&bond_facts.registered_pubkey).map_err(|e| e.to_string())?;
            // **The template is built to pay THIS miner.** That is the whole payout story: the
            // coinbase names the address the miner authenticated with, and the miner recomputes
            // the merkle root to check that it did.
            let miner_data = MinerData::new(kaspa_txscript::pay_to_address_script(&identity.pay_address), Vec::new());
            let template = self
                .mining_manager
                .clone()
                .get_block_template(&session, miner_data)
                .await
                .map_err(|e| format!("no block template: {e}"))?;
            if template.block.header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
                return Err(format!("this network declares algo {} — not a ConsensusV2 lane", template.block.header.pow_algo_id));
            }
            Ok(PreparedJobV1 {
                header: template.block.header.clone(),
                transactions: template.block.transactions.clone(),
                class_id: facts.class_id,
                artifact_root: facts.artifact_root,
                class_target: facts.class_target,
                pwu: facts.pwu,
                operator_id: bond_facts.operator_id,
                trace_retention_daa: facts.daa_score.saturating_add(facts.min_trace_retention_daa),
            })
        }
    }

    async fn publish(
        &self,
        attempt_id: Hash64,
        material: Vec<u8>,
        block: kaspa_consensus_core::block::Block,
    ) -> Result<Hash64, String> {
        // The promise, kept before it is made — the producer's own order, and for its own reason:
        // a claim whose material nobody serves cannot be licensed, so the bytes are on disk and
        // gossiped BEFORE the block that promises them is published.
        let bytes = self.retain(attempt_id, &material)?;
        self.flow_context.broadcast_palw_material(attempt_id, bytes).await;
        let session = self.consensus_manager.consensus().unguarded_session();
        let hash = block.hash();
        self.flow_context.submit_rpc_block(&session, block).await.map_err(|e| format!("the chain refused this block: {e}"))?;
        Ok(hash)
    }
}

impl AsyncService for PalwPoolService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_POOL
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        let _ = self.shutdown.send(true);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            let _ = self.shutdown.send(true);
            Ok(())
        })
    }
}
