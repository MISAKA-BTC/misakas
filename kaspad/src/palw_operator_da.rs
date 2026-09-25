//! **Lane B of the panel-seed stopgap (2026-09-26): the operator's non-seat data-availability
//! filer, gated by the operator's own replay** (node policy; a child module of `palw_panel`, whose
//! loop holds its one book and calls [`PalwPanelService::operator_da_tick_v1`] once a tick).
//!
//! **Why.** The panel-seed CRITICAL (`docs/t12-panel-seed-2026-09-25.md`): while audit P0-10 is
//! open a lottery win costs ~279 junk BLAKE2b draws, so an attacker re-rolls a claim's panel until
//! its own Sybil seats cover it, and a coverage lie reaches `Final` with 0 slash. The user's decision
//! of 2026-09-26 01:00 pairs fence F1 and the operator-anchor fence (lane A) with this node duty:
//! **an operator node that is not a seat on a claim's panel checks the claim, and accuses it**, so a
//! lie meets an honest operator even when its panel is captured (ADR-0152 §3.9 option 2, DA-1 /
//! DA-8: any bond may accuse; a non-seat session neither pauses the claim nor spends a seat's
//! budget).
//!
//! **What an accusation does** (the DA court as it stands at `0e8ec984e`, ADR-0152 §3.11): the
//! object is P2-6's own — a `DefaultAccused` naming row 0, tile 0 (C-9), built by the ONE builder
//! (`palw_da_accusation_object_v1`) and asked of the fold first (`palw_da_accusation_check_v1`); the
//! producer (or a covering `Valid` signer, X7) must answer every unit within `W_disclose` (1,200 DAA
//! on testnet-12). **A claim with no material behind its roots** (P0-10's junk) cannot answer: the
//! session defaults (S1, or S3 plus the vesting row after `Final`, S4 on covering signers) and the
//! accuser's exposure comes back with the reward. **Any claim whose roots open answers** — an honest
//! one, and a self-consistent garbage trace alike, since the court checks the answer by hash
//! arithmetic against the claim's own roots — and the session is refuted: its exposure
//! (`min(10% · S_P(stage), min_collateral)`: 320.10 MSK on the floor, 369.52 on 8k, 6,294.38 on 2M)
//! is held on the accuser's free half until the claim record retires (~3,122 DAA after the licence)
//! and then BURNED off a genesis bond that no object can top up (the lane B review, finding 2).
//!
//! **So the operator replays before it accuses — the replay gate** (the review's findings 1–2). A
//! claim is judged by the node's own replay of its job wherever the operator can run it: an attempt
//! claim (its job is derived from its block's header, `attempt_job_for_claim`, never served) of a
//! light class (outside C7 — no operator lane takes a heavy replay) whose class an eligible operator
//! bond declared capable. Up to [`PALW_OPERATOR_DA_JUDGES_V1`] such bonds are the claim's JUDGES, in
//! rank order, one turn each; on its turn a judge replays the claim off the loop (its own replay
//! slot, the host ledger's reservation — the SEAT-R runner, `PalwSeatReplaysV1`) and:
//! * **the roots and answer reproduce** — honest: nothing is filed, nothing is burned;
//! * **they do not** — a coverage lie, said loudly (`error!`): the judge accuses at once, turn or no
//!   turn. Junk defaults and is convicted. A garbage trace answers the accusation (row 0 opens), so
//!   the DA court does not convict it: that needs a non-seat `ExecutorRefuted` / `StepLeaf` demand
//!   built from the claim's capture (P2-8b/8d for non-seats), not built here — the error line is the
//!   operator's signal (decision (C)'s monitoring), and lane A is the structural fence;
//! * **this host cannot run it** (the class does not resolve here, the block is not held, the
//!   replay refused twice) — said loudly, and the next judge's turn comes.
//!
//! A claim NO operator can judge (a free prompt, whose job is served, not derived; a C7 class; a
//! class no eligible operator declared) is accused **blind**, exactly as before the gate — every
//! eligible operator bond in rank order, one turn each — but only within the blind budget below.
//!
//! **The budget** ([`palw_operator_da_budget_v1`], the review's findings 2–3), over the bond's
//! standing read off the chain once a DAA (`palw_operator_da_standing_v1`) plus this node's filings
//! not yet on chain:
//! * **the seat reserve** — no lane B filing leaves less than [`PALW_OPERATOR_DA_SEAT_RESERVE_PERMILLE_V1`]
//!   of the bond's collateral as A-6 room (234,766 MSK of a 939,063 MSK genesis bond: half the free
//!   half), so the node's own seat duties — a court at a claim's reservation, a P2-6 seat session, a
//!   held dissection — always find room on the free half they share with this lane;
//! * **the blind cap** — a blind filing is made only while the bond's held DA exposure stays within
//!   [`PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1`] of its collateral (18,781 MSK: ~58 floor
//!   sessions). A refuted exposure is held ~3,122 DAA before it burns, so blind filings can burn at
//!   most ~18.8k MSK per ~4.3 days per bond (≤ 0.46% of a genesis bond a day), however many
//!   unjudgeable external claims arrive;
//! * **the collateral floor** — nothing is filed once the bond's collateral net of slashes is below
//!   [`PALW_OPERATOR_DA_COLLATERAL_FLOOR_PERMILLE_V1`] of its genesis registration: lane B can never
//!   take a genesis bond below 90%, and says so loudly when it stops.
//!
//! A filing the budget refuses waits a re-plan; the turn passes to the next owner, whose bond has its
//! own budget.
//!
//! **Which claims** (`palw_operator_da_v1::palw_operator_da_candidates_v1`, the chain's read): licensed
//! (`ReceiptLicensed`, or `Final` with an unmatured vesting row) claims produced OUTSIDE the operator
//! set that relied on at least one `Valid` signer outside it. (When X10 arms, unaccused unlicensed
//! withholding becomes free and the read must offer the `Live` stage too.)
//!
//! **Who** — identity decides, nothing opts in ([[protocol-duties-are-always-on-not-flags]]): a node
//! whose bond is one of the operator's bonds — the bundle's genesis registrations
//! ([`palw_operator_registrations_v1`], testnet-12's eight cards `5e0d5f1b…:0..7`) — that carries
//! (`--palw-fee-outpoint`), past `palw_rcore_plus`. Anything else is a clean no-op.
//!
//! **One owner at a time** ([`palw_operator_da_plan_v1`]): the claim's pool — its judges, or with none
//! every eligible operator bond (neither its producer nor a seat of its current panel) — is ranked by
//! `H(domain ‖ claim ‖ bond)` ([`palw_operator_da_order_v1`]), the same on every node off the same
//! tip. Time from the stage's DAA is cut into turns of [`PALW_OPERATOR_DA_TURN_DAA_V1`]; turn `t`
//! belongs to rank `t mod n`. A node that is down lets its turn pass. **Backoff:** a claim on which
//! any operator bond already holds (or held) a session is left alone; an accuser OUTSIDE the
//! operator set is not a reason to back off (it may be the producer's own Sybil).
//!
//! **Load.** At most one replay of this lane in flight, started only while the seat's own replay
//! slots have room for a light replay (a seat's duty is never queued behind it); one item on the
//! court queue at a time, at least [`PALW_OPERATOR_DA_MIN_GAP_DAA_V1`] between two filings, a claim
//! sent at most [`PALW_OPERATOR_DA_SENDS_V1`] times; the chain read once a DAA. The item rides the
//! priority lane dated at its turn's end, behind every seat's court filing.
//!
//! **Consensus-inert.** Every object is one the fold already admits from any bond; nothing here is a
//! rule, an id or a fingerprint.

use super::*;
use kaspa_core::error;
use kaspa_consensus_core::palw_da_rcore_v1::{PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1, PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1};
use kaspa_consensus_core::palw_operator_da_v1::{PalwOperatorDaCandidateV1, PalwOperatorDaJobV1, PalwOperatorDaStandingV1};
use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1;

/// The court queue's round of this lane's accusation of a claim ([`palw_operator_da_queue_key_v1`]) —
/// a round no court move, P2-6 accusation (`u32::MAX`), reporter-filer object (`u32::MAX − 3 ..=
/// u32::MAX − 1`), P2-8d demand (`u32::MAX − 6`) or answer (a folded unit digest) is keyed by.
pub(super) const PALW_OPERATOR_DA_QUEUE_ROUND_V1: u32 = u32::MAX - 7;
/// The domain of an eligible accuser's rank on a claim.
pub(super) const PALW_OPERATOR_DA_RANK_DOMAIN_V1: &[u8] = b"misaka-node/operator-da/rank/v1";
/// **One owner's turn, in DAA** — long enough for a light replay to return and a carrier sent at its
/// end to land and be read back (a carrier lands in a DAA or two; the pre-t12 drill's slowest proof
/// took ~3), so the next owner finds the session and backs off; short enough that a claim whose
/// owner is down is judged within an hour of its licence.
pub(super) const PALW_OPERATOR_DA_TURN_DAA_V1: u64 = 30;
/// The fewest DAA between two filings of one node (the per-DAA cap: at most one every two DAA).
pub(super) const PALW_OPERATOR_DA_MIN_GAP_DAA_V1: u64 = 2;
/// A filing not on chain this long after it was queued is taken as lost, and may be sent once more.
pub(super) const PALW_OPERATOR_DA_RESEND_DAA_V1: u64 = PALW_OPERATOR_DA_TURN_DAA_V1;
/// Sends one node makes of one claim's accusation: the first, and one more if the first was lost.
pub(super) const PALW_OPERATOR_DA_SENDS_V1: u8 = 2;
/// **The judges a claim gets at most**: the first this many capable eligible operator bonds in rank
/// order each replay it once on its turn. An honest claim costs at most this many replays across the
/// fleet (and one more per node restart); a lie is accused by the first judge that is up.
pub(super) const PALW_OPERATOR_DA_JUDGES_V1: usize = 3;
/// **The seat reserve**, ‰ of the bond's collateral: A-6 room no lane B filing may take — half the
/// free half, kept for the node's own seat duties (courts, P2-6 seat sessions, held dissections).
pub(super) const PALW_OPERATOR_DA_SEAT_RESERVE_PERMILLE_V1: u128 = 250;
/// **The blind cap**, ‰ of the bond's collateral: the DA exposure the bond may hold when it files a
/// blind accusation (one no replay judged). With a refuted exposure held ~3,122 DAA before it burns,
/// this bounds what blind filings burn to ≤ 0.46% of the bond a day.
pub(super) const PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1: u128 = 20;
/// **The collateral floor**, ‰ of the bond's genesis registration: below it (net of every slash)
/// lane B files nothing — it never takes an operator bond below 90%.
pub(super) const PALW_OPERATOR_DA_COLLATERAL_FLOOR_PERMILLE_V1: u128 = 900;
/// The host ledger's role for this lane's replays.
pub(super) const PALW_OPERATOR_DA_REPLAY_ROLE_V1: &str = "operator-da";

/// The court queue's key of this lane's accusation of `claim` — one a claim.
pub(super) fn palw_operator_da_queue_key_v1(claim: Hash64) -> (Hash64, u32, bool) {
    (claim, PALW_OPERATOR_DA_QUEUE_ROUND_V1, false)
}

