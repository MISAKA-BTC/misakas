//! Generic, data-only frontend -> common PALWTIR1 container and an independently replayable receipt.
use clap::Parser;
use misaka_palw_tir_lower::{
    admission, artifact,
    frontend_pack::{BuildRecord, FrontendPack},
    weights::Checkpoint,
};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Compile a third-party declarative frontend pack into canonical TIR and a PALWTIR1 artifact")]
struct Args {
    /// Content-addressed frontend JSON; no model-family selection is performed.
    #[arg(long)]
    frontend_pack: PathBuf,
    /// Checkpoint directory or safetensors file.
    model: PathBuf,
    /// Config JSON (default: config.json beside the checkpoint).
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    out: PathBuf,
    /// Save a reproducible build receipt (this is not a source-fidelity certificate).
    #[arg(long)]
    record: PathBuf,
    /// Independently reproduce this receipt; a mismatch preserves an existing output artifact.
    #[arg(long)]
    expect_record: Option<PathBuf>,
    #[arg(long)]
    tokenizer: Option<PathBuf>,
    /// Maximum raw source bytes per read; changes no artifact or receipt identity.
    #[arg(long,default_value_t=1<<20)]
    block_bytes: usize,
    #[arg(long, default_value_t = 64)]
    tile_len: u32,
    #[arg(long, default_value_t = 64)]
    h_chunk: u32,
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("FRONTEND_EXPANSION_LIMIT: {}", path.display()));
    }
    Ok(bytes)
}
fn run(a: Args) -> Result<(), String> {
    let dir = if a.model.is_dir() { a.model.as_path() } else { a.model.parent().unwrap_or(Path::new(".")) };
    let config_path = a.config.unwrap_or_else(|| dir.join("config.json"));
    let config = serde_json::from_slice(&read(&config_path, 2 << 20)?).map_err(|e| e.to_string())?;
    let pack = FrontendPack::read(&a.frontend_pack).map_err(|e| e.to_string())?;
    let source = Checkpoint::open(&a.model).map_err(|e| e.to_string())?;
    let mut protected = source.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    protected.extend([config_path, a.frontend_pack.clone()]);
    if let Some(p) = &a.expect_record {
        protected.push(p.clone());
    }
    if let Some(p) = &a.tokenizer {
        protected.push(p.clone());
    }
    if let Some(p) = artifact::tokenizer_path_in(dir) {
        protected.push(p);
    }
    misaka_palw_tir_lower::frontend_pack::distinct_output(&a.out, &protected).map_err(|e| e.to_string())?;
    protected.push(a.out.clone());
    misaka_palw_tir_lower::frontend_pack::distinct_output(&a.record, &protected).map_err(|e| e.to_string())?;
    let inputs = misaka_palw_tir::admit::TirAdmitInputsV1 { tile_len: a.tile_len, h_chunk: a.h_chunk, ..admission::default_inputs() };
    let compiled = pack.compile_bounded(&config, &source, &inputs, a.block_bytes).map_err(|e| e.to_string())?;
    let tokenizer = match a.tokenizer.or_else(|| artifact::tokenizer_path_in(dir)) {
        Some(path) => artifact::tokenizer_id_of(&read(&path, 64 << 20)?),
        None => [0; 64],
    };
    let expected: Option<BuildRecord> =
        a.expect_record.map(|p| serde_json::from_slice(&read(&p, 64 << 20)?).map_err(|e| e.to_string())).transpose()?;
    let c = compiled.write_checked(&a.out, &source, tokenizer, a.block_bytes, expected.as_ref()).map_err(|e| e.to_string())?;
    std::fs::write(a.record, serde_json::to_vec_pretty(&c.record).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"artifact":a.out,"frontend_hash":pack.hash(),"artifact_digest":c.record.artifact_digest,
        "tensor_bytes":c.tensor_bytes,"source_bytes":c.source_bytes,"max_read_bytes":c.max_read_bytes,"source_read_bytes":c.source_read_bytes,"saturated_values":c.record.saturated_values,
        "source_equivalence":"SOURCE_EQUIVALENCE_UNVERIFIED"})
    );
    Ok(())
}
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("palw-tir-frontend: {e}");
        std::process::exit(1);
    }
}
