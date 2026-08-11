//! MISAKA Compute Token Program (design §9.3) — live read-surface smoke.
//!
//! Connects to a running kaspad's gRPC listener and exercises all three token
//! read RPCs end-to-end (client → protowire → server → consensus → token store
//! and back). Run against the TOK devnet the e2e harness leaves behind:
//!
//! ```text
//! cargo run --example token_read_smoke -p kaspa-grpc-client -- \
//!     grpc://127.0.0.1:27110 <owner-128-hex>
//! ```
//!
//! The owner argument is optional; without it only the supply and emission
//! views are read. Exit code 0 = every call answered (an `available: false`
//! answer on a non-token network is an ANSWER, not a failure — the point of
//! the smoke is that the wire round-trips).

use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().unwrap_or_else(|| "grpc://127.0.0.1:27110".into());
    let owner = args.next();

    let client = GrpcClient::connect(url.clone()).await.unwrap_or_else(|e| panic!("connect {url}: {e}"));

    let supply = client.get_token_supply(0).await.expect("getTokenSupply");
    println!(
        "supply    : available={} minted={} burned={} circulating={}",
        supply.available, supply.minted, supply.burned, supply.circulating
    );

    let emission = client.get_token_emission_info().await.expect("getTokenEmissionInfo");
    println!(
        "emission  : available={} epoch={} settled={} R={} X={} paid={} audit={} rewards={} root={} next={} fold_cursor={}",
        emission.available,
        emission.epoch,
        emission.settled,
        emission.budget,
        emission.network_compute,
        emission.paid_total,
        emission.audit_paid,
        emission.reward_count,
        emission.settlement_root,
        emission.next_settlement_epoch,
        emission.fold_cursor,
    );

    if let Some(owner) = owner {
        let entry = client.get_token_ledger_entry(0, owner.clone()).await.expect("getTokenLedgerEntry");
        println!("ledger    : owner={owner} available={} balance={} nonce={}", entry.available, entry.balance, entry.nonce);
    }

    client.disconnect().await.expect("disconnect");
}
