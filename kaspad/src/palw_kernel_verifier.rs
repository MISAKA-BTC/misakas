//! **Lane G14-K: the kernel route's public verifier, inside the node's service** (G14: the producer and every Panel seat collude; one
//! public bonded verifier outside the Panel carries a covered fault to its objective outcome from public authenticated material alone).
//!
//! Until this lane the kernel route (K2-TIR v1/v2 single programs, v4 segmented, v5 encoders) convicted only through a test harness's
//! verifier; the legacy route had kaspad's common fraud filer (`palw_fraud_filer`). This is the kernel route's twin, a part of
//! [`PalwPanelService`]'s tick:
//!
//! 1. **Discover** — once a DAA it reads the route through the node's own read API (`ConsensusApi::palw_kernel_route_v1`, op 211's
//!    source) and rebuilds the ledger with the terms the fold injects (`pursuit::kernel_consumer_ledger_v1`: G14-S's court scope past
//!    `palw_provider_court_v1`, the verifier-pay terms), so every dry run judges as the next block will.
//! 2. **Re-execute independently** with the operator's own copy of the registered model (ADR-0177 D7: a verifier holds the registered
//!    model; `--palw-kernel-model`, one file a model, matched to a class only by its registered parameter commitments — a model that
//!    does not open to them is never used). A single-program claim is replayed from the ledger's public commitments alone
//!    (`pursuit::kernel_check_program_v1`: no producer value is read, nothing is ever demanded of an honest producer). A segmented claim
//!    is replayed once per process (`seg_detect::prepare_reexecution_v1`) and searched against the `Respond` parts the accepted blocks
//!    carry (`pursuit::KernelBlockMaterialV1`), its prompt from the authenticated tiles (`pursuit::kernel_tiled_prompt_v1`). A typed
//!    `Memory` claim (RFC-0004) is replayed from its public pre-state (the chain's carried post-state, or M0 from the model) by
//!    `pursuit::kernel_check_typed_v1`; pipelines, composites and retrieval snapshots need more than one model and are not checked here.
//! 3. **Pursue** — the finding is the goal of `misaka_palw_remote::pursuit::plan_kernel_pursuit_on_v1`, which returns the step the chain
//!    still needs (seal → wait → file whole or in tag-113 chunks; demand → await the answer → re-check; conviction or the producer's DA
//!    default), dry-run against a clone of the rebuilt ledger before anything is sent: a filing the court would dismiss, a demand past
//!    the scope or the horizon, is reported and never sent.
//! 4. **Send** — the step's objects (`pursuit::kernel_step_sends_v1`) are signed DETERMINISTICALLY (the service's ML-DSA-87 signer is
//!    FIPS 204's deterministic variant, `rnd = 0^32` = `PURSUIT_DETERMINISTIC_RND_V1`: a restarted service rebuilds the same bytes, the
//!    same chunk group) and queued on the court lane (the priority lane's queue, `carry_priority_v1`), which funds each carrier from the
//!    virtual UTXO set at the object's own minimum (mature, unlocked, free; chained through its change, never a deposit spent twice) and
//!    submits it to the PQ mempool, from which the production template and the real block pipeline take it.
//!
//! **Nothing persists** (G14-R's rule): the book below is a cache of the chain and of this process's replays. A restart, a reorg, an IBD
//! or a pruned import re-derives every pursuit from the tip; an object already in a block is never re-planned, one still in this node's
//! mempool (sent before a restart) is never queued twice, and one queued is re-offered only past [`PALW_KERNEL_VERIFIER_RESEND_DAA_V1`].
//!
//! **Identity arms it; the flag only supplies the model.** A node with a funded bond (`--palw-fee-outpoint`) and at least one kernel
//! model runs it past `palw_probabilistic_constraints_v1` or `palw_panel_free_v1` (RFC-0015: the verifier an OPV claim relies on;
//! no shipped preset arms either and validation refuses both); everywhere else it is a clean no-op. It treats a Panel-licensed and an
//! OptimisticPublicVerification claim alike (`Challengeable` is open; an OPV claim has no seat to leave it to). Claims this bond produced are skipped (a producer does not prosecute itself).
use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1,
    palw_kernel_chunk_message_v1, palw_kernel_route_message_v1,
};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{ClaimBodyV1, KernelLedgerV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::route::KernelRouteObjectV1;
use misaka_palw_remote::pursuit::{
    KERNEL_SERVICE_CHUNK_CAP_V1, KernelBlockMaterialV1, KernelFilingV1, KernelGoalV1, KernelPursuitStepV1, KernelPursuitV1,
    KernelSendV1, kernel_check_program_v1, kernel_check_segmented_v1, kernel_check_typed_v1, kernel_class_model_root_v1,
    kernel_consumer_ledger_v1, kernel_segmented_tokens_v1, kernel_step_sends_v1, kernel_tiled_prompt_v1, plan_kernel_pursuit_on_v1,
};
use misaka_palw_tir::MapParams;

/// The court queue's round of this verifier's kernel-route objects — a round no other lane keys by (the reporter's `u32::MAX − 1…3`,
/// the replay filer's `− 6`, operator DA `− 7`, audit `− 8`, riders `− 9`, LG14-A `− 10…14`, public sources `− 15…18`).
/// It was `− 13`, which LG14-A's release round also used: a disarmed verifier's queue sweep dropped queued releases.
pub(super) const PALW_KERNEL_VERIFIER_ROUND_V1: u32 = u32::MAX - 19;

/// An object queued this many DAA ago and still not on chain (nor in this node's mempool) is offered again.
pub(super) const PALW_KERNEL_VERIFIER_RESEND_DAA_V1: u64 = 6;

/// How far below a segmented claim's commitment the block walk looks for its job's prompt tiles (a tiled job's tiles are posted
/// before the claim commits; a job older than this is not followed — reported, never guessed).
pub(super) const PALW_KERNEL_VERIFIER_PROMPT_LOOKBACK_DAA_V1: u64 = 4_000;

/// The most selected-chain blocks one tick's walk reads.
pub(super) const PALW_KERNEL_VERIFIER_WALK_MAX_BLOCKS_V1: usize = 40_000;

/// The magic of a kernel model file (`--palw-kernel-model`): the 8 bytes, then the Borsh of `Vec<(param, layer, TensorWireV1)>`.
pub(crate) const PALW_KERNEL_MODEL_MAGIC_V1: &[u8; 8] = b"PALWKM01";

/// Write `params` as a kernel model file (the operator's tool and the tests' fixture writer).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn palw_kernel_model_write_v1(path: &std::path::Path, params: &MapParams) -> std::io::Result<()> {
    let rows: Vec<(u16, Option<u16>, misaka_palw_kernel::public::TensorWireV1)> =
        params.tensors.iter().map(|((j, l), t)| (*j, *l, misaka_palw_kernel::public::TensorWireV1::of(t))).collect();
    let mut bytes = PALW_KERNEL_MODEL_MAGIC_V1.to_vec();
    bytes.extend(borsh::to_vec(&rows).map_err(std::io::Error::other)?);
    std::fs::write(path, bytes)
}

