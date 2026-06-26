//! EVM-lane commands over the node's Ethereum JSON-RPC (`kaspa-eth-rpc`).
//!
//! The adapter speaks plain HTTP/1.1 JSON-RPC (secp-free, no axum), so the
//! client side here is a tiny hand-rolled `TcpStream` POST — no `reqwest`, no
//! extra deps. Read-only in this Tier-A slice: balance / nonce / estimate-gas /
//! tx status / tx wait. (Signing + `evm send` is the Tier-B follow-up.)

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use kaspa_consensus_core::evm::EVM_NATIVE_SCALE;
use serde_json::{Value, json};

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

const WEI_PER_MSK: u128 = 1_000_000_000_000_000_000; // 1 MSK = 1e18 wei

// ---------------------------------------------------------------------------
// minimal HTTP/1.1 JSON-RPC client
// ---------------------------------------------------------------------------

fn http_post_json(url: &str, body: &str, timeout: Duration) -> Result<String, CliError> {
    let rest = url.strip_prefix("http://").ok_or_else(|| CliError::generic(format!("EVM RPC must be http:// (got {url})")))?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| CliError::generic(format!("bad EVM RPC port in {url}")))?),
        None => (hostport, 80u16),
    };
    let sockaddr = (host, port)
        .to_socket_addrs()
        .map_err(|e| CliError::connection(format!("resolve {host}:{port}: {e}")))?
        .next()
        .ok_or_else(|| CliError::connection(format!("no address for {host}:{port}")))?;
    let mut stream = TcpStream::connect_timeout(&sockaddr, timeout)
        .map_err(|e| CliError::connection(format!("EVM RPC connect {host}:{port}: {e} (is the node's --evm-rpc-listen up?)")))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| CliError::connection(format!("EVM RPC write: {e}")))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| CliError::connection(format!("EVM RPC read: {e}")))?;
    let text = String::from_utf8_lossy(&raw);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .ok_or_else(|| CliError::generic("malformed EVM RPC HTTP response (no body)".to_string()))?;
    Ok(body)
}

pub(crate) fn rpc_call(ctx: &Ctx, method: &str, params: Value) -> Result<Value, CliError> {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let body = http_post_json(&ctx.evm_rpc, &req, Duration::from_secs(ctx.timeout_secs))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| {
        CliError::generic(format!("EVM RPC bad JSON ({e}); body starts: {}", body.chars().take(160).collect::<String>()))
    })?;
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown");
        return Err(CliError::new(exit::GENERIC, format!("EVM RPC error ({method}): {msg}")));
    }
    v.get("result").cloned().ok_or_else(|| CliError::generic(format!("EVM RPC: no result for {method}")))
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn normalize_evm_addr(s: &str) -> Result<String, CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != 40 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(exit::GENERIC, format!("invalid EVM address (want 0x + 40 hex): {s}")));
    }
    Ok(format!("0x{}", h.to_ascii_lowercase()))
}

fn normalize_evm_hash(s: &str) -> Result<String, CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(exit::GENERIC, format!("invalid EVM tx hash (want 0x + 64 hex): {s}")));
    }
    Ok(format!("0x{}", h.to_ascii_lowercase()))
}

fn normalize_hex_data(s: &str) -> Result<String, CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() % 2 != 0 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(exit::GENERIC, format!("invalid calldata (want 0x + even hex): {s}")));
    }
    Ok(format!("0x{}", h.to_ascii_lowercase()))
}

pub(crate) fn parse_hex_u128(v: &Value) -> Result<u128, CliError> {
    let s = v.as_str().ok_or_else(|| CliError::generic(format!("expected a 0x quantity string, got {v}")))?;
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    let h = if h.is_empty() { "0" } else { h };
    u128::from_str_radix(h, 16).map_err(|e| CliError::generic(format!("bad hex quantity {s}: {e}")))
}

