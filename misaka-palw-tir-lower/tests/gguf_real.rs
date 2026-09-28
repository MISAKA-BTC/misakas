//! **A real GGUF file, read by this crate's block decoders** (ignored; local files only):
//!
//! ```text
//! PALW_GGUF_Q=…/Qwen3.5-2B-Q4_K_M.gguf PALW_GGUF_F16=…/<the same model in F16> \
//!   cargo test --release --test gguf_real -- --ignored --nocapture
//! ```
//!
//! Every tensor of the quantised file is dequantised by `crate::gguf` and compared with the same
//! tensor of the F16 file. The errors are the quantisation's own: ≈ 3–4 % relative RMS for 4-bit
//! blocks, ≈ 1 % for 6-bit, ≈ 0.2 % for 8-bit. A wrong nibble order, scale packing or block layout
//! reads as noise (≈ 100 % and more). Tensors of more than 2^26 values (the embedding) are skipped
//! to stay under the memory rule.

use misaka_palw_tir_lower::gguf::GgufFile;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
#[ignore]
fn a_real_q4_k_m_file_decodes_to_its_f16_twin() {
    let (Ok(q), Ok(f)) = (std::env::var("PALW_GGUF_Q"), std::env::var("PALW_GGUF_F16")) else {
        eprintln!("set PALW_GGUF_Q and PALW_GGUF_F16");
        return;
    };
    let q = GgufFile::open(&PathBuf::from(q)).expect("quantised file");
    let f = GgufFile::open(&PathBuf::from(f)).expect("f16 file");
    eprintln!(
        "{}: GGUF v{}, arch {:?}, {} tensors {:?}",
        q.path.display(),
        q.version,
        q.meta.get("general.architecture"),
        q.tensors.len(),
        q.type_counts()
    );
    let mut by_type: BTreeMap<String, (usize, f64, f64)> = BTreeMap::new();
    let mut worst: Vec<(f64, String)> = Vec::new();
    for (name, t) in &q.tensors {
        let Some(ft) = f.tensors.get(name) else { continue };
        if ft.dims != t.dims || t.numel() > 1 << 26 || t.ty.is_float() {
            continue;
        }
        let a = q.tensor_f32(name).expect("decode");
        let b = f.tensor_f32(name).expect("f16");
        let (mut num, mut den) = (0f64, 0f64);
        for (x, y) in a.data.iter().zip(&b.data) {
            num += ((x - y) as f64).powi(2);
            den += (*y as f64).powi(2);
        }
        let rel = (num / den.max(1e-30)).sqrt();
        let e = by_type.entry(t.ty.name()).or_insert((0, 0f64, 0f64));
        e.0 += 1;
        e.1 += rel;
        e.2 = e.2.max(rel);
        worst.push((rel, format!("{name} ({})", t.ty.name())));
    }
    for (ty, (n, sum, max)) in &by_type {
        eprintln!("  {ty:>5}: {n:>4} tensors, relative RMS error mean {:.4} max {:.4}", sum / *n as f64, max);
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (r, n) in worst.iter().take(5) {
        eprintln!("  worst: {n} {r:.4}");
    }
    assert!(!by_type.is_empty(), "no tensor in common");
    for (ty, (_, _, max)) in &by_type {
        assert!(*max < 0.2, "{ty}: a tensor decodes to noise ({max:.3})");
    }
}
