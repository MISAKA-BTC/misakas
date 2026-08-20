//! Resident agent vs a fresh process: the same seed must produce the same Layer-1 tag.
//!
//! This is the property ADR-0041 Decision 1′ rests on. It needs the real worker and the 1.2 GB
//! pinned model, so it is `#[ignore]`d:
//!
//! ```text
//! PALW_WORKER=$PWD/target/release/palw-worker \
//! MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf \
//!   cargo test -p misaka-palw-pow-driver --release --test palw_agent_equivalence -- --ignored --nocapture
//! ```
//!
//! Three DISTINCT seeds, because identical ones would only prove idempotence — not that
//! `shim_reset_context` hands job N+1 a context a fresh process would recognise.
//!
//! One note for whoever reads a failure here: the per-seed entropy lives in `gemm_trace_root`,
//! not in the generated text. Measured on this model, the greedy OUTPUT is the same generic
//! continuation for every seed, so equal `output_commitment`s across seeds are expected and are
//! not what this test is about. What must differ per seed is the tag, and it does.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use kaspa_consensus_core::pow_layer0::{
    POW_L1_PALW_N_PREDICT_V1, POW_L1_PALW_OUT_BYTES, palw_l1_tag_from_projection, palw_pow_prompt_v1, palw_pow_seed_v1,
};
use kaspa_hashes::Hash64;
use kaspa_pow::palw::palw_l1_tag;

/// Pins no determinism class, so the once-per-process calibration probe costs no inference.
const NET: &[u8] = b"devnet";

/// One fresh `--mode verify` process — the path that ships today, invoked directly rather than
/// through `palw_l1_tag` so its answer does NOT land in the by-seed cache the agent run must miss.
fn one_shot_projection(worker: &str, seed: &[u8; 32]) -> serde_json::Value {
    let mut child = Command::new(worker)
        .args(["--mode", "verify", "--prompt-stdin", "--n-predict", &POW_L1_PALW_N_PREDICT_V1.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Discarded rather than piped: nothing here reads it, and an unread pipe is the deadlock
        // the worker's stderr volume finds every time.
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the one-shot worker");
    child.stdin.take().expect("stdin was piped").write_all(palw_pow_prompt_v1(seed).as_bytes()).expect("feed the prompt");
    let out = child.wait_with_output().expect("wait for the one-shot worker");
    assert!(out.status.success(), "the one-shot worker exited with {}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| !l.trim().is_empty()).expect("a document on stdout");
    serde_json::from_str(line).expect("the worker document parses")
}

fn tag_of(doc: &serde_json::Value) -> [u8; POW_L1_PALW_OUT_BYTES] {
    let hash = |name: &str| {
        let hex = doc.get(name).and_then(|v| v.as_str()).unwrap_or_else(|| panic!("the document lacks {name}"));
        let mut bytes = [0u8; 64];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).expect("64-byte hex");
        Hash64::from_bytes(bytes)
    };
    let count = |name: &str| doc.get(name).and_then(|v| v.as_u64()).unwrap_or_else(|| panic!("the document lacks {name}")) as u32;
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
fn the_resident_agent_and_a_fresh_process_compute_the_same_tag() {
    let worker = std::env::var("PALW_WORKER").expect("set PALW_WORKER to the palw-worker binary");
    std::env::var("MISAKA_PALW_GGUF").expect("set MISAKA_PALW_GGUF to the pinned model");
    misaka_palw_pow_driver::install();
    // SAFETY: this integration test is its own binary and holds exactly one test, so nothing else
    // in the process reads or writes the environment while this runs.
    unsafe { std::env::remove_var("MISAKA_PALW_POW_FIXTURE") };

    let attempts: [(Hash64, u64, u64); 3] = [
        (Hash64::from_bytes([0x11; 64]), 1_700_000_000_000, 1),
        (Hash64::from_bytes([0x22; 64]), 1_700_000_000_001, 2),
        (Hash64::from_bytes([0x33; 64]), 1_700_000_000_002, 3),
    ];

    let baseline_started = Instant::now();
    let expected: Vec<_> = attempts
        .iter()
        .map(|&(hash, timestamp, nonce)| tag_of(&one_shot_projection(&worker, &palw_pow_seed_v1(hash, timestamp, nonce, NET))))
        .collect();
    let one_shot_elapsed = baseline_started.elapsed();

    // Non-vacuity: three seeds must be three tags, or the comparison below asserts nothing.
    assert_ne!(expected[0], expected[1], "two seeds produced one tag; the comparison below would be vacuous");
    assert_ne!(expected[1], expected[2], "two seeds produced one tag; the comparison below would be vacuous");
    assert_ne!(expected[0], expected[2], "two seeds produced one tag; the comparison below would be vacuous");

    // SAFETY: as above.
    unsafe { std::env::set_var("MISAKA_PALW_AGENT", "1") };
    let agent_started = Instant::now();
    for (i, &(hash, timestamp, nonce)) in attempts.iter().enumerate() {
        let got = palw_l1_tag(hash, timestamp, nonce, NET).expect("the resident agent path produces a tag");
        assert_eq!(got, expected[i], "seed {i}: the resident agent and a fresh process disagree on the tag");
    }
    let agent_elapsed = agent_started.elapsed();

    println!("one-shot, 3 fresh processes: {one_shot_elapsed:?}");
    println!("resident agent, 1 process including the model load: {agent_elapsed:?}");
    // And the agent was genuinely used. Every failure inside it falls back to the one-shot path,
    // which is the right behaviour and would also make this test pass while proving nothing — so
    // the cost is the evidence: a silent fallback runs the same three processes and lands within
    // noise of the baseline, not several times under it.
    assert!(
        agent_elapsed < one_shot_elapsed,
        "the agent path ({agent_elapsed:?}) was not faster than three one-shot processes \
         ({one_shot_elapsed:?}) — it most likely fell back, making the equality above vacuous"
    );
}
