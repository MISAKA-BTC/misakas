
//! The PALW panel service — a seat's whole duty, and the quorum's whole road to the chain
//! (launch blockers: "what is still missing", pieces 2 and 3).
//!
//! One service, three jobs, because they share one inbox:
//!
//! * **Seat.** Poll [`palw_seat_duties_v2`](kaspa_consensus_core::api::ConsensusApi::palw_seat_duties_v2)
//!   for the claims whose panels name this node's bond; verify the gossiped material against the
//!   claim's own committed roots (`base0_material_matches_claim_v1`); sign a
//!   [`PalwSeatReceiptV2`] and broadcast it. `Ok(true)` is `Valid`; `Ok(false)` and `Err` are the
//!   seat's honest `Unavailable` — a mismatch is the court's to convict, not a receipt's.
//! * **Collector.** Pool every receipt gossip delivers (own ones included) and ask consensus —
//!   [`palw_v2_receipt_quorum_assemble`](kaspa_consensus_core::api::ConsensusApi::palw_v2_receipt_quorum_assemble),
//!   which runs the ACCEPTANCE validator itself — whether a quorum stands. What comes back is the
//!   object a block would take, or nothing yet.
//! * **Submitter.** Wrap the object in a 0x4b transaction funded from this node's fee UTXO and
//!   hand it to the mempool. The escrowed worker reward this licenses is the producer's; the fee
//!   is this node's cost of keeping the lattice turning — and a seat that never submits still
//!   earns its keep, because ONE funded node per network suffices (everyone hears every receipt,
//!   and a duplicate submission dies at acceptance as a wrong-phase object, not on-chain).
//!
//! **The wallet decision, decided.** A `ReceiptLicensed` rides a transaction, and a transaction
//! needs a funded input. Rather than a wallet, the service takes ONE outpoint (`--palw-fee-outpoint`)
//! paying to the bond key's own P2PKH address, spends it for each submission, pays the change back
//! to the same address, and persists the rolling outpoint so a restart resumes the chain of change.
//! The operator funds one address once; the bond key — which the node already holds to sign
//! receipts — signs the spends. No second key, no key material beyond what a bonded node has.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectSpaceV1, PalwBisectTurnV1, PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{
    PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT, PALW_COURT_V2_MLDSA87_OPEN_CONTEXT, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT,
    PalwCourtVerdictProofV2, court_session_id_v2,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::mempool::tx::Orphan;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_flows::palw_gossip::PalwGossipEvent;
use kaspa_pq_validator_core::relay_fee_for_compute_mass;
use kaspa_txscript::MLDSA87_TX_CONTEXT;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};

const PALW_PANEL: &str = "palw-panel";
/// How many receipts one claim's pool holds. A panel has 5 seats; the rest is an attacker's spam,
/// and the assembler drops garbage anyway — the cap only bounds memory.
const RECEIPTS_PER_CLAIM: usize = 16;
/// Distinct material payloads kept per claim (mirrors the gossip relay budget).
const MATERIALS_PER_CLAIM: usize = 4;

/// **How many carriers this panel may have unconfirmed at once.**
///
/// Every carrier spends the previous one's change, so a panel's whole output is ONE chain of
/// dependent transactions — and a chain can only be mined in order. Without a bound, a panel that
/// sees hundreds of claims a minute extends it as fast as it can build, far past what the network
/// confirms, and the excess does not queue politely: a peer that has not seen a parent treats the
/// child as an orphan and drops it in relay, silently. Measured on the testnet-11 drill: one panel
/// submitted 791 carriers with zero mempool refusals, the producer received 492 and mined 302, and
/// of 300 `CourtOpened` exactly ONE ever reached a block — while `ReceiptLicensed` kept landing,
/// because those were the ones near the confirmed end of the chain.
///
/// Bounding the in-flight depth converts that silent loss into back-pressure. It also decides
/// WHICH work gets the scarce slots, because court moves are built and submitted before receipt
/// quorums: a rung has a deadline and a quorum does not.
const MAX_INFLIGHT_CARRIERS: usize = 8;
/// Submission attempts per assembled object before giving up (each tick retries).
const SUBMIT_ATTEMPTS: u32 = 3;

pub struct PalwPanelConfig {
    /// Path to the 32-byte hex ML-DSA-87 seed of the bond that holds this node's seats.
    pub key_path: String,
    /// `<txid>:<index>` of that bond — the identity `palw_seat_duties_v2` is asked about.
    pub bond: String,
    /// `<txid>:<index>` of the fee UTXO, paying to the bond key's P2PKH. `None` disables the
    /// submitter: the node still signs and broadcasts receipts, and somebody funded submits.
    pub fee_outpoint: Option<String>,
    /// Where the rolling fee outpoint survives a restart.
    pub state_dir: PathBuf,
    /// The court this network admits classes against — needed to resolve a duty's class the same
    /// way the producer does, because the court decides the geometry a class is registered at.
    pub court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    /// Converted-class artifacts this seat holds. A seat can only judge a class whose weights it
    /// has; the floor's are derived, so this is empty on an RC node.
    pub class_artifacts: Vec<PathBuf>,
    /// **Re-run every licensed claim and dispute the ones this node cannot reproduce.**
    ///
    /// Off by default because it is not free: it costs one full inference per licensed claim, and
    /// opening a court stakes this bond the claim's own reserved amount. A network wants some
    /// nodes doing it, not all of them — which is the same shape as any other watchdog.
    pub challenge: bool,
    /// DRILL ONLY: dispute even a claim this node reproduces exactly, so the innocent half of a
    /// round trip can be shown on a live chain. Refused on mainnet by the daemon.
    pub drill_challenge_all: bool,
    /// Where THIS node's producer persists the material behind its own attempts, when it produces.
    ///
    /// A court can open long after a claim licensed, and the in-memory pool does not live that
    /// long — it drops a claim once licensed, which is strictly before a dispute can start. The
    /// obligation to keep the trace already exists and is already on disk
    /// (`trace_retention_daa`); this is the panel reading it rather than a second copy of the
    /// same promise. Measured on the drill: 143 sessions opened, 4 answered.
    pub retention_dir: PathBuf,
    /// The pinned Metal worker, if this seat has one (ADR-0051). A seat without one cannot judge a
    /// Metal class and files nothing for it — "I could not verify", never an accusation.
    pub metal_worker: Option<PathBuf>,
}

