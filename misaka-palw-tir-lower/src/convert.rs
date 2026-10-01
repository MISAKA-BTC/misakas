//! **The streaming converter** (RFC-0002 Part II): a prepared model (spec → HL → TIR program), a
//! loader over its checkpoint, the calibration statistics and a quantisation policy in; a
//! `PALWTIR1` container out — produced one tensor, and for the big ones one block of rows, at a time
//! through a content-addressed chunk store ([`crate::lower::stream`]), and assembled in inventory
//! order from the chunks ([`misaka_palw_tir_artifact::chunks`]).
//!
//! **A function of its inputs, nothing else.** The container's tensors depend on the program, the
//! checkpoint's tensors, the calibration statistics and the policy — not on the machine, the thread
//! count, the memory budget, the block or chunk sizes (`tests/streaming_convert.rs` pins each of
//! them on every fixture). Anyone holding the public source and a runtime pack that names those
//! inputs rebuilds the same artifact, and so the same inventory root, class id and file digest:
//! that is what `palw-class pack build` and `pack verify` are.

use crate::error::{LowerError, Result};
use crate::float_ref::SiteStat;
use crate::float_ref::stream::OccParams;
use crate::fidelity::Prepared;
use crate::lower::{ChunkSink, StreamMaterialised, StreamOpts, StreamStats, materialise_stream};
use crate::quant::QuantPolicy;
use misaka_palw_tir_artifact::chunks::{ChunkStore, ChunkStoreStats, write_container_v1_chunked};
use std::collections::BTreeMap;
use std::path::Path;

/// What goes into the container besides the tensors.
pub struct ConvertOpts<'a> {
    /// How the work is cut (changes nothing in the result).
    pub stream: StreamOpts,
    /// `borsh(PalwTirLayoutV1)`, or empty (a lowerer's output before `palw-class declare-layout`).
    pub layout: Vec<u8>,
    /// The tokenizer id the class binds ([`crate::artifact::tokenizer_id_of`]), zero when none.
    pub tokenizer_id: [u8; 64],
    /// The container's provenance JSON, composed from what the materialisation found (the
    /// residual and logit scales); enters no identity.
    pub meta: &'a dyn Fn(&StreamMaterialised) -> serde_json::Value,
    /// Keep the chunk store after the container is written (it deduplicates later conversions: a
    /// composite's parent chunks, a rerun). Off: the store directory is removed.
    pub keep_chunks: bool,
}

/// What a conversion did.
#[derive(Clone, Debug)]
pub struct ConvertReport {
    /// The container's file digest (`misaka_palw_tir_artifact::file_digest_v1`), hex.
    pub file_digest: String,
    pub file_bytes: u64,
    pub resid_scale: f64,
    pub logits_scale: f64,
    pub quant_inexact: usize,
    pub stream: StreamStats,
    pub store: ChunkStoreStats,
    /// Chunks the artifact names, and how many of them are distinct (equal chunks are stored once).
    pub chunks: usize,
    pub distinct_chunks: usize,
    /// The most the chunk writer ever buffered.
    pub peak_chunk_buffer: usize,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Convert `prep` to the container `out`, spooling chunks in `store_dir`.
#[allow(clippy::too_many_arguments)]
pub fn convert_to_container(
    prep: &Prepared,
    loader: &dyn OccParams,
    calib: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    store_dir: &Path,
    out: &Path,
    opts: &ConvertOpts<'_>,
    progress: &dyn Fn(usize, usize),
) -> Result<ConvertReport> {
    let store = ChunkStore::open(store_dir).map_err(|e| LowerError::Io(format!("{}: {e}", store_dir.display())))?;
    let mut sink = ChunkSink::new(&store);
    let m = materialise_stream(&prep.lowered, &prep.hl, loader, calib, policy, &opts.stream, &mut sink, progress)?;
    let meta = (opts.meta)(&m).to_string();
    let digest = write_container_v1_chunked(out, &prep.lowered.program, opts.layout.clone(), opts.tokenizer_id, meta, &store, &sink.artifact)
        .map_err(|e| LowerError::Io(e.to_string()))?;
    let report = ConvertReport {
        file_digest: hex(&digest),
        file_bytes: std::fs::metadata(out).map_err(|e| LowerError::Io(e.to_string()))?.len(),
        resid_scale: m.resid_scale,
        logits_scale: m.logits_scale,
        quant_inexact: m.quant_inexact,
        stream: m.stats,
        store: store.stats(),
        chunks: sink.artifact.chunk_count(),
        distinct_chunks: sink.artifact.distinct_chunks().len(),
        peak_chunk_buffer: sink.peak_buffer,
    };
    if !opts.keep_chunks {
        drop(sink);
        let _ = std::fs::remove_dir_all(store_dir);
    }
    Ok(report)
}
