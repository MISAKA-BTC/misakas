//! A hand-rolled HTTP/1.1 JSON-RPC client implementing [`NodeRpc`] over a tokio
//! `TcpStream`.
//!
//! Same rationale as the §9 server hand-roll: the workspace pins tokio `1.42.1`,
//! so reqwest/hyper (which want tokio `>=1.44`) are out. The §9 eth-rpc server
//! answers each request with `Connection: close` + a `Content-Length`, so the
//! client is trivial: open a socket, write one request, read to EOF, split the
//! body off the header block. Keep-alive / connection pooling is a latency
//! optimization left as a follow-on — correctness does not need it.
//!
//! The request builder and response splitter are pure functions, unit-tested
//! below; only [`HttpNodeRpc::call`] touches a socket.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::node::{NodeBlock, NodeLog, NodeRpc, RpcError, parse_block, parse_logs, parse_quantity, unwrap_envelope};

/// Cap on a response body (mirrors the server's `MAX_BODY_BYTES`): a node that
/// streams more than this is treated as a transport fault rather than buffered
/// without bound.
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// HTTP/1.1 JSON-RPC client for a node's `--evm-rpc-listen` endpoint.
pub struct HttpNodeRpc {
    /// `host:port` dial target.
    addr: String,
    /// `Host:` header value (defaults to `addr`).
    host_header: String,
    /// Request path (default `/`).
    path: String,
    /// Per-request deadline (connect + write + read).
    timeout: Duration,
}

impl HttpNodeRpc {
    /// Build a client for `host:port` (e.g. `"127.0.0.1:8545"`).
    pub fn new(addr: impl Into<String>) -> Self {
        let addr = addr.into();
        Self { host_header: addr.clone(), path: "/".to_string(), addr, timeout: Duration::from_secs(30) }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// One JSON-RPC call: build the request, do the exchange under [`Self::timeout`],
    /// then unwrap the JSON-RPC envelope.
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let body = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .map_err(|e| RpcError::Json(e.to_string()))?;
        let request = build_request(&self.host_header, &self.path, &body);

        let raw = tokio::time::timeout(self.timeout, self.exchange(&request))
            .await
            .map_err(|_| RpcError::Transport("request timed out".to_string()))??;

        let body = split_response(&raw)?;
        let value: Value = serde_json::from_slice(body).map_err(|e| RpcError::Json(e.to_string()))?;
        unwrap_envelope(value)
    }

    /// Connect, send the request bytes, read the full response to EOF.
    async fn exchange(&self, request: &[u8]) -> Result<Vec<u8>, RpcError> {
        let mut stream = TcpStream::connect(&self.addr).await.map_err(|e| RpcError::Transport(e.to_string()))?;
        stream.write_all(request).await.map_err(|e| RpcError::Transport(e.to_string()))?;
        stream.flush().await.map_err(|e| RpcError::Transport(e.to_string()))?;

        // Server answers with `Connection: close`, so read until EOF — bounded.
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = stream.read(&mut chunk).await.map_err(|e| RpcError::Transport(e.to_string()))?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_RESPONSE_BYTES {
                return Err(RpcError::Transport("response exceeded size cap".to_string()));
            }
        }
        Ok(buf)
    }
}

/// An Ethereum `Quantity` for a block number (`0x`-prefixed, no leading zeros).
fn quantity(n: u64) -> String {
    format!("0x{n:x}")
}

/// Build a complete HTTP/1.1 POST request for a JSON-RPC `body`.
fn build_request(host: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

/// Split an HTTP response: verify a 2xx status line, then return the body slice
/// after the `\r\n\r\n` header terminator.
fn split_response(raw: &[u8]) -> Result<&[u8], RpcError> {
    let sep = find_subslice(raw, b"\r\n\r\n").ok_or(RpcError::Transport("response missing header terminator".to_string()))?;
    let head = &raw[..sep];
    let status_line = head.split(|&b| b == b'\r' || b == b'\n').next().unwrap_or(b"");
    let status_str = std::str::from_utf8(status_line).map_err(|_| RpcError::Transport("non-utf8 status line".to_string()))?;
    // "HTTP/1.1 200 OK" → the second whitespace token is the code.
    let code = status_str.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok());
    match code {
        Some(c) if (200..300).contains(&c) => Ok(&raw[sep + 4..]),
        Some(c) => Err(RpcError::Transport(format!("node returned HTTP {c}"))),
        None => Err(RpcError::Transport(format!("unparseable status line: {status_str}"))),
    }
}

/// First index of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

#[async_trait]
impl NodeRpc for HttpNodeRpc {
    async fn block_number(&self) -> Result<u64, RpcError> {
        let result = self.call("eth_blockNumber", json!([])).await?;
        parse_quantity(result.as_str().ok_or(RpcError::Decode("blockNumber not a string"))?)
    }

    async fn get_block(&self, number: u64) -> Result<Option<NodeBlock>, RpcError> {
        // `false` → tx hashes only (we never need full tx objects for indexing).
        let result = self.call("eth_getBlockByNumber", json!([quantity(number), false])).await?;
        parse_block(&result)
    }

    async fn get_logs(&self, from: u64, to: u64) -> Result<Vec<NodeLog>, RpcError> {
        let filter = json!({"fromBlock": quantity(from), "toBlock": quantity(to)});
        let result = self.call("eth_getLogs", json!([filter])).await?;
        parse_logs(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_headers_and_body() {
        let req = build_request("127.0.0.1:8545", "/", b"{\"x\":1}");
        let text = String::from_utf8(req).unwrap();
        assert!(text.starts_with("POST / HTTP/1.1\r\n"));
        assert!(text.contains("Host: 127.0.0.1:8545\r\n"));
        assert!(text.contains("Content-Length: 7\r\n"));
        assert!(text.contains("Connection: close\r\n"));
        assert!(text.ends_with("\r\n\r\n{\"x\":1}"));
    }

    #[test]
    fn split_response_returns_body_on_2xx() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"result\":\"0x1\"}";
        assert_eq!(split_response(raw).unwrap(), b"{\"result\":\"0x1\"}");
    }

    #[test]
    fn split_response_rejects_non_2xx_and_garbage() {
        let err = b"HTTP/1.1 500 Internal Server Error\r\n\r\noops";
        assert!(matches!(split_response(err), Err(RpcError::Transport(_))));
        let no_sep = b"HTTP/1.1 200 OK\r\nincomplete";
        assert!(matches!(split_response(no_sep), Err(RpcError::Transport(_))));
    }

    #[test]
    fn quantity_renders_without_leading_zeros() {
        assert_eq!(quantity(0), "0x0");
        assert_eq!(quantity(255), "0xff");
    }

    #[test]
    fn find_subslice_basics() {
        assert_eq!(find_subslice(b"abcde", b"cd"), Some(2));
        assert_eq!(find_subslice(b"abc", b"xy"), None);
        assert_eq!(find_subslice(b"abc", b""), None);
    }
}
