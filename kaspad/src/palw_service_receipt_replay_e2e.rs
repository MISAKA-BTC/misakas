//! Transfer accepted block bodies through the standard RPC block codec to a
//! fresh process. Only the dev fixture genesis UTXOs are imported into its DB.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};

const SETTLED_ARCHIVE: &str = "MISAKA_TN1_SETTLED_ARCHIVE";
pub(super) const RESULT: &str = "MISAKA_TN1_REPLAY_RESULT";

#[derive(Serialize, Deserialize)]
struct ReceiptContext {
    claim: Hash64,
    receipt: Hash64,
    child: Hash64,
    tip: Hash64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settlement_anchor: Option<Hash64>,
}

fn write_archive(chain: &Chain, context: &ReceiptContext, path: &std::path::Path) {
    let mut writer = BufWriter::new(std::fs::File::create_new(path).unwrap());
    serde_json::to_writer(&mut writer, context).unwrap();
    writeln!(writer).unwrap();
    for block in &chain.history {
        serde_json::to_writer(&mut writer, &kaspa_rpc_core::RpcRawBlock::from(block)).unwrap();
        writeln!(writer).unwrap();
    }
    writer.flush().unwrap();
}

fn observe(chain: &Chain, context: &ReceiptContext) -> serde_json::Value {
    assert_eq!(chain.tc.get_sink(), context.tip);
    let mut result = check_paid_chain(chain, context.claim, context.receipt, context.child);
    if let Some(anchor) = context.settlement_anchor {
        let (_, state) = chain.tc.palw_state_v2_store.read().load_tip(&chain.bundle.state).unwrap().unwrap();
        let row = state.claim(&anchor).expect("settlement attempt retained");
        assert!(matches!(row.source, kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::Attempt));
        let PalwClaimPhaseV2::Final { final_daa } = row.phase else { panic!("settlement attempt must be Final") };
        let accepting_daa = result["workerAcceptedDaa"].as_u64().unwrap();
        let settlement = chain.tc.palw_settlement_v1(accepting_daa).unwrap();
        assert!(row.accepted_daa >= accepting_daa && settlement.settled && settlement.depth > 0);
        result["settlementAnchor"] = serde_json::json!({
            "claim": anchor, "block": row.accepted_block, "acceptedDaa": row.accepted_daa, "finalDaa": final_daa
        });
    }
    result
}

pub(super) async fn check_readers(chain: &Chain, claim: Hash64, receipt: Hash64, child: Hash64, anchor: Hash64) {
    let context = ReceiptContext { claim, receipt, child, tip: chain.tc.get_sink(), settlement_anchor: Some(anchor) };
    let expected = observe(chain, &context);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("receipt.jsonl");
    write_archive(chain, &context, &input);
    let (_, module) = module_path!().split_once("::").unwrap();
    let (parent, _) = module.rsplit_once("::").unwrap();
    let name = format!("{parent}::tir_frame_node_tn1_default_semantics_spends_an_actual_honest_quantum");
    tokio::task::spawn_blocking(move || {
        for reader in 1..=2 {
            let result = dir.path().join(format!("reader-{reader}.json"));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([&name, "--exact", "--nocapture", "--test-threads=1"])
                .env(RESULT, &result)
                .env_remove(SETTLED_ARCHIVE)
                .env("KASPA_TEST_CLOCK_ISOLATED", "1")
                .stdin(std::fs::File::open(&input).unwrap())
                .status()
                .unwrap();
            assert!(status.success(), "receipt reader {reader} failed: {status}");
            let observed: serde_json::Value = serde_json::from_reader(std::fs::File::open(&result).unwrap()).unwrap();
            assert_eq!(observed, expected, "independent reader {reader}");
        }
    })
    .await
    .unwrap();
}

pub(super) async fn read_archive(result: PathBuf) {
    let mut input = BufReader::new(std::io::stdin()).lines();
    let mut context: ReceiptContext = serde_json::from_str(&input.next().expect("missing receipt context").unwrap()).unwrap();
    let (config, bundle, premine, _) = tn1_panel_config();
    let _clock = kaspa_core::time::test_clock::start_at(config.params.genesis.timestamp);
    let mut peer = Chain::new(config, bundle, &premine);
    let extend_to = std::env::var_os(SETTLED_ARCHIVE).map(PathBuf::from);
    let mut count = 0;
    for line in input {
        let raw: kaspa_rpc_core::RpcRawBlock = serde_json::from_str(&line.unwrap()).unwrap();
        let block = Block::try_from(raw).expect("RPC block decodes");
        kaspa_core::time::test_clock::advance_to(block.header.timestamp);
        if extend_to.is_some() {
            peer.history.push(block.clone());
        }
        peer.tc.validate_and_insert_block(block).virtual_state_task.await.expect("reader validates block");
        count += 1;
    }
    assert_eq!(peer.tc.get_sink(), context.tip);
    if let Some(path) = extend_to {
        assert!(context.settlement_anchor.is_none(), "archive already carries a settlement anchor");
        let tip = peer.tc.get_block(context.tip).unwrap();
        peer.time = tip.header.timestamp;
        peer.nonce = tip.header.nonce;
        let fixture = TirFixture::new(&peer);
        let mut floats = reader_funding(&peer);
        let anchor = settle_worker(&mut peer, &fixture, &mut floats, context.child).await;
        context.tip = peer.tc.get_sink();
        context.settlement_anchor = Some(anchor);
        observe(&peer, &context);
        write_archive(&peer, &context, &path);
    }
    let observed = observe(&peer, &context);
    serde_json::to_writer(std::fs::File::create_new(result).unwrap(), &observed).unwrap();
    eprintln!("[service-tir-frame] process {} validated {count} blocks: {observed}", std::process::id());
}

