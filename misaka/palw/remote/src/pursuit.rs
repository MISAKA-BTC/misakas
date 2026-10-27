//! **Lane G14-R: a public prosecution on the kernel route that RESUMES — rebuilt from public reads, never from a private cache.**
//!
//! G14's bar asks one public bonded verifier outside every Panel to carry a covered fault to its objective outcome. A verifier is a
//! process: it restarts, its node reorgs, IBDs or imports a pruning snapshot, its RPC connection drops. This module is the part of
//! such a verifier that decides **what to send next**, and it decides it from the chain's public state ALONE (the route rows op 211
//! serves, which rebuild the kernel ledger and the route's chunk lane) plus the verifier's own deterministic finding (its check of
//! the claim against public material and the registered model). It holds no state between calls, so:
//!
//! * **restart** — a verifier that stopped between any two steps (its seal, the seal's maturity, any chunk of a filing, a demand, the
//!   producer's answers, the filing, the court) calls [`plan_kernel_pursuit_v1`] again and gets the step the chain still needs. A seal
//!   the chain holds is never re-sent (an identical re-seal is in any case a no-op on the ledger since G14-R), a chunk the chain holds
//!   is never re-sent, a demand it holds is never re-filed (a re-file would be a free join), and a convicted claim takes nothing more;
//! * **reorg** — the plan is a function of the tip it is given: when a reorg removes the seal, the demand, the served answer or the
//!   filing, the rows say so and the plan re-derives (re-seal, re-demand, re-file); when it re-adds them, the plan sees them and
//!   waits or stops. Nothing is cached across tips, so there is nothing to invalidate;
//! * **IBD / pruned import** — a node that started after the claim, the demand or a partial filing serves the same rows once synced
//!   (the pruning-point carriage carries the route, tail `0xEC`), so the same call on it continues the same pursuit;
//! * **RPC** — the plan reads a [`PalwKernelRouteStateV1`] however it was obtained: the in-process builders or a socket client.
//!   A dropped connection loses nothing; the next read rebuilds the view.
//!
//! **Never a wrong filing.** Before it returns `File` or `Demand` the planner applies the object to a CLONE of the rebuilt ledger at
//! the next DAA — the chain's own rule, run on the chain's own rows — and returns it only if the clone convicts (or opens / joins the
//! demand). A filing the court would dismiss (a fee) or refuse (past the horizon) is reported, never planned.
//!
//! **Deterministic carriers (what "rebuilt" requires of the signer).** A chunked filing's group id is the hash of the SIGNED outer
//! object. A verifier that re-signs after a restart with fresh randomness builds a different group, strands the chain's half-built one
//! (its deposit forfeits at its TTL) and opens a second (a second deposit, and one of the two per-bond slots). The signer of a pursuit's
//! carriers must therefore be deterministic in the message (FIPS 204's deterministic ML-DSA, `rnd = 0^32`, or a key-derived hedge).
//! A group of the opener's that targets the claim under another id is reported as [`KernelPursuitStepV1::Stranded`], never silently
//! duplicated.
//!
//! **Deadlines (G14-B).** [`KernelPursuitStepV1`] steps carry the DAA by which they must land; [`kernel_pursuit_resume_bound_v1`] is
//! the latest DAA a stopped verifier may come back at and still convict (before Final, and within liability): the remaining carriage
//! (one chunk a DAA, [`misaka_palw_kernel::budget::ROUTE_DAA_PER_CHUNK_V1`]) plus one block to fold, subtracted from the claim's
//! horizon. A resumed pursuit needs no seal: a seal buys bounty precedence, never admissibility.
//!
//! Node policy, consensus-inert: nothing here is a rule; the ledger it consults is.

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_kernel_route_v1::{
    PalwKernelChunkGroupV1, PalwKernelChunkTargetV1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_chunks_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, palw_object_chunks_with_cap_v1};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::budget::ROUTE_DAA_PER_CHUNK_V1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{ClaimRowV1, KernelLedgerV1, LedgerEventV1, ProsecutionV1, proof_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::route::{AuthV1, KernelRouteObjectV1};

/// **What the verifier's own check concluded** (its deterministic finding from public material and the registered model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelGoalV1 {
    /// A proof of a covered fault: seal it, file it.
    Prove(ProsecutionV1),
    /// Positions the check could not decide from what is public: demand each (in order); a served one is re-checked by the caller.
    Demand(Vec<(u8, u32)>),
}

/// **One pursuit**: the claim, the verifier's bond (its V2 key: the chunk lane's opener; its kernel id is derived), and its finding.
#[derive(Clone, Debug)]
pub struct KernelPursuitV1 {
    pub claim: Digest,
    pub bond: PalwBondKeyV2,
    pub goal: KernelGoalV1,
    /// **The collateral the fold will sync for this bond** before it applies the verifier's object (the V2 collateral net of every
    /// non-kernel reservation: what op 202's bond row and the route's own reservation give). The route learns a bond only when it
    /// first signs a route object, so a verifier the route has never seen is absent from the rows; the dry run syncs it exactly as
    /// the fold will. `None`: use the rows as they are (a bond the route already knows).
    pub synced_collateral: Option<u64>,
}

impl KernelPursuitV1 {
    /// The verifier's kernel bond id (the accuser / demander the objects name).
    pub fn accuser(&self) -> Digest {
        palw_kernel_bond_id_v1(&self.bond)
    }
}

