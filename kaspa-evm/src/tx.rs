//! EIP-2718 typed-transaction decoding → revm `TxEnv`, with secp/k256 sender
//! recovery (design §3.1: `evm_payload.transactions` are EIP-2718 bytes).
//!
//! A tx whose bytes fail to decode or whose signature fails to recover is simply
//! not includable — the producer must not put it in a block. The block stays
//! valid (only producer commitment/diff faults invalidate a block, design §6.3);
//! a syntactic encoding pre-check lives in body validation (P3). User execution
//! failures (revert / OOG / bad nonce) are receipts with `status = 0`, never
//! block-invalid (§8.2).

use alloy_consensus::{Transaction as _, TxEnvelope};
use alloy_eips::eip2718::{Decodable2718, Encodable2718};
use revm::primitives::{Address, TxEnv, U256};

/// audit EVM-02: decode exactly ONE EIP-2718 typed-transaction envelope and require
/// the input to be its CANONICAL encoding — the WHOLE buffer consumed AND
/// `encoded_2718() == raw`. `tx_hash` is keccak256 over the raw bytes and keys the
/// `transactions_root`, the receipt/inclusion lookup, the EVM mempool/relay identity,
/// and (for an F002 withdraw) the synthetic withdrawal UTXO outpoint
/// (`synthetic_withdrawal_txid(evm_tx_hash, op_index)`). Without this, `signed‖garbage`
/// (trailing bytes) or a non-canonical RLP would decode to the SAME execution under a
/// DIFFERENT hash — a malleable alias. Closed deterministically at the consensus
/// boundary (admission + execution decode), independent of the decoder's leniency.
fn decode_canonical_2718(raw: &[u8]) -> Result<TxEnvelope, String> {
    let mut buf = raw;
    let envelope = TxEnvelope::decode_2718(&mut buf).map_err(|e| format!("decode: {e}"))?;
    if !buf.is_empty() {
        return Err(format!("non-canonical EVM tx: {} trailing byte(s) after the envelope", buf.len()));
    }
    if envelope.encoded_2718().as_slice() != raw {
        return Err("non-canonical EVM tx encoding (re-encode != raw)".to_string());
    }
    Ok(envelope)
}

/// audit #1/#2: the SHANGHAI tx-type allowlist. Only Legacy, EIP-2930 and
/// EIP-1559 are modelled; newer typed envelopes (EIP-4844, EIP-7702) carry
/// variant-specific semantics this lane does not implement, so they are
/// rejected at admission rather than executed through the shared accessors.
fn is_supported_tx_type(envelope: &TxEnvelope) -> bool {
    matches!(envelope, TxEnvelope::Legacy(_) | TxEnvelope::Eip2930(_) | TxEnvelope::Eip1559(_))
}
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------
// O1 (optimization design v0.1): bounded sender-recovery cache.
//
// `keccak256(raw tx bytes) → recovered signer` is a PURE function, yet the
// same tx's signer is recovered ≥2× on every verifying node (body-validation
// class-1 admission + acceptance execution) and ≥5× on the submitting node
// (mempool admission, template re-admission, template execution, body,
// acceptance) at ~80µs per k256 recovery. Memoizing it is consensus-neutral
// by construction: the value is derived from the key's own preimage, and a
// wrong insert is impossible through this API (insert happens only with the
// result of an actual recovery over the hashed bytes).
//
// Two-generation swap keeps the structure O(1) amortized and bounded at
// 2 × GENERATION_CAP entries with zero eviction bookkeeping.
// ---------------------------------------------------------------------------

const SENDER_CACHE_GENERATION_CAP: usize = 8_192;

struct SenderCache {
    current: HashMap<kaspa_hashes::EvmH256, Address>,
    previous: HashMap<kaspa_hashes::EvmH256, Address>,
}

impl SenderCache {
    fn get(&self, hash: &kaspa_hashes::EvmH256) -> Option<Address> {
        self.current.get(hash).or_else(|| self.previous.get(hash)).copied()
    }

    fn insert(&mut self, hash: kaspa_hashes::EvmH256, sender: Address) {
        if self.current.len() >= SENDER_CACHE_GENERATION_CAP {
            self.previous = std::mem::take(&mut self.current);
        }
        self.current.insert(hash, sender);
    }
}

fn sender_cache() -> &'static Mutex<SenderCache> {
    static CACHE: OnceLock<Mutex<SenderCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SenderCache { current: HashMap::new(), previous: HashMap::new() }))
}

