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
//! 1. Read [`PalwProducerFactsV2`] and pre-flight against them — wrong key, spent budget, a full
//!    exposure ceiling or (past R-core+) a bond under the producer floor are all knowable before an
//!    inference is spent ([`palw_producer_ready_v1`]).
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
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2, class_ticket_v3,
};
use kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_core::{error, info, trace, warn};
use kaspa_hashes::Hash64;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_p2p_flows::flow_context::FlowContext;
use misaka_palw_base0::produce::base0_rc_job_anchor_v1;

pub const PALW_PRODUCER: &str = "palw-producer";

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
    /// **How long an ATTEMPT capture stays on disk** (`--palw-attempt-retention-minutes`) — see
    /// [`retained_capture_prune_due_v1`]. Free-prompt captures do not read it.
    pub attempt_retention: std::time::Duration,
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
    /// **The network's prompt-commitment form** (ADR-0081 Decision 3) — `Params::palw_prompt_ids_form_v1()`,
    /// handed to every backend this service resolves and to every payload it decodes.
    pub prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// **Artifact files this node holds, for classes whose weights are not derivable.**
    ///
    /// The floor's artifact is minted from a pinned seed by every node, so it needs no file and
    /// this stays empty on an RC node. A converted class — a real checkpoint quantized offline —
    /// cannot be re-derived from anything the node has, so its bytes must be carried. Loaded once
    /// at startup and matched against what the CHAIN says the class is; a file that does not
    /// match is not used, never trusted into service.
    pub class_artifacts: Vec<std::path::PathBuf>,
    /// ADR-0067 tier ④: the byte bound on resident artifacts (0 = unbounded).
    pub class_cache_bytes: u64,
    /// ADR-0112: how much of a mapped class's weights this node keeps in memory.
    pub class_residency: misaka_palw_sdk::PalwWeightResidencyV1,
    /// ADR-0132: the node's per-class counters this producer reports its draws into.
    pub telemetry: std::sync::Arc<crate::palw_economics::PalwNodeTelemetryV1>,
}

pub struct PalwProducerService {
    config: PalwProducerConfig,
    /// Fired by `signal_exit` so this service's `start` future can finish — the panel's fix
    /// (`PalwPanelService::shutdown`), copied to the lane that never got it. The ADR-0068 drill
    /// measured the omission three out of three times: every server stopped, the worker loop
    /// kept ticking, the AsyncRuntime join parked the main thread in `pthread_join` forever, and
    /// only SIGKILL ended the process. On a fleet under systemd that is a silent
    /// `TimeoutStopSec`-then-SIGKILL on EVERY restart.
    shutdown: kaspa_utils::triggers::SingleTrigger,
    /// Loaded once at construction, through the SDK — each file by its own container's rules
    /// (digest-checked whole for the dense tier, mapped and rooted for the Qwen3.6 tier); whether
    /// a holding is the artifact the CHAIN registered is decided per block, against the producer
    /// facts.
    class_holdings: Vec<misaka_palw_sdk::PalwLoadedArtifactV1>,
    /// Draws whose class ticket WON and whose Layer-0 digest then lost against the template's
    /// `bits` — the count that names a chain whose `bits` has priced the lane out (5f card §10b:
    /// bits at p = 1.5e-3 while every class ticket that won lost here, and nothing said so).
    network_draw_lost: std::sync::atomic::AtomicU64,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    /// **The network's own ruleset, for the fences a producer's own decisions turn on.** The panel
    /// already holds it for exactly this reason; the producer resolved none, which is why it could
    /// not tell a chain whose data-availability court is in force from one where it is dormant
    /// (mainnet audit 2026-09-06, C-5). Read, never reconstructed: a second spelling of a fence is
    /// a second network.
    consensus_config: Arc<Config>,
    /// `None` disables production and says why at startup — a producer that cannot sign is not a
    /// producer, and finding that out at the first template is finding it out too late.
    keypair: Option<Box<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>>,
    /// The same seed, kept so the receipt lane can build a `ValidatorKey` — its signer API is
    /// what `build_fp_receipt_spend_envelope` takes, and generating the keypair twice from one
    /// seed is the derivation being deterministic, not two identities.
    key_seed: Option<[u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]>,
    bond: Option<TransactionOutpoint>,
    miner_data: Option<MinerData>,
    /// **Why the ATTEMPT lane can never produce for `--palw-producer-class` on this node**, where
    /// `producer_class_unproducible_v1` could say so for certain at startup (the route-matrix
    /// audit's #1). A soft refusal, like every other one this constructor makes: the node, its seat,
    /// its RPC and this producer's receipt lane (which is per bond and needs no artifact) keep running,
    /// and the attempt lane holds with this sentence as its `disabled` status.
    class_refusal: Option<String>,
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
/// **The answer envelope beside the material** (ADR-0084 Decisions 2 and 5): `<claim>.answer`,
/// the `FPA1` payload the resolver serves in place of a material the transport would refuse.
/// Staged by the submitter at submission, or derived once from the retained capture by the
/// panel and cached under this name.
pub(crate) const PALW_RETAINED_ANSWER_SUFFIX: &str = ".answer";

/// **Is this chain's data-availability court in force at `daa_score`?** (ADR-0062; mainnet audit
/// 2026-09-06, C-5.)
///
/// One spelling, read straight off the ruleset, so the producer's refusal and the panel's duty
/// scan cannot disagree about whether a claim made now is defensible.
pub(crate) fn palw_da_court_in_force_v1(config: &Config, daa_score: u64) -> bool {
    config.params.palw_da_court.is_some_and(|fence| fence.is_active(daa_score))
}

/// **ADR-0099 Decision 5 / ADR-0100: whether the one-move court is in force at this DAA** — the
/// seat's reading of `Params::palw_shard_court`, through the fence's own mode-aware accessor, so
/// a seat files an accusation exactly on the networks whose acceptance layer takes one.
pub(crate) fn palw_shard_court_in_force_v1(config: &Config, daa_score: u64) -> bool {
    config.params.palw_shard_court_active_at(daa_score)
}

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

/// Where a claim's answer envelope is retained (ADR-0084): the material's path with the answer
/// suffix, so the two are siblings and a directory listing pairs them.
pub(crate) fn palw_retained_answer_path(dir: &std::path::Path, claim: &Hash64) -> std::path::PathBuf {
    dir.join(format!("{claim}{PALW_RETAINED_ANSWER_SUFFIX}"))
}

/// The wall-clock horizon of every retained capture that is not an attempt claim's: the bind +
/// receipt windows at the frozen 120 s cadence are ~40 h, and two days of re-serving covers every
/// claim that can still be licensed.
const PALW_RETENTION_HORIZON: std::time::Duration = std::time::Duration::from_secs(48 * 3600);

/// **Is a retained capture due for pruning?** The time rule `palw_retention::PalwRetentionJanitor` acts on,
/// pure so it can be pinned without a consensus instance. `age` is the file's (`None` = unreadable,
/// treated as old, as before); `claim` is the chain's view of the claim at the tip (`None` = never
/// mined, or dropped).
///
/// * **An ATTEMPT claim's capture is due at `attempt_horizon`.** Its seats license it by replaying
///   the block's job (ADR-0084 Decision 7), never from these bytes, and a court or a
///   data-availability accusation that asks this node later is answered from a capture re-made by
///   that same replay and checked against the claim's committed roots (the panel's
///   `remade_attempt_capture_v1`). So the file only serves the first minutes. It was kept 48 h, and
///   a graph-v5 capture is ~0.8 GB a block: once ADR-0112/0117 made a Qwen3.6 draw take ~45 s
///   instead of ~18 min, C's seat2 wrote 53 GB in 1 h 45 min and filled its 387 GB disk
///   (2026-09-12), taking the node's database down with it.
/// * **A FREE-PROMPT claim's capture is the one copy anywhere** (graph-v5 captures are over the
///   transport cap, so only the executor can open an interval): 48 h, and never while the chain can
///   still ask about it ([`free_prompt_retention_is_owed`]).
/// * **A claim the chain does not know**: 48 h, as before.
pub(crate) fn retained_capture_prune_due_v1(
    age: Option<std::time::Duration>,
    claim: Option<(&kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2, &kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2)>,
    attempt_horizon: std::time::Duration,
) -> bool {
    use kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2 as S;
    let age = age.unwrap_or(std::time::Duration::MAX);
    match claim {
        Some((S::Attempt, _)) => age >= attempt_horizon,
        Some((source, phase)) => age >= PALW_RETENTION_HORIZON && !free_prompt_retention_is_owed(source, phase),
        None => age >= PALW_RETENTION_HORIZON,
    }
}

/// **Can a seat, a redrawn seat, a challenger or an accuser still ask for this free-prompt
/// claim's capture?** Separated so it can be pinned without a consensus instance. Owed while a seat, a redrawn seat, a challenger or an accuser can still ask
/// for an opening; not once the claim is `Final` (nothing can open a challenge on it any more) or
/// `Voided` (there is nothing left to defend). An attempt claim is never "owed" here — its seats
/// replay the anchor's job instead (ADR-0084 Decision 7) and it keeps the wall-clock horizon.
fn free_prompt_retention_is_owed(
    source: &kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2,
    phase: &kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2,
) -> bool {
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2 as P, PalwClaimSourceV2 as S};
    matches!(source, S::FreePrompt { .. })
        && matches!(phase, P::Provisional | P::PanelBound { .. } | P::ReceiptLicensed { .. } | P::DefaultDisputed { .. })
}

/// **A hold that outlives this is not a hold; it is a producer that does not work** (the 2026-09-23
/// route-matrix audit's #1). Half an hour: longer than any honest wait this loop knows (an epoch
/// boundary, a registry span, a sync), far shorter than the deployment the first testnet-12 fleet spent
/// holding at INFO.
const PALW_PRODUCER_STARVED_AFTER: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Log a hold at its own level while it is young, and at ERROR — saying how long nothing has been
/// produced — once it has outlived [`PALW_PRODUCER_STARVED_AFTER`] since the producer last made progress.
fn log_producer_hold_v1(detail: &str, loud: bool, since_progress: std::time::Duration) {
    if since_progress >= PALW_PRODUCER_STARVED_AFTER {
        error!(
            "[{PALW_PRODUCER}] NOT PRODUCING for {} min — holding: {detail}",
            since_progress.as_secs() / 60
        );
    } else if loud {
        warn!("[{PALW_PRODUCER}] holding: {detail}");
    } else {
        info!("[{PALW_PRODUCER}] holding: {detail}");
    }
}

/// **Why the attempt lane holds — as the chain would refuse the attempt** (ADR-0152 v3.1 post-edit
/// 11, U2 / P6; T08's node half).
///
/// Past `Params::palw_rcore_plus` admission (items 7b and 8) and the fold (`apply_attempt`:
/// `ProducerBelowFloor`, then `AttemptExposureCeiling`) measure a bond by two numbers the old
/// pre-check never read: the producer floor, and the one committed ledger (own commitments,
/// registration and every `max(duty, live lock)`). Both refusals are non-fatal for a block's own
/// attempt — the fold skips it and the block stands — so a producer still asking
/// `reserved_exposure` mined blocks whose work became no claim: an inference spent, and nothing in
/// the log to say why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwProducerHoldV1 {
    /// One of `ready_to_produce`'s verdicts, by the sentence it returns (`PALW_NOT_READY_*_V2`). The
    /// operator CLI matches those constants, so the committed-ledger hold is reported under
    /// `PALW_NOT_READY_EXPOSURE_FULL_V2` too — it IS that hold, measured on the ledger the chain
    /// reads — and only the bracket's numbers say which ledger.
    NotReady(&'static str),
    /// **U2 (S-SPEC §10a): the bond's posted collateral is `shortfall` sompi below the producer
    /// floor `floor`** (`PalwProducerBondFactsV2::producer_floor_shortfall`, the fold's own
    /// function). The fold skips every attempt under it for as long as the bond lives, so there is
    /// nothing to wait for — and nothing to top up either: no object raises a registered bond's
    /// collateral (`BondRegistered` and `BondRetireRequested` are the only bond writers, and "one
    /// key, one bond" refuses a second registration from the same key, retired or not). Registration
    /// is refused below the same floor (`palw_bond_registration_floor_v1`) and kaspad sizes a bond
    /// at or above it, so the one way a bond gets here is a slash. The sentence therefore names the
    /// shortfall (the spec's words) and then the only way out: a bond of at least `floor` under a
    /// NEW key.
    BelowProducerFloor { shortfall: u64, floor: u64 },
    /// **SW-10: the eligible stake a claim of this class by this bond would draw its panel from is
    /// below the draw's floor** (`PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1`), so a claim made now binds
    /// no panel at its anchor (SW-8) and voids at `BindTimeout`. Raised only where
    /// [`palw_class_eligible_stake_at_floor_v1`] answers, which today it never does.
    EligibleStakeBelowFloor,
}

