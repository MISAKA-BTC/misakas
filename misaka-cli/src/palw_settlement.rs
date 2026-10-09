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
    if let Some(native) = &r.native_settlement {
        // A below-finalized conflict is an alarm, not a stop reason: every settlement label is withheld until the node resyncs.
        if native.stop == Some(kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1::FinalizedConflict) {
            return "SAFETY ALARM: this node's chain abandons a head it had published as finalized; safe and finalized are withheld \
                    until it is resynced (a validated pruning-point import) — do not rely on its settlement labels"
                .into();
        }
        let head = |h: Option<kaspa_consensus_core::Hash64>| h.map(|h| h.to_string()).unwrap_or_else(|| "unavailable".into());
        return format!("PALW native settlement: latest={} safe={} finalized={} depth={} work={} stop={:?}",
            head(native.latest), head(native.safe), head(native.finalized), native.depth, native.unique_work, native.stop);
    }
    if let Some(fence) = r.dns_retired_at { return format!("DNS retired at DAA {fence}; native settlement snapshot unavailable"); }
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

/// One short clause for one thing `safe` is waiting for. Every number is the node's own; nothing here is a promise.
fn wait_clause(w: &kaspa_consensus_core::palw_native_readiness_v1::SafeWaitV1) -> String {
    use kaspa_consensus_core::palw_native_readiness_v1::SafeWaitV1 as W;
    match w {
        W::InvalidPolicy => "the settlement policy is unusable".into(),
        W::MissingHistory { gap, block } => match block {
            Some(b) => format!("missing history ({gap:?}) at {b}"),
            None => format!("missing history ({gap:?})"),
        },
        W::Unexecuted => "no executed block yet".into(),
        W::FinalizedConflict => "a finalized-head conflict is recorded".into(),
        W::FrontierBehind { frontier_blue, effect_blue, frontier_on_branch } => {
            format!("safe frontier at blue {frontier_blue} has not reached blue {effect_blue}{}", if *frontier_on_branch { "" } else { " (not on this branch)" })
        }
        W::OpenClaim { claim, stage, wait_daa, .. } => match wait_daa {
            Some(d) => format!("claim {claim} is {stage:?}, trace retention lapses in {d} DAA"),
            None => format!("claim {claim} is still {stage:?}"),
        },
        W::OpenDaSession { claim, claim_known, deadline_daa, wait_daa } => format!(
            "data-availability court open on claim {claim}{} until DAA {deadline_daa} ({wait_daa} DAA)",
            if *claim_known { "" } else { " (claim not in state)" }
        ),
        W::WaitingMaturity { facts, work, wait_daa, ready_daa, .. } => match ready_daa {
            Some(at) => format!("waiting on maturity: {facts} fact(s), work {work}; the first matures in {wait_daa} DAA, enough by DAA {at}"),
            None => format!("waiting on maturity: {facts} fact(s), work {work}; the first matures in {wait_daa} DAA, but they would not be enough"),
        },
        W::InsufficientDepth { have, need } => format!("{have} of {need} settled anchors"),
        W::InsufficientWork { have, need } => format!("work {have} of {need}"),
        W::ConcentratedWork { dimension, top_permille, cap_permille } => {
            format!("{dimension:?} concentration {top_permille} permille over the {cap_permille} cap")
        }
        W::DuplicateWork => "one work identity appears twice".into(),
        W::ArithmeticOverflow => "evidence arithmetic overflowed".into(),
    }
}

