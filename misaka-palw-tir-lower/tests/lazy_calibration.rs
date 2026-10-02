//! **L1: the float reference's calibration, row-streamed** (RFC-0002 Part II §II.9). A layer-major run holds one layer's weights; here the large
//! params are bound LAZILY — read from the checkpoint when an op asks, a stack of experts by the rows of the experts a router selected — so
//! the reference needs the largest single tensor (or one expert) resident, not a layer. The statistics are the resident run's, bit for bit; a
//! mixture's expert stack is never read whole; the cache bounds what stays resident between asks.

use misaka_palw_tir_lower::error::Result;
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::{Streamed, run_layer_major};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::weights::{Tensor, TensorMeta, TensorSource};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

/// A checkpoint that counts what it is asked for.
struct Probe {
    inner: Box<dyn TensorSource + Sync>,
    /// The largest tensor read whole, in elements, and the names of the ones read whole.
    max_whole: AtomicUsize,
    whole: std::sync::Mutex<Vec<String>>,
    /// Row-range reads, and the largest one in elements.
    row_reads: AtomicUsize,
    max_rows: AtomicUsize,
}

impl TensorSource for Probe {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.inner.shape(name)
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let t = self.inner.load(name)?;
        self.max_whole.fetch_max(t.numel(), Relaxed);
        self.whole.lock().unwrap().push(name.to_string());
        Ok(t)
    }
    fn names(&self) -> Vec<String> {
        self.inner.names()
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.inner.metadata(name)
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        self.inner.read_slice(name, range)
    }
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let t = self.inner.load_rows(name, rows)?;
        self.row_reads.fetch_add(1, Relaxed);
        self.max_rows.fetch_max(t.numel(), Relaxed);
        Ok(t)
    }
    fn serves_row_ranges(&self) -> bool {
        self.inner.serves_row_ranges()
    }
}

fn run(name: &str) -> (usize, usize, usize, usize, usize, Vec<String>) {
    let dir = fixture(name);
    let (prep, source) = fidelity::open_model(&dir, &LowerOpts::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(source.serves_row_ranges(), "{name}: a safetensors checkpoint serves row ranges");
    let seqs = fidelity::random_sequences(prep.hl.vocab, 2, 9, 5);
    // The resident run: every param of an occurrence loaded whole.
    let resident = Streamed::new(&prep.hl, &prep.binding, source.as_ref());
    let want = run_layer_major(&prep.hl, &resident, &seqs, true, false, &|_, _| {}).unwrap_or_else(|e| panic!("{name}: {e}")).stats;
    assert!(!want.is_empty(), "{name}: calibration measured something");
    // The lazy run: every param of one element or more lazy, nothing kept between asks.
    let probe = Probe { inner: source, max_whole: AtomicUsize::new(0), whole: Default::default(), row_reads: AtomicUsize::new(0), max_rows: AtomicUsize::new(0) };
    let lazy = Streamed::new(&prep.hl, &prep.binding, &probe).with_lazy(1, 0);
    let got = run_layer_major(&prep.hl, &lazy, &seqs, true, false, &|_, _| {}).unwrap_or_else(|e| panic!("{name} (lazy): {e}")).stats;
    assert_eq!(got, want, "{name}: the statistics are the resident run's");
    // And with a cache that holds everything: the same again.
    let cached = Streamed::new(&prep.hl, &prep.binding, &probe).with_lazy(1, 1 << 30);
    let got = run_layer_major(&prep.hl, &cached, &seqs, true, false, &|_, _| {}).unwrap_or_else(|e| panic!("{name} (cached): {e}")).stats;
    assert_eq!(got, want, "{name}: and the cached run's");
    // The largest param the program has, in elements.
    let largest = prep.hl.params.iter().map(|d| d.shape.iter().product::<usize>()).max().unwrap_or(0);
    (
        largest,
        probe.max_whole.load(Relaxed),
        probe.row_reads.load(Relaxed),
        probe.max_rows.load(Relaxed),
        prep.hl.params.iter().filter(|d| d.shape.len() == 3).map(|d| d.shape.iter().product::<usize>()).max().unwrap_or(0),
        probe.whole.lock().unwrap().clone(),
    )
}

#[test]
fn a_dense_decoders_calibration_is_the_same_with_every_param_lazy() {
    let (largest, max_whole, _, _, _, _) = run("hf/llama");
    assert!(max_whole <= largest, "no tensor was read whole beyond the program's largest param ({max_whole} > {largest})");
}

#[test]
fn a_mixtures_expert_stack_is_read_by_the_experts_a_router_selected_and_never_whole() {
    for name in ["hf/qwen3_moe", "hf/mixtral", "hf/granitemoe"] {
        let (_largest, _max_whole, row_reads, max_rows, stack, whole) = run(name);
        assert!(stack > 0, "{name}: the program has a stack of experts");
        assert!(row_reads > 0, "{name}: experts were read by row range");
        assert!(max_rows < stack, "{name}: one read of rows ({max_rows} elements) is a fraction of the stack ({stack})");
        let experts_whole: Vec<&String> = whole.iter().filter(|n| n.contains("expert")).collect();
        assert!(experts_whole.is_empty(), "{name}: an expert tensor was read whole: {experts_whole:?}");
    }
}
