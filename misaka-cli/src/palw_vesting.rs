//! **ADR-0152 P2-11 — `misaka palw vesting`: where a vested reward stands** (testnet-12, R-core+).
//!
//! Past `palw_rcore_plus` a Final claim's reward is not paid at Final. It is named in a vesting row,
//! waits out the conviction window on two clocks (the DAA clock to `expiry`, and the second clock's
//! licences), latches, waits its turn behind every row before it, moves into the payout queue and is
//! minted by the next coinbase — whose output then matures like any coinbase output. A conviction
//! inside the window burns the row instead. This command prints that path for a bond, a payout
//! address or a claim, from `getPalwVesting` (op 199), in the node's own terms: every date and every
//! "moves next block" is the node's answer from the vesting rules themselves, never this CLI's
//! arithmetic. The one thing added here is time: an ETA in DAA is converted at the pace the node
//! measured (`measuredMsPerDaa`), and marked as an estimate.
//!
//! **Op 199 is new.** A node built before it does not answer "unknown method": it drops the
//! WebSocket. So the read is this command's only one on its connection, and a dropped connection is
//! reported as the node's age ([`vesting_read_error`]), not as a network fault.

use crate::node::Ctx;
use crate::operator::catalog::msk;
use crate::operator::status::group;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwVestingRequest, GetPalwVestingResponse, RpcPalwVestingRow};

/// What the probe of op 199 says when it fails: a node that DROPPED the connection predates the op
/// (it closes the WebSocket on an unknown one); a node still connected refused the request for its
/// own reason, which is printed as it came.
pub(crate) fn vesting_read_error(e: impl std::fmt::Display, still_connected: bool) -> CliError {
    if still_connected {
        CliError::new(exit::GENERIC, format!("getPalwVesting: {e}"))
    } else {
        CliError::new(
            exit::COMPONENT_DOWN,
            format!(
                "getPalwVesting: {e} — the node closed the connection on op 199, so it predates ADR-0152's vesting read; \
                 rebuild it from this tree (its rewards still vest: only the read is missing)"
            ),
        )
    }
}

/// `~ 3h 20m` for `daa` DAA at the node's measured pace; empty without one.
pub(crate) fn duration_at_pace(daa: u64, ms_per_daa: Option<u64>) -> String {
    let Some(ms) = ms_per_daa.filter(|ms| *ms > 0) else { return String::new() };
    let secs = daa.saturating_mul(ms) / 1000;
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3_600, (secs % 3_600) / 60);
    match (d, h) {
        (0, 0) => format!("≈ {m}m"),
        (0, _) => format!("≈ {h}h {m:02}m"),
        _ => format!("≈ {d}d {h}h"),
    }
}

fn u128_of(text: &str) -> u128 {
    text.parse().unwrap_or(0)
}

fn short(id: &str) -> &str {
    &id[..8.min(id.len())]
}

/// **Where one row stands, in one line**: its stage, then what still holds it — the DAA clock, the
/// second clock's licences, a DA session, a halt — or its turn in the queue, and the earliest DAA it
/// moves (exact when the next block moves it; otherwise a lower bound, marked `≥`).
pub(crate) fn row_status(row: &RpcPalwVestingRow, now: u64, halted: bool, ms_per_daa: Option<u64>) -> String {
    let when = |daa: u64| {
        let pace = duration_at_pace(daa.saturating_sub(now), ms_per_daa);
        if pace.is_empty() { format!("DAA {}", group(daa)) } else { format!("DAA {} ({pace})", group(daa)) }
    };
    if row.in_next_block {
        return format!("moves in the next block (DAA {}); minted by the block after", group(row.eta_daa));
    }
    if let Some(at) = row.matured_at {
        let ahead = row.moves_ahead.map(|m| format!(", {m} move(s) ahead")).unwrap_or_default();
        return format!("latched at DAA {}; waits its turn{ahead} — moves ≥ {}", group(at), when(row.eta_daa));
    }
    let mut held = Vec::new();
    if !row.daa_clock_met {
        held.push(format!("the DAA clock to {}", when(row.expiry_daa)));
    }
    if let Some(needed) = row.licences_needed.filter(|needed| row.licences_since_final < *needed) {
        let bound = row.second_clock_bound_daa.map(|b| format!(", released by DAA {} at the latest", group(b))).unwrap_or_default();
        held.push(format!("{} more licence(s) of {needed}{bound}", needed - row.licences_since_final));
    }
    if row.da_session_open {
        held.push("an open DA session on its claim".to_string());
    }
    if halted {
        held.push("the chain's licence halt (nothing latches until an anchor settles)".to_string());
    }
    if held.is_empty() {
        format!("mature; latches in the next block — moves ≥ {}", when(row.eta_daa))
    } else {
        format!("vesting: held by {} — moves ≥ {}", held.join(" and "), when(row.eta_daa))
    }
}

