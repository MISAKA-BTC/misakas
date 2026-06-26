//! PREA PQ smart-account signer (Tier B, behind the `evm-send` feature).
//!
//! Produces the two authorization signatures the `MisakaPqSmartAccount` consumes,
//! matching the Solidity contract byte-for-byte (PREA design v1.1 §13/§14/§16):
//!
//!   misaka prea sign-root    --key-file <ml-dsa-seed> --account 0x… --version 1
//!         --nonce 0 --valid-after 0 --valid-until <n> --max-relayer-fee 0
//!         --to 0x… --value <wei> --calldata 0x…
//!   misaka prea sign-session --mnemonic-file <secp> --account 0x… --version 1
//!         --to 0x… --value <wei> --call-index 0 --max-relayer-fee 0 --calldata 0x…
//!
//! `sign-root` signs an `executeRoot` op with the ML-DSA-87 Operational Root key and
//! emits the F003 v0x02 precompile input + the ready-to-submit `executeRoot(...)`
//! calldata. `sign-session` signs an `executeSession` op with a restricted secp256k1
//! session key (EIP-2 low-s, v∈{27,28}) and emits the 65-byte `r‖s‖v` + the
//! `executeSession(...)` calldata. The secret is NEVER a CLI value (file/stdin only).

use std::str::FromStr;
use std::time::Duration;

use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use alloy_signer::SignerSync;
use alloy_sol_types::{SolCall, sol};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::evm::{EVM_CHAIN_ID, F003_PREA_OP_MLDSA87_CONTEXT, F003_PREA_ROOT_MLDSA87_CONTEXT, F003_VERSION_PREA_ROOT};
use kaspa_consensus_core::network::NetworkId;
use kaspa_hashes::{blake2b_512_address_payload, blake2b_512_keyed};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use serde_json::json;

use crate::evm_send::EvmKeySource;
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

/// Audit H-3: the F003 `MLDSA87_VERIFY` precompile is activation-fenced
/// (`evm_f003_mldsa_verify_activation_daa_score`, `u64::MAX` ⇒ inert on every
/// network today). While F003 is inactive the precompile is NOT registered, so a
/// `0x…F003` call behaves as an empty account — every PREA PQ-smart-account ROOT
/// (ML-DSA-87) authorization this signer produces is INOPERABLE: the account's
/// `executeRoot` would revert because the signature can never verify. Refuse the
/// op up front (clear error) rather than emitting an authorization that cannot be
/// submitted, so the operator does not burn a root nonce / relayer fee against a
/// fence. `virtual_daa` is the connected node's consensus virtual DAA score.
pub fn ensure_f003_active(network: NetworkId, virtual_daa: u64) -> CliResult {
    let activation = Params::from(network).evm_f003_mldsa_verify_activation_daa_score;
    if virtual_daa < activation {
        let when = if activation == u64::MAX {
            "F003 is fence-inert on this network (no activation DAA score is scheduled)".to_string()
        } else {
            format!("F003 activates at DAA score {activation}; the node is at {virtual_daa}")
        };
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!(
                "refusing PREA root op: the ML-DSA-87 F003 precompile is NOT active on '{network}' — \
                 {when}. An executeRoot authorization produced now CANNOT be verified on-chain (the \
                 precompile is unregistered, so the account would revert). Retry once F003 is active."
            ),
        ));
    }
    Ok(())
}

/// Connect to the node's wRPC, confirm it matches `--network`, and refuse the PREA
/// root op unless F003 is active at the node's current virtual DAA score (audit
/// H-3). Mirrors `wallet::connect`'s endpoint resolution + network-match guard.
pub async fn gate_root_op(ctx: &Ctx) -> CliResult {
    let net = NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let hostport = ctx.rpc.clone().unwrap_or_else(|| format!("127.0.0.1:{}", net.network_type().default_borsh_rpc_port()));
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(ctx.timeout_secs.clamp(2, 15))),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client
        .connect(Some(options))
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e} (node up with --rpclisten-borsh?)")))?;
    let server = client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo: {e}")))?;
    let _ = client.disconnect().await;
    if server.network_id.to_string() != ctx.network {
        return Err(CliError::new(
            exit::NETWORK_MISMATCH,
            format!("node is '{}' but --network is '{}'", server.network_id, ctx.network),
        ));
    }
    ensure_f003_active(server.network_id, server.virtual_daa_score)
}

