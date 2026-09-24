//! **ADR-0152 V-1…V-8, read side: what `getPalwVesting` (op 199) and claim row v3 report**
//! (phase2-plan §1.6, P2-10; testnet-12, R-core+).
//!
//! Past `palw_rcore_plus` a Final claim's reward is a row in `PalwChainStateV2::vesting`, not a
//! payout, so "was I paid?" stopped being a question the claim record could answer: the reward
//! waits out the conviction window on two clocks (V-4), latches, queues behind every row before it
//! in `(expiry_daa, claim_id)` order (V-7), moves at step 3d and is minted by the next block's
//! coinbase. Until this module an operator could see none of that — `misaka rewards` called a
//! vesting row "paid" and `misaka work` promised it spendable 3,147+ DAA early (phase2-plan F10).
//!
//! **Every answer is the vesting rules' own function, never a restatement** (one rule, one place):
//! a row's maturity is [`palw_vesting_row_maturity_v1`], the halt [`palw_chain_vesting_halted_v1`],
//! "what moves next block" [`palw_vesting_next_block_plan_v1`] — the committed-state entry point,
//! which replays the next block's 1b drain and latch; the planner itself is never run on a
//! committed state here — a row's place in V-7's order [`palw_vesting_mint_positions_v1`], and B-3's
//! hold on a payee's collateral [`palw_bond_is_payee_of_unmatured_row_v1`]. Every one of them is a
//! function of `(params, state)` and the raw second-clock depth (I-8), so the caller passes the
//! committed tip, the DAA the next block will be folded at (the virtual's) and the raw depth at that
//! DAA — the three facts the fold's step 3d will read.
//!
//! **Nothing on the block path reads this module**, and it writes nothing. It is bounded: a page of
//! rows (at most [`PALW_VESTING_READ_ROW_CAP_V1`]) gets its positions in one walk of V-7's order,
//! and the chain-wide totals are one walk of the rows with no hashing (≤ 9,121 rows at the T-3b
//! bound, phase2-plan §5.7).
//!
//! **What it cannot report, and says so.** A row that BURNED leaves no row and no mark on a claim a
//! reader can tell from a pre-Final void (a convicted Final is `Voided { CourtFraud }` like any other
//! court void), so the claim row never says `burned`: burns are the `burned` counter chain-wide, and
//! per payee they need the delta notes (`PalwVestingNoteV1::Burned`), which is P2-10b's optional
//! per-payee index and not this read.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_hashes::Hash64;

use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimStateV2, PalwStateParamsV2, palw_bond_is_payee_of_unmatured_row_v1,
    palw_second_clock_depth_v1,
};
use crate::palw_vesting_v1::{
    PalwVestingCountersV1, PalwVestingLegKindV1, PalwVestingLegV1, PalwVestingMaturityV1, PalwVestingMintPlanV1, PalwVestingRowV1,
    PalwVestingSourceV1, palw_chain_vesting_halted_v1, palw_vesting_mint_positions_v1, palw_vesting_next_block_plan_v1,
    palw_vesting_row_maturity_v1,
};

/// The most rows one read returns (phase2-plan §5.7: a payee of a busy class can be named in
/// thousands of live rows at the T-3b bound; the rest is paged with `after`).
pub const PALW_VESTING_READ_ROW_CAP_V1: usize = 500;

/// **Whose rewards a read is about.** A bond names its rows (as producer or credited seat, through
/// the payee index), its pending reporter rewards (as their current best reveal) and its awarded
/// ones (by the bond's payout payload, which S-7 fixed at conviction). A payload names every row
/// leg and reporter reward paid to it, whichever bond earned it — the payout address's view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVestingPayeeV1 {
    Bond(PalwBondKeyV2),
    Payload(Hash64),
}

/// What a read asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVestingQueryV1 {
    /// The chain-wide totals, and the head of V-7's order as the rows.
    Chain,
    /// One payee's rows and reporter rewards.
    Payee(PalwVestingPayeeV1),
    /// One claim's row.
    Claim(Hash64),
}

/// **The earliest DAA at which a row can move** — a LOWER bound, exact only when the next block's
/// plan takes it (`estimated == false`, and then `daa` is the next block's). The move's block is
/// `daa`; its mint is the block after; its output is spendable `coinbase_spend_maturity` after that
/// (Decision A, which V-4 never reads). The queue is not in the bound: the keys ahead of a row are an
/// upper bound on what they spend (rows sharing a seat payee share a key), so they cannot bound the
/// wait from below; the read reports the row's position beside it instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwVestingEtaV1 {
    pub daa: u64,
    pub estimated: bool,
}

