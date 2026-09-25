//! **Lane B of the panel-seed stopgap (2026-09-26): the operator's non-seat data-availability
//! filer** (node policy; a child module of `palw_panel`, whose loop holds its one book and calls
//! [`PalwPanelService::operator_da_tick_v1`] once a tick).
//!
//! **Why.** The panel-seed CRITICAL (`docs/t12-panel-seed-2026-09-25.md`): while audit P0-10 is
//! open a lottery win costs ~279 junk BLAKE2b draws, so an attacker re-rolls a claim's panel until
//! its own Sybil seats cover it, and a coverage lie reaches `Final` with 0 slash. The user's decision
//! of 2026-09-26 01:00 pairs fence F1 and the operator-anchor fence with this node duty: **an
//! operator node that is not a seat on a claim's panel accuses the claim of withholding**, so a junk
//! claim meets an honest accuser even when its panel is captured (ADR-0152 §3.9 option 2, DA-1 /
//! DA-8: any bond may accuse; a non-seat session neither pauses the claim nor spends a seat's
//! budget).
//!
//! **What the accusation does** (the DA court as it stands at `0e8ec984e`, ADR-0152 §3.11):
//! * the object is P2-6's own — a `DefaultAccused` naming row 0, tile 0 (C-9,
//!   `PALW_DA_AUTO_NAMED_UNIT_V1`), built by the ONE builder (`palw_da_accusation_object_v1`) and
//!   asked of the fold first (`palw_da_accusation_check_v1`, C-8 with A-6's room);
//! * the fold opens a session keyed `(claim, accuser)` and draws up to three more units inside the
//!   committed run, seeded by the accepting block (DA-3); the producer — or a covering `Valid` signer
//!   (X7) — must answer every unit with a `MaterialDisclosedV2` within `W_disclose` (= the challenge
//!   window, 1,200 DAA on testnet-12);
//! * **a claim with no material behind its roots** (P0-10's junk attempt, ADR §3.9's "naive" row)
//!   cannot answer: the session defaults — S1 at `Licensed` (the commitment, `E` from uncommitted
//!   stake if released, a strike), or after `Final` S3 `min(25%·C, 3G)` plus the whole vesting row
//!   (the session re-keys the row and keeps the locks live, DA-5) — and every covering full-mask
//!   `Valid` signer takes S4 (DA-7, armed since P2-7). The accuser's exposure comes back, and it wins
//!   the DA reward R (the earliest defaulted session's accuser, R-1);
//! * **an honest claim answers** (its producer's P2-7 responder, one `Flat`): the session is refuted
//!   and its exposure — `min(10% · S_P(stage), min_collateral)`, 320.10 MSK on the floor, 369.52 on
//!   8k, 6,294.38 on 2M — is held on the accuser's free half until the claim record retires, then
//!   BURNED (refunded only if the claim is convicted, DA-6). **That burn is this duty's price.**
//! * **What it does not convict** (ADR §3.9's "garbage" row): a producer that committed a well-formed
//!   garbage trace answers the disclosure. Convicting it needs a replay of the claim's job against the
//!   disclosed binding (a `StepLeaf` demand, then `ExecutorRefuted` / `LogitsNotStepOutput`) — P2-8b /
//!   P2-8d, which today run only on a seat's own replay. Named, not closed here.
//!
//! **Which claims** (`palw_operator_da_v1::palw_operator_da_candidates_v1`, the chain's read): licensed
//! (`ReceiptLicensed`, or `Final` with an unmatured vesting row) claims produced OUTSIDE the operator
//! set that relied on at least one `Valid` signer outside it. Under the stopgap's operator trust a
//! claim the operator produced, or licensed by operator signers alone, is vouched for by the operator's
//! own replay, and a claim that never licenses forfeits at its second failed panel (D1, S0′) — so
//! these are exactly the claims a captured panel could carry to `Final` unverified. (When X10 arms,
//! unaccused unlicensed withholding becomes free and the read must offer the `Live` stage too.)
//!
//! **Who files** — identity decides, nothing opts in ([[protocol-duties-are-always-on-not-flags]]):
//! a node whose bond is one of the operator's bonds — the bundle's genesis registrations
//! ([`palw_operator_bonds_v1`], testnet-12's eight cards `5e0d5f1b…:0..7`) — that carries
//! (`--palw-fee-outpoint`), past `palw_rcore_plus`. Anything else is a clean no-op and keeps no book.
//!
//! **Exactly one filer a claim, with a fallback** ([`palw_operator_da_plan_v1`]): the claim's eligible
//! accusers are the operator bonds that are neither its producer nor a seat of its current panel,
//! ranked by `H(domain ‖ claim ‖ bond)` ([`palw_operator_da_order_v1`]) — every node computes the same
//! order off the same tip. Time from the stage's DAA is cut into turns of
//! [`PALW_OPERATOR_DA_TURN_DAA_V1`]; turn `t` belongs to rank `t mod n`, and only the turn's owner
//! files. A node that is down or out of room simply lets its turn pass; the next owner files in the
//! next turn. After a fleet restart, a node joining late, or any delay, still exactly one node is on
//! turn — never all at once. **Backoff:** a claim on which any operator bond already holds (or held)
//! a session — an honest accuser is there — is left alone; an accuser OUTSIDE the operator set is
//! not a reason to back off (it may be the producer's own Sybil, answering at `deadline − 1`).
//!
//! **Load.** One item of this lane in the court queue at a time, at least
//! [`PALW_OPERATOR_DA_MIN_GAP_DAA_V1`] between two filings of one node (≤ 0.5 a DAA a node, 4 a DAA
//! the fleet), a claim sent at most [`PALW_OPERATOR_DA_SENDS_V1`] times by one node and never again
//! within [`PALW_OPERATOR_DA_RESEND_DAA_V1`], and the chain read once a DAA. The item rides the
//! priority lane (the court queue, as P2-6's accusations do) dated at the end of the node's turn, so
//! EDF puts every seat's court filing (due within its 60-DAA landing margin) ahead of it; it takes at
//! most one slot every two DAA from the node's own carriers, and a possession proof keeps its two
//! guards — M1's escalation ahead of the court queue, and V07's readiness lane (`rcore/n1-carrier`)
//! where the main slot is taken. No carrier code, lane or scheduler rule changes here.
//!
//! **Expected traffic** (the tests' `the_expected_load_at_t12s_claim_rate` prints it): with `λ`
//! external licensed claims a DAA, the fleet files `λ` accusations a DAA (one a claim), each drawing
//! one answer carrier from the producer: `2λ` carriers a DAA network-wide, `λ/8` accusations a DAA per
//! operator node. At the floor lane's ~1 claim a DAA (ADR-0152 T-2) and 120 s a DAA: 720 accusations
//! and 720 answers a day, one filing per node every ~16 min; burned exposure ≈ 230k MSK a day across
//! the eight bonds (~3% of one 939,063 MSK genesis bond a day each) while every external claim is
//! honest; held exposure ≈ 125k MSK a bond in steady state (a session's exposure is held ~3,120 DAA,
//! the licence-to-retirement span) — inside A-6's free half (≈ 469k).
//!
//! **Consensus-inert.** Every object is one the fold already admits from any bond; nothing here is a
//! rule, an id or a fingerprint.

