//! **The streaming loader's source layer** over every Hugging Face fixture: a block-wise
//! evaluation of an HL param's source expression (`weights::stream::eval_src_rows`) is, row for
//! row, the whole evaluation — for every param, every layer, every block size — and the byte-range
//! reads of a sharded checkpoint equal the whole-tensor reads.

use misaka_palw_tir_lower::weights::stream::{eval_src_rows, row_blocks, src_row_space};
use misaka_palw_tir_lower::weights::{Checkpoint, Resolver, TensorSource, eval_src, layers_of_param};
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.retain(|d| d.join("config.json").exists());
    v.sort();
    v
}

#[test]
fn row_ranges_equal_the_whole_evaluation() {
    let none = BTreeMap::new();
    let (mut streamable, mut whole_only, mut checked_rows) = (0usize, 0usize, 0usize);
    let fx = fixtures();
    assert!(!fx.is_empty(), "no fixtures on disk");
    for dir in &fx {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let spec = parse_config_str(&cfg).unwrap_or_else(|e| panic!("{name}: {e}"));
        let prog = hl::build_program(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let binding = hf_weights::bind(&spec, &prog).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ck = Checkpoint::open(dir).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(ck.serves_row_ranges());
        for (pi, d) in prog.params.iter().enumerate() {
            let layers: Vec<Option<usize>> = if d.per_layer {
                layers_of_param(&prog, pi as u32).into_iter().map(|l| Some(prog.model_layer(l))).collect()
            } else {
                vec![None]
            };
            // Layers of one param differ only in the layer number; two or three of them are enough.
            for layer in layers.into_iter().take(3) {
                let r = Resolver::new(&ck, &binding.aliases);
                let src = &binding.srcs[pi];
                let space = src_row_space(src, &r, layer, &none).unwrap_or_else(|e| panic!("{name} `{}`: {e}", d.name));
                let Some((rows, cols)) = space else {
                    whole_only += 1;
                    continue;
                };
                streamable += 1;
                let whole = eval_src(src, &r, layer, &none).unwrap_or_else(|e| panic!("{name} `{}`: {e}", d.name));
                assert_eq!(whole.numel(), rows * cols, "{name} `{}`: the row space {rows}×{cols} of {:?}", d.name, whole.shape);
                for bs in [1usize, 2, 3, 5, 64, rows.max(1)] {
                    let mut got: Vec<f32> = Vec::with_capacity(whole.numel());
                    for b in row_blocks(rows, bs) {
                        let t = eval_src_rows(src, &r, layer, &none, b.clone())
                            .unwrap_or_else(|e| panic!("{name} `{}` rows {b:?}: {e}", d.name));
                        assert_eq!(t.shape, vec![b.len(), cols]);
                        got.extend(t.data);
                        checked_rows += b.len();
                    }
                    assert_eq!(got.len(), whole.data.len(), "{name} `{}` block {bs}", d.name);
                    assert!(
                        got.iter().zip(&whole.data).all(|(a, b)| a.to_bits() == b.to_bits()),
                        "{name} `{}` (layer {layer:?}) block {bs}: the block-wise value differs from the whole",
                        d.name
                    );
                }
            }
        }
    }
    eprintln!("{} fixtures: {streamable} param instances evaluated by row ranges ({checked_rows} rows), {whole_only} whole-only", fx.len());
    assert!(streamable > 500, "only {streamable} streamable instances");
}

#[test]
fn byte_ranges_of_a_sharded_checkpoint_equal_its_whole_reads() {
    // Any fixture: every tensor read in odd pieces is the tensor read whole.
    let dir = fixtures().into_iter().find(|d| d.file_name().is_some_and(|n| n == "llama")).expect("llama fixture");
    let ck = Checkpoint::open(&dir).unwrap();
    for n in ck.names() {
        let m = ck.metadata(&n).unwrap();
        assert!(m.bytes > 0 && m.dtype == "BF16" || m.dtype == "F32" || m.dtype == "F16", "{n}: {m:?}");
        let whole = ck.read_slice(&n, 0..m.bytes).unwrap();
        assert_eq!(whole.len() as u64, m.bytes);
        let mut pieced = Vec::new();
        let mut at = 0u64;
        while at < m.bytes {
            let end = (at + 13).min(m.bytes);
            pieced.extend(ck.read_slice(&n, at..end).unwrap());
            at = end;
        }
        assert_eq!(whole, pieced, "{n}");
        // Rows decoded from a range are the whole tensor's rows.
        let t = ck.load(&n).unwrap();
        let (rows, cols) = m.rows_cols();
        let mid = ck.load_rows(&n, rows / 2..rows).unwrap();
        assert_eq!(mid.data, t.data[(rows / 2) * cols..]);
        assert!(ck.read_slice(&n, 0..m.bytes + 1).is_err());
    }
}
