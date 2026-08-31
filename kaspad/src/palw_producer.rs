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
    challenge_v2, class_ticket_v2,
};
use kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use misaka_palw_base0::produce::base0_rc_job_anchor_v1;

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
    /// ADR-0067: arm the chain-registered-class arm (`--palw-chain-classes`).
    pub chain_classes: bool,
    /// The chain this producer signs for. Bound into the network domain so a signature is a
    /// statement about one incarnation of a network, not about its NAME (audit M2-18).
    pub genesis_hash: kaspa_hashes::Hash64,
    /// Where the execution material behind each published attempt is kept for as long as its
    /// `trace_retention_daa` promises. See `retain_execution` for why this is not optional.
    pub retention_dir: std::path::PathBuf,
    /// **The operator's `--enable-unsynced-mining`, threaded to the producer** — the same escape
    /// the RPC mining path honours (`rpc/service`: `!enable_unsynced_mining && !is_synced` ⇒
    /// refuse). Without it a PALW network cannot be BORN: `should_mine` requires the sink to be
    /// "nearly synced", which means a sink timestamp within a quarter of the difficulty window of
    /// now — and a genesis timestamp is by definition in the past, so on a fresh chain the answer
    /// is false for every node at once and nobody may produce block 1. Measured on testnet-12's
    /// first launch: two peers connected, participation open, and the producer held silently.
    ///
    /// The gate's other two clauses — chain participation and peer connectivity — are NOT waived
    /// by this. They are the ones that stop a node extending a chain it has no business on.
    pub enable_unsynced_mining: bool,
    /// **DRILL ONLY: commit a corrupted execution.** `Some(leaf)` makes every block this node
    /// produces carry a self-consistent fraud — one lane of that step leaf changed and the
    /// commitment re-derived — so a court can be shown convicting on a live chain. The daemon
    /// refuses to set it on a network carrying value.
    pub drill_tamper_leaf: Option<u64>,
    /// Which class to produce for. The daemon passes the bundle's `base_class_id` — the liveness
    /// floor — because that is the one class ADR-0039 W6′ guarantees is always producible.
    pub class_id: Hash64,
    /// The court this network's classes are admitted against. It decides which `(tile_len, n_ctx)`
    /// a class is registered at, and therefore its class id — so resolution cannot be done without
    /// it, and it must be the CHAIN's court rather than a default reconstructed here.
    pub court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    /// **Artifact files this node holds, for classes whose weights are not derivable.**
    ///
    /// The floor's artifact is minted from a pinned seed by every node, so it needs no file and
    /// this stays empty on an RC node. A converted class — a real checkpoint quantized offline —
    /// cannot be re-derived from anything the node has, so its bytes must be carried. Loaded once
    /// at startup and matched against what the CHAIN says the class is; a file that does not
    /// match is not used, never trusted into service.
    pub class_artifacts: Vec<std::path::PathBuf>,
}