/// Whether a court-queue entry is this lane's.
pub(super) fn palw_operator_da_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::DefaultAccused { .. }) && (round, responder) == (PALW_OPERATOR_DA_QUEUE_ROUND_V1, false)
}

/// **The operator's bonds: the bundle's genesis registrations**, in bond order, each with the
/// collateral it was registered with — on testnet-12 the eight cards of the premine `5e0d5f1b…`,
/// outputs 0..7. Empty off `ConsensusV2`.
pub(crate) fn palw_operator_registrations_v1(params: &kaspa_consensus_core::config::params::Params) -> Vec<(PalwBondKeyV2, u64)> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Vec::new();
    };
    let bonds: BTreeMap<PalwBondKeyV2, u64> = bundle
        .genesis_objects
        .iter()
        .filter_map(|object| match object {
            PalwConsensusObjectV2::BondRegistered { bond, collateral, .. } => Some((*bond, *collateral)),
            _ => None,
        })
        .collect();
    bonds.into_iter().collect()
}

/// **Is the lane armed on this node?** Past `palw_rcore_plus` (the DA court), on a node that carries,
/// whose bond is an operator's. Identity decides; there is no flag.
pub(super) fn palw_operator_da_armed_v1(rcore_plus: bool, carries: bool, operator: bool) -> bool {
    rcore_plus && carries && operator
}

/// `H(PALW_OPERATOR_DA_RANK_DOMAIN_V1 ‖ claim ‖ bond)` — an eligible accuser's place on a claim.
pub(crate) fn palw_operator_da_rank_key_v1(claim: &Hash64, bond: &PalwBondKeyV2) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_OPERATOR_DA_RANK_DOMAIN_V1).to_state();
    state.update(claim.as_byte_slice());
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The operator bonds that may accuse `claim` as non-seats, in the order their turns come**: every
/// bond of `operators` that is neither the producer nor a seat of the claim's current panel, by
/// [`palw_operator_da_rank_key_v1`] (the bond itself breaks a tie).
pub(crate) fn palw_operator_da_order_v1(claim: &PalwOperatorDaCandidateV1, operators: &[PalwBondKeyV2]) -> Vec<PalwBondKeyV2> {
    let mut order: Vec<(Hash64, PalwBondKeyV2)> = operators
        .iter()
        .filter(|bond| **bond != claim.producer && !claim.seats.contains(bond))
        .map(|bond| (palw_operator_da_rank_key_v1(&claim.claim_id, bond), *bond))
        .collect();
    order.sort();
    order.dedup_by(|a, b| a.1 == b.1);
    order.into_iter().map(|(_, bond)| bond).collect()
}

/// **Whether the operator lane judges a claim by replay at all**: an attempt claim (its job is the
/// chain's, derived from its block — a free prompt's is served), of a class outside C7 (no operator
/// replay holds a heavy slot), whose class the registry still holds.
pub(crate) fn palw_operator_da_replayable_v1(job: &PalwOperatorDaJobV1) -> bool {
    !job.free_prompt && !job.held_to_final && job.artifact_root.is_some()
}

/// **The claim's judges** — the eligible operator bonds ([`palw_operator_da_order_v1`]) that declared
/// the claim's class capable, in rank order, at most [`PALW_OPERATOR_DA_JUDGES_V1`]; none when the
/// lane does not replay the claim ([`palw_operator_da_replayable_v1`]).
pub(crate) fn palw_operator_da_judges_v1(claim: &PalwOperatorDaCandidateV1, operators: &[PalwBondKeyV2]) -> Vec<PalwBondKeyV2> {
    if !palw_operator_da_replayable_v1(&claim.job) {
        return Vec::new();
    }
    palw_operator_da_order_v1(claim, operators)
        .into_iter()
        .filter(|bond| claim.capable.contains(bond))
        .take(PALW_OPERATOR_DA_JUDGES_V1)
        .collect()
}

/// Why a node does not act on a claim now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaSkipV1 {
    /// This node's bond is not an operator's.
    NotOperator,
    /// This node produced the claim (the fold refuses it: `DaAccuserIsTheProducer`).
    Producer,
    /// This node is a seat of the claim's current panel: its accusation is P2-6's, never this lane's.
    Seat,
    /// An operator bond already accuses (or accused) the claim: an honest accuser is there.
    OperatorAccuses,
    /// DA-8's sixteen non-seat sessions are spent on the claim and no operator got one — its
    /// producer's Sybils may have filled the budget to shield it. Nothing can be filed; said loudly.
    Shielded,
    /// The claim has judges and this node is not one: their replays decide it, and a blind
    /// accusation here would burn an honest claim they reproduced.
    NotAJudge,
    /// No operator bond is eligible (every one is the producer or a seat).
    NoneEligible,
}

/// **What this node does about one candidate now** (the turn rule alone; the book adds the verdict).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaPlanV1 {
    /// This node's turn to JUDGE the claim: replay it, and accuse only if the replay refutes it.
    /// `rank` of `of` judges; `due` the turn's end.
    Judge { rank: usize, of: usize, due: u64 },
    /// This node's turn to accuse BLIND (no operator can judge the claim by replay), within the blind
    /// budget, landing by `due` (the turn's end). `rank` of `of` eligible.
    File { rank: usize, of: usize, due: u64 },
    /// Not now: this node's next turn starts at `from_daa` (or DA-8's three open non-seat sessions
    /// are full, and `from_daa` is the next DAA).
    Wait { from_daa: u64 },
    Skip(PalwOperatorDaSkipV1),
}

/// **The turn rule** (the module's "One owner at a time"): the claim's pool is its judges, or with
/// none every eligible operator bond; turn `t = ⌊(now − stage_daa) / PALW_OPERATOR_DA_TURN_DAA_V1⌋`
/// belongs to rank `t mod n` of the pool. Pure over the candidate, so every node on the same tip
/// gets the same answer.
pub(crate) fn palw_operator_da_plan_v1(
    claim: &PalwOperatorDaCandidateV1,
    me: &PalwBondKeyV2,
    operators: &[PalwBondKeyV2],
    now_daa: u64,
) -> PalwOperatorDaPlanV1 {
    use PalwOperatorDaPlanV1 as P;
    use PalwOperatorDaSkipV1 as S;
    if !operators.contains(me) {
        return P::Skip(S::NotOperator);
    }
    if claim.producer == *me {
        return P::Skip(S::Producer);
    }
    if claim.seats.contains(me) {
        return P::Skip(S::Seat);
    }
    if !claim.operator_accusers.is_empty() {
        return P::Skip(S::OperatorAccuses);
    }
    if claim.opened_non_seat_total >= PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1 {
        return P::Skip(S::Shielded);
    }
    let judges = palw_operator_da_judges_v1(claim, operators);
    let judging = !judges.is_empty();
    let pool = if judging { judges } else { palw_operator_da_order_v1(claim, operators) };
    let n = pool.len() as u64;
    let Some(rank) = pool.iter().position(|bond| bond == me) else {
        return P::Skip(if judging { S::NotAJudge } else { S::NoneEligible });
    };
    let turn = now_daa.saturating_sub(claim.stage_daa) / PALW_OPERATOR_DA_TURN_DAA_V1;
    let mine_next = turn + (rank as u64 + n - turn % n) % n;
    if mine_next != turn {
        return P::Wait { from_daa: claim.stage_daa.saturating_add(mine_next.saturating_mul(PALW_OPERATOR_DA_TURN_DAA_V1)) };
    }
    let due = claim.stage_daa.saturating_add((turn + 1).saturating_mul(PALW_OPERATOR_DA_TURN_DAA_V1)).min(claim.accuse_until_daa);
    if judging {
        return P::Judge { rank, of: pool.len(), due };
    }
    if claim.open_non_seat >= PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1 {
        return P::Wait { from_daa: now_daa + 1 };
    }
    P::File { rank, of: pool.len(), due }
}

/// **What this node's replay of a claim came to** (the replay gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaVerdictV1 {
    /// The replay reproduces the claim's roots and answer at its price: honest, never accused here.
    Reproduces,
    /// It does not: a coverage lie. Accused at once, turn or no turn, and said loudly.
    Refuted,
    /// This host could not judge it (why): nothing filed; the next judge's turn comes.
    Unjudged(&'static str),
}

/// **A returned replay as a verdict** — SEAT-R's one rule ([`palw_seat_replay_step_v1`]: both roots,
/// the priced work, and SEAT-S2's answer) against the claim's committed facts. `None` for a replay
/// that refused (retried once, then [`PalwOperatorDaVerdictV1::Unjudged`]).
pub(crate) fn palw_operator_da_verdict_v1(
    result: &Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String>,
    job: &PalwOperatorDaJobV1,
) -> Option<PalwOperatorDaVerdictV1> {
    if result.is_err() {
        return None;
    }
    Some(match palw_seat_replay_step_v1(result, job.execution_root, job.trace_root, job.work_leaves, job.output_root) {
        PalwSeatReplayStepV1::Licensed => PalwOperatorDaVerdictV1::Reproduces,
        PalwSeatReplayStepV1::Refuted => PalwOperatorDaVerdictV1::Refuted,
        PalwSeatReplayStepV1::NoVerdict | PalwSeatReplayStepV1::Waiting => {
            PalwOperatorDaVerdictV1::Unjudged("the family's replay names no answer to compare")
        }
    })
}

/// **What the budget says of one filing** ([`palw_operator_da_budget_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaBudgetV1 {
    Within,
    /// It would leave less than the seat reserve as A-6 room.
    Reserve,
    /// Blind, and the bond's held DA exposure would pass the blind cap.
    BlindCap,
    /// The bond's collateral is below the floor: lane B files nothing.
    Floor,
}

/// **The budget** (the module's header): `standing` is the chain's, `pending` the exposure of this
/// node's filings not yet on chain, `exposure` the new session's (the fold's `admission.exposure`),
/// `genesis` the bond's registered collateral, `blind` whether no replay judged the claim.
pub(crate) fn palw_operator_da_budget_v1(
    standing: &PalwOperatorDaStandingV1,
    genesis: u64,
    pending: u128,
    exposure: u128,
    blind: bool,
) -> PalwOperatorDaBudgetV1 {
    let collateral = standing.collateral as u128;
    if collateral.saturating_mul(1000) < (genesis as u128).saturating_mul(PALW_OPERATOR_DA_COLLATERAL_FLOOR_PERMILLE_V1) {
        return PalwOperatorDaBudgetV1::Floor;
    }
    let reserve = collateral.saturating_mul(PALW_OPERATOR_DA_SEAT_RESERVE_PERMILLE_V1) / 1000;
    if standing.accuser_room < exposure.saturating_add(pending).saturating_add(reserve) {
        return PalwOperatorDaBudgetV1::Reserve;
    }
    let cap = collateral.saturating_mul(PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1) / 1000;
    if blind && standing.da_held.saturating_add(pending).saturating_add(exposure) > cap {
        return PalwOperatorDaBudgetV1::BlindCap;
    }
    PalwOperatorDaBudgetV1::Within
}

/// **What the book offers this node next** ([`PalwOperatorDaBookV1::next`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaActionV1 {
    /// Start this node's replay of the claim (its judge's turn, no verdict yet).
    Replay,
    /// Accuse a claim this node's replay refuted, landing by `due`.
    FileRefuted { due: u64 },
    /// Accuse blind (no operator judges the claim), landing by `due` — `rank` of `of` eligible.
    FileBlind { rank: usize, of: usize, due: u64 },
}

