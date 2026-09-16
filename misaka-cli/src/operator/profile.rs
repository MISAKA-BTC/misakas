//! **What this host mines with** — ADR-0122 Decision 6.
//!
//! One answer, assembled in a fixed precedence: flags on this command, then `~/.misaka/mining.toml`,
//! then the running `kaspad`'s own command line, then the defaults `kaspad` itself would use. The
//! third source is what lets `misaka mining status` describe a fleet node that was never set up
//! with this CLI: everything it needs is already in the unit file's flags.
//!
//! `mining.toml` is its own file, not a section of `config.toml`: every `misaka` built so far
//! parses `config.toml` with `deny_unknown_fields`, so a new section there would turn every older
//! binary on the host into a hard parse error (ADR-0122 §14).

use crate::operator::procs::{self, KaspadArgs, Proc};
use crate::{CliError, exit};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `~/.misaka/mining.toml`, as written by `misaka mining setup` or by hand.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct MiningToml {
    pub(crate) mining: MiningSection,
    pub(crate) advanced: AdvancedSection,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct MiningSection {
    pub(crate) enabled: Option<bool>,
    pub(crate) network: Option<String>,
    /// `base` (the network's floor class) or a 128-hex class id. Names arrive with `getPalwClasses`.
    pub(crate) model: Option<String>,
    /// Where rewards are paid; the key's own address when absent.
    pub(crate) wallet: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) prompt: Option<bool>,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AdvancedSection {
    pub(crate) appdir: Option<String>,
    pub(crate) bond: Option<String>,
    pub(crate) fee_outpoint: Option<String>,
    pub(crate) artifact: Option<ArtifactList>,
    pub(crate) peers: Vec<String>,
    pub(crate) listen: Option<String>,
    pub(crate) rpc_borsh: Option<String>,
    pub(crate) resident_bytes: Option<String>,
    pub(crate) stop_grace_secs: Option<u64>,
    pub(crate) challenge: Option<bool>,
    pub(crate) kaspad: Option<String>,
    pub(crate) extra_kaspad_args: Vec<String>,
    pub(crate) prompt: PromptSection,
}

/// One artifact or several.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(crate) enum ArtifactList {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PromptSection {
    pub(crate) listen: Option<String>,
    pub(crate) outbox: Option<String>,
    pub(crate) worker: Option<String>,
    pub(crate) identity: Option<String>,
    /// The bound artifact the worker serves (default: the first `[advanced] artifact`).
    pub(crate) artifact: Option<String>,
    pub(crate) tokenizer: Option<String>,
    /// The helper binaries, when they are not beside this `misaka`.
    pub(crate) gateway: Option<String>,
    pub(crate) rail: Option<String>,
}

impl MiningToml {
    pub(crate) fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".misaka").join("mining.toml"))
    }

    /// A missing file is `None`. A malformed one is an error that names the file and the field:
    /// a typo in a mining configuration silently ignored is a node mining the wrong thing.
    pub(crate) fn load(path: &Path) -> Result<Option<MiningToml>, CliError> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map(Some).map_err(|e| CliError::new(exit::CONFIG, format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CliError::new(exit::CONFIG, format!("read {}: {e}", path.display()))),
        }
    }
}

/// Where each value of the profile came from, so a screen can say "from the running node" rather
/// than let a value read off a process pass for one the operator configured.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    Flag,
    File,
    Process,
    Default,
}

/// The flags every `mining` / `doctor` / `work` command accepts to override the profile.
#[derive(Clone, Debug, Default)]
pub(crate) struct Overrides {
    pub(crate) config: Option<PathBuf>,
    pub(crate) appdir: Option<String>,
    pub(crate) key_file: Option<String>,
    pub(crate) bond: Option<String>,
    pub(crate) class: Option<String>,
    pub(crate) outbox: Option<String>,
}

/// The prompt lane's files and endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PromptLane {
    pub(crate) outbox: Option<PathBuf>,
    pub(crate) gateway_listen: String,
    pub(crate) worker: Option<PathBuf>,
    pub(crate) identity: Option<PathBuf>,
    pub(crate) artifact: Option<PathBuf>,
    pub(crate) tokenizer: Option<PathBuf>,
    pub(crate) gateway_bin: Option<PathBuf>,
    pub(crate) rail_bin: Option<PathBuf>,
}

