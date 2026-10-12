//! `~/.misaka/config.toml` — operator config (design §11).
//!
//! Precedence is CLI > env > config-file > built-in default. clap fills the
//! CLI/env layer (the global `--network` / `--rpc` / `--evm-rpc` flags, each with an
//! env var); this module supplies the config-file layer plus the
//! `misaka config init` / `misaka config show` commands.

use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Values a `~/.misaka/config.toml` may set. Every field is optional — an absent key
/// falls through to the built-in default. The schema matches the design's §11.1
/// example; fields not yet consumed by `misaka` (`node.grpc`/`node.wrpc_json`,
/// `evm.chain_id`, the `[validator]` section) are accepted for forward-compatibility.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub network_id: Option<String>,
    pub node: NodeConfig,
    pub evm: EvmConfig,
    pub validator: ValidatorConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeConfig {
    pub grpc: Option<String>,
    pub wrpc_borsh: Option<String>,
    pub wrpc_json: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvmConfig {
    pub rpc_url: Option<String>,
    pub chain_id: Option<u64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidatorConfig {
    pub key: Option<String>,
    pub signed_epoch_db: Option<String>,
}

impl Config {
    /// `~/.misaka/config.toml`, or None if the home dir cannot be determined.
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".misaka").join("config.toml"))
    }

    /// Load the config from the default path. A missing file yields the empty default
    /// (no error — config is optional); a malformed file is a HARD error so a typo is
    /// never silently ignored.
    pub fn load() -> Result<Config, CliError> {
        match Self::default_path() {
            Some(path) => Self::load_from(&path),
            None => Ok(Config::default()),
        }
    }

    pub fn load_from(path: &Path) -> Result<Config, CliError> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| CliError::new(exit::GENERIC, format!("parse {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(CliError::new(exit::GENERIC, format!("read {}: {e}", path.display()))),
        }
    }
}

/// `misaka config init` — write a `~/.misaka/config.toml` scaffold for `network_id`,
/// with the canonical per-network ports filled in (via the shared endpoint resolver).
/// Refuses to overwrite an existing file unless `force`.
pub fn init(network_id: &str, force: bool) -> CliResult {
    let path = Config::default_path().ok_or_else(|| CliError::new(exit::GENERIC, "cannot determine home directory for ~/.misaka"))?;
    if path.exists() && !force {
        return Err(CliError::new(exit::GENERIC, format!("{} already exists (use --force to overwrite)", path.display())));
    }
    let nid = NetworkId::from_str(network_id)
        .map_err(|e| CliError::new(exit::GENERIC, format!("invalid network-id '{network_id}': {e}")))?;
    let grpc = nid.default_endpoint_port(EndpointKind::NodeGrpc);
    let borsh = nid.default_endpoint_port(EndpointKind::NodeWrpcBorsh);
    let json = nid.default_endpoint_port(EndpointKind::NodeWrpcJson);
    let evm = nid.default_endpoint_port(EndpointKind::EvmRpcHttp);
    let scaffold = format!(
        "# MISAKA operator config — `misaka config show` prints the effective values.\n\
         # Precedence: CLI flag > env var > this file > built-in default.\n\
         network_id = \"{network_id}\"\n\n\
         [node]\n\
         # node gRPC (miner / low-level RPC)\n\
         grpc = \"127.0.0.1:{grpc}\"\n\
         # node wRPC Borsh (validator / wallet / operator) — what `misaka` connects to\n\
         wrpc_borsh = \"127.0.0.1:{borsh}\"\n\
         # node wRPC JSON (explorer / browser)\n\
         wrpc_json = \"127.0.0.1:{json}\"\n\n\
         [evm]\n\
         rpc_url = \"http://127.0.0.1:{evm}\"\n\
         # chain_id = 5067595\n\n\
         [validator]\n\
         # key = \"validator.seed\"\n\
         # signed_epoch_db = \"validator.{network_id}.state\"\n",
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {}: {e}", parent.display())))?;
    }
    std::fs::write(&path, scaffold).map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", path.display())))?;
    println!("wrote {}", path.display());
    Ok(())
}

