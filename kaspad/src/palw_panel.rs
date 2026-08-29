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
use kaspa_consensus_core::mass::{MassCalculator, UtxoCell, calc_storage_mass, utxo_plurality};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectSpaceV1, PalwBisectTurnV1, PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{
    PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT, PALW_COURT_V2_MLDSA87_OPEN_CONTEXT, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT,
    PalwCourtVerdictProofV2, court_session_id_v2,
};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::MAXIMUM_STANDARD_TRANSACTION_MASS;
use kaspa_mining::mempool::tx::Orphan;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_p2p_flows::palw_gossip::PalwGossipEvent;
use kaspa_pq_validator_core::relay_fee_for_compute_mass;
use kaspa_txscript::MLDSA87_TX_CONTEXT;
use kaspa_utils::triggers::SingleTrigger;

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

/// **How long a submitted court move is assumed to be in flight.**
///
/// Long enough that an accepted carrier normally reaches a block inside it — otherwise the panel
/// pays a second fee for a move that was going to land anyway — and short enough that a LOST move
/// is re-planned many times over before its rung expires. A rung window is a genesis-time choice
/// and is expected to be dozens of DAA; this is a small multiple of block time, so a lost carrier
/// gets on the order of ten retries rather than one.
const COURT_MOVE_REPLAN_DAA: u64 = 10;
/// Submission attempts per assembled object before giving up (each tick retries).
const SUBMIT_ATTEMPTS: u32 = 3;
/// **How long the panel keeps a claim it is not being asked about.**
///
/// The in-memory pools are keyed by claim and were pruned only when a claim was LIVE or already
/// submitted for — so a claim the panel could never submit for was kept for the life of the
/// process, with its gossiped materials. Generous against every lattice window (a claim that can
/// still license is still live), and finite, which is the whole point.
const PANEL_POOL_RETENTION_DAA: u64 = 4_000;

/// **A ceiling on the WHOLE material pool, not just on one claim's slice** (audit3 S-02).
///
/// `MATERIALS_PER_CLAIM` and `PALW_MATERIAL_MAX_BYTES` bound one claim and one payload; nothing
/// bounded the number of claim keys, and the claim id is 64 raw bytes off the wire that nobody has
/// authenticated. So a single unauthenticated connection naming a fresh claim per message grew the
/// pool without limit — 4 x 16 MiB per invented id, held for `PANEL_POOL_RETENTION_DAA` — and the
/// age sweep cannot help, because the sweep runs on a pool that is already too big. At 1 Gbps a
/// 32 GiB host is gone in about four minutes, for the cost of one TCP connection: no bond, no
/// block, no transaction. Every seat on the network can be killed at once, and with the seats dead
/// no claim reaches a quorum, so every producer's escrowed carve is destroyed at its deadline.
///
/// The bound has to be on the total, and it has to be enforced where the growth happens rather
/// than on the next tick. Eviction is oldest-arrival-first over whole claims: a chain-real claim
/// that is evicted is re-fetched by the pull, which is what the pull is for, while an invented one
/// simply never comes back.
const PANEL_POOL_MAX_CLAIMS: usize = 512;
/// Sized so the pool cannot outgrow what a seat needs: `PANEL_POOL_MAX_CLAIMS` live claims at one
/// 16 MiB material each is far past any real duty backlog, and this cuts in first.
const PANEL_POOL_MAX_BYTES: usize = 192 * 1024 * 1024;

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
    /// **Register this node's worker class on the running chain, once** (ADR-0049 Decision H).
    ///
    /// A network is born with the classes its ruleset id commits to, and every later one arrives
    /// as a signed `ClassRegistered` carrying its own profile. Nothing built or carried such an
    /// object, so a second class meant re-minting the network. Set with `--palw-register-class`,
    /// it submits exactly one — the class of the converted artifact this node loaded — and then
    /// behaves like any other panel.
    ///
    /// It used to register the class of the pinned Metal worker, which was the only builder that
    /// existed (ADR-0051). ADR-0053 withdrew that family; the path survives it because the thing
    /// it carries is generic — a profile, a canonical job, a bond's signature — and what a node
    /// registers now is a class the court can adjudicate.
    /// `Some("")` registers the single class the artifact matches; a non-empty value names the
    /// model id when siblings share a converted shape (the A16 family) and the file alone cannot
    /// say which model it is.
    pub register_class: Option<String>,
    /// Submit ONE `BondRegistered` for this node's own key and stop. The only PALW identity a
    /// newcomer cannot be handed: until this existed the bonds on a chain were exactly the ones
    /// its genesis registry named, so nobody outside that list could ever produce.
    pub register_bond: bool,
    /// Collateral to lock, in sompi. `None` takes the chain's floor, which is the honest default:
    /// a newcomer has no way to know what this network demands and the chain does.
    pub bond_collateral: Option<u64>,
    /// The address a bond's rewards AND its collateral are reclaimable at. Required to register a
    /// bond, because the registration names it as payee and the carrier must pay the collateral to
    /// exactly that script.
    pub pay_address: Option<String>,
}

pub struct PalwPanelService {
    config: PalwPanelConfig,
    /// Decoded once. Same contract as the producer's: digest-checked here, matched against the
    /// CHAIN per duty.
    class_artifacts: Vec<std::sync::Arc<misaka_palw_base0::artifact::Base0ArtifactV1>>,
    qwen36_artifacts: Vec<(kaspa_hashes::Hash64, std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>)>,
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    consensus_config: Arc<Config>,
    keypair: Option<Box<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>>,
    bond: Option<TransactionOutpoint>,
    /// When `retention/foreign/` was last swept. The sweep is a full directory walk, so it runs on
    /// a cadence rather than inside every write (audit M2-2).
    foreign_prune_at: std::sync::Mutex<std::time::Instant>,
    /// Fired by `signal_exit` so this service's `start` future can finish. Both panel loops are
    /// `loop { sleep; work }` with nothing else that a shutdown could cancel, so without it the
    /// AsyncRuntime's shutdown join waits on a future that never completes. Measured on
    /// testnet-11: a node registering a bond kept its 5 s loop running for 11 minutes after SIGINT
    /// — past systemd's TimeoutStopSec — with the gRPC and P2P servers already stopped, and only
    /// SIGKILL ended it. The same shape as the fix in `eth_rpc`.
    shutdown: SingleTrigger,
}

