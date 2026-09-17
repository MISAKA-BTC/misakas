//! ADR-0131 Decision 1 — `misaka palw economics`: what each class is paid per unit of the compute it
//! ran, on today's leaf basis and on the economic-compute bases, from what the node holds.
//!
//! The node answers `getPalwClassEconomics` (op 185) with each class's registration numbers, its
//! target, its job's economic compute and a census of its claims; this module prices every class on
//! every basis with the SAME functions consensus-core tests (`palw_economic_compute_v1`), and prints
//! `F_m` (sompi per 10⁹ MAC-equivalents of `Final` compute), `A_m` (per attempted compute, voided
//! claims and the draws included), the panel's pool per unit of verification compute, and the gap
//! `max / min − 1` between every pair of model classes. Nothing here is a rule: it is the shadow.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMICS_RATE_SCALE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, PalwClassEconomicsInputV1, PalwClassEconomicsV1,
    PalwClassMeasureV1, PalwRewardBasisV1, palw_basis_unit_v1, palw_class_economics_v1, palw_gap_permille_v1,
    palw_priced_reward_u128_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::{PALW_LEDGER_RATE_SCALE_V1, palw_rate_priced_reward_v1};
use kaspa_consensus_core::palw_verification_profile_v1::{
    PALW_SPAN_ANCHORS_V1, PALW_VERIFICATION_REFERENCE_V1, PalwClassTimingFactsV1, PalwPanelCapacityInputV1, PalwPanelCapacityV1,
    PalwVerificationProfileV1, palw_panel_capacity_v1, palw_verification_profile_v1,
};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwClassEconomicsResponse, RpcPalwClassEconomics};
use serde_json::json;

/// The bases, in the order the report prints them.
pub(crate) const BASES: [(PalwRewardBasisV1, &str); 3] = [
    (PalwRewardBasisV1::CurrentLeaves, "leaves"),
    (PalwRewardBasisV1::EconomicJob, "economic_job"),
    (PalwRewardBasisV1::EconomicAttempted, "economic_attempted"),
];

/// One class, priced on one basis, with the census it was priced from.
pub(crate) struct ClassOnBasis {
    pub(crate) name: String,
    pub(crate) class_id: String,
    pub(crate) is_base_class: bool,
    pub(crate) sets_unit: bool,
    pub(crate) economics: PalwClassEconomicsV1,
}

