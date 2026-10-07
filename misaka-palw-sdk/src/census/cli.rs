//! `palw-class census …` — the census's two offline steps (the HTTP side is `tools/hf_census/`).
//!
//! ```text
//! palw-class census classify [--snapshot ID] [--network testnet-12] [--height DAA] [--policy none|permissive-card-v0] [--tree SHA]
//!     < listing.jsonl > classified.jsonl          one ListingV1 per line in; one ClassifiedV1 (row at the listing depth + fetch plan) out
//! palw-class census gates [--snapshot ID] [--network …] [--height …] [--policy …] [--tree SHA] [--threads N] --dirs FILE
//!     > rows.jsonl                                 one fetched repository directory per line of FILE; one CensusRowV1 per line out
//! ```
//!
//! Every input line gives one output line, in order; a line that cannot be read is an error that names it (never a dropped repository).

use super::classify;
use super::gates::{CensusContext, evaluate};
use super::listing::ListingV1;
use super::rights::RightsPolicy;
use super::store::Fetched;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const USAGE: &str = "palw-class census classify|gates [--snapshot ID] [--network ID] [--height DAA] [--max-context N] [--policy none|permissive-card-v0] [--tree SHA] [--threads N] [--depth headers|shape] [--assume-task TASK] [--judge-budget-secs N] [--judge-cache FILE] [--dirs FILE [--follow]]";

fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    args.remove(i);
    Some(args.remove(i))
}

/// Run `palw-class census <args>`; `network` is the command line's `--network` (taken by the caller).
pub fn run(args: &[String], network: Option<String>) -> Result<(), String> {
    let mut args = args.to_vec();
    let sub = if args.is_empty() { return Err(USAGE.into()) } else { args.remove(0) };
    let snapshot = take(&mut args, "--snapshot").unwrap_or_else(|| "unnamed".into());
    let network = network.unwrap_or_else(|| "testnet-12".into());
    let height = match take(&mut args, "--height") {
        Some(h) => Some(h.parse::<u64>().map_err(|e| format!("--height {h}: {e}"))?),
        None => None,
    };
    let policy_s = take(&mut args, "--policy").unwrap_or_else(|| "none".into());
    let policy = RightsPolicy::parse(&policy_s).ok_or_else(|| format!("--policy {policy_s}: none | permissive-card-v0"))?;
    let tree = take(&mut args, "--tree").unwrap_or_else(|| "unstated".into());
    let threads: usize = take(&mut args, "--threads").map(|t| t.parse().unwrap_or(1)).unwrap_or(1).clamp(1, 16);
    let dirs = take(&mut args, "--dirs");
    let depth = match take(&mut args, "--depth").as_deref() {
        None | Some("shape") => crate::preflight::Depth::Shape,
        Some("headers") => crate::preflight::Depth::Headers,
        Some(other) => return Err(format!("--depth {other}: headers | shape")),
    };
    let assume_task = take(&mut args, "--assume-task");
    let judge_cache = take(&mut args, "--judge-cache");
    let judge_budget = match take(&mut args, "--judge-budget-secs") {
        Some(b) => Some(std::time::Duration::from_secs(b.parse::<u64>().map_err(|e| format!("--judge-budget-secs {b}: {e}"))?)),
        None => None,
    };
    let follow = args.iter().any(|a| a == "--follow");
    args.retain(|a| a != "--follow");
    let max_context = match take(&mut args, "--max-context") {
        Some(c) => Some(c.parse::<u32>().map_err(|e| format!("--max-context {c}: {e}"))?),
        None => None,
    };
    if !args.is_empty() {
        return Err(format!("unexpected arguments {args:?}\n{USAGE}"));
    }
    // A census declares every class at one context (the preflight's search for the widest admissible context runs admission many
    // times per class): `--max-context N` fixes it, else the model's declared maximum capped at 8,192 with one retry at 2,048.
    let mut ctx = CensusContext::new(&snapshot, &network, height, policy, &tree);
    ctx.options.depth = depth;
    ctx.assume_task = assume_task;
    if let Some(path) = &judge_cache {
        ctx.cache = crate::preflight::JudgeCache::with_file(std::path::Path::new(path))?;
        eprintln!("census: {} judgment(s) read from {path}", ctx.cache.len());
    }
    ctx.judge_budget = judge_budget;
    if let Some(b) = judge_budget {
        ctx.ruleset.context_rule = format!("{}; a layout search's budget {} s", ctx.ruleset.context_rule, b.as_secs());
    }
    if let Some(c) = max_context {
        ctx = ctx.with_context_rule(super::gates::ContextRule::Fixed(c));
    }
    // A network that cannot be judged on is an error before any line is read.
    crate::preflight::chain::PreflightNetwork::parse(&network)?;
    match sub.as_str() {
        "classify" => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            let mut unreadable = 0u64;
            for (n, line) in stdin.lock().lines().enumerate() {
                let line = line.map_err(|e| format!("stdin line {}: {e}", n + 1))?;
                if line.trim().is_empty() {
                    continue;
                }
                // A record that does not read is a result (the report counts it as a failure in D_all), never a dropped repository
                // and never the end of the run.
                match serde_json::from_str::<ListingV1>(&line) {
                    Ok(l) => {
                        let c = classify(&l, &ctx);
                        serde_json::to_writer(&mut out, &c).map_err(|e| e.to_string())?;
                    }
                    Err(e) => {
                        unreadable += 1;
                        let repo = serde_json::from_str::<serde_json::Value>(&line)
                            .ok()
                            .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(str::to_string));
                        eprintln!("stdin line {}: {e}", n + 1);
                        serde_json::to_writer(
                            &mut out,
                            &serde_json::json!({"error": format!("LISTING_UNREADABLE: {e}"), "line": n + 1, "repo": repo}),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                out.write_all(b"\n").map_err(|e| e.to_string())?;
            }
            out.flush().map_err(|e| e.to_string())?;
            if unreadable > 0 {
                eprintln!("census classify: {unreadable} listing record(s) did not read (written as LISTING_UNREADABLE rows)");
            }
            Ok(())
        }
        "gates" => {
            let file = dirs.ok_or("census gates needs --dirs FILE (one fetched repository directory per line)")?;
            let ctx = Arc::new(ctx);
            let read = |file: &str| -> Result<Vec<String>, String> {
                Ok(std::fs::read_to_string(file)
                    .map_err(|e| format!("{file}: {e}"))?
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect())
            };
            let mut done = 0usize;
            loop {
                let lines = read(&file)?;
                let ended = lines.last().is_some_and(|l| l == "END");
                let batch: Vec<PathBuf> = lines.iter().skip(done).filter(|l| l.as_str() != "END").map(PathBuf::from).collect();
                if !batch.is_empty() {
                    done += batch.len();
                    gate_batch(batch, &ctx, threads)?;
                }
                // `--follow`: the file grows while a fetch runs; it ends with a line `END`.
                if !follow || (ended && done + 1 >= lines.len()) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(20));
            }
            let (hits, misses) = ctx.cache.stats();
            eprintln!("census gates: {done} repositories; chain judgments {misses} computed, {hits} shared");
            Ok(())
        }
        other => Err(format!("census {other}: {USAGE}")),
    }
}

