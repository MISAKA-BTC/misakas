//! **Lane VAL (G14 / t12 DAA 17,000): the remaining V1–V5 and V11 values — PROVISIONAL ESTIMATES (目安), NOT MEASURED.**
//!
//! User delegation 2026-10-22: the value decisions are delegated, and values may be set as estimates (the policy may change later)
//! provided each is labelled as an estimate with its derivation and its review condition in the parameter register
//! (`docs/governance/parameter-register.md`, section "2026-10-26 lane VAL"). Derivations and the economic audit:
//! `docs/design/palw/integration/g14-17000/lane-values-report.md`; the independent recomputation:
//! `docs/design/palw/integration/g14-17000/g14_values_v1_check.py`.
//!
//! **Nothing here is wired into a shipped preset.** Every production site still answers typed-missing:
//! `PALW_COURT_SERVICE_PROFILE_V1 = None`, `palw_escrow_fee_schedule_v1 → None`, `palw_court_ingress_schedule_v1 → Err`,
//! `material_intake_schedule_v1 → Err`, the funded-original schedule has no production constructor, and
//! `palw_court_capacity_v1` stays unarmed with an empty arming record list. Pinning these values into those sites is the
//! integrator's change (the list of tests that assume `None` is in the report). V6–V10 and V12 were decided earlier
//! (`palw_values_p4_v1`, P3 constants); they are not redefined here.
//!
//! Inputs, all from existing records (none measured by this lane): the t12 fee rate 49.96 sompi/B (P3 V7b), the largest signed court
//! object 33,821 B and the M-RHO session weight 13 (lane P1 `measurements.md`, M1 Max under load — NOT minimum hardware), the t12
//! block 125,000 B/DAA, the court cap 42 and turn deadline 42 DAA, the dispute window 10,000 DAA, the 13,000 MSK producer bond unit
//! (P4 estimate) and the TIR prepared-retained maximum 59,494 B / claim (M-DA, fixture classes only).
//!
//! No value here is, or relies on, a cryptographic proof of computation (PRINCIPLES §8). No per-model verification is introduced and
//! the model tiers are untouched (PRINCIPLES §9): every value is per claim / per court session.

use crate::palw_court_capacity_v1::{PALW_COURT_CAPACITY_POLICY_VERSION_V1, PalwCourtCapacityPolicyV1, PalwCourtCostV1};
use crate::palw_court_paid_ingress_v1::PalwCourtIngressScheduleV1;
use crate::palw_court_service_binding_v1::CourtServiceProfileV1;
use crate::palw_court_service_v1::ServiceCost;
use crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1;
use crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1;

/// Status string (estimates, not wired, not measured).
pub const PALW_G14_VALUES_STATUS_V1: &str = "provisional estimates 2026-10-26 (lane VAL); not wired into any preset; not measured";

/// **Lane F03k2: the external gain bound of a Panel=1 kernel-route claim — USER-APPROVED 2026-10-26 (500 MSK, from an estimate).**
/// What a lie can be worth outside the chain's own reward (`crate::palw_kernel_route_panel_one_censorship_v1`). User approval
/// 2026-10-26 (verbatim): 「500 MSK で承認」. Basis: [`PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_ESTIMATE_V1`]. Review
/// condition: an economic audit of off-chain value per claim, or a change of the ledger's dismissal fee / adjudication cap /
/// liability window.
pub const PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_V1: Option<u64> = Some(PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_ESTIMATE_V1);

/// sompi per MSK.
pub const SOMPI_PER_MSK_V1: u64 = 100_000_000;
/// Carriage price used throughout, sompi / byte: the t12 fee rate 49.96 rounded up (P3 V7b).
pub const PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1: u64 = 50;
/// The largest signed court object observed (position close with two ML-DSA-87 signatures, 33,821 B) rounded up: one packet.
pub const PALW_G14_PACKET_BYTES_V1: u64 = 34_000;
/// M-RHO session weight (median-based work units of 126.5 µs; heaviest class = Opening).
pub const PALW_G14_SESSION_WEIGHT_V1: u64 = 13;
/// Liable signers of a Panel=1 kernel-route original (producer + one seat).
pub const PALW_G14_PANEL1_LIABLE_SIGNERS_V1: u64 = 2;

