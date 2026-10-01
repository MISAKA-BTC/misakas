//! **`misaka model preflight <model>`** (RFC-0002 Part II §II.2): can this model be registered, and what is
//! missing — answered from headers, before the weights are downloaded.
//!
//! The same report `palw-class preflight <model>` prints (human or JSON), from the same library
//! (`misaka_palw_sdk::preflight`). It reads the network from the CLI's `--network` (default testnet-12) and
//! needs no node. Given a registration artifact instead (a `.palwtir` file, a legacy artifact), the command keeps
//! its old meaning: the live chain's processor judges it.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use clap::Args;
use misaka_palw_sdk::preflight::{Depth, InputKind, Options, detect, run};
use std::path::{Path, PathBuf};

#[derive(Args, Debug, Clone, Default)]
pub struct ModelPreflightArgs {
    /// How far to go: `headers` (the convert stage only) or `shape` (also the chain's conditions at a height).
    #[arg(long, value_name = "DEPTH", default_value = "shape")]
    pub depth: String,
    /// The DAA the chain's conditions are judged at (default: the first height at which every fence the network
    /// schedules is in force).
    #[arg(long, value_name = "DAA")]
    pub height: Option<u64>,
    /// For a lone config.json: a directory of `*.safetensors` files, or header prefixes of them.
    #[arg(long, value_name = "DIR")]
    pub headers: Option<PathBuf>,
    /// A quant-format descriptor file (misaka.palw.quant-format.v1); repeatable.
    #[arg(long, value_name = "FILE")]
    pub quant_format: Vec<PathBuf>,
    /// A data adapter file (misaka.palw.model-adapter.v1).
    #[arg(long, value_name = "FILE")]
    pub adapter: Option<PathBuf>,
    /// The context the class would be declared at, in positions (default: the widest the program and the network admit).
    #[arg(long, value_name = "N")]
    pub max_context: Option<u32>,
    #[arg(long, value_name = "N")]
    pub tile_len: Option<u32>,
    #[arg(long, value_name = "N")]
    pub h_chunk: Option<u32>,
    /// The program's history bound is the held one.
    #[arg(long)]
    pub held: bool,
    /// The memory a seat has, in GiB (default: the reference seat).
    #[arg(long, value_name = "GIB")]
    pub seat_memory_gib: Option<u64>,
}

/// Whether `input` is a model to read the headers of (a directory with a config.json, a config.json, a .gguf), and
/// not a registration artifact the live chain judges.
pub fn is_model(input: &Path) -> bool {
    matches!(detect(input), Ok(k) if k != InputKind::Artifact)
}

pub fn model_preflight(ctx: &Ctx, input: &Path, a: &ModelPreflightArgs) -> CliResult {
    let depth =
        Depth::parse(&a.depth).ok_or_else(|| CliError::new(exit::CONFIG, format!("--depth {}: headers, shape or full", a.depth)))?;
    let defaults = Options::default();
    let opts = Options {
        depth,
        network: Some(ctx.network.clone()),
        height: a.height,
        quant_formats: a.quant_format.clone(),
        adapter: a.adapter.clone(),
        headers: a.headers.clone(),
        max_context: a.max_context,
        tile_len: a.tile_len.unwrap_or(defaults.tile_len),
        h_chunk: a.h_chunk.unwrap_or(defaults.h_chunk),
        held: a.held,
        seat_memory_gib: a.seat_memory_gib,
    };
    let report = run(input, &opts).map_err(|e| CliError::new(exit::MODEL, e))?;
    if ctx.output == OutputFormat::Json {
        println!("{}", report.to_json());
    } else {
        print!("{}", report.render());
    }
    if report.registrable() {
        return Ok(());
    }
    let first = report.blockers().first().map(|b| b.code.clone()).unwrap_or_else(|| "UNKNOWN".to_string());
    Err(CliError::new(exit::MODEL, format!("{} blocker(s); the first is {first}", report.blockers().len())))
}
