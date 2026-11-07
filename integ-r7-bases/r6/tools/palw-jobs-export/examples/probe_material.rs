//! Ask every material reader what it makes of a file, BY NAME.
//!
//! The explorer's answer column went empty for every 5f row and the exporter could only say
//! `None` — one value for "no file", "wrong shape" and "wrong version" alike. This prints what
//! each reader actually says, so the next empty column is one command instead of a guess.
//!
//! `cargo run --release -p misaka-palw-jobs-export --example probe_material -- <file.material>`

fn hex16(bytes: &[u8]) -> String {
    bytes.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe_material <file.material>");
    let bytes = std::fs::read(&path).expect("read the material");
    println!("{path}: {} bytes, first 16 = {}", bytes.len(), hex16(&bytes));

    match misaka_palw_base0::produce::base0_fp_material_decode_v2(&bytes) {
        Ok(f) => println!(
            "  folded (MSKFPMV2): version {}, prompt {:?}, generated {:?}",
            f.version, f.prompt_token_ids, f.generated_token_ids
        ),
        Err(e) => println!("  folded (MSKFPMV2): {e}"),
    }
    match misaka_palw_base0::produce::base0_material_decode_v1(&bytes) {
        Ok((binding, tiles, logits, generated, ..)) => println!(
            "  dense tuple: binding v{}, step_leaf_count {}, tiles {}, logit rows {}, generated {:?}",
            binding.version,
            binding.step_leaf_count,
            tiles.len(),
            logits.len(),
            generated
        ),
        Err(e) => println!("  dense tuple: {e}"),
    }
    match misaka_palw_base0::qwen36_backend::qwen36_material_decode_v1(&bytes) {
        Some(r) => println!("  qwen36 flat: generated {:?}", r.generated),
        None => println!("  qwen36 flat: does not decode"),
    }
    match misaka_palw_base0::qwen25_a16_backend::qwen25_a16_material_decode_v1(&bytes) {
        Some(r) => println!("  qwen25-a16 flat: generated {:?}", r.generated),
        None => println!("  qwen25-a16 flat: does not decode"),
    }
}