impl std::fmt::Display for PalwProducerHoldV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady(sentence) => f.write_str(sentence),
            // No " [" in this sentence: `misaka-cli`'s `hold_from_log` cuts the sentence from the
            // numbers at the last one, and prints a sentence it does not know whole — so the way
            // out reaches an operator even through a CLI that predates it.
            Self::BelowProducerFloor { shortfall, floor } => write!(
                f,
                "top up {shortfall} sompi to reach the producer floor — but a registered bond's collateral cannot be raised, so \
                 register a bond of at least {floor} sompi under a NEW key (`misaka key gen --out <new seed>`, then `misaka mining \
                 setup --key-file <new seed>`) and produce with that one; `misaka bond retire` releases what this bond still holds"
            ),
            Self::EligibleStakeBelowFloor => write!(
                f,
                "the eligible stake this bond's claim would draw its panel from is below the draw's {}‰ floor, so a claim made now \
                 would void at BindTimeout",
                kaspa_consensus_core::palw_panel_v2::PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1
            ),
        }
    }
}

/// **What the pre-check reads past `Params::palw_rcore_plus`, besides the facts** — `None` below
/// the fence, so the old path has nothing new it could read ([`palw_rcore_plus_reads_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwRcorePlusReadsV1 {
    /// The producer floor: the bundle's `min_collateral_sompi`, the number
    /// `palw_bond_producer_floor_shortfall_v1` measures posted collateral against. Only the hold's
    /// sentence reads it — the size a replacement bond must post; WHETHER the bond holds is the
    /// facts' shortfall, the fold's own answer.
    pub producer_floor: u64,
    /// [`palw_class_eligible_stake_at_floor_v1`]'s answer; `None` (unknown) never holds.
    pub eligible_stake_at_floor: Option<bool>,
}

/// [`PalwRcorePlusReadsV1`] for a candidate at `candidate_daa`, or `None` where the fence is not in
/// force there ([`palw_rcore_plus_producer_floor_v1`] decides which).
pub(crate) fn palw_rcore_plus_reads_v1(
    params: &kaspa_consensus_core::config::params::Params,
    session: &kaspa_consensusmanager::ConsensusProxy,
    class_id: Hash64,
    executor_bond: &TransactionOutpoint,
    candidate_daa: u64,
) -> Option<PalwRcorePlusReadsV1> {
    palw_rcore_plus_producer_floor_v1(params, candidate_daa).map(|producer_floor| PalwRcorePlusReadsV1 {
        producer_floor,
        eligible_stake_at_floor: palw_class_eligible_stake_at_floor_v1(session, class_id, executor_bond, candidate_daa),
    })
}

/// **The producer floor where `palw_rcore_plus` is in force at `candidate_daa`, else `None`** — the
/// one place the pre-check asks which side of the fence it is on. `palw_rcore_plus_fence` is `Some`
/// only on a `ConsensusV2` network, so the bundle the floor is read from exists exactly when the
/// fence can be in force; its `min_collateral_sompi` is what the fold's
/// `palw_bond_producer_floor_shortfall_v1` measures against.
pub(crate) fn palw_rcore_plus_producer_floor_v1(
    params: &kaspa_consensus_core::config::params::Params,
    candidate_daa: u64,
) -> Option<u64> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return None;
    };
    params.palw_rcore_plus_active_at(candidate_daa).then(|| bundle.state.min_collateral_sompi())
}

/// **The attempt lane's pre-check, on the ledger the chain measures it by** (ADR-0152 P6).
///
/// Below the fence (`rcore_plus` is `None`) — testnet-11, devnet, mainnet — this IS
/// `PalwProducerFactsV2::ready_to_produce`, byte for byte: the `reserved_exposure` ledger, no floor,
/// no stake question. Past it, in the order the fold refuses: the key and the bond (unchanged), then
/// the producer floor (`apply_attempt` checks it before the class gate), the class gate, the epoch
/// budget, the committed room (`has_committed_room`, the exact inequality admission item 8 and the
/// fold's ceiling apply to `committed`), and last the eligible-stake answer. The stake question is
/// not a refusal of the attempt — the fold accepts it and the draw voids it later — so it is asked
/// only after every question that is.
///
/// **Not yet the RPC's verdict.** `getPalwProducerFacts`' `not_ready_reason` still answers
/// `ready_to_produce`, so past the fence it can say "ready" for a bond this node holds; closing that
/// needs the verdict in consensus-core, where the RPC and this loop can both call it.
pub(crate) fn palw_producer_ready_v1(
    facts: &PalwProducerFactsV2,
    local_pubkey: &[u8],
    rcore_plus: Option<PalwRcorePlusReadsV1>,
) -> Result<(), PalwProducerHoldV1> {
    use kaspa_consensus_core::palw_producer_v2::{
        PALW_NOT_READY_BOND_UNKNOWN_V2, PALW_NOT_READY_CLASS_NOT_ADMITTING_V2, PALW_NOT_READY_EPOCH_BUDGET_V2,
        PALW_NOT_READY_EXPOSURE_FULL_V2,
    };
    let Some(reads) = rcore_plus else {
        return facts.ready_to_produce(local_pubkey).map_err(PalwProducerHoldV1::NotReady);
    };
    facts.ready_to_spend_receipts(local_pubkey).map_err(PalwProducerHoldV1::NotReady)?;
    let bond = facts.bond.as_ref().ok_or(PalwProducerHoldV1::NotReady(PALW_NOT_READY_BOND_UNKNOWN_V2))?;
    if let Some(shortfall) = bond.producer_floor_shortfall {
        return Err(PalwProducerHoldV1::BelowProducerFloor { shortfall, floor: reads.producer_floor });
    }
    if facts.class_admission_refusal.is_some() {
        return Err(PalwProducerHoldV1::NotReady(PALW_NOT_READY_CLASS_NOT_ADMITTING_V2));
    }
    if !facts.has_epoch_room() {
        return Err(PalwProducerHoldV1::NotReady(PALW_NOT_READY_EPOCH_BUDGET_V2));
    }
    if !bond.has_committed_room() {
        return Err(PalwProducerHoldV1::NotReady(PALW_NOT_READY_EXPOSURE_FULL_V2));
    }
    if reads.eligible_stake_at_floor == Some(false) {
        return Err(PalwProducerHoldV1::EligibleStakeBelowFloor);
    }
    Ok(())
}

/// **The `holding:` line's detail**, which is also the runtime's `producer_reason` — the sentence,
/// then the numbers that tell one cause of it from another.
///
/// Below the fence the bracket is the one this loop has always printed (`exposure=reserved/ceiling
/// per_claim=…`, which `misaka-cli`'s `HoldNumbers::from_bracket` reads). Past it `exposure=` carries
/// the COMMITTED ledger — the number the chain compares with the ceiling — marked `ledger=committed`,
/// and a bond under the producer floor adds `floor_shortfall=`: a reader of the old bracket would see
/// room on a bond the fold refuses, which is the confusion this whole change exists to end.
pub(crate) fn palw_producer_hold_detail_v1(facts: &PalwProducerFactsV2, hold: &PalwProducerHoldV1, rcore_plus_active: bool) -> String {
    format!(
        "{hold} [class={} epoch={} produced={} budget={}{}{}]",
        facts.class_id,
        facts.epoch_index,
        facts.epoch_produced_blocks,
        facts.epoch_budget_blocks,
        match &facts.bond {
            Some(bond) if rcore_plus_active => format!(
                " exposure={}/{} per_claim={} ledger=committed{}",
                bond.committed,
                bond.exposure_ceiling,
                bond.claim_exposure,
                bond.producer_floor_shortfall.map(|shortfall| format!(" floor_shortfall={shortfall}")).unwrap_or_default()
            ),
            Some(bond) => format!(" exposure={}/{} per_claim={}", bond.reserved_exposure, bond.exposure_ceiling, bond.claim_exposure),
            None => String::new(),
        },
        // Route-matrix #7: the gate's own words when it is the registry that holds.
        facts.class_admission_refusal.as_deref().map(|why| format!(" registry=\"{why}\"")).unwrap_or_default()
    )
}

/// **Room for one canonical free-prompt claim, asked before its inference is run** (the panel's
/// `build_canonical_claim`; ADR-0152 P6).
///
/// Past `palw_rcore_plus` the fold refuses a `FreePromptCommitted` below the producer floor
/// (`ProducerBelowFloor`, the FP arm's own check) and prices it against the one committed ledger
/// (`palw_fp_bond_room_v2`), so the pre-check reads the same two numbers — `claim_exposure` still
/// standing in for the commitment's own reservation, as it always has here. That stand-in is the
/// attempt lane's price, not the FP arm's, so this is an estimate that saves the inference; the
/// fold's exact answer is asked after it ([`palw_canonical_claim_bond_room_v1`]). Below the fence:
/// the `reserved_exposure` inequality and its sentence, byte for byte.
pub(crate) fn palw_canonical_claim_room_v1(
    bond: &kaspa_consensus_core::palw_producer_v2::PalwProducerBondFactsV2,
    rcore_plus: Option<PalwRcorePlusReadsV1>,
) -> Result<(), String> {
    let Some(reads) = rcore_plus else {
        if bond.reserved_exposure.saturating_add(bond.claim_exposure) > bond.exposure_ceiling {
            return Err(format!(
                "no exposure room for a canonical claim: bond backs {} and one claim needs {} against a ceiling of {}",
                bond.reserved_exposure, bond.claim_exposure, bond.exposure_ceiling
            ));
        }
        return Ok(());
    };
    if let Some(shortfall) = bond.producer_floor_shortfall {
        return Err(format!("holding: {}", PalwProducerHoldV1::BelowProducerFloor { shortfall, floor: reads.producer_floor }));
    }
    if !bond.has_committed_room() {
        return Err(format!(
            "no exposure room for a canonical claim: bond commits {} on the one committed ledger and one claim needs {} against a \
             ceiling of {}",
            bond.committed, bond.claim_exposure, bond.exposure_ceiling
        ));
    }
    if reads.eligible_stake_at_floor == Some(false) {
        return Err(format!("holding: {}", PalwProducerHoldV1::EligibleStakeBelowFloor));
    }
    Ok(())
}

/// **The canonical claim against the fold's own room, once its price is known** (ADR-0152 P6; the
/// P6 review's finding 4).
///
/// The node's price answer (`palw_fp_commitment_price_v1` with the bond) carries `bond_room` —
/// `palw_fp_bond_room_v2`, the FP arm's `ceiling − backed` on the one committed ledger, capability
/// exposure included, at the virtual's DAA and raw depth — and the fold refuses
/// `FreePromptExposureCeiling` exactly when `backed + reserved + rights_reserved > ceiling`. So
/// `reserved + rights_reserved > bond_room` is that refusal, known before the carrier is built or a
/// fee is spent; the estimate before the inference ([`palw_canonical_claim_room_v1`]) prices with
/// the attempt lane's number and can be off either way. A `bond_room` saturated at 0 (backing over
/// the ceiling) refuses any claim that reserves a sompi, as the fold does. `None` — a bond the chain
/// does not hold — says nothing here; the fold's own refusal of that is elsewhere.
///
/// Below the fence this asks nothing, so the canonical lane there is byte-identical to before.
pub(crate) fn palw_canonical_claim_bond_room_v1(
    price: &kaspa_consensus_core::palw_state_v2::PalwFpCommitmentPriceV1,
    bond_room: Option<u128>,
    rcore_plus_active: bool,
) -> Result<(), String> {
    if !rcore_plus_active {
        return Ok(());
    }
    let Some(room) = bond_room else { return Ok(()) };
    let holds = price.reserved.saturating_add(price.rights_reserved);
    if holds > room {
        return Err(format!(
            "the chain would refuse this canonical claim (FreePromptExposureCeiling): it holds {holds} sompi ({} reserved + {} \
             receipt rights) against {room} of room on the one committed ledger",
            price.reserved, price.rights_reserved
        ));
    }
    Ok(())
}