/// Read a kernel model file.
pub(crate) fn palw_kernel_model_read_v1(path: &std::path::Path) -> Result<MapParams, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let body = bytes.strip_prefix(PALW_KERNEL_MODEL_MAGIC_V1.as_slice()).ok_or("not a kernel model file (PALWKM01)")?;
    let rows: Vec<(u16, Option<u16>, misaka_palw_kernel::public::TensorWireV1)> =
        borsh::from_slice(body).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut params = MapParams::default();
    for (j, l, wire) in rows {
        params.tensors.insert((j, l), wire.decode().map_err(|e| format!("{}: tensor ({j}, {l:?}): {e}", path.display()))?);
    }
    Ok(params)
}

/// One model this verifier holds: its tensors and the parameter-commitment roots it opens to (both forms: v1/v2 single programs
/// commit `ParamCommitmentsV1::of`, v4/v5 `of_v3`). A class is checked with a model only when the class's registered root is one of
/// these — the registered commitments, never a declaration.
struct PalwKernelModelV1 {
    roots: [Digest; 2],
    params: std::sync::Arc<MapParams>,
}

/// What this process concluded about one claim (a cache: a claim id commits to every byte the check reads, so a single-program
/// finding never changes; a segmented one is re-searched against the blocks every tick, its own replay kept).
#[derive(Default)]
struct PalwKernelCaseV1 {
    /// Single program: the finding (`None` = it checks).
    program: Option<Option<KernelGoalV1>>,
    /// Segmented: the own replay's roots (a pure function of the record and the model).
    replay: Option<misaka_palw_kernel::seg_detect::OwnReplayV1>,
    /// The last step reported (each change is logged once).
    last: Option<String>,
    /// Why this node cannot check the claim (logged once, never a filing).
    unjudged: Option<String>,
}

