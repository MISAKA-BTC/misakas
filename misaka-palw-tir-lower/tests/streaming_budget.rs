//! **A checkpoint several times the memory budget converts inside the budget** (RFC-0002 Part II,
//! the streaming loader). A synthetic sharded Llama-shaped checkpoint — a few hundred MB of BF16
//! across many `.safetensors` shards and an index, written to disk here — is lowered, and its
//! integer artifact produced by `materialise_stream` into a content-addressed chunk store and
//! assembled into a `PALWTIR1` container, while a counting global allocator watches every byte the
//! process holds. The peak of the conversion (above what the process held before it) must stay
//! within the budget — a fraction of the checkpoint, and less than the embedding table alone as
//! `f32` — and nothing may have been loaded whole after being deferred.
//!
//! Calibration (the float reference over a few positions, one layer's weights resident at a time)
//! runs before the budgeted window and is reported, not budgeted: making the float reference itself
//! row-streamed is separate work (RFC-0002 Part II §II.5, known limits).
//!
//! One test only, because the allocator counts the whole process. Sizes: `TIR_BUDGET_LAYERS`
//! (default 24 layers, ≈ 270 MB), `TIR_BUDGET_MIB` (default 32); a 2 GB-disk run is
//! `TIR_BUDGET_LAYERS=100 TIR_BUDGET_MIB=48`.

use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_artifact::chunks::{ChunkStore, write_container_v1_chunked};
use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{ChunkSink, LowerOpts, StreamOpts, materialise_stream};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, TensorSource};
use misaka_palw_tir_lower::fidelity;
use rand::{Rng, SeedableRng};
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

// ───────────────────────────── the counting allocator ─────────────────────────────

struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn add(n: usize) {
    let cur = CUR.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(cur, Relaxed);
}