/// **One row as a read reports it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVestingRowReadV1 {
    pub row: PalwVestingRowV1,
    /// V-4 at the next block's DAA and raw depth ([`palw_vesting_row_maturity_v1`]).
    pub maturity: PalwVestingMaturityV1,
    /// The legs the query is about: the payee's own (a producer that also sat is named twice), or
    /// every leg for a claim or chain read.
    pub legs: Vec<PalwVestingLegV1>,
    /// `(moves ahead, keys ahead)` in V-7's order ([`palw_vesting_mint_positions_v1`]).
    pub position: Option<(usize, usize)>,
    /// The next block's step 3d moves it ([`palw_vesting_next_block_plan_v1`]).
    pub in_next_block: bool,
    pub eta: PalwVestingEtaV1,
}

impl PalwVestingRowReadV1 {
    /// Σ of [`Self::legs`].
    pub fn legs_sompi(&self) -> u64 {
        self.legs.iter().fold(0u64, |sum, leg| sum.saturating_add(leg.amount))
    }
}

/// Where a reporter reward stands (S-7): `Pending` while its reveal window is open — the payee is
/// its CURRENT best reveal, which an earlier reveal can still displace — and `Awarded` once the
/// window closed and step 3d has yet to move it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReporterRewardStageV1 {
    Pending,
    Awarded,
}

/// **One reporter reward as a read reports it.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwReporterRewardReadV1 {
    pub offence_key: Hash64,
    pub stage: PalwReporterRewardStageV1,
    /// The reporter bond: the best reveal's while pending; `None` once awarded (`reporter_rewards`
    /// stores the payload S-7 fixed, not the bond — the `ReporterAwarded` note names it).
    pub reporter: Option<PalwBondKeyV2>,
    /// Where it pays: the best reveal's payload while pending, the fixed one once awarded; zero for
    /// a pending reward nobody has revealed for yet.
    pub payload: Hash64,
    pub amount: u64,
    /// The reveal window's end, while pending.
    pub reveal_until: Option<u64>,
    /// The next block's step 3d moves it (awarded rewards go first, V-7).
    pub in_next_block: bool,
}

/// One door's rows (the "p" ADR-0152 §8.4 O-2 asks the observation program to measure: which licence
/// door made the Finals that are vesting now).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwVestingDoorCountV1 {
    pub rows: usize,
    pub latched_rows: usize,
    pub sompi: u128,
}

/// **Everything one `getPalwVesting` read answers.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVestingReadV1 {
    /// The DAA the committed state was folded at.
    pub tip_daa: u64,
    /// The DAA the next block folds at — every clock and the plan are asked there.
    pub next_daa: u64,
    /// `Params::palw_rcore_plus` is in force at `next_daa`. Below it no row exists and nothing vests.
    pub rcore_plus_active: bool,
    /// V-4(b) at `next_daa` ([`palw_chain_vesting_halted_v1`]).
    pub halted: bool,
    /// The raw second-clock depth (`None`: no second clock configured — not a halt, F11), and the
    /// depth after the liveness escape.
    pub raw_depth: Option<u64>,
    pub escaped_depth: Option<u64>,
    /// `settled_attempt_finals`: the second clock's count now.
    pub settled_now: u64,
    pub counters: PalwVestingCountersV1,
    pub live_rows: usize,
    pub latched_rows: usize,
    pub live_sompi: u128,
    pub latched_sompi: u128,
    /// Rows past the first unlatched one in V-7's order that are latched (stop-never-skip holds
    /// them behind the head).
    pub latched_behind_head: usize,
    pub reporter_pending_rows: usize,
    pub reporter_awarded_rows: usize,
    /// What the next block's step 3d moves ([`palw_vesting_next_block_plan_v1`]).
    pub next_block: PalwVestingMintPlanV1,
    /// Live rows by the door of their Final-basis licence set, keyed by
    /// [`palw_licence_door_name_v1`].
    pub licence_histogram: BTreeMap<&'static str, PalwVestingDoorCountV1>,
    /// The query's bond is in the registry (always `true` for other queries).
    pub bond_known: bool,
    /// **B-3**: the bond is payee of a row still unmatured by V-4(a), so its collateral is locked
    /// ([`palw_bond_is_payee_of_unmatured_row_v1`]). `false` for other queries.
    pub payee_holds_collateral: bool,
    /// The page of rows, in V-7's order.
    pub rows: Vec<PalwVestingRowReadV1>,
    /// Every row the query matched, before the page.
    pub rows_total: usize,
    /// The cursor after the page's last row, when rows were left out.
    pub next_after: Option<(u64, Hash64)>,
    /// Σ of the query's legs over EVERY matched row (not just the page): unlatched, then latched.
    pub maturing_sompi: u128,
    pub latched_sompi_of_query: u128,
    /// The payee's reporter rewards (empty for other queries).
    pub reporter_rewards: Vec<PalwReporterRewardReadV1>,
}

/// The name a licence door prints under (the `getPalwClaims` spelling of the doors).
pub fn palw_licence_door_name_v1(door: &PalwLicenceDoorTagV1) -> &'static str {
    match door {
        PalwLicenceDoorTagV1::Quorum => "quorum",
        PalwLicenceDoorTagV1::Coverage => "coverage",
        PalwLicenceDoorTagV1::Optimistic => "optimistic",
        PalwLicenceDoorTagV1::ShardPart { .. } => "shard_part",
    }
}