/// ADR-0132: one model class's end-to-end reading under every basis — what its `Final`s would have
/// been paid per attempted MAC-eq at the class's ACTUAL Final rate, on today's basis, the two
/// economic bases and proposal C's rate.
pub(crate) struct ClassEndToEnd {
    pub(crate) name: String,
    pub(crate) class_id: String,
    /// The escrow one `Final` holds (the ledger's, else the census's per accepted claim).
    pub(crate) escrow_per_claim: u64,
    /// Class draws × network draws × one draw's job.
    pub(crate) attempted_per_claim: u128,
    pub(crate) finals: u64,
    pub(crate) attempted_compute: u128,
    /// `(basis name, reward per Final, sompi per 10⁹ MAC-eq attempted over the ledger's window)`.
    pub(crate) under: Vec<(&'static str, u64, u128)>,
    /// ADR-0133 §9a: proposal C's reward before the escrow cap, after it, how much of the cap it
    /// uses (permille), and whether it saturates — a class above 80 % cap utilization is not
    /// activatable as a paid class.
    pub(crate) uncapped_economic_reward: u128,
    pub(crate) capped_reward: u64,
    pub(crate) cap_utilization_permille: u32,
    pub(crate) cap_saturated: bool,
}

/// The cap utilization above which a class is not activatable as a paid class (ADR-0133 §9a).
pub(crate) const PALW_CAP_UTILIZATION_ACTIVATION_LIMIT_PERMILLE: u32 = 800;

/// The whole report: per basis, every class and the gaps between the model classes.
pub(crate) struct Report {
    pub(crate) tip_daa: u64,
    pub(crate) seat_count: u16,
    pub(crate) prefill_draw: bool,
    pub(crate) economic_compute_version: u16,
    /// ADR-0132: the tip's `bits` and the network draws a class win costs at it.
    pub(crate) network_bits: u32,
    pub(crate) network_expected_attempts_q32: u128,
    /// ADR-0132: the node's end-to-end ledger — whether it exists, its span, and the gaps of the
    /// three actual metrics among the model classes (`None` with the names of the classes paid
    /// nothing where the gap is infinite).
    pub(crate) ledger_available: bool,
    pub(crate) ledger_claims: u64,
    pub(crate) ledger_first_daa: u64,
    pub(crate) ledger_last_daa: u64,
    pub(crate) actual_gaps: [ActualGap; 3],
    pub(crate) end_to_end: Vec<ClassEndToEnd>,
    /// The rate proposal C prices with here: the constant that pays the heaviest live model class's
    /// attempted compute its escrow whole (sompi per 10⁹ MAC-eq).
    pub(crate) rate_sompi_per_giga: u128,
    /// ADR-0133: each model class's verification profile as the reference derives it from its
    /// compute and this node's measured replays, and its panel's capacity at the ledger's rate.
    pub(crate) capacity: Vec<ClassCapacity>,
    pub(crate) rows: Vec<RpcPalwClassEconomics>,
    /// `(basis name, unit, classes, gap_final, gap_attempted, gap_panel)` — the gaps between the
    /// model classes' `F_m`, `A_m` and panel rates, `None` where a class was paid nothing per unit.
    pub(crate) bases: Vec<(&'static str, u128, Vec<ClassOnBasis>, Option<u128>, Option<u128>, Option<u128>)>,
}

fn parse_u128(s: &str) -> u128 {
    s.parse().unwrap_or(0)
}

/// A Q32 count as a decimal with three places, in integers.
fn q32_decimal(s: &str) -> String {
    let v = parse_u128(s);
    format!("{}.{:03}", v >> 32, ((v & 0xFFFF_FFFF) * 1000) >> 32)
}

fn measure_of(row: &RpcPalwClassEconomics, network_expected_attempts_q32: u128) -> PalwClassMeasureV1 {
    PalwClassMeasureV1 {
        leaves: row.pwu_per_inference,
        draw_compute: parse_u128(&row.economic_compute_job),
        expected_attempts_q32: parse_u128(&row.expected_attempts_q32),
        network_expected_attempts_q32,
        // ADR-0124 Decision 6's unit: an `Active`, weight-bearing (share above zero) model class.
        sets_unit: !row.is_base_class && row.share_permille > 0 && row.status == "Active" && row.economic_source != "unknown",
        // …and the floor is not priced at all.
        priced: !row.is_base_class,
    }
}

/// The name a row prints under: the model the build names, else the class id's head.
fn name_of(row: &RpcPalwClassEconomics) -> String {
    if row.model_id.is_empty() { format!("{}…", &row.class_id[..row.class_id.len().min(16)]) } else { row.model_id.clone() }
}

/// **Price every class on every basis** from the node's answer.
/// One actual metric's gap among the model classes with claims: `max / min − 1` in permille, or
/// `None` and the classes paid nothing (an infinite gap), or `None` and no names (nothing to compare).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActualGap {
    pub(crate) metric: &'static str,
    pub(crate) gap_permille: Option<u128>,
    pub(crate) paid_nothing: Vec<String>,
}

fn actual_gap(metric: &'static str, rows: &[RpcPalwClassEconomics], rate: impl Fn(&RpcPalwClassEconomics) -> u128) -> ActualGap {
    let mut paid = Vec::new();
    let mut paid_nothing = Vec::new();
    for row in rows.iter().filter(|r| !r.is_base_class && r.ledger.available && r.ledger.claims > 0) {
        match rate(row) {
            0 => paid_nothing.push(name_of(row)),
            r => paid.push(r),
        }
    }
    let gap_permille = if !paid_nothing.is_empty() || paid.len() < 2 {
        None
    } else {
        palw_gap_permille_v1(*paid.iter().max().unwrap(), *paid.iter().min().unwrap())
    };
    ActualGap { metric, gap_permille, paid_nothing }
}

/// ADR-0132 §4: every model class's `Final`s priced under each basis at the class's actual Final
/// rate, over what its producers ran — the end-to-end comparison `current` vs `EconomicJob` vs
/// `EconomicAttempted` vs proposal C's rate.
fn end_to_end(r: &GetPalwClassEconomicsResponse, measures: &[PalwClassMeasureV1]) -> (Vec<ClassEndToEnd>, u128) {
    let attempted_per_claim = |m: &PalwClassMeasureV1| m.measure(PalwRewardBasisV1::EconomicAttempted);
    let escrow_per_claim = |row: &RpcPalwClassEconomics| -> u64 {
        let ledger = &row.ledger;
        if ledger.available && ledger.finals > 0 {
            (parse_u128(&ledger.escrow_final_sompi) / ledger.finals as u128) as u64
        } else if row.claims_accepted > 0 {
            (parse_u128(&row.escrow_accepted_sompi) / row.claims_accepted as u128) as u64
        } else {
            0
        }
    };
    // The rate: the heaviest live model class's attempted compute is paid its escrow whole.
    let rate = r
        .classes
        .iter()
        .zip(measures)
        .filter(|(row, m)| m.sets_unit && !row.is_base_class && escrow_per_claim(row) > 0)
        .map(|(row, m)| (escrow_per_claim(row), attempted_per_claim(m)))
        .max_by_key(|(_, attempted)| *attempted)
        .map(|(escrow, attempted)| if attempted == 0 { 0 } else { escrow as u128 * PALW_LEDGER_RATE_SCALE_V1 / attempted })
        .unwrap_or(0);
    let classes = r
        .classes
        .iter()
        .zip(measures)
        .filter(|(row, _)| !row.is_base_class)
        .map(|(row, m)| {
            let escrow = escrow_per_claim(row);
            let attempted = attempted_per_claim(m);
            let finals = row.ledger.finals;
            let attempted_compute = parse_u128(&row.ledger.attempted_compute);
            let over_window = |reward_per_final: u64| -> u128 {
                if attempted_compute == 0 {
                    0
                } else {
                    reward_per_final as u128 * finals as u128 * PALW_LEDGER_RATE_SCALE_V1 / attempted_compute
                }
            };
            let mut under: Vec<(&'static str, u64, u128)> = Vec::new();
            for (basis, name) in BASES.iter() {
                let unit = palw_basis_unit_v1(*basis, measures);
                let reward = if m.priced { palw_priced_reward_u128_v1(escrow, m.measure(*basis), unit) } else { escrow };
                under.push((name, reward, over_window(reward)));
            }
            let by_rate = palw_rate_priced_reward_v1(escrow, attempted, rate);
            under.push(("EconomicRate (C)", by_rate, over_window(by_rate)));
            let uncapped_economic_reward = attempted.saturating_mul(rate) / PALW_LEDGER_RATE_SCALE_V1;
            let cap_utilization_permille = if escrow == 0 {
                0
            } else {
                (uncapped_economic_reward.saturating_mul(1_000) / escrow as u128).min(u32::MAX as u128) as u32
            };
            ClassEndToEnd {
                name: name_of(row),
                class_id: row.class_id.clone(),
                escrow_per_claim: escrow,
                attempted_per_claim: attempted,
                finals,
                attempted_compute,
                under,
                uncapped_economic_reward,
                capped_reward: by_rate,
                cap_utilization_permille,
                cap_saturated: uncapped_economic_reward > escrow as u128,
            }
        })
        .collect();
    (classes, rate)
}

/// ADR-0133: one model class's profile and panel arithmetic, from the node's answer.
pub(crate) struct ClassCapacity {
    pub(crate) name: String,
    pub(crate) class_id: String,
    pub(crate) profile: PalwVerificationProfileV1,
    pub(crate) accepted_per_span_milli: u64,
    pub(crate) capacity: PalwPanelCapacityV1,
    pub(crate) eligible_seats: u32,
    pub(crate) bound_inflight: u64,
    pub(crate) duty_seats_inflight: u64,
    pub(crate) seat_exposure_inflight_sompi: u128,
    pub(crate) free_collateral_sompi: u128,
}

/// The profile from the class's compute (the reference's estimate) and this node's replays where it
/// has any (their mean as the warm p99 stand-in; a replay that read storage is not separated yet), the
/// accepted rate from the ledger's window (claims over its accepted-DAA span, per execution span), the
/// panel's capacity from the census's eligible seats.
fn capacity_of(r: &GetPalwClassEconomicsResponse, seat_count: u16) -> Vec<ClassCapacity> {
    let reference = PALW_VERIFICATION_REFERENCE_V1;
    r.classes
        .iter()
        .filter(|row| !row.is_base_class)
        .map(|row| {
            let t = &row.telemetry;
            let measured_ms = if t.available && t.replays > 0 { t.replay_millis / t.replays } else { 0 };
            let facts = PalwClassTimingFactsV1 {
                draw_compute: parse_u128(&row.economic_compute_job),
                artifact_bytes: 0,
                warm_p99_ms: measured_ms,
                cold_p99_ms: 0,
            };
            let profile = palw_verification_profile_v1(&facts, &reference, seat_count.max(1));
            let l = &row.ledger;
            let span_daa = l.last_accepted_daa.saturating_sub(l.first_accepted_daa).max(1);
            let accepted_per_span_milli =
                if l.available && l.claims > 1 { l.claims * 1_000 * PALW_SPAN_ANCHORS_V1 / span_daa } else { 0 };
            let seat_exposure = if row.duty_seats_inflight > 0 {
                parse_u128(&row.seat_exposure_inflight_sompi) / row.duty_seats_inflight as u128
            } else {
                0
            };
            let capacity = palw_panel_capacity_v1(
                &PalwPanelCapacityInputV1 {
                    accepted_per_span_milli,
                    profile,
                    eligible_seats: row.eligible_seats,
                    cold_fraction_permille: 0,
                    seat_exposure_sompi: seat_exposure,
                },
                &reference,
            );
            ClassCapacity {
                name: name_of(row),
                class_id: row.class_id.clone(),
                profile,
                accepted_per_span_milli,
                capacity,
                eligible_seats: row.eligible_seats,
                bound_inflight: row.claims_panel_bound,
                duty_seats_inflight: row.duty_seats_inflight,
                seat_exposure_inflight_sompi: parse_u128(&row.seat_exposure_inflight_sompi),
                free_collateral_sompi: parse_u128(&row.free_collateral_sompi),
            }
        })
        .collect()
}

pub(crate) fn report(r: &GetPalwClassEconomicsResponse) -> Report {
    let network_expected_attempts_q32 = parse_u128(&r.network_expected_attempts_q32).max(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1);
    let measures: Vec<PalwClassMeasureV1> = r.classes.iter().map(|row| measure_of(row, network_expected_attempts_q32)).collect();
    let actual_gaps = [
        actual_gap("producer MSK / attempted CCU", &r.classes, |row| parse_u128(&row.ledger.producer_per_attempted_compute)),
        actual_gap("panel MSK / verification CCU", &r.classes, |row| parse_u128(&row.ledger.panel_per_verification_compute)),
        actual_gap("total MSK / attempted CCU", &r.classes, |row| parse_u128(&row.ledger.total_per_attempted_compute)),
    ];
    let (end_to_end, rate_sompi_per_giga) = end_to_end(r, &measures);
    let capacity = capacity_of(r, r.seat_count);
    let bases = BASES
        .iter()
        .map(|(basis, name)| {
            let unit = palw_basis_unit_v1(*basis, &measures);
            let classes: Vec<ClassOnBasis> = r
                .classes
                .iter()
                .zip(&measures)
                .map(|(row, measure)| ClassOnBasis {
                    name: name_of(row),
                    class_id: row.class_id.clone(),
                    is_base_class: row.is_base_class,
                    sets_unit: measure.sets_unit,
                    economics: palw_class_economics_v1(
                        *basis,
                        &PalwClassEconomicsInputV1 {
                            measure: *measure,
                            escrow_final_sompi: parse_u128(&row.escrow_final_sompi),
                            claims_accepted: row.claims_accepted,
                            claims_final: row.claims_final,
                            seats_replaying: r.seat_count as u64,
                        },
                        unit,
                    ),
                })
                .collect();
            let (gap_final, gap_attempted, gap_panel) = model_class_gaps(&classes);
            (*name, unit, classes, gap_final, gap_attempted, gap_panel)
        })
        .collect();
    Report {
        tip_daa: r.tip_daa,
        seat_count: r.seat_count,
        prefill_draw: r.prefill_draw,
        economic_compute_version: r.economic_compute_version,
        network_bits: r.network_bits,
        network_expected_attempts_q32,
        ledger_available: r.ledger_available,
        ledger_claims: r.ledger_claims,
        ledger_first_daa: r.ledger_first_daa,
        ledger_last_daa: r.ledger_last_daa,
        actual_gaps,
        end_to_end,
        rate_sompi_per_giga,
        capacity,
        rows: r.classes.clone(),
        bases,
    }
}

/// The widest gap among the model classes that set the unit and were paid: `max / min − 1` of
/// `F_m`, of `A_m` and of the panel's rate per verification compute.
fn model_class_gaps(classes: &[ClassOnBasis]) -> (Option<u128>, Option<u128>, Option<u128>) {
    let gap = |pick: fn(&PalwClassEconomicsV1) -> u128| -> Option<u128> {
        let rates: Vec<u128> = classes.iter().filter(|c| c.sets_unit).map(|c| pick(&c.economics)).collect();
        if rates.len() < 2 {
            return None;
        }
        palw_gap_permille_v1(*rates.iter().max()?, *rates.iter().min()?)
    };
    (
        gap(PalwClassEconomicsV1::total_per_final_compute),
        gap(PalwClassEconomicsV1::total_per_attempted_compute),
        gap(PalwClassEconomicsV1::panel_per_verification_compute),
    )
}

fn msk(sompi: u128) -> String {
    format!("{}.{:02}", sompi / 100_000_000, (sompi % 100_000_000) / 1_000_000)
}

/// Sompi per 10⁹ MAC-equivalents, printed as MSK per 10¹² (`MSK/T`): three decimals.
fn rate(sompi_per_giga: u128) -> String {
    let per_tera = sompi_per_giga.saturating_mul(1000);
    format!("{}.{:03}", per_tera / 100_000_000, (per_tera % 100_000_000) / 100_000)
}

fn permille(gap: Option<u128>) -> String {
    match gap {
        Some(g) => format!("{}.{}%", g / 10, g % 10),
        None => "n/a".to_string(),
    }
}

/// The human report.
pub(crate) fn render(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "PALW class economics at DAA {} · {} seats a panel · attempt job {} · EconomicComputeV1 table v{}\n",
        report.tip_daa,
        report.seat_count,
        if report.prefill_draw { "prefill-only (ADR-0117)" } else { "canonical" },
        report.economic_compute_version
    ));
    out.push_str("Census (attempt-lane claims the node's state still holds):\n");
    out.push_str(&format!(
        "  {:<28} {:>7} {:>12} {:>14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}\n",
        "class", "share‰", "leaves", "job MAC-eq", "EA", "accepted", "final", "voided", "redrawn", "Final%"
    ));
    for row in &report.rows {
        let terminal = row.claims_final + row.claims_voided;
        let final_rate =
            if terminal == 0 { "—".to_string() } else { format!("{:.1}", row.claims_final as f64 * 100.0 / terminal as f64) };
        out.push_str(&format!(
            "  {:<28} {:>7} {:>12} {:>14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}\n",
            name_of(row),
            row.share_permille,
            row.pwu_per_inference,
            if row.economic_source == "unknown" { "?".to_string() } else { row.economic_compute_job.clone() },
            q32_decimal(&row.expected_attempts_q32),
            row.claims_accepted,
            row.claims_final,
            row.claims_voided,
            row.claims_redrawn,
            final_rate
        ));
    }
    for (name, unit, classes, gap_final, gap_attempted, gap_panel) in &report.bases {
        out.push_str(&format!("\nBasis {name} (unit {unit}) — MSK per 10¹² MAC-eq:\n"));
        out.push_str(&format!(
            "  {:<28} {:>6} {:>12} {:>10} {:>10} {:>10} {:>10} {:>12}\n",
            "class", "price‰", "reward/Final", "F prod", "F total", "A prod", "A total", "panel/verify"
        ));
        for c in classes {
            let e = &c.economics;
            out.push_str(&format!(
                "  {:<28} {:>6} {:>12} {:>10} {:>10} {:>10} {:>10} {:>12}{}\n",
                c.name,
                e.price_permille,
                msk(e.reward_per_final_sompi as u128),
                rate(e.producer_per_final_compute()),
                rate(e.total_per_final_compute()),
                rate(e.producer_per_attempted_compute()),
                rate(e.total_per_attempted_compute()),
                rate(e.panel_per_verification_compute()),
                if c.is_base_class {
                    "  (floor, unpriced)"
                } else if !c.sets_unit {
                    "  (does not set the unit)"
                } else {
                    ""
                }
            ));
        }
        out.push_str(&format!(
            "  gap between model classes: F {} · A {} · panel/verify {}\n",
            permille(*gap_final),
            permille(*gap_attempted),
            permille(*gap_panel)
        ));
    }
    out.push_str("\nF = paid / Final compute, A = paid / attempted compute (expected draws × every accepted claim); 5% good, 10% acceptable, 20%+ a distortion.\n");
    out.push_str("Leaves is the rule in force; the economic bases are the shadow (ADR-0131). Panel pay is the pool, credited seats are not retained past Final.\n");
    out.push_str(&format!(
        "\nNetwork draw (ADR-0132): bits 0x{:08x} → {} network draws a class win; attempted compute = class draws × network draws × one draw's job.\n",
        report.network_bits,
        q32_decimal(&report.network_expected_attempts_q32.to_string())
    ));
    if !report.ledger_available {
        out.push_str("End to end: this node keeps no ledger (a node built before ADR-0132, or not ConsensusV2).\n");
        return out;
    }
    out.push_str(&format!(
        "\nEnd to end — this node's ledger: {} claims, accepted DAA {}–{} (what was ACTUALLY paid, by the rule at each Final):\n",
        report.ledger_claims, report.ledger_first_daa, report.ledger_last_daa
    ));
    out.push_str(&format!(
        "  {:<28} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>12} {:>10} {:>10}\n",
        "class", "claims", "bound", "licens", "final", "voided", "lic‰", "fin‰", "producer MSK", "panel MSK", "burned MSK"
    ));
    for row in report.rows.iter().filter(|r| r.ledger.available) {
        let l = &row.ledger;
        out.push_str(&format!(
            "  {:<28} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>12} {:>10} {:>10}\n",
            name_of(row),
            l.claims,
            l.bound,
            l.licensed,
            l.finals,
            l.voided,
            l.licence_rate_permille,
            l.final_rate_permille,
            msk(parse_u128(&l.producer_paid_sompi)),
            msk(parse_u128(&l.panel_paid_sompi)),
            msk(parse_u128(&l.burned_sompi))
        ));
    }
    out.push_str(&format!(
        "  {:<28} {:>12} {:>12} {:>12} {:>10} {:>10} {:>10} {:>8} {:>8} {:>8}\n",
        "class", "attempted G", "final G", "verify G", "prod/attmp", "panel/vrf", "tot/attmp", "EA cls", "EA net", "wait F"
    ));
    for row in report.rows.iter().filter(|r| r.ledger.available) {
        let l = &row.ledger;
        out.push_str(&format!(
            "  {:<28} {:>12} {:>12} {:>12} {:>10} {:>10} {:>10} {:>8} {:>8} {:>8}\n",
            name_of(row),
            giga(parse_u128(&l.attempted_compute)),
            giga(parse_u128(&l.final_compute)),
            giga(parse_u128(&l.verification_compute)),
            rate(parse_u128(&l.producer_per_attempted_compute)),
            rate(parse_u128(&l.panel_per_verification_compute)),
            rate(parse_u128(&l.total_per_attempted_compute)),
            q32_decimal(&l.avg_expected_attempts_q32),
            q32_decimal(&l.avg_network_expected_attempts_q32),
            l.avg_final_wait_daa
        ));
    }
    for g in &report.actual_gaps {
        out.push_str(&format!("  actual gap, {}: {}\n", g.metric, actual_gap_text(g)));
    }
    let telemetry: Vec<&RpcPalwClassEconomics> = report.rows.iter().filter(|r| r.telemetry.available).collect();
    if !telemetry.is_empty() {
        out.push_str("This node's own work (since the process started):\n");
        for row in telemetry {
            let t = &row.telemetry;
            out.push_str(&format!(
                "  {:<28} draws {} · class wins {} · blocks {} · {} s/draw · {} MiB/draw · replays {} · {} s/replay · receipts V {} U {} I {} · openings {}\n",
                name_of(row),
                t.draws,
                t.class_wins,
                t.produced,
                if t.draws == 0 { 0 } else { t.draw_millis / t.draws / 1000 },
                if t.draws == 0 { 0 } else { t.storage_read_mib / t.draws },
                t.replays,
                if t.replays == 0 { 0 } else { t.replay_millis / t.replays / 1000 },
                t.receipts_valid,
                t.receipts_unavailable,
                t.receipts_incapable,
                t.openings_held
            ));
        }
    }
    out.push_str(&format!(
        "\nEach class's Finals under every basis, at its actual Final rate — sompi per 10⁹ MAC-eq attempted (rate for C: {} sompi per 10⁹, the heaviest live class paid whole):\n",
        report.rate_sompi_per_giga
    ));
    if let Some(first) = report.end_to_end.first() {
        out.push_str(&format!("  {:<28} {:>8} {:>13}", "class", "finals", "attempted/clm"));
        for (name, _, _) in &first.under {
            out.push_str(&format!(" {:>22}", name));
        }
        out.push('\n');
    }
    for c in &report.end_to_end {
        out.push_str(&format!("  {:<28} {:>8} {:>13}", c.name, c.finals, giga(c.attempted_per_claim)));
        for (_, reward, over) in &c.under {
            out.push_str(&format!(" {:>10} {:>11}", msk(*reward as u128), rate(*over)));
        }
        out.push('\n');
    }
    if let Some(first) = report.end_to_end.first() {
        out.push_str("  gap among model classes paid:");
        for (i, (name, _, _)) in first.under.iter().enumerate() {
            let rates: Vec<u128> = report.end_to_end.iter().filter_map(|c| c.under.get(i)).map(|u| u.2).filter(|r| *r > 0).collect();
            let gap =
                if rates.len() >= 2 { palw_gap_permille_v1(*rates.iter().max().unwrap(), *rates.iter().min().unwrap()) } else { None };
            out.push_str(&format!(" {name} {}", permille(gap)));
        }
        out.push('\n');
    }
    out.push_str("A class whose Finals are zero is paid nothing under every basis: that is liveness, not price (ADR-0132 §2).\n");
    if !report.end_to_end.is_empty() {
        out.push_str("Escrow cap under the rate (ADR-0133 §9a; a class above 80 % is not activatable as a paid class):\n");
        for c in &report.end_to_end {
            out.push_str(&format!(
                "  {:<28} uncapped {} · capped {} · cap utilization {}‰{}{}\n",
                c.name,
                msk(c.uncapped_economic_reward),
                msk(c.capped_reward as u128),
                c.cap_utilization_permille,
                if c.cap_saturated { " · SATURATED" } else { "" },
                if c.cap_utilization_permille > PALW_CAP_UTILIZATION_ACTIVATION_LIMIT_PERMILLE {
                    " · not activatable as paid"
                } else {
                    ""
                }
            ));
        }
    }
    if !report.capacity.is_empty() {
        out.push_str(
            "\nPanel capacity (ADR-0133; profile from the graph's compute and this node's replays, rate from the ledger's window):\n",
        );
        out.push_str(&format!(
            "  {:<28} {:>7} {:>7} {:>7} {:>8} {:>8} {:>9} {:>7} {:>7} {:>8} {:>12} {:>12} {:>5}\n",
            "class",
            "window",
            "prefch",
            "cap",
            "warm s",
            "clm/span",
            "eligible",
            "bound",
            "duties",
            "util‰",
            "exposure MSK",
            "free MSK",
            "live"
        ));
        for c in &report.capacity {
            out.push_str(&format!(
                "  {:<28} {:>7} {:>7} {:>7} {:>8} {:>8} {:>9} {:>7} {:>7} {:>8} {:>12} {:>12} {:>5}\n",
                c.name,
                c.profile.verification_window_spans,
                c.profile.artifact_prefetch_spans,
                c.profile.max_inflight_claims,
                c.profile.warm_p99_ms / 1_000,
                format!("{}.{:03}", c.accepted_per_span_milli / 1_000, c.accepted_per_span_milli % 1_000),
                c.eligible_seats,
                c.bound_inflight,
                c.duty_seats_inflight,
                c.capacity.utilization_permille,
                msk(c.seat_exposure_inflight_sompi),
                msk(c.free_collateral_sompi),
                if c.capacity.live { "yes" } else { "no" }
            ));
        }
        out.push_str("  window/prefetch in execution spans (5 anchors); cap = inflight claims Little's law allows at 70 % over seven seats; util = seat-time demanded / offered a span.\n");
    }
    out
}

