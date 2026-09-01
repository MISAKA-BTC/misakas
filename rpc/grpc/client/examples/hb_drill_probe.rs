//! ADR-0068 Phase 1 drill probe: header-level facts (algo id, DAA, blue work, timestamp) for
//! the heartbeat drill's evidence trail. Drill tooling only — not wired into anything.
//!
//! ```text
//! cargo run --example hb_drill_probe -p kaspa-grpc-client -- grpc://127.0.0.1:36610 info
//! cargo run --example hb_drill_probe -p kaspa-grpc-client -- grpc://127.0.0.1:36610 block <hash>
//! cargo run --example hb_drill_probe -p kaspa-grpc-client -- grpc://127.0.0.1:36610 recent <n>
//! ```

use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;

fn ts(ms: u64) -> String {
    // Human-readable UTC-ish stamp without pulling chrono: seconds since epoch is enough for a
    // drill log correlated against node logs that print local time.
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

async fn print_block(client: &GrpcClient, hash: kaspa_rpc_core::RpcHash, with_txs: bool) {
    let block = client.get_block(hash, true).await.expect("getBlock");
    let h = &block.header;
    println!(
        "block {} algo={} daa={} blue_score={} blue_work={:x} ts={} bits={:08x} txs={}",
        h.hash,
        h.pow_algo_id,
        h.daa_score,
        h.blue_score,
        h.blue_work,
        ts(h.timestamp),
        h.bits,
        block.transactions.len(),
    );
    if with_txs {
        for (i, tx) in block.transactions.iter().enumerate() {
            println!(
                "  tx[{i}] id={} inputs={} outputs={} payload={}B",
                tx.verbose_data.as_ref().map(|v| v.transaction_id.to_string()).unwrap_or_default(),
                tx.inputs.len(),
                tx.outputs.len(),
                tx.payload.len(),
            );
        }
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("url");
    let mode = args.next().unwrap_or_else(|| "info".to_string());
    let client = GrpcClient::connect(url.clone()).await.unwrap_or_else(|e| panic!("connect {url}: {e}"));
    match mode.as_str() {
        "info" => {
            let info = client.get_block_dag_info().await.expect("dagInfo");
            println!(
                "network={} blocks={} headers={} daa={} tips={:?} pruning={} sink={}",
                info.network,
                info.block_count,
                info.header_count,
                info.virtual_daa_score,
                info.tip_hashes,
                info.pruning_point_hash,
                info.sink,
            );
        }
        "block" => {
            let hash: kaspa_rpc_core::RpcHash = args.next().expect("hash").parse().expect("hash hex");
            print_block(&client, hash, true).await;
        }
        "recent" => {
            let n: usize = args.next().unwrap_or_else(|| "10".to_string()).parse().expect("count");
            let info = client.get_block_dag_info().await.expect("dagInfo");
            let mut cursor = info.sink;
            for _ in 0..n {
                let block = client.get_block(cursor, false).await.expect("getBlock");
                let h = &block.header;
                println!(
                    "block {} algo={} daa={} blue_score={} blue_work={:x} ts={} parents={}",
                    h.hash,
                    h.pow_algo_id,
                    h.daa_score,
                    h.blue_score,
                    h.blue_work,
                    ts(h.timestamp),
                    h.parents_by_level.first().map(|p| p.len()).unwrap_or(0),
                );
                let Some(parent) = h.parents_by_level.first().and_then(|p| p.first()).copied() else {
                    break;
                };
                cursor = parent;
            }
        }
        other => panic!("unknown mode {other}"),
    }
}
