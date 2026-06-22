//! EVM-lane KEYED commands (Tier B, behind the `evm-send` feature). The EVM
//! lane is secp256k1 + EIP-1559, so this module (and its deps) are feature-gated
//! to keep the default `misaka` build secp-free.
//!
//!   misaka evm wallet create  --out evm.mnemonic
//!   misaka evm wallet import   --out evm.mnemonic   (phrase on stdin)
//!   misaka evm wallet address  --mnemonic-file evm.mnemonic
//!   misaka evm send --mnemonic-file evm.mnemonic --to 0x… --amount 1.5 [--yes] [--wait]
//!
//! Signing copies the consensus-tested path in kaspa-evm/examples/evm_tx_gen.rs
//! (TxEip1559 → PrivateKeySigner → EIP-2718 encode). Submit reuses the eth.rs
//! hand-rolled JSON-RPC client. Keys derive via kaspa-bip32 (BIP-39 + BIP-44
//! m/44'/60'/0'/0/0), matching docs/misaka-evm-wallet-profile-v1.md and the
//! chrome-extension wallet byte-for-byte. Default dry-run; live send needs --yes.

use std::io::{Read, Write};
use std::str::FromStr;

use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use kaspa_bip32::{secp256k1::SecretKey, DerivationPath, ExtendedPrivateKey, Language, Mnemonic, WordCount};
use kaspa_consensus_core::evm::EVM_CHAIN_ID;
use serde_json::json;
use zeroize::Zeroizing;

use crate::node::Ctx;
use crate::{exit, CliError, CliResult, OutputFormat};

const WEI_PER_MSK: u128 = 1_000_000_000_000_000_000;
const HD_PATH: &str = "m/44'/60'/0'/0/0";

/// Where the EVM secp256k1 key comes from. The secret is NEVER a CLI value.
pub struct EvmKeySource {
    pub mnemonic_file: Option<String>,
    pub key_file: Option<String>,
    pub key_stdin: bool,
}

impl EvmKeySource {
    /// Resolve the 32-byte secp256k1 secret. Returned in a `Zeroizing` wrapper so
    /// the buffer is wiped on drop (audit M-07); it `Deref`s to `[u8; 32]`.
    pub fn resolve(&self) -> Result<Zeroizing<[u8; 32]>, CliError> {
        match (&self.mnemonic_file, &self.key_file, self.key_stdin) {
            (Some(p), None, false) => derive_from_mnemonic(read_trimmed(p)?.trim()),
            (None, Some(p), false) => decode_key_hex(read_trimmed(p)?.trim()),
            (None, None, true) => {
                // stdin may carry a mnemonic OR a hex key; wipe it on drop.
                let mut s = Zeroizing::new(String::new());
                std::io::stdin().read_to_string(&mut s).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("stdin: {e}")))?;
                let t = s.trim();
                if t.split_whitespace().count() >= 12 {
                    derive_from_mnemonic(t)
                } else {
                    decode_key_hex(t)
                }
            }
            (None, None, false) => Err(CliError::new(
                exit::WALLET_LOCKED,
                "no EVM key source — pass --mnemonic-file / --key-file / --key-stdin (never a key on the command line)".to_string(),
            )),
            _ => Err(CliError::new(exit::GENERIC, "specify exactly ONE EVM key source".to_string())),
        }
    }

    pub fn signer(&self) -> Result<PrivateKeySigner, CliError> {
        let k = self.resolve()?;
        PrivateKeySigner::from_bytes(&B256::from(*k)).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("bad EVM key: {e}")))
    }

    pub fn address(&self) -> Result<Address, CliError> {
        Ok(self.signer()?.address())
    }
}

/// Read a key/mnemonic file, wiping the buffer on drop. Before reading, refuse a
/// non-regular file (symlink/device/fifo — `symlink_metadata` does NOT follow the
/// link) and warn on group/world-readable perms (audit M-07; stricter than, and
/// consistent with, validator-core `load_validator_seed`).
fn read_trimmed(path: &str) -> Result<Zeroizing<String>, CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("stat {path}: {e}")))?;
        if !meta.file_type().is_file() {
            return Err(CliError::new(
                exit::WALLET_LOCKED,
                format!("key file {path} is not a regular file (symlink/device/fifo refused)"),
            ));
        }
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            eprintln!("warning: key file {path} is group/world-accessible (mode {mode:o}); restrict it to 0600");
        }
    }
    std::fs::read_to_string(path).map(Zeroizing::new).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("read {path}: {e}")))
}

