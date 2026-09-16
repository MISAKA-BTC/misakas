//! ADR-0125: the execution lane's producer — a round block in every one-second round this node's bond
//! holds a permit for.
//!
//! The lane is permissioned by the chain, not by work: each span's schedule, derived from the attempt
//! claims that reached `Final` in the span before, grants each round's permits to bonds. This service
//! asks the node's own consensus which permits the current round has
//! (`ConsensusApi::palw_round_view_v1`), and when one names the producer's bond and has not been used,
//! it takes an ordinary template (the mining manager's transaction selection), has consensus re-shape
//! it into a round block (`round_adapt_block_template`: the lane's parents, algo 10, a zero-subsidy
//! coinbase naming the bond's registered payout), solves the lane's constant PoW, signs the permit
//! envelope over the solved header and submits the block.
//!
//! It holds the same credentials the attempt producer does — `--palw-producer-key` and
//! `--palw-producer-bond` — because a permit is the bond's, and the signature the header stage checks
//! is the bond's key's. The fees the round block's transactions pay reach the bond's payout when a
//! chain block merges it and grants the permit.

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_execution_lane_v1::{
    PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_CONTEXT, PALW_EXEC_ROUND_MS, PalwExecEnvelopeV1, palw_exec_signing_message_v1,
    palw_execution_round_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use std::sync::Arc;

pub const PALW_ROUND_PRODUCER: &str = "palw-round-producer";

/// The most nonces one round block's search tries. The lane's target is `2⁻¹⁶`, so this is sixteen
/// times the expected work — enough that a miss means something is wrong rather than unlucky.
const NONCES_PER_ROUND_BLOCK: u64 = 1 << 20;

/// How often the worker looks at the clock between rounds.
const POLL_MS: u64 = 200;

pub struct PalwRoundProducerConfig {
    /// Path to the 32-byte hex ML-DSA-87 seed whose verification key the bond registered.
    pub key_path: String,
    /// `<txid>:<index>` of the bond output.
    pub bond: String,
    pub network_id: NetworkId,
    /// The chain this producer signs for — bound into the network domain, as the attempt producer's is.
    pub genesis_hash: kaspa_hashes::Hash64,
    /// Rounds are whole seconds since this.
    pub genesis_timestamp_ms: u64,
}

pub struct PalwRoundProducerService {
    config: PalwRoundProducerConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    key: Option<kaspa_pq_validator_core::ValidatorKey>,
    bond: Option<PalwBondKeyV2>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwRoundProducerService {
    pub fn new(
        config: PalwRoundProducerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        let key = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => Some(kaspa_pq_validator_core::ValidatorKey::from_seed(seed)),
            Err(err) => {
                warn!("[{PALW_ROUND_PRODUCER}] {err} — the round lane is not produced");
                None
            }
        };
        let bond = match crate::palw_producer::parse_outpoint(&config.bond) {
            Ok(outpoint) => Some(PalwBondKeyV2(outpoint)),
            Err(err) => {
                warn!("[{PALW_ROUND_PRODUCER}] {err} — the round lane is not produced");
                None
            }
        };
        Self { config, consensus_manager, mining_manager, flow_context, key, bond, shutdown: Default::default() }
    }

    async fn tick(&self, period: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(period) => true,
            _ = self.shutdown.listener.clone() => false,
        }
    }

    pub async fn worker(self: &Arc<Self>) {
        let (Some(key), Some(bond)) = (self.key.as_ref(), self.bond) else {
            info!("[{PALW_ROUND_PRODUCER}] not producing (see the startup warning above)");
            return;
        };
        info!("[{PALW_ROUND_PRODUCER}] starting — ADR-0125 execution lane, bond {}:{}", bond.0.transaction_id, bond.0.index);
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.network_id.to_string().as_bytes(),
            Some(self.config.genesis_hash),
        );
        let mut last_round = 0u64;
        let mut produced = 0u64;
        loop {
            if !self.tick(std::time::Duration::from_millis(POLL_MS)).await {
                break;
            }
            let round = palw_execution_round_v1(kaspa_core::time::unix_now(), self.config.genesis_timestamp_ms);
            if round <= last_round {
                continue;
            }
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // A round block nobody hears is a permit spent on nothing, and one on a chain this node
            // may not extend is a block the network refuses — the heartbeat miner's two holds.
            if !(self.flow_context.hub().has_peers() && self.flow_context.is_consensus_participation_allowed()) {
                continue;
            }
            let Some(view) = session.palw_round_view_v1(round) else {
                continue;
            };
            last_round = round;
            let Some(permit) = view.permits.iter().find(|permit| permit.bond == bond && !view.used.contains(&permit.index)) else {
                continue;
            };
            match self.produce(&session, key, bond, network_domain, round, permit.index).await {
                Ok(hash) => {
                    produced += 1;
                    trace!("[{PALW_ROUND_PRODUCER}] round {round} permit {} → {hash} (#{produced})", permit.index);
                    if produced.is_power_of_two() {
                        info!("[{PALW_ROUND_PRODUCER}] {produced} round blocks produced (latest round {round})");
                    }
                }
                Err(err) => warn!("[{PALW_ROUND_PRODUCER}] round {round}: {err}"),
            }
        }
        info!("[{PALW_ROUND_PRODUCER}] stopping ({produced} round blocks this run)");
    }

    /// One round block: template, adapt, solve, sign, submit.
    async fn produce(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        key: &kaspa_pq_validator_core::ValidatorKey,
        bond: PalwBondKeyV2,
        network_domain: kaspa_hashes::Hash64,
        round: u64,
        permit_index: u16,
    ) -> Result<kaspa_consensus_core::BlockHash, String> {
        let payload = session
            .palw_bond_payout_payload_v2(bond)
            .ok_or_else(|| "the bond is not registered on this node's chain".to_string())?;
        let payout = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes());
        let template = self
            .mining_manager
            .clone()
            .get_block_template(session, MinerData::new(payout.clone(), Vec::new()))
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        let mut adapted =
            session.round_adapt_block_template(template, round, payout).map_err(|e| format!("the lane refused the template: {e}"))?;
        let header0 = adapted.block.header.clone();
        let network_id = self.config.network_id;
        let nonce = tokio::task::spawn_blocking(move || {
            let state = kaspa_pow::StateLayer0::new(&header0, network_id.to_string().as_bytes());
            (0..NONCES_PER_ROUND_BLOCK).find(|&nonce| state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false))
        })
        .await
        .map_err(|e| format!("the nonce search task did not finish: {e}"))?
        .ok_or_else(|| format!("no nonce in {NONCES_PER_ROUND_BLOCK} tries"))?;
        let header = &mut adapted.block.header;
        header.nonce = nonce;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let message = palw_exec_signing_message_v1(network_domain, pre_pow, header.timestamp, nonce, round, permit_index, &bond);
        let envelope = PalwExecEnvelopeV1 {
            version: PALW_EXEC_ENVELOPE_VERSION_V1,
            network_domain,
            round,
            permit_index,
            bond,
            pubkey: key.public_key().to_vec(),
            signature: key.sign_with_context(message.as_byte_slice(), PALW_EXEC_MLDSA87_CONTEXT).to_vec(),
        };
        header.palw_commitment = envelope.encode();
        header.finalize();
        // A round ends a second after it starts; a block solved past it still carries its round's
        // timestamp and stays valid while the chain can merge it, so it is submitted either way.
        let late = kaspa_core::time::unix_now().saturating_sub(header.timestamp) > PALW_EXEC_ROUND_MS;
        let block: kaspa_consensus_core::block::Block = adapted.block.to_immutable();
        let hash = block.hash();
        self.flow_context.submit_rpc_block(session, block).await.map_err(|e| format!("the chain refused a round block: {e}"))?;
        if late {
            trace!("[{PALW_ROUND_PRODUCER}] round {round} submitted after its second");
        }
        Ok(hash)
    }
}

impl AsyncService for PalwRoundProducerService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_ROUND_PRODUCER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_ROUND_PRODUCER);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_ROUND_PRODUCER);
            Ok(())
        })
    }
}
