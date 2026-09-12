//! **The components as they are running on this host** — ADR-0122 Decision 5's readiness gates,
//! and the reason `misaka mining status` needs no configuration on a node an operator already runs.
//!
//! A fleet host starts `kaspad` from a unit file with twenty-odd flags, and every fact the
//! operator surface needs is in that command line: the appdir, the key path, the bond, the pay
//! address, the class, the artifacts, and whether it produces. So the running process is read the
//! way the fleet's roll script reads it — found by its `--appdir`, its image checked against the
//! binary on disk — and its arguments are parsed here, as `kaspad`'s own parser spells them.
//!
//! Nothing here reads a key: `--palw-producer-key` is a PATH, and only the path is kept.

use std::path::PathBuf;

/// One running process of interest.
#[derive(Clone, Debug)]
pub(crate) struct Proc {
    pub(crate) pid: u32,
    pub(crate) exe: Option<PathBuf>,
    pub(crate) args: Vec<String>,
    /// Unix seconds.
    pub(crate) start_time: u64,
    /// The running image is no longer the file at its path: the binary was replaced after this
    /// process started (Linux's `/proc/<pid>/exe` reads `… (deleted)`), or — where that cannot be
    /// read — the file on disk is newer than the process.
    pub(crate) image_replaced: Option<bool>,
}

impl Proc {
    pub(crate) fn uptime_secs(&self) -> u64 {
        now_unix().saturating_sub(self.start_time)
    }
}

pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The three helper binaries by the names the tree builds them under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Component {
    Kaspad,
    Gateway,
    Rail,
}

impl Component {
    fn matches(self, basename: &str) -> bool {
        match self {
            Component::Kaspad => basename == "kaspad",
            Component::Gateway => basename == "misaka-palw-gateway",
            Component::Rail => basename == "misaka-palw-fp-rail",
        }
    }
}

/// Every running instance of `component`, from the process table.
pub(crate) fn find(component: Component) -> Vec<Proc> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always).with_exe(UpdateKind::Always),
    );
    let mut out = Vec::new();
    for (pid, p) in sys.processes() {
        let args: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let exe = p.exe().map(PathBuf::from);
        let basename = exe
            .as_ref()
            .and_then(|e| e.file_name())
            .map(|n| n.to_string_lossy().trim_end_matches(" (deleted)").to_string())
            .or_else(|| args.first().map(|a| a.rsplit('/').next().unwrap_or(a).to_string()))
            .unwrap_or_default();
        if !component.matches(&basename) {
            continue;
        }
        let pid = pid.as_u32();
        out.push(Proc {
            pid,
            image_replaced: image_replaced(pid, exe.as_deref(), p.start_time()),
            exe,
            args,
            start_time: p.start_time(),
        });
    }
    out.sort_by_key(|p| p.pid);
    out
}

/// Is the running image still the file at its path? `None` when this host cannot say.
fn image_replaced(pid: u32, exe: Option<&std::path::Path>, start_time: u64) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(target) = std::fs::read_link(format!("/proc/{pid}/exe")) {
            return Some(target.to_string_lossy().ends_with(" (deleted)"));
        }
    }
    let _ = pid;
    // Elsewhere: a binary modified after the process started is not the image it runs.
    let modified = std::fs::metadata(exe?).ok()?.modified().ok()?;
    let modified = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(modified > start_time.saturating_add(1))
}

/// What a `kaspad` command line says about the node it starts. Flags are read the way `kaspad`'s
/// clap parser takes them: `--flag=value` or `--flag value`, booleans bare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KaspadArgs {
    pub(crate) appdir: Option<String>,
    pub(crate) logdir: Option<String>,
    /// `testnet-11`, `devnet`, `mainnet`, … — `--testnet [--netsuffix=N]`, `--devnet`, `--simnet`.
    pub(crate) network: String,
    pub(crate) produce: bool,
    pub(crate) panel: bool,
    pub(crate) challenge: bool,
    pub(crate) utxoindex: bool,
    pub(crate) key: Option<String>,
    pub(crate) bond: Option<String>,
    pub(crate) pay_address: Option<String>,
    pub(crate) class: Option<String>,
    pub(crate) fee_outpoint: Option<String>,
    pub(crate) artifacts: Vec<String>,
    pub(crate) rpclisten_borsh: Option<String>,
    pub(crate) listen: Option<String>,
    pub(crate) addpeers: Vec<String>,
    pub(crate) register_bond: bool,
    pub(crate) enable_unsynced_mining: bool,
    /// Flags that swap a devnet's consensus parameters (`--palw-devnet-floor-only`, the
    /// `--palw-*-devnet` family): a node started with one runs a ruleset of its own, and this
    /// CLI's fingerprint for the network does not describe it.
    pub(crate) ruleset_flags: Vec<String>,
}

