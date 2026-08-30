//! **The heartbeat miner** — ADR-0060 Decision 1's operational half.
//!
//! A bondless, one-thread hash miner for the heartbeat lane: it asks the mining manager for an
//! ordinary template (so the mempool's transactions — bond registrations included — ride the
//! block), lets consensus re-shape it into the lane
//! (`ConsensusApi::heartbeat_adapt_block_template`: algo-3, the lane's own bits and slot, an
//! empty carriage, a zero-subsidy coinbase), waits for the slot, grinds BLAKE2b-512 ∥ SHA3-512
//! nonces, and submits.
//!
//! It earns fees only, by design. In calm weather this service wakes for a few seconds an hour;
//! in a crisis — every bonded lane silent — the slot ladder tightens to the full cadence and
//! this one thread is what keeps the chain's clock (and every PALW timeout sweep) running.
//! Anyone may run it; the more that do, the harder the lane's own retarget and the same 24/day.

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use std::sync::Arc;

pub const PALW_HEARTBEAT: &str = "palw-heartbeat-miner";

/// Nonces per adapted template before rebuilding. The lane's floor is 2²⁴ expected hashes; two
/// floors' worth makes a give-up rare while a long grind against a moving median time stays
/// bounded. Bounded LOUDLY, like the producer's: a silent give-up looks like an unreachable
/// difficulty.
const NONCES_PER_TEMPLATE: u64 = 1 << 25;

#[derive(Clone, Debug)]
pub struct PalwHeartbeatMinerConfig {
    /// Where the lane's FEES are paid (the subsidy is zero by rule). ML-DSA-87 P2PKH.
    pub pay_address: String,
    pub address_prefix: kaspa_addresses::Prefix,
    pub network_id: NetworkId,
    /// The operator's `--enable-unsynced-mining`, honoured exactly as the producer honours it:
    /// it waives only the "sink is older than the sync window" clause, never peers or
    /// participation.
    pub enable_unsynced_mining: bool,
}

pub struct PalwHeartbeatMinerService {
    config: PalwHeartbeatMinerConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    miner_data: Option<MinerData>,
}

impl PalwHeartbeatMinerService {
    pub fn new(
        config: PalwHeartbeatMinerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        // The same pay-address gate the producer applies, for the same reason: a non-PQ script
        // in the coinbase payload poisons every descendant's reward fan-out.
        let miner_data = match kaspa_addresses::Address::try_from(config.pay_address.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => {
                warn!("[{PALW_HEARTBEAT}] pay address is not ML-DSA-87 P2PKH — heartbeat mining disabled");
                None
            }
            Ok(addr) if addr.prefix != config.address_prefix => {
                warn!(
                    "[{PALW_HEARTBEAT}] pay address is for {} and this node is {} — heartbeat mining disabled",
                    addr.prefix, config.address_prefix
                );
                None
            }
            Ok(addr) => Some(MinerData::new(kaspa_txscript::pay_to_address_script(&addr), Vec::new())),
            Err(err) => {
                warn!("[{PALW_HEARTBEAT}] pay address is unusable: {err} — heartbeat mining disabled");
                None
            }
        };
        Self { config, consensus_manager, mining_manager, flow_context, miner_data }
    }

    pub async fn worker(self: &Arc<Self>) {
        let Some(miner_data) = self.miner_data.clone() else {
            info!("[{PALW_HEARTBEAT}] not mining (see the startup warning above)");
            return;
        };
        info!("[{PALW_HEARTBEAT}] starting — bondless heartbeat lane (ADR-0060), fee-only, one thread");
        let mut mined = 0u64;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // The same participation gate the producer consults, with the same narrow escape.
            if !self.flow_context.should_mine(&session).await {
                let peers_and_participation =
                    self.flow_context.hub().has_peers() && self.flow_context.is_consensus_participation_allowed();
                if !(self.config.enable_unsynced_mining && peers_and_participation) {
                    trace!("[{PALW_HEARTBEAT}] holding: the mining rule engine says this node should not mine");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            }
            match self.mine_one(&session, miner_data.clone()).await {
                Ok(Some(hash)) => {
                    mined += 1;
                    info!("[{PALW_HEARTBEAT}] heartbeat #{mined} {hash} — the clock ticked");
                }
                Ok(None) => {}
                Err(err) => {
                    warn!("[{PALW_HEARTBEAT}] {err}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// One template, one adapt, at most one slot wait, one bounded nonce search.
    async fn mine_one(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        miner_data: MinerData,
    ) -> Result<Option<kaspa_consensus_core::BlockHash>, String> {
        let template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        let (mut template, earliest) =
            session.heartbeat_adapt_block_template(template).map_err(|e| format!("the lane refused the template: {e}"))?;
        let now = kaspa_core::time::unix_now();
        if earliest > now {
            // Inside the slot. Sleep up to the boundary (capped so a ladder change mid-wait is
            // picked up by a fresh template) and try again with fresh facts.
            let wait = (earliest - now).min(60_000u64);
            trace!("[{PALW_HEARTBEAT}] slot in {} s", wait / 1000);
            tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
            return Ok(None);
        }
        // Grind. The lane's floor is ~2²⁴ hashes — seconds of one core — and the retarget can
        // raise it when many nodes run this service; the search stays bounded and loud.
        let header0 = template.block.header.clone();
        let network_id = self.config.network_id;
        let found = tokio::task::spawn_blocking(move || {
            let state = kaspa_pow::StateLayer0::new(&header0, network_id.to_string().as_bytes());
            (0..NONCES_PER_TEMPLATE).find(|&nonce| state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false))
        })
        .await
        .map_err(|e| format!("the nonce search task did not finish: {e}"))?;
        let Some(nonce) = found else {
            trace!("[{PALW_HEARTBEAT}] no nonce in {NONCES_PER_TEMPLATE} tries against this template");
            return Ok(None);
        };
        template.block.header.nonce = nonce;
        template.block.header.finalize();
        let block: kaspa_consensus_core::block::Block = template.block.to_immutable();
        let hash = block.hash();
        self.flow_context
            .submit_rpc_block(session, block)
            .await
            .map_err(|e| format!("the chain refused a heartbeat this node mined: {e}"))?;
        Ok(Some(hash))
    }
}

impl AsyncService for PalwHeartbeatMinerService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_HEARTBEAT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_HEARTBEAT);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_HEARTBEAT);
            Ok(())
        })
    }
}