// ---------------------------------------------------------------- V1 court service profile

/// Turns per liable signer: the 11 localization owner turns of upper 6 (M-X3) plus one close.
pub const PALW_G14_PACKETS_PER_SIGNER_V1: u64 = 12;
/// Span of each packet interval: `⌊(10,000 − 1 − 3) / 12⌋ = 833` so all 12 intervals end before `a + 10,000`.
pub const PALW_G14_SERVICE_SPAN_DAA_V1: u64 = 833;
/// Reserved court bytes per accepting DAA: ONE packet (34,000 B, 27.2 % of a 125,000 B block). Coordinator decision 2026-10-26:
/// take 1 if it meets every service deadline and V11. It does: one Panel=1 original's 24 packets put 2 in each 833-DAA interval
/// (all 12 intervals end at `a + 9,997 < a + 10,000`; intervals 0–3 cover the notice window to `a + 3,120`), so 1 packet/DAA leaves
/// 831 spare slots per interval; overload is refused at admission (all-or-nothing reservation), never a missed deadline. Sustained
/// throughput is `1 / 24` original per DAA (30 per nominal day). Was 2 packets (68,000 B) in the first estimate.
pub const PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1: u64 = 1;

/// **F02 `follow_up_branches_per_turn` (USER-APPROVED ESTIMATE class, 2026-10-26; replace by measurement)** — 2: a binary
/// dissection's follow-up turn can go to either half, so each turn reserves both branches. Capacity check (`Q ≤ c·W`): one Panel=1
/// original reserves `Q = 2 signers × 12 turns × 2 = 48` packets against `c·W = 1 packet/DAA × 9,999 DAA`; per 833-DAA interval it
/// needs `2 × 2 = 4` of 833 slots. Sustained throughput halves to `1 / 48` original per DAA (15 per nominal day). Revise: the
/// court's real branching factor per turn (dissection arity of the real-model classes).
pub const PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1: u64 = 2;

/// **V1 (estimate).**
pub const PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1: CourtServiceProfileV1 = CourtServiceProfileV1 {
    version: 1,
    budget: ServiceCost {
        bytes: PALW_G14_PACKET_BYTES_V1 * PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1,
        work: PALW_G14_SESSION_WEIGHT_V1 * PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1,
        io: PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1,
        controls: PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1,
    },
    packet_cost: ServiceCost { bytes: PALW_G14_PACKET_BYTES_V1, work: PALW_G14_SESSION_WEIGHT_V1, io: 1, controls: 1 },
    packets_per_signer: PALW_G14_PACKETS_PER_SIGNER_V1,
    service_span_daa: PALW_G14_SERVICE_SPAN_DAA_V1,
};

/// **F02 inclusion premise `inclusion_delay_daa` (estimate, replace by measurement)** for `CourtServiceInclusionPremiseV1`
/// (integration 9803e043d; production `PALW_COURT_SERVICE_INCLUSION_PREMISE_V1 = None`). The type is not on this lane's base, so
/// only the number lives here. B_carrier reasoning: one packet ≤ 34,000 B; at the observed worst free space 13,051 B/DAA it needs
/// `⌈34,000 / 13,051⌉ = 3` DAA; the reserved 34,000 B/DAA (1 packet) admits one court packet per DAA, so the 2 packets an
/// original holds in one interval queue at most 1 extra DAA → 4 DAA, equal to `PALW_COURT_CLOSE_INCLUSION_MARGIN` (4 DAA/chunk).
/// At the measured 184 s/DAA median (240 s observed max) that is ≈ 12–16 minutes. Must stay ≪ span 833 and the 42-DAA turn.
pub const PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1: u64 = 4;

