//! kaspa-pq EVM Lane v0.4 (§16 RPC, design §8): secondary log-posting index key
//! codec for fast long-range `eth_getLogs` by address / topic.
//!
//! Postings live in the `EvmLogs` store (prefix 205). They are RPC index only —
//! never part of any commitment — and are written for every UTXO-valid block
//! (side branches included), so a query MUST canonical-filter each posting's
//! `l1_hash` against the `evm_number` map before trusting it (the same
//! canonical-resolution backstop the `evm_number` index uses).
//!
//! Key layout (a single RocksDB key per (log, kind)):
//! ```text
//!   kind:1 || selector(20|32) || evm_number:u64-be || l1_hash:64 || tx_index:u32-be || in_receipt_log_index:u32-be
//! ```
//! `evm_number` sits immediately after `kind || selector`, so a prefix range
//! scan over a fixed `(kind, selector)` bucket yields postings in ascending
//! block order — and within a block in `(tx_index, in_receipt_log_index)` order,
//! i.e. ascending block-global `logIndex`. The value is empty; the key carries
//! everything the reader needs to fetch and re-validate the log.

use kaspa_hashes::Hash64;

/// Posting kind (the leading key byte). Determines the selector width: an
/// `Address` posting keys a 20-byte contract address; a `TopicN` posting keys a
/// 32-byte indexed topic at position `N` (0..=3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogPostingKind {
    Address = 0x01,
    Topic0 = 0x02,
    Topic1 = 0x03,
    Topic2 = 0x04,
    Topic3 = 0x05,
}

impl LogPostingKind {
    /// The selector byte-width keyed by this posting kind.
    pub fn selector_len(self) -> usize {
        match self {
            LogPostingKind::Address => 20,
            _ => 32,
        }
    }

    /// The `TopicN` kind for topic position `n` (0..=3), if it is indexable.
    pub fn topic(n: usize) -> Option<LogPostingKind> {
        match n {
            0 => Some(LogPostingKind::Topic0),
            1 => Some(LogPostingKind::Topic1),
            2 => Some(LogPostingKind::Topic2),
            3 => Some(LogPostingKind::Topic3),
            _ => None,
        }
    }
}

/// One log's canonical-chain coordinates, encoded into the posting key. Ordered
/// by `evm_number` so a bucket range scan walks blocks in order; `tx_index` +
/// `in_receipt_log_index` then order logs within a block (= block-global
/// `logIndex` order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogPostingLoc {
    pub evm_number: u64,
    /// The accepting L1 block hash — re-validated against the `evm_number` map
    /// at query time so a non-canonical (side-branch) posting reads as absent.
    pub l1_hash: Hash64,
    pub tx_index: u32,
    pub in_receipt_log_index: u32,
}

/// The fixed key-suffix length after `kind || selector` (number + hash + tx + log).
const LOC_LEN: usize = 8 + 64 + 4 + 4;

/// `kind || selector` — the query bucket prefix that scopes a range scan to one
/// address/topic.
pub fn log_posting_bucket(kind: LogPostingKind, selector: &[u8]) -> Vec<u8> {
    debug_assert_eq!(selector.len(), kind.selector_len(), "selector width must match the kind");
    let mut v = Vec::with_capacity(1 + selector.len());
    v.push(kind as u8);
    v.extend_from_slice(selector);
    v
}

/// The full posting key for one `(kind, selector, log)`.
pub fn encode_log_posting_key(kind: LogPostingKind, selector: &[u8], loc: &LogPostingLoc) -> Vec<u8> {
    let mut v = log_posting_bucket(kind, selector);
    v.reserve(LOC_LEN);
    v.extend_from_slice(&loc.evm_number.to_be_bytes());
    v.extend_from_slice(&loc.l1_hash.as_bytes());
    v.extend_from_slice(&loc.tx_index.to_be_bytes());
    v.extend_from_slice(&loc.in_receipt_log_index.to_be_bytes());
    v
}