/// Judge one batch on `threads` workers, printing each row as soon as every row before it is printed (input order).
fn gate_batch(list: Vec<PathBuf>, ctx: &Arc<CensusContext>, threads: usize) -> Result<(), String> {
    let n = list.len();
    let results: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(vec![None; n]));
    let next = Arc::new(Mutex::new(0usize));
    let list = Arc::new(list);
    let stdout = Arc::new(Mutex::new((std::io::stdout(), 0usize)));
    let mut handles = Vec::new();
    for _ in 0..threads {
        let (results, next, ctx, list, stdout) = (results.clone(), next.clone(), ctx.clone(), list.clone(), stdout.clone());
        handles.push(
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn(move || -> Result<(), String> {
                    loop {
                        let i = {
                            let mut g = next.lock().map_err(|_| "poisoned")?;
                            if *g >= list.len() {
                                return Ok(());
                            }
                            *g += 1;
                            *g - 1
                        };
                        let dir = &list[i];
                        // A repository that cannot be judged is a row that says so, never the end of the run.
                        let line = gate_one(dir, &ctx).unwrap_or_else(|e| {
                            serde_json::json!({"error": format!("CENSUS_ROW_FAILED: {e}"), "dir": dir.display().to_string()})
                                .to_string()
                        });
                        results.lock().map_err(|_| "poisoned")?[i] = Some(line);
                        let mut s = stdout.lock().map_err(|_| "poisoned")?;
                        let r = results.lock().map_err(|_| "poisoned")?;
                        while s.1 < r.len() && r[s.1].is_some() {
                            let l = r[s.1].as_ref().expect("checked");
                            writeln!(s.0, "{l}").map_err(|e| e.to_string())?;
                            s.1 += 1;
                        }
                        s.0.flush().map_err(|e| e.to_string())?;
                    }
                })
                .map_err(|e| e.to_string())?,
        );
    }
    for h in handles {
        h.join().map_err(|_| "a census worker panicked".to_string())??;
    }
    Ok(())
}

fn gate_one(dir: &std::path::Path, ctx: &CensusContext) -> Result<String, String> {
    let t = std::fs::read_to_string(dir.join("listing.json")).map_err(|e| format!("listing.json: {e}"))?;
    let l: ListingV1 = serde_json::from_str(&t).map_err(|e| format!("listing.json: {e}"))?;
    let fx = Fetched::load(dir)?;
    let t0 = std::time::Instant::now();
    let mut row = serde_json::to_value(evaluate(&l, Some(&fx), ctx)).map_err(|e| e.to_string())?;
    row["elapsed_ms"] = serde_json::json!(t0.elapsed().as_millis() as u64);
    serde_json::to_string(&row).map_err(|e| e.to_string())
}
