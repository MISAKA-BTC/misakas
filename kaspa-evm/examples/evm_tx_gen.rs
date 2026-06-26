//! kaspa-pq EVM Lane activation-prep tool (Y10 / relay e2e driver): sign N
//! EIP-1559 transfers from a deterministic test key and submit them to a live
//! node, or query a tx's inclusion status. Dev-tooling only — examples build
//! against dev-deps, so the alloy signer and the gRPC client never enter the
//! production (secp-free) tree.
//!
//! On a fresh devnet the senders are UNFUNDED, so every tx is a deterministic
//! class-2 acceptance skip — exactly what Y10/relay need: the payload bytes
//! still relay, fill templates, propagate in blocks and land in the tx-lookup
//! index (`included_in` + `last_skip_class=2`), without touching EVM state.
//!
//! Usage:
//!   gen:      cargo run -p kaspa-evm --example evm_tx_gen -- gen <count> <nonce_start> [calldata_len] [key_byte]
//!   submit:   cargo run -p kaspa-evm --example evm_tx_gen -- submit <grpc_url> <count> <nonce_start> [calldata_len] [key_byte]
//!   status:   cargo run -p kaspa-evm --example evm_tx_gen -- status <grpc_url> <tx_hash_hex>
//!   addr:     cargo run -p kaspa-evm --example evm_tx_gen -- addr [key_byte]            (print the key's EVM address)
//!   claim:    cargo run -p kaspa-evm --example evm_tx_gen -- claim <grpc_url> <lock_txid_hex> <index>
//!   withdraw: cargo run -p kaspa-evm --example evm_tx_gen -- withdraw <grpc_url> <nonce> <amount_sompi> <dest_payload_64B_hex> [key_byte]
//!   receipt:  cargo run -p kaspa-evm --example evm_tx_gen -- receipt <grpc_url> <tx_hash_hex>
//!   balance:  cargo run -p kaspa-evm --example evm_tx_gen -- balance <grpc_url> <kaspa_address>
//!
//! `calldata_len` (default 0) pads the tx with zero calldata to fatten payload
//! bytes (Y10: fill blocks toward the 128 KiB cap); gas_limit covers the
//! calldata intrinsic so admission's gas band passes.
//!
//! The bridge e2e cycle: validator `deposit-lock` (UTXO side) → `claim` (queue
//! the DepositClaim on a MINING node) → the claim executes in an accepting
//! chain block crediting the EVM address → `withdraw` (EVM tx calling F002
//! with value + [spk ver BE ‖ script] calldata) → a synthetic UTXO
//! materializes at the destination — checked with `balance`.

use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
use alloy_eips::eip2718::Encodable2718;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;
use revm::primitives::{Address, B256, U256};

