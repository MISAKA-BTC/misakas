//! §9 — RFC 6455 WebSocket transport for the Ethereum JSON-RPC adapter.
//!
//! This is a HAND-ROLLED, server-side-only WebSocket layer, for the same reason
//! the crate hand-rolls its HTTP/1.1 server: the workspace pins tokio `1.42.1`,
//! so the obvious frameworks (axum/hyper/jsonrpsee, or the wRPC stack's
//! `workflow-rpc`) are out — they require tokio `>=1.44` and/or drag in the
//! whole `workflow-rs` + Borsh graph, breaking the adapter's secp-free,
//! dependency-light guarantee. RFC 6455 server framing is *simpler* than the
//! HTTP/1.1 parsing already shipped here (fixed 2–14 byte header + payload), so
//! the trade-off the crate already made for HTTP holds for WebSocket too.
//!
//! **This slice (§9 slice 2) is the transport skeleton only.** It does the
//! upgrade handshake, the frame codec, and routes ordinary JSON-RPC requests
//! over the socket through the SAME [`crate::process`] dispatch the HTTP path
//! uses — so e.g. `eth_chainId` works over `ws://`. `eth_subscribe` is not yet a
//! method (it falls through to `METHOD_NOT_FOUND`); slice 3 adds the
//! subscription registry + event sources on top of this exact reader/writer
//! split (the architecture is already what subscriptions need: a bounded
//! outbound queue drained by a dedicated writer task, independent of the reader).
//!
//! Backpressure (design §9.4 / R-10): the per-connection outbound queue is
//! BOUNDED. A consumer that lets it fill is a slow consumer and the connection
//! is closed — there is no unbounded buffering anywhere.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::{EthProvider, codes, err_value, process};

/// RFC 6455 §1.3 handshake GUID, concatenated with the client key to form the
/// `Sec-WebSocket-Accept` response.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

// RFC 6455 §5.2 opcodes.
const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BIN: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// Max payload of a single inbound frame AND of a fully reassembled message — a
/// peer cannot make us buffer more than this (mirrors the HTTP `MAX_BODY_BYTES`).
const MAX_WS_MESSAGE: usize = 4 * 1024 * 1024;
/// Bounded per-connection outbound queue depth (design §9.4: "max queued
/// notifications"). A subscriber/peer that lets this fill is a slow consumer and
/// the connection is closed rather than buffering without bound.
const OUTBOUND_QUEUE: usize = 2048;
/// WebSocket keepalive ping period. Long-lived WS connections have no blanket
/// `CONN_TIMEOUT`; periodic pings (plus the peer's TCP state) detect dead links.
const PING_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// Max concurrent subscriptions per connection (design §9.4) — bounds the
/// forwarding tasks a single connection can spawn (a DoS guard); excess
/// `eth_subscribe`s are refused until the client unsubscribes.
const MAX_SUBS_PER_CONN: usize = 64;

/// If the request head is a WebSocket upgrade (a `GET` carrying
/// `Upgrade: websocket` and a `Sec-WebSocket-Key`), return that key. Otherwise
/// `None` and the caller serves it as ordinary HTTP.
pub fn upgrade_key(head: &str) -> Option<String> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    // WebSocket upgrades are GETs (RFC 6455 §4.1). A POST etc. is not an upgrade.
    if !request_line.split_whitespace().next().unwrap_or("").eq_ignore_ascii_case("GET") {
        return None;
    }
    let mut has_upgrade = false;
    let mut key: Option<String> = None;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else { continue };
        let k = k.trim();
        if k.eq_ignore_ascii_case("upgrade") && v.to_ascii_lowercase().contains("websocket") {
            has_upgrade = true;
        } else if k.eq_ignore_ascii_case("sec-websocket-key") {
            key = Some(v.trim().to_string());
        }
    }
    if has_upgrade { key } else { None }
}