impl PalwOperatorDaActionV1 {
    fn blind(self) -> bool {
        matches!(self, Self::FileBlind { .. })
    }
}

/// **The lane's book** (in memory: a restart forgets it, and the chain's `operator_accusers` is what
/// stops a second filing — the book only spaces this node's own and remembers its verdicts).
#[derive(Default)]
pub(super) struct PalwOperatorDaBookV1 {
    /// The operator set, derived once from the params.
    operators: Vec<PalwBondKeyV2>,
    /// Each operator bond's genesis collateral (the floor's base).
    registered: BTreeMap<PalwBondKeyV2, u64>,
    /// The chain's candidates and this bond's standing, read once a DAA (`read_at`).
    candidates: Vec<PalwOperatorDaCandidateV1>,
    standing: Option<PalwOperatorDaStandingV1>,
    read_at: Option<u64>,
    /// claim → (the DAA this node last queued its accusation at, how many times).
    sent: BTreeMap<Hash64, (u64, u8)>,
    /// claim → (the exposure of this node's filing, the DAA it was queued): what the chain's standing
    /// does not show yet — until an operator accuser appears on the claim, it leaves the candidates,
    /// or it is taken as lost.
    pending: BTreeMap<Hash64, (u128, u64)>,
    /// claim → the DAA before which it is not asked again (the fold refused it for a reason a wait
    /// may change: DA-8's open cap).
    deferred: BTreeMap<Hash64, u64>,
    /// Claims done with (a verdict that files nothing, the fold's `AccusedBefore` / `Answered`, a seat
    /// now, a refusal no wait changes, a shielded claim said once).
    settled: std::collections::BTreeSet<Hash64>,
    /// This node's verdicts, by claim (a `Refuted` one waits here until it is filed).
    verdicts: BTreeMap<Hash64, PalwOperatorDaVerdictV1>,
    /// The lane's replay runner (SEAT-R's, its own instance) and the one replay in flight.
    pub(super) replays: PalwSeatReplaysV1,
    pub(super) replaying: Option<(Hash64, PalwSeatReplayKeyV1)>,
    /// The DAA a replay start was last refused (the ledger): no start for a re-plan after.
    replay_held_at: Option<u64>,
    /// The DAA of this node's last filing (the per-DAA cap).
    last_filed_daa: Option<u64>,
    /// The DAA the fold (A-6's room) or the budget last refused a filing: nothing is filed for a
    /// re-plan after.
    room_refused_at: Option<u64>,
    /// The DAA a scan last found nothing to do at: the candidates are not scanned again within it
    /// (every change of the book clears it).
    pub(super) idle_at: Option<u64>,
}

impl PalwOperatorDaBookV1 {
    pub(super) fn new(registrations: Vec<(PalwBondKeyV2, u64)>) -> Self {
        Self {
            operators: registrations.iter().map(|(bond, _)| *bond).collect(),
            registered: registrations.into_iter().collect(),
            ..Default::default()
        }
    }

    pub(super) fn operators(&self) -> &[PalwBondKeyV2] {
        &self.operators
    }

    /// Forget everything but the operator set (the lane disarmed: below the fence, no carrier). A
    /// running replay is detached with the runner (its reservation is held until it returns).
    pub(super) fn clear(&mut self) {
        let registrations = self.operators.iter().map(|bond| (*bond, self.registered.get(bond).copied().unwrap_or(0))).collect();
        *self = Self::new(registrations);
    }

    /// Whether the candidates are due a re-read at `now_daa` (once a DAA).
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// The chain's candidates and this bond's standing at `now_daa`; every memory of a claim that left
    /// them is dropped (so the book is bounded by the chain's own list), and a pending filing leaves
    /// once an operator accuser is on its claim or it is taken as lost.
    pub(super) fn refresh(
        &mut self,
        candidates: Vec<PalwOperatorDaCandidateV1>,
        standing: Option<PalwOperatorDaStandingV1>,
        now_daa: u64,
    ) {
        let live: std::collections::BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        self.sent.retain(|claim, _| live.contains(claim));
        self.deferred.retain(|claim, until| live.contains(claim) && now_daa < *until);
        self.settled.retain(|claim| live.contains(claim));
        self.verdicts.retain(|claim, _| live.contains(claim));
        self.pending.retain(|claim, (_, at)| {
            candidates.iter().any(|c| c.claim_id == *claim && c.operator_accusers.is_empty())
                && now_daa < at.saturating_add(PALW_OPERATOR_DA_RESEND_DAA_V1)
        });
        self.replays.retain_live(|claim| live.contains(claim));
        if self.replaying.is_some_and(|(claim, _)| !live.contains(&claim)) {
            self.replaying = None;
        }
        self.candidates = candidates;
        self.standing = standing;
        self.read_at = Some(now_daa);
        self.idle_at = None;
    }

    pub(super) fn candidate(&self, claim: &Hash64) -> Option<&PalwOperatorDaCandidateV1> {
        self.candidates.iter().find(|c| c.claim_id == *claim)
    }

    pub(super) fn standing(&self) -> Option<PalwOperatorDaStandingV1> {
        self.standing
    }

    /// The bond's genesis collateral (`0` for a bond that is not an operator's).
    pub(super) fn registered(&self, bond: &PalwBondKeyV2) -> u64 {
        self.registered.get(bond).copied().unwrap_or(0)
    }

    /// The exposure of this node's filings not yet on chain.
    pub(super) fn pending_exposure(&self) -> u128 {
        self.pending.values().map(|(exposure, _)| *exposure).fold(0u128, u128::saturating_add)
    }

    /// Whether a replay may start at `now_daa`: none in flight, and none refused within a re-plan.
    pub(super) fn may_start_replay(&self, now_daa: u64) -> bool {
        self.replaying.is_none() && self.replay_held_at.is_none_or(|at| now_daa >= at.saturating_add(COURT_MOVE_REPLAN_DAA))
    }

    /// Whether the lane may still file `claim` at `now_daa` (a queued item that is no longer this
    /// node's to file leaves the queue unsent): a refuted claim while nothing skips it, any other on
    /// this node's blind turn.
    pub(super) fn still_files(&self, claim: &Hash64, me: &PalwBondKeyV2, now_daa: u64) -> bool {
        self.candidate(claim).is_some_and(|c| match palw_operator_da_plan_v1(c, me, &self.operators, now_daa) {
            PalwOperatorDaPlanV1::Skip(_) => false,
            PalwOperatorDaPlanV1::File { .. } => true,
            _ => self.verdicts.get(claim) == Some(&PalwOperatorDaVerdictV1::Refuted),
        })
    }

    /// **What this node does next, if anything** — the first candidate (oldest stage first) it has not
    /// settled, deferred, or sent too recently or too often, as:
    /// * [`PalwOperatorDaActionV1::FileRefuted`] — its replay refuted the claim (any turn);
    /// * [`PalwOperatorDaActionV1::Replay`] — its judge's turn and no verdict yet, while `may_replay`;
    /// * [`PalwOperatorDaActionV1::FileBlind`] — its blind turn.
    ///
    /// A filing waits while one of this lane's items is queued (`queued`), within the per-DAA gap, and
    /// within a re-plan of a room or budget refusal; a replay does not.
    pub(super) fn next(
        &self,
        me: &PalwBondKeyV2,
        now_daa: u64,
        queued: bool,
        may_replay: bool,
    ) -> Option<(Hash64, PalwOperatorDaActionV1)> {
        let may_file = !queued
            && self.last_filed_daa.is_none_or(|at| now_daa >= at.saturating_add(PALW_OPERATOR_DA_MIN_GAP_DAA_V1))
            && self.room_refused_at.is_none_or(|at| now_daa >= at.saturating_add(COURT_MOVE_REPLAN_DAA));
        self.candidates.iter().find_map(|claim| {
            let id = claim.claim_id;
            if self.settled.contains(&id) || self.deferred.get(&id).is_some_and(|until| now_daa < *until) {
                return None;
            }
            if let Some((at, sends)) = self.sent.get(&id)
                && (*sends >= PALW_OPERATOR_DA_SENDS_V1 || now_daa < at.saturating_add(PALW_OPERATOR_DA_RESEND_DAA_V1))
            {
                return None;
            }
            let plan = palw_operator_da_plan_v1(claim, me, &self.operators, now_daa);
            match (self.verdicts.get(&id), plan) {
                (_, PalwOperatorDaPlanV1::Skip(_)) => None,
                (Some(PalwOperatorDaVerdictV1::Refuted), _) => may_file.then_some((
                    id,
                    PalwOperatorDaActionV1::FileRefuted {
                        due: now_daa.saturating_add(PALW_OPERATOR_DA_TURN_DAA_V1).min(claim.accuse_until_daa),
                    },
                )),
                (Some(_), _) => None,
                (None, PalwOperatorDaPlanV1::Judge { .. }) => {
                    (may_replay && self.replaying.is_none_or(|(claim, _)| claim != id)).then_some((id, PalwOperatorDaActionV1::Replay))
                }
                (None, PalwOperatorDaPlanV1::File { rank, of, due }) => {
                    may_file.then_some((id, PalwOperatorDaActionV1::FileBlind { rank, of, due }))
                }
                (None, PalwOperatorDaPlanV1::Wait { .. }) => None,
            }
        })
    }

    /// This node's verdict on `claim`: a `Refuted` one waits to be filed; any other settles the claim
    /// here (an honest claim is never accused by this node; one it cannot judge is the next judge's).
    pub(super) fn judge(&mut self, claim: Hash64, verdict: PalwOperatorDaVerdictV1) {
        self.verdicts.insert(claim, verdict);
        if verdict != PalwOperatorDaVerdictV1::Refuted {
            self.settled.insert(claim);
        }
        self.idle_at = None;
    }

    #[cfg(test)]
    pub(super) fn verdict(&self, claim: &Hash64) -> Option<PalwOperatorDaVerdictV1> {
        self.verdicts.get(claim).copied()
    }

    /// This node queued its accusation of `claim` at `now_daa`, `exposure` on its free half.
    pub(super) fn queued(&mut self, claim: Hash64, exposure: u128, now_daa: u64) {
        let entry = self.sent.entry(claim).or_insert((now_daa, 0));
        *entry = (now_daa, entry.1.saturating_add(1));
        self.pending.insert(claim, (exposure, now_daa));
        self.last_filed_daa = Some(now_daa);
        self.idle_at = None;
    }

    pub(super) fn settle(&mut self, claim: Hash64) {
        self.settled.insert(claim);
        self.idle_at = None;
    }

    #[cfg(test)]
    pub(super) fn is_settled(&self, claim: &Hash64) -> bool {
        self.settled.contains(claim)
    }

    pub(super) fn defer(&mut self, claim: Hash64, until_daa: u64) {
        self.deferred.insert(claim, until_daa);
        self.idle_at = None;
    }

    pub(super) fn room_refused(&mut self, now_daa: u64) {
        self.room_refused_at = Some(now_daa);
        self.idle_at = None;
    }

    pub(super) fn replay_held(&mut self, now_daa: u64) {
        self.replay_held_at = Some(now_daa);
        self.idle_at = None;
    }

