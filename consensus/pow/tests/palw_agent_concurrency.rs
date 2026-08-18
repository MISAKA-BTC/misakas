//! ADR-0041 Decision 2: several seeds at once must be faster, and must be the same tags.
//!
//! The speedup is bounded by cores, not by the permit count — each worker pins
//! `qwen35_pins::CPU_THREADS` threads, so `permits × CPU_THREADS` above the core count buys
//! nothing. This test therefore asserts the DIRECTION (parallel beats serial, same answers) and
//! prints the ratio, which is the number that should be re-measured per host rather than assumed.
//!
//! Real worker, real model, own binary (it mutates the environment and the process-wide pool):
//!
//! ```text
//! PALW_WORKER=$PWD/target/release/palw-worker \
//! MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf \
//!   cargo test -p kaspa-pow --release --test palw_agent_concurrency -- --ignored --nocapture
//! ```

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use kaspa_consensus_core::pow_layer0::{
    POW_L1_PALW_N_PREDICT_V1, POW_L1_PALW_OUT_BYTES, palw_l1_tag_from_projection, palw_pow_prompt_v1, palw_pow_seed_v1,
};
use kaspa_hashes::Hash64;
use kaspa_pow::palw::{inference_concurrency, palw_l1_tag};

const NET: &[u8] = b"devnet";
/// Three concurrent workers × 4 pinned threads each is 12 — the core count of the machine this
/// was written on. Above that the permits are real but the cores are not.
const PERMITS: usize = 3;

fn attempt(n: u64) -> (Hash64, u64, u64) {
    (Hash64::from_bytes([n as u8; 64]), 1_700_000_000_000 + n, n)
}

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

/// Runs one attempt per thread and returns the wall clock for the whole group.
fn concurrently(attempts: &[(Hash64, u64, u64)]) -> (Vec<[u8; POW_L1_PALW_OUT_BYTES]>, std::time::Duration) {
    let started = Instant::now();
    let tags = std::thread::scope(|scope| {
        let handles =
            attempts.iter().map(|&(hash, ts, nonce)| scope.spawn(move || palw_l1_tag(hash, ts, nonce, NET))).collect::<Vec<_>>();
        handles.into_iter().map(|h| h.join().expect("no thread panicked").expect("a tag")).collect::<Vec<_>>()
    });
    (tags, started.elapsed())
}

#[test]
#[ignore = "needs the real palw-worker and the 1.2 GB pinned model; see this file's header"]
fn concurrent_seeds_are_faster_and_are_the_same_tags() {
    let worker = std::env::var("PALW_WORKER").expect("set PALW_WORKER to the palw-worker binary");
    std::env::var("MISAKA_PALW_GGUF").expect("set MISAKA_PALW_GGUF to the pinned model");
    // SAFETY: this integration test is its own binary and holds exactly one test, and both reads
    // below happen after these writes.
    unsafe {
        std::env::remove_var("MISAKA_PALW_POW_FIXTURE");
        std::env::set_var("MISAKA_PALW_AGENT", "1");
        std::env::set_var("MISAKA_PALW_CONCURRENCY", PERMITS.to_string());
    }
    assert_eq!(inference_concurrency(), PERMITS, "the gate did not take the configured permit count");

    // Baseline truth, from processes that share nothing with the pool.
    let measured: Vec<_> = (1..=PERMITS as u64).map(attempt).collect();
    let expected: Vec<_> =
        measured.iter().map(|&(hash, ts, nonce)| one_shot_tag(&worker, &palw_pow_seed_v1(hash, ts, nonce, NET))).collect();
    assert_ne!(expected[0], expected[1], "two seeds produced one tag; the comparison below would be vacuous");

    // Warm the pool: this group pays the model loads, so it is not the group we time.
    let (_, warm) = concurrently(&(101..=100 + PERMITS as u64).map(attempt).collect::<Vec<_>>());

    // Serial through the warm pool, then the same amount of work concurrently.
    let serial_started = Instant::now();
    for &(hash, ts, nonce) in &(201..=200 + PERMITS as u64).map(attempt).collect::<Vec<_>>() {
        palw_l1_tag(hash, ts, nonce, NET).expect("a tag");
    }
    let serial = serial_started.elapsed();
    let (tags, parallel) = concurrently(&measured);

    for (i, tag) in tags.iter().enumerate() {
        assert_eq!(tag, &expected[i], "seed {i}: a concurrently-computed tag differs from a fresh process's");
    }

    println!("permits: {PERMITS} (pool warmed in {warm:?})");
    println!("{PERMITS} seeds serially through the pool: {serial:?}");
    println!("{PERMITS} seeds concurrently:              {parallel:?}");
    println!("speedup: {:.2}x", serial.as_secs_f64() / parallel.as_secs_f64());
    // Direction only. The magnitude is a property of the host's cores, not of this code, and
    // asserting a magnitude here would be asserting something about the machine.
    assert!(
        parallel < serial,
        "concurrent seeds ({parallel:?}) were not faster than serial ones ({serial:?}) — the permits are not being used"
    );
}