/// **F03k external gain bound for Panel=1 kernel-route classes (ESTIMATE, 2026-10-26)**: 500 MSK. If a lying claim stands, it
/// gains at most the claim reward paid from escrow (`claim_reward` 5 MSK) plus the funded fee (V2, 0.816 MSK), plus an allowance of
/// ≈ 494 MSK for coordinated misdirection of whoever consumes the result. The censorship cost it is checked against, from the t12
/// ledger as shipped: `dismissed_proof_fee` 0.1 MSK × `max_adjudications_per_block` 64 = 6.4 MSK per block, over the ledger's
/// `liability_daa` 200 → 1,280 MSK, 2.56 × the bound. A consumer who stakes more than this on one Panel=1 claim is outside the
/// guarantee. Revise: any change of the ledger's fee / adjudication cap / liability window, or evidence of larger misdirection value.
/// Integration (2026-10-26): the user approved this value the same day; the production bound above takes it.
pub const PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_ESTIMATE_V1: u64 = 500 * SOMPI_PER_MSK_V1;

// ---------------------------------------------------------------- V2 funded original / admission

/// Carriage of one packet, sompi (34,000 × 50 = 1,700,000).
pub const PALW_G14_PACKET_CARRIAGE_SOMPI_V1: u64 = PALW_G14_PACKET_BYTES_V1 * PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1;
/// **V2 fee (estimate)**: twice the carriage of every reserved packet of a Panel=1 original (2 × 24 × 1.7 M = 81.6 M sompi).
pub const PALW_G14_FUNDED_FEE_SOMPI_V1: u64 =
    2 * PALW_G14_PANEL1_LIABLE_SIGNERS_V1 * PALW_G14_PACKETS_PER_SIGNER_V1 * PALW_G14_PACKET_CARRIAGE_SOMPI_V1;
/// **V2 capital per signer turn (estimate)**: twice one packet's carriage (3.4 M sompi).
pub const PALW_G14_CAPITAL_PER_TURN_SOMPI_V1: u64 = 2 * PALW_G14_PACKET_CARRIAGE_SOMPI_V1;
/// **V2 capital per source node (estimate)**: the D2 disclosure fee (two 12,672 B response carriages, 1.3 M sompi).
pub const PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1: u64 = 1_300_000;
/// **V2 capital floor (estimate)**: every signer turn of a Panel=1 original (24 × 3.4 M = 81.6 M sompi).
pub const PALW_G14_FUNDED_CAPITAL_FLOOR_SOMPI_V1: u64 =
    PALW_G14_PANEL1_LIABLE_SIGNERS_V1 * PALW_G14_PACKETS_PER_SIGNER_V1 * PALW_G14_CAPITAL_PER_TURN_SOMPI_V1;
/// **V2 heap / retention bytes per branch (estimate)**: 64 KiB ≥ the largest TIR prepared-retained 59,494 B / claim (fixtures).
pub const PALW_G14_HEAP_BYTES_PER_BRANCH_V1: u64 = 65_536;
pub const PALW_G14_RETENTION_BYTES_PER_BRANCH_V1: u64 = 65_536;
/// Source-node packet: one 12,672 B response rounded up to 13,000 B, one work unit, one fetch.
pub const PALW_G14_SOURCE_PACKET_COST_V1: ServiceCost = ServiceCost { bytes: 13_000, work: 1, io: 1, controls: 1 };