/// **A reader over one committed state**: the next block's plan, computed once, and the rows' reads
/// against it. Holds the three facts the next fold's step 3d reads (I-8).
pub struct PalwVestingReaderV1<'s> {
    state: &'s PalwChainStateV2,
    params: &'s PalwStateParamsV2,
    next_daa: u64,
    raw_depth: Option<u64>,
    plan: PalwVestingMintPlanV1,
    planned_rows: BTreeSet<Hash64>,
    planned_reporters: BTreeSet<Hash64>,
}

impl<'s> PalwVestingReaderV1<'s> {
    /// `state` is the committed tip; `next_daa` the DAA the next block folds at; `raw_depth` the raw
    /// second-clock depth the next block's extras will carry (`None` below `palw_audit_2026_09_23`).
    pub fn new(state: &'s PalwChainStateV2, params: &'s PalwStateParamsV2, next_daa: u64, raw_depth: Option<u64>) -> Self {
        let plan = palw_vesting_next_block_plan_v1(state, params, next_daa, raw_depth);
        let mut planned_rows = BTreeSet::new();
        let mut planned_reporters = BTreeSet::new();
        for next in &plan.moves {
            match &next.source {
                PalwVestingSourceV1::Row { claim_id } => planned_rows.insert(*claim_id),
                PalwVestingSourceV1::Reporter { offence_id } => planned_reporters.insert(*offence_id),
            };
        }
        Self { state, params, next_daa, raw_depth, plan, planned_rows, planned_reporters }
    }

    pub fn plan(&self) -> &PalwVestingMintPlanV1 {
        &self.plan
    }

    pub fn next_daa(&self) -> u64 {
        self.next_daa
    }

    /// V-4 for `row` at the next block ([`palw_vesting_row_maturity_v1`]).
    pub fn maturity(&self, row: &PalwVestingRowV1) -> PalwVestingMaturityV1 {
        palw_vesting_row_maturity_v1(self.state, self.params, row, self.next_daa, self.raw_depth)
    }

    /// **[`PalwVestingEtaV1`] for a row**, from its maturity and whether the plan takes it: the
    /// next block's DAA when it does; otherwise the next block after it, or the DAA clock's expiry
    /// when that is later. A row the second clock or a halt holds past its expiry has no earlier
    /// date than the next block, and its per-obligation release (`second_clock_bound_daa`) is an
    /// upper bound the maturity carries beside it.
    pub fn eta(&self, row: &PalwVestingRowV1, maturity: &PalwVestingMaturityV1) -> PalwVestingEtaV1 {
        if self.planned_rows.contains(&row.claim_id) {
            return PalwVestingEtaV1 { daa: self.next_daa, estimated: false };
        }
        let after_next = self.next_daa.saturating_add(1);
        let clock = if maturity.mature_now || maturity.daa_clock_met { after_next } else { row.expiry_daa.max(after_next) };
        PalwVestingEtaV1 { daa: clock, estimated: true }
    }

    /// Reads for `rows` (any order in; V-7's order is the caller's), with the legs `legs_of`
    /// selects, the positions of all of them from ONE walk.
    pub fn read_rows<'r>(
        &self,
        rows: &[&'r PalwVestingRowV1],
        legs_of: impl Fn(&PalwVestingRowV1) -> Vec<PalwVestingLegV1>,
    ) -> Vec<PalwVestingRowReadV1> {
        let wanted: BTreeSet<Hash64> = rows.iter().map(|row| row.claim_id).collect();
        let positions = palw_vesting_mint_positions_v1(self.state, &wanted);
        rows.iter()
            .map(|row| {
                let maturity = self.maturity(row);
                let eta = self.eta(row, &maturity);
                PalwVestingRowReadV1 {
                    row: (*row).clone(),
                    maturity,
                    legs: legs_of(row),
                    position: positions.get(&row.claim_id).copied(),
                    in_next_block: self.planned_rows.contains(&row.claim_id),
                    eta,
                }
            })
            .collect()
    }

    /// **Where a claim's vested reward stands, from the claim record and the table** — claim row
    /// v3's `vesting_stage`. `Some(row read)` while the row lives. `Moved` for a Final that vested
    /// (past `palw_rcore_plus` at its Final, with an escrow) and has no row left: a conviction
    /// reverses the Final it burns (`reverse_convicted_final`), so a Final with no row MOVED. `None`
    /// for everything else: not Final, never vested (below the fence, no escrow, a free-prompt
    /// claim), or voided — including a convicted Final, which the claim record cannot tell from a
    /// void before Final (the module doc).
    pub fn claim_stage(&self, claim_id: &Hash64, claim: Option<&PalwClaimStateV2>) -> Option<PalwClaimVestingV1> {
        self.claim_stages(&[(*claim_id, claim)]).remove(claim_id)
    }

