//! RFC-0008 v2, amendment 1 (spec §10.1, §10.7): **the `EXEC_SLICE` producer** — the executor's node publishing a work slice of a
//! session whose root authorises its bond.
//!
//! A slice is verified only through a claim of the G14 kernel route (`palw_exec_v2_verify`), so the computation, its evidence and its
//! DA material are the kernel route's: the executor posts the slice's kernel job (its nonce is the slice job nonce), seals, commits and
//! serves its kernel claim through the route's own objects and tooling. What remains is the **carriage**: once the claim is on chain, an
//! intent file in `<app>/palw/<network>/exec-slices/` names `(root, index, kernel claim)` —
//!
//! ```json
//! {"root": "<128 hex>", "index": 0, "kernelClaim": "<128 hex>"}
//! ```
//!
//! — and this service derives the slice statement from public rows alone (`ConsensusApi::palw_exec_v2_slice_statement_v1`: the plan's
//! range, the root's bindings, the token states of the claim's prompt and run, the claim and its evidence), has consensus re-shape a
//! template into an `EXEC_SLICE` block (`exec_v2_slice_adapt_block_template`: the lane's parents, algo 10, the coinbase alone), solves
//! the lane's constant PoW, signs the `PXE2` envelope in the slice domain and submits it. Production backpressure (§10.3): it publishes
//! only the root's next index, never two carriers for one slice inside the anchoring window, and nothing below the fence. **Strand
//! recovery (§10.7):** a carrier no anchor on this node's chain covered by the time it left the window is republished at the current
//! anchor — the honest reattachment the v2 permit rules protect. An intent whose slice the chain accepted is removed.
//!
//! It holds the round producer's credentials (`--palw-producer-key`, `--palw-producer-bond`): a slice is the bond's, and the header stage
//! checks the bond's key. Dormant on every shipped preset (`palw_exec_payload_v2` is `None`), where it is not started.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_exec_v2::{
    PALW_EXEC_V2_WIRE_VERSION, PalwExecSubtypeV2, PalwExecV2Envelope, PalwWorkSliceV1, palw_work_slice_payload_root_v2,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const PALW_EXEC_SLICE_PRODUCER: &str = "palw-exec-slice-producer";

/// How often the worker reads its intents.
const POLL_MS: u64 = 2_000;

/// The most nonces one lane block's search tries (the lane's constant target is `2⁻¹⁶`; sixteen times the expected work).
const NONCES_PER_LANE_BLOCK: u64 = 1 << 20;

/// **How long a submitted carrier is waited for before it is republished**, in DAA: the anchoring window is two spans (a span is
/// one DAA on testnet-12), so a carrier no anchor covered after three DAA has left the window and can never be covered (§10.7).
pub const PALW_EXEC_SLICE_REPUBLISH_AFTER_DAA: u64 = 3;

pub struct PalwExecSliceProducerConfig {
    /// Path to the 32-byte hex ML-DSA-87 seed whose verification key the bond registered.
    pub key_path: String,
    /// `<txid>:<index>` of the bond output.
    pub bond: String,
    pub network_id: NetworkId,
    /// The chain this producer signs for — bound into the network domain.
    pub genesis_hash: Hash64,
    /// `Params::palw_exec_payload_v2_fence`: below it nothing is published.
    pub exec_v2_fence: kaspa_consensus_core::config::params::ForkActivation,
    /// The directory the operator's intents are read from.
    pub intents_dir: std::path::PathBuf,
}

/// **An operator's intent**: publish slice `index` of `root`, backed by the kernel claim `kernel_claim` (already on chain).
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecSliceIntentV1 {
    pub root: String,
    pub index: u32,
    pub kernel_claim: String,
}

impl PalwExecSliceIntentV1 {
    fn ids(&self) -> Result<(Hash64, Hash64), String> {
        let parse = |text: &str, what: &str| -> Result<Hash64, String> {
            let mut bytes = [0u8; 64];
            faster_hex::hex_decode(text.as_bytes(), &mut bytes).map_err(|_| format!("{what} is not 128 hex characters"))?;
            Ok(Hash64::from_bytes(bytes))
        };
        Ok((parse(&self.root, "root")?, parse(&self.kernel_claim, "kernelClaim")?))
    }
}

/// What the worker does with an intent this poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwExecSliceStepV1 {
    /// Publish (first time, or a stranded carrier's republish).
    Publish { republish: bool },
    /// A carrier is in flight inside the window: wait.
    Wait,
    /// The chain accepted the slice (or moved past it): the intent is done.
    Done,
    /// Not publishable now, for the named reason.
    Hold(String),
}