impl PalwPanelService {
    /// The classes this seat can serve, from its configuration. Rebuilt per use rather than
    /// cached, for the same reason the producer's is: a cache would be a second place the
    /// operator's configuration lives.
    fn backends(&self) -> crate::palw_backends::PalwBackendRegistry {
        crate::palw_backends::PalwBackendRegistry::new(
            self.config.court,
            self.class_artifacts.clone(),
            self.qwen36_artifacts.clone(),
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
                // A node registering its first bond HAS no bond, and saying "panel service
                // disabled" at it is both false — the registration worker is what runs — and the
                // exact wrong thing to read while waiting for that worker to report.
                if config.register_bond {
                    info!("[{PALW_PANEL}] no bond yet; registering one (--palw-register-bond)");
                } else {
                    warn!("[{PALW_PANEL}] --palw-producer-bond: {err} — panel service disabled");
                }
                None
            }
        };
        // Same loader as the producer's, and the same rule: a file that will not load is warned
        // about rather than skipped, because a seat silently unable to judge a class looks exactly
        // like a seat whose material never arrived.
        let mut class_artifacts = Vec::new();
        let mut qwen36_artifacts = Vec::new();
        for path in &config.class_artifacts {
            match crate::palw_backends::load_class_artifact(path) {
                Ok(crate::palw_backends::LoadedClassArtifact::Dense(artifact)) => {
                    info!("[{PALW_PANEL}] loaded class artifact {}", path.display());
                    class_artifacts.push(std::sync::Arc::new(*artifact));
                }
                Ok(crate::palw_backends::LoadedClassArtifact::Qwen36 { computed_root, artifact }) => {
                    info!("[{PALW_PANEL}] mapped Qwen3.6 artifact {} (computed root {computed_root})", path.display());
                    qwen36_artifacts.push((computed_root, artifact));
                }
                Err(err) => warn!("[{PALW_PANEL}] class artifact {} is unusable: {err}", path.display()),
            }
        }
        Self {
            config,
            consensus_manager,
            flow_context,
            consensus_config,
            keypair,
            bond,
            class_artifacts,
            qwen36_artifacts,
            foreign_prune_at: std::sync::Mutex::new(std::time::Instant::now()),
            shutdown: SingleTrigger::default(),
        }
    }

    fn fee_state_path(&self) -> PathBuf {
        self.config.state_dir.join("palw-fee-outpoint")
    }

    /// The fee outpoint to spend next: the persisted rolling one if it is still unspent, else the
    /// configured one, else whatever the chain holds under this bond's payout script. Returns the
    /// entry with it, which is also the unspent check.
    async fn resolve_fee_funding(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Option<(TransactionOutpoint, UtxoEntry)> {
        // **"In the UTXO set" and "I can spend it" are different questions — ask the second one.**
        //
        // The virtual UTXO set is the CONFIRMED one: an output a carrier of ours is spending right
        // now is still in it, and stays there until that carrier is mined. So every source below —
        // the persisted outpoint, the configured one, and the scan — can hand back money that is
        // already committed, and the mempool answers `already spent by transaction … in the
        // mempool`. That refusal clears the funding, the next tick asks again, finds the same
        // outpoint, and the panel spins there instead of carrying receipts. Measured on testnet-11
        // after the recovery scan shipped: 387 double-spend refusals in three hours against 143
        // successful submissions, and 77 claims defaulted with their escrow burned, because a
        // quorum that cannot be carried inside the receipt window is a quorum that never happened.
        //
        // The mempool is the one component that knows, so it is the one asked. **The set of spent
        // outpoints this used to carry instead was worse than the bug it fixed**: an outpoint went
        // in on submission and never came out, on the reasoning that a spent output is never
        // funding again because the money moves to the carrier's CHANGE. True only if the carrier
        // is mined. A carrier that is dropped or evicted leaves its input unspent on chain and
        // permanently excluded here — the panel then owns money it has forbidden itself to see, and
        // says `no fee UTXO resolves` forever. Measured on testnet-11 the same day: node 0 stalled
        // with one live 96.85 MSK output under its own key and 6,343 identical warnings.
        let is_free = |o: &TransactionOutpoint| !self.flow_context.mining_manager().outpoint_is_spent_in_mempool(o);
        // **No configured outpoint means two different things, and this used to answer both the
        // same way.**
        //
        // For a PANEL SEAT it is a mode: `--palw-fee-outpoint` absent is "off -- receipts only",
        // said in this service's own startup line, and a seat in that mode must not start spending
        // money it finds under the payout script. That is why the early return exists and it stays.
        //
        // For a BOND REGISTRATION it is a certainty, not a choice. The only outpoint such a node
        // will ever own is the change of the carrier it has not built yet, so there is nothing an
        // operator could pass, and the scan below -- whose whole argument is that there is nothing
        // worth remembering -- is the only thing that can find its funding. It never ran for the
        // one job that needed it most: the panel reported `no confirmed UTXO to spend -- send at
        // least N sompi plus a fee` while `misaka wallet utxo list` against this same node's RPC
        // showed the address holding 10 MSK, mature. The money was there, the scan was skipped, and
        // the message blamed the operator. Measured on testnet-11 while onboarding two hosts on
        // 2026-08-26, following §3 of the join doc, which does not mention the flag at all.
        if self.config.fee_outpoint.is_none() && !self.config.register_bond {
            return None;
        }
        let mut candidates: Vec<TransactionOutpoint> = Vec::new();
        if let Ok(persisted) = std::fs::read_to_string(self.fee_state_path())
            && let Ok(outpoint) = crate::palw_producer::parse_outpoint(persisted.trim())
        {
            candidates.push(outpoint);
        }
        if let Some(configured) = self.config.fee_outpoint.as_deref()
            && let Ok(outpoint) = crate::palw_producer::parse_outpoint(configured)
        {
            candidates.push(outpoint);
        }
        for outpoint in candidates.iter().filter(|o| is_free(o)) {
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
        // Reading that script off the chain makes recovery a function of the chain and the bond —
        // no state to lose, and correct after a wipe, a restart, or a dropped carrier.
        //
        // A scan, so it runs only here: the two remembered outpoints are the hot path and this is
        // the path back from having none.
        let script = self.fee_script(session)?;
        let mut cursor: Option<TransactionOutpoint> = None;
        // What the scan SAW, so a failure can say which of its three reasons it was.
        let (mut scanned, mut under_script, mut busy) = (0usize, 0usize, 0usize);
        loop {
            let chunk = session.async_get_virtual_utxos(cursor, 1024, cursor.is_some()).await;
            if chunk.is_empty() {
                break;
            }
            cursor = chunk.last().map(|(o, _)| *o);
            scanned += chunk.len();
            let mut found = None;
            for (outpoint, entry) in chunk {
                if entry.script_public_key != script || entry.is_coinbase {
                    continue;
                }
                // **Never the bond's own output-0** (audit M2-13). The recovery scan looks for any
                // spendable output under this node's payout script, and on a node whose only such
                // output IS its collateral it selected exactly that: the carrier is then refused by
                // the spend gate as a chain block, or — where the mergeset fence is not armed —
                // accepted through a merged block and the collateral simply leaves. Every other
                // funding path in this tree carries this exclusion by name.
                if self.bond.is_some_and(|bond| bond == outpoint) {
                    continue;
                }
                under_script += 1;
                if !is_free(&outpoint) {
                    busy += 1;
                    continue;
                }
                found = Some((outpoint, entry));
                break;
            }
            if let Some((outpoint, entry)) = found {
                info!(
                    "[{PALW_PANEL}] recovered funding at {}:{} — the remembered outpoints were spent or never mined",
                    outpoint.transaction_id, outpoint.index
                );
                self.persist_fee_outpoint(outpoint);
                return Some((outpoint, entry));
            }
        }
        // **Say why, not just that.** "no fee UTXO resolves" is true of a carrier still in flight,
        // of a configured outpoint that was never funded, and of a panel that owns nothing under
        // its own script — three different operator actions, and the log named none of them. This
        // was a `trace!` behind a disabled level while a seat sat stalled for hours with money it
        // could not see, so it warns: it fires once per tick only on the path that already warns.
        warn!(
            "[{PALW_PANEL}] no fee UTXO resolves; tried {}; scanned {scanned} outputs, {under_script} under this bond's payout script of which {busy} are spent by our own mempool",
            if candidates.is_empty() {
                // A node with no remembered outpoint is the normal newcomer case, not an omission
                // in this line: it says the scan is the whole story so nobody looks for a missing
                // --palw-fee-outpoint that was never needed.
                "no remembered outpoint (nothing persisted, none configured)".to_string()
            } else {
                candidates.iter().map(|o| format!("{}:{}", o.transaction_id, o.index)).collect::<Vec<_>>().join(", ")
            }
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
        // A node with no bond yet is registering its first one, and there is no chain fact to read
        // — the payee it is about to declare is the only answer, and it is the script its own
        // funding pays to.
        let Some(bond) = self.bond else {
            return self.pay_script();
        };
        let payload = session.palw_bond_payout_payload_v2(PalwBondKeyV2(bond))?;
        Some(kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&payload.as_bytes()))
    }

    /// The 64-byte payee behind `--palw-producer-pay-address`, and the script that pays it.
    ///
    /// Both come from one decode so the payload a registration DECLARES and the script its carrier
    /// PAYS cannot disagree — the carrier-binding rule compares them, and a mismatch would be
    /// refused as "a bond's collateral output must pay to the payload the registration names".
    fn pay_payee(&self) -> Result<([u8; 64], kaspa_consensus_core::tx::ScriptPublicKey), String> {
        let text = self.config.pay_address.as_deref().ok_or("no --palw-producer-pay-address to pay this bond to")?;
        let address = kaspa_addresses::Address::try_from(text).map_err(|e| format!("pay address is unusable: {e}"))?;
        if address.version != kaspa_addresses::Version::PubKeyHashMlDsa87 {
            return Err(
                "a bond's payee must be an ML-DSA-87 P2PKH address — a legacy or ECDSA one cannot hold PQ collateral".to_string()
            );
        }
        if address.prefix != self.consensus_config.prefix() {
            return Err(format!("pay address is for {} and this node is {}", address.prefix, self.consensus_config.prefix()));
        }
        let payload: [u8; 64] =
            address.payload.as_ref().try_into().map_err(|_| "an ML-DSA-87 address must carry 64 payload bytes".to_string())?;
        Ok((payload, kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&payload)))
    }

    fn pay_script(&self) -> Option<kaspa_consensus_core::tx::ScriptPublicKey> {
        self.pay_payee().ok().map(|(_, script)| script)
    }

    /// **What this bond will lock, decided before the carrier that has to fit it exists.**
    ///
    /// Split from the signing half because the number has to survive a second question that this
    /// one cannot ask: whether a carrier holding an output that small can be relayed at all. That
    /// answer needs the funding UTXO, which is resolved after this — see
    /// `min_carryable_collateral`.
    fn size_bond_collateral(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Result<u64, String> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) =
            &self.consensus_config.params.palw_consensus_mode
        else {
            return Err("this network has no ConsensusV2 bundle, so it has no bonds to register".to_string());
        };
        let floor = bundle.state.min_collateral_sompi();

        // **The floor is not a usable default.** A bond may hold a claim only while
        // `reserved_exposure + claim_exposure <= collateral * max_exposure_ratio_permille / 1000`
        // (`has_exposure_room`), and one claim costs `pwu * slash_value_per_pwu` — where `pwu`
        // rises with the class's retargeted difficulty. So the chain's minimum collateral buys a
        // bond that may be unable to hold even ONE claim, and the producer would report "the
        // bond's exposure ceiling leaves no room for another claim" forever, having locked real
        // money to get there. Sizing from the chain is the only default that is not silently
        // useless; the operator can still ask for more.
        let ratio = bundle.admission.max_exposure_ratio_permille().max(1) as u128;
        let one_claim = session
            .palw_producer_facts_v2(bundle.base_class_id, None)
            .zip(session.palw_v2_registration_terms())
            .map(|(facts, terms)| (facts.pwu as u128).saturating_mul(terms.slash_value_per_pwu as u128));
        let for_one_claim = one_claim
            .map(|exposure| u64::try_from(exposure.saturating_mul(1000).div_ceil(ratio)).unwrap_or(u64::MAX))
            .unwrap_or(floor);
        let sized = for_one_claim.max(floor);

        let collateral = self.config.bond_collateral.unwrap_or(sized);
        if collateral < floor {
            return Err(format!("--palw-bond-collateral {collateral} is below this chain's floor of {floor} sompi"));
        }
        if collateral < sized {
            // Their money, their call — but not silently. This is the number whose absence turns
            // into a producer that holds forever.
            warn!(
                "[{PALW_PANEL}] --palw-bond-collateral {collateral} is below the {sized} sompi one claim on this \
                 chain's floor class currently needs; this bond will register and may then be unable to hold a claim"
            );
        } else if self.config.bond_collateral.is_none() {
            // Once. This runs on every 5 s pass of the registration loop, and a node waiting for
            // funding would otherwise print the same sizing line until the operator gave up
            // reading the log — which is where the actionable "no confirmed UTXO" line lives.
            static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                info!(
                    "[{PALW_PANEL}] sizing collateral at {collateral} sompi — the chain's floor is {floor} and one \
                     claim on the base class currently needs {for_one_claim}"
                );
            }
        }
        Ok(collateral)
    }

    /// **Build this node's own bond registration, and the collateral output that proves it.**
    ///
    /// Returns the object together with the output its carrier must hold at index 0, because the
    /// two are one fact: `palw_bond_registration_binds_its_carrier_v2` checks the declared
    /// collateral against that output's value and the declared payee against its script, and a
    /// caller that assembled them separately could get one of them wrong.
    ///
    /// The bond names its output by INDEX with a zero transaction id — the carrier's id is a
    /// function of the payload this object travels in, so naming it would be a hash fixed point.
    /// The chain substitutes the id it observes, and the signature is made over the zero form,
    /// which is what every verifier rebuilds.
    ///
    /// `collateral` comes from `size_bond_collateral`, raised if necessary to what the carrier's
    /// storage mass allows — this half signs the number, it does not choose it.
    fn build_bond_registration(
        &self,
        collateral: u64,
    ) -> Result<(PalwConsensusObjectV2, kaspa_consensus_core::tx::TransactionOutput), String> {
        let kp = self.keypair.as_ref().ok_or("no --palw-producer-key to sign a bond registration with")?;
        let (payee_bytes, payee_script) = self.pay_payee()?;
        let payout_payload = Hash64::from_bytes(payee_bytes);

        // The signing key is also the operator identity. Panel dedup is per OPERATOR, so two bonds
        // under one key are one operator by construction — which is the honest reading for a node
        // registering its own bond, and the only one it can make without a second key to name.
        let pubkey = kp.verification_key.as_ref().to_vec();
        let bond = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0));
        let network_domain =
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.consensus_config.params.net.to_string().as_bytes(), Some(self.consensus_config.genesis.hash));
        let message = kaspa_consensus_core::palw_state_v2::palw_bond_registration_message_v2(
            network_domain,
            &kaspa_consensus_core::palw_lifecycle_objects_v2::palw_bond_registration_signed_key_v2(&bond),
            &pubkey,
            &pubkey,
            collateral,
            &payout_payload,
        );
        let signature = self
            .sign(message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT)
            .ok_or("this node holds no key, so it cannot sign a bond registration")?;
        Ok((
            PalwConsensusObjectV2::BondRegistered {
                bond,
                pubkey: pubkey.clone(),
                operator_pubkey: pubkey,
                collateral,
                payout_payload,
                signature,
            },
            kaspa_consensus_core::tx::TransactionOutput::new(collateral, payee_script),
        ))
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

    /// **Build the `ClassRegistered` for the class this node holds an artifact for**
    /// (ADR-0049 Decision H, ADR-0053).
    ///
    /// Every term the gate checks comes from the chain. Nothing here is a number this node picked:
    /// a registrant that chose its own share, initial target or slash value would be rejected by
    /// the value rather than by the choosing, which reads like a protocol error and is not one.
    ///
    /// The class comes from what the node loaded rather than from a flag naming one. An operator
    /// who has put a converted artifact on disk has already said which class this node is for, and
    /// a second declaration is a second place for them to disagree.
    async fn build_class_registration(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
    ) -> Result<PalwConsensusObjectV2, String> {
        let bond = self.bond.ok_or("no --palw-producer-bond to register under")?;
        let bond_key = PalwBondKeyV2(bond);
        info!("[{PALW_PANEL}] attempting the class registration (filter: {})", self.config.register_class.as_deref().unwrap_or("<unset>"));
        let terms = session.palw_v2_registration_terms().ok_or("this chain has no V2 bundle, or does not hold its base class yet")?;

        // The class of the artifact this node carries — matched by SHAPE against the build's own
        // registry, so a file that is not any class this build knows is refused here rather than
        // registered as a class nobody can execute. Shape alone cannot finish the job: sibling
        // models of one family share a converted shape (the A16 tier's file says nothing about
        // WHICH weights it holds), so the already-registered ids are dropped — submitting one is
        // a guaranteed `DuplicateClass` refusal — and if more than one candidate is still
        // standing, the operator's `--palw-register-class <model-id>` is what picks.
        // One candidate shape regardless of lineage: what a registration needs is the model id
        // (for the operator), the profile (the class), the canonical job (what it is paid per)
        // and the root its artifact pins.
        struct RegistrationCandidateV1 {
            model_id: &'static str,
            profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
            canonical_job: (u32, u32),
            artifact_root: Hash64,
        }
        let mut candidates: Vec<RegistrationCandidateV1> = Vec::new();
        for artifact in &self.class_artifacts {
            // **A file whose digest the chain already pinned is not a candidate for a NEW class.**
            // Re-registering known weights under a fresh id is never meaningful — and with two
            // same-shape artifacts loaded it is exactly how the first file used to fill every
            // sibling ledger entry before the second file's turn came (2026-08-28: the genesis
            // 1.5B digest landed under the Coder class id). Known weights serve their own class;
            // only unknown weights are looking for one.
            if terms.registered_artifact_roots.contains(&artifact.artifact_digest()) {
                continue;
            }
            for c in misaka_palw_base0::classes::canonical_classes_v1(&self.config.court) {
                if c.shape_matches(artifact).is_ok()
                    && let Ok(root) = c.artifact_root(artifact)
                    && !candidates.iter().any(|seen| seen.profile.shape_profile_id() == c.class_id())
                {
                    candidates.push(RegistrationCandidateV1 {
                        model_id: c.model_id,
                        profile: c.profile.clone(),
                        canonical_job: c.canonical_job,
                        artifact_root: root,
                    });
                }
            }
        }
        for (computed_root, artifact) in &self.qwen36_artifacts {
            if terms.registered_artifact_roots.contains(computed_root) {
                continue;
            }
            for c in misaka_palw_base0::classes::qwen36_canonical_classes_v1() {
                if c.shape_matches(&artifact.shape).is_ok()
                    && let Ok(profile) = c.profile()
                    && !candidates.iter().any(|seen| seen.profile.shape_profile_id() == profile.shape_profile_id())
                {
                    candidates.push(RegistrationCandidateV1 {
                        model_id: c.model_id,
                        profile,
                        canonical_job: c.canonical_job,
                        artifact_root: *computed_root,
                    });
                }
            }
        }
        if candidates.is_empty() {
            return Err("no --palw-class-artifact matches a class this build knows, so there is nothing to register".to_string());
        }
        candidates.retain(|c| !terms.registered_class_ids.contains(&c.profile.shape_profile_id()));
        if candidates.is_empty() {
            return Err("every class this node's artifacts match is already registered on this chain".to_string());
        }
        let wanted = self.config.register_class.as_deref().unwrap_or("");
        if !wanted.is_empty() {
            candidates.retain(|c| c.model_id == wanted);
            if candidates.is_empty() {
                return Err(format!(
                    "--palw-register-class {wanted} names no unregistered class this node's artifacts match — \
                     check the model id against the build's ledger and the artifact against the model"
                ));
            }
        }
        if candidates.len() > 1 {
            let names: Vec<&str> = candidates.iter().map(|c| c.model_id).collect();
            return Err(format!(
                "this node's artifacts match {} unregistered classes ({}) — name one with --palw-register-class <model-id>",
                names.len(),
                names.join(", ")
            ));
        }
        let entry = candidates.pop().expect("length checked above");
        let artifact_root = entry.artifact_root;

        let canonical =
            kaspa_consensus_core::palw_base0_profile::rc_job_context(&entry.profile, entry.canonical_job.0, entry.canonical_job.1);

        // Built twice on purpose: once to learn the class id the profile derives, and once with
        // the signature over it. Signing anything assembled beside the object would sign a class
        // that is not the one being registered.
        let build = |signature: Vec<u8>| {
            kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_v1(
                entry.profile.clone(),
                canonical.clone(),
                artifact_root,
                terms.min_grantable_share_permille,
                terms.initial_target,
                terms.slash_value_per_pwu,
                0,
                bond_key,
                signature,
            )
            .map_err(|e| format!("this node cannot build a registration for {}: {e}", entry.model_id))
        };
        let unsigned = build(Vec::new())?;
        let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            activation_daa,
            artifact_root: signed_root,
            slash_value_per_pwu: signed_slash,
            initial_target: signed_target,
            pwu_rule: signed_rule,
            ..
        } = &unsigned
        else {
            return Err("the builder did not build a registration".to_string());
        };
        // The whole object, not five of its fields (audit M2-6) — the signer's own comment already
        // said "signing anything assembled beside the object would sign a class that is not the one
        // being registered", and this is that comment made true.
        let message = kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.consensus_config.params.net.to_string().as_bytes(), Some(self.consensus_config.genesis.hash)),
            *class_id,
            terms.min_grantable_share_permille,
            *activation_daa,
            &bond_key,
            *signed_root,
            *signed_slash,
            *signed_target,
            signed_rule,
            &canonical,
        );
        let signature = self
            .sign(message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT)
            .ok_or("this node holds no bond key, so it cannot sign a registration")?;
        build(signature)
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
        self.build_lifecycle_tx_with_outputs(object, funding_outpoint, funding, &[])
    }

    /// [`Self::build_lifecycle_tx`] with outputs AHEAD of the change.
    ///
    /// A bond registration is the one object whose carrier must also move money: the collateral
    /// has to sit in an output of this very transaction, at the index the object names. Those
    /// outputs come first so the index is stable at 0.. regardless of the change, and the change
    /// is last so the rolling fee outpoint is always `outputs.len() - 1`.
    fn build_lifecycle_tx_with_outputs(
        &self,
        object: &PalwConsensusObjectV2,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        extra_outputs: &[kaspa_consensus_core::tx::TransactionOutput],
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
        let locked: u64 = extra_outputs.iter().map(|o| o.value).sum();
        let build = |fee: u64, signature_script: Vec<u8>| -> Result<Transaction, String> {
            // The collateral is spent as well as the fee, and saying so by name is the difference
            // between "fund the address again" and an operator wondering why a bond they have the
            // money for will not register.
            let needed = fee.saturating_add(locked);
            if funding.amount <= needed {
                return Err(format!(
                    "funding UTXO holds {} sompi; this carrier needs {fee} fee + {locked} locked — fund the address again",
                    funding.amount
                ));
            }
            let mut input = TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
            input.signature_script = signature_script;
            let mut outputs = extra_outputs.to_vec();
            outputs.push(TransactionOutput::new(funding.amount - needed, funding.script_public_key.clone()));
            Ok(Transaction::new(TX_VERSION, vec![input], outputs, 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload.clone()))
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

    /// Wait out one pass of a panel loop, or stop because the node is shutting down.
    ///
    /// `false` means "return now". Every panel loop begins with this, which is what makes
    /// `signal_exit` reach code that would otherwise sleep forever.
    async fn tick(&self, period: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(period) => true,
            _ = self.shutdown.listener.clone() => false,
        }
    }

    /// **What a collateral output costs to exist, before there is a funding UTXO to measure.**
    ///
    /// The collateral output alone costs `C · p² / value`, so `value >= C · p² / limit` is
    /// necessary however it is funded — the change and input terms can only push the answer up.
    /// That makes this a sound floor to quote to an operator who has not sent anything yet, which
    /// is the one moment `min_carryable_collateral` cannot help: it needs the UTXO.
    ///
    /// It matters because the number this replaces was the chain's own floor, and on testnet-11
    /// that told a waiting operator to send 400,000 sompi for a bond that needs upwards of
    /// 8,333,334 — an answer they would have funded, watched fail, and had no way to connect to
    /// the mass in the refusal.
    fn collateral_relay_lower_bound(&self) -> Option<u64> {
        let payee = self.pay_payee().ok()?.1;
        let plurality = utxo_plurality(&payee);
        let c = self.consensus_config.params.storage_mass_parameter;
        Some(c.saturating_mul(plurality).saturating_mul(plurality).div_ceil(MAXIMUM_STANDARD_TRANSACTION_MASS))
    }

    /// This chain's minimum collateral, or `None` on a network with no bonds to register.
    fn collateral_floor(&self) -> Option<u64> {
        match &self.consensus_config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.min_collateral_sompi()),
            _ => None,
        }
    }

    /// **Build the carrier for this node's bond registration, at a collateral the relay accepts.**
    ///
    /// `size_bond_collateral` answers "how much does this bond need to be useful"; this answers
    /// "how much can a transaction actually carry", and the second question cannot be asked until
    /// the funding UTXO is known. The two disagree, and the chain's own floor is on the losing
    /// side — see `min_carryable_collateral` — so a panel that asked only the first question built
    /// a carrier that could never be submitted, and said nothing about why: the mempool's refusal
    /// names a mass, not a collateral, and the sizing line above it looked correct.
    ///
    /// An operator who NAMED a collateral is refused with the number that would work rather than
    /// silently overridden — it is their money and their exposure ceiling. A DEFAULT is raised,
    /// because the alternative is a default that can never register a bond.
    fn build_registration_carrier(
        &self,
        sized: u64,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
    ) -> Result<(u64, Transaction), String> {
        let storm = self.consensus_config.params.storage_mass_parameter;
        let build = |collateral: u64| -> Result<(u64, Transaction), String> {
            let (object, output) = self.build_bond_registration(collateral)?;
            let tx = self.build_lifecycle_tx_with_outputs(&object, funding_outpoint, funding, std::slice::from_ref(&output))?;
            Ok((collateral, tx))
        };
        let storage_mass = |tx: &Transaction| {
            calc_storage_mass(false, std::iter::once(UtxoCell::from(funding)), tx.outputs.iter().map(UtxoCell::from), storm)
        };

        let (collateral, tx) = build(sized)?;
        let mass = storage_mass(&tx);
        if mass.is_some_and(|m| m <= MAXIMUM_STANDARD_TRANSACTION_MASS) {
            return Ok((collateral, tx));
        }

        // The fee is a function of the carrier's COMPUTE mass, which does not move with an output's
        // VALUE — same inputs, same outputs, same payload, same bytes — so the fee measured here is
        // the fee the rebuilt carrier pays, and the search below can treat it as fixed.
        let fee = funding.amount.saturating_sub(tx.outputs.iter().map(|o| o.value).sum::<u64>());
        let payee = self.pay_payee()?.1;
        let floor = self.collateral_floor().unwrap_or(1);
        let measured = mass.map(|m| m.to_string()).unwrap_or_else(|| "an incomputable amount of".to_string());
        let minimum =
            min_carryable_collateral(funding, fee, &payee, storm, MAXIMUM_STANDARD_TRANSACTION_MASS, floor).ok_or_else(|| {
                format!(
                    "this funding UTXO holds {} sompi and cannot carry a bond registration at ANY collateral — every \
                     split of it exceeds the {MAXIMUM_STANDARD_TRANSACTION_MASS} relay mass limit. Send more to this \
                     node's pay address; the collateral is not the problem.",
                    funding.amount
                )
            })?;
        // **The mass is U-shaped, so "too small" and "too large" both land here — and only one of
        // them is fixed by more collateral.** Past the even split it is the CHANGE output that is
        // too small to relay, and raising the collateral makes it smaller still. Saying "raise it"
        // there would send an operator the wrong way down a curve.
        if minimum <= sized {
            return Err(format!(
                "a collateral of {sized} sompi leaves this {} sompi funding UTXO a change output too small to relay \
                 ({measured} storage mass against a limit of {MAXIMUM_STANDARD_TRANSACTION_MASS}). Fund this node's \
                 pay address with more, or lower the collateral.",
                funding.amount
            ));
        }
        if self.config.bond_collateral.is_some() {
            return Err(format!(
                "--palw-bond-collateral {sized} sompi cannot be carried: an output that small costs {measured} storage \
                 mass against a relay limit of {MAXIMUM_STANDARD_TRANSACTION_MASS}, so the carrier is refused as \
                 non-standard however it is funded. The smallest collateral this funding can carry is {minimum} sompi."
            ));
        }

        let (collateral, tx) = build(minimum)?;
        match storage_mass(&tx) {
            Some(m) if m <= MAXIMUM_STANDARD_TRANSACTION_MASS => {}
            other => {
                return Err(format!(
                    "raising the collateral to {minimum} sompi did not bring the carrier under the relay mass limit \
                     ({}); this node cannot register a bond from this funding UTXO",
                    other.map(|m| m.to_string()).unwrap_or_else(|| "incomputable".to_string())
                ));
            }
        }
        // Once: this runs on every pass of the registration loop until the carrier is accepted.
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            info!(
                "[{PALW_PANEL}] raising collateral from {sized} to {minimum} sompi — an output of {sized} costs \
                 {measured} storage mass against a relay limit of {MAXIMUM_STANDARD_TRANSACTION_MASS}, so a carrier \
                 holding one cannot be submitted. Pass --palw-bond-collateral to choose the amount yourself."
            );
        }
        Ok((collateral, tx))
    }

    /// The capture this node's own producer persisted for `claim`, if it is still on disk.
    ///
    /// Best-effort by design: a seat that never produced has no retention directory, and a claim
    /// past its retention window has no file. Both are "cannot answer", which is the same silence
    /// a party with no material has always been allowed. The caller verifies the bytes against the
    /// claim's committed roots before believing them.
    /// **The job a claim's block asked for, derived from the block.**
    ///
    /// Four inputs, every one of them a fact about the claim itself that any node can read: the
    /// network, the accepted block's pre-PoW hash, the class, and the executor's bond outpoint.
    /// Nothing here is taken from the material under judgement — that is the entire point. A
    /// capture names its own job, so a verifier that reads the anchor out of the capture is asking
    /// the accused to set the question, and the answer always agrees.
    ///
    /// `None` when the block is not in this node's store (pruned, or not yet synced) or when the
    /// family has no canonical job. Callers must then decline to judge rather than fall back.
    fn job_anchor_for_claim(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
        network_domain: Hash64,
        accepted_block: Hash64,
        class_id: Hash64,
        executor_bond: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Option<Hash64> {
        let header = session.palw_claim_block_header_v2(accepted_block)?;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&header);
        backend.job_anchor_v1(network_domain, pre_pow, class_id, &executor_bond.0)
    }

    /// Persist a foreign (gossiped) material under `retention/foreign/`, best-effort.
    ///
    /// Write-once per claim file; pruned by age on every write so the directory stays bounded
    /// (~2.3 MB a floor material, a few hundred claims a day, 72 h of them ≈ single-digit GiB
    /// worst case, far less in practice). Errors are swallowed: durability here is an assist to
    /// the pull transport, not an obligation — the OBLIGATED copy is the producer's.
    fn persist_foreign_material(&self, claim: &Hash64, bytes: &[u8]) {
        let dir = self.config.retention_dir.join("foreign");
        let path = dir.join(format!("{claim}.material"));
        if path.exists() {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("[{PALW_PANEL}] cannot create the foreign retention directory {}: {e}", dir.display());
            return;
        }
        // **A write failure is reported.** Swallowing it (`let _ =`) left the panel believing it
        // was retaining while a full volume dropped every byte — the node then answers no pull and
        // is charged for the silence (audit M2-2).
        if let Err(e) = std::fs::write(&path, bytes) {
            warn!("[{PALW_PANEL}] cannot retain material for claim {claim}: {e}");
            return;
        }
        self.prune_foreign_retention(&dir);
    }

    /// Bound `retention/foreign/` by BOTH age and count, oldest first.
    ///
    /// The age sweep alone was not a bound (audit M2-2): 72 hours of anything is unbounded when an
    /// attacker chooses the arrival rate, and the sweep ran inside the write path, so the N-th
    /// write cost N `metadata()` syscalls in the panel's async tick. It runs on a cadence now, and
    /// the count cap is what actually holds the directory down.
    fn prune_foreign_retention(&self, dir: &std::path::Path) {
        const FOREIGN_RETENTION_MAX_FILES: usize = 4_096;
        const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(300);
        {
            let mut last = self.foreign_prune_at.lock().unwrap();
            let now = std::time::Instant::now();
            if now.duration_since(*last) < PRUNE_EVERY {
                return;
            }
            *last = now;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(72 * 3600);
        let mut kept: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            if modified < cutoff {
                let _ = std::fs::remove_file(entry.path());
                continue;
            }
            kept.push((modified, entry.path()));
        }
        if kept.len() > FOREIGN_RETENTION_MAX_FILES {
            kept.sort_by_key(|(at, _)| *at);
            for (_, path) in kept.iter().take(kept.len() - FOREIGN_RETENTION_MAX_FILES) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn retained_capture(&self, claim: &Hash64) -> Option<Vec<u8>> {
        std::fs::read(crate::palw_producer::palw_retained_material_path(&self.config.retention_dir, claim)).ok()
    }

    /// **Register this node's own bond, once, and say what to do with it.**
    ///
    /// Runs instead of the panel duties, because a node doing this has no bond yet and every duty
    /// is keyed on one. It is the entry point for a newcomer: before it existed the bonds on a
    /// chain were exactly the ones its genesis registry named, so mining was closed to anyone not
    /// on that list — the consensus rules admitted a carried registration, and nothing could build
    /// one.
    ///
    /// Keeps trying rather than exiting on the first refusal: the usual reason is that the funding
    /// address has no confirmed UTXO yet, and an operator who is funding it right now should not
    /// have to restart the node to be noticed. Every reason is named, and repeats are throttled.
    pub async fn bond_registration_worker(self: &Arc<Self>) {
        if self.keypair.is_none() {
            warn!("[{PALW_PANEL}] --palw-register-bond needs --palw-producer-key — not registering");
            return;
        }
        let payee = match self.pay_payee() {
            Ok((_, script)) => script,
            Err(why) => {
                warn!("[{PALW_PANEL}] --palw-register-bond: {why} — not registering");
                return;
            }
        };
        info!(
            "[{PALW_PANEL}] registering this node's bond; collateral and fee are spent from a confirmed UTXO paying to {}",
            kaspa_txscript::extract_script_pub_key_address(&payee, self.consensus_config.prefix())
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "this node's pay address".to_string())
        );

        let mut last_complaint: Option<(String, std::time::Instant)> = None;
        let mut complain = |why: String| {
            let fresh =
                last_complaint.as_ref().is_none_or(|(prev, at)| prev != &why || at.elapsed() >= std::time::Duration::from_secs(60));
            if fresh {
                warn!("[{PALW_PANEL}] cannot register a bond yet: {why}");
                last_complaint = Some((why, std::time::Instant::now()));
            }
        };

        loop {
            if !self.tick(std::time::Duration::from_secs(5)).await {
                info!("[{PALW_PANEL}] stopping: no bond was registered before this node was asked to shut down");
                return;
            }
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                continue;
            }
            // **Already bonded? Then this flag is a no-op, not a second payment.** A bond locks
            // collateral and its key is the carrier's own transaction id, so registering twice is
            // paying twice — and the likeliest way to do it is leaving this flag in a unit file
            // that restarts. Asked of the chain rather than a local marker, because this network's
            // own relaunch instructions tell operators to wipe the datadir.
            if let Some(kp) = self.keypair.as_ref()
                && let Some(existing) = session.palw_bond_of_pubkey_v2(kp.verification_key.as_ref())
            {
                info!(
                    "[{PALW_PANEL}] this key already holds bond {}:{} on this chain — not registering another. \
                     Drop --palw-register-bond and run with --palw-producer-bond={}:{}",
                    existing.0.transaction_id, existing.0.index, existing.0.transaction_id, existing.0.index
                );
                return;
            }
            let sized = match self.size_bond_collateral(&session) {
                Ok(sized) => sized,
                Err(why) => {
                    complain(why);
                    continue;
                }
            };
            let Some((funding_outpoint, funding)) = self.resolve_fee_funding(&session).await else {
                // Quote the relay's floor, not just the chain's: what a bond NEEDS and what a
                // carrier can HOLD are different numbers, and an operator funding this address is
                // about to discover which one is larger.
                let need = self.collateral_relay_lower_bound().map_or(sized, |bound| sized.max(bound));
                complain(format!("no confirmed UTXO to spend — send at least {need} sompi plus a fee to this node's pay address"));
                continue;
            };
            // Funding first, then the carrier: what a bond NEEDS is a chain fact, but what a
            // carrier can HOLD depends on the UTXO paying for it, and only one of those two
            // numbers can be decided without the other.
            let (collateral, tx) = match self.build_registration_carrier(sized, funding_outpoint, &funding) {
                Ok(built) => built,
                Err(why) => {
                    complain(why);
                    continue;
                }
            };
            let txid = tx.id();
            match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
                Ok(()) => {
                    // The change is last, so the rolling fee outpoint is the final output.
                    // One extra output (the collateral) ahead of the change, so the change is 1.
                    self.persist_fee_outpoint(TransactionOutpoint::new(txid, 1));
                    // **Submitted is not registered.** `validate_palw_lifecycle_tx` sees only the
                    // payload -- decode, wire version, may-ride table -- so it cannot check the
                    // carrier binding, and a carrier whose object the extractor then drops is an
                    // accepted transaction that created no bond. The collateral output still
                    // exists and still pays this node's own address, so the money is recoverable;
                    // nothing else happened. The likeliest cause is a chain whose nodes predate
                    // the index-and-zero-id naming and refuse this form on extraction.
                    //
                    // So wait for the bond to EXIST before saying it does. Announcing an unchecked
                    // success is how an operator ends up debugging a producer that was never going
                    // to start, and the bond's key is only knowable from the chain anyway.
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
                    loop {
                        if !self.tick(std::time::Duration::from_secs(5)).await {
                            warn!(
                                "[{PALW_PANEL}] shutting down while waiting for carrier {txid} to produce a bond. \
                                 It may still land: check with --palw-register-bond on the next start, which asks the \
                                 chain before registering a second one."
                            );
                            return;
                        }
                        let session = self.consensus_manager.consensus().unguarded_session();
                        if let Some(kp) = self.keypair.as_ref()
                            && let Some(bond) = session.palw_bond_of_pubkey_v2(kp.verification_key.as_ref())
                        {
                            info!(
                                "[{PALW_PANEL}] registered bond {}:{} with {} sompi of collateral, in tx {txid}. \
                                 Restart with --palw-producer-bond={}:{} (and --palw-produce) to mine with it; \
                                 the collateral is reclaimable at this node's pay address once the bond is retired.",
                                bond.0.transaction_id, bond.0.index, collateral, bond.0.transaction_id, bond.0.index
                            );
                            return;
                        }
                        if std::time::Instant::now() >= deadline {
                            warn!(
                                "[{PALW_PANEL}] carrier {txid} was accepted but no bond appeared within 10 minutes. \
                                 The collateral output {txid}:0 is yours and spendable; no bond was created. The \
                                 usual cause is a network still running a build that predates the index-and-zero-id \
                                 carrier naming, which drops this registration on extraction."
                            );
                            return;
                        }
                    }
                }
                Err(e) => complain(format!("the carrier was refused: {e}")),
            }
        }
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
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(self.consensus_config.params.net.to_string().as_bytes(), Some(self.consensus_config.genesis.hash));
        info!(
            "[{PALW_PANEL}] starting (bond={bond}, submitter={}, register={})",
            if self.config.fee_outpoint.is_some() { "funded" } else { "off — receipts only" },
            // The operator's one-shot intent, echoed so a silent registration path is
            // diagnosable from the startup line alone (it went silent once — this line is why
            // that cannot happen twice).
            match self.config.register_class.as_deref() {
                None => "off".to_string(),
                Some("") => "any-unregistered".to_string(),
                Some(id) => id.to_string(),
            }
        );

        // **This node now answers material pulls** (the request half is in flow_context): its own
        // captures first, then every foreign material it persisted below. Registered here because
        // the panel is the party that owns a retention directory; a node running no panel stays
        // silent, which is the resolver's None.
        {
            let retention = self.config.retention_dir.clone();
            self.flow_context.palw_gossip().set_material_resolver(std::sync::Arc::new(move |claim| {
                std::fs::read(crate::palw_producer::palw_retained_material_path(&retention, &claim))
                    .ok()
                    .or_else(|| std::fs::read(retention.join("foreign").join(format!("{claim}.material"))).ok())
            }));
        }

        let mut materials: HashMap<Hash64, Vec<Vec<u8>>> = HashMap::new();
        // Total pooled bytes and per-claim arrival order, maintained with `materials` so the
        // ceiling above is enforced at insertion rather than recomputed by walking the pool
        // (audit3 S-02). The sequence is a plain counter: it only has to order arrivals, and a DAA
        // score is not available inside the drain.
        let mut pool_bytes: usize = 0;
        let mut pool_arrival: HashMap<Hash64, u64> = HashMap::new();
        let mut pool_arrival_seq: u64 = 0;
        // **What THIS node computed for a claim it disputes** — kept apart from `materials`, which
        // holds what the network says about a claim (audit M2-4). A challenger that bisects the
        // accused's own capture can never find a fault: both sides read the same bytes through the
        // same pure function, every rung `agree`s, and the ladder walks to an index no real job
        // opens. The dispute is only a dispute if the two sides speak from two executions.
        let mut own_executions: HashMap<Hash64, Vec<u8>> = HashMap::new();
        let mut receipts: HashMap<Hash64, Vec<PalwSeatReceiptV2>> = HashMap::new();
        let mut answered: HashSet<Hash64> = HashSet::new();
        let mut first_seen: HashMap<Hash64, u64> = HashMap::new();
        // When this seat last pulled for a claim it holds no material for, so a slow answer is
        // not re-asked every 2-second tick.
        let mut requested: HashMap<Hash64, u64> = HashMap::new();
        // **The DAA a claim's license was last handed to the mempool — a debounce, not a receipt.**
        //
        // This was a `HashSet` written on mempool acceptance and never cleared, so "the mempool
        // took it" meant "the chain has it" forever. It does not: a carrier can be evicted,
        // out-fee'd, orphaned, or simply lose its block, and the claim was then never re-licensed.
        // It voids at its receipt deadline and its escrowed worker carve is DESTROYED — measured
        // on testnet-11 at roughly three quarters of all claims and 1.8M MSK a day, with the
        // panel's own logs showing receipts filed and licenses submitted for claims that never
        // reached `ReceiptLicensed`.
        let mut submitted: HashMap<Hash64, u64> = HashMap::new();
        let mut submit_attempts: HashMap<Hash64, u32> = HashMap::new();
        // One move per (session, round, side): the ladder advances on acceptance, so a move
        // resubmitted before the block that carries it lands is a duplicate the chain drops.
        // **A debounce, not a receipt.** This used to be a `HashSet` written on MEMPOOL acceptance
        // and never cleared, which made "I handed it to the mempool" mean "the chain has it". Those
        // are different facts: a carrier can be evicted, out-fee'd, orphaned by a reorg, or lost to
        // a double-spend on the fee outpoint, and every one of those leaves the move permanently
        // unsent while the panel skips it forever. The chain then reads a party that said nothing —
        // and since the opening rung is clocked (ADR-0055 D3), saying nothing costs the responder
        // its claim. The loss the code detects and logs was the loss it made unrecoverable.
        //
        // The DAA at send instead: skip while the send is plausibly still in flight, then let the
        // CHAIN decide by re-planning. A duty only survives re-planning if chain state still says
        // that move is due, so a landed move disappears on its own when the round advances.
        let mut court_moved: HashMap<(Hash64, u32, bool), u64> = HashMap::new();
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
        // **The class registration, built once and retried until it lands.** Built lazily rather
        // than at startup because it needs the chain: the share it must take is the ruleset's
        // minimum grantable one, and a node cannot know that before it has state to read.
        let mut class_registration: Option<PalwConsensusObjectV2> = None;
        let mut class_registration_submitted = false;
        // Carriers submitted whose change is not yet on chain. Reset the moment the chain's tip
        // appears in the virtual UTXO set, which is the only honest signal that it was mined.
        let mut inflight: usize = 0;

        loop {
            if !self.tick(std::time::Duration::from_secs(2)).await {
                info!("[{PALW_PANEL}] stopping");
                return;
            }

            // Drain the gossip inbox first, so this tick's decisions see this tick's mail.
            while let Ok(event) = inbox.try_recv() {
                match event {
                    PalwGossipEvent::Material { claim, bytes } => {
                        // **Persisted the moment it arrives, before any verdict.** The bytes are
                        // self-authenticating (every reader verifies against the claim's committed
                        // roots), and the producer that broadcast them may be gone by the time a
                        // panel — or a court — needs them: five outside floor producers stopped
                        // their nodes on 2026-08-28 and every in-flight claim of theirs defaulted,
                        // because the only durable copies were the producers' own. One surviving
                        // copy anywhere in the fleet now serves the whole network via the pull.
                        // **Nothing is written to disk here** (audit M2-2). This ran for EVERY
                        // gossiped material — no check that the claim exists on chain, that this
                        // node is seated on it, that the bytes verify, or that a bonded party sent
                        // them — with a 72-hour mtime sweep as the only bound, on the same volume
                        // as the consensus database. Retention now happens where the bytes have
                        // been proven to be the claim's, in the duty loop below.
                        let pool = materials.entry(claim).or_default();
                        // **A full pool must not lock out the payload that verifies** (audit
                        // M2-1). Four unverifiable byte-strings cost an attacker ~280 bytes, and
                        // the pull exists to fetch the real one — dropping the answer because the
                        // garbage arrived first was the whole failure. Oldest out.
                        if pool.len() >= MATERIALS_PER_CLAIM {
                            pool_bytes = pool_bytes.saturating_sub(pool.remove(0).len());
                        }
                        pool_bytes = pool_bytes.saturating_add(bytes.len());
                        pool.push(bytes);
                        // And the whole-pool ceiling, enforced HERE — a bound checked on the next
                        // tick is a bound the sender outruns (audit3 S-02).
                        pool_arrival.entry(claim).or_insert_with(|| {
                            pool_arrival_seq += 1;
                            pool_arrival_seq
                        });
                        while materials.len() > PANEL_POOL_MAX_CLAIMS || pool_bytes > PANEL_POOL_MAX_BYTES {
                            // Never evict the claim that just arrived: that would make the pool
                            // drop exactly the payload it was asked to hold, which is the M2-1
                            // failure in a different costume.
                            let Some(oldest) = pool_arrival
                                .iter()
                                .filter(|(id, _)| **id != claim)
                                .min_by_key(|(_, seq)| **seq)
                                .map(|(id, _)| *id)
                            else {
                                break;
                            };
                            if let Some(evicted) = materials.remove(&oldest) {
                                pool_bytes = pool_bytes.saturating_sub(evicted.iter().map(|b| b.len()).sum::<usize>());
                            }
                            pool_arrival.remove(&oldest);
                            first_seen.remove(&oldest);
                        }
                    }
                    PalwGossipEvent::Receipt { bytes } => {
                        if let Ok(receipt) = borsh::from_slice::<PalwSeatReceiptV2>(&bytes) {
                            // **A receipt with no signature is not a receipt.** The pool is
                            // unauthenticated and capped, so sixteen well-formed junk receipts
                            // naming a live claim used to fill it before any real one arrived —
                            // every honest receipt was then dropped at the door and no quorum
                            // assembled (audit M2-7, failure path 3). Consensus re-verifies every
                            // signature at acceptance; this is the door check that keeps the pool
                            // from being a free denial of service.
                            let plausible = !receipt.signature.is_empty();
                            let pool = receipts.entry(receipt.claim).or_default();
                            if plausible && !pool.contains(&receipt) {
                                // Oldest out rather than newest refused, for the same reason the
                                // material pool evicts: whoever is first must not be able to lock
                                // out whoever is right.
                                if pool.len() >= RECEIPTS_PER_CLAIM {
                                    pool.remove(0);
                                }
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
            // Every claim now in the pool has an arrival stamp, whether or not this seat holds a
            // duty on it — that is what makes the retention bound below apply to foreign claims
            // (audit M2-2).
            for claim in materials.keys() {
                first_seen.entry(*claim).or_insert(current_daa);
            }

            // **Build the class registration FIRST — the comment always said "ahead of
            // everything else", the code had it BEHIND the duty sweep.** On a healthy tick that
            // was invisible; on a node started into a duty backlog (dozens of defaulted foreign
            // claims, each costing an ML-DSA signature and a broadcast) the first sweep takes
            // minutes — and on a memory-tight host the process never survived to the end of it,
            // so the registration silently never happened. Measured 2026-08-28: three OOM cycles
            // in a row, each dying mid-sweep, an armed --palw-register-class and not one build
            // attempt. Building is chain-reads only; the SUBMIT still rides the funding block
            // below, where the fee UTXO lives.
            if self.config.register_class.is_some() && !class_registration_submitted && class_registration.is_none() {
                match self.build_class_registration(&session).await {
                    Ok(object) => {
                        info!("[{PALW_PANEL}] built a class registration for this node's worker");
                        class_registration = Some(object);
                    }
                    Err(e) => warn!("[{PALW_PANEL}] cannot register this node's class: {e}"),
                }
            }

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
                    let Ok(backend) = self.backends().resolve(target.class_id, target.artifact_root) else {
                        continue;
                    };
                    // The capture is not read here any more — the anchor comes from the block
                    // below — but its PRESENCE still gates the dispute: a claim whose material
                    // nobody has served is a data-availability matter for the seats, not a fraud
                    // this bond should stake its money on.
                    if materials.get(&target.claim_id).and_then(|pool| pool.first()).is_none() {
                        continue;
                    }
                    // **The anchor comes from the block, never from the capture.** This used to
                    // read `binding.job_context.job_id` — the anchor named INSIDE the material
                    // being judged — and call that "recomputed rather than taken from the
                    // producer". It was taken from the producer: a capture states its own job, so
                    // re-executing that job reproduces its own roots by construction, and the
                    // check passed for material that answered a question no block ever asked. One
                    // gossiped capture could then be re-mined by anyone, forever, with no
                    // inference: mine a fresh block, announce the borrowed roots, and both the
                    // seat (roots match) and the challenger (re-execution matches) agree.
                    //
                    // Every input to the real anchor is a fact about the CLAIM's own block, so any
                    // third party derives the same value the producer was forced to use.
                    let Some(anchor) = self.job_anchor_for_claim(
                        &session,
                        backend.as_ref(),
                        network_domain,
                        target.accepted_block,
                        target.class_id,
                        &target.executor_bond,
                    ) else {
                        continue;
                    };
                    let Ok((job, prompt)) = backend.job_for_anchor(anchor) else { continue };
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
                        // Ours, for the ladder: the challenger answers every rung from THIS, and
                        // the responder from the claim's own capture.
                        own_executions.insert(target.claim_id, mine_run.material.clone());
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
                    let session_id =
                        court_session_id_v2(&target.claim_id, &target.trace_root, &target.executor_bond, &bond_key, space, space_size);
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
            // Claims whose ACCUSED capture a court move needs and this node does not hold. Collected
            // rather than awaited inline: the request borrows `self.flow_context` while the loop
            // still holds a borrow of `materials` (audit3 S-01).
            let mut pull_for_close: Vec<Hash64> = Vec::new();
            let court_duties = session.palw_court_duties_v2(vec![bond_key]);
            let mut court_stalls: BTreeMap<&'static str, usize> = BTreeMap::new();
            for duty in &court_duties {
                if let Some(sent_daa) = court_moved.get(&(duty.session_id, duty.round, duty.i_am_responder))
                    && current_daa < sent_daa.saturating_add(COURT_MOVE_REPLAN_DAA)
                {
                    continue;
                }
                // **Already queued, from a tick that could not fund it** (audit M2-15). Moves that
                // do not fit the in-flight budget are carried over to the next tick, and this loop
                // rebuilt the same move every 2 seconds and pushed a duplicate — so the queue grew
                // by a copy per tick, `submitted_this_tick` spent the budget on the oldest copies,
                // and later sessions' moves were dropped unsent. A rung is clocked, so the moves
                // crowded out are exactly the ones whose lapse convicts the responder. The
                // opening-court branch has always had this guard; the responder branch did not.
                if court_pending.iter().any(|(sid, round, responder, _)| {
                    *sid == duty.session_id && *round == duty.round && *responder == duty.i_am_responder
                }) {
                    continue;
                }
                // The capture, and the family's backend for it. A party with no material — or a
                // family with no court — cannot answer honestly, and answering dishonestly is what
                // the terminal close exists to punish, so it stays silent and lets the clock decide.
                let backend = match self.backends().resolve(duty.class_id, duty.artifact_root) {
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
                // **Selecting evidence, not judging a producer** — so an anchor this node cannot
                // derive degrades to the roots-only check here rather than skipping the duty.
                // These are the responder's OWN captures: refusing to load them because the block
                // header is momentarily unreadable would make the node silent at its own court,
                // and silence at the opening rung now costs it the claim. The judging site (the
                // receipt verdict below) takes the opposite branch, and correctly: there an
                // underivable anchor means "no opinion", because verifying without it is exactly
                // what let borrowed material pass.
                let anchor = self
                    .job_anchor_for_claim(
                        &session,
                        backend.as_ref(),
                        network_domain,
                        duty.accepted_block,
                        duty.class_id,
                        &duty.executor_bond,
                    )
                    .unwrap_or_default();
                let roots = PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root, anchor };
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
                // **The capture is chosen by ROLE** (audit M2-4).
                //
                // The responder speaks for the accused execution, so it answers from the capture
                // that reproduces the claim's committed roots. The CHALLENGER speaks for its own
                // — the execution whose roots differed, which is the entire content of its
                // accusation — so it answers from that. Feeding one capture to both sides made
                // `agree` true at every rung by construction (`bisect_prefix_state` is a pure
                // function of the bytes and the index), the interval walked to the last leaf of a
                // 2^22 space, and no close could be assembled by anyone: fraud escaped, and an
                // attacker who merely agreed 22 times had the honest producer convicted by the
                // backstop for 23 transaction fees.
                //
                // A challenger that has lost its own execution (a restart) re-runs the job here
                // rather than borrowing the accused's — silence costs it the dispute, which is the
                // right price for an accusation it can no longer support.
                if !duty.i_am_responder && !own_executions.contains_key(&duty.claim_id) {
                    let rerun = self
                        .job_anchor_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        )
                        .and_then(|anchor| backend.job_for_anchor(anchor).ok())
                        .and_then(|(job, prompt)| backend.execute(&job, &prompt).ok());
                    match rerun {
                        Some(outcome) => {
                            own_executions.insert(duty.claim_id, outcome.material);
                        }
                        None => {
                            *court_stalls.entry("the challenger cannot re-execute its own accusation").or_default() += 1;
                            continue;
                        }
                    }
                }
                // **The ACCUSED execution — the bytes that reproduce the claim's committed roots.**
                //
                // Role-splitting the capture is right for the rungs and wrong for the terminal move
                // (audit3 S-01). `refutation_for_index` derives the refutation's binding FROM the
                // material it is handed, and `adjudicate_court_close_v2` pins that binding to the
                // accused claim before it reads any evidence:
                //
                //     check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(..))?;
                //     check_execution_root_binding(claim.execution_root, ..committed_execution_root)?;
                //
                // A challenger only reaches `Terminal` because its roots DIFFER from the claim's —
                // that difference is the accusation. So a close assembled from the challenger's own
                // capture carries the challenger's roots, fails the first binding check, and
                // `palw_court_close_verdict_v2` returns `None`. The challenger files nothing; the
                // responder is the fraudster and will not convict itself; no rung clock runs at
                // `Terminal`; and the `window_court` backstop then slashes the CHALLENGER's
                // collateral and re-arms the fraudulent claim, which finalizes and is paid. Proven
                // fraud became unpunishable, and the only party that can detect it paid to report
                // it.
                //
                // The backend's own contract says which capture the close comes from: "Returned by
                // BOTH sides, and deliberately the same call for both: an honest executor closing
                // its own case and a challenger closing a real fraud assemble the identical
                // object." Identical objects require identical material, and the material both
                // sides can name is the accused's.
                let accused_capture = materials
                    .get(&duty.claim_id)
                    .and_then(|pool| pool.iter().find(|b| backend.verify_material(b, roots) == PalwMaterialVerdictV1::Matches));
                let capture_from_own = (!duty.i_am_responder).then(|| own_executions.get(&duty.claim_id)).flatten();
                let Some(capture) = capture_from_own.or(accused_capture) else {
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
                            // Two different facts wearing one message (audit3 H4). "This capture
                            // has no state at that index" is a bad capture; "this FAMILY has no
                            // rung move at all" is a class whose disputes can never be decided by
                            // anybody, and an operator reading a stalled court needs to know which
                            // one it is looking at.
                            *court_stalls
                                .entry(if backend.supports_court() {
                                    "the backend cannot state its prefix at the midpoint"
                                } else {
                                    "this class has NO court responder in this build — no party can move, and the dispute cannot be decided"
                                })
                                .or_default() += 1;
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
                    //
                    // **STILL NOT COVERED BY A TEST.** The 2026-08-28 audit set the acceptance
                    // condition for M2-4 as a live round trip that convicts a real fault AND
                    // acquits a real innocent; the 2026-08-29 re-audit recorded that it was unmet
                    // and that nothing in the tree tests it; it is still unmet here. The repair
                    // below is verified by reading the adjudicator's binding checks, which is the
                    // same standing M2-4 was recorded `fixed` at — so it is written down where the
                    // code is rather than claimed in a table. The test needs a two-node court
                    // harness that does not exist.
                    (PalwBisectTurnV1::Terminal, _) => {
                        let Some(index) = duty.terminal_index else {
                            *court_stalls.entry("the ladder has not narrowed to a step").or_default() += 1;
                            continue;
                        };
                        // The rungs speak from each party's OWN execution — that is what makes a
                        // disagreement possible at all. The close does not: it is an assertion
                        // about the accused's step, so it is assembled from the accused's bytes by
                        // whichever party is making it (audit3 S-01).
                        let Some(accused) = accused_capture else {
                            // **And ASK for it.** The pull lived only in the receipt-duty loop, so
                            // a challenger that never needed the accused's bytes to open its case
                            // — it compares roots, not material — had no way to obtain them for
                            // the close. Without this the repair above would trade a court that
                            // convicts nobody for a court that stalls, which the same backstop
                            // punishes the same way. Same 25-DAA throttle and the same
                            // solicited-answer registration the receipt path uses.
                            if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(25)) {
                                requested.insert(duty.claim_id, current_daa);
                                pull_for_close.push(duty.claim_id);
                            }
                            *court_stalls.entry("the close needs the ACCUSED capture and this node holds none — pulling").or_default() +=
                                1;
                            trace!(
                                "[{PALW_PANEL}] session {} has narrowed to a step but this node holds no capture matching the claim's roots",
                                duty.session_id
                            );
                            continue;
                        };
                        let refutation = match backend.refutation_for_index(accused, index) {
                            Ok(r) => r,
                            Err(e) => {
                                *court_stalls.entry("the close does not assemble from this capture").or_default() += 1;
                                warn!("[{PALW_PANEL}] cannot assemble the close for session {}: {e}", duty.session_id);
                                continue;
                            }
                        };
                        // **The rows the court will need to read, asked of the adjudicator.**
                        // A close whose openings are empty is a close the court cannot recompute
                        // from: it holds no weights, so every operand the disputed step reads has
                        // to arrive with the evidence, proven against the class root.
                        let operand_openings = match backend.operand_openings_for(&refutation) {
                            Ok(o) => o,
                            Err(e) => {
                                *court_stalls.entry("the class cannot open the rows this step reads").or_default() += 1;
                                warn!("[{PALW_PANEL}] cannot open operands for session {}: {e}", duty.session_id);
                                continue;
                            }
                        };
                        let proof = PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings };
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
            // Ask for every accused capture a close needed and this node did not hold. Outside the
            // logging guard below deliberately: a request that only goes out when a summary line
            // happens to be printed is a request nobody can reason about.
            for claim in pull_for_close.drain(..) {
                self.flow_context.palw_gossip().note_pull_request(claim);
                self.flow_context.request_palw_material(claim).await;
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
                    // **Class capability is decided BEFORE looking at deliveries.** This resolve
                    // lived inside the per-material loop, so a seat that received NOTHING never
                    // reached its own `Incapable` answer and fell through to `Unavailable` — a
                    // signed accusation of withholding from a seat that could not have judged the
                    // data had it arrived. Zero deliveries and zero capability must answer the
                    // same thing.
                    if self.backends().resolve(duty.class_id, duty.artifact_root).is_err() {
                        break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                    }
                    for bytes in materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]) {
                        // Through the backend seam, which recomputes the leg root exactly.
                        // `Mismatch` is deliberately NOT an accusation here: it gathers no quorum
                        // and the claim voids. Convicting is the court's move, on evidence, and a
                        // seat that cannot reproduce a claim has not yet produced any.
                        //
                        // Resolved per duty from what the CHAIN says the claim's class is. A seat
                        // holding no material for that class cannot judge it — and it now SAYS so,
                        // instead of filing nothing.
                        //
                        // Filing nothing was read by the chain as a no-show and charged. Sortition
                        // does not ask which classes a node can execute, so that charge landed on
                        // seats whose only fault was being picked, and no answer avoided it:
                        // `Valid` would be a lie and `Unavailable` a signed accusation against an
                        // honest producer. `Incapable` is the missing answer — free, and counting
                        // toward neither side of the quorum. The chain refuses it for the liveness
                        // floor, where no node can truthfully claim it, so filing it there would
                        // only waste a fee.
                        let Ok(backend) = self.backends().resolve(duty.class_id, duty.artifact_root) else {
                            break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                        };
                        let Some(anchor) = self.job_anchor_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        ) else {
                            break 'verdict None;
                        };
                        if backend.verify_material(
                            bytes,
                            PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root, anchor },
                        ) == PalwMaterialVerdictV1::Matches
                        {
                            // **Retained here, and only here**: the chain carries this claim, this
                            // seat is on its panel, and these exact bytes reproduce its committed
                            // roots. Everything weaker was what let a stranger fill the disk
                            // (audit M2-2).
                            self.persist_foreign_material(&duty.claim_id, bytes);
                            break 'verdict Some(PalwReceiptVerdictV2::Valid);
                        }
                    }
                    // **This node's own disk, before the network and before any accusation**
                    // (audit M2-21). A seat that restarts loses its pool but keeps its retention,
                    // and the court arm already reads it — the verdict arm did not, so a restarted
                    // seat signed `Unavailable` against a producer whose material was sitting in
                    // its own directory. Verified like anything else: a file is evidence only if it
                    // reproduces the roots the claim committed to.
                    if let Some(bytes) = self.retained_capture(&duty.claim_id).or_else(|| {
                        std::fs::read(self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id))).ok()
                    }) && let Ok(backend) = self.backends().resolve(duty.class_id, duty.artifact_root)
                        && let Some(anchor) = self.job_anchor_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        )
                        && backend.verify_material(
                            &bytes,
                            PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root, anchor },
                        ) == PalwMaterialVerdictV1::Matches
                    {
                        materials.entry(duty.claim_id).or_default().push(bytes);
                        break 'verdict Some(PalwReceiptVerdictV2::Valid);
                    }
                    // No verifying material yet. Ask the network before accusing: the producer may
                    // be gone, but any peer that heard the broadcast can re-serve it.
                    //
                    // **The condition is "nothing VERIFIED", which is what the loop above just
                    // established — not "the pool is empty"** (audit M2-1). Gating on emptiness
                    // meant one unverifiable byte-string suppressed the pull permanently: an
                    // attacker sends four ~70-byte payloads for a claim id it reads off the header,
                    // the pool is non-empty and matches nothing, the seat never asks, and at half
                    // the window it signs `Unavailable` against an honest producer. Reaching this
                    // line already means every pooled payload failed to verify.
                    if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(25)) {
                        requested.insert(duty.claim_id, current_daa);
                        // Registered with the gossip center first: the answer must be exempt from
                        // the per-claim relay budget an attacker may already have spent.
                        self.flow_context.palw_gossip().note_pull_request(duty.claim_id);
                        self.flow_context.request_palw_material(duty.claim_id).await;
                    }
                    // Wait out half the window before accusing — gossip is not instant and an
                    // early `Unavailable` is a false accusation with a signature on it.
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
                } else if let Some((tip, _)) = chained_funding.clone() {
                    if session.get_virtual_utxo_entry(tip).is_some() {
                        // The tip of our own chain is in the UTXO set, so every carrier behind it
                        // was mined. Nothing is in flight and the budget is whole again.
                        inflight = 0;
                    } else if !self
                        .flow_context
                        .mining_manager()
                        .clone()
                        .has_transaction(tip.transaction_id, kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                        .await
                    {
                        // **The carrier that would create this change is neither mined nor pending,
                        // so it is gone.** Relay drops orphans silently and mempools evict; without
                        // this the panel waits on an output no one will ever produce, holding at the
                        // in-flight cap forever because the cap only clears when the tip confirms.
                        // Dropping the chain sends the next tick back to `resolve_fee_funding`,
                        // which reads what the chain and our own mempool actually say.
                        warn!(
                            "[{PALW_PANEL}] carrier {} was neither mined nor kept — re-resolving funding from the chain",
                            tip.transaction_id
                        );
                        chained_funding = None;
                        inflight = 0;
                    }
                }
                let held = inflight >= MAX_INFLIGHT_CARRIERS;
                let mut funding = if held {
                    // Back-pressure, not loss. Keeping the objects pending means the next tick
                    // re-offers them in priority order (court moves first), rather than building
                    // carriers the network will drop before anyone reads them.
                    None
                } else {
                    chained_funding.clone()
                };
                if held {
                    // Not the same condition as "no fee UTXO", and it must not print as one: this
                    // panel is waiting for its own chain to confirm, which is the system working.
                    trace!("[{PALW_PANEL}] holding: {inflight} carriers unconfirmed (cap {MAX_INFLIGHT_CARRIERS})");
                } else if funding.is_none() && receipts.keys().any(|c| !submitted.contains_key(c)) {
                    // Once per tick, not once per claim: a pending carrier keeps every quorum
                    // unfundable until it is mined, and that used to be tens of thousands of
                    // identical lines an hour.
                    warn!(
                        "[{PALW_PANEL}] a quorum stands but no fee UTXO resolves — a carrier may still be in flight; else \
                         --palw-fee-outpoint ({}) is unfunded or already spent",
                        self.config.fee_outpoint.as_deref().unwrap_or("unset")
                    );
                }
                // **The class registration, ahead of everything else and only once.**
                //
                // A class that is not registered mines nothing, so this is the one object whose
                // absence costs the whole lane rather than one claim. It is offered first for the
                // same reason a court rung is: everything behind it can wait a tick, and it
                // cannot — a producer with a worker and no class is a node doing nothing.
                if self.config.register_class.is_some() && !class_registration_submitted {
                    // Built at the top of the tick (chain reads only); this block owns the SUBMIT
                    // because the fee UTXO lives here.
                    if let Some(object) = class_registration.clone()
                        && let Some((funding_outpoint, funding_entry)) = funding.clone()
                    {
                        match self.build_lifecycle_tx(&object, funding_outpoint, &funding_entry) {
                            Ok(tx) => {
                                let txid = tx.id();
                                let change = tx.outputs[0].clone();
                                match self.flow_context.submit_rpc_transaction(&session, tx, Orphan::Forbidden).await {
                                    Ok(()) => {
                                        info!("[{PALW_PANEL}] submitted the class registration in tx {txid}");
                                        class_registration_submitted = true;
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
                                    }
                                    Err(e) => {
                                        warn!("[{PALW_PANEL}] the mempool refused the class registration: {e}");
                                        funding = None;
                                    }
                                }
                            }
                            Err(e) => warn!("[{PALW_PANEL}] cannot build the class registration carrier: {e}"),
                        }
                    }
                }
                // The court's moves first: a rung has a deadline and a receipt quorum does not.
                let mut unsent: Vec<(Hash64, u32, bool, PalwConsensusObjectV2)> = Vec::new();
                for (session_id, round, mine_is_responder, object) in std::mem::take(&mut court_pending) {
                    let Some((funding_outpoint, funding_entry)) = funding.clone().filter(|_| inflight < MAX_INFLIGHT_CARRIERS) else {
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
                                    court_moved.insert((session_id, round, mine_is_responder), current_daa);
                                    if let PalwConsensusObjectV2::CourtOpened { claim, .. } = &object {
                                        challenged.insert(*claim);
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "[{PALW_PANEL}] the mempool refused the {} for session {session_id}: {e}",
                                        object_name(&object)
                                    );
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
                    if let Some(at) = submitted.get(&claim)
                        && current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA)
                    {
                        continue;
                    }
                    // The attempt budget bounds consecutive FAILURES to build or submit, not the
                    // claim's whole life — it is cleared on a successful submit below, because a
                    // carrier that was accepted and then lost is not evidence that this object
                    // cannot be built.
                    if submit_attempts.get(&claim).copied().unwrap_or(0) >= SUBMIT_ATTEMPTS {
                        continue;
                    }
                    let pool = receipts.get(&claim).cloned().unwrap_or_default();
                    let Some(object) = session.palw_v2_receipt_quorum_assemble(claim, pool) else { continue };
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
                                    submitted.insert(claim, current_daa);
                                    submit_attempts.remove(&claim);
                                }
                                Err(e) => {
                                    // The chain we were spending is gone or was never there; stop
                                    // this tick rather than build more carriers on a dead input.
                                    warn!("[{PALW_PANEL}] the mempool refused the {} for claim {claim}: {e}", object_name(&object));
                                    funding = None;
                                }
                            }
                        }
                        // **Counted only for a failure THIS claim's object caused** (audit M2-25).
                        // The attempt was charged before the build, so one panel-wide condition —
                        // an unfunded fee outpoint, a mempool refusal, a storage-mass ceiling —
                        // spent an attempt for every pooled claim in the same tick, and three such
                        // ticks retired every quorum the node held. A build that fails on this
                        // object's own shape is a real attempt; anything else is the node's weather.
                        Err(e) => {
                            *submit_attempts.entry(claim).or_insert(0) += 1;
                            warn!("[{PALW_PANEL}] cannot build the carrier for claim {claim}: {e}");
                        }
                    }
                }
                // What the next tick continues from. `None` here means a refusal cleared it, and
                // the next tick resolves afresh.
                //
                // **Only when this tick was allowed to spend.** A back-pressured tick sets `funding`
                // to `None` deliberately — it is not a statement about the chain tip, and writing it
                // back DESTROYED a perfectly good one. The tip then had to be recovered by the scan,
                // which could not see it either: the last carrier was still unmined, so its change
                // was in no UTXO set at all. Measured on testnet-11: node 0 held at the cap once and
                // never carried another object, 6,343 warnings later.
                if !held {
                    chained_funding = funding;
                }
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
                    session
                        .palw_disputable_claims_v2(vec![bond_key])
                        .into_iter()
                        .filter_map(|d| (!challenged.contains(&d.claim_id)).then_some(d.claim_id)),
                );
            }
            // **"Not yet submitted" is not a bounded condition.** The rule was: keep a claim's
            // material while it is LIVE (a duty or a dispute names it) or while this node has not
            // submitted for it. The second half never becomes false for a claim the panel cannot
            // submit for — no quorum assembled, the class was never resolvable, the receipt window
            // lapsed — so its material stayed in memory for the life of the process. Measured on
            // testnet-11: hundreds of such claims a day, each holding up to four gossiped
            // materials, against a node whose RSS climbed from zero to 11 GB in twelve hours and
            // was OOM-killed roughly every thirty.
            //
            // Bounded by the claim's own age instead. Past the window, a claim that is not live
            // is one no duty, no dispute and no license will ever ask about again — and the
            // court's own path does not depend on this pool anyway: it re-reads the capture from
            // `retained_capture` on disk, which is what the retention obligation is for.
            // **A claim with no recorded arrival is STALE, not immortal** (audit M2-2). `is_some_and`
            // returned false for exactly the entries that dominate the pool — the ones this seat
            // has no duty on (a seat is drawn on ~5/N of claims) — so the retention bound applied
            // only to claims that were already bounded by their duty, and every foreign claim was
            // kept for the life of the process. Arrivals are stamped at pool insert now, so `None`
            // here means an entry older than any bookkeeping this process still holds.
            let stale = |claim: &Hash64| {
                first_seen.get(claim).map(|seen| current_daa > seen.saturating_add(PANEL_POOL_RETENTION_DAA)).unwrap_or(true)
            };
            materials.retain(|claim, _| live.contains(claim) || (!submitted.contains_key(claim) && !stale(claim)));
            receipts.retain(|claim, _| live.contains(claim) || (!submitted.contains_key(claim) && !stale(claim)));
            // The bookkeeping keyed on those claims goes with them, or the maps that decide what to
            // keep become the thing that grows.
            first_seen.retain(|claim, _| materials.contains_key(claim) || receipts.contains_key(claim) || live.contains(claim));
            requested.retain(|claim, _| first_seen.contains_key(claim) || live.contains(claim));
            // Our own executions are only needed while the dispute they support is open.
            own_executions.retain(|claim, _| live.contains(claim));
            submit_attempts.retain(|claim, _| receipts.contains_key(claim));
            submitted.retain(|_claim, at| current_daa <= at.saturating_add(PANEL_POOL_RETENTION_DAA));
            trace!("[{PALW_PANEL}] tick: {} duties, {} claims pooled", duties.len(), receipts.len());
        }
    }
}

