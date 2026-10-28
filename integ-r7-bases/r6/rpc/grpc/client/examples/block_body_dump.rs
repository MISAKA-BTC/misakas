//! Forensic: dump one block's body composition (per-tx subnetwork + payload size + id) from a
//! node's gRPC, so two nodes' stored bodies for one hash can be diffed byte-for-fact.
//!
//! ```text
//! cargo run --example block_body_dump -p kaspa-grpc-client -- grpc://127.0.0.1:17510 <block-hash-hex>
//! ```

use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("url");
    let hash = args.next().expect("block hash");
    let client = GrpcClient::connect(url.clone()).await.unwrap_or_else(|e| panic!("connect {url}: {e}"));
    let block = client.get_block(hash.parse().expect("hash"), true).await.expect("getBlock");
    println!("daa={} blue={} txs={}", block.header.daa_score, block.header.blue_score, block.transactions.len());
    for (i, tx) in block.transactions.iter().enumerate() {
        println!(
            "  tx[{i}] subnet={} inputs={} outputs={} payload={}B id={}",
            tx.subnetwork_id,
            tx.inputs.len(),
            tx.outputs.len(),
            tx.payload.len(),
            tx.verbose_data.as_ref().map(|v| v.transaction_id.to_string()).unwrap_or_default(),
        );
    }
    // The coinbase's outputs are the disputed artifact — print them fully.
    if let Some(cb) = block.transactions.first() {
        for (i, o) in cb.outputs.iter().enumerate() {
            println!("  coinbase out[{i}] value={} spk={}", o.value, hex_of(o.script_public_key.script()));
        }
    }
}

fn hex_of(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s.truncate(48);
    s
}