// SAFETY: every call forwards to the system allocator and only counts sizes on the side.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new >= l.size() {
                add(new - l.size());
            } else {
                CUR.fetch_sub(l.size() - new, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// The bytes the process holds now, and a watermark to read the peak against.
struct Window {
    base: usize,
}

impl Window {
    fn start() -> Self {
        let base = CUR.load(Relaxed);
        PEAK.store(base, Relaxed);
        Window { base }
    }
    /// The most the process held above the start, so far.
    fn peak(&self) -> usize {
        PEAK.load(Relaxed).saturating_sub(self.base)
    }
}

// ───────────────────────────── the synthetic checkpoint ─────────────────────────────

const HIDDEN: usize = 512;
const FFN: usize = 2048;
const VOCAB: usize = 32768;

fn env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Write one shard: tensors of BF16 values from a seeded generator, streamed to disk in pieces.
fn write_shard(path: &Path, tensors: &[(String, Vec<usize>)], seed: u64) -> u64 {
    let mut header = serde_json::Map::new();
    let mut off = 0u64;
    for (n, shape) in tensors {
        let bytes = shape.iter().product::<usize>() as u64 * 2;
        header.insert(n.clone(), serde_json::json!({"dtype": "BF16", "shape": shape, "data_offsets": [off, off + bytes]}));
        off += bytes;
    }
    header.insert("__metadata__".into(), serde_json::json!({"format": "pt"}));
    let h = serde_json::to_vec(&serde_json::Value::Object(header)).expect("json");
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("shard"));
    f.write_all(&(h.len() as u64).to_le_bytes()).expect("write");
    f.write_all(&h).expect("write");
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut piece = Vec::with_capacity(1 << 20);
    for (n, shape) in tensors {
        let count = shape.iter().product::<usize>();
        // Norm gains near 1, weights small.
        let (centre, spread) = if shape.len() == 1 { (1.0f32, 0.1f32) } else { (0.0, 0.06) };
        let mut left = count;
        while left > 0 {
            let k = left.min(1 << 19);
            piece.clear();
            for _ in 0..k {
                let v = centre + spread * rng.gen_range(-1.0f32..1.0);
                piece.extend_from_slice(&((v.to_bits() >> 16) as u16).to_le_bytes());
            }
            f.write_all(&piece).expect("write");
            left -= k;
        }
        let _ = n;
    }
    f.flush().expect("flush");
    off
}

/// A sharded Llama-shaped checkpoint of `layers` layers: a shard per few layers, an index, a config.
fn write_checkpoint(dir: &Path, layers: usize) -> u64 {
    std::fs::create_dir_all(dir).expect("dir");
    let config = serde_json::json!({
        "architectures": ["LlamaForCausalLM"], "model_type": "llama",
        "hidden_size": HIDDEN, "intermediate_size": FFN, "num_hidden_layers": layers,
        "num_attention_heads": 8, "num_key_value_heads": 8, "head_dim": HIDDEN / 8,
        "vocab_size": VOCAB, "max_position_embeddings": 256, "rms_norm_eps": 1e-5, "rope_theta": 10000.0,
        "hidden_act": "silu", "attention_bias": false, "mlp_bias": false, "tie_word_embeddings": false
    });
    std::fs::write(dir.join("config.json"), config.to_string()).expect("config");
    let mut groups: Vec<Vec<(String, Vec<usize>)>> = Vec::new();
    groups.push(vec![("model.embed_tokens.weight".into(), vec![VOCAB, HIDDEN])]);
    for l in 0..layers {
        let p = format!("model.layers.{l}.");
        let t = vec![
            (format!("{p}input_layernorm.weight"), vec![HIDDEN]),
            (format!("{p}self_attn.q_proj.weight"), vec![HIDDEN, HIDDEN]),
            (format!("{p}self_attn.k_proj.weight"), vec![HIDDEN, HIDDEN]),
            (format!("{p}self_attn.v_proj.weight"), vec![HIDDEN, HIDDEN]),
            (format!("{p}self_attn.o_proj.weight"), vec![HIDDEN, HIDDEN]),
            (format!("{p}post_attention_layernorm.weight"), vec![HIDDEN]),
            (format!("{p}mlp.gate_proj.weight"), vec![FFN, HIDDEN]),
            (format!("{p}mlp.up_proj.weight"), vec![FFN, HIDDEN]),
            (format!("{p}mlp.down_proj.weight"), vec![HIDDEN, FFN]),
        ];
        if l % 3 == 0 {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("a group").extend(t);
    }
    groups.push(vec![("model.norm.weight".into(), vec![HIDDEN]), ("lm_head.weight".into(), vec![VOCAB, HIDDEN])]);
    let n = groups.len();
    let mut wm = serde_json::Map::new();
    let mut total = 0u64;
    for (i, g) in groups.iter().enumerate() {
        let file = format!("model-{:05}-of-{:05}.safetensors", i + 1, n);
        total += write_shard(&dir.join(&file), g, 0xC0FFEE + i as u64);
        for (name, _) in g {
            wm.insert(name.clone(), serde_json::json!(file));
        }
    }
    std::fs::write(dir.join("model.safetensors.index.json"), serde_json::json!({"metadata": {}, "weight_map": wm}).to_string()).expect("index");
    total
}

#[test]
fn a_checkpoint_many_times_the_budget_converts_inside_it() {
    let layers = env("TIR_BUDGET_LAYERS", 24);
    let budget = env("TIR_BUDGET_MIB", 32) << 20;
    let root = std::env::temp_dir().join(format!("tir-budget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (ckdir, work) = (root.join("ckpt"), root.join("work"));
    std::fs::create_dir_all(&work).expect("work");
    let ck_bytes = write_checkpoint(&ckdir, layers);
    let table_f32 = VOCAB * HIDDEN * 4;
    eprintln!(
        "checkpoint: {layers} layers, {:.0} MB on disk; budget {:.0} MiB ({:.1}× smaller); the embedding table alone is {:.0} MiB as f32",
        ck_bytes as f64 / 1e6,
        budget as f64 / (1 << 20) as f64,
        ck_bytes as f64 / budget as f64,
        table_f32 as f64 / (1 << 20) as f64
    );
    assert!(ck_bytes > 6 * budget as u64, "the checkpoint must be several times the budget");
    assert!(table_f32 > budget, "the budget must be below one table held whole");

    // Lower, and calibrate (one layer resident at a time; reported, not budgeted).
    let cfg = std::fs::read_to_string(ckdir.join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&ckdir).expect("open");
    assert!(ck.serves_row_ranges() && ck.names().len() == layers * 9 + 3);
    let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
    let calib = fidelity::random_sequences(prep.hl.vocab, 1, 3, 7);
    let cal = Window::start();
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &|_, _| {}).expect("calibrate");
    eprintln!("calibration peak: {:.1} MiB", cal.peak() as f64 / (1 << 20) as f64);

    // The conversion: occurrence by occurrence, row block by row block, chunk by chunk.
    let opts = StreamOpts { defer_min_elems: 1 << 20, block_elems: 1 << 20 };
    let win = Window::start();
    let store = ChunkStore::open(&work.join("chunks")).expect("store");
    let mut sink = ChunkSink::new(&store);
    let m = materialise_stream(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &opts, &mut sink, &|_, _| {})
        .expect("materialise_stream");
    let after_materialise = win.peak();
    let container = work.join("artifact.palwtir");
    write_container_v1_chunked(&container, &prep.lowered.program, Vec::new(), [0u8; 64], "{}".into(), &store, &sink.artifact).expect("assemble");
    let peak = win.peak();
    let mib = |b: usize| b as f64 / (1 << 20) as f64;
    eprintln!(
        "conversion: {} tensors / {:.0} MB in {} blocks ({} instances by blocks), largest block {:.1} MiB f32; chunk buffer ≤ {:.1} MiB; peak {:.1} MiB (materialise alone {:.1}); {} chunks, {:.0} MB written, {} deduplicated",
        m.stats.instances,
        m.stats.bytes as f64 / 1e6,
        m.stats.blocks,
        m.stats.by_blocks,
        mib(m.stats.max_block_f32_bytes),
        mib(sink.peak_buffer),
        mib(peak),
        mib(after_materialise),
        sink.artifact.chunk_count(),
        store.stats().bytes_written as f64 / 1e6,
        store.stats().deduplicated
    );
    assert!(m.stats.by_blocks >= 3, "the embedding, the head and the big projections go by blocks");
    assert!(m.stats.whole.is_empty(), "nothing needed loading whole: {:?}", m.stats.whole);
    assert!(m.stats.max_block_f32_bytes <= 4 * opts.block_elems + 4 * HIDDEN, "a block is {}", m.stats.max_block_f32_bytes);
    assert!(peak <= budget, "the conversion held {:.1} MiB at its peak; the budget is {:.1} MiB", mib(peak), mib(budget));
    // The container is a valid artifact of the program: every tensor present and as declared.
    let c = PalwTirContainerV1::open(&container).expect("a valid container");
    assert_eq!(c.program, prep.lowered.program);
    assert!(std::fs::metadata(&container).expect("file").len() > m.stats.bytes);
    let _ = std::fs::remove_dir_all(&root);
}