/// Recover (or recall) the signer of `envelope`, memoized under the tx's
/// keccak256 hash. The closure-free split keeps the lock window tiny: lookup,
/// recover OUTSIDE the lock, insert.
fn recover_signer_cached(envelope: &TxEnvelope, hash: kaspa_hashes::EvmH256) -> Result<Address, String> {
    if let Some(sender) = sender_cache().lock().expect("sender cache lock").get(&hash) {
        return Ok(sender);
    }
    let sender = envelope.recover_signer().map_err(|e| format!("signer recovery: {e}"))?;
    sender_cache().lock().expect("sender cache lock").insert(hash, sender);
    Ok(sender)
}

/// Why a raw EVM transaction could not be turned into an executable `TxEnv`.
#[derive(Debug, Clone)]
pub enum TxDecodeError {
    /// EIP-2718 / RLP decoding failed.
    Decode(String),
    /// ECDSA signer recovery failed.
    Recover(String),
}

impl std::fmt::Display for TxDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxDecodeError::Decode(e) => write!(f, "evm tx decode: {e}"),
            TxDecodeError::Recover(e) => write!(f, "evm tx signer recovery: {e}"),
        }
    }
}

/// The Ethereum transaction hash: keccak256 over the raw EIP-2718 bytes.
pub fn tx_hash(raw: &[u8]) -> kaspa_hashes::EvmH256 {
    kaspa_hashes::EvmH256::from_bytes(revm::primitives::keccak256(raw).0)
}

/// Metadata of an admitted EVM transaction (the fields a mempool needs to key,
/// order, replace, and select it). Produced by [`admit_tx_info`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedEvmTx {
    /// keccak256 over the raw EIP-2718 bytes — the Ethereum tx hash.
    pub hash: kaspa_hashes::EvmH256,
    /// Recovered ECDSA signer.
    pub sender: kaspa_consensus_core::evm::EvmAddress,
    pub nonce: u64,
    pub gas_limit: u64,
    /// EIP-1559 `max_fee_per_gas` (legacy/2930: the gas price) — the mempool's
    /// fee-ordering key.
    pub max_fee_per_gas: u128,
    /// EIP-1559 `max_priority_fee_per_gas`. Legacy / EIP-2930 carry no tip field; we
    /// store the gas price (== `max_fee_per_gas`), the geth representation, so the
    /// EFFECTIVE tip `min(priority, max_fee − basefee)` correctly yields their
    /// `gas_price − basefee` (a 0 here would wrongly sink every legacy tx). The
    /// mempool orders by effective tip so a high-`max_fee` zero-tip 1559 tx cannot
    /// outrank a paying one (the miner's revenue is the tip, not the fee ceiling).
    pub max_priority_fee_per_gas: u128,
}

/// v0.4 §6.1 class-1 payload admission (syntactic, per tx): EIP-2718 decode +
/// ECDSA signer recovery + chain-id binding + a declared gas-limit sanity band
/// (≥ the 21k intrinsic floor, +32k for creates; ≤ the per-chain-block accepted
/// gas cap, since a never-acceptable tx is not includable data). Runs at body
/// validation, where a violation invalidates the PAYLOAD block itself — the
/// producer chose its own payload (design v0.4 §6.2). Deterministic and
/// context-free (no state, no basefee: those are class-2 acceptance skips).
///
/// Audit L6 — the floor is deliberately the FIXED intrinsic (21k/53k), NOT the
/// calldata-inclusive EIP-2028/3860 intrinsic: re-implementing revm's exact
/// intrinsic-gas formula here would risk a consensus split if the two ever
/// diverged. A tx with `fixed_floor <= gas_limit < true_intrinsic` therefore
/// passes admission and is rejected by revm at acceptance
/// (`CallGasCostMoreThanGasLimit`) — a DETERMINISTIC class-2 skip, erring on
/// the safe side (a marginal tx skips instead of invalidating the payload
/// block). Design §6.1's class-1 row states this boundary.
pub fn admit_tx(raw: &[u8]) -> Result<(), String> {
    admit_tx_info(raw).map(|_| ())
}