    /// The candidates not yet settled that DA-8's lifetime cap shields from every operator
    /// ([`PalwOperatorDaSkipV1::Shielded`]) — what the tick says once, loudly.
    pub(super) fn shielded(&self, me: &PalwBondKeyV2, now_daa: u64) -> Vec<Hash64> {
        self.candidates
            .iter()
            .filter(|c| {
                !self.settled.contains(&c.claim_id)
                    && palw_operator_da_plan_v1(c, me, &self.operators, now_daa)
                        == PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::Shielded)
            })
            .map(|c| c.claim_id)
            .collect()
    }
}

/// **What the lane does with the fold's answer to one of its accusations.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwOperatorDaStepV1 {
    /// Build it and queue it for the priority lane.
    File,
    /// Done with the claim.
    Settle,
    /// A-6's room on this bond's free half: ask again a re-plan later (anything, not this claim only).
    RoomRetry,
    /// DA-8's three open non-seat sessions are full: this claim again a re-plan later.
    Defer,
}

/// **The fold's answer (`PalwDaAccusationCheckV1`) as this lane's step.** A `File` the fold would
/// open as a SEAT session is P2-6's, never this lane's (this bond was drawn onto the panel since the
/// read): settled. Only A-6's room and DA-8's open cap are waited out; every other refusal stands.
pub(super) fn palw_operator_da_step_v1(check: &PalwDaAccusationCheckV1) -> PalwOperatorDaStepV1 {
    use kaspa_consensus_core::palw_state_v2::PalwStateV2Error as E;
    match check {
        PalwDaAccusationCheckV1::File { admission, .. } if !admission.accuser_is_seat => PalwOperatorDaStepV1::File,
        PalwDaAccusationCheckV1::File { .. } | PalwDaAccusationCheckV1::AccusedBefore | PalwDaAccusationCheckV1::Answered => {
            PalwOperatorDaStepV1::Settle
        }
        PalwDaAccusationCheckV1::Refused(E::AccusationExposureCeiling { .. }) => PalwOperatorDaStepV1::RoomRetry,
        PalwDaAccusationCheckV1::Refused(E::DaSessionBudgetExhausted { why, .. }) if why.contains("open on a claim at once") => {
            PalwOperatorDaStepV1::Defer
        }
        PalwDaAccusationCheckV1::Refused(_) => PalwOperatorDaStepV1::Settle,
    }
}

impl PalwPanelService {
    /// **Lane B's half of the tick** (the module's header): re-read the candidates and this bond's
    /// standing once a DAA, drop a queued item that is no longer this node's to file, poll the lane's
    /// replay, and then either start one replay (a judge's turn) or queue at most one accusation —
    /// asked of the fold first, held to the budget, built by the one builder, dated by its turn.
    /// `seat_replay_room`: whether the seat's own replay slots have room for a light replay (the lane
    /// never starts one otherwise).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn operator_da_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwOperatorDaBookV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        // What the priority lane carried of this lane's items since the last tick (`carry_priority_v1`
        // marks each sent key in `court_moved`), said here. This lane keeps its own spacing (`sent`),
        // never `court_moved`'s debounce, so its entries leave here and nothing grows there on its
        // account.
        court_moved.retain(|(claim, round, responder), at| {
            if (*round, *responder) != (PALW_OPERATOR_DA_QUEUE_ROUND_V1, false) {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: the operator's non-seat DefaultAccused was carried at DAA {at} (lane B)");
            false
        });
        if !palw_operator_da_armed_v1(
            self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
            self.config.fee_outpoint.is_some(),
            book.operators().contains(&bond_key),
        ) {
            book.clear();
            court_pending.retain(|(_, round, responder, object)| !palw_operator_da_queued_v1(*round, *responder, object));
            return;
        }
        if book.stale(current_daa) {
            let operators = book.operators().to_vec();
            let (candidates, standing) = session
                .clone()
                .spawn_blocking(move |c| (c.palw_operator_da_candidates_v1(operators), c.palw_operator_da_standing_v1(bond_key)))
                .await;
            book.refresh(candidates, standing, current_daa);
            // A queued item that is no longer this node's to file now (its turn passed, an operator
            // accuses, the claim left the candidates) leaves the queue unsent.
            court_pending.retain(|(claim, round, responder, object)| {
                !palw_operator_da_queued_v1(*round, *responder, object) || book.still_files(claim, &bond_key, current_daa)
            });
            // A claim shielded by DA-8's lifetime cap is said once, loudly: it may be a Sybil shield.
            for claim in book.shielded(&bond_key, current_daa) {
                warn!(
                    "[{PALW_PANEL}] claim {claim}: licensed by outside signers and no operator could accuse it — DA-8's sixteen \
                     non-seat sessions are spent (a possible Sybil shield; panel-seed stopgap (B))"
                );
                book.settle(claim);
            }
        }
        // The lane's replay, polled every tick until it returns.
        if let Some((claim, key)) = book.replaying {
            match book.replays.poll(&key, current_daa).await {
                PalwSeatReplayPollV1::Running => {}
                PalwSeatReplayPollV1::Absent => book.replaying = None,
                PalwSeatReplayPollV1::Done { result, .. } => {
                    book.replaying = None;
                    let candidate = book.candidate(&claim).cloned();
                    match (candidate.as_ref().and_then(|c| palw_operator_da_verdict_v1(&result, &c.job)), candidate) {
                        (Some(verdict), Some(candidate)) => {
                            self.operator_da_say_verdict_v1(&candidate, verdict, &result);
                            book.judge(claim, verdict);
                        }
                        (None, Some(candidate)) => {
                            if book.replays.retry_refused(&key, current_daa, candidate.accuse_until_daa) {
                                warn!(
                                    "[{PALW_PANEL}] claim {claim}: the operator's replay refused ({}) — starting it once more (lane B)",
                                    result.as_ref().err().map(String::as_str).unwrap_or("")
                                );
                                book.idle_at = None;
                            } else {
                                let verdict = PalwOperatorDaVerdictV1::Unjudged("the replay refused twice");
                                self.operator_da_say_verdict_v1(&candidate, verdict, &result);
                                book.judge(claim, verdict);
                            }
                        }
                        (_, None) => {}
                    }
                }
            }
        }
        // One scan of the candidates a DAA while there is nothing to do.
        if book.idle_at == Some(current_daa) {
            return;
        }
        let queued = court_pending.iter().any(|(_, round, responder, object)| palw_operator_da_queued_v1(*round, *responder, object));
        let may_replay = seat_replay_room && book.may_start_replay(current_daa);
        let Some((claim, action)) = book.next(&bond_key, current_daa, queued, may_replay) else {
            book.idle_at = Some(current_daa);
            return;
        };
        let Some(candidate) = book.candidate(&claim).cloned() else { return };
        let due = match action {
            PalwOperatorDaActionV1::Replay => {
                self.operator_da_start_replay_v1(session, book, &candidate, current_daa, network_domain);
                return;
            }
            PalwOperatorDaActionV1::FileRefuted { due } | PalwOperatorDaActionV1::FileBlind { due, .. } => due,
        };
        let Some(check) = session.palw_da_accusation_check_v1(claim, bond_key) else { return };
        match (palw_operator_da_step_v1(&check), &check) {
            (PalwOperatorDaStepV1::File, PalwDaAccusationCheckV1::File { unit, admission }) => {
                // The budget (findings 2–3) before a carrier is paid for: the seat reserve, the blind
                // cap, the collateral floor — over the chain's standing and this node's pending filings.
                let Some(standing) = book.standing() else { return };
                let budget = palw_operator_da_budget_v1(
                    &standing,
                    book.registered(&bond_key),
                    book.pending_exposure(),
                    admission.exposure,
                    action.blind(),
                );
                if budget != PalwOperatorDaBudgetV1::Within {
                    book.room_refused(current_daa);
                    let line = format!(
                        "[{PALW_PANEL}] claim {claim}: the operator's {} accusation is held by lane B's budget ({budget:?}): collateral {} \
                         sompi of {} registered, A-6 room {}, DA exposure held {} (+{} pending), this session {} — the seat reserve is \
                         {}‰ of C, the blind cap {}‰, the floor {}‰ of the registration",
                        if action.blind() { "blind" } else { "REFUTED claim's" },
                        standing.collateral,
                        book.registered(&bond_key),
                        standing.accuser_room,
                        standing.da_held,
                        book.pending_exposure(),
                        admission.exposure,
                        PALW_OPERATOR_DA_SEAT_RESERVE_PERMILLE_V1,
                        PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1,
                        PALW_OPERATOR_DA_COLLATERAL_FLOOR_PERMILLE_V1,
                    );
                    if action.blind() {
                        crate::palw_backends::note_throttled_v1("panel-operator-da-budget", || line);
                    } else {
                        crate::palw_backends::note_throttled_v1("panel-operator-da-budget-refuted", || line);
                    }
                    return;
                }
                match kaspa_consensus_core::palw_da_rcore_v1::palw_da_accusation_object_v1(
                    &network_domain,
                    claim,
                    *unit,
                    bond_key,
                    |message, context| self.sign(message, context),
                ) {
                    Ok(object) => {
                        info!(
                            "[{PALW_PANEL}] claim {claim}: the operator's non-seat accusation (lane B, {action:?}) — producer {:?}, {} \
                             outside Valid signer(s); {unit:?} at stage {:?}, {} sompi on this bond's free half, the session's deadline \
                             DAA {} (panel-seed stopgap (B), DA-8)",
                            candidate.producer.0,
                            candidate.outside_signers.len(),
                            admission.stage,
                            admission.exposure,
                            admission.deadline_daa
                        );
                        let key = palw_operator_da_queue_key_v1(claim);
                        court_due.insert(key, due.max(current_daa));
                        court_pending.push((key.0, key.1, key.2, object));
                        book.queued(claim, admission.exposure, current_daa);
                    }
                    Err(why) => {
                        warn!("[{PALW_PANEL}] claim {claim}: cannot build the operator's accusation: {why}");
                        book.settle(claim);
                    }
                }
            }
            (PalwOperatorDaStepV1::RoomRetry, _) => {
                book.room_refused(current_daa);
                crate::palw_backends::note_throttled_v1("panel-operator-da-room", || {
                    format!(
                        "[{PALW_PANEL}] claim {claim}: the operator's accusation waits for room on this bond's free half — {check:?}"
                    )
                });
            }
            (PalwOperatorDaStepV1::Defer, _) => book.defer(claim, current_daa + COURT_MOVE_REPLAN_DAA),
            (_, check) => {
                info!("[{PALW_PANEL}] claim {claim}: not accused by the operator — {check:?}");
                book.settle(claim);
            }
        }
    }