fn derive_from_mnemonic(phrase: &str) -> Result<Zeroizing<[u8; 32]>, CliError> {
    let m = Mnemonic::new(phrase, Language::English).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("invalid mnemonic: {e}")))?;
    // bip32 `Seed` already zeroizes on drop; the derived secret is wrapped below.
    let seed = m.to_seed("");
    let xprv = ExtendedPrivateKey::<SecretKey>::new(seed).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("xprv: {e}")))?;
    let path = DerivationPath::from_str(HD_PATH).map_err(|e| CliError::new(exit::GENERIC, format!("path: {e}")))?;
    let child = xprv.derive_path(&path).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("derive {HD_PATH}: {e}")))?;
    Ok(Zeroizing::new(child.private_key().secret_bytes()))
}

fn decode_key_hex(s: &str) -> Result<Zeroizing<[u8; 32]>, CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(exit::WALLET_LOCKED, "EVM key must be 64 hex chars (32-byte secp256k1)".to_string()));
    }
    let mut k = Zeroizing::new([0u8; 32]);
    faster_hex::hex_decode(h.as_bytes(), &mut *k).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("bad key hex: {e}")))?;
    Ok(k)
}

// ---------------------------------------------------------------------------
// evm wallet create / import / address
// ---------------------------------------------------------------------------

pub fn wallet_create(ctx: &Ctx, out: &str) -> CliResult {
    let m = Mnemonic::random(WordCount::Words24, Language::English)
        .map_err(|e| CliError::new(exit::GENERIC, format!("mnemonic gen: {e}")))?;
    let phrase = m.phrase();
    write_secret_file(out, phrase.as_bytes())?;
    let addr = derive_from_mnemonic(phrase).and_then(|k| {
        PrivateKeySigner::from_bytes(&B256::from(*k)).map(|s| s.address()).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))
    })?;
    match ctx.output {
        OutputFormat::Human => {
            println!("Wrote a new 24-word BIP-39 mnemonic to {out} (mode 0600). BACK IT UP — it cannot be recovered.");
            println!("EVM address: {addr}");
        }
        OutputFormat::Json => println!("{}", json!({ "ok": true, "file": out, "address": addr.to_string() })),
    }
    Ok(())
}

pub fn wallet_import(ctx: &Ctx, out: &str) -> CliResult {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("stdin: {e}")))?;
    let phrase = s.trim();
    // validate before writing
    let addr = derive_from_mnemonic(phrase).and_then(|k| {
        PrivateKeySigner::from_bytes(&B256::from(*k)).map(|s| s.address()).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))
    })?;
    write_secret_file(out, phrase.as_bytes())?;
    match ctx.output {
        OutputFormat::Human => {
            println!("Imported the mnemonic to {out} (mode 0600).");
            println!("EVM address: {addr}");
        }
        OutputFormat::Json => println!("{}", json!({ "ok": true, "file": out, "address": addr.to_string() })),
    }
    Ok(())
}

pub fn wallet_address(ctx: &Ctx, ks: &EvmKeySource) -> CliResult {
    let addr = ks.address()?;
    match ctx.output {
        OutputFormat::Human => println!("{addr}"),
        OutputFormat::Json => println!("{}", json!({ "ok": true, "address": addr.to_string() })),
    }
    Ok(())
}

fn write_secret_file(out: &str, bytes: &[u8]) -> Result<(), CliError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(out)
        .map_err(|e| CliError::new(exit::GENERIC, format!("create {out}: {e} (refusing to overwrite)")))?;
    f.write_all(bytes).map_err(|e| CliError::new(exit::GENERIC, format!("write {out}: {e}")))?;
    f.write_all(b"\n").ok();
    Ok(())
}