/// **The verifier's book** — a cache of the chain and of this process's replays; nothing persists (module doc).
#[derive(Default)]
pub(super) struct PalwKernelVerifierBookV1 {
    loaded: bool,
    models: Vec<PalwKernelModelV1>,
    cases: HashMap<Digest, PalwKernelCaseV1>,
    /// A tiled job's prompt, once read in full from the blocks (immutable: authenticated against the job's root).
    prompts: HashMap<Digest, Vec<u32>>,
    /// What was queued, by the object's hash, and the DAA it was queued at.
    queued: HashMap<Hash64, u64>,
    last_daa: Option<u64>,
}

impl PalwKernelVerifierBookV1 {
    fn load(&mut self, paths: &[std::path::PathBuf]) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        for path in paths {
            match palw_kernel_model_read_v1(path) {
                Ok(params) => {
                    let roots = [
                        misaka_palw_kernel::trace::ParamCommitmentsV1::of(&params).root(),
                        misaka_palw_kernel::trace::ParamCommitmentsV1::of_v3(&params).root(),
                    ];
                    info!(
                        "[{PALW_PANEL}] kernel verifier: holds the model {} ({} tensors, commitment root {})",
                        path.display(),
                        params.tensors.len(),
                        Hash64::from_bytes(roots[0])
                    );
                    self.models.push(PalwKernelModelV1 { roots, params: std::sync::Arc::new(params) });
                }
                Err(why) => warn!("[{PALW_PANEL}] kernel verifier: cannot load the model {}: {why}", path.display()),
            }
        }
    }

    fn model_for(&self, registered_root: &Digest) -> Option<std::sync::Arc<MapParams>> {
        self.models.iter().find(|m| m.roots.contains(registered_root)).map(|m| m.params.clone())
    }

    /// Forget everything about the route (the fence left, the bond unfunded).
    fn clear(&mut self) {
        self.cases.clear();
        self.queued.clear();
        self.last_daa = None;
    }
}

/// The object hash the book and the mempool scan key a queued object by.
fn palw_kernel_object_key_v1(object: &PalwConsensusObjectV2) -> Hash64 {
    let bytes = borsh::to_vec(object).expect("an object serializes");
    Hash64::from_bytes(*blake2b_simd::Params::new().hash_length(64).hash(&bytes).as_array())
}

/// Is the claim still something a filing or a demand can change? (Convicted, timed out and past-liability claims are not.)
fn palw_kernel_claim_open_v1(ledger: &KernelLedgerV1, claim: &Digest) -> bool {
    let Some(row) = ledger.claims.get(claim) else { return false };
    if row.convicted {
        return false;
    }
    match &row.life.state {
        ClaimStateV1::TimedOut { .. } | ClaimStateV1::Convicted { .. } => false,
        ClaimStateV1::Final { .. } | ClaimStateV1::Unavailable { .. } => row.liability_until.is_some_and(|until| ledger.daa <= until),
        _ => true,
    }
}

