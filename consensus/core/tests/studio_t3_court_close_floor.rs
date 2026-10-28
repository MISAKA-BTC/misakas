//! studio/t3-liveness (2026-10-22, measurement only) — L-COURT-01, the floor of the per-attempt work a refused split-close
//! completion forces on every validator, at seven assembly sizes from 1 KB to the 27-chunk ceiling, with a separate warm-up and
//! median / p95 / max over repeated runs, per step (assemble, keyed digest, borsh decode, borsh re-encode) and in total.
//!
//! The steps are the ones `palw_validate_court_object_v1` (consensus/src/pipeline/virtual_processor/processor/palw_validate/court.rs)
//! runs on a completing `CourtCloseChunk` before `adjudicate_court_close_v5`, plus the own-encoding measure `check_close_cost_v2`
//! takes for the TIR/Gen/Eval/AttnDissection arms. The payload is one `Vec<u8>` (memcpy-speed decode), so these are LOWER bounds
//! on the floor; the adjudication proper is measured in `palw_held_close_chunks.rs::studio_t3_court_adjudication_cost`.
//!
//! Run: `cargo test --release -p kaspa-consensus-core --test studio_t3_court_close_floor -- --ignored --nocapture --test-threads=1`
//! (`STUDIO_T3_RUNS=30` overrides the run count).

#![allow(warnings)]
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{
    PALW_COURT_CLOSE_CHUNK_MAX_BYTES, PalwConsensusObjectV2, PalwCourtSideV1, palw_court_close_chunk_digest_v1,
};
use std::time::Instant;

fn chunked(total: usize) -> Vec<Vec<u8>> {
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

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = if v.len() % 2 == 1 { v[v.len() / 2] } else { (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0 };
    let rank = ((0.95 * v.len() as f64).ceil() as usize).clamp(1, v.len());
    (med, v[rank - 1], *v.last().unwrap())
}

#[test]
#[ignore = "studio/t3-liveness measurement; run explicitly with --ignored --release"]
fn studio_t3_court_refused_completion_floor_cost_by_size() {
    let runs: usize = std::env::var("STUDIO_T3_RUNS").ok().map(|s| s.parse().unwrap()).unwrap_or(30);
    let warmup = 3usize;
    let max = PALW_COURT_CLOSE_CHUNK_MAX_BYTES;
    println!("STUDIO_T3_COURT chunk_max_bytes={max} runs={runs} warmup={warmup}");
    let sizes = [1_000usize, 16_000, 128_000, 512_000, 1_000_000, 2_000_000, 27 * max - 64];
    for total in sizes {
        let parts = chunked(total);
        let bytes: usize = parts.iter().map(Vec::len).sum();
        let mut t_asm = Vec::new();
        let mut t_dig = Vec::new();
        let mut t_dec = Vec::new();
        let mut t_enc = Vec::new();
        let mut t_all = Vec::new();
        let mut sink = 0usize;
        for i in 0..warmup + runs {
            let t0 = Instant::now();
            let mut assembled = Vec::new();
            for part in &parts {
                assembled.extend_from_slice(part);
            }
            let t1 = Instant::now();
            let digest = palw_court_close_chunk_digest_v1(&assembled);
            let t2 = Instant::now();
            let decoded = borsh::from_slice::<PalwConsensusObjectV2>(&assembled).unwrap();
            let t3 = Instant::now();
            let reencoded = borsh::to_vec(&decoded).unwrap();
            let t4 = Instant::now();
            sink = sink.wrapping_add(std::hint::black_box(reencoded.len())).wrapping_add(digest.as_byte_slice()[0] as usize);
            if i >= warmup {
                let us = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e6;
                t_asm.push(us(t0, t1));
                t_dig.push(us(t1, t2));
                t_dec.push(us(t2, t3));
                t_enc.push(us(t3, t4));
                t_all.push(us(t0, t4));
            }
        }
        let (a, d, c, e, all) = (stats(t_asm), stats(t_dig), stats(t_dec), stats(t_enc), stats(t_all));
        println!(
            "STUDIO_T3_COURT bytes={bytes} chunks={} total_us(med/p95/max)={:.1}/{:.1}/{:.1} ns_per_byte_med={:.3} \
             assemble_us={:.1}/{:.1}/{:.1} digest_us={:.1}/{:.1}/{:.1} decode_us={:.1}/{:.1}/{:.1} reencode_us={:.1}/{:.1}/{:.1} (sink {sink})",
            parts.len(),
            all.0,
            all.1,
            all.2,
            all.0 * 1e3 / bytes as f64,
            a.0,
            a.1,
            a.2,
            d.0,
            d.1,
            d.2,
            c.0,
            c.1,
            c.2,
            e.0,
            e.1,
            e.2
        );
    }
}