// ---------------------------------------------------------------------------
// evm send
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn send(
    ctx: &Ctx,
    ks: &EvmKeySource,
    to: &str,
    amount_wei: u128,
    gas_limit: Option<u64>,
    max_fee: Option<u128>,
    nonce_override: Option<u64>,
    yes: bool,
    wait: bool,
) -> CliResult {
    let signer = ks.signer()?;
    let from = signer.address();
    let from_s = from.to_string();
    let to_addr = parse_dest(to)?;

    // chain-id guard
    let cid = crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_chainId", json!([]))?)? as u64;
    if cid != EVM_CHAIN_ID {
        return Err(CliError::new(exit::NETWORK_MISMATCH, format!("node chainId 0x{cid:x} != expected 0x{EVM_CHAIN_ID:x}")));
    }

    // nonce: pending tag (so back-to-back sends increment correctly)
    let nonce = match nonce_override {
        Some(n) => n,
        None => crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_getTransactionCount", json!([from_s, "pending"]))?)? as u64,
    };
    // max_fee: from the head base fee (eth_gasPrice); no priority market.
    let max_fee = match max_fee {
        Some(f) => f,
        None => crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_gasPrice", json!([]))?)?,
    };
    // gas: eth_estimateGas unless overridden.
    let gas = match gas_limit {
        Some(g) => g,
        None => {
            let call = json!({ "from": from_s, "to": to_addr.to_string(), "value": format!("0x{amount_wei:x}") });
            crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_estimateGas", json!([call]))?)? as u64
        }
    };

    let raw = sign_eip1559(&*ks.resolve()?, nonce, gas, max_fee, TxKind::Call(to_addr), U256::from(amount_wei), Bytes::new())?;
    let raw_hex = format!("0x{}", faster_hex::hex_string(&raw));

    let submit = yes;
    let txid = if submit {
        Some(
            crate::eth::rpc_call(ctx, "eth_sendRawTransaction", json!([raw_hex]))?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| CliError::new(exit::TX_REJECTED, "eth_sendRawTransaction: no tx hash".to_string()))?,
        )
    } else {
        None
    };

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "dryRun": !submit, "from": from_s, "to": to_addr.to_string(),
                    "amountWei": amount_wei.to_string(), "nonce": nonce, "gas": gas, "maxFeePerGas": max_fee.to_string(), "txid": txid })
        ),
        OutputFormat::Human => {
            println!("From   : {from_s}");
            println!("To     : {to_addr}");
            println!("Amount : {} MSK  ({amount_wei} wei)", format_msk(amount_wei));
            println!("Nonce  : {nonce}   Gas: {gas}   MaxFee: {max_fee} wei/gas");
            println!("Mode   : {}", if submit { "SUBMIT" } else { "dry-run (no broadcast; pass --yes)" });
            if let Some(t) = &txid {
                println!("Txid   : {t}");
            }
        }
    }
    if submit && wait {
        if let Some(t) = &txid {
            return crate::eth::tx_wait(ctx, t, 600, 2);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// evm deploy / evm call  — raw contract-creation and raw calldata. The CLI is
// deliberately ABI-agnostic: produce the init code / calldata with `forge` /
// `cast` (or the misaka-nft helper) and submit it here. Keeps the CLI free of a
// Solidity ABI encoder while still giving operators a keyed deploy/mint path.
// ---------------------------------------------------------------------------

/// Deploy a contract: an EIP-1559 contract-creation tx whose input is `init_code`
/// (creation bytecode with the ABI-encoded constructor args already appended).
/// Prints the deterministic CREATE address (keccak(rlp[sender, nonce])[12:]).
#[allow(clippy::too_many_arguments)]
pub fn deploy(
    ctx: &Ctx,
    ks: &EvmKeySource,
    init_code: Vec<u8>,
    value_wei: u128,
    gas_limit: Option<u64>,
    max_fee: Option<u128>,
    nonce_override: Option<u64>,
    yes: bool,
    wait: bool,
) -> CliResult {
    if init_code.is_empty() {
        return Err(CliError::new(exit::GENERIC, "empty init code — nothing to deploy".to_string()));
    }
    let from = ks.address()?;
    let from_s = from.to_string();
    guard_chain_id(ctx)?;

    let nonce = resolve_nonce(ctx, &from_s, nonce_override)?;
    let max_fee = resolve_fee(ctx, max_fee)?;
    let input = Bytes::from(init_code);
    let gas = match gas_limit {
        Some(g) => g,
        None => {
            let call = json!({ "from": from_s, "data": format!("0x{}", faster_hex::hex_string(&input)), "value": format!("0x{value_wei:x}") });
            crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_estimateGas", json!([call]))?)? as u64
        }
    };
    // Deterministic contract address from sender + this nonce.
    let contract = from.create(nonce);

    let raw = sign_eip1559(&*ks.resolve()?, nonce, gas, max_fee, TxKind::Create, U256::from(value_wei), input.clone())?;
    let raw_hex = format!("0x{}", faster_hex::hex_string(&raw));

    let txid = submit_if(ctx, yes, &raw_hex)?;

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "dryRun": !yes, "from": from_s, "contractAddress": contract.to_string(),
                    "initCodeLen": input.len(), "nonce": nonce, "gas": gas, "maxFeePerGas": max_fee.to_string(),
                    "valueWei": value_wei.to_string(), "txid": txid })
        ),
        OutputFormat::Human => {
            println!("From     : {from_s}");
            println!("Contract : {contract}   (deterministic from sender+nonce)");
            println!("InitCode : {} bytes", input.len());
            println!("Nonce    : {nonce}   Gas: {gas}   MaxFee: {max_fee} wei/gas");
            println!("Mode     : {}", if yes { "SUBMIT" } else { "dry-run (no broadcast; pass --yes)" });
            if let Some(t) = &txid {
                println!("Txid     : {t}");
            }
        }
    }
    if yes && wait {
        if let Some(t) = &txid {
            return crate::eth::tx_wait(ctx, t, 600, 2);
        }
    }
    Ok(())
}