fn sign_tx(key_byte: u8, nonce: u64, calldata_len: usize) -> (Address, Vec<u8>) {
    let signer = PrivateKeySigner::from_bytes(&B256::from([key_byte; 32])).unwrap();
    // 21k intrinsic + 4 gas per zero calldata byte + headroom; clamp to the
    // admission gas band's upper bound (the per-chain-block accepted gas cap).
    let gas_limit = (21_000 + 4 * calldata_len as u64 + 1_000).min(MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK);
    let tx = TxEip1559 {
        chain_id: EVM_CHAIN_ID,
        nonce,
        gas_limit,
        max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128,
        max_priority_fee_per_gas: 0,
        to: revm::primitives::TxKind::Call(Address::with_last_byte(0x22)),
        value: U256::from(1u64),
        access_list: Default::default(),
        input: vec![0u8; calldata_len].into(),
    };
    let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
    (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = vec![0u8; bytes.len() * 2];
    faster_hex::hex_encode(bytes, &mut s).unwrap();
    String::from_utf8(s).unwrap()
}

/// Sign a contract-CREATE tx (eth-rpc e2e: exercises `eth_getTransactionReceipt`
/// `contractAddress` + `eth_getLogs`). Returns `(sender, contract_address, raw)`.
fn sign_deploy(key_byte: u8, nonce: u64, init_code: Vec<u8>) -> (Address, Address, Vec<u8>) {
    let signer = PrivateKeySigner::from_bytes(&B256::from([key_byte; 32])).unwrap();
    let tx = TxEip1559 {
        chain_id: EVM_CHAIN_ID,
        nonce,
        gas_limit: 200_000,
        max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128,
        max_priority_fee_per_gas: 0,
        to: revm::primitives::TxKind::Create,
        value: U256::ZERO,
        access_list: Default::default(),
        input: init_code.into(),
    };
    let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
    (signer.address(), signer.address().create(nonce), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: evm_tx_gen gen <count> <nonce_start> [calldata_len] [key_byte] | submit <grpc_url> <count> <nonce_start> [calldata_len] [key_byte] | status <grpc_url> <tx_hash_hex>";
    match args.get(1).map(String::as_str) {
        Some("gen") => {
            let count: u64 = args[2].parse().expect("count");
            let nonce_start: u64 = args[3].parse().expect("nonce_start");
            let calldata_len: usize = args.get(4).map(|s| s.parse().expect("calldata_len")).unwrap_or(0);
            let key_byte: u8 = args.get(5).map(|s| s.parse().expect("key_byte")).unwrap_or(0x11);
            for nonce in nonce_start..nonce_start + count {
                let (sender, raw) = sign_tx(key_byte, nonce, calldata_len);
                println!("{} {} {}", nonce, sender, hex_of(&raw));
            }
        }
        Some("gendeploy") => {
            // eth-rpc e2e: a contract whose constructor emits LOG0 then deploys
            // empty runtime code. Init: PUSH1 0,PUSH1 0,LOG0, PUSH1 0,PUSH1 0,RETURN.
            let nonce: u64 = args[2].parse().expect("nonce");
            let key_byte: u8 = args.get(3).map(|s| s.parse().expect("key_byte")).unwrap_or(0x11);
            let init_code = vec![0x60, 0x00, 0x60, 0x00, 0xa0, 0x60, 0x00, 0x60, 0x00, 0xf3];
            let (sender, contract, raw) = sign_deploy(key_byte, nonce, init_code);
            println!("sender {sender}");
            println!("contract {contract}");
            println!("raw 0x{}", hex_of(&raw));
        }
        Some("submit") => {
            let url = args[2].clone();
            let count: u64 = args[3].parse().expect("count");
            let nonce_start: u64 = args[4].parse().expect("nonce_start");
            let calldata_len: usize = args.get(5).map(|s| s.parse().expect("calldata_len")).unwrap_or(0);
            let key_byte: u8 = args.get(6).map(|s| s.parse().expect("key_byte")).unwrap_or(0x11);
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let mut ok = 0u64;
            for nonce in nonce_start..nonce_start + count {
                let (_, raw) = sign_tx(key_byte, nonce, calldata_len);
                match client.submit_evm_transaction(hex_of(&raw)).await {
                    Ok(resp) => {
                        ok += 1;
                        println!("nonce {} -> {}", nonce, resp.transaction_hash);
                    }
                    Err(e) => println!("nonce {} -> ERROR {}", nonce, e),
                }
            }
            println!("submitted {}/{}", ok, count);
        }
        Some("status") => {
            let url = args[2].clone();
            let hash = args[3].clone();
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let s = client.get_evm_tx_inclusion_status(hash).await.expect("status call");
            println!(
                "pending={} included_in={:?} accepted_in={:?} receipt_index={} last_skip_class={}",
                s.pending, s.included_in, s.accepted_in, s.receipt_index, s.last_skip_class
            );
        }
        Some("addr") => {
            let key_byte: u8 = args.get(2).map(|s| s.parse().expect("key_byte")).unwrap_or(0x11);
            let signer = PrivateKeySigner::from_bytes(&B256::from([key_byte; 32])).unwrap();
            println!("{}", signer.address());
        }
        Some("claim") => {
            let url = args[2].clone();
            let txid = args[3].clone();
            let index: u32 = args[4].parse().expect("index");
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let r = client.submit_evm_deposit_claim(txid, index).await.expect("claim call");
            println!(
                "queued: evm_address={} credit={} sompi (tip {} to the accepting coinbase)",
                r.evm_address, r.amount_sompi, r.claim_tip_sompi
            );
        }
        Some("withdraw") => {
            // F002 withdraw: a payable call carrying value = amount_sompi × SCALE
            // wei and calldata [spk version u16 BE ‖ 69-byte ML-DSA P2PKH script
            // for dest_payload]. On success the executor burns the escrow and
            // consensus materializes a synthetic UTXO at the destination.
            let url = args[2].clone();
            let nonce: u64 = args[3].parse().expect("nonce");
            let amount_sompi: u64 = args[4].parse().expect("amount_sompi");
            let dest_hex = args[5].clone();
            let key_byte: u8 = args.get(6).map(|s| s.parse().expect("key_byte")).unwrap_or(0x11);

            let mut dest_payload = [0u8; 64];
            faster_hex::hex_decode(dest_hex.as_bytes(), &mut dest_payload).expect("dest payload must be 128 hex chars (64 bytes)");
            let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&dest_payload);
            let mut calldata = spk.version().to_be_bytes().to_vec();
            calldata.extend_from_slice(spk.script());

            let value_wei = amount_sompi as u128 * kaspa_consensus_core::evm::EVM_NATIVE_SCALE as u128;
            let signer = PrivateKeySigner::from_bytes(&B256::from([key_byte; 32])).unwrap();
            let tx = TxEip1559 {
                chain_id: EVM_CHAIN_ID,
                nonce,
                gas_limit: 100_000,
                max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128,
                max_priority_fee_per_gas: 0,
                to: revm::primitives::TxKind::Call(kaspa_evm::withdraw::f002_address()),
                value: U256::from(value_wei),
                access_list: Default::default(),
                input: calldata.into(),
            };
            let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
            let raw = TxEnvelope::from(tx.into_signed(sig)).encoded_2718();
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let resp = client.submit_evm_transaction(hex_of(&raw)).await.expect("withdraw submit");
            println!("withdraw tx from {} (nonce {nonce}, {amount_sompi} sompi) -> {}", signer.address(), resp.transaction_hash);
        }
        Some("receipt") => {
            let url = args[2].clone();
            let hash = args[3].clone();
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let r = client.get_evm_transaction_receipt(hash).await.expect("receipt call");
            println!(
                "found={} succeeded={} accepting_block={} evm_number={} gas_used={} logs={}",
                r.found,
                r.succeeded,
                r.accepting_block,
                r.evm_number,
                r.gas_used,
                r.logs.len()
            );
        }
        Some("balance") => {
            let url = args[2].clone();
            let addr = kaspa_rpc_core::RpcAddress::try_from(args[3].as_str()).expect("kaspa address");
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let b = client.get_balance_by_address(addr).await.expect("balance call");
            println!("balance_sompi={b}");
        }
        Some("dns") => {
            // The node-wide DNS confirmation view over gRPC — the operator's
            // dnsConfirmed signal (also wired into `kaspa-pq-validator status`
            // over wRPC; this is the gRPC differential).
            let url = args[2].clone();
            let client = GrpcClient::connect(url).await.expect("gRPC connect");
            let d = client.get_dns_confirmation().await.expect("dns call");
            println!(
                "available={} dns_confirmed={} pow_confirmed={} work={}/{} stake={}/{} health={} anchor={} anchor_daa={} note={:?}",
                d.available,
                d.dns_confirmed,
                d.pow_confirmed,
                d.work_depth,
                d.required_work_depth,
                d.stake_depth,
                d.required_stake_depth,
                d.health,
                d.last_dns_confirmed_anchor,
                d.last_dns_confirmed_anchor_daa_score,
                d.note
            );
        }
        Some("payload") => {
            // The 64-byte address payload (hex) of a bech32 kaspa address — the
            // `dest_payload` input of the `withdraw` mode.
            let addr = kaspa_rpc_core::RpcAddress::try_from(args[2].as_str()).expect("kaspa address");
            println!("{}", hex_of(&addr.payload));
        }
        _ => eprintln!("{usage}"),
    }
}
