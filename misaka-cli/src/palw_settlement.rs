//! ADR-0127 Decision 3 — `misaka palw settlement`, and the settlement depth `wallet utxo list`
//! prints beside each output.
//!
//! A transaction's security is the number of settled PALW anchors at or after the block that
//! accepted it — blocks whose PALW claim reached `Final` — and never a count of blocks: an execution
//! block or a heartbeat block adds no anchor. The node answers from its sink's state
//! (`getPalwSettlement`, op 182); this module prints the answer and decides the exit a script waits
//! on.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::GetPalwSettlementResponse;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::KaspaRpcClient;
use std::collections::{BTreeMap, BTreeSet};

/// The one line `misaka palw settlement` prints.
pub(crate) fn settlement_line(r: &GetPalwSettlementResponse) -> String {
    if !r.available {
        return format!(
            "settlement unavailable at DAA {}: the node keeps no PALW state, or cannot date its safe frontier",
            r.daa_score
        );
    }
    if r.settled {
        format!("settled at depth {} ({} anchors pending) — frontier DAA {}", depth_text(r), r.pending_anchors, r.safe_frontier_daa)
    } else if r.safe_frontier_blue_score == 0 && r.safe_frontier_daa == 0 {
        format!("not settled: no anchor has reached Final yet, {} anchors pending", r.pending_anchors)
    } else {
        format!("not settled: frontier DAA {} < {}, {} anchors pending", r.safe_frontier_daa, r.daa_score, r.pending_anchors)
    }
}

/// `12`, or `≥12` where older anchors may have retired from the node's state and the count is a
/// lower bound.
fn depth_text(r: &GetPalwSettlementResponse) -> String {
    if r.depth_is_lower_bound { format!("≥{}", r.depth) } else { r.depth.to_string() }
}

/// **The exit a script waits on.** Without `--min-depth` the command reports and succeeds; with it,
/// success only once settled at that many anchors or more, and [`exit::TIMEOUT_PENDING`] until then.
/// A node that cannot answer is an error, never "not yet": waiting on it would wait forever.
pub(crate) fn settlement_exit(r: &GetPalwSettlementResponse, min_depth: Option<u64>) -> i32 {
    if !r.available {
        return exit::GENERIC;
    }
    match min_depth {
        None => exit::SUCCESS,
        Some(n) if r.settled && r.depth >= n => exit::SUCCESS,
        Some(_) => exit::TIMEOUT_PENDING,
    }
}

/// `misaka palw settlement --daa <d> [--min-depth <n>]`.
pub(crate) async fn run(ctx: &Ctx, daa: u64, min_depth: Option<u64>) -> CliResult {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_settlement(daa).await;
    let connected = reader.client.is_connected();
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| {
        let why =
            if connected { e.to_string() } else { format!("{e} (a node built before getPalwSettlement closes the connection on it)") };
        CliError::new(exit::CONNECTION, format!("getPalwSettlement: {why}"))
    })?;
    let code = settlement_exit(&response, min_depth);
    match ctx.output {
        OutputFormat::Json => {
            let mut doc = serde_json::to_value(&response).expect("a response serializes");
            doc["schema"] = "misaka.palw.settlement.v1".into();
            doc["minDepth"] = min_depth.into();
            doc["reached"] = min_depth.map(|_| code == exit::SUCCESS).into();
            println!("{doc}");
        }
        OutputFormat::Human => println!("{}", settlement_line(&response)),
    }
    match (code, min_depth) {
        (exit::SUCCESS, _) => Ok(()),
        (exit::TIMEOUT_PENDING, Some(n)) => Err(CliError::new(
            code,
            if response.settled {
                format!("depth {} of {n} anchors at DAA {daa}: wait for more anchors", depth_text(&response))
            } else {
                format!("DAA {daa} is not settled yet ({} anchors pending): wait for more anchors", response.pending_anchors)
            },
        )),
        _ => Err(CliError::new(code, settlement_line(&response))),
    }
}

/// The settlement column `wallet utxo list` prints for one output.
pub(crate) fn settlement_cell(r: &GetPalwSettlementResponse) -> String {
    if r.settled { format!("depth {}", depth_text(r)) } else { format!("unsettled ({} pending)", r.pending_anchors) }
}

