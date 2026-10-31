//! Export a converted artifact for the consensus registration acceptance test.
//! No synthetic weights/root, signing key, network activation or registration is created here.
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: export-tir-registration-fixture <declared.palwtir> <fresh.json>".into());
    }
    let path = Path::new(&args[0]);
    let entry = TirLineageV1::open_entry(path)?;
    let mut source = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let value = serde_json::json!({
        "schema": "misaka.tir.real-artifact-registration-fixture.v1",
        "name": entry.model_id,
        "program_borsh_hex": hex(&entry.class.program),
        "layout_borsh_hex": hex(&borsh::to_vec(&entry.class.layout)?),
        "tokenizer_id_hex": entry.class.tokenizer_id.to_string(),
        "artifact_root_hex": entry.artifact_root.to_string(),
        "class_id_hex": entry.class_id().to_string(),
        "artifact_sha256": hex(&digest.finalize()),
        "artifact_bytes": std::fs::metadata(path)?.len(),
        "weights_loaded": true,
        "max_context": entry.class.layout.max_context,
        "registration_performed": false,
    });
    let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&args[1])?;
    serde_json::to_writer_pretty(&mut output, &value)?;
    println!("class={} root={}", entry.class_id(), entry.artifact_root);
    Ok(())
}