impl PalwPanelService {
    /// **The kernel verifier's half of the tick** (module doc): discover, re-execute, plan, queue.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn kernel_verifier_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwKernelVerifierBookV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        // What the priority lane carried of this verifier's items: said and forgotten (it keeps its own spacing, never the debounce).
        court_moved.retain(|(claim, round, responder), at| {
            if *round != PALW_KERNEL_VERIFIER_ROUND_V1 || *responder {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: the kernel verifier's object was carried at DAA {at} (G14-K)");
            false
        });
        let params = &self.consensus_config.params;
        // RFC-0015 §2 / §7.1: under Panel=0 (`palw_panel_free_v1` in force) no seat is drawn, so this public verifier is the role the
        // network relies on for OptimisticPublicVerification claims; it runs wherever either fence is in force (the identity and the
        // model still decide, never an opt-in). Both are dormant on every preset.
        let armed = params
            .palw_probabilistic_constraints_v1
            .is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(current_daa))
            || params.palw_panel_free_active_at(current_daa);
        if !(armed && self.config.fee_outpoint.is_some() && !self.config.kernel_models.is_empty()) {
            if book.last_daa.is_some() {
                book.clear();
                court_pending.retain(|(_, round, responder, _)| *round != PALW_KERNEL_VERIFIER_ROUND_V1 || *responder);
            }
            return;
        }
        book.load(&self.config.kernel_models);
        if book.models.is_empty() || book.last_daa == Some(current_daa) {
            return;
        }
        book.last_daa = Some(current_daa);
        let Some(route) = session.clone().spawn_blocking(|c| c.palw_kernel_route_v1()).await else { return };
        // The fences the fold injects, as in force at the next block (`palw_kernel_route_extras_at`).
        let next = current_daa.saturating_add(1);
        let in_force = |f: Option<kaspa_consensus_core::config::params::ForkActivation>| {
            f.filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(next))
                .map(|f| f.daa_score())
        };
        let provider_court = in_force(params.palw_provider_court_v1);
        let verifier_pay = params
            .palw_verifier_pay_v1
            .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|f| f.daa_score());
        let ledger = match kernel_consumer_ledger_v1(&route, provider_court, verifier_pay) {
            Ok(ledger) => ledger,
            Err(why) => {
                warn!("[{PALW_PANEL}] kernel verifier: the route's rows do not rebuild: {why}");
                return;
            }
        };
        // The verifier's own bond as the fold will sync it: its collateral less the route's chunk deposits it holds (op 202's row).
        let me = palw_kernel_bond_id_v1(&bond_key);
        let collateral = session
            .clone()
            .spawn_blocking(move |c| {
                c.palw_claim_rows_v1(bond_key, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor, false, 0)
            })
            .await
            .and_then(|rows| rows.bond)
            .map(|b| b.collateral);
        let Some(collateral) = collateral else { return };
        let synced = collateral.saturating_sub(route.chunk_deposits_of_v1(&bond_key));
        // The claims worth a look: open, not ours, of a class whose registered commitments a held model opens to.
        let candidates: Vec<Digest> = ledger
            .claims
            .iter()
            .filter(|(id, row)| {
                row.producer != me
                    && palw_kernel_claim_open_v1(&ledger, id)
                    && kernel_class_model_root_v1(&ledger, &row.class_binding_id).is_some_and(|root| book.model_for(&root).is_some())
            })
            .map(|(id, _)| *id)
            .collect();
        book.cases.retain(|id, _| ledger.claims.contains_key(id));
        if candidates.is_empty() {
            return;
        }
        // Segmented claims read the accepted blocks (prompt tiles, `Respond` parts): one walk a tick, down to the oldest one's horizon.
        let segmented: Vec<&Digest> =
            candidates.iter().filter(|id| matches!(ledger.claims[*id].body, ClaimBodyV1::Segmented { .. })).collect();
        let objects: Vec<PalwConsensusObjectV2> = if segmented.is_empty() {
            Vec::new()
        } else {
            let floor = segmented
                .iter()
                .map(|id| ledger.claims[*id].committed_daa.saturating_sub(PALW_KERNEL_VERIFIER_PROMPT_LOOKBACK_DAA_V1))
                .min()
                .unwrap_or(0);
            let raw = session
                .clone()
                .spawn_blocking(move |c| {
                    let mut out = Vec::new();
                    crate::palw_panel::walk_accepted_lifecycle_objects_v1(
                        c,
                        floor,
                        PALW_KERNEL_VERIFIER_WALK_MAX_BLOCKS_V1,
                        &mut |o| {
                            if matches!(
                                o,
                                PalwConsensusObjectV2::KernelRouteV1 { .. }
                                    | PalwConsensusObjectV2::KernelRouteChunkV1 { .. }
                                    | PalwConsensusObjectV2::ObjectChunk { .. }
                            ) {
                                out.push(o)
                            }
                        },
                    );
                    out.reverse(); // oldest first: the first copy of a chunk index is the one the lane kept
                    out
                })
                .await;
            misaka_palw_remote::pursuit::kernel_reassemble_objects_v1(raw)
        };
        // What this node's mempool already carries (sent before a restart, or queued and not yet mined): never queued twice.
        let (pooled, orphans) = self
            .flow_context
            .mining_manager()
            .clone()
            .get_all_transactions(kaspa_mining::model::tx_query::TransactionQuery::All)
            .await;
        let in_pool: HashSet<Hash64> = pooled
            .iter()
            .chain(orphans.iter())
            .filter_map(|p| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&p.tx.payload).ok())
            .map(|payload| palw_kernel_object_key_v1(&payload.object))
            .collect();
        let salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-verifier/salt/v1", &me);
        for claim in candidates {
            let row = &ledger.claims[&claim];
            let root = kernel_class_model_root_v1(&ledger, &row.class_binding_id).expect("filtered above");
            let model = book.model_for(&root).expect("filtered above");
            let goal = self.kernel_verifier_goal_v1(book, &ledger, &claim, model.as_ref(), salt, &objects);
            let case = book.cases.entry(claim).or_default();
            let goal = match goal {
                Ok(Some(goal)) => goal,
                Ok(None) => continue,
                Err(why) => {
                    if case.unjudged.as_deref() != Some(why.as_str()) {
                        warn!("[{PALW_PANEL}] kernel verifier: claim {} is not judged here: {why} (G14-K)", Hash64::from_bytes(claim));
                        case.unjudged = Some(why);
                    }
                    continue;
                }
            };
            let pursuit = KernelPursuitV1 { claim, bond: bond_key, goal, synced_collateral: Some(synced) };
            let filing = match &pursuit.goal {
                KernelGoalV1::Prove(proof) => self.kernel_route_signed_v1(
                    network_domain,
                    bond_key,
                    &KernelRouteObjectV1::FileProof { accuser: me, claim, proof: proof.clone() },
                ),
                KernelGoalV1::Demand(_) => None,
            };
            let f = filing.as_ref().map(|signed| KernelFilingV1 { signed, chunk_cap: KERNEL_SERVICE_CHUNK_CAP_V1 });
            let step = match plan_kernel_pursuit_on_v1(&route, &ledger, &pursuit, f.as_ref()) {
                Ok(step) => step,
                Err(why) => {
                    warn!("[{PALW_PANEL}] kernel verifier: claim {}: no plan: {why}", Hash64::from_bytes(claim));
                    continue;
                }
            };
            let case = book.cases.entry(claim).or_default();
            let said = format!("{step:?}");
            let shown = said.chars().take(240).collect::<String>();
            if case.last.as_deref() != Some(shown.as_str()) {
                match &step {
                    KernelPursuitStepV1::WouldBeDismissed { .. }
                    | KernelPursuitStepV1::Refused { .. }
                    | KernelPursuitStepV1::TooLate { .. }
                    | KernelPursuitStepV1::Stranded { .. } => {
                        warn!("[{PALW_PANEL}] kernel verifier: claim {}: {shown} — nothing sent (G14-K)", Hash64::from_bytes(claim))
                    }
                    _ => info!("[{PALW_PANEL}] kernel verifier: claim {}: {shown} (G14-K)", Hash64::from_bytes(claim)),
                }
                case.last = Some(shown);
            }
            let sends = match kernel_step_sends_v1(&step, &pursuit, f.as_ref()) {
                Ok(sends) => sends,
                Err(why) => {
                    warn!("[{PALW_PANEL}] kernel verifier: claim {}: {why}", Hash64::from_bytes(claim));
                    continue;
                }
            };
            for send in sends {
                let object = match send {
                    KernelSendV1::Route(KernelRouteObjectV1::FileProof { .. }) => filing.clone(),
                    KernelSendV1::Route(k) => self.kernel_route_signed_v1(network_domain, bond_key, &k),
                    KernelSendV1::Chunk(chunk) => {
                        let message = palw_kernel_chunk_message_v1(network_domain, &chunk);
                        self.sign(message.as_byte_slice(), PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1)
                            .map(|signature| PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(chunk), signature })
                    }
                };
                let Some(object) = object else { continue };
                let key = palw_kernel_object_key_v1(&object);
                let recently =
                    book.queued.get(&key).is_some_and(|at| current_daa < at.saturating_add(PALW_KERNEL_VERIFIER_RESEND_DAA_V1));
                let queued = court_pending.iter().any(|(_, round, _, o)| *round == PALW_KERNEL_VERIFIER_ROUND_V1 && *o == object);
                if recently || queued || in_pool.contains(&key) {
                    continue;
                }
                let session_key = Hash64::from_bytes(claim);
                info!(
                    "[{PALW_PANEL}] claim {session_key}: the kernel verifier queues {} (G14-K)",
                    palw_kernel_object_name_v1(&object)
                );
                // Dated by the step's own deadline (the planner's `by_daa`: before Final where the claim can still reach it, else
                // its liability horizon), so the court lane's EDF carries the pursuit closest to its horizon first.
                court_due.insert(
                    (session_key, PALW_KERNEL_VERIFIER_ROUND_V1, false),
                    palw_kernel_step_due_v1(&step).unwrap_or(current_daa),
                );
                court_pending.push((session_key, PALW_KERNEL_VERIFIER_ROUND_V1, false, object));
                book.queued.insert(key, current_daa);
            }
        }
        book.queued.retain(|_, at| current_daa < at.saturating_add(4 * PALW_KERNEL_VERIFIER_RESEND_DAA_V1));
    }

    /// The verifier's own finding for `claim` (cached per claim in `book`): `Ok(None)` when it checks.
    #[allow(clippy::too_many_arguments)]
    fn kernel_verifier_goal_v1(
        &self,
        book: &mut PalwKernelVerifierBookV1,
        ledger: &KernelLedgerV1,
        claim: &Digest,
        model: &MapParams,
        salt: Digest,
        objects: &[PalwConsensusObjectV2],
    ) -> Result<Option<KernelGoalV1>, String> {
        let row = &ledger.claims[claim];
        match &row.body {
            ClaimBodyV1::Program { .. } | ClaimBodyV1::Pipeline { .. } => {
                let case = book.cases.entry(*claim).or_default();
                if case.program.is_none() {
                    let found = kernel_check_program_v1(ledger, claim, model, salt)?;
                    if found.is_some() {
                        info!(
                            "[{PALW_PANEL}] kernel verifier: its own replay refutes claim {} with the registered model (G14-K)",
                            Hash64::from_bytes(*claim)
                        );
                    }
                    case.program = Some(found);
                }
                Ok(case.program.clone().expect("set above"))
            }
            ClaimBodyV1::Segmented { .. } => {
                let view = ledger.seg_claim_view_v1(claim).ok_or("no segmented record")?;
                let tiled = match (&view.job.inline_prompt, book.prompts.get(&row.job_id)) {
                    (Some(_), _) => None,
                    (None, Some(p)) => Some(p.clone()),
                    (None, None) => {
                        let found = kernel_tiled_prompt_v1(objects, &row.job_id, view.job.prompt_len, &view.job.prompt_root);
                        if let Some(p) = &found {
                            book.prompts.insert(row.job_id, p.clone());
                        }
                        found
                    }
                };
                let tokens = kernel_segmented_tokens_v1(&view, tiled.as_deref())
                    .ok_or("the job's prompt tiles are not all on the readable chain")?;
                let case = book.cases.entry(*claim).or_default();
                if case.replay.is_none() {
                    let art = |j: u16, l: Option<u16>| model.tensors.get(&(j, l)).cloned();
                    case.replay = Some(misaka_palw_kernel::seg_detect::prepare_reexecution_v1(&view.context(), &art, &tokens)?);
                }
                let served = KernelBlockMaterialV1::from_objects(objects, claim, &view);
                kernel_check_segmented_v1(&view, case.replay.as_ref().expect("set above"), &served, model, &tokens)
            }
            // RFC-0004 typed roots: a memory class (the only typed kind one model checks — `kernel_class_model_root_v1`).
            ClaimBodyV1::Spec(_) => {
                let case = book.cases.entry(*claim).or_default();
                if case.program.is_none() {
                    case.program = Some(kernel_check_typed_v1(ledger, claim, model, salt)?);
                }
                Ok(case.program.clone().expect("set above"))
            }
        }
    }

    /// `object` as a `KernelRouteV1` carrier object of `bond`, signed deterministically (`None`: the service holds no key).
    fn kernel_route_signed_v1(
        &self,
        network_domain: Hash64,
        bond: PalwBondKeyV2,
        object: &KernelRouteObjectV1,
    ) -> Option<PalwConsensusObjectV2> {
        let bytes = object.encode();
        let message = palw_kernel_route_message_v1(network_domain, &bond, &bytes);
        let signature = self.sign(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
        Some(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
    }
}

/// The DAA a step's objects must land by (the planner's deadline), for the court lane's EDF.
pub(super) fn palw_kernel_step_due_v1(step: &KernelPursuitStepV1) -> Option<u64> {
    match step {
        KernelPursuitStepV1::Seal { by_daa, .. }
        | KernelPursuitStepV1::FileWhole { by_daa }
        | KernelPursuitStepV1::FileChunks { by_daa, .. }
        | KernelPursuitStepV1::Demand { by_daa, .. } => Some(*by_daa),
        _ => None,
    }
}

/// A queued object's name for the log (the kernel object's own name inside a route carrier).
pub(super) fn palw_kernel_object_name_v1(object: &PalwConsensusObjectV2) -> String {
    match object {
        PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } => {
            KernelRouteObjectV1::decode(bytes).map(|k| k.name().to_string()).unwrap_or_else(|_| "KernelRouteV1".into())
        }
        PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, .. } => format!("KernelRouteChunkV1 {}/{}", chunk.index + 1, chunk.count),
        other => object_name(other).to_string(),
    }
}
