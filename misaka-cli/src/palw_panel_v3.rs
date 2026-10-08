//! **`misaka palw panel-v3` — the permissionless Panel's observation, read from a node** (RFC-0010, lane C2).
//!
//! What the chain holds of a V3-rule claim: which rule governs it, its V2 phase, the engine's phase, the seal (the checkpoint it
//! was frozen against), the frozen candidate snapshot, the epoch's certified-output state (`collecting`, `certified` or
//! `unavailable`), the assignment (seed, seats, exposure, the inclusion witness block), the redraws so far and the terminal reason
//! (`SEAL_UNAVAILABLE`, `BEACON_UNAVAILABLE`, `NO_CAPABLE_PANEL`, `PANEL_UNAVAILABLE` — all non-fraud — or `RELEASED`). Read-only
//! (`getPalwPanelV3Status`, op 220: a node built before it drops the connection, so the read goes on a connection of its own).
//! Nothing here is a rule. On today's chain no beacon source is approved, so every sealed claim reads `BEACON_UNAVAILABLE`
//! (`EXTERNAL_GATE_PENDING`): the document says so, it does not paper over it.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::GetPalwPanelV3StatusRequest;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::error::rpc_error_is_connection_loss;

#[derive(clap::Args, Clone, Debug, Default)]
pub(crate) struct PanelV3Args {
    /// A 128-hex claim id (repeatable, at most 64). None: the first tracked claims in id order.
    #[arg(long = "claim", value_name = "CLAIM_ID")]
    pub(crate) claim: Vec<String>,
    /// At most this many tracked claims when none is named (0: the node's default, 16; its cap is 64).
    #[arg(long, default_value_t = 0)]
    pub(crate) limit: u32,
    /// JSON output (`--output json` does the same).
    #[arg(long)]
    pub(crate) json: bool,
}

fn read_error(e: &kaspa_rpc_core::RpcError) -> CliError {
    if rpc_error_is_connection_loss(e) {
        CliError::new(
            exit::COMPONENT_DOWN,
            format!("getPalwPanelV3Status: {e} — the node closed the connection on op 220, so it predates RFC-0010's observation"),
        )
    } else {
        CliError::new(exit::GENERIC, format!("getPalwPanelV3Status: {e}"))
    }
}

fn text(v: &serde_json::Value, key: &str) -> String {
    match &v[key] {
        serde_json::Value::Null => "-".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn short(hash: &str) -> String {
    if hash.len() > 16 { format!("{}…", &hash[..16]) } else { hash.to_string() }
}

/// The human view of one observation document.
pub(crate) fn render(doc: &serde_json::Value) -> String {
    let mut out = String::new();
    let overview = &doc["overview"];
    if overview["active"] != serde_json::Value::Bool(true) {
        out.push_str("permissionless Panel: not active on this chain (the fence has not been crossed)\n");
        return out;
    }
    out.push_str(&format!(
        "permissionless Panel: tip {} (height {}, DAA {}) policy {}\n  tracked {}  pendingSeal {}  sealed {}  entropyReady {}  bound {}  released {}  voided {}  retained work ids {}\n  certified epochs: {}\n",
        short(&text(overview, "tip")),
        text(overview, "height"),
        text(overview, "daa"),
        short(&text(overview, "policyId")),
        text(overview, "trackedClaims"),
        text(overview, "pendingSeal"),
        text(overview, "sealed"),
        text(overview, "entropyReady"),
        text(overview, "bound"),
        text(overview, "released"),
        text(overview, "voided"),
        text(overview, "retainedWorkIds"),
        overview["certifiedEpochs"].as_array().map(|a| a.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "none".into()),
    ));
    for claim in doc["claims"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "\nclaim {}\n  rule {}  accepted DAA {}  V2 phase {}  engine phase {}  retries {}\n",
            text(claim, "claimId"),
            text(claim, "rule"),
            text(claim, "acceptedDaa"),
            text(claim, "v2Phase"),
            text(claim, "enginePhase"),
            text(claim, "retries"),
        ));
        if let Some(seal) = claim["seal"].as_object() {
            out.push_str(&format!("  seal: {}\n", serde_json::Value::Object(seal.clone())));
        }
        if claim["snapshot"].is_object() {
            let s = &claim["snapshot"];
            out.push_str(&format!(
                "  snapshot: root {} checkpoint {} (DAA {}) candidates {}\n",
                short(&text(s, "root")),
                short(&text(s, "checkpoint")),
                text(s, "checkpointDaa"),
                text(s, "candidates")
            ));
        }
        if claim["beacon"].is_object() {
            let b = &claim["beacon"];
            out.push_str(&format!(
                "  beacon: epoch {} release DAA {} deadline DAA {} state {} output {}\n",
                text(b, "epoch"),
                text(b, "releaseDaa"),
                text(b, "deadlineDaa"),
                text(b, "state"),
                short(&text(b, "output"))
            ));
        }
        if claim["assignment"].is_object() {
            let a = &claim["assignment"];
            out.push_str(&format!(
                "  assignment: retry {} seed {} seats {} exposure/seat {} bound DAA {} witness block {}\n",
                text(a, "retryIndex"),
                short(&text(a, "seed")),
                a["seats"].as_array().map(|s| s.len()).unwrap_or(0),
                text(a, "exposure"),
                text(a, "boundDaa"),
                short(&text(a, "bindingBlock"))
            ));
        }
        if claim["terminal"].is_object() {
            let t = &claim["terminal"];
            out.push_str(&format!("  terminal: {} at DAA {} (fraud: {})\n", text(t, "reason"), text(t, "daa"), text(t, "fraud")));
        }
    }
    for id in doc["unknown"].as_array().into_iter().flatten() {
        out.push_str(&format!("\nclaim {}: not held by this state\n", id.as_str().unwrap_or("?")));
    }
    out
}