sol! {
    function executeRoot(
        address target,
        uint256 value,
        bytes callData,
        uint64 validAfterBlock,
        uint64 validUntilBlock,
        uint64 nonce,
        bytes publicKey,
        bytes signature,
        uint256 maxRelayerFee
    ) external returns (bytes);

    function executeSession(
        address target,
        uint256 value,
        bytes callData,
        uint64 callIndex,
        bytes ecdsaSig,
        uint256 maxRelayerFee
    ) external returns (bytes);
}

/// Fields common to a root op preimage / session op hash.
pub struct OpFields {
    pub account: Address,
    pub version: u64,
    pub target: Address,
    pub value: U256,
    pub call_data: Vec<u8>,
    pub max_relayer_fee: U256,
}

/// `executeRoot`'s canonical preimage — the exact `abi.encodePacked(...)` the contract
/// builds in `_opPreimage`. Tight packing (no field padding): domain ‖ chainId(32) ‖
/// account(20) ‖ version(8) ‖ nonce(8) ‖ validAfter(8) ‖ validUntil(8) ‖
/// maxRelayerFee(32) ‖ target(20) ‖ value(32) ‖ callData.
fn root_preimage(f: &OpFields, valid_after: u64, valid_until: u64, nonce: u64) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(b"MISAKA_PQ_EXECUTE_ROOT_V1");
    p.extend_from_slice(&U256::from(EVM_CHAIN_ID).to_be_bytes::<32>());
    p.extend_from_slice(f.account.as_slice());
    p.extend_from_slice(&f.version.to_be_bytes());
    p.extend_from_slice(&nonce.to_be_bytes());
    p.extend_from_slice(&valid_after.to_be_bytes());
    p.extend_from_slice(&valid_until.to_be_bytes());
    p.extend_from_slice(&f.max_relayer_fee.to_be_bytes::<32>());
    p.extend_from_slice(f.target.as_slice());
    p.extend_from_slice(&f.value.to_be_bytes::<32>());
    p.extend_from_slice(&f.call_data);
    p
}

/// `executeSession`'s op hash — the exact `keccak256(abi.encode(...))` of `_sessionOpHash`.
/// `abi.encode` left-pads every (static) field to 32 bytes: domain(bytes32) ‖
/// chainId(uint256) ‖ account(address) ‖ version(uint64) ‖ target(address) ‖
/// value(uint256) ‖ keccak256(callData)(bytes32) ‖ callIndex(uint64) ‖
/// maxRelayerFee(uint256).
fn session_op_hash(f: &OpFields, call_index: u64) -> B256 {
    let mut buf = Vec::with_capacity(9 * 32);
    buf.extend_from_slice(keccak256(b"MISAKA_PQ_EXECUTE_SESSION_V1").as_slice());
    buf.extend_from_slice(&U256::from(EVM_CHAIN_ID).to_be_bytes::<32>());
    buf.extend_from_slice(&pad_addr(f.account));
    buf.extend_from_slice(&pad_u64(f.version));
    buf.extend_from_slice(&pad_addr(f.target));
    buf.extend_from_slice(&f.value.to_be_bytes::<32>());
    buf.extend_from_slice(keccak256(&f.call_data).as_slice());
    buf.extend_from_slice(&pad_u64(call_index));
    buf.extend_from_slice(&f.max_relayer_fee.to_be_bytes::<32>());
    keccak256(&buf)
}

fn pad_addr(a: Address) -> [u8; 32] {
    let mut o = [0u8; 32];
    o[12..32].copy_from_slice(a.as_slice());
    o
}

fn pad_u64(v: u64) -> [u8; 32] {
    let mut o = [0u8; 32];
    o[24..32].copy_from_slice(&v.to_be_bytes());
    o
}

