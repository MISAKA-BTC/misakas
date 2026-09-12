//! **`misaka work list | show <id> | why <id>`** — ADR-0122 Decision 2: every work this host made,
//! one work's path from the request (or the won draw) to the reward, and the reason a work stands
//! where it stands.
//!
//! An id is the claim id, resolved from any unique prefix of four hex or more, or a prompt-lane
//! job's `job:<hex>` before its claim exists. A full 128-hex id that is not one of this host's works
//! is still read from the chain: any claim can be shown, whoever made it.

use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::procs;
use crate::operator::snapshot::{self, Snapshot, WorkRow};
use crate::operator::status::{ago, counts, group};
use crate::operator::work::{self, Lane, Mark};
use crate::palw_claim::Windows;
use crate::{CliError, CliResult, OutputFormat, exit};
use serde_json::json;
use std::time::Duration;

fn timeout(ctx: &crate::node::Ctx) -> Duration {
    Duration::from_secs(ctx.timeout_secs.clamp(2, 10))
}

/// `misaka work list`.
pub(crate) async fn list(ctx: &crate::node::Ctx, profile: crate::operator::profile::Profile) -> CliResult {
    let snap = Snapshot::gather(profile, timeout(ctx), true).await;
    if let Err((url, why)) = &snap.node {
        return Err(CliError::new(exit::CONNECTION, format!("the node at {url} is not answering: {why}")));
    }
    let now = procs::now_unix() as i64;
    if ctx.output == OutputFormat::Json {
        let rows: Vec<serde_json::Value> = snap.works.iter().map(row_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "misaka.work.list.v1",
                "network": snap.profile.network,
                "counts": counts(&snap.works),
                "works": rows,
                "unfollowed": snap.work_errors,
            }))
            .expect("serializable")
        );
        return Ok(());
    }
    let c = counts(&snap.works);
    println!(
        "{}   computed {} · submitted {} · accepted {} · failed {}",
        paint::bold(&format!("WORK · {} · {}", snap.profile.network, snap.works.len())),
        c.computed,
        c.submitted,
        c.accepted,
        c.failed
    );
    if snap.works.is_empty() {
        println!("  {}", paint::dim("no work found: no produced block in the node's log, and no prompt-lane job in the outbox"));
    } else {
        println!("{}", paint::dim(&format!("  {:<12}{:<8}{:<19}{:<58}{}", "ID", "LANE", "STAGE", "DETAIL", "AGE")));
        for w in &snap.works {
            let age = w.seen_ts.map(|t| ago(now - t)).unwrap_or_default();
            let mut detail = w.reading.detail.clone();
            if detail.chars().count() > 56 {
                detail = detail.chars().take(55).collect::<String>() + "…";
            }
            println!(
                "  {}{:<8}{:<19}{detail:<58}{age}",
                paint::cyan(&format!("{:<12}", w.display_id())),
                w.lane.name(),
                w.reading.state.name()
            );
        }
    }
    for e in &snap.work_errors {
        println!("  {}", paint::dim(&format!("(could not follow {e})")));
    }
    Ok(())
}

fn row_json(w: &WorkRow) -> serde_json::Value {
    json!({
        "id": w.claim_id,
        "job": w.job,
        "lane": w.lane,
        "state": w.reading.state,
        "detail": w.reading.detail,
        "deadline_daa": w.reading.deadline_daa,
        "estimated": w.reading.estimated,
        "seen_unix": w.seen_ts,
        "block": w.block,
        "phase": w.chain.as_ref().filter(|c| c.found).map(|c| c.phase.clone()),
        "phase_daa": w.chain.as_ref().filter(|c| c.found).map(|c| c.phase_daa),
        "void_reason": w.chain.as_ref().filter(|c| !c.void_reason.is_empty()).map(|c| c.void_reason.clone()),
    })
}

