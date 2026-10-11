//! Unpublished private-process node for the RFC12 driver. No public Args flag
//! injects this profile. The helper has its own required opt-in Cargo feature.
use kaspa_consensus_core::{
    config::{
        Config, ConfigBuilder,
        drill::{PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1},
        params::ForkActivation,
    },
    palw_native_settlement_v1::{PalwDnsRetirementV1, PalwSettlementPolicyV1},
};
use kaspad_lib::{
    args::Args,
    daemon::Runtime,
    private_integration::{create_private_integration_core_with_runtime, private_probe_manifest},
};

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().collect();
    let mut salt = None;
    let mut profile = false;
    let mut print = false;
    args.retain(|argument| {
        if let Some(value) = argument.strip_prefix("--private-probe-salt=") {
            salt = Some(value.to_string());
            false
        } else if argument == "--private-probe-profile=native-v1" {
            profile = true;
            false
        } else if argument == "--print-private-probe-profile" {
            print = true;
            false
        } else {
            true
        }
    });
    if !profile {
        return Err("this unpublished helper needs --private-probe-profile=native-v1".into());
    }
    let salt = PalwDrillSaltV1::from_hex(salt.as_deref().ok_or("an explicit --private-probe-salt is required")?)
        .map_err(|error| error.to_string())?;
    let mut params = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt))?;
    // TEST values, predeclared and fingerprinted. These are transport/mechanics
    // criteria, never the approved D/W/concentration or 128-bit security policy.
    params.palw_dns_retirement = Some(PalwDnsRetirementV1 {
        activation: ForkActivation::new(1),
        settlement: PalwSettlementPolicyV1 {
            settled_anchor_depth: 1,
            unique_mature_work: 1,
            max_operator_permille: 1000,
            max_class_permille: 1000,
        },
        legacy_evidence_horizon_daa: 5,
    });
    let mut config: Config = ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().build();
    config.palw_drill_genesis_salt = Some(salt);
    let args = Args::parse(args).map_err(|error| error.to_string())?;
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