/// How a pursuit ended (from the chain's rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelPursuitOutcomeV1 {
    /// The claim is convicted (by this verifier's filing or anybody's: one conviction a claim).
    Convicted,
    /// The producer defaulted on a demand (`Unavailable { producer_defaulted }`): the availability outcome, never a conviction.
    Defaulted,
}

/// **The next step the chain still needs**, with the DAA by which it must land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelPursuitStepV1 {
    /// Send `SealProof { seal }` (bounty precedence; never required for admissibility).
    Seal { seal: Digest, by_daa: u64 },
    /// The seal is on chain; file once it is old enough to take the bounty (`file_from_daa`), and no later than `by_daa`.
    AwaitSeal { file_from_daa: u64, by_daa: u64 },
    /// Send the `FileProof` carrier whole (it fits one carrier).
    FileWhole { by_daa: u64 },
    /// Send these chunks (indices) of the filing's group `group` (the chain holds the others).
    FileChunks { group: Hash64, count: u8, missing: Vec<u8>, by_daa: u64 },
    /// Send `FileDemand` for this position (opens it, or joins the open one).
    Demand { stage: u8, position: u32, by_daa: u64 },
    /// This verifier's demand is open: the producer owes the answer by `deadline_daa` (the chain defaults it, nothing to send).
    AwaitAnswer { stage: u8, position: u32, deadline_daa: u64 },
    /// The position is served on chain (public from now on): the caller re-checks with the served bytes, and the goal changes.
    Recheck { stage: u8, position: u32 },
    /// The ledger has no such claim at this tip (not yet included, or reorged out): nothing to send until it reappears.
    NoClaim,
    /// The case is over.
    Done(KernelPursuitOutcomeV1),
    /// The chain's own court would DISMISS this filing (a fee): never sent. An honest verifier's finding should never land here.
    WouldBeDismissed { why: String },
    /// Nothing can change the claim any more (past its horizon, or a demand past its window): never sent, nothing charged.
    TooLate { why: String },
    /// The object is refused for another reason (the verifier's free collateral, a full shared session, …): never sent.
    Refused { why: String },
    /// A half-built group of THIS opener targets the claim under another id (a filing signed with other randomness): the planner
    /// will not open a second group behind it. The caller's signer must be deterministic (module doc).
    Stranded { group: Hash64, expires_daa: u64 },
}

/// **The filing's carrier and how it rides**: the signed outer object (`KernelRouteV1 { FileProof }`) and the chunk cap the
/// verifier's carrier uses (what [`palw_object_chunks_with_cap_v1`] decides "fits" by).
pub struct KernelFilingV1<'a> {
    pub signed: &'a PalwConsensusObjectV2,
    pub chunk_cap: usize,
}

/// The open chunk groups of `opener` that target `claim` (the route's aux table 41, as op 211 serves it).
pub fn opener_groups_for_claim_v1(
    route: &PalwKernelRouteStateV1,
    opener: &PalwBondKeyV2,
    claim: &Digest,
) -> Vec<(Hash64, PalwKernelChunkGroupV1)> {
    route
        .chunk_groups_v1()
        .into_iter()
        .filter(|(o, _, g)| o == opener && g.target == PalwKernelChunkTargetV1::Claim(*claim))
        .map(|(_, id, g)| (id, g))
        .collect()
}

/// **The claim's horizon, conservatively**: `(pre_final_by, close_by)` — the last DAA a conviction lands BEFORE the claim can reach
/// Final (`None` once it is Final or decided), and the last DAA a proof is still admissible (`None` when nothing can change it).
/// The live bound counts the policy's liability (a class's extension only lengthens it, G14-B), so it never overstates the time left.
pub fn kernel_claim_horizon_v1(ledger: &KernelLedgerV1, row: &ClaimRowV1) -> (Option<u64>, Option<u64>) {
    let p = &ledger.policy;
    let liability = p.liability_daa;
    match &row.life.state {
        ClaimStateV1::Final { .. } | ClaimStateV1::Unavailable { .. } => (None, row.liability_until),
        ClaimStateV1::TimedOut { .. } | ClaimStateV1::Convicted { .. } => (None, None),
        state => {
            // Final comes no earlier than the window's end (a served position's grace only delays it).
            let window_end = live_window_end(state, row.committed_daa, p.check_window_daa, p.challenge_window_daa);
            (Some(window_end.saturating_sub(1)), Some(window_end.saturating_add(liability)))
        }
    }
}

fn live_window_end(state: &ClaimStateV1, committed: u64, check: u64, challenge: u64) -> u64 {
    match state {
        ClaimStateV1::ProbabilisticPass { window_end_daa, .. }
        | ClaimStateV1::Challengeable { window_end_daa, .. }
        | ClaimStateV1::WindowClosed { window_end_daa } => *window_end_daa,
        ClaimStateV1::Checking { deadline_daa, .. } => deadline_daa.saturating_add(challenge),
        ClaimStateV1::Disputed { resume, .. } => live_window_end(resume, committed, check, challenge),
        _ => committed.saturating_add(check).saturating_add(challenge),
    }
}

