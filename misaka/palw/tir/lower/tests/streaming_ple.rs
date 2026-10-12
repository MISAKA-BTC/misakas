//! **The hashed n-gram table streams** (the fill hooks of `docs/design/palw/tir/generic-frontend-v1.md` §9.4): its chunks
//! are `RowKind::Mapped` params — their rows are gathered from the checkpoint's by a row map and their one scale is a
//! first pass over the used rows — so a conversion reads a block of rows at a time and never the table, and the
//! bytes are the whole conversion's (`tests/streaming_convert.rs` holds the byte equality over every fixture; this file
//! holds that the PLE tables really went by blocks, and how much was ever resident).

use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{LowerOpts, RowKind, StreamOpts, TensorSink, materialise_stream};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{LowerError, fidelity};
use std::path::Path;

#[derive(Default)]
struct Count {
    bytes: u64,
    instances: usize,
}
impl TensorSink for Count {
    fn begin(&mut self, _: u16, _: Option<u16>, _: u64) -> Result<(), LowerError> {
        self.instances += 1;
        Ok(())
    }
    fn push(&mut self, b: &[u8]) -> Result<(), LowerError> {
        self.bytes += b.len() as u64;
        Ok(())
    }
    fn end(&mut self) -> Result<(), LowerError> {
        Ok(())
    }
}

#[test]
fn the_ple_table_is_produced_by_blocks_never_whole() {
    for name in ["qwen4_ple_bigram", "qwen4_ple_trigram", "qwen4_exp"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
        // The chunks are mapped params of the table's HL param.
        let mapped: Vec<_> = prep.lowered.row_params.iter().filter(|(_, rp)| matches!(rp.kind, RowKind::Mapped(_))).collect();
        assert!(!mapped.is_empty(), "{name}: no mapped table params");
        let table = &prep.hl.params[mapped[0].1.hl as usize];
        let table_f32_bytes = table.shape.iter().product::<usize>() * 4;
        let ck = Checkpoint::open(&dir).unwrap();
        let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
        let calib = fidelity::random_sequences(prep.hl.vocab, 2, 8, 7);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
        // 64 elements a block: a few rows of the table at a time.
        let opts = StreamOpts { defer_min_elems: 0, block_elems: 64 };
        let mut sink = Count::default();
        let m = materialise_stream(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &opts, &mut sink, &quiet).unwrap();
        assert!(!m.stats.whole.iter().any(|n| n == &table.name), "{name}: the table was loaded whole for a fill: {:?}", m.stats.whole);
        assert!(m.stats.by_blocks >= mapped.len(), "{name}: {} instances by blocks of {} mapped params", m.stats.by_blocks, mapped.len());
        // What was ever resident of the table is a block of rows (the widest run a block touches), not the table.
        assert!(m.stats.max_block_f32_bytes * 4 < table_f32_bytes.max(1) || table_f32_bytes <= 4 * 64 * 4, "{name}: {} of {table_f32_bytes}", m.stats.max_block_f32_bytes);
        assert!(sink.bytes > 0);
    }
}