fn giga(compute: u128) -> String {
    format!("{}.{:03}", compute / 1_000_000_000, compute % 1_000_000_000 / 1_000_000)
}

fn actual_gap_text(g: &ActualGap) -> String {
    match (&g.gap_permille, g.paid_nothing.is_empty()) {
        (Some(gap), _) => permille(Some(*gap)),
        (None, false) => format!("∞ ({} paid nothing)", g.paid_nothing.join(", ")),
        (None, true) => "— (fewer than two model classes paid)".to_string(),
    }
}

/// The JSON report.
pub(crate) fn json_report(report: &Report) -> serde_json::Value {
    let basis = |(name, unit, classes, gap_final, gap_attempted, gap_panel): &(
        &str,
        u128,
        Vec<ClassOnBasis>,
        Option<u128>,
        Option<u128>,
        Option<u128>,
    )| {
        json!({
            "basis": name,
            "unit": unit.to_string(),
            "classes": classes.iter().map(|c| {
                let e = &c.economics;
                json!({
                    "class_id": c.class_id, "name": c.name, "base": c.is_base_class, "sets_unit": c.sets_unit,
                    "price_permille": e.price_permille,
                    "reward_per_final_sompi": e.reward_per_final_sompi,
                    "producer_per_final_sompi": e.producer_per_final_sompi,
                    "panel_pool_per_final_sompi": e.panel_pool_per_final_sompi,
                    "producer_paid_sompi": e.producer_paid_sompi.to_string(),
                    "panel_pool_sompi": e.panel_pool_sompi.to_string(),
                    "burned_sompi": e.burned_sompi.to_string(),
                    "final_compute": e.final_compute.to_string(),
                    "attempted_compute": e.attempted_compute.to_string(),
                    "panel_compute": e.panel_compute.to_string(),
                    "rate_scale": PALW_ECONOMICS_RATE_SCALE_V1.to_string(),
                    "producer_msk_per_final_compute": e.producer_per_final_compute().to_string(),
                    "panel_msk_per_final_compute": e.panel_per_final_compute().to_string(),
                    "total_msk_per_final_compute": e.total_per_final_compute().to_string(),
                    "producer_msk_per_attempted_compute": e.producer_per_attempted_compute().to_string(),
                    "panel_msk_per_attempted_compute": e.panel_per_attempted_compute().to_string(),
                    "total_msk_per_attempted_compute": e.total_per_attempted_compute().to_string(),
                    "panel_msk_per_verification_compute": e.panel_per_verification_compute().to_string(),
                })
            }).collect::<Vec<_>>(),
            "gap_final_permille": gap_final,
            "gap_attempted_permille": gap_attempted,
            "gap_panel_permille": gap_panel,
        })
    };
    let ledger = |l: &kaspa_rpc_core::RpcPalwClassLedgerTotals| {
        json!({
            "available": l.available, "claims": l.claims, "bound": l.bound, "licensed": l.licensed, "finals": l.finals, "voided": l.voided,
            "redrawn": l.redrawn, "paid_at_acceptance": l.paid_at_acceptance,
            "escrow_final_sompi": l.escrow_final_sompi, "producer_paid_sompi": l.producer_paid_sompi, "panel_paid_sompi": l.panel_paid_sompi,
            "reserve_sompi": l.reserve_sompi, "burned_sompi": l.burned_sompi,
            "attempted_ccu": l.attempted_compute, "final_ccu": l.final_compute, "verification_ccu": l.verification_compute,
            "msk_per_attempted_ccu": l.producer_per_attempted_compute, "panel_msk_per_verification_ccu": l.panel_per_verification_compute,
            "total_msk_per_attempted_ccu": l.total_per_attempted_compute, "msk_per_final_ccu": l.total_per_final_compute,
            "licence_rate_permille": l.licence_rate_permille, "final_of_licensed_permille": l.final_of_licensed_permille, "final_rate_permille": l.final_rate_permille,
            "avg_bind_wait_daa": l.avg_bind_wait_daa, "avg_licence_wait_daa": l.avg_licence_wait_daa, "avg_claim_lifetime_final_daa": l.avg_final_wait_daa,
            "avg_claim_lifetime_void_daa": l.avg_void_wait_daa,
            "avg_expected_attempts_q32": l.avg_expected_attempts_q32, "avg_expected_attempts_decimal": q32_decimal(&l.avg_expected_attempts_q32),
            "avg_network_expected_attempts_q32": l.avg_network_expected_attempts_q32,
            "first_accepted_daa": l.first_accepted_daa, "last_accepted_daa": l.last_accepted_daa,
        })
    };
    let telemetry = |t: &kaspa_rpc_core::RpcPalwClassNodeTelemetry| {
        json!({
            "available": t.available, "draws": t.draws, "class_wins": t.class_wins, "produced": t.produced, "draw_millis": t.draw_millis,
            "artifact_bytes_fetched_mib": t.storage_read_mib, "replays": t.replays, "replay_millis": t.replay_millis, "replay_leaves": t.replay_leaves,
            "receipts": { "valid": t.receipts_valid, "unavailable": t.receipts_unavailable, "incapable": t.receipts_incapable, "other": t.receipts_other },
            "openings_held": t.openings_held,
        })
    };
    json!({
        "schema": "misaka.palw.economics.v2",
        "tip_daa": report.tip_daa,
        "seat_count": report.seat_count,
        "prefill_draw": report.prefill_draw,
        "economic_compute_version": report.economic_compute_version,
        "network": { "bits": report.network_bits, "expected_attempts_q32": report.network_expected_attempts_q32.to_string(),
                     "expected_attempts_decimal": q32_decimal(&report.network_expected_attempts_q32.to_string()) },
        "ledger": { "available": report.ledger_available, "claims": report.ledger_claims, "first_daa": report.ledger_first_daa, "last_daa": report.ledger_last_daa,
                    "actual_gaps": report.actual_gaps.iter().map(|g| json!({ "metric": g.metric, "gap_permille": g.gap_permille, "paid_nothing": g.paid_nothing })).collect::<Vec<_>>() },
        "end_to_end": report.end_to_end.iter().map(|c| json!({
            "class_id": c.class_id, "name": c.name, "escrow_per_claim_sompi": c.escrow_per_claim, "attempted_ccu_per_claim": c.attempted_per_claim.to_string(),
            "finals": c.finals, "attempted_ccu": c.attempted_compute.to_string(),
            "under": c.under.iter().map(|(b, reward, over)| json!({ "basis": b, "reward_per_final_sompi": reward, "msk_per_attempted_ccu": over.to_string() })).collect::<Vec<_>>(),
            "uncapped_economic_reward_sompi": c.uncapped_economic_reward.to_string(), "capped_reward_sompi": c.capped_reward,
            "cap_utilization_permille": c.cap_utilization_permille, "cap_saturated": c.cap_saturated,
            "activatable_as_paid": c.cap_utilization_permille <= PALW_CAP_UTILIZATION_ACTIVATION_LIMIT_PERMILLE,
        })).collect::<Vec<_>>(),
        "rate_sompi_per_giga": report.rate_sompi_per_giga.to_string(),
        "panel_capacity": report.capacity.iter().map(|c| json!({
            "class_id": c.class_id, "name": c.name,
            "profile": { "verification_window_spans": c.profile.verification_window_spans, "artifact_prefetch_spans": c.profile.artifact_prefetch_spans,
                         "max_inflight_claims": c.profile.max_inflight_claims, "warm_p99_ms": c.profile.warm_p99_ms, "cold_p99_ms": c.profile.cold_p99_ms },
            "accepted_per_span_milli": c.accepted_per_span_milli, "eligible_seats": c.eligible_seats, "bound_inflight": c.bound_inflight,
            "duty_seats_inflight": c.duty_seats_inflight, "seat_exposure_inflight_sompi": c.seat_exposure_inflight_sompi.to_string(),
            "free_collateral_sompi": c.free_collateral_sompi.to_string(),
            "utilization_permille": c.capacity.utilization_permille, "inflight_claims_milli": c.capacity.inflight_claims_milli,
            "max_accepted_per_span_milli": c.capacity.max_accepted_per_span_milli, "seats_required": c.capacity.seats_required,
            "seats_required_n1": c.capacity.seats_required_n1, "seats_required_n2": c.capacity.seats_required_n2,
            "final_latency_spans": c.capacity.final_latency_spans, "live": c.capacity.live, "outage_margin_seats": c.capacity.outage_margin_seats,
        })).collect::<Vec<_>>(),
        "census": report.rows.iter().map(|row| json!({
            "class_id": row.class_id, "name": name_of(row), "base": row.is_base_class, "status": row.status,
            "share_permille": row.share_permille, "pwu_per_inference": row.pwu_per_inference,
            "class_target": row.class_target, "expected_attempts": row.expected_attempts,
            "expected_attempts_q32": row.expected_attempts_q32, "expected_attempts_decimal": q32_decimal(&row.expected_attempts_q32),
            "economic_compute_job": row.economic_compute_job, "economic_compute_canonical": row.economic_compute_canonical,
            "economic_source": row.economic_source,
            "claims": { "accepted": row.claims_accepted, "provisional": row.claims_provisional, "panel_bound": row.claims_panel_bound,
                        "licensed": row.claims_licensed, "final": row.claims_final, "voided": row.claims_voided, "redrawn": row.claims_redrawn },
            "escrow_accepted_sompi": row.escrow_accepted_sompi, "escrow_final_sompi": row.escrow_final_sompi,
            "ledger": ledger(&row.ledger), "telemetry": telemetry(&row.telemetry),
        })).collect::<Vec<_>>(),
        "bases": report.bases.iter().map(basis).collect::<Vec<_>>(),
    })
}