/// RFC 6455 §4.2.2: `base64(SHA1(key ‖ GUID))`.
fn accept_key(ws_key: &str) -> String {
    let mut h = Sha1::new();
    h.update(ws_key.as_bytes());
    h.update(WS_GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// One decoded inbound frame.
struct Frame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

/// A protocol-violation I/O error (closes the connection).
fn proto(msg: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
}

/// Buffered frame reader over any async source. Seeded with the bytes that
/// followed the handshake headers (`leftover`) so a peer that pipelines a frame
/// immediately after its upgrade request is not lost.
struct FrameReader<R> {
    rd: R,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    fn new(rd: R, leftover: Vec<u8>) -> Self {
        Self { rd, buf: leftover }
    }

    /// Ensure at least `n` bytes are buffered; `Ok(false)` on clean EOF.
    async fn fill_to(&mut self, n: usize) -> std::io::Result<bool> {
        let mut tmp = [0u8; 8192];
        while self.buf.len() < n {
            let k = self.rd.read(&mut tmp).await?;
            if k == 0 {
                return Ok(false);
            }
            self.buf.extend_from_slice(&tmp[..k]);
        }
        Ok(true)
    }

    /// Read the next frame; `Ok(None)` on clean EOF, `Err` on a protocol/I/O fault.
    async fn next(&mut self) -> std::io::Result<Option<Frame>> {
        if !self.fill_to(2).await? {
            return Ok(None);
        }
        let b0 = self.buf[0];
        let b1 = self.buf[1];
        let fin = b0 & 0x80 != 0;
        // We negotiate no extensions, so the RSV bits must be zero (RFC 6455 §5.2).
        if b0 & 0x70 != 0 {
            return Err(proto("ws: reserved bits set"));
        }
        let opcode = b0 & 0x0f;
        let masked = b1 & 0x80 != 0;
        let len7 = (b1 & 0x7f) as usize;
        let (len, header_len) = match len7 {
            126 => {
                if !self.fill_to(4).await? {
                    return Ok(None);
                }
                (u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize, 4)
            }
            127 => {
                if !self.fill_to(10).await? {
                    return Ok(None);
                }
                let mut a = [0u8; 8];
                a.copy_from_slice(&self.buf[2..10]);
                (u64::from_be_bytes(a) as usize, 10)
            }
            n => (n, 2),
        };
        // RFC 6455 §5.1: every client→server frame MUST be masked.
        if !masked {
            return Err(proto("ws: client frame not masked"));
        }
        // RFC 6455 §5.5: control frames are <=125 bytes and never fragmented.
        let is_control = opcode & 0x08 != 0;
        if is_control && (len > 125 || !fin) {
            return Err(proto("ws: invalid control frame"));
        }
        if len > MAX_WS_MESSAGE {
            return Err(proto("ws: frame too large"));
        }
        let total = header_len + 4 + len;
        if !self.fill_to(total).await? {
            return Ok(None);
        }
        let mask = [self.buf[header_len], self.buf[header_len + 1], self.buf[header_len + 2], self.buf[header_len + 3]];
        let body_start = header_len + 4;
        let mut payload = self.buf[body_start..total].to_vec();
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i & 3];
        }
        self.buf.drain(..total);
        Ok(Some(Frame { fin, opcode, payload }))
    }
}

/// Write one server frame: FIN=1, single-frame, UNMASKED (RFC 6455 §5.1 — a
/// server MUST NOT mask).
async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut hdr = Vec::with_capacity(10);
    hdr.push(0x80 | opcode); // FIN | opcode
    let len = payload.len();
    if len < 126 {
        hdr.push(len as u8);
    } else if len <= 0xffff {
        hdr.push(126);
        hdr.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        hdr.push(127);
        hdr.extend_from_slice(&(len as u64).to_be_bytes());
    }
    w.write_all(&hdr).await?;
    w.write_all(payload).await?;
    w.flush().await
}

/// An item queued for the writer task.
enum WsOut {
    Text(String),
    Pong(Vec<u8>),
    Ping,
    Close,
}