/// **The latest DAA a stopped verifier may resume at and still convict** with `carriage_daa` of carriage left (the filing's
/// missing chunks, one a DAA, plus one block to fold): `(before_final, within_liability)`. `None` where that horizon has passed.
pub fn kernel_pursuit_resume_bound_v1(ledger: &KernelLedgerV1, row: &ClaimRowV1, missing_chunks: u64) -> (Option<u64>, Option<u64>) {
    let carriage = missing_chunks.max(1).saturating_mul(ROUTE_DAA_PER_CHUNK_V1).saturating_add(1);
    let (pre, close) = kernel_claim_horizon_v1(ledger, row);
    let bound = |h: Option<u64>| h.and_then(|h| h.checked_sub(carriage)).filter(|b| *b >= ledger.daa);
    (bound(pre), bound(close))
}

/// The chain's own verdict on `object`, signed by `accuser`, applied to a clone of the rebuilt ledger at the next DAA — after the
/// signer's collateral sync the fold performs first.
fn dry_run(
    ledger: &KernelLedgerV1,
    accuser: &Digest,
    synced: Option<u64>,
    object: &KernelRouteObjectV1,
) -> Result<Vec<LedgerEventV1>, String> {
    let mut l = ledger.clone();
    l.begin_block(ledger.daa.saturating_add(1)).map_err(|r| format!("{r:?}"))?;
    if let Some(collateral) = synced {
        l.sync_bond(*accuser, collateral);
    }
    l.apply_object(object, &AuthV1 { signer_bond: *accuser }).map_err(|r| format!("{r:?}"))
}

fn past_horizon(why: &str) -> bool {
    why.contains("past the liability horizon")
        || why.contains("ended without passing")
        || why.contains("window is closed")
        || why.contains("already decided")
}

/// **The next step of `pursuit`, from the route's public rows at one tip** (module doc). `filing` is the signed `FileProof` carrier
/// (needed only for a `Prove` goal past its seal: it decides whole vs chunked and which chunks are missing).
pub fn plan_kernel_pursuit_v1(
    route: &PalwKernelRouteStateV1,
    pursuit: &KernelPursuitV1,
    filing: Option<&KernelFilingV1<'_>>,
) -> Result<KernelPursuitStepV1, String> {
    plan_kernel_pursuit_on_v1(route, &*route.ledger()?, pursuit, filing)
}

/// **The consumer's view of the route's ledger** (lane G14-K): the rows rebuilt, then the terms the fold injects at every load and
/// never roots — the court scope past `palw_provider_court_v1` (G14-S: 16 units per demander, the clear budget, 4 dismissed filings)
/// and `palw_verifier_pay_v1`'s terms — so a dry run judges a filing or a demand exactly as the next block's fold will.
/// `provider_court` / `verifier_pay`: the fences' activation DAAs where they are in force at the next block, as the consumer resolves
/// them (`None` below them: the ledger is the rows' alone). Not injected (no filing's verdict reads them): the attested artifact set
/// and DA16's provider-liable claims.
pub fn kernel_consumer_ledger_v1(
    route: &PalwKernelRouteStateV1,
    provider_court: Option<u64>,
    verifier_pay: Option<u64>,
) -> Result<kaspa_consensus_core::palw_native_resource_v1::ReservedKernelLedgerV1, String> {
    // The rebuilt ledger keeps its native-resource reservation (it derefs to `KernelLedgerV1`).
    let mut ledger = route.ledger()?;
    ledger.preview_mut_v1().court_scope_policy =
        provider_court.map(|activation_daa| misaka_palw_kernel::court_scope::CourtScopePolicyV1 { activation_daa });
    ledger.preview_mut_v1().verifier_pay_policy =
        verifier_pay.map(kaspa_consensus_core::palw_verifier_pay_v1::palw_verifier_pay_interim_policy_v1);
    Ok(ledger)
}