/// `misaka palw economics`.
pub(crate) async fn run(ctx: &Ctx) -> CliResult {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_class_economics().await;
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| {
        CliError::new(exit::CONNECTION, format!("getPalwClassEconomics: {e} (a node built before ADR-0131 does not serve it)"))
    })?;
    if !response.available {
        return Err(CliError::new(exit::GENERIC, "the node keeps no PALW class state (not a ConsensusV2 network)"));
    }
    let report = report(&response);
    match ctx.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&json_report(&report)).expect("serializable")),
        OutputFormat::Human => print!("{}", render(&report)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// testnet-11 past 6,001 as the node would answer it, with the live targets of 2026-09-17 and
    /// the hybrid's panels licensing nothing.
    fn t11() -> GetPalwClassEconomicsResponse {
        let row = |name: &str,
                   base: bool,
                   share: u16,
                   leaves: u64,
                   job: &str,
                   canonical: &str,
                   ea: u64,
                   accepted: u64,
                   finals: u64,
                   voided: u64| {
            RpcPalwClassEconomics {
                class_id: format!("{:0>128}", name.len()),
                model_id: name.to_string(),
                is_base_class: base,
                status: "Active".to_string(),
                share_permille: share,
                pwu_per_inference: leaves,
                class_target: "0".to_string(),
                expected_attempts: ea,
                expected_attempts_q32: ((ea as u128) << 32).to_string(),
                economic_compute_job: job.to_string(),
                economic_compute_canonical: canonical.to_string(),
                economic_source: "build_ledger".to_string(),
                claims_accepted: accepted,
                claims_final: finals,
                claims_voided: voided,
                escrow_accepted_sompi: (320_084_650_080u128 * accepted as u128).to_string(),
                escrow_final_sompi: (320_084_650_080u128 * finals as u128).to_string(),
                ..Default::default()
            }
        };
        GetPalwClassEconomicsResponse {
            available: true,
            tip_daa: 7_500,
            economic_compute_version: 1,
            seat_count: 5,
            prefill_draw: true,
            classes: vec![
                row("PALW-BASE-0/rc", true, 20, 7_708, "21657728", "30504896", 26_404, 14, 6, 0),
                row("Qwen3.6-35B-A3B/graph-v3", false, 488, 2_685_360, "18055200736", "21070759296", 1, 500, 0, 0),
                row("Qwen2.5-1.5B/graph-v5@512", false, 491, 6_630_544, "83102171136", "84653733376", 1, 797, 48, 320),
                row("Qwen3.8-27B/graph-v3", false, 1, 9_000_776, "172919123392", "198712432640", 3_165, 0, 0, 0),
            ],
            ..Default::default()
        }
    }

    /// **The report prices every class on every basis with consensus-core's functions**: the leaf
    /// basis's unit is the 27B's leaves and the economic bases' its compute; the hybrid, whose panels
    /// licensed nothing, has no `F` and the gap says so; the dense tier's `A` on the leaf basis is what
    /// its price, its final rate and its compute make it.
    #[test]
    fn the_report_prices_the_live_classes_and_names_the_gaps() {
        let report = report(&t11());
        assert_eq!(report.bases.len(), 3);
        let (name, unit, classes, gap_final, gap_attempted, _) = &report.bases[0];
        assert_eq!((*name, *unit), ("leaves", 9_000_776));
        assert_eq!(classes[2].economics.price_permille, 736, "Qwen2.5@512: 6,630,544 of 9,000,776");
        assert_eq!(classes[1].economics.price_permille, 298);
        assert_eq!(classes[0].economics.price_permille, 1000, "the floor is unpriced");
        // Both gaps are undefined while the hybrid is paid nothing per unit.
        assert_eq!((*gap_final, *gap_attempted), (None, None));
        let (_, unit_job, classes_job, _, _, _) = &report.bases[1];
        assert_eq!(*unit_job, 172_919_123_392, "the 27B's draw job sets the compute unit");
        assert_eq!(classes_job[2].economics.price_permille, 480, "83.10G of 172.92G");
        let (_, unit_att, _, _, _, _) = &report.bases[2];
        assert_eq!(*unit_att, 172_919_123_392 * 3_165, "…and its 3,165 expected attempts on the attempted basis");
        // The dense tier on the leaf basis: 48 finals of 797 accepted.
        let e = &classes[2].economics;
        assert_eq!(e.reward_per_final_sompi, 320_084_650_080 * 6_630_544 / 9_000_776, "73.6 % of 3,200.85 MSK: 2,357.95 MSK");
        assert_eq!(e.final_compute, 83_102_171_136 * 48);
        assert_eq!(e.attempted_compute, 83_102_171_136 * 797);
        let text = render(&report);
        assert!(text.contains("Basis leaves (unit 9000776)"));
        assert!(text.contains("(floor, unpriced)"));
        assert!(text.contains("gap between model classes: F n/a · A n/a"));
        let doc = json_report(&report);
        assert_eq!(doc["schema"], "misaka.palw.economics.v2");
        assert_eq!(doc["bases"][0]["classes"][2]["price_permille"], 736);
    }

    /// With both model classes licensing, the gaps are the basis's: the leaf basis pays the hybrid
    /// ~86 % more per unit of the compute it ran, the compute bases pay them alike.
    #[test]
    fn the_gaps_follow_the_basis_once_both_classes_are_paid() {
        let mut r = t11();
        r.classes[1].claims_final = 100;
        r.classes[1].escrow_final_sompi = (320_084_650_080u128 * 100).to_string();
        r.classes.truncate(3);
        let report = report(&r);
        let leaf_gap = report.bases[0].3.unwrap();
        assert!((855..=875).contains(&leaf_gap), "leaf basis gap_final {leaf_gap} ‰");
        assert!(report.bases[1].3.unwrap() <= 1, "economic_job gap_final");
        assert!(report.bases[2].3.unwrap() <= 1, "economic_attempted gap_final");
        // Panels replay every accepted claim and are paid on the `Final` ones: with the hybrid
        // licensing 100 of 500 and the dense tier 48 of 797, the pool per unit of verification
        // compute differs by the licence rates' ratio (0.2 / 0.0602 → 232 %) even on the job basis.
        let panel_gap = report.bases[1].5.unwrap();
        assert!((2_300..=2_340).contains(&panel_gap), "economic_job gap_panel {panel_gap} ‰");
        assert_eq!(report.bases[1].1, 83_102_171_136, "without the 27B the dense tier sets the compute unit");
    }

    /// **ADR-0132: the end-to-end reading reads what was paid, not the price.** The dense tier's
    /// ledger shows 48 Finals paid whole below the fences and the hybrid's 500 claims paid nothing;
    /// the actual gaps are infinite and name the hybrid; the four-basis table prices each class's
    /// Finals at its actual Final rate, so the hybrid is zero under every basis and the dense tier's
    /// rate-priced (C) reward equals its escrow when it is the heaviest live class; the network draw
    /// multiplies every attempted figure by the same factor.
    #[test]
    fn adr0132_the_end_to_end_reading_is_what_was_paid() {
        use kaspa_rpc_core::{RpcPalwClassLedgerTotals, RpcPalwClassNodeTelemetry};
        let mut r = t11();
        r.network_bits = 0x207f_ffff;
        r.network_expected_attempts_q32 = (2u128 << 32).to_string();
        r.ledger_available = true;
        r.ledger_claims = 1_419;
        r.ledger_first_daa = 3_542;
        r.ledger_last_daa = 5_774;
        let escrow = 275_628_448_680u128;
        r.classes[2].ledger = RpcPalwClassLedgerTotals {
            available: true,
            claims: 919,
            bound: 909,
            licensed: 165,
            finals: 48,
            voided: 320,
            redrawn: 493,
            escrow_final_sompi: (escrow * 48).to_string(),
            producer_paid_sompi: (escrow * 48).to_string(),
            attempted_compute: (83_102_171_136u128 * 3 * 919).to_string(),
            final_compute: (83_102_171_136u128 * 48).to_string(),
            verification_compute: (83_102_171_136u128 * 5 * 909).to_string(),
            producer_per_attempted_compute: ((escrow * 48) * 1_000_000_000 / (83_102_171_136u128 * 3 * 919)).to_string(),
            total_per_attempted_compute: ((escrow * 48) * 1_000_000_000 / (83_102_171_136u128 * 3 * 919)).to_string(),
            licence_rate_permille: 179,
            final_rate_permille: 130,
            avg_final_wait_daa: 2_017,
            avg_expected_attempts_q32: (3u128 << 31).to_string(),
            avg_network_expected_attempts_q32: (2u128 << 32).to_string(),
            ..Default::default()
        };
        r.classes[1].ledger = RpcPalwClassLedgerTotals {
            available: true,
            claims: 500,
            bound: 481,
            redrawn: 282,
            attempted_compute: (18_055_200_736u128 * 2 * 500).to_string(),
            verification_compute: (18_055_200_736u128 * 5 * 481).to_string(),
            ..Default::default()
        };
        r.classes[1].telemetry = RpcPalwClassNodeTelemetry {
            available: true,
            draws: 979,
            class_wins: 977,
            produced: 202,
            draw_millis: 979 * 120_000,
            storage_read_mib: 979 * 9_700,
            ..Default::default()
        };
        let report = report(&r);
        assert_eq!(report.network_expected_attempts_q32, 2u128 << 32);
        for g in &report.actual_gaps {
            assert_eq!(g.gap_permille, None, "{}", g.metric);
        }
        assert_eq!(report.actual_gaps[0].paid_nothing, vec!["Qwen3.6-35B-A3B/graph-v3".to_string()]);
        assert!(
            report.actual_gaps[1].paid_nothing.contains(&"Qwen2.5-1.5B/graph-v5@512".to_string()),
            "no seat was paid below 6,001 either"
        );
        // The four bases, at the actual Final rates.
        let dense = report.end_to_end.iter().find(|c| c.name.starts_with("Qwen2.5")).unwrap();
        let hybrid = report.end_to_end.iter().find(|c| c.name.starts_with("Qwen3.6")).unwrap();
        assert_eq!(dense.under.len(), 4);
        assert_eq!(dense.escrow_per_claim, escrow as u64, "the ledger's escrow per Final");
        assert_eq!(dense.attempted_per_claim, 83_102_171_136 * 2, "one class draw × two network draws × the draw job");
        assert!(hybrid.under.iter().all(|(_, _, over)| *over == 0), "no Final, nothing under any basis");
        let leaf = dense.under.iter().find(|(b, _, _)| *b == "leaves").unwrap();
        assert_eq!(leaf.1, (escrow * 6_630_544 / 9_000_776) as u64);
        let by_rate = dense.under.iter().find(|(b, _, _)| b.starts_with("EconomicRate")).unwrap();
        // The 27B has no claims, so it is not live and does not set the rate; the dense tier is the
        // heaviest live class and the rate pays its attempted compute the escrow whole (to the sompi
        // the rate's floor drops), while the hybrid would be paid its share of it.
        let heaviest = 83_102_171_136u128 * 2;
        assert_eq!(report.rate_sompi_per_giga, escrow * 1_000_000_000 / heaviest);
        assert!(by_rate.1 as u128 >= escrow - heaviest / 1_000_000_000 - 1, "{}", by_rate.1);
        let hybrid_by_rate = hybrid.under.iter().find(|(b, _, _)| b.starts_with("EconomicRate")).unwrap();
        assert_eq!(hybrid_by_rate.1 as u128 * 1_000 / escrow, 217, "18.06 G × 2 draws against 83.10 G × 2: 21.7 % of the escrow");
        // The cap is each class's OWN escrow per claim: the dense tier sets the rate, so it sits at
        // its cap; the hybrid's rate-priced reward is 21.7 % of the dense tier's 2,756 MSK Final
        // escrow and 18.7 % of its own 3,200 MSK per accepted claim (no Final yet, so the census's
        // escrow per accepted claim) — both far under the 80 % activation limit. A Kimi-class at 7×
        // the dense tier's compute would read 7,000 ‰ and be refused as a paid class.
        assert_eq!(hybrid.cap_utilization_permille, 187);
        assert!(!hybrid.cap_saturated);
        assert!((999..=1_000).contains(&dense.cap_utilization_permille), "{}", dense.cap_utilization_permille);
        let text = render(&report);
        assert!(text.contains("Escrow cap under the rate"));
        assert!(text.contains("End to end — this node's ledger: 1419 claims, accepted DAA 3542–5774"));
        assert!(text.contains("actual gap, producer MSK / attempted CCU: ∞ (Qwen3.6-35B-A3B/graph-v3 paid nothing)"));
        assert!(text.contains("draws 979 · class wins 977 · blocks 202 · 120 s/draw · 9700 MiB/draw"));
        assert!(text.contains("EconomicRate (C)"));
        let doc = json_report(&report);
        assert_eq!(doc["ledger"]["claims"], 1_419);
        assert_eq!(doc["census"][2]["ledger"]["finals"], 48);
        assert_eq!(doc["census"][1]["telemetry"]["artifact_bytes_fetched_mib"], 979 * 9_700);
        assert_eq!(doc["end_to_end"][1]["under"][0]["basis"], "leaves");
        assert_eq!(doc["end_to_end"][0]["cap_utilization_permille"], 187);
        assert_eq!(doc["end_to_end"][0]["activatable_as_paid"], true);
    }
}