pub struct PalwPanelService {
    config: PalwPanelConfig,
    /// Decoded once. Same contract as the producer's: digest-checked here, matched against the
    /// CHAIN per duty.
    class_artifacts: Vec<misaka_palw_base0::artifact::Base0ArtifactV1>,
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    consensus_config: Arc<Config>,
    keypair: Option<Box<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>>,
    bond: Option<TransactionOutpoint>,
}

impl PalwPanelService {
    /// The families this seat can serve, from its configuration. Rebuilt per use rather than
    /// cached, for the same reason the producer's is: a cache would be a second place the
    /// operator's configuration lives.
    fn backends(&self) -> crate::palw_backends::PalwBackendRegistry {
        crate::palw_backends::PalwBackendRegistry::new(
            self.config.court.clone(),
            self.class_artifacts.clone(),
            self.config.metal_worker.clone(),
            self.consensus_config.params.net.to_string().into_bytes(),
        )
    }

    pub fn new(
        config: PalwPanelConfig,
        consensus_manager: Arc<ConsensusManager>,
        flow_context: Arc<FlowContext>,
        consensus_config: Arc<Config>,
    ) -> Self {
        let keypair = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => Some(Box::new(libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed))),
            Err(err) => {
                warn!("[{PALW_PANEL}] {err} — panel service disabled");
                None
            }
        };
        let bond = match crate::palw_producer::parse_outpoint(&config.bond) {
            Ok(outpoint) => Some(outpoint),
            Err(err) => {
                warn!("[{PALW_PANEL}] --palw-producer-bond: {err} — panel service disabled");
                None
            }
        };
        // Same loader as the producer's, and the same rule: a file that will not load is warned
        // about rather than skipped, because a seat silently unable to judge a class looks exactly
        // like a seat whose material never arrived.
        let mut class_artifacts = Vec::new();
        for path in &config.class_artifacts {
            match std::fs::read(path).map_err(|e| e.to_string()).and_then(|bytes| {
                misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).map_err(|e| e.to_string())
            }) {
                Ok(artifact) => {
                    info!("[{PALW_PANEL}] loaded class artifact {}", path.display());
                    class_artifacts.push(artifact);
                }
                Err(err) => warn!("[{PALW_PANEL}] class artifact {} is unusable: {err}", path.display()),
            }
        }
        Self { config, consensus_manager, flow_context, consensus_config, keypair, bond, class_artifacts }
    }

    fn fee_state_path(&self) -> PathBuf {
        self.config.state_dir.join("palw-fee-outpoint")
    }

    /// The fee outpoint to spend next: the persisted rolling one if it is still unspent, else the
    /// configured one. Returns the entry with it, which is also the unspent check.
    async fn resolve_fee_funding(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Option<(TransactionOutpoint, UtxoEntry)> {
        let configured = self.config.fee_outpoint.as_deref()?;
        let mut candidates: Vec<TransactionOutpoint> = Vec::new();
        if let Ok(persisted) = std::fs::read_to_string(self.fee_state_path())
            && let Ok(outpoint) = crate::palw_producer::parse_outpoint(persisted.trim())
        {
            candidates.push(outpoint);
        }
        if let Ok(outpoint) = crate::palw_producer::parse_outpoint(configured) {
            candidates.push(outpoint);
        }
        for outpoint in &candidates {
            if let Some(entry) = session.get_virtual_utxo_entry(*outpoint) {
                return Some((*outpoint, entry));
            }
        }
        // **Neither remembered outpoint exists, so find the money instead of remembering it.**
        //
        // Both memories can die, and on a live network both do. The CONFIGURED outpoint is a
        // genesis float, spendable exactly once — after the first carrier it is gone forever. The
        // PERSISTED one is the change of the last carrier this panel submitted, which is a promise
        // the chain never made: a carrier dropped in relay, or evicted, is an outpoint that will
        // never exist. When the rolling chain breaks anywhere, the panel is left pointing at a
        // ghost and a spent genesis output, and it can never fund anything again — measured on
        // testnet-11 as `no fee UTXO resolves` forever, on every seat, while the seats kept filing
        // receipts nobody could carry.
        //
        // There is no need to remember anything. Every carrier pays its change back to the SAME
        // script it spent, so this panel's money is whatever the UTXO set holds under its own key.
        // Deriving that script from the keypair makes recovery a function of the chain and the key
        // — no state to lose, and correct after a wipe, a restart, or a dropped carrier.
        //
        // A scan, so it runs only here: the two remembered outpoints are the hot path and this is
        // the path back from having none.
        let script = self.fee_script(session)?;
        let mut cursor: Option<TransactionOutpoint> = None;
        loop {
            let chunk = session.async_get_virtual_utxos(cursor, 1024, cursor.is_some()).await;
            if chunk.is_empty() {
                break;
            }
            cursor = chunk.last().map(|(o, _)| *o);
            if let Some((outpoint, entry)) = chunk.into_iter().find(|(_, e)| e.script_public_key == script && !e.is_coinbase)
            {
                info!("[{PALW_PANEL}] recovered funding at {}:{} — the remembered outpoints were spent or never mined", outpoint.transaction_id, outpoint.index);
                self.persist_fee_outpoint(outpoint);
                return Some((outpoint, entry));
            }
        }
        // **Say which outpoints were tried.** "no fee UTXO resolves" is true of a carrier still in
        // flight, of a configured outpoint that was never funded, and of one the chain has spent —
        // three different operator actions, and the log named none of them. Traced rather than
        // warned because the tick-level warning above already fires once per tick; this is the
        // detail behind it.
        trace!(
            "[{PALW_PANEL}] no fee UTXO resolves; tried {}",
            candidates.iter().map(|o| format!("{}:{}", o.transaction_id, o.index)).collect::<Vec<_>>().join(", ")
        );
        None
    }

    /// The script this panel's carriers pay change to.
    ///
    /// **Read from the CHAIN, not derived from the local key.** A bond's payout address is a
    /// registration fact — whoever registered the bond named it — and it is not a function of the
    /// signing key this panel holds. The genesis fee floats pay to exactly that script, and every
    /// carrier pays its change back to the script it spent, so this is what "an output I can
    /// spend" looks like. Deriving it from the keypair produced a script nothing on the chain pays
    /// to, and the recovery scan found nothing while reporting no error: it was looking for money
    /// that does not exist rather than for the money that does.
    fn fee_script(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Option<kaspa_consensus_core::tx::ScriptPublicKey> {
        let payload = session.palw_bond_payout_payload_v2(PalwBondKeyV2(self.bond?))?;
        Some(kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&payload.as_bytes()))
    }

    /// Sign `message` under this node's bond key in `context`. `None` when the node holds no key,
    /// which is a receipts-only seat and has nothing to say in a court either.
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        let kp = self.keypair.as_ref()?;
        match libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message, context, [0u8; 32]) {
            Ok(sig) => Some(sig.as_ref().to_vec()),
            Err(e) => {
                warn!("[{PALW_PANEL}] ML-DSA-87 sign failed: {e:?}");
                None
            }
        }
    }

    fn persist_fee_outpoint(&self, outpoint: TransactionOutpoint) {
        let _ = std::fs::create_dir_all(&self.config.state_dir);
        if let Err(e) = std::fs::write(self.fee_state_path(), format!("{}:{}", outpoint.transaction_id, outpoint.index)) {
            warn!("[{PALW_PANEL}] cannot persist the rolling fee outpoint: {e} — a restart will fall back to --palw-fee-outpoint");
        }
    }

    /// Build and sign the funded 0x4b carrier for one lifecycle object. The same 1-in/1-out shape
    /// every overlay transaction in this codebase uses; the fee is the node's own relay minimum
    /// for the transaction's real mass, so our own mempool cannot refuse what we built.
    fn build_lifecycle_tx(
        &self,
        object: &PalwConsensusObjectV2,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
    ) -> Result<Transaction, String> {
        let kp = self.keypair.as_ref().ok_or("no signing key")?;
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
            .map_err(|e| format!("the lifecycle payload does not serialize: {e}"))?;
        let params = &self.consensus_config.params;
        let mass_calculator = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );

        // Two passes: the fee depends on the mass, and the mass on the (fixed-size) signature. A
        // dummy signature of the real length prices the transaction, then the real one replaces it.
        let build = |fee: u64, signature_script: Vec<u8>| -> Result<Transaction, String> {
            if funding.amount <= fee {
                return Err(format!("fee UTXO holds {} sompi, the fee is {fee} — fund the address again", funding.amount));
            }
            let mut input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
            input.signature_script = signature_script;
            let change = TransactionOutput::new(funding.amount - fee, funding.script_public_key.clone());
            Ok(Transaction::new(TX_VERSION, vec![input], vec![change], 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload.clone()))
        };

        let dummy_sig_script = {
            let sig = vec![0u8; kaspa_txscript::MLDSA87_SIG_LEN + 1];
            kaspa_txscript::script_builder::ScriptBuilder::new()
                .add_data(&sig)
                .and_then(|b| b.add_data(kp.verification_key.as_ref()))
                .map(|b| b.drain())
                .map_err(|e| format!("sig script shape: {e}"))?
        };
        let priced = build(1, dummy_sig_script)?;
        let fee = relay_fee_for_compute_mass(mass_calculator.calc_non_contextual_masses(&priced).compute_mass);

        let unsigned = build(fee, vec![])?;
        let mtx = MutableTransaction::with_entries(unsigned, vec![funding.clone()]);
        let reused = Mldsa87SigHashReusedValuesUnsync::new();
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);
        let mut sig_data =
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0u8; 32])
                .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))?
                .as_ref()
                .to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let signature_script = kaspa_txscript::script_builder::ScriptBuilder::new()
            .add_data(&sig_data)
            .and_then(|b| b.add_data(kp.verification_key.as_ref()))
            .map(|b| b.drain())
            .map_err(|e| format!("sig script: {e}"))?;
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script = signature_script;
        tx.finalize();
        Ok(tx)
    }

    /// The capture this node's own producer persisted for `claim`, if it is still on disk.
    ///
    /// Best-effort by design: a seat that never produced has no retention directory, and a claim
    /// past its retention window has no file. Both are "cannot answer", which is the same silence
    /// a party with no material has always been allowed. The caller verifies the bytes against the
    /// claim's committed roots before believing them.
    fn retained_capture(&self, claim: &Hash64) -> Option<Vec<u8>> {
        std::fs::read(crate::palw_producer::palw_retained_material_path(&self.config.retention_dir, claim)).ok()
    }

    pub async fn worker(self: &Arc<Self>) {
        let (Some(bond), true) = (self.bond, self.keypair.is_some()) else {
            info!("[{PALW_PANEL}] not running (see the startup warning above)");
            return;
        };
        let bond_key = PalwBondKeyV2(bond);
        let Some(mut inbox) = self.flow_context.palw_gossip().take_inbox() else {
            warn!("[{PALW_PANEL}] the gossip inbox was already taken — panel service disabled");
            return;
        };
        let network_domain =
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2(self.consensus_config.params.net.to_string().as_bytes());
        info!(
            "[{PALW_PANEL}] starting (bond={bond}, submitter={})",
            if self.config.fee_outpoint.is_some() { "funded" } else { "off — receipts only" }
        );

        let mut materials: HashMap<Hash64, Vec<Vec<u8>>> = HashMap::new();
        let mut receipts: HashMap<Hash64, Vec<PalwSeatReceiptV2>> = HashMap::new();
        let mut answered: HashSet<Hash64> = HashSet::new();
        let mut first_seen: HashMap<Hash64, u64> = HashMap::new();
        let mut submitted: HashSet<Hash64> = HashSet::new();
        let mut submit_attempts: HashMap<Hash64, u32> = HashMap::new();
        // One move per (session, round, side): the ladder advances on acceptance, so a move
        // resubmitted before the block that carries it lands is a duplicate the chain drops.
        let mut court_moved: HashSet<(Hash64, u32, bool)> = HashSet::new();
        // Claims this node has already judged: either reproduced (nothing to say) or disputed.
        let mut challenged: HashSet<Hash64> = HashSet::new();
        // **Pending court moves survive the tick.** They used to be built into a per-tick vector
        // and `mem::take`n by the submitter, which dropped every one of them whenever the fee UTXO
        // was busy carrying a receipt — and the claim had already been marked judged, so the
        // dispute was never rebuilt. Measured: 22 frauds detected, 0 courts opened.
        let mut court_pending: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)> = Vec::new();
        // The fee UTXO we are currently spending from, carried across ticks so a mempool chain is
        // not rebuilt from a stale root every two seconds. See the note at its first use.
        let mut chained_funding: Option<(TransactionOutpoint, UtxoEntry)> = None;
        // Carriers submitted whose change is not yet on chain. Reset the moment the chain's tip
        // appears in the virtual UTXO set, which is the only honest signal that it was mined.
        let mut inflight: usize = 0;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            // Drain the gossip inbox first, so this tick's decisions see this tick's mail.
            while let Ok(event) = inbox.try_recv() {
                match event {
                    PalwGossipEvent::Material { claim, bytes } => {
                        let pool = materials.entry(claim).or_default();
                        if pool.len() < MATERIALS_PER_CLAIM {
                            pool.push(bytes);
                        }
                    }
                    PalwGossipEvent::Receipt { bytes } => {
                        if let Ok(receipt) = borsh::from_slice::<PalwSeatReceiptV2>(&bytes) {
                            let pool = receipts.entry(receipt.claim).or_default();
                            if pool.len() < RECEIPTS_PER_CLAIM && !pool.contains(&receipt) {
                                pool.push(receipt);
                            }
                        }
                    }
                }
            }

            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            let current_daa = session.get_virtual_daa_score();


            // --- the challenger's half: dispute a licensed claim whose execution is not the
            // canonical one ---
            //
            // `CourtOpened` was constructed nowhere either, so the only disputes any chain ever saw
            // were the ones a test wrote by hand. A seat's `verify_material` cannot find this
            // fraud: it checks that the capture reproduces the roots the claim COMMITTED, which a
            // producer that ran a wrong execution and committed to it honestly still passes. The
            // only way to see it is to run the job yourself — which is the whole premise of the
            // class being deterministic — and compare.
            //
            // Opening costs this bond the claim's own stake, so it is done on a mismatch and never
            // on a suspicion.
            if self.config.challenge {
                for target in session.palw_disputable_claims_v2(vec![bond_key]) {
                    if challenged.contains(&target.claim_id) {
                        continue;
                    }
                    let Ok(backend) = self.backends().resolve(target.terms, target.class_id, target.artifact_root) else {
                        continue;
                    };
                    let Some(capture) = materials.get(&target.claim_id).and_then(|pool| pool.first()) else { continue };
                    let Ok((binding, _, _, _)) = misaka_palw_base0::produce::base0_material_decode_v1(capture) else { continue };
                    // The job is the ANCHOR's, and the anchor is the job's own id — so the
                    // canonical job is recomputed rather than taken from the producer.
                    let Ok((job, prompt)) = backend.job_for_anchor(binding.job_context.job_id) else { continue };
                    let Ok(mine_run) = backend.execute(&job, &prompt) else { continue };
                    let reproduced = mine_run.execution_root == target.execution_root && mine_run.trace_root == target.trace_root;
                    if reproduced && !self.config.drill_challenge_all {
                        // Reproduced and nothing to say: THIS is the only case where the claim is
                        // finished with. A dispute is finished with only once it has been carried.
                        challenged.insert(target.claim_id);
                        continue;
                    }
                    if reproduced {
                        warn!(
                            "[{PALW_PANEL}] DRILL: opening a court against claim {} which this node REPRODUCES — this \
                             challenge is meant to lose",
                            target.claim_id
                        );
                    } else {
                        warn!(
                            "[{PALW_PANEL}] claim {} committed an execution this node does not reproduce — opening a court",
                            target.claim_id
                        );
                    }
                    let space = PalwBisectSpaceV1::StepLeaves;
                    // **The RULESET's space, not this claim's.** A space the accuser chose is a
                    // ladder depth the accuser chose, so acceptance refuses any declaration that
                    // is not `max_step_leaf_count`. Opening at the job's own 7,900 leaves was
                    // refused on the live drill with exactly that reason, and the dispute died
                    // before the responder ever saw a duty.
                    //
                    // The padding is harmless to the bisection: above the real leaf count both
                    // parties commit to the same full prefix, so a divergence anywhere in the real
                    // leaves keeps producing disagreement until the interval narrows back into
                    // range and lands on it.
                    let space_size = self.config.court.max_step_leaf_count();
                    let session_id = court_session_id_v2(
                        &target.claim_id,
                        &target.trace_root,
                        &target.executor_bond,
                        &bond_key,
                        space,
                        space_size,
                    );
                    let Some(signature) = self.sign(session_id.as_byte_slice(), PALW_COURT_V2_MLDSA87_OPEN_CONTEXT) else { continue };
                    if court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) {
                        continue;
                    }
                    court_pending.push((
                        session_id,
                        0,
                        false,
                        PalwConsensusObjectV2::CourtOpened {
                            session_id,
                            claim: target.claim_id,
                            challenger_bond: bond_key,
                            space,
                            space_size,
                            signature,
                        },
                    ));
                }
            }

            // --- the court's half: answer the disputes this bond is a party to ---
            //
            // Nothing in this tree used to construct a `CourtDisclosed`. A challenger could open a
            // session and the accused had no software able to answer it, so every dispute ran out
            // on the clock — which is why the opening rung had to stop convicting on silence. This
            // is the missing half: both parties act from the SAME capture, through the same
            // functions, so the bisection converges on a real divergence rather than on whoever
            // stayed awake.
            // **Why a party made no move is the fact an operator needs, and it was unloggable.**
            //
            // Six different gates below `continue` on a session, and five of them said nothing at
            // all — so a fleet where nobody answered looked identical whether the node held no
            // backend, no capture, no midpoint, no prefix state, or no key. Measured on the live
            // drill: 357 sessions opened, 357 total moves (one apiece, all openings), and the log
            // could not distinguish "the responder is broken" from "the responder is not a party".
            // `court_stalls` counts the reasons and the tick prints them, which is bounded (one
            // line per tick, only when something stalled) and turns silence into a measurement.
            let court_duties = session.palw_court_duties_v2(vec![bond_key]);
            let mut court_stalls: BTreeMap<&'static str, usize> = BTreeMap::new();
            for duty in &court_duties {
                if court_moved.contains(&(duty.session_id, duty.round, duty.i_am_responder)) {
                    continue;
                }
                // The capture, and the family's backend for it. A party with no material — or a
                // family with no court — cannot answer honestly, and answering dishonestly is what
                // the terminal close exists to punish, so it stays silent and lets the clock decide.
                let backend = match self.backends().resolve(duty.terms, duty.class_id, duty.artifact_root) {
                    Ok(backend) => backend,
                    Err(why) => {
                        // Not rate-limited by session: this one is a NODE-level misconfiguration
                        // (no worker, wrong model, unservable class), identical for every session,
                        // so it is counted per tick like the rest and named once here.
                        *court_stalls.entry("no backend for the class").or_default() += 1;
                        trace!("[{PALW_PANEL}] session {} cannot resolve a backend: {why}", duty.session_id);
                        continue;
                    }
                };
                // **This node's own retained copy, when the pool no longer has it.**
                //
                // `retention_dir` was declared, documented and wired into the config — and never
                // read. The in-memory pool drops a claim once it licenses, which is strictly
                // BEFORE a dispute can start: a challenger has to re-execute the job before it
                // knows there is anything to dispute, so by the time the court duty appears the
                // producer's own capture is gone from memory. The producer then depended on
                // hearing itself through `rebroadcast_retained`, whose 60-second burst try_sends
                // hundreds of files into a 256-slot inbox and silently drops the overflow, so
                // which claims it could answer about was decided by directory order.
                //
                // Measured across two drills with the field unread: 143 sessions opened / 4
                // answered, then 357 opened / 3 answered — the same shape both times.
                //
                // The obligation to keep these bytes already exists and is already on disk
                // (`trace_retention_daa`); this is the panel reading it rather than a second copy
                // of the same promise. Read once and put in the pool, because a court duty makes
                // the claim `live` — so the retention rule below keeps it for the session's life
                // and the next tick costs nothing. Still gated by `verify_material`: a file is
                // evidence only if it reproduces the roots the claim committed to.
                let roots = PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root };
                let pool_has_it = materials
                    .get(&duty.claim_id)
                    .map(|pool| pool.iter().any(|b| backend.verify_material(b, roots) == PalwMaterialVerdictV1::Matches))
                    .unwrap_or(false);
                if !pool_has_it
                    && let Some(bytes) = self.retained_capture(&duty.claim_id)
                    && backend.verify_material(&bytes, roots) == PalwMaterialVerdictV1::Matches
                {
                    info!(
                        "[{PALW_PANEL}] session {} answered from this node's retained capture for claim {}",
                        duty.session_id, duty.claim_id
                    );
                    let pool = materials.entry(duty.claim_id).or_default();
                    if pool.len() < MATERIALS_PER_CLAIM {
                        pool.push(bytes);
                    }
                }
                let Some(capture) = materials
                    .get(&duty.claim_id)
                    .and_then(|pool| pool.iter().find(|b| backend.verify_material(b, roots) == PalwMaterialVerdictV1::Matches))
                else {
                    *court_stalls
                        .entry(if materials.contains_key(&duty.claim_id) {
                            // Held material, and none of it reproduces the claim's roots — a
                            // different failure from holding none, and the one that means the pool
                            // is carrying somebody else's bytes for this claim.
                            "capture held but none matches the claim's roots"
                        } else {
                            "no capture for the claim"
                        })
                        .or_default() += 1;
                    trace!("[{PALW_PANEL}] session {} needs a move but this node holds no matching capture", duty.session_id);
                    continue;
                };
                let object = match (duty.turn, duty.i_am_responder) {
                    // Our disclosure: the state of OUR execution at the midpoint the ladder asks
                    // about. A prefix commitment, so agreeing at an index means agreeing before it.
                    (PalwBisectTurnV1::AwaitDisclosure, true) => {
                        let Some(midpoint) = duty.midpoint else {
                            *court_stalls.entry("the interval has no midpoint to disclose").or_default() += 1;
                            continue;
                        };
                        let Some(mid_state) = backend.bisect_prefix_state(capture, midpoint) else {
                            *court_stalls.entry("the backend cannot state its prefix at the midpoint").or_default() += 1;
                            continue;
                        };
                        let disclosure = PalwBisectDisclosureV1 {
                            version: PALW_BISECT_OBJECT_VERSION_V1,
                            session_id: duty.session_id,
                            round: duty.round,
                            midpoint,
                            mid_state,
                        };
                        let message = borsh::to_vec(&disclosure).expect("a disclosure is borsh-serializable");
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT) else {
                            *court_stalls.entry("no signing key for a disclosure").or_default() += 1;
                            continue;
                        };
                        Some(PalwConsensusObjectV2::CourtDisclosed { session_id: duty.session_id, disclosure, signature })
                    }
                    // Our verdict: does the responder's disclosed state match ours at that index?
                    // `agree` means the prefix matches, so the divergence is ABOVE the midpoint.
                    (PalwBisectTurnV1::AwaitVerdict, false) => {
                        let Some(disclosed) = duty.last_disclosure else {
                            *court_stalls.entry("no disclosure to post a verdict about").or_default() += 1;
                            continue;
                        };
                        let Some(ours) = backend.bisect_prefix_state(capture, disclosed.0) else {
                            *court_stalls.entry("the backend cannot state its prefix at the disclosed index").or_default() += 1;
                            continue;
                        };
                        let verdict = PalwBisectVerdictV1 {
                            version: PALW_BISECT_OBJECT_VERSION_V1,
                            session_id: duty.session_id,
                            round: duty.round,
                            agree: ours == disclosed.1,
                        };
                        let message = borsh::to_vec(&verdict).expect("a verdict is borsh-serializable");
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT) else {
                            *court_stalls.entry("no signing key for a verdict").or_default() += 1;
                            continue;
                        };
                        Some(PalwConsensusObjectV2::CourtVerdictPosted { session_id: duty.session_id, verdict, signature })
                    }
                    // The terminal move. EITHER party may make it, and it is deliberately the SAME
                    // call for both: an honest executor closing its own case and a challenger
                    // closing a real fraud assemble the identical object, and
                    // `adjudicate_court_close_v2` is what decides which way it reads. A prover only
                    // one side could run would be a prover that decides the verdict.
                    (PalwBisectTurnV1::Terminal, _) => {
                        let Some(index) = duty.terminal_index else {
                            *court_stalls.entry("the ladder has not narrowed to a step").or_default() += 1;
                            continue;
                        };
                        let refutation = match backend.refutation_for_index(capture, index) {
                            Ok(r) => r,
                            Err(e) => {
                                *court_stalls.entry("the close does not assemble from this capture").or_default() += 1;
                                warn!("[{PALW_PANEL}] cannot assemble the close for session {}: {e}", duty.session_id);
                                continue;
                            }
                        };
                        let proof = PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings: Vec::new() };
                        // **The chain says what the evidence means, not us.** A `CourtClosed` must
                        // announce the verdict the proof derives to, and the pipeline refuses one
                        // that names any other — so asking first is both the only way to spend a
                        // fee on an object that will land, and the right ordering: the party that
                        // assembled the evidence is not the party that reads it.
                        let Some(verdict) = session.palw_court_close_verdict_v2(&duty.session_id, &proof) else {
                            *court_stalls.entry("the chain reads no verdict from this close").or_default() += 1;
                            trace!("[{PALW_PANEL}] session {} has no adjudicable close from this capture yet", duty.session_id);
                            continue;
                        };
                        info!("[{PALW_PANEL}] session {} closes as {verdict:?} on step {index}", duty.session_id);
                        Some(PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof })
                    }
                    // Not our move: the other party owes this rung. Counted, because "waiting" and
                    // "broken" are the two readings of a quiet court and only one is a problem.
                    _ => {
                        *court_stalls.entry("waiting — the rung is the other party's").or_default() += 1;
                        None
                    }
                };
                let Some(object) = object else { continue };
                court_pending.push((duty.session_id, duty.round, duty.i_am_responder, object));
            }
            if !court_stalls.is_empty() {
                let responder_of = court_duties.iter().filter(|d| d.i_am_responder).count();
                info!(
                    "[{PALW_PANEL}] court: {} sessions ({responder_of} as responder), {} pending; not moved: {}",
                    court_duties.len(),
                    court_pending.len(),
                    court_stalls.iter().map(|(why, n)| format!("{n}× {why}")).collect::<Vec<_>>().join(", ")
                );
            }

            // --- the seat's half: answer every duty exactly once ---
            let duties = session.palw_seat_duties_v2(vec![bond_key]);
            for duty in &duties {
                if answered.contains(&duty.claim_id) || current_daa > duty.receipt_deadline {
                    continue;
                }
                first_seen.entry(duty.claim_id).or_insert(current_daa.max(duty.bound_daa));
                let verdict = 'verdict: {
                    for bytes in materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]) {
                        // Through the family's backend (ADR-0051 step 1). A seat does not know
                        // which family it is judging — Family D recomputes the leg root exactly,
                        // Family M will spot-replay within a tolerance — and `Mismatch` is
                        // deliberately NOT an accusation here: it gathers no quorum and the claim
                        // voids, which is the soft failure both families share.
                        // Resolved per duty from what the CHAIN says the claim's class is
                        // (ADR-0051 step 1). A seat holding no material for that class cannot
                        // judge it and says so by filing nothing — which is the same shape as
                        // "the material has not arrived", and correctly so: both are "I cannot
                        // verify", never "the producer lied".
                        let Ok(backend) = self.backends().resolve(duty.terms, duty.class_id, duty.artifact_root) else {
                            break 'verdict None;
                        };
                        if backend.verify_material(
                            bytes,
                            PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root },
                        ) == PalwMaterialVerdictV1::Matches
                        {
                            break 'verdict Some(PalwReceiptVerdictV2::Valid);
                        }
                    }
                    // No verifying material yet. Wait out half the window before accusing —
                    // gossip is not instant and an early `Unavailable` is a false accusation
                    // with a signature on it.
                    let window = duty.receipt_deadline.saturating_sub(duty.bound_daa);
                    if current_daa >= duty.bound_daa.saturating_add(window / 2) {
                        break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                            chunk_index: 0,
                            requested_daa: first_seen[&duty.claim_id],
                        });
                    }
                    None
                };
                let Some(verdict) = verdict else { continue };
                let signed_daa = current_daa.clamp(duty.bound_daa, duty.receipt_deadline);
                let message = palw_receipt_message_v2(network_domain, duty.claim_id, verdict, signed_daa);
                let kp = self.keypair.as_ref().expect("checked at start");
                let signature = match libcrux_ml_dsa::ml_dsa_87::sign(
                    &kp.signing_key,
                    message.as_byte_slice(),
                    PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                    [0u8; 32],
                ) {
                    Ok(sig) => sig.as_ref().to_vec(),
                    Err(e) => {
                        warn!("[{PALW_PANEL}] ML-DSA-87 sign failed for claim {}: {e:?}", duty.claim_id);
                        continue;
                    }
                };
                let receipt = PalwSeatReceiptV2 { claim: duty.claim_id, verdict, seat_bond: bond_key, signed_daa, signature };
                let bytes = borsh::to_vec(&receipt).expect("a receipt serializes");
                info!("[{PALW_PANEL}] filed a {:?} receipt for claim {}", verdict_name(&verdict), duty.claim_id);
                receipts.entry(duty.claim_id).or_default().push(receipt);
                answered.insert(duty.claim_id);
                self.flow_context.broadcast_palw_seat_receipt(bytes).await;
            }

            // --- the collector + submitter's half ---
            if self.config.fee_outpoint.is_some() {
                // Resolve the fee UTXO ONCE per tick and then CHAIN it: the change of a carrier
                // this tick already submitted is not in the virtual UTXO set — it is in our own
                // mempool — so re-resolving per claim hands every claim after the first the same
                // spent outpoint, and the mempool refuses it as a double spend. Chaining is also
                // what lets the submitter keep up: one carrier per tick cannot track a lane that
                // mints a claim per block.
                // **The mempool chain survives the tick.**
                //
                // Chaining the change within a tick was not enough: `resolve_fee_funding` reads the
                // VIRTUAL utxo set, so a carrier submitted last tick is invisible to it until it is
                // mined — and the fallback is the CONFIGURED outpoint, which is still unspent in
                // virtual because only the mempool has spent it. Every tick therefore rebuilt a
                // transaction spending an output its own predecessor had already taken, and the
                // mempool refused it as a double spend. Measured on the drill: 53 frauds detected,
                // 115 courts intended, `already spent by transaction … in the mempool`, zero
                // carried.
                //
                // So the chained entry is held across ticks and only re-resolved when we have none
                // — and dropped the moment a submission is refused, because that is the signal that
                // the chain we were extending is not one the mempool will accept.
                if chained_funding.is_none() {
                    chained_funding = self.resolve_fee_funding(&session).await;
                    inflight = 0;
                } else if let Some((tip, _)) = &chained_funding
                    && session.get_virtual_utxo_entry(*tip).is_some()
                {
                    // The tip of our own chain is in the UTXO set, so every carrier behind it was
                    // mined. Nothing is in flight and the budget is whole again.
                    inflight = 0;
                }
                let mut funding = if inflight >= MAX_INFLIGHT_CARRIERS {
                    // Back-pressure, not loss. Keeping the objects pending means the next tick
                    // re-offers them in priority order (court moves first), rather than building
                    // carriers the network will drop before anyone reads them.
                    None
                } else {
                    chained_funding.clone()
                };
                if inflight >= MAX_INFLIGHT_CARRIERS {
                    // Not the same condition as "no fee UTXO", and it must not print as one: this
                    // panel is waiting for its own chain to confirm, which is the system working.
                    trace!("[{PALW_PANEL}] holding: {inflight} carriers unconfirmed (cap {MAX_INFLIGHT_CARRIERS})");
                } else if funding.is_none() && receipts.keys().any(|c| !submitted.contains(c)) {
                    // Once per tick, not once per claim: a pending carrier keeps every quorum
                    // unfundable until it is mined, and that used to be tens of thousands of
                    // identical lines an hour.
                    warn!(
                        "[{PALW_PANEL}] a quorum stands but no fee UTXO resolves — a carrier may still be in flight; else \
                         --palw-fee-outpoint ({}) is unfunded or already spent",
                        self.config.fee_outpoint.as_deref().unwrap_or("unset")
                    );
                }
                // The court's moves first: a rung has a deadline and a receipt quorum does not.
                let mut unsent: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)> = Vec::new();
                for (session_id, round, mine_is_responder, object) in std::mem::take(&mut court_pending) {
                    let Some((funding_outpoint, funding_entry)) = funding.clone().filter(|_| inflight < MAX_INFLIGHT_CARRIERS)
                    else {
                        // The fee UTXO is busy. Keep the move: a rung has a deadline, and a dispute
                        // dropped here is a dispute that never happens.
                        unsent.push((session_id, round, mine_is_responder, object));
                        continue;
                    };
                    match self.build_lifecycle_tx(&object, funding_outpoint, &funding_entry) {
                        Ok(tx) => {
                            let txid = tx.id();
                            let change = tx.outputs[0].clone();
                            match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
                                Ok(()) => {
                                    info!(
                                        "[{PALW_PANEL}] submitted {} for court session {session_id} round {round} in tx {txid}",
                                        object_name(&object)
                                    );
                                    let next = TransactionOutpoint::new(txid, 0);
                                    self.persist_fee_outpoint(next);
                                    funding = Some((
                                        next,
                                        UtxoEntry {
                                            amount: change.value,
                                            script_public_key: change.script_public_key,
                                            block_daa_score: current_daa,
                                            is_coinbase: false,
                                        },
                                    ));
                                    inflight += 1;
                                    court_moved.insert((session_id, round, mine_is_responder));
                                    if let PalwConsensusObjectV2::CourtOpened { claim, .. } = &object {
                                        challenged.insert(*claim);
                                    }
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused the {} for session {session_id}: {e}", object_name(&object));
                                    funding = None;
                                }
                            }
                        }
                        Err(e) => warn!("[{PALW_PANEL}] cannot build the carrier for session {session_id}: {e}"),
                    }
                }
                court_pending = unsent;
                let claims: Vec<Hash64> = receipts.keys().copied().collect();
                for claim in claims {
                    if inflight >= MAX_INFLIGHT_CARRIERS {
                        break;
                    }
                    let Some((funding_outpoint, funding_entry)) = funding.clone() else { break };
                    if submitted.contains(&claim) || submit_attempts.get(&claim).copied().unwrap_or(0) >= SUBMIT_ATTEMPTS {
                        continue;
                    }
                    let pool = receipts.get(&claim).cloned().unwrap_or_default();
                    let Some(object) = session.palw_v2_receipt_quorum_assemble(claim, pool) else { continue };
                    *submit_attempts.entry(claim).or_insert(0) += 1;
                    match self.build_lifecycle_tx(&object, funding_outpoint, &funding_entry) {
                        Ok(tx) => {
                            let txid = tx.id();
                            // The change this carrier creates, read off the carrier itself rather
                            // than recomputed — it is the next carrier's input.
                            let change = tx.outputs[0].clone();
                            match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
                                Ok(()) => {
                                    info!("[{PALW_PANEL}] submitted {} for claim {claim} in tx {txid}", object_name(&object));
                                    let next = TransactionOutpoint::new(txid, 0);
                                    self.persist_fee_outpoint(next);
                                    funding = Some((
                                        next,
                                        UtxoEntry {
                                            amount: change.value,
                                            script_public_key: change.script_public_key,
                                            block_daa_score: current_daa,
                                            is_coinbase: false,
                                        },
                                    ));
                                    inflight += 1;
                                    submitted.insert(claim);
                                }
                                Err(e) => {
                                    // The chain we were spending is gone or was never there; stop
                                    // this tick rather than build more carriers on a dead input.
                                    warn!("[{PALW_PANEL}] the mempool refused the {} for claim {claim}: {e}", object_name(&object));
                                    funding = None;
                                }
                            }
                        }
                        Err(e) => warn!("[{PALW_PANEL}] cannot build the carrier for claim {claim}: {e}"),
                    }
                }
                // What the next tick continues from. `None` here means a refusal cleared it, and
                // the next tick resolves afresh.
                chained_funding = funding;
            }

            // Forget what the chain has moved past: a claim with no duty and no unsubmitted quorum
            // holds memory for nothing. (Duties vanish when a claim leaves `PanelBound`.)
            //
            // **A disputed claim is not moved past.** A court opens on a `ReceiptLicensed` claim,
            // which is a phase past `PanelBound` — so the old rule dropped the tiles exactly when a
            // dispute could start needing them, and neither party could disclose or refute. Claims
            // under an open court keep their material for as long as the session lives.
            // **And a claim this node has not yet judged is not moved past either.**
            //
            // The sequence that bites: a claim licenses, its seat duty vanishes, `submitted` holds
            // it, and the material is evicted — and only THEN does a court open on it. By the time
            // the court duty appears the tiles are gone, so the challenger cannot refute and the
            // responder cannot disclose. Keeping court duties alone was not enough because the
            // eviction happens strictly earlier.
            //
            // The window is bounded by this node's own decision latency rather than by the claim's
            // life: a challenger holds a licensed claim's capture until it has either reproduced it
            // (nothing to say) or carried a `CourtOpened` — both of which put it in `challenged` on
            // the next tick. A producer needs no such rule; it re-broadcasts its own material while
            // its claim is unresolved, and `mark_own_material` feeds that back to itself.
            let mut live: HashSet<Hash64> = duties.iter().map(|d| d.claim_id).collect();
            live.extend(court_duties.iter().map(|d| d.claim_id));
            live.extend(court_pending.iter().filter_map(|(_, _, _, o)| match o {
                PalwConsensusObjectV2::CourtOpened { claim, .. } => Some(*claim),
                _ => None,
            }));
            if self.config.challenge {
                live.extend(
                    session.palw_disputable_claims_v2(vec![bond_key]).into_iter().filter_map(|d| {
                        (!challenged.contains(&d.claim_id)).then_some(d.claim_id)
                    }),
                );
            }
            materials.retain(|claim, _| live.contains(claim) || !submitted.contains(claim));
            receipts.retain(|claim, _| live.contains(claim) || !submitted.contains(claim));
            trace!("[{PALW_PANEL}] tick: {} duties, {} claims pooled", duties.len(), receipts.len());
        }
    }
}