pub(crate) async fn run(ctx: &Ctx, args: PanelV3Args) -> CliResult {
    let PanelV3Args { claim, limit, json } = args;
    let request = GetPalwPanelV3StatusRequest { claim_ids: claim, limit };
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_panel_v3_status(request).await;
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| read_error(&e))?;
    if !response.available {
        return Err(CliError::new(exit::GENERIC, "this node has no PALW V2 state to read (not ConsensusV2, or no state yet)"));
    }
    let mut doc: serde_json::Value = serde_json::from_str(&response.json)
        .map_err(|e| CliError::new(exit::GENERIC, format!("the node's observation is not JSON: {e}")))?;
    if json || ctx.output == OutputFormat::Json {
        doc["schema"] = "misaka.palw.panel-v3.v1".into();
        println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    } else {
        print!("{}", render(&doc));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subcommand_parses_its_claims_and_limit() {
        use clap::Parser;
        let id = "ab".repeat(64);
        let cli = crate::Cli::try_parse_from(["misaka", "palw", "panel-v3", "--claim", &id, "--limit", "5", "--json"]).expect("parses");
        let crate::Command::Palw(crate::PalwCmd::PanelV3(args)) = cli.command else { panic!("panel-v3") };
        assert_eq!((args.claim, args.limit, args.json), (vec![id], 5, true));
    }

    #[test]
    fn the_human_view_names_the_phase_the_beacon_state_and_the_terminal_reason() {
        let doc = serde_json::json!({
            "version": 1,
            "overview": {"version": 1, "active": true, "tip": "ab".repeat(64), "height": 9, "daa": 1234, "policyId": "cd".repeat(64),
                "trackedClaims": 1, "retainedWorkIds": 0, "pendingSeal": 0, "sealed": 0, "entropyReady": 0, "bound": 0, "released": 0,
                "voided": 1, "certifiedEpochs": []},
            "claims": [{"version": 1, "claimId": "ef".repeat(64), "rule": "permissionlessV3", "acceptedDaa": 1001,
                "v2Phase": "voided:BeaconUnavailable", "enginePhase": "voided", "acceptanceOrder": 0, "seal": null, "snapshot": null,
                "beacon": {"epoch": 3, "releaseDaa": 1010, "deadlineDaa": 1018, "scheme": "00", "state": "unavailable", "output": null},
                "assignment": null, "retries": 0, "terminal": {"reason": "BEACON_UNAVAILABLE", "daa": 1019, "fraud": false}}],
            "unknown": ["12".repeat(64)]
        });
        let text = render(&doc);
        assert!(text.contains("engine phase voided"), "{text}");
        assert!(text.contains("state unavailable"), "{text}");
        assert!(text.contains("terminal: BEACON_UNAVAILABLE at DAA 1019 (fraud: false)"), "{text}");
        assert!(text.contains("not held by this state"), "{text}");
        let inactive = serde_json::json!({"overview": {"active": false}, "claims": [], "unknown": []});
        assert!(render(&inactive).contains("not active"));
    }
}