/// **SEAM — ADR-0152 SW-8 / SW-10: would a claim of `class_id` by `executor_bond`, anchored at
/// `candidate_daa`, draw its panel from eligible stake at or above the draw's 875‰ floor?** (the
/// 2026-09-24 audit, on SW-8's anchor-block-only binding.)
///
/// A claim binds its panel at its anchor block alone (SW-8), and the stake-weighted draw refuses
/// (`InsufficientEligibleStake`) where the operators eligible to sit on it weigh less than
/// `PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1` of every operator that could sit — so an attempt accepted
/// while that holds voids at `BindTimeout`, its escrow burned, however honest its work. The audit
/// asked the producer to see that before it spends the inference.
///
/// **Keyed by the executor, not the class alone.** The draw refuses a seat sharing the executor's
/// `pubkey` or `operator_id` (`palw_panel_eligible_bonds_v2`'s executor exclusion), so the eligible
/// weight is this claim's, not the class's: a producer that is itself a large share of the class's
/// eligible stake can pass a class-wide test and still fall under 875‰ on its own claim.
///
/// **No consensus read answers it yet, and this node adds none**: the stake-weighted draw (M4) is
/// not wired — `palw_panel_draw_policy_at` leaves `PalwPanelDrawPolicyV1::stake` at `None` and
/// nothing computes SW-10's refusal — so this answers `None` (unknown), which never holds. The read
/// it needs is a `ConsensusApi` call beside `palw_producer_facts_v2`, keyed `(class_id,
/// executor_bond, candidate_daa)` and answered at the tip's state under the draw policy
/// `palw_panel_draw_policy_at(candidate_daa)` resolves with `stake: Some(PalwPanelStakeDrawV1::V1)`:
/// SW-10's two weights — the eligible operators' for this claim (the draw's own
/// `palw_panel_eligible_bonds_*` filter with the executor's key and operator excluded, one-ledger
/// room included, each weighted `min(posted MSK, weight_cap_msk)`) and the base weight (Active, at
/// the producer floor, registered before the anchor, capable) — or the comparison against
/// `eligible_floor_permille` itself, computed by the one function the draw's refusal calls.
///
/// Liveness: on testnet-12, the only network that arms the fence, heartbeat blocks keep the DAA
/// moving while every producer holds, and the room that makes a bond eligible is released on the
/// DAA clock (the second clock's hold is bounded by `2 × window_court`), so a hold on this answer
/// cannot become the deadlock the floor's epoch-budget exemption exists to prevent.
pub(crate) fn palw_class_eligible_stake_at_floor_v1(
    _session: &kaspa_consensusmanager::ConsensusProxy,
    _class_id: Hash64,
    _executor_bond: &TransactionOutpoint,
    _candidate_daa: u64,
) -> Option<bool> {
    None
}

