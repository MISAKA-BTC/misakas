//! **`misaka palw registry` — ADR-0135's permissionless model registry as the node holds it.**
//!
//! The node answers `getPalwModelRegistry` (op 186) with each class's lifecycle row (state, the
//! work read off its graph, the profile derived from it, the last boundary's reading), the seats
//! ready for it now, and every seat's possession proof. Nothing here is set by a human: a class
//! walks REGISTERED → PREFETCHING → PROBATION → ACTIVE_LIMITED → ACTIVE on chain-visible facts and
//! falls to HELD alone. Below the fence the rows are empty and the command says so.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::GetPalwModelRegistryResponse;
use kaspa_rpc_core::api::rpc::RpcApi;
use serde_json::json;

fn short(id: &str) -> String {
    id.chars().take(12).collect::<String>() + "…"
}

pub(crate) fn render(r: &GetPalwModelRegistryResponse) -> String {
    let mut out = String::new();
    let fence = if r.scheduled { format!("scheduled at DAA {}", r.fence_daa) } else { "dormant (no fence scheduled)".to_string() };
    out.push_str(&format!(
        "Model registry (ADR-0135): {fence} · {} at the tip (DAA {}) · span {} DAA{}\n",
        if r.active { "ACTIVE" } else { "not in force" },
        r.tip_daa,
        r.span_daa,
        if r.active && r.tip_daa < r.grace_until_daa {
            format!(" · activation grace until DAA {}", r.grace_until_daa)
        } else {
            String::new()
        }
    ));
    if r.active {
        out.push_str(&format!(
            "  globals: {} MAC-eq a span · {} bytes a span · {} + {} seats · {} ‰ utilization · {} probation claims · {} stable spans · readiness age {} spans × {} collateral\n",
            r.reference_work_per_span,
            r.reference_bytes_per_span,
            r.seat_count,
            r.spare_seats,
            r.utilization_permille,
            r.probation_claims,
            r.stable_epochs,
            r.readiness_probe_max_age_spans,
            r.readiness_collateral_multiple
        ));
    }
    out.push_str(&format!(
        "  {:<14} {:<15} {:>6} {:>7} {:>8} {:>5} {:>6} {:>6} {:>8} {:>8} {:>8} {:>6}\n",
        "class", "state", "since", "window", "prefetch", "cap", "need", "ready", "inflight", "util‰", "adm‰", "share"
    ));
    for c in r.classes.iter().filter(|c| c.has_row) {
        out.push_str(&format!("    {} — {}\n", short(&c.class_id), c.reason));
    }
    let voids: u32 = r.classes.iter().map(|c| c.no_capable_panel_voids).sum();
    if voids > 0 {
        out.push_str(&format!("  ({voids} claims voided as NoCapablePanel across the classes)\n"));
    }
    if r.active {
        out.push_str(&format!(
            "  classes: {} active · {} limited · {} probation · {} prefetching · {} registered · {} held · bonds: {} active, {} with headroom for a seat\n",
            r.classes_active, r.classes_active_limited, r.classes_probation, r.classes_prefetching, r.classes_registered, r.classes_held, r.bonds_active, r.bonds_with_headroom
        ));
    }
    for c in &r.classes {
        out.push_str(&format!(
            "  {:<14} {:<15} {:>6} {:>7} {:>8} {:>5} {:>6} {:>6} {:>8} {:>8} {:>8} {:>6}\n",
            format!("{}{}", short(&c.class_id), if c.is_base_class { "*" } else { "" }),
            if c.has_row { c.state.clone() } else { "legacy".to_string() },
            c.since_span,
            c.verification_window_spans,
            c.artifact_prefetch_spans,
            c.max_inflight_claims,
            c.required_ready_seats,
            c.ready_seats_now,
            c.inflight_now,
            c.utilization_permille,
            c.admission_milli,
            c.share_permille
        ));
    }
    if r.readiness.is_empty() {
        out.push_str("  no possession proofs on the chain\n");
    } else {
        out.push_str(&format!("  possession proofs ({}):\n", r.readiness.len()));
        for p in &r.readiness {
            out.push_str(&format!(
                "    {}:{} → {} leaf {} at DAA {} (span {}){}\n",
                short(&p.bond_txid),
                p.bond_index,
                short(&p.class_id),
                p.leaf_index,
                p.proved_daa,
                p.proved_span,
                if p.not_ready_reason.is_empty() { String::new() } else { format!(" · not ready: {}", p.not_ready_reason) }
            ));
        }
    }
    out
}

pub(crate) async fn run(ctx: &Ctx) -> CliResult {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_model_registry().await;
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| {
        CliError::new(exit::CONNECTION, format!("getPalwModelRegistry: {e} (a node built before ADR-0135 does not serve it)"))
    })?;
    if !response.available {
        return Err(CliError::new(exit::GENERIC, "the node keeps no PALW class state (not a ConsensusV2 network)"));
    }
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": "misaka.palw.registry.v1", "registry": response })).expect("serializable")
        ),
        OutputFormat::Human => print!("{}", render(&response)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_rpc_core::{RpcPalwModelLifecycle, RpcPalwSeatReadiness};

    #[test]
    fn adr0135_the_registry_renders_its_rows_and_says_when_it_is_dormant() {
        let dormant = GetPalwModelRegistryResponse { available: true, tip_daa: 5_798, ..Default::default() };
        let text = render(&dormant);
        assert!(text.contains("dormant (no fence scheduled)") && text.contains("no possession proofs"), "{text}");

        let live = GetPalwModelRegistryResponse {
            available: true,
            tip_daa: 7_000,
            scheduled: true,
            fence_daa: 6_500,
            active: true,
            span_daa: 5,
            seat_count: 5,
            spare_seats: 2,
            classes: vec![RpcPalwModelLifecycle {
                class_id: "ab".repeat(32),
                has_row: true,
                state: "Held".to_string(),
                verification_window_spans: 2,
                max_inflight_claims: 9,
                required_ready_seats: 7,
                ready_seats_now: 3,
                reason: "held: ready 3 < 5 for a panel (recovers through probation once 7 are ready)".to_string(),
                ..Default::default()
            }],
            readiness: vec![RpcPalwSeatReadiness {
                bond_txid: "cd".repeat(32),
                class_id: "ab".repeat(32),
                proved_daa: 6_990,
                fresh: false,
                not_ready_reason: "stale".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = render(&live);
        assert!(
            text.contains("scheduled at DAA 6500")
                && text.contains("Held")
                && text.contains("not ready: stale")
                && text.contains("recovers through probation"),
            "{text}"
        );
    }
}
