//! `misaka palw claims` — the finalized claims of a DAA window, and who did the work on each.
//!
//! This is the C5 reader. The category it feeds — LLM mining, the largest of the five in the
//! testnet points scorer — has never scored anybody, because its only reader indexes **algo-4**
//! leaf registrations and Receipt-DA objects. ConsensusV2 has neither: measured on testnet-11 on
//! 2026-09-07, the last 400 blocks were algo 6 and algo 8, no algo-4 at all, and no node on the
//! fleet runs `--palw-da-import-dir`. That reader refuses rather than reporting an empty result,
//! which is right, and leaves C5 with nothing to count.
//!
//! What replaced the replica pair is better evidence, not worse. A claim is accepted, the chain
//! draws five bonded seats, each re-derives the execution from the served material and signs a
//! verdict, and three matching receipts license it. So the unit here is **one claim that reached
//! `Final`** — the chain's own statement that the work was done, judged, and is no longer
//! disputable, and the same event the fold pays the miner at.
//!
//! # Two participants, and both are named
//!
//! A replica pair had one kind of worker. This lane has two, and both spend the GPU the category
//! exists to buy: the **producer** ran the inference that made the block, and each **panel seat**
//! ran the same execution again to check it. A class is minable only while at least three seats
//! hold its artifact, so the seats are the supply that keeps a class alive rather than a
//! formality. This command reports both; how they share the pool is the scorer's business.
//!
//! # Why `Final` is asked of the node and not computed here
//!
//! `Final` is `licensed_daa + window_challenge`, **deferred while any court session on the claim
//! is open**. Reproducing that in a reader means reproducing a consensus rule in a scorer, and the
//! copy drifts the first time the rule moves. So the walk finds candidate claims cheaply — every
//! `ReceiptLicensed` object rides in a block — and the node is asked for each one's phase
//! (`getPalwFreePromptClaim`, which answers for attempt-lane claims too, with
//! `is_free_prompt: false`). The reader never decides that a claim is final; it repeats what the
//! node said.
//!
//! # What it does not do
//!
//! No signing, no spending, no submitting: a block walk and one read per candidate. A claim that
//! is `Voided` is reported with its phase and no credit, because "did not happen as claimed" is a
//! fact the scorer needs and an omission it cannot tell from a gap in the walk.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use serde_json::json;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

/// A seat's answer on one claim, as the licensing object carried it.
struct SeatSay {
    bond: String,
    verdict: String,
}

/// Connect the way every other `palw` reader does — same resolution, same refusals.
async fn connect(ctx: &Ctx) -> Result<(KaspaRpcClient, NetworkId), CliError> {
    let net = NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let registry = misaka_endpoints::EndpointRegistry::load(&ctx.network);
    let hostport = misaka_endpoints::resolve(&net, EndpointKind::NodeWrpcBorsh, ctx.rpc.as_deref(), registry.as_ref());
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    client
        .connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_secs(ctx.timeout_secs.clamp(2, 30))),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        }))
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e} (node up with --rpclisten-borsh?)")))?;
    let server = client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo: {e}")))?;
    if server.network_id.to_string() != ctx.network {
        return Err(CliError::new(
            exit::NETWORK_MISMATCH,
            format!("node is '{}' but --network is '{}'", server.network_id, ctx.network),
        ));
    }
    Ok((client, net))
}