/// Everything the operator surface knows about this host's miner, with where each value came from.
#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub(crate) network: String,
    pub(crate) network_source: Source,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) config: Option<MiningToml>,
    pub(crate) appdir: PathBuf,
    pub(crate) log_file: PathBuf,
    pub(crate) key_path: Option<PathBuf>,
    pub(crate) bond: Option<String>,
    pub(crate) bond_source: Source,
    /// 128 hex, or `None` for the network's base class.
    pub(crate) class: Option<String>,
    pub(crate) pay_address: Option<String>,
    pub(crate) fee_outpoint: Option<String>,
    pub(crate) artifacts: Vec<PathBuf>,
    pub(crate) produce: bool,
    pub(crate) panel: bool,
    pub(crate) rpc: Option<String>,
    pub(crate) prompt: Option<PromptLane>,
    pub(crate) stop_grace_secs: u64,
    /// The running node this profile describes, when one was found.
    pub(crate) kaspad: Option<(Proc, KaspadArgs)>,
    /// Several nodes of this network run here and nothing said which: their pids.
    pub(crate) ambiguous_kaspads: Vec<u32>,
}

/// The stop grace a Qwen3.6 node needs (3–4 minutes to stop, measured on the fleet).
pub(crate) const DEFAULT_STOP_GRACE_SECS: u64 = 240;

/// The gateway's default listen address (`misaka-palw-gateway --listen`).
pub(crate) const GATEWAY_DEFAULT_LISTEN: &str = "127.0.0.1:8790";

impl Profile {
    /// Assemble the profile. `global_network` is `--network` (or `MISAKA_NETWORK`) when given;
    /// `global_rpc` is `--rpc`.
    pub(crate) fn resolve(ov: &Overrides, global_network: Option<&str>, global_rpc: Option<&str>) -> Result<Profile, CliError> {
        let config_path = ov.config.clone().or_else(MiningToml::default_path);
        let config = match &config_path {
            Some(p) => MiningToml::load(p)?,
            None => None,
        };
        let file = config.clone().unwrap_or_default();
        let (network, network_source) = match (global_network, file.mining.network.as_deref()) {
            (Some(n), _) => (n.to_string(), Source::Flag),
            (None, Some(n)) => (n.to_string(), Source::File),
            // A mining command never guesses a network: the CLI's own default is testnet-10, which
            // is not the network anyone mines. The running node is asked instead, below.
            (None, None) => (String::new(), Source::Default),
        };
        let appdir_hint = ov.appdir.clone().or_else(|| file.advanced.appdir.clone());
        // The running node: by network and appdir when those are known, else the one kaspad here.
        let (kaspad, ambiguous) = if network.is_empty() {
            let all: Vec<(Proc, KaspadArgs)> = procs::find(procs::Component::Kaspad)
                .into_iter()
                .map(|p| {
                    let a = procs::parse_kaspad_args(&p.args);
                    (p, a)
                })
                .collect();
            match all.len() {
                0 => (None, Vec::new()),
                1 => (all.into_iter().next(), Vec::new()),
                _ => (None, all.iter().map(|(p, _)| p.pid).collect()),
            }
        } else {
            match procs::the_kaspad(&network, appdir_hint.as_deref()) {
                Ok(k) => (k, Vec::new()),
                Err(pids) => (None, pids),
            }
        };
        let args = kaspad.as_ref().map(|(_, a)| a.clone()).unwrap_or_default();
        let (network, network_source) = if network.is_empty() {
            match &kaspad {
                Some((_, a)) => (a.network.clone(), Source::Process),
                None => ("testnet-11".to_string(), Source::Default),
            }
        } else {
            (network, network_source)
        };

        let pick = |flag: &Option<String>, file: &Option<String>, process: &Option<String>| -> (Option<String>, Source) {
            match (flag, file, process) {
                (Some(v), _, _) => (Some(v.clone()), Source::Flag),
                (None, Some(v), _) => (Some(v.clone()), Source::File),
                (None, None, Some(v)) => (Some(v.clone()), Source::Process),
                _ => (None, Source::Default),
            }
        };
        let (appdir, _) = pick(&ov.appdir, &file.advanced.appdir, &args.appdir);
        let appdir = PathBuf::from(procs::expand_home(&appdir.unwrap_or_else(|| "~/.rusty-kaspa".to_string())));
        let log_file = match &args.logdir {
            Some(dir) if kaspad.is_some() => PathBuf::from(procs::expand_home(dir)).join("rusty-kaspa.log"),
            _ => crate::operator::nodelog::default_log_file(&appdir, &network),
        };
        let (key, _) = pick(&ov.key_file, &file.mining.key, &args.key);
        let key_path = key.map(|k| PathBuf::from(procs::expand_home(&k)));
        let (bond, bond_source) = pick(&ov.bond, &file.advanced.bond, &args.bond);
        let model = file.mining.model.clone().filter(|m| !m.eq_ignore_ascii_case("base") && !m.eq_ignore_ascii_case("floor"));
        let (class, _) = pick(&ov.class, &model, &args.class);
        let (pay_address, _) = pick(&None, &file.mining.wallet, &args.pay_address);
        let (fee_outpoint, _) = pick(&None, &file.advanced.fee_outpoint, &args.fee_outpoint);
        let artifacts: Vec<PathBuf> = match &file.advanced.artifact {
            Some(ArtifactList::One(a)) => vec![PathBuf::from(procs::expand_home(a))],
            Some(ArtifactList::Many(v)) => v.iter().map(|a| PathBuf::from(procs::expand_home(a))).collect(),
            None => args.artifacts.iter().map(|a| PathBuf::from(procs::expand_home(a))).collect(),
        };
        let rpc = global_rpc
            .map(str::to_string)
            .or_else(|| file.advanced.rpc_borsh.clone())
            .or_else(|| args.rpclisten_borsh.clone().filter(|v| v != "default").map(|v| v.replace("0.0.0.0", "127.0.0.1")));
        let prompt_enabled = file.mining.prompt.unwrap_or(false) || ov.outbox.is_some() || file.advanced.prompt.outbox.is_some();
        let home = |v: &Option<String>| v.as_ref().map(|x| PathBuf::from(procs::expand_home(x)));
        let lane = &file.advanced.prompt;
        let prompt = prompt_enabled.then(|| PromptLane {
            outbox: home(&ov.outbox.clone().or_else(|| lane.outbox.clone())),
            gateway_listen: lane.listen.clone().unwrap_or_else(|| GATEWAY_DEFAULT_LISTEN.to_string()),
            worker: home(&lane.worker),
            identity: home(&lane.identity),
            artifact: home(&lane.artifact).or_else(|| artifacts.first().cloned()),
            tokenizer: home(&lane.tokenizer),
            gateway_bin: home(&lane.gateway),
            rail_bin: home(&lane.rail),
        });
        Ok(Profile {
            produce: args.produce || (kaspad.is_none() && config.is_some()),
            panel: args.panel || (kaspad.is_none() && config.is_some()),
            network,
            network_source,
            config_path: config_path.filter(|_| config.is_some()),
            config,
            appdir,
            log_file,
            key_path,
            bond,
            bond_source,
            class,
            pay_address,
            fee_outpoint,
            artifacts,
            rpc,
            prompt,
            stop_grace_secs: file.advanced.stop_grace_secs.unwrap_or(DEFAULT_STOP_GRACE_SECS),
            kaspad,
            ambiguous_kaspads: ambiguous,
        })
    }

