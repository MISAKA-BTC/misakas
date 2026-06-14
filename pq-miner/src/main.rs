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
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use kaspa_wallet_keys::kaspa_pq::derive_keypair;
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
    /// Mine the coinbase to the kaspa-pq ML-DSA-87 address derived from this BIP39
    /// mnemonic (path m/0/0/0 under `--network-id`). Lets a kaspa-pq wallet that
    /// imports the same mnemonic see the mined funds. If unset, an unspendable
    /// ML-DSA-87 P2PKH placeholder is used.
    #[arg(long)]
    pay_mnemonic: Option<String>,
    /// Mine the coinbase directly to this bech32 address (e.g. a validator funding address
    /// `misakadev:...`). Takes priority over `--pay-mnemonic`; its prefix must match the
    /// network. Lets mined coins be staked as a validator bond.
    #[arg(long)]
    pay_address: Option<String>,
    /// Allow mining to an UNSPENDABLE placeholder address when neither `--pay-address` nor
    /// `--pay-mnemonic` is set (rewards are permanently lost). For PoW smoke tests only; without it
    /// the miner refuses to start rather than silently burning coinbase rewards.
    #[arg(long, default_value_t = false)]
    allow_burn: bool,
    /// Minimum wall-clock interval between submitted blocks, in milliseconds
    /// (0 = no throttle). At trivial difficulty, set this to pace block
    /// production so a multi-datacenter mesh does not outrun cross-DC
    /// propagation — e.g. `2000` on each of two miners yields ~1 block/s
    /// combined. Prevents the GHOSTDAG split-brain that occurs when the block
    /// interval is far below the inter-node propagation delay.
    #[arg(long, default_value_t = 0)]
    min_block_interval_ms: u64,
    /// Benchmark mode: measure the raw BLAKE2b-512 ∥ SHA3-512 Layer-1 hash-rate (H/s) across all
    /// cores for this many seconds, print it, and exit (no node connection). Used to calibrate the
    /// genesis difficulty — at equilibrium the DAA settles difficulty ≈ aggregate-H/s ÷ target-BPS.
    #[arg(long)]
    bench_secs: Option<u64>,
}

