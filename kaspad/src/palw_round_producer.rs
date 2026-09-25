//! ADR-0125: the execution lane's producer — a round block in every one-second round this node's bond
//! holds a permit for.
//!
//! The lane is permissioned by the chain, not by work: each span's schedule — the attempt claims that
//! reached `Final` two spans before, seeded at the span's first chain block by the latest
//! attempt-carrying chain block of the span between (ADR-0130) — grants each round's permits to bonds,
//! at most one an operator and never in two consecutive rounds of the span. This service
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
//!
//! **Where the schedule's permits are TICKETS, it signs every one of its bond's that it can see —
//! past rounds included** (the 2026-09-23 route-matrix re-audit's #1). Past ADR-0151's bundle a
//! span's tickets sit on consecutive rounds from the round of the block that OPENS the span, plus a
//! three-round lead. But a round block anchors at the sink's SELECTED PARENT, so this node reads span
//! `n`'s schedule only once the next chain block on top of the opening block has arrived — on
//! testnet-12, where the DAA moves only on 120 s heartbeats, often a whole heartbeat later. Signing
//! only the wall clock's current round then used a span's tickets from the moment the view flipped:
//! about 38% of them at an exponential 120 s gap, none at all when the next chain block came 123 s or
//! more after the opener. Consensus takes a backdated round — a round block's timestamp need only be
//! its round's and past the median time (`round_adapt_block_template` stamps
//! `max(round start, MTP + 1)` and refuses a round the median time has passed) — so each visible
//! ticket of this bond on a round after the last one signed and not after the current one is signed,
//! oldest first. The residual moves to the window's TAIL: a ticket whose round is still ahead when
//! the view flips to the next span is lost, and the head of the window, which was the whole loss, is
//! not.

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_execution_lane_v1::{
    PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_CONTEXT, PALW_EXEC_ROUND_MS, PalwExecEnvelopeV1, PalwExecScheduleV1,
    palw_exec_signing_message_v1, palw_execution_round_v1,
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
    /// Where the last round this producer signed for survives a restart (ADR-0125 §7.3): a permit is
    /// one block, and a node restarted inside a round must not sign a second block for it.
    pub last_signed_round_path: std::path::PathBuf,
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

    /// Whether the bond's key and outpoint loaded — the worker produces nothing otherwise, and the
    /// daemon says so in one `PALW duties NOT as planned` line (`crate::palw_duties`).
    pub fn bond_identity_loaded(&self) -> bool {
        self.key.is_some() && self.bond.is_some()
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
        // ADR-0125 §7.3: never sign a round at or below the last one signed before a restart — two
        // blocks for one permit burn the permit and slash the bond, whoever's crash caused them. Past
        // it, the highest round this run TRIED: a round refused once (the median time passed it) is
        // not tried again, and rounds are only ever tried in ascending order.
        let mut last_round = last_signed_round(&self.config.last_signed_round_path);
        // The wall-clock round last looked at: the view is read once a round, not once a poll.
        let mut polled_round = 0u64;
        let mut produced = 0u64;
        loop {
            if !self.tick(std::time::Duration::from_millis(POLL_MS)).await {
                break;
            }
            let now_round = palw_execution_round_v1(kaspa_core::time::unix_now(), self.config.genesis_timestamp_ms);
            if now_round <= last_round || now_round <= polled_round {
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
            let Some(status) = session.async_palw_round_lane_status_v1(now_round).await else {
                continue;
            };
            polled_round = now_round;
            // The current round always; where permits are tickets, every earlier ticket of this bond
            // the anchor span's schedule shows (route-matrix re-audit #1) — oldest first.
            let rounds = rounds_to_sign_v1(status.tickets_only.then_some(status.schedule.as_ref()).flatten(), &bond, last_round, now_round);
            for round in rounds {
                let view = if round == now_round {
                    status.view.clone()
                } else {
                    match session.palw_round_view_v1(round) {
                        Some(view) => view,
                        None => break,
                    }
                };
                // A chain block arrived since the schedule was read: the anchor moved to another
                // span, whose schedule the next round's read takes up.
                if view.span != status.view.span {
                    break;
                }
                let Some(permit) = view.permits.iter().find(|permit| permit.bond == bond && !view.used.contains(&permit.index)) else {
                    continue;
                };
                last_round = round;
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
        let payload =
            session.palw_bond_payout_payload_v2(bond).ok_or_else(|| "the bond is not registered on this node's chain".to_string())?;
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
        // ADR-0125 §7.3: the round is recorded BEFORE it is signed. A crash between the two costs this
        // round's permit; the other order could sign one permit twice across a restart.
        record_signed_round(&self.config.last_signed_round_path, round)?;
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

/// **The rounds to try now, oldest first**: this bond's tickets in `schedule` on rounds after
/// `last_round` and before `now_round` (the ones the view could not show while they were current —
/// see the module doc), then `now_round` itself. `schedule` is `None` where permits are not tickets
/// (the ADR-0125 lottery): there only the current round is tried, as before.
fn rounds_to_sign_v1(schedule: Option<&PalwExecScheduleV1>, bond: &PalwBondKeyV2, last_round: u64, now_round: u64) -> Vec<u64> {
    let mut rounds: Vec<u64> = schedule
        .map(|schedule| {
            schedule
                .quanta
                .iter()
                .filter(|ticket| ticket.bond == *bond && ticket.scheduled_round > last_round && ticket.scheduled_round < now_round)
                .map(|ticket| ticket.scheduled_round)
                .collect()
        })
        .unwrap_or_default();
    rounds.sort_unstable();
    rounds.dedup();
    if now_round > last_round {
        rounds.push(now_round);
    }
    rounds
}

/// The last round this producer recorded before signing, or 0 when nothing was.
fn last_signed_round(path: &std::path::Path) -> u64 {
    std::fs::read_to_string(path).ok().and_then(|text| text.trim().parse::<u64>().ok()).unwrap_or(0)
}

/// Record `round` before signing for it. A failure refuses the signature: a round this node cannot
/// remember signing is a round a restart could sign again.
fn record_signed_round(path: &std::path::Path, round: u64) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, round.to_string()).map_err(|e| format!("cannot record round {round} before signing it (not signing): {e}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// **ADR-0125 §7.3: a restart resumes after the last round it recorded**, so a producer never
    /// signs a second block for a permit it already signed — and a round it cannot record is a
    /// round it does not sign.
    #[test]
    fn a_restart_resumes_after_the_last_signed_round() {
        let dir = std::env::temp_dir().join(format!("palw-round-producer-test-{}", std::process::id()));
        let path = dir.join("state").join("palw-round-last-signed");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(last_signed_round(&path), 0, "nothing recorded");
        record_signed_round(&path, 41).expect("recorded");
        record_signed_round(&path, 42).expect("recorded");
        assert_eq!(last_signed_round(&path), 42, "the last round signed survives the restart");
        std::fs::write(&path, "not a round").unwrap();
        assert_eq!(last_signed_round(&path), 0, "an unreadable record reads as none");
        let blocked = dir.join("a-file");
        std::fs::write(&blocked, "").unwrap();
        assert!(record_signed_round(&blocked.join("under-a-file"), 43).is_err(), "no record, no signature");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The route-matrix re-audit's #1: a span's tickets are signed from the moment the schedule is
    /// visible, the head of the window included.** The view flips to span `n` only once a chain block
    /// on top of the opening block arrives; by then the first tickets' rounds have passed. The rounds
    /// tried are this bond's tickets after the last one signed and before now — ascending, so the
    /// restart record stays monotone — then the current round; another bond's tickets and those
    /// already signed are not; and where permits are not tickets only the current round is tried.
    #[test]
    fn the_rounds_to_sign_are_every_visible_ticket_of_this_bond_then_the_current_round() {
        use kaspa_consensus_core::palw_execution_quanta_v1::PalwExecQuantumV1;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        use kaspa_hashes::Hash64;
        let bond = |n: u64| PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0));
        let (mine, theirs) = (bond(1), bond(2));
        let ticket = |owner: PalwBondKeyV2, round: u64| PalwExecQuantumV1 {
            quantum_id: Hash64::from_u64_word(round),
            final_id: Hash64::from_u64_word(7),
            index: 0,
            bond: owner,
            operator_id: Hash64::from_u64_word(9),
            domain: Hash64::from_u64_word(3),
            scheduled_round: round,
        };
        // A span opened at round 1,000: tickets from 1,003, interleaved between two bonds.
        let schedule = PalwExecScheduleV1 {
            span_index: 5,
            seed: Hash64::from_u64_word(1),
            domains: Vec::new(),
            finals: Vec::new(),
            quanta: (1_003..1_123).map(|r| ticket(if r % 3 == 0 { theirs } else { mine }, r)).collect(),
        };
        // The view flipped at round 1,090 (a heartbeat 90 s after the opener): the 58 of this bond's
        // tickets on rounds 1,003..1,089 were invisible while current, and are signed now.
        let now = 1_090;
        let rounds = rounds_to_sign_v1(Some(&schedule), &mine, 0, now);
        let expected: Vec<u64> = (1_003..now).filter(|r| r % 3 != 0).chain([now]).collect();
        assert_eq!(rounds, expected, "every visible ticket of this bond, oldest first, then the current round");
        assert_eq!(rounds.len(), 58 + 1);
        assert!(rounds.windows(2).all(|w| w[0] < w[1]), "ascending: the restart record never goes back");
        // Resumed after signing through 1,050: only what follows.
        assert_eq!(rounds_to_sign_v1(Some(&schedule), &mine, 1_050, now)[0], 1_051 + u64::from(1_051 % 3 == 0));
        // Nothing past the current round is signed ahead.
        assert!(rounds_to_sign_v1(Some(&schedule), &mine, 0, 1_010).iter().all(|r| *r <= 1_010));
        // The lottery (no tickets) tries the current round alone, as before; a round already signed, none.
        assert_eq!(rounds_to_sign_v1(None, &mine, 0, now), vec![now]);
        assert!(rounds_to_sign_v1(Some(&schedule), &mine, now, now).is_empty());
    }
}