use super::*;
use kaspa_consensus_core::palw_da_rcore_v1::{PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1, PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1};
use kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaCandidateV1;
use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1;

/// The court queue's round of this lane's accusation of a claim ([`palw_operator_da_queue_key_v1`]) —
/// a round no court move, P2-6 accusation (`u32::MAX`), reporter-filer object (`u32::MAX − 3 ..=
/// u32::MAX − 1`), P2-8d demand (`u32::MAX − 6`) or answer (a folded unit digest) is keyed by.
pub(super) const PALW_OPERATOR_DA_QUEUE_ROUND_V1: u32 = u32::MAX - 7;
/// The domain of an eligible accuser's rank on a claim.
pub(super) const PALW_OPERATOR_DA_RANK_DOMAIN_V1: &[u8] = b"misaka-node/operator-da/rank/v1";
/// **One accuser's turn, in DAA** — long enough for a carrier sent at its start to land and be read
/// back (a carrier lands in a DAA or two; the pre-t12 drill's slowest proof took ~3), so the next
/// owner finds the session and backs off; short enough that a claim whose owner is down is accused
/// within an hour of its licence.
pub(super) const PALW_OPERATOR_DA_TURN_DAA_V1: u64 = 30;
/// The fewest DAA between two filings of one node (the per-DAA cap: at most one every two DAA).
pub(super) const PALW_OPERATOR_DA_MIN_GAP_DAA_V1: u64 = 2;
/// A filing not on chain this long after it was queued is taken as lost, and may be sent once more.
pub(super) const PALW_OPERATOR_DA_RESEND_DAA_V1: u64 = PALW_OPERATOR_DA_TURN_DAA_V1;
/// Sends one node makes of one claim's accusation: the first, and one more if the first was lost.
pub(super) const PALW_OPERATOR_DA_SENDS_V1: u8 = 2;

