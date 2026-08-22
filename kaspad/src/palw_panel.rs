
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

use std::collections::{HashMap, HashSet};
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
use kaspa_consensus_core::palw_bisect::{PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectTurnV1, PalwBisectVerdictV1};
use kaspa_consensus_core::palw_court_v2::{
    PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT, PalwCourtVerdictProofV2,
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
use misaka_palw_base0::backend::Base0Backend;

const PALW_PANEL: &str = "palw-panel";
/// How many receipts one claim's pool holds. A panel has 5 seats; the rest is an attacker's spam,
/// and the assembler drops garbage anyway — the cap only bounds memory.
const RECEIPTS_PER_CLAIM: usize = 16;
/// Distinct material payloads kept per claim (mirrors the gossip relay budget).
const MATERIALS_PER_CLAIM: usize = 4;
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
    fn resolve_fee_funding(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Option<(TransactionOutpoint, UtxoEntry)> {
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
        for outpoint in candidates {
            if let Some(entry) = session.get_virtual_utxo_entry(outpoint) {
                return Some((outpoint, entry));
            }
        }
        None
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

            let mut court_pending: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)> = Vec::new();
            // --- the court's half: answer the disputes this bond is a party to ---
            //
            // Nothing in this tree used to construct a `CourtDisclosed`. A challenger could open a
            // session and the accused had no software able to answer it, so every dispute ran out
            // on the clock — which is why the opening rung had to stop convicting on silence. This
            // is the missing half: both parties act from the SAME capture, through the same
            // functions, so the bisection converges on a real divergence rather than on whoever
            // stayed awake.
            let court_duties = session.palw_court_duties_v2(vec![bond_key]);
            for duty in &court_duties {
                if court_moved.contains(&(duty.session_id, duty.round, duty.i_am_responder)) {
                    continue;
                }
                // The capture, and the family's backend for it. A party with no material — or a
                // family with no court — cannot answer honestly, and answering dishonestly is what
                // the terminal close exists to punish, so it stays silent and lets the clock decide.
                let Ok(resolved) = misaka_palw_base0::classes::resolve_class_v1(
                    &self.config.court,
                    duty.class_id,
                    duty.artifact_root,
                    &self.class_artifacts,
                ) else {
                    continue;
                };
                let backend = Base0Backend::new(resolved);
                let Some(capture) = materials.get(&duty.claim_id).and_then(|pool| {
                    pool.iter().find(|b| {
                        backend.verify_material(
                            b,
                            PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root },
                        ) == PalwMaterialVerdictV1::Matches
                    })
                }) else {
                    trace!("[{PALW_PANEL}] session {} needs a move but this node holds no matching capture", duty.session_id);
                    continue;
                };
                let object = match (duty.turn, duty.i_am_responder) {
                    // Our disclosure: the state of OUR execution at the midpoint the ladder asks
                    // about. A prefix commitment, so agreeing at an index means agreeing before it.
                    (PalwBisectTurnV1::AwaitDisclosure, true) => {
                        let Some(midpoint) = duty.midpoint else { continue };
                        let Some(mid_state) = backend.bisect_prefix_state(capture, midpoint) else { continue };
                        let disclosure = PalwBisectDisclosureV1 {
                            version: PALW_BISECT_OBJECT_VERSION_V1,
                            session_id: duty.session_id,
                            round: duty.round,
                            midpoint,
                            mid_state,
                        };
                        let message = borsh::to_vec(&disclosure).expect("a disclosure is borsh-serializable");
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT) else { continue };
                        Some(PalwConsensusObjectV2::CourtDisclosed { session_id: duty.session_id, disclosure, signature })
                    }
                    // Our verdict: does the responder's disclosed state match ours at that index?
                    // `agree` means the prefix matches, so the divergence is ABOVE the midpoint.
                    (PalwBisectTurnV1::AwaitVerdict, false) => {
                        let Some(disclosed) = duty.last_disclosure else { continue };
                        let Some(ours) = backend.bisect_prefix_state(capture, disclosed.0) else { continue };
                        let verdict = PalwBisectVerdictV1 {
                            version: PALW_BISECT_OBJECT_VERSION_V1,
                            session_id: duty.session_id,
                            round: duty.round,
                            agree: ours == disclosed.1,
                        };
                        let message = borsh::to_vec(&verdict).expect("a verdict is borsh-serializable");
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT) else { continue };
                        Some(PalwConsensusObjectV2::CourtVerdictPosted { session_id: duty.session_id, verdict, signature })
                    }
                    // The terminal move. EITHER party may make it, and it is deliberately the SAME
                    // call for both: an honest executor closing its own case and a challenger
                    // closing a real fraud assemble the identical object, and
                    // `adjudicate_court_close_v2` is what decides which way it reads. A prover only
                    // one side could run would be a prover that decides the verdict.
                    (PalwBisectTurnV1::Terminal, _) => {
                        let Some(index) = duty.terminal_index else { continue };
                        let refutation = match backend.refutation_for_index(capture, index) {
                            Ok(r) => r,
                            Err(e) => {
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
                            trace!("[{PALW_PANEL}] session {} has no adjudicable close from this capture yet", duty.session_id);
                            continue;
                        };
                        info!("[{PALW_PANEL}] session {} closes as {verdict:?} on step {index}", duty.session_id);
                        Some(PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof })
                    }
                    _ => None,
                };
                let Some(object) = object else { continue };
                court_pending.push((duty.session_id, duty.round, duty.i_am_responder, object));
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
                        let Ok(resolved) = misaka_palw_base0::classes::resolve_class_v1(
                            &self.config.court,
                            duty.class_id,
                            duty.artifact_root,
                            &self.class_artifacts,
                        ) else {
                            break 'verdict None;
                        };
                        let backend = Base0Backend::new(resolved);
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
                let mut funding = self.resolve_fee_funding(&session);
                if funding.is_none() && receipts.keys().any(|c| !submitted.contains(c)) {
                    // Once per tick, not once per claim: a pending carrier keeps every quorum
                    // unfundable until it is mined, and that used to be tens of thousands of
                    // identical lines an hour.
                    warn!("[{PALW_PANEL}] a quorum stands but no fee UTXO resolves — a carrier may still be in flight; else fund --palw-fee-outpoint");
                }
                // The court's moves first: a rung has a deadline and a receipt quorum does not.
                for (session_id, round, mine_is_responder, object) in std::mem::take(&mut court_pending) {
                    let Some((funding_outpoint, funding_entry)) = funding.clone() else { break };
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
                                    court_moved.insert((session_id, round, mine_is_responder));
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
                let claims: Vec<Hash64> = receipts.keys().copied().collect();
                for claim in claims {
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
            }

            // Forget what the chain has moved past: a claim with no duty and no unsubmitted quorum
            // holds memory for nothing. (Duties vanish when a claim leaves `PanelBound`.)
            //
            // **A disputed claim is not moved past.** A court opens on a `ReceiptLicensed` claim,
            // which is a phase past `PanelBound` — so the old rule dropped the tiles exactly when a
            // dispute could start needing them, and neither party could disclose or refute. Claims
            // under an open court keep their material for as long as the session lives.
            let mut live: HashSet<Hash64> = duties.iter().map(|d| d.claim_id).collect();
            live.extend(court_duties.iter().map(|d| d.claim_id));
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

fn object_name(object: &PalwConsensusObjectV2) -> &'static str {
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { .. } => "ReceiptLicensed",
        PalwConsensusObjectV2::ProducerDefaulted { .. } => "ProducerDefaulted",
        _ => "lifecycle object",
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
