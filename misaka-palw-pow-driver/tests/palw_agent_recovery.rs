//! A resident agent that dies must cost a delay, never a tag and never the node.
//!
//! This is the risk a long-lived child adds that a one-shot process does not have: it can be
//! OOM-killed, or restarted by an operator, between two seeds — and the validator finds out by
//! writing to a pipe whose far end is gone. On Unix that is a `SIGPIPE` away from killing the
//! node instead of returning an error, and a wedged handle is a stalled IBD, so the behaviour is
//! worth a test rather than an argument.
//!
//! Real worker, real model, so `#[ignore]`d — its own binary because it mutates the environment
//! and the process-wide agent handle:
//!
//! ```text
//! PALW_WORKER=$PWD/target/release/palw-worker \
//! MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf \
//!   cargo test -p misaka-palw-pow-driver --release --test palw_agent_recovery -- --ignored --nocapture
//! ```

use std::io::Write;
use std::process::{Command, Stdio};

use kaspa_consensus_core::pow_layer0::{
    POW_L1_PALW_N_PREDICT_V1, POW_L1_PALW_OUT_BYTES, palw_l1_tag_from_projection, palw_pow_prompt_v1, palw_pow_seed_v1,
};
use kaspa_hashes::Hash64;
use kaspa_pow::palw::palw_l1_tag;

const NET: &[u8] = b"devnet";

fn one_shot_tag(worker: &str, seed: &[u8; 32]) -> [u8; POW_L1_PALW_OUT_BYTES] {
    let mut child = Command::new(worker)
        .args(["--mode", "verify", "--prompt-stdin", "--n-predict", &POW_L1_PALW_N_PREDICT_V1.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the one-shot worker");
    child.stdin.take().expect("stdin was piped").write_all(palw_pow_prompt_v1(seed).as_bytes()).expect("feed the prompt");
    let out = child.wait_with_output().expect("wait for the one-shot worker");
    assert!(out.status.success(), "the one-shot worker exited with {}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| !l.trim().is_empty()).expect("a document on stdout");
    let doc: serde_json::Value = serde_json::from_str(line).expect("the worker document parses");
    let hash = |name: &str| {
        let mut bytes = [0u8; 64];
        faster_hex::hex_decode(doc[name].as_str().expect("hex field").as_bytes(), &mut bytes).expect("64-byte hex");
        Hash64::from_bytes(bytes)
    };
    let count = |name: &str| doc[name].as_u64().expect("count field") as u32;
    palw_l1_tag_from_projection(
        &hash("output_commitment"),
        &hash("gemm_trace_root"),
        &hash("operation_schedule_commitment"),
        count("prefill_tokens"),
        count("decode_tokens"),
    )
}

#[test]
#[ignore = "needs the real palw-worker and the 1.2 GB pinned model; see this file's header"]
fn a_dead_agent_costs_a_delay_and_not_a_tag() {
    let worker = std::env::var("PALW_WORKER").expect("set PALW_WORKER to the palw-worker binary");
    std::env::var("MISAKA_PALW_GGUF").expect("set MISAKA_PALW_GGUF to the pinned model");
    misaka_palw_pow_driver::install();
    // SAFETY: this integration test is its own binary and holds exactly one test.
    unsafe {
        std::env::remove_var("MISAKA_PALW_POW_FIXTURE");
        std::env::set_var("MISAKA_PALW_AGENT", "1");
    }

    let first = (Hash64::from_bytes([0xA1; 64]), 1_700_000_000_000u64, 11u64);
    let after = (Hash64::from_bytes([0xB2; 64]), 1_700_000_000_001u64, 22u64);
    let expected_after = one_shot_tag(&worker, &palw_pow_seed_v1(after.0, after.1, after.2, NET));

    // Seed one brings an agent up.
    palw_l1_tag(first.0, first.1, first.2, NET).expect("the first seed starts a resident agent");

    // Kill it out from under the validator, the way an OOM sweep would.
    let killed = Command::new("pkill").args(["-f", "palw-worker --mode pow-agent"]).status().expect("run pkill");
    assert!(killed.success(), "no resident agent was running to kill — the first seed did not use one");
    // pkill returns before the process is reaped; the point is only that the far end is gone by
    // the time the next request writes, which the agent's own exit guarantees within a moment.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Seed two must still produce the right tag — by respawning the agent or by falling back to a
    // one-shot process. Which of the two happened is deliberately not asserted: both are correct,
    // and pinning one would be testing the implementation instead of the property.
    let got = palw_l1_tag(after.0, after.1, after.2, NET).expect("a dead agent must not fail the seed after it");
    assert_eq!(got, expected_after, "the seed after a killed agent produced the wrong tag");
}
