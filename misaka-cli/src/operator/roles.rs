//! **`misaka rewards` and `misaka verifier status`** — ADR-0122 §6.4 and §8.2.
//!
//! `rewards` answers "what have I been paid, and what is still coming?", split the way the chain
//! pays. The block share arrives in a coinbase whatever becomes of the claim. The escrow waits for
//! `final`, and a void destroys it. A prompt-lane claim is paid only through receipt blocks this
//! bond's own producer mines.
//!
//! **ADR-0152 (testnet-12): `final` is not paid.** Past `palw_rcore_plus` a Final names its reward
//! in a vesting row, and the reward is minted only once the row matures and moves — a conviction
//! first burns it. So a Final whose row lives is `vesting`, one whose row moved is `minted`, and only
//! a Final that never vested (below the fence) is `paid` (phase2-plan F10).
//!
//! `verifier status` shows what this bond judges as a seat. It says plainly that seats are not
//! paid: verifying is what turns the network's claims final, this operator's own included, and a
//! seat that dissents from the quorum is charged.

use crate::operator::finding::paint;
use crate::operator::profile::Profile;
use crate::operator::snapshot::Snapshot;
use crate::operator::status::group;

use crate::operator::work::{Lane, Outcome, VestingStage, WorkState};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::api::rpc::RpcApi;
use serde_json::json;
use std::time::Duration;

/// Amounts as the other operator screens print them: grouped, two decimals.
fn msk(sompi: u64) -> String {
    crate::operator::catalog::msk(sompi as u128)
}

/// The reward track, summed from the works and the wallet.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Rewards {
    /// Escrow of block-lane claims still being verified: paid only if they turn final.
    pub(crate) escrowed_sompi: u64,
    pub(crate) escrowed_claims: usize,
    /// Escrow queued for the next coinbase.
    pub(crate) queued_sompi: u64,
    pub(crate) queued_claims: usize,
    /// Escrow of final claims that has left the queue — paid (spendable or maturing in the wallet).
    /// Only a Final that did not vest: below ADR-0152's fence, final and paid are one event.
    pub(crate) paid_sompi: u64,
    pub(crate) paid_claims: usize,
    /// ADR-0152: this bond's legs of Finals whose reward still VESTS — named in a row, minted only
    /// once it matures and moves, burned if a conviction lands first. Not money yet.
    pub(crate) vesting_sompi: u64,
    pub(crate) vesting_claims: usize,
    /// ADR-0152: escrow of vested Finals whose row moved — a coinbase minted it (spendable or
    /// maturing in the wallet).
    pub(crate) minted_sompi: u64,
    pub(crate) minted_claims: usize,
    /// Escrow of voided claims: destroyed, never paid.
    pub(crate) forfeited_sompi: u64,
    pub(crate) forfeited_claims: usize,
    /// Prompt-lane quanta won and spent as receipt blocks, and the claims still waiting on a draw.
    pub(crate) quanta_spent: u64,
    pub(crate) prompt_final_claims: usize,
    pub(crate) prompt_pending_claims: usize,
}

/// Sum the works' reward track. Only rows the node served carry an escrow; a row read off the log
/// is counted in none of the sums (its escrow is unknown), and `known` says how many were.
pub(crate) fn sum(works: &[crate::operator::snapshot::WorkRow]) -> (Rewards, usize) {
    let mut r = Rewards::default();
    let mut known = 0;
    for w in works {
        let Some(extra) = &w.extra else { continue };
        known += 1;
        match (w.lane, w.reading.state.outcome(), w.reading.state) {
            (Lane::Block, Outcome::InFlight | Outcome::Paused, _) => {
                r.escrowed_sompi += extra.escrow_sompi;
                r.escrowed_claims += 1;
            }
            (Lane::Block, Outcome::Mined, _) => match (extra.payout_pending_sompi, extra.vesting) {
                (Some(q), _) => {
                    r.queued_sompi += q;
                    r.queued_claims += 1;
                }
                (None, Some(v)) if v.stage != VestingStage::Moved => {
                    r.vesting_sompi += v.payee_sompi;
                    r.vesting_claims += 1;
                }
                (None, Some(_)) => {
                    r.minted_sompi += extra.escrow_sompi;
                    r.minted_claims += 1;
                }
                (None, None) => {
                    r.paid_sompi += extra.escrow_sompi;
                    r.paid_claims += 1;
                }
            },
            (Lane::Block, Outcome::Failed, _) => {
                r.forfeited_sompi += extra.escrow_sompi;
                r.forfeited_claims += 1;
            }
            (Lane::Prompt, Outcome::Mined, state) => {
                r.quanta_spent += w.chain.as_ref().map_or(0, |c| c.quanta_spent as u64);
                if state == WorkState::RewardPending {
                    r.prompt_pending_claims += 1;
                } else {
                    r.prompt_final_claims += 1;
                }
            }
            _ => {}
        }
    }
    (r, known)
}