    /// [`Self::claim_stage`] for a page of claims, the live rows' positions from ONE walk (claim
    /// rows come 500 at a time; one walk per row would be 500 walks of the table).
    pub fn claim_stages(&self, claims: &[(Hash64, Option<&PalwClaimStateV2>)]) -> BTreeMap<Hash64, PalwClaimVestingV1> {
        let rows: Vec<&PalwVestingRowV1> = claims.iter().filter_map(|(id, _)| self.state.vesting_row(id)).collect();
        let mut out: BTreeMap<Hash64, PalwClaimVestingV1> = self
            .read_rows(&rows, |row| row.legs().collect())
            .into_iter()
            .map(|read| {
                let stage =
                    if read.row.matured_at.is_some() { PalwClaimVestingStageV1::Latched } else { PalwClaimVestingStageV1::Maturing };
                (read.row.claim_id, PalwClaimVestingV1 { stage, read: Some(read) })
            })
            .collect();
        for (claim_id, claim) in claims {
            if out.contains_key(claim_id) {
                continue;
            }
            if let Some(PalwClaimStateV2 { phase: PalwClaimPhaseV2::Final { final_daa }, escrowed_reward, .. }) = claim
                && *escrowed_reward > 0
                && self.params.rcore_plus_active_at(*final_daa)
            {
                out.insert(*claim_id, PalwClaimVestingV1 { stage: PalwClaimVestingStageV1::Moved, read: None });
            }
        }
        out
    }
}

/// Claim row v3's stage of a claim's vested reward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimVestingStageV1 {
    /// The row lives and has not latched.
    Maturing,
    /// Latched (V-4 held once), waiting its turn in V-7's order.
    Latched,
    /// Moved into the queue: minted by the block after the move.
    Moved,
}

impl PalwClaimVestingStageV1 {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Maturing => "maturing",
            Self::Latched => "latched",
            Self::Moved => "moved",
        }
    }
}

/// One claim's vesting, as claim row v3 reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimVestingV1 {
    pub stage: PalwClaimVestingStageV1,
    /// The row's read while it lives.
    pub read: Option<PalwVestingRowReadV1>,
}

/// The legs of `row` that pay `payee`: a bond's by `payee_bond`, a payload's by `payload` (the
/// reserve pays nobody).
pub fn palw_vesting_legs_paying_v1(row: &PalwVestingRowV1, payee: &PalwVestingPayeeV1) -> Vec<PalwVestingLegV1> {
    row.legs()
        .filter(|leg| match payee {
            PalwVestingPayeeV1::Bond(bond) => leg.payee_bond == Some(*bond),
            PalwVestingPayeeV1::Payload(payload) => leg.kind != PalwVestingLegKindV1::Reserve && leg.payload == *payload,
        })
        .collect()
}