impl PalwProducerService {
    pub fn new(
        config: PalwProducerConfig,
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
        consensus_config: Arc<Config>,
    ) -> Self {
        // The first startup refusal, kept for `getPalwNodeStatus` (ADR-0122 §6.5) beside the line
        // that logs it.
        let mut refusal: Option<String> = None;
        let (keypair, key_seed) = match kaspa_pq_validator_core::load_validator_seed(&config.key_path) {
            Ok(seed) => (Some(Box::new(libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed))), Some(seed)),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] {err} — production disabled");
                refusal.get_or_insert_with(|| err.to_string());
                (None, None)
            }
        };
        let bond = match parse_outpoint(&config.bond) {
            Ok(o) => Some(o),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] {err} — production disabled");
                refusal.get_or_insert_with(|| err.to_string());
                None
            }
        };
        // The pay address is checked HERE rather than at the first template: a legacy or ECDSA
        // address puts a non-PQ script in the coinbase, the block is dead on arrival, and its
        // reward poisons descendants' fan-out. The RPC path refuses it for the same reason.
        let miner_data = match kaspa_addresses::Address::try_from(config.pay_address.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => {
                warn!("[{PALW_PRODUCER}] pay address is not ML-DSA-87 P2PKH — production disabled");
                refusal.get_or_insert_with(|| "pay address is not ML-DSA-87 P2PKH".to_string());
                None
            }
            Ok(addr) if addr.prefix != config.address_prefix => {
                warn!(
                    "[{PALW_PRODUCER}] pay address is for {} and this node is {} — production disabled",
                    addr.prefix, config.address_prefix
                );
                refusal
                    .get_or_insert_with(|| format!("pay address is for {} and this node is {}", addr.prefix, config.address_prefix));
                None
            }
            Ok(addr) => Some(MinerData::new(kaspa_txscript::pay_to_address_script(&addr), Vec::new())),
            Err(err) => {
                warn!("[{PALW_PRODUCER}] pay address is unusable: {err} — production disabled");
                refusal.get_or_insert_with(|| format!("pay address is unusable: {err}"));
                None
            }
        };
        flow_context.update_palw_runtime(|r| {
            r.producer_bond = config.bond.clone();
            r.producer_class = config.class_id.to_string();
            match &refusal {
                Some(why) => r.set_producer("disabled", why),
                None => r.set_producer("syncing", "starting"),
            }
        });
        // Loaded once — through the SDK, each file by its own container's magic, and once PER
        // PROCESS: the panel names the same list and takes this constructor's holdings rather than
        // mapping and hashing the same file again (`palw_backends::load_class_holdings_v1`). Each
        // file is refused loudly rather than skipped quietly: an operator who passed
        // `--palw-class-artifact` meant this node to produce for that class, and a node that
        // silently fell back to the floor would look like a working producer that never touches
        // the class they deployed 1.7 GiB for.
        let sdk =
            misaka_palw_sdk::PalwClassSdk::builtin_v1(config.court, config.prompt_ids_form, config.network_id.as_bytes().to_vec());
        let class_holdings = crate::palw_backends::load_class_holdings_v1(
            PALW_PRODUCER,
            &sdk,
            &config.class_artifacts,
            config.class_cache_bytes,
            config.class_residency,
        );
        // **A producer class this node can never produce for is refused at startup** (the 2026-09-23
        // route-matrix audit's #1). The first testnet-12 fleet launched with a producer class whose
        // registered root no artifact on it could match, and every producer said "holding" at INFO for
        // a whole deployment while the chain ran on heartbeats — the launch was judged on the clock
        // ticking. A configuration that cannot work is a startup refusal, said where the operator is
        // looking (stdout as well as the log), not a hold.
        //
        // **And a SOFT one, like every other refusal above** (the route-matrix re-audit's #11). This
        // called `process::exit(1)` during daemon construction, which took the whole kaspad down with
        // it — the panel seat, the RPC, the relay, on a public host the public entry node — for a
        // producer misconfiguration, and under systemd's `Restart=` turned it into a crash loop that
        // re-mapped the class artifacts on every lap: the failure shape of the staged-script incident
        // that killed a public node. Now the attempt lane is disabled with the sentence as its status,
        // the ERROR line ends in `— production disabled` (the phrase `misaka-cli`'s nodelog parses),
        // and the node, its seat and this producer's receipt lane keep running.
        let class_refusal = if refusal.is_none()
            && let Some(why) = crate::palw_backends::producer_class_unproducible_v1(&config.class_id, &class_holdings, &consensus_config.params)
        {
            let remedy = "Give it the artifact that class registered (convert it and check it with `palw-class manifest \
                          --check`), name a class one of its artifacts pairs with (`palw-class inspect <artifact>`), or drop \
                          --palw-producer-class to produce for the floor";
            println!(
                "--palw-producer-class: {why}\n\nThis node will not produce attempts for that class (the node, its panel seat \
                 and the receipt lane keep running). {remedy}."
            );
            error!("[{PALW_PRODUCER}] --palw-producer-class: {why} — production disabled");
            error!("[{PALW_PRODUCER}] {remedy}.");
            flow_context.update_palw_runtime(|r| r.set_producer("disabled", &why));
            Some(why)
        } else {
            None
        };
        Self {
            config,
            shutdown: kaspa_utils::triggers::SingleTrigger::default(),
            consensus_manager,
            mining_manager,
            flow_context,
            consensus_config,
            keypair,
            key_seed,
            bond,
            miner_data,
            class_holdings,
            network_draw_lost: std::sync::atomic::AtomicU64::new(0),
            class_refusal,
        }
    }

    /// Sleep `period`, or return `false` the moment `signal_exit` fires — the panel's `tick`,
    /// copied verbatim. Every wait in the worker loop goes through this, which is what makes
    /// shutdown reach code that would otherwise sleep forever (ADR-0068 drill finding F1).
    async fn tick(&self, period: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(period) => true,
            _ = self.shutdown.listener.clone() => false,
        }
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
        // **Armed here or the flag is half a flag.** `resolve_or_chain`'s chain arm goes through
        // the SDK's `resolve_chain_registered`, which refuses unless the SDK ITSELF was armed — so
        // a registry built unarmed made `--palw-chain-classes` inert on the producer no matter
        // what the config said, while the panel's half worked. The two halves must agree, and the
        // agreement is this constructor.
        let net = self.config.network_id.as_bytes().to_vec();
        if self.config.chain_classes {
            crate::palw_backends::PalwBackendRegistry::new_with_chain_classes(
                self.config.court,
                self.config.prompt_ids_form,
                self.class_holdings.clone(),
                net,
            )
        } else {
            crate::palw_backends::PalwBackendRegistry::new(
                self.config.court,
                self.config.prompt_ids_form,
                self.class_holdings.clone(),
                net,
            )
        }
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
            self.flow_context.update_palw_runtime(|r| r.set_producer("disabled", "no signing key"));
            return;
        }
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.network_id.as_bytes(),
            Some(self.config.genesis_hash),
        );
        info!("[{PALW_PRODUCER}] starting (bond={bond}, key={})", self.config.key_path);

        let mut produced = 0u64;
        // The last hold reason actually printed, and when. A producer can hold for hours on one
        // unchanging cause, and repeating it every 5 s buries the line that would explain it: this
        // loop wrote 5,281 identical warnings on a live testnet node while it produced nothing.
        let mut last_hold: Option<String> = None;
        let mut last_hold_at: Option<std::time::Instant> = None;
        // When this producer last made progress — started, or produced a block. A hold measured from
        // here past `PALW_PRODUCER_STARVED_AFTER` is logged as the failure it is (`log_producer_hold_v1`).
        let mut last_progress_at = std::time::Instant::now();
        // Lost draws are the ordinary state and log nothing each; the count is what says whether
        // the lottery this node is drawing can be won at all (testnet-11 5f: 8 h at 40 % CPU and
        // not one line, against a chance per draw of 1e-7).
        let mut draws: u64 = 0;
        let mut draws_reported_at: Option<std::time::Instant> = None;
        // Where the bucket walk stands on the template last drawn against — see `produce_one`.
        let mut cursor: Option<(Hash64, u64)> = None;
        // The last receipt-lane failure printed, and when — the same once-per-change-then-every-
        // five-minutes rule as the holds. The receipt lane is now tried while the attempt lane
        // holds, on that branch's 5 s cadence, so an unchanging failure must not repeat per tick.
        let mut last_receipt_err: Option<String> = None;
        let mut last_receipt_err_at: Option<std::time::Instant> = None;
        loop {
            if !self.tick(std::time::Duration::from_millis(200)).await {
                break;
            }
            // The retention prune is not this loop's any more: `palw_retention::PalwRetentionJanitor`
            // runs it on every node, producing or not, once a minute (it ran here, where a node that
            // did not produce never pruned, and where it ran every 300 draws rather than every minute).
            let session = self.consensus_manager.consensus().unguarded_session();
            if session.async_is_consensus_in_transitional_ibd_state().await {
                self.flow_context.update_palw_runtime(|r| r.set_producer("syncing", "the node is in IBD"));
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
                let has_peers = self.flow_context.hub().has_peers();
                let participation_allowed = self.flow_context.is_consensus_participation_allowed();
                if !(self.config.enable_unsynced_mining && has_peers && participation_allowed) {
                    // Visible, once per change and then every five minutes — this was a `trace!`,
                    // and a pool slot on testnet-11 sat behind it for hours at 5 % CPU while its
                    // operator read "registered, healthy chain view, drawing nothing" (5f card §10b).
                    // The three predicates are the whole diagnosis: a sink older than the sync
                    // window needs `--enable-unsynced-mining`; no peers or a closed participation
                    // gate hold regardless of that flag.
                    let detail = format!(
                        "the mining rule engine says this node should not mine [enable_unsynced_mining={} peers={} participation_allowed={}]",
                        self.config.enable_unsynced_mining, has_peers, participation_allowed
                    );
                    self.flow_context.update_palw_runtime(|r| r.set_producer("holding", &detail));
                    let stale = last_hold_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                    if last_hold.as_deref() != Some(detail.as_str()) || stale {
                        log_producer_hold_v1(&detail, false, last_progress_at.elapsed());
                        last_hold = Some(detail);
                        last_hold_at = Some(std::time::Instant::now());
                    }
                    if !self.tick(std::time::Duration::from_secs(2)).await {
                        break;
                    }
                    continue;
                }
                if produced == 0 {
                    info!(
                        "[{PALW_PRODUCER}] the sink is older than the sync window (a fresh chain always is) and --enable-unsynced-mining is set: producing anyway, with peers connected and participation open"
                    );
                }
            }
            let Some(facts) = session.palw_producer_facts_v2(self.config.class_id, Some(bond)) else {
                let detail = format!("this network has no ConsensusV2 facts for class {} — nothing to produce", self.config.class_id);
                self.flow_context.update_palw_runtime(|r| r.set_producer("holding", &detail));
                let stale = last_hold_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                if last_hold.as_deref() != Some(detail.as_str()) || stale {
                    log_producer_hold_v1(&detail, false, last_progress_at.elapsed());
                    last_hold = Some(detail);
                    last_hold_at = Some(std::time::Instant::now());
                }
                if !self.tick(std::time::Duration::from_secs(5)).await {
                    break;
                }
                continue;
            };
            // **The receipt lane, first — and ahead of the attempt lane's readiness.** A certified
            // free-prompt claim whose quantum wins its draw is a receipt block waiting to be mined,
            // and it needs no nonce search — the quantum ticket is the lottery, already decided at
            // the claim's beacon. So it is cheaper than an attempt and, unlike one, it turns a claim
            // over into weight. Tried before the attempt for both reasons.
            //
            // It used to be tried only once `ready_to_produce` passed, so a bond whose ATTEMPT lane
            // held — the exposure ceiling full, or the attempt class's epoch budget spent — never
            // spent its quanta either. Neither hold is about a receipt (it opens no claim and draws
            // on no attempt budget: `ready_to_spend_receipts`), and a winning quantum is spendable
            // only inside its use window, so the hold did not delay a free-prompt executor's pay, it
            // forfeited it. The bond that fills its ceiling is exactly the one committing
            // free-prompt claims.
            // ADR-0135: the registry's class-local gate, read before a draw is built. Not a rule of
            // this node's — the fold refuses what it refuses — but a draw for a HELD class or one at
            // its inflight cap is a block the chain will not take, so it is not built.
            if let Some(detail) = self.registry_holds_class(&session) {
                self.flow_context.update_palw_runtime(|r| r.set_producer("holding", &detail));
                let stale = last_hold_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                if last_hold.as_deref() != Some(detail.as_str()) || stale {
                    log_producer_hold_v1(&detail, false, last_progress_at.elapsed());
                    last_hold = Some(detail);
                    last_hold_at = Some(std::time::Instant::now());
                }
                if !self.tick(std::time::Duration::from_secs(5)).await {
                    break;
                }
                continue;
            }
            if facts.ready_to_spend_receipts(&self.verification_key()).is_ok() {
                match self.produce_receipt(&session, network_domain, bond, miner_data.clone()).await {
                    Ok(Some(hash)) => {
                        produced += 1;
                        last_progress_at = std::time::Instant::now();
                        info!("[{PALW_PRODUCER}] produced RECEIPT block #{produced} {hash} (a certified free-prompt claim, mined)");
                        self.flow_context.update_palw_runtime(|r| {
                            r.receipt_blocks += 1;
                            r.last_block = hash.to_string();
                            r.last_block_unix = kaspa_p2p_flows::flow_context::unix_now_secs();
                        });
                        last_receipt_err = None;
                        continue;
                    }
                    Ok(None) => {} // No winning quantum right now; fall through to an attempt.
                    Err(err) => {
                        let stale = last_receipt_err_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                        if last_receipt_err.as_deref() != Some(err.as_str()) || stale {
                            warn!("[{PALW_PRODUCER}] receipt: {err}");
                            last_receipt_err = Some(err);
                            last_receipt_err_at = Some(std::time::Instant::now());
                        }
                    }
                }
            }
            // Route-matrix #11: a class this node can never produce for disables the attempt lane
            // alone — the receipt lane above keeps spending this bond's quanta.
            if let Some(why) = &self.class_refusal {
                self.flow_context.update_palw_runtime(|r| r.set_producer("disabled", why));
                if !self.tick(std::time::Duration::from_secs(5)).await {
                    break;
                }
                continue;
            }
            // **ADR-0152 P6: past `palw_rcore_plus` the pre-check reads the ledger the fold reads** —
            // the producer floor and the committed room, resolved at the candidate's DAA (the one
            // `facts` were built for) — and below it `ready_to_produce` unchanged. See
            // `palw_producer_ready_v1`.
            let rcore_plus = palw_rcore_plus_reads_v1(&self.consensus_config.params, &session, facts.class_id, &bond, facts.daa_score);
            if let Err(hold) = palw_producer_ready_v1(&facts, &self.verification_key(), rcore_plus) {
                // **The reason alone is not a diagnosis.** "this class's epoch budget is already
                // spent" is what a class that exhausted its cap says AND what a class that was
                // never granted one says, and those are opposite problems: the first resolves at
                // the next boundary, the second is a class holding share with no entry in the
                // budget table. Telling them apart took reading consensus source; the numbers that
                // separate them are right here, so carry them.
                let detail = palw_producer_hold_detail_v1(&facts, &hold, rcore_plus.is_some());
                // Once per change, then no more than once every 5 minutes while it persists: a
                // hold that never changes is still worth seeing in a log an operator scrolls.
                self.flow_context.update_palw_runtime(|r| r.set_producer("holding", &detail));
                let stale = last_hold_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300));
                if last_hold.as_deref() != Some(detail.as_str()) || stale {
                    log_producer_hold_v1(&detail, true, last_progress_at.elapsed());
                    last_hold = Some(detail);
                    last_hold_at = Some(std::time::Instant::now());
                }
                if !self.tick(std::time::Duration::from_secs(5)).await {
                    break;
                }
                continue;
            }
            // Cleared so the next hold, whatever it is, prints immediately rather than being
            // suppressed as a repeat of one the node has since recovered from.
            last_hold = None;
            last_hold_at = None;
            self.flow_context.update_palw_runtime(|r| r.set_producer("drawing", ""));
            let outcome = self.produce_one(&session, &facts, network_domain, bond, miner_data.clone(), &mut cursor).await;
            let network_lost = self.network_draw_lost.load(std::sync::atomic::Ordering::Relaxed);
            self.flow_context.update_palw_runtime(|r| {
                r.draws = draws + u64::from(outcome.is_ok());
                r.network_lost = network_lost;
                r.last_draw_unix = kaspa_p2p_flows::flow_context::unix_now_secs();
            });
            match outcome {
                Ok(Some((hash, claim))) => {
                    draws += 1;
                    produced += 1;
                    last_progress_at = std::time::Instant::now();
                    info!(
                        "[{PALW_PRODUCER}] produced block #{produced} {hash} (class ticket under target; Layer-0 as the fence reads it)"
                    );
                    // ADR-0122 Decision 8: the work's own line, by the id every later stage of it
                    // carries — the claim id is the attempt id — beside the prose line above.
                    let id = claim.to_string();
                    info!(
                        "[{PALW_PRODUCER}] event work={} lane=block stage=SUBMITTED block={hash} claim={id}",
                        &id[..16.min(id.len())]
                    );
                    self.flow_context.update_palw_runtime(|r| {
                        r.produced_blocks = produced;
                        r.last_block = hash.to_string();
                        r.last_block_unix = kaspa_p2p_flows::flow_context::unix_now_secs();
                    });
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
                Ok(None) => {
                    // A lost draw. The next call draws the next bucket, or a fresh template. Once
                    // every five minutes, the count and the odds — so a lottery that cannot be won
                    // reads as one from the log an operator already watches, not from a silence.
                    draws += 1;
                    if draws_reported_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(300)) {
                        let class_p = facts.class_target as f64 / u128::MAX as f64;
                        let network_lost = self.network_draw_lost.load(std::sync::atomic::Ordering::Relaxed);
                        info!(
                            "[{PALW_PRODUCER}] {draws} draws this run, {produced} produced, {network_lost} won the class ticket and lost the network draw against bits; class ticket p = {class_p:.3e} per draw (1 in {:.3e})",
                            1.0 / class_p.max(f64::MIN_POSITIVE)
                        );
                        draws_reported_at = Some(std::time::Instant::now());
                    }
                }
                Err(err) => warn!("[{PALW_PRODUCER}] {err}"),
            }
        }
        self.flow_context.update_palw_runtime(|r| r.set_producer("stopped", ""));
        info!("[{PALW_PRODUCER}] stopping ({produced} blocks this run)");
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
        // A win licenses a block only inside its use window (invariant F14), and the spendable
        // list does not filter on it — a quantum whose window closed stays in the list, unspent,
        // forever. Picking one built a template, signed it and had the chain refuse it on every
        // pass, and this lane now runs on every pass the attempt lane holds, too. So a win is
        // taken only while the next block could still carry it: a template carries the virtual's
        // DAA score, which is the one the window is checked against.
        let next_daa = session.get_virtual_daa_score();
        let Some(win) = spendable.into_iter().find(|q| q.wins && next_daa <= q.spend_deadline_daa) else {
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

    /// One template, one inference, one draw (ADR-0072).
    ///
    /// There is no nonce search any more. Both lotteries — the class ticket and the Layer-0 digest
    /// against `bits` — are functions of the execution commitment, which no nonce inside the
    /// anchor's bucket and no timestamp moves; the ADR-0071 audit measured the search this
    /// replaced at four million free draws per inference. What a producer re-rolls is the
    /// inference itself: the next bucket is a different job. `cursor` is where that walk stands.
    /// ADR-0135: why the registry would refuse this class's next claim now, if it would — `None`
    /// below the fence, during the activation grace, for the base class, and for a class the
    /// registry admits with room in flight.
    fn registry_holds_class(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Option<String> {
        let read = session.palw_model_registry_v1()?;
        if !read.active || read.tip_daa < read.grace_until_daa {
            return None;
        }
        let class = read.classes.iter().find(|c| c.class_id == self.config.class_id)?;
        if class.is_base_class {
            return None;
        }
        let row = class.row.as_ref()?;
        if !row.state.admits_claims() {
            return Some(format!("the model registry holds class {}: {:?} since span {}", class.class_id, row.state, row.since_span));
        }
        // 2026-09-24 DoS audit review of #11: ask the question the fold's class gate asks — the
        // panel budget where ADR-0137 D5 governs, the inflight cap otherwise — on a read that counts
        // free-prompt claims past the audit fence. An attempt the gate refuses at step 4
        // disqualifies its whole block, so the producer asks first. Past `palw_audit_2026_09_23`
        // the read's `panel_room` is the fold's rate room (2026-09-24 audit #4), and for a class
        // HELD TO FINAL (ADR-0152's C7, `palw_panel_held_to_final_v1`: a window of at least 1,000
        // spans, testnet-12's 2M row) no more than its static inflight cap leaves — c_2M = 1 — so
        // one number answers both questions the gate asks.
        if read.panel_room_enforced {
            if class.panel_room == 0 {
                return Some(format!("class {} has no panel room left in the network's verification budget", class.class_id));
            }
            return None;
        }
        if class.inflight_now >= row.profile.max_inflight_claims {
            return Some(format!(
                "class {} is at the registry's inflight cap ({} of {})",
                class.class_id, class.inflight_now, row.profile.max_inflight_claims
            ));
        }
        None
    }

    async fn produce_one(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        facts: &PalwProducerFactsV2,
        network_domain: Hash64,
        bond: TransactionOutpoint,
        miner_data: MinerData,
        cursor: &mut Option<(Hash64, u64)>,
    ) -> Result<Option<(kaspa_consensus_core::BlockHash, Hash64)>, String> {
        let mut template = self
            .mining_manager
            .clone()
            .get_block_template(session, miner_data)
            .await
            .map_err(|e| format!("no block template: {e}"))?;
        // **Either attempt id, because the template's id is the CHAIN's answer** (ADR-0072 SA-4).
        //
        // Spelled `!= POW_ALGO_ID_PALW_COMMITTED_V2` this was a stall with a deploy date: past an
        // armed `palw_attempt_activation` the virtual processor declares algo-9 on every template,
        // and this returned an error for every one of them, on every node, from the same DAA score
        // onward. It could not self-heal either — `PalwRulesetV2::validate` requires the bundle's
        // `algorithm_id` to be 6, so `palw_required_algo_id` is never 9 and the comparison could
        // never come back. The fence exists to be crossed; the producer must be able to build on
        // the far side of it.
        //
        // What this still refuses is a template from a network that is not running an attempt lane
        // at all — a kHeavyHash or Argon2id template — which is the check that was meant.
        if !template_declares_an_attempt_lane(template.block.header.pow_algo_id) {
            return Err(format!("this network declares algo {} — not a ConsensusV2 lane", template.block.header.pow_algo_id));
        }
        // The job is the TEMPLATE's AND the bucket's. See `base0_rc_job_anchor_v1` for why it is
        // not the challenge's, and `PALW_TICKET_NONCE_BUCKET_LOG2` for what a bucket is.
        //
        // **The bucket is the draw, and it walks.** The engine is deterministic, so
        // (template, bucket) → job → execution → ticket is a function: a bucket this template has
        // already lost stays lost, and re-running it is the one thing a producer must never do.
        // The cursor is what stops that across calls — a template with the same pre-PoW hash
        // (same parents; the timestamp is outside it) resumes at the bucket after the last one
        // drawn, and any other template starts at zero. Bucket 2^42 would push the nonce out of
        // its 64 bits; a template that has lost that many draws has long since gone stale.
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&template.block.header);
        let nonce_bucket = match *cursor {
            Some((at, next)) if at == pre_pow && next < (1u64 << (64 - PALW_TICKET_NONCE_BUCKET_LOG2)) => next,
            _ => 0,
        };
        *cursor = Some((pre_pow, nonce_bucket + 1));
        let nonce = nonce_bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
        let anchor = base0_rc_job_anchor_v1(network_domain, pre_pow, facts.class_id, &bond, nonce_bucket);

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
        //
        // **And on a chain whose data-availability court is IN FORCE it is a refusal, not a
        // warning** (mainnet audit 2026-09-06, C-5). Past `palw_da_court` an accusation naming one
        // trace event is admissible against any non-terminal claim, and silence past the disclose
        // window takes `claim.reserved` and the escrowed reward. Producing a claim this build
        // cannot answer for is therefore not "unpoliceable", it is a funded, evidence-free loss the
        // producer chose to underwrite. Not producing costs this node a block; producing costs it
        // collateral it can never defend, so the fail-closed direction is the one that does not
        // destroy the operator's bond. Once every shipped family answers (this release), the arm is
        // unreachable for the classes this tree ships and fires only for a build genuinely holding
        // no responder — which is the case it is for.
        //
        // The DAA is the TEMPLATE's, the one clock this function already resolves against: it is
        // the score `trace_retention_daa` is set from below, and the score the claim this call is
        // about will be folded at.
        if !backend.supports_court() {
            self.warn_once_no_court(facts.class_id);
            if palw_da_court_in_force_v1(&self.consensus_config, template.block.header.daa_score) {
                return Err(format!(
                    "this node will not produce for class {}: its backend declares no court responder and this chain's \
                     data-availability court is in force, so any claim produced here could be defaulted for the price of \
                     one accusation with nothing this build can file in answer (ADR-0062 D3)",
                    facts.class_id
                ));
            }
        }
        // **ADR-0093 as built: the same refusal for the dissection's turn.** A claim of a class with
        // a fused attention site can be disputed down to a fused leaf, where the responder owes a
        // root claim and — with `palw_court_responder_coverage` retired, unarmed on every preset —
        // silence is a conviction. A backend that cannot dissect its class (a fused tile wider than
        // a head, or a family with no evidence verb) would be underwriting a claim it can never
        // defend there; on a chain whose k-ary court is armed that is a refusal, as the DA court's is.
        if backend.has_fused_site()
            && !backend.supports_dissection()
            && self.consensus_config.params.palw_kary_court_active_at(template.block.header.daa_score)
        {
            return Err(format!(
                "this node will not produce for class {}: its fused attention site cannot be dissected by this build, and a \
                 claim disputed down to that leaf is convicted by the silence it could not answer (ADR-0093)",
                facts.class_id
            ));
        }
        // **Through the seam.** The backend is the class's execution path; this
        // function no longer knows which family it is producing for, which is what lets a second
        // one exist. Which backend it is, is the CHAIN's answer (`facts.terms.family`).
        let (job, prompt) = backend.job_for_anchor(anchor).map_err(|e| format!("the job this template implies: {e}"))?;
        // **ADR-0117: past the fence the draw is one forward** — the canonical job without its
        // decode calls, read at THIS block's height through the one spelling the seats replay it
        // with (`palw_attempt_job_v1`).
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(
            job,
            self.consensus_config.params.palw_prefill_draw_active_at(template.block.header.daa_score),
        );
        // **Off the async worker.** The inference and the nonce grind are pure CPU with no await in
        // them, and they ran inline on the shared `AsyncRuntime` — pinning one tokio worker thread.
        // Trivial at genesis difficulty and not trivial at all once the retarget pulls the search
        // out to the 120 s cadence, at which point that thread is busy essentially all the time and
        // every other service on the runtime is short one worker.
        // **Fail closed on the attempt's own working set, and RESERVE it** (ADR-0151 follow-up,
        // items 1–3). The K/V cache of a held-context attempt is sized by the prefill, not by the
        // artifact: the 2M row's canonical job prefills 262,143 positions and its cache is 14 GiB of
        // `i32` rows (7 GiB of `i16`). The run that found this had a 5.25 GiB share, no gate, and a
        // dmesg line. The need is the producer role's resource profile — the same derivation the
        // panel's pre-check and the court read — and it is taken from the process-wide ledger
        // before a byte is allocated, so a seat's replay in this process cannot start beside it on
        // the strength of a `MemAvailable` that has not yet seen it. A refusal names the need, the
        // bounds and every reservation already held, and holds — the same shape as every other
        // producer hold. The reservation lives until this function returns, on every path.
        // The holding is found through the door the backend came through (the route-matrix
        // re-audit's #5): a chain-registered class's artifact was priced at zero bytes off the tables.
        let need = self.backends().role_memory_need_for_backend_or_chain_v1(
            backend.as_ref(),
            facts.class_id,
            facts.artifact_root,
            Some(&job),
            kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::Producer,
            |id| if self.config.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None },
        );
        let reserved = crate::palw_memory_ledger::host_ledger_v1().reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "producer", class_id: facts.class_id, job: job.context_hash() },
            need.total_bytes(),
        );
        // The ledger as it stands after this decision, where `getPalwNodeStatus` reads it — on the
        // refusal as much as on the grant, since the refusal is the state an operator asks about.
        self.flow_context.update_palw_runtime(crate::palw_backends::publish_memory_ledger_v1);
        let _reserved = reserved.map_err(|refusal| {
            format!(
                "this attempt needs {} for a {}-token prefill and {refusal} — holding rather than being OOM-killed; a \
                 narrower class, a host with the memory, or the running duty finishing, produces",
                need.describe(),
                prompt.len()
            )
        })?;
        let (job_for_blocking, prompt_for_blocking) = (job.clone(), prompt.clone());
        let tamper = self.config.drill_tamper_leaf;
        // ADR-0112 Decision 8: what one draw reads from storage, printed beside the draw. The
        // number that said the fleet's draws were page faults, made a line an operator can watch.
        let storage_before = crate::palw_backends::storage_snapshot_v1(&self.class_holdings);
        let draw_started = std::time::Instant::now();
        let (run, answer_ids) = tokio::task::spawn_blocking(move || {
            let run = match tamper {
                None => backend.execute(&job_for_blocking, &prompt_for_blocking),
                Some(leaf) => backend.execute_with_injected_fault(&job_for_blocking, &prompt_for_blocking, leaf),
            }?;
            // The answer's ids, read back off the capture by the family that wrote it — for the
            // attempt-lane answer envelope (ADR-0084 Decision 4) staged beside the material.
            let answer_ids = backend.fp_committed_output_ids(&run.material);
            Ok::<_, String>((run, answer_ids))
        })
        .await
        .map_err(|e| format!("the execution task did not finish: {e}"))??;
        let storage_read_mib = crate::palw_backends::log_draw_storage_v1(PALW_PRODUCER, &storage_before, &self.class_holdings);
        let draw_millis = draw_started.elapsed().as_millis() as u64;

        // Every field is fixed now: the roots are the execution's, the six chain facts are
        // `facts`', and the challenge binds the position — this template, this timestamp, this
        // nonce — and moves neither lottery. The draw was made the moment the inference finished.
        let timestamp = template.block.header.timestamp;
        let mut attempt = PalwAttemptUnsignedV2 {
            // **The current version, on both ids this producer can build for** (ADR-0072 SA-3).
            //
            // `PalwAttemptLaneV1::attempt_version` is the current version on `Unfenced` (every
            // shipped preset) and on `ExecutionArm` (algo-9, past an armed fence), so a template
            // declaring either id wants exactly this number. The third arm — `LegacyArm`, an armed
            // network BELOW its fence — wants the pre-ADR-0072 version, and this producer cannot
            // build for it: the pre-ADR-0072 lottery arithmetic those blocks were mined under was
            // deleted at Relaunch 5's re-genesis, so a legacy envelope could not pass PoW here
            // whatever version it declared. That is the honest limit of §3 option (b), and it is
            // why arming this fence is safe at genesis (`ForkActivation::always()`, where
            // `LegacyArm` is unreachable) and not safe at a future height on a chain with real
            // pre-ADR-0072 history.
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, timestamp, nonce, facts.class_id, &bond),
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
            // it can keep. Derived, not chosen, and PINNED by admission (ADR-0072 Decision 8):
            // this header's own DAA score plus the network's lattice windows.
            trace_retention_daa: template.block.header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        // A dummy of the right length so the shape gate sees the real wire size at the draw. The
        // signature is outside the priced bytes, so it changes neither lottery — it is made once,
        // over the attempt id, after the draw is known to have won.
        let sig_len = self
            .keypair
            .as_ref()
            .map(|kp| {
                libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &[0u8; 64], PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0u8; 32])
                    .map(|s| s.as_ref().len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // **The draw.** The class lottery first, then the network's against `bits` — both are
        // functions of the one execution, so both are decided here, once, with no search and no
        // blocking task: two hashes, not a loop.
        let mut class_won = false;
        let search: Option<(u64, PalwAttemptUnsignedV2)> = {
            let ticket = class_ticket_v3(&attempt, anchor);
            if ticket > facts.class_target {
                trace!("[{PALW_PRODUCER}] bucket {nonce_bucket}: the class draw lost");
                None
            } else {
                class_won = true;
                let mut header = template.block.header.clone();
                header.nonce = nonce;
                header.palw_commitment =
                    PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature: vec![0u8; sig_len] }.encode_wire();
                let state = kaspa_pow::StateLayer0::new(&header, self.config.network_id.as_bytes());
                // ADR-0132 S: past the single lottery the chain admits an attempt header's digest
                // unconditionally, so the producer must not throw the forward away on a network
                // draw the chain no longer runs (the 2026-09-18 drill: every producer kept counting
                // "won the class ticket and lost the network draw" past the fence).
                let single_lottery = self.consensus_config.params.palw_single_lottery_at(header.daa_score);
                if state.check_pow_layer0_v2(nonce, single_lottery).map(|(ok, _)| ok).unwrap_or(false) {
                    Some((nonce, attempt.clone()))
                } else {
                    self.network_draw_lost.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    trace!("[{PALW_PRODUCER}] bucket {nonce_bucket}: the class draw won, the network draw lost");
                    None
                }
            }
        };
        // ADR-0132: one draw, both lotteries, into the node's counters — what the chain credits
        // (a class win) against what the forward cost (every draw, its time, its storage reads).
        self.config.telemetry.producer_draw(facts.class_id, class_won, search.is_some(), draw_millis, storage_read_mib.unwrap_or(0));
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
            // The last bracket of the attempt (ADR-0151 follow-up): the material is on disk, the
            // execution's buffers are gone, and this is what the process holds after one attempt.
            crate::palw_backends::log_memory_phase_v1(PALW_PRODUCER, "attempt returned and its material retained", crate::palw_backends::armed_ram_scale_pub_v1());
            // **And the answer envelope beside it** (ADR-0084 Decision 4): the anchor, the prompt
            // the anchor derives, and the answer's ids — what a seat on the interval arm needs when
            // this capture (748 MB on the graph-v5 class) does not fit the transport. Best-effort:
            // a missing envelope costs the serving node one decode of the capture on the first
            // pull, not the claim.
            if let Some(ids) = answer_ids.as_deref() {
                let prompt_ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
                let answer = kaspa_consensus_core::palw_attempt_v2::palw_attempt_answer_encode_v1(anchor, &prompt_ids, ids);
                let path = palw_retained_answer_path(&self.config.retention_dir, &message);
                let tmp = path.with_extension("answer.partial");
                if let Err(e) = std::fs::write(&tmp, &answer).and_then(|()| std::fs::rename(&tmp, &path)) {
                    warn!("[{PALW_PRODUCER}] cannot retain the answer envelope for attempt {message}: {e}");
                }
            }
            template.block.header.nonce = nonce;
            template.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature }.encode_wire();
            template.block.header.finalize();
            let block: kaspa_consensus_core::block::Block = template.block.clone().to_immutable();
            let hash = block.hash();
            // **The block first, the announcement after** (ADR-0084 Decision 3). This broadcast
            // used to come before the submit, and on 5f a 748 MB material announced to five peers
            // occupied every link for longer than the flow window: the routers were torn down and
            // the block — queued behind the bytes — never entered the DAG (card §6l). The material
            // is retained and SERVED (the pull, answered with the answer envelope when the capture
            // does not fit); the announcement is a courtesy the transport skips over the cap.
            self.flow_context
                .submit_rpc_block(session, block)
                .await
                .map_err(|e| format!("the chain refused a block this node produced: {e}"))?;
            self.flow_context.broadcast_palw_material(message, material).await;
            return Ok(Some((hash, message)));
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
        // The half that was missing (ADR-0068 drill finding F1): without the trigger the trace
        // above was the whole implementation, the worker loop never learned, and the AsyncRuntime
        // join waited forever on a future that could not finish.
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_PRODUCER);
            Ok(())
        })
    }
}