/// **The producer's decision for one intent** — pure, so the backpressure and the strand rule are testable: `statement` is the tip's
/// answer for the intent, `in_flight` the carrier this node last submitted for it (and the DAA it was submitted at), `anchored` whether
/// an anchor on this node's chain covered that carrier, `now_daa` the sink's DAA.
pub fn palw_exec_slice_step_v1(
    statement: &Result<PalwWorkSliceV1, String>,
    in_flight: Option<(BlockHash, u64)>,
    anchored: bool,
    now_daa: u64,
) -> PalwExecSliceStepV1 {
    match statement {
        Err(why) if why.contains("is not the next one") || why.contains("is not open") => PalwExecSliceStepV1::Done,
        Err(why) => PalwExecSliceStepV1::Hold(why.clone()),
        Ok(_) => match in_flight {
            None => PalwExecSliceStepV1::Publish { republish: false },
            // Covered, yet the index is still the next one: the fold refused the carrier (its record says why); a second identical
            // carrier would be refused alike, so the intent is held for the operator.
            Some(_) if anchored => PalwExecSliceStepV1::Hold("the chain covered the carrier and refused the slice".into()),
            Some((_, at)) if now_daa <= at.saturating_add(PALW_EXEC_SLICE_REPUBLISH_AFTER_DAA) => PalwExecSliceStepV1::Wait,
            Some(_) => PalwExecSliceStepV1::Publish { republish: true },
        },
    }
}

pub struct PalwExecSliceProducerService {
    config: PalwExecSliceProducerConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    key: Option<kaspa_pq_validator_core::ValidatorKey>,
    bond: Option<PalwBondKeyV2>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwExecSliceProducerService {
    pub fn new(
        config: PalwExecSliceProducerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        let key = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => Some(kaspa_pq_validator_core::ValidatorKey::from_seed(seed)),
            Err(err) => {
                warn!("[{PALW_EXEC_SLICE_PRODUCER}] {err} — no work slice is published");
                None
            }
        };
        let bond = match crate::palw_producer::parse_outpoint(&config.bond) {
            Ok(outpoint) => Some(PalwBondKeyV2(outpoint)),
            Err(err) => {
                warn!("[{PALW_EXEC_SLICE_PRODUCER}] {err} — no work slice is published");
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

    /// The intents on disk, with the files they came from (an unreadable or malformed file is warned about and skipped).
    fn intents(&self) -> Vec<(std::path::PathBuf, PalwExecSliceIntentV1)> {
        let Ok(entries) = std::fs::read_dir(&self.config.intents_dir) else { return Vec::new() };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            match std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| serde_json::from_slice::<PalwExecSliceIntentV1>(&bytes).map_err(|e| e.to_string()))
            {
                Ok(intent) => out.push((path, intent)),
                Err(err) => warn!("[{PALW_EXEC_SLICE_PRODUCER}] {}: {err} — skipped", path.display()),
            }
        }
        out.sort_by(|a, b| (a.1.root.as_str(), a.1.index).cmp(&(b.1.root.as_str(), b.1.index)));
        out
    }

    pub async fn worker(self: &Arc<Self>) {
        let (Some(key), Some(bond)) = (self.key.as_ref(), self.bond) else {
            info!("[{PALW_EXEC_SLICE_PRODUCER}] not producing (see the startup warning above)");
            return;
        };
        info!(
            "[{PALW_EXEC_SLICE_PRODUCER}] starting — RFC-0008 v2 work slices of bond {}:{}, intents in {}",
            bond.0.transaction_id,
            bond.0.index,
            self.config.intents_dir.display()
        );
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.network_id.to_string().as_bytes(),
            Some(self.config.genesis_hash),
        );
        let mut in_flight: BTreeMap<(Hash64, u32), (BlockHash, u64)> = BTreeMap::new();
        let mut published = 0u64;
        loop {
            if !self.tick(std::time::Duration::from_millis(POLL_MS)).await {
                break;
            }
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            if !(self.flow_context.hub().has_peers() && self.flow_context.is_consensus_participation_allowed()) {
                continue;
            }
            let now_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
            if !self.config.exec_v2_fence.is_active(now_daa) {
                continue;
            }
            for (path, intent) in self.intents() {
                let (root, claim) = match intent.ids() {
                    Ok(ids) => ids,
                    Err(err) => {
                        warn!("[{PALW_EXEC_SLICE_PRODUCER}] {}: {err}", path.display());
                        continue;
                    }
                };
                let statement = session.palw_exec_v2_slice_statement_v1(root, intent.index, claim, bond);
                let flight = in_flight.get(&(root, intent.index)).copied();
                let anchored = flight.is_some_and(|(carrier, _)| session.palw_exec_v2_anchored_v1(carrier));
                match palw_exec_slice_step_v1(&statement, flight, anchored, now_daa) {
                    PalwExecSliceStepV1::Done => {
                        in_flight.remove(&(root, intent.index));
                        let _ = std::fs::remove_file(&path);
                        trace!("[{PALW_EXEC_SLICE_PRODUCER}] slice {root}/{} done", intent.index);
                    }
                    PalwExecSliceStepV1::Wait => {}
                    PalwExecSliceStepV1::Hold(why) => trace!("[{PALW_EXEC_SLICE_PRODUCER}] slice {root}/{} held: {why}", intent.index),
                    PalwExecSliceStepV1::Publish { republish } => {
                        let Ok(slice) = statement else { continue };
                        match self.produce(&session, key, bond, network_domain, slice).await {
                            Ok(hash) => {
                                published += 1;
                                in_flight.insert((root, intent.index), (hash, now_daa));
                                if republish {
                                    info!(
                                        "[{PALW_EXEC_SLICE_PRODUCER}] slice {root}/{} republished as {hash}: its carrier left the anchoring window uncovered",
                                        intent.index
                                    );
                                } else {
                                    trace!("[{PALW_EXEC_SLICE_PRODUCER}] slice {root}/{} → {hash} (#{published})", intent.index);
                                }
                            }
                            Err(err) => warn!("[{PALW_EXEC_SLICE_PRODUCER}] slice {root}/{}: {err}", intent.index),
                        }
                    }
                }
            }
        }
        info!("[{PALW_EXEC_SLICE_PRODUCER}] stopping ({published} work-slice carriers this run)");
    }