/// Sign an `executeRoot` op with the ML-DSA-87 Operational Root key. The signer
/// commits to `keyed_blake2b_512(F003_PREA_OP_MLDSA87_CONTEXT, preimage)` under
/// `F003_PREA_ROOT_MLDSA87_CONTEXT`; F003 binds the pubkey to its address payload.
#[allow(clippy::too_many_arguments)]
pub fn sign_root(out: OutputFormat, key: &KeySource, f: &OpFields, valid_after: u64, valid_until: u64, nonce: u64) -> CliResult {
    let vk = key.load_key()?;
    let pubkey = vk.public_key().to_vec();
    let expected = blake2b_512_address_payload(&pubkey);
    let preimage = root_preimage(f, valid_after, valid_until, nonce);
    let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, &preimage);
    let signature = vk.sign_with_context(digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT);

    // The exact F003 v0x02 input: version ‖ payload64 ‖ pubkey ‖ sig ‖ preimage.
    let mut f003 = Vec::with_capacity(1 + 64 + pubkey.len() + signature.len() + preimage.len());
    f003.push(F003_VERSION_PREA_ROOT);
    f003.extend_from_slice(expected.as_byte_slice());
    f003.extend_from_slice(&pubkey);
    f003.extend_from_slice(&signature);
    f003.extend_from_slice(&preimage);

    let calldata = executeRootCall {
        target: f.target,
        value: f.value,
        callData: Bytes::copy_from_slice(&f.call_data),
        validAfterBlock: valid_after,
        validUntilBlock: valid_until,
        nonce,
        publicKey: Bytes::copy_from_slice(&pubkey),
        signature: Bytes::copy_from_slice(&signature),
        maxRelayerFee: f.max_relayer_fee,
    }
    .abi_encode();

    emit_root(out, &pubkey, &signature, &f003, &calldata);
    Ok(())
}

/// Sign an `executeSession` op with a restricted secp256k1 session key, producing the
/// 65-byte `r ‖ s ‖ v` (v∈{27,28}, low-s) the account's `_recover` expects.
pub fn sign_session(out: OutputFormat, key: &EvmKeySource, f: &OpFields, call_index: u64) -> CliResult {
    let signer = key.signer()?;
    let hash = session_op_hash(f, call_index);
    let sig = signer.sign_hash_sync(&hash).map_err(|e| CliError::new(exit::GENERIC, format!("session sign: {e}")))?;

    // r ‖ s ‖ v, with v as the Ethereum recovery byte 27/28. k256 yields canonical
    // low-s, which the contract requires (it rejects s > secp256k1n/2).
    let mut rsv = [0u8; 65];
    rsv[0..32].copy_from_slice(&sig.r().to_be_bytes::<32>());
    rsv[32..64].copy_from_slice(&sig.s().to_be_bytes::<32>());
    rsv[64] = 27 + sig.v() as u8;

    let calldata = executeSessionCall {
        target: f.target,
        value: f.value,
        callData: Bytes::copy_from_slice(&f.call_data),
        callIndex: call_index,
        ecdsaSig: Bytes::copy_from_slice(&rsv),
        maxRelayerFee: f.max_relayer_fee,
    }
    .abi_encode();

    emit_session(out, signer.address(), hash, &rsv, &calldata);
    Ok(())
}

fn emit_root(out: OutputFormat, pubkey: &[u8], sig: &[u8], f003: &[u8], calldata: &[u8]) {
    match out {
        OutputFormat::Json => {
            println!(
                "{}",
                json!({
                    "kind": "executeRoot",
                    "publicKey": hex0x(pubkey),
                    "signature": hex0x(sig),
                    "f003Input": hex0x(f003),
                    "calldata": hex0x(calldata),
                })
            );
        }
        OutputFormat::Human => {
            println!("executeRoot authorization (ML-DSA-87, F003 v0x02):");
            println!("  publicKey  ({} bytes): {}", pubkey.len(), hex0x(pubkey));
            println!("  signature  ({} bytes): {}", sig.len(), hex0x(sig));
            println!("  F003 input ({} bytes): {}", f003.len(), hex0x(f003));
            println!("  calldata   (submit to the account or via the EntryPoint):");
            println!("    {}", hex0x(calldata));
        }
    }
}