/// Serve a WebSocket connection: complete the handshake, then run the frame
/// loop until EOF/close/error. Generic over the socket so the codec is testable
/// over an in-memory duplex.
///
/// Architecture: the socket is split; a dedicated WRITER task owns the write
/// half and drains a bounded outbound queue (so notification pushes in slice 3
/// never block on the reader, and a slow consumer is bounded → closed). A
/// keepalive task pings periodically. The READER loop assembles messages and
/// routes JSON-RPC requests through the shared HTTP dispatch.
pub async fn serve_ws<S>(stream: S, provider: Arc<dyn EthProvider>, ws_key: &str, leftover: Vec<u8>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (rd, mut wr) = tokio::io::split(stream);

    // Handshake: 101 Switching Protocols with the computed accept key.
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(ws_key)
    );
    wr.write_all(resp.as_bytes()).await?;
    wr.flush().await?;
    kaspa_core::trace!("[eth-rpc] websocket connection established");

    let (out_tx, mut out_rx) = mpsc::channel::<WsOut>(OUTBOUND_QUEUE);

    // Writer task: the ONLY writer of the socket. Ends on channel close or a
    // write error (which, by dropping `wr`, also unblocks the reader's peer).
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let r = match msg {
                WsOut::Text(s) => write_frame(&mut wr, OP_TEXT, s.as_bytes()).await,
                WsOut::Pong(p) => write_frame(&mut wr, OP_PONG, &p).await,
                WsOut::Ping => write_frame(&mut wr, OP_PING, &[]).await,
                WsOut::Close => {
                    let _ = write_frame(&mut wr, OP_CLOSE, &[]).await;
                    break;
                }
            };
            if r.is_err() {
                break;
            }
        }
    });

    // Keepalive pinger. `try_send` (never blocks): if the queue is full the link
    // is already unhealthy, and if it is closed the connection is gone — stop.
    let ping_tx = out_tx.clone();
    let pinger = tokio::spawn(async move {
        let mut tick = tokio::time::interval(PING_INTERVAL);
        tick.tick().await; // the first tick fires immediately — skip it
        loop {
            tick.tick().await;
            if ping_tx.try_send(WsOut::Ping).is_err() {
                break;
            }
        }
    });

    // Per-connection subscription registry (§9). Dropped at teardown → aborts all
    // forwarding tasks (each holds an outbound sender), letting the writer exit.
    let conn = WsConn::new(out_tx.clone());
    let mut reader = FrameReader::new(rd, leftover);
    // (first opcode, accumulated payload) while reassembling a fragmented message.
    let mut assembling: Option<(u8, Vec<u8>)> = None;

    let result = loop {
        let frame = match reader.next().await {
            Ok(Some(f)) => f,
            Ok(None) => break Ok(()),  // clean EOF
            Err(e) => break Err(e),    // protocol / I/O fault
        };
        match frame.opcode {
            OP_PING => {
                if out_tx.try_send(WsOut::Pong(frame.payload)).is_err() {
                    break Ok(()); // slow/closed consumer → done
                }
            }
            OP_PONG => {} // keepalive acknowledgement — ignore
            OP_CLOSE => {
                let _ = out_tx.try_send(WsOut::Close);
                break Ok(());
            }
            OP_TEXT | OP_BIN | OP_CONT => {
                let (first_op, mut acc) = match (frame.opcode, assembling.take()) {
                    (OP_CONT, Some(s)) => s,
                    (OP_CONT, None) => break Err(proto("ws: continuation without start")),
                    (op, None) => (op, Vec::new()),
                    (_op, Some(_)) => break Err(proto("ws: new data frame mid-fragment")),
                };
                if acc.len() + frame.payload.len() > MAX_WS_MESSAGE {
                    break Err(proto("ws: message too large"));
                }
                acc.extend_from_slice(&frame.payload);
                if !frame.fin {
                    assembling = Some((first_op, acc));
                    continue;
                }
                // Complete message. JSON-RPC is text; a binary frame is tolerated
                // as long as it is valid UTF-8 JSON (some clients send opcode 0x2).
                let text = match String::from_utf8(acc) {
                    Ok(t) => t,
                    Err(_) => break Err(proto("ws: non-utf8 message")),
                };
                if handle_message(&provider, &conn, &out_tx, text).await.is_err() {
                    // Outbound closed or full (slow consumer) → end the connection.
                    break Ok(());
                }
            }
            _ => break Err(proto("ws: unknown opcode")),
        }
    };

    // Teardown: drop the reader's sender + the registry (which aborts every
    // forwarding task, releasing their outbound-sender clones) + stop the pinger,
    // so the writer observes all senders gone, drains, and exits — then join it so
    // the socket is flushed/closed before we return.
    drop(out_tx);
    drop(conn);
    pinger.abort();
    let _ = writer.await;
    result
}

/// Decode one JSON-RPC message and route it. `eth_subscribe`/`eth_unsubscribe`
/// are connection-stateful — they create/drop entries in this connection's [`WsConn`]
/// registry — so they are handled here rather than in the shared HTTP dispatch;
/// every other method goes through [`crate::process`] exactly as over HTTP.
/// Returns `Err(())` if the outbound queue is closed/full (slow consumer) so the
/// caller tears down.
async fn handle_message(provider: &Arc<dyn EthProvider>, conn: &WsConn, out: &mpsc::Sender<WsOut>, text: String) -> Result<(), ()> {
    let val: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return send_value(out, err_value(codes::PARSE_ERROR, &format!("parse error: {e}"))),
    };
    match val.get("method").and_then(|m| m.as_str()) {
        Some("eth_subscribe") => return send_value(out, conn.subscribe(provider, &val)),
        Some("eth_unsubscribe") => return send_value(out, conn.unsubscribe(&val)),
        _ => {}
    }
    match process(provider, val).await {
        Some(resp) => send_value(out, resp),
        None => Ok(()), // notification — no reply
    }
}

/// Per-connection subscription registry (§9). `eth_subscribe` allocates a fresh
/// id, spawns a task that forwards the requested event stream into this
/// connection's outbound queue as `eth_subscription` notifications, and records
/// its [`JoinHandle`]; `eth_unsubscribe` (and connection teardown via [`Drop`])
/// aborts that task. Ids are per-connection (the Ethereum contract: a
/// subscription id is only meaningful on the connection that created it).
struct WsConn {
    /// The connection's outbound queue (the writer task owns the socket).
    out: mpsc::Sender<WsOut>,
    /// Monotonic per-connection id source.
    next_id: AtomicU64,
    /// Active subscriptions: numeric id → its forwarding task.
    subs: Mutex<HashMap<u64, JoinHandle<()>>>,
}

impl WsConn {
    fn new(out: mpsc::Sender<WsOut>) -> Self {
        Self { out, next_id: AtomicU64::new(1), subs: Mutex::new(HashMap::new()) }
    }

    /// Allocate a fresh subscription id as a `(numeric key, hex string)` pair —
    /// the key indexes the registry; the hex string is the client-facing id.
    fn alloc_id(&self) -> (u64, String) {
        let num = self.next_id.fetch_add(1, Ordering::Relaxed);
        (num, format!("0x{num:x}"))
    }