/// Find the work `id` names among this host's works, or read it from the chain.
async fn find(snap: &Snapshot, id: &str) -> Result<WorkRow, CliError> {
    let claims: Vec<&str> = snap.works.iter().filter_map(|w| w.claim_id.as_deref()).collect();
    let jobs: Vec<&str> = snap.works.iter().filter_map(|w| w.job.as_deref()).map(|j| j.trim_start_matches("fp-job-")).collect();
    let by_job = id.trim().to_ascii_lowercase().starts_with("job:");
    let pool: Vec<&str> = if by_job { jobs.clone() } else { claims.iter().chain(jobs.iter()).copied().collect() };
    match work::resolve_prefix(id, pool) {
        Ok(hit) => {
            let w = snap
                .works
                .iter()
                .find(|w| w.claim_id.as_deref() == Some(hit) || w.job.as_deref().map(|j| j.trim_start_matches("fp-job-")) == Some(hit))
                .expect("resolved from these works");
            Ok(w.clone())
        }
        Err(many) if many.len() > 1 => Err(CliError::new(
            exit::GENERIC,
            format!("'{id}' names {} works: {}", many.len(), many.iter().map(|m| work::short_id(m)).collect::<Vec<_>>().join(", ")),
        )),
        Err(_) => {
            // Not one of ours. A full claim id is read from the chain all the same.
            let node = snap.node.as_ref().map_err(|(url, why)| CliError::new(exit::CONNECTION, format!("{url}: {why}")))?;
            let full = id.trim().trim_start_matches("job:");
            if full.len() != 128 {
                return Err(CliError::new(
                    exit::GENERIC,
                    format!(
                        "no work of this host starts with '{id}' (misaka work list shows them; a full 128-hex claim id reads any claim)"
                    ),
                ));
            }
            let chain = snapshot::ask_claim(node, full)
                .await
                .ok_or_else(|| CliError::new(exit::CONNECTION, format!("getPalwFreePromptClaim {full} did not answer")))?;
            if !chain.found {
                return Err(CliError::new(exit::GENERIC, format!("this chain holds no claim {}", work::short_id(full))));
            }
            let lane = if chain.is_free_prompt { Lane::Prompt } else { Lane::Block };
            let reading = work::classify(
                lane,
                None,
                None,
                Some(&chain),
                false,
                node.windows.as_ref(),
                node.nv.coinbase_spendable_after(),
                node.daa(),
            );
            Ok(WorkRow {
                lane,
                claim_id: Some(full.to_string()),
                job: None,
                block: None,
                seen_ts: None,
                chain: Some(chain),
                outbox: None,
                reading,
                extra: None,
            })
        }
    }
}

/// One work as JSON (`misaka.work.show.v1`): its row, its marked path, and the reading's meaning
/// and next step — what `work show --output json` prints and the dashboard serves.
pub(crate) async fn show_json(
    profile: crate::operator::profile::Profile,
    id: &str,
    timeout: Duration,
) -> Result<serde_json::Value, CliError> {
    let snap = Snapshot::gather(profile, timeout, true).await;
    let w = find(&snap, id).await?;
    let node = snap.node.as_ref().ok();
    let now = node.map(|n| n.daa()).unwrap_or(0);
    let windows = node.and_then(|n| n.windows);
    let why = explain(&w, windows.as_ref(), now);
    let path: Vec<serde_json::Value> =
        work::timeline(w.lane, w.reading.state, w.chain.as_ref().map(|c| c.void_reason.as_str()).filter(|r| !r.is_empty()))
            .into_iter()
            .map(|(s, m)| json!({ "stage": s, "mark": m }))
            .collect();
    let mut doc = row_json(&w);
    doc["schema"] = json!("misaka.work.show.v1");
    doc["path"] = json!(path);
    doc["meaning"] = json!(why.as_ref().map(|r| r.meaning.clone()));
    doc["next"] = json!(why.as_ref().map(|r| r.next.clone()));
    doc["tip_daa"] = json!(now);
    Ok(doc)
}

/// `misaka work show <id>`: the path, marked.
pub(crate) async fn show(ctx: &crate::node::Ctx, profile: crate::operator::profile::Profile, id: &str) -> CliResult {
    let snap = Snapshot::gather(profile, timeout(ctx), true).await;
    let w = find(&snap, id).await?;
    let node = snap.node.as_ref().ok();
    let now = node.map(|n| n.daa()).unwrap_or(0);
    let windows = node.and_then(|n| n.windows);
    let why = explain(&w, windows.as_ref(), now);
    if ctx.output == OutputFormat::Json {
        let path: Vec<serde_json::Value> =
            work::timeline(w.lane, w.reading.state, w.chain.as_ref().map(|c| c.void_reason.as_str()).filter(|r| !r.is_empty()))
                .into_iter()
                .map(|(s, m)| json!({ "stage": s, "mark": m }))
                .collect();
        let mut doc = row_json(&w);
        doc["schema"] = json!("misaka.work.show.v1");
        doc["path"] = json!(path);
        doc["meaning"] = json!(why.as_ref().map(|r| r.meaning.clone()));
        doc["next"] = json!(why.as_ref().map(|r| r.next.clone()));
        println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
        return Ok(());
    }
    let class = w.chain.as_ref().filter(|c| c.found).map(|c| format!(" · class {}…", work::short_id(&c.class_id))).unwrap_or_default();
    let bond = w
        .chain
        .as_ref()
        .filter(|c| c.found)
        .map(|c| match c.executor_bond.split_once(':') {
            Some((tx, i)) => format!(" · bond {}…:{i}", work::short_id(tx)),
            None => format!(" · bond {}", c.executor_bond),
        })
        .unwrap_or_default();
    println!("{}", paint::bold(&format!("WORK {} · {} lane{class}{bond}", w.display_id(), w.lane.name())));
    if let Some(id) = &w.claim_id {
        println!("  claim {}", paint::cyan(id));
    }
    if let Some(job) = &w.job {
        println!("  job   {job}");
    }
    if let Some(block) = &w.block {
        println!("  block {block}");
    }
    println!();
    let chain = w.chain.as_ref().filter(|c| c.found);
    for (stage, mark) in work::timeline(w.lane, w.reading.state, chain.map(|c| c.void_reason.as_str()).filter(|r| !r.is_empty())) {
        let symbol = match mark {
            Mark::Done => paint::green(mark.symbol()),
            Mark::Current => paint::yellow(mark.symbol()),
            Mark::End => paint::red(mark.symbol()),
            _ => paint::dim(mark.symbol()),
        };
        let at = match (stage, chain) {
            (work::WorkState::OnChain, Some(c)) => format!("DAA {}", group(c.accepted_daa)),
            (s, Some(c)) if s == w.reading.state => format!("DAA {}", group(c.phase_daa)),
            _ => String::new(),
        };
        let name = format!("{:<18}", stage.name());
        let name = if mark == Mark::Ahead || mark == Mark::NotReached { paint::dim(&name) } else { name };
        let detail = if stage == w.reading.state { w.reading.detail.clone() } else { String::new() };
        println!("  {symbol} {name} {at:<12} {detail}");
    }
    if w.reading.estimated {
        println!("  {}", paint::dim("(≈: an estimate — the claim read does not carry that date)"));
    }
    if let Some(r) = why {
        println!();
        println!("  {}  {}", paint::bold("NEXT"), r.next);
    }
    Ok(())
}