/// **Can this producer build on the lane the template declares?**
///
/// A free function because the answer has to be checkable: as a `!=` against one constant inside an
/// async method it was a network-wide stall with a deploy date. `build_block_template` writes
/// `PalwAttemptLaneV1::attempt_algo_id()` for the network's fence at the virtual DAA score, so past
/// an armed `palw_attempt_activation` every template on every node declares algo-9 — and a producer
/// that only knows algo-6 stops producing everywhere at the same height, with no configuration that
/// could bring it back (`PalwRulesetV2::validate` pins the bundle's `algorithm_id` at 6, so
/// `palw_required_algo_id` is never 9).
fn template_declares_an_attempt_lane(pow_algo_id: u8) -> bool {
    kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(pow_algo_id)
}

#[cfg(test)]
mod attempt_lane_tests {
    use super::template_declares_an_attempt_lane;
    use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_RECEIPT_V3, PalwAttemptLaneV1};

    /// **The producer accepts every id the template builder can declare** — asked of the lane
    /// resolver, not of a list, because the list is what went stale.
    ///
    /// Red the moment this is a comparison against a single attempt id: `ExecutionArm`'s id is 9.
    #[test]
    fn the_producer_builds_on_every_lane_the_chain_can_declare() {
        for lane in [PalwAttemptLaneV1::Unfenced, PalwAttemptLaneV1::LegacyArm, PalwAttemptLaneV1::ExecutionArm] {
            let declared = lane.attempt_algo_id();
            assert!(
                template_declares_an_attempt_lane(declared),
                "{lane:?} makes the template declare algo-{declared}; refusing it stops production on every node at the fence"
            );
        }
        // And it still refuses what the check was actually for: a template from a network that is
        // not running an attempt lane at all.
        assert!(!template_declares_an_attempt_lane(POW_ALGO_ID_KHEAVYHASH));
        // The receipt lane is a different producer path (`produce_one_receipt`), so this one must
        // not claim it.
        assert!(!template_declares_an_attempt_lane(POW_ALGO_ID_PALW_RECEIPT_V3));
    }
}