pub struct PalwProducerService {
    config: PalwProducerConfig,
    /// Loaded once at construction, through the SDK — each file by its own container's rules
    /// (digest-checked whole for the dense tier, mapped and rooted for the Qwen3.6 tier); whether
    /// a holding is the artifact the CHAIN registered is decided per block, against the producer
    /// facts.
    class_holdings: Vec<misaka_palw_sdk::PalwLoadedArtifactV1>,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    /// `None` disables production and says why at startup — a producer that cannot sign is not a
    /// producer, and finding that out at the first template is finding it out too late.
    keypair: Option<Box<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>>,
    /// The same seed, kept so the receipt lane can build a `ValidatorKey` — its signer API is
    /// what `build_fp_receipt_spend_envelope` takes, and generating the keypair twice from one
    /// seed is the derivation being deterministic, not two identities.
    key_seed: Option<[u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]>,
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

/// The extension a retained capture is stored under. Named once, because two files write it and
/// three read it.
pub(crate) const PALW_RETAINED_MATERIAL_SUFFIX: &str = ".material";

/// **Where a claim's retained capture lives — the one place that decides.**
///
/// The producer writes these files and the panel reads them back to answer a court about its own
/// work, from two different modules. While each built the name itself they could drift silently
/// into disagreement, and the failure mode of that drift is not an error: it is a responder that
/// finds nothing, discloses nothing, and loses every dispute on the clock. One function, so the
/// writer and the reader cannot disagree about what a claim's file is called.
pub(crate) fn palw_retained_material_path(dir: &std::path::Path, claim: &Hash64) -> std::path::PathBuf {
    dir.join(format!("{claim}{PALW_RETAINED_MATERIAL_SUFFIX}"))
}

impl PalwProducerService {
    pub fn new(
        config: PalwProducerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
    ) -> Self {
        let (keypair, key_seed) = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => (Some(Box::new(libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed))), Some(seed)),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] {err} — production disabled");
                (None, None)
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
        // Loaded once — through the SDK, each file by its own container's magic — and each file
        // is refused loudly rather than skipped quietly: an operator who passed
        // `--palw-class-artifact` meant this node to produce for that class, and a node that
        // silently fell back to the floor would look like a working producer that never touches
        // the class they deployed 1.7 GiB for.
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(config.court, config.network_id.as_bytes().to_vec());
        let mut class_holdings = Vec::new();
        for path in &config.class_artifacts {
            match sdk.load_artifact(path) {
                Ok(holding) => {
                    info!("[{PALW_PRODUCER}] {}", holding.summary);
                    class_holdings.push(holding);
                }
                Err(err) => warn!("[{PALW_PRODUCER}] class artifact {} is unusable: {err}", path.display()),
            }
        }
        Self { config, consensus_manager, mining_manager, flow_context, keypair, key_seed, bond, miner_data, class_holdings }
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
    /// The classes this node can serve, from its configuration. Rebuilt per call rather than
    /// cached: it is a handful of clones, and a cache would be a second place the operator's
    /// configuration lives.
    /// One line per class, not one per template: this is a standing property of the family, and a
    /// warning repeated every block is a warning nobody reads (audit3 H4).
    fn warn_once_no_court(&self, class_id: kaspa_consensus_core::Hash64) {
        use std::sync::OnceLock;
        static WARNED: OnceLock<std::sync::Mutex<std::collections::HashSet<kaspa_consensus_core::Hash64>>> = OnceLock::new();
        let warned = WARNED.get_or_init(Default::default);
        if warned.lock().map(|mut w| w.insert(class_id)).unwrap_or(false) {
            warn!(
                "[palw-producer] class {class_id} has NO court responder in this build: neither party can make a move at any \
                 rung, so a dispute about its claims can never be decided. Its arithmetic is unpoliceable — nothing here can \
                 convict a fraudulent producer of this class, and nothing can clear an honest one."
            );
        }
    }

    fn backends(&self) -> crate::palw_backends::PalwBackendRegistry {
        crate::palw_backends::PalwBackendRegistry::new(
            self.config.court,
            self.class_holdings.clone(),
            self.config.network_id.as_bytes().to_vec(),
        )
    }

