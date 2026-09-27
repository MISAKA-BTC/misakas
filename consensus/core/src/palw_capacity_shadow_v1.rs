//! **ADR-0160 §6.3 / §7.5 — shadow accounting: what the capacity formulas would say about the live
//! chain, next to what the chain says today.** Node-only; no consensus rule reads it.
//!
//! [`palw_capacity_shadow_v1`] takes one committed PALW state and a list of ramp steps
//! ([`PalwCapacityStepV1`]) and reports, as if every live claim had been accepted past ADR-0160's
//! fences at that step:
//!
//! * **per claim** — the raw and the staged weight (§4.2), today's weight reservation `w` and the
//!   capped one `min(w, R_budget − held)` (§4.1), today's commitment and the new one per step
//!   (`m_c + reserved`, E-3's release, E-4's void hold);
//! * **per bond** — collateral, `W_cap`, `X_b`, `min(X_b, W_cap)`, the A-1 commitment today and per
//!   step (its own claims, its seat reservations and locks repriced by AS-1/AS-2 and lane escrow's
//!   bind, the rest unchanged), the instantaneous floor-claim capacity `N_instant` today and per step
//!   (E-T3's number), and whether AG-3 would have frozen it;
//! * **network** — `bounded_immature` today vs `Σ_b min(X_b, W_cap)`, seat duty and lock totals and
//!   the §5.4 seat-capacity estimate per step, the licence queue and the carriage it needs,
//!   attribution counters per class (convictions by kind, DA sessions by non-panel filers, the
//!   conviction-latency histogram), and — for bonds a caller names as adversarial, the O-3 runs of
//!   ADR-0160 §9 Stage 0 — the measured attribution rate `q` per (class, strategy) with the A8 alarm.
//!
//! **Which steps.** A caller's steps; else [`palw_capacity_display_steps_v1`]: the schedule F-L arms
//! when the params carry one, else the UNCREDITED ramp ([`PALW_CAPACITY_UNCREDITED_STEPS_V1`]:
//! ρ 10 … 1000 at `q = 0`, so `m_c = E`). ADR-0160 v1's reference ramp (`q = 143‰`) is priced only
//! when a caller names it, and under the rules as built it prices exactly as the uncredited ramp
//! (below `q_seat`, nothing is credited); v1's figures for it (E-T3's 20 / 50 / 101 / 203 / 2,030)
//! appear only in each step's "superseded (v1, L = 3G)" column (review of lane shadow, round 2,
//! finding 2).
//!
//! **Where each "new" value comes from.** The formulas are [`crate::palw_capacity_formulas_v1`]'s;
//! the reading of the state is this module's and it is stated where it is an approximation:
//!
//! * The as-if rule applies to every attempt claim; free-prompt claims keep today's accounting
//!   (E-6: `E = 0`, `rights_reserved` per claim), and so does their reservation.
//! * **`m_c` is priced as the fold prices it** — lane escrow's `palw_escrow_term_v2` (`d6a058249`,
//!   [`palw_capacity_m_c_v2`]): a credit applies only from `q_seat` (250‰), and `m*` is priced on the
//!   conviction floor `rcore/cap-int`'s fold gives the claim ([`palw_capacity_conviction_floor_v1`]:
//!   `Tier(min(100‰ · C_min, 3E))`, 1,300 MSK on testnet-12, while lane liab keeps kinds 5 / 12 on
//!   the tier). So a 13k bond holds 2 floor claims below 250‰, 4 at 250‰ and 10 at 500‰, at every ρ;
//!   `⌈E/ρ⌉` binds only from 693‰ (ρ 10) to 997‰ (ρ 1000). Lane D replaces the rule (ADR-0160 v2
//!   §5.3; v3's operator audit door, D-23), and the shadow follows it there (v2 D.11 / v3 D.9).
//! * **Where the consensus lanes already fix a reading, the shadow takes theirs** (review of lane
//!   shadow, finding 3), so G5's "shadow = fold" check can hold on `rcore/cap-int`:
//!   - the stage of a `DefaultDisputed` claim is its resumed phase's (lane weight's
//!     `stage_of_phase`), and so is "unlicensed";
//!   - the weight budget `R_budget` is held by NON-TERMINAL claims only (lane weight's index releases
//!     a terminal claim's `reserved` at once): it is spent in acceptance order (`accepted_daa`, then
//!     claim id) over the bond's live attempt claims, and a void still inside its hold keeps the
//!     `reserved` it was priced at in its commitment without holding budget;
//!   - E-4's hold (`m_c + reserved` to `voided_daa + h_obl`, `h_obl = window_receipt`, inclusive
//!     like the abandon hold) is kept for the reasons nobody is convicted under, not for a
//!     conviction's void (lane escrow's `palw_void_reason_keeps_obligation_v1`;
//!     [`palw_capacity_void_reason_keeps_obligation_v1`]).
//! * A seat duty row is repriced by lane liab's AS-1 ([`palw_capacity_seat_duty_liab_v1`]:
//!   `min(max(⌈λ/ρ⌉, lock′), commitment′ / seats)`, `lock′` AS-2's lock — the lock the licence will
//!   post, undivided below `q_seat`), and what the seat RESERVES at bind is lane escrow's
//!   ([`palw_capacity_seat_bind_reservation_v1`]: `max(duty′, lock′)` for an attributable attempt, so
//!   a credited commitment never binds a seat below the lock it must post). The row's stored
//!   `seat_exposure` `d` is the λ-term (an attempt row not capped at bind: floor and 8k, where
//!   `λ ≥ lock_2`; `0` on a free-prompt row, whose duty IS its `lock_2`) and `lock_2` is re-derived
//!   from the claim's frozen gain at `k′ = 2` (`palw_rcore_lock_v1`, with no buyback slice beyond the
//!   cap — an upper bound for a class with a pair). A row capped by `commitment / seats` at bind (2M)
//!   reports a lower bound, and the count of such rows is reported.
//! * AS-2's lock credit is lane liab's consensus rule ([`palw_capacity_seat_lock_liab_v1`]: `q_seat`
//!   = 250‰ flat, `L_seat = 3G`), so a lock shrinks by ρ only under a step crediting `q ≥ 250‰`.
//!   §10 D-5's recommendation (`L_seat =` [`PALW_CAPACITY_SEAT_L_IF_D5_SOMPI_V1`], the 130,000 MSK
//!   seat floor, `q_seat ≈ 25‰`) is undecided and reported only as its own column
//!   (`seat_credit_if_d5`, `seat_capacity_if_d5_milli_per_daa`), never folded into A-1 or `N_instant`.
//! * AG-3's freeze reads the chain's conviction records ([`palw_capacity_conviction_freeze_class_v1`]):
//!   `DaDefault` is the intent class (final), and so is `CourtConviction` unless its claim was voided
//!   `CourtHeldVerdict` (a held dissection's verdict proves the producer's filings false, not the
//!   committed execution: tier, as lane liab reads it); `ExecutorRefuted` and `PanelFalseValidV2`
//!   may be either (their contradiction kind is not in the record), so they freeze for
//!   `window_court` and are reported as undetermined; the rest are tier-capped. A
//!   `PanelFalseValidV2` finding that ACTED on its claim (the claim, or its liability row, voided
//!   `CourtFraud` in the record's own block — what a finding that proves the claim false writes)
//!   charges the claim's producer by the same class as the seat, as lane liab's `liable` list does;
//!   the kind-3 records a DA default writes on its covering signers (the claim voided
//!   `ProducerWithholding` in the same block) charge the seats only. On `rcore/cap-int` the shadow
//!   reads lane liab's rooted freeze map instead of re-deriving it.
//!
//! **The measured `q` counts only what the credit prices** (review of lane shadow, round 2,
//! finding 1). An O-3 run names its bonds, each with its strategy ([`PalwCapacityStrategyV1`]:
//! naive, garbage, borrowed — ADR-0160 v2 §5.3's table — or unnamed), and `q` is measured per
//! (class, strategy) over RESOLVED attempt claims (the credit prices `E`; a free-prompt claim has
//! none) ([`palw_capacity_adversary_outcome_v1`]):
//!
//! * **caught** — the claim's own producer convicted by a route the credit prices, before the
//!   claim's earliest maturity (`accepted_daa + window_court`: no reward can have moved). As built
//!   that is lane liab's `palw_producer_conviction_credits_q_v1` — the intent class, the only
//!   producer convictions that collect at least the `L` the credit assumes — as far as a record
//!   shows it ([`palw_capacity_route_credits_q_v1`]): a `DaDefault` record (S1, intent as lane liab
//!   builds it), or a `CourtConviction` on a `CourtFraud` void (a proven verdict);
//! * **misses** — a priced conviction that came later (`caught_late`: the reward may have escaped);
//!   a conviction by a route the credit does not price (`caught_unpriced`, by route: a kind-4
//!   refutation or a kind-3 finding acting on the claim, whose contradiction tag — tier 5 / 12 or
//!   intent 9 / 10 / 11 / 13 — is not in the record; a held verdict; a court default; a conviction
//!   void whose record the state does not hold; any other kind); and a claim resolved with no
//!   producer conviction (`undetected`: `Final`, or voided for a reason that convicts nobody);
//! * **out of `q`** — claims still in flight, and claims a bond-level forfeiture voided before
//!   their own outcome (`censored`; [`palw_capacity_void_attribution_v1`] is an exhaustive match: a
//!   void reason added later does not compile until it is classed). Seat-only records naming a claim
//!   (a covering signer's, a finding that restated an earlier void) are counted beside it
//!   (`seat_only`) and change nothing.
//!
//! So `q` is a LOWER bound: never inflated by a conviction the credit cannot rely on. Its cost is
//! that garbage and borrowed read 0 here (their routes are kind 4, untagged in the record) until
//! `rcore/cap-int` reads the tag from lane liab's rooted freeze map, or lane D's AG-6 prices the tier
//! routes — **this build's `q` is evidence for the naive route only, never a credit's input** (v2's
//! G2 needs ≥ 50 claims per class and strategy with the routes in force; v3 retires the sampled `q`
//! altogether — the credit becomes the audit door's `d = 1000‰`, G2′ — and D.9 turns this column
//! into `d` and the operator's would-be audit latency).
//!
//! **The A8 alarm, per step** ([`PalwCapacityStepShadowV1::q_alarm`]): the step's
//! `q_required = 2 × max(q_needed at the priced routes' L = 3G, q_seat, q_credit)` — 500‰ for any
//! credit up to 250‰, v2 §8.2's bar — and the alarm fires when a measured (class, strategy) row of
//! an attributable class is below it, or when the step credits anything (`q_credit > 0`) and some
//! attributable class the chain holds has a named strategy nobody measured. C7 rows never alarm (C7
//! is never credited). A node that names no bond measures nothing, and alarms only on a credited step.
//!
//! **Cost** (S-I3): one pass over the claims, the bonds, their locks, the duty rows, the DA sessions
//! and the conviction records — `O(claims + bonds + locks + rows)` per call, with a sort of each
//! bond's live claims. kaspad calls it every [`PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1`] DAA.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_hashes::Hash64;

use crate::palw_capacity_formulas_v1::{
    PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1, PALW_CAPACITY_Q_SEAT_PERMILLE_V1, PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1,
    PALW_CAPACITY_SOMPI_PER_MSK_V1, PALW_CAPACITY_UNCREDITED_STEPS_V1, PALW_CAPACITY_W_FCW_SOMPI_V1, PalwCapacityConvictionFloorV1,
    PalwCapacitySeatCapitalInputsV1, PalwCapacityStageV1, PalwCapacityStepV1, palw_capacity_bond_weight_term_v1,
    palw_capacity_carriers_per_block_v1, palw_capacity_claims_per_daa_milli_v1, palw_capacity_consensus_reservation_s1_v1,
    palw_capacity_conviction_floor_v1, palw_capacity_conviction_l_v1, palw_capacity_escrow_credit_applies_v1, palw_capacity_h_obl_v1,
    palw_capacity_is_unlicensed_v1, palw_capacity_m_c_v1, palw_capacity_m_c_v2, palw_capacity_m_ramp_v1, palw_capacity_n_instant_v1,
    palw_capacity_q_needed_permille_v1, palw_capacity_q_needed_permille_v2, palw_capacity_seat_bind_reservation_v1,
    palw_capacity_seat_capital_per_claim_v1, palw_capacity_seat_credit_applies_v1, palw_capacity_seat_credit_liab_v1,
    palw_capacity_seat_duty_liab_v1, palw_capacity_seat_duty_with_lock_v1, palw_capacity_seat_lock_liab_v1,
    palw_capacity_seat_lock_v1, palw_capacity_stage_of_claim_v1, palw_capacity_staged_weight_v1, palw_capacity_void_holds_v1,
    palw_capacity_void_reason_keeps_obligation_v1, palw_capacity_weight_budget_sompi_v1, palw_capacity_weight_cap_v1,
    palw_capacity_weight_full_v1,
};
use crate::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
use crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1;
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwClassStatusV2,
    PalwRcoreGateV1, PalwStateParamsV2, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_bond_committed_raw_v1,
    palw_claim_commitment_v1, palw_claim_g_v1, palw_rcore_class_is_c7_v1, palw_rcore_gate_room_of_v1, palw_rcore_lock_v1,
    palw_second_clock_depth_v1,
};

/// kaspad recomputes the shadow every this many DAA of the tip (ADR-0160 §7.5).
pub const PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1: u64 = 10;

/// The reference bond of the log line's `N13k[ρ]`: the 13,000 MSK producer floor.
pub const PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1: u64 = 13_000 * 100_000_000;

/// **`L_seat` of §10 D-5's conditional column** (§4.7): the 130,000 MSK seat floor, what a located
/// `PanelFalseValidV2` forfeits at least under D-5's recommendation (`q_seat` ≈ 25‰). D-5 is a user
/// decision still open and lane liab prices AS-2 at `L_seat = 3G` (250‰), so this value prices
/// only `seat_credit_if_d5` / `seat_capacity_if_d5_milli_per_daa`, never A-1 or `N_instant`.
pub const PALW_CAPACITY_SEAT_L_IF_D5_SOMPI_V1: u128 = 130_000 * PALW_CAPACITY_SOMPI_PER_MSK_V1;

/// The seats of a floor panel when the state holds none to count (testnet-12's panel).
pub const PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1: u32 = 5;

/// The conviction-latency histogram's bucket upper bounds (DAA from the claim's acceptance to the
/// conviction, exclusive); a last bucket takes everything at or past 3,000 (`window_court`).
pub const PALW_CAPACITY_LATENCY_BUCKETS_V1: [u64; 7] = [10, 50, 100, 300, 600, 1_200, 3_000];

/// Licences counted as "recent" (the carriage's observed rate) within this many DAA of `now`.
pub const PALW_CAPACITY_SHADOW_RECENT_DAA_V1: u64 = PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1;

/// **An O-3 run's strategy** (ADR-0160 v2 §5.3's table): what a named adversary bond's fraudulent
/// claims are, so `q` is measured per (class, strategy) — a fraudster picks the strategy the
/// auditors catch least, so a credit needs every named strategy measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwCapacityStrategyV1 {
    /// Roots with no material behind them: an honest seat cannot replay, and the route is the DA
    /// default (S1).
    Naive,
    /// Real material, wrong arithmetic or logits: a kind-4 refutation, contradiction 5 / 12 (tier).
    Garbage,
    /// A real execution of another job, output or prompt: kind 4, contradiction 9 / 10 / 11 / 13
    /// (intent).
    Borrowed,
    /// The run named no strategy: measured and reported, but it backs no strategy's credit.
    #[default]
    Unnamed,
}

impl PalwCapacityStrategyV1 {
    /// The strategies a credit must see measured.
    pub const NAMED: [Self; 3] = [Self::Naive, Self::Garbage, Self::Borrowed];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Naive => "naive",
            Self::Garbage => "garbage",
            Self::Borrowed => "borrowed",
            Self::Unnamed => "unnamed",
        }
    }

    /// `naive`, `garbage` or `borrowed` (any case); anything else is no strategy.
    pub fn parse_v1(text: &str) -> Option<Self> {
        Self::NAMED.into_iter().find(|s| s.name().eq_ignore_ascii_case(text.trim()))
    }
}

/// **One bond of an O-3 run, and its strategy.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCapacityAdversaryV1 {
    pub bond: PalwBondKeyV2,
    pub strategy: PalwCapacityStrategyV1,
}

/// **`<txid>:<index>[:<strategy>]` → the outpoint text and the strategy** (unnamed when there is
/// none) — the one reading of kaspad's `--palw-capacity-shadow-adversary`, `getPalwCapacityShadow`'s
/// `adversaryBonds` and the CLI's `--adversary`. The outpoint itself is parsed by the caller.
pub fn palw_capacity_split_adversary_v1(text: &str) -> Result<(&str, PalwCapacityStrategyV1), String> {
    let text = text.trim();
    match text.rsplit_once(':') {
        Some((head, tail)) if head.contains(':') => PalwCapacityStrategyV1::parse_v1(tail)
            .map(|strategy| (head, strategy))
            .ok_or_else(|| format!("'{text}': unknown strategy '{tail}' (naive, garbage or borrowed)")),
        _ => Ok((text, PalwCapacityStrategyV1::Unnamed)),
    }
}

/// **A bond named twice must name one strategy** — the list a caller hands the shadow, refused when
/// one bond carries two.
pub fn palw_capacity_check_adversaries_v1(adversaries: &[PalwCapacityAdversaryV1]) -> Result<(), String> {
    let mut seen: BTreeMap<PalwBondKeyV2, PalwCapacityStrategyV1> = BTreeMap::new();
    for adversary in adversaries {
        if let Some(previous) = seen.insert(adversary.bond, adversary.strategy)
            && previous != adversary.strategy
        {
            return Err(format!(
                "bond {}:{} is named as {} and as {}",
                adversary.bond.0.transaction_id,
                adversary.bond.0.index,
                previous.name(),
                adversary.strategy.name()
            ));
        }
    }
    Ok(())
}

