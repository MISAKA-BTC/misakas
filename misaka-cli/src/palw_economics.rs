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
    PALW_ECONOMICS_RATE_SCALE_V1, PalwClassEconomicsInputV1, PalwClassEconomicsV1, PalwClassMeasureV1, PalwRewardBasisV1,
    palw_basis_unit_v1, palw_class_economics_v1, palw_gap_permille_v1,
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

/// The whole report: per basis, every class and the gaps between the model classes.
pub(crate) struct Report {
    pub(crate) tip_daa: u64,
    pub(crate) seat_count: u16,
    pub(crate) prefill_draw: bool,
    pub(crate) economic_compute_version: u16,
    pub(crate) rows: Vec<RpcPalwClassEconomics>,
    /// `(basis name, unit, classes, gap_final, gap_attempted, gap_panel)` — the gaps between the
    /// model classes' `F_m`, `A_m` and panel rates, `None` where a class was paid nothing per unit.
    pub(crate) bases: Vec<(&'static str, u128, Vec<ClassOnBasis>, Option<u128>, Option<u128>, Option<u128>)>,
}

fn parse_u128(s: &str) -> u128 {
    s.parse().unwrap_or(0)
}

fn measure_of(row: &RpcPalwClassEconomics) -> PalwClassMeasureV1 {
    PalwClassMeasureV1 {
        leaves: row.pwu_per_inference,
        draw_compute: parse_u128(&row.economic_compute_job),
        expected_attempts: row.expected_attempts,
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
pub(crate) fn report(r: &GetPalwClassEconomicsResponse) -> Report {
    let measures: Vec<PalwClassMeasureV1> = r.classes.iter().map(measure_of).collect();
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
            row.expected_attempts,
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
    out
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
    json!({
        "schema": "misaka.palw.economics.v1",
        "tip_daa": report.tip_daa,
        "seat_count": report.seat_count,
        "prefill_draw": report.prefill_draw,
        "economic_compute_version": report.economic_compute_version,
        "census": report.rows.iter().map(|row| json!({
            "class_id": row.class_id, "name": name_of(row), "base": row.is_base_class, "status": row.status,
            "share_permille": row.share_permille, "pwu_per_inference": row.pwu_per_inference,
            "class_target": row.class_target, "expected_attempts": row.expected_attempts,
            "economic_compute_job": row.economic_compute_job, "economic_compute_canonical": row.economic_compute_canonical,
            "economic_source": row.economic_source,
            "claims": { "accepted": row.claims_accepted, "provisional": row.claims_provisional, "panel_bound": row.claims_panel_bound,
                        "licensed": row.claims_licensed, "final": row.claims_final, "voided": row.claims_voided, "redrawn": row.claims_redrawn },
            "escrow_accepted_sompi": row.escrow_accepted_sompi, "escrow_final_sompi": row.escrow_final_sompi,
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

    /// testnet-11 past 7,001 as the node would answer it, with the live targets of 2026-09-17 and
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
        assert_eq!(doc["schema"], "misaka.palw.economics.v1");
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
}