/// [`plan_kernel_pursuit_v1`] over a ledger the caller rebuilt (the consumer's view, [`kernel_consumer_ledger_v1`]): one rebuild a tick
/// serves every claim the verifier pursues.
pub fn plan_kernel_pursuit_on_v1(
    route: &PalwKernelRouteStateV1,
    ledger: &KernelLedgerV1,
    pursuit: &KernelPursuitV1,
    filing: Option<&KernelFilingV1<'_>>,
) -> Result<KernelPursuitStepV1, String> {
    let ledger = ledger.clone();
    let claim = pursuit.claim;
    let accuser = pursuit.accuser();
    let Some(row) = ledger.claims.get(&claim) else { return Ok(KernelPursuitStepV1::NoClaim) };
    if row.convicted {
        return Ok(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
    }
    let (pre_final, close) = kernel_claim_horizon_v1(&ledger, row);
    let Some(close) = close else {
        return Ok(KernelPursuitStepV1::TooLate { why: format!("the claim is decided: {:?}", row.life.state) });
    };
    // A step lands before Final when it still can; after it, within liability.
    let by = pre_final.filter(|p| *p > ledger.daa).unwrap_or(close);
    match &pursuit.goal {
        KernelGoalV1::Prove(proof) => {
            let file = KernelRouteObjectV1::FileProof { accuser, claim, proof: proof.clone() };
            match dry_run(&ledger, &accuser, pursuit.synced_collateral, &file) {
                Err(why) if past_horizon(&why) => return Ok(KernelPursuitStepV1::TooLate { why }),
                Err(why) => return Ok(KernelPursuitStepV1::Refused { why }),
                Ok(events) => {
                    if events.iter().any(|e| matches!(e, LedgerEventV1::Duplicate { .. })) {
                        return Ok(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted));
                    }
                    if let Some(why) = events.iter().find_map(|e| match e {
                        LedgerEventV1::ProofDismissed { why, .. } => Some(why.clone()),
                        _ => None,
                    }) {
                        return Ok(KernelPursuitStepV1::WouldBeDismissed { why });
                    }
                    if !events.iter().any(|e| matches!(e, LedgerEventV1::Convicted { .. })) {
                        return Ok(KernelPursuitStepV1::Refused { why: format!("the filing convicts nothing: {events:?}") });
                    }
                }
            }
            // The seal: bounty precedence, sent once, never re-sent while the chain holds it, skipped when waiting for it would cost
            // the deadline.
            let delay = ledger.policy.claim_seal_delay_daa;
            let seal = proof_seal_v1(&claim, &accuser, proof);
            let next = ledger.daa.saturating_add(1);
            match ledger.proof_seals.get(&(claim, accuser)) {
                Some(s) if s.seal == seal => {
                    let mature = s.daa.saturating_add(delay);
                    if mature > ledger.daa && mature < by {
                        return Ok(KernelPursuitStepV1::AwaitSeal { file_from_daa: mature, by_daa: by });
                    }
                }
                // No seal of these bytes (none, or another proof's): seal if it can still mature before the deadline.
                _ if next.saturating_add(delay) < by => {
                    let seal_object = KernelRouteObjectV1::SealProof { accuser, claim, seal };
                    if dry_run(&ledger, &accuser, pursuit.synced_collateral, &seal_object).is_ok() {
                        return Ok(KernelPursuitStepV1::Seal { seal, by_daa: by });
                    }
                }
                _ => {}
            }
            let Some(filing) = filing else {
                return Err("a Prove pursuit past its seal needs its signed filing carrier".into());
            };
            plan_filing(route, pursuit, filing, by)
        }
        KernelGoalV1::Demand(positions) => {
            if matches!(row.life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }) {
                return Ok(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Defaulted));
            }
            // Every position of the round is demanded first (one a step), then the earliest open deadline is awaited.
            let mut awaiting: Option<(u8, u32, u64)> = None;
            for &(stage, position) in positions {
                let key = (claim, stage, position);
                if let Some(d) = ledger.demands.get(&key)
                    && d.demanders.iter().any(|(b, _)| *b == accuser)
                {
                    if awaiting.is_none_or(|(_, _, at)| d.deadline_daa < at) {
                        awaiting = Some((stage, position, d.deadline_daa));
                    }
                    continue;
                }
                let demand = KernelRouteObjectV1::FileDemand { demander: accuser, claim, stage, position };
                match dry_run(&ledger, &accuser, pursuit.synced_collateral, &demand) {
                    Ok(_) => return Ok(KernelPursuitStepV1::Demand { stage, position, by_daa: by }),
                    Err(why) if why.contains("already served") => return Ok(KernelPursuitStepV1::Recheck { stage, position }),
                    Err(why) if past_horizon(&why) => return Ok(KernelPursuitStepV1::TooLate { why }),
                    // The per-demander session cap (segmented claims): wait for an open one to close before the next.
                    Err(why) if why.contains("open sessions") && awaiting.is_some() => continue,
                    Err(why) => return Ok(KernelPursuitStepV1::Refused { why }),
                }
            }
            match awaiting {
                Some((stage, position, deadline_daa)) => Ok(KernelPursuitStepV1::AwaitAnswer { stage, position, deadline_daa }),
                None => Err("a Demand pursuit with no position".into()),
            }
        }
    }
}

fn plan_filing(
    route: &PalwKernelRouteStateV1,
    pursuit: &KernelPursuitV1,
    filing: &KernelFilingV1<'_>,
    by: u64,
) -> Result<KernelPursuitStepV1, String> {
    let fits = palw_object_chunks_with_cap_v1(filing.signed, filing.chunk_cap).map_err(|e| format!("{e:?}"))?.is_none();
    if fits {
        return Ok(KernelPursuitStepV1::FileWhole { by_daa: by });
    }
    let chunks = palw_kernel_chunks_v1(filing.signed, pursuit.bond, PalwKernelChunkTargetV1::Claim(pursuit.claim), filing.chunk_cap)?;
    let group = chunks[0].group;
    let count = chunks[0].count;
    let mine = opener_groups_for_claim_v1(route, &pursuit.bond, &pursuit.claim);
    match mine.iter().find(|(id, _)| *id == group) {
        Some((_, g)) => {
            let have: BTreeSet<u8> = g.parts.keys().copied().collect();
            let missing: Vec<u8> = (0..count).filter(|i| !have.contains(i)).collect();
            Ok(KernelPursuitStepV1::FileChunks { group, count, missing, by_daa: by })
        }
        None => match mine.first() {
            Some((other, g)) => Ok(KernelPursuitStepV1::Stranded { group: *other, expires_daa: g.expires_daa }),
            None => Ok(KernelPursuitStepV1::FileChunks { group, count, missing: (0..count).collect(), by_daa: by }),
        },
    }
}

/// **The deterministic ML-DSA-87 randomness of a pursuit carrier** (module doc): FIPS 204's deterministic variant. A verifier that
/// re-signs the same object after a restart produces the same bytes, the same chunk group, the same carrier.
pub const PURSUIT_DETERMINISTIC_RND_V1: [u8; 32] = [0u8; 32];