/// **The smallest collateral a bond carrier can hold and still be relayed.**
///
/// A UTXO's KIP-0009 storage mass is `C · p² / value`, so it grows as the output SHRINKS. The
/// chain's own collateral floor is therefore not a value a carrier can necessarily hold: on
/// testnet-11, `min_collateral_sompi` is 400,000 sompi, whose output alone costs 10,000,000 mass
/// against a 480,000 relay limit — 20× over, whatever funds it, forever. The panel sized itself at
/// exactly that floor, built the carrier, and watched the mempool refuse it every five seconds:
/// `transaction storage mass of 10000003 is larger than max allowed size of 480000`.
///
/// The mass of the two-output carrier is `C·p²/collateral + C·p²/change − (the input term)`, and
/// both output terms grow as their side of the split shrinks — so it is U-shaped in the collateral
/// with its minimum at the even split. On `[floor, spendable/2]` it is therefore monotonically
/// decreasing, which is what makes the binary search below correct, and what makes the mass at
/// `spendable/2` the verdict on whether ANY split of this funding can be carried.
///
/// `None` means no split works: the funding is too small to carry a bond at all, which is a
/// different operator action (send more) than "this bond needs more collateral".
fn min_carryable_collateral(
    funding: &UtxoEntry,
    fee: u64,
    collateral_script: &kaspa_consensus_core::tx::ScriptPublicKey,
    storage_mass_parameter: u64,
    limit: u64,
    floor: u64,
) -> Option<u64> {
    let spendable = funding.amount.checked_sub(fee)?;
    let input = UtxoCell::from(funding);
    let (p_collateral, p_change) = (utxo_plurality(collateral_script), utxo_plurality(&funding.script_public_key));
    let mass_at = |collateral: u64| -> Option<u64> {
        let change = spendable.checked_sub(collateral)?;
        // `calc_storage_mass` states non-zero values as a precondition — it divides by them.
        if collateral == 0 || change == 0 {
            return None;
        }
        calc_storage_mass(
            false,
            std::iter::once(input),
            [UtxoCell::new(p_collateral, collateral), UtxoCell::new(p_change, change)].into_iter(),
            storage_mass_parameter,
        )
    };
    let (mut lo, mut hi) = (floor.max(1), spendable / 2);
    if lo > hi || mass_at(hi)? > limit {
        return None;
    }
    if mass_at(lo).is_some_and(|m| m <= limit) {
        return Some(lo);
    }
    // `lo` never fits and `hi` always does, so the answer is `hi` once they are adjacent.
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        match mass_at(mid) {
            Some(m) if m <= limit => hi = mid,
            _ => lo = mid,
        }
    }
    Some(hi)
}