/// [`admit_tx`] returning the admitted tx's metadata (§16 EVM mempool: the
/// SAME rule the body-validation class-1 check applies, so a mempool-admitted
/// tx can never make the node's own template payload-block-invalid).
pub fn admit_tx_info(raw: &[u8]) -> Result<AdmittedEvmTx, String> {
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EVM_CHAIN_ID, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};

    // audit R2-#3: reject a tx that can never fit a payload BEFORE paying the
    // EIP-2718 decode + ECDSA signer-recovery cost (a cheap raw-length gate
    // against RPC/P2P resource exhaustion). Same threshold the mempool's
    // pool-insert uses (empty-payload base + the 4-byte per-tx length prefix).
    // A relay peer announcing such a tx is misbehaving (deterministic class-1).
    let empty_payload_base = EvmExecutionPayload::default().payload_bytes().len();
    if empty_payload_base + 4 + raw.len() > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
        return Err(format!("tx of {} bytes can never fit a payload (cap {MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK})", raw.len()));
    }

    let envelope = decode_canonical_2718(raw)?;
    // audit #1/#2: explicit tx-type allowlist. The pinned spec is SHANGHAI, which
    // supports only Legacy / EIP-2930 / EIP-1559. alloy_consensus can decode newer
    // typed envelopes (EIP-4844 blobs, EIP-7702 auth-lists) whose variant-specific
    // semantics we do NOT model — admitting one and executing it via the common
    // accessors would silently drop those semantics. Reject anything outside the
    // allowlist at admission so it can never enter a payload block.
    if !is_supported_tx_type(&envelope) {
        return Err(format!("unsupported EVM tx type {:#04x} under SHANGHAI (allowed: legacy/EIP-2930/EIP-1559)", envelope.tx_type() as u8));
    }
    let hash = tx_hash(raw);
    let sender = recover_signer_cached(&envelope, hash)?; // O1 memo
    match envelope.chain_id() {
        Some(EVM_CHAIN_ID) => {}
        other => return Err(format!("chain_id {other:?} != EVM_CHAIN_ID {EVM_CHAIN_ID}")),
    }
    let intrinsic_floor = if envelope.kind().is_create() { 53_000 } else { 21_000 };
    if envelope.gas_limit() < intrinsic_floor {
        return Err(format!("gas_limit {} below the intrinsic floor {intrinsic_floor}", envelope.gas_limit()));
    }
    if envelope.gas_limit() > MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK {
        return Err(format!(
            "gas_limit {} exceeds MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK {MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK}",
            envelope.gas_limit()
        ));
    }
    Ok(AdmittedEvmTx {
        hash,
        sender: EvmAddress::from_bytes(sender.into_array()),
        nonce: envelope.nonce(),
        gas_limit: envelope.gas_limit(),
        max_fee_per_gas: envelope.max_fee_per_gas(),
        // Legacy / EIP-2930 carry no priority field → tip is `gas_price − basefee`.
        // The geth representation sets tipCap = feeCap = gas_price so the effective-tip
        // formula `min(priority, max_fee − basefee)` reduces to `gas_price − basefee`.
        max_priority_fee_per_gas: envelope.max_priority_fee_per_gas().unwrap_or_else(|| envelope.max_fee_per_gas()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O1: the memoized recovery returns the identical sender as a direct
    /// recovery, and the two-generation eviction keeps the cache bounded
    /// without ever serving a wrong value (the value is key-derived).
    #[test]
    fn sender_cache_is_transparent_and_bounded() {
        let raw = fixture_raw(0);
        let envelope = TxEnvelope::decode_2718(&mut &raw[..]).unwrap();
        let direct = envelope.recover_signer().unwrap();
        let h = tx_hash(&raw);
        // Twice: miss-then-recover, then pure cache hit — identical result.
        assert_eq!(recover_signer_cached(&envelope, h).unwrap(), direct);
        assert_eq!(recover_signer_cached(&envelope, h).unwrap(), direct);
        assert_eq!(sender_cache().lock().unwrap().get(&h), Some(direct));

        // Generation swap: flood 2×CAP synthetic entries; the structure stays
        // bounded at ≤ 2×CAP and old entries fall out after two swaps.
        {
            let mut cache = sender_cache().lock().unwrap();
            for i in 0..(2 * SENDER_CACHE_GENERATION_CAP) {
                let mut k = [0u8; 32];
                k[..8].copy_from_slice(&(i as u64).to_le_bytes());
                k[31] = 0xEE;
                cache.insert(kaspa_hashes::EvmH256::from_bytes(k), Address::ZERO);
            }
            assert!(cache.current.len() + cache.previous.len() <= 2 * SENDER_CACHE_GENERATION_CAP);
            assert!(cache.get(&h).is_none(), "the fixture's entry aged out after two generation swaps");
        }
        // And a re-lookup after eviction simply re-recovers — still identical.
        assert_eq!(recover_signer_cached(&envelope, h).unwrap(), direct);
    }

    fn fixture_raw(nonce: u64) -> Vec<u8> {
        use alloy_consensus::{SignableTransaction, TxEip1559};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
        use revm::primitives::{Address, TxKind, B256, U256};
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit: 21_000,
            max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(Address::with_last_byte(0x22)),
            value: U256::from(500u64),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        TxEnvelope::from(tx.into_signed(sig)).encoded_2718()
    }

    /// audit #1: a non-empty EIP-2930/1559 access list is carried into the
    /// `TxEnv` (so revm charges the exact access-list intrinsic gas and warms
    /// the listed slots), and an empty list maps to an empty `TxEnv` list.
    #[test]
    fn access_list_is_carried_into_txenv() {
        use alloy_consensus::{SignableTransaction, TxEip1559};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_eips::eip2930::{AccessList, AccessListItem};
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
        use revm::primitives::{Address, TxKind, B256, U256};
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let al = AccessList(vec![AccessListItem {
            address: Address::with_last_byte(0xAB),
            storage_keys: vec![B256::from([0x01u8; 32]), B256::from([0x02u8; 32])],
        }]);
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID, nonce: 0, gas_limit: 60_000,
            max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128, max_priority_fee_per_gas: 0,
            to: TxKind::Call(Address::with_last_byte(0x22)), value: U256::from(1u64),
            access_list: al, input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        let raw = TxEnvelope::from(tx.into_signed(sig)).encoded_2718();
        let env = decode_tx_to_env(&raw).unwrap();
        assert_eq!(env.access_list.len(), 1, "the access list must reach the TxEnv");
        assert_eq!(env.access_list[0].storage_keys.len(), 2);
        // An empty-access-list tx maps to an empty TxEnv list (the common case).
        let env0 = decode_tx_to_env(&fixture_raw(0)).unwrap();
        assert!(env0.access_list.is_empty());
    }

    #[test]
    fn admit_tx_info_extracts_the_mempool_metadata() {
        let raw = fixture_raw(7);
        let info = admit_tx_info(&raw).unwrap();
        assert_eq!(info.nonce, 7);
        assert_eq!(info.gas_limit, 21_000);
        assert_eq!(info.max_fee_per_gas, kaspa_consensus_core::evm::EVM_INITIAL_BASE_FEE as u128);
        assert_eq!(info.hash.as_bytes(), revm::primitives::keccak256(&raw).0, "Ethereum tx hash = keccak256(raw 2718 bytes)");
        // admit_tx and admit_tx_info enforce the identical rule.
        assert!(admit_tx(&raw).is_ok());
        // A truncated tx is inadmissible, not a panic.
        assert!(admit_tx_info(&raw[..raw.len() - 5]).is_err());
    }

    /// audit EVM-02: a canonical signed tx admits and decodes; the same envelope
    /// with TRAILING bytes is a hash-malleable alias of the same execution, so both
    /// the admission gate and the execution decode must reject it (not silently
    /// decode the envelope and ignore the suffix).
    #[test]
    fn rejects_trailing_bytes_canonical_only() {
        let raw = fixture_raw(3);
        assert!(admit_tx_info(&raw).is_ok());
        assert!(decode_tx_to_env(&raw).is_ok());

        let mut with_suffix = raw.clone();
        with_suffix.push(0x00);
        // keccak(raw‖0x00) != keccak(raw): a different hash for the same execution.
        assert_ne!(tx_hash(&with_suffix), tx_hash(&raw));
        assert!(admit_tx_info(&with_suffix).is_err(), "trailing bytes must be inadmissible");
        assert!(decode_tx_to_env(&with_suffix).is_err(), "trailing bytes must fail the execution decode too");
        // The pure helper rejects it directly.
        assert!(decode_canonical_2718(&with_suffix).is_err());
        assert!(decode_canonical_2718(&raw).is_ok());
    }

    /// Prints the canonical signed-tx fixture used by the consensus §16 e2e
    /// test (consensus has no signing deps, so it embeds these bytes as hex).
    /// Regenerate with:
    ///   cargo test -p kaspa-evm fixture_generator -- --ignored --nocapture
    #[test]
    #[ignore = "fixture generator, run with --ignored --nocapture"]
    fn fixture_generator() {
        for nonce in [0u64, 1] {
            let raw = fixture_raw(nonce);
            println!("nonce {nonce}: {}", alloy_primitives::hex::encode(&raw));
        }
    }
}

/// Decode one EIP-2718 typed-transaction byte string and map it to a revm
/// `TxEnv` (recovering the sender). Deterministic: the same bytes always yield
/// the same caller + env.
pub fn decode_tx_to_env(raw: &[u8]) -> Result<TxEnv, TxDecodeError> {
    // audit EVM-02: same canonical-encoding gate as admission (defense-in-depth on
    // the execution path; a body-valid payload already only contains admitted txs).
    let envelope = decode_canonical_2718(raw).map_err(TxDecodeError::Decode)?;
    // audit #1/#2: defense-in-depth allowlist (admission already enforced it).
    // A body-valid payload can only contain admitted txs, but the executor must
    // never run an out-of-allowlist envelope through the common accessors.
    if !is_supported_tx_type(&envelope) {
        return Err(TxDecodeError::Decode(format!("unsupported EVM tx type {:#04x} under SHANGHAI", envelope.tx_type() as u8)));
    }
    // O1 memo: the acceptance-execution path re-recovers the same signer body
    // validation already recovered — the keccak (sub-µs) buys back ~80µs.
    let caller = recover_signer_cached(&envelope, tx_hash(raw)).map_err(TxDecodeError::Recover)?;

    let mut tx = TxEnv::default();
    tx.caller = caller;
    tx.gas_limit = envelope.gas_limit();
    // For EIP-1559, `gas_price` carries the max fee and `gas_priority_fee` the
    // priority tip; for legacy/2930, `max_fee_per_gas` returns the gas price and
    // the priority fee is `None`.
    tx.gas_price = U256::from(envelope.max_fee_per_gas());
    tx.gas_priority_fee = envelope.max_priority_fee_per_gas().map(U256::from);
    tx.transact_to = envelope.kind();
    tx.value = envelope.value();
    tx.data = envelope.input().clone();
    tx.nonce = Some(envelope.nonce());
    tx.chain_id = envelope.chain_id();
    // audit #1: carry the EIP-2930/1559 access list so revm charges the exact
    // access-list intrinsic gas and warms the listed addresses/slots. revm
    // re-exports alloy's `AccessListItem`, so the envelope's list maps directly.
    // Empty for legacy and for the common transfers/calls — a no-op there.
    if let Some(access_list) = envelope.access_list() {
        tx.access_list = access_list.0.clone();
    }
    Ok(tx)
}

/// Decoded fields of an EVM tx for the eth-rpc adapter (`eth_getTransactionByHash`
/// + `eth_getTransactionReceipt` `from`/`to`/`contractAddress`). Primitive output
/// (no revm/alloy types) so the thin `rpc/eth` crate stays secp/revm-free; the
/// node-side provider calls this under kaspad's `evm` feature.
#[derive(Clone, Debug)]
pub struct DecodedEthTx {
    pub hash: kaspa_hashes::EvmH256,
    pub from: [u8; 20],
    /// `None` ⇒ contract creation.
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    /// Call value in wei, big-endian 32 bytes.
    pub value: [u8; 32],
    pub gas_limit: u64,
    /// EIP-1559 max fee (legacy/2930: the gas price).
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: Option<u128>,
    pub input: Vec<u8>,
    /// 0 = legacy, 1 = EIP-2930, 2 = EIP-1559.
    pub tx_type: u8,
    pub chain_id: Option<u64>,
    /// `CREATE(from, nonce)` for a creation, else `None`.
    pub contract_address: Option<[u8; 20]>,
}

/// Decode + recover a raw EIP-2718 tx into [`DecodedEthTx`] for the eth-rpc adapter.
pub fn decode_eth_tx(raw: &[u8]) -> Result<DecodedEthTx, TxDecodeError> {
    let envelope = decode_canonical_2718(raw).map_err(TxDecodeError::Decode)?;
    if !is_supported_tx_type(&envelope) {
        return Err(TxDecodeError::Decode(format!("unsupported EVM tx type {:#04x}", envelope.tx_type() as u8)));
    }
    let hash = tx_hash(raw);
    let from_addr = recover_signer_cached(&envelope, hash).map_err(TxDecodeError::Recover)?;
    let nonce = envelope.nonce();
    let to = match envelope.kind() {
        revm::primitives::TxKind::Call(a) => Some(a.into_array()),
        revm::primitives::TxKind::Create => None,
    };
    let contract_address = if to.is_none() { Some(from_addr.create(nonce).into_array()) } else { None };
    Ok(DecodedEthTx {
        hash,
        from: from_addr.into_array(),
        to,
        nonce,
        value: envelope.value().to_be_bytes::<32>(),
        gas_limit: envelope.gas_limit(),
        max_fee_per_gas: envelope.max_fee_per_gas(),
        max_priority_fee_per_gas: envelope.max_priority_fee_per_gas(),
        input: envelope.input().to_vec(),
        tx_type: envelope.tx_type() as u8,
        chain_id: envelope.chain_id(),
        contract_address,
    })
}