/// Call a contract with raw `data` (selector + ABI-encoded args). Optional value.
#[allow(clippy::too_many_arguments)]
pub fn call(
    ctx: &Ctx,
    ks: &EvmKeySource,
    to: &str,
    data: Vec<u8>,
    value_wei: u128,
    gas_limit: Option<u64>,
    max_fee: Option<u128>,
    nonce_override: Option<u64>,
    yes: bool,
    wait: bool,
) -> CliResult {
    let from = ks.address()?;
    let from_s = from.to_string();
    let to_addr = parse_dest(to)?;
    guard_chain_id(ctx)?;

    let nonce = resolve_nonce(ctx, &from_s, nonce_override)?;
    let max_fee = resolve_fee(ctx, max_fee)?;
    let input = Bytes::from(data);
    let data_hex = format!("0x{}", faster_hex::hex_string(&input));
    let gas = match gas_limit {
        Some(g) => g,
        None => {
            let call = json!({ "from": from_s, "to": to_addr.to_string(), "data": data_hex, "value": format!("0x{value_wei:x}") });
            crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_estimateGas", json!([call]))?)? as u64
        }
    };

    let raw = sign_eip1559(&*ks.resolve()?, nonce, gas, max_fee, TxKind::Call(to_addr), U256::from(value_wei), input.clone())?;
    let raw_hex = format!("0x{}", faster_hex::hex_string(&raw));

    let txid = submit_if(ctx, yes, &raw_hex)?;

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "dryRun": !yes, "from": from_s, "to": to_addr.to_string(),
                    "dataLen": input.len(), "nonce": nonce, "gas": gas, "maxFeePerGas": max_fee.to_string(),
                    "valueWei": value_wei.to_string(), "txid": txid })
        ),
        OutputFormat::Human => {
            println!("From   : {from_s}");
            println!("To     : {to_addr}");
            println!("Data   : {} bytes", input.len());
            println!("Value  : {} MSK  ({value_wei} wei)", format_msk(value_wei));
            println!("Nonce  : {nonce}   Gas: {gas}   MaxFee: {max_fee} wei/gas");
            println!("Mode   : {}", if yes { "SUBMIT" } else { "dry-run (no broadcast; pass --yes)" });
            if let Some(t) = &txid {
                println!("Txid   : {t}");
            }
        }
    }
    if yes && wait {
        if let Some(t) = &txid {
            return crate::eth::tx_wait(ctx, t, 600, 2);
        }
    }
    Ok(())
}

fn guard_chain_id(ctx: &Ctx) -> Result<(), CliError> {
    let cid = crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_chainId", json!([]))?)? as u64;
    if cid != EVM_CHAIN_ID {
        return Err(CliError::new(exit::NETWORK_MISMATCH, format!("node chainId 0x{cid:x} != expected 0x{EVM_CHAIN_ID:x}")));
    }
    Ok(())
}

fn resolve_nonce(ctx: &Ctx, from_s: &str, nonce_override: Option<u64>) -> Result<u64, CliError> {
    match nonce_override {
        Some(n) => Ok(n),
        None => Ok(crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_getTransactionCount", json!([from_s, "pending"]))?)? as u64),
    }
}

fn resolve_fee(ctx: &Ctx, max_fee: Option<u128>) -> Result<u128, CliError> {
    match max_fee {
        Some(f) => Ok(f),
        None => crate::eth::parse_hex_u128(&crate::eth::rpc_call(ctx, "eth_gasPrice", json!([]))?),
    }
}

fn submit_if(ctx: &Ctx, yes: bool, raw_hex: &str) -> Result<Option<String>, CliError> {
    if !yes {
        return Ok(None);
    }
    Ok(Some(
        crate::eth::rpc_call(ctx, "eth_sendRawTransaction", json!([raw_hex]))?
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| CliError::new(exit::TX_REJECTED, "eth_sendRawTransaction: no tx hash".to_string()))?,
    ))
}