/// The seek key for the start of `[from_number, ..]` within a bucket — pass to
/// `seek_iterator` to skip every block below `from_number` (it sorts before any
/// real posting at `from_number`, whose hash/indices follow).
pub fn log_posting_seek_key(kind: LogPostingKind, selector: &[u8], from_number: u64) -> Vec<u8> {
    let mut v = log_posting_bucket(kind, selector);
    v.extend_from_slice(&from_number.to_be_bytes());
    v
}

/// Parse a full posting key back into `(kind, selector, loc)`. Returns `None` on
/// an unknown kind or a length that does not match the kind's selector width.
pub fn decode_log_posting_key(key: &[u8]) -> Option<(LogPostingKind, Vec<u8>, LogPostingLoc)> {
    let kind = match key.first()? {
        0x01 => LogPostingKind::Address,
        0x02 => LogPostingKind::Topic0,
        0x03 => LogPostingKind::Topic1,
        0x04 => LogPostingKind::Topic2,
        0x05 => LogPostingKind::Topic3,
        _ => return None,
    };
    let sl = kind.selector_len();
    if key.len() != 1 + sl + LOC_LEN {
        return None;
    }
    let mut off = 1;
    let selector = key[off..off + sl].to_vec();
    off += sl;
    let evm_number = u64::from_be_bytes(key[off..off + 8].try_into().ok()?);
    off += 8;
    let mut hb = [0u8; 64];
    hb.copy_from_slice(&key[off..off + 64]);
    off += 64;
    let l1_hash = Hash64::from_bytes(hb);
    let tx_index = u32::from_be_bytes(key[off..off + 4].try_into().ok()?);
    off += 4;
    let in_receipt_log_index = u32::from_be_bytes(key[off..off + 4].try_into().ok()?);
    Some((kind, selector, LogPostingLoc { evm_number, l1_hash, tx_index, in_receipt_log_index }))
}

/// Encode just the posting MEMBER — the bytes stored per set entry under a
/// `(kind, selector)` bucket: `number-be || l1_hash || tx-be || log-be`
/// (a fixed `LOC_LEN` bytes, so a bucket scan orders members by block ascending,
/// then by `(tx_index, in_receipt_log_index)` = block-global `logIndex`).
pub fn encode_log_posting_loc(loc: &LogPostingLoc) -> Vec<u8> {
    let mut v = Vec::with_capacity(LOC_LEN);
    v.extend_from_slice(&loc.evm_number.to_be_bytes());
    v.extend_from_slice(&loc.l1_hash.as_bytes());
    v.extend_from_slice(&loc.tx_index.to_be_bytes());
    v.extend_from_slice(&loc.in_receipt_log_index.to_be_bytes());
    v
}