    /// **Start this node's replay of `candidate`'s job** (a judge's turn): the anchor's job derived
    /// from the claim's block (`attempt_job_for_claim`, the one place a seat or a challenger turns an
    /// attempt claim into the job it replays), off the loop in the lane's own slot under the host
    /// ledger's reservation at the full seat's need — SEAT-R's attempt replay, for a non-seat. A claim
    /// this host cannot replay is `Unjudged` here, loudly (its bond declared the class capable); a
    /// ledger refusal waits a re-plan.
    fn operator_da_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwOperatorDaBookV1,
        candidate: &PalwOperatorDaCandidateV1,
        current_daa: u64,
        network_domain: Hash64,
    ) {
        let claim = candidate.claim_id;
        let job = &candidate.job;
        let unjudged = |book: &mut PalwOperatorDaBookV1, why: &'static str, detail: String| {
            warn!(
                "[{PALW_PANEL}] claim {claim}: this operator node is one of the claim's judges and cannot replay it — {why}{detail}; \
                 the next judge's turn comes (lane B)"
            );
            book.judge(claim, PalwOperatorDaVerdictV1::Unjudged(why));
        };
        let Some(artifact_root) = job.artifact_root.filter(|_| palw_operator_da_replayable_v1(job)) else {
            return unjudged(book, "the lane does not replay this claim", String::new());
        };
        let backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => return unjudged(book, "its class does not resolve on this host", format!(" ({why})")),
        };
        let Some((ctx, prompt)) =
            self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &candidate.producer)
        else {
            return unjudged(book, "its block's header is not held here", String::new());
        };
        let key = (claim, ctx.job_id);
        let need = self.backends().role_memory_need_for_backend_or_chain_v1(
            backend.as_ref(),
            job.class_id,
            artifact_root,
            Some(&ctx),
            kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat,
            |id| self.chain_carriage_v1(session, id),
        );
        let reserved = match self.reserve_replay_v1(PALW_OPERATOR_DA_REPLAY_ROLE_V1, &need, job.class_id, claim) {
            Ok(reserved) => reserved,
            Err(why) => {
                book.replay_held(current_daa);
                crate::palw_backends::note_throttled_v1("panel-operator-da-ledger", || {
                    format!("[{PALW_PANEL}] claim {claim}: the operator's replay waits: {why} (lane B)")
                });
                return;
            }
        };
        info!(
            "[{PALW_PANEL}] claim {claim}: judging the claim by replaying the anchor's job off the loop before any accusation \
             (lane B's replay gate; producer {:?}, {} outside Valid signer(s))",
            candidate.producer.0,
            candidate.outside_signers.len()
        );
        book.replays.start(key, job.class_id, false, current_daa, Some(reserved), backend, move |b| b.execute_for_verdict(&ctx, &prompt));
        book.replaying = Some((claim, key));
        book.idle_at = None;
    }

    /// The verdict, said: an honest claim quietly, a refuted one as the error it is.
    fn operator_da_say_verdict_v1(
        &self,
        candidate: &PalwOperatorDaCandidateV1,
        verdict: PalwOperatorDaVerdictV1,
        result: &Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String>,
    ) {
        let claim = candidate.claim_id;
        match verdict {
            PalwOperatorDaVerdictV1::Reproduces => info!(
                "[{PALW_PANEL}] claim {claim}: the operator's replay reproduces the claim's roots and answer — honest, not accused \
                 (lane B's replay gate)"
            ),
            PalwOperatorDaVerdictV1::Refuted => error!(
                "[{PALW_PANEL}] claim {claim}: the operator's replay of the anchor's job does NOT reproduce the claim (replayed {:?}; \
                 claimed execution {}, trace {}, output {}) — a COVERAGE LIE licensed by outside signers {:?} (producer {:?}). \
                 Accusing it now: junk defaults and is convicted, but a self-consistent garbage trace answers a DA accusation and \
                 goes on to Final — escalate (lane A's operator-anchor fence; panel-seed stopgap (B)/(C))",
                result.as_ref().ok(),
                candidate.job.execution_root,
                candidate.job.trace_root,
                candidate.job.output_root,
                candidate.outside_signers,
                candidate.producer.0
            ),
            PalwOperatorDaVerdictV1::Unjudged(why) => warn!(
                "[{PALW_PANEL}] claim {claim}: the operator's replay came to no verdict — {why} ({:?}); the next judge's turn comes \
                 (lane B)",
                result.as_ref().err()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1;
    use kaspa_consensus_core::palw_state_v2::{PalwDaAdmissionV1, PalwStateV2Error};

    const GENESIS_C: u64 = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    const MSK: u128 = 100_000_000;

    fn bond(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0x5E0D), n))
    }

    fn outsider(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xE7), n))
    }

    fn operators() -> Vec<PalwBondKeyV2> {
        (0..8).map(bond).collect()
    }

    fn registrations() -> Vec<(PalwBondKeyV2, u64)> {
        operators().into_iter().map(|b| (b, GENESIS_C)).collect()
    }

    fn job() -> PalwOperatorDaJobV1 {
        PalwOperatorDaJobV1 {
            accepted_block: Hash64::from_u64_word(0xB10C),
            class_id: Hash64::from_u64_word(0xF100),
            artifact_root: Some(Hash64::from_u64_word(0xA7)),
            execution_root: Hash64::from_u64_word(0xE1),
            trace_root: Hash64::from_u64_word(0x71),
            output_root: Hash64::from_u64_word(0x01),
            work_leaves: 0,
            free_prompt: false,
            held_to_final: false,
        }
    }

    /// A claim produced outside, licensed at `stage_daa` by a panel of `seats` — a free prompt, so no
    /// operator judges it: the blind path.
    fn candidate(claim: u64, stage_daa: u64, seats: Vec<PalwBondKeyV2>) -> PalwOperatorDaCandidateV1 {
        PalwOperatorDaCandidateV1 {
            claim_id: Hash64::from_u64_word(claim),
            producer: outsider(0),
            stage: PalwDaStageV1::Licensed,
            stage_daa,
            seats,
            outside_signers: vec![outsider(1), outsider(2)],
            operator_accusers: vec![],
            open_non_seat: 0,
            opened_non_seat_total: 0,
            accuse_until_daa: stage_daa + 3_000,
            job: PalwOperatorDaJobV1 { free_prompt: true, ..job() },
            capable: operators(),
        }
    }

    /// The same claim on the attempt lane of a light class every operator declared: judged.
    fn judged(claim: u64, stage_daa: u64, seats: Vec<PalwBondKeyV2>) -> PalwOperatorDaCandidateV1 {
        PalwOperatorDaCandidateV1 { job: job(), ..candidate(claim, stage_daa, seats) }
    }

    fn panel() -> Vec<PalwBondKeyV2> {
        vec![bond(1), outsider(1), outsider(2), bond(4), outsider(3)]
    }

    /// A genesis bond's standing with nothing held.
    fn fresh() -> PalwOperatorDaStandingV1 {
        PalwOperatorDaStandingV1 { collateral: GENESIS_C, accuser_room: GENESIS_C as u128 / 2, da_held: 0 }
    }

    /// **The operator set is testnet-12's eight genesis cards** — the premine's outputs 0..7, read
    /// off the bundle's genesis registrations with the collateral each posted (the floor's base);
    /// nothing on a network without a V2 bundle.
    #[test]
    fn the_operator_set_is_testnet12s_eight_genesis_bonds() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let registrations = palw_operator_registrations_v1(&t12);
        let bonds: Vec<PalwBondKeyV2> = registrations.iter().map(|(bond, _)| *bond).collect();
        let premine = kaspa_consensus_core::config::premine::premine_txid_for(t12.net);
        assert_eq!(bonds.len(), 8, "eight genesis bonds");
        assert!(bonds.iter().all(|b| b.0.transaction_id == premine), "all on the premine {premine}");
        assert_eq!(bonds.iter().map(|b| b.0.index).collect::<Vec<_>>(), (0..8).collect::<Vec<u32>>(), "outputs 0..7");
        assert!(registrations.iter().all(|(_, c)| *c == GENESIS_C), "each registered at the genesis collateral: {registrations:?}");
        assert!(palw_operator_registrations_v1(&Params::from(NetworkId::new(NetworkType::Mainnet))).is_empty(), "no genesis cards on mainnet");
        let book = PalwOperatorDaBookV1::new(registrations);
        assert_eq!(book.registered(&bonds[3]), GENESIS_C);
        assert_eq!(book.registered(&outsider(0)), 0);
        // Armed exactly where identity says: rcore_plus, a carrier, an operator's bond — no flag.
        for (rcore, carries, operator) in [(true, true, true), (false, true, true), (true, false, true), (true, true, false)] {
            assert_eq!(palw_operator_da_armed_v1(rcore, carries, operator), rcore && carries && operator);
        }
    }

    /// **Eligibility and order.** The producer and every seat of the current panel are out; the rest
    /// are ordered by the claim's rank key, the same on every node, and a different claim draws a
    /// different order.
    #[test]
    fn the_order_leaves_out_the_producer_and_the_seats_and_is_the_claims_own() {
        let ops = operators();
        let c = candidate(0xC1, 1_500, panel());
        let order = palw_operator_da_order_v1(&c, &ops);
        assert_eq!(order.len(), 6, "eight operators less the two seated");
        assert!(!order.contains(&bond(1)) && !order.contains(&bond(4)), "seats are P2-6's");
        let mut sorted = order.clone();
        sorted.sort_by_key(|b| palw_operator_da_rank_key_v1(&c.claim_id, b));
        assert_eq!(order, sorted, "rank-key order");
        let mut shuffled = ops.clone();
        shuffled.reverse();
        assert_eq!(palw_operator_da_order_v1(&c, &shuffled), order, "independent of how the set is listed");
        let own = PalwOperatorDaCandidateV1 { producer: bond(2), ..c.clone() };
        assert!(!palw_operator_da_order_v1(&own, &ops).contains(&bond(2)), "never the producer");
        let orders: std::collections::BTreeSet<Vec<PalwBondKeyV2>> =
            (0..32u64).map(|n| palw_operator_da_order_v1(&candidate(0xD0 + n, 1_500, panel()), &ops)).collect();
        assert!(orders.len() > 16, "claims draw their own orders: {}", orders.len());
    }

    /// **The judges** (the replay gate): the eligible operator bonds that declared the claim's class,
    /// in rank order, at most three — and none for a claim the lane does not replay (a free prompt, a
    /// C7 class, a class gone from the registry), which falls to the blind rotation over every
    /// eligible bond. A claim with judges is never filed blind by anyone: a non-judge skips it.
    #[test]
    fn the_judges_are_the_capable_eligible_bonds_and_a_non_judge_never_files_blind() {
        let ops = operators();
        let c = judged(0xC5, 1_500, panel());
        let order = palw_operator_da_order_v1(&c, &ops);
        let judges = palw_operator_da_judges_v1(&c, &ops);
        assert_eq!(judges, order[..PALW_OPERATOR_DA_JUDGES_V1].to_vec(), "the first three eligible, in rank order");
        // Only the declared: drop the first two ranks' capability.
        let partly = PalwOperatorDaCandidateV1 { capable: ops.iter().copied().filter(|b| !order[..2].contains(b)).collect(), ..c.clone() };
        assert_eq!(palw_operator_da_judges_v1(&partly, &ops), order[2..5].to_vec(), "capability, then rank");
        // A seat that declared is still a seat, never a judge.
        assert!(judges.iter().all(|j| !c.seats.contains(j) && *j != c.producer));
        for (why, job) in [
            ("a free prompt", PalwOperatorDaJobV1 { free_prompt: true, ..job() }),
            ("a C7 class", PalwOperatorDaJobV1 { held_to_final: true, ..job() }),
            ("a class gone from the registry", PalwOperatorDaJobV1 { artifact_root: None, ..job() }),
        ] {
            let blind = PalwOperatorDaCandidateV1 { job, ..c.clone() };
            assert!(palw_operator_da_judges_v1(&blind, &ops).is_empty(), "{why}: no judges");
            assert!(
                matches!(palw_operator_da_plan_v1(&blind, &order[0], &ops, 1_500), PalwOperatorDaPlanV1::File { rank: 0, of: 6, .. }),
                "{why}: the blind rotation over all six"
            );
        }
        let nobody = PalwOperatorDaCandidateV1 { capable: vec![], ..c.clone() };
        assert!(palw_operator_da_judges_v1(&nobody, &ops).is_empty(), "no eligible operator declared the class: blind");
        // With judges, a non-judge skips the claim at every DAA — it never files blind.
        let outside_the_judges: Vec<PalwBondKeyV2> = order.iter().copied().filter(|b| !judges.contains(b)).collect();
        for now in (1_500..1_500 + 12 * PALW_OPERATOR_DA_TURN_DAA_V1).step_by(7) {
            for me in &outside_the_judges {
                assert_eq!(
                    palw_operator_da_plan_v1(&c, me, &ops, now),
                    PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::NotAJudge),
                    "{me:?} at {now}"
                );
            }
        }
    }

    /// **Exactly one owner at every DAA, whatever the DAA** — the rank-0 node's turn opens at the
    /// licence, the next rank's a turn later, round and round, over the claim's pool: its judges
    /// (each a `Judge` turn) or, with none, every eligible bond (each a blind `File` turn). Each Wait
    /// names the DAA its turn opens.
    #[test]
    fn exactly_one_node_of_the_claims_pool_is_on_turn_at_every_daa() {
        let ops = operators();
        for (c, n) in [(candidate(0xC2, 1_500, panel()), 6usize), (judged(0xC2, 1_500, panel()), PALW_OPERATOR_DA_JUDGES_V1)] {
            let judging = !palw_operator_da_judges_v1(&c, &ops).is_empty();
            let pool = if judging { palw_operator_da_judges_v1(&c, &ops) } else { palw_operator_da_order_v1(&c, &ops) };
            assert_eq!(pool.len(), n);
            for now in 1_500..1_500 + 3 * 6 * PALW_OPERATOR_DA_TURN_DAA_V1 {
                let on_turn: Vec<(PalwBondKeyV2, PalwOperatorDaPlanV1)> = ops
                    .iter()
                    .map(|me| (*me, palw_operator_da_plan_v1(&c, me, &ops, now)))
                    .filter(|(_, plan)| matches!(plan, PalwOperatorDaPlanV1::File { .. } | PalwOperatorDaPlanV1::Judge { .. }))
                    .collect();
                assert_eq!(on_turn.len(), 1, "DAA {now}: {on_turn:?}");
                let turn = (now - 1_500) / PALW_OPERATOR_DA_TURN_DAA_V1;
                let (owner, plan) = on_turn[0];
                let rank = (turn % n as u64) as usize;
                assert_eq!(owner, pool[rank], "turn {turn} is rank {rank}'s");
                let due = 1_500 + (turn + 1) * PALW_OPERATOR_DA_TURN_DAA_V1;
                let expected = if judging {
                    PalwOperatorDaPlanV1::Judge { rank, of: n, due }
                } else {
                    PalwOperatorDaPlanV1::File { rank, of: n, due }
                };
                assert_eq!(plan, expected);
                for me in pool.iter().filter(|me| **me != owner) {
                    let PalwOperatorDaPlanV1::Wait { from_daa } = palw_operator_da_plan_v1(&c, me, &ops, now) else {
                        panic!("{me:?} waits")
                    };
                    assert!(from_daa > now && (from_daa - 1_500) % PALW_OPERATOR_DA_TURN_DAA_V1 == 0);
                    assert!(
                        matches!(
                            palw_operator_da_plan_v1(&c, me, &ops, from_daa),
                            PalwOperatorDaPlanV1::File { .. } | PalwOperatorDaPlanV1::Judge { .. }
                        ),
                        "its turn opens where the Wait says"
                    );
                }
            }
            assert_eq!(palw_operator_da_plan_v1(&c, &bond(1), &ops, 1_500), PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::Seat));
            assert_eq!(
                palw_operator_da_plan_v1(&c, &outsider(9), &ops, 1_500),
                PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::NotOperator)
            );
        }
    }

    /// **Backoff and DA-8.** An operator accuser on chain stops every node; an outside accuser stops
    /// none; DA-8's open cap is waited out a DAA at a time; its lifetime cap is a shield, said once.
    #[test]
    fn an_operator_accuser_stops_everyone_and_da8_is_waited_out_or_named() {
        let ops = operators();
        let c = candidate(0xC3, 1_500, panel());
        let owner = palw_operator_da_order_v1(&c, &ops)[0];
        let accused = PalwOperatorDaCandidateV1 { operator_accusers: vec![bond(6)], ..c.clone() };
        for me in &ops {
            assert!(!matches!(palw_operator_da_plan_v1(&accused, me, &ops, 1_500), PalwOperatorDaPlanV1::File { .. }));
            let judged = PalwOperatorDaCandidateV1 { job: job(), ..accused.clone() };
            assert!(!matches!(palw_operator_da_plan_v1(&judged, me, &ops, 1_500), PalwOperatorDaPlanV1::Judge { .. }));
        }
        assert_eq!(
            palw_operator_da_plan_v1(&accused, &owner, &ops, 1_500),
            PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::OperatorAccuses)
        );
        let outside = PalwOperatorDaCandidateV1 { operator_accusers: vec![], ..c.clone() };
        assert!(matches!(palw_operator_da_plan_v1(&outside, &owner, &ops, 1_500), PalwOperatorDaPlanV1::File { .. }));
        let full = PalwOperatorDaCandidateV1 { open_non_seat: PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1, ..c.clone() };
        assert_eq!(palw_operator_da_plan_v1(&full, &owner, &ops, 1_500), PalwOperatorDaPlanV1::Wait { from_daa: 1_501 });
        let spent = PalwOperatorDaCandidateV1 { opened_non_seat_total: PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1, ..c.clone() };
        assert_eq!(palw_operator_da_plan_v1(&spent, &owner, &ops, 1_500), PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::Shielded));
        let everyone_seated = PalwOperatorDaCandidateV1 { seats: ops.clone(), ..c.clone() };
        assert_eq!(
            palw_operator_da_plan_v1(&everyone_seated, &bond(0), &ops, 1_500),
            PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::Seat)
        );
    }

    fn admission(seat: bool) -> PalwDaAdmissionV1 {
        PalwDaAdmissionV1 { stage: PalwDaStageV1::Licensed, accuser_is_seat: seat, exposure: 32_010_000_000, deadline_daa: 2_700 }
    }

    /// **The fold's answer as a step.** A non-seat `File` files; a seat `File` is P2-6's; A-6's room
    /// and DA-8's open cap wait; `AccusedBefore`, `Answered` and every other refusal settle.
    #[test]
    fn the_folds_answer_maps_to_the_lanes_step() {
        use PalwOperatorDaStepV1 as S;
        let unit = kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_AUTO_NAMED_UNIT_V1;
        let claim = Hash64::from_u64_word(1);
        let cases = [
            (PalwDaAccusationCheckV1::File { unit, admission: admission(false) }, S::File),
            (PalwDaAccusationCheckV1::File { unit, admission: admission(true) }, S::Settle),
            (PalwDaAccusationCheckV1::AccusedBefore, S::Settle),
            (PalwDaAccusationCheckV1::Answered, S::Settle),
            (
                PalwDaAccusationCheckV1::Refused(PalwStateV2Error::AccusationExposureCeiling {
                    bond: bond(0),
                    edge: "data-availability session",
                    backed: 1,
                    accusation: 2,
                    ceiling: 3,
                }),
                S::RoomRetry,
            ),
            (
                PalwDaAccusationCheckV1::Refused(PalwStateV2Error::DaSessionBudgetExhausted {
                    claim,
                    accuser: bond(0),
                    why: "at most three non-seat sessions are open on a claim at once",
                }),
                S::Defer,
            ),
            (
                PalwDaAccusationCheckV1::Refused(PalwStateV2Error::DaSessionBudgetExhausted {
                    claim,
                    accuser: bond(0),
                    why: "at most sixteen non-seat sessions open on a claim over its life",
                }),
                S::Settle,
            ),
            (PalwDaAccusationCheckV1::Refused(PalwStateV2Error::DaClaimNotAccusable(claim)), S::Settle),
            (PalwDaAccusationCheckV1::Refused(PalwStateV2Error::DaCourtDormant), S::Settle),
        ];
        for (check, step) in cases {
            assert_eq!(palw_operator_da_step_v1(&check), step, "{check:?}");
        }
    }

    /// **The verdict is SEAT-R's rule on a real replay** — the floor, testnet-12's draw rule: the
    /// operator's replay of an honest attempt's job reproduces it (never accused); P0-10's junk — any
    /// made-up root — and an honest arithmetic under another answer are refuted (accused, loudly); a
    /// replay that refused is no verdict yet (started once more, then `Unjudged`).
    #[test]
    fn the_verdict_is_seat_rs_rule_on_the_floors_own_replay() {
        use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        let backend = super::super::seat_s_tests::floor_backend();
        let draw = kaspa_consensus_core::config::params::palw_t12_shipped_params().palw_prefill_draw_active_at(0);
        let (anchor_job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x00C0_FFEE)).expect("job");
        let run_job = palw_attempt_job_v1(anchor_job, draw);
        let claim = backend.execute(&run_job, &prompt).expect("the producer's run");
        let replayed = backend.execute_for_verdict(&run_job, &prompt);
        let honest = PalwOperatorDaJobV1 {
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            output_root: claim.output_root,
            ..job()
        };
        assert_eq!(palw_operator_da_verdict_v1(&replayed, &honest), Some(PalwOperatorDaVerdictV1::Reproduces));
        for (why, lie) in [
            ("junk execution root", PalwOperatorDaJobV1 { execution_root: Hash64::from_u64_word(0xBAD), ..honest.clone() }),
            ("junk trace root", PalwOperatorDaJobV1 { trace_root: Hash64::from_u64_word(0xBAD), ..honest.clone() }),
            ("another answer (SEAT-S2)", PalwOperatorDaJobV1 { output_root: Hash64::from_u64_word(0xBAD), ..honest.clone() }),
            ("all junk (P0-10)", job()),
        ] {
            assert_eq!(palw_operator_da_verdict_v1(&replayed, &lie), Some(PalwOperatorDaVerdictV1::Refuted), "{why}");
        }
        assert_eq!(palw_operator_da_verdict_v1(&Err("the backend refused".into()), &honest), None);
    }

    /// **The budget, at a testnet-12 genesis bond** (939,063.21 MSK): the seat reserve keeps a quarter
    /// of C as A-6 room for the node's own seat duties (review finding 3 — lane B used to file until
    /// the fold refused, and a seat's own accusation was then refused `AccusationExposureCeiling`);
    /// the blind cap holds blind filings to 2% of C of DA exposure; the floor stops everything below
    /// 90% of the registration. A refuted claim's filing passes the blind cap, never the reserve or
    /// the floor. Pending filings count before the chain shows them.
    #[test]
    fn the_budget_keeps_the_seat_reserve_caps_blind_exposure_and_stops_at_the_floor() {
        use PalwOperatorDaBudgetV1 as B;
        let floor_session = 32_009_500_000u128; // 320.095 MSK
        let c = GENESIS_C as u128;
        assert_eq!(palw_operator_da_budget_v1(&fresh(), GENESIS_C, 0, floor_session, true), B::Within);
        // The seat reserve: A-6 room below C/4 + the session is refused, blind or not.
        let reserve = c * PALW_OPERATOR_DA_SEAT_RESERVE_PERMILLE_V1 / 1000;
        assert_eq!(reserve / MSK, 234_765, "a quarter of 939,063 MSK");
        let tight = PalwOperatorDaStandingV1 { accuser_room: reserve + floor_session - 1, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&tight, GENESIS_C, 0, floor_session, false), B::Reserve);
        assert_eq!(palw_operator_da_budget_v1(&tight, GENESIS_C, 0, floor_session, true), B::Reserve);
        let exact = PalwOperatorDaStandingV1 { accuser_room: reserve + floor_session, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&exact, GENESIS_C, 0, floor_session, false), B::Within);
        assert_eq!(palw_operator_da_budget_v1(&exact, GENESIS_C, 1, floor_session, false), B::Reserve, "pending counts");
        // The blind cap: 2% of C held.
        let cap = c * PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1 / 1000;
        assert_eq!(cap / MSK, 18_781, "2% of 939,063 MSK");
        assert_eq!(cap / floor_session, 58, "~58 floor sessions held at once");
        let held = PalwOperatorDaStandingV1 { da_held: cap - floor_session, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&held, GENESIS_C, 0, floor_session, true), B::Within);
        assert_eq!(palw_operator_da_budget_v1(&held, GENESIS_C, 1, floor_session, true), B::BlindCap);
        let full = PalwOperatorDaStandingV1 { da_held: cap, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&full, GENESIS_C, 0, floor_session, true), B::BlindCap);
        assert_eq!(palw_operator_da_budget_v1(&full, GENESIS_C, 0, floor_session, false), B::Within, "a refuted claim is filed");
        // The floor: below 90% of the registration, nothing.
        let worn = PalwOperatorDaStandingV1 { collateral: (c * 900 / 1000) as u64 - 1, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&worn, GENESIS_C, 0, floor_session, false), B::Floor);
        assert_eq!(palw_operator_da_budget_v1(&worn, GENESIS_C, 0, floor_session, true), B::Floor);
        let at = PalwOperatorDaStandingV1 { collateral: (c * 900 / 1000) as u64, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&at, GENESIS_C, 0, floor_session, false), B::Within);
        // A 2M session (6,294.38 MSK) blind: two fit under the cap, the third does not.
        let two_m = 629_438_000_000u128;
        let two = PalwOperatorDaStandingV1 { da_held: 2 * two_m, ..fresh() };
        assert_eq!(palw_operator_da_budget_v1(&two, GENESIS_C, 0, two_m, true), B::BlindCap);
    }

    /// **The book spaces this node's blind filings.** One queued at a time; two DAA between filings;
    /// a room or budget refusal holds everything a re-plan; a sent claim is not sent again within a
    /// turn, and at most twice; a deferred or settled claim is skipped; a claim that leaves the chain's
    /// list leaves the book; a pending filing's exposure counts until the chain names an operator
    /// accuser on its claim.
    #[test]
    fn the_book_spaces_this_nodes_filings() {
        let ops = operators();
        let a = candidate(0xA1, 1_500, panel());
        let b = candidate(0xA2, 1_500, panel());
        // The node on turn for both at 1_500 (pick claims whose rank 0 is the same bond).
        let me = palw_operator_da_order_v1(&a, &ops)[0];
        let b = if palw_operator_da_order_v1(&b, &ops)[0] == me {
            b
        } else {
            (0..512u64)
                .map(|n| candidate(0xB000 + n, 1_500, panel()))
                .find(|c| palw_operator_da_order_v1(c, &ops)[0] == me)
                .expect("some claim ranks the same bond first")
        };
        let blind = |action: Option<(Hash64, PalwOperatorDaActionV1)>| action.map(|(c, a)| (c, a.blind()));
        let mut book = PalwOperatorDaBookV1::new(registrations());
        book.refresh(vec![a.clone(), b.clone()], Some(fresh()), 1_500);
        assert_eq!(blind(book.next(&me, 1_500, false, true)), Some((a.claim_id, true)), "the oldest first (the chain's order), blind");
        assert_eq!(book.next(&me, 1_500, true, true), None, "one of this lane's items queued at a time");
        book.queued(a.claim_id, 32_009_500_000, 1_500);
        assert_eq!(book.pending_exposure(), 32_009_500_000);
        assert_eq!(book.next(&me, 1_501, false, true), None, "two DAA between filings");
        assert_eq!(blind(book.next(&me, 1_502, false, true)), Some((b.claim_id, true)), "the sent claim is spaced out");
        book.queued(b.claim_id, 32_009_500_000, 1_502);
        assert_eq!(book.next(&me, 1_510, false, true), None, "both sent within a turn");
        // a lands: the chain names this node, the pending exposure is the chain's now.
        let landed = PalwOperatorDaCandidateV1 { operator_accusers: vec![me], ..a.clone() };
        book.refresh(vec![landed, b.clone()], Some(fresh()), 1_511);
        assert_eq!(book.pending_exposure(), 32_009_500_000, "only b's is pending");
        // Still on chain unaccused a turn later (the carrier was lost): sent once more at most, on the
        // node's next turn (1_500 + 6 turns); the lost one is no longer pending.
        let again = 1_500 + 6 * PALW_OPERATOR_DA_TURN_DAA_V1;
        book.refresh(vec![a.clone(), b.clone()], Some(fresh()), again);
        assert_eq!(book.pending_exposure(), 0, "a filing not on chain a turn later is lost");
        assert_eq!(book.next(&me, again, false, true).map(|(c, _)| c), Some(a.claim_id));
        book.queued(a.claim_id, 1, again);
        book.queued(b.claim_id, 1, again + 2);
        assert_eq!(book.next(&me, again + 12 * PALW_OPERATOR_DA_TURN_DAA_V1, false, true), None, "never a third send");
        // Room and deferral.
        let mut book = PalwOperatorDaBookV1::new(registrations());
        book.refresh(vec![a.clone(), b.clone()], Some(fresh()), 1_500);
        book.room_refused(1_500);
        assert_eq!(book.next(&me, 1_500 + COURT_MOVE_REPLAN_DAA - 1, false, true), None, "a room refusal holds a re-plan");
        book.defer(a.claim_id, 1_520);
        assert_eq!(book.next(&me, 1_510, false, true).map(|(c, _)| c), Some(b.claim_id), "a deferred claim is skipped");
        book.settle(b.claim_id);
        assert_eq!(book.next(&me, 1_515, false, true), None);
        assert_eq!(book.next(&me, 1_520, false, true).map(|(c, _)| c), Some(a.claim_id), "the deferral ends");
        book.refresh(vec![a.clone()], Some(fresh()), 1_521);
        assert!(!book.is_settled(&b.claim_id), "a claim that left the chain's list leaves the book");
    }

    /// **The replay gate in the book.** On a judge's turn the book offers a replay — only while a
    /// replay may start; a `Reproduces` verdict settles the claim and nothing is ever filed for it; a
    /// `Refuted` one is filed at once, even after the judge's turn has passed, and stays offered while
    /// a filing is held; an `Unjudged` one settles it here (the next judge's turn comes). A queued
    /// refuted filing stays this node's to file off-turn; a blind one leaves the queue with its turn.
    #[test]
    fn the_book_replays_on_a_judges_turn_and_files_only_what_its_replay_refutes() {
        let ops = operators();
        let honest = judged(0xD1, 1_500, panel());
        let judge = palw_operator_da_judges_v1(&honest, &ops)[0];
        let lie = (0..512u64)
            .map(|n| judged(0xD200 + n, 1_500, panel()))
            .find(|c| palw_operator_da_judges_v1(c, &ops)[0] == judge)
            .expect("a second claim the same bond judges first");
        let mut book = PalwOperatorDaBookV1::new(registrations());
        book.refresh(vec![honest.clone(), lie.clone()], Some(fresh()), 1_500);
        assert_eq!(book.next(&judge, 1_500, false, false), None, "no replay may start: nothing");
        assert_eq!(book.next(&judge, 1_500, false, true), Some((honest.claim_id, PalwOperatorDaActionV1::Replay)));
        book.judge(honest.claim_id, PalwOperatorDaVerdictV1::Reproduces);
        assert!(book.is_settled(&honest.claim_id), "honest: settled, never filed");
        assert_eq!(book.next(&judge, 1_501, false, true), Some((lie.claim_id, PalwOperatorDaActionV1::Replay)));
        book.judge(lie.claim_id, PalwOperatorDaVerdictV1::Refuted);
        // Past the judge's turn (1_530 is the next judge's), a refuted claim is still filed.
        let late = 1_500 + PALW_OPERATOR_DA_TURN_DAA_V1 + 5;
        assert!(matches!(palw_operator_da_plan_v1(&lie, &judge, &ops, late), PalwOperatorDaPlanV1::Wait { .. }));
        assert_eq!(
            book.next(&judge, late, false, true),
            Some((lie.claim_id, PalwOperatorDaActionV1::FileRefuted { due: late + PALW_OPERATOR_DA_TURN_DAA_V1 }))
        );
        assert!(book.still_files(&lie.claim_id, &judge, late), "a queued refuted filing stays this node's off-turn");
        book.room_refused(late);
        assert_eq!(book.next(&judge, late + 1, false, true), None, "a held filing waits a re-plan");
        assert!(matches!(
            book.next(&judge, late + COURT_MOVE_REPLAN_DAA, false, true),
            Some((_, PalwOperatorDaActionV1::FileRefuted { .. }))
        ));
        // Another operator's accusation lands first: backoff, whatever the verdict.
        let accused = PalwOperatorDaCandidateV1 { operator_accusers: vec![bond(7)], ..lie.clone() };
        book.refresh(vec![honest.clone(), accused], Some(fresh()), late + COURT_MOVE_REPLAN_DAA + 1);
        assert_eq!(book.next(&judge, late + COURT_MOVE_REPLAN_DAA + 1, false, true), None);
        assert_eq!(book.verdict(&lie.claim_id), Some(PalwOperatorDaVerdictV1::Refuted));
        // Unjudged: settled here.
        let mut book = PalwOperatorDaBookV1::new(registrations());
        book.refresh(vec![lie.clone()], Some(fresh()), 1_500);
        book.judge(lie.claim_id, PalwOperatorDaVerdictV1::Unjudged("its class does not resolve on this host"));
        assert!(book.is_settled(&lie.claim_id));
        assert_eq!(book.next(&judge, 1_500, false, true), None);
        // A blind queued item is not this node's once its turn passed.
        let blind = candidate(0xD3, 1_500, panel());
        let owner = palw_operator_da_order_v1(&blind, &ops)[0];
        let mut book = PalwOperatorDaBookV1::new(registrations());
        book.refresh(vec![blind.clone()], Some(fresh()), 1_500);
        assert!(book.still_files(&blind.claim_id, &owner, 1_500));
        assert!(!book.still_files(&blind.claim_id, &owner, 1_500 + PALW_OPERATOR_DA_TURN_DAA_V1));
        // The replay slot: one in flight, and a ledger refusal holds a re-plan.
        assert!(book.may_start_replay(1_500));
        book.replay_held(1_500);
        assert!(!book.may_start_replay(1_500 + COURT_MOVE_REPLAN_DAA - 1));
        assert!(book.may_start_replay(1_500 + COURT_MOVE_REPLAN_DAA));
    }

    /// **The fleet, simulated on the turn rule and the book** — eight operator nodes, a stream of
    /// external licensed claims at 1.25 a DAA on random five-seat panels: a third honest attempts, a
    /// third junk attempts (a replay refutes them), a third free prompts no operator can judge. Every
    /// replay returns within the DAA and every carrier lands a DAA later. Then:
    /// * **no honest claim is ever accused** — the burn on honest judged claims is zero;
    /// * every junk claim is accused exactly once, and every free prompt exactly once (blind);
    /// * an honest claim is replayed at most [`PALW_OPERATOR_DA_JUDGES_V1`] times across the fleet;
    /// * with the rank-0 node of every claim DOWN the next owner takes it one turn later, still once;
    /// * no node files twice within two DAA.
    #[test]
    fn the_fleet_accuses_every_lie_once_and_no_honest_claim() {
        let ops = operators();
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Kind {
            Honest,
            Junk,
            Blind,
        }
        for down_rank0 in [false, true] {
            let claims: Vec<(PalwOperatorDaCandidateV1, Kind)> = (0..300u64)
                .map(|n| {
                    let key = palw_operator_da_rank_key_v1(&Hash64::from_u64_word(0x5EA7 + n), &bond(0));
                    let bytes = key.as_byte_slice();
                    // Two operator seats and three outside ones, drawn from the claim's own bytes.
                    let (x, y) = ((bytes[0] % 8) as u32, (bytes[1] % 7) as u32);
                    let y = if y >= x { y + 1 } else { y };
                    let seats = vec![bond(x), bond(y), outsider(1), outsider(2), outsider(3)];
                    let stage = 2_000 + n * 4 / 5;
                    match n % 3 {
                        0 => (judged(0xF000 + n, stage, seats), Kind::Honest),
                        1 => (judged(0xF000 + n, stage, seats), Kind::Junk),
                        _ => (candidate(0xF000 + n, stage, seats), Kind::Blind),
                    }
                })
                .collect();
            let kind: BTreeMap<Hash64, Kind> = claims.iter().map(|(c, k)| (c.claim_id, *k)).collect();
            let first_owner = |c: &PalwOperatorDaCandidateV1| {
                let judges = palw_operator_da_judges_v1(c, &ops);
                if judges.is_empty() { palw_operator_da_order_v1(c, &ops)[0] } else { judges[0] }
            };
            let down: BTreeMap<Hash64, PalwBondKeyV2> =
                claims.iter().filter(|_| down_rank0).map(|(c, _)| (c.claim_id, first_owner(c))).collect();
            let mut books: Vec<PalwOperatorDaBookV1> = ops.iter().map(|_| PalwOperatorDaBookV1::new(registrations())).collect();
            let mut accused: BTreeMap<Hash64, Vec<PalwBondKeyV2>> = Default::default();
            let mut replays: BTreeMap<Hash64, usize> = Default::default();
            let mut per_node: BTreeMap<PalwBondKeyV2, Vec<u64>> = Default::default();
            // (claim, accuser, lands at)
            let mut in_flight: Vec<(Hash64, PalwBondKeyV2, u64)> = Vec::new();
            for now in 2_000..2_000 + 300 * 4 / 5 + 600 {
                in_flight.retain(|(claim, accuser, lands)| {
                    if *lands <= now {
                        accused.entry(*claim).or_default().push(*accuser);
                        false
                    } else {
                        true
                    }
                });
                let chain: Vec<PalwOperatorDaCandidateV1> = claims
                    .iter()
                    .filter(|(c, _)| c.stage_daa <= now)
                    .map(|(c, _)| PalwOperatorDaCandidateV1 {
                        operator_accusers: accused.get(&c.claim_id).cloned().unwrap_or_default(),
                        ..c.clone()
                    })
                    .collect();
                for (me, book) in ops.iter().zip(books.iter_mut()) {
                    book.refresh(chain.clone(), Some(fresh()), now);
                    // A node acts on what the book offers it (a few steps a DAA); "queued" clears when
                    // its carrier lands.
                    for _ in 0..4 {
                        let queued = in_flight.iter().any(|(_, accuser, _)| accuser == me);
                        let Some((claim, action)) = book.next(me, now, queued, true) else { break };
                        if down.get(&claim) == Some(me) {
                            book.settle(claim); // a node that is down does nothing
                            continue;
                        }
                        match action {
                            PalwOperatorDaActionV1::Replay => {
                                *replays.entry(claim).or_default() += 1;
                                let verdict = match kind[&claim] {
                                    Kind::Honest => PalwOperatorDaVerdictV1::Reproduces,
                                    Kind::Junk => PalwOperatorDaVerdictV1::Refuted,
                                    Kind::Blind => unreachable!("a free prompt is never replayed"),
                                };
                                book.judge(claim, verdict);
                            }
                            PalwOperatorDaActionV1::FileRefuted { .. } | PalwOperatorDaActionV1::FileBlind { .. } => {
                                book.queued(claim, 1, now);
                                per_node.entry(*me).or_default().push(now);
                                in_flight.push((claim, *me, now + 1));
                            }
                        }
                    }
                }
            }
            for (c, k) in &claims {
                let got = accused.get(&c.claim_id).map(Vec::len).unwrap_or(0);
                match k {
                    Kind::Honest => {
                        assert_eq!(got, 0, "down_rank0={down_rank0}: an honest claim is never accused");
                        let r = replays.get(&c.claim_id).copied().unwrap_or(0);
                        assert!(
                            (1..=PALW_OPERATOR_DA_JUDGES_V1).contains(&r),
                            "down_rank0={down_rank0}: an honest claim replayed {r} times"
                        );
                    }
                    Kind::Junk | Kind::Blind => {
                        assert_eq!(got, 1, "down_rank0={down_rank0}: {k:?} claim {} accused once: {:?}", c.claim_id, accused.get(&c.claim_id))
                    }
                }
                if down_rank0 && got == 1 {
                    assert_ne!(Some(&accused[&c.claim_id][0]), down.get(&c.claim_id), "the down node filed nothing");
                }
            }
            for (node, filed) in &per_node {
                assert!(filed.windows(2).all(|w| w[1] >= w[0] + PALW_OPERATOR_DA_MIN_GAP_DAA_V1), "{node:?}: the per-DAA cap");
            }
        }
    }

    /// **What this duty costs at testnet-12's claim rate** — printed, and the budget's figures pinned.
    /// Before the replay gate every external licensed claim was accused and every honest one burned
    /// its refuted exposure off a genesis bond (320.10 MSK on the floor): 230k MSK a day at 1 claim a
    /// DAA, 86% of every genesis bond in four weeks. Now an honest judged claim burns nothing (it
    /// costs ≤ 3 replays), and the blind path — claims no operator can judge — is capped: at most 2% of
    /// C held, so at most ~0.46% of a genesis bond burned a day, and never below 90% of it.
    #[test]
    fn the_expected_load_at_t12s_claim_rate() {
        use kaspa_consensus_core::palw_da_rcore_v1::palw_da_session_exposure_v1;
        let floor_commitment = 320_095 * MSK / 100; // 3,200.95 MSK (ADR-0152 §3.8's floor reservation)
        let floor_min_collateral = 13_000 * 100_000_000u64;
        let exposure = palw_da_session_exposure_v1(floor_commitment, floor_min_collateral);
        assert_eq!(exposure, 32_009_500_000, "10% of the floor's commitment: 320.095 MSK");
        let c = GENESIS_C as u128;
        let held_span_daa: u128 = 120 + 3_002; // licence → Final → retirement (the review's probe: 3,122)
        let daa_per_day = 86_400 / 120u128;
        let cap = c * PALW_OPERATOR_DA_BLIND_HELD_CAP_PERMILLE_V1 / 1000;
        let blind_burn_per_day = cap * daa_per_day / held_span_daa;
        let floor_loss = c - c * PALW_OPERATOR_DA_COLLATERAL_FLOOR_PERMILLE_V1 / 1000;
        for lambda_x100 in [25u128, 100, 150] {
            let accused_before = lambda_x100 * daa_per_day / 100;
            let burned_before = accused_before * exposure;
            let replays_per_node = lambda_x100 * daa_per_day * PALW_OPERATOR_DA_JUDGES_V1 as u128 / 100 / 8;
            println!(
                "λ = {:.2} external licensed claims/DAA: before the gate {} MSK a day burned fleet-wide ({:.2}% of a genesis bond each, \
                 {:.0}% in 28 days) — now 0 on honest judged claims (≤ {} replays a node a day), and blind filings capped at {} MSK \
                 held a bond: ≤ {} MSK burned a bond a day ({:.2}% of C), never past {} MSK a bond (the 90% floor)",
                lambda_x100 as f64 / 100.0,
                burned_before / MSK,
                (burned_before / 8) as f64 * 100.0 / c as f64,
                ((burned_before / 8) as f64 * 28.0 * 100.0 / c as f64).min(100.0),
                replays_per_node,
                cap / MSK,
                blind_burn_per_day / MSK,
                blind_burn_per_day as f64 * 100.0 / c as f64,
                floor_loss / MSK,
            );
        }
        assert_eq!(blind_burn_per_day / MSK, 4_331, "the blind cap's burn bound: ~4.3k MSK a bond a day");
        assert!(blind_burn_per_day * 1000 / c < 5, "under 0.5% of a genesis bond a day");
        assert_eq!(floor_loss / MSK, 93_906, "lane B never takes more than 10% of a genesis bond");
        // The fleet's ceiling on filings: one every two DAA a node.
        assert_eq!(8 * 100 / PALW_OPERATOR_DA_MIN_GAP_DAA_V1, 400, "4 accusations a DAA across eight nodes");
    }

    /// **Wired where it runs, with no flag** (source inspection): the tick calls the lane after the
    /// reporter filer and before the carriers, holds its book from the operator registrations the
    /// params name, hands it the seat slots' room, and no CLI option reaches it.
    #[test]
    fn the_tick_wires_the_lane_before_the_carriers_and_behind_no_flag() {
        let panel = include_str!("palw_panel.rs");
        let tick = panel.find("self.operator_da_tick_v1(").expect("the tick calls the lane");
        let reporter = panel.find("// --- P2-8: the reporter's commit–reveal filer").expect("the reporter filer");
        let carriers = panel.find("// --- the collector + submitter's half ---").expect("the carriers");
        assert!(reporter < tick && tick < carriers, "after the reporter filer, before the priority lane carries the court queue");
        assert!(panel.contains(
            "palw_operator_da::PalwOperatorDaBookV1::new(palw_operator_da::palw_operator_registrations_v1(&self.consensus_config.params))"
        ));
        assert!(panel[tick..carriers].contains("seat_replays.has_room(false)"), "the lane's replay yields to the seat's slots");
        let args = include_str!("args.rs");
        assert!(!args.contains("operator-da") && !args.contains("operator_da"), "no opt-in or opt-out flag");
    }
}