    /// Is there anything here to describe? No file, no running node and no flags is "not set up".
    pub(crate) fn is_configured(&self) -> bool {
        self.config.is_some() || self.kaspad.is_some() || self.bond.is_some()
    }

    pub(crate) fn retention_dir(&self) -> PathBuf {
        self.appdir.join(format!("misaka-{}", self.network)).join("palw-retention")
    }

    /// Where the panel persists its fee outpoint once it has used one (`palw_panel_state_dir`).
    pub(crate) fn persisted_fee_outpoint(&self) -> PathBuf {
        self.appdir.join(format!("misaka-{}", self.network)).join("palw-panel").join("palw-fee-outpoint")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema in ADR-0122 §7 parses as written, and a misspelt field is an error, not a no-op.
    #[test]
    fn the_documented_file_parses_and_a_typo_is_refused() {
        let text = r#"
[mining]
enabled = true
network = "testnet-11"
model   = "base"
wallet  = "misakatest:qz"
key     = "~/.misaka/miner.seed"
prompt  = false

[advanced]
appdir        = "~/.misaka/testnet-11/node"
bond          = "aa:0"
fee_outpoint  = "bb:1"
artifact      = "~/.misaka/models/qwen25-1.5b-a16.bound.palwart"
peers         = ["169.58.39.220:26311"]
listen        = "0.0.0.0:26311"
rpc_borsh     = "127.0.0.1:27210"
resident_bytes = "auto"
stop_grace_secs = 240
challenge     = false
extra_kaspad_args = []

[advanced.prompt]
listen  = "127.0.0.1:8790"
outbox  = "~/.misaka/testnet-11/outbox"
worker  = "/abs/path/palw-a16-fp-worker"
"#;
        let parsed: MiningToml = toml::from_str(text).expect("the documented file parses");
        assert_eq!(parsed.mining.network.as_deref(), Some("testnet-11"));
        assert_eq!(parsed.advanced.bond.as_deref(), Some("aa:0"));
        assert!(matches!(parsed.advanced.artifact, Some(ArtifactList::One(_))));
        let many: MiningToml = toml::from_str("[advanced]\nartifact = [\"a\", \"b\"]\n").unwrap();
        assert!(matches!(many.advanced.artifact, Some(ArtifactList::Many(ref v)) if v.len() == 2));
        assert!(toml::from_str::<MiningToml>("[mining]\nnetwrok = \"testnet-11\"\n").is_err(), "a typo must not be ignored");
        assert!(toml::from_str::<MiningToml>("[mining]\n[extra]\n").is_err());
    }
}