#[cfg(test)]
mod retention_tests {
    use super::{PALW_RETENTION_HORIZON, free_prompt_retention_is_owed, retained_capture_prune_due_v1};
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2 as P, PalwClaimSourceV2 as S, PalwVoidReasonV2 as R};

    /// **A live free-prompt claim's capture outlives the wall-clock horizon; nothing else does.**
    /// Every phase in which a seat, a redrawn panel, a challenger or an accuser can still ask the
    /// executor for an opening keeps the file; `Final` and every void release it; an attempt claim
    /// is never kept past the horizon by this rule, whatever its phase.
    #[test]
    fn a_live_free_prompt_claim_keeps_its_capture_and_nothing_else_does() {
        let fp = S::FreePrompt { quanta: 8, spent: Default::default() };
        for live in [P::Provisional, P::PanelBound { bound_daa: 10 }, P::ReceiptLicensed { licensed_daa: 20 }] {
            assert!(free_prompt_retention_is_owed(&fp, &live), "{live:?} can still be asked about");
            assert!(!free_prompt_retention_is_owed(&S::Attempt, &live), "an attempt claim keeps the clock");
        }
        assert!(!free_prompt_retention_is_owed(&fp, &P::Final { final_daa: 30 }));
        for reason in [R::BindTimeout, R::ReceiptTimeout, R::CourtFraud, R::ProducerWithholding] {
            assert!(!free_prompt_retention_is_owed(&fp, &P::Voided { voided_daa: 40, reason }));
        }
    }

    /// **An attempt capture goes at its own short horizon, whatever its phase; a live free-prompt
    /// capture never goes; everything else goes at 48 h.** The boundary is exact on both clocks, and
    /// an unreadable age is an old file, as it always was.
    #[test]
    fn an_attempt_capture_is_pruned_at_its_own_horizon_and_a_live_free_prompt_one_is_not() {
        use std::time::Duration;
        let hour = Duration::from_secs(3600);
        let fp = S::FreePrompt { quanta: 8, spent: Default::default() };
        let phases = [
            P::Provisional,
            P::PanelBound { bound_daa: 10 },
            P::ReceiptLicensed { licensed_daa: 20 },
            P::Final { final_daa: 30 },
            P::Voided { voided_daa: 40, reason: R::CourtFraud },
        ];
        for phase in &phases {
            // An attempt claim: due at the horizon it is given, to the second, in every phase — a
            // court asking later is answered from a replay.
            assert!(!retained_capture_prune_due_v1(Some(hour - Duration::from_secs(1)), Some((&S::Attempt, phase)), hour));
            assert!(retained_capture_prune_due_v1(Some(hour), Some((&S::Attempt, phase)), hour), "{phase:?}");
            // The attempt horizon never reaches a free-prompt capture.
            assert!(!retained_capture_prune_due_v1(Some(hour), Some((&fp, phase)), hour), "{phase:?}");
        }
        // A free-prompt capture past 48 h: kept while the chain can still ask, pruned once it cannot.
        let old = PALW_RETENTION_HORIZON;
        for live in &phases[..3] {
            assert!(!retained_capture_prune_due_v1(Some(old * 10), Some((&fp, live)), hour), "{live:?} is still owed");
        }
        for settled in &phases[3..] {
            assert!(!retained_capture_prune_due_v1(Some(old - Duration::from_secs(1)), Some((&fp, settled)), hour));
            assert!(retained_capture_prune_due_v1(Some(old), Some((&fp, settled)), hour), "{settled:?}");
        }
        // A claim the chain does not know keeps the 48 h clock; an unreadable age counts as old.
        assert!(!retained_capture_prune_due_v1(Some(old - Duration::from_secs(1)), None, hour));
        assert!(retained_capture_prune_due_v1(Some(old), None, hour));
        assert!(retained_capture_prune_due_v1(None, None, hour));
        assert!(retained_capture_prune_due_v1(None, Some((&S::Attempt, &P::Provisional)), hour));
    }
}

#[cfg(test)]
mod tests {
    use super::palw_da_court_in_force_v1;

    /// **A producer misconfiguration never takes the node down** (the route-matrix re-audit's #11).
    /// The unproducible-class refusal called `process::exit(1)` from the constructor, which under
    /// systemd's `Restart=` is a crash loop of the whole kaspad — seat, RPC and public entry node with
    /// it. Every startup refusal here is soft now; the class one says `— production disabled`, the
    /// phrase `misaka-cli`'s nodelog reads a disabled producer by, and the worker holds on it after
    /// the receipt lane rather than returning.
    #[test]
    fn a_producer_class_refusal_disables_the_attempt_lane_and_never_exits_the_process() {
        let whole = include_str!("palw_producer.rs");
        let production = &whole[..whole.find("#[cfg(test)]\nmod tests {").expect("the test module")];
        assert!(!production.contains("std::process::exit("), "a producer refusal must not exit the node");
        assert!(production.contains("--palw-producer-class: {why} — production disabled"), "the nodelog phrase");
        let worker = &production[production.find("pub async fn worker(").expect("the worker")..];
        let receipt = worker.find("self.produce_receipt(").expect("the receipt lane");
        let hold = worker.find("if let Some(why) = &self.class_refusal").expect("the attempt lane's hold");
        let attempt = worker.find("palw_producer_ready_v1(&facts,").expect("the attempt lane");
        assert!(receipt < hold && hold < attempt, "the refusal holds the attempt lane after the receipt lane has run");
    }
    use kaspa_consensus_core::config::Config;
    use kaspa_consensus_core::config::params::{devnet_shipped_params, palw_rc_shipped_params};

    /// **A producer's refusal to make an undefendable claim turns on the CHAIN's fence, at the
    /// boundary the chain uses** (ADR-0062 D3; mainnet audit 2026-09-06, C-5).
    ///
    /// The refusal above is unreachable in a unit test — it needs a template, a session and a
    /// resolved backend — so what is pinned here is the whole of its condition: the one predicate
    /// it and the panel's duty scan both read. The rule is that this is the ruleset's own
    /// `palw_da_court`, resolved at the block's DAA and nowhere reconstructed, so a courtless
    /// build is refused on exactly the blocks where an accusation against it would be admissible
    /// and on no others. The boundary is the assertion that can fail: an off-by-one here is a
    /// producer that mines one block it cannot defend, or refuses one it could.
    ///
    /// It cannot pass vacuously — testnet-11 schedules the court rather than arming it, so both
    /// answers are demanded of the same ruleset.
    #[test]
    fn the_da_court_is_in_force_exactly_from_the_height_the_ruleset_schedules() {
        let rc = Config::new(palw_rc_shipped_params());
        let scheduled = rc.params.palw_da_court.expect("testnet-11 schedules the data-availability court").daa_score();
        assert!(scheduled > 0, "a court armed from genesis would make the boundary below untestable");
        assert!(!palw_da_court_in_force_v1(&rc, scheduled - 1), "the last block before the flag day is not past it");
        assert!(palw_da_court_in_force_v1(&rc, scheduled), "the flag day itself is past the flag day");
        assert!(palw_da_court_in_force_v1(&rc, u64::MAX), "and every block after it");

        // Devnet arms no data-availability court, so nothing there is refused for want of a
        // responder — the predicate must be false at every height, not merely at low ones.
        let devnet = Config::new(devnet_shipped_params());
        assert!(devnet.params.palw_da_court.is_none(), "devnet's data-availability court is deliberately dormant");
        for daa in [0u64, 1, scheduled, u64::MAX] {
            assert!(!palw_da_court_in_force_v1(&devnet, daa), "devnet has no data-availability court at DAA {daa}");
        }
    }
}

/// **ADR-0152 P6 (post-edit 11, U2): the producer's pre-check against the chain's refusal** — T08's
/// node half, at the pure-function level: the facts `palw_producer_facts_v4` builds (the builder the
/// consensus API calls), the decision [`palw_producer_ready_v1`] takes on them, and what admission
/// and the fold do with an attempt built from those same facts on the same state at the same DAA.
///
/// Every state here is built by the fold from objects and attempts — this crate cannot write a
/// chain state's ledgers by hand, and should not. So a bond is put under the producer floor by
/// evaluating it against a floor above what it registered (the shape `palw_producer_v2`'s own v4
/// test uses; on a live network a slash is what does it), and its committed room is spent by a
/// claim of its own the fold has already recorded. `palw_producer_t12_tests` runs the same decision
/// on testnet-12's own fold, with a live seat lock the fold wrote.
#[cfg(test)]
mod p6_tests {
    use super::{
        PalwProducerHoldV1, PalwRcorePlusReadsV1, palw_canonical_claim_bond_room_v1, palw_canonical_claim_room_v1,
        palw_producer_hold_detail_v1, palw_producer_ready_v1, palw_rcore_plus_producer_floor_v1,
    };
    use kaspa_consensus_core::palw_admission_v2::{
        PalwAdmissionParamsV2, PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2,
    };
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1,
        challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    use kaspa_consensus_core::palw_producer_v2::{PALW_NOT_READY_EXPOSURE_FULL_V2, PalwProducerFactsV2, palw_producer_facts_v4};
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2,
        PalwTransitionExtrasV1, apply_palw_transition_v2, apply_palw_transition_v2_with_extras,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const NET: u64 = 0x4E45_5457;
    /// The producer's local verification key — the one bond 1 registers.
    const KEY: [u8; 4] = [7; 4];
    /// The key a bond registered to replace bond 1 names (a key registers one bond).
    const KEY_2: [u8; 4] = [8; 4];
    /// The candidate block's DAA: the one the facts are read for and the chain judges the attempt at.
    const CANDIDATE: u64 = 101;
    /// Collateral far above any claim here, for the cases the ceiling is not the question in.
    const AMPLE: u64 = 1_000_000;

