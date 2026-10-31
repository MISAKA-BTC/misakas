//! Unpublished private-process node for the RFC12 driver. No public Args flag
//! injects this profile. The helper has its own required opt-in Cargo feature.
#![allow(warnings)]
use kaspa_consensus_core::config::{
    Config,
    drill::{PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1},
    private_population::{PrivatePopulationV1, derive_private_population_v1},
};
use kaspad_lib::{
    args::Args,
    daemon::Runtime,
    private_integration::{
        create_private_integration_core_with_runtime, export_private_complete_bootstrap_v1, private_complete_check_params_v1,
        private_native_settlement_params_v1, private_probe_manifest,
    },
};

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().collect();
    let mut salt = None;
    let mut profile = None;
    let mut population_size = None;
    let mut print = false;
    let mut export_bootstrap = None;
    args.retain(|argument| {
        if let Some(value) = argument.strip_prefix("--private-probe-salt=") {
            salt = Some(value.to_string());
            false
        } else if let Some(value) = argument.strip_prefix("--private-probe-profile=") {
            profile = Some(value.to_string());
            false
        } else if let Some(value) = argument.strip_prefix("--private-probe-bonds=") {
            population_size = Some(value.to_string());
            false
        } else if argument == "--print-private-probe-profile" {
            print = true;
            false
        } else if let Some(value) = argument.strip_prefix("--private-probe-export-bootstrap=") {
            export_bootstrap = Some(value.to_string());
            false
        } else {
            true
        }
    });
    let profile = profile.as_deref().ok_or("this unpublished helper needs an explicit --private-probe-profile")?;
    let salt = PalwDrillSaltV1::from_hex(salt.as_deref().ok_or("an explicit --private-probe-salt is required")?)
        .map_err(|error| error.to_string())?;
    let population = match (profile, population_size.as_deref()) {
        ("native-v1", None) => None,
        ("complete-v1", None) => None,
        ("population-v1", Some(size)) => Some(PrivatePopulationV1::new(size.parse().map_err(|_| "invalid private bond count")?)?),
        _ => return Err("use native-v1/complete-v1 without a size, or population-v1 with explicit --private-probe-bonds".into()),
    };
    let params = match population {
        Some(population) => derive_private_population_v1(&salt, population)?.params,
        None if profile == "complete-v1" => private_complete_check_params_v1(&salt)?,
        None => palw_chain_params_v1(palw_drill_network_v1(), Some(&salt))?,
    };
    // Every private profile carries the same actual Rule E dependencies and
    // explicit mechanics policy. Ordinary ConfigBuilder retains its refusal.
    let params = private_native_settlement_params_v1(params)?;
    let mut config = Config::new(params);
    config.perf.adjust_to_consensus_params(&config.params);
    config.palw_drill_genesis_salt = Some(salt);
    config.private_population = population;
    let args = Args::parse(args).map_err(|error| error.to_string())?;
    if let Some(path) = export_bootstrap {
        if profile != "complete-v1" {
            return Err("the fixture exporter requires the exact complete-v1 construction profile".into());
        }
        let manifest = export_private_complete_bootstrap_v1(&config, std::path::Path::new(&path))?;
        println!("{}", serde_json::to_string(&manifest).map_err(|error| error.to_string())?);
        return Ok(());
    }
    println!("{}", serde_json::to_string(&private_probe_manifest(&config)).map_err(|error| error.to_string())?);
    if print {
        return Ok(());
    }
    kaspa_core::log::try_init_logger(&args.log_level);
    let (core, rpc) = create_private_integration_core_with_runtime(&Runtime::default(), &args, 4096, config)?;
    drop(rpc);
    let workers = core.start();
    core.join(workers);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("RFC12 private probe refused: {error}");
        std::process::exit(2);
    }
}
