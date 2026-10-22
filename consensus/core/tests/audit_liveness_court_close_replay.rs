//! audit(liveness-court) L-COURT-01: the per-attempt work a refused split-close completion forces on every validator.
//!
//! A completing `CourtCloseChunk` is assembled, digested, borsh-decoded and adjudicated by `palw_validate_court_object_v1`
//! (consensus/src/pipeline/virtual_processor/processor/palw_validate/court.rs:119-200) BEFORE the court slot is charged, and past
//! `palw_audit_2026_09_11` the slot is charged only when the object APPLIES (palw_accept.rs, AC-SLOT). A refused completion leaves
//! the group intact, so the same 1-byte chunk can be resubmitted (mempool: rejected, no fee) or re-carried (block: dropped, the
//! block stands) and every node repeats the work. This test measures the floor of that work — the steps that run whatever the
//! proof says (assemble, keyed digest, borsh decode, and the borsh re-encode `check_close_cost_v2` runs for every TIR/Gen/Eval/
//! AttnDissection arm) — at 1 chunk, ~1 MB and the 27-chunk ruleset ceiling. Bytes are a `Vec<u8>` payload (memcpy-speed decode),
//! so these are LOWER bounds: a real proof (paths of `Hash64`, i32 rows) decodes slower, and adjudication itself is not included.
//!
//! Run: `cargo test --release -p kaspa-consensus-core --test audit_liveness_court_close_replay -- --nocapture`

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{
    PALW_COURT_CLOSE_CHUNK_MAX_BYTES, PalwConsensusObjectV2, PalwCourtSideV1, palw_court_close_chunk_digest_v1,
};
use std::time::Instant;

fn chunked(total: usize) -> Vec<Vec<u8>> {
    // The object whose bytes are split: one large Vec<u8> payload, borsh-encoded, then cut at the carrier size with a 1-byte tail
    // (the attacker's cheapest completing chunk).
    let payload = PalwConsensusObjectV2::CourtCloseChunk {
        session_id: Hash64::from_u64_word(7),
        side: PalwCourtSideV1::Challenger,
        index: 0,
        bytes: (0..total).map(|i| (i * 31 % 251) as u8).collect(),
    };
    let encoded = borsh::to_vec(&payload).unwrap();
    let (head, tail) = encoded.split_at(encoded.len() - 1);
    let mut parts: Vec<Vec<u8>> = head.chunks(PALW_COURT_CLOSE_CHUNK_MAX_BYTES).map(|c| c.to_vec()).collect();
    parts.push(tail.to_vec());
    parts
}

#[test]
fn audit_liveness_court_refused_completion_floor_cost() {
    let tail_object = PalwConsensusObjectV2::CourtCloseChunk {
        session_id: Hash64::from_u64_word(7),
        side: PalwCourtSideV1::Challenger,
        index: 27,
        bytes: vec![0u8; 1],
    };
    println!("MEASURE completing-chunk object bytes (1-byte tail) = {}", borsh::to_vec(&tail_object).unwrap().len());
    println!("MEASURE chunk max bytes = {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}");
    for total in [PALW_COURT_CLOSE_CHUNK_MAX_BYTES - 64, 1_000_000, 27 * PALW_COURT_CLOSE_CHUNK_MAX_BYTES - 64] {
        let parts = chunked(total);
        let reps = 20u32;
        let mut sink = 0usize;
        let start = Instant::now();
        for _ in 0..reps {
            // 1. assemble (court.rs:123-132)
            let mut assembled = Vec::new();
            for part in &parts {
                assembled.extend_from_slice(part);
            }
            // 2. keyed digest against the declaration's close_digest (court.rs:133-134)
            let digest = palw_court_close_chunk_digest_v1(&assembled);
            // 3. borsh decode (court.rs:135-137)
            let decoded = borsh::from_slice::<PalwConsensusObjectV2>(&assembled).unwrap();
            // 4. check_close_cost_v2's own-encoding measure for TIR/Gen/Eval/AttnDissection arms (palw_court_v2.rs:2947-2972)
            let reencoded = borsh::to_vec(&decoded).unwrap();
            sink = sink.wrapping_add(reencoded.len()).wrapping_add(digest.as_byte_slice()[0] as usize);
        }
        let per = start.elapsed() / reps;
        let bytes: usize = parts.iter().map(Vec::len).sum();
        println!(
            "MEASURE assembled_bytes={bytes} chunks={} per_attempt={:?} ns_per_byte={:.2} (sink {sink})",
            parts.len(),
            per,
            per.as_nanos() as f64 / bytes as f64
        );
    }
}