/// The court queue's key of this lane's accusation of `claim` — one a claim.
pub(super) fn palw_operator_da_queue_key_v1(claim: Hash64) -> (Hash64, u32, bool) {
    (claim, PALW_OPERATOR_DA_QUEUE_ROUND_V1, false)
}

/// Whether a court-queue entry is this lane's.
pub(super) fn palw_operator_da_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::DefaultAccused { .. }) && (round, responder) == (PALW_OPERATOR_DA_QUEUE_ROUND_V1, false)
}

/// **The operator's bonds: the bundle's genesis registrations**, in bond order — on testnet-12 the
/// eight cards of the premine `5e0d5f1b…`, outputs 0..7. Empty off `ConsensusV2`.
pub(crate) fn palw_operator_bonds_v1(params: &kaspa_consensus_core::config::params::Params) -> Vec<PalwBondKeyV2> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Vec::new();
    };
    let bonds: std::collections::BTreeSet<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|object| match object {
            PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(*bond),
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

/// Why a node does not accuse a claim now.
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
    /// No operator bond is eligible (every one is the producer or a seat).
    NoneEligible,
}

/// **What this node does about one candidate now.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwOperatorDaPlanV1 {
    /// This node's turn: file, and have it land by `due` (the turn's end, when the next owner steps
    /// in). `rank` of `of` eligible.
    File {
        rank: usize,
        of: usize,
        due: u64,
    },
    /// Not now: this node's next turn starts at `from_daa` (or DA-8's three open non-seat sessions
    /// are full, and `from_daa` is the next DAA).
    Wait {
        from_daa: u64,
    },
    Skip(PalwOperatorDaSkipV1),
}

/// **The turn rule** (the module's "Exactly one filer a claim"): turn `t = ⌊(now − stage_daa) /
/// PALW_OPERATOR_DA_TURN_DAA_V1⌋` belongs to rank `t mod n` of [`palw_operator_da_order_v1`]; only the
/// owner files. Pure over the candidate, so every node on the same tip gets the same answer.
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
    let order = palw_operator_da_order_v1(claim, operators);
    let n = order.len() as u64;
    let Some(rank) = order.iter().position(|bond| bond == me) else { return P::Skip(S::NoneEligible) };
    let turn = now_daa.saturating_sub(claim.stage_daa) / PALW_OPERATOR_DA_TURN_DAA_V1;
    let mine_next = turn + (rank as u64 + n - turn % n) % n;
    if mine_next != turn {
        return P::Wait { from_daa: claim.stage_daa.saturating_add(mine_next.saturating_mul(PALW_OPERATOR_DA_TURN_DAA_V1)) };
    }
    if claim.open_non_seat >= PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1 {
        return P::Wait { from_daa: now_daa + 1 };
    }
    let due = claim.stage_daa.saturating_add((turn + 1).saturating_mul(PALW_OPERATOR_DA_TURN_DAA_V1)).min(claim.accuse_until_daa);
    P::File { rank, of: order.len(), due }
}

