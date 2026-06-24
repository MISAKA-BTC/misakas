//! The node seam (§10.5): what the indexer needs from a live node, and the
//! eth-rpc JSON DTOs it parses.
//!
//! The indexer talks to the node ONLY through public Ethereum JSON-RPC. For the
//! reconcile + backfill loop that is exactly three reads:
//!
//! * `eth_blockNumber` — the canonical EVM head height,
//! * `eth_getBlockByNumber` — the block id (hash) at a height, to detect reorgs,
//! * `eth_getLogs` — the canonical logs in a height range, the transfer source.
//!
//! [`NodeRpc`] abstracts those three so the [`crate::engine`] driver is testable
//! against a fake in-memory node; [`crate::http`] is the real HTTP/1.1 impl. The
//! DTOs ([`NodeBlock`], [`NodeLog`]) and the pure JSON parsers below are shared
//! by both and unit-tested here without any socket.

use async_trait::async_trait;
use misaka_evm_indexer_core::sync::BlockId;
use serde_json::Value;

/// A failure talking to the node: transport, malformed JSON, a JSON-RPC error
/// object, or a value we could not decode into a DTO.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("invalid JSON response: {0}")]
    Json(String),
    #[error("node returned JSON-RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("could not decode {0}")]
    Decode(&'static str),
}

/// The block fields the reconcile planner needs: its height, its eth-rpc id
/// (hash), and its parent. `l1_hash` is not separately exposed by standard
/// eth-rpc (the rpc `hash` IS the accepting L1 hash truncated to 32, per the §9
/// adapter), so the store row's `l1_hash` mirrors `rpc_hash` for node-sourced
/// blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeBlock {
    pub number: u64,
    pub rpc_hash: [u8; 32],
    pub parent_hash: [u8; 32],
}

impl NodeBlock {
    /// The planner's identity for this block at its height.
    pub fn block_id(&self) -> BlockId {
        BlockId { number: self.number, rpc_hash: self.rpc_hash }
    }
}

/// One `eth_getLogs` entry, decoded into primitives the core decoder consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeLog {
    pub address: [u8; 20],
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub tx_hash: [u8; 32],
    pub tx_index: u32,
    pub log_index: u32,
}

/// The node read surface the indexer drives. Async so the HTTP impl can await IO;
/// the engine tests implement it over an in-memory chain.
#[async_trait]
pub trait NodeRpc {
    /// `eth_blockNumber`: the current canonical EVM head height.
    async fn block_number(&self) -> Result<u64, RpcError>;

    /// `eth_getBlockByNumber(number, false)`: the canonical block at a height, or
    /// `None` if the node has no block there (height above its head).
    async fn get_block(&self, number: u64) -> Result<Option<NodeBlock>, RpcError>;

    /// `eth_getLogs` over the inclusive height range `[from, to]`. The caller
    /// keeps the range within the node's `eth_getLogs` block-span cap.
    async fn get_logs(&self, from: u64, to: u64) -> Result<Vec<NodeLog>, RpcError>;
}

// --- pure JSON helpers (unit-tested below; no IO) ---

/// Unwrap a JSON-RPC 2.0 response envelope: an `error` object becomes
/// [`RpcError::Rpc`]; otherwise the `result` value is returned (`null` when the
/// method legitimately returns null, e.g. a missing block).
pub fn unwrap_envelope(v: Value) -> Result<Value, RpcError> {
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = err.get("message").and_then(Value::as_str).unwrap_or("").to_string();
        return Err(RpcError::Rpc { code, message });
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

/// Parse a hex `Quantity` (`0x`-prefixed, e.g. `eth_blockNumber`) into a `u64`.
pub fn parse_quantity(s: &str) -> Result<u64, RpcError> {
    let h = s.strip_prefix("0x").ok_or(RpcError::Decode("quantity missing 0x prefix"))?;
    if h.is_empty() {
        return Ok(0); // "0x" — treat as zero rather than an error
    }
    u64::from_str_radix(h, 16).map_err(|_| RpcError::Decode("quantity not valid hex u64"))
}

/// Parse a variable-length `0x`-prefixed hex byte string (log `data`).
pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, RpcError> {
    let h = s.strip_prefix("0x").ok_or(RpcError::Decode("hex missing 0x prefix"))?;
    if h.len() % 2 != 0 {
        return Err(RpcError::Decode("hex has odd length"));
    }
    let mut out = vec![0u8; h.len() / 2];
    faster_hex::hex_decode(h.as_bytes(), &mut out).map_err(|_| RpcError::Decode("hex not valid"))?;
    Ok(out)
}

/// Parse a fixed-width `0x`-prefixed hex value (a 20- or 32-byte hash/address).
pub fn parse_hex_fixed<const N: usize>(s: &str, what: &'static str) -> Result<[u8; N], RpcError> {
    let h = s.strip_prefix("0x").ok_or(RpcError::Decode(what))?;
    if h.len() != 2 * N {
        return Err(RpcError::Decode(what));
    }
    let mut out = [0u8; N];
    faster_hex::hex_decode(h.as_bytes(), &mut out).map_err(|_| RpcError::Decode(what))?;
    Ok(out)
}

fn str_field<'a>(v: &'a Value, key: &'static str, what: &'static str) -> Result<&'a str, RpcError> {
    v.get(key).and_then(Value::as_str).ok_or(RpcError::Decode(what))
}

