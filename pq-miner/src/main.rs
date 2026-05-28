//! kaspa-pq Layer 0 CPU grinder / miner.
//!
//! A minimal mining client for the kaspa-pq Layer 0 PoW (BLAKE2b-512,
//! 512-bit target, ADR-0007/0008). It repeatedly:
//!   1. requests a block template from a node (`get_block_template`),
//!   2. grinds a nonce that satisfies the Layer 0 target using
//!      `kaspa_pow::StateLayer0` (multi-threaded via rayon),
//!   3. submits the solved block (`submit_block`).
//!
//! The `--network-id` bytes MUST match the node's
//! `NetworkId::to_string()` (e.g. `devnet`) so the finalizer domain
//! separation agrees with consensus validation.

use clap::Parser;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_bip32::{Language, Mnemonic};
use kaspa_consensus_core::header::Header;
use kaspa_wallet_keys::kaspa_pq::derive_keypair;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[command(about = "kaspa-pq Layer 0 CPU miner")]
struct Args {
    /// gRPC endpoint of the node (host:port).
    #[arg(long, default_value = "127.0.0.1:26610")]
    rpc: String,
    /// Network id string fed to the Layer 0 finalizer (must equal the node's NetworkId::to_string()).
    #[arg(long, default_value = "devnet")]
    network_id: String,
    /// Stop after mining this many blocks (0 = run forever).
    #[arg(long, default_value_t = 0)]
    blocks: u64,
    /// Mine the coinbase to the kaspa-pq ML-DSA-65 address derived from this BIP39
    /// mnemonic (path m/0/0/0 under `--network-id`). Lets a kaspa-pq wallet that
    /// imports the same mnemonic see the mined funds. If unset, an unspendable
    /// PubKey placeholder is used.
    #[arg(long)]
    pay_mnemonic: Option<String>,
}

#[tokio::main]
async fn main() {
    kaspa_core::log::try_init_logger("INFO");
    let args = Args::parse();

    let prefix = match args.network_id.as_str() {
        "mainnet" => Prefix::Mainnet,
        "simnet" => Prefix::Simnet,
        s if s.starts_with("testnet") => Prefix::Testnet,
        _ => Prefix::Devnet,
    };
    // Coinbase pay address. With `--pay-mnemonic`, derive the kaspa-pq ML-DSA-65
    // P2PKH address (matching the wallet's `KaspaPqKeyPair.fromMnemonic` path) so a
    // wallet importing the same mnemonic can spend the mined coins. Otherwise use an
    // unspendable PubKey placeholder (PoW-smoke only).
    let pay_address = match &args.pay_mnemonic {
        Some(phrase) => {
            let mnemonic = Mnemonic::new(phrase.trim(), Language::English).expect("invalid BIP39 mnemonic");
            let seed = mnemonic.to_seed("");
            let kp = derive_keypair(&args.network_id, 0, 0, 0, seed.as_bytes());
            let addr = kp.address(prefix);
            log::info!("mining coinbase to ML-DSA-65 address: {addr}");
            addr
        }
        None => Address::new(prefix, Version::PubKey, &[0u8; 32]),
    };
    let network_id = args.network_id.clone().into_bytes();

    let ctx = SubscriptionContext::new();
    let client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", args.rpc),
        Some(ctx),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("failed to connect to node gRPC");

    log::info!("connected to {}; mining network_id={} to {}", args.rpc, args.network_id, pay_address);

    let mut mined = 0u64;
    loop {
        let mut template = match client.get_block_template(pay_address.clone(), vec![]).await {
            Ok(t) => t,
            Err(e) => {
                log::warn!("get_block_template failed: {e}; retrying in 1s");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        // Convert the template header to a consensus Header to drive the Layer 0 grind.
        let header: Header = match (&template.block.header).try_into() {
            Ok(h) => h,
            Err(e) => {
                log::warn!("header convert failed: {e}; retrying");
                continue;
            }
        };

        // Grind the Layer 0 nonce (multi-threaded). `StateLayer0` caches the
        // nonce-independent pre-PoW state; `check_pow_layer0(n)` varies n.
        let state = kaspa_pow::StateLayer0::new(&header, &network_id);
        let found = (0u64..u64::MAX).into_par_iter().find_any(|&n| state.check_pow_layer0(n).map(|(ok, _)| ok).unwrap_or(false));
        let Some(nonce) = found else {
            log::warn!("no nonce found in range; refetching template");
            continue;
        };

        template.block.header.nonce = nonce;
        match client.submit_block(template.block, false).await {
            Ok(_) => {
                mined += 1;
                log::info!("mined block #{mined} (nonce={nonce}, daa_score={})", header.daa_score);
            }
            Err(e) => log::warn!("submit_block failed: {e}"),
        }

        if args.blocks != 0 && mined >= args.blocks {
            log::info!("done: mined {mined} blocks");
            break;
        }
    }
}