fn network_default_endpoint(network: &str, kind: EndpointKind) -> String {
    match NetworkId::from_str(network) {
        Ok(nid) => format!("127.0.0.1:{}", nid.default_endpoint_port(kind)),
        Err(_) => "<unresolved>".to_string(),
    }
}

fn resolve_node_grpc_display(
    network: &str,
    cli_or_env: &Option<String>,
    config_value: &Option<String>,
    env_value: Option<&str>,
) -> String {
    if let Some(value) = cli_or_env {
        let source = if env_value == Some(value.as_str()) { "env MISAKA_NODE_GRPC" } else { "cli --node-grpc" };
        return format!("{value} ({source})");
    }
    if let Some(value) = config_value {
        return format!("{value} (config file)");
    }
    format!("{} (network default)", network_default_endpoint(network, EndpointKind::NodeGrpc))
}

/// `misaka config show` — print the EFFECTIVE config (after CLI/env/file/default
/// resolution) plus the config-file path and whether it exists.
pub fn show(
    output: OutputFormat,
    network: &str,
    rpc: &Option<String>,
    node_grpc: &Option<String>,
    config_node_grpc: &Option<String>,
    evm_rpc: &str,
) -> CliResult {
    let path = Config::default_path();
    let path_str = path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "<unknown>".to_string());
    let exists = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    // The resolved node wRPC Borsh endpoint (default derives from the network if unset).
    let borsh =
        rpc.clone().unwrap_or_else(|| format!("{} (network default)", network_default_endpoint(network, EndpointKind::NodeWrpcBorsh)));
    let grpc = resolve_node_grpc_display(network, node_grpc, config_node_grpc, std::env::var("MISAKA_NODE_GRPC").ok().as_deref());
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "config_file": path_str,
                "config_file_exists": exists,
                "effective": { "network_id": network, "node_grpc": grpc, "node_wrpc_borsh": borsh, "evm_rpc_url": evm_rpc }
            })
        ),
        OutputFormat::Human => {
            println!("config file:     {path_str}{}", if exists { "" } else { "  (not present — using defaults)" });
            println!("network-id:      {network}");
            println!("node-grpc:       {grpc}");
            println!("node-wrpc-borsh: {borsh}");
            println!("evm-rpc-url:     {evm_rpc}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_design_schema() {
        let toml = r#"
            network_id = "testnet-10"
            [node]
            grpc = "127.0.0.1:26210"
            wrpc_borsh = "127.0.0.1:27210"
            wrpc_json = "127.0.0.1:28210"
            [evm]
            rpc_url = "http://127.0.0.1:8545"
            chain_id = 5067595
            [validator]
            key = "validator.seed"
            signed_epoch_db = "validator.testnet-10.state"
        "#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.network_id.as_deref(), Some("testnet-10"));
        assert_eq!(c.node.wrpc_borsh.as_deref(), Some("127.0.0.1:27210"));
        assert_eq!(c.evm.rpc_url.as_deref(), Some("http://127.0.0.1:8545"));
        assert_eq!(c.evm.chain_id, Some(5067595));
        assert_eq!(c.validator.key.as_deref(), Some("validator.seed"));
    }

    #[test]
    fn empty_and_partial_configs_default_missing() {
        assert!(toml::from_str::<Config>("").unwrap().network_id.is_none());
        let c: Config = toml::from_str("network_id = \"devnet\"\n").unwrap();
        assert_eq!(c.network_id.as_deref(), Some("devnet"));
        assert!(c.node.wrpc_borsh.is_none());
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert!(toml::from_str::<Config>("netwrok_id = \"oops\"\n").is_err());
    }

    #[test]
    fn node_grpc_display_shows_source() {
        assert_eq!(
            resolve_node_grpc_display("testnet-10", &Some("127.0.0.1:9999".to_string()), &None, Some("127.0.0.1:9999")),
            "127.0.0.1:9999 (env MISAKA_NODE_GRPC)"
        );
        assert_eq!(
            resolve_node_grpc_display("testnet-10", &None, &Some("127.0.0.1:26210".to_string()), None),
            "127.0.0.1:26210 (config file)"
        );
        assert_eq!(resolve_node_grpc_display("testnet-10", &None, &None, None), "127.0.0.1:26210 (network default)");
    }
}