// ---- Lane G14-K: the service half — what a running verifier (kaspad's `PalwPanelService`) calls every tick ----------------------
//
// The planner above decides the next step from the rows and a goal. A service also needs (1) the goal — its own deterministic check
// of the claim against public material and the registered model it holds (ADR-0177 D7), (2) for a segmented claim, the public
// material the chain's blocks carry (the producer's `Respond` parts, the prompt tiles), and (3) the objects a planned step sends.
// All three are pure functions of public bytes and the verifier's model: nothing here is a rule, and nothing is kept between calls.

/// The fixed chunk cap a service's filings ride the route's chunk lane with (a filing that does not fit one carrier). Fixed so every
/// rebuild after a restart derives the same group (module doc, "deterministic carriers").
pub const KERNEL_SERVICE_CHUNK_CAP_V1: usize = 50_000;

/// A public source holding no producer value: the commitments-only replay must never read one (a read is a bug, answered `None`).
struct NoProducerValuesV1;

impl misaka_palw_kernel::ledger::PublicSourceV1 for NoProducerValuesV1 {
    fn node(&self, _: u8, _: u32, _: u16, _: u16) -> Option<misaka_palw_tir::Tensor> {
        None
    }
}

/// The goal a finding sets (`None`: the claim checks — an honest verifier has nothing to send).
pub fn kernel_goal_of_finding_v1(finding: misaka_palw_kernel::ledger::OutsiderFindingV1) -> Option<KernelGoalV1> {
    use misaka_palw_kernel::ledger::OutsiderFindingV1;
    match finding {
        OutsiderFindingV1::Clean => None,
        OutsiderFindingV1::Prosecute(proof) => Some(KernelGoalV1::Prove(proof)),
        OutsiderFindingV1::Demand(positions) => Some(KernelGoalV1::Demand(positions)),
    }
}

/// **A single-program (K2-TIR v1/v2) or pipeline claim, checked by exact replay** from the ledger's public commitments, the job and
/// the verifier's registered model (`OutsiderV1::check_computation`): no producer value is read, so no position is ever demanded of an
/// honest producer and a lie is proved from what is already public. `Err`: the verifier cannot check (its model does not open to the
/// registered commitments, a typed or segmented claim) — never producer misconduct, never a filing.
pub fn kernel_check_program_v1(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    model: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
    salt: Digest,
) -> Result<Option<KernelGoalV1>, String> {
    misaka_palw_kernel::ledger::OutsiderV1 { ledger, claim: *claim, material: &NoProducerValuesV1, artifact: model, salt }
        .check_computation()
        .map(kernel_goal_of_finding_v1)
}

/// **The registered parameter-commitment root a verifier's ONE model must open to** to check the claims of `class_binding_id`: a
/// single-program or segmented class's own commitments; a `Memory` typed class's rule program (its base weights — the memory's M0
/// comes from them, every later pre-state from the chain's rows). `None` where one model is not enough or none is used: a pipeline
/// (one program a stage), a composite (a model a stage), a retrieval snapshot (no weights), an unknown class.
pub fn kernel_class_model_root_v1(ledger: &KernelLedgerV1, class_binding_id: &Digest) -> Option<Digest> {
    if let Some(class) = ledger.classes.get(class_binding_id) {
        return Some(class.param_commitments.root());
    }
    match &ledger.typed.classes.get(class_binding_id)?.kind {
        misaka_palw_kernel::spec::SpecClassKindV1::Memory { rule, .. } => Some(rule.param_commitments.root()),
        _ => None,
    }
}

/// **A typed-root claim (RFC-0004 Part II), checked by exact replay** from the public commitments, the authenticated public pre-state
/// (the chain's carried post-state, or M0 from the verifier's model) and the model (`SpecOutsiderV1::check_computation`): no producer
/// node value is read. A pre-state nobody publishes is a demand (the claim's DA), never a guess. No public snapshot copy is used
/// (every retrieval slice comes from the chain).
pub fn kernel_check_typed_v1(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    model: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
    salt: Digest,
) -> Result<Option<KernelGoalV1>, String> {
    misaka_palw_kernel::spec::outsider::SpecOutsiderV1 {
        ledger,
        claim: *claim,
        material: &NoProducerValuesV1,
        artifact: model,
        snapshots: &(),
        salt,
    }
    .check_computation()
    .map(kernel_goal_of_finding_v1)
}

/// The public token stream a segmented claim's replay runs on: the job's prompt (inline, or the authenticated tiles `prompt`) and every
/// delivered id but the last (the last is selected, never fed). `None` while the prompt is not public in full.
pub fn kernel_segmented_tokens_v1(
    view: &misaka_palw_kernel::seg_ledger::SegClaimViewV1,
    tiled_prompt: Option<&[u32]>,
) -> Option<Vec<u32>> {
    let prompt: Vec<u32> = match (&view.job.inline_prompt, tiled_prompt) {
        (Some(ids), _) => ids.clone(),
        (None, Some(ids)) if ids.len() == view.job.prompt_len as usize => ids.to_vec(),
        _ => return None,
    };
    let fed = view.generated.len().saturating_sub(1);
    Some(prompt.into_iter().chain(view.generated[..fed].iter().copied()).collect())
}