    /// One `EXEC_SLICE` block: template, adapt, solve, sign, submit.
    async fn produce(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        key: &kaspa_pq_validator_core::ValidatorKey,
        bond: PalwBondKeyV2,
        network_domain: Hash64,
        slice: PalwWorkSliceV1,
    ) -> Result<BlockHash, String> {
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
            session.exec_v2_slice_adapt_block_template(template, payout).map_err(|e| format!("the lane refused the template: {e}"))?;
        let header0 = adapted.block.header.clone();
        let network_id = self.config.network_id;
        let nonce = tokio::task::spawn_blocking(move || {
            let state = kaspa_pow::StateLayer0::new(&header0, network_id.to_string().as_bytes());
            (0..NONCES_PER_LANE_BLOCK).find(|&nonce| state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false))
        })
        .await
        .map_err(|e| format!("the nonce search task did not finish: {e}"))?
        .ok_or_else(|| format!("no nonce in {NONCES_PER_LANE_BLOCK} tries"))?;
        let anchor = adapted.selected_parent_hash;
        let header = &mut adapted.block.header;
        header.nonce = nonce;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        header.palw_commitment = signed_slice_commitment(key, network_domain, bond, slice, anchor, pre_pow, header.timestamp, nonce);
        header.finalize();
        let block: kaspa_consensus_core::block::Block = adapted.block.to_immutable();
        let hash = block.hash();
        crate::palw_producer::palw_until_exit_v1(&self.shutdown.listener, self.flow_context.submit_rpc_block(session, block))
            .await
            .ok_or_else(|| format!("{}: slice block {hash} was not submitted", crate::palw_producer::PALW_PRODUCER_EXITING))?
            .map_err(|e| format!("the chain refused a slice block: {e}"))?;
        Ok(hash)
    }
}