/// The flags that take a value, so a bare `--flag value` form is read as one flag and its value.
const VALUED: &[&str] = &[
    "appdir",
    "logdir",
    "netsuffix",
    "palw-producer-key",
    "palw-producer-bond",
    "palw-producer-pay-address",
    "palw-producer-class",
    "palw-fee-outpoint",
    "palw-class-artifact",
    "rpclisten-borsh",
    "rpclisten",
    "rpclisten-json",
    "listen",
    "addpeer",
    "connect",
    "palw-bond-collateral",
    "palw-class-cache-bytes",
    "palw-class-resident-bytes",
    "palw-attempt-retention-minutes",
    "palw-heartbeat-miner-address",
    "evm-rpc-listen",
];

pub(crate) fn parse_kaspad_args(argv: &[String]) -> KaspadArgs {
    let mut a = KaspadArgs::default();
    let (mut testnet, mut devnet, mut simnet, mut suffix) = (false, false, false, None::<String>);
    let mut i = 1; // argv[0] is the binary
    while i < argv.len() {
        let arg = &argv[i];
        i += 1;
        let Some(flag) = arg.strip_prefix("--") else { continue };
        let (name, inline) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        let value = || -> Option<String> { inline.clone() };
        let take = |i: &mut usize| -> Option<String> {
            if inline.is_some() {
                return inline.clone();
            }
            if VALUED.contains(&name) && *i < argv.len() && !argv[*i].starts_with("--") {
                *i += 1;
                return Some(argv[*i - 1].clone());
            }
            None
        };
        match name {
            "testnet" => testnet = value().is_none_or(|v| v != "false"),
            "devnet" => devnet = value().is_none_or(|v| v != "false"),
            "simnet" => simnet = value().is_none_or(|v| v != "false"),
            "netsuffix" => suffix = take(&mut i),
            "appdir" => a.appdir = take(&mut i),
            "logdir" => a.logdir = take(&mut i),
            "palw-produce" => a.produce = value().is_none_or(|v| v != "false"),
            "palw-panel" => a.panel = value().is_none_or(|v| v != "false"),
            "palw-challenge" => a.challenge = value().is_none_or(|v| v != "false"),
            "utxoindex" => a.utxoindex = value().is_none_or(|v| v != "false"),
            "palw-register-bond" => a.register_bond = value().is_none_or(|v| v != "false"),
            "enable-unsynced-mining" => a.enable_unsynced_mining = value().is_none_or(|v| v != "false"),
            "palw-producer-key" => a.key = take(&mut i),
            "palw-producer-bond" => a.bond = take(&mut i),
            "palw-producer-pay-address" => a.pay_address = take(&mut i),
            "palw-producer-class" => a.class = take(&mut i),
            "palw-fee-outpoint" => a.fee_outpoint = take(&mut i),
            "palw-class-artifact" => a.artifacts.extend(take(&mut i)),
            "rpclisten-borsh" => a.rpclisten_borsh = take(&mut i).or(Some("default".to_string())),
            "listen" => a.listen = take(&mut i),
            "addpeer" | "connect" => a.addpeers.extend(take(&mut i)),
            other if VALUED.contains(&other) => {
                take(&mut i);
            }
            other if other == "palw-devnet-floor-only" || (other.starts_with("palw-") && other.ends_with("-devnet")) => {
                a.ruleset_flags.push(format!("--{other}"));
            }
            _ => {}
        }
    }
    // `--netsuffix` alone does not select testnet: kaspad boots mainnet (the classes runbook's
    // warning), and this parser says what kaspad would do, not what the operator meant.
    a.network = if devnet {
        "devnet".to_string()
    } else if simnet {
        "simnet".to_string()
    } else if testnet {
        match suffix {
            Some(s) => format!("testnet-{s}"),
            None => "testnet-10".to_string(),
        }
    } else {
        "mainnet".to_string()
    };
    a
}