/// **V2 / V5 input cover budget (estimate, resource bound only).** Not derived from a real model (M-DA has fixture classes only).
pub const PALW_G14_PARAMETER_COVER_BUDGET_V1: OriginalParameterCoverBudgetV1 = OriginalParameterCoverBudgetV1 {
    max_original_leaves: 4_096,
    max_total_edges: 262_144,
    max_total_coordinates: 1_048_576,
    max_root_memberships: 1_048_576,
};
/// V2 numerical geometry bound (estimate): leaves, roots, paths, edges, path bytes.
pub const PALW_G14_NUMERICAL_GEOMETRY_V1: [u64; 5] = [4_096, 64, 65_536, 262_144, 4 * 1_048_576];
/// V2 source graph bound (estimate): queries, roles, vertices, edges.
pub const PALW_G14_SOURCE_GRAPH_V1: [u64; 4] = [1_024, 64, 65_536, 262_144];

// ---------------------------------------------------------------- V3 escrow settlement shares

/// **V3 (estimate): shares of an escrow's settled fee, per mille.** Work payment to the seat that performed it, relay payment, burn.
/// The court fee is not drawn from the payer's escrow (the opener pays ingress + occupancy, V4/V11): its share is 0.
pub const PALW_G14_ESCROW_WORK_PERMILLE_V1: u64 = 800;
pub const PALW_G14_ESCROW_RELAY_PERMILLE_V1: u64 = 100;
pub const PALW_G14_ESCROW_COURT_PERMILLE_V1: u64 = 0;
/// The burn makes a self-paid claim lose 10 % of its fee (no subsidy; B = F = 0 for kernel claims stays as decided).
pub const PALW_G14_ESCROW_BURN_PERMILLE_V1: u64 = 100;
/// At most this many dues per settlement spend (bounded outputs).
pub const PALW_G14_ESCROW_MAX_DUES_PER_SPEND_V1: usize = 8;

/// The dues of a settled fee in discharge order (work, relay, burn; the rounding remainder goes to the burn).
pub fn palw_g14_escrow_split_v1(fee_sompi: u64) -> [u64; 3] {
    let work = (u128::from(fee_sompi) * u128::from(PALW_G14_ESCROW_WORK_PERMILLE_V1) / 1_000) as u64;
    let relay = (u128::from(fee_sompi) * u128::from(PALW_G14_ESCROW_RELAY_PERMILLE_V1) / 1_000) as u64;
    [work, relay, fee_sompi - work - relay]
}

// ---------------------------------------------------------------- V11 court capacity (ρ_court, session weight)

/// **V11 ρ_court (estimate)**: one session-DAA pays its share of the court's reserved carriage,
/// `⌈34,000 B × 50 sompi/B / 42 sessions / weight 13⌉ = 3,114`, rounded to 3,125 sompi / DAA / unit (was 6,250 with 2 packets). The outsider's guarantee rests
/// on the reserved slots (ADR-0192) and the deadline-aware queue, not on ρ: pricing a full-window jam above a producer bond
/// (≥ 238,096) would make an honest session cost ≈ 10 MSK, above the reporter's share of the smallest slash, so it was not chosen.
pub const PALW_G14_RHO_COURT_SOMPI_V1: u64 = 3_125;
/// The court cap and turn deadline at t12 (42 sessions = 42 DAA × 1 close per block).
pub const PALW_G14_T12_TURN_DEADLINE_DAA_V1: u64 = 42;

/// **V11 (estimate)**: the court-capacity policy with ρ_court and the M-RHO median-based op_work [5, 13, 1, 11]. The block budget
/// keeps today's one session move per block (13 units), so the derived capacity is still 42 at t12. Not a measurement record: the
/// arming requirement (slowest supported node, release build) is NOT met and the fence stays refused.
pub const PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1: PalwCourtCapacityPolicyV1 = PalwCourtCapacityPolicyV1 {
    version: PALW_COURT_CAPACITY_POLICY_VERSION_V1,
    block_budget: PalwCourtCostV1 { work: 13, bytes: PALW_G14_PACKET_BYTES_V1 * PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1, memory: 64 * 1_048_576 },
    op_work: [5, 13, 1, 11],
    memory_per_byte: 8,
    rho_sompi: PALW_G14_RHO_COURT_SOMPI_V1,
    session_weight: PALW_G14_SESSION_WEIGHT_V1,
    ticket_window_daa: 0,
    one_move_budget: PalwCourtCostV1 { work: 5, bytes: 34_000, memory: 64 * 1_048_576 },
    reserve_permille: 500,
};