/// Resolve a hex blob (init code or calldata) from an inline `--…` hex string or
/// a `--…-file` path. Tolerates `0x`, whitespace, and newlines.
pub fn read_hex_blob(inline: &Option<String>, file: &Option<String>) -> Result<Vec<u8>, CliError> {
    let raw = match (inline, file) {
        (Some(s), None) => s.clone(),
        (None, Some(p)) => std::fs::read_to_string(p).map_err(|e| CliError::new(exit::GENERIC, format!("read {p}: {e}")))?,
        (Some(_), Some(_)) => return Err(CliError::new(exit::GENERIC, "pass either the inline hex OR the --…-file, not both".to_string())),
        (None, None) => return Err(CliError::new(exit::GENERIC, "no hex blob — pass the inline hex or a --…-file".to_string())),
    };
    let cleaned: String = raw.split_whitespace().collect();
    let h = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")).unwrap_or(&cleaned);
    if h.is_empty() {
        return Ok(Vec::new());
    }
    if h.len() % 2 != 0 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(exit::GENERIC, "blob is not valid even-length hex".to_string()));
    }
    let mut out = vec![0u8; h.len() / 2];
    faster_hex::hex_decode(h.as_bytes(), &mut out).map_err(|e| CliError::new(exit::GENERIC, format!("bad hex: {e}")))?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn sign_eip1559(
    key32: &[u8; 32],
    nonce: u64,
    gas_limit: u64,
    max_fee: u128,
    kind: TxKind,
    value: U256,
    input: Bytes,
) -> Result<Vec<u8>, CliError> {
    let signer = PrivateKeySigner::from_bytes(&B256::from(*key32)).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("signer: {e}")))?;
    let tx = TxEip1559 {
        chain_id: EVM_CHAIN_ID,
        nonce,
        gas_limit,
        max_fee_per_gas: max_fee,
        max_priority_fee_per_gas: 0,
        to: kind,
        value,
        access_list: Default::default(),
        input,
    };
    let sig = signer.sign_hash_sync(&tx.signature_hash()).map_err(|e| CliError::new(exit::GENERIC, format!("sign: {e}")))?;
    Ok(TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
}

/// Parse + validate a destination address. Rejects the zero address and warns
/// on system / precompile addresses (a common fat-finger that burns funds).
fn parse_dest(s: &str) -> Result<Address, CliError> {
    let addr = Address::from_str(s).map_err(|e| CliError::new(exit::GENERIC, format!("bad --to address: {e}")))?;
    if addr == Address::ZERO {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "refusing to send to the zero address".to_string()));
    }
    let bytes = addr.into_array();
    let low = bytes[..19].iter().all(|b| *b == 0);
    if low && (1..=9).contains(&bytes[19]) {
        eprintln!("warning: {addr} is an EVM precompile address — funds sent here are unrecoverable");
    }
    Ok(addr)
}

fn format_msk(wei: u128) -> String {
    let whole = wei / WEI_PER_MSK;
    let frac = wei % WEI_PER_MSK;
    if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{}", format!("{frac:018}").trim_end_matches('0'))
    }
}

/// Parse a decimal MSK string into wei (1 MSK = 1e18 wei).
pub fn parse_msk_to_wei(s: &str) -> Result<u128, CliError> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    if frac.len() > 18 || !frac.bytes().all(|b| b.is_ascii_digit()) || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CliError::new(exit::GENERIC, format!("invalid amount '{s}' (MSK, max 18 decimals)")));
    }
    let whole: u128 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| CliError::new(exit::GENERIC, format!("invalid amount '{s}'")))? };
    let frac_wei: u128 = format!("{frac:0<18}").parse().map_err(|_| CliError::new(exit::GENERIC, format!("invalid amount '{s}'")))?;
    whole.checked_mul(WEI_PER_MSK).and_then(|w| w.checked_add(frac_wei)).ok_or_else(|| CliError::new(exit::GENERIC, "amount overflow".to_string()))
}