/// What one shadow computation reads besides the state and its params.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacityShadowOptionsV1 {
    /// The ramp steps to price. Empty: [`palw_capacity_display_steps_v1`]`(None)`, the uncredited
    /// ramp — the processor's read passes F-L's armed schedule instead once the params carry one.
    /// ADR-0160 v1's reference ramp (`PALW_CAPACITY_REFERENCE_STEPS_V1`, q 143‰) only when named here.
    pub steps: Vec<PalwCapacityStepV1>,
    /// The second clock's RAW depth at `now_daa` (the processor's `palw_settled_anchor_depth_at`);
    /// `None` is the DAA-only lock rule. Only lock liveness reads it.
    pub raw_depth: Option<u64>,
    /// The bonds of an O-3 run and their strategies: their claims measure `q`
    /// ([`palw_capacity_check_adversaries_v1`] refuses a bond named with two; here the first wins).
    pub adversaries: Vec<PalwCapacityAdversaryV1>,
    /// The block transient-mass limit the carriage estimate divides (`Params::max_block_mass`);
    /// `0` reads 500,000, testnet-12's.
    pub block_mass_limit: u64,
    /// `E` of a claim the next block would accept (`palw_claim_escrow_v1` at the tip's subsidy
    /// with the carve the fold resolves there — ADR-0126's overlay carve on testnet-12), the
    /// reference when the state holds no attempt claim to read it off. `None`: the bundle's own
    /// carve of the tip's subsidy.
    pub reference_escrow_sompi: Option<u64>,
}

/// **One claim, today and under the new rules.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityClaimShadowV1 {
    pub claim_id: Hash64,
    pub bond: PalwBondKeyV2,
    pub class_id: Hash64,
    /// The phase's name (`provisional`, `panel-bound`, `licensed`, `final`, `voided`, `disputed`).
    pub phase: &'static str,
    pub stage: PalwCapacityStageV1,
    pub accepted_daa: u64,
    pub free_prompt: bool,
    /// A C7 class (no conviction route: `m_c = E`, the Final weight ceiling).
    pub c7: bool,
    /// The stored `immature_contribution` — today's weight while not Final.
    pub raw_w: u128,
    pub w_full: u128,
    pub staged_w: u128,
    /// Today's weight reservation (`claim.reserved`) and the capped one.
    pub reserved_today: u128,
    pub reserved_new: u128,
    /// Today's escrow term on the bond (`claim_escrow_reservation_v1`).
    pub escrow_term_today: u128,
    /// SR-1 has released the escrow term (E-3 releases `m_c` on the same conditions).
    pub escrow_released: bool,
    /// Today's commitment (`palw_claim_commitment_v1`).
    pub commitment_today: u128,
    /// `L = 3G` for this claim (v1's pricing; the superseded column and A8's route `L`).
    pub l_sompi: u128,
    /// `m_c` per step as the fold prices it ([`palw_capacity_m_c_v2`]; 0 for a free-prompt claim).
    pub m_new: Vec<u128>,
    /// The commitment per step.
    pub commitment_new: Vec<u128>,
}

/// **One bond, today and under the new rules.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityBondShadowV1 {
    pub bond: PalwBondKeyV2,
    /// Posted collateral (`record.collateral`) and its 500‰ ceiling.
    pub collateral: u64,
    pub ceiling: u128,
    /// A seat: registered, active, capable of at least one class.
    pub seat: bool,
    /// Claims with a commitment under either rule, and of them the unlicensed ones.
    pub live_claims: u64,
    pub unlicensed_claims: u64,
    /// Today's immature weight of its non-terminal claims (`Σ raw_w`).
    pub raw_immature_today: u128,
    pub w_cap: u128,
    /// `X_b = Σ` staged weight of its provisional claims, and `min(X_b, W_cap)`.
    pub x_b: u128,
    pub capped: u128,
    /// The lane's `R_budget` (withdrawn at stage 1: the fold reserves `⌈w / ρ⌉`), for its history.
    pub r_budget: u128,
    /// Σ of its live claims' stage-1 reservation at the first step's ρ.
    pub reserved_new_total: u128,
    /// A-1 today (`palw_bond_committed_raw_v1`), and of it the bond's own claims' commitments.
    pub committed_today: u128,
    pub own_claims_today: u128,
    pub committed_new: Vec<u128>,
    /// Floor claims its collateral holds at once with its other commitments as they stand
    /// (`N_instant`), and how many more it can open now.
    pub n_instant_today: u64,
    pub n_instant_new: Vec<u64>,
    pub n_more_today: u64,
    pub n_more_new: Vec<u64>,
    /// AG-3 would hold it frozen now; `freeze_final` if an intent-class conviction did it.
    pub frozen_would_be: bool,
    pub freeze_final: bool,
    pub freeze_undetermined: bool,
    pub first_conviction_daa: Option<u64>,
    pub convictions: u64,
}

/// **The network under one step.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityStepShadowV1 {
    pub step: PalwCapacityStepV1,
    /// `m_c` of the reference floor claim as the fold prices it ([`palw_capacity_m_c_v2`]).
    pub m_floor: u128,
    /// The credit lane escrow's rule needs for `⌈E/ρ⌉` to bind ([`palw_capacity_q_needed_permille_v2`],
    /// never below `q_seat`): 693 … 997‰ on testnet-12's floor.
    pub q_needed_permille: u16,
    /// The ramp term `⌈E/ρ⌉` is what `m_floor` is (the credit applies and reaches `q_needed`).
    pub ramp_binds: bool,
    /// The credit applies — lane escrow's gate and lane liab's AS-2, one threshold (`q_credit ≥
    /// 250‰`): `m_c` falls below `E` and the locks shrink by ρ only past it.
    pub seat_credit: bool,
    /// §10 D-5's conditional column (`L_seat` = the 130,000 MSK seat floor, `q_seat` ≈ 25‰; an open
    /// user decision): would the seat credit apply, and the §5.4 estimate if it did.
    pub seat_credit_if_d5: bool,
    pub seat_capacity_if_d5_milli_per_daa: u64,
    pub claims_commitment_total: u128,
    pub committed_total: u128,
    /// What the seats reserve at bind (lane liab's AS-1 duty, lane escrow's `max(duty′, lock′)`) and
    /// the locks (AS-2), summed over the rows.
    pub seat_duty_total: u128,
    pub seat_lock_total: u128,
    /// §5.4: floor claims per DAA the seats' capital sustains, in thousandths (the reservation at
    /// bind and the AS-2 lock of the reference floor claim).
    pub seat_capacity_milli_per_daa: u64,
    /// `N_instant` of a fresh 13,000 MSK bond on floor claims.
    pub n_instant_13k: u64,
    /// **The "superseded (v1, L = 3G)" column**: `m_c` and `N_instant(13k)` as ADR-0160 v1 priced
    /// them (`L = 3G`, no `q_seat` gate) — E-T3's 20 / 50 / 101 / 203 / 2,030 at q 143‰. Not a rule
    /// anywhere; kept so the v1 tables can be read against what the fold gives.
    pub m_floor_v1_superseded: u128,
    pub n_instant_13k_v1_superseded: u64,
    /// `q` the routes the credit prices need at their `L` (the reference claim's `3G`) for `⌈E/ρ⌉`
    /// (v1's figure: 131 … 143‰) — the first term of `q_required_permille`.
    pub q_needed_route_permille: u16,
    /// **A8's bar**: `2 × max(q_needed_route, q_seat, q_credit)` — 500‰ for a credit up to 250‰.
    pub q_required_permille: u16,
    /// A8: a measured (class, strategy) row of an attributable class is below `q_required`, or
    /// [`Self::q_alarm_unmeasured`].
    pub q_alarm: bool,
    /// The step credits (`q_credit > 0`) and an attributable class the chain holds has a named
    /// strategy with no measurement.
    pub q_alarm_unmeasured: bool,
}

/// **Attribution counters of one class** (`class_id` zero: records whose claim the state no longer
/// holds, or that name none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacityClassAttributionV1 {
    pub class_id: Hash64,
    pub claims_live: u64,
    pub claims_final: u64,
    pub claims_voided: u64,
    /// Voided claims by reason (`Debug` names), attributed or not.
    pub voids_by_reason: Vec<(String, u64)>,
    /// Voids whose reason is a conviction (`CourtFraud`, `ProducerWithholding`, `CourtDefault`,
    /// `CourtHeldVerdict`), priced or not.
    pub voids_attributed: u64,
    /// Conviction records by `PalwOffenceKindV1` discriminant.
    pub convictions_by_kind: Vec<(u8, u64)>,
    /// Open DA sessions by filers that are not a seat of the claim's panel, and by seats.
    pub da_open_non_seat: u64,
    pub da_open_seat: u64,
    /// Sessions ever opened by non-seats on the claims the state still holds.
    pub da_opened_non_seat_total: u64,
    /// DAA from acceptance to conviction, bucketed by [`PALW_CAPACITY_LATENCY_BUCKETS_V1`].
    pub conviction_latency_histogram: [u64; 8],
}

/// **The measured attribution rate of one (class, strategy)** — the attempt claims of the named
/// bonds of that strategy the state holds: `caught + caught_late + caught_unpriced + undetected +
/// censored + in_flight`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwCapacityAdversaryRowV1 {
    pub class_id: Hash64,
    pub strategy: PalwCapacityStrategyV1,
    /// A C7 class: never credited, never alarms.
    pub c7: bool,
    pub claims: u64,
    /// Its producer convicted by a route the credit prices before its earliest maturity.
    pub caught: u64,
    /// Convicted by a priced route only at or past `accepted_daa + window_court` (a miss for `q`).
    pub caught_late: u64,
    /// Convicted only by routes the credit does not price (a miss for `q`), and by which.
    pub caught_unpriced: u64,
    pub unpriced_by_route: Vec<(&'static str, u64)>,
    /// Resolved with no producer conviction (a miss for `q`).
    pub undetected: u64,
    /// Voided by a bond-level forfeiture before its own outcome (out of `q`).
    pub censored: u64,
    /// Not yet resolved (out of `q` until it is).
    pub in_flight: u64,
    /// Claims a seat-only record names (counted beside; the claim's outcome is above).
    pub seat_only: u64,
    /// `caught / (caught + caught_late + caught_unpriced + undetected)` in permille (`None` while
    /// none has resolved).
    pub q_measured_permille: Option<u16>,
}

/// **Everything one shadow computation reports.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapacityShadowV1 {
    pub now_daa: u64,
    /// The committed tip's DAA (the state's last point).
    pub tip_daa: u64,
    pub steps: Vec<PalwCapacityStepShadowV1>,
    pub claims: Vec<PalwCapacityClaimShadowV1>,
    pub bonds: Vec<PalwCapacityBondShadowV1>,
    // Weight (J-1).
    pub bounded_immature_today: u128,
    pub bounded_immature_new: u128,
    pub safe_weight: u128,
    pub w_cap_total: u128,
    // The reference floor claim the network figures price.
    pub reference_escrow: u128,
    pub reference_w_floor: u128,
    pub reference_l: u128,
    /// The conviction floor its `m_c` is priced against ([`palw_capacity_conviction_floor_v1`]).
    pub reference_floor: PalwCapacityConvictionFloorV1,
    pub reference_seats: u32,
    pub reference_duty: u128,
    pub reference_lock: u128,
    // Money.
    pub claims_commitment_today: u128,
    pub committed_today_total: u128,
    // Seats.
    pub seats: u64,
    pub seat_usable_capital: u128,
    pub seat_duty_total_today: u128,
    pub seat_lock_total_today: u128,
    pub seat_capacity_today_milli_per_daa: u64,
    pub duty_rows: u64,
    /// Duty rows capped by `commitment / seats` at bind: their repriced duty is a lower bound.
    pub duty_rows_capped: u64,
    // Carriage.
    pub licence_queue: u64,
    pub licence_queue_oldest_bound_daa: Option<u64>,
    pub licensed_recent: u64,
    pub carriers_per_block: u64,
    pub carriage_blocks_to_drain: u64,
    // Attribution.
    pub attribution: Vec<PalwCapacityClassAttributionV1>,
    /// `q` per (class, strategy) of the named O-3 bonds, in (class, strategy) order.
    pub adversary: Vec<PalwCapacityAdversaryRowV1>,
    /// The attributable (not C7) classes a credited step must see measured: `Active` classes the
    /// chain holds and the classes of its attempt claims.
    pub credited_classes: Vec<Hash64>,
    pub convictions_total: u64,
}

impl PalwCapacityShadowV1 {
    /// The step row for `rho`, if priced.
    pub fn step_for_rho(&self, rho: u32) -> Option<&PalwCapacityStepShadowV1> {
        self.steps.iter().find(|s| s.step.rho == rho)
    }

    /// The adversary row of (`class_id`, `strategy`), if measured.
    pub fn adversary_row(&self, class_id: &Hash64, strategy: PalwCapacityStrategyV1) -> Option<&PalwCapacityAdversaryRowV1> {
        self.adversary.iter().find(|row| row.class_id == *class_id && row.strategy == strategy)
    }

    /// **The one compact log line** (ADR-0160 §7.5): `capacity-shadow: daa=… immature today/new=…/…
    /// bonds=… claims=… N13k[ρ@q‰]=… seatcap[ρ@q‰]=… queue=… convictions=…`. Weights in FCW, seat
    /// capacity in floor claims per DAA. Every per-step figure is labelled with the `q` it credits,
    /// so a conditional row (a credit the chain has not measured) cannot pass for the armed rule;
    /// every figure is the rule as built (the superseded v1 column is the RPC's and the CLI's only).
    pub fn summary(&self) -> String {
        let fcw = |w: u128| w / crate::palw_capacity_formulas_v1::PALW_CAPACITY_FCW_V1;
        let n13k: Vec<String> =
            self.steps.iter().map(|s| format!("{}@{}:{}", s.step.rho, s.step.q_credit_permille, s.n_instant_13k)).collect();
        let seatcap: Vec<String> = self
            .steps
            .iter()
            .map(|s| {
                format!(
                    "{}@{}:{}.{:02}",
                    s.step.rho,
                    s.step.q_credit_permille,
                    s.seat_capacity_milli_per_daa / 1_000,
                    (s.seat_capacity_milli_per_daa % 1_000) / 10
                )
            })
            .collect();
        let alarm = if self.steps.iter().any(|s| s.q_alarm) { " q-ALARM" } else { "" };
        format!(
            "capacity-shadow: daa={} immature today/new={}/{} FCW bonds={} claims={} N13k[ρ@q‰]={} seatcap[ρ@q‰]={} (today {}.{:02}/DAA) queue={} convictions={}{}",
            self.now_daa,
            fcw(self.bounded_immature_today),
            fcw(self.bounded_immature_new),
            self.bonds.len(),
            self.claims.len(),
            n13k.join(","),
            seatcap.join(","),
            self.seat_capacity_today_milli_per_daa / 1_000,
            (self.seat_capacity_today_milli_per_daa % 1_000) / 10,
            self.licence_queue,
            self.convictions_total,
            alarm
        )
    }
}

/// **The shadow at `now_daa` over `steps`** (ADR-0160 §7.5's signature): the DAA-only lock rule, no
/// adversary set, testnet-12's block mass. [`palw_capacity_shadow_with_v1`] takes the rest.
pub fn palw_capacity_shadow_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now_daa: u64,
    steps: &[PalwCapacityStepV1],
) -> PalwCapacityShadowV1 {
    palw_capacity_shadow_with_v1(state, params, now_daa, &PalwCapacityShadowOptionsV1 { steps: steps.to_vec(), ..Default::default() })
}

/// The phase's short name.
fn phase_name(phase: &PalwClaimPhaseV2) -> &'static str {
    match phase {
        PalwClaimPhaseV2::Provisional => "provisional",
        PalwClaimPhaseV2::PanelBound { .. } => "panel-bound",
        PalwClaimPhaseV2::ReceiptLicensed { .. } => "licensed",
        PalwClaimPhaseV2::Final { .. } => "final",
        PalwClaimPhaseV2::Voided { .. } => "voided",
        PalwClaimPhaseV2::DefaultDisputed { .. } => "disputed",
    }
}

/// **What a void's reason says about its claim's producer**, before any record is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCapacityVoidAttributionV1 {
    /// The void is a conviction of the producer; which route — and whether the credit prices it —
    /// is read off the conviction record ([`palw_capacity_route_of_record_v1`]).
    Conviction,
    /// The claim resolved without anyone being convicted of it (a timeout, a failed class).
    Undetected,
    /// The claim was voided because its BOND was convicted on another claim (AG-2's aggregate
    /// forfeiture): its own outcome was never observed, so it is out of `q` — counting it caught
    /// would read a K-claim campaign with one conviction as `q = 1`, counting it missed as `1/K`.
    Censored,
}

/// **The class of every void reason — an exhaustive match, on purpose** (review of lane shadow,
/// finding 2): a reason added by a later lane does not compile here until it is classed. Lane liab's
/// `AggregateForfeit` (borsh 9, AG-2: every other live claim of a forfeited bond) is `Censored` (the arm
/// rcore/cap-s1's merge added; v2's `TaintLapsed` — AG-7 — would be a miss, lane D).
pub fn palw_capacity_void_attribution_v1(reason: PalwVoidReasonV2) -> PalwCapacityVoidAttributionV1 {
    match reason {
        PalwVoidReasonV2::CourtFraud
        | PalwVoidReasonV2::ProducerWithholding
        | PalwVoidReasonV2::CourtDefault
        | PalwVoidReasonV2::CourtHeldVerdict => PalwCapacityVoidAttributionV1::Conviction,
        PalwVoidReasonV2::BindTimeout
        | PalwVoidReasonV2::ReceiptTimeout
        | PalwVoidReasonV2::NoCapablePanel
        | PalwVoidReasonV2::UnavailableQuorum
        | PalwVoidReasonV2::NotReplayBacked => PalwCapacityVoidAttributionV1::Undetected,
        PalwVoidReasonV2::AggregateForfeit => PalwCapacityVoidAttributionV1::Censored,
    }
}

/// A void whose reason is a conviction of the producer (priced or not).
fn void_is_attributed(reason: PalwVoidReasonV2) -> bool {
    palw_capacity_void_attribution_v1(reason) == PalwCapacityVoidAttributionV1::Conviction
}

/// **The route a producer conviction took** (ADR-0160 v2 §5.3's routes), as far as the chain's
/// records show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwCapacityRouteV1 {
    /// A `DaDefault` record (kind 5) on the producer — S1, the naive strategy's route. Intent-class
    /// as lane liab builds it (the user's 17:50 decision 1 moves S1 to the tier on lane D, where v2
    /// prices it by DA-7's covering-signer charge instead).
    DaDefault,
    /// A `CourtConviction` (kind 6) on a `CourtFraud` void: a proven verdict, intent-class.
    CourtFraud,
    /// A `CourtConviction` on a `CourtHeldVerdict` void: a held dissection's verdict, tier.
    CourtHeldVerdict,
    /// An `ExecutorRefuted` (kind 4) on the producer: contradiction 5 / 12 (tier, garbage) or
    /// 9 / 10 / 11 / 13 (intent, borrowed) — the tag is not in the record.
    Refuted,
    /// A `PanelFalseValidV2` (kind 3) that acted on the claim: the producer is charged by the seat's
    /// contradiction class, which the record does not carry either.
    SeatFinding,
    /// A `CourtDefault` void: no record, never reaches the conviction funnel.
    CourtDefault,
    /// A conviction void (`CourtFraud`, `ProducerWithholding`) with no record of the producer in
    /// the state: nothing says which route it was.
    Unrecorded,
    /// Any other kind naming the claim against its producer (0, 1, 2).
    Other,
}