/// **The signed `palw_commitment` of an `EXEC_SLICE` block**: the `PXE2` envelope naming the anchor the block hangs from, the slice and
/// its payload commitment, signed in the slice domain and ML-DSA context over the solved header. Pure.
#[allow(clippy::too_many_arguments)]
fn signed_slice_commitment(
    key: &kaspa_pq_validator_core::ValidatorKey,
    network_domain: Hash64,
    bond: PalwBondKeyV2,
    slice: PalwWorkSliceV1,
    anchor: BlockHash,
    pre_pow: Hash64,
    timestamp: u64,
    nonce: u64,
) -> Vec<u8> {
    let mut envelope = PalwExecV2Envelope {
        version: PALW_EXEC_V2_WIRE_VERSION,
        network_domain,
        anchor,
        subtype: PalwExecSubtypeV2::Slice,
        tx_permit: None,
        payload_root: palw_work_slice_payload_root_v2(&slice),
        executor_bond: bond,
        work_slice: Some(slice),
        pubkey: key.public_key().to_vec(),
        signature: vec![0; kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
    };
    let message = envelope.signing_message(pre_pow, timestamp, nonce).expect("a slice envelope has a message to sign");
    envelope.signature = key.sign_with_context(message.as_byte_slice(), envelope.mldsa87_context()).to_vec();
    envelope.encode()
}

impl AsyncService for PalwExecSliceProducerService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_EXEC_SLICE_PRODUCER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_EXEC_SLICE_PRODUCER);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_EXEC_SLICE_PRODUCER);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_exec_v2::PalwWorkRangeV1;

    fn slice() -> PalwWorkSliceV1 {
        let h = Hash64::from_u64_word;
        PalwWorkSliceV1 {
            root_claim_id: h(1),
            slice_index: 0,
            class_id: h(2),
            canonical_job_id: h(3),
            kernel_version: 1,
            plan_root: h(4),
            canonical_range: PalwWorkRangeV1 { start: 10, end: 20 },
            predecessor_state_root: h(5),
            result_state_root: h(6),
            input_root: h(7),
            output_root: h(8),
            evidence_root: h(9),
            da_root: h(10),
            executor_bond: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_u64_word(11),
                0,
            )),
        }
    }

    /// **Backpressure and strand recovery, as one decision table**: the next index is published once; a carrier in flight is waited for
    /// inside the window; one that left the window uncovered is republished; a covered carrier whose slice the chain refused is held
    /// (a copy would be refused alike); an accepted slice (the root moved past the index) ends the intent; anything else is held by name.
    #[test]
    fn the_producer_publishes_once_waits_inside_the_window_and_republishes_a_stranded_carrier() {
        let ok: Result<PalwWorkSliceV1, String> = Ok(slice());
        let carrier = BlockHash::from_u64_word(77);
        assert_eq!(palw_exec_slice_step_v1(&ok, None, false, 100), PalwExecSliceStepV1::Publish { republish: false });
        assert_eq!(palw_exec_slice_step_v1(&ok, Some((carrier, 100)), false, 100), PalwExecSliceStepV1::Wait);
        assert_eq!(
            palw_exec_slice_step_v1(&ok, Some((carrier, 100)), false, 100 + PALW_EXEC_SLICE_REPUBLISH_AFTER_DAA),
            PalwExecSliceStepV1::Wait,
            "still inside the window"
        );
        assert_eq!(
            palw_exec_slice_step_v1(&ok, Some((carrier, 100)), false, 101 + PALW_EXEC_SLICE_REPUBLISH_AFTER_DAA),
            PalwExecSliceStepV1::Publish { republish: true },
            "left the window uncovered: republished at the current anchor"
        );
        assert!(matches!(palw_exec_slice_step_v1(&ok, Some((carrier, 100)), true, 200), PalwExecSliceStepV1::Hold(_)));
        let accepted: Result<PalwWorkSliceV1, String> = Err("slice 0 is not the next one (1)".into());
        assert_eq!(palw_exec_slice_step_v1(&accepted, Some((carrier, 100)), true, 200), PalwExecSliceStepV1::Done);
        let missing: Result<PalwWorkSliceV1, String> = Err("the kernel route holds no such claim".into());
        assert!(matches!(palw_exec_slice_step_v1(&missing, None, false, 100), PalwExecSliceStepV1::Hold(_)));
    }

    #[test]
    fn an_intent_parses_from_its_file_form_and_a_bad_id_is_named() {
        let intent: PalwExecSliceIntentV1 =
            serde_json::from_str(&format!("{{\"root\":\"{}\",\"index\":3,\"kernelClaim\":\"{}\"}}", "ab".repeat(64), "cd".repeat(64)))
                .unwrap();
        let (root, claim) = intent.ids().unwrap();
        assert_eq!(root.as_bytes(), [0xab; 64]);
        assert_eq!(claim.as_bytes(), [0xcd; 64]);
        let bad = PalwExecSliceIntentV1 { root: "zz".into(), index: 0, kernel_claim: "00".repeat(64) };
        assert!(bad.ids().unwrap_err().contains("root"));
    }
}