    /// Takes the ALREADY-ENCODED material rather than the run: the encoding is the backend's,
    /// because only the code that produced material knows how to write it. This function's job is
    /// the obligation — that the bytes are on disk before the block that promises them is
    /// published — and that is the backend's business either way.
    fn retain_execution(&self, attempt_id: Hash64, material: &[u8]) -> Result<Vec<u8>, String> {
        std::fs::create_dir_all(&self.config.retention_dir)
            .map_err(|e| format!("cannot create the retention directory {}: {e}", self.config.retention_dir.display()))?;
        let path = palw_retained_material_path(&self.config.retention_dir, &attempt_id);
        // Still the ONE codec — the retention file, the gossip broadcast and the seat's decode all
        // read these exact bytes — it is just applied one frame up now, where the backend is.
        let bytes = material.to_vec();
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
            let Some(stem) = name.strip_suffix(PALW_RETAINED_MATERIAL_SUFFIX) else { continue };
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
                // **Past the horizon it is not just un-broadcast, it is deleted** (audit M2-22).
                // Retention grew monotonically on the consensus volume — the same volume RocksDB
                // is on — because nothing ever removed a file. The obligation ends with the
                // lattice: a claim older than this can no longer be licensed or disputed, so the
                // bytes serve nobody.
                if let Err(e) = std::fs::remove_file(&path) {
                    trace!("[{PALW_PRODUCER}] cannot prune retained material {}: {e}", path.display());
                }
                continue;
            }
            // **Announced, not pushed** (audit M2-22). This re-broadcast every retained material to
            // every peer once a minute — 291 MB per peer per minute for a QWEN25-A16 producer, of
            // bytes those peers have already deduplicated and dropped. Since protocol 104 a seat
            // that needs a claim's material ASKS for it, and the serve answers that asker directly;
            // the producer's job here is to keep the bytes, and to be reachable.
            let _ = claim;
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
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.network_id.as_bytes(),
            Some(self.config.genesis_hash),
        );
        info!("[{PALW_PRODUCER}] starting (bond={bond}, key={})", self.config.key_path);

        let mut produced = 0u64;
        let mut ticks = 0u64;
        // The last hold reason actually printed, and when. A producer can hold for hours on one
        // unchanging cause, and repeating it every 5 s buries the line that would explain it: this
        // loop wrote 5,281 identical warnings on a live testnet node while it produced nothing.
        let mut last_hold: Option<String> = None;
        let mut last_hold_at: Option<std::time::Instant> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            ticks += 1;
            if ticks.is_multiple_of(300) {
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
                // The operator's explicit escape, and ONLY it: peer connectivity and chain
                // participation are checked separately below, so `--enable-unsynced-mining` buys
                // exactly the "my sink is older than the window" waiver a network's first block
                // needs — never permission to mine alone or on a quarantined chain.
                let peers_and_participation =
                    self.flow_context.hub().has_peers() && self.flow_context.is_consensus_participation_allowed();
                if !(self.config.enable_unsynced_mining && peers_and_participation) {
                    trace!("[{PALW_PRODUCER}] holding: the mining rule engine says this node should not mine");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
                if produced == 0 {
                    info!(
                        "[{PALW_PRODUCER}] the sink is older than the sync window (a fresh chain always is) and --enable-unsynced-mining is set: producing anyway, with peers connected and participation open"
                    );
                }
            }
            let Some(facts) = session.palw_producer_facts_v2(self.config.class_id, Some(bond)) else {
                trace!("[{PALW_PRODUCER}] this network has no ConsensusV2 facts — nothing to produce");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            };
            if let Err(why) = facts.ready_to_produce(&self.verification_key()) {
                // **The reason alone is not a diagnosis.** "this class's epoch budget is already
                // spent" is what a class that exhausted its cap says AND what a class that was
                // never granted one says, and those are opposite problems: the first resolves at
                // the next boundary, the second is a class holding share with no entry in the
                // budget table. Telling them apart took reading consensus source; the numbers that
                // separate them are right here, so carry them.
                let detail = format!(
                    "{why} [class={} epoch={} produced={} budget={}{}]",
                    facts.class_id,
                    facts.epoch_index,
                    facts.epoch_produced_blocks,
                    facts.epoch_budget_blocks,
                    match &facts.bond {
                        Some(bond) =>
                            format!(" exposure={}/{} per_claim={}", bond.reserved_exposure, bond.exposure_ceiling, bond.claim_exposure),
                        None => String::new(),
                    }
                );
                // Once per change, then no more than once every 5 minutes while it persists: a
                // hold that never changes is still worth seeing in a log an operator scrolls.
                let stale = last_hold_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                if last_hold.as_deref() != Some(detail.as_str()) || stale {
                    warn!("[{PALW_PRODUCER}] holding: {detail}");
                    last_hold = Some(detail);
                    last_hold_at = Some(std::time::Instant::now());
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            // Cleared so the next hold, whatever it is, prints immediately rather than being
            // suppressed as a repeat of one the node has since recovered from.
            last_hold = None;
            last_hold_at = None;
            // **The receipt lane, first.** A certified free-prompt claim whose quantum wins its
            // draw is a receipt block waiting to be mined, and it needs no nonce search — the
            // quantum ticket is the lottery, already decided at the claim's beacon. So it is
            // cheaper than an attempt and, unlike one, it turns a claim over into weight. Tried
            // before the attempt for both reasons.
            match self.produce_receipt(&session, network_domain, bond, miner_data.clone()).await {
                Ok(Some(hash)) => {
                    produced += 1;
                    info!("[{PALW_PRODUCER}] produced RECEIPT block #{produced} {hash} (a certified free-prompt claim, mined)");
                    continue;
                }
                Ok(None) => {} // No winning quantum right now; fall through to an attempt.
                Err(err) => warn!("[{PALW_PRODUCER}] receipt: {err}"),
            }
            match self.produce_one(&session, &facts, network_domain, bond, miner_data.clone()).await {
                Ok(Some(hash)) => {
                    produced += 1;
                    info!("[{PALW_PRODUCER}] produced block #{produced} {hash} (class ticket + Layer-0 both under target)");
                    // **The two numbers that say whether this is a PALW network or a hash chain
                    // wearing its clothes.** `safe_weight` leaves zero only when a claim reaches
                    // `Final`, which needs the whole lattice — panel, receipts, quorum, a submitted
                    // `ReceiptLicensed`. Nothing logged it and no RPC returned it, so a fleet could
                    // run all day looking healthy while every claim it made quietly voided. Printed
                    // every block: rising `unresolved` against a flat zero `weight` is the
                    // signature of a lattice that never turns over, and it should be visible from
                    // the log an operator already watches.
                    info!(
                        "[{PALW_PRODUCER}] palw weight={} live_total={} final_claims={} unresolved={} courts={}",
                        facts.safe_weight, facts.live_total, facts.final_claims, facts.unresolved_claims, facts.open_courts
                    );
                }
                Ok(None) => trace!("[{PALW_PRODUCER}] no nonce in {NONCES_PER_TEMPLATE} tries against this template"),
                Err(err) => warn!("[{PALW_PRODUCER}] {err}"),
            }
        }
    }

    /// **One receipt block, if a quantum wins right now.**
    ///
    /// Asks the chain for this bond's spendable quanta (`palw_fp_spendable_v3` — each row carries
    /// the beacon and the ticket-vs-target verdict as read at virtual), takes the first winner, and
    /// builds a header on a fresh template with `pow_algo_id = 7` and the signed spend envelope in
    /// `palw_commitment`. No nonce search: a receipt block's lottery is the quantum ticket, decided
    /// at the claim's beacon, so a winning row is already a valid block modulo signing.
    ///
    /// `Ok(None)` means no quantum wins yet — the ordinary state, and not an error. It is the same
    /// answer whether there are no certified claims or their tickets simply lost this draw; the
    /// operator-facing distinction lives in the log line `produce_one` already prints.
    async fn produce_receipt(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        bond: TransactionOutpoint,
        miner_data: MinerData,
    ) -> Result<Option<kaspa_consensus_core::BlockHash>, String> {
        let seed = self.key_seed.ok_or("no signing key")?;
        let spendable = session.palw_fp_spendable_v3(bond);
        let Some(win) = spendable.into_iter().find(|q| q.wins) else {
            return Ok(None);
        };

        let mut template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;

        // The header the spend binds is THIS one — its pre-pow hash, timestamp and nonce — so the
        // envelope is built after the template exists and re-bound if the template's fields change.
        template.block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&template.block.header);
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        let envelope = key.build_fp_receipt_spend_envelope(
            network_domain,
            pre_pow,
            template.block.header.timestamp,
            template.block.header.nonce,
            win.claim_id,
            win.quantum_index,
            bond,
            win.beacon.beacon_block,
        );
        template.block.header.palw_commitment = envelope.encode();
        template.block.header.finalize();
        let block: kaspa_consensus_core::block::Block = template.block.clone().to_immutable();
        let hash = block.hash();
        self.flow_context
            .submit_rpc_block(session, block)
            .await
            .map_err(|e| format!("the chain refused a receipt block this node produced: {e}"))?;
        Ok(Some(hash))
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

        // **The class comes from the CHAIN, not from a constant here.** This resolved the floor by
        // name — `base0_profile_v1(PALW_RC_BASE0_GEOMETRY)` and `palw_rc_base0_artifact_v1()` —
        // so `class_id` was configurable while the graph and the weights were not, and a node
        // could not produce for a second class however it was registered.
        //
        // `resolve_class_v1` takes the two facts the chain states — which graph (`class_id`) and
        // which weights (`artifact_root`) — and refuses unless this node holds material matching
        // BOTH. The floor is derived so it resolves from nothing; a converted class resolves from
        // a file the operator deployed. Derive, never declare (ADR-0046): the producer proves it
        // has what the chain named rather than asserting it.
        let backend = self
            .backends()
            .resolve_or_chain(facts.class_id, facts.artifact_root, |id| {
                if self.config.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None }
            })
            .map_err(|e| format!("this node cannot produce for the registered class: {e}"))?;
        // **Say out loud that this class cannot be defended in court** (audit3 H4). A family that
        // takes the trait defaults for `bisect_prefix_state`/`refutation_for_index` cannot make a
        // move at any rung, so a dispute about one of its claims can never leave round 0 whichever
        // party is honest. The chain no longer charges anybody for that silence, but the producer
        // should know that its claims are, in practice, unpoliceable — and so should whoever reads
        // its logs before trusting the class.
        if !backend.supports_court() {
            self.warn_once_no_court(facts.class_id);
        }
        // **Through the seam.** The backend is the class's execution path; this
        // function no longer knows which family it is producing for, which is what lets a second
        // one exist. Which backend it is, is the CHAIN's answer (`facts.terms.family`).
        let (job, prompt) = backend.job_for_anchor(anchor).map_err(|e| format!("the job this template implies: {e}"))?;
        // **Off the async worker.** The inference and the nonce grind are pure CPU with no await in
        // them, and they ran inline on the shared `AsyncRuntime` — pinning one tokio worker thread.
        // Trivial at genesis difficulty and not trivial at all once the retarget pulls the search
        // out to the 120 s cadence, at which point that thread is busy essentially all the time and
        // every other service on the runtime is short one worker.
        let (job_for_blocking, prompt_for_blocking) = (job.clone(), prompt.clone());
        let tamper = self.config.drill_tamper_leaf;
        let run = tokio::task::spawn_blocking(move || match tamper {
            None => backend.execute(&job_for_blocking, &prompt_for_blocking),
            Some(leaf) => backend.execute_with_injected_fault(&job_for_blocking, &prompt_for_blocking, leaf),
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
            // hand (`run.material`, encoded by the backend), which is what makes the promise one
            // it can keep.
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
            let material = self.retain_execution(message, &run.material)?;
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
