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
        PrivateKernelPaidCheckSpecV4, PrivateKernelPaidReplaceSpecV4, PrivateKernelPaidWorkSpecV4, PrivateNativeCustodySpecV3, PrivateRecoveryAckFeeSpecV4,
        create_private_integration_core_with_kernel_paid_consumers_v4, create_private_integration_core_with_native_custody_v3,
        create_private_integration_core_with_native_custody_v4, create_private_integration_core_with_runtime,
        export_private_complete_bootstrap_v1, export_private_paid_bootstrap_v3, export_private_paid_bootstrap_v4,
        export_private_paid_program_work_v3, prepare_private_integration_probe, private_complete_check_params_v1,
        private_g14_drill_params_v1, private_late_retirement_params_v1, private_native_settlement_params_v1,
        private_paid_claim_params_v3, private_paid_claim_params_v4, private_probe_manifest,
    },
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
const USAGE: &str = "misaka-rfc12-private-node (requires --features private-integration-config)
--private-probe-profile=native-v1|population-v1|complete-v1|paid-v3|paid-v4|g14-v1 --private-probe-salt=HEX --testnet --netsuffix=12
population-v1 additionally requires --private-probe-bonds=N (actual funded registry only).
g14-v1 (the G14 collusion drill, scripts/drills/g14_collusion_drill.py): --private-probe-g14-at=N (0: every G14 fence from
  genesis; N > 0: testnet-12's shipped rules below N and the DAA-14,000 candidate set from N), --private-probe-g14-set=full|
  route-required-only (default full), --private-probe-panel-seats=1..3 (default 2). Armed through drill arming records naming
  this genesis and height, never pinned; the shipped validation still refuses the ruleset.
Any profile: --private-probe-retirement-daa=N (N >= 2) arms palw_dns_retirement_v1 at DAA N instead of DAA 1, so a private
  run accumulates DNS-validator history before the fence (RFC-0012 §4.4); Rule E and its commitment stay at DAA 1. The
  height is part of the profile's identity: every node, export and print of one run must name the same N.
Read-only mode: --print-private-probe-profile
Offline export: --private-probe-export-bootstrap=FRESH_DIRECTORY (complete-v1, paid-v3 or paid-v4)
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
  --private-probe-kernel-paid-work=ABSOLUTE_JSON (explicit all-target producer work, owned outbox,
    separate producer carrier payer/funding and input/material/workspace/fee limits; never native acceptance by itself)
  --private-probe-kernel-paid-check=ABSOLUTE_JSON (explicit assigned checker bond/key, provider endpoint,
    acquisition limits and separate receipt payer/funding; checker-only mode needs no producer exchange)
  --private-probe-kernel-replace=ABSOLUTE_JSON (sponsor's bounded provider replacement for one root: new sorted
    2..8 roster, signed valid_until_daa and input cap; replays the producer outbox's own request/context, never native
    acceptance by itself)
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
                        | "--private-probe-kernel-replace"
                        | "--private-probe-retirement-daa"
                        | "--private-probe-g14-at"
                        | "--private-probe-g14-set"
                        | "--private-probe-panel-seats"
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
    let retirement_daa = flags
        .values
        .remove("--private-probe-retirement-daa")
        .map(|n| n.parse::<u64>().map_err(|_| "private retirement DAA must be u64".to_owned()))
        .transpose()?;
    let g14_at = flags
        .values
        .remove("--private-probe-g14-at")
        .map(|n| n.parse::<u64>().map_err(|_| "the G14 drill height must be u64".to_owned()))
        .transpose()?;
    let g14_set = match flags.values.remove("--private-probe-g14-set").as_deref() {
        None | Some("full") => kaspa_consensus_core::config::private_g14_drill::PrivateG14DrillSetV1::Full,
        Some("route-required-only") => kaspa_consensus_core::config::private_g14_drill::PrivateG14DrillSetV1::RouteRequiredOnly,
        Some(other) => return Err(format!("unknown G14 drill set {other} (full | route-required-only)")),
    };
    let panel_seats = flags
        .values
        .remove("--private-probe-panel-seats")
        .map(|n| n.parse::<u16>().map_err(|_| "the G14 drill Panel size must be u16".to_owned()))
        .transpose()?;
    if profile == "g14-v1" && g14_at.is_none() {
        return Err("the g14-v1 profile requires --private-probe-g14-at (0: genesis mode)".into());
    }
    if profile != "g14-v1" && (g14_at.is_some() || panel_seats.is_some()) {
        return Err("--private-probe-g14-at / --private-probe-panel-seats belong to the g14-v1 profile".into());
    }
    let bootstrap = flags.values.remove("--private-probe-export-bootstrap");
    let paid_request = flags.values.remove("--private-probe-paid-request");
    let paid_output = flags.values.remove("--private-probe-export-paid-work");
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
        && (profile != "paid-v4" || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
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
    let kernel_replace_file = flags.values.remove("--private-probe-kernel-replace");
    if kernel_replace_file.is_some()
        && (profile != "paid-v4" || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("kernel replacement options require an explicit paid-v4 custody sponsor node".into());
    }
    let kernel_replace = kernel_replace_file
        .map(|file| {
            let path = Path::new(&file);
            if !path.is_absolute() {
                return Err("kernel replacement specification path must be absolute".into());
            }
            PrivateKernelPaidReplaceSpecV4::read(path)
        })
        .transpose()?;
    if kernel_checker_file.is_some() && (profile != "paid-v4" || flags.print || bootstrap.is_some() || paid_output.is_some()) {
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
    if !recovery_providers.is_empty()
        && (profile != "paid-v4" || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("recovery provider roster requires paid-v4 custody node mode".into());
    }
    if ack_fees.is_some() && (profile != "paid-v4" || native.is_none() || flags.print || bootstrap.is_some() || paid_output.is_some())
    {
        return Err("recovery ACK fee options require paid-v4 hosted provider node mode".into());
    }
    if native.is_some()
        && (!matches!(profile.as_str(), "paid-v3" | "paid-v4") || flags.print || bootstrap.is_some() || paid_output.is_some())
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
        "complete-v1" => private_complete_check_params_v1(&salt)?,
        "paid-v3" => private_paid_claim_params_v3(&salt)?,
        "paid-v4" => private_paid_claim_params_v4(&salt)?,
        "native-v1" => private_native_settlement_params_v1(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt))?)?,
        "population-v1" => {
            private_native_settlement_params_v1(derive_private_population_v1(&salt, population.expect("size checked"))?.params)?
        }
        "g14-v1" => private_g14_drill_params_v1(&salt, g14_at.expect("height checked"), g14_set, panel_seats.unwrap_or(2))?,
        _ => return Err("unallocated private profile".into()),
    };
    // RFC-0012 §4.4: only the retirement height moves; the DAA-1 profile is unchanged when the option is absent.
    let params = match retirement_daa {
        Some(daa) => private_late_retirement_params_v1(params, daa)?,
        None => params,
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
            _ => return Err("bootstrap export requires complete-v1, paid-v3 or paid-v4".into()),
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
    let (core, rpc) = if kernel_work.is_some() || kernel_checker.is_some() || kernel_replace.is_some() {
        create_private_integration_core_with_kernel_paid_consumers_v4(
            &runtime,
            &args,
            budget,
            config,
            native,
            ack_fees,
            recovery_providers,
            kernel_work,
            kernel_checker,
            kernel_replace,
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
