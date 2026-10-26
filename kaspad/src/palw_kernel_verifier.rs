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
use crate::palw_panel::court_outbox::CourtOutboxV1;
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

/// **Lane g14-court-capacity-r2: the court queue's round of this bond's kernel interim-seat receipts** — distinct from the verifier's
/// filings (`PALW_KERNEL_VERIFIER_ROUND_V1`) and every other lane's.
pub(super) const PALW_KERNEL_SEAT_ROUND_V1: u32 = u32::MAX - 20;

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
    /// Lane g14-court-capacity-r2: the interim-seat duties this bond has decided, claim → `true` signed a passing receipt /
    /// `false` refused it (its own check found a fault: the public verifier below files it).
    seat_decided: HashMap<Digest, bool>,
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

    /// Lane KNODE: the held model a registered root opens to, for the slice roles (`palw_kernel_roles`) that share this book's models.
    pub(super) fn model_for_root_v1(&self, registered_root: &Digest) -> Option<std::sync::Arc<MapParams>> {
        self.model_for(registered_root)
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
        court_outbox: &mut CourtOutboxV1,
    ) {
        // What the priority lane carried of this verifier's items: said and forgotten (it keeps its own spacing, never the debounce).
        court_outbox.retain_submitted(|(claim, round, responder), at| {
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
                court_outbox.retain_pending(|(_, round, responder, _)| *round != PALW_KERNEL_VERIFIER_ROUND_V1 || *responder);
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
        // Lane g14-court-capacity-r2 (`palw_detection_policy_v1`, dormant): the verifier holds at most its class policy's
        // `claims_in_flight` claims of a class at once — the oldest commits first (closest to their horizons); a claim past the cap
        // waits for a slot and is counted (`CapacityRefusal`), never dropped silently.
        let candidates = palw_kernel_verifier_admit_v1(
            candidates,
            |id| {
                let row = &ledger.claims[id];
                (Hash64::from_bytes(row.class_binding_id), row.committed_daa)
            },
            |class| match kaspa_consensus_core::palw_detection_policy_v1::palw_detection_regime_v1(
                params.palw_detection_policy_v1.as_ref(),
                class,
                0,
                next,
            ) {
                kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionRegimeV1::Policy(p) => Some(p.resources.claims_in_flight),
                _ => None,
            },
            |class, id| {
                self.config.telemetry.detection_loss_once(
                    class,
                    Hash64::from_bytes(id),
                    kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionLossV1::CapacityRefusal,
                )
            },
        );
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
        // **Lane g14-court-capacity-r2: the kernel route's interim SEAT on real nodes.** For every open single-program claim whose
        // Panel assignment names this bond, before the assignment's deadline and not yet receipted by it: the claim is re-executed
        // from the public commitments with the registered model (the public verifier's own replay, cached below), and this bond's
        // assigned scope is checked against its OWN re-executed trace (`FreshVerifierV1::check` — every value authenticated against
        // the claim's commitments, the claim's public challenge). Only a clean replay AND a passing scope check signs a passing
        // receipt; a fault signs nothing (the verifier pass below files it). The policy's `claims_in_flight` bounds the duties held.
        let base_class = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.base_class_id),
            _ => None,
        };
        let operator = match base_class {
            Some(base) => session
                .clone()
                .spawn_blocking(move |c| c.palw_producer_facts_v2(base, Some(bond_key.0)))
                .await
                .and_then(|facts| facts.bond)
                .map(|b| b.operator_id),
            None => None,
        };
        if let Some(operator) = operator {
            let seat_salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-seat/salt/v1", &me);
            for claim in &candidates {
                let Some(object) =
                    self.kernel_seat_receipt_v1(book, &route, &ledger, claim, bond_key, operator, current_daa, seat_salt, &objects)
                else {
                    continue;
                };
                let key = palw_kernel_object_key_v1(&object);
                let queued = court_outbox.pending().iter().any(|(_, round, _, o)| *round == PALW_KERNEL_SEAT_ROUND_V1 && *o == object);
                if queued || in_pool.contains(&key) {
                    continue;
                }
                let session_key = Hash64::from_bytes(*claim);
                info!("[{PALW_PANEL}] claim {session_key}: this interim seat queues its passing receipt (kernel route, G14-K)");
                let deadline = route.assignment_of(claim).map_or(current_daa, |a| a.deadline_daa);
                court_outbox.set_due((session_key, PALW_KERNEL_SEAT_ROUND_V1, false), deadline);
                court_outbox.push_pending((session_key, PALW_KERNEL_SEAT_ROUND_V1, false, object));
            }
        }
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
                let queued =
                    court_outbox.pending().iter().any(|(_, round, _, o)| *round == PALW_KERNEL_VERIFIER_ROUND_V1 && *o == object);
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
                court_outbox.set_due(
                    (session_key, PALW_KERNEL_VERIFIER_ROUND_V1, false),
                    palw_kernel_step_due_v1(&step).unwrap_or(current_daa),
                );
                court_outbox.push_pending((session_key, PALW_KERNEL_VERIFIER_ROUND_V1, false, object));
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

    /// **This bond's passing receipt for its interim-seat duty on `claim`, if it owes one now and its checks pass** (lane
    /// g14-court-capacity-r2). `None`: not a seat of this claim, past the deadline, already receipted (on chain or decided here), the
    /// duty refused for `claims_in_flight`, or a check that did not pass (a fault is logged and left to the public verifier's filing).
    #[allow(clippy::too_many_arguments)]
    fn kernel_seat_receipt_v1(
        &self,
        book: &mut PalwKernelVerifierBookV1,
        route: &kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1,
        ledger: &KernelLedgerV1,
        claim: &Digest,
        bond_key: PalwBondKeyV2,
        operator: Hash64,
        current_daa: u64,
        salt: Digest,
        objects: &[PalwConsensusObjectV2],
    ) -> Option<PalwConsensusObjectV2> {
        use misaka_palw_kernel::receipt::{CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1, ReceiptInputsV1, receipt_for_v1};
        use misaka_palw_kernel::verify::ScopeVerdictV1;
        let assignment = route.assignment_of(claim)?;
        let seat = assignment.seats.iter().find(|s| s.bond == bond_key)?.clone();
        if current_daa >= assignment.deadline_daa || book.seat_decided.contains_key(claim) {
            return None;
        }
        if route.receipts_of(claim).iter().any(|r| r.seat_bond == seat.kernel_bond) {
            book.seat_decided.insert(*claim, true);
            return None;
        }
        let row = ledger.claims.get(claim)?;
        let ClaimBodyV1::Program { claim: kernel_claim, evidence, .. } = &row.body else { return None };
        // Receipts count only while the claim awaits its Panel's coverage.
        if !matches!(row.life.state, ClaimStateV1::Committed | ClaimStateV1::ChallengeBound { .. } | ClaimStateV1::Checking { .. }) {
            return None;
        }
        let class = ledger.classes.get(&row.class_binding_id)?;
        // The policy's `claims_in_flight`: the seat holds at most that many duties of a class (to their deadlines).
        let class_key = Hash64::from_bytes(row.class_binding_id);
        if let kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionRegimeV1::Policy(policy) =
            kaspa_consensus_core::palw_detection_policy_v1::palw_detection_regime_v1(
                self.consensus_config.params.palw_detection_policy_v1.as_ref(),
                &class_key,
                0,
                current_daa.saturating_add(1),
            )
            && !self.config.telemetry.detection_take_duty_v1(
                class_key,
                Hash64::from_bytes(*claim),
                current_daa,
                policy.resources.claims_in_flight,
                assignment.deadline_daa,
            )
        {
            self.config.telemetry.detection_loss_once(
                class_key,
                Hash64::from_bytes(*claim),
                kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionLossV1::CapacityRefusal,
            );
            return None;
        }
        let root = class.param_commitments.root();
        let model = book.model_for(&root)?;
        // 1. The whole claim, re-executed from the public commitments (the verifier's replay, cached per claim).
        let refuted = match self.kernel_verifier_goal_v1(book, ledger, claim, model.as_ref(), salt, objects) {
            Ok(goal) => goal.is_some(),
            Err(why) => {
                warn!("[{PALW_PANEL}] kernel seat: claim {} is not checkable here: {why} — no receipt", Hash64::from_bytes(*claim));
                return None;
            }
        };
        // 2. The assigned scope, checked against this seat's own re-executed trace.
        let job = ledger.jobs.get(&row.job_id)?;
        let fed = kernel_claim.generated.len().saturating_sub(1);
        let stream: Vec<u32> = job.prompt.iter().chain(&kernel_claim.generated[..fed]).copied().collect();
        let verdict = (!refuted)
            .then(|| misaka_palw_kernel::trace::trace_v1(&class.program, model.as_ref(), &stream).ok())
            .flatten()
            .and_then(|trace| {
                let (record, header) = ledger.public_record(claim)?;
                let fresh =
                    misaka_palw_kernel::public::FreshVerifierV1::from_public_bytes(&record.to_bytes(), &ledger.known, header).ok()?;
                Some(fresh.check(&misaka_palw_kernel::verify::TraceMaterialV1 { trace: &trace, params: model.as_ref() }, &seat.scope))
            });
        let Some(verdict @ ScopeVerdictV1::Pass { .. }) = verdict else {
            warn!(
                "[{PALW_PANEL}] kernel seat: claim {}: its own check does not pass ({}) — this seat signs nothing; the public verifier \
                 files (G14-K)",
                Hash64::from_bytes(*claim),
                if refuted { "the replay refutes it".to_string() } else { format!("{verdict:?}") }
            );
            book.seat_decided.insert(*claim, false);
            return None;
        };
        let inputs = ReceiptInputsV1 {
            descriptor: &class.descriptor,
            evidence,
            claim_id: *claim,
            assignment_root: assignment.assignment_root,
            challenge_anchor: assignment.challenge_anchor,
            sample_seed: assignment.sample_seed,
            seat_bond: seat.kernel_bond,
            seat_operator: operator.as_bytes(),
            signed_daa: current_daa,
        };
        let receipt = receipt_for_v1(&inputs, &seat.scope, &verdict)?;
        let signature = self.sign(receipt.signing_message().as_ref(), CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1)?;
        book.seat_decided.insert(*claim, true);
        Some(PalwConsensusObjectV2::KernelConstraintReceiptV1 { receipt: Box::new(receipt), signature })
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
/// **Lane g14-court-capacity-r2: the claims the kernel verifier takes this tick** — per class with a detection policy, the
/// `claims_in_flight` oldest (by commit DAA, then id); the rest are reported through `refused`. A class with no policy (below the
/// fence) is not capped. Pure.
pub(crate) fn palw_kernel_verifier_admit_v1(
    mut candidates: Vec<Digest>,
    facts: impl Fn(&Digest) -> (Hash64, u64),
    cap_of: impl Fn(&Hash64) -> Option<u32>,
    mut refused: impl FnMut(Hash64, Digest),
) -> Vec<Digest> {
    candidates.sort_by_key(|id| (facts(id).1, *id));
    let mut taken: std::collections::BTreeMap<Hash64, u32> = std::collections::BTreeMap::new();
    candidates
        .into_iter()
        .filter(|id| {
            let (class, _) = facts(id);
            let Some(cap) = cap_of(&class) else { return true };
            let n = taken.entry(class).or_default();
            if *n < cap {
                *n += 1;
                true
            } else {
                refused(class, *id);
                false
            }
        })
        .collect()
}

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

#[cfg(test)]
mod claims_in_flight_tests {
    use super::*;

    /// **The kernel verifier takes no more claims of a class than its policy's `claims_in_flight`**, oldest commits first; the rest are
    /// reported (counted as `CapacityRefusal`); a class with no policy is not capped.
    #[test]
    fn g14r2_the_kernel_verifier_respects_claims_in_flight() {
        let d = |n: u8| [n; 64];
        let capped = Hash64::from_u64_word(1);
        let free = Hash64::from_u64_word(2);
        let facts = |id: &Digest| match id[0] {
            1 => (capped, 30),
            2 => (capped, 10),
            3 => (capped, 20),
            _ => (free, 5),
        };
        let mut refused = Vec::new();
        let taken = palw_kernel_verifier_admit_v1(
            vec![d(1), d(2), d(3), d(4), d(5)],
            facts,
            |class| (*class == capped).then_some(2),
            |class, id| refused.push((class, id)),
        );
        assert_eq!(taken, vec![d(4), d(5), d(2), d(3)], "the two oldest of the capped class, every claim of the free one");
        assert_eq!(refused, vec![(capped, d(1))], "the newest capped claim waits and is counted");
    }
}