/// **A segmented (K2-TIR-v4/v5) claim, checked by the verifier's own replay against what the chain serves**: `own` is its replay's
/// position roots (`seg_detect::prepare_reexecution_v1`, a pure function of the record and the model: recomputed by every new
/// process), `served` the positions the blocks carry. Matching roots: `None`. A divergence: the adaptive search's next demand, or the
/// element proof once the served material localizes it (`seg_detect::check_claim_by_reexecution_v1`).
pub fn kernel_check_segmented_v1(
    view: &misaka_palw_kernel::seg_ledger::SegClaimViewV1,
    own: &misaka_palw_kernel::seg_detect::OwnReplayV1,
    served: &dyn misaka_palw_kernel::element::SegMaterialV1,
    model: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
    tokens: &[u32],
) -> Result<Option<KernelGoalV1>, String> {
    use misaka_palw_kernel::element::SegFindingV1;
    let art = |j: u16, l: Option<u16>| model.param(0, j, l);
    let r = misaka_palw_kernel::seg_detect::check_claim_by_reexecution_v1(
        &view.context(),
        &own.roots,
        own.decode_mismatch,
        served,
        &art,
        tokens,
    );
    match r.finding {
        SegFindingV1::Clean => Ok(None),
        SegFindingV1::Fault(f) => Ok(Some(KernelGoalV1::Prove(misaka_palw_kernel::ledger::ProsecutionV1::Segmented(f.to_bytes())))),
        SegFindingV1::Demand(ps) => Ok(Some(KernelGoalV1::Demand(ps.into_iter().map(|p| (0, p)).collect()))),
        SegFindingV1::Inconsistent(why) => Err(why),
    }
}

/// **Every object the given carrier objects make, whole**: the certification lane's `ObjectChunk`s and the route's own chunk lane
/// (tag 113) reassembled per group once every part is present (any order; the first copy of an index wins, as the lane's), every other
/// object as it rode. Input order is kept for whole objects; an assembled one is emitted where its last part was seen.
pub fn kernel_reassemble_objects_v1(objects: impl IntoIterator<Item = PalwConsensusObjectV2>) -> Vec<PalwConsensusObjectV2> {
    use std::collections::BTreeMap;
    type Parts = (u8, BTreeMap<u8, Vec<u8>>);
    let mut generic: BTreeMap<Hash64, Parts> = BTreeMap::new();
    let mut lane: BTreeMap<(PalwBondKeyV2, Hash64), Parts> = BTreeMap::new();
    let mut out = Vec::new();
    let complete = |entry: &mut Parts, out: &mut Vec<PalwConsensusObjectV2>| {
        if entry.1.len() == entry.0 as usize && entry.0 > 0 {
            let whole: Vec<u8> = entry.1.values().flatten().copied().collect();
            if let Ok(o) = borsh::from_slice::<PalwConsensusObjectV2>(&whole) {
                out.push(o);
            }
            entry.1.clear();
            entry.0 = 0; // assembled once
        }
    };
    for o in objects {
        match o {
            PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes } => {
                let entry = generic.entry(group).or_insert_with(|| (count, BTreeMap::new()));
                if entry.0 == count {
                    entry.1.entry(index).or_insert(bytes);
                    complete(entry, &mut out);
                }
            }
            PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, .. } => {
                let entry = lane.entry((chunk.opener, chunk.group)).or_insert_with(|| (chunk.count, BTreeMap::new()));
                if entry.0 == chunk.count {
                    entry.1.entry(chunk.index).or_insert(chunk.bytes);
                    complete(entry, &mut out);
                }
            }
            whole => out.push(whole),
        }
    }
    out
}

/// The kernel route objects (decoded strictly) among whole carrier objects.
fn kernel_route_objects_v1(objects: &[PalwConsensusObjectV2]) -> impl Iterator<Item = KernelRouteObjectV1> + '_ {
    objects.iter().filter_map(|o| match o {
        PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } => KernelRouteObjectV1::decode(bytes).ok(),
        _ => None,
    })
}

/// **A tiled job's prompt from the tiles the blocks carry**, each authenticated against the job's committed root (an inauthentic tile
/// is skipped, the first authentic copy of an index kept). `None` until every tile is present.
pub fn kernel_tiled_prompt_v1(
    objects: &[PalwConsensusObjectV2],
    job_id: &Digest,
    prompt_len: u32,
    prompt_root: &Digest,
) -> Option<Vec<u32>> {
    let tiles = misaka_palw_kernel::seg::prompt_tiles_v1(prompt_len);
    let mut got: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for k in kernel_route_objects_v1(objects) {
        if let KernelRouteObjectV1::PostPromptTile { job, tile } = k
            && job == *job_id
            && tile.authenticates(prompt_len, prompt_root)
        {
            got.entry(tile.index).or_insert(tile.ids);
        }
    }
    (got.len() as u32 == tiles).then(|| got.into_values().flatten().collect())
}

/// **The positions of a segmented claim the chain's blocks serve** — every `Respond` part for the claim (stage 0), assembled and
/// authenticated against the committed segment roots (`seg_da::assemble_position_full_v1`); a position whose parts do not all ride is
/// not material. A hiding value's served leaf hashes are kept beside it (G14-S).
#[derive(Clone, Debug, Default)]
pub struct KernelBlockMaterialV1 {
    pub positions: std::collections::BTreeMap<u32, misaka_palw_kernel::seg_da::AssembledPositionV1>,
    pub hidden: std::collections::BTreeMap<u32, misaka_palw_kernel::seg_da::HiddenLeavesV1>,
}

