//! misaminer — MISAKA (kaspa-pq) Layer-0 CPU miner.
//!
//! A small, standalone mining client for the MISAKA / kaspa-pq Layer-0 PoW
//! (BLAKE2b-512, 512-bit target). Point it at a MISAKA gateway/pool (or a
//! node's gRPC), set your payout wallet, and it will:
//!   1. request a block template (`get_block_template`),
//!   2. grind a Layer-0 nonce with `kaspa_pow::StateLayer0` (multi-threaded),
//!   3. submit the solved block (`submit_block`).
//!
//! `--network-id` MUST match the node's `NetworkId::to_string()` (e.g. `testnet-200`)
//! so the Layer-0 finalizer domain separation agrees with consensus validation.

use clap::Parser;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_bip32::{Language, Mnemonic};
use kaspa_consensus_core::header::Header;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use kaspa_wallet_keys::kaspa_pq::derive_keypair;
use rayon::prelude::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "misaminer", version, about = "misaminer — MISAKA (kaspa-pq) Layer-0 BLAKE2b-512 CPU miner")]
struct Args {
    /// Mining endpoint as host:port — a MISAKA gateway/pool or a node's gRPC.
    #[arg(long, visible_alias = "rpc", default_value = "127.0.0.1:26210")]
    pool: String,
    /// Your payout wallet (bech32, e.g. `misakatest:...`); mined coinbase rewards go here.
    #[arg(long, visible_alias = "pay-address")]
    wallet: Option<String>,
    /// Network id string — must equal the node's NetworkId::to_string() (e.g. `testnet-200`, `mainnet`).
    #[arg(long, default_value = "testnet-200")]
    network_id: String,
    /// Worker / rig name shown in logs (handy when running several rigs).
    #[arg(long, default_value = "rig0")]
    worker: String,
    /// CPU threads to grind with (0 = all logical cores).
    #[arg(long, default_value_t = 0)]
    threads: usize,
    /// Stop after mining this many blocks (0 = run forever).
    #[arg(long, default_value_t = 0)]
    blocks: u64,
    /// Derive the payout address from this BIP39 mnemonic (path m/0/0/0) instead of `--wallet`.
    /// UNSAFE: leaks the BIP39 phrase into process args / shell history / systemd / container
    /// inspect; DISABLED unless MISAKA_ALLOW_UNSAFE_CLI_SECRETS=1; prefer --pay-mnemonic-file
    /// or --pay-mnemonic-stdin.
    #[arg(long)]
    pay_mnemonic: Option<String>,
    /// Read the payout BIP39 mnemonic from a file instead of the command line (preferred; audit v22).
    #[arg(long)]
    pay_mnemonic_file: Option<String>,
    /// Read the payout BIP39 mnemonic from stdin instead of the command line (preferred; audit v22).
    #[arg(long, default_value_t = false)]
    pay_mnemonic_stdin: bool,
    /// Allow mining to an UNSPENDABLE placeholder address when neither `--wallet` nor
    /// `--pay-mnemonic` is set (rewards are permanently lost). For PoW smoke tests only; without it
    /// the miner refuses to start rather than silently burning coinbase rewards.
    #[arg(long, default_value_t = false)]
    allow_burn: bool,
    /// Minimum wall-clock ms between submitted blocks (0 = no throttle). 1000-2000 keeps a
    /// multi-datacenter mesh from out-running propagation (GHOSTDAG split-brain) at low difficulty.
    #[arg(long, default_value_t = 1000)]
    min_block_interval_ms: u64,
}

