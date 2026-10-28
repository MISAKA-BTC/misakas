//! Read a GGUF header and report what a converter would find, optionally decoding one tensor.
//!
//! ```text
//! gguf-probe <header-bytes-file> [tensor-name] [tensor-bytes-file]
//! ```
//!
//! The header file may be a PREFIX of a 24 GiB checkpoint — the directory lives in the first few
//! tens of megabytes, and every tensor's byte range is reported so the caller can fetch one over
//! HTTP without the rest.

use misaka_palw_base0::gguf::{dequantize, parse_directory};

fn main() {
    let mut args = std::env::args().skip(1);
    let header = args.next().expect("usage: gguf-probe <header-file> [tensor] [tensor-file]");
    let bytes = std::fs::read(&header).expect("the header file");
    let dir = parse_directory(&bytes).expect("a GGUF header");

    println!("tensors {} | data starts at {}", dir.tensors.len(), dir.data_start);
    for key in ["general.architecture", "qwen35moe.block_count", "qwen35moe.embedding_length", "qwen35moe.expert_count"] {
        if let Some(v) = dir.metadata.get(key) {
            println!("  {key} = {v:?}");
        }
    }

    let Some(name) = args.next() else {
        for t in dir.tensors.values().take(6) {
            let (a, b) = t.range();
            println!("  {:44} {:?} {:?} bytes {}..{}", t.name, t.dims, t.kind, a, b);
        }
        return;
    };
    let t = dir.tensors.get(&name).unwrap_or_else(|| panic!("no tensor {name}"));
    let (a, b) = t.range();
    println!("{} {:?} {:?} elements {} bytes {}..{}", t.name, t.dims, t.kind, t.elements(), a, b);

    let Some(path) = args.next() else { return };
    let data = std::fs::read(&path).expect("the tensor file");
    let values = dequantize(t, &data).expect("the tensor decodes");
    let n = values.len();
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / n as f64;
    let rms = (values.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / n as f64).sqrt();
    let absmax = values.iter().fold(0f32, |a, v| a.max(v.abs()));
    let zeros = values.iter().filter(|v| **v == 0.0).count();
    println!("  n {n} mean {mean:.6} rms {rms:.6} absmax {absmax:.6} zeros {zeros}");
    println!("  first 8: {:?}", &values[..8.min(n)]);

    // **What int8 costs against what the checkpoint already carries.**
    //
    // Q4_K is four bits with a scale per 32 elements; int8 with one scale per output ROW is eight
    // bits over 2,048. Which is more accurate is not obvious from the bit widths, and the answer
    // decides whether a finer weight scale is the next thing to build.
    if let Some(out_dim) = std::env::args().nth(4).and_then(|v| v.parse::<usize>().ok()) {
        let k = n / out_dim.max(1);
        let mut row_err = 0f64;
        let mut row_sig = 0f64;
        let mut grp_err = 0f64;
        for c in 0..out_dim {
            let row = &values[c * k..(c + 1) * k];
            let absmax = row.iter().fold(0f32, |a, v| a.max(v.abs())) as f64;
            let scale = if absmax > 0.0 { absmax / 127.0 } else { 1.0 };
            for v in row {
                let q = (*v as f64 / scale).round().clamp(-127.0, 127.0) * scale;
                row_err += (q - *v as f64).powi(2);
                row_sig += (*v as f64).powi(2);
            }
            // The same eight bits with a scale per 32, which is Q4_K's granularity.
            for group in row.chunks(32) {
                let a = group.iter().fold(0f32, |a, v| a.max(v.abs())) as f64;
                let s = if a > 0.0 { a / 127.0 } else { 1.0 };
                for v in group {
                    let q = (*v as f64 / s).round().clamp(-127.0, 127.0) * s;
                    grp_err += (q - *v as f64).powi(2);
                }
            }
        }
        println!(
            "  int8 per row    relative error {:.4e}\n  int8 per 32     relative error {:.4e}",
            (row_err / row_sig).sqrt(),
            (grp_err / row_sig).sqrt()
        );
    }
}