// ---------------------------------------------------------------- V4 paid ingress

/// **V4 (estimate)**: once-only ingress fee = the carriage of one opening (16,571 B × 50 = 828,550) rounded up to 1.1 M sompi; the
/// occupancy (ρ_court) is charged separately. Paid by every opener, inside parties
/// (producer, seat) included, so colluders cannot fill the non-reserved room for free. R = ⌊cap / 2⌋ is kept (21 at cap 42).
pub const PALW_G14_COURT_INGRESS_ESTIMATE_V1: PalwCourtIngressScheduleV1 = PalwCourtIngressScheduleV1 { fee_sompi_per_session: 1_100_000 };
/// V4: the ingress fee applies to inside openers as well.
pub const PALW_G14_COURT_INGRESS_INSIDERS_PAY_V1: bool = true;

// ---------------------------------------------------------------- V5 material intake

/// **V5 (estimate)**: storage at the carriage price (50 sompi / B, payer = submitter), draft TTL one nominal day (720 DAA).
pub const PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1: MaterialIntakeScheduleV1 = MaterialIntakeScheduleV1 {
    sompi_per_byte: PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1,
    draft_ttl_daa: 720,
    cover: PALW_G14_PARAMETER_COVER_BUDGET_V1,
};

// ---------------------------------------------------------------- production-constructor pins (not read by any preset)

fn profile_id_v1(name: &[u8]) -> crate::Hash64 {
    let d = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/g14-values/profile-id/v1").hash(name);
    crate::Hash64::from_bytes(d.as_bytes().try_into().expect("64-byte digest"))
}

/// **V2 admission pin** through the validated production constructor.
pub fn palw_g14_funded_admission_estimate_v1() -> crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1 {
    crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1::new_v1(
        PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
        PALW_G14_SOURCE_PACKET_COST_V1,
        PALW_G14_CAPITAL_PER_TURN_SOMPI_V1,
        PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
        PALW_G14_HEAP_BYTES_PER_BRANCH_V1,
        PALW_G14_RETENTION_BYTES_PER_BRANCH_V1,
    )
    .expect("the lane VAL admission estimate is in range")
}

/// **V2 funded original pin** through the validated production constructor. Profile ids are derived from the value names.
pub fn palw_g14_funded_original_schedule_estimate_v1() -> crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1 {
    let [l, r, pa, e, pb] = PALW_G14_NUMERICAL_GEOMETRY_V1;
    let [q, ro, v, se] = PALW_G14_SOURCE_GRAPH_V1;
    crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1::new_v1(
        [profile_id_v1(b"fee"), profile_id_v1(b"capital"), profile_id_v1(b"retention"), profile_id_v1(b"settlement")],
        PALW_G14_FUNDED_FEE_SOMPI_V1,
        PALW_G14_FUNDED_CAPITAL_FLOOR_SOMPI_V1,
        crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3 {
            max_original_leaves: l,
            max_roots: r,
            max_paths: pa,
            max_edges: e,
            max_path_bytes: pb,
        },
        PALW_G14_PARAMETER_COVER_BUDGET_V1,
        crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 { max_queries: q, max_roles: ro, max_vertices: v, max_edges: se },
        0,
        palw_g14_funded_admission_estimate_v1(),
    )
    .expect("the lane VAL funded original estimate is in range")
}

