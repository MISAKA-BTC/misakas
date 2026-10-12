//! **A composite class shares its parent's chunks** (RFC-0002 Part II, content-addressed chunks).
//! A candidate is a parent plus an adapter (RFC-0004): its artifact is the parent's tensors
//! unchanged, followed by the adapter's section (`tests/lora.rs` pins the first half byte for
//! byte). Converting the parent and then the candidate through ONE chunk store therefore writes the
//! parent's bytes once: the candidate's conversion stores only what is new — the adapter's
//! tensors — and both containers assemble from the same chunks.

use misaka_palw_tir_artifact::chunks::{ChunkStore, write_container_v1_chunked};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, SiteStat};
use misaka_palw_tir_lower::lower::{self, ChunkSink, LowerOpts, StreamOpts, materialise_stream};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};
use misaka_palw_tir_lower::{fidelity, hf_weights, hl, lora};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn a_candidate_converted_after_its_parent_stores_only_the_adapter() {
    for name in ["llama_r16", "qwen2_all_r4"] {
        let ad_dir = root().join("hf-lora").join(name);
        let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
        let base_dir = root().join("hf").join(meta["base"].as_str().unwrap());
        let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
        let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
        let spec_p = misaka_palw_tir_lower::parse_config_str(&cfg).unwrap();
        let hl_p = hl::build_program(&spec_p).unwrap();
        let bind_p = hf_weights::bind(&spec_p, &hl_p).unwrap();
        let ck = Checkpoint::open(&base_dir).unwrap();
        let (pf, _) = ParamStore::from_source(&hl_p, &bind_p, &ck).unwrap();
        let mut spec_c = spec_p.clone();
        lora::attach(&mut spec_c, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap()).unwrap();
        let hl_c = hl::build_program(&spec_c).unwrap();
        let bind_c = hf_weights::bind(&spec_c, &hl_c).unwrap();
        let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).unwrap();
        let (cf, _) = ParamStore::from_source(&hl_c, &bind_c, &Overlay { base: &ck, over: &ad }).unwrap();
        let calib = fidelity::random_sequences(hl_p.vocab, 4, 24, 7);
        let quiet = |_: usize, _: usize| {};
        let (lp, lc) = (Resident(Arc::new(pf)), Resident(Arc::new(cf)));
        let stats_p = fidelity::calibrate(&hl_p, &lp, &calib, &quiet).unwrap();
        let lw_p = lower::lower(&hl_p, &opts).unwrap();
        let stats_c = fidelity::calibrate(&hl_c, &lc, &calib, &quiet).unwrap();
        let mut stats: BTreeMap<String, SiteStat> = stats_p.clone();
        for (k, v) in stats_c {
            if k.contains(lower::LORA_MARK) {
                stats.insert(k, v);
            }
        }
        let mut lw_c = lower::lower(&hl_c, &opts).unwrap();
        let p = lower::adapter_params_last(&mut lw_c).unwrap();
        let policy = QuantPolicy::default();
        let so = StreamOpts::default();
        let dir = std::env::temp_dir().join(format!("tir-composite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ChunkStore::open(&dir.join("chunks")).unwrap();
        // The parent, into the store.
        let mut sink_p = ChunkSink::new(&store);
        let mp = materialise_stream(&lw_p, &hl_p, &lp, &stats_p, &policy, &so, &mut sink_p, &quiet).unwrap();
        let after_parent = store.stats();
        // The candidate, into the same store.
        let mut sink_c = ChunkSink::new(&store);
        let mc = materialise_stream(&lw_c, &hl_c, &lc, &stats, &policy, &so, &mut sink_c, &quiet).unwrap();
        let after_candidate = store.stats();
        let adapter_bytes: u64 = sink_c.artifact.tensors.values().filter(|t| t.param as usize >= p).map(|t| t.bytes).sum();
        let new_bytes = after_candidate.bytes_written - after_parent.bytes_written;
        eprintln!(
            "{name}: parent {} B in {} chunks; the candidate wrote {new_bytes} B new (its adapter section is {adapter_bytes} B) and found {} chunks already stored",
            mp.stats.bytes,
            sink_p.artifact.chunk_count(),
            after_candidate.deduplicated - after_parent.deduplicated
        );
        // The candidate's tensors are the parent's plus the adapter's: what it adds to the store is
        // the adapter section (a duplicate chunk inside it costs nothing), never the parent again.
        assert!(new_bytes <= adapter_bytes, "{name}: {new_bytes} new bytes, the adapter is {adapter_bytes}");
        assert!(after_candidate.deduplicated - after_parent.deduplicated >= sink_p.artifact.chunk_count() as u64, "{name}: the parent's chunks were not found");
        assert_eq!(mc.stats.bytes, mp.stats.bytes + adapter_bytes, "{name}");
        // Both containers assemble from the one store.
        for (tag, lw, sink) in [("parent", &lw_p, &sink_p), ("candidate", &lw_c, &sink_c)] {
            let out = dir.join(format!("{tag}.palwtir"));
            write_container_v1_chunked(&out, &lw.program, Vec::new(), [0u8; 64], "{}".into(), &store, &sink.artifact).unwrap();
            assert!(misaka_palw_tir_artifact::PalwTirContainerV1::open(&out).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
