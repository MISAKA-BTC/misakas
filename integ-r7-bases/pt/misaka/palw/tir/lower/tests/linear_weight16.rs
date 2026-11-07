//! Explicit decoder precision profile: legacy bytes, streamed split rows, expert rows and bounds.
use misaka_palw_tir_lower::{
    fidelity,
    float_ref::stream::Streamed,
    lower::{LowerOpts, RowKind, StreamOpts, TensorSink, materialise, materialise_stream},
    quant::QuantPolicy,
    weights::Checkpoint,
};
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
struct Sink {
    tensors: BTreeMap<(u16, Option<u16>), Vec<u8>>,
    current: Option<((u16, Option<u16>), usize)>,
}
impl TensorSink for Sink {
    fn begin(&mut self, p: u16, l: Option<u16>, bytes: u64) -> misaka_palw_tir_lower::error::Result<()> {
        self.tensors.insert((p, l), Vec::new());
        self.current = Some(((p, l), bytes as usize));
        Ok(())
    }
    fn push(&mut self, b: &[u8]) -> misaka_palw_tir_lower::error::Result<()> {
        self.tensors.get_mut(&self.current.unwrap().0).unwrap().extend_from_slice(b);
        Ok(())
    }
    fn end(&mut self) -> misaka_palw_tir_lower::error::Result<()> {
        let (key, bytes) = self.current.take().unwrap();
        assert_eq!(self.tensors[&key].len(), bytes);
        Ok(())
    }
}

#[test]
fn legacy_profile_is_identical_and_wide_split_and_expert_rows_stream_exactly() {
    let mut split = 0;
    let mut experts = 0;
    for name in ["llama", "mixtral", "qwen3_5"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let old = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
        let explicit = fidelity::prepare(&cfg, &LowerOpts { linear_weight_bits: 8, ..LowerOpts::default() }).unwrap();
        let wide = fidelity::prepare(&cfg, &LowerOpts { linear_weight_bits: 16, ..LowerOpts::default() }).unwrap();
        assert_eq!(old.lowered.program.encode(), explicit.lowered.program.encode());
        assert_ne!(old.lowered.program.encode(), wide.lowered.program.encode());
        let ck = Checkpoint::open(&dir).unwrap();
        let loader = Streamed::new(&old.hl, &old.binding, &ck);
        let sequences = fidelity::random_sequences(old.hl.vocab, 2, 8, 7);
        let stats = fidelity::calibrate(&old.hl, &loader, &sequences, &|_, _| {}).unwrap();
        let policy = QuantPolicy::default();
        let mat = materialise(&old.lowered, &old.hl, &loader, &stats, &policy, &|_, _| {}).unwrap();
        let same = materialise(&explicit.lowered, &explicit.hl, &loader, &stats, &policy, &|_, _| {}).unwrap();
        assert_eq!(mat.params.tensors, same.params.tensors);
        let w = materialise(&wide.lowered, &wide.hl, &loader, &stats, &policy, &|_, _| {}).unwrap();
        assert_ne!(mat.params.tensors, w.params.tensors);
        for (param, rp) in &wide.lowered.row_params {
            let d = wide.lowered.program.params.iter().find(|d| &d.name == param).unwrap();
            assert_ne!(d.dtype, misaka_palw_tir::DType::I8, "{name}: missed projection {param}");
            if let RowKind::SplitMain { bits, f_max, .. } = rp.kind {
                assert_eq!(bits, 16);
                assert!(f_max <= 24);
                split += 1;
            }
            if d.shape.len() == 3 {
                experts += 1;
            }
        }
        for elements in [1, 100, usize::MAX] {
            let mut sink = Sink::default();
            let report = materialise_stream(
                &wide.lowered,
                &wide.hl,
                &loader,
                &stats,
                &policy,
                &StreamOpts { defer_min_elems: 0, block_elems: elements },
                &mut sink,
                &|_, _| {},
            )
            .unwrap();
            assert_eq!(report.logits_scale, w.logits_scale);
            let expected: BTreeMap<_, _> = w.params.tensors.iter().map(|(k, t)| (*k, t.le_bytes())).collect();
            assert_eq!(sink.tensors, expected, "{name}: block {elements}");
        }
        let logits = fidelity::int_logits(&wide.lowered.program, &w.params, &sequences[0], w.logits_scale, &|_| {}).unwrap();
        assert!(logits.iter().flatten().all(|v| v.is_finite()));
    }
    assert!(split > 0, "split profile was not exercised");
    assert!(experts > 0, "expert profile was not exercised");
}
#[test]
fn unsupported_main_width_is_refused_before_lowering() {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/llama/config.json")).unwrap();
    for bits in [0, 7, 15, 32] {
        assert!(fidelity::prepare(&cfg, &LowerOpts { linear_weight_bits: bits, ..LowerOpts::default() }).is_err());
    }
}