/// `misaka rewards`.
pub(crate) async fn rewards(ctx: &crate::node::Ctx, profile: Profile) -> CliResult {
    let snap = Snapshot::gather(profile, Duration::from_secs(ctx.timeout_secs.clamp(2, 10)), true).await;
    if let Err((url, why)) = &snap.node {
        return Err(CliError::new(exit::CONNECTION, format!("the node at {url} is not answering: {why}")));
    }
    let (r, known) = sum(&snap.works);
    let receipts = snap.node_status.as_ref().map(|s| s.receipt_blocks);
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "misaka.rewards.v1",
                "network": snap.profile.network,
                "wallet": snap.wallet.as_ref().and_then(|w| w.as_ref().ok()),
                "claims": r,
                "claims_with_escrow_known": known,
                "receipt_blocks_this_run": receipts,
                "works_source": snap.works_source,
            }))
            .expect("serializable")
        );
        return Ok(());
    }
    let pay = snap.wallet.as_ref().and_then(|w| w.as_ref().ok());
    println!(
        "{}",
        paint::bold(&format!(
            "REWARDS · {} · paid to {}",
            snap.profile.network,
            pay.map(|w| crate::operator::doctor::short_address(&w.address)).unwrap_or_else(|| "?".into())
        ))
    );
    let line = |label: &str, amount: String, claims: String, what: &str| {
        println!("  {label:<12}{amount:>20}  {claims:>7}  {}", paint::dim(what));
    };
    line("", "MSK".into(), "claims".into(), "");
    match &snap.wallet {
        Some(Ok(w)) => {
            line("spendable", msk(w.spendable_sompi), "—".into(), "mature outputs at the pay address");
            let next = w.next_mature_daa.map(|d| format!(" — next at DAA {}", group(d))).unwrap_or_default();
            line(
                "maturing",
                msk(w.maturing_sompi),
                w.maturing_outputs.to_string(),
                &format!("coinbase outputs not yet spendable{next}"),
            );
        }
        Some(Err(e)) => println!("  {}", paint::yellow(&format!("wallet not read: {e}"))),
        None => println!("  {}", paint::dim("no pay address known (set [mining] wallet, or name the key)")),
    }
    if snap.works_source == "node" {
        line("queued", msk(r.queued_sompi), r.queued_claims.to_string(), "escrow of final claims, in the next coinbase");
        line(
            "escrowed",
            msk(r.escrowed_sompi),
            r.escrowed_claims.to_string(),
            "claims still being verified — paid only if they turn final",
        );
        line(
            "forfeited",
            msk(r.forfeited_sompi),
            r.forfeited_claims.to_string(),
            "escrow of voided claims — a Final convicted while it vested included: destroyed, not paid",
        );
        if r.vesting_claims > 0 || r.minted_claims > 0 {
            line(
                "vesting",
                msk(r.vesting_sompi),
                r.vesting_claims.to_string(),
                "this bond's share of Finals still vesting: minted only once each row matures (misaka palw vesting)",
            );
            line("minted", msk(r.minted_sompi), r.minted_claims.to_string(), "escrow of vested Finals a coinbase minted (in the wallet above)");
        }
        line("paid", msk(r.paid_sompi), r.paid_claims.to_string(), "escrow of final claims that left the queue (in the wallet above)");
        if let Some(v) = snap.wallet.as_ref().and_then(|w| w.as_ref().ok()).and_then(|w| w.vesting.as_ref()) {
            if v.reporter_pending_sompi + v.reporter_awarded_sompi > 0 {
                line(
                    "reporter",
                    msk(v.reporter_pending_sompi + v.reporter_awarded_sompi),
                    "—".into(),
                    &format!(
                        "reporter rewards to the pay address: {} in their reveal window, {} awarded and moving first",
                        msk(v.reporter_pending_sompi),
                        msk(v.reporter_awarded_sompi)
                    ),
                );
            }
            if v.bond_held == Some(true) {
                println!(
                    "  {}",
                    paint::yellow("B-3: the bond's collateral stays locked while it is payee of a row the conviction window still holds")
                );
            }
            if v.halted {
                println!("  {}", paint::yellow("the chain is in a licence halt: no vesting row matures until an anchor settles"));
            }
        }
        let prompt = format!(
            "{} quanta spent as receipt blocks{}",
            r.quanta_spent,
            receipts.map(|n| format!(" · {n} receipt block(s) this run")).unwrap_or_default()
        );
        line("prompt lane", String::new(), (r.prompt_final_claims + r.prompt_pending_claims).to_string(), &prompt);
    } else {
        println!(
            "  {}",
            paint::dim("escrows are not listed: this node predates getPalwClaims (ADR-0122), so only the wallet is read")
        );
    }
    Ok(())
}