/// Whether the unsafe raw-secret CLI override (MISAKA_ALLOW_UNSAFE_CLI_SECRETS=1)
/// is honored. Audit (2026-06-27) M-6: the override is dev-build-only. Release
/// binaries (`debug_assertions` off) ALWAYS refuse raw CLI secrets regardless of
/// the env var, so a misconfigured production deployment cannot leak secrets.
fn unsafe_cli_secrets_allowed() -> bool {
    cfg!(debug_assertions) && std::env::var("MISAKA_ALLOW_UNSAFE_CLI_SECRETS").as_deref() == Ok("1")
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("\n  misaminer v{VERSION}  —  MISAKA (kaspa-pq) Layer-0 CPU miner");
    println!("  ---------------------------------------------------------");
    println!("  pool/node : {}", args.pool);
    println!("  network   : {}", args.network_id);
    println!("  worker    : {}", args.worker);
    println!("  threads   : {}\n", if args.threads == 0 { num_threads_label() } else { args.threads.to_string() });

    kaspa_core::log::try_init_logger("INFO");

    if args.threads > 0
        && let Err(e) = rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global()
    {
        log::warn!("could not set thread pool to {} threads: {e}; using default", args.threads);
    }

    let prefix = match args.network_id.as_str() {
        "mainnet" => Prefix::Mainnet,
        "simnet" => Prefix::Simnet,
        s if s.starts_with("testnet") => Prefix::Testnet,
        _ => Prefix::Devnet,
    };
    // Coinbase payout address. `--wallet` (explicit bech32) wins; else derive from
    // `--pay-mnemonic` (ML-DSA-87 P2PKH, m/0/0/0) so a wallet importing the same
    // mnemonic can spend the rewards; else an unspendable placeholder (PoW-smoke only).
    // Audit (2026-06-27, v22): resolve the mnemonic from a file or stdin in preference to
    // --pay-mnemonic (which leaks the BIP39 phrase into process args / shell history / systemd / logs).
    let pay_mnemonic_resolved: Option<String> = if let Some(p) = &args.pay_mnemonic_file {
        Some(std::fs::read_to_string(p).expect("failed to read --pay-mnemonic-file").trim().to_string())
    } else if args.pay_mnemonic_stdin {
        use std::io::Read as _;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).expect("failed to read the mnemonic from stdin");
        Some(s.trim().to_string())
    } else if let Some(m) = args.pay_mnemonic.clone() {
        // Audit (2026-06-27) M-1: passing the mnemonic on the command line leaks it into process args /
        // shell history / systemd unit / container inspect / logs. Refuse the raw-secret CLI form by
        // default; only honor it when the operator explicitly opts in via MISAKA_ALLOW_UNSAFE_CLI_SECRETS=1.
        if !unsafe_cli_secrets_allowed() {
            eprintln!(
                "refusing to start: --pay-mnemonic passes the BIP39 mnemonic on the command line, where it leaks \
                 into process args / shell history / systemd unit / container inspect / logs. Use --pay-mnemonic-file, \
                 --pay-mnemonic-stdin, or --wallet instead. The MISAKA_ALLOW_UNSAFE_CLI_SECRETS=1 override is honored \
                 only in dev builds; release binaries always refuse this form."
            );
            std::process::exit(1);
        }
        log::warn!(
            "--pay-mnemonic passes the BIP39 mnemonic on the command line (it leaks into process args / shell \
             history / systemd unit / container inspect / logs). Prefer --pay-mnemonic-file, --pay-mnemonic-stdin, or --wallet."
        );
        Some(m)
    } else {
        None
    };
    let pay_address = match (&args.wallet, &pay_mnemonic_resolved) {
        (Some(addr), _) => {
            let parsed = Address::try_from(addr.trim()).expect("invalid --wallet bech32 address");
            assert_eq!(parsed.prefix, prefix, "--wallet prefix does not match network {}", args.network_id);
            log::info!("payout wallet: {parsed}");
            parsed
        }
        (None, Some(phrase)) => {
            let mnemonic = Mnemonic::new(phrase.trim(), Language::English).expect("invalid BIP39 mnemonic");
            let seed = mnemonic.to_seed("");
            let kp = derive_keypair(&args.network_id, 0, 0, 0, seed.as_bytes());
            let addr = kp.address(prefix);
            log::info!("payout wallet (from mnemonic): {addr}");
            addr
        }
        (None, None) => {
            // Fail closed: silently mining to an unspendable address permanently loses every reward.
            // Require an explicit opt-in (--allow-burn) for the PoW-smoke-test use case.
            if !args.allow_burn {
                eprintln!(
                    "refusing to start: no --wallet or --pay-mnemonic set, so coinbase rewards would be mined to an \
                     UNSPENDABLE placeholder and permanently lost. Set --wallet/--pay-mnemonic, or pass --allow-burn \
                     for a PoW smoke test."
                );
                std::process::exit(1);
            }
            log::warn!("--allow-burn: mining to an UNSPENDABLE placeholder (rewards are lost) — PoW smoke test only.");
            // PQ-only: the placeholder must be the standard ML-DSA-87 P2PKH class
            // (all-zero hash → unspendable but class-valid) so the coinbase it funds
            // passes the consensus output-class rule.
            Address::new(prefix, Version::PubKeyHashMlDsa87, &[0u8; 64])
        }
    };
    let network_id = args.network_id.clone().into_bytes();

    let ctx = SubscriptionContext::new();
    let client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", args.pool),
        Some(ctx),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("failed to connect to the mining endpoint (gRPC)");

    log::info!("[{}] connected to {}; mining network_id={} to {}", args.worker, args.pool, args.network_id, pay_address);

    let min_interval = std::time::Duration::from_millis(args.min_block_interval_ms);
    if !min_interval.is_zero() {
        log::info!("throttling block production to >= {} ms between blocks", args.min_block_interval_ms);
    }
    // Initialize so the first block is mined immediately (no startup wait).
    let mut last_block = std::time::Instant::now().checked_sub(min_interval).unwrap_or_else(std::time::Instant::now);

    let mut mined = 0u64;
    loop {
        // Pace block production: a block interval far below the cross-DC propagation
        // delay splits the DAG (GHOSTDAG cannot converge).
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

        // Convert the template header to a consensus Header to drive the Layer-0 grind.
        let header: Header = match (&template.block.header).try_into() {
            Ok(h) => h,
            Err(e) => {
                log::warn!("header convert failed: {e}; retrying");
                continue;
            }
        };

        // Grind the Layer-0 nonce (multi-threaded). `StateLayer0` caches the
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
                log::info!("[{}] mined block #{mined} (nonce={nonce}, daa_score={})", args.worker, header.daa_score);
            }
            Err(e) => log::warn!("submit_block failed: {e}"),
        }

        if args.blocks != 0 && mined >= args.blocks {
            log::info!("done: mined {mined} blocks");
            break;
        }
    }
}

