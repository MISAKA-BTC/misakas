//! **The PALW-RC block producer** (ADR-0042) — the thing that makes a `ConsensusV2` network live.
//!
//! Until this existed a testnet-12 node had a genesis and no second block. `misaminer` and
//! `pq-miner` both branch on `POW_ALGO_ID_PALW_LLM | POW_ALGO_ID_PALW_OLLAMA` and nothing else, and
//! the only algo-6 carriage builder in the tree was a `pub(crate)` test helper.
//!
//! # Why it lives in the node
//!
//! Because the carriage cannot be built anywhere else yet, and because of what the challenge binds.
//! `challenge_v2` covers the header's TIMESTAMP and NONCE, so an attempt is mounted at one header
//! position and one only: moving the nonce invalidates the carriage. A miner that received a
//! stamped template and ground the nonce would produce nothing but `PalwV2ChallengeMismatch`. The
//! nonce search and the carriage build are therefore the same loop, and that loop needs the class
//! target, the pwu, the bond registration and the epoch budget — all chain state.
//!
//! Third-party mining over RPC needs those facts on the wire, which is a protocol change and a
//! separate piece of work. This is the piece that makes the RC a network; that is the piece that
//! makes it a network anyone can mine. Saying so is better than shipping a half of it.
//!
//! # The loop, and what each step costs
//!
//! 1. Read [`PalwProducerFactsV2`] and pre-flight against them — wrong key, spent budget or a full
//!    exposure ceiling are all knowable before an inference is spent.
//! 2. Build a template. Its `pre_pow_hash` anchors the JOB (`base0_rc_job_anchor_v1`), so one
//!    template is one job.
//! 3. Run the job — one inference, measured at ~40 ms on the RC floor.
//! 4. Grind the nonce. Per nonce: rebuild the attempt, hash it, check the class ticket and the
//!    Layer-0 target. `l1_tag_v2` is a free CPU expansion, deliberately, so this stays a nonce
//!    search rather than an inference search.
//! 5. Sign ONCE, on a hit. The signature is outside `commitment_root_v2`, so signing per nonce
//!    would be an ML-DSA-87 operation thrown away 99.99% of the time.
//!
//! # The key
//!
//! Loaded with `load_validator_seed`, the same hardened path the validator uses: owner-only perms
//! at creation, no symlinks, fail closed. This service generates no key — an operator makes one
//! with `misaka-cli`, registers the verification key in the genesis card, and points this at the
//! seed.

use std::sync::Arc;

use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    challenge_v2, class_ticket_v2, palw_network_domain_v2,
};
use kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_anchor_v1, base0_rc_job_v1};

pub const PALW_PRODUCER: &str = "palw-producer";

/// How many nonces to try against one template before rebuilding.
///
/// A template goes stale as the past-median time moves, and a stale template's block is refused for
/// its timestamp rather than its work — so the search is bounded and the loop refetches. Bounded
/// LOUDLY: a silent give-up would look identical to a network whose difficulty is out of reach.
const NONCES_PER_TEMPLATE: u64 = 4_000_000;

#[derive(Clone, Debug)]
pub struct PalwProducerConfig {
    /// Path to the 32-byte hex ML-DSA-87 seed whose verification key the genesis bond registered.
    pub key_path: String,
    /// `<txid>:<index>` of the bond output — the same one the genesis card names.
    pub bond: String,
    /// Where the block reward is paid. Must be an ML-DSA-87 P2PKH address (PQ-only consensus).
    pub pay_address: String,
    pub address_prefix: kaspa_addresses::Prefix,
    /// The network's own domain, derived from its `NetworkId` string exactly as consensus does.
    pub network_id: String,
    /// Where the execution material behind each published attempt is kept for as long as its
    /// `trace_retention_daa` promises. See `retain_execution` for why this is not optional.
    pub retention_dir: std::path::PathBuf,
    /// Which class to produce for. The daemon passes the bundle's `base_class_id` — the liveness
    /// floor — because that is the one class ADR-0039 W6′ guarantees is always producible.
    pub class_id: Hash64,
}

pub struct PalwProducerService {
    config: PalwProducerConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    /// `None` disables production and says why at startup — a producer that cannot sign is not a
    /// producer, and finding that out at the first template is finding it out too late.
    keypair: Option<Box<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>>,
    bond: Option<TransactionOutpoint>,
    miner_data: Option<MinerData>,
}

