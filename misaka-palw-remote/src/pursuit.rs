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
    let ledger = route.ledger()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resume_bound_leaves_the_carriage_and_one_fold() {
        // The arithmetic alone (the node tests run it against real rows): 3 missing chunks need 3 DAA of carriage and 1 to fold.
        let carriage = 3u64.max(1) * ROUTE_DAA_PER_CHUNK_V1 + 1;
        assert_eq!(carriage, 4);
        assert_eq!(PURSUIT_DETERMINISTIC_RND_V1, [0u8; 32], "FIPS 204 deterministic signing");
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