fn num_threads_label() -> String {
    match std::thread::available_parallelism() {
        Ok(n) => format!("{} (all cores)", n.get()),
        Err(_) => "all cores".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::unsafe_cli_secrets_allowed;

    // Audit M-6: the raw-secret CLI override must be honored ONLY in dev builds.
    #[test]
    fn unsafe_cli_secrets_gated_on_debug_assertions() {
        const KEY: &str = "MISAKA_ALLOW_UNSAFE_CLI_SECRETS";
        // SAFETY: tests run single-threaded for this serial assertion; no other
        // thread reads this env var concurrently. Restored before returning.
        let prev = std::env::var(KEY).ok();

        unsafe { std::env::remove_var(KEY) };
        assert!(!unsafe_cli_secrets_allowed(), "must be false when the env var is unset");

        unsafe { std::env::set_var(KEY, "1") };
        // In a release build (debug_assertions off) the override is ALWAYS refused.
        assert_eq!(unsafe_cli_secrets_allowed(), cfg!(debug_assertions));

        unsafe { std::env::set_var(KEY, "0") };
        assert!(!unsafe_cli_secrets_allowed(), "must be false for any value other than \"1\"");

        match prev {
            Some(v) => unsafe { std::env::set_var(KEY, v) },
            None => unsafe { std::env::remove_var(KEY) },
        }
    }
}