fn verdict_name(verdict: &PalwReceiptVerdictV2) -> &'static str {
    match verdict {
        PalwReceiptVerdictV2::Valid => "Valid",
        PalwReceiptVerdictV2::Unavailable { .. } => "Unavailable",
    }
}

/// **Name every variant, and let the compiler keep it that way.**
///
/// This had two arms and a `_ => "lifecycle object"` catch-all, so every court move — the opening,
/// the disclosure, the verdict, the close — logged under one indistinguishable name. On a live
/// drill that is the difference between a readable transcript and none: "submitted lifecycle
/// object for court session X round 0" is true of a challenger opening a case and of the accused
/// answering it, and reading a fleet's court traffic meant guessing which. The catch-all is gone
/// on purpose, so a new object cannot be added without deciding what it is called here.
fn object_name(object: &PalwConsensusObjectV2) -> &'static str {
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { .. } => "ReceiptLicensed",
        PalwConsensusObjectV2::ProducerDefaulted { .. } => "ProducerDefaulted",
        PalwConsensusObjectV2::BondRegistered { .. } => "BondRegistered",
        PalwConsensusObjectV2::BondRetireRequested { .. } => "BondRetireRequested",
        PalwConsensusObjectV2::ClassRegistered { .. } => "ClassRegistered",
        PalwConsensusObjectV2::ClassFrozen { .. } => "ClassFrozen",
        PalwConsensusObjectV2::PanelBound { .. } => "PanelBound",
        PalwConsensusObjectV2::FreePromptCommitted { .. } => "FreePromptCommitted",
        PalwConsensusObjectV2::CourtOpened { .. } => "CourtOpened",
        PalwConsensusObjectV2::CourtDisclosed { .. } => "CourtDisclosed",
        PalwConsensusObjectV2::CourtVerdictPosted { .. } => "CourtVerdictPosted",
        PalwConsensusObjectV2::CourtClosed { .. } => "CourtClosed",
    }
}

