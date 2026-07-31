//! misaka-palw-bridge daemon entry point. See `lib.rs` for what this is (and is not).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use misaka_palw_bridge::chain::{ChainFacts, PinnedChainFacts, RpcChainFacts};
use misaka_palw_bridge::http::{HttpConfig, serve};
use misaka_palw_bridge::state::BridgeState;

struct Args {
    listen: SocketAddr,
    data_dir: PathBuf,
    auth_token: Option<String>,
    assignment_deadline_ms: i64,
    network_id: u32,
    /// Live node wRPC (`host:port`, Borsh) — the real chain-facts source.
    node_rpc: Option<String>,
    /// Pinned chain-facts JSON. Mutually exclusive with `--node-rpc`; NOT live.
    pinned_facts: Option<PathBuf>,
    /// Require bonded, signed providers (seams 1-4). Off ⇒ the dev-harness behaviour.
    require_bonded: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        listen: "127.0.0.1:26621".parse().expect("static addr"),
        data_dir: PathBuf::from("palw-bridge-data"),
        auth_token: None,
        assignment_deadline_ms: 120_000,
        network_id: 111,
        node_rpc: None,
        pinned_facts: None,
        require_bonded: false,
    };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i).cloned().ok_or_else(|| format!("{} needs a value", argv[*i - 1]))
        };
        match argv[i].as_str() {
            "--listen" => args.listen = take(&mut i)?.parse().map_err(|e| format!("--listen: {e}"))?,
            "--data-dir" => args.data_dir = PathBuf::from(take(&mut i)?),
            "--auth-token" => args.auth_token = Some(take(&mut i)?),
            "--assignment-deadline-ms" => {
                args.assignment_deadline_ms =
                    take(&mut i)?.parse().map_err(|e| format!("--assignment-deadline-ms: {e}"))?
            }
            "--network-id" => args.network_id = take(&mut i)?.parse().map_err(|e| format!("--network-id: {e}"))?,
            "--node-rpc" => args.node_rpc = Some(take(&mut i)?),
            "--pinned-facts" => args.pinned_facts = Some(PathBuf::from(take(&mut i)?)),
            "--require-bonded" => args.require_bonded = true,
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    if args.node_rpc.is_some() && args.pinned_facts.is_some() {
        return Err("--node-rpc and --pinned-facts are mutually exclusive".into());
    }
    if args.require_bonded && args.node_rpc.is_none() && args.pinned_facts.is_none() {
        return Err("--require-bonded needs a chain-facts source (--node-rpc or --pinned-facts)".into());
    }
    Ok(args)
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("misaka-palw-bridge: {e}");
            eprintln!(
                "usage: misaka-palw-bridge [--listen 127.0.0.1:26621] [--data-dir <dir>] \
                 [--auth-token T] [--assignment-deadline-ms N] [--network-id N] \
                 [--node-rpc host:port | --pinned-facts <file>] [--require-bonded]"
            );
            std::process::exit(2);
        }
    };

    let chain: Option<std::sync::Arc<dyn ChainFacts>> = match (&args.node_rpc, &args.pinned_facts) {
        (Some(rpc), _) => match RpcChainFacts::connect(rpc).await {
            Ok(facts) => Some(std::sync::Arc::new(facts)),
            Err(e) => {
                eprintln!("misaka-palw-bridge: chain facts: {e}");
                std::process::exit(1);
            }
        },
        (None, Some(path)) => match PinnedChainFacts::load(path) {
            Ok(facts) => Some(std::sync::Arc::new(facts)),
            Err(e) => {
                eprintln!("misaka-palw-bridge: chain facts: {e}");
                std::process::exit(1);
            }
        },
        (None, None) => None,
    };
    match &chain {
        Some(facts) => eprintln!("[palw-bridge] chain facts: {}", facts.source_label()),
        None => eprintln!(
            "[palw-bridge] chain facts: NONE — consensus seams disabled (dev harness mode; \
             challenges, bonds, DA and arbitration are all off)"
        ),
    }

    let state = match BridgeState::open(&args.data_dir, args.assignment_deadline_ms, args.network_id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("misaka-palw-bridge: journal: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "[palw-bridge] journal seq {} head {} (data-dir {})",
        state.seq(),
        &state.head_root_hex()[..16],
        args.data_dir.display()
    );

    let state = Arc::new(Mutex::new(state));
    if let Err(e) = serve(
        state,
        HttpConfig {
            listen: args.listen,
            auth_token: args.auth_token,
            chain,
            require_bonded: args.require_bonded,
            network_id: args.network_id,
        },
    )
    .await
    {
        eprintln!("misaka-palw-bridge: {e}");
        std::process::exit(1);
    }
}