/// **The lane's book** (in memory: a restart forgets it, and the chain's `operator_accusers` is what
/// stops a second filing — the book only spaces this node's own).
#[derive(Clone, Debug, Default)]
pub(super) struct PalwOperatorDaBookV1 {
    /// The operator set, derived once from the params.
    operators: Vec<PalwBondKeyV2>,
    /// The chain's candidates, read once a DAA (`read_at`).
    candidates: Vec<PalwOperatorDaCandidateV1>,
    read_at: Option<u64>,
    /// claim → (the DAA this node last queued its accusation at, how many times).
    sent: BTreeMap<Hash64, (u64, u8)>,
    /// claim → the DAA before which it is not asked again (the fold refused it for a reason a wait
    /// may change: DA-8's open cap).
    deferred: BTreeMap<Hash64, u64>,
    /// Claims done with (the fold's `AccusedBefore` / `Answered`, a seat now, a refusal no wait
    /// changes, a shielded claim said once).
    settled: std::collections::BTreeSet<Hash64>,
    /// The DAA of this node's last filing (the per-DAA cap).
    last_filed_daa: Option<u64>,
    /// The DAA the fold last refused a filing for A-6's room: nothing is asked for a re-plan after.
    room_refused_at: Option<u64>,
    /// The DAA a scan last found nothing to file at: the candidates are not scanned again within it
    /// (every change of the book clears it).
    pub(super) idle_at: Option<u64>,
}

impl PalwOperatorDaBookV1 {
    pub(super) fn new(operators: Vec<PalwBondKeyV2>) -> Self {
        Self { operators, ..Default::default() }
    }

    pub(super) fn operators(&self) -> &[PalwBondKeyV2] {
        &self.operators
    }

    /// Forget everything but the operator set (the lane disarmed: below the fence, no carrier).
    pub(super) fn clear(&mut self) {
        *self = Self::new(std::mem::take(&mut self.operators));
    }

    /// Whether the candidates are due a re-read at `now_daa` (once a DAA).
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// The chain's candidates at `now_daa`; every memory of a claim that left them is dropped, so the
    /// book is bounded by the chain's own candidate list.
    pub(super) fn refresh(&mut self, candidates: Vec<PalwOperatorDaCandidateV1>, now_daa: u64) {
        let live: std::collections::BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        self.sent.retain(|claim, _| live.contains(claim));
        self.deferred.retain(|claim, until| live.contains(claim) && now_daa < *until);
        self.settled.retain(|claim| live.contains(claim));
        self.candidates = candidates;
        self.read_at = Some(now_daa);
        self.idle_at = None;
    }

    pub(super) fn candidate(&self, claim: &Hash64) -> Option<&PalwOperatorDaCandidateV1> {
        self.candidates.iter().find(|c| c.claim_id == *claim)
    }

