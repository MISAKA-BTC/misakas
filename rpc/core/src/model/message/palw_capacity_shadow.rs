//! **ADR-0160 §7.5: the capacity shadow (op 201; node-only, lane shadow)** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_capacity_shadow::*`).

use super::*;

// ---------------------------------------------------------------------------------------------
// ADR-0160 §7.5: the capacity shadow (op 201; node-only, lane shadow)
// ---------------------------------------------------------------------------------------------

/// One ramp step to price: from `from_daa` (informational in a shadow), the ramp factor `rho` and
/// the credited attribution rate in permille.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityStep {
    pub from_daa: u64,
    pub rho: u32,
    pub q_credit_permille: u32,
}

impl Serializer for RpcPalwCapacityStep {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.from_daa, writer)?;
        store!(u32, &self.rho, writer)?;
        store!(u32, &self.q_credit_permille, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityStep {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { from_daa: load!(u64, reader)?, rho: load!(u32, reader)?, q_credit_permille: load!(u32, reader)? })
    }
}

/// **ADR-0160 §7.5: `getPalwCapacityShadow` (op 201)** — what the capacity formulas would reserve,
/// weigh and allow for every live claim and bond of the tip, per ramp step, next to today's values.
/// Node-only: no verdict reads it. **Op 201 is new: a node built before it drops the WebSocket on
/// it**, so a client asks it last on a connection, or reconnects.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwCapacityShadowRequest {
    /// The steps to price, as the fold would (lane escrow's `m_c`: a credit applies only from
    /// `q_seat` = 250‰). Empty: the schedule the chain's F-L fence arms when its params carry one,
    /// else the UNCREDITED ramp ρ 10 / 25 / 50 / 100 / 1000 at q 0 (`m_c = E`). ADR-0160 v1's
    /// reference ramp (q 143‰) prices as the uncredited one under the rules as built; its v1 figures
    /// are in each step row's superseded column.
    pub steps: Vec<RpcPalwCapacityStep>,
    /// `<txid>:<index>`: only this bond's row and claims; empty for every bond.
    pub bond: String,
    /// The bonds of an O-3 run, `<txid>:<index>[:<strategy>]` (`naive`, `garbage` or `borrowed`;
    /// none: unnamed): their claims measure `q` per (class, strategy). A bond named with two
    /// strategies is refused.
    pub adversary_bonds: Vec<String>,
    /// Return the per-claim rows (the bond rows and totals always come).
    pub include_claims: bool,
    /// At most this many claim rows (and bond rows); 0 asks for the node's cap (500).
    pub limit: u32,
}

impl Serializer for GetPalwCapacityShadowRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcPalwCapacityStep>, &self.steps, writer)?;
        store!(String, &self.bond, writer)?;
        store!(Vec<String>, &self.adversary_bonds, writer)?;
        store!(bool, &self.include_claims, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwCapacityShadowRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            steps: deserialize!(Vec<RpcPalwCapacityStep>, reader)?,
            bond: load!(String, reader)?,
            adversary_bonds: load!(Vec<String>, reader)?,
            include_claims: load!(bool, reader)?,
            limit: load!(u32, reader)?,
        })
    }
}

/// The network under one step. `u128` quantities are decimal strings (sompi, or raw weight units).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityStepRow {
    pub step: RpcPalwCapacityStep,
    /// `m_c` of a reference floor claim as the fold prices it (lane escrow's term: the `q_seat` gate,
    /// `m*` on the conviction floor), and the credit that rule needs for `⌈E/ρ⌉` to bind (permille,
    /// never below `q_seat`; 693 … 997 on testnet-12's floor).
    pub m_floor_sompi: String,
    pub q_needed_permille: u32,
    /// `m_floor` is the ramp term `⌈E/ρ⌉`; and the credit applies (`q_credit ≥ 250‰`: lane
    /// escrow's gate and lane liab's AS-2, one threshold — `m_c < E` and the locks shrink by ρ).
    pub ramp_binds: bool,
    pub seat_credit: bool,
    pub claims_commitment_sompi: String,
    pub committed_sompi: String,
    pub seat_duty_sompi: String,
    pub seat_lock_sompi: String,
    /// §5.4: floor claims per DAA the seat capital sustains, in thousandths — what a seat reserves at
    /// bind (lane liab's AS-1 duty, lane escrow's `max(duty′, lock′)`) and the AS-2 lock.
    pub seat_capacity_milli_per_daa: u64,
    /// Floor claims a fresh 13,000 MSK bond holds at once.
    pub n_instant_13k: u64,
    /// A8: a measured (class, strategy) `q` of an attributable class is below `qRequiredPermille`,
    /// or `qAlarmUnmeasured`.
    pub q_alarm: bool,
    /// §10 D-5's conditional column (an open decision: `L_seat` = the 130,000 MSK seat bond, so
    /// `q_seat` ≈ 25‰): would the seat credit apply, and the §5.4 estimate if it did.
    pub seat_credit_if_d5: bool,
    pub seat_capacity_if_d5_milli_per_daa: u64,
    /// **Superseded (ADR-0160 v1, `L = 3G`, no `q_seat` gate)**: `m_c` and a 13k bond's floor claims
    /// as v1 priced them — not a rule anywhere.
    pub m_floor_v1_superseded_sompi: String,
    pub n_instant_13k_v1_superseded: u64,
    /// `q` the routes the credit prices need at their `L = 3G` (131 … 143‰), and A8's bar
    /// `2 × max(that, q_seat, q_credit)` (500‰ for a credit up to 250‰).
    pub q_needed_route_permille: u32,
    pub q_required_permille: u32,
    /// The step credits (`q_credit > 0`) and an attributable class has a named strategy nobody
    /// measured.
    pub q_alarm_unmeasured: bool,
}

impl Serializer for RpcPalwCapacityStepRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcPalwCapacityStep, &self.step, writer)?;
        store!(String, &self.m_floor_sompi, writer)?;
        store!(u32, &self.q_needed_permille, writer)?;
        store!(bool, &self.ramp_binds, writer)?;
        store!(bool, &self.seat_credit, writer)?;
        store!(String, &self.claims_commitment_sompi, writer)?;
        store!(String, &self.committed_sompi, writer)?;
        store!(String, &self.seat_duty_sompi, writer)?;
        store!(String, &self.seat_lock_sompi, writer)?;
        store!(u64, &self.seat_capacity_milli_per_daa, writer)?;
        store!(u64, &self.n_instant_13k, writer)?;
        store!(bool, &self.q_alarm, writer)?;
        store!(bool, &self.seat_credit_if_d5, writer)?;
        store!(u64, &self.seat_capacity_if_d5_milli_per_daa, writer)?;
        store!(String, &self.m_floor_v1_superseded_sompi, writer)?;
        store!(u64, &self.n_instant_13k_v1_superseded, writer)?;
        store!(u32, &self.q_needed_route_permille, writer)?;
        store!(u32, &self.q_required_permille, writer)?;
        store!(bool, &self.q_alarm_unmeasured, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityStepRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            step: deserialize!(RpcPalwCapacityStep, reader)?,
            m_floor_sompi: load!(String, reader)?,
            q_needed_permille: load!(u32, reader)?,
            ramp_binds: load!(bool, reader)?,
            seat_credit: load!(bool, reader)?,
            claims_commitment_sompi: load!(String, reader)?,
            committed_sompi: load!(String, reader)?,
            seat_duty_sompi: load!(String, reader)?,
            seat_lock_sompi: load!(String, reader)?,
            seat_capacity_milli_per_daa: load!(u64, reader)?,
            n_instant_13k: load!(u64, reader)?,
            q_alarm: load!(bool, reader)?,
            seat_credit_if_d5: load!(bool, reader)?,
            seat_capacity_if_d5_milli_per_daa: load!(u64, reader)?,
            m_floor_v1_superseded_sompi: load!(String, reader)?,
            n_instant_13k_v1_superseded: load!(u64, reader)?,
            q_needed_route_permille: load!(u32, reader)?,
            q_required_permille: load!(u32, reader)?,
            q_alarm_unmeasured: load!(bool, reader)?,
        })
    }
}

/// One claim, today and per step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityClaimRow {
    pub claim_id: String,
    pub bond: String,
    pub class_id: String,
    /// `provisional`, `panel-bound`, `licensed`, `final`, `voided`, `disputed`.
    pub phase: String,
    /// ADR-0160 §4.2's weight stage: `created`, `anchored`, `licensed`, `licensed-partial`, …
    pub stage: String,
    pub accepted_daa: u64,
    pub free_prompt: bool,
    pub c7: bool,
    pub raw_weight: String,
    pub staged_weight: String,
    pub reserved_today_sompi: String,
    pub reserved_new_sompi: String,
    pub commitment_today_sompi: String,
    /// Per step, in the request's order.
    pub commitment_new_sompi: Vec<String>,
}

impl Serializer for RpcPalwCapacityClaimRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.phase, writer)?;
        store!(String, &self.stage, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        store!(bool, &self.free_prompt, writer)?;
        store!(bool, &self.c7, writer)?;
        store!(String, &self.raw_weight, writer)?;
        store!(String, &self.staged_weight, writer)?;
        store!(String, &self.reserved_today_sompi, writer)?;
        store!(String, &self.reserved_new_sompi, writer)?;
        store!(String, &self.commitment_today_sompi, writer)?;
        store!(Vec<String>, &self.commitment_new_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityClaimRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            bond: load!(String, reader)?,
            class_id: load!(String, reader)?,
            phase: load!(String, reader)?,
            stage: load!(String, reader)?,
            accepted_daa: load!(u64, reader)?,
            free_prompt: load!(bool, reader)?,
            c7: load!(bool, reader)?,
            raw_weight: load!(String, reader)?,
            staged_weight: load!(String, reader)?,
            reserved_today_sompi: load!(String, reader)?,
            reserved_new_sompi: load!(String, reader)?,
            commitment_today_sompi: load!(String, reader)?,
            commitment_new_sompi: load!(Vec<String>, reader)?,
        })
    }
}

