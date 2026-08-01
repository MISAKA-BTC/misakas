//! `misaka mtp …` — the self-serve half of the Testnet Points Program (ADR-0038 D3).
//!
//! Two operations, both trustless-by-design:
//!
//! * `points <id>` — a thin read-only HTTP client for the MTP service's query API
//!   (`GET /mtp/v1/points/<id>`). The service is a *mirror* of signed ledgers, so
//!   the numbers it returns are only as trustworthy as `verify-epoch` proves.
//! * `verify-epoch <file.jsonl>` — the trustless check: it re-verifies a published,
//!   ML-DSA-87-signed epoch ledger *locally*. With `--facts` it additionally runs
//!   the deterministic recompute and byte-compares, closing the loop the ADR's
//!   self-verification recipe describes (signature → rules-hash → recompute).
//!
//! No new dependencies: the HTTP client is a hand-rolled `TcpStream` GET (the same
//! secp-free, reqwest-free house style as `eth.rs`), and the verification reuses
//! the deterministic core (`misaka-mtp`).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use misaka_mtp::{Category, Contribution, EpochInput, EpochLedger, Rules, Severity, score_epoch};
use misaka_mtp_collectors::{ManualAward, append_manual_award};
use serde_json::{Value, json};

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat};

/// ML-DSA-87 verification-key length (2592 bytes = 5184 hex chars).
const MLDSA87_PK_LEN: usize = 2592;

// ---------------------------------------------------------------------------
// minimal HTTP/1.1 GET client (mirrors eth.rs's hand-rolled POST)
// ---------------------------------------------------------------------------

fn http_get(url: &str, timeout: Duration) -> Result<(u16, String), CliError> {
    // This client is the hand-rolled HTTP/1.1 GET below — it has no TLS, so an https:// endpoint is
    // a missing capability here, not a misconfiguration by the caller. Say which it is: the previous
    // wording ("must be http://") reads as "you typed the wrong scheme" and sends someone to edit a
    // config that was already correct.
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        CliError::generic(format!(
            "this CLI cannot fetch {url}: its MTP client speaks plain HTTP/1.1 only, with no TLS.\n\
             For an https:// endpoint, fetch the ledger with any HTTPS client and verify it locally — \
             verification is offline and is the part that actually proves anything:\n  \
             curl -sO <endpoint>/mtp/v1/points\n  \
             misaka mtp verify-epoch <epoch-N.M.jsonl> --pubkey-file <operator.pub>\n\
             Point --endpoint / MISAKA_MTP_ENDPOINT at an http:// instance (a local service, or a \
             tunnel to one) to use the query subcommands directly."
        ))
    })?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| CliError::generic(format!("bad MTP port in {url}")))?),
        None => (hostport, 80u16),
    };
    let sockaddr = (host, port)
        .to_socket_addrs()
        .map_err(|e| CliError::connection(format!("resolve {host}:{port}: {e}")))?
        .next()
        .ok_or_else(|| CliError::connection(format!("no address for {host}:{port}")))?;
    let mut stream = TcpStream::connect_timeout(&sockaddr, timeout)
        .map_err(|e| CliError::connection(format!("MTP connect {host}:{port}: {e} (is misaka-mtp-service serving?)")))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).map_err(|e| CliError::connection(format!("MTP write: {e}")))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| CliError::connection(format!("MTP read: {e}")))?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) =
        text.split_once("\r\n\r\n").ok_or_else(|| CliError::generic("malformed MTP HTTP response (no body)".to_string()))?;
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| CliError::generic("malformed MTP HTTP status line".to_string()))?;
    Ok((status, body.to_string()))
}

// ---------------------------------------------------------------------------
// `misaka mtp points <id>`
// ---------------------------------------------------------------------------