impl PalwCapacityRouteV1 {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DaDefault => "da-default",
            Self::CourtFraud => "court-fraud",
            Self::CourtHeldVerdict => "court-held-verdict",
            Self::Refuted => "refuted-untagged",
            Self::SeatFinding => "seat-finding-untagged",
            Self::CourtDefault => "court-default",
            Self::Unrecorded => "unrecorded",
            Self::Other => "other",
        }
    }
}

/// **Does a producer conviction by `route` count toward the attribution rate a step may credit?**
/// Lane liab's `palw_producer_conviction_credits_q_v1`: exactly the intent class — AG-2 takes the whole
/// bond, at least the `L` the credit assumes — as far as a record shows it: a proven court verdict. A
/// `DaDefault` is NOT counted since the user's decision 1 (rcore/cap-s1): S1 is tier-class, and a first
/// default collects the commitment alone.
/// A kind 3 / 4 record's class is its contradiction's, which the record does not carry, so it is
/// NOT counted (conservative: it could be 5 / 12, which collects the S2 tier alone and leaves
/// `EV(1) > 0` at a credited step — lane liab's L-T6); nor are a held verdict, a court default or
/// any other kind. On `rcore/cap-int` the tag is readable through lane liab's rooted freeze map, and
/// lane D's rule (AG-6 on the tier routes; v3's audit door) replaces this one (v2 D.11 / v3 D.9).
pub fn palw_capacity_route_credits_q_v1(route: PalwCapacityRouteV1) -> bool {
    match route {
        PalwCapacityRouteV1::CourtFraud => true,
        PalwCapacityRouteV1::DaDefault
        | PalwCapacityRouteV1::CourtHeldVerdict
        | PalwCapacityRouteV1::Refuted
        | PalwCapacityRouteV1::SeatFinding
        | PalwCapacityRouteV1::CourtDefault
        | PalwCapacityRouteV1::Unrecorded
        | PalwCapacityRouteV1::Other => false,
    }
}

/// **The route of one conviction record of a claim's producer** (exhaustive over the kinds, so a
/// kind added later is routed by name before this compiles). A `PanelFalseValidV2` is the producer's
/// only when it acted on the claim ([`palw_capacity_false_valid_producer_v1`]); the caller decides.
pub fn palw_capacity_route_of_record_v1(
    offence: &PalwConsumedOffenceV1,
    claim: Option<&PalwClaimStateV2>,
    liability: Option<&PalwPanelLiabilityRecordV1>,
) -> PalwCapacityRouteV1 {
    match offence.kind {
        PalwOffenceKindV1::DaDefault => PalwCapacityRouteV1::DaDefault,
        PalwOffenceKindV1::CourtConviction => match conviction_void(claim, liability).map(|(_, reason)| reason) {
            Some(PalwVoidReasonV2::CourtFraud) => PalwCapacityRouteV1::CourtFraud,
            Some(PalwVoidReasonV2::CourtHeldVerdict) => PalwCapacityRouteV1::CourtHeldVerdict,
            _ => PalwCapacityRouteV1::Other,
        },
        PalwOffenceKindV1::ExecutorRefuted => PalwCapacityRouteV1::Refuted,
        PalwOffenceKindV1::PanelFalseValidV2 => PalwCapacityRouteV1::SeatFinding,
        PalwOffenceKindV1::ExecutorEquivocation | PalwOffenceKindV1::PanelFalseValid | PalwOffenceKindV1::CourtExecutorGuilty => {
            PalwCapacityRouteV1::Other
        }
    }
}

/// **What one adversary claim's outcome counts as in `q`.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCapacityOutcomeV1 {
    /// Its producer convicted by a priced route before its earliest maturity.
    Caught,
    /// Convicted by a priced route only at or past `accepted_daa + window_court`.
    CaughtLate,
    /// Convicted only by a route the credit does not price.
    CaughtUnpriced(PalwCapacityRouteV1),
    /// Resolved with no producer conviction.
    Undetected,
    /// Voided by a bond-level forfeiture before its own outcome (out of `q`).
    Censored,
    /// Not resolved yet (out of `q`).
    InFlight,
}

/// **The outcome of one adversary claim** from its producer's conviction records `(route, DAA)`
/// (review of lane shadow, round 2, finding 1): a priced route before the claim's earliest maturity
/// (`accepted_daa + window_court` — a row matures no earlier than `Final + window_court`) is caught;
/// later, a miss (`CaughtLate`); only unpriced routes, a miss by the earliest of them; no record: the
/// void's reason decides (a conviction void with no record is a miss — `CourtDefault`,
/// `CourtHeldVerdict`, else `Unrecorded`), `Final` is undetected, anything else in flight. A
/// conviction naming the claim decides it whatever its phase (AG-5 keeps a void convictable).
pub fn palw_capacity_adversary_outcome_v1(
    claim: &PalwClaimStateV2,
    convictions: &[(PalwCapacityRouteV1, u64)],
    window_court: u64,
) -> PalwCapacityOutcomeV1 {
    let horizon = claim.accepted_daa.saturating_add(window_court);
    let priced: Vec<u64> =
        convictions.iter().filter(|(route, _)| palw_capacity_route_credits_q_v1(*route)).map(|(_, daa)| *daa).collect();
    if priced.iter().any(|daa| *daa < horizon) {
        return PalwCapacityOutcomeV1::Caught;
    }
    if !priced.is_empty() {
        return PalwCapacityOutcomeV1::CaughtLate;
    }
    if let Some((route, _)) = convictions.iter().min_by_key(|(route, daa)| (*daa, *route)) {
        return PalwCapacityOutcomeV1::CaughtUnpriced(*route);
    }
    match &claim.phase {
        PalwClaimPhaseV2::Voided { reason, .. } => match palw_capacity_void_attribution_v1(*reason) {
            PalwCapacityVoidAttributionV1::Conviction => PalwCapacityOutcomeV1::CaughtUnpriced(match reason {
                PalwVoidReasonV2::CourtDefault => PalwCapacityRouteV1::CourtDefault,
                PalwVoidReasonV2::CourtHeldVerdict => PalwCapacityRouteV1::CourtHeldVerdict,
                _ => PalwCapacityRouteV1::Unrecorded,
            }),
            PalwCapacityVoidAttributionV1::Undetected => PalwCapacityOutcomeV1::Undetected,
            PalwCapacityVoidAttributionV1::Censored => PalwCapacityOutcomeV1::Censored,
        },
        PalwClaimPhaseV2::Final { .. } => PalwCapacityOutcomeV1::Undetected,
        _ => PalwCapacityOutcomeV1::InFlight,
    }
}

/// **The steps shown when a caller names none**: the schedule F-L arms (`armed`, the processor's
/// read passes it once the params carry `palw_capacity_aggregate_liability`), else the uncredited
/// ramp [`PALW_CAPACITY_UNCREDITED_STEPS_V1`]. Never v1's reference ramp: its `q = 143‰` is a
/// credit no chain has measured, so it is shown only when asked (review of lane shadow, finding 1).
pub fn palw_capacity_display_steps_v1(armed: Option<&[PalwCapacityStepV1]>) -> Vec<PalwCapacityStepV1> {
    match armed {
        Some(steps) if !steps.is_empty() => steps.to_vec(),
        _ => PALW_CAPACITY_UNCREDITED_STEPS_V1.to_vec(),
    }
}

/// **The freeze class AG-3 would give a conviction of `kind`** (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCapacityFreezeClassV1 {
    /// Never lifted (AG-2's forfeiture).
    Intent,
    /// Lifted at `since + window_court` without a further conviction.
    Tier,
    /// Either, by a contradiction kind the record does not carry: held for `window_court`.
    Undetermined,
}

/// The shadow's reading of AG-2's intent class from a conviction record's kind alone.
pub fn palw_capacity_freeze_class_v1(kind: PalwOffenceKindV1) -> PalwCapacityFreezeClassV1 {
    match kind {
        // rcore/cap-s1, the user's decision 1: a DA default is TIER-class (a lifting freeze); a court
        // conviction stays intent.
        PalwOffenceKindV1::CourtConviction => PalwCapacityFreezeClassV1::Intent,
        PalwOffenceKindV1::ExecutorRefuted | PalwOffenceKindV1::PanelFalseValidV2 => PalwCapacityFreezeClassV1::Undetermined,
        _ => PalwCapacityFreezeClassV1::Tier,
    }
}

/// The void the chain wrote on a conviction's claim: the claim's own `Voided` phase, else its
/// liability row's (a claim already retired, or voided before the conviction reached it).
fn conviction_void(
    claim: Option<&PalwClaimStateV2>,
    liability: Option<&PalwPanelLiabilityRecordV1>,
) -> Option<(u64, PalwVoidReasonV2)> {
    match claim.map(|c| &c.phase) {
        Some(PalwClaimPhaseV2::Voided { voided_daa, reason }) => Some((*voided_daa, *reason)),
        _ => liability.and_then(|row| row.voided_daa.zip(row.void_reason)),
    }
}

/// **The freeze class of one conviction record** (review of lane shadow, finding 3(b)): the kind's
/// ([`palw_capacity_freeze_class_v1`]), except a `CourtConviction` whose claim was voided
/// `CourtHeldVerdict` — a held dissection's verdict, which lane liab keeps on the tier.
pub fn palw_capacity_conviction_freeze_class_v1(
    offence: &PalwConsumedOffenceV1,
    claim: Option<&PalwClaimStateV2>,
    liability: Option<&PalwPanelLiabilityRecordV1>,
) -> PalwCapacityFreezeClassV1 {
    match offence.kind {
        PalwOffenceKindV1::CourtConviction
            if conviction_void(claim, liability).is_some_and(|(_, reason)| reason == PalwVoidReasonV2::CourtHeldVerdict) =>
        {
            PalwCapacityFreezeClassV1::Tier
        }
        kind => palw_capacity_freeze_class_v1(kind),
    }
}

/// **The claim's producer, when a `PanelFalseValidV2` finding acted on its claim** (review of lane
/// shadow, finding 3(c)): a finding that proves the claim false voids it (or reverses its `Final`)
/// `CourtFraud` in the record's own block (`act_on_convicted_claim_v1`), and lane liab then charges
/// the producer by the seat's class. Read off the void the chain wrote at the record's DAA; `None`
/// for any other kind, a finding that restated an earlier void, the kind-3 records a DA default
/// writes on its covering signers (their claim voids `ProducerWithholding`, and the producer is
/// charged by the `DaDefault` record itself), or a producer that is the accused itself.
pub fn palw_capacity_false_valid_producer_v1(
    offence: &PalwConsumedOffenceV1,
    claim: Option<&PalwClaimStateV2>,
    liability: Option<&PalwPanelLiabilityRecordV1>,
) -> Option<PalwBondKeyV2> {
    if offence.kind != PalwOffenceKindV1::PanelFalseValidV2 || offence.claim_id == Hash64::default() {
        return None;
    }
    let (voided_daa, reason) = conviction_void(claim, liability)?;
    if voided_daa != offence.accepted_daa || reason != PalwVoidReasonV2::CourtFraud {
        return None;
    }
    let producer = claim.map(|c| c.bond).or_else(|| liability.map(|row| row.executor_bond))?;
    (producer.0 != offence.accused).then_some(producer)
}

/// Does the attempt claim hold a commitment under the new rule at `now_daa` (non-terminal, or voided
/// within `h_obl` for a reason E-4 holds)?
fn holds_new_rule(claim: &PalwClaimStateV2, h_obl: u64, now_daa: u64) -> bool {
    match &claim.phase {
        PalwClaimPhaseV2::Final { .. } => false,
        PalwClaimPhaseV2::Voided { voided_daa, reason } => {
            palw_capacity_void_reason_keeps_obligation_v1(*reason) && palw_capacity_void_holds_v1(*voided_daa, h_obl, now_daa)
        }
        _ => true,
    }
}

/// Does lane escrow's bind reserve the lock for a seat on `claim`? An attributable attempt with an
/// escrow (`palw_escrow_bind_reserves_the_lock_v1` under the as-if rule).
fn bind_reserves_the_lock(params: &PalwStateParamsV2, state: &PalwChainStateV2, claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::Attempt)
        && claim.escrowed_reward > 0
        && !palw_rcore_class_is_c7_v1(params, state, &claim.class_id)
}

fn latency_bucket(latency: u64) -> usize {
    PALW_CAPACITY_LATENCY_BUCKETS_V1.iter().position(|upper| latency < *upper).unwrap_or(PALW_CAPACITY_LATENCY_BUCKETS_V1.len())
}

#[derive(Default)]
struct BondFreeze {
    first: Option<u64>,
    last: u64,
    count: u64,
    intent: bool,
    undetermined: bool,
}

#[derive(Default)]
struct BondAcc {
    own_today: u128,
    own_new: Vec<u128>,
    raw_immature: u128,
    x_b: u128,
    reserved_new_total: u128,
    live: u64,
    unlicensed: u64,
    duties_today: u128,
    duties_new: Vec<u128>,
}