/// One bond, today and per step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityBondRow {
    pub bond: String,
    pub collateral_sompi: u64,
    pub seat: bool,
    pub live_claims: u64,
    pub unlicensed_claims: u64,
    /// Today's immature weight of its claims, `W_cap`, `X_b` and `min(X_b, W_cap)` (raw units).
    pub raw_immature: String,
    pub w_cap: String,
    pub x_b: String,
    pub capped: String,
    pub r_budget_sompi: String,
    pub reserved_new_sompi: String,
    pub committed_today_sompi: String,
    pub own_claims_today_sompi: String,
    pub committed_new_sompi: Vec<String>,
    pub n_instant_today: u64,
    pub n_instant_new: Vec<u64>,
    pub n_more_today: u64,
    pub n_more_new: Vec<u64>,
    pub frozen_would_be: bool,
    pub freeze_final: bool,
    pub freeze_undetermined: bool,
    pub convictions: u64,
}

impl Serializer for RpcPalwCapacityBondRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        store!(u64, &self.collateral_sompi, writer)?;
        store!(bool, &self.seat, writer)?;
        store!(u64, &self.live_claims, writer)?;
        store!(u64, &self.unlicensed_claims, writer)?;
        store!(String, &self.raw_immature, writer)?;
        store!(String, &self.w_cap, writer)?;
        store!(String, &self.x_b, writer)?;
        store!(String, &self.capped, writer)?;
        store!(String, &self.r_budget_sompi, writer)?;
        store!(String, &self.reserved_new_sompi, writer)?;
        store!(String, &self.committed_today_sompi, writer)?;
        store!(String, &self.own_claims_today_sompi, writer)?;
        store!(Vec<String>, &self.committed_new_sompi, writer)?;
        store!(u64, &self.n_instant_today, writer)?;
        store!(Vec<u64>, &self.n_instant_new, writer)?;
        store!(u64, &self.n_more_today, writer)?;
        store!(Vec<u64>, &self.n_more_new, writer)?;
        store!(bool, &self.frozen_would_be, writer)?;
        store!(bool, &self.freeze_final, writer)?;
        store!(bool, &self.freeze_undetermined, writer)?;
        store!(u64, &self.convictions, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityBondRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            bond: load!(String, reader)?,
            collateral_sompi: load!(u64, reader)?,
            seat: load!(bool, reader)?,
            live_claims: load!(u64, reader)?,
            unlicensed_claims: load!(u64, reader)?,
            raw_immature: load!(String, reader)?,
            w_cap: load!(String, reader)?,
            x_b: load!(String, reader)?,
            capped: load!(String, reader)?,
            r_budget_sompi: load!(String, reader)?,
            reserved_new_sompi: load!(String, reader)?,
            committed_today_sompi: load!(String, reader)?,
            own_claims_today_sompi: load!(String, reader)?,
            committed_new_sompi: load!(Vec<String>, reader)?,
            n_instant_today: load!(u64, reader)?,
            n_instant_new: load!(Vec<u64>, reader)?,
            n_more_today: load!(u64, reader)?,
            n_more_new: load!(Vec<u64>, reader)?,
            frozen_would_be: load!(bool, reader)?,
            freeze_final: load!(bool, reader)?,
            freeze_undetermined: load!(bool, reader)?,
            convictions: load!(u64, reader)?,
        })
    }
}

/// One class's attribution counters (`classId` empty: records whose claim the state no longer holds).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityAttributionRow {
    pub class_id: String,
    pub claims_live: u64,
    pub claims_final: u64,
    pub claims_voided: u64,
    pub voids_attributed: u64,
    /// `Reason=count`.
    pub voids_by_reason: Vec<String>,
    /// `kind=count` (the `PalwOffenceKindV1` discriminant).
    pub convictions_by_kind: Vec<String>,
    pub da_open_non_seat: u64,
    pub da_open_seat: u64,
    pub da_opened_non_seat_total: u64,
    /// DAA from acceptance to conviction: < 10, 50, 100, 300, 600, 1,200, 3,000, and the rest.
    pub conviction_latency_histogram: Vec<u64>,
}

impl Serializer for RpcPalwCapacityAttributionRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(u64, &self.claims_live, writer)?;
        store!(u64, &self.claims_final, writer)?;
        store!(u64, &self.claims_voided, writer)?;
        store!(u64, &self.voids_attributed, writer)?;
        store!(Vec<String>, &self.voids_by_reason, writer)?;
        store!(Vec<String>, &self.convictions_by_kind, writer)?;
        store!(u64, &self.da_open_non_seat, writer)?;
        store!(u64, &self.da_open_seat, writer)?;
        store!(u64, &self.da_opened_non_seat_total, writer)?;
        store!(Vec<u64>, &self.conviction_latency_histogram, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityAttributionRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            claims_live: load!(u64, reader)?,
            claims_final: load!(u64, reader)?,
            claims_voided: load!(u64, reader)?,
            voids_attributed: load!(u64, reader)?,
            voids_by_reason: load!(Vec<String>, reader)?,
            convictions_by_kind: load!(Vec<String>, reader)?,
            da_open_non_seat: load!(u64, reader)?,
            da_open_seat: load!(u64, reader)?,
            da_opened_non_seat_total: load!(u64, reader)?,
            conviction_latency_histogram: load!(Vec<u64>, reader)?,
        })
    }
}

/// **One (class, strategy) of the named O-3 bonds: the measured attribution rate** — counted only on
/// what the credit prices (the claim's own producer convicted by `DaDefault` or a proven court verdict
/// before the claim's earliest maturity); every other resolution is a miss. A lower bound: kind-3/4
/// routes (garbage, borrowed) read as misses until their contradiction tag is readable.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwCapacityAdversaryRow {
    pub class_id: String,
    /// `naive`, `garbage`, `borrowed` or `unnamed`.
    pub strategy: String,
    /// A C7 class: never credited, never alarms.
    pub c7: bool,
    /// `caught + caughtLate + caughtUnpriced + undetected + censored + inFlight`.
    pub claims: u64,
    pub caught: u64,
    /// A priced conviction at or past `accepted + window_court` (a miss).
    pub caught_late: u64,
    /// Convicted only by routes the credit does not price (a miss), and `route=count`.
    pub caught_unpriced: u64,
    pub unpriced_by_route: Vec<String>,
    /// Resolved with no producer conviction (a miss).
    pub undetected: u64,
    /// Voided by a bond-level forfeiture before its own outcome, and not yet resolved (out of `q`).
    pub censored: u64,
    pub in_flight: u64,
    /// Claims a seat-only record names (counted beside).
    pub seat_only: u64,
    /// `caught / (caught + caughtLate + caughtUnpriced + undetected)` in permille; absent while none
    /// has resolved.
    pub q_measured_permille: Option<u32>,
}

impl Serializer for RpcPalwCapacityAdversaryRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.strategy, writer)?;
        store!(bool, &self.c7, writer)?;
        store!(u64, &self.claims, writer)?;
        store!(u64, &self.caught, writer)?;
        store!(u64, &self.caught_late, writer)?;
        store!(u64, &self.caught_unpriced, writer)?;
        store!(Vec<String>, &self.unpriced_by_route, writer)?;
        store!(u64, &self.undetected, writer)?;
        store!(u64, &self.censored, writer)?;
        store!(u64, &self.in_flight, writer)?;
        store!(u64, &self.seat_only, writer)?;
        store!(Option<u32>, &self.q_measured_permille, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwCapacityAdversaryRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            strategy: load!(String, reader)?,
            c7: load!(bool, reader)?,
            claims: load!(u64, reader)?,
            caught: load!(u64, reader)?,
            caught_late: load!(u64, reader)?,
            caught_unpriced: load!(u64, reader)?,
            unpriced_by_route: load!(Vec<String>, reader)?,
            undetected: load!(u64, reader)?,
            censored: load!(u64, reader)?,
            in_flight: load!(u64, reader)?,
            seat_only: load!(u64, reader)?,
            q_measured_permille: load!(Option<u32>, reader)?,
        })
    }
}