    /// `palw_producer_v2`'s contract fixture with the producer floor and the fence as arguments:
    /// `palw_rcore_plus` armed at genesis (the only way it arms) or not at all, and the 500‰ ceiling
    /// on both the state and the admission side, as `palw_mode_v2` requires of every bundle.
    fn params(floor: u64, rcore_plus: bool) -> PalwStateParamsV2 {
        let p = PalwStateParamsV2::new(500, 100, 100, 100, 100, 1_000, h64(1), 4, 1_000, floor, 100, 100)
            .unwrap()
            .with_fp_exposure_ceiling(500)
            .unwrap();
        if rcore_plus { p.with_rcore_plus_mirrors(Some(0), 0, Vec::new()) } else { p }
    }

    fn admission() -> PalwAdmissionParamsV2 {
        PalwAdmissionParamsV2::new(500).unwrap()
    }

    /// The pre-check's reads past the fence, with `floor` the one `params(floor, true)` holds.
    fn past(floor: u64, eligible_stake_at_floor: Option<bool>) -> Option<PalwRcorePlusReadsV1> {
        Some(PalwRcorePlusReadsV1 { producer_floor: floor, eligible_stake_at_floor })
    }

    /// Where `misaka-cli`'s `hold_from_log` cuts a `holding:` detail into sentence and numbers.
    fn cli_sentence(detail: &str) -> &str {
        detail.rfind(" [").map(|i| &detail[..i]).unwrap_or(detail)
    }

