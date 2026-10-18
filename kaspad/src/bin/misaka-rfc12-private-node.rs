//! Explicit feature-only loopback private construction entry point. Public
//! kaspad Args/presets never select this constructor. Real PoW and exact salted
//! monetary genesis remain mandatory; private mechanics are not release approval.
use kaspa_consensus_core::config::{
    Config,
    drill::{PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1},
    private_population::{PrivatePopulationV1, derive_private_population_v1},
};
use kaspad_lib::{
    args::Args,
    daemon::{DESIRED_DAEMON_SOFT_FD_LIMIT, MINIMUM_DAEMON_SOFT_FD_LIMIT, Runtime},
    private_integration::{
        AdditionalCampaignWorkersV1, PrivateKernelMaterialReplacementSpecV4, PrivateKernelPaidCheckSpecV4,
        PrivateKernelPaidMaterialGenerationSpecV5, PrivateKernelPaidWorkSpecV4, PrivateNativeCustodySpecV3,
        PrivatePaidMaterialGenerationExportSpecV5, PrivatePaidObservationClaimsV5, PrivateRecoveryAckFeeSpecV4,
        PrivateSourceBootstrapCheckerSpecV1, PrivateSourceBootstrapProducerSpecV1,
        create_private_integration_core_with_campaign_workers_v1, create_private_integration_core_with_native_custody_v3,
        create_private_integration_core_with_native_custody_v4, create_private_integration_core_with_paid_observations_v5,
        create_private_integration_core_with_runtime, create_private_integration_core_with_source_workers_v1,
        export_private_complete_bootstrap_v1, export_private_paid_bootstrap_v3, export_private_paid_bootstrap_v4,
        export_private_paid_program_work_v3, export_private_source_paid_bootstrap_v1, prepare_private_integration_probe,
        private_complete_check_params_v1, private_native_settlement_params_v1, private_paid_claim_params_v3,
        private_paid_claim_params_v4, private_probe_manifest, private_source_bootstrap_paid_params_with_consumer_v6,
        private_source_bootstrap_params_v1,
    },
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
const USAGE: &str = "misaka-rfc12-private-node (requires --features private-integration-config)
--private-probe-profile=native-v1|population-v1|complete-v1|paid-v3|paid-v4|source-v1|source-paid-v1 --private-probe-salt=HEX --testnet --netsuffix=12
source-v1/source-paid-v1 require --private-probe-source-policy=ABSOLUTE_BORSH (explicit nonzero activation and full policy;
  no values are selected or approved) and optionally --private-probe-source-check=ABSOLUTE_JSON
  (one actual drawn slot, registered model material, independent carrier key/input and finite resource/fee caps).
  --private-probe-source-open=ABSOLUTE_JSON opens one actual native claim with its real producer key;
  optional production generates direct Program work and submits separate immutable seal/reveal/open carriers;
  source-paid-v1 combines the exact paid-v4 base and this explicit source policy on every node.
  --private-probe-paid-bootstrap-source-policy=ABSOLUTE_BORSH optionally arms the distinct V6 consumer
  (u64 nonzero scheduled activation, PaidBootstrapSourcePolicyV6); all nodes must use the exact same policy.
population-v1 additionally requires --private-probe-bonds=N (actual funded registry only).
Read-only mode: --print-private-probe-profile
Offline export: --private-probe-export-bootstrap=FRESH_DIRECTORY (complete-v1, paid-v3, paid-v4 or source-paid-v1)
Offline actual paid Program work: --private-probe-paid-request=FILE --private-probe-export-paid-work=FRESH_DIRECTORY
paid-v3/paid-v4 node mode, RFC-0008 PAV3 native custody (unverified, unapproved mechanics):
  --private-probe-native-provider-keys=SEED_FILE@TXID:INDEX,SEED_FILE@TXID:INDEX[,...] (fixture seats, >=2 operators)
  --private-probe-native-provider-listen=ADDR:PORT (serve them to other private nodes; omit to serve this producer)
  --private-probe-native-exchange=ADDR:PORT (this node's producer seals through that provider service, signing each
    request with its --palw-producer-key; responses are verified with the chain's registered roster keys)
  --private-probe-native-network=loopback|remote (default loopback; remote admits separate hosts — every frame is
    ML-DSA-87 authenticated, nothing is encrypted: the exchanged material is public)
paid-v4 hosted providers additionally require an explicit ACK carrier fee payer:
  --private-probe-recovery-ack-key=ABSOLUTE_SEED_FILE
  --private-probe-recovery-ack-funding=TXID:INDEX[,TXID:INDEX,...]
  --private-probe-recovery-ack-fee-cap=SOMPI (maximum per carrier; same-key confirmed change may fund later ACKs)
  --private-probe-native-recovery-providers=TXID:INDEX,TXID:INDEX[,...] (v4 producing custody nodes; sorted distinct 2..8)
  --private-probe-paid-generation-export=ABSOLUTE_JSON (readonly own-state original-consent export for V5;
    exact expected sequence/current consent; immutable output, no payer/producer required)
  --private-probe-paid-observe-claims=HASH64[,HASH64,...] (1..64 exact base claims; readonly native facts,
    selected approval and full one-time settlement remain UNKNOWN without their complete evidence)
  --private-probe-kernel-paid-work=ABSOLUTE_JSON (explicit all-target producer work, owned outbox,
    separate producer carrier payer/funding and input/material/workspace/fee limits; never native acceptance by itself;
    V6 new work additionally needs source_lock_funding_outpoint + maximum_source_lock_fee_sompi,
    independent of original claim inputs; no Fund operation is synthesized)
  --private-probe-kernel-paid-check=ABSOLUTE_JSON (explicit assigned checker bond/key, provider endpoint,
    acquisition limits and separate receipt payer/funding; checker-only mode needs no producer exchange)
  --private-probe-kernel-paid-material-generation-v5=ABSOLUTE_JSON (exact original signed claim continuation,
    separate explicit funding slot/fee cap; exclusive with paid-work and material-replacement workers)
  --private-probe-kernel-paid-material-generation-v7=ABSOLUTE_JSON (version7 private authorization,
    exact original signed V6 source-policy commit; same native generation metadata, distinct immutable V7 WAL,
    exclusive with V5/original producer/replacement; no new enrollment/context/source draw)
  --private-probe-kernel-material-replacement=ABSOLUTE_JSON (original producer's explicit replacement sender,
    immutable context/bundle/intent, input byte/heap limits and durable proposal WAL; provider funds its carrier)
Node mode additionally needs explicit loopback P2P/gRPC/wRPC listeners, absolute owned --appdir,
--nodnsseed --outpeers=0 --disable-upnp. Ordinary node flags are parsed by the shipped Args parser.
The manifest exposes exact policy/monetary identity and releaseApproval=false; it does not prove
useful-work maturity, source independence, 128-bit security, pruned IBD, market fills or resource acceptance.";
#[derive(Debug)]
struct PrivateFlags {
    values: BTreeMap<String, String>,
    print: bool,
    help: bool,
    ordinary: Vec<String>,
}
fn split(args: Vec<String>) -> Result<PrivateFlags, String> {
    let mut out = PrivateFlags {
        values: BTreeMap::new(),
        print: false,
        help: false,
        ordinary: vec![args.first().cloned().unwrap_or_else(|| "misaka-rfc12-private-node".into())],
    };
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--print-private-probe-profile" {
            if out.print {
                return Err("duplicate private print option".into());
            }
            out.print = true;
        } else if matches!(arg.as_str(), "--help" | "-h") {
            out.help = true;
        } else {
            let (name, inline) = arg.split_once('=').map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
            if name.starts_with("--private-probe-") {
                if !matches!(
                    name,
                    "--private-probe-profile"
                        | "--private-probe-salt"
                        | "--private-probe-bonds"
                        | "--private-probe-export-bootstrap"
                        | "--private-probe-paid-request"
                        | "--private-probe-export-paid-work"
                        | "--private-probe-native-provider-keys"
                        | "--private-probe-native-provider-listen"
                        | "--private-probe-native-exchange"
                        | "--private-probe-native-network"
                        | "--private-probe-recovery-ack-key"
                        | "--private-probe-recovery-ack-funding"
                        | "--private-probe-recovery-ack-fee-cap"
                        | "--private-probe-native-recovery-providers"
                        | "--private-probe-kernel-paid-work"
                        | "--private-probe-kernel-paid-check"
                        | "--private-probe-kernel-paid-material-generation-v5"
                        | "--private-probe-kernel-paid-material-generation-v7"
                        | "--private-probe-kernel-material-replacement"
                        | "--private-probe-source-policy"
                        | "--private-probe-paid-bootstrap-source-policy"
                        | "--private-probe-source-check"
                        | "--private-probe-source-open"
                        | "--private-probe-paid-generation-export"
                        | "--private-probe-paid-observe-claims"
                ) {
                    return Err(format!("unknown private option {name}"));
                }
                let value = if let Some(v) = inline {
                    v.to_owned()
                } else {
                    i += 1;
                    args.get(i).filter(|v| !v.starts_with("--")).ok_or_else(|| format!("{name} needs a value"))?.clone()
                };
                if value.is_empty() || out.values.insert(name.to_owned(), value).is_some() {
                    return Err(format!("empty or duplicate private option {name}"));
                }
            } else {
                out.ordinary.push(arg.clone());
            }
        }
        i += 1;
    }
    Ok(out)
}
fn run(raw: Vec<String>) -> Result<(), String> {
    let mut flags = split(raw)?;
    if flags.help {
        println!("{USAGE}");
        return Ok(());
    }
    let profile = flags.values.remove("--private-probe-profile").ok_or("explicit private profile required")?;
    let salt = PalwDrillSaltV1::from_hex(&flags.values.remove("--private-probe-salt").ok_or("explicit private salt required")?)
        .map_err(|e| e.to_string())?;
    let bonds = flags
        .values
        .remove("--private-probe-bonds")
        .map(|n| n.parse::<u32>().map_err(|_| "private bond size must be u32".to_owned()))
        .transpose()?;
    let bootstrap = flags.values.remove("--private-probe-export-bootstrap");
    let paid_request = flags.values.remove("--private-probe-paid-request");
    let paid_output = flags.values.remove("--private-probe-export-paid-work");
    let source_profile = matches!(profile.as_str(), "source-v1" | "source-paid-v1");
    let paid_v4_profile = matches!(profile.as_str(), "paid-v4" | "source-paid-v1");
    let source_policy = flags.values.remove("--private-probe-source-policy");
    if source_profile != source_policy.is_some() {
        return Err("source-v1/source-paid-v1 require their explicit policy file and other profiles cannot use it".into());
    }
    let paid_bootstrap_source_policy = flags.values.remove("--private-probe-paid-bootstrap-source-policy");
    if paid_bootstrap_source_policy.is_some() && profile != "source-paid-v1" {
        return Err("V6 downstream source policy requires explicit source-paid-v1 profile".into());
    }
    let source_checker_file = flags.values.remove("--private-probe-source-check");
    if source_checker_file.is_some() && (!source_profile || flags.print || bootstrap.is_some() || paid_output.is_some()) {
        return Err("source checker requires explicit source-v1/source-paid-v1 node mode".into());
    }
    let source_checker = source_checker_file.map(|file| PrivateSourceBootstrapCheckerSpecV1::read(Path::new(&file))).transpose()?;
    let source_producer = flags
        .values
        .remove("--private-probe-source-open")
        .map(|file| {
            if !source_profile || flags.print || bootstrap.is_some() || paid_output.is_some() {
                return Err("source opening requires explicit source-v1/source-paid-v1 node mode".into());
            }
            PrivateSourceBootstrapProducerSpecV1::read(Path::new(&file))
        })
        .transpose()?;
    let generation_export = flags
        .values
        .remove("--private-probe-paid-generation-export")
        .map(|file| {
            if !paid_v4_profile || flags.print || bootstrap.is_some() || paid_output.is_some() {
                return Err("readonly generation export requires paid-v4/source-paid-v1 node mode".into());
            }
            PrivatePaidMaterialGenerationExportSpecV5::read(Path::new(&file))
        })
        .transpose()?;
    let observations = match flags.values.remove("--private-probe-paid-observe-claims") {
        Some(raw) => {
            if !paid_v4_profile || flags.print || bootstrap.is_some() || paid_output.is_some() {
                return Err("paid observation claims require explicit paid-v4 node mode".into());
            }
            PrivatePaidObservationClaimsV5::parse(&raw)?
        }
        None => PrivatePaidObservationClaimsV5::empty(),
    };
    let native = PrivateNativeCustodySpecV3::parse(
        flags.values.remove("--private-probe-native-provider-keys").as_deref(),
        flags.values.remove("--private-probe-native-provider-listen").as_deref(),
        flags.values.remove("--private-probe-native-exchange").as_deref(),
        flags.values.remove("--private-probe-native-network").as_deref(),
    )?;
    let ack_fees = PrivateRecoveryAckFeeSpecV4::parse(
        flags.values.remove("--private-probe-recovery-ack-key").as_deref(),
        flags.values.remove("--private-probe-recovery-ack-funding").as_deref(),
        flags.values.remove("--private-probe-recovery-ack-fee-cap").as_deref(),
    )?;
    let recovery_providers = flags
        .values
        .remove("--private-probe-native-recovery-providers")
        .map(|raw| kaspad_lib::private_integration::parse_private_recovery_provider_roster_v4(&raw))
        .transpose()?
        .unwrap_or_default();
    let kernel_work_file = flags.values.remove("--private-probe-kernel-paid-work");
    if kernel_work_file.is_some()
        && (!paid_v4_profile || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("kernel paid-work options require an explicit paid-v4 custody producer node".into());
    }
    let kernel_work = kernel_work_file
        .map(|file| {
            let path = Path::new(&file);
            if !path.is_absolute() {
                return Err("kernel paid-work specification path must be absolute".into());
            }
            PrivateKernelPaidWorkSpecV4::read(path)
        })
        .transpose()?;
    let kernel_checker_file = flags.values.remove("--private-probe-kernel-paid-check");
    if kernel_checker_file.is_some() && (!paid_v4_profile || flags.print || bootstrap.is_some() || paid_output.is_some()) {
        return Err("kernel checker options require explicit paid-v4 node mode".into());
    }
    let kernel_checker = kernel_checker_file
        .map(|file| {
            let path = Path::new(&file);
            if !path.is_absolute() {
                return Err("kernel checker specification path must be absolute".into());
            }
            PrivateKernelPaidCheckSpecV4::read(path)
        })
        .transpose()?;
    let kernel_replacement_file = flags.values.remove("--private-probe-kernel-material-replacement");
    if kernel_replacement_file.is_some()
        && (!paid_v4_profile || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("kernel material replacement requires an explicit paid-v4 custody producer node".into());
    }
    let kernel_replacement = kernel_replacement_file
        .map(|file| {
            let path = Path::new(&file);
            if !path.is_absolute() {
                return Err("kernel material replacement specification path must be absolute".into());
            }
            PrivateKernelMaterialReplacementSpecV4::read(path)
        })
        .transpose()?;
    let generation_v5 = flags.values.remove("--private-probe-kernel-paid-material-generation-v5");
    let generation_v7 = flags.values.remove("--private-probe-kernel-paid-material-generation-v7");
    if generation_v5.is_some() && generation_v7.is_some() {
        return Err("one immutable continuation slot permits only one explicit V5 or V7 namespace".into());
    }
    let kernel_generation = if let Some(file) = generation_v7 {
        if profile != "source-paid-v1"
            || paid_bootstrap_source_policy.is_none()
            || flags.print
            || bootstrap.is_some()
            || paid_output.is_some()
        {
            return Err("V7 continuation requires source-paid-v1 node mode and its explicit V6 source policy".into());
        }
        PrivateKernelPaidMaterialGenerationSpecV5::read_v7(Path::new(&file)).map(Some)?
    } else if let Some(file) = generation_v5 {
        if !paid_v4_profile || flags.print || bootstrap.is_some() || paid_output.is_some() {
            return Err("V5 continuation requires paid-v4 node mode".into());
        }
        PrivateKernelPaidMaterialGenerationSpecV5::read(Path::new(&file)).map(Some)?
    } else {
        None
    };
    if kernel_generation.is_some() && (kernel_work.is_some() || kernel_replacement.is_some()) {
        return Err("V5 continuation cannot run with the original paid producer/replacement worker".into());
    }
    if !recovery_providers.is_empty()
        && (!paid_v4_profile || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("recovery provider roster requires paid-v4 custody node mode".into());
    }
    if ack_fees.is_some() && (!paid_v4_profile || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some()) {
        return Err("recovery ACK fee options require paid-v4 hosted provider node mode".into());
    }
    if native.is_some()
        && (!matches!(profile.as_str(), "paid-v3" | "paid-v4" | "source-paid-v1")
            || flags.print
            || bootstrap.is_some()
            || paid_output.is_some())
    {
        return Err("the PAV3 native custody options require paid-v3 or paid-v4 node mode".into());
    }
    if !flags.values.is_empty() {
        return Err("unconsumed private option".into());
    }
    if usize::from(flags.print) + usize::from(bootstrap.is_some()) + usize::from(paid_output.is_some()) > 1
        || paid_request.is_some() != paid_output.is_some()
    {
        return Err("private manifest/bootstrap/paid-export modes are exclusive; paid export needs its request".into());
    }
    let args = match Args::parse(flags.ordinary) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp) => e.exit(),
        Err(e) => return Err(e.to_string()),
    };
    if args.network() != palw_drill_network_v1() || args.palw_drill_genesis_salt.is_some() || args.override_params_file.is_some() {
        return Err("private entry requires testnet-12 and refuses ordinary genesis/parameter overrides".into());
    }
    let population = match (profile.as_str(), bonds) {
        ("population-v1", Some(n)) => Some(PrivatePopulationV1::new(n)?),
        ("population-v1", None) => return Err("population-v1 requires an explicit funded size".into()),
        (_, Some(_)) => return Err("only population-v1 accepts a registry size".into()),
        _ => None,
    };
    let params = match profile.as_str() {
        "source-v1" => private_source_bootstrap_params_v1(&salt, Path::new(source_policy.as_ref().ok_or("source policy absent")?))?,
        "source-paid-v1" => private_source_bootstrap_paid_params_with_consumer_v6(
            &salt,
            Path::new(source_policy.as_ref().ok_or("source policy absent")?),
            paid_bootstrap_source_policy.as_deref().map(Path::new),
        )?,
        "complete-v1" => private_complete_check_params_v1(&salt)?,
        "paid-v3" => private_paid_claim_params_v3(&salt)?,
        "paid-v4" => private_paid_claim_params_v4(&salt)?,
        "native-v1" => private_native_settlement_params_v1(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt))?)?,
        "population-v1" => {
            private_native_settlement_params_v1(derive_private_population_v1(&salt, population.expect("size checked"))?.params)?
        }
        _ => return Err("unallocated private profile".into()),
    };
    if params.skip_proof_of_work {
        return Err("private fixture must retain real PoW".into());
    }
    let mut config = Config::new(params);
    config.palw_drill_genesis_salt = Some(salt);
    config.private_population = population;
    if flags.print {
        println!("{}", serde_json::to_string_pretty(&private_probe_manifest(&config)).map_err(|e| e.to_string())?);
        return Ok(());
    }
    if let Some(output) = bootstrap {
        let exported = match profile.as_str() {
            "complete-v1" => export_private_complete_bootstrap_v1(&config, Path::new(&output))?,
            "paid-v3" => export_private_paid_bootstrap_v3(&config, Path::new(&output))?,
            "paid-v4" => export_private_paid_bootstrap_v4(&config, Path::new(&output))?,
            "source-paid-v1" => export_private_source_paid_bootstrap_v1(&config, Path::new(&output))?,
            _ => return Err("bootstrap export requires complete-v1, paid-v3, paid-v4 or source-paid-v1".into()),
        };
        println!("{}", serde_json::to_string_pretty(&exported).map_err(|e| e.to_string())?);
        return Ok(());
    }
    if let (Some(request), Some(output)) = (paid_request, paid_output) {
        println!(
            "{}",
            serde_json::to_string_pretty(&export_private_paid_program_work_v3(&config, Path::new(&request), Path::new(&output))?)
                .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    // Ownership must be pinned before the logger can make a nonempty directory.
    prepare_private_integration_probe(&args, &config)?;
    let limit = kaspa_utils::fd_budget::try_set_fd_limit(DESIRED_DAEMON_SOFT_FD_LIMIT).map_err(|e| e.to_string())?;
    if limit < MINIMUM_DAEMON_SOFT_FD_LIMIT {
        return Err(format!("private node needs at least {MINIMUM_DAEMON_SOFT_FD_LIMIT} file descriptors"));
    }
    let available = i32::try_from(limit).map_err(|_| "private descriptor limit exceeds the node budget type")?;
    let rpc_clients = i32::try_from(args.rpc_max_clients).map_err(|_| "RPC client count exceeds the node budget type")?;
    let inbound = i32::try_from(args.inbound_limit).map_err(|_| "inbound count exceeds the node budget type")?;
    let outbound = i32::try_from(args.outbound_target).map_err(|_| "outbound count exceeds the node budget type")?;
    let budget = available
        .checked_sub(rpc_clients)
        .and_then(|n| n.checked_sub(inbound))
        .and_then(|n| n.checked_sub(outbound))
        .filter(|n| *n > 0)
        .ok_or("private file descriptor budget exhausted")?;
    let runtime = Runtime::from_args(&args);
    let (core, rpc) = if profile == "source-v1" {
        create_private_integration_core_with_source_workers_v1(&runtime, &args, budget, config, source_checker, source_producer)?
    } else if profile == "source-paid-v1" || generation_export.is_some() {
        create_private_integration_core_with_campaign_workers_v1(
            &runtime,
            &args,
            budget,
            config,
            native,
            ack_fees,
            recovery_providers,
            kernel_work,
            kernel_checker,
            kernel_replacement,
            kernel_generation,
            observations,
            AdditionalCampaignWorkersV1 { source_checker, source_producer, generation_export },
        )?
    } else if kernel_work.is_some()
        || kernel_checker.is_some()
        || kernel_replacement.is_some()
        || kernel_generation.is_some()
        || !observations.is_empty()
    {
        create_private_integration_core_with_paid_observations_v5(
            &runtime,
            &args,
            budget,
            config,
            native,
            ack_fees,
            recovery_providers,
            kernel_work,
            kernel_checker,
            kernel_replacement,
            kernel_generation,
            observations,
        )?
    } else {
        match native {
            Some(native) if profile == "paid-v4" => create_private_integration_core_with_native_custody_v4(
                &runtime,
                &args,
                budget,
                config,
                native,
                ack_fees,
                recovery_providers,
            )?,
            Some(native) => create_private_integration_core_with_native_custody_v3(&runtime, &args, budget, config, native)?,
            None => create_private_integration_core_with_runtime(&runtime, &args, budget, config)?,
        }
    };
    Arc::new(kaspa_core::signals::Signals::new(&core)).init();
    // Rpc service does not outlive shutdown's Core reference accounting.
    drop(rpc);
    core.run();
    Ok(())
}
fn main() {
    kaspa_alloc::init_allocator_with_default_settings();
    if let Err(why) = run(std::env::args().collect()) {
        eprintln!("private RFC12 node: {why}");
        std::process::exit(1);
    }
}