/// **The `getPalwCapacityShadow` answer.** `u128` quantities are decimal strings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwCapacityShadowResponse {
    /// False off `ConsensusV2`, or before the node has PALW state.
    pub available: bool,
    /// The DAA the shadow is priced at (the next block's) and the committed tip's.
    pub now_daa: u64,
    pub tip_daa: u64,
    /// The one-line summary kaspad logs every 10 DAA.
    pub summary: String,
    pub bounded_immature_today: String,
    pub bounded_immature_new: String,
    pub safe_weight: String,
    pub w_cap_total: String,
    /// The reference floor claim the network rows price: E, w, L = 3G (v1's pricing: the superseded
    /// column and A8's route `L`), the conviction tier its `m_c` is priced against (lane escrow's
    /// `Tier`: 1,300 MSK on testnet-12), seats, duty and lock.
    pub reference_escrow_sompi: String,
    pub reference_w_floor_sompi: String,
    pub reference_l_sompi: String,
    pub reference_conviction_tier_sompi: String,
    pub reference_seats: u32,
    pub reference_duty_sompi: String,
    pub reference_lock_sompi: String,
    pub claims_commitment_today_sompi: String,
    pub committed_today_sompi: String,
    pub seats: u64,
    pub seat_usable_capital_sompi: String,
    pub seat_duty_today_sompi: String,
    pub seat_lock_today_sompi: String,
    pub seat_capacity_today_milli_per_daa: u64,
    pub duty_rows: u64,
    /// Duty rows capped by `commitment / seats` at bind: their repriced duty is a lower bound.
    pub duty_rows_capped: u64,
    pub licence_queue: u64,
    pub licence_queue_oldest_bound_daa: Option<u64>,
    pub licensed_recent: u64,
    pub carriers_per_block: u64,
    pub carriage_blocks_to_drain: u64,
    pub convictions_total: u64,
    pub steps: Vec<RpcPalwCapacityStepRow>,
    pub bonds: Vec<RpcPalwCapacityBondRow>,
    /// Bond rows the query matched before the limit.
    pub bonds_total: u64,
    pub claims: Vec<RpcPalwCapacityClaimRow>,
    /// Claim rows the query matched before the limit.
    pub claims_total: u64,
    pub attribution: Vec<RpcPalwCapacityAttributionRow>,
    /// `q` per (class, strategy) of the O-3 bonds the request named.
    pub adversary: Vec<RpcPalwCapacityAdversaryRow>,
}

impl Serializer for GetPalwCapacityShadowResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.now_daa, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.summary, writer)?;
        store!(String, &self.bounded_immature_today, writer)?;
        store!(String, &self.bounded_immature_new, writer)?;
        store!(String, &self.safe_weight, writer)?;
        store!(String, &self.w_cap_total, writer)?;
        store!(String, &self.reference_escrow_sompi, writer)?;
        store!(String, &self.reference_w_floor_sompi, writer)?;
        store!(String, &self.reference_l_sompi, writer)?;
        store!(String, &self.reference_conviction_tier_sompi, writer)?;
        store!(u32, &self.reference_seats, writer)?;
        store!(String, &self.reference_duty_sompi, writer)?;
        store!(String, &self.reference_lock_sompi, writer)?;
        store!(String, &self.claims_commitment_today_sompi, writer)?;
        store!(String, &self.committed_today_sompi, writer)?;
        store!(u64, &self.seats, writer)?;
        store!(String, &self.seat_usable_capital_sompi, writer)?;
        store!(String, &self.seat_duty_today_sompi, writer)?;
        store!(String, &self.seat_lock_today_sompi, writer)?;
        store!(u64, &self.seat_capacity_today_milli_per_daa, writer)?;
        store!(u64, &self.duty_rows, writer)?;
        store!(u64, &self.duty_rows_capped, writer)?;
        store!(u64, &self.licence_queue, writer)?;
        store!(Option<u64>, &self.licence_queue_oldest_bound_daa, writer)?;
        store!(u64, &self.licensed_recent, writer)?;
        store!(u64, &self.carriers_per_block, writer)?;
        store!(u64, &self.carriage_blocks_to_drain, writer)?;
        store!(u64, &self.convictions_total, writer)?;
        serialize!(Vec<RpcPalwCapacityStepRow>, &self.steps, writer)?;
        serialize!(Vec<RpcPalwCapacityBondRow>, &self.bonds, writer)?;
        store!(u64, &self.bonds_total, writer)?;
        serialize!(Vec<RpcPalwCapacityClaimRow>, &self.claims, writer)?;
        store!(u64, &self.claims_total, writer)?;
        serialize!(Vec<RpcPalwCapacityAttributionRow>, &self.attribution, writer)?;
        serialize!(Vec<RpcPalwCapacityAdversaryRow>, &self.adversary, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwCapacityShadowResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            now_daa: load!(u64, reader)?,
            tip_daa: load!(u64, reader)?,
            summary: load!(String, reader)?,
            bounded_immature_today: load!(String, reader)?,
            bounded_immature_new: load!(String, reader)?,
            safe_weight: load!(String, reader)?,
            w_cap_total: load!(String, reader)?,
            reference_escrow_sompi: load!(String, reader)?,
            reference_w_floor_sompi: load!(String, reader)?,
            reference_l_sompi: load!(String, reader)?,
            reference_conviction_tier_sompi: load!(String, reader)?,
            reference_seats: load!(u32, reader)?,
            reference_duty_sompi: load!(String, reader)?,
            reference_lock_sompi: load!(String, reader)?,
            claims_commitment_today_sompi: load!(String, reader)?,
            committed_today_sompi: load!(String, reader)?,
            seats: load!(u64, reader)?,
            seat_usable_capital_sompi: load!(String, reader)?,
            seat_duty_today_sompi: load!(String, reader)?,
            seat_lock_today_sompi: load!(String, reader)?,
            seat_capacity_today_milli_per_daa: load!(u64, reader)?,
            duty_rows: load!(u64, reader)?,
            duty_rows_capped: load!(u64, reader)?,
            licence_queue: load!(u64, reader)?,
            licence_queue_oldest_bound_daa: load!(Option<u64>, reader)?,
            licensed_recent: load!(u64, reader)?,
            carriers_per_block: load!(u64, reader)?,
            carriage_blocks_to_drain: load!(u64, reader)?,
            convictions_total: load!(u64, reader)?,
            steps: deserialize!(Vec<RpcPalwCapacityStepRow>, reader)?,
            bonds: deserialize!(Vec<RpcPalwCapacityBondRow>, reader)?,
            bonds_total: load!(u64, reader)?,
            claims: deserialize!(Vec<RpcPalwCapacityClaimRow>, reader)?,
            claims_total: load!(u64, reader)?,
            attribution: deserialize!(Vec<RpcPalwCapacityAttributionRow>, reader)?,
            adversary: deserialize!(Vec<RpcPalwCapacityAdversaryRow>, reader)?,
        })
    }
}