/// `<txid>:<index>`, the same spelling `--stake-bond` uses.
pub(crate) fn parse_outpoint(s: &str) -> Result<TransactionOutpoint, String> {
    let (txid, index) = s.split_once(':').ok_or_else(|| format!("'{s}' is not <txid>:<index>"))?;
    let transaction_id: kaspa_consensus_core::tx::TransactionId =
        txid.parse().map_err(|e| format!("'{txid}' is not a transaction id: {e}"))?;
    let index: u32 = index.parse().map_err(|e| format!("'{index}' is not an output index: {e}"))?;
    Ok(TransactionOutpoint::new(transaction_id, index))
}

impl PalwProducerService {
    pub fn new(
        config: PalwProducerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        let keypair = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => Some(Box::new(libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed))),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] {err} — production disabled");
                None
            }
        };
        let bond = match parse_outpoint(&config.bond) {
            Ok(o) => Some(o),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] {err} — production disabled");
                None
            }
        };
        // The pay address is checked HERE rather than at the first template: a legacy or ECDSA
        // address puts a non-PQ script in the coinbase, the block is dead on arrival, and its
        // reward poisons descendants' fan-out. The RPC path refuses it for the same reason.
        let miner_data = match kaspa_addresses::Address::try_from(config.pay_address.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => {
                warn!("[{PALW_PRODUCER}] pay address is not ML-DSA-87 P2PKH — production disabled");
                None
            }
            Ok(addr) if addr.prefix != config.address_prefix => {
                warn!(
                    "[{PALW_PRODUCER}] pay address is for {} and this node is {} — production disabled",
                    addr.prefix, config.address_prefix
                );
                None
            }
            Ok(addr) => Some(MinerData::new(kaspa_txscript::pay_to_address_script(&addr), Vec::new())),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] pay address is unusable: {err} — production disabled");
                None
            }
        };
        Self { config, consensus_manager, mining_manager, flow_context, keypair, bond, miner_data }
    }

    /// **Keep what the attempt promises to keep.**
    ///
    /// `trace_retention_daa` is a data-availability obligation: the producer is telling the chain it
    /// will serve this execution's material until that DAA score. It was signing that promise and
    /// then dropping the material on the floor — `run.tiles` and `run.binding` died when
    /// `produce_one` returned, and nothing in the tree persisted or served them. A panel asking for
    /// a chunk would have found nothing, and the honest answer to "did you keep it?" was no.
    ///
    /// Written BEFORE the block is published, and a write failure aborts the publish: a promise you
    /// have already broken is not one to make. Keyed by the attempt id, which is what a challenge
    /// names.
    fn retain_execution(
        &self,
        attempt_id: Hash64,
        run: &misaka_palw_base0::produce::Base0ExecutionV1,
    ) -> Result<Vec<u8>, String> {
        std::fs::create_dir_all(&self.config.retention_dir)
            .map_err(|e| format!("cannot create the retention directory {}: {e}", self.config.retention_dir.display()))?;
        let path = self.config.retention_dir.join(format!("{attempt_id}.material"));
        // The ONE codec (`base0_material_encode_v1`): the retention file, the gossip broadcast and
        // the seat's decode all read these exact bytes, so the three cannot drift.
        let bytes = misaka_palw_base0::produce::base0_material_encode_v1(run).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("material.partial");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        // Rename last: a reader never sees a half-written obligation.
        std::fs::rename(&tmp, &path).map_err(|e| format!("cannot publish {}: {e}", path.display()))?;
        Ok(bytes)
    }

    /// **Discharge the data-availability obligation in the open** (launch blockers: "what is
    /// still missing", piece 1). Re-broadcast every retained material younger than the lattice's
    /// own horizon, so seats that connected after the original broadcast — or missed it — still
    /// hear it. Peers deduplicate by digest, so a re-broadcast they have seen costs one message
    /// and no relay.
    async fn rebroadcast_retained(&self) {
        let Ok(entries) = std::fs::read_dir(&self.config.retention_dir) else { return };
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let Some(stem) = name.strip_suffix(".material") else { continue };
            let Ok(claim) = stem.parse::<Hash64>() else { continue };
            // The bind + receipt windows at the frozen 120 s cadence are ~40 h; two days of
            // re-serving covers every claim that can still be licensed, and stops for the rest.
            let fresh = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .map(|age| age < std::time::Duration::from_secs(48 * 3600))
                .unwrap_or(false);
            if !fresh {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                self.flow_context.broadcast_palw_material(claim, bytes).await;
            }
        }
    }

    fn verification_key(&self) -> Vec<u8> {
        self.keypair.as_ref().map(|kp| kp.verification_key.as_ref().to_vec()).unwrap_or_default()
    }

    pub async fn worker(self: &Arc<Self>) {
        let (Some(bond), Some(miner_data)) = (self.bond, self.miner_data.clone()) else {
            info!("[{PALW_PRODUCER}] not producing (see the startup warning above)");
            return;
        };
        if self.keypair.is_none() {
            info!("[{PALW_PRODUCER}] not producing (no signing key)");
            return;
        }
        let network_domain = palw_network_domain_v2(self.config.network_id.as_bytes());
        info!("[{PALW_PRODUCER}] starting (bond={bond}, key={})", self.config.key_path);

        let mut produced = 0u64;
        let mut ticks = 0u64;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            ticks += 1;
            if ticks % 300 == 0 {
                // Every ~60 s: re-serve the retained material of still-licensable claims.
                self.rebroadcast_retained().await;
            }
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // **The gate every participation path consults** — its own doc's words. This loop
            // bypassed it, so it would produce with zero peers, on a stale sink, and while the
            // chain-participation gate was closed: none of which the RPC mining path allows, and
            // all of which put blocks on a chain this node has no business extending.
            if !self.flow_context.should_mine(&session).await {
                trace!("[{PALW_PRODUCER}] holding: the mining rule engine says this node should not mine");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            let Some(facts) = session.palw_producer_facts_v2(self.config.class_id, Some(bond)) else {
                trace!("[{PALW_PRODUCER}] this network has no ConsensusV2 facts — nothing to produce");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            };
            if let Err(why) = facts.ready_to_produce(&self.verification_key()) {
                warn!("[{PALW_PRODUCER}] holding: {why}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            match self.produce_one(&session, &facts, network_domain, bond, miner_data.clone()).await {
                Ok(Some(hash)) => {
                    produced += 1;
                    info!("[{PALW_PRODUCER}] produced block #{produced} {hash} (class ticket + Layer-0 both under target)");
                }
                Ok(None) => trace!("[{PALW_PRODUCER}] no nonce in {NONCES_PER_TEMPLATE} tries against this template"),
                Err(err) => warn!("[{PALW_PRODUCER}] {err}"),
            }
        }
    }

    /// One template, one inference, one bounded nonce search.
    async fn produce_one(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        facts: &PalwProducerFactsV2,
        network_domain: Hash64,
        bond: TransactionOutpoint,
        miner_data: MinerData,
    ) -> Result<Option<kaspa_consensus_core::BlockHash>, String> {
        let mut template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        if template.block.header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            return Err(format!("this network declares algo {} — not a ConsensusV2 lane", template.block.header.pow_algo_id));
        }
        // The job is the TEMPLATE's, so it is computed once and every nonce reuses it. See
        // `base0_rc_job_anchor_v1` for why it is not the challenge's.
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&template.block.header);
        let anchor = base0_rc_job_anchor_v1(network_domain, pre_pow, facts.class_id, &bond);
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .map_err(|e| format!("the floor's graph is not expressible: {e:?}"))?;
        let artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().map_err(|e| format!("the floor's artifact: {e:?}"))?;
        let (job, prompt) = base0_rc_job_v1(
            &profile,
            anchor,
            artifact.shape.vocab,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
        );
        // **Off the async worker.** The inference and the nonce grind are pure CPU with no await in
        // them, and they ran inline on the shared `AsyncRuntime` — pinning one tokio worker thread.
        // Trivial at genesis difficulty and not trivial at all once the retarget pulls the search
        // out to the 120 s cadence, at which point that thread is busy essentially all the time and
        // every other service on the runtime is short one worker.
        let (job_for_blocking, prompt_for_blocking, profile_for_blocking) = (job.clone(), prompt.clone(), profile.clone());
        let run = tokio::task::spawn_blocking(move || {
            base0_execute_for_attempt_v1(&artifact, &profile_for_blocking, &job_for_blocking, &prompt_for_blocking)
                .map_err(|e| format!("the job: {e}"))
        })
        .await
        .map_err(|e| format!("the execution task did not finish: {e}"))??;

        // Every field but the challenge is fixed now: the roots are the execution's and the six
        // chain facts are `facts`'. The nonce moves the challenge, the challenge moves the
        // commitment root, and the root moves BOTH lotteries.
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: Hash64::default(),
            class_id: facts.class_id,
            executor_bond: bond,
            executor_pubkey: self.verification_key(),
            operator_id: facts.bond.as_ref().ok_or("the bond vanished between the pre-flight and the build")?.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            pwu: facts.pwu,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            // The retention window a producer promises to keep the trace for. The material is in
            // hand (`run.tiles`), which is what makes the promise one it can keep.
            // Derived from the network's own lattice windows, not chosen: see the field's docs.
            trace_retention_daa: facts.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let timestamp = template.block.header.timestamp;
        // A dummy of the right length so the shape gate sees the real wire size during the search.
        // The signature is outside `commitment_root_v2`, so it changes neither lottery — signing
        // per nonce would throw away an ML-DSA-87 operation every try.
        let sig_len = self
            .keypair
            .as_ref()
            .map(|kp| {
                libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &[0u8; 64], PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0u8; 32])
                    .map(|s| s.as_ref().len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // The nonce search, also off the async worker and for the same reason as the inference: it
        // is a pure-CPU loop with no await in it, and at the retargeted cadence it runs long enough
        // to hold a tokio worker for essentially all of it.
        let search = {
            let header0 = template.block.header.clone();
            let network_id = self.config.network_id.clone();
            let (class_id, class_target) = (facts.class_id, facts.class_target);
            let mut attempt_for_search = attempt.clone();
            tokio::task::spawn_blocking(move || {
                let mut header = header0;
                for nonce in 0..NONCES_PER_TEMPLATE {
                    attempt_for_search.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, class_id, &bond);
                    if class_ticket_v2(&attempt_for_search) > class_target {
                        continue;
                    }
                    // The class lottery is won; now the network's. Only one nonce in many reaches
                    // here, so the expensive check runs rarely.
                    header.nonce = nonce;
                    header.palw_commitment =
                        PalwAttemptEnvelopeV2 { attempt: attempt_for_search.clone(), signature: vec![0u8; sig_len] }.encode_wire();
                    let state = kaspa_pow::StateLayer0::new(&header, network_id.as_bytes());
                    if state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false) {
                        return Some((nonce, attempt_for_search));
                    }
                }
                None
            })
            .await
            .map_err(|e| format!("the nonce search task did not finish: {e}"))?
        };
        if let Some((nonce, won)) = search {
            attempt = won;
            // Both under target. Sign the attempt id ONCE and publish.
            let kp = self.keypair.as_ref().ok_or("no signing key")?;
            let message = attempt_id_v2(&attempt);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &kp.signing_key,
                message.as_byte_slice(),
                PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
                [0x5Au8; 32],
            )
            .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))?
            .as_ref()
            .to_vec();
            // The promise, kept before it is made. See `retain_execution`.
            let material = self.retain_execution(message, &run)?;
            // And SERVED, not just kept: the panel's seats verify these bytes against the claim's
            // committed roots, and a receipt cannot be filed about material nobody has.
            self.flow_context.broadcast_palw_material(message, material).await;
            template.block.header.nonce = nonce;
            template.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature }.encode_wire();
            template.block.header.finalize();
            let block: kaspa_consensus_core::block::Block = template.block.clone().to_immutable();
            let hash = block.hash();
            self.flow_context
                .submit_rpc_block(session, block)
                .await
                .map_err(|e| format!("the chain refused a block this node produced: {e}"))?;
            return Ok(Some(hash));
        }
        Ok(None)
    }
}

impl AsyncService for PalwProducerService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_PRODUCER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_PRODUCER);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_PRODUCER);
            Ok(())
        })
    }
}