/// Look up an id's points via the service query API. In JSON mode the service's
/// response is passed through verbatim; in human mode a compact summary is shown
/// with a pointer to `verify-epoch` (the trust anchor).
pub fn points(ctx: &Ctx, id: &str, endpoint: &str) -> CliResult {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/mtp/v1/points/{id}");
    let (status, body) = http_get(&url, Duration::from_secs(ctx.timeout_secs))?;

    if status == 404 {
        return Err(CliError::generic(format!("no points found for id '{id}' (not registered, or no published epoch yet)")));
    }
    if status != 200 {
        return Err(CliError::generic(format!("MTP service returned HTTP {status}: {}", body.trim())));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| CliError::generic(format!("MTP response was not JSON: {e}")))?;

    match ctx.output {
        OutputFormat::Json => println!("{v}"),
        OutputFormat::Human => {
            let cum = &v["cumulative"];
            println!("id:         {id}");
            // C5 is shown with a marker: the points are measured and signed, but what they are
            // worth is undecided (`c5_token_settlement_enabled() == false`). Printing it as a plain
            // total alongside the others would imply an entitlement nobody has committed to.
            let c5 = cum["c5"].as_u64().unwrap_or(0);
            println!(
                "cumulative: C1 {}  C2 {}  C3 {}  C4 {}  C5 {}{}  (total {} mpts)",
                cum["c1"],
                cum["c2"],
                cum["c3"],
                cum["c4"],
                c5,
                if c5 > 0 { "*" } else { "" },
                cum["total"]
            );
            if let Some(epochs) = v["epochs"].as_array() {
                println!("epochs:     {}", epochs.len());
                for e in epochs {
                    let flag = if e["superseded"].as_bool().unwrap_or(false) { " (superseded issues exist)" } else { "" };
                    println!(
                        "  epoch {} [{}] issue {} — C1 {} C2 {} C3 {} C4 {} C5 {}  ← {}{}",
                        e["epoch"],
                        e["network"].as_str().unwrap_or("?"),
                        e["issue"],
                        e["c1"],
                        e["c2"],
                        e["c3"],
                        e["c4"],
                        e["c5"].as_u64().unwrap_or(0),
                        e["file"].as_str().unwrap_or("?"),
                        flag
                    );
                }
            }
            if c5 > 0 && !ctx.quiet {
                println!(
                    "\n* C5 (LLM mining) points are recorded and signed, but PROVISIONAL: no token \
                     distribution has been decided for them yet."
                );
            }
            if !ctx.quiet {
                println!("\nverify it yourself:  misaka mtp verify-epoch <the epoch-N.issue.jsonl> --pubkey <operator hex>");
                println!("(the signed ledger is the authority — this view is only a mirror of it.)");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `misaka mtp leaderboard` — all ids' cumulative points
// ---------------------------------------------------------------------------

/// Show the full points leaderboard (every id) via the service's `GET /mtp/v1/points`
/// (testnet-only). Read-only mirror of the signed ledgers; a row is only as trustworthy
/// as `verify-epoch` proves, so the human view points there.
pub fn leaderboard(ctx: &Ctx, endpoint: &str, top: usize) -> CliResult {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/mtp/v1/points");
    let (status, body) = http_get(&url, Duration::from_secs(ctx.timeout_secs))?;
    if status != 200 {
        return Err(CliError::generic(format!("MTP service returned HTTP {status}: {}", body.trim())));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| CliError::generic(format!("MTP response was not JSON: {e}")))?;

    match ctx.output {
        OutputFormat::Json => println!("{v}"),
        OutputFormat::Human => {
            let network = v["network"].as_str().unwrap_or("?");
            let participants = v["participants"].as_u64().unwrap_or(0);
            let epochs = v["epochs_counted"].as_u64().unwrap_or(0);
            println!("MTP leaderboard [{network}] — {participants} participant(s) over {epochs} epoch(s)");
            let empty = Vec::new();
            let entries = v["entries"].as_array().unwrap_or(&empty);
            if entries.is_empty() {
                println!("(no published epochs yet)");
                return Ok(());
            }
            println!(
                "{:>4}  {:<28} {:>13} {:>11} {:>11} {:>11} {:>11} {:>11}",
                "rank", "id", "total", "C1 node", "C2 bug", "C3 verify", "C4 infra", "C5 llm*"
            );
            let shown = if top == 0 { entries.len() } else { top.min(entries.len()) };
            for e in entries.iter().take(shown) {
                let c = &e["cumulative"];
                let rank = e["rank"].as_u64().unwrap_or(0);
                let id = e["id"].as_str().unwrap_or("?");
                let g = |k: &str| c[k].as_u64().unwrap_or(0);
                println!(
                    "{rank:>4}  {id:<28} {:>13} {:>11} {:>11} {:>11} {:>11} {:>11}",
                    g("total"),
                    g("c1"),
                    g("c2"),
                    g("c3"),
                    g("c4"),
                    g("c5")
                );
            }
            if top != 0 && entries.len() > shown {
                println!("… {} more (pass `--top 0` for the full board)", entries.len() - shown);
            }
            if !ctx.quiet {
                println!("\n(totals are milli-points; verify any id with:  misaka mtp verify-epoch <epoch.jsonl> --pubkey <hex>)");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `misaka mtp verify-epoch <file.jsonl>`
// ---------------------------------------------------------------------------

fn read_pubkey(pubkey: Option<&str>, pubkey_file: Option<&str>) -> Result<Vec<u8>, CliError> {
    let hex = match (pubkey, pubkey_file) {
        (Some(h), _) => h.trim().to_string(),
        (None, Some(path)) => std::fs::read_to_string(path)
            .map_err(|e| CliError::generic(format!("cannot read pubkey file '{path}': {e}")))?
            .trim()
            .to_string(),
        (None, None) => {
            return Err(CliError::generic("verify-epoch needs the operator pubkey: pass --pubkey <hex> or --pubkey-file <path>"));
        }
    };
    let mut bytes = vec![0u8; hex.len() / 2];
    faster_hex::hex_decode(hex.as_bytes(), &mut bytes)
        .map_err(|e| CliError::generic(format!("operator pubkey is not valid hex: {e}")))?;
    if bytes.len() != MLDSA87_PK_LEN {
        return Err(CliError::generic(format!(
            "operator pubkey must be {MLDSA87_PK_LEN} bytes ({} hex chars); got {}",
            MLDSA87_PK_LEN * 2,
            bytes.len()
        )));
    }
    Ok(bytes)
}

/// Verify a published, signed epoch ledger locally (ADR-0038 D3 self-verification).
///
/// Runs the recipe in order and stops at the first failure:
///  1. **signature** — `EpochLedger::verify(pubkey)` against the operator key;
///  2. **rules-hash** — the ledger's pinned `rules_hash` matches the current
///     [`Rules`] document (so the scores were computed under the published rules);
///  3. **recompute** (only with `--facts`) — feed the published `EpochInput`
///     through `score_epoch` and byte-compare the resulting ledger scores/hashes.
pub fn verify_epoch(
    output: OutputFormat,
    file: &str,
    pubkey: Option<&str>,
    pubkey_file: Option<&str>,
    facts: Option<&str>,
) -> CliResult {
    let text = std::fs::read_to_string(file).map_err(|e| CliError::generic(format!("cannot read ledger '{file}': {e}")))?;
    let ledger: EpochLedger = serde_json::from_str(text.trim())
        .map_err(|e| CliError::generic(format!("'{file}' is not a valid epoch ledger JSONL: {e}")))?;
    let pk = read_pubkey(pubkey, pubkey_file)?;

    // 1) signature.
    let sig_ok = ledger.verify(&pk);
    // 2) rules-hash (the current v1 rules; a future rules-version bump ships its doc).
    let want_rules = faster_hex::hex_string(&Rules::default().rules_hash().as_bytes());
    let rules_ok = ledger.rules_hash == want_rules;

    // 3) optional full recompute from published facts.
    let recompute = match facts {
        Some(path) => {
            let ftext = std::fs::read_to_string(path).map_err(|e| CliError::generic(format!("cannot read facts '{path}': {e}")))?;
            let input: EpochInput = serde_json::from_str(ftext.trim())
                .map_err(|e| CliError::generic(format!("'{path}' is not a valid EpochInput JSON: {e}")))?;
            let mut recomputed = score_epoch(&input, &Rules::default());
            // score_epoch produces an unsigned ledger; compare the signable content.
            recomputed.sig_mldsa87 = None;
            let mut published = ledger.clone();
            published.sig_mldsa87 = None;
            Some(recomputed == published)
        }
        None => None,
    };

    let all_ok = sig_ok && rules_ok && recompute.unwrap_or(true);

    match output {
        OutputFormat::Json => {
            println!(
                "{}",
                json!({
                    "ok": all_ok,
                    "epoch": ledger.epoch,
                    "network": ledger.network,
                    "signature_valid": sig_ok,
                    "rules_hash_matches": rules_ok,
                    "recompute_matches": recompute,
                    "rules_hash": ledger.rules_hash,
                    "inputs_hash": ledger.inputs_hash,
                    "score_rows": ledger.scores.len(),
                })
            );
        }
        OutputFormat::Human => {
            println!("epoch {} [{}] — {} score rows", ledger.epoch, ledger.network, ledger.scores.len());
            println!("  signature (ML-DSA-87):  {}", if sig_ok { "VALID" } else { "INVALID" });
            // The version is read from the constant, not spelled into the string: this line said
            // "matches v1" for every ledger, so after RULES_VERSION went to 2 it reported v1 while
            // checking v2 rules — the one number an operator reads to tell rule sets apart.
            println!(
                "  rules-hash matches v{}: {}",
                misaka_mtp::rules::RULES_VERSION,
                if rules_ok { "yes" } else { "NO (different rules version?)" }
            );
            match recompute {
                Some(true) => println!("  recompute byte-compare: MATCH"),
                Some(false) => println!("  recompute byte-compare: MISMATCH"),
                None => println!("  recompute byte-compare: skipped (pass --facts <EpochInput.json> to run it)"),
            }
            println!("  rules_hash:  {}", ledger.rules_hash);
            println!("  inputs_hash: {}", ledger.inputs_hash);
            println!("\n{}", if all_ok { "OK — this ledger is authentic." } else { "FAILED — do not trust this ledger." });
        }
    }

    if all_ok { Ok(()) } else { Err(CliError::generic("epoch ledger verification failed")) }
}

// ---------------------------------------------------------------------------
// `misaka mtp award …` — manually add a verification-required contribution
// ---------------------------------------------------------------------------

/// Record one hand-curated award for a **verification-required** category — C2 bug,
/// C3 verify, or C4 infra — that the auto pipeline deliberately does NOT collect (those
/// need human review). The award is appended to a local manual-awards JSONL; at epoch
/// time the service (`misaka-mtp-service run-epoch`) loads the awards for that
/// `(epoch, network)` and merges them into the scored, signed ledger alongside the auto
/// (node/validator/chain-fixed) facts. This is the operator's "add points by hand after
/// our own verification" path.
#[allow(clippy::too_many_arguments)]
pub fn award(
    ctx: &Ctx,
    file: &str,
    epoch: u64,
    network: &str,
    id: &str,
    category: &str,
    points: Option<u64>,
    severity: Option<&str>,
    first_report: bool,
    fix_accepted: bool,
    note: &str,
) -> CliResult {
    let contribution = match category.to_ascii_lowercase().as_str() {
        "bug" | "c2" => {
            let sev =
                severity.ok_or_else(|| CliError::generic("`--category bug` needs `--severity S0|S1|S2|S3` (the triaged severity)"))?;
            let severity = match sev.to_ascii_uppercase().as_str() {
                "S0" => Severity::S0,
                "S1" => Severity::S1,
                "S2" => Severity::S2,
                "S3" => Severity::S3,
                other => return Err(CliError::generic(format!("unknown severity '{other}' (expected S0|S1|S2|S3)"))),
            };
            Contribution::Bug { severity, first_report, fix_pr_accepted: fix_accepted }
        }
        "verify" | "c3" => {
            let pts = points.ok_or_else(|| CliError::generic("`--category verify` needs `--points <N>` (the reviewed award)"))?;
            Contribution::Fixed { category: Category::Verify, base_points: pts }
        }
        "infra" | "c4" => {
            let pts = points.ok_or_else(|| CliError::generic("`--category infra` needs `--points <N>` (the reviewed award)"))?;
            Contribution::Fixed { category: Category::Infra, base_points: pts }
        }
        "node" | "c1" => {
            return Err(CliError::generic(
                "category 'node' (C1) is auto-collected from uptime / validator / chain-fixed facts and cannot be awarded by hand",
            ));
        }
        other => return Err(CliError::generic(format!("unknown category '{other}' (expected bug | verify | infra)"))),
    };

    let award = ManualAward { epoch, network: network.to_string(), id: id.to_string(), contribution, note: note.to_string() };
    append_manual_award(file, &award).map_err(|e| CliError::generic(format!("cannot append to manual-awards '{file}': {e}")))?;

    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&award).map_err(|e| CliError::generic(format!("award JSON: {e}")))?)
        }
        OutputFormat::Human => {
            println!("recorded manual award → {file}");
            println!("  epoch {epoch} [{network}]  id {id}  category {category}");
            match &award.contribution {
                Contribution::Bug { severity, first_report, fix_pr_accepted } => {
                    println!("  bug: severity {severity:?}, first_report {first_report}, fix_accepted {fix_pr_accepted}")
                }
                Contribution::Fixed { category, base_points } => println!("  fixed: {category:?}, {base_points} base points"),
                _ => {}
            }
            if !note.is_empty() {
                println!("  note: {note}");
            }
            if !ctx.quiet {
                println!("\nit will be merged into the next `run-epoch` for this (epoch, network) and appear in the signed ledger.");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// register — produce a signed registration request (ADR-0038 D3 preserved)
// ---------------------------------------------------------------------------

/// `misaka mtp register` — sign the operator-issued invitation and emit the request to submit.
///
/// This is the participant half of registration, and it is deliberately **offline**. ADR-0038 D3
/// fixes the service's HTTP surface as read-only, so there is no endpoint to POST a registration
/// to and none is added here: the operator issues an invitation out-of-band
/// (`misaka-mtp-service issue-nonce`), this command signs it, and the participant submits the
/// resulting JSON through whatever channel the operator runs — a pull request, a form. The
/// operator ingests it with `misaka-mtp-service register`.
///
/// What is signed is the canonical challenge from `misaka_mtp::registry`, the same function the
/// operator's verifier calls, so the two cannot drift apart.
pub fn register(ctx: &Ctx, invitation_file: &str, key_file: &str, out: Option<&str>) -> CliResult {
    let raw = std::fs::read_to_string(invitation_file)
        .map_err(|e| CliError::generic(format!("cannot read invitation '{invitation_file}': {e}")))?;
    let inv: Value = serde_json::from_str(&raw).map_err(|e| CliError::generic(format!("invitation is not JSON: {e}")))?;

    let field = |k: &str| -> Result<String, CliError> {
        inv.get(k)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| CliError::generic(format!("invitation is missing the string field '{k}'")))
    };
    let network = field("network")?;
    let github = field("github")?;
    let address = field("address")?;
    let nonce = field("nonce")?;
    let issued_at_ms = inv
        .get("issued_at_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| CliError::generic("invitation is missing the numeric field 'issued_at_ms'"))?;

    let key = crate::keys::KeySource { key_file: Some(key_file.to_owned()), key_stdin: false }.load_key()?;

    // The invitation names an address; refuse if this key is not it. Signing anyway would produce a
    // request the operator can only reject, after the single-use nonce is already burned.
    let prefix = crate::prefix_of(&network)?;
    let derived = key.funding_address(prefix).to_string();
    if derived != address {
        return Err(CliError::generic(format!(
            "this key does not own the invited address.\n  invitation: {address}\n  this key:   {derived}\n\
             Ask the operator to re-issue the invitation for the address you actually hold — the nonce is \
             single-use, so signing with the wrong key would burn it."
        )));
    }

    let challenge = misaka_mtp::registry::registration_challenge(&network, &github, &address, &nonce, issued_at_ms);
    let signature = key.sign_with_context(&challenge, misaka_mtp::MTP_REGISTER_CONTEXT);

    let request = json!({
        "network": network,
        "github": github,
        "address": address,
        "nonce": nonce,
        "issued_at_ms": issued_at_ms,
        "pubkey_hex": faster_hex::hex_string(key.public_key()),
        "signature_hex": faster_hex::hex_string(&signature),
    });
    let body = serde_json::to_string_pretty(&request).map_err(|e| CliError::generic(format!("request JSON: {e}")))?;

    match out {
        Some(path) => {
            std::fs::write(path, format!("{body}\n")).map_err(|e| CliError::generic(format!("cannot write '{path}': {e}")))?
        }
        None => println!("{body}"),
    }

    if matches!(ctx.output, OutputFormat::Human) && out.is_some() {
        let path = out.unwrap_or_default();
        println!("signed registration request → {path}");
        println!("  github {github}  address {address}  [{network}]");
        println!(
            "\nSubmit this file to the operator (pull request / form). Nothing was sent anywhere: the MTP \
             HTTP surface is read-only by design (ADR-0038 D3), so registration is ingested operator-side."
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// collect — a vantage's uptime observations, from the node this host already runs
// ---------------------------------------------------------------------------

/// `misaka mtp collect` — record what THIS host's node currently sees, as one JSONL line per peer.
///
/// This is the C1 fact source, and it deliberately writes a file rather than into the fact store:
/// the store lives in `misaka-mtp-service`, which is kept dependency-light, while the wRPC client
/// lives here. Emitting JSONL for the operator to ingest matches how `manual-awards.jsonl` and
/// `registrations.jsonl` already cross that boundary, and it means a vantage host does not need the
/// service — or its data dir — present at all.
///
/// No new protocol: a crawler asking its own node "who are you connected to, and are they in IBD"
/// is `getConnectedPeerInfo`, which already carries everything a `NodeRecord`/`UptimeSample` needs —
/// a stable peer id for `node_key`, `is_ibd_peer` for the at-sync-required bit (reachable but
/// desynced does NOT count as up), the user agent for the version bonus, and the address for the
/// co-location key.
///
/// Attribution is NOT decided here. Peers are recorded by `node_key`; mapping those to a ledger id
/// is the operator's roster step, because a peer cannot yet assert ownership on the wire.
pub async fn collect(ctx: &Ctx, vantage: &str, out: Option<&str>) -> CliResult {
    use kaspa_rpc_core::api::rpc::RpcApi;

    if vantage.trim().is_empty() {
        return Err(CliError::generic("--vantage must name this observation point (e.g. JP, DE) — it is the evidence link"));
    }
    let hostport = match &ctx.rpc {
        Some(hp) => hp.clone(),
        None => "127.0.0.1:27210".to_string(),
    };
    let timeout = Duration::from_secs(ctx.timeout_secs);
    let client = crate::node::try_connect(&format!("ws://{hostport}"), timeout)
        .await
        .map_err(|e| CliError::connection(format!("cannot reach the node at {hostport}: {e}")))?;

    let info = client.get_server_info().await.map_err(|e| CliError::connection(format!("getServerInfo failed: {e}")))?;
    let observed_network = info.network_id.to_string();
    if observed_network != ctx.network {
        let _ = client.disconnect().await;
        return Err(CliError::generic(format!(
            "this node is on '{observed_network}' but --network says '{}'. Samples are scoped per network, \
             so collecting here would file observations under the wrong one.",
            ctx.network
        )));
    }

    let peers = client
        .get_connected_peer_info_call(None, kaspa_rpc_core::GetConnectedPeerInfoRequest {})
        .await
        .map_err(|e| CliError::connection(format!("getConnectedPeerInfo failed: {e}")))?;
    let _ = client.disconnect().await;

    let at_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);

    let mut lines = Vec::new();
    for p in &peers.peer_info {
        let addr = p.address.to_string();
        lines.push(
            json!({
                "network": observed_network,
                "vantage": vantage,
                "at_ms": at_ms,
                "node_key": p.id.to_string(),
                "address": addr,
                // Reachable AND not still downloading the chain. A peer in IBD is up but not usable,
                // which is exactly the distinction the uptime rule draws.
                "in_sync": !p.is_ibd_peer,
                "user_agent": p.user_agent,
                "advertised_protocol_version": p.advertised_protocol_version,
                "time_connected_ms": p.time_connected,
                "last_ping_ms": p.last_ping_duration,
                "is_outbound": p.is_outbound,
            })
            .to_string(),
        );
    }

    let body = if lines.is_empty() { String::new() } else { format!("{}\n", lines.join("\n")) };
    match out {
        Some(path) => {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| CliError::generic(format!("cannot open '{path}': {e}")))?;
            f.write_all(body.as_bytes()).map_err(|e| CliError::generic(format!("cannot append '{path}': {e}")))?;
        }
        None => print!("{body}"),
    }

    if matches!(ctx.output, OutputFormat::Human) && !ctx.quiet {
        let in_sync = peers.peer_info.iter().filter(|p| !p.is_ibd_peer).count();
        eprintln!(
            "vantage {vantage} [{observed_network}]: {} peer(s) observed, {in_sync} in sync{}",
            peers.peer_info.len(),
            match out {
                Some(p) => format!(" → appended to {p}"),
                None => String::new(),
            }
        );
        if peers.peer_info.is_empty() {
            eprintln!("no peers: this vantage saw nothing, which is a real observation — not an error.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// validators — the bonded validator roster and its slash state, from the chain
// ---------------------------------------------------------------------------

/// `misaka mtp validators` — emit one JSONL line per stake bond: who is bonded, for how much, and
/// whether the bond is active, unbonding or **slashed**.
///
/// This is the honest subset of the attestation picture. `GetStakeBonds` is registry state on the
/// selected chain, so the roster and the slash flag are chain-derived facts. What it does NOT carry
/// is per-epoch participation: no RPC on this node reports which validator signed which epoch.
/// `getValidatorStatus` is the local node's own self-report, and
/// `getAttestationQualityDeficits` aggregates stake per epoch without attributing it. Filling
/// `attested` per validator per epoch means indexing attestation transactions out of blocks — a
/// separate job. Nothing here guesses it, and nothing here writes an `attested` field.
///
/// An empty result is ambiguous by construction and is reported as such rather than as "no
/// validators": a network with PALW inert (testnet-10) legitimately has no bonds, and the `RpcApi`
/// trait's default `get_stake_bonds_call` also returns an empty page for any impl that does not
/// override it.
pub async fn validators(ctx: &Ctx, out: Option<&str>) -> CliResult {
    use kaspa_rpc_core::api::rpc::RpcApi;

    let hostport = match &ctx.rpc {
        Some(hp) => hp.clone(),
        None => "127.0.0.1:27210".to_string(),
    };
    let timeout = Duration::from_secs(ctx.timeout_secs);
    let client = crate::node::try_connect(&format!("ws://{hostport}"), timeout)
        .await
        .map_err(|e| CliError::connection(format!("cannot reach the node at {hostport}: {e}")))?;

    let info = client.get_server_info().await.map_err(|e| CliError::connection(format!("getServerInfo failed: {e}")))?;
    let observed_network = info.network_id.to_string();
    if observed_network != ctx.network {
        let _ = client.disconnect().await;
        return Err(CliError::generic(format!(
            "this node is on '{observed_network}' but --network says '{}'. Bond state is per network, \
             so reading here would file the roster under the wrong one.",
            ctx.network
        )));
    }

    // Page to exhaustion. Stopping at the first page would silently truncate the roster, and a
    // truncated roster reads as "these are all the validators" — the one wrong answer to avoid.
    let mut lines = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0usize;
    // Pinned to page 1's value, per the request doc: without it a bond whose status changes
    // mid-walk can be skipped, and a roster that silently drops a validator is worse than none.
    let mut pinned_pov: Option<u64> = None;
    loop {
        let page = client
            .get_stake_bonds_call(
                None,
                kaspa_rpc_core::GetStakeBondsRequest {
                    owner_pubkey_hash: None,
                    status_in: None,
                    cursor,
                    limit: 0, // server default
                    pov_daa_score: pinned_pov,
                },
            )
            .await
            .map_err(|e| CliError::connection(format!("getStakeBonds failed: {e}")))?;
        pages += 1;
        pinned_pov.get_or_insert(page.pov_daa_score);
        for b in &page.bonds {
            lines.push(
                json!({
                    "network": observed_network,
                    "validator_id": b.validator_id,
                    "bond_outpoint": b.bond_outpoint,
                    "owner_pubkey_hash": b.owner_pubkey_hash,
                    "amount": b.amount,
                    "activation_daa_score": b.activation_daa_score,
                    // Both statuses, because they can disagree: `stored` is what was written,
                    // `effective` is what holds at the sink. Collapsing them would hide a bond that
                    // is stored active but no longer effective.
                    "stored_status": b.stored_status,
                    "effective_status": b.effective_status,
                    "slashed": b.stored_status == "slashed" || b.effective_status == "slashed",
                    "unbond_request_daa_score": b.unbond_request_daa_score,
                    "unbonding_period_blocks": b.unbonding_period_blocks,
                    // The height this snapshot is true at — a roster with no point of view cannot
                    // be compared against a later one.
                    "pov_daa_score": page.pov_daa_score,
                })
                .to_string(),
            );
        }
        match page.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }
    let _ = client.disconnect().await;
    let pov_daa_score = pinned_pov.unwrap_or(0);

    if lines.is_empty() {
        eprintln!(
            "no stake bonds on {observed_network} at daa {pov_daa_score} ({pages} page(s)). This is the \
             expected reading where PALW is inert; it is NOT evidence that a PALW-active network has no validators."
        );
    } else {
        let slashed = lines.iter().filter(|l| l.contains("\"slashed\":true")).count();
        eprintln!("{} bond(s) on {observed_network} at daa {pov_daa_score}, {slashed} slashed", lines.len());
    }

    let body = if lines.is_empty() { String::new() } else { format!("{}\n", lines.join("\n")) };
    match out {
        Some(path) => {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| CliError::generic(format!("cannot open '{path}': {e}")))?;
            f.write_all(body.as_bytes()).map_err(|e| CliError::generic(format!("cannot append '{path}': {e}")))?;
        }
        None => print!("{body}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// attestations — per-validator, per-epoch participation, indexed out of blocks
// ---------------------------------------------------------------------------

/// `misaka mtp attestations` — walk blocks and emit one JSONL row per validator attestation.
///
/// This is the piece no RPC provides. `getValidatorStatus` is a node's self-report and
/// `getAttestationQualityDeficits` aggregates stake per epoch without naming anyone, so
/// "did validator V attest in epoch E" only exists inside blocks: a transaction on subnetwork
/// `0x11` (`SUBNETWORK_ID_STAKE_ATTESTATION_SHARD`) whose payload borsh-decodes to a
/// `StakeAttestationShardPayload` carrying `StakeAttestation`s, each naming a `validator_id`, an
/// `epoch` and the selected-chain anchor it approves.
///
/// Two honesty bounds this walk cannot escape, both reported rather than hidden:
///
/// - **A pruned node has no blocks below its pruning point.** Absence of an attestation before that
///   height means "not retained here", never "did not attest". The starting height is printed.
/// - **This records what was committed, not whether it was counted.** A decoded attestation is
///   evidence the shard reached a block; whether consensus admitted it toward finality is a
///   separate question this command does not answer.
///
/// `slashed` is deliberately not a column here — bond status is registry state, read with
/// `misaka mtp validators`, and duplicating it from a block walk would invite the two to disagree.
pub async fn attestations(ctx: &Ctx, low_hash: Option<&str>, max_blocks: usize, out: Option<&str>) -> CliResult {
    use kaspa_consensus_core::dns_finality::StakeAttestationShardPayload;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
    use kaspa_rpc_core::api::rpc::RpcApi;

    let hostport = match &ctx.rpc {
        Some(hp) => hp.clone(),
        None => "127.0.0.1:27210".to_string(),
    };
    let timeout = Duration::from_secs(ctx.timeout_secs);
    let client = crate::node::try_connect(&format!("ws://{hostport}"), timeout)
        .await
        .map_err(|e| CliError::connection(format!("cannot reach the node at {hostport}: {e}")))?;

    let info = client.get_server_info().await.map_err(|e| CliError::connection(format!("getServerInfo failed: {e}")))?;
    let observed_network = info.network_id.to_string();
    if observed_network != ctx.network {
        let _ = client.disconnect().await;
        return Err(CliError::generic(format!(
            "this node is on '{observed_network}' but --network says '{}'. Attestations are per network, \
             so indexing here would file them under the wrong one.",
            ctx.network
        )));
    }

    // Default to the pruning point: the oldest height this node can actually answer for. Starting
    // lower would silently return nothing and read as "no attestations".
    let start = match low_hash {
        Some(h) => {
            h.parse::<kaspa_rpc_core::RpcHash>().map_err(|e| CliError::generic(format!("--low-hash is not a block hash: {e}")))?
        }
        None => {
            let dag = client.get_block_dag_info().await.map_err(|e| CliError::connection(format!("getBlockDagInfo failed: {e}")))?;
            dag.pruning_point_hash
        }
    };

    let mut lines = Vec::new();
    let mut cursor = start;
    let mut scanned = 0usize;
    let mut shards = 0usize;
    let mut undecodable = 0usize;
    loop {
        let batch =
            client.get_blocks(Some(cursor), true, true).await.map_err(|e| CliError::connection(format!("getBlocks failed: {e}")))?;
        // The low hash is echoed back as the first element, so a batch of one means the walk is
        // standing still — the termination condition, not an error.
        if batch.blocks.len() <= 1 {
            break;
        }
        for b in &batch.blocks {
            scanned += 1;
            for tx in &b.transactions {
                if tx.subnetwork_id != SUBNETWORK_ID_STAKE_ATTESTATION_SHARD {
                    continue;
                }
                shards += 1;
                let payload: StakeAttestationShardPayload = match borsh::from_slice(&tx.payload) {
                    Ok(p) => p,
                    Err(_) => {
                        // A shard tx that will not decode is itself a finding — counted and
                        // reported, never skipped in silence.
                        undecodable += 1;
                        continue;
                    }
                };
                for att in &payload.attestations {
                    lines.push(
                        json!({
                            "network": observed_network,
                            "validator_id": att.validator_id.to_string(),
                            "att_epoch": att.epoch,
                            // The containing block's header time. The fact store windows facts by
                            // wall-clock milliseconds, so an epoch NUMBER here would land the fact
                            // in 1970 and drop it from every real window without an error.
                            "at_ms": b.header.timestamp,
                            // Present in a block == committed. Every row this walk emits is an
                            // observed attestation; a validator that did not attest produces no
                            // row at all, which is why absence must be read against the scan range.
                            "attested": true,
                            "target_hash": att.target_hash.to_string(),
                            "target_daa_score": att.target_daa_score,
                            "bond_outpoint": format!("{}:{}", att.bond_outpoint.transaction_id, att.bond_outpoint.index),
                            "validator_set_commitment": att.validator_set_commitment.to_string(),
                            "evidence_block": b.verbose_data.as_ref().map(|v| v.hash.to_string()),
                            "evidence_tx": tx.verbose_data.as_ref().map(|v| v.transaction_id.to_string()),
                        })
                        .to_string(),
                    );
                }
            }
        }
        if scanned >= max_blocks {
            eprintln!("stopped at the --max-blocks bound of {max_blocks}; the range scanned is NOT the whole chain");
            break;
        }
        let next = match batch.blocks.last().and_then(|b| b.verbose_data.as_ref()).map(|v| v.hash) {
            Some(h) => h,
            None => break,
        };
        if next == cursor {
            break;
        }
        cursor = next;
    }
    let _ = client.disconnect().await;

    let field = |l: &str, k: &str| -> Option<String> {
        l.split(&format!("\"{k}\":")).nth(1).map(|s| s.trim_start_matches('"').split(['"', ',']).next().unwrap_or("").to_string())
    };
    let validators: std::collections::BTreeSet<String> = lines.iter().filter_map(|l| field(l, "validator_id")).collect();
    // A DAG includes the same transaction in more than one block, so one attestation can surface
    // several times. Both numbers are printed because the raw row count is NOT a participation
    // count — on the first live run it was 407 rows for 186 distinct (validator, epoch) pairs, and
    // a consumer that summed rows would have over-credited every validator by ~2.2x.
    let distinct: std::collections::BTreeSet<(String, String)> =
        lines.iter().filter_map(|l| Some((field(l, "validator_id")?, field(l, "att_epoch")?))).collect();
    eprintln!(
        "{} row(s) = {} distinct (validator, epoch) from {} validator(s) in {} shard tx(s) over {} block(s) from {} on {}",
        lines.len(),
        distinct.len(),
        validators.len(),
        shards,
        scanned,
        start,
        observed_network
    );
    if lines.len() != distinct.len() {
        eprintln!("  NOTE: rows repeat where a shard tx landed in several blocks — dedup on (validator_id, att_epoch) before scoring");
    }
    if undecodable > 0 {
        eprintln!("  WARNING: {undecodable} shard tx(s) did not borsh-decode");
    }
    if lines.is_empty() {
        eprintln!(
            "  no attestations in this range. That is 'none retained/committed here', NOT proof that \
             no validator attested — a pruned node holds nothing below its pruning point."
        );
    }

    let body = if lines.is_empty() { String::new() } else { format!("{}\n", lines.join("\n")) };
    match out {
        Some(path) => {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| CliError::generic(format!("cannot open '{path}': {e}")))?;
            f.write_all(body.as_bytes()).map_err(|e| CliError::generic(format!("cannot append '{path}': {e}")))?;
        }
        None => print!("{body}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// palw-leaves — accepted C5 replica work, indexed off the finality-buried chain
// ---------------------------------------------------------------------------

/// One JSONL row per accepted, receipt-resolved PALW leaf — the C5 chain reader.
///
/// This is the fetch half that `misaka-mtp-collectors::PalwReplicaCollector` deliberately leaves
/// out of scope; its output is what `misaka-mtp-service ingest-palw` normalizes into
/// `llm_replica_work` facts. Three sources are joined, and every byte that decides credit is
/// either on the chain or verified against a chain commitment:
///
/// 1. **Leaf bodies** — leaf-chunk transactions (subnetwork `0x32`) carry every registered
///    [`PalwPublicLeafV1`], so the walk itself recovers batch/leaf → bonds, reward scripts and
///    the Receipt-v3 expectations.
/// 2. **Acceptance** — a chain block's mergeset BLUES are the blocks consensus accepted; an
///    algo-4 header in that set (palw_proof_type != 0) consumes exactly one leaf. Only chain
///    blocks at or below the finality coordinate (`virtual DAA − params.finality_depth`) are
///    joined: a leaf that can still be reorged out must not be scored, and the same coordinate
///    is re-checked by the collector downstream. Mergeset REDS are counted but never credited —
///    consensus pays them nothing, so neither does C5.
/// 3. **Receipts** — the Receipt-DA Object-v2 is off-chain by design (the leaf commits only
///    `receipt_da_root`). `--da-dir` points at the kaspad `--palw-da-import-dir` spool
///    (`<root_hex>.palwda`, written by `misaka palw da enqueue`); every object is canonically
///    decoded and its commitment recomputed against the leaf's chain-committed root, length and
///    chunk count before a single field is trusted. The k=2 verdict is re-derived from the two
///    receipt projections (`MatchProjectionV2::first_mismatch`), not read from a flag.
///
/// The owner address per slot comes from the leaf's own reward script — the ADR-0019 §8 ML-DSA
/// P2PKH template whose 64-byte payload is the bond owner's public-key hash — via the same
/// `extract_script_pub_key_address` the wallet uses, so it is byte-identical to the address a
/// provider registered with `misaka mtp register`.
///
/// Honesty bounds, all reported rather than hidden: a pruned node holds nothing below its
/// pruning point; a missing `.palwda` object drops that leaf loudly (never silently); re-running
/// an overlapping range re-emits rows, which is safe because ingest dedups on the execution
/// nullifier.
pub async fn palw_leaves(ctx: &Ctx, da_dir: &str, low_hash: Option<&str>, max_blocks: usize, out: Option<&str>) -> CliResult {
    use kaspa_consensus_core::palw::{PalwLeafChunkV1, PalwPublicLeafV1};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LEAF_CHUNK;
    use kaspa_hashes::Hash64;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use std::collections::{HashMap, HashSet};

    let network_id: kaspa_consensus_core::network::NetworkId =
        ctx.network.parse().map_err(|e| CliError::generic(format!("--network '{}' is not a network id: {e}", ctx.network)))?;
    let params = kaspa_consensus_core::config::params::Params::from(network_id);
    let prefix = kaspa_addresses::Prefix::from(network_id.network_type);
    let da_root = std::path::PathBuf::from(da_dir);

    let hostport = match &ctx.rpc {
        Some(hp) => hp.clone(),
        None => "127.0.0.1:27210".to_string(),
    };
    let timeout = Duration::from_secs(ctx.timeout_secs);
    let client = crate::node::try_connect(&format!("ws://{hostport}"), timeout)
        .await
        .map_err(|e| CliError::connection(format!("cannot reach the node at {hostport}: {e}")))?;

    let info = client.get_server_info().await.map_err(|e| CliError::connection(format!("getServerInfo failed: {e}")))?;
    let observed_network = info.network_id.to_string();
    if observed_network != ctx.network {
        let _ = client.disconnect().await;
        return Err(CliError::generic(format!(
            "this node is on '{observed_network}' but --network says '{}'. Leaves are scoped per network, \
             so indexing here would file C5 work under the wrong one.",
            ctx.network
        )));
    }

    let dag = client.get_block_dag_info().await.map_err(|e| CliError::connection(format!("getBlockDagInfo failed: {e}")))?;
    // The credit boundary. The collector re-checks every row against this same number, so a
    // reader bug cannot smuggle an unburied leaf into the ledger.
    let finality_daa_score = dag.virtual_daa_score.saturating_sub(params.finality_depth);
    let start = match low_hash {
        Some(h) => {
            h.parse::<kaspa_rpc_core::RpcHash>().map_err(|e| CliError::generic(format!("--low-hash is not a block hash: {e}")))?
        }
        None => dag.pruning_point_hash,
    };

    // Pass 1 — one walk collects all three joins' inputs.
    let mut algo4: HashMap<kaspa_rpc_core::RpcHash, (Hash64, u32)> = HashMap::new();
    let mut leaves: HashMap<(Hash64, u32), PalwPublicLeafV1> = HashMap::new();
    let mut chain_blocks: Vec<(kaspa_rpc_core::RpcHash, u64, u64, Vec<kaspa_rpc_core::RpcHash>, Vec<kaspa_rpc_core::RpcHash>)> =
        Vec::new();
    let mut scanned_hashes: HashSet<kaspa_rpc_core::RpcHash> = HashSet::new();
    let mut scanned = 0usize;
    let mut undecodable_chunks = 0usize;
    let mut cursor = start;
    loop {
        let batch =
            client.get_blocks(Some(cursor), true, true).await.map_err(|e| CliError::connection(format!("getBlocks failed: {e}")))?;
        if batch.blocks.len() <= 1 {
            break;
        }
        for b in &batch.blocks {
            let Some(v) = b.verbose_data.as_ref() else { continue };
            if !scanned_hashes.insert(v.hash) {
                continue; // the cursor echo — already recorded
            }
            scanned += 1;
            if b.header.palw_proof_type != 0 {
                algo4.insert(v.hash, (b.header.palw_batch_id, b.header.palw_leaf_index));
            }
            for tx in &b.transactions {
                if tx.subnetwork_id != SUBNETWORK_ID_PALW_LEAF_CHUNK {
                    continue;
                }
                let chunk: PalwLeafChunkV1 = match borsh::from_slice(&tx.payload) {
                    Ok(c) => c,
                    Err(_) => {
                        undecodable_chunks += 1;
                        continue;
                    }
                };
                for leaf in chunk.leaves {
                    leaves.entry((leaf.batch_id, leaf.leaf_index)).or_insert(leaf);
                }
            }
            if v.is_chain_block {
                chain_blocks.push((
                    v.hash,
                    b.header.daa_score,
                    b.header.timestamp,
                    v.merge_set_blues_hashes.clone(),
                    v.merge_set_reds_hashes.clone(),
                ));
            }
        }
        if scanned >= max_blocks {
            eprintln!("stopped at the --max-blocks bound of {max_blocks}; the range scanned is NOT the whole chain");
            break;
        }
        let next = match batch.blocks.last().and_then(|b| b.verbose_data.as_ref()).map(|v| v.hash) {
            Some(h) => h,
            None => break,
        };
        if next == cursor {
            break;
        }
        cursor = next;
    }
    let _ = client.disconnect().await;

    // Pass 2 — join acceptance → leaf body → verified receipt object, dropping loudly.
    let mut lines = Vec::new();
    let mut accepted_seen: HashSet<(Hash64, u32)> = HashSet::new();
    let (mut accepted, mut not_buried, mut red_algo4, mut unresolved_blues, mut duplicate_accepts) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut missing_leaf_body = 0usize;
    let mut legacy_v1 = 0usize;
    let mut missing_objects: Vec<String> = Vec::new();
    let mut invalid_objects: Vec<String> = Vec::new();
    for (chash, cdaa, cts, blues, reds) in &chain_blocks {
        red_algo4 += reds.iter().filter(|h| algo4.contains_key(h)).count();
        for h in blues {
            let Some((batch_id, leaf_index)) = algo4.get(h) else {
                if !scanned_hashes.contains(h) {
                    unresolved_blues += 1; // merged blue below the scan floor — its header was never seen
                }
                continue;
            };
            if *cdaa > finality_daa_score {
                not_buried += 1;
                continue;
            }
            if !accepted_seen.insert((*batch_id, *leaf_index)) {
                duplicate_accepts += 1;
                continue;
            }
            accepted += 1;
            let Some(leaf) = leaves.get(&(*batch_id, *leaf_index)) else {
                missing_leaf_body += 1;
                continue;
            };
            if leaf.receipt_da_object_version != kaspa_consensus_core::palw::da::PALW_RECEIPT_DA_OBJECT_VERSION_V2 {
                legacy_v1 += 1; // closed-net legacy receipts carry no Receipt-v3 slots to credit
                continue;
            }
            let root_hex = faster_hex::hex_string(leaf.receipt_da_root.as_byte_slice());
            let Some(object_bytes) = read_da_object(&da_root, &root_hex) else {
                missing_objects.push(root_hex);
                continue;
            };
            match palw_leaf_row(
                &observed_network,
                finality_daa_score,
                leaf,
                &h.to_string(),
                &chash.to_string(),
                *cdaa,
                *cts,
                &object_bytes,
                prefix,
            ) {
                Ok(row) => lines.push(row.to_string()),
                Err(reason) => invalid_objects.push(format!("{root_hex}: {reason}")),
            }
        }
    }

    eprintln!(
        "{} row(s) from {} accepted leaf/leaves over {} chain block(s) in {} scanned block(s) from {} on {} \
         (finality DAA {}, virtual {})",
        lines.len(),
        accepted,
        chain_blocks.len(),
        scanned,
        start,
        observed_network,
        finality_daa_score,
        dag.virtual_daa_score
    );
    if not_buried > 0 {
        eprintln!("  {not_buried} acceptance(s) above the finality coordinate — not creditable YET; re-run once buried");
    }
    if missing_leaf_body > 0 {
        eprintln!(
            "  {missing_leaf_body} accepted leaf/leaves without a registration in range — the leaf chunk is below the \
             scan floor (or pruned). Re-run with an earlier --low-hash if the node retains it."
        );
    }
    if !missing_objects.is_empty() {
        eprintln!(
            "  {} accepted leaf/leaves dropped: no `<root>.palwda` in {} — copy the DA spool over or point --da-dir at it:",
            missing_objects.len(),
            da_root.display()
        );
        for r in missing_objects.iter().take(8) {
            eprintln!("    missing {r}");
        }
    }
    if !invalid_objects.is_empty() {
        eprintln!("  {} object(s) REFUSED against their chain commitment (never credited):", invalid_objects.len());
        for r in invalid_objects.iter().take(8) {
            eprintln!("    {r}");
        }
    }
    if legacy_v1 > 0 {
        eprintln!("  {legacy_v1} legacy Object-v1 leaf/leaves skipped (closed-net receipts; no Receipt-v3 slots)");
    }
    if red_algo4 > 0 {
        eprintln!("  {red_algo4} algo-4 block(s) merged RED — consensus pays them nothing, so no C5 credit either");
    }
    if duplicate_accepts > 0 {
        eprintln!("  WARNING: {duplicate_accepts} duplicate (batch, leaf) acceptance(s) — a leaf must mint at most once");
    }
    if unresolved_blues > 0 {
        eprintln!("  {unresolved_blues} merged blue(s) below the scan floor were not classifiable (start earlier to cover them)");
    }
    if undecodable_chunks > 0 {
        eprintln!("  WARNING: {undecodable_chunks} leaf-chunk tx(s) did not borsh-decode");
    }
    if lines.is_empty() {
        eprintln!(
            "  no creditable C5 rows in this range. That is 'none observed here', NOT proof no replica work was \
             accepted — check the drop counts above and the scan floor."
        );
    }

    let body = if lines.is_empty() { String::new() } else { format!("{}\n", lines.join("\n")) };
    match out {
        Some(path) => {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| CliError::generic(format!("cannot open '{path}': {e}")))?;
            f.write_all(body.as_bytes()).map_err(|e| CliError::generic(format!("cannot append '{path}': {e}")))?;
        }
        None => print!("{body}"),
    }
    Ok(())
}

/// Locate `<root_hex>.palwda` under the DA spool root or its `incoming/` subdirectory, refusing
/// anything larger than the consensus object bound.
fn read_da_object(da_root: &std::path::Path, root_hex: &str) -> Option<Vec<u8>> {
    let name = format!("{root_hex}.palwda");
    for candidate in [da_root.join(&name), da_root.join("incoming").join(&name)] {
        if let Ok(bytes) = std::fs::read(&candidate) {
            if bytes.len() <= kaspa_consensus_core::palw::da::PALW_DA_MAX_OBJECT_BYTES {
                return Some(bytes);
            }
        }
    }
    None
}

/// Build one output row from an accepted leaf and its Receipt-DA Object-v2 bytes, or say exactly
/// why the object cannot be trusted. Pure — this is the verification seam the tests pin.
///
/// Nothing from the object is used before it survives: canonical decode, commitment (root, length,
/// chunk count) against the leaf's chain-committed values, leaf/batch/bond binding, slot shape
/// (A=0 / B=1), envelope↔receipt credential agreement, and the job challenge the leaf pinned at
/// registration. The k=2 verdict is then re-derived from the two projections.
#[allow(clippy::too_many_arguments)]
fn palw_leaf_row(
    network: &str,
    finality_daa_score: u64,
    leaf: &kaspa_consensus_core::palw::PalwPublicLeafV1,
    minted_block: &str,
    accepting_block: &str,
    accepted_daa_score: u64,
    completed_at_ms: u64,
    object_bytes: &[u8],
    prefix: kaspa_addresses::Prefix,
) -> Result<Value, String> {
    use kaspa_consensus::processes::palw_da::decode_canonical_palw_receipt_da_object_v2;
    use kaspa_consensus_core::palw::da::{PALW_RECEIPT_DA_OBJECT_VERSION_V2, palw_receipt_da_commitment};

    let object =
        decode_canonical_palw_receipt_da_object_v2(object_bytes).map_err(|e| format!("object does not decode canonically: {e:?}"))?;
    let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, object_bytes)
        .map_err(|e| format!("object commitment: {e:?}"))?;
    if commitment.root != leaf.receipt_da_root
        || commitment.object_len != leaf.receipt_da_object_len
        || commitment.chunk_count != leaf.receipt_da_chunk_count
    {
        return Err("object commitment does not match the leaf's receipt_da_root/len/chunks".into());
    }
    if object.batch_id != leaf.batch_id
        || object.leaf_index != leaf.leaf_index
        || object.provider_a_bond != leaf.provider_a_bond
        || object.provider_b_bond != leaf.provider_b_bond
        || object.provider_a_bond == object.provider_b_bond
    {
        return Err("object binding (batch/leaf/bonds) does not match the leaf".into());
    }
    let (a, b) = (&object.receipt_a, &object.receipt_b);
    if a.replica_slot != 0 || b.replica_slot != 1 {
        return Err("receipt slots are not the canonical A=0 / B=1 pairing".into());
    }
    if object.envelope_a.signer_credential_id != a.worker_credential_id
        || object.envelope_b.signer_credential_id != b.worker_credential_id
        || object.envelope_a.body_digest != a.signing_digest()
        || object.envelope_b.body_digest != b.signing_digest()
    {
        return Err("envelope does not bind its receipt (credential/digest)".into());
    }
    if a.projection.job_challenge != leaf.receipt_v3_job_challenge || b.projection.job_challenge != leaf.receipt_v3_job_challenge {
        return Err("receipt job challenge does not match the challenge the leaf pinned".into());
    }
    let k2_matched = a.projection.first_mismatch(&b.projection).is_none();

    let owner_a = kaspa_txscript::extract_script_pub_key_address(&leaf.provider_a_reward_script, prefix)
        .map_err(|e| format!("provider A reward script is not addressable: {e}"))?;
    let owner_b = kaspa_txscript::extract_script_pub_key_address(&leaf.provider_b_reward_script, prefix)
        .map_err(|e| format!("provider B reward script is not addressable: {e}"))?;

    let slot = |r: &misaka_palw::receipt_v3::ComputeReceiptV3,
                bond: &kaspa_consensus_core::tx::TransactionOutpoint,
                owner: &kaspa_addresses::Address| {
        json!({
            "replica_slot": r.replica_slot,
            "execution_nullifier": r.execution_nullifier.to_string(),
            "worker_credential_id": r.worker_credential_id.to_string(),
            "provider_bond": format!("{}:{}", bond.transaction_id, bond.index),
            "owner_address": owner.to_string(),
        })
    };
    Ok(json!({
        "network": network,
        "finality_daa_score": finality_daa_score,
        "batch_id": leaf.batch_id.to_string(),
        "leaf_index": leaf.leaf_index,
        "minted_block": minted_block,
        "accepting_block": accepting_block,
        "accepted_daa_score": accepted_daa_score,
        "completed_at_ms": completed_at_ms,
        "pair_id": object.matched_pair_id.to_string(),
        "job_challenge": a.projection.job_challenge.to_string(),
        "k2_matched": k2_matched,
        "canonical_compute_units": a.projection.canonical_compute_units,
        "slots": [slot(a, &leaf.provider_a_bond, &owner_a), slot(b, &leaf.provider_b_bond, &owner_b)],
    }))
}

#[cfg(test)]
mod palw_leaves_tests {
    use super::palw_leaf_row;
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus::processes::palw_da::{PalwReceiptDaObjectV2, palw_receipt_da_object_v2_bytes};
    use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
    use kaspa_consensus_core::palw::PalwPublicLeafV1;
    use kaspa_consensus_core::palw::da::{
        PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwProviderSessionAuthorizationV1, palw_receipt_da_commitment,
    };
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_hashes::Hash64;
    use misaka_palw::receipt_v3::{
        ComputeReceiptV3, ImplementationTelemetryV3, MLDSA87_ALGORITHM_ID, MatchProjectionV2, RECEIPT_V3_VERSION, SignedEnvelopeV3,
    };

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn projection() -> MatchProjectionV2 {
        MatchProjectionV2 {
            compute_set_id: h(0x22),
            job_challenge: h(0x33),
            output_commitment: h(0x44),
            schedule_root: h(0x55),
            execution_root: h(0x66),
            route_root: h(0x77),
            state_root: h(0x88),
            canonical_compute_units: 781_556,
            token_count: 6,
            stop_reason: 0,
        }
    }

    fn receipt(slot: u8, credential: Hash64, nullifier: Hash64, projection: MatchProjectionV2) -> ComputeReceiptV3 {
        ComputeReceiptV3 {
            receipt_version: RECEIPT_V3_VERSION,
            network_id: h(0x11),
            projection,
            telemetry: ImplementationTelemetryV3 { runtime_class_id: [slot; 32], runtime_manifest_hash: [slot + 1; 32] },
            worker_credential_id: credential,
            replica_slot: slot,
            execution_nullifier: nullifier,
            issued_epoch: 10,
            expires_epoch: 20,
        }
    }

    fn envelope(receipt: &ComputeReceiptV3) -> SignedEnvelopeV3 {
        SignedEnvelopeV3 {
            body_digest: receipt.signing_digest(),
            algorithm: MLDSA87_ALGORITHM_ID,
            signer_credential_id: receipt.worker_credential_id,
            signature: vec![0u8; 8],
        }
    }

    fn auth(bond: TransactionOutpoint) -> PalwProviderSessionAuthorizationV1 {
        PalwProviderSessionAuthorizationV1 {
            version: 1,
            network_id: 0,
            provider_bond: bond,
            owner_public_key: Vec::new(),
            session_public_key: Vec::new(),
            valid_from_epoch: 0,
            valid_until_epoch: 0,
            authorization_nonce: h(0x00),
            signature: Vec::new(),
        }
    }

    /// A leaf and the canonical Object-v2 bytes that really commit to it (root/len/chunks derived,
    /// not invented) — the same construction path `palw da enqueue` validates.
    fn fixture(receipt_b: Option<ComputeReceiptV3>) -> (PalwPublicLeafV1, Vec<u8>) {
        let bond_a = TransactionOutpoint::new(h(0x0A), 0);
        let bond_b = TransactionOutpoint::new(h(0x0B), 0);
        let a = receipt(0, h(0xA1), h(0xEA), projection());
        let b = receipt_b.unwrap_or_else(|| receipt(1, h(0xB1), h(0xEB), projection()));
        let object = PalwReceiptDaObjectV2 {
            version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            network_id: h(0x11),
            batch_id: h(0x02),
            leaf_index: 7,
            provider_a_bond: bond_a,
            provider_b_bond: bond_b,
            envelope_a: envelope(&a),
            envelope_b: envelope(&b),
            receipt_a: a,
            receipt_b: b,
            session_authorization_a: auth(bond_a),
            session_authorization_b: auth(bond_b),
            matched_pair_id: h(0xCC),
        };
        let bytes = palw_receipt_da_object_v2_bytes(&object).expect("canonical object");
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &bytes).expect("commitment");
        let leaf = PalwPublicLeafV1 {
            version: 3,
            batch_id: h(0x02),
            leaf_index: 7,
            job_nullifier: h(0x33),
            ticket_nullifier_commitment: h(0x01),
            model_profile_id: h(0x03),
            runtime_class_id: h(0x04),
            shape_id: 1,
            quantum_count: 1,
            proof_type: 1,
            provider_a_bond: bond_a,
            provider_b_bond: bond_b,
            provider_a_reward_script: p2pkh_mldsa87_spk(&[0xAA; 64]),
            provider_b_reward_script: p2pkh_mldsa87_spk(&[0xBB; 64]),
            ticket_authority_pk_hash: h(0x05),
            private_match_commitment: h(0x06),
            receipt_da_object_version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            receipt_da_root: commitment.root,
            receipt_da_object_len: commitment.object_len,
            receipt_da_chunk_count: commitment.chunk_count,
            receipt_v3_compute_set_id: h(0x22),
            receipt_v3_job_challenge: h(0x33),
            receipt_v3_issued_epoch: 10,
            receipt_v3_expires_epoch: 20,
            registered_epoch: 11,
            activation_epoch: 12,
            expiry_epoch: 20,
            leaf_bond_sompi: 1,
            // ADR-0045 D3-b LeafV2 — external-branch shape (the indexer copies these verbatim;
            // clause verification happened at acceptance, not here).
            a_commit: kaspa_hashes::Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: h(0x44),
            assignment_proof_root: h(0x45),
            dispatch_kind: kaspa_consensus_core::palw::PALW_DISPATCH_KIND_BEACON_ASSIGNED,
        };
        (leaf, bytes)
    }

    fn row(leaf: &PalwPublicLeafV1, bytes: &[u8]) -> Result<serde_json::Value, String> {
        palw_leaf_row("testnet-20", 1_000, leaf, "minted", "accepting", 900, 1_700_000_000_000, bytes, Prefix::Testnet)
    }

    /// The happy path: one row, both slots in A=0/B=1 order, every credit-deciding field carried
    /// verbatim, and the owner address derived from the leaf's own reward-script payload.
    #[test]
    fn an_accepted_leaf_becomes_one_row_with_both_slots() {
        let (leaf, bytes) = fixture(None);
        let row = row(&leaf, &bytes).expect("row");
        assert_eq!(row["network"], "testnet-20");
        assert_eq!(row["leaf_index"], 7);
        assert_eq!(row["k2_matched"], true);
        assert_eq!(row["canonical_compute_units"], 781_556);
        assert_eq!(row["pair_id"], h(0xCC).to_string());
        assert_eq!(row["job_challenge"], h(0x33).to_string());
        let slots = row["slots"].as_array().expect("slots");
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0]["replica_slot"], 0);
        assert_eq!(slots[1]["replica_slot"], 1);
        assert_eq!(slots[0]["execution_nullifier"], h(0xEA).to_string());
        assert_eq!(slots[1]["execution_nullifier"], h(0xEB).to_string());
        assert_eq!(slots[0]["provider_bond"], format!("{}:0", h(0x0A)));
        // The owner address is the reward script's 64-byte ML-DSA P2PKH payload, encoded exactly
        // as the wallet (and therefore `misaka mtp register`) encodes it.
        assert_eq!(slots[0]["owner_address"], Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &[0xAA; 64]).to_string());
        assert_eq!(slots[1]["owner_address"], Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &[0xBB; 64]).to_string());
    }

    /// A diverging projection is REPORTED as k2_matched=false, not hidden and not an error — the
    /// collector downstream is the one that refuses to credit it.
    #[test]
    fn a_diverging_projection_reports_k2_false() {
        let mut divergent = projection();
        divergent.token_count = 7;
        let (leaf, bytes) = fixture(Some(receipt(1, h(0xB1), h(0xEB), divergent)));
        let row = row(&leaf, &bytes).expect("row still emitted");
        assert_eq!(row["k2_matched"], false);
    }

    /// An object that does not hash to the leaf's chain-committed root is refused wholesale — a
    /// spool file cannot substitute receipts for the ones the leaf committed to.
    #[test]
    fn an_object_failing_the_chain_commitment_is_refused() {
        let (mut leaf, bytes) = fixture(None);
        leaf.receipt_da_root = h(0xFF);
        let err = row(&leaf, &bytes).unwrap_err();
        assert!(err.contains("commitment"), "{err}");
    }

    /// An object for a DIFFERENT leaf (right root file name, wrong binding) is refused.
    #[test]
    fn an_object_bound_to_another_leaf_is_refused() {
        let (mut leaf, bytes) = fixture(None);
        leaf.leaf_index = 9;
        let err = row(&leaf, &bytes).unwrap_err();
        assert!(err.contains("binding"), "{err}");
    }

    /// Receipt slots must be the canonical A=0 / B=1 pairing; anything else is evidence we do not
    /// understand.
    #[test]
    fn a_non_canonical_slot_pairing_is_refused() {
        let (leaf, _) = fixture(None);
        let a = receipt(1, h(0xA1), h(0xEA), projection());
        let b = receipt(0, h(0xB1), h(0xEB), projection());
        let object = PalwReceiptDaObjectV2 {
            version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            network_id: h(0x11),
            batch_id: h(0x02),
            leaf_index: 7,
            provider_a_bond: leaf.provider_a_bond,
            provider_b_bond: leaf.provider_b_bond,
            envelope_a: envelope(&a),
            envelope_b: envelope(&b),
            receipt_a: a,
            receipt_b: b,
            session_authorization_a: auth(leaf.provider_a_bond),
            session_authorization_b: auth(leaf.provider_b_bond),
            matched_pair_id: h(0xCC),
        };
        let bytes = palw_receipt_da_object_v2_bytes(&object).expect("canonical object");
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &bytes).expect("commitment");
        let mut leaf = leaf;
        leaf.receipt_da_root = commitment.root;
        leaf.receipt_da_object_len = commitment.object_len;
        leaf.receipt_da_chunk_count = commitment.chunk_count;
        let err = row(&leaf, &bytes).unwrap_err();
        assert!(err.contains("A=0 / B=1"), "{err}");
    }

    /// An envelope naming a different credential than its receipt is internally inconsistent
    /// evidence — refused before any field is trusted.
    #[test]
    fn an_envelope_credential_mismatch_is_refused() {
        let (leaf, _) = fixture(None);
        let a = receipt(0, h(0xA1), h(0xEA), projection());
        let b = receipt(1, h(0xB1), h(0xEB), projection());
        let mut envelope_a = envelope(&a);
        envelope_a.signer_credential_id = h(0x99);
        let object = PalwReceiptDaObjectV2 {
            version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            network_id: h(0x11),
            batch_id: h(0x02),
            leaf_index: 7,
            provider_a_bond: leaf.provider_a_bond,
            provider_b_bond: leaf.provider_b_bond,
            envelope_a,
            envelope_b: envelope(&b),
            receipt_a: a,
            receipt_b: b,
            session_authorization_a: auth(leaf.provider_a_bond),
            session_authorization_b: auth(leaf.provider_b_bond),
            matched_pair_id: h(0xCC),
        };
        let bytes = palw_receipt_da_object_v2_bytes(&object).expect("canonical object");
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &bytes).expect("commitment");
        let mut leaf = leaf;
        leaf.receipt_da_root = commitment.root;
        leaf.receipt_da_object_len = commitment.object_len;
        leaf.receipt_da_chunk_count = commitment.chunk_count;
        let err = row(&leaf, &bytes).unwrap_err();
        assert!(err.contains("envelope"), "{err}");
    }

    /// The challenge the leaf pinned at registration is the one the receipts must answer.
    #[test]
    fn a_receipt_answering_a_different_challenge_is_refused() {
        let (mut leaf, bytes) = fixture(None);
        leaf.receipt_v3_job_challenge = h(0x55);
        let err = row(&leaf, &bytes).unwrap_err();
        assert!(err.contains("challenge"), "{err}");
    }
}
