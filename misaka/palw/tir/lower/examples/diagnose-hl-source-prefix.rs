//! Bounded failure-targeted text diagnostic on original source weights. The
//! streamed high-level float reference is compared with the publisher's HF
//! implementation separately; it is not a consensus or coverage certificate.
use misaka_palw_tir_lower::{
    fidelity,
    float_ref::stream::{OccParams, Streamed, post_logits, run_to_post},
    lower::LowerOpts,
};
use std::{io::Read, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() != 3 {
        return Err("usage: diagnose-hl-source-prefix source-directory frozen-token-file frozen-layout-job-protocol".into());
    }
    let mut bytes = vec![];
    std::fs::File::open(&a[1])?.take((64 << 10) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 10 {
        return Err("token JSON byte cap".into());
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    let ss = v["sequences"].as_array().filter(|s| s.len() == 1).ok_or("one bounded sequence")?;
    let ids = ss[0].as_array().filter(|s| !s.is_empty() && s.len() <= 64).ok_or("1..64 prefix tokens")?;
    let ids =
        ids.iter().map(|v| v.as_u64().and_then(|v| usize::try_from(v).ok()).ok_or("bounded token")).collect::<Result<Vec<_>, _>>()?;
    let mut score_bytes = vec![];
    std::fs::File::open(&a[2])?.take((64 << 10) + 1).read_to_end(&mut score_bytes)?;
    if score_bytes.len() > 64 << 10 {
        return Err("score protocol byte cap".into());
    }
    let protocol: serde_json::Value = serde_json::from_slice(&score_bytes)?;
    let score_ids = protocol["score_token_ids"].as_array().filter(|v| v.len() <= 8).ok_or("bounded score ids")?;
    let score_ids = score_ids
        .iter()
        .map(|v| v.as_u64().and_then(|v| usize::try_from(v).ok()).ok_or("bounded score id"))
        .collect::<Result<Vec<_>, _>>()?;
    let source = Path::new(&a[0]);
    let (prep, ck) = fidelity::open_model(source, &LowerOpts::default())?;
    if ids.iter().any(|t| *t >= prep.hl.vocab) {
        return Err("source vocabulary".into());
    }
    let started = std::time::Instant::now();
    let loader = Streamed::new(&prep.hl, &prep.binding, ck.as_ref()).with_lazy(65536, 128 << 20);
    let post = run_to_post(&prep.hl, &loader, std::slice::from_ref(&ids), &|i, n| eprintln!("source float occurrence{i}/{n}"))?;
    let store = loader.load(prep.hl.post, None)?;
    let logits = post_logits(&prep.hl, &store, &post[0][ids.len() - 1], *ids.last().unwrap(), ids.len() - 1)?;
    let mut top = (0..logits.len()).collect::<Vec<_>>();
    top.sort_by(|a, b| logits[*b].total_cmp(&logits[*a]).then(a.cmp(b)));
    top.truncate(5);
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.source-HL-prefix-diagnostic.v1","fed_tokens":ids,
        "reference":"current generic high-level float interpreter, streamed original checkpoint weights, f32 storage/f64 contractions",
        "vocab":logits.len(),"top5":top.iter().map(|i|serde_json::json!({"token_id":i,"score":logits[*i]})).collect::<Vec<_>>(),
        "selected_scores":score_ids.iter().filter_map(|i|logits.get(*i).map(|v|serde_json::json!({"token_id":i,"score":v}))).collect::<Vec<_>>(),
        "elapsed_seconds":started.elapsed().as_secs_f64(),"loader_row_cache_bytes":128<<20,"positions":post[0].len(),
        "source_manifest_pins_authenticated_by_this_tool":false,"source_fidelity_fullcontext_native_final_or_a6_a8_established":false})
    );
    Ok(())
}