    fn outpoint(n: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 }
    }

    fn bond_registered(n: u64, key: [u8; 4], operator: u8, collateral: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(outpoint(n)),
            pubkey: key.to_vec(),
            operator_pubkey: vec![operator; 8],
            collateral,
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        }
    }

    /// The fence on, as every network that arms R-core+ also arms the audit fence at or below it.
    fn extras() -> PalwTransitionExtrasV1 {
        PalwTransitionExtrasV1 { audit_2026_09_23_active: true, ..Default::default() }
    }

    /// One floor class on the derived pwu rule and bond 1 posting `collateral` — registered under a
    /// floor of one sompi, so the floor a test judges it by is the test's choice — and then
    /// `prior_claims` claims of bond 1's own, each its own block's attempt, recorded by the fold on
    /// the side of the fence the test is on.
    fn state(collateral: u64, prior_claims: u64, rcore_plus: bool) -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
                initial_target: u128::MAX / 4,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            bond_registered(1, KEY, 0x21, collateral),
        ];
        let registry = params(1, rcore_plus);
        let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(1), daa_score: 100, blue_score: 1, subsidy: 0 };
        let mut state = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &registry, &ctx, &objects, None).unwrap().0;
        for prior in 1..=prior_claims {
            let env = attempt(&facts(&state, &registry, 1), 1, prior);
            let ctx =
                PalwBlockContextV2 { block: Hash64::from_u64_word(10 + prior), daa_score: 100, blue_score: 1 + prior, subsidy: 0 };
            state =
                apply_palw_transition_v2_with_extras(&state, &registry, &ctx, &[], Some(&env), false, false, false, false, &extras())
                    .unwrap()
                    .0;
            assert!(state.claim(&attempt_id_v2(&env.attempt)).is_some(), "prior claim {prior} is recorded");
        }
        state
    }

    /// The facts the consensus API hands the producer (`palw_producer_facts_v2_impl` calls v4).
    fn facts(state: &PalwChainStateV2, p: &PalwStateParamsV2, bond: u64) -> PalwProducerFactsV2 {
        palw_producer_facts_v4(
            state,
            p,
            &admission(),
            Hash64::from_u64_word(1),
            CANDIDATE,
            h64(1),
            Some(&PalwBondKeyV2(outpoint(bond))),
            None,
            None,
            None,
            true,
            0,
            None,
        )
        .expect("the floor class is registered")
    }

    /// An attempt built from nothing but `facts`, its class ticket won — the producer's own loop.
    /// `salt` separates one execution from another, so each is its own claim.
    fn attempt(facts: &PalwProducerFactsV2, bond: u64, salt: u64) -> PalwAttemptEnvelopeV2 {
        let bond_facts = facts.bond.as_ref().expect("a registered bond");
        let mut env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(NET),
                challenge: challenge_v2(h64(NET), h64(0x5050_4800), 7, 1, facts.class_id, &outpoint(bond)),
                class_id: facts.class_id,
                executor_bond: outpoint(bond),
                executor_pubkey: bond_facts.registered_pubkey.clone(),
                operator_id: bond_facts.operator_id,
                artifact_root: facts.artifact_root,
                trace_root: h64(31),
                output_root: h64(32),
                execution_root: h64(41),
                pwu: facts.pwu,
                trace_manifest_root: attempt_trace_manifest_root_v1(h64(31), 1),
                trace_chunk_count: 1,
                trace_retention_daa: 999_999,
            },
            signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let anchor = execution_anchor_v3(h64(NET), h64(0x5050_4800), facts.class_id, &outpoint(bond), 1);
        let won = (0u64..100_000).any(|n| {
            env.attempt.trace_root = h64(0x3100_0000_0000_0000u64.wrapping_add(salt << 32).wrapping_add(n));
            class_ticket_v3(&env.attempt, anchor) <= facts.class_target
        });
        assert!(won, "a quarter-of-the-space target is winnable in 1e5 tries");
        env
    }

    /// **What the chain does with that attempt at the candidate block**: admission's verdict, and
    /// whether the fold — the block's own attempt, audit fence on — records a claim for it. A floor
    /// or ceiling refusal of an own attempt is non-fatal, so the fold's answer is the claim's
    /// absence, not an error.
    fn chain(
        state: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        facts: &PalwProducerFactsV2,
        bond: u64,
    ) -> (Result<(), PalwAdmissionV2Error>, bool) {
        let env = attempt(facts, bond, 0);
        let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(2), daa_score: CANDIDATE, blue_score: 100, subsidy: 0 };
        let admitted =
            check_palw_attempt_admission_v2(state, p, &admission(), &ctx, &env, PalwEpochBudgetFencesV1::default()).map(|_| ());
        let (next, _) = apply_palw_transition_v2_with_extras(state, p, &ctx, &[], Some(&env), false, false, false, false, &extras())
            .expect("a refused own attempt is skipped, never fatal");
        (admitted, next.claim(&attempt_id_v2(&env.attempt)).is_some())
    }

    /// One claim's exposure on this fixture, as the facts price it.
    fn one_claim() -> u128 {
        let state = state(AMPLE, 0, true);
        facts(&state, &params(1, true), 1).bond.unwrap().claim_exposure
    }

    /// **U2: a bond under the producer floor holds with the top-up it is short, the sentence names
    /// the one way out — a bond at the floor under a new key, since nothing raises a registered
    /// bond's collateral — and the producer draws again once that bond is registered.** The old
    /// pre-check mined it — room is not what is short — and the chain refused it at both layers.
    #[test]
    fn a_bond_under_the_producer_floor_holds_naming_its_shortfall_and_draws_once_a_bond_at_the_floor_is_registered() {
        let floor = AMPLE + AMPLE / 4;
        let p = params(floor, true);
        let state = state(AMPLE, 0, true);
        let f = facts(&state, &p, 1);
        let bond = f.bond.as_ref().unwrap();
        assert_eq!(bond.producer_floor_shortfall, Some(250_000));
        assert!(bond.has_committed_room(), "room is not what holds it");
        assert_eq!(f.ready_to_produce(&KEY), Ok(()), "the old pre-check would have run the inference");

        let hold = palw_producer_ready_v1(&f, &KEY, past(floor, None)).unwrap_err();
        assert_eq!(hold, PalwProducerHoldV1::BelowProducerFloor { shortfall: 250_000, floor });
        let detail = palw_producer_hold_detail_v1(&f, &hold, true);
        assert!(format!("holding: {detail}").starts_with("holding: top up 250000 sompi to reach the producer floor — "), "{detail}");
        // The way out, with the size it must post: the review's finding 1 — "top up" alone is an
        // instruction no object on this chain can carry out.
        assert!(detail.contains("a registered bond's collateral cannot be raised"), "{detail}");
        assert!(detail.contains(&format!("register a bond of at least {floor} sompi under a NEW key")), "{detail}");
        assert!(detail.contains(" floor_shortfall=250000"), "{detail}");
        // `misaka-cli` cuts the sentence at the last " [": it must get the whole sentence (a CLI
        // that does not know it prints it verbatim) and the numbers after it.
        assert_eq!(cli_sentence(&detail), hold.to_string());

        let (admitted, folded) = chain(&state, &p, &f, 1);
        assert!(
            matches!(
                admitted,
                Err(PalwAdmissionV2Error::ProducerBelowFloor { collateral: AMPLE, floor: chain_floor, .. }) if chain_floor == floor
            ),
            "{admitted:?}"
        );
        assert!(!folded, "the fold skips it (`ProducerBelowFloor`): the block would have carried no claim");

        // Re-registered at the floor — the producer pointed at bond 2 and its key (no object tops a
        // bond up, and a key registers one bond).
        let ctx = PalwBlockContextV2 { block: Hash64::from_u64_word(3), daa_score: 100, blue_score: 50, subsidy: 0 };
        let state = apply_palw_transition_v2(&state, &p, &ctx, &[bond_registered(2, KEY_2, 0x22, floor)], None).unwrap().0;
        let f = facts(&state, &p, 2);
        assert_eq!(f.bond.as_ref().unwrap().producer_floor_shortfall, None);
        assert_eq!(palw_producer_ready_v1(&f, &KEY_2, past(floor, None)), Ok(()));
        let (admitted, folded) = chain(&state, &p, &f, 2);
        assert_eq!(admitted, Ok(()));
        assert!(folded, "and the claim lands");
    }

    /// **SR-7: a bond whose committed room is spent holds where the `reserved_exposure` check
    /// passed** — the case that proves the switch. Everything a bond stands behind that is not its
    /// own claims (registration, and every seat `Valid` lock above its duty) is committed and never
    /// reserved, so the old ledger shows room the chain does not grant. The facts here are the
    /// fixture's, with `committed` carrying what one live seat lock adds, to pin the boundary and
    /// the sentences; `palw_producer_t12_tests` reaches the same hold through testnet-12's fold, with
    /// a lock the fold wrote, and checks admission and the fold refuse there too.
    #[test]
    fn a_bond_whose_committed_room_is_spent_holds_where_the_reserved_exposure_check_passed() {
        let p = params(1, true);
        let state = state(AMPLE, 0, true);
        let clean = facts(&state, &p, 1);
        let mut f = clean.clone();
        let bond = f.bond.as_mut().unwrap();
        let lock = bond.exposure_ceiling - bond.committed - bond.claim_exposure + 1;
        bond.committed += lock;
        let bond = f.bond.as_ref().unwrap();
        assert!(bond.has_exposure_room(), "the old ledger still shows room");
        assert_eq!(f.ready_to_produce(&KEY), Ok(()), "and the old pre-check would have run the inference");
        assert!(!bond.has_committed_room());

        let hold = palw_producer_ready_v1(&f, &KEY, past(1, None)).unwrap_err();
        assert_eq!(hold, PalwProducerHoldV1::NotReady(PALW_NOT_READY_EXPOSURE_FULL_V2));
        let detail = palw_producer_hold_detail_v1(&f, &hold, true);
        let numbers =
            format!(" exposure={}/{} per_claim={} ledger=committed]", bond.committed, bond.exposure_ceiling, bond.claim_exposure);
        assert!(detail.starts_with(PALW_NOT_READY_EXPOSURE_FULL_V2) && detail.ends_with(&numbers), "{detail}");
        assert_eq!(
            palw_canonical_claim_room_v1(bond, past(1, None)),
            Err(format!(
                "no exposure room for a canonical claim: bond commits {} on the one committed ledger and one claim needs {} against \
                 a ceiling of {}",
                bond.committed, bond.claim_exposure, bond.exposure_ceiling
            ))
        );
        assert_eq!(palw_canonical_claim_room_v1(bond, None), Ok(()), "the canonical claim's old check saw room too");

        // One sompi less and the claim fits exactly: `committed + claim == ceiling` is room.
        let mut f = clean;
        let bond = f.bond.as_mut().unwrap();
        bond.committed += lock - 1;
        assert_eq!(palw_producer_ready_v1(&f, &KEY, past(1, None)), Ok(()));
    }

    /// **T08, node half: the node holds exactly when the chain refuses, and for the chain's reason**,
    /// over the floor's three sides (met, one sompi short, a quarter short) × the committed room's
    /// three (ample; spent by a recorded claim to the sompi the next one needs; one sompi short of
    /// it). The floor is reported first because `apply_attempt` and admission both ask it first.
    #[test]
    fn the_nodes_decision_is_the_chains_refusal_on_the_same_state() {
        let claim = u64::try_from(one_claim()).unwrap();
        assert!(claim > 0, "the fixture prices a claim");
        // Collateral whose 500‰ ceiling is exactly two claims (the recorded one and the candidate),
        // or two claims less one sompi.
        let rooms = [("ample", AMPLE, 0), ("exact", 4 * claim, 1), ("one over", 4 * claim - 2, 1)];
        let mut holds = 0;
        for (room, collateral, prior) in rooms {
            let state = state(collateral, prior, true);
            for (side, floor) in [("met", collateral), ("one short", collateral + 1), ("quarter short", collateral + collateral / 4)] {
                let p = params(floor, true);
                let f = facts(&state, &p, 1);
                assert_eq!(f.bond.as_ref().unwrap().committed, u128::from(prior * claim), "{room}: the recorded claim is committed");
                let node = palw_producer_ready_v1(&f, &KEY, past(floor, None));
                let (admitted, folded) = chain(&state, &p, &f, 1);
                let case = format!("room {room}, floor {side}: node={node:?} admission={admitted:?} fold={folded}");
                assert_eq!(node.is_ok(), admitted.is_ok(), "{case}");
                assert_eq!(node.is_ok(), folded, "{case}");
                match (&node, &admitted) {
                    (Ok(()), Ok(())) => {}
                    (
                        Err(PalwProducerHoldV1::BelowProducerFloor { shortfall, floor: node_floor }),
                        Err(PalwAdmissionV2Error::ProducerBelowFloor { collateral: posted, floor: chain_floor, .. }),
                    ) => {
                        assert_eq!(
                            (*shortfall, *node_floor, *posted, *chain_floor),
                            (floor - collateral, floor, collateral, floor),
                            "{case}"
                        );
                    }
                    (
                        Err(PalwProducerHoldV1::NotReady(PALW_NOT_READY_EXPOSURE_FULL_V2)),
                        Err(PalwAdmissionV2Error::ExposureCeilingExceeded { .. }),
                    ) => {}
                    _ => panic!("the node and the chain disagree on the reason — {case}"),
                }
                holds += usize::from(node.is_err());
            }
        }
        assert_eq!(holds, 7, "the two short floors under every room, and the met floor one sompi over");
    }

    /// **Below the fence the old checks stand byte for byte** — testnet-11, devnet and mainnet do not
    /// move. A bond under what would be the floor is not held (the facts' floor is dormant, and the
    /// chain takes the attempt); the decision is `ready_to_produce`'s on every variation, including
    /// facts whose `committed` and floor fields would hold past the fence; the bracket is the one
    /// this loop always printed; the canonical claim's room check is the old inequality; and the
    /// fold's-room check after its price asks nothing. (Below the fence the pre-check has no reads to
    /// be handed — `palw_rcore_plus_reads_v1` is `None` there, `the_fence_side_is_read_from_the_networks_params`.)
    #[test]
    fn below_the_fence_the_pre_check_is_ready_to_produce_byte_for_byte() {
        let p = params(AMPLE + 1, false);
        let state = state(AMPLE, 1, false);
        let f = facts(&state, &p, 1);
        let bond = f.bond.as_ref().unwrap();
        assert_eq!(bond.producer_floor_shortfall, None, "the floor is dormant below the fence");
        assert_eq!(palw_producer_ready_v1(&f, &KEY, None), Ok(()));
        let (admitted, folded) = chain(&state, &p, &f, 1);
        assert_eq!(admitted, Ok(()));
        assert!(folded, "the chain below the fence takes what the old pre-check passes");

        // Every verdict `ready_to_produce` can give, plus the two fields the fence-off path must not
        // read, set to values that would hold past it.
        let variations: Vec<Box<dyn Fn(&mut PalwProducerFactsV2)>> = vec![
            Box::new(|_| {}),
            Box::new(|f| f.bond = None),
            Box::new(|f| f.bond.as_mut().unwrap().registered_pubkey = vec![9; 4]),
            Box::new(|f| f.class_admission_refusal = Some("class … is Prefetching under the model registry".to_string())),
            Box::new(|f| {
                f.is_base_class = false;
                f.epoch_budget_blocks = 0;
            }),
            Box::new(|f| {
                let bond = f.bond.as_mut().unwrap();
                bond.reserved_exposure = bond.exposure_ceiling;
            }),
            Box::new(|f| {
                let bond = f.bond.as_mut().unwrap();
                bond.committed = u128::MAX;
                bond.producer_floor_shortfall = Some(5);
            }),
        ];
        for vary in &variations {
            let mut f = f.clone();
            vary(&mut f);
            let old = f.ready_to_produce(&KEY);
            let new = palw_producer_ready_v1(&f, &KEY, None);
            assert_eq!(new, old.map_err(PalwProducerHoldV1::NotReady));
            if let (Err(why), Err(hold)) = (old, &new) {
                // The bracket exactly as the worker formatted it before this change.
                let before = format!(
                    "{why} [class={} epoch={} produced={} budget={}{}{}]",
                    f.class_id,
                    f.epoch_index,
                    f.epoch_produced_blocks,
                    f.epoch_budget_blocks,
                    match &f.bond {
                        Some(bond) =>
                            format!(" exposure={}/{} per_claim={}", bond.reserved_exposure, bond.exposure_ceiling, bond.claim_exposure),
                        None => String::new(),
                    },
                    f.class_admission_refusal.as_deref().map(|why| format!(" registry=\"{why}\"")).unwrap_or_default()
                );
                assert_eq!(palw_producer_hold_detail_v1(&f, hold, false), before);
            }
            // The canonical claim's room check, likewise.
            if let Some(bond) = &f.bond {
                let old_room = if bond.reserved_exposure.saturating_add(bond.claim_exposure) > bond.exposure_ceiling {
                    Err(format!(
                        "no exposure room for a canonical claim: bond backs {} and one claim needs {} against a ceiling of {}",
                        bond.reserved_exposure, bond.claim_exposure, bond.exposure_ceiling
                    ))
                } else {
                    Ok(())
                };
                assert_eq!(palw_canonical_claim_room_v1(bond, None), old_room);
            }
        }
        // The price's room check: nothing below the fence, however far over the room.
        assert_eq!(palw_canonical_claim_bond_room_v1(&fp_price(u128::MAX, 1), Some(0), false), Ok(()));
    }

    /// **The SW-10 seam holds only on a known shortfall, and only past the fence**, after every
    /// question the fold refuses on. Its answer is `None` on every build today
    /// (`palw_class_eligible_stake_at_floor_v1`), so it moves nothing yet; this pins what it does once
    /// a consensus read answers.
    #[test]
    fn the_eligible_stake_answer_holds_only_when_it_is_known_to_be_short() {
        let p = params(1, true);
        let state = state(AMPLE, 0, true);
        let f = facts(&state, &p, 1);
        assert_eq!(palw_producer_ready_v1(&f, &KEY, past(1, None)), Ok(()), "unknown never holds");
        assert_eq!(palw_producer_ready_v1(&f, &KEY, past(1, Some(true))), Ok(()));
        let hold = palw_producer_ready_v1(&f, &KEY, past(1, Some(false))).unwrap_err();
        assert_eq!(hold, PalwProducerHoldV1::EligibleStakeBelowFloor);
        assert!(hold.to_string().contains("875‰"), "{hold}");
        assert!(!hold.to_string().contains(" ["), "the CLI's cut: {hold}");
        // A refusal of the attempt itself is reported first.
        let mut short = f.clone();
        short.bond.as_mut().unwrap().producer_floor_shortfall = Some(3);
        let floor_hold = PalwProducerHoldV1::BelowProducerFloor { shortfall: 3, floor: 1 };
        assert_eq!(palw_producer_ready_v1(&short, &KEY, past(1, Some(false))), Err(floor_hold.clone()));
        let bond = f.bond.as_ref().unwrap();
        assert_eq!(palw_canonical_claim_room_v1(bond, past(1, None)), Ok(()));
        assert_eq!(palw_canonical_claim_room_v1(bond, past(1, Some(false))), Err(format!("holding: {hold}")));
        // And the canonical claim asks the floor first too, in the attempt lane's words.
        let canonical = palw_canonical_claim_room_v1(short.bond.as_ref().unwrap(), past(1, Some(false))).unwrap_err();
        assert_eq!(canonical, format!("holding: {floor_hold}"));
        assert!(canonical.starts_with("holding: top up 3 sompi to reach the producer floor — "), "{canonical}");
    }

    /// A free-prompt price holding `reserved + rights_reserved`.
    fn fp_price(reserved: u128, rights_reserved: u128) -> kaspa_consensus_core::palw_state_v2::PalwFpCommitmentPriceV1 {
        kaspa_consensus_core::palw_state_v2::PalwFpCommitmentPriceV1 {
            quanta: 1,
            pwu: 1,
            rights_reserved,
            reserved,
            priced_in_compute: true,
            derived_work: None,
        }
    }

    /// **The review's finding 4: past the fence a canonical claim is held to the fold's own room once
    /// its price is known** — the FP arm refuses `backed + reserved + rights_reserved > ceiling`, and
    /// the node's price answer carries `bond_room = ceiling − backed` (`palw_fp_bond_room_v2`), so
    /// the claim fits exactly when `reserved + rights_reserved <= bond_room`. Rights count: they are
    /// what the ledger holds beside the reservation.
    #[test]
    fn past_the_fence_a_priced_canonical_claim_is_held_to_the_folds_own_room() {
        let room = 1_000u128;
        assert_eq!(palw_canonical_claim_bond_room_v1(&fp_price(room, 0), Some(room), true), Ok(()), "exactly the room fits");
        assert_eq!(palw_canonical_claim_bond_room_v1(&fp_price(room - 10, 10), Some(room), true), Ok(()));
        let over = palw_canonical_claim_bond_room_v1(&fp_price(room - 10, 11), Some(room), true).unwrap_err();
        assert_eq!(
            over,
            "the chain would refuse this canonical claim (FreePromptExposureCeiling): it holds 1001 sompi (990 reserved + 11 receipt \
             rights) against 1000 of room on the one committed ledger"
        );
        // Backing already over the ceiling reads as room 0, and the fold refuses any sompi there.
        assert!(palw_canonical_claim_bond_room_v1(&fp_price(1, 0), Some(0), true).is_err());
        // A room the answer did not carry (no bond asked about, or one the chain does not hold) says
        // nothing here; overflow saturates rather than wrapping into room.
        assert_eq!(palw_canonical_claim_bond_room_v1(&fp_price(u128::MAX, u128::MAX), None, true), Ok(()));
        assert!(palw_canonical_claim_bond_room_v1(&fp_price(u128::MAX, u128::MAX), Some(u128::MAX - 1), true).is_err());
    }

    /// **The fence side is the network's**: testnet-12 arms `palw_rcore_plus` at genesis and hands the
    /// pre-check its bundle's floor — the `min_collateral_sompi` the fold measures a bond against —
    /// and testnet-11, devnet and mainnet hand it nothing, so their producers keep `ready_to_produce`.
    #[test]
    fn the_fence_side_is_read_from_the_networks_params() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
        let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is a ConsensusV2 network") };
        assert!(t12.palw_rcore_plus_active_at(0), "the premise: testnet-12 arms R-core+ at genesis");
        for daa in [0u64, 1, 1_000_000] {
            assert_eq!(palw_rcore_plus_producer_floor_v1(&t12, daa), Some(bundle.state.min_collateral_sompi()));
        }
        assert_eq!(
            bundle.state.min_collateral_sompi(),
            kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(bundle.state.min_collateral_sompi(), true),
            "the floor a replacement bond is told to post is the one registration asks"
        );
        for (name, p) in [
            ("testnet-11", kaspa_consensus_core::config::params::palw_rc_shipped_params()),
            ("testnet-11 identity", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
            ("devnet", kaspa_consensus_core::config::params::devnet_shipped_params()),
            ("mainnet", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ] {
            for daa in [0u64, 1, 1_000_000, u64::MAX] {
                assert_eq!(palw_rcore_plus_producer_floor_v1(&p, daa), None, "{name} at DAA {daa} keeps the old pre-check");
            }
        }
    }
}
