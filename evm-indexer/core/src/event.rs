//! §10.2 — decode a raw EVM log into normalized token-transfer rows.
//!
//! ERC-20 and ERC-721 share the `Transfer(address,address,uint256)` topic0, so
//! they are disambiguated by shape (§10.2): an indexed third value (4 topics,
//! empty data) is an ERC-721 tokenId; a value in `data` (3 topics, 32-byte data)
//! is an ERC-20 amount. ERC-1155 uses `TransferSingle`/`TransferBatch`/`URI`.
//! A `Transfer`-topic log that fits neither shape is kept as a raw
//! [`DecodedEvent::UnknownTransfer`] (the design's "never drop a raw transfer");
//! a recognized-but-malformed event (e.g. an oversized `TransferBatch` array) is
//! a [`DecodeError`] the caller records as a malformed log, not a transfer.

use alloy_primitives::U256;

/// Cap on a `TransferBatch` array length (§10.2 "異常に大きなarray"): a batch
/// claiming more pairs than this is rejected as malformed rather than expanded.
pub const BATCH_DECODE_BUDGET: usize = 4096;
/// Cap on an ABI-decoded `URI` string length (bytes).
pub const URI_DECODE_BUDGET: usize = 64 * 1024;

/// Token standard of a decoded transfer. Discriminants match the schema's
/// `token_transfers.standard smallint` (§10.3). (serde derives land with the
/// storage/query slices that actually serialize these.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TokenStandard {
    Erc20 = 0,
    Erc721 = 1,
    Erc1155 = 2,
}

/// One normalized token transfer — a `token_transfers` row (§10.3) before block
/// context (block/tx/log index) is attached by the caller. A mint is
/// `from == 0x0`, a burn is `to == 0x0` (§10.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenTransfer {
    pub standard: TokenStandard,
    /// The token contract (the log's `address`).
    pub token: [u8; 20],
    /// ERC-1155 operator; `None` for ERC-20/721.
    pub operator: Option<[u8; 20]>,
    pub from: [u8; 20],
    pub to: [u8; 20],
    /// ERC-721/1155 token id; `None` for ERC-20.
    pub token_id: Option<U256>,
    /// ERC-20/1155 amount; `1` for ERC-721 (a single token).
    pub amount: U256,
}

/// The outcome of decoding one log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedEvent {
    /// One transfer (ERC-20/721/1155-single) or many (ERC-1155-batch, expanded).
    Transfers(Vec<TokenTransfer>),
    /// ERC-1155 `URI(string,uint256)`.
    Uri { id: U256, value: String },
    /// A `Transfer`-topic0 log whose shape matched neither ERC-20 nor ERC-721;
    /// the raw log is kept by the caller, but no normalized row is produced.
    UnknownTransfer,
}

/// A recognized event that could not be safely decoded (recorded as a malformed
/// log, never expanded into transfers).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("TransferBatch ids/values length mismatch ({ids} != {values})")]
    BatchLengthMismatch { ids: usize, values: usize },
    #[error("TransferBatch array length {len} exceeds decode budget {budget}")]
    BatchTooLarge { len: usize, budget: usize },
    #[error("malformed ABI data: {0}")]
    MalformedAbi(&'static str),
}

// --- event topic0 hashes (keccak256 of the signature), parsed at compile time ---

const fn hexval(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => 0,
    }
}

/// Compile-time parse of a 64-char lowercase-hex string into a 32-byte topic.
const fn topic(hex: &[u8; 64]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (hexval(hex[i * 2]) << 4) | hexval(hex[i * 2 + 1]);
        i += 1;
    }
    out
}

