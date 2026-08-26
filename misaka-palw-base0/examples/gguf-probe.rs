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
}