/// `misaka verifier status`: the claims this bond is seated on, and what it owes them.
pub(crate) async fn verifier_status(ctx: &crate::node::Ctx, profile: Profile) -> CliResult {
    let snap = Snapshot::gather(profile.clone(), Duration::from_secs(ctx.timeout_secs.clamp(2, 10)), false).await;
    let node = snap
        .node
        .as_ref()
        .map_err(|(url, why)| CliError::new(exit::CONNECTION, format!("the node at {url} is not answering: {why}")))?;
    let bond = profile.bond.clone().ok_or_else(|| {
        CliError::new(exit::CONFIG, "name the bond: --bond <txid>:<index> (or [advanced] bond, or run it on the node's host)")
    })?;
    if !node.ops_0122 {
        return Err(CliError::new(
            exit::COMPONENT_DOWN,
            format!(
                "the node at {} predates getPalwClaims (ADR-0122), so it cannot list seat duties — rebuild it from this tree",
                node.url
            ),
        ));
    }
    let duties = node.client().get_palw_claims(bond.clone(), "seat".into(), false, 200).await.map_err(|e| {
        CliError::new(exit::CONNECTION, format!("getPalwClaims: {e} (a node older than ADR-0122 does not list seat duties)"))
    })?;
    // **The receipt deadline a seat is held to is the chain's per-claim one** (`bound + W_r(c)`,
    // ADR-0133 §11.3), which the panel view carries; a claim row carries the global window's. Read
    // once for this bond's seats; a node that cannot answer leaves the rows' own.
    let per_claim_deadline: std::collections::HashMap<String, u64> = node
        .client()
        .get_palw_panel_assignments(kaspa_rpc_core::GetPalwPanelAssignmentsRequest { claim_id: String::new(), seat_id: bond.clone() })
        .await
        .map(|r| r.assignments.into_iter().map(|a| (a.claim_id, a.deadline_daa)).collect())
        .unwrap_or_default();
    let deadline_of = |c: &kaspa_rpc_core::RpcPalwClaimRow| match c.phase.as_str() {
        "panel_bound" => per_claim_deadline.get(&c.claim_id).copied().or(c.deadline_daa),
        _ => c.deadline_daa,
    };
    // What the bond is seated for: a bond judges only the classes it declared, and a registration
    // declares none.
    let base = match &node.nv.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => Some(b.base_class_id.to_string()),
        _ => None,
    };
    let judges: Vec<String> = duties
        .bond_capable_classes
        .iter()
        .map(|c| if Some(c) == base.as_ref() { "base (the floor)".to_string() } else { format!("{}…", &c[..8.min(c.len())]) })
        .collect();
    let rt = snap.node_status.as_ref();
    let now = node.daa();
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "misaka.verifier.status.v1",
                "bond": bond,
                "panel_running": rt.map(|r| r.panel_running),
                "submitter_funded": rt.map(|r| r.panel_submitter),
                "bond_known": duties.bond_known,
                "capable_classes": duties.bond_capable_classes,
                "tip_daa": duties.tip_daa,
                "duties": duties.claims.iter().map(|c| json!({
                    "claim_id": c.claim_id, "class_id": c.class_id, "executor_bond": c.executor_bond, "phase": c.phase,
                    "seat": c.seats.iter().position(|s| *s == bond).map(|i| i + 1), "seats": c.seats.len(), "deadline_daa": deadline_of(c),
                    "free_prompt": c.is_free_prompt,
                })).collect::<Vec<_>>(),
                "paid": false,
            }))
            .expect("serializable")
        );
        return Ok(());
    }
    let running = match rt {
        Some(r) if r.panel_running => {
            format!("panel running · submitter {}", if r.panel_submitter { "funded" } else { "off (receipts only)" })
        }
        Some(_) => "no panel runs on this node — it judges nothing".to_string(),
        None => "this node predates getPalwNodeStatus".to_string(),
    };
    let head = if duties.claims.is_empty() {
        format!("○ VERIFYING — no claim seats this bond right now · {running}")
    } else {
        format!("● VERIFYING — seated on {} claim(s) · {running}", duties.claims.len())
    };
    println!("{}", paint::bold(&head));
    println!("  bond {bond}");
    if !duties.bond_known {
        println!("  {}", paint::red("the registry holds no bond at this outpoint — it is seated for nothing"));
    } else if judges.is_empty() {
        println!(
            "  {} it declared no class, and a bond is drawn only for the classes it declared (misaka verifier setup declares them)",
            paint::yellow("judges nothing:")
        );
    } else {
        println!("  judges {}", judges.join(", "));
    }
    if !duties.claims.is_empty() {
        println!("{}", paint::dim(&format!("  {:<12}{:<12}{:<8}{:<18}{:<8}{}", "CLAIM", "CLASS", "SEAT", "PHASE", "LANE", "DUE")));
        for c in &duties.claims {
            let seat =
                c.seats.iter().position(|s| *s == bond).map(|i| format!("{}/{}", i + 1, c.seats.len())).unwrap_or_else(|| "?".into());
            let due = deadline_of(c)
                .map(|d| if d > now { format!("DAA {} (in {})", group(d), group(d - now)) } else { format!("DAA {}", group(d)) })
                .unwrap_or_default();
            println!(
                "  {}{:<12}{:<8}{:<18}{:<8}{due}",
                paint::cyan(&format!("{:<12}", &c.claim_id[..8.min(c.claim_id.len())])),
                format!("{}…", &c.class_id[..8.min(c.class_id.len())]),
                seat,
                c.phase,
                if c.is_free_prompt { "prompt" } else { "block" }
            );
        }
    }
    println!(
        "  {}  seats are not paid; verifying is what turns the network's claims — this operator's own included — final, and a seat",
        paint::bold("Pay")
    );
    println!("        that dissents from the quorum is charged, up to the minimum collateral");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::snapshot::WorkRow;
    use crate::operator::work::{ClaimExtra, Reading};

    fn row(lane: Lane, state: WorkState, extra: Option<ClaimExtra>) -> WorkRow {
        WorkRow {
            lane,
            claim_id: Some("ab".repeat(64)),
            job: None,
            block: None,
            seen_ts: None,
            chain: None,
            outbox: None,
            reading: Reading { state, detail: String::new(), deadline_daa: None, estimated: false },
            extra,
        }
    }

    /// The reward track splits the way the chain pays: in flight is escrowed, final is queued or
    /// paid, voided is forfeited — and a row whose escrow nobody served counts in none of them.
    #[test]
    fn the_reward_track_splits_the_way_the_chain_pays() {
        let e = |escrow, pending| Some(ClaimExtra { deadline_daa: None, escrow_sompi: escrow, payout_pending_sompi: pending, vesting: None });
        let works = vec![
            row(Lane::Block, WorkState::WaitingReceipts, e(100, None)),
            row(Lane::Block, WorkState::RewardPending, e(200, Some(200))),
            row(Lane::Block, WorkState::Rewarded, e(300, None)),
            row(Lane::Block, WorkState::Voided, e(400, None)),
            row(Lane::Block, WorkState::QuorumReached, None),
        ];
        let (r, known) = sum(&works);
        assert_eq!(known, 4, "the log-read row is not counted");
        assert_eq!((r.escrowed_sompi, r.queued_sompi, r.paid_sompi, r.forfeited_sompi), (100, 200, 300, 400));
        assert_eq!((r.escrowed_claims, r.queued_claims, r.paid_claims, r.forfeited_claims), (1, 1, 1, 1));
    }

    /// **T52: a Final whose row lives is `vesting`, not `paid`** (ADR-0152, phase2-plan F10) — its
    /// amount is the bond's own legs, not the escrow; a moved row is `minted`; a queued move is
    /// `queued`; and only a Final that never vested is `paid`.
    #[test]
    fn t52_a_final_with_a_live_row_is_vesting_not_paid() {
        use crate::operator::work::VestingExtra;
        let v = |stage, payee| VestingExtra {
            stage,
            payee_sompi: payee,
            expiry_daa: Some(12_000),
            licences_since_final: 0,
            licences_needed: Some(30),
            matured_at: None,
            eta_daa: Some(12_000),
            eta_estimated: true,
        };
        let e = |pending, vesting| Some(ClaimExtra { deadline_daa: None, escrow_sompi: 1_000, payout_pending_sompi: pending, vesting });
        let works = vec![
            row(Lane::Block, WorkState::RewardPending, e(None, Some(v(VestingStage::Maturing, 400)))),
            row(Lane::Block, WorkState::RewardPending, e(None, Some(v(VestingStage::Latched, 300)))),
            row(Lane::Block, WorkState::Rewarded, e(None, Some(v(VestingStage::Moved, 0)))),
            row(Lane::Block, WorkState::RewardPending, e(Some(250), Some(v(VestingStage::Moved, 0)))),
            row(Lane::Block, WorkState::Rewarded, e(None, None)),
        ];
        let (r, _) = sum(&works);
        assert_eq!((r.vesting_sompi, r.vesting_claims), (700, 2), "the bond's legs, not the escrow");
        assert_eq!((r.minted_sompi, r.minted_claims), (1_000, 1));
        assert_eq!((r.queued_sompi, r.queued_claims), (250, 1));
        assert_eq!((r.paid_sompi, r.paid_claims), (1_000, 1), "only the Final that never vested is paid");
    }
}