/// The `kaspad` this host runs for `network`, when there is exactly one — or the one whose
/// `--appdir` is `appdir`. Several without a way to choose is `Err` naming their pids.
pub(crate) fn the_kaspad(network: &str, appdir: Option<&str>) -> Result<Option<(Proc, KaspadArgs)>, Vec<u32>> {
    let mut found: Vec<(Proc, KaspadArgs)> = find(Component::Kaspad)
        .into_iter()
        .map(|p| {
            let args = parse_kaspad_args(&p.args);
            (p, args)
        })
        .filter(|(_, a)| a.network == network)
        .collect();
    if let Some(dir) = appdir {
        let want = expand_home(dir);
        found.retain(|(_, a)| a.appdir.as_deref().map(expand_home).as_deref() == Some(want.as_str()));
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(found.iter().map(|(p, _)| p.pid).collect()),
    }
}

/// `~/x` → `$HOME/x`, the way kaspad expands `--appdir`.
pub(crate) fn expand_home(path: &str) -> String {
    match (path.strip_prefix('~'), dirs::home_dir()) {
        (Some(rest), Some(home)) => format!("{}{rest}", home.display()),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    /// A fleet unit's command line, read into the facts the operator surface needs.
    #[test]
    fn a_fleet_command_line_is_read_as_kaspad_reads_it() {
        let a = parse_kaspad_args(&argv(
            "/opt/misaka/kaspad --testnet --netsuffix=11 --appdir=/root/.t11 --utxoindex --rpclisten-borsh=default \
             --palw-produce --palw-panel --palw-producer-key=/root/miner.seed --palw-producer-bond aa:0 \
             --palw-producer-pay-address=misakatest:qz --palw-fee-outpoint=bb:1 --palw-class-artifact=/m/a.palwart \
             --palw-class-artifact /m/b.palwq36 --addpeer=169.58.39.220:26311 --yes",
        ));
        assert_eq!(a.network, "testnet-11");
        assert_eq!(a.appdir.as_deref(), Some("/root/.t11"));
        assert!(a.produce && a.panel && a.utxoindex && !a.challenge);
        assert_eq!(a.key.as_deref(), Some("/root/miner.seed"));
        assert_eq!(a.bond.as_deref(), Some("aa:0"), "the space-separated form is one flag and its value");
        assert_eq!(a.fee_outpoint.as_deref(), Some("bb:1"));
        assert_eq!(a.artifacts, vec!["/m/a.palwart".to_string(), "/m/b.palwq36".to_string()]);
        assert_eq!(a.rpclisten_borsh.as_deref(), Some("default"));
        assert_eq!(a.addpeers, vec!["169.58.39.220:26311".to_string()]);
    }

    /// The network is what kaspad would boot, not what the operator meant: a suffix without
    /// `--testnet` is mainnet, and bare `--testnet` is testnet-10.
    #[test]
    fn the_network_is_what_kaspad_would_boot() {
        assert_eq!(parse_kaspad_args(&argv("kaspad --netsuffix=11")).network, "mainnet");
        assert_eq!(parse_kaspad_args(&argv("kaspad --testnet")).network, "testnet-10");
        assert_eq!(parse_kaspad_args(&argv("kaspad --devnet")).network, "devnet");
        assert_eq!(parse_kaspad_args(&argv("kaspad --testnet --netsuffix 11")).network, "testnet-11");
        let floor = parse_kaspad_args(&argv("kaspad --devnet --palw-devnet-floor-only --palw-model-devnet"));
        assert_eq!(floor.ruleset_flags, vec!["--palw-devnet-floor-only".to_string(), "--palw-model-devnet".to_string()]);
    }

    /// A bare `--rpclisten-borsh` enables the listener on its default address.
    #[test]
    fn a_bare_borsh_listener_is_the_default_address() {
        assert_eq!(parse_kaspad_args(&argv("kaspad --rpclisten-borsh")).rpclisten_borsh.as_deref(), Some("default"));
        assert_eq!(parse_kaspad_args(&argv("kaspad")).rpclisten_borsh, None);
    }
}