impl AsyncService for PalwPanelService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_PANEL
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_PANEL);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_PANEL);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A responder must be able to answer a court about work it has already forgotten.**
    ///
    /// `retention_dir` was declared on this config, documented at length with the measurement that
    /// motivated it, and read by nothing. The panel's in-memory pool drops a claim the moment it
    /// licenses, which is strictly before a dispute can start — a challenger has to re-execute the
    /// job before it knows there is anything to dispute — so a producer's own capture is reliably
    /// gone by the time its court duty appears. Two live drills measured the same shape: 143
    /// sessions opened and 4 answered, then 357 opened and 3 answered.
    ///
    /// What makes that class of bug survive review is that nothing fails. The field compiles, the
    /// config carries it, the responder simply says nothing and loses on the clock — which is
    /// indistinguishable from a party that is not a party. So this asserts the round trip through
    /// the FILESYSTEM rather than the happy path in memory: the producer's writer and the panel's
    /// reader must agree, byte for byte, about what a claim's file is called.
    #[test]
    fn the_writer_and_the_reader_agree_about_where_a_capture_lives() {
        use kaspa_hashes::Hash64;
        let dir = std::env::temp_dir().join(format!("palw-retention-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let claim = Hash64::from_u64_word(0xC0FFEE);
        let bytes = b"the capture behind one attempt".to_vec();

        // The producer's side of the promise, through the same function `retain_execution` uses.
        let path = crate::palw_producer::palw_retained_material_path(&dir, &claim);
        std::fs::write(&path, &bytes).unwrap();

        // And the panel's side, through the same function `retained_capture` uses. A drift in
        // either format string breaks this, which is the point: the drift's own symptom is
        // silence.
        let read = std::fs::read(crate::palw_producer::palw_retained_material_path(&dir, &claim)).unwrap();
        assert_eq!(read, bytes, "the panel must find what the producer retained");

        // And `rebroadcast_retained` walks the directory by stripping the same suffix, so a name
        // the writer produces must be one that walk recognizes.
        let name = path.file_name().unwrap().to_str().unwrap();
        let stem = name
            .strip_suffix(crate::palw_producer::PALW_RETAINED_MATERIAL_SUFFIX)
            .expect("the retention walk strips this exact suffix");
        assert_eq!(stem.parse::<Hash64>().unwrap(), claim, "and the stem must parse back to the claim it names");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
