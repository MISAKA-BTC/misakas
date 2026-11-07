// kaspa-pq Phase 7 (PR-7.5) — Rust SDK example.
//
// This example walks through the kaspa-pq sending workflow end-to-end on the
// kaspa-pq SDK surface:
//
//   1. Derive an ML-DSA-87 keypair from a BIP39 mnemonic using the
//      kaspa-pq keygen XOF (`kaspa_wallet_keys::kaspa_pq::derive_keypair`).
//   2. Compute the kaspa-pq P2PKH address for the chosen network prefix.
//   3. (Optional) wRPC-connect to a kaspa-pq node on its default simnet
//      port (27510 = upstream 17510 + 10000) and call `getInfo` as a smoke
//      test.
//   4. Build a placeholder 32-byte sighash digest and sign it with the
//      kaspa-pq tx context (`MLDSA87_TX_CONTEXT`).
//   5. Locally verify the signature with `libcrux_ml_dsa::ml_dsa_87::verify`.
//
// Building an actual signed transaction against a real UTXO requires
// node-side UTXO selection, which depends on a connected client and is left
// as a follow-up (see ADR-0006 §"Out of scope for Phase 7"). This example
// stops at the cryptographic boundary so it remains runnable without a
// live node — pass `--node ws://127.0.0.1:27510` to additionally exercise
// the wRPC `getInfo` smoke test.
//
// Run:
//     cargo run -p kaspa-pq-send-example --bin kaspa-pq-send

use kaspa_addresses::Prefix;
use kaspa_bip32::{Language, Mnemonic};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_TX_CONTEXT};
use kaspa_wallet_keys::kaspa_pq::derive_keypair;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use libcrux_ml_dsa::ml_dsa_87;

const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

const KASPA_PQ_SIMNET_WRPC_BORSH: &str = "ws://127.0.0.1:27510";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_url = std::env::args().nth(2).unwrap_or_else(|| {
        if std::env::args().any(|a| a == "--node") {
            // bare `--node` means "use the default kaspa-pq simnet URL".
            KASPA_PQ_SIMNET_WRPC_BORSH.to_string()
        } else {
            String::new()
        }
    });

    println!("=== kaspa-pq Phase 7 PR-7.5 SDK example ===\n");

    // 1. BIP39 mnemonic -> ML-DSA-87 keypair on simnet, path m/0/0/0.
    let mnemonic = Mnemonic::new(TEST_MNEMONIC, Language::English)?;
    let seed = mnemonic.to_seed("");
    let kp = derive_keypair("simnet", 0, 0, 0, seed.as_bytes());
    let pk_bytes = kp.public_key_bytes();
    println!("Step 1: derived ML-DSA-87 keypair from BIP39 mnemonic + simnet/0/0/0");
    println!("        public_key.len  = {}", pk_bytes.len());

    // 2. kaspa-pq P2PKH address.
    let address = kp.address(Prefix::Simnet);
    let address_str: String = address.into();
    println!("Step 2: kaspa-pq simnet address = {address_str}");

    // 3. Optional wRPC smoke test against a running kaspa-pq simnet node.
    if !node_url.is_empty() {
        println!("\nStep 3: connecting to kaspa-pq simnet node at {node_url}");
        match KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&node_url), None, None, None) {
            Ok(client) => {
                if let Err(e) = client.connect(None).await {
                    println!("        (offline) connect failed: {e}");
                } else {
                    match client.get_info().await {
                        Ok(info) => {
                            println!("        getInfo OK: server_version={}", info.server_version);
                        }
                        Err(e) => println!("        getInfo failed: {e}"),
                    }
                    let _ = client.disconnect().await;
                }
            }
            Err(e) => println!("        client construction failed: {e}"),
        }
    } else {
        println!("\nStep 3: skipped (pass --node to exercise wRPC against a kaspa-pq simnet node)");
    }

    // 4. Build a placeholder 32-byte sighash and sign it with the kaspa-pq tx
    //    context. In a real tx flow, this digest is produced by
    //    kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash
    //    on the populated transaction; the kaspa-pq script engine then
    //    recomputes the same value and verifies under MLDSA87_TX_CONTEXT.
    let sighash = [0xa5u8; 32];
    let signature_bytes = kp.sign(&sighash, [0x77u8; 32]);
    println!("\nStep 4: signed placeholder sighash, signature.len = {}", signature_bytes.len());

    // 5. Local verify — same primitive the consensus opcode runs.
    assert_eq!(pk_bytes.len(), MLDSA87_PK_LEN);
    let vk = ml_dsa_87::MLDSA87VerificationKey::new(*pk_bytes);
    let sig = ml_dsa_87::MLDSA87Signature::new(signature_bytes);
    ml_dsa_87::verify(&vk, &sighash, MLDSA87_TX_CONTEXT, &sig).map_err(|e| format!("kaspa-pq local verify failed: {e:?}"))?;
    println!("Step 5: local verify OK under MLDSA87_TX_CONTEXT.");

    println!("\n(Building a real signed transaction against live UTXOs is a Phase 5'");
    println!("continuation — see docs/adr/0006-rpc-wasm-sdk-types.md §1.)");

    Ok(())
}