/// **RFC-0009 stage D: `getPalwStateProof` (op 202)** — one collection of the PALW state, proven against the header that commits it.
///
/// The node returns the header of `block_hash` (or of its sink when empty) and the state that header commits — the state as-of the block's
/// SELECTED PARENT: the state-root preimage and every row of the requested collection. A client that pinned the block's hash recomputes the
/// header's hash, takes `palw_state_root` from it and checks `state_preimage` and `rows` against it
/// (`kaspa_consensus_core::palw_state_proof_v1`); until then every field is `UNVERIFIED_REMOTE_STATE`. The proof is O(rows) because the state
/// commitment is flat. **Op 202 is new: a node built before it drops the WebSocket on it**, so a client asks it last on a connection, or
/// reconnects.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStateProofRequest {
    /// 128 hex: the block whose header commits the state. Empty: the sink.
    pub block_hash: String,
    /// `bonds`, `classes` or `claims`.
    pub collection: String,
}

impl Serializer for GetPalwStateProofRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(String, &self.collection, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwStateProofRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { block_hash: load!(String, reader)?, collection: load!(String, reader)? })
    }
}

/// One committed row: the borsh bytes of the key and of the value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwProofRow {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl Serializer for RpcPalwProofRow {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u8>, &self.key, writer)?;
        store!(Vec<u8>, &self.value, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwProofRow {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { key: load!(Vec<u8>, reader)?, value: load!(Vec<u8>, reader)? })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStateProofResponse {
    /// False with `reason` when the node cannot prove (no V2 ruleset, a header that commits no state, a block too far behind the sink, a
    /// table over the flat-proof cap, or a state this node holds that its own header contradicts).
    pub available: bool,
    pub reason: String,
    /// The block whose header is returned (the sink when the request named none).
    pub block_hash: String,
    pub header: Option<RpcHeader>,
    /// The state-root preimage (ADR-0043 order): every collection root and the small scalar blocks.
    pub state_preimage: Vec<u8>,
    /// The collection the rows open.
    pub collection: String,
    /// Every row of the collection, in key order.
    pub rows: Vec<RpcPalwProofRow>,
    /// New native-audit query only: bounded original Core readonly record.
    /// Clients rederive it from the anchored proof; these bytes are not truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_audit_record: Option<Vec<u8>>,
    /// Optional v33 bounded coinbase/path transport, never a redemption flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_coinbase_proof: Option<Vec<u8>>,
}

impl Serializer for GetPalwStateProofResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let version = if self.native_coinbase_proof.is_some() {
            3u16
        } else if self.native_audit_record.is_some() {
            2u16
        } else {
            1u16
        };
        store!(u16, &version, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.reason, writer)?;
        store!(String, &self.block_hash, writer)?;
        serialize!(Option<RpcHeader>, &self.header, writer)?;
        store!(Vec<u8>, &self.state_preimage, writer)?;
        store!(String, &self.collection, writer)?;
        serialize!(Vec<RpcPalwProofRow>, &self.rows, writer)?;
        if version >= 2 {
            store!(Option<Vec<u8>>, &self.native_audit_record, writer)?;
        }
        if version == 3 {
            store!(Option<Vec<u8>>, &self.native_coinbase_proof, writer)?;
        }
        Ok(())
    }
}

impl Deserializer for GetPalwStateProofResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if !matches!(version, 1..=3) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown readonly proof response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            reason: load!(String, reader)?,
            block_hash: load!(String, reader)?,
            header: deserialize!(Option<RpcHeader>, reader)?,
            state_preimage: load!(Vec<u8>, reader)?,
            collection: load!(String, reader)?,
            rows: deserialize!(Vec<RpcPalwProofRow>, reader)?,
            native_audit_record: if version >= 2 { load!(Option<Vec<u8>>, reader)? } else { None },
            native_coinbase_proof: if version == 3 { load!(Option<Vec<u8>>, reader)? } else { None },
        })
    }
}

