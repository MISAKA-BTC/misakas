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

const USAGE: &str = "palw-class census classify|gates [--snapshot ID] [--network ID] [--height DAA] [--max-context N] [--policy none|permissive-card-v0] [--tree SHA] [--threads N] [--dirs FILE]";

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
    let max_context = match take(&mut args, "--max-context") {
        Some(c) => Some(c.parse::<u32>().map_err(|e| format!("--max-context {c}: {e}"))?),
        None => None,
    };
    if !args.is_empty() {
        return Err(format!("unexpected arguments {args:?}\n{USAGE}"));
    }
    let mut ctx = CensusContext::new(&snapshot, &network, height, policy, &tree);
    // A census judges every class at one published declared context (the preflight's search for the widest admissible context runs
    // admission many times per class); `None` keeps the preflight's search.
    ctx.options.max_context = max_context;
    ctx.ruleset.max_context = max_context;
    // A network that cannot be judged on is an error before any line is read.
    crate::preflight::chain::PreflightNetwork::parse(&network)?;
    match sub.as_str() {
        "classify" => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            for (n, line) in stdin.lock().lines().enumerate() {
                let line = line.map_err(|e| format!("stdin line {}: {e}", n + 1))?;
                if line.trim().is_empty() {
                    continue;
                }
                let l: ListingV1 = serde_json::from_str(&line).map_err(|e| format!("stdin line {}: {e}", n + 1))?;
                let c = classify(&l, &ctx);
                serde_json::to_writer(&mut out, &c).map_err(|e| e.to_string())?;
                out.write_all(b"\n").map_err(|e| e.to_string())?;
            }
            out.flush().map_err(|e| e.to_string())
        }
        "gates" => {
            let file = dirs.ok_or("census gates needs --dirs FILE (one fetched repository directory per line)")?;
            let list: Vec<PathBuf> = std::fs::read_to_string(&file)
                .map_err(|e| format!("{file}: {e}"))?
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| PathBuf::from(l.trim()))
                .collect();
            let n = list.len();
            let results: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(vec![None; n]));
            let next = Arc::new(Mutex::new(0usize));
            let ctx = Arc::new(ctx);
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
                                let line = gate_one(dir, &ctx).map_err(|e| format!("{}: {e}", dir.display()))?;
                                results.lock().map_err(|_| "poisoned")?[i] = Some(line);
                                // Print every finished row that is next in order.
                                let mut s = stdout.lock().map_err(|_| "poisoned")?;
                                let r = results.lock().map_err(|_| "poisoned")?;
                                while s.1 < r.len() && r[s.1].is_some() {
                                    let l = r[s.1].as_ref().expect("checked");
                                    writeln!(s.0, "{l}").map_err(|e| e.to_string())?;
                                    s.1 += 1;
                                }
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
        other => Err(format!("census {other}: {USAGE}")),
    }
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
