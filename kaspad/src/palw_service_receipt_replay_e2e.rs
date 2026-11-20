//! Transfer accepted block bodies through the standard RPC block codec to a
//! fresh process. Only the dev fixture genesis UTXOs are imported into its DB.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};

pub(super) const RESULT: &str = "MISAKA_TN1_REPLAY_RESULT";

#[derive(Serialize, Deserialize)]
struct ReceiptContext {
    claim: Hash64,
    receipt: Hash64,
    child: Hash64,
    tip: Hash64,
}

pub(super) async fn check_readers(chain: &Chain, claim: Hash64, receipt: Hash64, child: Hash64) {
    let expected = check_paid_chain(chain, claim, receipt, child);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("receipt.jsonl");
    {
        let mut writer = BufWriter::new(std::fs::File::create_new(&input).unwrap());
        let context = ReceiptContext { claim, receipt, child, tip: chain.tc.get_sink() };
        serde_json::to_writer(&mut writer, &context).unwrap();
        writeln!(writer).unwrap();
        for block in &chain.history {
            serde_json::to_writer(&mut writer, &kaspa_rpc_core::RpcRawBlock::from(block)).unwrap();
            writeln!(writer).unwrap();
        }
        writer.flush().unwrap();
    }
    let (_, module) = module_path!().split_once("::").unwrap();
    let (parent, _) = module.rsplit_once("::").unwrap();
    let name = format!("{parent}::tir_frame_node_tn1_default_semantics_spends_an_actual_honest_quantum");
    tokio::task::spawn_blocking(move || {
        for reader in 1..=2 {
            let result = dir.path().join(format!("reader-{reader}.json"));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([&name, "--exact", "--nocapture", "--test-threads=1"])
                .env(RESULT, &result)
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
    let context: ReceiptContext = serde_json::from_str(&input.next().expect("missing receipt context").unwrap()).unwrap();
    let (config, bundle, premine, _) = tn1_panel_config();
    let _clock = kaspa_core::time::test_clock::start_at(config.params.genesis.timestamp);
    let peer = Chain::new(config, bundle, &premine);
    let mut count = 0;
    for line in input {
        let raw: kaspa_rpc_core::RpcRawBlock = serde_json::from_str(&line.unwrap()).unwrap();
        let block = Block::try_from(raw).expect("RPC block decodes");
        kaspa_core::time::test_clock::advance_to(block.header.timestamp);
        peer.tc.validate_and_insert_block(block).virtual_state_task.await.expect("reader validates block");
        count += 1;
    }
    assert_eq!(peer.tc.get_sink(), context.tip);
    let observed = check_paid_chain(&peer, context.claim, context.receipt, context.child);
    serde_json::to_writer(std::fs::File::create_new(result).unwrap(), &observed).unwrap();
    eprintln!("[service-tir-frame] process {} validated {count} blocks: {observed}", std::process::id());
}
