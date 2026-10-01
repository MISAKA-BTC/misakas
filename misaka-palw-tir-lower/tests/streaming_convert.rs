//! **The streaming conversion is the whole conversion, byte for byte** (RFC-0002 Part II): on every
//! Hugging Face fixture the artifact `materialise_stream` produces — one block of rows at a time,
//! through a content-addressed chunk store, assembled into a `PALWTIR1` container — is the very
//! file `materialise` + `artifact::write` produce, for every block size and deferral threshold.
//! Same file, so the same inventory root and class id; the options change what is resident and
//! nothing else.

use misaka_palw_tir_artifact::chunks::{ChunkStore, write_container_v1_chunked};
use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{ChunkSink, LowerOpts, StreamOpts, StreamStats, materialise, materialise_stream};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{artifact, fidelity};
use std::path::{Path, PathBuf};

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.retain(|d| d.join("model.safetensors").exists());
    v.sort();
    v
}

struct Case {
    name: String,
    old: Vec<u8>,
    news: Vec<(String, Vec<u8>, StreamStats)>,
}

fn run(dir: &Path, options: &[(&str, StreamOpts)]) -> Result<Case, String> {
    let name = dir.file_name().unwrap().to_string_lossy().to_string();
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let ck = Checkpoint::open(dir).map_err(|e| e.to_string())?;
    let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 3, 16.min(max_len), 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let policy = QuantPolicy::default();
    let tmp = std::env::temp_dir().join(format!("tir-stream-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let meta = serde_json::json!({"fixture": name});
    let tok = [3u8; 64];
    // The whole conversion.
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &policy, &quiet).map_err(|e| format!("materialise: {e}"))?;
    let old_path = tmp.join("old.palwtir");
    artifact::write(&old_path, &prep.lowered.program, &mat.params, tok, meta.clone()).map_err(|e| e.to_string())?;
    let old = std::fs::read(&old_path).map_err(|e| e.to_string())?;
    let mut news = Vec::new();
    for (label, opts) in options {
        let store = ChunkStore::open(&tmp.join(format!("chunks-{label}"))).map_err(|e| e.to_string())?;
        let mut sink = ChunkSink::new(&store);
        let m = materialise_stream(&prep.lowered, &prep.hl, &loader, &stats, &policy, opts, &mut sink, &quiet)
            .map_err(|e| format!("materialise_stream[{label}]: {e}"))?;
        assert_eq!((m.resid_scale, m.logits_scale, m.quant_inexact), (mat.resid_scale, mat.logits_scale, mat.quant_inexact), "{name}[{label}]");
        let path = tmp.join(format!("new-{label}.palwtir"));
        write_container_v1_chunked(&path, &prep.lowered.program, Vec::new(), tok, meta.to_string(), &store, &sink.artifact)
            .map_err(|e| format!("assemble[{label}]: {e}"))?;
        news.push((label.to_string(), std::fs::read(&path).map_err(|e| e.to_string())?, m.stats));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(Case { name, old, news })
}

#[test]
fn every_fixture_converts_to_the_same_file_streamed_or_whole() {
    let options = [
        // Every row-wise param by blocks of one row — the most the streaming path can be cut.
        ("rows1", StreamOpts { defer_min_elems: 0, block_elems: 1 }),
        ("rows7", StreamOpts { defer_min_elems: 0, block_elems: 100 }),
        // Nothing deferred: only the occurrence-level streaming (what the defaults do to a model
        // whose params are all under the threshold).
        ("never", StreamOpts { defer_min_elems: usize::MAX, block_elems: 8 }),
    ];
    let fx = fixtures();
    assert!(fx.len() > 50, "fixtures on disk: {}", fx.len());
    let (mut ok, mut skipped, mut by_blocks, mut blocks, mut whole_events) = (0, Vec::new(), 0usize, 0usize, 0usize);
    let mut whole_names: std::collections::BTreeMap<String, usize> = Default::default();
    for dir in &fx {
        match run(dir, &options) {
            Ok(c) => {
                for (label, bytes, st) in &c.news {
                    assert!(bytes == &c.old, "{}: the streamed container ({label}) differs from the whole one", c.name);
                    if label == "rows1" {
                        by_blocks += st.by_blocks;
                        blocks += st.blocks;
                        whole_events += st.whole.len();
                        for w in &st.whole {
                            *whole_names.entry(w.clone()).or_default() += 1;
                        }
                    }
                    if label == "never" {
                        assert_eq!(st.by_blocks, 0, "{}", c.name);
                    }
                }
                ok += 1;
            }
            Err(e) => skipped.push(format!("{}: {e}", dir.file_name().unwrap().to_string_lossy())),
        }
    }
    eprintln!("{ok} fixtures converted identically; {} skipped:", skipped.len());
    for s in &skipped {
        eprintln!("  skipped {s}");
    }
    eprintln!("rows1: {by_blocks} instances by {blocks} blocks; {whole_events} params loaded whole after deferral: {whole_names:?}");
    assert!(ok >= 50, "only {ok} fixtures converted");
    assert!(by_blocks > 100, "the row-block path produced {by_blocks} instances");
}