    /// Handle `eth_subscribe`. Returns the JSON-RPC response value (the new
    /// subscription id on success, or an error object).
    fn subscribe(&self, provider: &Arc<dyn EthProvider>, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        // Per-connection subscription cap (design §9.4): bound the forwarding tasks
        // one connection can spawn. Checked before allocating an id or spawning.
        if self.subs.lock().unwrap().len() >= MAX_SUBS_PER_CONN {
            return err_with_id(id, codes::SERVER_ERROR, "too many subscriptions on this connection (max 64); unsubscribe first");
        }
        let kind = req.get("params").and_then(|p| p.get(0)).and_then(|k| k.as_str());
        match kind {
            Some("newPendingTransactions") => {
                let (num, sub_id) = self.alloc_id();
                let rx = provider.subscribe_pending_txs();
                let handle = spawn_forward(sub_id.clone(), rx, self.out.clone(), |hash: &[u8; 32]| {
                    Some(json!(format!("0x{}", faster_hex::hex_string(hash))))
                });
                self.subs.lock().unwrap().insert(num, handle);
                json!({ "jsonrpc": "2.0", "id": id, "result": sub_id })
            }
            Some("newHeads") => {
                let (num, sub_id) = self.alloc_id();
                let rx = provider.subscribe_new_heads();
                let handle =
                    spawn_forward(sub_id.clone(), rx, self.out.clone(), |head: &crate::EthBlock| Some(crate::render_head(head)));
                self.subs.lock().unwrap().insert(num, handle);
                json!({ "jsonrpc": "2.0", "id": id, "result": sub_id })
            }
            Some("logs") => {
                // The optional second param is an eth_getLogs-style filter object —
                // {address, topics}. Reuse the HTTP parsers so a logs subscription
                // and eth_getLogs accept exactly the same filter shape.
                let filter = req.get("params").and_then(|p| p.get(1));
                let addresses = match crate::parse_address_list(filter.and_then(|f| f.get("address"))) {
                    Ok(a) => a,
                    Err(e) => return err_with_id(id, e.code, &e.message),
                };
                let topics = match crate::parse_topic_filter(filter.and_then(|f| f.get("topics"))) {
                    Ok(t) => t,
                    Err(e) => return err_with_id(id, e.code, &e.message),
                };
                // Design §9.4: refuse an UNFILTERED logs subscription — an all-logs
                // firehose on this unauthenticated endpoint. "Unfiltered" means no
                // address AND every topic position is a wildcard (covers no filter,
                // `{}`, `topics:[]`, and `topics:[null,…]`); clients that truly want
                // everything use the bounded `eth_getLogs` instead.
                if addresses.is_empty() && topics.iter().all(|pos| pos.is_empty()) {
                    return err_with_id(
                        id,
                        codes::INVALID_PARAMS,
                        "eth_subscribe logs requires at least one filter (address or a non-wildcard topic); an unfiltered all-logs subscription is refused",
                    );
                }
                let (num, sub_id) = self.alloc_id();
                let rx = provider.subscribe_logs();
                let handle = spawn_forward(sub_id.clone(), rx, self.out.clone(), move |ev: &crate::EthLogEvent| {
                    // Per-subscription filter applied here (the stream is unfiltered);
                    // a detached log (removed=true) is delivered only if it matches.
                    log_matches(&ev.log, &addresses, &topics).then(|| crate::render_log(&ev.log, ev.removed))
                });
                self.subs.lock().unwrap().insert(num, handle);
                json!({ "jsonrpc": "2.0", "id": id, "result": sub_id })
            }
            Some(other) => err_with_id(
                id,
                codes::INVALID_PARAMS,
                &format!("unsupported subscription '{other}' (newPendingTransactions, newHeads, logs supported)"),
            ),
            None => err_with_id(id, codes::INVALID_PARAMS, "eth_subscribe: missing subscription kind"),
        }
    }

    /// Handle `eth_unsubscribe`. Aborts the forwarding task; result is whether the
    /// id named a live subscription on THIS connection (the Ethereum contract).
    fn unsubscribe(&self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let parsed = req
            .get("params")
            .and_then(|p| p.get(0))
            .and_then(|s| s.as_str())
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|h| u64::from_str_radix(h, 16).ok());
        let removed = match parsed {
            Some(num) => match self.subs.lock().unwrap().remove(&num) {
                Some(handle) => {
                    handle.abort();
                    true
                }
                None => false,
            },
            None => false,
        };
        json!({ "jsonrpc": "2.0", "id": id, "result": removed })
    }
}

impl Drop for WsConn {
    fn drop(&mut self) {
        // Connection gone → stop every forwarding task (each holds an outbound
        // sender clone; aborting them lets the writer task observe all senders
        // dropped and exit, so serve_ws's `writer.await` completes).
        for (_, handle) in self.subs.lock().unwrap().drain() {
            handle.abort();
        }
    }
}

/// Build an `eth_subscription` notification frame body for subscription `sub_id`.
fn subscription_note(sub_id: &str, result: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "eth_subscription",
        "params": { "subscription": sub_id, "result": result },
    }))
    .unwrap_or_else(|_| "null".to_string())
}