fn emit_session(out: OutputFormat, signer: Address, hash: B256, rsv: &[u8], calldata: &[u8]) {
    match out {
        OutputFormat::Json => {
            println!(
                "{}",
                json!({
                    "kind": "executeSession",
                    "sessionKey": format!("{signer:?}"),
                    "opHash": hex0x(hash.as_slice()),
                    "ecdsaSig": hex0x(rsv),
                    "calldata": hex0x(calldata),
                })
            );
        }
        OutputFormat::Human => {
            println!("executeSession authorization (secp256k1 session key):");
            println!("  sessionKey: {signer:?}");
            println!("  opHash:     {}", hex0x(hash.as_slice()));
            println!("  ecdsaSig (r‖s‖v, 65 bytes): {}", hex0x(rsv));
            println!("  calldata (submit to the account or via the EntryPoint):");
            println!("    {}", hex0x(calldata));
        }
    }
}

fn hex0x(b: &[u8]) -> String {
    format!("0x{}", faster_hex::hex_string(b))
}

/// Parse the `sign-root` CLI args and emit the executeRoot authorization.
#[allow(clippy::too_many_arguments)]
pub fn run_sign_root(
    out: OutputFormat,
    key: &KeySource,
    account: &str,
    version: u64,
    nonce: u64,
    valid_after: u64,
    valid_until: u64,
    max_relayer_fee: &str,
    to: &str,
    value: &str,
    calldata: &str,
) -> CliResult {
    let f = OpFields {
        account: parse_address(account)?,
        version,
        target: parse_address(to)?,
        value: parse_u256(value)?,
        call_data: parse_hex(calldata)?,
        max_relayer_fee: parse_u256(max_relayer_fee)?,
    };
    sign_root(out, key, &f, valid_after, valid_until, nonce)
}

/// Parse the `sign-session` CLI args and emit the executeSession authorization.
#[allow(clippy::too_many_arguments)]
pub fn run_sign_session(
    out: OutputFormat,
    key: &EvmKeySource,
    account: &str,
    version: u64,
    call_index: u64,
    max_relayer_fee: &str,
    to: &str,
    value: &str,
    calldata: &str,
) -> CliResult {
    let f = OpFields {
        account: parse_address(account)?,
        version,
        target: parse_address(to)?,
        value: parse_u256(value)?,
        call_data: parse_hex(calldata)?,
        max_relayer_fee: parse_u256(max_relayer_fee)?,
    };
    sign_session(out, key, &f, call_index)
}

/// Parse an `Address` from a 0x-prefixed 20-byte hex string.
pub fn parse_address(s: &str) -> Result<Address, CliError> {
    s.parse::<Address>().map_err(|e| CliError::new(exit::GENERIC, format!("bad address {s}: {e}")))
}

/// Parse a `U256` wei amount from decimal or 0x-hex.
pub fn parse_u256(s: &str) -> Result<U256, CliError> {
    let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        U256::from_str_radix(h, 16)
    } else {
        U256::from_str_radix(s, 10)
    };
    v.map_err(|e| CliError::new(exit::GENERIC, format!("bad uint256 {s}: {e}")))
}