/// **One read per distinct DAA score, or none at all.** The first call is the probe: a node built
/// before `getPalwSettlement` closes the WebSocket on the op rather than refusing it, so the caller
/// must make this the connection's last read, and a failure — or a node that keeps no PALW state it
/// can date — leaves the column out instead of failing the command.
pub(crate) async fn settlement_by_daa(
    client: &KaspaRpcClient,
    scores: impl IntoIterator<Item = u64>,
) -> Option<BTreeMap<u64, GetPalwSettlementResponse>> {
    let mut answers = BTreeMap::new();
    for daa in scores.into_iter().collect::<BTreeSet<_>>() {
        match client.get_palw_settlement(daa).await {
            Ok(response) if response.available => {
                answers.insert(daa, response);
            }
            _ => return None,
        }
    }
    Some(answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(settled: bool, depth: u64, pending: u64, lower_bound: bool) -> GetPalwSettlementResponse {
        GetPalwSettlementResponse {
            available: true,
            sink_daa: 5_000,
            daa_score: 4_000,
            settled,
            depth,
            pending_anchors: pending,
            depth_is_lower_bound: lower_bound,
            safe_frontier_blue_score: if settled { 4_100 } else { 3_900 },
            safe_frontier_daa: if settled { 4_200 } else { 3_950 },
        }
    }

    #[test]
    fn the_line_names_the_depth_in_anchors_and_the_frontier() {
        assert_eq!(settlement_line(&answer(true, 12, 3, false)), "settled at depth 12 (3 anchors pending) — frontier DAA 4200");
        assert_eq!(settlement_line(&answer(true, 12, 0, true)), "settled at depth ≥12 (0 anchors pending) — frontier DAA 4200");
        assert_eq!(settlement_line(&answer(false, 1, 2, false)), "not settled: frontier DAA 3950 < 4000, 2 anchors pending");
        let no_frontier =
            GetPalwSettlementResponse { safe_frontier_blue_score: 0, safe_frontier_daa: 0, ..answer(false, 0, 4, false) };
        assert_eq!(settlement_line(&no_frontier), "not settled: no anchor has reached Final yet, 4 anchors pending");
        let unavailable = GetPalwSettlementResponse { daa_score: 4_000, ..Default::default() };
        assert!(settlement_line(&unavailable).starts_with("settlement unavailable at DAA 4000"));
    }

    #[test]
    fn min_depth_exits_zero_only_once_settled_that_deep() {
        assert_eq!(settlement_exit(&answer(true, 12, 0, false), None), exit::SUCCESS, "a report succeeds");
        assert_eq!(settlement_exit(&answer(false, 0, 3, false), None), exit::SUCCESS, "a report of not-settled succeeds too");
        assert_eq!(settlement_exit(&answer(true, 6, 0, false), Some(6)), exit::SUCCESS, "exactly the depth asked");
        assert_eq!(settlement_exit(&answer(true, 5, 1, false), Some(6)), exit::TIMEOUT_PENDING, "one anchor short");
        assert_eq!(settlement_exit(&answer(true, 9, 0, true), Some(6)), exit::SUCCESS, "a lower bound past the depth is past it");
        assert_eq!(
            settlement_exit(&answer(false, 40, 0, false), Some(6)),
            exit::TIMEOUT_PENDING,
            "depth without settlement is not settled"
        );
        assert_eq!(settlement_exit(&answer(true, 0, 0, false), Some(0)), exit::SUCCESS, "--min-depth 0 is settled at all");
        let unavailable = GetPalwSettlementResponse::default();
        assert_eq!(settlement_exit(&unavailable, Some(1)), exit::GENERIC, "a node that cannot answer is no reason to wait");
        assert_eq!(settlement_exit(&unavailable, None), exit::GENERIC);
    }

    #[test]
    fn the_wallet_column_is_the_depth_or_what_is_pending() {
        assert_eq!(settlement_cell(&answer(true, 12, 3, false)), "depth 12");
        assert_eq!(settlement_cell(&answer(true, 7, 0, true)), "depth ≥7");
        assert_eq!(settlement_cell(&answer(false, 0, 2, false)), "unsettled (2 pending)");
    }
}