/// **RFC-0009 L2: `getPalwForkChoiceOpening` (op 203)** — the fork-choice openings of blocks' post-states, and the node's sink and tips.
///
/// For each block the node returns its header, the 292-byte fork-choice leaf of its post-state (`PalwForkChoiceLeafV1`, borsh, the
/// ADR-0176 D3 weight-allocation slot included — every reader reads that one versioned allocation), the ADR-0043 root the leaf is
/// enveloped with, and the root the post-state is committed as. A client believes none of it until
/// `PalwForkChoiceOpeningV1::verify` holds against a root it trusts (an attestation, a chain child's header); `sink` and `tips` are what
/// this node shows — a peer hiding a tip is caught by another peer. **Op 203 is new: a node built before it drops the WebSocket on it.**
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwForkChoiceOpeningRequest {
    /// 128 hex each, at most 16. Empty: the sink and every tip.
    pub block_hashes: Vec<String>,
    /// **RFC09 (v2):** open each entry's V2/V3 rule-E window above this blue score (the fork point's), for the rule-E comparator.
    #[serde(default)]
    pub rule_e_window_above_blue: Option<u64>,
    /// **RFC09 (v2):** bonds (hex of borsh `PalwBondKeyV2`) whose registry openings each entry carries (at most 64):
    /// V2 membership or V3 capital, selected by the committed leaf's form.
    #[serde(default)]
    pub registry_bonds: Vec<String>,
}

impl Serializer for GetPalwForkChoiceOpeningRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(Vec<String>, &self.block_hashes, writer)?;
        store!(Option<u64>, &self.rule_e_window_above_blue, writer)?;
        store!(Vec<String>, &self.registry_bonds, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwForkChoiceOpeningRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let block_hashes = load!(Vec<String>, reader)?;
        let (rule_e_window_above_blue, registry_bonds) =
            if version >= 2 { (load!(Option<u64>, reader)?, load!(Vec<String>, reader)?) } else { (None, Vec::new()) };
        Ok(Self { block_hashes, rule_e_window_above_blue, registry_bonds })
    }
}

/// One block's opening, or why the node cannot weigh it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwForkChoiceEntry {
    pub block_hash: String,
    pub available: bool,
    pub reason: String,
    pub header: Option<RpcHeader>,
    /// The borsh fork-choice leaf (292 bytes, the weight-allocation slot included).
    pub leaf: Vec<u8>,
    /// The ADR-0043 root the leaf is enveloped with (128 hex).
    pub inner_root: String,
    /// The root the post-state is committed as (128 hex) — the envelope past the fence, the ADR-0043 root below it.
    pub committed_root: String,
    /// True past the fence (the committed root is the envelope).
    pub committed_form: bool,
    /// **RFC09 (v2):** the committed leaf in the form the post-state's point selects (v1, rule-E v2 or v3); empty when flat. Verify it
    /// with `palw_verify_committed_leaf_v2` against the committed root — the full node's own reader.
    #[serde(default)]
    pub committed_leaf: Vec<u8>,
    /// **RFC09 (v2):** borsh `PalwRuleEWindowOpeningV1` above the asked blue score (v3 only; empty: not asked or not v3).
    #[serde(default)]
    pub rule_e_window: Vec<u8>,
    /// **RFC09 (v2):** borsh `PalwRuleERegistryOpeningV3` of each asked bond, in the asked order (v3 only).
    #[serde(default)]
    pub registry_openings: Vec<Vec<u8>>,
}

impl Serializer for RpcPalwForkChoiceEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.reason, writer)?;
        serialize!(Option<RpcHeader>, &self.header, writer)?;
        store!(Vec<u8>, &self.leaf, writer)?;
        store!(String, &self.inner_root, writer)?;
        store!(String, &self.committed_root, writer)?;
        store!(bool, &self.committed_form, writer)?;
        store!(Vec<u8>, &self.committed_leaf, writer)?;
        store!(Vec<u8>, &self.rule_e_window, writer)?;
        store!(Vec<Vec<u8>>, &self.registry_openings, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwForkChoiceEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let mut entry = Self {
            block_hash: load!(String, reader)?,
            available: load!(bool, reader)?,
            reason: load!(String, reader)?,
            header: deserialize!(Option<RpcHeader>, reader)?,
            leaf: load!(Vec<u8>, reader)?,
            inner_root: load!(String, reader)?,
            committed_root: load!(String, reader)?,
            committed_form: load!(bool, reader)?,
            ..Default::default()
        };
        if version >= 2 {
            entry.committed_leaf = load!(Vec<u8>, reader)?;
            entry.rule_e_window = load!(Vec<u8>, reader)?;
            entry.registry_openings = load!(Vec<Vec<u8>>, reader)?;
        }
        Ok(entry)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwForkChoiceOpeningResponse {
    /// False with `reason` when the node serves no opening at all (no V2 ruleset, too many blocks named).
    pub available: bool,
    pub reason: String,
    /// The node's sink and DAG tips (128 hex each).
    pub sink: String,
    pub tips: Vec<String>,
    pub entries: Vec<RpcPalwForkChoiceEntry>,
    /// The node's DNS BFT gate facts (an issuer copies them into its attestation): whether the overlay is in its `Active` stage, and the
    /// confirmed DNS-final anchor (128 hex, empty with none) and its DAA. `dnsOverlay` false: the network runs no overlay.
    pub dns_overlay: bool,
    pub dns_stage_active: bool,
    pub dns_confirmed_anchor: String,
    pub dns_confirmed_anchor_daa: u64,
}

impl Serializer for GetPalwForkChoiceOpeningResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.reason, writer)?;
        store!(String, &self.sink, writer)?;
        store!(Vec<String>, &self.tips, writer)?;
        serialize!(Vec<RpcPalwForkChoiceEntry>, &self.entries, writer)?;
        store!(bool, &self.dns_overlay, writer)?;
        store!(bool, &self.dns_stage_active, writer)?;
        store!(String, &self.dns_confirmed_anchor, writer)?;
        store!(u64, &self.dns_confirmed_anchor_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwForkChoiceOpeningResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            reason: load!(String, reader)?,
            sink: load!(String, reader)?,
            tips: load!(Vec<String>, reader)?,
            entries: deserialize!(Vec<RpcPalwForkChoiceEntry>, reader)?,
            dns_overlay: load!(bool, reader)?,
            dns_stage_active: load!(bool, reader)?,
            dns_confirmed_anchor: load!(String, reader)?,
            dns_confirmed_anchor_daa: load!(u64, reader)?,
        })
    }
}

/// **RFC-0008 v2: `getPalwExecV2Status` (op 240)** — the EXEC v2 lane's observation of the tip: counts, the named work roots (or, when
/// none is named, the first ones in id order) with every slice and the state of its kernel-route verification claim, the refusal record
/// of `block_hash` when one is named, and the lane's health. Read-only: no rule reads it. **Op 240 is new: a node built before it drops
/// the WebSocket on it**, so a client asks it on a connection of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwExecV2StatusRequest {
    /// Root claim ids, 128 hex each (at most 64). Empty: the first roots.
    pub root_claim_ids: Vec<String>,
    /// A chain block whose refusal record to read (128 hex), or empty for none.
    pub block_hash: String,
}

impl Serializer for GetPalwExecV2StatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<String>, &self.root_claim_ids, writer)?;
        store!(String, &self.block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwExecV2StatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { root_claim_ids: load!(Vec<String>, reader)?, block_hash: load!(String, reader)? })
    }
}

/// **The `getPalwExecV2Status` answer.** `json` is the camelCase serde form of the node's `PalwExecV2ObservationV1`, whose own
/// `version` is `observation_version`: fields are only ever appended. Empty (and `available` false) where `palw_exec_payload_v2` is not
/// armed or before PALW state exists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwExecV2StatusResponse {
    pub available: bool,
    pub observation_version: u32,
    pub json: String,
}

impl Serializer for GetPalwExecV2StatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u32, &self.observation_version, writer)?;
        store!(String, &self.json, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwExecV2StatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, observation_version: load!(u32, reader)?, json: load!(String, reader)? })
    }
}

/// **RFC-0010: `getPalwPanelV3Status` (op 220)** — the permissionless Panel's observation of the tip: the engine's overview and the
/// status of the claims named (or, when none is, the first tracked claims in id order). Read-only: no rule reads it. **Op 220 is
/// new: a node built before it drops the WebSocket on it**, so a client asks it on a connection of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelV3StatusRequest {
    /// Claim ids, 128 hex each (at most 64). Empty: the first tracked claims.
    pub claim_ids: Vec<String>,
    /// At most this many tracked claims when none is named; 0 asks for the node's default (16); the node's cap is 64.
    pub limit: u32,
}

impl Serializer for GetPalwPanelV3StatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<String>, &self.claim_ids, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelV3StatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { claim_ids: load!(Vec<String>, reader)?, limit: load!(u32, reader)? })
    }
}

/// **The `getPalwPanelV3Status` answer.** `json` is the camelCase serde form of the node's `PanelV3ObservationV1`
/// (`overview`, `claims`, `unknown`), whose own `version` field is `observation_version`: fields are only ever appended, and a
/// reader of version 1 ignores what it does not know. Empty (and `available` false) off `ConsensusV2` or before PALW state exists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPanelV3StatusResponse {
    pub available: bool,
    pub observation_version: u32,
    pub json: String,
}

impl Serializer for GetPalwPanelV3StatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u32, &self.observation_version, writer)?;
        store!(String, &self.json, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPanelV3StatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, observation_version: load!(u32, reader)?, json: load!(String, reader)? })
    }
}