/// **The shadow with every input** (see [`PalwCapacityShadowOptionsV1`]).
pub fn palw_capacity_shadow_with_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now_daa: u64,
    options: &PalwCapacityShadowOptionsV1,
) -> PalwCapacityShadowV1 {
    let steps: Vec<PalwCapacityStepV1> =
        if options.steps.is_empty() { palw_capacity_display_steps_v1(None) } else { options.steps.clone() };
    let n_steps = steps.len();
    let h_obl = palw_capacity_h_obl_v1(params.window_receipt());
    let window_court = params.window_court();
    let ratio = params.fp_max_exposure_ratio_permille();
    let min_collateral = params.min_collateral_sompi();
    let mut adversary: BTreeMap<PalwBondKeyV2, PalwCapacityStrategyV1> = BTreeMap::new();
    for named in &options.adversaries {
        adversary.entry(named.bond).or_insert(named.strategy);
    }

    // ---- the reference floor claim ----------------------------------------------------------
    // E is the same for every class (720‰ of the subsidy): the newest attempt claim's, or the
    // carve of the tip's subsidy when the state holds none.
    let newest_attempt = state
        .claims_iter()
        .filter(|(_, c)| matches!(c.source, PalwClaimSourceV2::Attempt) && c.escrowed_reward > 0)
        .max_by_key(|(id, c)| (c.accepted_daa, **id));
    let reference_escrow: u128 = match (newest_attempt, options.reference_escrow_sompi) {
        (Some((_, claim)), _) => u128::from(claim.escrowed_reward),
        (None, Some(escrow)) => u128::from(escrow),
        (None, None) => {
            u128::from(crate::palw_state_v2::palw_claim_escrow_v1(params, state.last_point().map(|p| p.subsidy).unwrap_or(0), None))
        }
    };
    let reference_w_floor = PALW_CAPACITY_W_FCW_SOMPI_V1;
    let reference_l = palw_capacity_conviction_l_v1(reference_escrow.saturating_add(reference_w_floor));
    let reference_floor = palw_capacity_conviction_floor_v1(min_collateral, reference_escrow);
    let floor_class = params.base_class_id();

    // ---- the producers' conviction records, per claim (A8's routes) --------------------------
    // `(route, DAA)` of every record that convicts a claim's OWN producer, and the claims a
    // seat-only record names. Only claims the state holds are measured.
    let mut producer_convictions: BTreeMap<Hash64, Vec<(PalwCapacityRouteV1, u64)>> = BTreeMap::new();
    let mut seat_only_claims: BTreeSet<Hash64> = BTreeSet::new();
    for (_, offence) in state.consumed_offences_iter() {
        if offence.claim_id == Hash64::default() {
            continue;
        }
        let Some(claim) = state.claim(&offence.claim_id) else { continue };
        let liability = state.panel_liability(&offence.claim_id);
        let producer_charged = if offence.kind == PalwOffenceKindV1::PanelFalseValidV2 {
            palw_capacity_false_valid_producer_v1(offence, Some(claim), liability) == Some(claim.bond)
        } else {
            PalwBondKeyV2(offence.accused) == claim.bond
        };
        if producer_charged {
            let route = palw_capacity_route_of_record_v1(offence, Some(claim), liability);
            producer_convictions.entry(offence.claim_id).or_default().push((route, offence.accepted_daa));
        } else {
            seat_only_claims.insert(offence.claim_id);
        }
    }

    // ---- pass 1: claims, grouped by bond in acceptance order ---------------------------------
    let mut by_bond: BTreeMap<PalwBondKeyV2, Vec<(u64, Hash64)>> = BTreeMap::new();
    let mut attribution: BTreeMap<Hash64, PalwCapacityClassAttributionV1> = BTreeMap::new();
    let mut adversary_rows: BTreeMap<(Hash64, PalwCapacityStrategyV1), PalwCapacityAdversaryRowV1> = BTreeMap::new();
    let mut unpriced: BTreeMap<(Hash64, PalwCapacityStrategyV1), BTreeMap<PalwCapacityRouteV1, u64>> = BTreeMap::new();
    let mut credited_classes: BTreeSet<Hash64> = state
        .classes_iter()
        .filter(|(id, class)| matches!(class.status, PalwClassStatusV2::Active) && !palw_rcore_class_is_c7_v1(params, state, id))
        .map(|(id, _)| *id)
        .collect();
    let mut voids: BTreeMap<Hash64, BTreeMap<String, u64>> = BTreeMap::new();
    let mut licence_queue = 0u64;
    let mut licence_queue_oldest: Option<u64> = None;
    let mut licensed_recent = 0u64;
    for (id, claim) in state.claims_iter() {
        let c7 = palw_rcore_class_is_c7_v1(params, state, &claim.class_id);
        if matches!(claim.source, PalwClaimSourceV2::Attempt) && !c7 {
            credited_classes.insert(claim.class_id);
        }
        let class = attribution
            .entry(claim.class_id)
            .or_insert_with(|| PalwCapacityClassAttributionV1 { class_id: claim.class_id, ..Default::default() });
        match &claim.phase {
            PalwClaimPhaseV2::Final { .. } => class.claims_final += 1,
            PalwClaimPhaseV2::Voided { reason, .. } => {
                class.claims_voided += 1;
                if void_is_attributed(*reason) {
                    class.voids_attributed += 1;
                }
                *voids.entry(claim.class_id).or_default().entry(format!("{reason:?}")).or_default() += 1;
            }
            PalwClaimPhaseV2::PanelBound { bound_daa } => {
                class.claims_live += 1;
                licence_queue += 1;
                licence_queue_oldest = Some(licence_queue_oldest.map_or(*bound_daa, |d| d.min(*bound_daa)));
            }
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                class.claims_live += 1;
                if now_daa.saturating_sub(*licensed_daa) <= PALW_CAPACITY_SHADOW_RECENT_DAA_V1 {
                    licensed_recent += 1;
                }
            }
            _ => class.claims_live += 1,
        }
        if let Some(da) = state.da_claim(id) {
            class.da_opened_non_seat_total += u64::from(da.opened_non_seat_total);
        }
        // The credit prices attempt claims (a free-prompt claim carries no `E`, E-6): only they measure q.
        if let Some(strategy) = adversary.get(&claim.bond).filter(|_| matches!(claim.source, PalwClaimSourceV2::Attempt)) {
            let key = (claim.class_id, *strategy);
            let row = adversary_rows.entry(key).or_insert_with(|| PalwCapacityAdversaryRowV1 {
                class_id: claim.class_id,
                strategy: *strategy,
                c7,
                ..Default::default()
            });
            row.claims += 1;
            if seat_only_claims.contains(id) {
                row.seat_only += 1;
            }
            let records = producer_convictions.get(id).map(Vec::as_slice).unwrap_or(&[]);
            match palw_capacity_adversary_outcome_v1(claim, records, window_court) {
                PalwCapacityOutcomeV1::Caught => row.caught += 1,
                PalwCapacityOutcomeV1::CaughtLate => row.caught_late += 1,
                PalwCapacityOutcomeV1::CaughtUnpriced(route) => {
                    row.caught_unpriced += 1;
                    *unpriced.entry(key).or_default().entry(route).or_default() += 1;
                }
                PalwCapacityOutcomeV1::Undetected => row.undetected += 1,
                PalwCapacityOutcomeV1::Censored => row.censored += 1,
                PalwCapacityOutcomeV1::InFlight => row.in_flight += 1,
            }
        }
        by_bond.entry(claim.bond).or_default().push((claim.accepted_daa, *id));
    }

    // ---- pass 2: per bond, the budget in acceptance order, and every claim row --------------
    let mut claims_out: Vec<PalwCapacityClaimShadowV1> = Vec::new();
    let mut accs: BTreeMap<PalwBondKeyV2, BondAcc> = BTreeMap::new();
    // Per claim: the commitment each step's duty is capped by (bind-time commitment′).
    let mut bind_commitment_new: BTreeMap<Hash64, Vec<u128>> = BTreeMap::new();
    let mut claims_commitment_today = 0u128;
    let mut claims_commitment_new = vec![0u128; n_steps];
    for (bond, mut ids) in by_bond {
        ids.sort_unstable();
        let acc = accs.entry(bond).or_insert_with(|| BondAcc {
            own_new: vec![0; n_steps],
            duties_new: vec![0; n_steps],
            ..Default::default()
        });
        for (_, id) in ids {
            let Some(claim) = state.claim(&id) else { continue };
            let free_prompt = matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. });
            let c7 = palw_rcore_class_is_c7_v1(params, state, &claim.class_id);
            let stage = palw_capacity_stage_of_claim_v1(claim);
            let w_full = palw_capacity_weight_full_v1(claim.immature_contribution, c7);
            let staged_w = palw_capacity_staged_weight_v1(stage, w_full);
            let escrow_term_today = params.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
            let commitment_today = palw_claim_commitment_v1(params, claim, now_daa).unwrap_or(0);
            let l_sompi = palw_capacity_conviction_l_v1(palw_claim_g_v1(state, &id).map(|g| g.g()).unwrap_or(0));
            let holds = holds_new_rule(claim, h_obl, now_daa);
            // Stage 1 (rcore/cap-s1): the fold reserves `⌈w / ρ⌉` at the step's ρ, a function of the
            // claim alone (no budget, no held reservation); a void inside its hold keeps the reservation
            // it was priced at in its commitment.
            let reserved_new_at = |rho: u32| -> u128 {
                if free_prompt {
                    claim.reserved
                } else if holds {
                    palw_capacity_consensus_reservation_s1_v1(claim.reserved, rho)
                } else {
                    0
                }
            };
            let reserved_new = reserved_new_at(steps.first().map_or(1, |step| step.rho));
            let e = u128::from(claim.escrowed_reward);
            let floor = palw_capacity_conviction_floor_v1(min_collateral, e);
            let mut m_new = Vec::with_capacity(n_steps);
            let mut commitment_new = Vec::with_capacity(n_steps);
            let mut bind_new = Vec::with_capacity(n_steps);
            for step in &steps {
                if free_prompt {
                    // E-6: the FP lane is unchanged.
                    m_new.push(0);
                    commitment_new.push(commitment_today);
                    bind_new.push(crate::palw_state_v2::palw_claim_bond_reservation_v1(params, claim).unwrap_or(0));
                    continue;
                }
                // Lane escrow's term: the q_seat gate, m* on the conviction floor, never above E.
                let m = palw_capacity_m_c_v2(e, Some(step), !c7, floor, ratio);
                let reserved_new = reserved_new_at(step.rho);
                let full = m.saturating_add(reserved_new);
                let commitment = match &claim.phase {
                    PalwClaimPhaseV2::Final { .. } => 0,
                    PalwClaimPhaseV2::Voided { .. } => {
                        if holds {
                            full
                        } else {
                            0
                        }
                    }
                    _ => {
                        if claim.rcore.escrow_released {
                            reserved_new
                        } else {
                            full
                        }
                    }
                };
                m_new.push(m);
                commitment_new.push(commitment);
                bind_new.push(full);
            }
            let live = commitment_today > 0 || commitment_new.iter().any(|c| *c > 0) || stage.is_provisional();
            if !claim.phase.is_terminal() {
                acc.raw_immature = acc.raw_immature.saturating_add(claim.immature_contribution);
            }
            if !free_prompt && !claim.phase.is_terminal() {
                acc.reserved_new_total = acc.reserved_new_total.saturating_add(reserved_new);
            }
            if stage.is_provisional() {
                acc.x_b = acc.x_b.saturating_add(staged_w);
            }
            acc.own_today = acc.own_today.saturating_add(commitment_today);
            claims_commitment_today = claims_commitment_today.saturating_add(commitment_today);
            for (i, c) in commitment_new.iter().enumerate() {
                acc.own_new[i] = acc.own_new[i].saturating_add(*c);
                claims_commitment_new[i] = claims_commitment_new[i].saturating_add(*c);
            }

            bind_commitment_new.insert(id, bind_new);
            if !live {
                continue;
            }
            acc.live += 1;
            if palw_capacity_is_unlicensed_v1(claim) {
                acc.unlicensed += 1;
            }
            claims_out.push(PalwCapacityClaimShadowV1 {
                claim_id: id,
                bond,
                class_id: claim.class_id,
                phase: phase_name(&claim.phase),
                stage,
                accepted_daa: claim.accepted_daa,
                free_prompt,
                c7,
                raw_w: claim.immature_contribution,
                w_full,
                staged_w,
                reserved_today: claim.reserved,
                reserved_new,
                escrow_term_today,
                escrow_released: claim.rcore.escrow_released,
                commitment_today,
                l_sompi,
                m_new,
                commitment_new,
            });
        }
    }

    // ---- seat duties: every row, repriced per step -------------------------------------------
    let mut duty_rows = 0u64;
    let mut duty_rows_capped = 0u64;
    let mut seat_duty_total_today = 0u128;
    let mut seat_duty_total_new = vec![0u128; n_steps];
    // (seat, claim) → what the seat reserves per step, for the lock excess.
    let mut duty_of: BTreeMap<(PalwBondKeyV2, Hash64), Vec<u128>> = BTreeMap::new();
    let mut floor_duty_sum = 0u128;
    let mut floor_duty_rows = 0u128;
    let mut floor_seat_sum = 0u128;
    for (claim_id, row) in state.panel_duty_rows_iter() {
        let seats = row.seats.len();
        if seats == 0 {
            continue;
        }
        duty_rows += 1;
        let d = row.seat_exposure;
        let claim = state.claim(claim_id);
        let commitment_today_at_bind =
            claim.and_then(|c| crate::palw_state_v2::palw_claim_bond_reservation_v1(params, c)).unwrap_or(u128::MAX);
        if d >= commitment_today_at_bind / seats as u128 {
            duty_rows_capped += 1;
        }
        if claim.is_some_and(|c| c.class_id == floor_class) {
            floor_duty_sum = floor_duty_sum.saturating_add(d);
            floor_duty_rows += 1;
            floor_seat_sum += seats as u128;
        }
        let bind_new = bind_commitment_new.get(claim_id);
        // Lane liab's AS-1: the λ-term divides, the lock term is AS-2's lock of the claim's lock_2;
        // lane escrow's bind: an attributable attempt's seat reserves max(duty′, lock′).
        let free_prompt = claim.is_some_and(|c| matches!(c.source, PalwClaimSourceV2::FreePrompt { .. }));
        let reserves_lock = claim.is_some_and(|c| bind_reserves_the_lock(params, state, c));
        let (lambda_term, lock_2) = if free_prompt {
            (0, d)
        } else {
            (d, palw_claim_g_v1(state, claim_id).map(|g| palw_rcore_lock_v1(g.g_res, g.escrowed_reward, 0, 2)).unwrap_or(0))
        };
        let per_step: Vec<u128> = steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let commitment = bind_new.map(|v| v[i]).unwrap_or(commitment_today_at_bind);
                let duty = palw_capacity_seat_duty_liab_v1(lambda_term, lock_2, commitment, seats, Some(step));
                palw_capacity_seat_bind_reservation_v1(duty, palw_capacity_seat_lock_liab_v1(lock_2, Some(step)), reserves_lock)
            })
            .collect();
        seat_duty_total_today = seat_duty_total_today.saturating_add(d.saturating_mul(seats as u128));
        for (i, duty) in per_step.iter().enumerate() {
            seat_duty_total_new[i] = seat_duty_total_new[i].saturating_add(duty.saturating_mul(seats as u128));
        }
        for seat in row.seats.keys() {
            let acc = accs.entry(*seat).or_insert_with(|| BondAcc {
                own_new: vec![0; n_steps],
                duties_new: vec![0; n_steps],
                ..Default::default()
            });
            acc.duties_today = acc.duties_today.saturating_add(d);
            for (i, duty) in per_step.iter().enumerate() {
                acc.duties_new[i] = acc.duties_new[i].saturating_add(*duty);
            }
            duty_of.insert((*seat, *claim_id), per_step.clone());
        }
    }

    // ---- per bond: A-1 today and per step, N_instant, the freeze ------------------------------
    let mut freezes: BTreeMap<PalwBondKeyV2, BondFreeze> = BTreeMap::new();
    let mut convictions_total = 0u64;
    let mut kinds: BTreeMap<Hash64, BTreeMap<u8, u64>> = BTreeMap::new();
    for (_, offence) in state.consumed_offences_iter() {
        convictions_total += 1;
        let named = offence.claim_id != Hash64::default();
        let claim = named.then(|| state.claim(&offence.claim_id)).flatten();
        let liability = named.then(|| state.panel_liability(&offence.claim_id)).flatten();
        let class = palw_capacity_conviction_freeze_class_v1(offence, claim, liability);
        let charged =
            std::iter::once(PalwBondKeyV2(offence.accused)).chain(palw_capacity_false_valid_producer_v1(offence, claim, liability));
        for bond in charged {
            let freeze = freezes.entry(bond).or_default();
            freeze.first = Some(freeze.first.map_or(offence.accepted_daa, |d| d.min(offence.accepted_daa)));
            freeze.last = freeze.last.max(offence.accepted_daa);
            freeze.count += 1;
            match class {
                PalwCapacityFreezeClassV1::Intent => freeze.intent = true,
                PalwCapacityFreezeClassV1::Undetermined => freeze.undetermined = true,
                PalwCapacityFreezeClassV1::Tier => {}
            }
        }
        let class_id = claim.map(|c| c.class_id).unwrap_or_default();
        *kinds.entry(class_id).or_default().entry(offence.kind as u8).or_default() += 1;
        let class = attribution.entry(class_id).or_insert_with(|| PalwCapacityClassAttributionV1 { class_id, ..Default::default() });
        if let Some(claim) = claim {
            class.conviction_latency_histogram[latency_bucket(offence.accepted_daa.saturating_sub(claim.accepted_daa))] += 1;
        }
    }
    for ((claim_id, _), session) in state.da_sessions_iter() {
        let class_id = state.claim(claim_id).map(|c| c.class_id).unwrap_or_default();
        let class = attribution.entry(class_id).or_insert_with(|| PalwCapacityClassAttributionV1 { class_id, ..Default::default() });
        if session.accuser_is_seat {
            class.da_open_seat += 1;
        } else {
            class.da_open_non_seat += 1;
        }
    }

    let escaped_depth = palw_second_clock_depth_v1(options.raw_depth, state.recent_anchor_daas(), now_daa, window_court);
    let settled_now = state.settled_attempt_finals();
    let mut bonds_out: Vec<PalwCapacityBondShadowV1> = Vec::new();
    let mut seat_lock_total_today = 0u128;
    let mut seat_lock_total_new = vec![0u128; n_steps];
    let mut floor_lock_sum = 0u128;
    let mut floor_lock_count = 0u128;
    let mut seats = 0u64;
    let mut seat_usable_capital = 0u128;
    let mut committed_today_total = 0u128;
    let mut committed_new_total = vec![0u128; n_steps];
    let mut bounded_immature_new = 0u128;
    let mut w_cap_total = 0u128;
    let reference_steps_m: Vec<u128> =
        steps.iter().map(|step| palw_capacity_m_c_v2(reference_escrow, Some(step), true, reference_floor, ratio)).collect();
    let empty_acc = BondAcc { own_new: vec![0; n_steps], duties_new: vec![0; n_steps], ..Default::default() };
    for (bond, record) in state.bonds_iter() {
        let acc = accs.get(bond).unwrap_or(&empty_acc);
        let collateral = record.collateral;
        let ceiling = u128::from(collateral).saturating_mul(u128::from(ratio)) / 1_000;
        let seat = matches!(record.status, PalwBondStatusV2::Active) && !record.capable_classes.is_empty();
        if seat {
            seats += 1;
            seat_usable_capital = seat_usable_capital.saturating_add(ceiling);
        }
        // Locks: today's excess over the duty, and each step's over the seat's reservation.
        let mut locks_excess_today = 0u128;
        let mut locks_excess_new = vec![0u128; n_steps];
        for ((_, claim_id), lock) in state.slashable_locks_of(bond) {
            let claim = state.claim(claim_id);
            let live =
                lock.is_live_v3(now_daa, settled_now, escaped_depth, window_court) || claim.is_some_and(|c| !c.phase.is_terminal());
            if !live {
                continue;
            }
            let duty_today =
                state.panel_duty_row_of(claim_id).filter(|r| r.seats.contains_key(bond)).map(|r| r.seat_exposure).unwrap_or(0);
            locks_excess_today = locks_excess_today.saturating_add(lock.amount.saturating_sub(duty_today));
            seat_lock_total_today = seat_lock_total_today.saturating_add(lock.amount);
            if claim.is_some_and(|c| c.class_id == floor_class) {
                floor_lock_sum = floor_lock_sum.saturating_add(lock.amount);
                floor_lock_count += 1;
            }
            let duties = duty_of.get(&(*bond, *claim_id));
            for (i, step) in steps.iter().enumerate() {
                // Lane liab's AS-2 (q_seat 250‰, whatever the claim's E and G), not D-5's column.
                let lock_new = palw_capacity_seat_lock_liab_v1(lock.amount, Some(step));
                let duty_new = duties.map(|v| v[i]).unwrap_or(0);
                locks_excess_new[i] = locks_excess_new[i].saturating_add(lock_new.saturating_sub(duty_new));
                seat_lock_total_new[i] = seat_lock_total_new[i].saturating_add(lock_new);
            }
        }
        let committed_today = palw_bond_committed_raw_v1(state, params, bond, now_daa, options.raw_depth);
        let registration = state.registration_exposure(bond);
        let other_reserved = state.reserved_exposure(bond).saturating_sub(acc.own_today).saturating_sub(acc.duties_today);
        let accuser = palw_accuser_exposure_v1(state, bond);
        let committed_new: Vec<u128> = (0..n_steps)
            .map(|i| {
                acc.own_new[i]
                    .saturating_add(acc.duties_new[i])
                    .saturating_add(other_reserved)
                    .saturating_add(registration)
                    .saturating_add(locks_excess_new[i])
            })
            .collect();
        let room = |committed: u128| palw_rcore_gate_room_of_v1(collateral, ratio, committed, accuser, PalwRcoreGateV1::Work);
        let per_claim_today = reference_escrow.saturating_add(reference_w_floor);
        let n_instant_today = palw_capacity_n_instant_v1(room(committed_today.saturating_sub(acc.own_today)), per_claim_today, 0, 0);
        let n_more_today = palw_capacity_n_instant_v1(room(committed_today), per_claim_today, 0, 0);
        // The lane's R_budget, withdrawn at stage 1 and shown for its history.
        let r_budget = palw_capacity_weight_budget_sompi_v1(collateral);
        // Stage 1: each claim reserves `⌈w / ρ⌉` (no budget), so N identical claims cost N·(m + ⌈w/ρ⌉).
        let n_instant_new: Vec<u64> = (0..n_steps)
            .map(|i| {
                palw_capacity_n_instant_v1(
                    room(committed_new[i].saturating_sub(acc.own_new[i])),
                    reference_steps_m[i],
                    palw_capacity_consensus_reservation_s1_v1(reference_w_floor, steps[i].rho),
                    u128::MAX,
                )
            })
            .collect();
        let n_more_new: Vec<u64> = (0..n_steps)
            .map(|i| {
                palw_capacity_n_instant_v1(
                    room(committed_new[i]),
                    reference_steps_m[i],
                    palw_capacity_consensus_reservation_s1_v1(reference_w_floor, steps[i].rho),
                    u128::MAX,
                )
            })
            .collect();
        let w_cap = palw_capacity_weight_cap_v1(collateral);
        let capped = palw_capacity_bond_weight_term_v1(acc.x_b, w_cap);
        bounded_immature_new = bounded_immature_new.saturating_add(capped);
        w_cap_total = w_cap_total.saturating_add(w_cap);
        committed_today_total = committed_today_total.saturating_add(committed_today);
        for (i, c) in committed_new.iter().enumerate() {
            committed_new_total[i] = committed_new_total[i].saturating_add(*c);
        }
        let freeze = freezes.get(bond);
        let (frozen_would_be, freeze_final, freeze_undetermined, first_conviction_daa, convictions) = match freeze {
            None => (false, false, false, None, 0),
            Some(f) => {
                let within = f.last.checked_add(window_court).is_none_or(|lift| now_daa < lift);
                (f.intent || within, f.intent, f.undetermined, f.first, f.count)
            }
        };
        bonds_out.push(PalwCapacityBondShadowV1 {
            bond: *bond,
            collateral,
            ceiling,
            seat,
            live_claims: acc.live,
            unlicensed_claims: acc.unlicensed,
            raw_immature_today: acc.raw_immature,
            w_cap,
            x_b: acc.x_b,
            capped,
            r_budget,
            reserved_new_total: acc.reserved_new_total,
            committed_today,
            own_claims_today: acc.own_today,
            committed_new,
            n_instant_today,
            n_instant_new,
            n_more_today,
            n_more_new,
            frozen_would_be,
            freeze_final,
            freeze_undetermined,
            first_conviction_daa,
            convictions,
        });
    }
    // A claim whose bond record is gone adds nothing under the cap: its `W_cap` (of no collateral) is 0.

    // ---- the reference floor claim's seat prices, and the §5.4 estimate per step ------------
    let reference_seats = if floor_duty_rows > 0 {
        u32::try_from(floor_seat_sum / floor_duty_rows).unwrap_or(PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1)
    } else {
        PALW_CAPACITY_SHADOW_DEFAULT_SEATS_V1
    }
    .max(1);
    let reference_duty = if floor_duty_rows > 0 {
        floor_duty_sum / floor_duty_rows
    } else {
        // λ binds a floor duty at `E/seats` (the harness's 640.17 MSK).
        reference_escrow / u128::from(reference_seats)
    };
    let reference_lock = if floor_lock_count > 0 {
        floor_lock_sum / floor_lock_count
    } else {
        // L-1 at k′ = 2 on the floor's residual gain `w`.
        palw_rcore_lock_v1(reference_w_floor, u64::try_from(reference_escrow).unwrap_or(u64::MAX), 0, 2)
    };
    let seat_capacity = |duty: u128, lock: u128| {
        palw_capacity_claims_per_daa_milli_v1(
            seat_usable_capital,
            palw_capacity_seat_capital_per_claim_v1(&PalwCapacitySeatCapitalInputsV1 {
                duty_seats: reference_seats,
                duty_sompi: duty,
                duty_hold_daa: PALW_CAPACITY_SEAT_DUTY_HOLD_DAA_V1,
                lock_seats: reference_seats,
                lock_sompi: lock,
                lock_hold_daa: window_court,
            }),
        )
    };
    let seat_capacity_today_milli_per_daa = seat_capacity(reference_duty, reference_lock);
    let block_mass = if options.block_mass_limit == 0 { 500_000 } else { options.block_mass_limit };
    let carriers_per_block = palw_capacity_carriers_per_block_v1(block_mass, PALW_CAPACITY_COVERAGE_CARRIER_MASS_V1);
    let carriage_blocks_to_drain = if carriers_per_block == 0 { u64::MAX } else { licence_queue.div_ceil(carriers_per_block) };

    // ---- attribution rows ---------------------------------------------------------------------
    let mut attribution_out: Vec<PalwCapacityClassAttributionV1> = attribution
        .into_values()
        .map(|mut row| {
            row.voids_by_reason = voids.remove(&row.class_id).map(|m| m.into_iter().collect()).unwrap_or_default();
            row.convictions_by_kind = kinds.remove(&row.class_id).map(|m| m.into_iter().collect()).unwrap_or_default();
            row
        })
        .collect();
    attribution_out.sort_by_key(|row| row.class_id);
    let adversary_out: Vec<PalwCapacityAdversaryRowV1> = adversary_rows
        .into_iter()
        .map(|(key, mut row)| {
            row.unpriced_by_route = unpriced
                .remove(&key)
                .map(|routes| routes.into_iter().map(|(route, n)| (route.name(), n)).collect())
                .unwrap_or_default();
            let resolved = row.caught + row.caught_late + row.caught_unpriced + row.undetected;
            row.q_measured_permille =
                (resolved > 0).then(|| u16::try_from(u128::from(row.caught) * 1_000 / u128::from(resolved)).unwrap_or(1_000));
            row
        })
        .collect();
    // Every (attributable class, named strategy) a credited step needs measured.
    let measured: BTreeSet<(Hash64, PalwCapacityStrategyV1)> =
        adversary_out.iter().filter(|row| row.q_measured_permille.is_some()).map(|row| (row.class_id, row.strategy)).collect();
    let unmeasured = credited_classes
        .iter()
        .any(|class_id| PalwCapacityStrategyV1::NAMED.iter().any(|strategy| !measured.contains(&(*class_id, *strategy))));

    let budget_13k = palw_capacity_weight_budget_sompi_v1(PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1);
    let ceiling_13k = u128::from(PALW_CAPACITY_SHADOW_REFERENCE_BOND_SOMPI_V1) * u128::from(ratio) / 1_000;
    let step_rows: Vec<PalwCapacityStepShadowV1> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let m_floor = reference_steps_m[i];
            let credit = palw_capacity_escrow_credit_applies_v1(step.q_credit_permille);
            // Lane liab's AS-1/AS-2 on the reference floor claim, lane escrow's reservation at bind
            // (it is an attributable attempt), and D-5's column (its lock in the duty's lock term).
            let commitment_new = m_floor.saturating_add(reference_w_floor);
            let seats_n = reference_seats as usize;
            let lock = palw_capacity_seat_lock_liab_v1(reference_lock, Some(step));
            let duty = palw_capacity_seat_duty_liab_v1(reference_duty, reference_lock, commitment_new, seats_n, Some(step));
            let reservation = palw_capacity_seat_bind_reservation_v1(duty, lock, true);
            let lock_if_d5 =
                palw_capacity_seat_lock_v1(reference_lock, Some(step), reference_escrow, PALW_CAPACITY_SEAT_L_IF_D5_SOMPI_V1);
            let duty_if_d5 = palw_capacity_seat_duty_with_lock_v1(reference_duty, lock_if_d5, commitment_new, seats_n, step.rho);
            let reservation_if_d5 = palw_capacity_seat_bind_reservation_v1(duty_if_d5, lock_if_d5, true);
            // A8: the bar every attributable (class, strategy) measurement must clear.
            let q_needed_route = palw_capacity_q_needed_permille_v1(reference_escrow, step.rho, reference_l);
            let q_required = q_needed_route.max(PALW_CAPACITY_Q_SEAT_PERMILLE_V1).max(step.q_credit_permille).saturating_mul(2);
            let below = adversary_out.iter().filter(|row| !row.c7).filter_map(|row| row.q_measured_permille).any(|q| q < q_required);
            let q_alarm_unmeasured = step.q_credit_permille > 0 && unmeasured;
            // v1's superseded pricing, for its column only.
            let m_floor_v1 = palw_capacity_m_c_v1(reference_escrow, Some(step), true, reference_l);
            PalwCapacityStepShadowV1 {
                step: *step,
                m_floor,
                q_needed_permille: palw_capacity_q_needed_permille_v2(reference_escrow, step.rho, reference_floor, ratio),
                ramp_binds: credit && m_floor == palw_capacity_m_ramp_v1(reference_escrow, step.rho),
                seat_credit: palw_capacity_seat_credit_liab_v1(step.q_credit_permille),
                seat_credit_if_d5: palw_capacity_seat_credit_applies_v1(
                    step.q_credit_permille,
                    reference_escrow,
                    PALW_CAPACITY_SEAT_L_IF_D5_SOMPI_V1,
                ),
                seat_capacity_if_d5_milli_per_daa: seat_capacity(reservation_if_d5, lock_if_d5),
                claims_commitment_total: claims_commitment_new[i],
                committed_total: committed_new_total[i],
                seat_duty_total: seat_duty_total_new[i],
                seat_lock_total: seat_lock_total_new[i],
                seat_capacity_milli_per_daa: seat_capacity(reservation, lock),
                n_instant_13k: palw_capacity_n_instant_v1(
                    ceiling_13k,
                    m_floor,
                    palw_capacity_consensus_reservation_s1_v1(reference_w_floor, step.rho),
                    u128::MAX,
                ),
                m_floor_v1_superseded: m_floor_v1,
                n_instant_13k_v1_superseded: palw_capacity_n_instant_v1(ceiling_13k, m_floor_v1, reference_w_floor, budget_13k),
                q_needed_route_permille: q_needed_route,
                q_required_permille: q_required,
                q_alarm: below || q_alarm_unmeasured,
                q_alarm_unmeasured,
            }
        })
        .collect();

    PalwCapacityShadowV1 {
        now_daa,
        tip_daa: state.last_point().map(|p| p.daa_score).unwrap_or(0),
        steps: step_rows,
        claims: claims_out,
        bonds: bonds_out,
        bounded_immature_today: state.bounded_immature(),
        bounded_immature_new,
        safe_weight: state.safe_weight(),
        w_cap_total,
        reference_escrow,
        reference_w_floor,
        reference_l,
        reference_floor,
        reference_seats,
        reference_duty,
        reference_lock,
        claims_commitment_today,
        committed_today_total,
        seats,
        seat_usable_capital,
        seat_duty_total_today,
        seat_lock_total_today,
        seat_capacity_today_milli_per_daa,
        duty_rows,
        duty_rows_capped,
        licence_queue,
        licence_queue_oldest_bound_daa: licence_queue_oldest,
        licensed_recent,
        carriers_per_block,
        carriage_blocks_to_drain,
        attribution: attribution_out,
        adversary: adversary_out,
        credited_classes: credited_classes.into_iter().collect(),
        convictions_total,
    }
}