/// **The `getPalwVesting` read** (op 199; phase2-plan §1.6). `limit` bounds the page (0 = the cap,
/// and never above it); `after` resumes past a `(expiry_daa, claim_id)` cursor. Rows come in V-7's
/// order. Pure in its arguments; the processor passes its cached tip.
pub fn palw_vesting_read_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    next_daa: u64,
    raw_depth: Option<u64>,
    query: &PalwVestingQueryV1,
    limit: usize,
    after: Option<(u64, Hash64)>,
) -> PalwVestingReadV1 {
    let reader = PalwVestingReaderV1::new(state, params, next_daa, raw_depth);
    let window_court = params.window_court();
    let limit = match limit {
        0 => PALW_VESTING_READ_ROW_CAP_V1,
        n => n.min(PALW_VESTING_READ_ROW_CAP_V1),
    };

    // The chain-wide totals: one walk, no hashing.
    let mut live_rows = 0usize;
    let mut latched_rows = 0usize;
    let mut live_sompi = 0u128;
    let mut latched_sompi = 0u128;
    let mut latched_behind_head = 0usize;
    let mut head_seen = false;
    let mut licence_histogram: BTreeMap<&'static str, PalwVestingDoorCountV1> = BTreeMap::new();
    for row in state.vesting_iter_by_expiry() {
        let total = row.total_sompi_u128();
        live_rows += 1;
        live_sompi += total;
        let door = licence_histogram.entry(palw_licence_door_name_v1(&row.licence_door)).or_default();
        door.rows += 1;
        door.sompi += total;
        if row.matured_at.is_some() {
            latched_rows += 1;
            latched_sompi += total;
            door.latched_rows += 1;
            if head_seen {
                latched_behind_head += 1;
            }
        } else {
            head_seen = true;
        }
    }

    // The rows the query names, in V-7's order, with the legs it is about.
    let payee = match query {
        PalwVestingQueryV1::Payee(payee) => Some(*payee),
        _ => None,
    };
    let legs_of = |row: &PalwVestingRowV1| match &payee {
        Some(payee) => palw_vesting_legs_paying_v1(row, payee),
        None => row.legs().collect(),
    };
    let matched: Vec<&PalwVestingRowV1> = match query {
        PalwVestingQueryV1::Chain => state.vesting_iter_by_expiry().collect(),
        PalwVestingQueryV1::Claim(claim_id) => state.vesting_row(claim_id).into_iter().collect(),
        PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond)) => {
            // The payee index holds a bond once per row (a set), unlatched first; V-7's order is
            // `(expiry_daa, claim_id)`.
            let mut rows: Vec<&PalwVestingRowV1> = state.vesting_rows_of_payee(bond).collect();
            rows.sort_by_key(|row| (row.expiry_daa, row.claim_id));
            rows.dedup_by_key(|row| row.claim_id);
            rows
        }
        PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Payload(payload)) => state
            .vesting_iter_by_expiry()
            .filter(|row| row.producer.payload == *payload || row.seats.iter().any(|(_, payout)| payout.payload == *payload))
            .collect(),
    };
    let mut maturing_sompi = 0u128;
    let mut latched_sompi_of_query = 0u128;
    for row in &matched {
        let sompi: u128 = legs_of(row).iter().map(|leg| leg.amount as u128).sum();
        if row.matured_at.is_some() {
            latched_sompi_of_query += sompi;
        } else {
            maturing_sompi += sompi;
        }
    }
    let rows_total = matched.len();
    let page: Vec<&PalwVestingRowV1> =
        matched.into_iter().filter(|row| after.is_none_or(|cursor| (row.expiry_daa, row.claim_id) > cursor)).take(limit + 1).collect();
    let next_after = (page.len() > limit).then(|| (page[limit - 1].expiry_daa, page[limit - 1].claim_id));
    let page = &page[..page.len().min(limit)];
    let rows = reader.read_rows(page, legs_of);

    // The payee's reporter rewards.
    let payout_payload = match payee {
        Some(PalwVestingPayeeV1::Bond(bond)) => state.bond(&bond).map(|b| b.payout_payload),
        Some(PalwVestingPayeeV1::Payload(payload)) => Some(payload),
        None => None,
    };
    let mut reporter_rewards = Vec::new();
    if let Some(payee) = payee {
        for (offence_key, pending) in state.reward_pending_iter() {
            let Some(best) = pending.best else { continue };
            let ours = match payee {
                PalwVestingPayeeV1::Bond(bond) => best.reporter == bond,
                PalwVestingPayeeV1::Payload(payload) => best.payload == payload,
            };
            if ours {
                reporter_rewards.push(PalwReporterRewardReadV1 {
                    offence_key: *offence_key,
                    stage: PalwReporterRewardStageV1::Pending,
                    reporter: Some(best.reporter),
                    payload: best.payload,
                    amount: pending.amount,
                    reveal_until: Some(pending.reveal_until),
                    in_next_block: false,
                });
            }
        }
        if let Some(payload) = payout_payload {
            for (offence_key, payout) in state.reporter_rewards_iter().filter(|(_, payout)| payout.payload == payload) {
                reporter_rewards.push(PalwReporterRewardReadV1 {
                    offence_key: *offence_key,
                    stage: PalwReporterRewardStageV1::Awarded,
                    reporter: None,
                    payload: payout.payload,
                    amount: payout.amount,
                    reveal_until: None,
                    in_next_block: reader.planned_reporters.contains(offence_key),
                });
            }
        }
    }

    let (bond_known, payee_holds_collateral) = match payee {
        Some(PalwVestingPayeeV1::Bond(bond)) => {
            (state.bond(&bond).is_some(), palw_bond_is_payee_of_unmatured_row_v1(state, params, &bond, next_daa, raw_depth))
        }
        _ => (true, false),
    };

    PalwVestingReadV1 {
        tip_daa: state.last_point().map(|point| point.daa_score).unwrap_or(0),
        next_daa,
        rcore_plus_active: params.rcore_plus_active_at(next_daa),
        halted: palw_chain_vesting_halted_v1(state, raw_depth, next_daa, window_court),
        raw_depth,
        escaped_depth: palw_second_clock_depth_v1(raw_depth, state.recent_anchor_daas(), next_daa, window_court),
        settled_now: state.settled_attempt_finals(),
        counters: state.vesting_counters(),
        live_rows,
        latched_rows,
        live_sompi,
        latched_sompi,
        latched_behind_head,
        reporter_pending_rows: state.reward_pending_iter().count(),
        reporter_awarded_rows: state.reporter_rewards_iter().count(),
        next_block: reader.plan.clone(),
        licence_histogram,
        bond_known,
        payee_holds_collateral,
        rows,
        rows_total,
        next_after,
        maturing_sompi,
        latched_sompi_of_query,
        reporter_rewards,
    }
}

