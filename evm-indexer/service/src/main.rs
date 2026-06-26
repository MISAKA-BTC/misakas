//! `misaka-evm-indexer` — the off-chain EVM token-transfer indexer daemon (§10).
//!
//! Config (env, all optional):
//! * `MISAKA_INDEXER_NODE`           node eth-rpc `host:port`        (default `127.0.0.1:8545`)
//! * `MISAKA_INDEXER_FINALITY_DEPTH` blocks below head held immutable (default `100`)
//! * `MISAKA_INDEXER_MAX_BLOCKS`     blocks applied per pass          (default `500`)
//! * `MISAKA_INDEXER_POLL_MS`        idle poll period in ms           (default `2000`)
//! * `MISAKA_INDEXER_PG_URL`         libpq URL — required with `--features pg`
//!
//! The daemon poll-drives [`sync_once`]: while behind the node head it loops
//! immediately (bounded catch-up); once caught up it sleeps `POLL_MS` before
//! re-checking. A §9 WebSocket `newHeads`/`logs` subscription would cut that
//! idle latency but is a pure optimization over this loop (documented follow-on).
//! The query HTTP API (§10.6) and the token-metadata worker are the remaining
//! follow-ons; the [`IndexStore`] read surface they need is already in place.

use std::time::Duration;

use misaka_evm_indexer::{HttpNodeRpc, IndexStore, NodeRpc, sync_once};

struct Config {
    node_addr: String,
    finality_depth: u64,
    max_blocks: u64,
    poll: Duration,
    #[allow(dead_code)] // only read by the `pg` build
    pg_url: Option<String>,
}

impl Config {
    fn from_env() -> Self {
        fn var_u64(key: &str, default: u64) -> u64 {
            std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
        }
        Config {
            node_addr: std::env::var("MISAKA_INDEXER_NODE").unwrap_or_else(|_| "127.0.0.1:8545".to_string()),
            finality_depth: var_u64("MISAKA_INDEXER_FINALITY_DEPTH", 100),
            max_blocks: var_u64("MISAKA_INDEXER_MAX_BLOCKS", 500).max(1),
            poll: Duration::from_millis(var_u64("MISAKA_INDEXER_POLL_MS", 2000)),
            pg_url: std::env::var("MISAKA_INDEXER_PG_URL").ok(),
        }
    }
}

/// The poll-driven indexing loop, generic over the node + store so the same
/// driver runs against either backend. Exits cleanly on Ctrl-C / SIGINT.
async fn run<N, S>(node: N, mut store: S, cfg: &Config)
where
    N: NodeRpc + Sync,
    S: IndexStore + Send,
{
    loop {
        let pass = sync_once(&node, &mut store, cfg.finality_depth, cfg.max_blocks);
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!("[misaka-evm-indexer] shutdown signal received; exiting");
                return;
            }
            result = pass => match result {
                Ok(o) => {
                    if o.applied > 0 || o.reverted > 0 {
                        eprintln!(
                            "[misaka-evm-indexer] head={:?} applied={} reverted={} transfers={} malformed={}{}",
                            o.new_head, o.applied, o.reverted, o.transfers, o.malformed,
                            if o.caught_up { "" } else { " (catching up)" },
                        );
                    }
                    // While catching up, loop immediately; once at the head, idle.
                    if o.caught_up {
                        tokio::time::sleep(cfg.poll).await;
                    }
                }
                Err(e) => {
                    eprintln!("[misaka-evm-indexer] sync error: {e}");
                    tokio::time::sleep(cfg.poll).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let cfg = Config::from_env();
    let node = HttpNodeRpc::new(cfg.node_addr.clone());
    eprintln!(
        "[misaka-evm-indexer] node={} finality_depth={} max_blocks/pass={} poll={}ms",
        cfg.node_addr,
        cfg.finality_depth,
        cfg.max_blocks,
        cfg.poll.as_millis(),
    );

    #[cfg(feature = "pg")]
    {
        let url = cfg.pg_url.clone().unwrap_or_else(|| {
            eprintln!("[misaka-evm-indexer] FATAL: built with --features pg but MISAKA_INDEXER_PG_URL is unset");
            std::process::exit(2);
        });
        let store = match misaka_evm_indexer::PgIndexStore::connect(&url).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[misaka-evm-indexer] FATAL: PostgreSQL connect/migrate failed: {e}");
                std::process::exit(1);
            }
        };
        eprintln!("[misaka-evm-indexer] storage: PostgreSQL");
        run(node, store, &cfg).await;
    }

    #[cfg(not(feature = "pg"))]
    {
        eprintln!(
            "[misaka-evm-indexer] storage: IN-MEMORY (volatile — state is lost on restart). \
             Rebuild with --features pg for a persistent PostgreSQL backend."
        );
        run(node, misaka_evm_indexer::MemIndexStore::new(), &cfg).await;
    }
}