/// Decode a posting member produced by [`encode_log_posting_loc`]. `None` on a
/// length mismatch.
pub fn decode_log_posting_loc(bytes: &[u8]) -> Option<LogPostingLoc> {
    if bytes.len() != LOC_LEN {
        return None;
    }
    let mut off = 0;
    let evm_number = u64::from_be_bytes(bytes[off..off + 8].try_into().ok()?);
    off += 8;
    let mut hb = [0u8; 64];
    hb.copy_from_slice(&bytes[off..off + 64]);
    off += 64;
    let l1_hash = Hash64::from_bytes(hb);
    let tx_index = u32::from_be_bytes(bytes[off..off + 4].try_into().ok()?);
    off += 4;
    let in_receipt_log_index = u32::from_be_bytes(bytes[off..off + 4].try_into().ok()?);
    Some(LogPostingLoc { evm_number, l1_hash, tx_index, in_receipt_log_index })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    #[test]
    fn roundtrip_address_and_topic() {
        let addr = [0xABu8; 20];
        let loc = LogPostingLoc { evm_number: 42, l1_hash: h(7), tx_index: 3, in_receipt_log_index: 5 };
        let key = encode_log_posting_key(LogPostingKind::Address, &addr, &loc);
        let (kind, sel, got) = decode_log_posting_key(&key).expect("decodes");
        assert_eq!(kind, LogPostingKind::Address);
        assert_eq!(sel, addr.to_vec());
        assert_eq!(got, loc);

        let topic = [0xCDu8; 32];
        let key = encode_log_posting_key(LogPostingKind::Topic1, &topic, &loc);
        let (kind, sel, got) = decode_log_posting_key(&key).expect("decodes");
        assert_eq!(kind, LogPostingKind::Topic1);
        assert_eq!(sel, topic.to_vec());
        assert_eq!(got, loc);
    }

    #[test]
    fn loc_member_roundtrip_and_orders_by_number() {
        let loc = LogPostingLoc { evm_number: 9, l1_hash: h(4), tx_index: 2, in_receipt_log_index: 1 };
        let m = encode_log_posting_loc(&loc);
        assert_eq!(m.len(), 8 + 64 + 4 + 4, "fixed-length member");
        assert_eq!(decode_log_posting_loc(&m), Some(loc));
        assert!(decode_log_posting_loc(&m[..m.len() - 1]).is_none(), "short member rejected");
        // Fixed length ⇒ byte order == (number, hash, tx, log) order — the
        // property that makes a bucket scan walk blocks ascending.
        let lo = encode_log_posting_loc(&LogPostingLoc { evm_number: 5, l1_hash: h(0xFF), tx_index: u32::MAX, in_receipt_log_index: u32::MAX });
        let hi = encode_log_posting_loc(&LogPostingLoc { evm_number: 6, l1_hash: h(0), tx_index: 0, in_receipt_log_index: 0 });
        assert!(lo < hi, "lower block number sorts first regardless of within-block position");
    }

    #[test]
    fn keys_sort_by_number_then_position_within_a_bucket() {
        let addr = [0x11u8; 20];
        let mk = |n: u64, tx: u32, li: u32| {
            encode_log_posting_key(LogPostingKind::Address, &addr, &LogPostingLoc { evm_number: n, l1_hash: h(1), tx_index: tx, in_receipt_log_index: li })
        };
        // Ascending evm_number sorts first; within a block, (tx_index, log_index).
        assert!(mk(5, 9, 9) < mk(6, 0, 0), "lower block number sorts before higher");
        assert!(mk(5, 0, 0) < mk(5, 0, 1), "within a block, lower log index first");
        assert!(mk(5, 0, 7) < mk(5, 1, 0), "earlier tx sorts before a later tx's logs");
        // The seek key for `from_number` sorts at/before the first real posting there.
        assert!(log_posting_seek_key(LogPostingKind::Address, &addr, 5) <= mk(5, 0, 0));
        // ...and strictly after every posting in the prior block.
        assert!(log_posting_seek_key(LogPostingKind::Address, &addr, 5) > mk(4, u32::MAX, u32::MAX));
    }

    #[test]
    fn bucket_is_the_shared_prefix() {
        let addr = [0x22u8; 20];
        let bucket = log_posting_bucket(LogPostingKind::Address, &addr);
        let key = encode_log_posting_key(LogPostingKind::Address, &addr, &LogPostingLoc { evm_number: 1, l1_hash: h(2), tx_index: 0, in_receipt_log_index: 0 });
        assert!(key.starts_with(&bucket), "every posting begins with its (kind, selector) bucket");
        assert_eq!(bucket.len(), 1 + 20);
        assert_eq!(bucket[0], LogPostingKind::Address as u8);
    }

    #[test]
    fn malformed_keys_decode_to_none() {
        assert!(decode_log_posting_key(&[]).is_none(), "empty");
        assert!(decode_log_posting_key(&[0xFF]).is_none(), "unknown kind");
        // Address kind but a topic-width (32) selector ⇒ wrong total length.
        let mut bad = vec![LogPostingKind::Address as u8];
        bad.extend_from_slice(&[0u8; 32 + LOC_LEN]);
        assert!(decode_log_posting_key(&bad).is_none(), "selector width mismatch");
    }
}