#[cfg(test)]
mod tests {
    //! **S-T1 / S-I4 (shadow half): the shadow on planted states prices every step as the fold would
    //! (lane escrow's term, lane liab's AS-1/AS-2, lane escrow's bind), keeps ADR-0160 v1's golden
    //! tables only in the superseded column, and measures `q` only on what the credit prices.**
    use super::*;
    use crate::palw_capacity_formulas_v1::{
        PALW_CAPACITY_C7_WEIGHT_CEILING_V1, PALW_CAPACITY_FCW_V1, PALW_CAPACITY_REFERENCE_STEPS_V1, palw_capacity_m_ramp_v1,
    };
    use crate::palw_panel_var_v1::PalwSlashableLockV1;
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwBondStateV2, PalwClaimRcoreV1, PalwDeltaEntryV2, PalwPanelDutyRowV1, PalwStateDeltaV2, apply_delta_v2,
    };
    use crate::palw_verification_v2::PalwSegmentMaskV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    const MSK: u64 = 100_000_000;
    /// Appendix A's E and w (sompi), and the raw weights.
    const E: u64 = 320_084_650_080;
    const W_FLOOR: u128 = 10_752_660;
    const W_2M: u128 = 5_974_294_206_820;
    const RAW_2M: u128 = 335_728_175_722_137;
    const NOW: u64 = 5_000;
    /// A genesis card: 939,063.21 MSK.
    const CARD: u64 = 93_906_321_000_000;
    /// `m_c` of a floor claim as the fold prices it on testnet-12 — rcore/cap-s1's floor `Tier(0)` (the
    /// user's decisions 1 + 2: a first DA default collects the commitment alone), κ 500‰ — at a credit
    /// of 250‰ (0.6E) and 500‰ (E/3), at every ρ.
    const M_AT_250: u128 = 192_050_790_048;
    const M_AT_500: u128 = 106_694_883_360;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond_key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB000 + n), 0))
    }
    fn floor() -> Hash64 {
        h(1)
    }
    fn two_m() -> Hash64 {
        h(3)
    }

    /// testnet-12's shape: β 100‰, window_receipt 600 (= h_obl), window_court 3,000, the 500‰
    /// ceiling, the 13,000 MSK producer floor, option A's escrow term from genesis, R-core+ from
    /// genesis with 2M in C7.
    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 600, 600, 120, 3_000, 1_000, floor(), 4, 1_000, 13_000 * MSK, 800, 600)
            .unwrap()
            .with_fp_exposure_ceiling(500)
            .unwrap()
            .with_escrow_backed_exposure_from_daa(Some(0))
            .with_rcore_plus_mirrors(Some(0), 0, vec![two_m()])
    }

    fn bond(n: u64, collateral: u64, seat: bool) -> PalwBondStateV2 {
        PalwBondStateV2 {
            pubkey: vec![n as u8; 4],
            operator_id: h(0x0B00 + n),
            collateral,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 1,
            payout_payload: h(0x9A00 + n),
            capable_classes: if seat { [floor()].into_iter().collect() } else { Default::default() },
        }
    }

    fn claim(
        producer: u64,
        class_id: Hash64,
        phase: PalwClaimPhaseV2,
        reserved: u128,
        raw: u128,
        accepted_daa: u64,
    ) -> PalwClaimStateV2 {
        PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id,
            bond: bond_key(producer),
            pwu: 1,
            accepted_daa,
            rebound_daa: None,
            accepted_blue_score: accepted_daa,
            accepted_block: h(0xB0),
            trace_root: h(0x71),
            output_root: h(0x72),
            execution_root: h(0xE0),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            reserved,
            immature_contribution: raw,
            escrowed_reward: E,
            work_leaves: 0,
            work_id: None,
            phase,
            rights_reserved: 0,
            job_identity: Hash64::default(),
            rcore: PalwClaimRcoreV1::default(),
        }
    }

    fn floor_claim(producer: u64, phase: PalwClaimPhaseV2, accepted_daa: u64) -> PalwClaimStateV2 {
        claim(producer, floor(), phase, W_FLOOR, PALW_CAPACITY_FCW_V1, accepted_daa)
    }

    fn voided(daa: u64, reason: PalwVoidReasonV2) -> PalwClaimPhaseV2 {
        PalwClaimPhaseV2::Voided { voided_daa: daa, reason }
    }

    fn offence(kind: PalwOffenceKindV1, accused: u64, daa: u64, claim_id: Hash64) -> PalwConsumedOffenceV1 {
        PalwConsumedOffenceV1 {
            kind,
            accused: bond_key(accused).0,
            amount: 1,
            accepted_daa: daa,
            execution_root: Hash64::default(),
            collected: 1,
            claim_id,
        }
    }

    fn adversary(n: u64, strategy: PalwCapacityStrategyV1) -> PalwCapacityAdversaryV1 {
        PalwCapacityAdversaryV1 { bond: bond_key(n), strategy }
    }

    /// A planted chain: bonds, claims (with each producer's `reserved_exposure` the sum of its
    /// claims' commitments plus its duties, as the fold keeps it), and `extra` entries.
    struct Plant {
        bonds: Vec<(u64, PalwBondStateV2)>,
        claims: Vec<(Hash64, PalwClaimStateV2)>,
        duties: Vec<(Hash64, PalwPanelDutyRowV1)>,
        extra: Vec<PalwDeltaEntryV2>,
    }

    impl Plant {
        fn new() -> Self {
            Self { bonds: Vec::new(), claims: Vec::new(), duties: Vec::new(), extra: Vec::new() }
        }
        fn bond(mut self, n: u64, collateral: u64, seat: bool) -> Self {
            self.bonds.push((n, bond(n, collateral, seat)));
            self
        }
        fn claim(mut self, id: u64, record: PalwClaimStateV2) -> Self {
            self.claims.push((h(id), record));
            self
        }
        fn record(mut self, key: u64, record: PalwConsumedOffenceV1) -> Self {
            self.extra.push(PalwDeltaEntryV2::ConsumedOffence { key: h(key), old: None, new: Some(record) });
            self
        }
        fn state(self) -> PalwChainStateV2 {
            let p = params();
            let mut entries: Vec<PalwDeltaEntryV2> = self
                .bonds
                .iter()
                .map(|(n, b)| PalwDeltaEntryV2::Bond { key: bond_key(*n), old: None, new: Some(b.clone()) })
                .collect();
            let mut exposure: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
            let mut immature = 0u128;
            for (id, c) in &self.claims {
                *exposure.entry(c.bond).or_default() += palw_claim_commitment_v1(&p, c, NOW).unwrap();
                if !c.phase.is_terminal() {
                    immature += c.immature_contribution;
                }
                entries.push(PalwDeltaEntryV2::Claim { key: *id, old: None, new: Some(c.clone()) });
            }
            for (id, row) in &self.duties {
                for seat in row.seats.keys() {
                    *exposure.entry(*seat).or_default() += row.seat_exposure;
                }
                entries.push(PalwDeltaEntryV2::PanelDuties { key: *id, old: None, new: Some(row.clone()) });
            }
            for (key, value) in exposure {
                entries.push(PalwDeltaEntryV2::Exposure { key, old: None, new: Some(value) });
            }
            entries.push(PalwDeltaEntryV2::Weights { old: (0, 0), new: (0, immature) });
            let point = PalwBlockContextV2 { block: h(0xB1), daa_score: NOW, blue_score: NOW, subsidy: 0 };
            entries.push(PalwDeltaEntryV2::LastPoint { old: None, new: Some(point) });
            entries.extend(self.extra);
            apply_delta_v2(&PalwChainStateV2::genesis(), &PalwStateDeltaV2 { point, entries }, &p).expect("the planted state folds")
        }
    }

    fn eight_cards(plant: Plant) -> Plant {
        (0..8u64).fold(plant, |p, i| p.bond(100 + i, CARD, true))
    }

    fn row_of(shadow: &PalwCapacityShadowV1, n: u64) -> &PalwCapacityBondShadowV1 {
        shadow.bonds.iter().find(|b| b.bond == bond_key(n)).expect("the bond's row")
    }

    fn claim_row(shadow: &PalwCapacityShadowV1, id: u64) -> Option<&PalwCapacityClaimShadowV1> {
        shadow.claims.iter().find(|c| c.claim_id == h(id))
    }

    /// `(claims, caught, late, unpriced, undetected, censored, in flight, q)` of one adversary row.
    fn counts(row: &PalwCapacityAdversaryRowV1) -> (u64, u64, u64, u64, u64, u64, u64, Option<u16>) {
        (
            row.claims,
            row.caught,
            row.caught_late,
            row.caught_unpriced,
            row.undetected,
            row.censored,
            row.in_flight,
            row.q_measured_permille,
        )
    }

    fn named(shadow: &PalwCapacityShadowV1, strategy: PalwCapacityStrategyV1) -> &PalwCapacityAdversaryRowV1 {
        shadow.adversary_row(&floor(), strategy).expect("the floor's row for the strategy")
    }

    /// **E-T3 and §5.2 on a planted chain, as the fold prices them** (review of lane shadow, round 2,
    /// finding 2). By default (no steps named, no F-L schedule) the shadow prices the UNCREDITED
    /// ramp: at `q = 0`, `m_c = E`, so a fresh 13k / 100k / 1M bond holds 2 / 15 / 156 floor claims
    /// at every ρ, and under lane liab's AS-1 the floor duty falls only to the undivided `lock_2`
    /// (640.17 → 240.13 MSK) — eight genesis cards go 0.94 → 1.00 floor claims/DAA at every ρ.
    /// Named, v1's reference ramp (q 143‰) prices EXACTLY the same — 143‰ is below `q_seat`, so lane
    /// escrow's gate keeps `m_c = E` and the locks stay — and its v1 figures (E-T3's 20 … 2,030) are
    /// in the superseded column only; with nothing measured, a credited step alarms. A credit of
    /// 250‰ opens the gate: `m_c` = 1,400.51 MSK on the `Tier(1,300)` floor (4 per 13k, not
    /// ×ρ), while the seat side divides by ρ (the locks at `q_seat`, the duty's λ-term).
    #[test]
    fn s_t1_fresh_bonds_hold_the_claim_counts_the_fold_gives() {
        let state = eight_cards(Plant::new())
            .bond(1, 13_000 * MSK, false)
            .bond(2, 100_000 * MSK, false)
            .bond(3, 1_000_000 * MSK, false)
            .bond(9, 1_000_000 * MSK, false)
            .claim(0x900, floor_claim(9, PalwClaimPhaseV2::Provisional, NOW - 3))
            .state();
        let near = |got: u64, want: u64| got.abs_diff(want) * 200 <= want;

        // ---- the default display: the uncredited ramp ----
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        assert_eq!(shadow.reference_escrow, u128::from(E), "E read off the newest attempt claim");
        // rcore/cap-s1 (decisions 1 + 2): the cheapest route's actual collection — a first DA default's 0.
        assert_eq!(shadow.reference_floor, PalwCapacityConvictionFloorV1::Tier(0), "the fold's floor: Tier(0)");
        assert_eq!(
            shadow.steps.iter().map(|s| (s.step.rho, s.step.q_credit_permille)).collect::<Vec<_>>(),
            vec![(10, 0), (25, 0), (50, 0), (100, 0), (1000, 0)],
            "no steps named and no F-L schedule: the uncredited ramp, never the reference one"
        );
        for (n, today) in [(1u64, 2u64), (2, 15), (3, 156)] {
            let row = row_of(&shadow, n);
            assert_eq!(row.n_instant_today, today, "bond {n} today");
            assert_eq!(row.n_instant_new, vec![today; 5], "bond {n}: at q = 0, m_c = E at every ρ");
            assert_eq!(row.committed_today, 0);
        }
        assert_eq!(shadow.steps.iter().map(|s| s.n_instant_13k).collect::<Vec<_>>(), vec![2; 5]);
        assert!(shadow.steps.iter().all(|s| s.m_floor == u128::from(E) && !s.ramp_binds && !s.seat_credit && !s.seat_credit_if_d5));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm && s.q_required_permille == 500), "uncredited: nothing to back, the bar 500‰");
        // §5.4: 8 cards × 469,531.6 MSK against a floor claim's duty and lock — 0.94/DAA today.
        assert_eq!(shadow.seats, 8);
        assert_eq!(shadow.seat_usable_capital, 8 * u128::from(CARD) / 2);
        assert_eq!(shadow.reference_duty, u128::from(E) / 5, "λ binds the floor duty at E/5 = 640.17 MSK");
        let lock_msk = shadow.reference_lock / u128::from(MSK);
        assert!((239..=241).contains(&lock_msk), "the floor's L-1 lock at k′ = 2 is ≈ 240.1 MSK, got {lock_msk}");
        assert!(near(shadow.seat_capacity_today_milli_per_daa, 940), "{}", shadow.seat_capacity_today_milli_per_daa);
        // Lane liab's AS-1 at q 0: the duty is the undivided lock_2 (240.13 MSK) at every ρ ≥ 3,
        // so 3,756,252.8 MSK / (5 × 240.13 × (122 + 3,000)) = 1.002/DAA.
        for s in &shadow.steps {
            assert!(
                near(s.seat_capacity_milli_per_daa, 1_002),
                "ρ {}: seat capacity {} vs 1,002",
                s.step.rho,
                s.seat_capacity_milli_per_daa
            );
            assert_eq!(s.seat_capacity_if_d5_milli_per_daa, s.seat_capacity_milli_per_daa, "q = 0 is below D-5's 25‰ too");
        }
        assert_eq!((shadow.carriers_per_block, shadow.licence_queue, shadow.carriage_blocks_to_drain), (3, 0, 0));
        let line = shadow.summary();
        assert!(line.contains("N13k[ρ@q‰]=10@0:2,25@0:2,50@0:2,100@0:2,1000@0:2"), "{line}");
        assert!(line.contains("seatcap[ρ@q‰]=10@0:1.00,") && !line.contains("ALARM"), "{line}");

        // ---- v1's reference ramp, named: below q_seat nothing is credited (×1) ----
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &PALW_CAPACITY_REFERENCE_STEPS_V1);
        assert_eq!(shadow.steps.iter().map(|s| s.step.rho).collect::<Vec<_>>(), vec![10, 25, 50, 100, 1000]);
        for (n, today) in [(1u64, 2u64), (2, 15), (3, 156)] {
            let row = row_of(&shadow, n);
            assert_eq!(row.n_instant_new, vec![today; 5], "bond {n}: 143‰ < q_seat, m_c = E");
            assert_eq!(row.n_more_new, row.n_instant_new, "a fresh bond can open all of them now");
        }
        assert!(shadow.steps.iter().all(|s| s.m_floor == u128::from(E) && s.n_instant_13k == 2 && !s.ramp_binds && !s.seat_credit));
        // The superseded (v1, L = 3G) column keeps E-T3: v1 priced 143‰ as ⌈E/ρ⌉.
        assert_eq!(shadow.steps.iter().map(|s| s.n_instant_13k_v1_superseded).collect::<Vec<_>>(), vec![20, 50, 101, 203, 2_030]);
        assert_eq!(
            shadow.steps.iter().map(|s| s.m_floor_v1_superseded).collect::<Vec<_>>(),
            [10u32, 25, 50, 100, 1000].map(|rho| palw_capacity_m_ramp_v1(u128::from(E), rho)).to_vec()
        );
        // What the fold needs for ⌈E/ρ⌉ (rcore/cap-s1's Tier(0): decisions 1 + 2), and what the priced
        // routes need at 3G.
        assert_eq!(shadow.steps.iter().map(|s| s.q_needed_permille).collect::<Vec<_>>(), vec![819, 924, 961, 981, 999]);
        assert_eq!(shadow.steps.iter().map(|s| s.q_needed_route_permille).collect::<Vec<_>>(), vec![131, 138, 141, 142, 143]);
        // No seat credit: the duty is lock_2 (the credit no longer cuts the commitment under it) —
        // ≈ 1.00/DAA, not v1's 1.03–1.04; D-5's column (L_seat = 130k, 25‰) divides both, ×ρ.
        for s in &shadow.steps {
            assert!(near(s.seat_capacity_milli_per_daa, 1_002), "ρ {}: {}", s.step.rho, s.seat_capacity_milli_per_daa);
            assert!(s.seat_credit_if_d5);
        }
        for (s, want) in shadow.steps.iter().zip([9_409u64, 23_523, 47_047, 94_094, 940_944]) {
            assert!(
                near(s.seat_capacity_if_d5_milli_per_daa, want),
                "D-5 seat capacity {} vs {want}",
                s.seat_capacity_if_d5_milli_per_daa
            );
        }
        // Credited (143‰ > 0) and nothing measured: A8 alarms on every step.
        assert_eq!(shadow.credited_classes, vec![floor()]);
        assert!(shadow.steps.iter().all(|s| s.q_alarm && s.q_alarm_unmeasured && s.q_required_permille == 500));
        let line = shadow.summary();
        assert!(line.contains("N13k[ρ@q‰]=10@143:2,25@143:2,50@143:2,100@143:2,1000@143:2") && line.ends_with("q-ALARM"), "{line}");

        // ---- a credit of q_seat (250‰): the gate opens; m_c on rcore/cap-s1's Tier(0) floor, locks ÷ρ ----
        let credited: Vec<PalwCapacityStepV1> =
            PALW_CAPACITY_REFERENCE_STEPS_V1.iter().map(|s| PalwCapacityStepV1 { q_credit_permille: 250, ..*s }).collect();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &credited);
        assert!(shadow.steps.iter().all(|s| s.seat_credit && s.seat_credit_if_d5 && !s.ramp_binds));
        assert!(shadow.steps.iter().all(|s| s.m_floor == M_AT_250 && s.n_instant_13k == 3), "1,920.51 MSK: 3 per 13k at every ρ");
        assert_eq!(row_of(&shadow, 3).n_instant_new, vec![260; 5], "1M: ⌊(500,000 − R_budget) / 1,920.51⌋");
        for (s, want) in shadow.steps.iter().zip([9_409u64, 23_523, 47_047, 94_094, 940_944]) {
            assert!(near(s.seat_capacity_milli_per_daa, want), "seat capacity {} vs {want}", s.seat_capacity_milli_per_daa);
        }
        // q 500‰, the most D-8 can ever credit: E/3 = 1,066.95 MSK, 6 per 13k, still at every ρ.
        let most: Vec<PalwCapacityStepV1> =
            PALW_CAPACITY_REFERENCE_STEPS_V1.iter().map(|s| PalwCapacityStepV1 { q_credit_permille: 500, ..*s }).collect();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &most);
        assert!(shadow.steps.iter().all(|s| s.m_floor == M_AT_500 && s.n_instant_13k == 6 && s.q_required_permille == 1_000));
    }

    /// The default steps: F-L's armed schedule when the params carry one, else the uncredited ramp.
    #[test]
    fn the_display_steps_are_the_armed_schedule_else_uncredited() {
        assert_eq!(palw_capacity_display_steps_v1(None), PALW_CAPACITY_UNCREDITED_STEPS_V1.to_vec());
        assert_eq!(palw_capacity_display_steps_v1(Some(&[])), PALW_CAPACITY_UNCREDITED_STEPS_V1.to_vec());
        let armed = [PalwCapacityStepV1 { from_daa: 1_200, rho: 10, q_credit_permille: 0 }];
        assert_eq!(palw_capacity_display_steps_v1(Some(&armed)), armed.to_vec(), "testnet-12's first step, as armed");
    }

    /// **W-I1 in the shadow: claims × N is not fork power × N.** A 13k bond's licensed floor claims
    /// weigh 1 FCW each today; under J-1 the bond's term is at most 2 FCW at every N, a 2M claim
    /// included; and every claim reserves stage 1's `⌈w / ρ⌉` at the step's ρ (W-I4 as rcore/cap-s1
    /// restates it: no budget, the fold's reservation).
    #[test]
    fn s_t1_claims_times_n_is_not_fork_power_times_n() {
        let licensed = |d| PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: d };
        for n in [1u64, 10, 100, 1_000] {
            let mut plant = eight_cards(Plant::new()).bond(1, 13_000 * MSK, false);
            for i in 0..n {
                plant = plant.claim(0x1_0000 + i, floor_claim(1, licensed(NOW - 1), NOW - 30 + (i % 7)));
            }
            let state = plant.state();
            let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
            let row = row_of(&shadow, 1);
            assert_eq!(row.x_b, u128::from(n) * PALW_CAPACITY_FCW_V1, "N = {n}: every licensed claim at full weight");
            assert_eq!(row.capped, u128::from(n.min(2)) * PALW_CAPACITY_FCW_V1, "N = {n}: capped at W_cap = 2 FCW");
            assert_eq!(shadow.bounded_immature_today, u128::from(n) * PALW_CAPACITY_FCW_V1, "today: additive");
            assert_eq!(shadow.bounded_immature_new, row.capped);
            let rho = u128::from(PALW_CAPACITY_UNCREDITED_STEPS_V1[0].rho);
            assert_eq!(row.reserved_new_total, u128::from(n) * W_FLOOR.div_ceil(rho), "W-I4 (stage 1): ⌈w / ρ⌉ each");
            assert!(shadow.claims.iter().all(|c| c.reserved_new == W_FLOOR.div_ceil(rho)), "every claim alike, no budget order");
        }
        // A 2M claim (C7 in these params) on the same bond: its reservation is ⌈59,742.94 MSK / ρ⌉ at
        // each step (stage 1), its Final weight 8k's, and the bond's term stays 2 FCW.
        let state = eight_cards(Plant::new())
            .bond(1, 13_000 * MSK, false)
            .claim(0x2000, claim(1, two_m(), licensed(NOW - 1), W_2M, RAW_2M, NOW - 40))
            .claim(0x2001, floor_claim(1, PalwClaimPhaseV2::Provisional, NOW - 2))
            .state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        let big = claim_row(&shadow, 0x2000).unwrap();
        assert!(big.c7);
        assert_eq!(big.w_full, PALW_CAPACITY_C7_WEIGHT_CEILING_V1);
        let rhos: Vec<u128> = PALW_CAPACITY_UNCREDITED_STEPS_V1.iter().map(|step| u128::from(step.rho)).collect();
        assert_eq!(big.reserved_new, W_2M.div_ceil(rhos[0]), "stage 1: ⌈w / ρ⌉ at the first step, no budget");
        assert_eq!(claim_row(&shadow, 0x2001).unwrap().reserved_new, W_FLOOR.div_ceil(rhos[0]), "the second claim alike");
        assert_eq!(row_of(&shadow, 1).capped, 2 * PALW_CAPACITY_FCW_V1);
        assert_eq!(big.m_new, vec![u128::from(E); 5], "C7: m = E at every step");
        assert_eq!(big.commitment_new, rhos.iter().map(|rho| u128::from(E) + W_2M.div_ceil(*rho)).collect::<Vec<_>>());
        assert_eq!(big.commitment_today, u128::from(E) + W_2M);
        let credited = [PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 500 }];
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &credited);
        assert_eq!(claim_row(&shadow, 0x2000).unwrap().m_new, vec![u128::from(E)], "C7 is never credited");
        assert_eq!(shadow.credited_classes, vec![floor()], "C7 is not a credited class");
    }

    /// **The commitment table by phase** (§4.4 E-3/E-4, E-6) and the bond's A-1 recomposed, at a
    /// credited step (500‰: `m_c` = 640.17 MSK on the fold's floor).
    #[test]
    fn s_t1_commitments_by_phase_and_the_void_hold() {
        let mut released = floor_claim(2, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: NOW - 1 }, NOW - 25);
        released.rcore.escrow_released = true;
        let mut fp = floor_claim(2, PalwClaimPhaseV2::Provisional, NOW - 1);
        fp.source = PalwClaimSourceV2::FreePrompt { quanta: 4, spent: Default::default() };
        fp.escrowed_reward = 0;
        fp.rights_reserved = 777;
        let state = eight_cards(Plant::new())
            .bond(2, 100_000 * MSK, false)
            .claim(1, floor_claim(2, PalwClaimPhaseV2::Provisional, NOW - 30))
            .claim(2, floor_claim(2, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 9 }, NOW - 29))
            .claim(3, released)
            .claim(4, floor_claim(2, voided(NOW - 5, PalwVoidReasonV2::BindTimeout), NOW - 605))
            .claim(5, floor_claim(2, voided(NOW - 601, PalwVoidReasonV2::ReceiptTimeout), NOW - 900))
            .claim(6, floor_claim(2, PalwClaimPhaseV2::Final { final_daa: NOW - 3 }, NOW - 200))
            .claim(7, fp)
            .state();
        let p = params();
        let steps = [PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 500 }];
        let shadow = palw_capacity_shadow_v1(&state, &p, NOW, &steps);
        let m = M_AT_500;
        let full_today = u128::from(E) + W_FLOOR;
        // Stage 1 (rcore/cap-s1): the reservation at ρ = 10 is ⌈w / 10⌉.
        let w10 = W_FLOOR.div_ceil(10);
        let expect: [(u64, u128, u128); 5] = [
            (1, full_today, m + w10),
            (2, full_today, m + w10),
            (3, W_FLOOR, w10),
            // E-4: an unconvicted void holds m_c + reserved for h_obl = 600 — today it holds nothing.
            (4, 0, m + w10),
            (7, W_FLOOR + 777, W_FLOOR + 777),
        ];
        for (id, today, new) in expect {
            let row = claim_row(&shadow, id).unwrap_or_else(|| panic!("claim {id} reported"));
            assert_eq!((row.commitment_today, row.commitment_new[0]), (today, new), "claim {id}");
        }
        assert!(claim_row(&shadow, 5).is_none(), "a void past h_obl holds nothing under either rule");
        assert!(claim_row(&shadow, 6).is_none(), "Final holds nothing");
        assert_eq!(claim_row(&shadow, 7).unwrap().m_new, vec![0], "E-6: the FP lane keeps its accounting");
        assert_eq!(claim_row(&shadow, 1).unwrap().m_new, vec![m], "lane escrow's term at 500‰");
        assert_eq!(claim_row(&shadow, 1).unwrap().stage, PalwCapacityStageV1::Created);
        assert_eq!(claim_row(&shadow, 2).unwrap().stage, PalwCapacityStageV1::Anchored);
        assert_eq!(claim_row(&shadow, 2).unwrap().staged_w, PALW_CAPACITY_FCW_V1 / 100);
        let row = row_of(&shadow, 2);
        assert_eq!(row.committed_today, state.reserved_exposure(&bond_key(2)), "A-1 = the planted exposure");
        assert_eq!(row.own_claims_today, row.committed_today);
        assert_eq!(row.committed_new, vec![2 * (m + w10) + w10 + (m + w10) + W_FLOOR + 777]);
        // The void keeps its reservation in its hold; the three live attempt claims reserve ⌈w / 10⌉
        // each (stage 1: no budget).
        assert_eq!(claim_row(&shadow, 4).unwrap().reserved_new, w10);
        assert_eq!(row.reserved_new_total, 3 * w10);
        assert_eq!(row.unlicensed_claims, 3, "provisional, panel-bound and the FP claim");
        assert_eq!(shadow.licence_queue, 1);
        assert_eq!(shadow.licence_queue_oldest_bound_daa, Some(NOW - 9));
        assert_eq!(shadow.licensed_recent, 1);
    }

    /// **Where a consensus lane fixes the reading, the shadow reads as it does** (review of lane
    /// shadow, finding 3, probes P2/P3): (a) a DA accusation on a licensed claim keeps its full
    /// weight and it stays licensed (lane weight); (b) a `CourtConviction` whose claim was voided
    /// `CourtHeldVerdict` is tier-class, lifted at `since + window_court` (lane liab); (c) a
    /// `PanelFalseValidV2` finding that acted on its claim charges the producer too (lane liab);
    /// (d) a void releases the weight budget at once (lane weight's index); (e) a conviction's void
    /// holds no obligation, an unconvicted one holds `m_c + reserved` for `h_obl` (lane escrow).
    #[test]
    fn s_t1_the_shadow_reads_as_the_consensus_lanes_do() {
        // (a) DefaultDisputed { resumed: ReceiptLicensed } on bond 1.
        let mut disputed = floor_claim(1, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: NOW - 5 }, NOW - 30);
        disputed.phase = PalwClaimPhaseV2::DefaultDisputed {
            accused_daa: NOW - 2,
            missing_event_index: 0,
            accuser: bond_key(100),
            accuser_exposure: 0,
            resumed: Box::new(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: NOW - 5 }),
        };
        let mut bound_disputed = floor_claim(1, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 8 }, NOW - 31);
        bound_disputed.phase = PalwClaimPhaseV2::DefaultDisputed {
            accused_daa: NOW - 2,
            missing_event_index: 0,
            accuser: bond_key(100),
            accuser_exposure: 0,
            resumed: Box::new(PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 8 }),
        };
        let state = eight_cards(Plant::new())
            .bond(1, 13_000 * MSK, false)
            .claim(0x77, disputed)
            .claim(0x78, bound_disputed)
            // (b) bond 20: a held dissection's verdict 3,500 DAA ago; bond 22: a proven one.
            .bond(20, 13_000 * MSK, false)
            .claim(0x90, floor_claim(20, voided(NOW - 3_500, PalwVoidReasonV2::CourtHeldVerdict), NOW - 3_600))
            .bond(22, 13_000 * MSK, false)
            .claim(0x92, floor_claim(22, voided(NOW - 3_500, PalwVoidReasonV2::CourtFraud), NOW - 3_600))
            // (c) bond 21's claim voided CourtFraud by a kind-3 against seat 101 in the same block; bond
            // 23's claim voided by a DA default 50 DAA before a kind-3 against seat 102 restated it;
            // bond 24's DA default in one block: its DaDefault record, and the kind-3 record the
            // default writes on covering signer 103, its claim voided ProducerWithholding.
            .bond(21, 13_000 * MSK, false)
            .claim(0x91, floor_claim(21, voided(NOW - 10, PalwVoidReasonV2::CourtFraud), NOW - 100))
            .bond(23, 13_000 * MSK, false)
            .claim(0x93, floor_claim(23, voided(NOW - 60, PalwVoidReasonV2::ProducerWithholding), NOW - 100))
            .bond(24, 13_000 * MSK, false)
            .claim(0x94, floor_claim(24, voided(NOW - 10, PalwVoidReasonV2::ProducerWithholding), NOW - 100))
            // (d)/(e) bond 3: a timed-out void and a convicted void inside h_obl, then a live claim.
            .bond(3, 13_000 * MSK, false)
            .claim(0xD0, floor_claim(3, voided(NOW - 20, PalwVoidReasonV2::ReceiptTimeout), NOW - 50))
            .claim(0xD1, floor_claim(3, voided(NOW - 20, PalwVoidReasonV2::CourtFraud), NOW - 49))
            .claim(0xD2, floor_claim(3, PalwClaimPhaseV2::Provisional, NOW - 2))
            .claim(0xD3, floor_claim(3, PalwClaimPhaseV2::Provisional, NOW - 1))
            .record(0xC0, offence(PalwOffenceKindV1::CourtConviction, 20, NOW - 3_500, h(0x90)))
            .record(0xC2, offence(PalwOffenceKindV1::CourtConviction, 22, NOW - 3_500, h(0x92)))
            .record(0xC1, offence(PalwOffenceKindV1::PanelFalseValidV2, 101, NOW - 10, h(0x91)))
            .record(0xC3, offence(PalwOffenceKindV1::PanelFalseValidV2, 102, NOW - 10, h(0x93)))
            .record(0xC4, offence(PalwOffenceKindV1::DaDefault, 24, NOW - 10, h(0x94)))
            .record(0xC5, offence(PalwOffenceKindV1::PanelFalseValidV2, 103, NOW - 10, h(0x94)))
            .state();
        let steps = [PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 0 }];
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &steps);

        // (a)
        let row = claim_row(&shadow, 0x77).unwrap();
        assert_eq!(row.stage, PalwCapacityStageV1::Licensed { permille: 1_000 }, "an accusation keeps the licensed stage");
        assert_eq!(row.staged_w, row.w_full);
        assert_eq!(claim_row(&shadow, 0x78).unwrap().stage, PalwCapacityStageV1::Anchored, "a bound claim disputed: Anchored");
        assert_eq!(row_of(&shadow, 1).unlicensed_claims, 1, "only the disputed PanelBound claim is unlicensed");

        // (b)
        let frozen = |n| {
            let r = row_of(&shadow, n);
            (r.frozen_would_be, r.freeze_final, r.freeze_undetermined, r.convictions)
        };
        assert_eq!(frozen(20), (false, false, false, 1), "held verdict: tier, lifted at since + window_court");
        assert_eq!(frozen(22), (true, true, false, 1), "a proven verdict: intent, final");

        // (c)
        assert_eq!(frozen(101), (true, false, true, 1), "the seat");
        assert_eq!(frozen(21), (true, false, true, 1), "the producer of the claim the finding voided, by the seat's class");
        assert_eq!(frozen(102), (true, false, true, 1));
        assert_eq!(frozen(23), (false, false, false, 0), "a finding that restated an earlier void does not charge the producer");
        assert_eq!(frozen(24), (true, false, false, 1), "a DA default: the producer by its DaDefault record alone — a TIER freeze (decision 1)");
        assert_eq!(frozen(103), (true, false, true, 1), "the default's covering signer");
        assert_eq!(shadow.convictions_total, 6);

        // (d) and (e)
        let m = u128::from(E);
        let d = |id| {
            let r = claim_row(&shadow, id);
            r.map(|r| (r.reserved_new, r.commitment_new[0]))
        };
        // Stage 1 (rcore/cap-s1): every claim reserves ⌈w / ρ⌉ at the first display step (ρ 10).
        let w10 = W_FLOOR.div_ceil(u128::from(PALW_CAPACITY_UNCREDITED_STEPS_V1[0].rho));
        assert_eq!(d(0xD0), Some((w10, m + w10)), "E-4: a timed-out void holds m_c + reserved for h_obl");
        assert_eq!(d(0xD1), None, "a conviction's void is charged, not held");
        assert_eq!(d(0xD2), Some((w10, m + w10)), "a live claim reserves ⌈w / ρ⌉, whatever voided before it");
        assert_eq!(d(0xD3), Some((w10, m + w10)), "and so does the next");
        let row = row_of(&shadow, 3);
        assert_eq!(row.reserved_new_total, 2 * w10, "W-I4 (stage 1) over the live claims");
        assert_eq!(row.committed_new[0], 3 * (m + w10));
    }

    /// **Seat duties and locks under lane liab's AS-1/AS-2 and lane escrow's bind**: the identity step
    /// (ρ 1, q 0) reproduces every seat's A-1; at ρ = 10 below `q_seat` (q 0 and v1's 143‰ alike)
    /// the seat reserves the claim's undivided `lock_2` — lane escrow's gate keeps `m_c = E`, so the
    /// commitment no longer cuts the duty under the lock it must post (v1 priced 143‰ as a credit
    /// and cut it to `(⌈E/10⌉ + w)/5`) — and at `q_seat` (250‰) `⌈λ/10⌉` with the lock divided too.
    #[test]
    fn s_t1_seat_duties_and_locks_reprice_by_rho() {
        let duty = u128::from(E) / 5;
        let seats: BTreeMap<PalwBondKeyV2, u64> = (100..105u64).map(|n| (bond_key(n), 0)).collect();
        let mut plant = eight_cards(Plant::new())
            .bond(1, 100_000 * MSK, false)
            .claim(0x10, floor_claim(1, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 3 }, NOW - 25))
            .claim(0x11, floor_claim(1, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 2 }, NOW - 24))
            .claim(0x12, floor_claim(1, PalwClaimPhaseV2::Final { final_daa: NOW - 50 }, NOW - 200));
        for id in [0x10u64, 0x11] {
            plant.duties.push((h(id), PalwPanelDutyRowV1 { seats: seats.clone(), seat_exposure: duty }));
        }
        let lock = 24_010_000_000u128;
        plant.extra.push(PalwDeltaEntryV2::SlashableLock {
            key: (bond_key(100), h(0x12)),
            old: None,
            new: Some(PalwSlashableLockV1 {
                claim: h(0x12),
                amount: lock,
                expiry_daa: NOW + 2_950,
                settled_at_final: 0,
                attested: PalwSegmentMaskV2::NONE,
                segments: 0,
            }),
        });
        let state = plant.state();
        let steps = [
            PalwCapacityStepV1 { from_daa: 0, rho: 1, q_credit_permille: 0 },
            PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 143 },
            PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 250 },
            PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 0 },
        ];
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &steps);
        for n in 100..108u64 {
            let row = row_of(&shadow, n);
            assert_eq!(row.committed_new[0], row.committed_today, "the identity step is today's A-1 (seat {n})");
        }
        let seat = row_of(&shadow, 100);
        // The bound claims' own lock_2 (L-1 at k′ = 2 on their frozen gain w): ≈ 240.1 MSK.
        let lock_2 = palw_rcore_lock_v1(W_FLOOR, E, 0, 2);
        assert!(lock_2 > duty.div_ceil(10) && lock_2 < duty);
        assert_eq!(seat.committed_today, 2 * duty + lock);
        assert_eq!(seat.committed_new[1], 2 * lock_2 + lock, "143‰ < q_seat: m_c = E, the seat reserves lock_2, the lock stays");
        assert_eq!(seat.committed_new[2], 2 * duty.div_ceil(10) + lock.div_ceil(10), "250‰: both divide");
        assert_eq!(seat.committed_new[3], 2 * lock_2 + lock, "testnet-12's first step (ρ 10, q 0): the duty is lock_2");
        assert_eq!(shadow.duty_rows, 2);
        assert_eq!(shadow.duty_rows_capped, 0, "the floor duty is λ-bound, below commitment / seats");
        assert_eq!(shadow.seat_duty_total_today, 2 * 5 * duty);
        assert_eq!(shadow.steps[1].seat_duty_total, 2 * 5 * lock_2);
        assert_eq!(shadow.steps[3].seat_duty_total, 2 * 5 * lock_2);
        assert_eq!(shadow.seat_lock_total_today, lock);
        assert_eq!(
            shadow.steps.iter().map(|s| (s.seat_lock_total, s.seat_credit)).collect::<Vec<_>>(),
            vec![(lock, false), (lock, false), (lock.div_ceil(10), true), (lock, false)]
        );
        assert_eq!(shadow.reference_duty, duty, "the floor rows' mean duty");
        assert_eq!(shadow.reference_lock, lock, "the floor locks' mean");
    }

    /// **AG-3 read off the conviction records, and the class counters** (convictions by kind, DA by
    /// non-seats, the latency histogram).
    #[test]
    fn s_t1_freeze_and_class_counters() {
        let mut plant = eight_cards(Plant::new())
            .bond(10, 13_000 * MSK, false)
            .bond(11, 13_000 * MSK, false)
            .bond(12, 13_000 * MSK, false)
            .bond(13, 13_000 * MSK, false)
            .bond(14, 13_000 * MSK, false)
            .claim(0x50, floor_claim(10, voided(NOW - 4_000, PalwVoidReasonV2::ProducerWithholding), NOW - 4_050))
            .claim(0x60, floor_claim(14, voided(NOW - 10, PalwVoidReasonV2::CourtFraud), NOW - 700))
            .claim(0x61, floor_claim(14, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 4 }, NOW - 30))
            .claim(0x62, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 3))
            .record(0xA0, offence(PalwOffenceKindV1::DaDefault, 10, NOW - 4_000, h(0x50)))
            .record(0xA1, offence(PalwOffenceKindV1::ExecutorEquivocation, 11, NOW - 10, Hash64::default()))
            .record(0xA2, offence(PalwOffenceKindV1::ExecutorEquivocation, 12, NOW - 3_001, Hash64::default()))
            .record(0xA3, offence(PalwOffenceKindV1::ExecutorRefuted, 13, NOW - 100, Hash64::default()));
        let session = |seat| crate::palw_da_rcore_v1::PalwDaSessionV1 {
            opened_daa: NOW - 2,
            deadline_daa: NOW + 20,
            accuser_is_seat: seat,
            exposure: 0,
            units: Vec::new(),
            stage: crate::palw_da_rcore_v1::PalwDaStageV1::Live,
        };
        plant.extra.push(PalwDeltaEntryV2::DaSession { key: (h(0x61), bond_key(100)), old: None, new: Some(session(true)) });
        plant.extra.push(PalwDeltaEntryV2::DaSession { key: (h(0x61), bond_key(101)), old: None, new: Some(session(false)) });
        plant.extra.push(PalwDeltaEntryV2::DaClaim {
            key: h(0x61),
            old: None,
            new: Some(crate::palw_da_rcore_v1::PalwDaClaimV1 {
                open_seat_sessions: 1,
                open_other_sessions: 1,
                opened_non_seat_total: 3,
                ..Default::default()
            }),
        });
        let state = plant.state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        let frozen = |n| {
            let r = row_of(&shadow, n);
            (r.frozen_would_be, r.freeze_final, r.freeze_undetermined, r.convictions)
        };
        // rcore/cap-s1, decision 1: a DA default is TIER-class — 4,000 DAA ago, lifted at since + window_court.
        assert_eq!(frozen(10), (false, false, false, 1), "DA default: tier class, lifted");
        assert_eq!(frozen(11), (true, false, false, 1), "a tier conviction 10 DAA ago: frozen");
        assert_eq!(frozen(12), (false, false, false, 1), "3,001 DAA ago: lifted at since + window_court");
        assert_eq!(frozen(13), (true, false, true, 1), "ExecutorRefuted: sub-kind undetermined, held for window_court");
        assert_eq!(shadow.convictions_total, 4);
        let floor_row = shadow.attribution.iter().find(|a| a.class_id == floor()).unwrap();
        assert_eq!(floor_row.convictions_by_kind, vec![(PalwOffenceKindV1::DaDefault as u8, 1)]);
        assert_eq!(floor_row.conviction_latency_histogram, [0, 0, 1, 0, 0, 0, 0, 0], "50 DAA from acceptance: the [50, 100) bucket");
        assert_eq!((floor_row.da_open_non_seat, floor_row.da_open_seat, floor_row.da_opened_non_seat_total), (1, 1, 3));
        assert_eq!((floor_row.claims_voided, floor_row.voids_attributed, floor_row.claims_live), (2, 2, 2));
        assert_eq!(floor_row.voids_by_reason, vec![("CourtFraud".to_string(), 1), ("ProducerWithholding".to_string(), 1)]);
        let keyless = shadow.attribution.iter().find(|a| a.class_id == Hash64::default()).unwrap();
        assert_eq!(keyless.convictions_by_kind.iter().map(|(_, n)| n).sum::<u64>(), 3, "records naming no claim");
        assert!(shadow.adversary.is_empty(), "no O-3 bond named: nothing is measured");
    }

    /// **VP-A (review of lane shadow, round 2, finding 1): `q` counts only what the credit prices.**
    /// A garbage campaign convicted only by routes the credit does not price — a kind-4 refutation
    /// (its tag, 5 / 12 or 9–13, is not in the record), a held verdict, a court default — plus a seat
    /// record restating a timeout void and two uncaught `Final`s reads `q = 0` (it used to read
    /// 666‰), and A8 alarms on every step, the default display and v1's reference ramp alike.
    #[test]
    fn vp_a_convictions_the_credit_cannot_rely_on_are_misses() {
        use PalwCapacityStrategyV1::{Garbage, Naive};
        let state = eight_cards(Plant::new())
            .bond(14, 13_000 * MSK, false)
            .claim(0x70, floor_claim(14, voided(NOW - 40, PalwVoidReasonV2::CourtFraud), NOW - 100))
            .claim(0x71, floor_claim(14, voided(NOW - 40, PalwVoidReasonV2::CourtHeldVerdict), NOW - 100))
            .claim(0x72, floor_claim(14, voided(NOW - 40, PalwVoidReasonV2::CourtDefault), NOW - 100))
            .claim(0x73, floor_claim(14, voided(NOW - 60, PalwVoidReasonV2::ReceiptTimeout), NOW - 100))
            .claim(0x74, floor_claim(14, PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, NOW - 200))
            .claim(0x75, floor_claim(14, PalwClaimPhaseV2::Final { final_daa: NOW - 19 }, NOW - 199))
            .record(0xE0, offence(PalwOffenceKindV1::ExecutorRefuted, 14, NOW - 40, h(0x70)))
            .record(0xE1, offence(PalwOffenceKindV1::CourtConviction, 14, NOW - 40, h(0x71)))
            .record(0xE3, offence(PalwOffenceKindV1::PanelFalseValidV2, 101, NOW - 10, h(0x73)))
            .state();
        for steps in [Vec::new(), PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec()] {
            let options = PalwCapacityShadowOptionsV1 { adversaries: vec![adversary(14, Garbage)], steps, ..Default::default() };
            let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options);
            let row = named(&shadow, Garbage);
            assert_eq!(counts(row), (6, 0, 0, 3, 3, 0, 0, Some(0)), "three unpriced convictions and three undetected: q 0");
            assert_eq!(row.unpriced_by_route, vec![("court-held-verdict", 1), ("refuted-untagged", 1), ("court-default", 1)]);
            assert_eq!(row.seat_only, 1, "the seat's kind 3 on the timed-out claim is counted beside");
            assert!(shadow.adversary_row(&floor(), Naive).is_none());
            assert!(shadow.steps.iter().all(|s| s.q_alarm), "q 0 < 500‰ on every step");
            assert!(shadow.summary().ends_with("q-ALARM"));
        }
        // The same campaign with a proven verdict (kind 6 on a CourtFraud void) and a DA default in
        // place of the refutation and the held verdict: the verdict counts; the DA default does NOT since
        // the user's decision 1 (S1 is tier-class: a first default collects the commitment alone — the
        // credit cannot rely on it, rcore/cap-s1); nor does the court default.
        let campaign = |second: PalwOffenceKindV1, void: PalwVoidReasonV2| {
            eight_cards(Plant::new())
                .bond(14, 13_000 * MSK, false)
                .claim(0x70, floor_claim(14, voided(NOW - 40, PalwVoidReasonV2::CourtFraud), NOW - 100))
                .claim(0x71, floor_claim(14, voided(NOW - 40, void), NOW - 100))
                .claim(0x72, floor_claim(14, voided(NOW - 40, PalwVoidReasonV2::CourtDefault), NOW - 100))
                .record(0xE0, offence(PalwOffenceKindV1::CourtConviction, 14, NOW - 40, h(0x70)))
                .record(0xE1, offence(second, 14, NOW - 40, h(0x71)))
                .state()
        };
        let options = PalwCapacityShadowOptionsV1 { adversaries: vec![adversary(14, Naive)], ..Default::default() };
        let shadow =
            palw_capacity_shadow_with_v1(&campaign(PalwOffenceKindV1::DaDefault, PalwVoidReasonV2::ProducerWithholding), &params(), NOW, &options);
        let row = named(&shadow, Naive);
        assert_eq!(counts(row), (3, 1, 0, 2, 0, 0, 0, Some(333)), "decision 1: the DA default is a miss for the credit");
        assert!(row.unpriced_by_route.contains(&("da-default", 1)), "{:?}", row.unpriced_by_route);
        assert!(shadow.steps.iter().all(|s| s.q_alarm), "333‰ < 500‰");
        // Two proven verdicts: both count, the court default does not.
        let shadow =
            palw_capacity_shadow_with_v1(&campaign(PalwOffenceKindV1::CourtConviction, PalwVoidReasonV2::CourtFraud), &params(), NOW, &options);
        assert_eq!(counts(named(&shadow, Naive)), (3, 2, 0, 1, 0, 0, 0, Some(666)));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm), "666‰ ≥ 500‰ at an uncredited step");
    }

    /// **VP-B (review of lane shadow, round 2, finding 1): a credited step needs twice ITS credit,
    /// every named strategy measured.** A measured 300‰ alarms at a step crediting `q_seat` (250‰:
    /// the bar is 500‰), and so does an unmeasured strategy; three strategies measured at 600‰
    /// clear a 250‰ and a 300‰ credit, and not a 301‰ one (the bar 602‰).
    #[test]
    fn vp_b_a_credited_step_needs_twice_its_credit_for_every_strategy() {
        use PalwCapacityStrategyV1::{Borrowed, Garbage, Naive};
        // Bond 14 (naive): 3 caught of 10 resolved — by proven verdicts (rcore/cap-s1: since the user's
        // decision 1 a DA default is tier-class and the credit does not count it).
        let campaign = |plant: Plant, bond: u64, base: u64, caught: u64| {
            let mut plant = plant.bond(bond, 13_000 * MSK, false);
            for i in 0..10u64 {
                let id = base + i;
                if i < caught {
                    plant = plant
                        .claim(id, floor_claim(bond, voided(NOW - 40, PalwVoidReasonV2::CourtFraud), NOW - 200 + i))
                        .record(0x1_0000 + id, offence(PalwOffenceKindV1::CourtConviction, bond, NOW - 40, h(id)));
                } else {
                    plant = plant.claim(id, floor_claim(bond, PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, NOW - 200 + i));
                }
            }
            plant
        };
        let at = |q| vec![PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: q }];
        let state = campaign(eight_cards(Plant::new()), 14, 0x100, 3).state();
        let options = PalwCapacityShadowOptionsV1 { adversaries: vec![adversary(14, Naive)], steps: at(250), ..Default::default() };
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options);
        assert_eq!(named(&shadow, Naive).q_measured_permille, Some(300));
        let s = &shadow.steps[0];
        assert!(s.seat_credit && s.q_required_permille == 500 && s.q_alarm, "300 < 2 × 250");
        // Three strategies at 600‰ each.
        let state = campaign(campaign(campaign(eight_cards(Plant::new()), 14, 0x100, 6), 15, 0x200, 6), 16, 0x300, 6).state();
        let three = vec![adversary(14, Naive), adversary(15, Garbage), adversary(16, Borrowed)];
        for (q, alarm) in [(250u16, false), (300, false), (301, true), (0, false)] {
            let options = PalwCapacityShadowOptionsV1 { adversaries: three.clone(), steps: at(q), ..Default::default() };
            let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options);
            assert!(PalwCapacityStrategyV1::NAMED.iter().all(|s| named(&shadow, *s).q_measured_permille == Some(600)));
            let s = &shadow.steps[0];
            assert_eq!((s.q_alarm, s.q_alarm_unmeasured), (alarm, false), "credit {q}‰: bar {}", s.q_required_permille);
        }
        // One strategy unmeasured: a credited step alarms whatever the others read; uncredited not.
        let two = vec![adversary(14, Naive), adversary(15, Garbage)];
        let options = PalwCapacityShadowOptionsV1 { adversaries: two.clone(), steps: at(250), ..Default::default() };
        let s = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options).steps[0].clone();
        assert!(s.q_alarm && s.q_alarm_unmeasured, "borrowed is unmeasured");
        let options = PalwCapacityShadowOptionsV1 { adversaries: two, steps: at(0), ..Default::default() };
        assert!(!palw_capacity_shadow_with_v1(&state, &params(), NOW, &options).steps[0].q_alarm);
        // An unnamed strategy is measured and reported, but backs no strategy's credit.
        let unnamed = vec![PalwCapacityAdversaryV1 { bond: bond_key(14), strategy: PalwCapacityStrategyV1::Unnamed }];
        let options = PalwCapacityShadowOptionsV1 { adversaries: unnamed, steps: at(250), ..Default::default() };
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options);
        assert_eq!(named(&shadow, PalwCapacityStrategyV1::Unnamed).q_measured_permille, Some(600));
        assert!(shadow.steps[0].q_alarm_unmeasured);
    }

    /// **A8 on planted states**: resolved claims only (in-flight claims are out of `q`), the
    /// auditors stop (q 250‰ < 500‰: every step alarms), a node naming no bond measures nothing
    /// (quiet on the uncredited display, alarming on a credited one), and the auditors run.
    #[test]
    fn a8_q_over_resolved_claims_and_the_alarm() {
        use PalwCapacityStrategyV1::Naive;
        let one = PalwCapacityShadowOptionsV1 { adversaries: vec![adversary(14, Naive)], ..Default::default() };
        let proven = |plant: Plant| {
            plant
                .claim(0x60, floor_claim(14, voided(NOW - 10, PalwVoidReasonV2::CourtFraud), NOW - 700))
                .record(0xB9, offence(PalwOffenceKindV1::CourtConviction, 14, NOW - 10, h(0x60)))
        };
        // One caught (a proven verdict) and three just created: nothing measured against the catch.
        let state = proven(eight_cards(Plant::new()).bond(14, 13_000 * MSK, false))
            .claim(0x61, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 5))
            .claim(0x62, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 4))
            .claim(0x63, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 3))
            .state();
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &one);
        assert_eq!(counts(named(&shadow, Naive)), (4, 1, 0, 0, 0, 0, 3, Some(1_000)));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm), "in-flight claims do not alarm");

        // The auditors stop: two `Final`, one timed out, nobody convicted: q = 1/4 = 250‰ < 500‰. The
        // bond's free-prompt claim (no `E`, nothing the credit prices) is not measured.
        let mut fp = floor_claim(14, PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, NOW - 200);
        fp.source = PalwClaimSourceV2::FreePrompt { quanta: 4, spent: Default::default() };
        fp.escrowed_reward = 0;
        let state = proven(eight_cards(Plant::new()).bond(14, 13_000 * MSK, false))
            .claim(0x61, floor_claim(14, PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, NOW - 200))
            .claim(0x62, floor_claim(14, voided(NOW - 30, PalwVoidReasonV2::BindTimeout), NOW - 300))
            .claim(0x63, floor_claim(14, PalwClaimPhaseV2::Final { final_daa: NOW - 9 }, NOW - 190))
            .claim(0x64, floor_claim(14, PalwClaimPhaseV2::Provisional, NOW - 3))
            .claim(0x65, fp)
            .state();
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &one);
        assert_eq!(counts(named(&shadow, Naive)), (5, 1, 0, 0, 3, 0, 1, Some(250)));
        assert!(shadow.steps.iter().all(|s| s.q_alarm && !s.q_alarm_unmeasured), "250‰ < 500‰ at every step");
        assert!(shadow.summary().ends_with("q-ALARM"), "{}", shadow.summary());
        let reference = PalwCapacityShadowOptionsV1 { steps: PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec(), ..one.clone() };
        assert!(palw_capacity_shadow_with_v1(&state, &params(), NOW, &reference).steps.iter().all(|s| s.q_alarm));
        // Unnamed, nothing is measured: quiet on the uncredited display — what a node without
        // `--palw-capacity-shadow-adversary` logs — and alarming on a credited step.
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &PalwCapacityShadowOptionsV1::default());
        assert!(shadow.adversary.is_empty() && shadow.steps.iter().all(|s| !s.q_alarm));
        let unmeasured = PalwCapacityShadowOptionsV1 { steps: PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec(), ..Default::default() };
        assert!(
            palw_capacity_shadow_with_v1(&state, &params(), NOW, &unmeasured).steps.iter().all(|s| s.q_alarm && s.q_alarm_unmeasured)
        );

        // The auditors run: proven verdicts convict the Final claim and the bound one — priced routes (a
        // DA default in their place is a miss since decision 1, below).
        let running = |kind| {
            // A proven verdict voids (or reverses) its claim `CourtFraud`; the other kinds are read on the
            // claims as the lane planted them.
            let (p61, p64) = if kind == PalwOffenceKindV1::CourtConviction {
                (voided(NOW - 1, PalwVoidReasonV2::CourtFraud), voided(NOW - 1, PalwVoidReasonV2::CourtFraud))
            } else {
                (PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, PalwClaimPhaseV2::PanelBound { bound_daa: NOW - 2 })
            };
            proven(eight_cards(Plant::new()).bond(14, 13_000 * MSK, false))
                .claim(0x61, floor_claim(14, p61, NOW - 200))
                .claim(0x64, floor_claim(14, p64, NOW - 25))
                .record(0xB0, offence(kind, 14, NOW - 1, h(0x61)))
                .record(0xB1, offence(kind, 14, NOW - 1, h(0x64)))
                .state()
        };
        let shadow = palw_capacity_shadow_with_v1(&running(PalwOffenceKindV1::CourtConviction), &params(), NOW, &one);
        assert_eq!(counts(named(&shadow, Naive)), (3, 3, 0, 0, 0, 0, 0, Some(1_000)));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm));
        // rcore/cap-s1, decision 1: DA defaults in their place are misses for the credit — 1 of 3.
        let shadow = palw_capacity_shadow_with_v1(&running(PalwOffenceKindV1::DaDefault), &params(), NOW, &one);
        let row = named(&shadow, Naive);
        assert_eq!((counts(row), row.unpriced_by_route.clone()), ((3, 1, 0, 2, 0, 0, 0, Some(333)), vec![("da-default", 2)]));
        assert!(shadow.steps.iter().all(|s| s.q_alarm));
        // Kind-4 refutations in their place are misses: they may be tier (5 / 12), whose collection
        // the credit cannot rely on — 1 of 3, and the alarm.
        let shadow = palw_capacity_shadow_with_v1(&running(PalwOffenceKindV1::ExecutorRefuted), &params(), NOW, &one);
        let row = named(&shadow, Naive);
        assert_eq!((counts(row), row.unpriced_by_route.clone()), ((3, 1, 0, 2, 0, 0, 0, Some(333)), vec![("refuted-untagged", 2)]));
        assert!(shadow.steps.iter().all(|s| s.q_alarm));
    }

    /// **A priced conviction past the claim's earliest maturity is a miss** (lane liab's rule: counted
    /// only before any reward of the bond can have matured), and a C7 class never alarms.
    #[test]
    fn late_convictions_are_misses_and_c7_never_alarms() {
        use PalwCapacityStrategyV1::Naive;
        // (Proven verdicts: a priced route since the user's decision 1 took the DA default out, rcore/cap-s1.)
        let state = eight_cards(Plant::new())
            .bond(14, 13_000 * MSK, false)
            .claim(0x80, floor_claim(14, voided(NOW - 50, PalwVoidReasonV2::CourtFraud), NOW - 3_050))
            .claim(0x81, floor_claim(14, voided(NOW - 50, PalwVoidReasonV2::CourtFraud), NOW - 3_049))
            .record(0xF0, offence(PalwOffenceKindV1::CourtConviction, 14, NOW - 50, h(0x80)))
            .record(0xF1, offence(PalwOffenceKindV1::CourtConviction, 14, NOW - 50, h(0x81)))
            .bond(15, 13_000 * MSK, false)
            .claim(0x90, claim(15, two_m(), PalwClaimPhaseV2::Final { final_daa: NOW - 20 }, W_2M, RAW_2M, NOW - 200))
            .state();
        let options =
            PalwCapacityShadowOptionsV1 { adversaries: vec![adversary(14, Naive), adversary(15, Naive)], ..Default::default() };
        let shadow = palw_capacity_shadow_with_v1(&state, &params(), NOW, &options);
        // 0x80: 3,000 DAA after acceptance — at the horizon, late; 0x81: 2,999, caught.
        assert_eq!(counts(named(&shadow, Naive)), (2, 1, 1, 0, 0, 0, 0, Some(500)));
        let c7 = shadow.adversary_row(&two_m(), Naive).expect("the 2M row");
        assert!(c7.c7 && c7.q_measured_permille == Some(0));
        assert!(shadow.steps.iter().all(|s| !s.q_alarm), "500‰ clears the bar; the C7 row's 0 never alarms");
    }

    /// **Every void reason is classed** (the match is exhaustive): the four conviction voids are
    /// convictions (their route read off the record), the five failures of time or capacity are
    /// undetected. Lane liab's `AggregateForfeit` is censored (rcore/cap-s1).
    #[test]
    fn every_void_reason_is_classed_for_q() {
        use PalwCapacityVoidAttributionV1::{Censored, Conviction, Undetected};
        let classes = [
            (PalwVoidReasonV2::BindTimeout, Undetected),
            (PalwVoidReasonV2::ReceiptTimeout, Undetected),
            (PalwVoidReasonV2::CourtFraud, Conviction),
            (PalwVoidReasonV2::ProducerWithholding, Conviction),
            (PalwVoidReasonV2::NoCapablePanel, Undetected),
            (PalwVoidReasonV2::UnavailableQuorum, Undetected),
            (PalwVoidReasonV2::NotReplayBacked, Undetected),
            (PalwVoidReasonV2::CourtDefault, Conviction),
            (PalwVoidReasonV2::CourtHeldVerdict, Conviction),
            (PalwVoidReasonV2::AggregateForfeit, Censored),
        ];
        for (i, (reason, class)) in classes.iter().enumerate() {
            assert_eq!(borsh::to_vec(reason).unwrap(), vec![i as u8], "the table lists every reason in borsh order");
            assert_eq!(palw_capacity_void_attribution_v1(*reason), *class, "{reason:?}");
        }
        // Only lane liab's credited routes count, by name.
        let priced: Vec<PalwCapacityRouteV1> = [
            PalwCapacityRouteV1::DaDefault,
            PalwCapacityRouteV1::CourtFraud,
            PalwCapacityRouteV1::CourtHeldVerdict,
            PalwCapacityRouteV1::Refuted,
            PalwCapacityRouteV1::SeatFinding,
            PalwCapacityRouteV1::CourtDefault,
            PalwCapacityRouteV1::Unrecorded,
            PalwCapacityRouteV1::Other,
        ]
        .into_iter()
        .filter(|route| palw_capacity_route_credits_q_v1(*route))
        .collect();
        assert_eq!(priced, vec![PalwCapacityRouteV1::CourtFraud], "decision 1: the DA default is tier-class, not credited");
    }

    /// **The O-3 bond list**: `<txid>:<index>[:<strategy>]`, a bond named with two strategies refused.
    #[test]
    fn the_adversary_list_names_a_strategy_per_bond() {
        let txid = "ab".repeat(64);
        assert_eq!(
            palw_capacity_split_adversary_v1(&format!("{txid}:3")),
            Ok((format!("{txid}:3").as_str(), PalwCapacityStrategyV1::Unnamed))
        );
        let named = format!(" {txid}:3:Garbage ");
        assert_eq!(palw_capacity_split_adversary_v1(&named), Ok((format!("{txid}:3").as_str(), PalwCapacityStrategyV1::Garbage)));
        assert!(palw_capacity_split_adversary_v1(&format!("{txid}:3:bogus")).is_err());
        assert_eq!(PalwCapacityStrategyV1::parse_v1("unnamed"), None, "unnamed is the absence of a strategy, not one");
        let same = [adversary(1, PalwCapacityStrategyV1::Naive), adversary(1, PalwCapacityStrategyV1::Naive)];
        assert!(palw_capacity_check_adversaries_v1(&same).is_ok());
        let twice = [adversary(1, PalwCapacityStrategyV1::Naive), adversary(1, PalwCapacityStrategyV1::Borrowed)];
        assert!(palw_capacity_check_adversaries_v1(&twice).is_err());
    }

    /// **S-I3's shape**: every claim and bond is visited once; a large planted chain reports every
    /// live claim, and the per-bond sums equal the per-claim rows.
    #[test]
    fn s_i3_one_pass_over_claims_and_bonds() {
        let mut plant = eight_cards(Plant::new());
        for b in 0..50u64 {
            plant = plant.bond(1_000 + b, (13_000 + 1_000 * b) * MSK, false);
            for i in 0..40u64 {
                plant = plant.claim(0x10_0000 + b * 100 + i, floor_claim(1_000 + b, PalwClaimPhaseV2::Provisional, NOW - 40 + i));
            }
        }
        let state = plant.state();
        let shadow = palw_capacity_shadow_v1(&state, &params(), NOW, &[]);
        assert_eq!(shadow.claims.len(), 2_000);
        assert_eq!(shadow.bonds.len(), 58);
        for row in shadow.bonds.iter().filter(|r| !r.seat) {
            let own: Vec<u128> =
                (0..5).map(|i| shadow.claims.iter().filter(|c| c.bond == row.bond).map(|c| c.commitment_new[i]).sum()).collect();
            assert_eq!(row.committed_new, own, "a producer's commitment is its claims'");
            assert_eq!(row.live_claims, 40);
        }
        assert_eq!(shadow.claims_commitment_today, 2_000 * (u128::from(E) + W_FLOOR));
    }
}