/// `misaka palw claims --since-daa N [--until-daa M]`.
pub(crate) async fn claims(ctx: &Ctx, since_daa: u64, until_daa: Option<u64>, max_blocks: u64) -> CliResult {
    let (client, _net) = connect(ctx).await?;
    let dag = client.get_block_dag_info().await.map_err(|e| CliError::new(exit::GENERIC, format!("getBlockDagInfo: {e}")))?;
    let tip_daa = dag.virtual_daa_score;
    let until = until_daa.unwrap_or(tip_daa);
    if until < since_daa {
        return Err(CliError::new(exit::GENERIC, format!("--until-daa {until} is below --since-daa {since_daa}")));
    }

    // Walk back from the sink collecting the licensing objects. They are what names the seats;
    // the claim's own facts come from the node in the second pass.
    let mut seats_of: BTreeMap<Hash64, Vec<SeatSay>> = BTreeMap::new();
    let mut walked = 0u64;
    let mut cursor = dag.sink;
    let mut oldest_daa = tip_daa;
    while walked < max_blocks {
        let block = match client.get_block(cursor, true).await {
            Ok(b) => b,
            Err(e) => return Err(CliError::new(exit::GENERIC, format!("getBlock {cursor}: {e}"))),
        };
        let daa = block.header.daa_score;
        oldest_daa = daa;
        walked += 1;
        for (claim, says) in licensed_in_block(&block) {
            seats_of.entry(claim).or_insert(says);
        }
        if daa < since_daa {
            break;
        }
        let Some(parent) = block.header.parents_by_level.first().and_then(|p| p.first()).copied() else { break };
        cursor = parent;
    }

    // Ask the node about each candidate. The phase is the node's word, never this reader's
    // arithmetic — see the module note on `Final`.
    //
    // **A zero has to say why it is a zero.** "final claims: 0" is true of a quiet week, of a
    // window that ends before anything licensed in it could mature, and of a reader that is
    // broken — and an operator cannot tell those apart. So every candidate's phase is tallied and
    // printed beside the count.
    let mut rows = Vec::new();
    let mut phases: BTreeMap<String, u64> = BTreeMap::new();
    let mut outside_window = 0u64;
    for (claim_id, says) in &seats_of {
        let answer = client
            .get_palw_free_prompt_claim(claim_id.to_string())
            .await
            .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwFreePromptClaim {claim_id}: {e}")))?;
        if !answer.found {
            *phases.entry("(the node does not know this claim)".into()).or_default() += 1;
            continue;
        }
        *phases.entry(answer.phase.clone()).or_default() += 1;
        if answer.phase != "Final" {
            continue;
        }
        if answer.phase_daa < since_daa || answer.phase_daa > until {
            outside_window += 1;
            continue;
        }
        // **A bond may not be paid on both sides of one claim.** The draw already excludes the
        // executor's bond, operator id and pubkey (`palw_panel_v2`), so this can only fire if that
        // rule moves — which is exactly why a scorer asserts it instead of assuming it.
        let seats: Vec<&SeatSay> = says.iter().filter(|s| s.bond != answer.executor_bond).collect();
        if seats.len() != says.len() {
            return Err(CliError::new(
                exit::GENERIC,
                format!("claim {claim_id}: the producer's own bond {} sits on its panel — refusing to score it", answer.executor_bond),
            ));
        }
        rows.push(json!({
            "network": ctx.network,
            "claim_id": claim_id.to_string(),
            "class_id": answer.class_id,
            "is_free_prompt": answer.is_free_prompt,
            "work_leaves": answer.work_leaves,
            "final_daa_score": answer.phase_daa,
            "accepted_block": answer.accepted_block,
            "accepted_daa_score": answer.accepted_daa,
            "producer_bond": answer.executor_bond,
            "seats": seats.iter().map(|s| json!({ "bond": s.bond, "verdict": s.verdict })).collect::<Vec<_>>(),
        }));
    }

    match ctx.output {
        OutputFormat::Json => {
            for row in &rows {
                println!("{row}");
            }
        }
        OutputFormat::Human => {
            println!("network      : {}", ctx.network);
            println!("window       : DAA {since_daa} .. {until}   (tip {tip_daa})");
            println!("blocks walked: {walked}  (down to DAA {oldest_daa})");
            println!("licensed seen: {}", seats_of.len());
            println!("final claims : {}", rows.len());
            if !phases.is_empty() {
                let tally = phases.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ");
                println!("their phases : {tally}");
            }
            if outside_window > 0 {
                println!("               {outside_window} were Final but outside DAA {since_daa}..{until}");
            }
            if walked >= max_blocks && oldest_daa > since_daa {
                println!();
                println!(
                    "NOTE: the walk stopped at --max-blocks {max_blocks} with DAA {oldest_daa} still above {since_daa}.\n\
                     Claims older than that are NOT in this output, and an absence here is the walk's, not the chain's."
                );
            }
            for row in &rows {
                println!(
                    "  {}  leaves {:>10}  final@{}  producer {}  seats {}",
                    &row["claim_id"].as_str().unwrap_or("")[..16],
                    row["work_leaves"],
                    row["final_daa_score"],
                    &row["producer_bond"].as_str().unwrap_or("")[..16.min(row["producer_bond"].as_str().unwrap_or("").len())],
                    row["seats"].as_array().map(Vec::len).unwrap_or(0)
                );
            }
        }
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// The `ReceiptLicensed` objects a block carries, as (claim, seat verdicts).
///
/// The extraction is consensus's own (`palw_lifecycle_objects_from_accepted_txs_v2`), not a second
/// decoder written here. A reader with its own copy of the wire format is a copy that can disagree
/// with the chain about what a block said, and the disagreement would show up as points.
fn licensed_in_block(block: &kaspa_rpc_core::RpcBlock) -> Vec<(Hash64, Vec<SeatSay>)> {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2;
    use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

    let txs: Vec<kaspa_consensus_core::tx::Transaction> =
        block.transactions.iter().filter_map(|t| kaspa_consensus_core::tx::Transaction::try_from(t.clone()).ok()).collect();
    let mut out = Vec::new();
    for carrier in palw_lifecycle_objects_from_accepted_txs_v2(&txs).objects {
        if let PalwConsensusObjectV2::ReceiptLicensed { claim, receipts } = carrier.object {
            let says = receipts
                .iter()
                .map(|r| SeatSay {
                    // Spelled exactly as the RPC spells `executor_bond`
                    // (`rpc/service/src/service.rs`), because the two are compared. A bond key
                    // written two ways compares unequal and the producer-on-its-own-panel guard
                    // would pass by never matching anything.
                    bond: format!("{}:{}", r.seat_bond.0.transaction_id, r.seat_bond.0.index),
                    verdict: match r.verdict {
                        PalwReceiptVerdictV2::Valid => "Valid",
                        PalwReceiptVerdictV2::Unavailable { .. } => "Unavailable",
                        PalwReceiptVerdictV2::Incapable => "Incapable",
                    }
                    .to_string(),
                })
                .collect();
            out.push((claim, says));
        }
    }
    out
}