/// **Why `safe` is where it is** — the lines printed under the settlement line when the node explains itself (RFC-0012 D1).
/// Empty on a node that sends no explanation.
pub(crate) fn readiness_lines(r: &GetPalwSettlementResponse) -> Vec<String> {
    let Some(x) = &r.native_readiness else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(w) = &x.stopped_early {
        out.push(format!("  safe is withheld: {}", wait_clause(w)));
        return out;
    }
    match (x.safe_lag_daa, x.safe_lag_blue) {
        (Some(daa), Some(blue)) => out.push(format!("  safe trails the newest executed block by {daa} DAA / {blue} blue")),
        _ => out.push("  no executed block is safe yet".into()),
    }
    if let Some(b) = &x.blocking {
        out.push(format!("  safe cannot pass {} (DAA {}, blue {}):", b.block, b.daa, b.blue));
        for w in &b.waits {
            out.push(format!("    - {}", wait_clause(w)));
        }
        if b.open_claims_total > 8 || b.open_sessions_total > 8 {
            out.push(format!("    ({} open claim(s), {} open session(s) in all)", b.open_claims_total, b.open_sessions_total));
        }
        if let Some(d) = b.earliest_ready_in_daa {
            out.push(format!("    on the facts already on the chain, no sooner than {d} DAA from now"));
        }
    }
    if let Some(head) = &x.finalized.withdrawn_from {
        out.push(format!(
            "  FINALIZED LABEL WITHDRAWN: {head} was published as finalized and is still canonical, but the evidence no longer certifies it; \
             safe and finalized read null until it is certified again"
        ));
    } else if let Some(w) = &x.finalized.wait {
        out.push(format!("  finalized: {w:?}"));
    }
    if x.skipped.total() > 0 {
        out.push(format!(
            "  evidence seen and not counted: {} voided, {} floor, {} under a DA session, {} unpriced, {} with no bond in state",
            x.skipped.voided, x.skipped.base_class, x.skipped.open_da, x.skipped.unpriced, x.skipped.bond_not_held
        ));
    }
    out
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
    // A node in FinalizedConflict will never settle anything until it is resynced: that is an error to a script, not "not yet".
    if r.native_settlement.as_ref().is_some_and(|s| s.stop == Some(kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1::FinalizedConflict)) {
        return exit::GENERIC;
    }
    match min_depth {
        None => exit::SUCCESS,
        Some(n) if r.settled && r.native_settlement.as_ref().map_or(r.depth, |s| s.depth) >= n => exit::SUCCESS,
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
            doc["schema"] = if response.dns_retired_at.is_some() { "misaka.palw.settlement.v2" } else { "misaka.palw.settlement.v1" }.into();
            doc["minDepth"] = min_depth.into();
            doc["reached"] = min_depth.map(|_| code == exit::SUCCESS).into();
            println!("{doc}");
        }
        OutputFormat::Human => {
            println!("{}", settlement_line(&response));
            for line in readiness_lines(&response) {
                println!("{line}");
            }
        }
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
    if r.dns_retired_at.is_some() && !r.settled { return "native unsettled / unavailable".into(); }
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
            dns_retired_at: None, native_settlement: None, native_readiness: None,
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

    /// RFC-0012 D1: the explanation prints what `safe` waits for, in the node's own numbers — and nothing when the node sends none.
    #[test]
    fn rfc0012_the_readiness_lines_name_what_safe_waits_for() {
        use kaspa_consensus_core::palw_native_readiness_v1::*;
        use kaspa_consensus_core::palw_native_settlement_v1::SkippedEvidenceV1;
        let h = kaspa_consensus_core::Hash64::from_u64_word;
        assert!(readiness_lines(&answer(true, 1, 0, false)).is_empty(), "no explanation, no lines");
        let blocking = EffectReadinessV1 {
            block: h(0xB4),
            daa: 1_001,
            blue: 900,
            in_safe_prefix: false,
            waits: vec![
                SafeWaitV1::OpenClaim {
                    claim: h(0xC1),
                    stage: ClaimStageV1::Final,
                    accepted_blue: 880,
                    retention_daa: 6_401,
                    next_deadline_daa: None,
                    wait_daa: Some(4_000),
                },
                SafeWaitV1::InsufficientDepth { have: 1, need: 3 },
                SafeWaitV1::WaitingMaturity { facts: 2, work: "40".into(), earliest_matured_daa: 6_401, wait_daa: 4_000, ready_daa: Some(6_500) },
            ],
            open_claims_total: 1,
            open_sessions_total: 0,
            earliest_ready_in_daa: Some(4_099),
            evidence: EvidenceTallyV1 { anchors: 1, work: "20".into(), matured_facts: 1, pending_facts: 2, pending_work: "40".into() },
        };
        let readiness = NativeSafeReadinessV1 {
            version: 1,
            generation: h(0x51),
            sink_daa: 2_401,
            sink_blue: 2_000,
            policy: ReadinessPolicyV1 {
                settled_anchor_depth: 3,
                unique_mature_work: "60".into(),
                max_operator_permille: 600,
                max_class_permille: 600,
            },
            maturity: native_maturity_report_v1(3_000, 120),
            executed_effects: 40,
            safe: Some(h(0xB3)),
            safe_lag_daa: Some(1_400),
            safe_lag_blue: Some(1_100),
            stop: Some(kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1::OpenLifecycle),
            stopped_early: None,
            blocking: Some(blocking),
            tip: None,
            finalized: FinalizedReadinessV1 {
                finalized: None,
                pruning_point: h(1),
                pruning_blue: Some(0),
                wait: Some(FinalizedWaitV1::PruningPointNotExecuted),
                withdrawn_from: None,
            },
            skipped: SkippedEvidenceV1 { bond_not_held: 1, ..Default::default() },
        };
        let response = GetPalwSettlementResponse { native_readiness: Some(readiness.clone()), ..answer(false, 1, 0, false) };
        let text = readiness_lines(&response).join("\n");
        for want in [
            "safe trails the newest executed block by 1400 DAA / 1100 blue",
            "safe cannot pass",
            "trace retention lapses in 4000 DAA",
            "1 of 3 settled anchors",
            "waiting on maturity: 2 fact(s), work 40; the first matures in 4000 DAA, enough by DAA 6500",
            "no sooner than 4099 DAA from now",
            "finalized: PruningPointNotExecuted",
            "1 with no bond in state",
        ] {
            assert!(text.contains(want), "{want:?} in\n{text}");
        }
        // Weighing that never happened says why, and nothing else.
        let stopped = NativeSafeReadinessV1 {
            stopped_early: Some(SafeWaitV1::MissingHistory { gap: HistoryGapV1::DeltaNotRetained, block: Some(h(0xB1)) }),
            blocking: None,
            safe: None,
            safe_lag_daa: None,
            safe_lag_blue: None,
            ..readiness.clone()
        };
        let lines = readiness_lines(&GetPalwSettlementResponse { native_readiness: Some(stopped), ..answer(false, 0, 0, false) });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("  safe is withheld: missing history (DeltaNotRetained) at "), "{}", lines[0]);
        // A published finalized label that was withdrawn is an alarm line, naming the head (RFC-0012 C11).
        let mut withdrawn = readiness;
        withdrawn.finalized.withdrawn_from = Some(h(0xF1));
        let text = readiness_lines(&GetPalwSettlementResponse { native_readiness: Some(withdrawn), ..answer(false, 1, 0, false) }).join("\n");
        assert!(text.contains("FINALIZED LABEL WITHDRAWN"), "{text}");
        assert!(!text.contains("finalized: PruningPointNotExecuted"), "the withdrawal replaces the plain wait line: {text}");
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

    /// RFC-0012: a finalized conflict is an alarm line and an error exit (a script must not wait on a node that cannot settle),
    /// while an ordinary stop reason stays a report that waits.
    #[test]
    fn rfc0012_a_finalized_conflict_is_an_alarm_and_an_error_exit_and_an_ordinary_stop_is_not() {
        use kaspa_consensus_core::palw_native_settlement_v1::{NativeSettlementSnapshotV1, SettlementStopV1};
        let native = |stop| NativeSettlementSnapshotV1 {
            version: 1,
            ruleset_id: Default::default(),
            policy_id: Default::default(),
            generation: Default::default(),
            retirement_daa: 5,
            frontier: None,
            latest: None,
            safe: None,
            finalized: None,
            depth: 0,
            unique_work: "0".into(),
            stop: Some(stop),
        };
        let response = |stop| GetPalwSettlementResponse {
            available: true,
            dns_retired_at: Some(5),
            native_settlement: Some(native(stop)),
            ..Default::default()
        };
        let conflict = response(SettlementStopV1::FinalizedConflict);
        assert!(settlement_line(&conflict).starts_with("SAFETY ALARM"));
        assert_eq!(settlement_exit(&conflict, Some(1)), exit::GENERIC);
        assert_eq!(settlement_exit(&conflict, None), exit::GENERIC);
        let ordinary = response(SettlementStopV1::FrontierNotCovered);
        assert!(settlement_line(&ordinary).contains("stop=Some(FrontierNotCovered)"));
        assert_eq!(settlement_exit(&ordinary, Some(1)), exit::TIMEOUT_PENDING, "still waiting");
        assert_eq!(settlement_exit(&ordinary, None), exit::SUCCESS, "a report succeeds");
    }

    #[test]
    fn the_wallet_column_is_the_depth_or_what_is_pending() {
        assert_eq!(settlement_cell(&answer(true, 12, 3, false)), "depth 12");
        assert_eq!(settlement_cell(&answer(true, 7, 0, true)), "depth ≥7");
        assert_eq!(settlement_cell(&answer(false, 0, 2, false)), "unsettled (2 pending)");
    }
}