// Resume from fee change actually present in the replayed UTXO set.
fn reader_funding(chain: &Chain) -> Vec<Funding> {
    let scripts = (0..chain.bonds.len()).map(card_payout_spk).collect::<Vec<_>>();
    let mut funds: Vec<Option<Funding>> = vec![None; scripts.len()];
    let mut cursor = None;
    loop {
        let page = chain.tc.get_virtual_utxos(cursor, 256, cursor.is_some());
        let Some((last, _)) = page.last() else { break };
        cursor = Some(*last);
        for (outpoint, entry) in page {
            if !entry.is_coinbase
                && let Some(card) = scripts.iter().position(|script| script == &entry.script_public_key)
                && funds[card].as_ref().is_none_or(|(_, old)| old.amount < entry.amount)
            {
                funds[card] = Some((outpoint, entry));
            }
        }
    }
    funds.into_iter().enumerate().map(|(card, utxo)| utxo.unwrap_or_else(|| panic!("missing fee change for card {card}"))).collect()
}

/// Advance the historical private fixture with a new executed attempt and the drawn seats' replay.
/// Native consensus determines whether the payout has settled at the UTXO's own accepting DAA.
pub(super) async fn settle_worker(chain: &mut Chain, fixture: &TirFixture, floats: &mut [Funding], child: Hash64) -> Hash64 {
    use kaspa_consensus_core::palw_attempt_v2::{palw_attempt_job_v1, palw_nonce_bucket_v1};

    let child = chain.tc.get_block(child).unwrap();
    let outpoint = TransactionOutpoint::new(child.transactions[0].id(), 0);
    let accepted = chain.tc.get_virtual_utxo_entry(outpoint).expect("accepted worker coinbase").block_daa_score;
    let claim = fixture.ir_attempt(chain, PROBER).await.expect("post-payout IR attempt admitted");
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    fixture.refresh_readiness(chain, floats).await;
    chain.advance(slot).await;
    fixture.ir_attempt(chain, 7).await;
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    for duty in &duties {
        assert_eq!((duty.class_id, duty.artifact_root), (fixture.class_id, fixture.root));
        let header = chain.tc.get_block(duty.accepted_block).unwrap().header;
        let backend = fixture.backend(chain);
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&header);
        let anchor = backend
            .job_anchor_v1(chain.network_domain, pre_pow, duty.class_id, &duty.executor_bond.0, palw_nonce_bucket_v1(header.nonce))
            .unwrap();
        let (job, prompt) = backend.job_for_anchor(anchor).unwrap();
        let job = palw_attempt_job_v1(job, chain.config.params.palw_prefill_draw_active_at(header.daa_score));
        let replay = backend.execute_for_verdict(&job, &prompt);
        assert_eq!(
            crate::palw_panel::palw_seat_replay_step_v1(
                &replay,
                duty.execution_root,
                duty.trace_root,
                duty.work_leaves,
                duty.output_root
            ),
            crate::palw_panel::PalwSeatReplayStepV1::Licensed,
            "drawn seat {} must reproduce the accepted attempt",
            duty.seat_index
        );
    }
    TirFixture::submit_valid_receipts(chain, floats, claim, &duties).await;
    let deadline = chain.daa() + chain.bundle.state.window_challenge_at(slot) + chain.bundle.state.window_court() + 100;
    loop {
        let phase = chain.view(claim).phase;
        let settlement = chain.tc.palw_settlement_v1(accepted).unwrap();
        if matches!(phase, PalwClaimPhaseV2::Final { .. }) && settlement.settled && settlement.depth > 0 {
            return claim;
        }
        assert!(
            !matches!(phase, PalwClaimPhaseV2::Voided { .. }) && chain.daa() < deadline,
            "attempt {claim}: {phase:?}, worker {accepted}: {settlement:?}"
        );
        chain.heartbeat(vec![]).await;
    }
}