#[tokio::main]
async fn main() {
    kaspa_core::log::try_init_logger("INFO");
    let args = Args::parse();

    // --bench-secs N: measure the raw BLAKE2b-512 ∥ SHA3-512 Layer-1 hash-rate across all cores (no
    // RPC), print it, and exit. The DAA settles difficulty ≈ aggregate-H/s ÷ target-BPS, so this
    // predicts the equilibrium difficulty the network reaches under un-throttled mining — set the
    // genesis `bits` near it to skip the initial instamine ramp.
    if let Some(secs) = args.bench_secs {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{Duration, Instant};
        let pre = kaspa_hashes::Hash64::from_bytes([0u8; 64]);
        let net: Arc<Vec<u8>> = Arc::new(args.network_id.clone().into_bytes());
        let nthreads = rayon::current_num_threads();
        let counter = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(secs.max(1));
        log::info!("benchmarking BLAKE2b-512 ∥ SHA3-512 hash-rate for {}s across {} threads…", secs.max(1), nthreads);
        let handles: Vec<_> = (0..nthreads)
            .map(|tid| {
                let c = counter.clone();
                let net = net.clone();
                std::thread::spawn(move || {
                    let mut n = tid as u64;
                    let mut local = 0u64;
                    while Instant::now() < deadline {
                        for _ in 0..64 {
                            // Measure the TRUE per-nonce grind cost: the BLAKE2b-SHA3 L1 tag PLUS the
                            // Layer-0 finalizer (a second BLAKE2b-512 over the full preimage). Omitting
                            // the finalizer overstates H/s by ~1/3, biasing the calibrated difficulty
                            // too HARD; including it makes the reported rate match real mining.
                            let tag = kaspa_consensus_core::pow_layer0::blake2b_sha3_l1_tag_v1(pre, n, net.as_slice());
                            let _ = kaspa_consensus_core::pow_layer0::pow_finalizer_blake2b_512(
                                net.as_slice(),
                                kaspa_consensus_core::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3,
                                pre,
                                0,
                                0x20018618,
                                n,
                                &tag,
                            );
                            n = n.wrapping_add(nthreads as u64);
                            local += 1;
                        }
                    }
                    c.fetch_add(local, Ordering::Relaxed);
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let elapsed = start.elapsed().as_secs_f64();
        let total = counter.load(Ordering::Relaxed);
        println!(
            "BLAKE2B_SHA3_HASHRATE {:.1} H/s  ({} hashes / {:.2}s, {} threads)",
            total as f64 / elapsed,
            total,
            elapsed,
            nthreads
        );
        return;
    }

    let prefix = match args.network_id.as_str() {
        "mainnet" => Prefix::Mainnet,
        "simnet" => Prefix::Simnet,
        s if s.starts_with("testnet") => Prefix::Testnet,
        _ => Prefix::Devnet,
    };
    // Coinbase pay address. With `--pay-mnemonic`, derive the kaspa-pq ML-DSA-87
    // P2PKH address (matching the wallet's `KaspaPqKeyPair.fromMnemonic` path) so a
    // wallet importing the same mnemonic can spend the mined coins. Otherwise use an
    // unspendable ML-DSA-87 P2PKH placeholder (PoW-smoke only).
    let pay_address = match (&args.pay_address, &args.pay_mnemonic) {
        // Explicit address wins — e.g. a validator funding address, so mined coins can be
        // staked into a bond. Its prefix must match the mining network.
        (Some(addr), _) => {
            let parsed = Address::try_from(addr.trim()).expect("invalid --pay-address bech32");
            assert_eq!(parsed.prefix, prefix, "--pay-address prefix does not match network {}", args.network_id);
            log::info!("mining coinbase to explicit address: {parsed}");
            parsed
        }
        (None, Some(phrase)) => {
            let mnemonic = Mnemonic::new(phrase.trim(), Language::English).expect("invalid BIP39 mnemonic");
            let seed = mnemonic.to_seed("");
            let kp = derive_keypair(&args.network_id, 0, 0, 0, seed.as_bytes());
            let addr = kp.address(prefix);
            log::info!("mining coinbase to ML-DSA-87 address: {addr}");
            addr
        }
        // kaspa-pq PQ-only: the no-wallet placeholder must itself be the standard
        // ML-DSA-87 P2PKH class (all-zero hash → unspendable but class-valid), so a
        // placeholder-mined coinbase passes the consensus output-class rule.
        (None, None) => {
            // Fail closed: silently mining to an unspendable address permanently loses every reward.
            if !args.allow_burn {
                eprintln!(
                    "refusing to start: no --pay-address or --pay-mnemonic set, so coinbase rewards would be mined to \
                     an UNSPENDABLE placeholder and permanently lost. Set --pay-address/--pay-mnemonic, or pass \
                     --allow-burn for a PoW smoke test."
                );
                std::process::exit(1);
            }
            log::warn!("--allow-burn: mining to an UNSPENDABLE placeholder (rewards are lost) — PoW smoke test only.");
            Address::new(prefix, Version::PubKeyHashMlDsa87, &[0u8; 64])
        }
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

    let min_interval = std::time::Duration::from_millis(args.min_block_interval_ms);
    if !min_interval.is_zero() {
        log::info!("throttling block production to >= {} ms between blocks", args.min_block_interval_ms);
    }
    // Initialize so the first block is mined immediately (no startup wait).
    let mut last_block = std::time::Instant::now().checked_sub(min_interval).unwrap_or_else(std::time::Instant::now);

    let mut mined = 0u64;
    loop {
        // Pace block production: a block interval far below the cross-DC
        // propagation delay splits the DAG (GHOSTDAG cannot converge).
        if !min_interval.is_zero() {
            let elapsed = last_block.elapsed();
            if elapsed < min_interval {
                tokio::time::sleep(min_interval - elapsed).await;
            }
        }
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
                last_block = std::time::Instant::now();
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