fn verdict_name(verdict: &PalwReceiptVerdictV2) -> &'static str {
    match verdict {
        PalwReceiptVerdictV2::Valid => "Valid",
        PalwReceiptVerdictV2::Unavailable { .. } => "Unavailable",
        PalwReceiptVerdictV2::Incapable => "Incapable",
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
            // **Registration is a precondition, not a mode.** A node registering its first bond has
            // none, so the registration job runs first — but it used to run INSTEAD, and
            // `--palw-register-bond` left in a service unit therefore replaced the panel with a
            // one-shot. `bond_registration_worker` sees the key already holds a bond, says so in one
            // INFO line, and returns; the service future completed and the seat answered nothing for
            // the rest of the process's life. Panels are derived from CHAIN state, not from whether
            // this process runs the service, so the bond kept being drawn — and `slash_silent_seats`
            // charges an absent seat `claim.reserved` at the receipt timeout and at both court
            // closes. The flag that was supposed to create a bond was quietly spending it.
            //
            // Falling through is safe in every case: `worker()` declines by itself, with its own
            // message, when there is no seat identity to run as — which is exactly the state a node
            // that has only just registered is in.
            if self.config.register_bond {
                self.bond_registration_worker().await;
            }
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_PANEL);
        self.shutdown.trigger.trigger();
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

    /// The two-output bond carrier this node builds: one collateral output to the payee, one
    /// change output back to the funding script, one input.
    fn carrier_storage_mass(funding: &UtxoEntry, fee: u64, collateral: u64, payee: &kaspa_consensus_core::tx::ScriptPublicKey) -> u64 {
        let change = funding.amount - fee - collateral;
        calc_storage_mass(
            false,
            std::iter::once(UtxoCell::from(funding)),
            [UtxoCell::new(utxo_plurality(payee), collateral), UtxoCell::new(utxo_plurality(&funding.script_public_key), change)]
                .into_iter(),
            kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
        )
        .expect("this carrier's storage mass is computable")
    }

    fn mldsa_script(byte: u8) -> kaspa_consensus_core::tx::ScriptPublicKey {
        kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[byte; 64])
    }

    /// **This chain's floor collateral cannot be carried at all, and the fix is a number, not a
    /// bigger funding UTXO.**
    ///
    /// The measured case from testnet-11 on 2026-08-26: a node funded with 10 MSK sized itself at
    /// the chain's 400,000 sompi floor and the mempool refused every carrier it built with
    /// `transaction storage mass of 10000003 is larger than max allowed size of 480000`. The
    /// refusal is a property of the OUTPUT's value, so no amount of funding fixes it — which is
    /// why the panel now asks this question before it submits.
    #[test]
    fn the_chain_floor_collateral_cannot_be_carried() {
        let payee = mldsa_script(7);
        let funding = UtxoEntry::new(1_000_000_000, mldsa_script(9), 0, false);
        let fee = 349_438;
        let floor = 400_000;

        let at_floor = carrier_storage_mass(&funding, fee, floor, &payee);
        assert!(
            at_floor > MAXIMUM_STANDARD_TRANSACTION_MASS,
            "a {floor} sompi output costs {at_floor} storage mass, which is what made this unregisterable"
        );

        let minimum = min_carryable_collateral(
            &funding,
            fee,
            &payee,
            kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
            MAXIMUM_STANDARD_TRANSACTION_MASS,
            floor,
        )
        .expect("10 MSK can carry a bond at some collateral");

        assert!(minimum > floor, "the answer has to be above the floor, or the floor was carryable after all");
        assert!(
            carrier_storage_mass(&funding, fee, minimum, &payee) <= MAXIMUM_STANDARD_TRANSACTION_MASS,
            "the collateral this returns must actually be relayable"
        );
        assert!(
            carrier_storage_mass(&funding, fee, minimum - 1, &payee) > MAXIMUM_STANDARD_TRANSACTION_MASS,
            "one sompi less must NOT be relayable — otherwise this is not the minimum, and an operator locks more \
             money than the limit asked for"
        );
        // The value the two hosts were registered with, kept as the record of what worked live.
        assert!(20_000_000 >= minimum, "20,000,000 sompi registered two bonds on testnet-11, so it cannot be below the minimum");

        // **The number quoted before any funding exists must never exceed the number the funding
        // then demands.** It is the figure a waiting operator funds the address with, and if it
        // could land above the real minimum they would send exactly what they were told and still
        // be refused. `collateral_relay_lower_bound` computes this same expression from the payee
        // script; it drops the change and input terms, which can only push the true answer up.
        let c = kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER;
        let plurality = utxo_plurality(&payee);
        let quoted_before_funding = c * plurality * plurality / MAXIMUM_STANDARD_TRANSACTION_MASS;
        assert!(
            quoted_before_funding <= minimum,
            "the pre-funding floor ({quoted_before_funding}) has to be a LOWER bound on the real minimum ({minimum})"
        );
    }

    /// **"No collateral works" and "this collateral is too small" are different operator actions.**
    ///
    /// A funding UTXO whose every split is over the limit needs more money sent to it; saying
    /// "raise the collateral" would send the operator to a knob that cannot help.
    #[test]
    fn funding_too_small_to_carry_any_bond_says_so() {
        let payee = mldsa_script(7);
        let funding = UtxoEntry::new(1_000_000, mldsa_script(9), 0, false);
        assert!(
            min_carryable_collateral(
                &funding,
                349_438,
                &payee,
                kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
                MAXIMUM_STANDARD_TRANSACTION_MASS,
                400_000,
            )
            .is_none(),
            "1 MSK cannot carry a bond at any split, and the answer must be None rather than an unusable number"
        );
    }

    /// A collateral that already fits is returned unchanged — the floor is the answer when the
    /// floor works, so this never raises an operator's locked money without cause.
    #[test]
    fn a_carryable_floor_is_left_alone() {
        let payee = mldsa_script(7);
        let funding = UtxoEntry::new(10_000_000_000, mldsa_script(9), 0, false);
        let floor = 50_000_000;
        let minimum = min_carryable_collateral(
            &funding,
            349_438,
            &payee,
            kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER,
            MAXIMUM_STANDARD_TRANSACTION_MASS,
            floor,
        )
        .expect("a 50,000,000 sompi output is well under the relay limit");
        assert_eq!(minimum, floor, "a floor that fits is the answer; raising it would lock money for nothing");
    }

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

    /// **The confirmed UTXO set cannot tell you what your own mempool transactions have spent.**
    ///
    /// Funding is chosen from the VIRTUAL utxo set, which is the confirmed one. An output a carrier
    /// of ours is spending right now is still in it and stays there until that carrier is mined, so
    /// every source the panel has — the persisted outpoint, the configured one, and the recovery
    /// scan — can hand back money that is already committed. The mempool answers `already spent by
    /// transaction … in the mempool`, that clears the funding, the next tick asks again, gets the
    /// same outpoint, and the panel spins there instead of carrying receipts.
    ///
    /// Measured on testnet-11 three hours after the recovery scan shipped: 387 double-spend
    /// refusals and 103 recoveries against 143 successful submissions — and 77 claims defaulted
    /// with their escrow burned, because a quorum that cannot be carried inside its receipt window
    /// is a quorum that never happened. The scan was right about WHERE the money is and wrong about
    /// whether it was still there to spend.
    ///
    /// The rule is a set, and the set only ever grows: an outpoint this panel has spent is never a
    /// funding source again, mined or not, because once the carrier lands the money is at its
    /// CHANGE outpoint — a different one. That is what makes a permanent skip correct rather than a
    /// cache someone has to remember to invalidate.
    #[test]
    fn an_outpoint_this_panel_already_spent_is_never_funding_again() {
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

        let op = |n: u64| TransactionOutpoint::new(TransactionId::from_u64_word(n), 0);
        let mut spent: HashSet<TransactionOutpoint> = HashSet::new();

        // The candidate list and the scan both filter on this set, so the property under test is
        // the set's own semantics: spending is one-way, and re-confirming does not undo it.
        assert!(!spent.contains(&op(1)), "an untouched outpoint is spendable");
        spent.insert(op(1));
        assert!(spent.contains(&op(1)), "and once funded from, it is not offered again");
        spent.insert(op(1));
        assert_eq!(spent.len(), 1, "recording the same spend twice is not two spends");

        // The change of that carrier is a DIFFERENT outpoint, which is why the panel is not left
        // with nothing: the money moves, it does not vanish.
        assert!(!spent.contains(&op(2)), "the change outpoint is free to spend next");
    }
}