/// `misaka work why <id>`: the reason, in the five fields.
pub(crate) async fn why(ctx: &crate::node::Ctx, profile: crate::operator::profile::Profile, id: &str) -> CliResult {
    let snap = Snapshot::gather(profile, timeout(ctx), true).await;
    let w = find(&snap, id).await?;
    let node = snap.node.as_ref().ok();
    let now = node.map(|n| n.daa()).unwrap_or(0);
    let windows = node.and_then(|n| n.windows);
    let Some(r) = explain(&w, windows.as_ref(), now) else {
        return Err(CliError::new(exit::GENERIC, "nothing is known about this work beyond its files"));
    };
    let mut finding =
        Finding::warning("", format!("{} — {}", w.display_id(), w.reading.state.name())).reason(r.meaning.clone()).fix(r.next.clone());
    // A work on its way is not a fault: the mark says where it stands, not that something is wrong.
    finding.severity = match w.reading.state.outcome() {
        work::Outcome::Failed => Severity::Error,
        work::Outcome::Mined => Severity::Ok,
        work::Outcome::Paused => Severity::Warning,
        work::Outcome::InFlight | work::Outcome::Unknown => Severity::Info,
    };
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": "misaka.work.why.v1",
                "id": w.claim_id,
                "job": w.job,
                "state": w.reading.state,
                "phase": r.state,
                "meaning": r.meaning,
                "next": r.next,
            }))
            .expect("serializable")
        );
    } else {
        // The code slot is empty: a work's state is not a fault of this host's until the reading says so.
        let text = finding.render();
        println!("{}", text.replace("   []", ""));
    }
    Ok(())
}

/// `palw claim`'s reading of the chain (or of the outbox, before the chain), which carries the
/// meaning and the next step in the node's own words — with the block lane's own next step where
/// that reading speaks for the prompt lane: an attempt claim's seats replay its block's job
/// (ADR-0084 Decision 7) rather than fetch openings, and what this node owes it is an answer to an
/// accusation until it is final.
fn explain(w: &WorkRow, windows: Option<&Windows>, now: u64) -> Option<crate::palw_claim::Reading> {
    if let Some(chain) = w.chain.as_ref().filter(|c| c.found) {
        let mut r = crate::palw_claim::explain(chain, windows, now, w.outbox.as_ref().and_then(|o| o.submitted_txid.as_deref()));
        if w.lane == Lane::Block && matches!(chain.phase.as_str(), "provisional" | "panel_bound" | "receipt_licensed") {
            r.next = "nothing to do. Keep this node up with --palw-panel until the claim is final: its seats replay the block's \
                      job, and a court or an accusation about it asks THIS node — one it cannot answer is decided against it"
                .to_string();
        }
        return Some(r);
    }
    if let Some(row) = &w.outbox {
        let dir = row.job_file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        return crate::palw_claim::outbox_reading(row, &dir, now)
            .or_else(|| w.chain.as_ref().map(|c| crate::palw_claim::explain(c, windows, now, row.submitted_txid.as_deref())));
    }
    w.chain.as_ref().map(|c| crate::palw_claim::explain(c, windows, now, None))
}