/// `keccak256("Transfer(address,address,uint256)")` — ERC-20 and ERC-721.
const TRANSFER: [u8; 32] = topic(b"ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
/// `keccak256("TransferSingle(address,address,address,uint256,uint256)")`.
const TRANSFER_SINGLE: [u8; 32] = topic(b"c3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62");
/// `keccak256("TransferBatch(address,address,address,uint256[],uint256[])")`.
const TRANSFER_BATCH: [u8; 32] = topic(b"4a39dc06d4c0dbc64b70af90fd698a233a518aa5d07e595d983b8c0526c8f7fb");
/// `keccak256("URI(string,uint256)")`.
const URI: [u8; 32] = topic(b"6bb7ff708619ba0610cba295a58592e0451dee2622938c8755667688daf3529b");

/// The last 20 bytes of a 32-byte indexed-address topic (left-padded with zeros).
fn addr_from_topic(t: &[u8; 32]) -> [u8; 20] {
    let mut a = [0u8; 20];
    a.copy_from_slice(&t[12..32]);
    a
}

/// Read a 32-byte big-endian word at `off`; `None` if `data` is too short.
fn word(data: &[u8], off: usize) -> Option<U256> {
    let end = off.checked_add(32)?;
    data.get(off..end).map(U256::from_be_slice)
}

/// Read a 32-byte word at `off` as a `usize` ABI offset/length; rejects values
/// that do not fit a `usize` (an attacker-supplied huge offset).
fn word_usize(data: &[u8], off: usize) -> Result<usize, DecodeError> {
    let w = word(data, off).ok_or(DecodeError::MalformedAbi("truncated ABI head"))?;
    usize::try_from(w).map_err(|_| DecodeError::MalformedAbi("ABI offset/length exceeds usize"))
}

/// Decode an ABI `uint256[]` located at `array_off` (points at the length word),
/// bounded by [`BATCH_DECODE_BUDGET`].
fn decode_u256_array(data: &[u8], array_off: usize) -> Result<Vec<U256>, DecodeError> {
    let len = word_usize(data, array_off)?;
    if len > BATCH_DECODE_BUDGET {
        return Err(DecodeError::BatchTooLarge { len, budget: BATCH_DECODE_BUDGET });
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        // element i sits at array_off + 32 (past the length word) + i*32.
        let off = array_off
            .checked_add(32)
            .and_then(|b| b.checked_add(i.checked_mul(32)?))
            .ok_or(DecodeError::MalformedAbi("array offset overflow"))?;
        out.push(word(data, off).ok_or(DecodeError::MalformedAbi("truncated array element"))?);
    }
    Ok(out)
}

/// Decode an ABI `string` whose head word (offset) is at `head_off`, bounded by
/// [`URI_DECODE_BUDGET`].
fn decode_abi_string(data: &[u8], head_off: usize) -> Result<String, DecodeError> {
    let off = word_usize(data, head_off)?;
    let len = word_usize(data, off)?;
    if len > URI_DECODE_BUDGET {
        return Err(DecodeError::MalformedAbi("URI string exceeds budget"));
    }
    let start = off.checked_add(32).ok_or(DecodeError::MalformedAbi("string offset overflow"))?;
    let end = start.checked_add(len).ok_or(DecodeError::MalformedAbi("string offset overflow"))?;
    let bytes = data.get(start..end).ok_or(DecodeError::MalformedAbi("truncated string bytes"))?;
    // URIs are text; lossy keeps a malformed-UTF8 URI rather than dropping it.
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Decode one log into a [`DecodedEvent`]. `Ok(None)` ⇒ the log's topic0 is not a
/// token-transfer event (ignored). `Err` ⇒ a recognized event that is malformed.
pub fn decode_log(address: [u8; 20], topics: &[[u8; 32]], data: &[u8]) -> Result<Option<DecodedEvent>, DecodeError> {
    let Some(topic0) = topics.first() else { return Ok(None) };

    if *topic0 == TRANSFER {
        // ERC-20: from, to indexed; amount in data (3 topics + 32-byte data).
        if topics.len() == 3 && data.len() == 32 {
            return Ok(Some(DecodedEvent::Transfers(vec![TokenTransfer {
                standard: TokenStandard::Erc20,
                token: address,
                operator: None,
                from: addr_from_topic(&topics[1]),
                to: addr_from_topic(&topics[2]),
                token_id: None,
                amount: U256::from_be_slice(data),
            }])));
        }
        // ERC-721: from, to, tokenId all indexed (4 topics + empty data).
        if topics.len() == 4 && data.is_empty() {
            return Ok(Some(DecodedEvent::Transfers(vec![TokenTransfer {
                standard: TokenStandard::Erc721,
                token: address,
                operator: None,
                from: addr_from_topic(&topics[1]),
                to: addr_from_topic(&topics[2]),
                token_id: Some(U256::from_be_bytes(topics[3])),
                amount: U256::from(1u64),
            }])));
        }
        // A Transfer topic0 of neither shape — kept raw, no normalized row.
        return Ok(Some(DecodedEvent::UnknownTransfer));
    }

    if *topic0 == TRANSFER_SINGLE {
        // TransferSingle(operator, from, to indexed; id, value in data).
        if topics.len() != 4 || data.len() != 64 {
            return Err(DecodeError::MalformedAbi("TransferSingle expects 4 topics + 64-byte data"));
        }
        return Ok(Some(DecodedEvent::Transfers(vec![TokenTransfer {
            standard: TokenStandard::Erc1155,
            token: address,
            operator: Some(addr_from_topic(&topics[1])),
            from: addr_from_topic(&topics[2]),
            to: addr_from_topic(&topics[3]),
            token_id: Some(U256::from_be_slice(&data[0..32])),
            amount: U256::from_be_slice(&data[32..64]),
        }])));
    }

    if *topic0 == TRANSFER_BATCH {
        // TransferBatch(operator, from, to indexed; ids[], values[] in data).
        if topics.len() != 4 {
            return Err(DecodeError::MalformedAbi("TransferBatch expects 4 topics"));
        }
        let off_ids = word_usize(data, 0)?;
        let off_values = word_usize(data, 32)?;
        let ids = decode_u256_array(data, off_ids)?;
        let values = decode_u256_array(data, off_values)?;
        if ids.len() != values.len() {
            return Err(DecodeError::BatchLengthMismatch { ids: ids.len(), values: values.len() });
        }
        let operator = Some(addr_from_topic(&topics[1]));
        let from = addr_from_topic(&topics[2]);
        let to = addr_from_topic(&topics[3]);
        let transfers = ids
            .into_iter()
            .zip(values)
            .map(|(id, amount)| TokenTransfer {
                standard: TokenStandard::Erc1155,
                token: address,
                operator,
                from,
                to,
                token_id: Some(id),
                amount,
            })
            .collect();
        return Ok(Some(DecodedEvent::Transfers(transfers)));
    }

    if *topic0 == URI {
        // URI(string value, uint256 indexed id): id in topic1, value in data.
        if topics.len() != 2 {
            return Err(DecodeError::MalformedAbi("URI expects 2 topics"));
        }
        let value = decode_abi_string(data, 0)?;
        return Ok(Some(DecodedEvent::Uri { id: U256::from_be_bytes(topics[1]), value }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::keccak256;

    /// The hardcoded topic0 constants equal keccak256 of their signatures.
    #[test]
    fn topic0_constants_match_keccak() {
        assert_eq!(TRANSFER, keccak256("Transfer(address,address,uint256)").0);
        assert_eq!(TRANSFER_SINGLE, keccak256("TransferSingle(address,address,address,uint256,uint256)").0);
        assert_eq!(TRANSFER_BATCH, keccak256("TransferBatch(address,address,address,uint256[],uint256[])").0);
        assert_eq!(URI, keccak256("URI(string,uint256)").0);
    }

    fn topic_addr(byte: u8) -> [u8; 32] {
        let mut t = [0u8; 32];
        t[12..32].copy_from_slice(&[byte; 20]);
        t
    }
    fn word_of(n: u64) -> [u8; 32] {
        U256::from(n).to_be_bytes::<32>()
    }

    #[test]
    fn decodes_erc20_transfer() {
        let token = [0x11u8; 20];
        let log = decode_log(token, &[TRANSFER, topic_addr(0xAA), topic_addr(0xBB)], &word_of(1000)).unwrap().unwrap();
        let DecodedEvent::Transfers(ts) = log else { panic!("expected transfers") };
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].standard, TokenStandard::Erc20);
        assert_eq!(ts[0].token, token);
        assert_eq!(ts[0].from, [0xAA; 20]);
        assert_eq!(ts[0].to, [0xBB; 20]);
        assert_eq!(ts[0].token_id, None);
        assert_eq!(ts[0].amount, U256::from(1000u64));
        assert!(ts[0].operator.is_none());
    }

    #[test]
    fn decodes_erc721_transfer() {
        let token = [0x22u8; 20];
        // 4 topics (from, to, tokenId indexed) + empty data.
        let log = decode_log(token, &[TRANSFER, topic_addr(0xAA), topic_addr(0xBB), word_of(42)], &[]).unwrap().unwrap();
        let DecodedEvent::Transfers(ts) = log else { panic!("expected transfers") };
        assert_eq!(ts[0].standard, TokenStandard::Erc721);
        assert_eq!(ts[0].token_id, Some(U256::from(42u64)));
        assert_eq!(ts[0].amount, U256::from(1u64), "ERC-721 transfers exactly one token");
    }

    #[test]
    fn decodes_erc1155_single() {
        let mut data = Vec::new();
        data.extend_from_slice(&word_of(7)); // id
        data.extend_from_slice(&word_of(3)); // value
        let log = decode_log([0x33; 20], &[TRANSFER_SINGLE, topic_addr(0x0E), topic_addr(0xAA), topic_addr(0xBB)], &data)
            .unwrap()
            .unwrap();
        let DecodedEvent::Transfers(ts) = log else { panic!("expected transfers") };
        assert_eq!(ts[0].standard, TokenStandard::Erc1155);
        assert_eq!(ts[0].operator, Some([0x0E; 20]));
        assert_eq!(ts[0].token_id, Some(U256::from(7u64)));
        assert_eq!(ts[0].amount, U256::from(3u64));
    }

    #[test]
    fn decodes_erc1155_batch_and_enforces_invariants() {
        // ABI head [off_ids=0x40][off_values=0xA0], then ids[len=2,10,20] at 0x40
        // (occupying 0x40..0xA0) and values[len=2,100,200] at 0xA0.
        let mut data = Vec::new();
        data.extend_from_slice(&word_of(0x40)); // offset to ids
        data.extend_from_slice(&word_of(0xA0)); // offset to values
        data.extend_from_slice(&word_of(2)); // ids len
        data.extend_from_slice(&word_of(10));
        data.extend_from_slice(&word_of(20));
        data.extend_from_slice(&word_of(2)); // values len
        data.extend_from_slice(&word_of(100));
        data.extend_from_slice(&word_of(200));
        let topics = [TRANSFER_BATCH, topic_addr(0x0E), topic_addr(0xAA), topic_addr(0xBB)];
        let DecodedEvent::Transfers(ts) = decode_log([0x44; 20], &topics, &data).unwrap().unwrap() else {
            panic!("expected transfers")
        };
        assert_eq!(ts.len(), 2, "batch expands to one row per (id,value) pair");
        assert_eq!(ts[0].token_id, Some(U256::from(10u64)));
        assert_eq!(ts[0].amount, U256::from(100u64));
        assert_eq!(ts[1].token_id, Some(U256::from(20u64)));
        assert_eq!(ts[1].amount, U256::from(200u64));

        // Length mismatch (ids=2, values=1) is a malformed-log error, not a panic.
        let mut bad = Vec::new();
        bad.extend_from_slice(&word_of(0x40));
        bad.extend_from_slice(&word_of(0xA0));
        bad.extend_from_slice(&word_of(2)); // ids len 2
        bad.extend_from_slice(&word_of(10));
        bad.extend_from_slice(&word_of(20));
        bad.extend_from_slice(&word_of(1)); // values len 1
        bad.extend_from_slice(&word_of(100));
        assert!(matches!(decode_log([0x44; 20], &topics, &bad), Err(DecodeError::BatchLengthMismatch { ids: 2, values: 1 })));

        // An oversized array length is rejected by the decode budget (no huge alloc).
        let mut huge = Vec::new();
        huge.extend_from_slice(&word_of(0x40));
        huge.extend_from_slice(&word_of(0x80));
        huge.extend_from_slice(&word_of((BATCH_DECODE_BUDGET + 1) as u64));
        assert!(matches!(decode_log([0x44; 20], &topics, &huge), Err(DecodeError::BatchTooLarge { .. })));
    }

    #[test]
    fn unknown_transfer_shape_is_kept_raw_not_dropped() {
        // Transfer topic0 but 2 topics + 32-byte data (neither ERC-20 nor 721).
        let log = decode_log([0x55; 20], &[TRANSFER, topic_addr(0xAA)], &word_of(1)).unwrap().unwrap();
        assert_eq!(log, DecodedEvent::UnknownTransfer);
    }

    #[test]
    fn non_token_topic0_is_ignored() {
        assert_eq!(decode_log([0x66; 20], &[[0x99u8; 32]], &[]).unwrap(), None);
        assert_eq!(decode_log([0x66; 20], &[], &[]).unwrap(), None);
    }

    #[test]
    fn truncated_data_errors_not_panics() {
        // TransferSingle with too-short data.
        let topics = [TRANSFER_SINGLE, topic_addr(0x0E), topic_addr(0xAA), topic_addr(0xBB)];
        assert!(decode_log([0x77; 20], &topics, &[0u8; 10]).is_err());
        // TransferBatch with a head pointing past the end.
        let bt = [TRANSFER_BATCH, topic_addr(0x0E), topic_addr(0xAA), topic_addr(0xBB)];
        let mut d = Vec::new();
        d.extend_from_slice(&word_of(0x40));
        d.extend_from_slice(&word_of(0x40));
        // no array body → truncated
        assert!(decode_log([0x77; 20], &bt, &d).is_err());
    }
}