impl KernelBlockMaterialV1 {
    pub fn from_objects(
        objects: &[PalwConsensusObjectV2],
        claim: &Digest,
        view: &misaka_palw_kernel::seg_ledger::SegClaimViewV1,
    ) -> Self {
        let mut parts: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = std::collections::BTreeMap::new();
        for k in kernel_route_objects_v1(objects) {
            if let KernelRouteObjectV1::Respond { claim: c, stage: 0, position, bytes } = k
                && c == *claim
            {
                parts.entry(position).or_default().push(bytes);
            }
        }
        let mut out = Self::default();
        for (p, parts) in parts {
            if let Some((a, leaves)) =
                misaka_palw_kernel::seg_da::assemble_position_full_v1(&view.program, &view.segment_roots, view.positions, p, &parts)
            {
                out.positions.insert(p, a);
                out.hidden.insert(p, leaves);
            }
        }
        out
    }
}

impl misaka_palw_kernel::element::SegMaterialV1 for KernelBlockMaterialV1 {
    fn position(&self, p: u32) -> Option<Vec<Vec<misaka_palw_tir::Tensor>>> {
        self.positions.get(&p).map(|(values, _, _)| values.clone())
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        self.positions.get(&p).map(|(_, siblings, _)| siblings.clone())
    }
    fn commitments(&self, _: &misaka_palw_tir::program::TirProgramV1, p: u32) -> Option<Vec<Vec<Digest>>> {
        self.positions.get(&p).map(|(_, _, commitments)| commitments.clone())
    }
    fn hidden_leaves(&self, _: &misaka_palw_tir::program::TirProgramV1, p: u32) -> Option<misaka_palw_kernel::seg_da::HiddenLeavesV1> {
        self.hidden.get(&p).cloned()
    }
}

/// One object a planned step sends, unsigned (the service signs it deterministically: a route object over
/// `palw_kernel_route_message_v1`, a chunk over `palw_kernel_chunk_message_v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelSendV1 {
    Route(KernelRouteObjectV1),
    Chunk(kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkV1),
}