    /// **The claim this node files next, if any, and its plan** — nothing while one of its items is
    /// queued (`queued`), within the per-DAA gap, or within a re-plan of a room refusal; otherwise the
    /// oldest candidate on this node's turn that it has not settled, deferred, or sent too recently or
    /// too often.
    pub(super) fn next(&self, me: &PalwBondKeyV2, now_daa: u64, queued: bool) -> Option<(Hash64, PalwOperatorDaPlanV1)> {
        if queued
            || self.last_filed_daa.is_some_and(|at| now_daa < at.saturating_add(PALW_OPERATOR_DA_MIN_GAP_DAA_V1))
            || self.room_refused_at.is_some_and(|at| now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
        {
            return None;
        }
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
            match palw_operator_da_plan_v1(claim, me, &self.operators, now_daa) {
                plan @ PalwOperatorDaPlanV1::File { .. } => Some((id, plan)),
                _ => None,
            }
        })
    }

    /// This node queued its accusation of `claim` at `now_daa`.
    pub(super) fn queued(&mut self, claim: Hash64, now_daa: u64) {
        let entry = self.sent.entry(claim).or_insert((now_daa, 0));
        *entry = (now_daa, entry.1.saturating_add(1));
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
    /// **Lane B's half of the tick** (the module's header): re-read the candidates once a DAA, drop a
    /// queued item that is no longer this node's to file, and queue at most one new accusation — asked
    /// of the fold first, built by the one builder, dated at the end of this node's turn.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn operator_da_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwOperatorDaBookV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
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
            let candidates = session.clone().spawn_blocking(move |c| c.palw_operator_da_candidates_v1(operators)).await;
            book.refresh(candidates, current_daa);
            // A queued item that is no longer this node's to file now (its turn passed, an operator
            // accuses, the claim left the candidates) leaves the queue unsent.
            court_pending.retain(|(claim, round, responder, object)| {
                if !palw_operator_da_queued_v1(*round, *responder, object) {
                    return true;
                }
                book.candidate(claim).is_some_and(|c| {
                    matches!(palw_operator_da_plan_v1(c, &bond_key, book.operators(), current_daa), PalwOperatorDaPlanV1::File { .. })
                })
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
        // One scan of the candidates a DAA while there is nothing to file.
        if book.idle_at == Some(current_daa) {
            return;
        }
        let queued = court_pending.iter().any(|(_, round, responder, object)| palw_operator_da_queued_v1(*round, *responder, object));
        let Some((claim, PalwOperatorDaPlanV1::File { rank, of, due })) = book.next(&bond_key, current_daa, queued) else {
            book.idle_at = Some(current_daa);
            return;
        };
        let Some(check) = session.palw_da_accusation_check_v1(claim, bond_key) else { return };
        match (palw_operator_da_step_v1(&check), &check) {
            (PalwOperatorDaStepV1::File, PalwDaAccusationCheckV1::File { unit, admission }) => {
                match kaspa_consensus_core::palw_da_rcore_v1::palw_da_accusation_object_v1(
                    &network_domain,
                    claim,
                    *unit,
                    bond_key,
                    |message, context| self.sign(message, context),
                ) {
                    Ok(object) => {
                        let (producer, outside) = book
                            .candidate(&claim)
                            .map(|c| (format!("{:?}", c.producer.0), c.outside_signers.len()))
                            .unwrap_or_default();
                        info!(
                            "[{PALW_PANEL}] claim {claim}: the operator's non-seat accusation (lane B) — rank {rank} of {of} eligible, \
                             producer {producer}, {outside} outside Valid signer(s); {unit:?} at stage {:?}, {} sompi on this bond's \
                             free half, the session's deadline DAA {} (panel-seed stopgap (B), DA-8)",
                            admission.stage, admission.exposure, admission.deadline_daa
                        );
                        let key = palw_operator_da_queue_key_v1(claim);
                        court_due.insert(key, due.max(current_daa));
                        court_pending.push((key.0, key.1, key.2, object));
                        book.queued(claim, current_daa);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1;
    use kaspa_consensus_core::palw_state_v2::{PalwDaAdmissionV1, PalwStateV2Error};

    fn bond(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0x5E0D), n))
    }

    fn outsider(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xE7), n))
    }

    fn operators() -> Vec<PalwBondKeyV2> {
        (0..8).map(bond).collect()
    }

    /// A claim produced outside, licensed at `stage_daa` by a panel of `seats`.
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
        }
    }

    fn panel() -> Vec<PalwBondKeyV2> {
        vec![bond(1), outsider(1), outsider(2), bond(4), outsider(3)]
    }

    /// **The operator set is testnet-12's eight genesis cards** — the premine's outputs 0..7, read
    /// off the bundle's genesis registrations; nothing on a network without a V2 bundle.
    #[test]
    fn the_operator_set_is_testnet12s_eight_genesis_bonds() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let bonds = palw_operator_bonds_v1(&t12);
        let premine = kaspa_consensus_core::config::premine::premine_txid_for(t12.net);
        assert_eq!(bonds.len(), 8, "eight genesis bonds");
        assert!(bonds.iter().all(|b| b.0.transaction_id == premine), "all on the premine {premine}");
        assert_eq!(bonds.iter().map(|b| b.0.index).collect::<Vec<_>>(), (0..8).collect::<Vec<u32>>(), "outputs 0..7");
        assert!(palw_operator_bonds_v1(&Params::from(NetworkId::new(NetworkType::Mainnet))).is_empty(), "no genesis cards on mainnet");
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

    /// **Exactly one owner at every DAA, whatever the DAA** — the rank-0 node's turn opens at the
    /// licence, the next rank's a turn later, round and round; after any delay (a fleet restart at
    /// licence + 500) still exactly one node is on turn. Each Wait names the DAA its turn opens.
    #[test]
    fn exactly_one_eligible_node_is_on_turn_at_every_daa() {
        let ops = operators();
        let c = candidate(0xC2, 1_500, panel());
        let order = palw_operator_da_order_v1(&c, &ops);
        for now in 1_500..1_500 + 3 * 6 * PALW_OPERATOR_DA_TURN_DAA_V1 {
            let on_turn: Vec<(PalwBondKeyV2, PalwOperatorDaPlanV1)> = ops
                .iter()
                .map(|me| (*me, palw_operator_da_plan_v1(&c, me, &ops, now)))
                .filter(|(_, plan)| matches!(plan, PalwOperatorDaPlanV1::File { .. }))
                .collect();
            assert_eq!(on_turn.len(), 1, "DAA {now}: {on_turn:?}");
            let turn = (now - 1_500) / PALW_OPERATOR_DA_TURN_DAA_V1;
            let (owner, plan) = on_turn[0];
            assert_eq!(owner, order[(turn % 6) as usize], "turn {turn} is rank {}'s", turn % 6);
            assert_eq!(
                plan,
                PalwOperatorDaPlanV1::File {
                    rank: (turn % 6) as usize,
                    of: 6,
                    due: 1_500 + (turn + 1) * PALW_OPERATOR_DA_TURN_DAA_V1
                }
            );
            for me in order.iter().filter(|me| **me != owner) {
                let PalwOperatorDaPlanV1::Wait { from_daa } = palw_operator_da_plan_v1(&c, me, &ops, now) else {
                    panic!("{me:?} waits")
                };
                assert!(from_daa > now && (from_daa - 1_500) % PALW_OPERATOR_DA_TURN_DAA_V1 == 0);
                assert!(
                    matches!(palw_operator_da_plan_v1(&c, me, &ops, from_daa), PalwOperatorDaPlanV1::File { .. }),
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
        }
        assert_eq!(
            palw_operator_da_plan_v1(&accused, &owner, &ops, 1_500),
            PalwOperatorDaPlanV1::Skip(PalwOperatorDaSkipV1::OperatorAccuses)
        );
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

    /// **The book spaces this node's filings.** One queued at a time; two DAA between filings; a room
    /// refusal holds everything a re-plan; a sent claim is not sent again within a turn, and at most
    /// twice; a deferred or settled claim is skipped; a claim that leaves the chain's list leaves the
    /// book.
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
        let mut book = PalwOperatorDaBookV1::new(ops.clone());
        book.refresh(vec![a.clone(), b.clone()], 1_500);
        let (first, _) = book.next(&me, 1_500, false).expect("its turn");
        assert_eq!(first, a.claim_id, "the oldest first (the chain's order)");
        assert_eq!(book.next(&me, 1_500, true), None, "one of this lane's items queued at a time");
        book.queued(first, 1_500);
        assert_eq!(book.next(&me, 1_501, false), None, "two DAA between filings");
        let (second, _) = book.next(&me, 1_502, false).expect("the gap passed");
        assert_eq!(second, b.claim_id, "the sent claim is spaced out");
        book.queued(second, 1_502);
        assert_eq!(book.next(&me, 1_510, false), None, "both sent within a turn");
        // Still on chain unaccused a turn later (the carrier was lost): each is sent once more at most.
        // Their turn has passed by then (1_530 is rank 1's), so the same node files again only on its
        // next turn: 1_500 + 6 turns.
        let again = 1_500 + 6 * PALW_OPERATOR_DA_TURN_DAA_V1;
        assert_eq!(book.next(&me, again, false).map(|(c, _)| c), Some(a.claim_id));
        book.queued(a.claim_id, again);
        book.queued(b.claim_id, again + 2);
        assert_eq!(book.next(&me, again + 12 * PALW_OPERATOR_DA_TURN_DAA_V1, false), None, "never a third send");
        // Room and deferral.
        let mut book = PalwOperatorDaBookV1::new(ops.clone());
        book.refresh(vec![a.clone(), b.clone()], 1_500);
        book.room_refused(1_500);
        assert_eq!(book.next(&me, 1_500 + COURT_MOVE_REPLAN_DAA - 1, false), None, "a room refusal holds a re-plan");
        book.defer(a.claim_id, 1_520);
        assert_eq!(book.next(&me, 1_510, false).map(|(c, _)| c), Some(b.claim_id), "a deferred claim is skipped");
        book.settle(b.claim_id);
        assert_eq!(book.next(&me, 1_515, false), None);
        assert_eq!(book.next(&me, 1_520, false).map(|(c, _)| c), Some(a.claim_id), "the deferral ends");
        book.refresh(vec![a.clone()], 1_521);
        assert!(!book.is_settled(&b.claim_id), "a claim that left the chain's list leaves the book");
    }

    /// **The fleet, simulated on the turn rule and the book** — eight operator nodes, a stream of
    /// external licensed claims at 1.25 a DAA, each on a random five-seat panel. Every claim gets
    /// exactly one accusation (the first owner files; its carrier lands a DAA later and every other
    /// node backs off); with the rank-0 node of every claim DOWN the next owner files one turn later,
    /// still once; no node files twice within two DAA; and each node carries about an eighth.
    #[test]
    fn the_fleet_files_each_claim_once_and_shares_the_load() {
        let ops = operators();
        for down_rank0 in [false, true] {
            let claims: Vec<PalwOperatorDaCandidateV1> = (0..400u64)
                .map(|n| {
                    let key = palw_operator_da_rank_key_v1(&Hash64::from_u64_word(0x5EA7 + n), &bond(0));
                    let bytes = key.as_byte_slice();
                    // Two operator seats and three outside ones, drawn from the claim's own bytes.
                    let (x, y) = ((bytes[0] % 8) as u32, (bytes[1] % 7) as u32);
                    let y = if y >= x { y + 1 } else { y };
                    let mut c =
                        candidate(0xF000 + n, 2_000 + n * 4 / 5, vec![bond(x), bond(y), outsider(1), outsider(2), outsider(3)]);
                    c.accuse_until_daa = c.stage_daa + 3_000;
                    c
                })
                .collect();
            let down: std::collections::BTreeMap<Hash64, PalwBondKeyV2> =
                claims.iter().filter(|_| down_rank0).map(|c| (c.claim_id, palw_operator_da_order_v1(c, &ops)[0])).collect();
            let mut books: Vec<PalwOperatorDaBookV1> = ops.iter().map(|_| PalwOperatorDaBookV1::new(ops.clone())).collect();
            let mut accused: std::collections::BTreeMap<Hash64, Vec<(PalwBondKeyV2, u64)>> = Default::default();
            let mut per_node: std::collections::BTreeMap<PalwBondKeyV2, Vec<u64>> = Default::default();
            // (claim, accuser, lands at)
            let mut in_flight: Vec<(Hash64, PalwBondKeyV2, u64)> = Vec::new();
            for now in 2_000..2_000 + 400 * 4 / 5 + 400 {
                in_flight.retain(|(claim, accuser, lands)| {
                    if *lands <= now {
                        accused.entry(*claim).or_default().push((*accuser, now));
                        false
                    } else {
                        true
                    }
                });
                let chain: Vec<PalwOperatorDaCandidateV1> = claims
                    .iter()
                    .filter(|c| c.stage_daa <= now)
                    .map(|c| PalwOperatorDaCandidateV1 {
                        operator_accusers: accused.get(&c.claim_id).map(|a| a.iter().map(|(b, _)| *b).collect()).unwrap_or_default(),
                        ..c.clone()
                    })
                    .collect();
                for (me, book) in ops.iter().zip(books.iter_mut()) {
                    book.refresh(chain.clone(), now);
                    // A node files what the book offers it; the "queued" flag clears when its carrier lands.
                    let queued = in_flight.iter().any(|(_, accuser, _)| accuser == me);
                    if let Some((claim, _)) = book.next(me, now, queued) {
                        if down.get(&claim) == Some(me) {
                            book.settle(claim); // a node that is down files nothing
                            continue;
                        }
                        book.queued(claim, now);
                        per_node.entry(*me).or_default().push(now);
                        in_flight.push((claim, *me, now + 1));
                    }
                }
            }
            assert_eq!(accused.len(), claims.len(), "down_rank0={down_rank0}: every claim is accused");
            for (claim, accusers) in &accused {
                assert_eq!(accusers.len(), 1, "down_rank0={down_rank0}: claim {claim} accused once: {accusers:?}");
                if down_rank0 {
                    assert_ne!(Some(&accusers[0].0), down.get(claim), "the down node filed nothing");
                }
            }
            for (node, filed) in &per_node {
                assert!(filed.windows(2).all(|w| w[1] >= w[0] + PALW_OPERATOR_DA_MIN_GAP_DAA_V1), "{node:?}: the per-DAA cap");
                let share = filed.len() as f64 / claims.len() as f64;
                assert!((0.05..0.25).contains(&share), "down_rank0={down_rank0}: {node:?} carries {share:.3} of the claims");
            }
        }
    }

    /// **What this duty costs at testnet-12's claim rate** — printed, and the ADR's figures pinned:
    /// the refuted exposure a session burns on an honest claim (DA-6: 10% of the stage's reward base,
    /// the floor's 3,200.95 MSK commitment → 320.10), the carriers (one accusation + one answer a
    /// claim), the per-node filing rate, and the exposure held on a bond in steady state against A-6's
    /// free half of a 939,063.21 MSK genesis bond.
    #[test]
    fn the_expected_load_at_t12s_claim_rate() {
        use kaspa_consensus_core::palw_da_rcore_v1::palw_da_session_exposure_v1;
        const MSK: u128 = 100_000_000;
        let floor_commitment = 320_095 * MSK / 100; // 3,200.95 MSK (ADR-0152 §3.8's floor reservation)
        let floor_min_collateral = 13_000 * 100_000_000u64;
        let exposure = palw_da_session_exposure_v1(floor_commitment, floor_min_collateral);
        assert_eq!(exposure, 32_009_500_000, "10% of the floor's commitment: 320.095 MSK");
        let genesis_c = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI as u128;
        let held_span_daa: u128 = 120 + 3_000; // licence → Final (short challenge) → retirement
        let secs_per_daa = 120u128;
        let daa_per_day = 86_400 / secs_per_daa;
        for lambda_x100 in [25u128, 100, 150] {
            let accusations_per_day = lambda_x100 * daa_per_day / 100;
            let burned_per_day = accusations_per_day * exposure;
            let held_per_bond = lambda_x100 * held_span_daa * exposure / 100 / 8;
            let per_node_minutes = 8 * 100 * secs_per_daa / lambda_x100 / 60;
            println!(
                "λ = {:.2} external licensed claims/DAA: {} accusations + {} answers a day ({} carriers), one filing per node every \
                 ~{} min; burned {} MSK a day fleet-wide ({} per bond, {:.2}% of a genesis bond); held {} MSK per bond ({:.1}% of C, \
                 A-6's free half is 50%)",
                lambda_x100 as f64 / 100.0,
                accusations_per_day,
                accusations_per_day,
                2 * accusations_per_day,
                per_node_minutes,
                burned_per_day / MSK,
                burned_per_day / MSK / 8,
                (burned_per_day / 8) as f64 * 100.0 / genesis_c as f64,
                held_per_bond / MSK,
                held_per_bond as f64 * 100.0 / genesis_c as f64,
            );
            assert!(held_per_bond * 2 < genesis_c, "held exposure stays inside A-6's free half at λ = {lambda_x100}/100");
        }
        // The fleet's ceiling: one filing every two DAA a node.
        assert_eq!(8 * 100 / PALW_OPERATOR_DA_MIN_GAP_DAA_V1, 400, "4 accusations a DAA across eight nodes");
    }

    /// **Wired where it runs, with no flag** (source inspection): the tick calls the lane after the
    /// reporter filer and before the carriers, holds its book from the operator set the params name,
    /// and no CLI option reaches it.
    #[test]
    fn the_tick_wires_the_lane_before_the_carriers_and_behind_no_flag() {
        let panel = include_str!("palw_panel.rs");
        let tick = panel.find("self.operator_da_tick_v1(").expect("the tick calls the lane");
        let reporter = panel.find("// --- P2-8: the reporter's commit–reveal filer").expect("the reporter filer");
        let carriers = panel.find("// --- the collector + submitter's half ---").expect("the carriers");
        assert!(reporter < tick && tick < carriers, "after the reporter filer, before the priority lane carries the court queue");
        assert!(panel.contains(
            "palw_operator_da::PalwOperatorDaBookV1::new(palw_operator_da::palw_operator_bonds_v1(&self.consensus_config.params))"
        ));
        let args = include_str!("args.rs");
        assert!(!args.contains("operator-da") && !args.contains("operator_da"), "no opt-in or opt-out flag");
    }
}
