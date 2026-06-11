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
//!   gen:    cargo run -p kaspa-evm --example evm_tx_gen -- gen <count> <nonce_start> [calldata_len] [key_byte]
//!   submit: cargo run -p kaspa-evm --example evm_tx_gen -- submit <grpc_url> <count> <nonce_start> [calldata_len] [key_byte]
//!   status: cargo run -p kaspa-evm --example evm_tx_gen -- status <grpc_url> <tx_hash_hex>
//!
//! `calldata_len` (default 0) pads the tx with zero calldata to fatten payload
//! bytes (Y10: fill blocks toward the 128 KiB cap); gas_limit covers the
//! calldata intrinsic so admission's gas band passes.

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
        _ => eprintln!("{usage}"),
    }
}