/// **The rows a bond is named in whose claim has retired** (claim row v3's `vesting_only_rows`):
/// a claim retires `claim_retirement` after its Final while its row lives until it moves (up to
/// `F + 9,000` on testnet-12), so from retirement on the row is the only record of the reward.
/// `executor` asks the rows it produced; otherwise the rows it was credited on as a seat. Newest
/// Final first, as `palw_claim_rows_v1` orders; `limit` bounds it (0 = no bound) and the bool says
/// whether any were left out.
pub fn palw_vesting_only_rows_v1(
    reader: &PalwVestingReaderV1<'_>,
    bond: &PalwBondKeyV2,
    executor: bool,
    limit: usize,
) -> (Vec<PalwVestingRowReadV1>, bool) {
    let mut rows: Vec<&PalwVestingRowV1> = reader
        .state
        .vesting_rows_of_payee(bond)
        .filter(|row| reader.state.claim(&row.claim_id).is_none())
        .filter(|row| if executor { row.producer_bond == *bond } else { row.seats.iter().any(|(seat, _)| seat == bond) })
        .collect();
    rows.sort_by(|a, b| b.final_daa.cmp(&a.final_daa).then_with(|| a.claim_id.cmp(&b.claim_id)));
    rows.dedup_by_key(|row| row.claim_id);
    let truncated = limit > 0 && rows.len() > limit;
    if truncated {
        rows.truncate(limit);
    }
    (reader.read_rows(&rows, |row| row.legs().collect()), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwBondStateV2, PalwBondStatusV2, PalwDeltaEntryV2, PalwPayoutV2, PalwPendingRewardV1,
        PalwRewardWinnerV1, PalwStateDeltaV2, apply_delta_v2,
    };
    use crate::palw_vesting_v1::{PalwVestingStopV1, palw_vesting_mint_position_v1};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond_key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB000 + n), 0))
    }

    fn payload(n: u64) -> Hash64 {
        h(0x9A00 + n)
    }

    /// The window the fixtures' params carry (`window_court`), so a row's expiry is `final + 20`.
    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 800, 0)
            .unwrap()
            .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
    }

    fn row(claim: u64, producer: u64, seats: &[u64], expiry_daa: u64, matured_at: Option<u64>) -> PalwVestingRowV1 {
        PalwVestingRowV1 {
            claim_id: h(claim),
            producer_bond: bond_key(producer),
            class_id: h(1),
            execution_root: h(0xE0),
            artifact_root: h(11),
            job_identity: Hash64::default(),
            free_prompt: false,
            trace_root: Hash64::default(),
            segment_count: 0,
            licence_door: if seats.len() > 2 { PalwLicenceDoorTagV1::Coverage } else { PalwLicenceDoorTagV1::Quorum },
            basis_k: 2,
            escrowed_reward: 1_000,
            buyback_bound: 0,
            producer: PalwPayoutV2 { payload: payload(producer), amount: 500 },
            seats: seats.iter().map(|s| (bond_key(*s), PalwPayoutV2 { payload: payload(*s), amount: 100 })).collect(),
            reserve: 500 - 100 * seats.len() as u64,
            final_daa: expiry_daa.saturating_sub(20),
            expiry_daa,
            settled_at_final: 0,
            matured_at,
        }
    }

    fn bond(n: u64) -> PalwBondStateV2 {
        PalwBondStateV2 {
            pubkey: vec![n as u8; 4],
            operator_id: h(0x0B00 + n),
            collateral: 1_000_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 1,
            payout_payload: payload(n),
            capable_classes: BTreeSet::new(),
        }
    }

    /// `rows`, two bonds, an awarded reporter reward to bond 7's payload and a pending one whose
    /// best reveal is bond 7 — installed through one delta, so the derived indexes are the delta
    /// path's.
    fn seeded(rows: &[PalwVestingRowV1]) -> PalwChainStateV2 {
        let mut entries: Vec<PalwDeltaEntryV2> =
            [2u64, 3, 4, 7].iter().map(|n| PalwDeltaEntryV2::Bond { key: bond_key(*n), old: None, new: Some(bond(*n)) }).collect();
        entries.extend(rows.iter().map(|r| PalwDeltaEntryV2::Vesting { key: r.claim_id, old: None, new: Some(r.clone()) }));
        let created: u128 = rows.iter().map(PalwVestingRowV1::total_sompi_u128).sum();
        entries.push(PalwDeltaEntryV2::VestingCounters {
            old: PalwVestingCountersV1::default(),
            new: PalwVestingCountersV1 { created, ..Default::default() },
        });
        entries.push(PalwDeltaEntryV2::ReporterReward {
            key: h(0xAA01),
            old: None,
            new: Some(PalwPayoutV2 { payload: payload(7), amount: 33 }),
        });
        entries.push(PalwDeltaEntryV2::RewardPending {
            key: h(0xAA02),
            old: None,
            new: Some(PalwPendingRewardV1 {
                amount: 44,
                reveal_until: 90,
                evidence_id: h(0xEE),
                best: Some(PalwRewardWinnerV1 { committed_daa: 5, commitment: h(0xC0), reporter: bond_key(7), payload: payload(7) }),
            }),
        });
        let point = PalwBlockContextV2 { block: h(0xB1), daa_score: 10, blue_score: 10, subsidy: 0 };
        apply_delta_v2(&PalwChainStateV2::genesis(), &PalwStateDeltaV2 { point, entries }, &params()).unwrap()
    }

    /// **T51 (read half): per producer, per seat payee, per payout address, per claim** — each names
    /// exactly the rows and legs it pays, in V-7's order, and the totals are the table's.
    #[test]
    fn t51_the_read_names_each_payees_rows_and_legs() {
        let rows = [
            row(0x10, 2, &[3, 4], 50, Some(40)),
            row(0x11, 3, &[2, 4], 60, None),
            row(0x12, 2, &[3, 4, 7], 70, None),
            row(0x13, 4, &[], 80, None),
        ];
        let state = seeded(&rows);
        let p = params();
        // Producer and seat: bond 2 produced 0x10 and 0x12 and sat on 0x11.
        let read = palw_vesting_read_v1(&state, &p, 55, None, &PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond_key(2))), 0, None);
        assert_eq!(read.rows.iter().map(|r| r.row.claim_id).collect::<Vec<_>>(), vec![h(0x10), h(0x11), h(0x12)]);
        assert_eq!(read.rows[0].legs.iter().map(|l| l.kind).collect::<Vec<_>>(), vec![PalwVestingLegKindV1::Producer]);
        assert_eq!(read.rows[1].legs.iter().map(|l| (l.kind, l.amount)).collect::<Vec<_>>(), vec![(PalwVestingLegKindV1::Seat, 100)]);
        assert_eq!((read.latched_sompi_of_query, read.maturing_sompi), (500, 600), "latched 0x10's 500; 0x11's 100 + 0x12's 500");
        assert!(read.bond_known);
        // The address view of bond 7's payload: its seat leg on 0x12 and both reporter rewards.
        let by_address =
            palw_vesting_read_v1(&state, &p, 55, None, &PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Payload(payload(7))), 0, None);
        assert_eq!(by_address.rows.len(), 1);
        assert_eq!(by_address.rows[0].legs.iter().map(|l| l.amount).collect::<Vec<_>>(), vec![100]);
        let stages: Vec<_> = by_address.reporter_rewards.iter().map(|r| (r.stage, r.amount)).collect();
        assert_eq!(stages, vec![(PalwReporterRewardStageV1::Pending, 44), (PalwReporterRewardStageV1::Awarded, 33)]);
        // The bond view of bond 7 finds the same rewards, pending by its reveal, awarded by its payload.
        let by_bond = palw_vesting_read_v1(&state, &p, 55, None, &PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond_key(7))), 0, None);
        assert_eq!(by_bond.reporter_rewards.len(), 2);
        // A claim read: every leg, reserve included.
        let one = palw_vesting_read_v1(&state, &p, 55, None, &PalwVestingQueryV1::Claim(h(0x12)), 0, None);
        assert_eq!(one.rows.len(), 1);
        assert_eq!(one.rows[0].legs.len(), 1 + 3 + 1);
        assert_eq!(one.rows[0].legs_sompi(), 1_000);
        // The totals are the table's.
        assert_eq!((one.live_rows, one.latched_rows, one.live_sompi, one.latched_sompi), (4, 1, 4_000, 1_000));
        assert_eq!(one.counters.created, 4_000);
        assert_eq!((one.reporter_awarded_rows, one.reporter_pending_rows), (1, 1));
        assert_eq!(one.licence_histogram["coverage"].rows, 1);
        assert_eq!(one.licence_histogram["quorum"].rows, 3);
    }

    /// **T51: the ETA lower bound matches the planner** — the next block's DAA exactly for the rows
    /// the plan moves, and later than it for every other row; positions are the single-row
    /// function's, and B-3's hold is the fold's predicate.
    #[test]
    fn t51_the_eta_is_the_planners_answer_where_it_has_one() {
        let rows = [
            row(0x20, 2, &[3], 50, Some(45)),
            row(0x21, 3, &[2], 55, None),
            row(0x22, 2, &[3, 4], 90, None),
        ];
        let state = seeded(&rows);
        let p = params();
        let next = 60;
        let read = palw_vesting_read_v1(&state, &p, next, None, &PalwVestingQueryV1::Chain, 0, None);
        let plan = palw_vesting_next_block_plan_v1(&state, &p, next, None);
        assert_eq!(read.next_block, plan, "the read carries the committed-state entry point's plan, nothing recomputed");
        assert_eq!(plan.stopped, PalwVestingStopV1::NotLatched, "0x22's DAA clock runs to 90");
        for r in &read.rows {
            let moves = plan.moves.iter().any(|m| m.source == PalwVestingSourceV1::Row { claim_id: r.row.claim_id });
            assert_eq!(r.in_next_block, moves);
            if moves {
                assert_eq!(r.eta, PalwVestingEtaV1 { daa: next, estimated: false });
            } else {
                assert!(r.eta.estimated && r.eta.daa > next);
            }
            assert_eq!(r.position, palw_vesting_mint_position_v1(&state, &r.row.claim_id));
        }
        // 0x20 (latched) and 0x21 (its DAA clock ran at 55) move next block; the reporter goes first.
        assert_eq!(plan.moves.len(), 3);
        assert_eq!(read.rows[2].eta.daa, 90, "a row whose DAA clock runs is dated by its expiry");
        // B-3: bond 2 is payee of 0x22, still unmatured by V-4(a) — the fold's own predicate.
        let bond2 = palw_vesting_read_v1(&state, &p, next, None, &PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond_key(2))), 0, None);
        assert!(bond2.payee_holds_collateral);
        assert_eq!(bond2.payee_holds_collateral, palw_bond_is_payee_of_unmatured_row_v1(&state, &p, &bond_key(2), next, None));
        let late = palw_vesting_read_v1(&state, &p, 95, None, &PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond_key(2))), 0, None);
        assert!(!late.payee_holds_collateral, "past every expiry with no second clock, nothing holds it");
    }

    /// **T51: `halted` flips exactly with `palw_chain_vesting_halted_v1`** — no second clock
    /// configured is not a halt (F11); a configured depth with no anchor settled for `2 × window_court`
    /// is, and then the plan moves no row.
    #[test]
    fn t51_halted_is_the_chains_halt_predicate() {
        let rows = [row(0x30, 2, &[], 50, None)];
        let state = seeded(&rows);
        let p = params();
        // `window_court` is 500: the escape needs 1,000 DAA with no anchor settled.
        for (raw, daa) in [(None, 60), (Some(3), 60), (Some(3), 1_200), (None, 1_200)] {
            let read = palw_vesting_read_v1(&state, &p, daa, raw, &PalwVestingQueryV1::Chain, 0, None);
            assert_eq!(read.halted, palw_chain_vesting_halted_v1(&state, raw, daa, p.window_court()), "raw {raw:?} at {daa}");
            if read.halted {
                assert!(read.rows.iter().all(|r| !r.in_next_block && !r.maturity.mature_now));
            }
        }
        assert!(!palw_vesting_read_v1(&state, &p, 60, None, &PalwVestingQueryV1::Chain, 0, None).halted, "F11");
        assert!(!palw_vesting_read_v1(&state, &p, 60, Some(3), &PalwVestingQueryV1::Chain, 0, None).halted, "the second clock holds; no halt");
        assert!(palw_vesting_read_v1(&state, &p, 1_200, Some(3), &PalwVestingQueryV1::Chain, 0, None).halted, "no anchor ever settled");
    }

    /// Paging: `limit` rows, a cursor after the last, and the next page resumes there; the sums
    /// cover every matched row, not only the page.
    #[test]
    fn the_read_pages_in_v7_order() {
        let rows: Vec<PalwVestingRowV1> = (0..5).map(|i| row(0x40 + i, 2, &[], 100 + i, None)).collect();
        let state = seeded(&rows);
        let p = params();
        let q = PalwVestingQueryV1::Payee(PalwVestingPayeeV1::Bond(bond_key(2)));
        let first = palw_vesting_read_v1(&state, &p, 10, None, &q, 2, None);
        assert_eq!(first.rows.iter().map(|r| r.row.claim_id).collect::<Vec<_>>(), vec![h(0x40), h(0x41)]);
        assert_eq!(first.next_after, Some((101, h(0x41))));
        assert_eq!((first.rows_total, first.maturing_sompi), (5, 2_500));
        let second = palw_vesting_read_v1(&state, &p, 10, None, &q, 2, first.next_after);
        assert_eq!(second.rows.iter().map(|r| r.row.claim_id).collect::<Vec<_>>(), vec![h(0x42), h(0x43)]);
        let last = palw_vesting_read_v1(&state, &p, 10, None, &q, 2, second.next_after);
        assert_eq!((last.rows.len(), last.next_after), (1, None));
    }

    /// The claim stage: a live row is maturing or latched, a Final that vested and has no row
    /// moved, and a claim with no row that never vested has no stage.
    #[test]
    fn the_claim_stage_reads_the_row_then_the_claim() {
        let state = seeded(&[row(0x50, 2, &[], 50, None), row(0x51, 2, &[], 60, Some(58))]);
        let p = params();
        let reader = PalwVestingReaderV1::new(&state, &p, 10, None);
        assert_eq!(reader.claim_stage(&h(0x50), None).map(|s| s.stage), Some(PalwClaimVestingStageV1::Maturing));
        assert_eq!(reader.claim_stage(&h(0x51), None).map(|s| s.stage), Some(PalwClaimVestingStageV1::Latched));
        assert_eq!(reader.claim_stage(&h(0x52), None), None, "no row and no claim record");
    }
}
