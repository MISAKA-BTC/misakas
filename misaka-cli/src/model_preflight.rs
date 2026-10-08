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
use kaspa_rpc_core::api::rpc::RpcApi;
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
    /// A seat tier to hold the class against, `[name=]GiB`; repeatable (default: the testnet-12 fleet's tiers, 3.5 and 8 GiB).
    #[arg(long, value_name = "[NAME=]GIB")]
    pub seat_share: Vec<String>,
    /// Ask a live node (`--network`, `--rpc`): its tip is the default height, and the registry's reading of every class it holds —
    /// the ready seats, the independent operators against the seating floor, the base population — is what a class already on the
    /// chain is judged by.
    #[arg(long)]
    pub node: bool,
    /// What a node said, saved as JSON (`--save-node-facts` writes it): judged against instead of a live node.
    #[arg(long, value_name = "FILE", conflicts_with = "node")]
    pub node_facts: Option<PathBuf>,
    /// With `--node`: write the facts the node gave to this file.
    #[arg(long, value_name = "FILE", requires = "node")]
    pub save_node_facts: Option<PathBuf>,
    /// For `--depth full`: the runtime pack directory (`palw-class pack build` writes it).
    #[arg(long, value_name = "DIR")]
    pub pack: Option<PathBuf>,
    /// For `--depth full`: the artifact to check against the pack.
    #[arg(long, value_name = "FILE")]
    pub artifact_file: Option<PathBuf>,
    /// The endpoint an `hf://org/name[@revision]` repository id is read from (default https://huggingface.co).
    #[arg(long, value_name = "URL")]
    pub hf_endpoint: Option<String>,
}

/// **The node's facts for a preflight**: `getPalwModelRegistry`'s answer, as [`misaka_palw_sdk::preflight::node::NodeFacts`].
pub fn node_facts_of(network: &str, r: &kaspa_rpc_core::GetPalwModelRegistryResponse) -> misaka_palw_sdk::preflight::node::NodeFacts {
    use misaka_palw_sdk::preflight::node::{NodeClassFact, NodeFacts, NodeSeatingFact};
    NodeFacts {
        network: network.to_string(),
        tip_daa: r.tip_daa,
        seat_count: u32::from(r.seat_count),
        spare_seats: u32::from(r.spare_seats),
        bonds_with_headroom: r.bonds_with_headroom,
        classes: r
            .classes
            .iter()
            .filter(|c| !c.is_base_class && c.has_row)
            .map(|c| NodeClassFact {
                class_id: c.class_id.clone(),
                artifact_root: c.artifact_root.clone(),
                state: c.state.clone(),
                ready_seats: c.ready_seats_now,
                required_ready_seats: c.required_ready_seats,
                seating: r.seating.iter().find(|s| s.class_id == c.class_id).map(|s| NodeSeatingFact {
                    ready_operators: s.ready_operators,
                    needed_operators: s.needed_operators,
                    independent_operators: s.independent_operators,
                    needed_independent: s.needed_independent,
                    base_operators: s.base_operators,
                    licensable_share_permille: s.licensable_share_permille,
                }),
            })
            .collect(),
    }
}

/// Whether `input` is a model to read the headers of (a directory with a config.json, a config.json, a .gguf), and
/// not a registration artifact the live chain judges.
pub fn is_model(input: &Path) -> bool {
    let s = input.to_string_lossy();
    if s.starts_with("hf://") || s.starts_with("http://") || s.starts_with("https://") {
        return true;
    }
    matches!(detect(input), Ok(k) if k != InputKind::Artifact)
}

pub async fn model_preflight(ctx: &Ctx, input: &Path, a: &ModelPreflightArgs) -> CliResult {
    let depth =
        Depth::parse(&a.depth).ok_or_else(|| CliError::new(exit::CONFIG, format!("--depth {}: headers, shape or full", a.depth)))?;
    let defaults = Options::default();
    let node = if a.node {
        let reader = crate::palw_derived::connect(ctx).await?;
        let answer = reader.client.get_palw_model_registry().await;
        let _ = reader.client.disconnect().await;
        let registry =
            answer.map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelRegistry: {e} (a node built before ADR-0135 does not serve it)")))?;
        if !registry.available {
            return Err(CliError::new(exit::GENERIC, "the node keeps no PALW class state (not a ConsensusV2 network)"));
        }
        let facts = node_facts_of(&ctx.network, &registry);
        if let Some(p) = &a.save_node_facts {
            std::fs::write(p, serde_json::to_string_pretty(&facts).unwrap_or_default())
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", p.display())))?;
        }
        Some(facts)
    } else if let Some(p) = &a.node_facts {
        let text = std::fs::read_to_string(p).map_err(|e| CliError::new(exit::CONFIG, format!("--node-facts {}: {e}", p.display())))?;
        Some(misaka_palw_sdk::preflight::node::NodeFacts::parse(&text).map_err(|e| CliError::new(exit::CONFIG, e))?)
    } else {
        None
    };
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
        seat_shares: a
            .seat_share
            .iter()
            .map(|s| misaka_palw_sdk::preflight::chain::parse_seat_share(s))
            .collect::<Result<_, _>>()
            .map_err(|e| CliError::new(exit::CONFIG, e))?,
        residency_pin_below_bytes: defaults.residency_pin_below_bytes,
        node,
        full: misaka_palw_sdk::preflight::full::FullInputs { pack: a.pack.clone(), artifact: a.artifact_file.clone() },
        lora: None,
        pipeline_admission: true,
    };
    // A repository id or an http(s) base URL is read by ranges (the headers only); anything else is a local model.
    let spelled = input.to_string_lossy().to_string();
    let endpoint = a.hf_endpoint.clone().unwrap_or_else(|| "https://huggingface.co".to_string());
    let remote_base = if let Some(rest) = spelled.strip_prefix("hf://") {
        let (repo, revision) = rest.split_once('@').unwrap_or((rest, "main"));
        Some(misaka_palw_sdk::preflight::remote::hf_base_url(&endpoint, repo, revision))
    } else if spelled.starts_with("http://") || spelled.starts_with("https://") {
        Some(spelled.clone())
    } else {
        None
    };
    let report = match remote_base {
        Some(base) if base.starts_with("http://") => misaka_palw_sdk::preflight::run_remote(
            &base,
            &misaka_palw_sdk::preflight::remote::HttpRangeFetcher::default(),
            &opts,
        ),
        Some(base) => misaka_palw_sdk::preflight::run_remote(&base, &misaka_palw_tir_lower::weights::CurlFetcher::new(), &opts),
        None => run(input, &opts),
    }
    .map_err(|e| CliError::new(exit::MODEL, e))?;
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