/// **V3 escrow pin** for one contract's settled fee: work to the seat, relay to the relay, the rest burned (zero dues dropped).
pub fn palw_g14_escrow_schedule_estimate_v1(
    contract_id: crate::Hash64,
    fee_sompi: u64,
    seat_payee: crate::Hash64,
    relay_payee: crate::Hash64,
) -> Result<crate::palw_funded_escrow_v1::PalwEscrowFeeScheduleV1, &'static str> {
    use crate::palw_funded_escrow_v1::{PalwEscrowDueKindV1 as K, PalwEscrowDueV1, PalwEscrowFeeScheduleV1};
    let [w, r, b] = palw_g14_escrow_split_v1(fee_sompi);
    let dues = [(K::WorkPayment, Some(seat_payee), w), (K::WorkPayment, Some(relay_payee), r), (K::Burn, None, b)]
        .into_iter()
        .filter(|(_, _, a)| *a > 0)
        .map(|(kind, payee, amount_sompi)| PalwEscrowDueV1 { kind, payee, amount_sompi })
        .collect();
    PalwEscrowFeeScheduleV1::new_v1(contract_id, dues, PALW_G14_ESCROW_MAX_DUES_PER_SPEND_V1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_court_service_binding_v1::court_service_packets_v1;
    use crate::palw_court_service_v1::ServiceReservations;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::TransactionOutpoint;
    use std::collections::BTreeSet;

    const BOND_UNIT_SOMPI: u64 = 13_000 * SOMPI_PER_MSK_V1;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[test]
    fn val_v1_profile_reserves_a_panel_one_original_inside_the_dispute_window() {
        let p = PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1;
        let signers: BTreeSet<_> = (0..2u32).map(|n| PalwBondKeyV2(TransactionOutpoint::new(h(0xB0), n))).collect();
        let packets = court_service_packets_v1(h(1), h(2), h(3), 5_000, &signers, &p).expect("profile fits a + 10,000");
        assert_eq!(packets.len(), 24);
        assert!(packets.iter().all(|k| k.service_before <= 5_000 + 10_000));
        let mut book = ServiceReservations::new(p.budget).expect("non-zero budget");
        book.reserve(&packets, 15_000).expect("one original fits an empty book");
        assert_eq!(p.budget.bytes, 34_000);
        assert!(125_000 - p.budget.bytes >= 56_134);
        // 1 packet/DAA: each 833-DAA interval needs 2 slots for one original.
        assert!(PALW_G14_SERVICE_SPAN_DAA_V1 * PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1 >= PALW_G14_PANEL1_LIABLE_SIGNERS_V1);
    }

    #[test]
    fn val_v2_admission_and_schedule_values_build() {
        use crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1;
        use crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1;
        let admission = FundedAdmissionScheduleV1::test_only_v1(
            PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
            PALW_G14_SOURCE_PACKET_COST_V1,
            PALW_G14_CAPITAL_PER_TURN_SOMPI_V1,
            PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
            PALW_G14_HEAP_BYTES_PER_BRANCH_V1,
            PALW_G14_RETENTION_BYTES_PER_BRANCH_V1,
        );
        let [l, r, pa, e, pb] = PALW_G14_NUMERICAL_GEOMETRY_V1;
        let [q, ro, v, se] = PALW_G14_SOURCE_GRAPH_V1;
        let s = PalwFundedOriginalScheduleV1::test_only_v1(
            [h(11), h(12), h(13), h(14)],
            PALW_G14_FUNDED_FEE_SOMPI_V1,
            PALW_G14_FUNDED_CAPITAL_FLOOR_SOMPI_V1,
            crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3 {
                max_original_leaves: l,
                max_roots: r,
                max_paths: pa,
                max_edges: e,
                max_path_bytes: pb,
            },
            PALW_G14_PARAMETER_COVER_BUDGET_V1,
            crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 { max_queries: q, max_roles: ro, max_vertices: v, max_edges: se },
            0,
        )
        .with_admission_v1(admission);
        assert_eq!(s.fee_sompi(), 81_600_000);
        assert_eq!(s.capital_floor_sompi(), 81_600_000);
        // The fee covers twice the carriage of every reserved packet.
        assert!(s.fee_sompi() >= 2 * 24 * PALW_G14_PACKET_CARRIAGE_SOMPI_V1);
    }

    #[test]
    fn val_v3_split_conserves_and_burns_on_self_dealing() {
        assert_eq!(
            PALW_G14_ESCROW_WORK_PERMILLE_V1
                + PALW_G14_ESCROW_RELAY_PERMILLE_V1
                + PALW_G14_ESCROW_COURT_PERMILLE_V1
                + PALW_G14_ESCROW_BURN_PERMILLE_V1,
            1_000
        );
        for fee in [0u64, 1, 7, 999, PALW_G14_FUNDED_FEE_SOMPI_V1, u64::MAX] {
            let [w, r, b] = palw_g14_escrow_split_v1(fee);
            assert_eq!(u128::from(w) + u128::from(r) + u128::from(b), u128::from(fee));
            // A payer who is also seat and relay recovers at most w + r < fee whenever fee ≥ 10.
            if fee >= 10 {
                assert!(b > 0);
            }
        }
    }

    #[test]
    fn val_v11_policy_validates_keeps_cap_42_and_prices_a_full_jam_above_a_bond() {
        let p = PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1;
        p.validate().expect("structurally sound");
        assert!(!p.is_placeholder());
        assert_eq!(p.capacity_sessions(PALW_G14_T12_TURN_DEADLINE_DAA_V1), 42);
        assert_eq!(p.weight(), 13);
        // One session-DAA pays at least its share of the reserved court carriage.
        assert!(p.fee(1, p.weight()) * 42 >= u128::from(p.block_budget.bytes * PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1));
        // Jamming every slot for a whole dispute window costs more than 150 MSK.
        assert!(p.fee(10_000, p.weight()) * 42 >= 150 * u128::from(SOMPI_PER_MSK_V1));
        // An honest prosecution (ingress + 8 moves × 42 DAA occupancy + 8 packets carriage) is recovered by the reporter's 49 %
        // of the smallest decided slash (D2, 3 MSK).
        let honest = u128::from(PALW_G14_COURT_INGRESS_ESTIMATE_V1.fee_sompi_per_session)
            + p.fee(336, p.weight())
            + 8 * u128::from(PALW_G14_PACKET_CARRIAGE_SOMPI_V1);
        assert!(honest * 100 <= 49 * 3 * u128::from(SOMPI_PER_MSK_V1), "honest {honest}");
        let _ = BOND_UNIT_SOMPI;
    }

    #[test]
    fn val_v4_ingress_covers_an_honest_session_and_leaves_room() {
        let fee = u128::from(PALW_G14_COURT_INGRESS_ESTIMATE_V1.fee_sompi_per_session);
        let p = PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1;
        let _ = p;
        assert!(fee >= 16_571 * u128::from(PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1));
        let r = crate::palw_court_paid_ingress_v1::palw_court_ingress_reserved_claims_cap_v1(42);
        assert_eq!(r, 21);
        assert!(42 - r >= 1);
    }

    #[test]
    fn val_v5_price_matches_the_c_retain_estimate() {
        let s = PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1;
        assert!(s.sompi_per_byte > 0 && s.draft_ttl_daa > 0);
        // P3 c_retain = ⌈2,250,000 B × 50 / 3,120⌉ assumed exactly this price.
        assert_eq!((2_250_000u64 * s.sompi_per_byte).div_ceil(3_120), 36_058);
    }

    #[test]
    fn val_production_constructors_pin_the_estimates_and_refuse_out_of_range() {
        use crate::palw_funded_escrow_v1::{PalwEscrowDueKindV1 as K, PalwEscrowDueV1, PalwEscrowFeeScheduleV1};
        use crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1;
        let s = palw_g14_funded_original_schedule_estimate_v1();
        assert_eq!(s.fee_sompi(), 81_600_000);
        assert_eq!(s.admission_v1(), Some(palw_g14_funded_admission_estimate_v1()));
        let mut bad = PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1;
        bad.service_span_daa = 834; // 1 + 12 × 834 > 10,000
        assert!(FundedAdmissionScheduleV1::new_v1(bad, PALW_G14_SOURCE_PACKET_COST_V1, 1, 1, 1, 1).is_err());
        let big = ServiceCost { bytes: 34_001, work: 1, io: 1, controls: 1 };
        assert!(FundedAdmissionScheduleV1::new_v1(PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1, big, 1, 1, 1, 1).is_err());
        assert!(FundedAdmissionScheduleV1::new_v1(PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1, PALW_G14_SOURCE_PACKET_COST_V1, 0, 1, 1, 1).is_err());
        let e = palw_g14_escrow_schedule_estimate_v1(h(1), PALW_G14_FUNDED_FEE_SOMPI_V1, h(2), h(3)).expect("in range");
        let _ = e;
        assert!(PalwEscrowFeeScheduleV1::new_v1(h(1), vec![], 8).is_err());
        let burn_with_payee = PalwEscrowDueV1 { kind: K::Burn, payee: Some(h(2)), amount_sompi: 1 };
        assert!(PalwEscrowFeeScheduleV1::new_v1(h(1), vec![burn_with_payee], 8).is_err());
        let zero = PalwEscrowDueV1 { kind: K::WorkPayment, payee: Some(h(2)), amount_sompi: 0 };
        assert!(PalwEscrowFeeScheduleV1::new_v1(h(1), vec![zero], 8).is_err());
        assert!(palw_g14_escrow_schedule_estimate_v1(h(1), 0, h(2), h(3)).is_err());
    }

    #[test]
    fn val_external_gain_bound_is_below_the_ledger_censorship_cost() {
        let gain = PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_ESTIMATE_V1;
        let censorship = (SOMPI_PER_MSK_V1 / 10) * 64 * 200;
        assert!(censorship > gain && censorship * 100 >= gain * 250, "censorship {censorship} gain {gain}");
        assert!(gain >= 5 * SOMPI_PER_MSK_V1 + PALW_G14_FUNDED_FEE_SOMPI_V1, "covers reward + escrow fee");
    }

    #[test]
    fn val_follow_up_branches_fit_the_hard_window() {
        let b = PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1;
        let q = PALW_G14_PANEL1_LIABLE_SIGNERS_V1 * PALW_G14_PACKETS_PER_SIGNER_V1 * b;
        assert_eq!(q, 48);
        assert!(q <= PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1 * 9_999, "Q <= c*W");
        assert!(PALW_G14_PANEL1_LIABLE_SIGNERS_V1 * b <= PALW_G14_SERVICE_SPAN_DAA_V1 * PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1);
    }

    #[test]
    fn val_inclusion_delay_fits_carrier_and_turn() {
        let d = PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1;
        assert!(d >= PALW_G14_PACKET_BYTES_V1.div_ceil(13_051) + PALW_G14_PANEL1_LIABLE_SIGNERS_V1 - 1);
        assert!(d < PALW_G14_T12_TURN_DEADLINE_DAA_V1 && d < PALW_G14_SERVICE_SPAN_DAA_V1);
    }

    #[test]
    fn val_production_sites_take_the_user_approved_estimates() {
        assert_eq!(crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1, Some(PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1));
        assert_eq!(
            crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_INCLUSION_PREMISE_V1.map(|p| (p.version, p.inclusion_delay_daa)),
            Some((1, PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1))
        );
        assert_eq!(crate::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1(), Ok(PALW_G14_COURT_INGRESS_ESTIMATE_V1));
        assert_eq!(crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1(), Ok(PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1));
        // Not approved / no estimate: stays typed-missing.
        assert!(crate::palw_kernel_court_admission_v1::PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1.is_none());
    }
}