/// **What `step` sends** for `pursuit` (`filing`: the signed `FileProof` carrier a `FileWhole` / `FileChunks` step sends, whole or cut
/// at its cap). Steps that wait, end or report send nothing; a step that needs the filing and has none is an error.
pub fn kernel_step_sends_v1(
    step: &KernelPursuitStepV1,
    pursuit: &KernelPursuitV1,
    filing: Option<&KernelFilingV1<'_>>,
) -> Result<Vec<KernelSendV1>, String> {
    let accuser = pursuit.accuser();
    let claim = pursuit.claim;
    Ok(match step {
        KernelPursuitStepV1::Seal { seal, .. } => {
            vec![KernelSendV1::Route(KernelRouteObjectV1::SealProof { accuser, claim, seal: *seal })]
        }
        KernelPursuitStepV1::FileWhole { .. } => {
            let KernelGoalV1::Prove(proof) = &pursuit.goal else { return Err("a filing step without a proof".into()) };
            vec![KernelSendV1::Route(KernelRouteObjectV1::FileProof { accuser, claim, proof: proof.clone() })]
        }
        KernelPursuitStepV1::FileChunks { missing, .. } => {
            let filing = filing.ok_or("a chunked filing step needs its signed carrier")?;
            palw_kernel_chunks_v1(filing.signed, pursuit.bond, PalwKernelChunkTargetV1::Claim(claim), filing.chunk_cap)?
                .into_iter()
                .filter(|c| missing.contains(&c.index))
                .map(KernelSendV1::Chunk)
                .collect()
        }
        KernelPursuitStepV1::Demand { stage, position, .. } => {
            vec![KernelSendV1::Route(KernelRouteObjectV1::FileDemand { demander: accuser, claim, stage: *stage, position: *position })]
        }
        _ => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resume_bound_leaves_the_carriage_and_one_fold() {
        // The arithmetic alone (the node tests run it against real rows): 3 missing chunks need 3 DAA of carriage and 1 to fold.
        let carriage = 3u64 * ROUTE_DAA_PER_CHUNK_V1 + 1;
        assert_eq!(carriage, 4);
        assert_eq!(PURSUIT_DETERMINISTIC_RND_V1, [0u8; 32], "FIPS 204 deterministic signing");
    }

    /// G14-K: the block reader reassembles both chunk lanes whatever order the parts ride in, once, and keeps whole objects.
    #[test]
    fn g14k_both_chunk_lanes_reassemble_in_any_order_once() {
        use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkV1;
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 3));
        let object = PalwConsensusObjectV2::KernelRouteV1 {
            bytes: (0..300u32).map(|i| i as u8).collect(),
            signer: bond,
            signature: vec![7; 90],
        };
        let other = PalwConsensusObjectV2::KernelRouteV1 { bytes: vec![1, 2, 3], signer: bond, signature: vec![9; 4] };
        let mut generic = palw_object_chunks_with_cap_v1(&object, 64).unwrap().expect("several chunks");
        generic.reverse();
        let dup = generic[0].clone();
        let lane: Vec<PalwConsensusObjectV2> = palw_kernel_chunks_v1(&object, bond, PalwKernelChunkTargetV1::Claim([5; 64]), 70)
            .unwrap()
            .into_iter()
            .rev()
            .map(|c: PalwKernelChunkV1| PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(c), signature: vec![] })
            .collect();
        let mut input = vec![other.clone()];
        input.extend(generic);
        input.push(dup);
        input.extend(lane);
        let out = kernel_reassemble_objects_v1(input);
        assert_eq!(out, vec![other, object.clone(), object], "whole first, then each lane's assembly once");
    }

    /// G14-K: what each step sends — a seal, a demand, the filing whole, the missing chunks only; waits send nothing.
    #[test]
    fn g14k_a_step_sends_exactly_its_objects() {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 1));
        let proof = misaka_palw_kernel::ledger::ProsecutionV1::Kernel(vec![4; 200]);
        let pursuit = KernelPursuitV1 { claim: [6; 64], bond, goal: KernelGoalV1::Prove(proof.clone()), synced_collateral: None };
        let me = pursuit.accuser();
        let seal = KernelPursuitStepV1::Seal { seal: [8; 64], by_daa: 9 };
        assert_eq!(
            kernel_step_sends_v1(&seal, &pursuit, None).unwrap(),
            vec![KernelSendV1::Route(KernelRouteObjectV1::SealProof { accuser: me, claim: [6; 64], seal: [8; 64] })]
        );
        let whole = KernelPursuitStepV1::FileWhole { by_daa: 9 };
        assert_eq!(
            kernel_step_sends_v1(&whole, &pursuit, None).unwrap(),
            vec![KernelSendV1::Route(KernelRouteObjectV1::FileProof { accuser: me, claim: [6; 64], proof })]
        );
        let signed = PalwConsensusObjectV2::KernelRouteV1 { bytes: vec![3; 500], signer: bond, signature: vec![1; 10] };
        let chunks = KernelPursuitStepV1::FileChunks { group: Hash64::default(), count: 3, missing: vec![0, 2], by_daa: 9 };
        let sent = kernel_step_sends_v1(&chunks, &pursuit, Some(&KernelFilingV1 { signed: &signed, chunk_cap: 200 })).unwrap();
        let idx: Vec<u8> = sent.iter().map(|s| if let KernelSendV1::Chunk(c) = s { c.index } else { 99 }).collect();
        assert_eq!(idx, vec![0, 2], "only the chunks the chain lacks");
        assert!(kernel_step_sends_v1(&chunks, &pursuit, None).is_err(), "a chunk step needs its signed carrier");
        let demand = KernelPursuitStepV1::Demand { stage: 0, position: 12, by_daa: 9 };
        assert_eq!(
            kernel_step_sends_v1(&demand, &pursuit, None).unwrap(),
            vec![KernelSendV1::Route(KernelRouteObjectV1::FileDemand { demander: me, claim: [6; 64], stage: 0, position: 12 })]
        );
        for wait in [
            KernelPursuitStepV1::AwaitSeal { file_from_daa: 1, by_daa: 2 },
            KernelPursuitStepV1::AwaitAnswer { stage: 0, position: 1, deadline_daa: 3 },
            KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted),
            KernelPursuitStepV1::WouldBeDismissed { why: "x".into() },
        ] {
            assert!(kernel_step_sends_v1(&wait, &pursuit, None).unwrap().is_empty(), "{wait:?} sends nothing");
        }
    }

    #[test]
    fn horizon_words_are_the_ledgers_refusals() {
        for why in
            ["past the liability horizon", "the claim ended without passing and holds nothing", "the challenge window is closed"]
        {
            assert!(past_horizon(why), "{why}");
        }
        assert!(!past_horizon("the accuser's free collateral does not cover the filing fee"));
    }
}

/// The kernel model file's magic (`--palw-kernel-model`, `misaka palw build file-proof --model`).
pub const KERNEL_MODEL_FILE_MAGIC_V1: &[u8; 8] = b"PALWKM01";

/// **Read a kernel model file** (`PALWKM01` + Borsh of `(layer, slot, tensor)` rows) — shared by the node's verifier and the CLI's
/// outsider filer, so both replay a claim against the same tensors.
pub fn kernel_model_read_v1(path: &std::path::Path) -> Result<misaka_palw_tir::MapParams, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let body = bytes.strip_prefix(KERNEL_MODEL_FILE_MAGIC_V1.as_slice()).ok_or("not a kernel model file (PALWKM01)")?;
    let rows: Vec<(u16, Option<u16>, misaka_palw_kernel::public::TensorWireV1)> =
        borsh::from_slice(body).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut params = misaka_palw_tir::MapParams::default();
    for (j, l, wire) in rows {
        params.tensors.insert((j, l), wire.decode().map_err(|e| format!("{}: tensor ({j}, {l:?}): {e}", path.display()))?);
    }
    Ok(params)
}

/// The parameter-commitment roots a model opens to (v1/v2 single programs commit `of`, v4/v5 `of_v3`).
pub fn kernel_model_roots_v1(params: &misaka_palw_tir::MapParams) -> [Digest; 2] {
    [
        misaka_palw_kernel::trace::ParamCommitmentsV1::of(params).root(),
        misaka_palw_kernel::trace::ParamCommitmentsV1::of_v3(params).root(),
    ]
}