/// **The human answer.** `spend_after` is the coinbase spend maturity (the wallet's gate on a minted
/// output).
pub(crate) fn render(r: &GetPalwVestingResponse, spend_after: u64) -> String {
    let mut out = String::new();
    if !r.available {
        out.push_str("vesting unavailable: the node keeps no ConsensusV2 PALW state (not testnet-12, or still syncing)\n");
        return out;
    }
    if !r.rcore_plus_active {
        out.push_str(&format!(
            "nothing vests on this network at DAA {}: a Final's reward is queued for the next coinbase at Final (no R-core+ fence)\n",
            group(r.next_daa)
        ));
        return out;
    }
    let pace = r.measured_ms_per_daa.map(|ms| format!(" · {:.1} s/DAA measured", ms as f64 / 1000.0)).unwrap_or_default();
    out.push_str(&format!("VESTING · next block at DAA {} (tip {}){pace}\n", group(r.next_daa), group(r.tip_daa)));
    out.push_str(&format!(
        "  chain     {} row(s) live, {} latched ({} behind an unlatched head) · {} MSK vesting, {} latched\n",
        r.live_rows,
        r.latched_rows,
        r.latched_behind_head,
        msk(u128_of(&r.live_sompi)),
        msk(u128_of(&r.latched_sompi))
    ));
    out.push_str(&format!(
        "            since genesis: created {} · minted {} · burned by convictions {} MSK\n",
        msk(u128_of(&r.created_sompi)),
        msk(u128_of(&r.moved_sompi)),
        msk(u128_of(&r.burned_sompi))
    ));
    let clock = match (r.second_clock_depth, r.halted) {
        (None, _) => "no second clock on this network: rows mature on the DAA clock alone".to_string(),
        (Some(_), true) => "HALTED — no anchor has settled for 2 × window_court: nothing latches until one does".to_string(),
        (Some(depth), false) => format!("second clock: a row also waits for {depth} settled anchor(s) past its Final (now {})", r.settled_anchors),
    };
    out.push_str(&format!("            {clock}\n"));
    let stop = match r.next_block_stopped.as_str() {
        "not_latched" => format!(", then stops at {} (not latched: stop, never skip)", short(&r.next_block_stopped_at)),
        "budget_full" => format!(", then stops at {} (the block's budget of new payout keys is spent — V-7)", short(&r.next_block_stopped_at)),
        _ => String::new(),
    };
    out.push_str(&format!(
        "            next block moves {} (legs {}, new payout keys {}){stop}\n",
        r.next_block_moves.len(),
        r.next_block_legs,
        r.next_block_new_keys
    ));
    if !r.licence_histogram.is_empty() {
        let doors: Vec<String> = r.licence_histogram.iter().map(|d| format!("{} {}", d.door, d.rows)).collect();
        out.push_str(&format!("            rows by licence door: {}\n", doors.join(" · ")));
    }
    let who = if !r.bond.is_empty() {
        Some(format!("bond {}", r.bond))
    } else if !r.payout_address.is_empty() {
        Some(format!("address {}", r.payout_address))
    } else if !r.claim_id.is_empty() {
        Some(format!("claim {}", short(&r.claim_id)))
    } else {
        None
    };
    if let Some(who) = who {
        out.push_str(&format!(
            "  {who}\n            {} row(s) · vesting {} MSK · latched {} MSK\n",
            r.rows_total,
            msk(u128_of(&r.maturing_sompi)),
            msk(u128_of(&r.query_latched_sompi))
        ));
        if !r.bond.is_empty() && !r.bond_known {
            out.push_str("            the registry holds no bond at this outpoint\n");
        }
        if r.payee_holds_collateral {
            out.push_str(
                "            B-3: this bond's collateral is LOCKED while it is payee of a row the conviction window still holds \
                 (its outpoint cannot be spent until the last such row matures)\n",
            );
        }
    }
    if !r.rows.is_empty() {
        out.push_str(&format!("  {:<10}{:<10}{:>16}  {}\n", "CLAIM", "STAGE", "MSK", "WHERE IT STANDS"));
        for row in &r.rows {
            let amount = if who_is_payee(r) { row.legs_sompi } else { row.total_sompi };
            out.push_str(&format!(
                "  {:<10}{:<10}{:>16}  {}\n",
                short(&row.claim_id),
                row.stage,
                msk(amount as u128),
                row_status(row, r.next_daa, r.halted, r.measured_ms_per_daa)
            ));
        }
        if !r.next_after.is_empty() {
            out.push_str(&format!("  … more: --after {}\n", r.next_after));
        }
    }
    for reward in &r.reporter_rewards {
        let state = match (reward.stage.as_str(), reward.reveal_until) {
            ("pending", Some(until)) => format!("pending — its reveal window closes at DAA {}", group(until)),
            (_, _) if reward.in_next_block => "awarded — moves in the next block".to_string(),
            _ => "awarded — waits its turn at the head of the queue".to_string(),
        };
        out.push_str(&format!("  reporter  {:<10}{:>16}  {state}\n", short(&reward.offence_key), msk(reward.sompi as u128)));
    }
    out.push_str(&format!(
        "  a moved row is minted by the next coinbase; that output is spendable {spend_after} DAA after its block (the wallet's gate)\n"
    ));
    out
}

