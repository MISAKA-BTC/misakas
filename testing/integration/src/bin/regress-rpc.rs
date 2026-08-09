//! The VPS regression's hands and eyes: read a node's chain state, or introduce it to a peer.
//!
//! The local end-to-end tests drive daemons in-process and assert on them directly. The VPS
//! regression cannot — the nodes there are separate processes on separate continents, reachable
//! only through the same RPC a validator uses. Without this, "did it converge?" gets answered by
//! grepping the log, which reports what the node *said while it was working* rather than what it
//! *settled on*: a line about validating a pruning proof reads identically whether that proof won
//! or lost.
//!
//! Two verbs, because the regression needs exactly two things.
//!
//! ```text
//! regress-rpc 127.0.0.1:41241
//!   → pruning_point=8e23…3e3a virtual_daa_score=6013 is_synced=false sink=fa01…9b22 …
//!
//! regress-rpc 127.0.0.1:41241 connect 160.16.131.119:41221
//!   → connected
//! ```
//!
//! `connect` is a mutation and is named as one. It is how the heavier branch is introduced part-way
//! through a round, which is the whole shape of the scenario: the better chain arrives late, from
//! far away, after a nearer and worse one has already won the race.
//!
//! Reading exits non-zero only when the node cannot be reached or refuses the query. An unsynced
//! node is a fact to report, not a failure — treating it as one would make "correctly held back"
//! indistinguishable from "unreachable", and that distinction is most of what the regression
//! measures.

use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(addr) = args.first() else {
        eprintln!("usage: regress-rpc <host:port> [connect <peer-host:port>]");
        return ExitCode::FAILURE;
    };

    let result = match args.get(1).map(String::as_str) {
        None => probe(addr).await,
        Some("connect") => match args.get(2) {
            Some(peer) => connect_peer(addr, peer).await,
            None => Err("connect needs a peer address".to_owned()),
        },
        Some(other) => Err(format!("unknown verb {other:?}; expected `connect`")),
    };

    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("regress-rpc: {addr}: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn client(addr: &str) -> Result<GrpcClient, String> {
    GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{addr}"),
        None,
        false,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .map_err(|e| format!("connect: {e}"))
}

async fn probe(addr: &str) -> Result<String, String> {
    let client = client(addr).await?;
    let dag = client.get_block_dag_info().await.map_err(|e| format!("getBlockDagInfo: {e}"))?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo: {e}"))?;
    let _ = client.disconnect().await;

    Ok(format!(
        "pruning_point={} virtual_daa_score={} is_synced={} sink={} network={} tip_count={}",
        dag.pruning_point_hash,
        dag.virtual_daa_score,
        server.is_synced,
        dag.sink,
        server.network_id,
        dag.tip_hashes.len(),
    ))
}

async fn connect_peer(addr: &str, peer: &str) -> Result<String, String> {
    let client = client(addr).await?;
    // Permanent, so a peer dropped after a failed IBD is dialled again rather than forgotten. A
    // one-shot connection would make the heavy branch vanish the first time its sync was refused,
    // and the round would then be measuring the address manager rather than the chain decision.
    client
        .add_peer(peer.try_into().map_err(|e| format!("bad peer address {peer:?}: {e}"))?, true)
        .await
        .map_err(|e| format!("addPeer: {e}"))?;
    let _ = client.disconnect().await;
    Ok(format!("connected {addr} -> {peer}"))
}