/// A JSON-RPC error object that echoes the request id (the shared `err_value`
/// always uses a null id; `eth_subscribe`/`eth_unsubscribe` must reply on theirs).
fn err_with_id(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// `eth_getLogs`-style filter match: a log passes if (no address filter OR its
/// address is listed) AND every topic position matches — an empty position is a
/// wildcard, otherwise the log's topic there must be one of the options. A log
/// with fewer topics than the filter has positions fails (unless wildcard).
fn log_matches(e: &crate::EthLogEntry, addresses: &[[u8; 20]], topics: &[Vec<[u8; 32]>]) -> bool {
    if !addresses.is_empty() && !addresses.contains(&e.address) {
        return false;
    }
    for (i, options) in topics.iter().enumerate() {
        if options.is_empty() {
            continue; // wildcard at this position
        }
        match e.topics.get(i) {
            Some(t) if options.contains(t) => {}
            _ => return false,
        }
    }
    true
}

/// Forward a subscription's broadcast stream into the outbound queue: each item
/// is passed to `render`, and a `Some(payload)` is wrapped in an `eth_subscription`
/// notification (a `None` is filtered out — used by `logs` to drop non-matching
/// entries). Ends when the source closes, or when the outbound queue is
/// closed/full (slow consumer / connection gone). A `Lagged` is logged and
/// skipped — the client reconnects + backfills per the §9.5 protocol.
fn spawn_forward<T, F>(sub_id: String, mut rx: broadcast::Receiver<T>, out: mpsc::Sender<WsOut>, render: F) -> JoinHandle<()>
where
    T: Clone + Send + 'static,
    F: Fn(&T) -> Option<Value> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(item) => {
                    let Some(payload) = render(&item) else { continue }; // filtered out
                    let note = subscription_note(&sub_id, payload);
                    if out.try_send(WsOut::Text(note)).is_err() {
                        break; // slow consumer / outbound closed
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    kaspa_core::trace!("[eth-rpc] ws subscription {sub_id} lagged {n} (client should reconnect + backfill)");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

/// Queue a JSON value as a text frame. `try_send` enforces the bounded-queue
/// backpressure: a full queue means a slow consumer → `Err(())` → close.
fn send_value(out: &mpsc::Sender<WsOut>, v: Value) -> Result<(), ()> {
    let s = serde_json::to_string(&v).unwrap_or_else(|_| "null".to_string());
    out.try_send(WsOut::Text(s)).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EthBlock, EthCallRequest, EthEvmTxStatus, EthFeeHistory, EthLogEntry, EthReceipt, EthResult, EthRpcError, EthTx};
    use kaspa_consensus_core::evm::EvmAccountSnapshot;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// RFC 6455 §1.3 worked example: the canonical accept-key test vector.
    #[test]
    fn accept_key_rfc_vector() {
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn upgrade_key_detection() {
        let ws = "GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n";
        assert_eq!(upgrade_key(ws).as_deref(), Some("dGhlIHNhbXBsZSBub25jZQ=="));
        // A POST (the HTTP JSON-RPC path) is never an upgrade.
        let post = "POST / HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: abc\r\n\r\n";
        assert_eq!(upgrade_key(post), None);
        // A GET without the Upgrade header is not an upgrade.
        let plain = "GET / HTTP/1.1\r\nSec-WebSocket-Key: abc\r\n\r\n";
        assert_eq!(upgrade_key(plain), None);
    }

    /// Encode a client→server frame (masked, per RFC 6455 §5.1) for tests.
    fn client_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mask = [0xA1u8, 0xB2, 0xC3, 0xD4];
        let mut f = vec![0x80 | opcode];
        let len = payload.len();
        if len < 126 {
            f.push(0x80 | len as u8);
        } else if len <= 0xffff {
            f.push(0x80 | 126);
            f.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            f.push(0x80 | 127);
            f.extend_from_slice(&(len as u64).to_be_bytes());
        }
        f.extend_from_slice(&mask);
        f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i & 3]));
        f
    }

    /// The frame reader unmasks a client frame; the frame writer emits an
    /// unmasked server frame the reader (fed a re-masked copy) round-trips.
    #[tokio::test]
    async fn frame_codec_round_trip() {
        // Reader: a masked client text frame decodes to the original payload,
        // across the 7-bit, 16-bit (126) and 64-bit (127) length encodings.
        for size in [5usize, 200, 70_000] {
            let payload = vec![0x5Au8; size];
            let frame = client_frame(OP_TEXT, &payload);
            let mut reader = FrameReader::new(&frame[..], Vec::new());
            let f = reader.next().await.unwrap().unwrap();
            assert_eq!(f.opcode, OP_TEXT);
            assert!(f.fin);
            assert_eq!(f.payload, payload);
        }
        // Writer: a server frame is UNMASKED (mask bit clear) and length-correct.
        let mut buf = Vec::new();
        write_frame(&mut buf, OP_TEXT, b"hello").await.unwrap();
        assert_eq!(buf[0], 0x80 | OP_TEXT);
        assert_eq!(buf[1] & 0x80, 0, "server frames must not be masked");
        assert_eq!(buf[1] & 0x7f, 5);
        assert_eq!(&buf[2..], b"hello");
        // An unmasked client frame is a protocol violation.
        let unmasked = vec![0x80 | OP_TEXT, 1u8, 0x00];
        let mut r2 = FrameReader::new(&unmasked[..], Vec::new());
        assert!(r2.next().await.is_err());
    }

    /// Minimal [`EthProvider`] for the transport tests. `chain_id` backs the
    /// `eth_chainId` round-trip; `pending` lets a test inject mempool admissions
    /// for the `newPendingTransactions` subscription. Everything else is unused.
    struct MockProvider {
        pending: broadcast::Sender<[u8; 32]>,
        heads: broadcast::Sender<crate::EthBlock>,
        logs: broadcast::Sender<crate::EthLogEvent>,
    }
    impl MockProvider {
        fn new() -> Self {
            Self { pending: broadcast::channel(16).0, heads: broadcast::channel(16).0, logs: broadcast::channel(16).0 }
        }
    }
    #[async_trait::async_trait]
    impl EthProvider for MockProvider {
        fn chain_id(&self) -> u64 {
            0x4d534b
        }
        fn subscribe_pending_txs(&self) -> broadcast::Receiver<[u8; 32]> {
            self.pending.subscribe()
        }
        fn subscribe_new_heads(&self) -> broadcast::Receiver<crate::EthBlock> {
            self.heads.subscribe()
        }
        fn subscribe_logs(&self) -> broadcast::Receiver<crate::EthLogEvent> {
            self.logs.subscribe()
        }
        fn client_version(&self) -> String {
            "mock".to_string()
        }
        async fn block_number(&self) -> EthResult<u64> {
            Ok(0)
        }
        async fn is_syncing(&self) -> bool {
            false
        }
        async fn gas_price(&self) -> EthResult<u128> {
            Ok(0)
        }
        async fn latest_account(&self, _a: [u8; 20]) -> EthResult<Option<EvmAccountSnapshot>> {
            Ok(None)
        }
        async fn eth_call(&self, _r: EthCallRequest, _block: crate::BlockId) -> EthResult<Vec<u8>> {
            Err(EthRpcError::server("unused"))
        }
        async fn estimate_gas(&self, _r: EthCallRequest, _block: crate::BlockId) -> EthResult<u64> {
            Err(EthRpcError::server("unused"))
        }
        async fn send_raw_transaction(&self, _raw: Vec<u8>) -> EthResult<[u8; 32]> {
            Err(EthRpcError::server("unused"))
        }
        async fn transaction_receipt(&self, _h: [u8; 32]) -> EthResult<Option<EthReceipt>> {
            Ok(None)
        }
        async fn transaction_by_hash(&self, _h: [u8; 32]) -> EthResult<Option<EthTx>> {
            Ok(None)
        }
        async fn evm_tx_status(&self, _h: [u8; 32]) -> EthResult<EthEvmTxStatus> {
            Err(EthRpcError::server("unused"))
        }
        async fn block_by_number(&self, _n: u64) -> EthResult<Option<EthBlock>> {
            Ok(None)
        }
        async fn block_by_tag(&self, _t: &str) -> EthResult<Option<EthBlock>> {
            Ok(None)
        }
        async fn block_by_hash(&self, _h: [u8; 32]) -> EthResult<Option<EthBlock>> {
            Ok(None)
        }
        async fn get_logs(&self, _f: u64, _t: u64, _a: Vec<[u8; 20]>, _tp: Vec<Vec<[u8; 32]>>) -> EthResult<Vec<EthLogEntry>> {
            Ok(vec![])
        }
        async fn fee_history(&self, _c: u64, _n: u64, _p: Vec<f64>) -> EthResult<EthFeeHistory> {
            Err(EthRpcError::server("unused"))
        }
    }

    /// End-to-end over an in-memory duplex: handshake → a real JSON-RPC request
    /// (`eth_chainId`) round-trips as a framed text reply → ping is answered with
    /// a pong → close ends the connection.
    /// Drain + assert the 101 handshake response (ends at CRLFCRLF).
    async fn read_handshake<R: AsyncReadExt + Unpin>(r: &mut R) {
        let mut head = Vec::new();
        let mut one = [0u8; 1];
        loop {
            let n = r.read(&mut one).await.unwrap();
            assert_ne!(n, 0, "server closed before handshake");
            head.push(one[0]);
            if head.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("HTTP/1.1 101 "), "got: {head:?}");
        assert!(head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }

    #[tokio::test]
    async fn ws_request_response_ping_close() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let provider: Arc<dyn EthProvider> = Arc::new(MockProvider::new());
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;

        // Send a real request as a masked client text frame.
        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId"}"#)).await.unwrap();

        // Read the server's reply frame (FIN|TEXT, unmasked) and assert the body.
        let reply = read_server_text(&mut crd).await;
        assert!(reply.contains("\"id\":1"), "reply: {reply}");
        assert!(reply.contains("0x4d534b"), "eth_chainId over ws: {reply}");

        // A ping is answered with a pong (same payload).
        cwr.write_all(&client_frame(OP_PING, b"hi")).await.unwrap();
        let (op, payload) = read_server_frame(&mut crd).await;
        assert_eq!(op, OP_PONG);
        assert_eq!(payload, b"hi");

        // A close frame ends the connection cleanly.
        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), srv).await.expect("serve_ws should return").unwrap();
        assert!(r.is_ok());
    }

    /// §9 slice 3: `eth_subscribe("newPendingTransactions")` returns an id, each
    /// mempool admission arrives as an `eth_subscription` notification, and
    /// `eth_unsubscribe` halts the stream.
    #[tokio::test]
    async fn ws_pending_tx_subscription() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let mock = Arc::new(MockProvider::new());
        let pending = mock.pending.clone(); // clone the sender before moving the Arc
        let provider: Arc<dyn EthProvider> = mock;
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;

        // Subscribe → a fresh id.
        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":7,"method":"eth_subscribe","params":["newPendingTransactions"]}"#))
            .await
            .unwrap();
        let resp: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert_eq!(resp["id"], 7);
        let sub_id = resp["result"].as_str().expect("subscription id").to_string();
        assert!(sub_id.starts_with("0x"));

        // An admission arrives as an eth_subscription notification carrying the hash.
        // (subscribe() registered the receiver synchronously, before this send.)
        pending.send([0xAB; 32]).unwrap();
        let note: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert_eq!(note["method"], "eth_subscription");
        assert_eq!(note["params"]["subscription"], sub_id);
        assert_eq!(note["params"]["result"], format!("0x{}", "ab".repeat(32)));

        // Unsubscribe → true, and the stream halts.
        let unsub_req = format!(r#"{{"jsonrpc":"2.0","id":8,"method":"eth_unsubscribe","params":["{sub_id}"]}}"#);
        cwr.write_all(&client_frame(OP_TEXT, unsub_req.as_bytes())).await.unwrap();
        let unsub: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert_eq!(unsub["result"], true);

        // A post-unsubscribe admission produces NO further notification. (The send
        // itself may report "no receivers" — the forwarder dropped its receiver
        // when the subscription was aborted — which is exactly the halt we want.)
        let _ = pending.send([0x11; 32]);
        let quiet = tokio::time::timeout(std::time::Duration::from_millis(200), read_server_frame(&mut crd)).await;
        assert!(quiet.is_err(), "unsubscribe must halt the stream");

        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), srv).await.expect("serve_ws should return").unwrap();
        assert!(r.is_ok());
    }

    /// §9 slice 4: `eth_subscribe("newHeads")` delivers each emitted block as an
    /// `eth_subscription` carrying the header-only payload (no `transactions`).
    #[tokio::test]
    async fn ws_new_heads_subscription() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let mock = Arc::new(MockProvider::new());
        let heads = mock.heads.clone();
        let provider: Arc<dyn EthProvider> = mock;
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;

        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":3,"method":"eth_subscribe","params":["newHeads"]}"#))
            .await
            .unwrap();
        let resp: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        let sub_id = resp["result"].as_str().expect("subscription id").to_string();

        // Emit a head → it arrives as a header-only eth_subscription notification.
        let mut base_fee = [0u8; 32];
        base_fee[31] = 7;
        let block = crate::EthBlock {
            number: 0x2a,
            hash: [0x11; 32],
            parent_hash: [0x22; 32],
            state_root: [0x33; 32],
            transactions_root: [0x44; 32],
            receipts_root: [0x55; 32],
            logs_bloom: vec![0u8; 256],
            timestamp: 0x66,
            gas_used: 21_000,
            gas_limit: 30_000_000,
            base_fee_per_gas: base_fee,
            miner: [0x77; 20],
            tx_hashes: vec![[0xAA; 32]],
            size: 100,
        };
        heads.send(block).unwrap();
        let note: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert_eq!(note["method"], "eth_subscription");
        assert_eq!(note["params"]["subscription"], sub_id);
        let head = &note["params"]["result"];
        assert_eq!(head["number"], "0x2a");
        assert_eq!(head["hash"], format!("0x{}", "11".repeat(32)));
        assert_eq!(head["parentHash"], format!("0x{}", "22".repeat(32)));
        assert_eq!(head["gasLimit"], "0x1c9c380"); // 30_000_000
        assert_eq!(head["baseFeePerGas"], "0x7");
        assert_eq!(head["miner"], format!("0x{}", "77".repeat(20)));
        assert!(head.get("transactions").is_none(), "newHeads is header-only");

        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), srv).await.expect("serve_ws should return").unwrap();
        assert!(r.is_ok());
    }

    /// §9 slice 5: `eth_subscribe("logs", filter)` delivers matching log events
    /// with the correct `removed` flag and drops non-matching ones (address filter).
    #[tokio::test]
    async fn ws_logs_subscription_filtered() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let mock = Arc::new(MockProvider::new());
        let logs = mock.logs.clone();
        let provider: Arc<dyn EthProvider> = mock;
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;

        // An UNFILTERED logs subscription is refused (design §9.4 — no all-logs
        // firehose on this unauthenticated endpoint).
        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":8,"method":"eth_subscribe","params":["logs"]}"#))
            .await
            .unwrap();
        let refused: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert!(refused.get("error").is_some(), "unfiltered logs subscription must be refused: {refused}");

        let addr_hex = format!("0x{}", "ab".repeat(20));
        let sub_req = format!(r#"{{"jsonrpc":"2.0","id":9,"method":"eth_subscribe","params":["logs",{{"address":"{addr_hex}"}}]}}"#);
        cwr.write_all(&client_frame(OP_TEXT, sub_req.as_bytes())).await.unwrap();
        let resp: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        let sub_id = resp["result"].as_str().expect("subscription id").to_string();

        let mk = |addr: [u8; 20], removed: bool| crate::EthLogEvent {
            log: crate::EthLogEntry {
                address: addr,
                topics: vec![],
                data: vec![],
                block_number: 5,
                block_hash: [0x01; 32],
                tx_hash: [0x02; 32],
                tx_index: 0,
                log_index: 0,
            },
            removed,
        };
        // A non-matching address is filtered out; the matching one (removed=true)
        // is delivered — proving both the address filter and the removed flag.
        logs.send(mk([0x11; 20], false)).unwrap();
        logs.send(mk([0xAB; 20], true)).unwrap();
        let note: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert_eq!(note["method"], "eth_subscription");
        assert_eq!(note["params"]["subscription"], sub_id);
        let log = &note["params"]["result"];
        assert_eq!(log["address"], addr_hex, "only the matching-address log is delivered");
        assert_eq!(log["removed"], true, "detached log carries removed=true");
        assert_eq!(log["blockNumber"], "0x5");

        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), srv).await.expect("serve_ws should return").unwrap();
        assert!(r.is_ok());
    }

    /// The per-connection subscription cap (design §9.4) refuses the 65th sub.
    #[tokio::test]
    async fn ws_subscription_cap_enforced() {
        let (client, server) = tokio::io::duplex(256 * 1024);
        let provider: Arc<dyn EthProvider> = Arc::new(MockProvider::new());
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;

        // MAX_SUBS_PER_CONN subscriptions succeed.
        for i in 0..MAX_SUBS_PER_CONN {
            let req = format!(r#"{{"jsonrpc":"2.0","id":{i},"method":"eth_subscribe","params":["newHeads"]}}"#);
            cwr.write_all(&client_frame(OP_TEXT, req.as_bytes())).await.unwrap();
            let resp: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
            assert!(resp.get("result").is_some(), "subscription {i} should succeed: {resp}");
        }
        // One more is refused with an error.
        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":99,"method":"eth_subscribe","params":["newHeads"]}"#))
            .await
            .unwrap();
        let resp: Value = serde_json::from_str(&read_server_text(&mut crd).await).unwrap();
        assert!(resp.get("error").is_some(), "the over-cap subscription must be refused: {resp}");

        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), srv).await.expect("serve_ws should return").unwrap();
    }

    /// A connection that drops with a LIVE subscription must abort its forwarding
    /// task (no leak): serve_ws returns promptly after the client closes, even
    /// though the subscription was never explicitly unsubscribed.
    #[tokio::test]
    async fn ws_close_with_active_subscription() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let provider: Arc<dyn EthProvider> = Arc::new(MockProvider::new());
        let srv = tokio::spawn(async move { serve_ws(server, provider, "dGhlIHNhbXBsZSBub25jZQ==", Vec::new()).await });

        let (mut crd, mut cwr) = tokio::io::split(client);
        read_handshake(&mut crd).await;
        cwr.write_all(&client_frame(OP_TEXT, br#"{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newPendingTransactions"]}"#))
            .await
            .unwrap();
        let _ = read_server_text(&mut crd).await; // the subscription id

        // Close WITHOUT unsubscribing — teardown (WsConn::drop) must abort the
        // still-live forwarder so the writer can exit and serve_ws can return.
        cwr.write_all(&client_frame(OP_CLOSE, b"")).await.unwrap();
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), srv)
            .await
            .expect("serve_ws must return promptly despite a live subscription")
            .unwrap();
        assert!(r.is_ok());
    }

    /// Read one server frame (FIN, single-frame, unmasked) → (opcode, payload).
    async fn read_server_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> (u8, Vec<u8>) {
        let mut h = [0u8; 2];
        r.read_exact(&mut h).await.unwrap();
        let opcode = h[0] & 0x0f;
        assert_eq!(h[1] & 0x80, 0, "server frame must be unmasked");
        let len7 = (h[1] & 0x7f) as usize;
        let len = match len7 {
            126 => {
                let mut b = [0u8; 2];
                r.read_exact(&mut b).await.unwrap();
                u16::from_be_bytes(b) as usize
            }
            127 => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b).await.unwrap();
                u64::from_be_bytes(b) as usize
            }
            n => n,
        };
        let mut payload = vec![0u8; len];
        r.read_exact(&mut payload).await.unwrap();
        (opcode, payload)
    }

    async fn read_server_text<R: AsyncReadExt + Unpin>(r: &mut R) -> String {
        let (op, payload) = read_server_frame(r).await;
        assert_eq!(op, OP_TEXT);
        String::from_utf8(payload).unwrap()
    }
}