/// Parse 0x-hex calldata (empty string / "0x" → empty).
pub fn parse_hex(s: &str) -> Result<Vec<u8>, CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.is_empty() {
        return Ok(Vec::new());
    }
    if h.len() % 2 != 0 {
        return Err(CliError::new(exit::GENERIC, "calldata hex must have even length".to_string()));
    }
    let mut buf = vec![0u8; h.len() / 2];
    faster_hex::hex_decode(h.as_bytes(), &mut buf).map_err(|e| CliError::new(exit::GENERIC, format!("bad calldata hex: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A throwaway ML-DSA seed (test only — never a real key).
    const SEED_HEX: &str = "7777777777777777777777777777777777777777777777777777777777777777";

    fn write_seed(dir: &std::path::Path) -> String {
        let p = dir.join("seed.hex");
        std::fs::write(&p, SEED_HEX).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn fields() -> OpFields {
        OpFields {
            account: "0xACACacAcACACACacacacacaCAcacACacACacaCAc".parse().unwrap(),
            version: 1,
            target: "0x7A7a7A7a7a7a7a7A7A7a7A7A7A7a7a7A7a7a7a7A".parse().unwrap(),
            value: U256::ZERO,
            call_data: vec![0x12, 0x34],
            max_relayer_fee: U256::ZERO,
        }
    }

    /// The CLI signer's F003 v0x02 output verifies through the REAL precompile logic
    /// (kaspa-evm `run_f003_verify`) with a real ML-DSA-87 signature — proving the
    /// preimage encoding matches the contract and the signature is valid. A flipped
    /// preimage byte must NOT verify.
    #[test]
    fn sign_root_output_verifies_through_real_f003() {
        use kaspa_pq_validator_core::ValidatorKey;
        let mut seed = [0u8; 32];
        faster_hex::hex_decode(SEED_HEX.as_bytes(), &mut seed).unwrap();
        let vk = ValidatorKey::from_seed(seed);
        let f = fields();
        let preimage = root_preimage(&f, 0, u64::MAX, 0);
        let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, &preimage);
        let sig = vk.sign_with_context(digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT);
        let pubkey = vk.public_key().to_vec();
        let expected = blake2b_512_address_payload(&pubkey);

        let mut input = vec![F003_VERSION_PREA_ROOT];
        input.extend_from_slice(expected.as_byte_slice());
        input.extend_from_slice(&pubkey);
        input.extend_from_slice(&sig);
        input.extend_from_slice(&preimage);
        assert!(kaspa_evm::mldsa_verify::run_f003_verify(&input), "CLI F003 input verifies");

        let mut tampered = input.clone();
        let off = 7284 + 25 + 32; // F003 prefix + OP_DOMAIN + chainId → first byte of `account`
        tampered[off] ^= 0x01;
        assert!(!kaspa_evm::mldsa_verify::run_f003_verify(&tampered), "tampered op does not verify");
    }

    /// The session signature recovers to the signing key and is canonical (v∈{27,28}).
    #[test]
    fn sign_session_recovers_to_signer() {
        use alloy_signer_local::PrivateKeySigner;
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let f = fields();
        let hash = session_op_hash(&f, 0);
        let sig = signer.sign_hash_sync(&hash).unwrap();
        let mut rsv = [0u8; 65];
        rsv[0..32].copy_from_slice(&sig.r().to_be_bytes::<32>());
        rsv[32..64].copy_from_slice(&sig.s().to_be_bytes::<32>());
        rsv[64] = 27 + sig.v() as u8;
        assert!(rsv[64] == 27 || rsv[64] == 28, "v is the Ethereum recovery byte");
        // recovered address matches the signer (the account's _recover does the same).
        let recovered = sig.recover_address_from_prehash(&hash).unwrap();
        assert_eq!(recovered, signer.address(), "session sig recovers to the session key");
    }

    /// `_sessionOpHash` is 9 abi.encode words (288 bytes) → a single keccak. Encoding
    /// must be deterministic and depend on maxRelayerFee (the tamper guard).
    #[test]
    fn session_op_hash_binds_max_relayer_fee() {
        let mut f = fields();
        let h0 = session_op_hash(&f, 0);
        f.max_relayer_fee = U256::from(1u64);
        let h1 = session_op_hash(&f, 0);
        assert_ne!(h0, h1, "changing maxRelayerFee changes the op hash");
    }

    /// Audit H-3: the root-op gate refuses while F003 is fence-inert (it is
    /// `u64::MAX` on every shipped network), with the UNSAFE_REFUSED exit code, and
    /// only admits an op once the node's virtual DAA reaches the activation score.
    #[test]
    fn ensure_f003_active_refuses_below_activation() {
        let net = NetworkId::from_str("testnet-10").unwrap();
        let activation = Params::from(net).evm_f003_mldsa_verify_activation_daa_score;
        // Today: fence-inert on every network — even an enormous DAA score is refused.
        assert_eq!(activation, u64::MAX, "F003 is expected fence-inert (governance fence)");
        let err = ensure_f003_active(net, u64::MAX - 1).expect_err("inert fence must refuse");
        assert_eq!(err.code, exit::UNSAFE_REFUSED);
        // The boundary: virtual_daa >= activation admits the op (proves the gate is a
        // real >= comparison, not a hard-coded refusal).
        assert!(ensure_f003_active(net, activation).is_ok(), "op is admitted at/after activation");
    }
}