/// A payee read shows each row's legs to that payee; a claim or chain read the whole row.
fn who_is_payee(r: &GetPalwVestingResponse) -> bool {
    !r.bond.is_empty() || !r.payout_address.is_empty()
}

/// `misaka palw vesting [--bond B | --address A | --claim C] [--limit N] [--after CURSOR]`.
pub(crate) async fn run(
    ctx: &Ctx,
    bond: Option<String>,
    address: Option<String>,
    claim: Option<String>,
    limit: u32,
    after: Option<String>,
    json: bool,
) -> CliResult {
    let request = GetPalwVestingRequest {
        bond: bond.unwrap_or_default(),
        payout_address: address.unwrap_or_default(),
        claim_id: claim.unwrap_or_default(),
        limit,
        after: after.unwrap_or_default(),
    };
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_vesting(request).await;
    let connected = reader.client.is_connected();
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| vesting_read_error(e, connected))?;
    let params = kaspa_consensus_core::config::params::Params::from(
        <kaspa_consensus_core::network::NetworkId as std::str::FromStr>::from_str(&ctx.network)
            .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?,
    );
    if json || ctx.output == OutputFormat::Json {
        let mut doc = serde_json::to_value(&response).expect("a response serializes");
        doc["schema"] = "misaka.palw.vesting.v1".into();
        doc["coinbaseSpendMaturity"] = params.coinbase_spend_maturity().into();
        println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    } else {
        print!("{}", render(&response, params.coinbase_spend_maturity()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_rpc_core::RpcPalwReporterReward;

    fn row(stage: &str) -> RpcPalwVestingRow {
        RpcPalwVestingRow {
            claim_id: "ab".repeat(64),
            stage: stage.to_string(),
            expiry_daa: 9_000,
            licences_needed: Some(30),
            licences_since_final: 12,
            second_clock_bound_daa: Some(15_000),
            eta_daa: 9_000,
            eta_estimated: true,
            total_sompi: 1_000_000_000,
            legs_sompi: 500_000_000,
            ..Default::default()
        }
    }

    /// Every hold is named with its date: the DAA clock, the licences still owed, a halt.
    #[test]
    fn a_rows_status_names_what_holds_it() {
        let text = row_status(&row("maturing"), 8_000, false, Some(120_000));
        assert!(text.contains("the DAA clock to DAA 9,000 (≈ 1d 9h)"), "{text}");
        assert!(text.contains("18 more licence(s) of 30, released by DAA 15,000 at the latest"), "{text}");
        assert!(text.contains("moves ≥ DAA 9,000"), "{text}");
        let halted = row_status(&RpcPalwVestingRow { daa_clock_met: true, licences_since_final: 30, ..row("maturing") }, 9_100, true, None);
        assert!(halted.contains("licence halt") && !halted.contains("DAA clock"), "{halted}");
        let next = row_status(&RpcPalwVestingRow { in_next_block: true, eta_daa: 9_101, eta_estimated: false, ..row("latched") }, 9_101, false, None);
        assert_eq!(next, "moves in the next block (DAA 9,101); minted by the block after");
        let latched = row_status(&RpcPalwVestingRow { matured_at: Some(9_050), moves_ahead: Some(4), eta_daa: 9_102, ..row("latched") }, 9_101, false, None);
        assert!(latched.contains("latched at DAA 9,050; waits its turn, 4 move(s) ahead"), "{latched}");
    }

    /// The answer says what B-3 does to the bond, what the chain holds and burned, each reporter
    /// reward's state, and that a minted output still waits the coinbase maturity.
    #[test]
    fn the_answer_names_the_hold_the_burns_and_the_spend_gate() {
        let r = GetPalwVestingResponse {
            available: true,
            rcore_plus_active: true,
            tip_daa: 8_999,
            next_daa: 9_000,
            second_clock_depth: Some(30),
            created_sompi: "300000000000".into(),
            moved_sompi: "100000000000".into(),
            burned_sompi: "2500000000".into(),
            live_sompi: "197500000000".into(),
            latched_sompi: "0".into(),
            bond: format!("{}:0", "cd".repeat(64)),
            bond_known: true,
            payee_holds_collateral: true,
            rows: vec![row("maturing")],
            rows_total: 1,
            maturing_sompi: "500000000".into(),
            query_latched_sompi: "0".into(),
            reporter_rewards: vec![RpcPalwReporterReward {
                offence_key: "ef".repeat(64),
                stage: "awarded".into(),
                sompi: 33,
                in_next_block: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = render(&r, 600);
        assert!(text.contains("burned by convictions 25.00 MSK"), "{text}");
        assert!(text.contains("B-3: this bond's collateral is LOCKED"), "{text}");
        assert!(
            text.contains("5.00  vesting: held") && !text.contains("10.00  vesting"),
            "a payee read prints the payee's legs (5 MSK), not the row (10): {text}"
        );
        assert!(text.contains("awarded — moves in the next block"), "{text}");
        assert!(text.contains("spendable 600 DAA after its block"), "{text}");
        let halted = render(&GetPalwVestingResponse { halted: true, ..r.clone() }, 600);
        assert!(halted.contains("HALTED"), "{halted}");
        let dormant = render(&GetPalwVestingResponse { rcore_plus_active: false, ..r }, 600);
        assert!(dormant.starts_with("nothing vests on this network"), "{dormant}");
    }

    /// **T51/T52: an old node drops the unknown op and the probe says so** — a dropped connection is
    /// the node's age (COMPONENT_DOWN, rebuild), a refusal on a live one is the node's own reason.
    #[test]
    fn t51_a_dropped_connection_reads_as_a_node_older_than_op_199() {
        let old = vesting_read_error("WebSocket closed", false);
        assert_eq!(old.code, exit::COMPONENT_DOWN);
        assert!(old.msg.contains("predates ADR-0152's vesting read"));
        let refused = vesting_read_error("name at most one of bond, payoutAddress and claimId", true);
        assert_eq!(refused.code, exit::GENERIC);
        assert!(!refused.msg.contains("predates"));
    }
}