/// Parse an `eth_getBlockByNumber` result object into a [`NodeBlock`]. A `null`
/// result (no block at that height) maps to `Ok(None)`.
pub fn parse_block(result: &Value) -> Result<Option<NodeBlock>, RpcError> {
    if result.is_null() {
        return Ok(None);
    }
    Ok(Some(NodeBlock {
        number: parse_quantity(str_field(result, "number", "block.number")?)?,
        rpc_hash: parse_hex_fixed(str_field(result, "hash", "block.hash")?, "block.hash")?,
        parent_hash: parse_hex_fixed(str_field(result, "parentHash", "block.parentHash")?, "block.parentHash")?,
    }))
}

/// Parse one `eth_getLogs` array element into a [`NodeLog`].
pub fn parse_log(v: &Value) -> Result<NodeLog, RpcError> {
    let topics = v
        .get("topics")
        .and_then(Value::as_array)
        .ok_or(RpcError::Decode("log.topics"))?
        .iter()
        .map(|t| parse_hex_fixed::<32>(t.as_str().ok_or(RpcError::Decode("log.topic"))?, "log.topic"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NodeLog {
        address: parse_hex_fixed(str_field(v, "address", "log.address")?, "log.address")?,
        topics,
        data: parse_hex_bytes(str_field(v, "data", "log.data")?)?,
        block_number: parse_quantity(str_field(v, "blockNumber", "log.blockNumber")?)?,
        block_hash: parse_hex_fixed(str_field(v, "blockHash", "log.blockHash")?, "log.blockHash")?,
        tx_hash: parse_hex_fixed(str_field(v, "transactionHash", "log.transactionHash")?, "log.transactionHash")?,
        tx_index: parse_quantity(str_field(v, "transactionIndex", "log.transactionIndex")?)? as u32,
        log_index: parse_quantity(str_field(v, "logIndex", "log.logIndex")?)? as u32,
    })
}

/// Parse an `eth_getLogs` result (an array) into [`NodeLog`]s.
pub fn parse_logs(result: &Value) -> Result<Vec<NodeLog>, RpcError> {
    result.as_array().ok_or(RpcError::Decode("getLogs result not an array"))?.iter().map(parse_log).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quantity_round_trips_and_rejects() {
        assert_eq!(parse_quantity("0x0").unwrap(), 0);
        assert_eq!(parse_quantity("0x").unwrap(), 0);
        assert_eq!(parse_quantity("0x10").unwrap(), 16);
        assert_eq!(parse_quantity("0xdeadbeef").unwrap(), 0xdead_beef);
        assert!(parse_quantity("10").is_err(), "missing 0x");
        assert!(parse_quantity("0xzz").is_err(), "not hex");
    }

    #[test]
    fn hex_fixed_enforces_width() {
        let a: [u8; 20] = parse_hex_fixed(&format!("0x{}", "11".repeat(20)), "addr").unwrap();
        assert_eq!(a, [0x11; 20]);
        assert!(parse_hex_fixed::<32>("0x1122", "h").is_err(), "too short for 32");
        assert!(parse_hex_fixed::<20>("1122", "h").is_err(), "missing prefix");
    }

    #[test]
    fn envelope_distinguishes_error_result_and_null() {
        let ok = json!({"jsonrpc":"2.0","id":1,"result":"0x5"});
        assert_eq!(unwrap_envelope(ok).unwrap(), json!("0x5"));

        let null = json!({"jsonrpc":"2.0","id":1,"result":null});
        assert!(unwrap_envelope(null).unwrap().is_null());

        let err = json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"boom"}});
        match unwrap_envelope(err) {
            Err(RpcError::Rpc { code, message }) => {
                assert_eq!(code, -32000);
                assert_eq!(message, "boom");
            }
            other => panic!("expected Rpc error, got {other:?}"),
        }
    }

    #[test]
    fn parse_block_handles_null_and_object() {
        assert_eq!(parse_block(&Value::Null).unwrap(), None);
        let b = json!({
            "number": "0x3",
            "hash": format!("0x{}", "ab".repeat(32)),
            "parentHash": format!("0x{}", "cd".repeat(32)),
        });
        let nb = parse_block(&b).unwrap().unwrap();
        assert_eq!(nb.number, 3);
        assert_eq!(nb.rpc_hash, [0xab; 32]);
        assert_eq!(nb.parent_hash, [0xcd; 32]);
        assert_eq!(nb.block_id(), BlockId { number: 3, rpc_hash: [0xab; 32] });
    }

    #[test]
    fn parse_log_decodes_all_fields() {
        let lg = json!({
            "address": format!("0x{}", "44".repeat(20)),
            "topics": [format!("0x{}", "ee".repeat(32)), format!("0x{}", "0f".repeat(32))],
            "data": "0x00ff",
            "blockNumber": "0x7",
            "blockHash": format!("0x{}", "07".repeat(32)),
            "transactionHash": format!("0x{}", "01".repeat(32)),
            "transactionIndex": "0x2",
            "logIndex": "0x5",
        });
        let parsed = parse_log(&lg).unwrap();
        assert_eq!(parsed.address, [0x44; 20]);
        assert_eq!(parsed.topics, vec![[0xee; 32], [0x0f; 32]]);
        assert_eq!(parsed.data, vec![0x00, 0xff]);
        assert_eq!(parsed.block_number, 7);
        assert_eq!(parsed.tx_index, 2);
        assert_eq!(parsed.log_index, 5);
        // And an array of them.
        let arr = json!([lg]);
        assert_eq!(parse_logs(&arr).unwrap().len(), 1);
    }
}
