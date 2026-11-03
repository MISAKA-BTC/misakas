//! Explicit, local-only cost measurement of a real A16 artifact. No network or pin.
//! This measures the declared graph-v7 A16 runtime, not the K2 fixture route.
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use misaka_palw_base0::artifact::decode_artifact_file_mapped_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
use misaka_palw_base0::mmap::ReadOnlyMap;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, sync::Arc, time::Instant};

fn hash_file(path: &Path) -> String {
    let mut input = File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = input.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    format!("{:x}", hash.finalize())
}

#[test]
#[ignore = "real artifact and pinned token file required; run under buildslot with RSS monitoring"]
fn g14_dolphin_long_context_cost() {
    let artifact_name = std::env::var("G14_A16_ARTIFACT").unwrap();
    let tokens_name = std::env::var("G14_A16_TOKENS").unwrap();
    let n: usize = std::env::var("G14_A16_POSITIONS").unwrap().parse().unwrap();
    let chunk: usize = std::env::var("G14_A16_CHUNK").unwrap_or_else(|_| "16".into()).parse().unwrap();
    assert!((1..=32).contains(&chunk));
    let path = Path::new(&artifact_name);
    let start = Instant::now();
    let artifact_sha = hash_file(path);
    assert_eq!(
        artifact_sha, "8d88abf0cf46d86bed52466328690dbfebf6b49efb2b278a9648f947c4843da6",
        "the user's Dolphin 1.5B 32k precise artifact, not a synthetic shape"
    );
    let tokens_sha = hash_file(Path::new(&tokens_name));
    let tokens_json: serde_json::Value = serde_json::from_reader(File::open(&tokens_name).unwrap()).unwrap();
    let tokens: Vec<usize> = tokens_json["sequences"][0].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
    let artifact = decode_artifact_file_mapped_v1(Arc::new(ReadOnlyMap::open(path).unwrap())).unwrap();
    assert_eq!(artifact.shape.max_position, 32768);
    assert!(n <= artifact.shape.max_position && n <= tokens.len());
    let profile =
        qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: artifact.shape.max_position as u32, ..QWEN25_1_5B }).unwrap();
    let engine = A16Engine::new(&artifact).unwrap();
    let plan = engine.plan_from_profile(&profile).expect("the real artifact serves the declared profile");
    let mut cache = A16Cache::new(artifact.shape.n_layers);
    println!(
        "G14_MODEL_SETUP {}",
        json!({"artifact": artifact_name, "artifact_sha256": artifact_sha,
        "artifact_bytes": path.metadata().unwrap().len(), "token_file": tokens_name, "tokens_sha256": tokens_sha,
        "positions_requested": n, "declared_max_context": artifact.shape.max_position,
        "shape_profile_id": profile.shape_profile_id().to_string(), "setup_wall_ns": start.elapsed().as_nanos(),
        "runtime": "A16 graph-v7 planned", "cache_profile": format!("{:?}", cache.storage()),
        "chunk_positions": chunk, "cold_fetch_measured": false, "arming_evidence": false,
        "kernel_slice_claim_measured": false, "nine_b_measured": false})
    );
    let mut total_trace_bytes = 0u64;
    let mut total_wall_ns = 0u128;
    let mut logits_digest = String::new();
    for first in (0..n).step_by(chunk) {
        let end = (first + chunk).min(n);
        // Post only at this chunk's end, like a prefill segment. This is explicitly
        // different from running and retaining the full vocabulary at every token.
        let start = Instant::now();
        let (logits, traces) = engine.forward_prefill_planned(&plan, &mut cache, &tokens[first..end], first, true).unwrap();
        let wall_ns = start.elapsed().as_nanos();
        assert_eq!(cache.rows(), end);
        assert_eq!(traces.len(), end - first);
        assert_eq!(logits.len(), artifact.shape.vocab);
        let trace_bytes: u64 = traces
            .iter()
            .map(|t| {
                t.pre.iter().chain(t.attn.iter().flat_map(|l| l.iter())).chain(t.post.iter()).map(|r| r.len() as u64 * 4).sum::<u64>()
            })
            .sum();
        let mut hash = Sha256::new();
        for v in &logits {
            hash.update(v.to_le_bytes());
        }
        logits_digest = format!("{:x}", hash.finalize());
        total_trace_bytes += trace_bytes;
        total_wall_ns += wall_ns;
        println!(
            "G14_MODEL_CHUNK {}",
            json!({"first": first, "end": end, "wall_ns": wall_ns,
            "cache_resident_bytes": cache.resident_bytes_v1(), "logical_trace_i32_bytes": trace_bytes,
            "logits_sha256": logits_digest})
        );
    }
    // Input integrity reads are outside the forward timings and occur in N=0 too.
    assert_eq!(hash_file(path), artifact_sha, "artifact changed during measurement");
    assert_eq!(hash_file(Path::new(&tokens_name)), tokens_sha, "token file changed during measurement");
    println!(
        "G14_MODEL_RESULT {}",
        json!({"positions_executed": n, "total_forward_wall_ns": total_wall_ns,
        "cache_resident_bytes": cache.resident_bytes_v1(), "logical_trace_i32_bytes_total": total_trace_bytes,
        "last_logits_sha256": logits_digest, "trace_retained": false, "da_encoded_bytes": null,
        "court_measured": false, "transfer_measured": false, "arming_evidence": false})
    );
}