/// wei -> "X[.frac] MSK" (1 MSK = 1e18 wei), trailing zeros trimmed.
fn format_msk(wei: u128) -> String {
    let whole = wei / WEI_PER_MSK;
    let frac = wei % WEI_PER_MSK;
    if frac == 0 {
        return whole.to_string();
    }
    let frac_str = format!("{frac:018}");
    format!("{whole}.{}", frac_str.trim_end_matches('0'))
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// EVM chain id (used by `node doctor` to probe the EVM RPC surface).
pub fn chain_id(ctx: &Ctx) -> Result<u64, CliError> {
    Ok(parse_hex_u128(&rpc_call(ctx, "eth_chainId", json!([]))?)? as u64)
}

pub fn balance(ctx: &Ctx, address: &str) -> CliResult {
    let addr = normalize_evm_addr(address)?;
    let wei = parse_hex_u128(&rpc_call(ctx, "eth_getBalance", json!([addr, "latest"]))?)?;
    match ctx.output {
        OutputFormat::Human => println!("{} MSK  ({wei} wei)", format_msk(wei)),
        OutputFormat::Json => {
            println!("{}", json!({ "ok": true, "address": addr, "balanceWei": wei.to_string(), "balanceMsk": format_msk(wei) }))
        }
    }
    Ok(())
}

pub fn nonce(ctx: &Ctx, address: &str) -> CliResult {
    let addr = normalize_evm_addr(address)?;
    let n = parse_hex_u128(&rpc_call(ctx, "eth_getTransactionCount", json!([addr, "latest"]))?)? as u64;
    match ctx.output {
        OutputFormat::Human => println!("{n}"),
        OutputFormat::Json => println!("{}", json!({ "ok": true, "address": addr, "nonce": n })),
    }
    Ok(())
}

pub fn estimate_gas(ctx: &Ctx, from: &str, to: Option<&str>, value_sompi: u64, data: Option<&str>) -> CliResult {
    let from = normalize_evm_addr(from)?;
    let value_wei = (value_sompi as u128) * (EVM_NATIVE_SCALE as u128);
    let mut call = json!({ "from": from, "value": format!("0x{value_wei:x}") });
    if let Some(to) = to {
        call["to"] = json!(normalize_evm_addr(to)?);
    }
    if let Some(data) = data {
        call["input"] = json!(normalize_hex_data(data)?);
    }
    let gas = parse_hex_u128(&rpc_call(ctx, "eth_estimateGas", json!([call]))?)? as u64;
    match ctx.output {
        OutputFormat::Human => println!("{gas}"),
        OutputFormat::Json => println!("{}", json!({ "ok": true, "gas": gas })),
    }
    Ok(())
}

pub fn tx_status(ctx: &Ctx, hash: &str) -> CliResult {
    let h = normalize_evm_hash(hash)?;
    let status = rpc_call(ctx, "misaka_getEvmTxStatus", json!([h]))?;
    print_tx_status(ctx, &status);
    Ok(())
}

pub fn tx_wait(ctx: &Ctx, hash: &str, timeout_secs: u64, poll_secs: u64) -> CliResult {
    let h = normalize_evm_hash(hash)?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let poll = Duration::from_secs(poll_secs.max(1));
    let mut last_state = String::new();
    loop {
        let status = rpc_call(ctx, "misaka_getEvmTxStatus", json!([h]))?;
        let state = status.get("state").and_then(Value::as_str).unwrap_or("unknown").to_string();
        // progress to STDERR (keeps stdout clean for the final result / JSON)
        if state != last_state && ctx.output == OutputFormat::Human && !ctx.quiet {
            eprintln!(
                "[{:>4}s] state={state}",
                (timeout_secs as i64 - deadline.saturating_duration_since(Instant::now()).as_secs() as i64).max(0)
            );
            last_state = state;
        }
        let st = status.get("state").and_then(Value::as_str).unwrap_or("unknown");
        if st == "accepted" {
            print_tx_status(ctx, &status);
            return Ok(());
        }
        if Instant::now() >= deadline {
            print_tx_status(ctx, &status);
            return Err(CliError::new(
                exit::TIMEOUT_PENDING,
                format!("tx {h} not accepted within {timeout_secs}s (last state: {st})"),
            ));
        }
        std::thread::sleep(poll);
    }
}

fn print_tx_status(ctx: &Ctx, s: &Value) {
    if ctx.output == OutputFormat::Json {
        println!("{}", json!({ "ok": true, "status": s }));
        return;
    }
    let get_str = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("-").to_string();
    let state = get_str("state");
    let included = s.get("includedIn").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let accepted = s.get("acceptedIn");
    let accepted_disp = match accepted {
        Some(Value::Object(o)) => {
            let block = o.get("block").and_then(Value::as_str).unwrap_or("?");
            let idx = o.get("receiptIndex").and_then(Value::as_u64).unwrap_or(0);
            format!("{block} (receipt {idx})")
        }
        _ => "pending".to_string(),
    };
    let skip = s.get("lastSkipClass").and_then(Value::as_u64).map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
    let in_pool = s.get("inMempool").and_then(Value::as_bool).unwrap_or(false);
    let nonce = s.get("nonce").and_then(Value::as_str).map(parse_quantity_disp).unwrap_or_else(|| "-".to_string());
    let gas = s.get("gasLimit").and_then(Value::as_str).map(parse_quantity_disp).unwrap_or_else(|| "-".to_string());

    println!("Transaction : {}", get_str("transactionHash"));
    println!("State       : {state}");
    println!("In mempool  : {in_pool}");
    println!("Included in : {included} DAG block(s)");
    println!("Accepted in : {accepted_disp}");
    println!("Skip class  : {skip}");
    println!("Sender      : {}", get_str("sender"));
    println!("Nonce       : {nonce}");
    println!("Gas limit   : {gas}");
    // Operator-facing interpretation: a skipped/included tx is NOT permanently
    // failed under the §6.1 gas-pool model — a later chain block can re-include
    // and accept it once the canonical order shifts.
    if state != "accepted" && state != "unknown" {
        println!();
        println!("This transaction is not permanently failed; a later chain block may re-include and accept it.");
    }
    if state == "unknown" {
        println!();
        println!("Not seen on this node (never relayed here, already pruned, or wrong hash).");
    }
}

/// Render a hex `0x` quantity as decimal for display (falls back to the raw string).
fn parse_quantity_disp(s: &str) -> String {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    match u128::from_str_radix(if h.is_empty() { "0" } else { h }, 16) {
        Ok(n) => n.to_string(),
        Err(_) => s.to_string(),
    }
}
