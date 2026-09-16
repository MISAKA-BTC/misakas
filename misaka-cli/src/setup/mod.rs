//! Guided VPS setup helpers.
//!
//! This module intentionally keeps the host-mutation surface narrow: preflight
//! and status checks are read-only, and the node service installer is explicit.
//! It sets up a node only. The DNS-finality validator onboarding it used to carry
//! (validator key, funding miner, stake bond, validator sidecar) is retired: PALW
//! does not use validators, and PALW mining is set up with `misaka mining setup`.

use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use clap::{Args, Subcommand};
use kaspa_consensus_core::network::{EndpointKind, NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use serde::{Deserialize, Serialize};

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

type SetupResult<T> = Result<T, CliError>;

#[cfg(test)]
const DEFAULT_SERVICE_USER: &str = "misaka_user";
#[cfg(test)]
const DEFAULT_APPDIR: &str = "/var/lib/misaka";
#[cfg(test)]
const DEFAULT_STATE_FILE: &str = "/etc/misaka/setup.toml";
#[cfg(test)]
const DEFAULT_KASPAD_SERVICE: &str = "misaka-kaspad";
const DEFAULT_SEEDER_SERVICE: &str = "misaka-dnsseeder";
#[cfg(test)]
const DEFAULT_REPO_DIR: &str = "/opt/misakas";
#[cfg(test)]
const DEFAULT_REPO_URL: &str = "https://github.com/MISAKA-BTC/misakas.git";
const SETUP_LOG_DIR: &str = "/var/log/misaka-setup";
const DEFAULT_WEB_SESSION_FILE: &str = "/var/log/misaka-setup/web-session.json";
const DEFAULT_WEB_URL_FILE: &str = "/var/log/misaka-setup/web-url.txt";
const DEFAULT_WEB_TMUX_SESSION: &str = "misaka-setup-web";
const PREPARE_JOB: &str = "prepare-vps";

#[derive(Subcommand, Debug)]
pub enum SetupCmd {
    /// Check whether this VPS looks ready for MISAKA setup.
    Preflight(PreflightArgs),
    /// Create or preview the kaspad systemd service.
    Node(NodeSetupArgs),
    /// Show node/seeder status in one place.
    Status(StatusArgs),
    /// Start a temporary browser setup wizard for button-first node joining.
    Web(WebArgs),
    /// Print the currently saved setup Web UI URL.
    WebStatus(WebStatusArgs),
    /// Reopen the setup Web UI: reuse the saved URL if alive, otherwise start a new tmux session.
    WebResume(WebResumeArgs),
    /// Stop the saved setup Web UI session when possible.
    WebStop(WebStopArgs),
    /// Print safe Discord registration commands.
    Discord(DiscordArgs),
}

#[derive(Args, Debug, Clone)]
pub struct PreflightArgs {
    /// Service user expected by setup.
    #[arg(long, default_value = "misaka_user")]
    service_user: String,
    /// Node data directory.
    #[arg(long, default_value = "/var/lib/misaka")]
    appdir: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct NodeSetupArgs {
    /// Apply changes. Without --yes or --dry-run, setup refuses to mutate the host.
    #[arg(long)]
    yes: bool,
    /// Print the planned changes without writing files or starting services.
    #[arg(long)]
    dry_run: bool,
    /// Overwrite an existing service unit if its content differs.
    #[arg(long)]
    force: bool,
    /// Skip UFW rule creation.
    #[arg(long)]
    no_ufw: bool,
    /// Service user to create/use.
    #[arg(long, default_value = "misaka_user")]
    service_user: String,
    /// Node data directory.
    #[arg(long, default_value = "/var/lib/misaka")]
    appdir: PathBuf,
    /// systemd service name.
    #[arg(long, default_value = "misaka-kaspad")]
    service: String,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
    /// Public IPv4 address. If omitted, setup tries a best-effort curl lookup.
    #[arg(long)]
    public_ip: Option<String>,
    /// kaspad RPC listener profile.
    #[arg(long, default_value = "local-validator")]
    profile: String,
    /// Outgoing peer target.
    #[arg(long, default_value_t = 8)]
    outpeers: u16,
    /// Max inbound peers.
    #[arg(long, default_value_t = 64)]
    maxinpeers: u16,
    /// Minimum free disk percentage enforced by kaspad.
    #[arg(long, default_value_t = 15)]
    min_disk_free_percent: u8,
    /// Storage tuning for kaspad RocksDB. auto enables HDD tuning when the data mount is rotational.
    #[arg(long, default_value = "auto", value_parser = ["auto", "default", "hdd"])]
    storage_profile: String,
    /// Do not add --utxoindex. By default node setup is wallet-ready.
    #[arg(long)]
    no_utxoindex: bool,
}

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    /// Node service name.
    #[arg(long, default_value = "misaka-kaspad")]
    node_service: String,
    /// DNS seeder service name.
    #[arg(long, default_value = "misaka-dnsseeder")]
    seeder_service: String,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct DiscordArgs {
    /// Public node IP. Defaults to setup state, then a best-effort lookup.
    #[arg(long)]
    public_ip: Option<String>,
    /// Wallet/mining reward address, if the operator wants to register it.
    #[arg(long)]
    wallet: Option<String>,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct WebArgs {
    /// Bind 0.0.0.0 so the setup page can be opened from your browser.
    #[arg(long)]
    public: bool,
    /// HTTP port for the temporary setup page.
    #[arg(long, default_value_t = 8787)]
    port: u16,
    /// Public IPv4 address shown in the setup URL and passed to node setup.
    #[arg(long)]
    public_ip: Option<String>,
    /// One-time setup token. Omit to generate one.
    #[arg(long)]
    token: Option<String>,
    /// Stop the setup page after this many minutes without a valid request.
    #[arg(long, default_value_t = 60)]
    ttl_minutes: u64,
    /// Stop the setup page after this many minutes even if valid requests keep it alive.
    #[arg(long, default_value_t = 720)]
    max_ttl_minutes: u64,
    /// When --public is used, allow the current SSH client IP to access the Web UI port and block others via UFW.
    #[arg(long)]
    restrict_to_ssh_client: bool,
    /// When --public is used, allow this IPv4 to access the Web UI port via UFW. Can be repeated.
    #[arg(long = "allow-client-ip")]
    allow_client_ips: Vec<String>,
    /// Force overwrite of an existing differing node unit when pressing Install.
    #[arg(long)]
    force: bool,
    /// Storage tuning for kaspad RocksDB. auto enables HDD tuning when the data mount is rotational.
    #[arg(long, default_value = "auto", value_parser = ["auto", "default", "hdd"])]
    storage_profile: String,
    /// Skip UFW rule creation when pressing Install.
    #[arg(long)]
    no_ufw: bool,
    /// Service user to create/use.
    #[arg(long, default_value = "misaka_user")]
    service_user: String,
    /// Node data directory.
    #[arg(long, default_value = "/var/lib/misaka")]
    appdir: PathBuf,
    /// systemd service name.
    #[arg(long, default_value = "misaka-kaspad")]
    service: String,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
    /// Local MISAKA source directory used by the browser prepare step.
    #[arg(long, default_value = "/opt/misakas")]
    repo_dir: PathBuf,
    /// Git repository URL used if the source directory does not exist.
    #[arg(long, default_value = "https://github.com/MISAKA-BTC/misakas.git")]
    repo_url: String,
}

#[derive(Args, Debug, Clone)]
pub struct WebStatusArgs {
    /// Saved setup Web UI session metadata.
    #[arg(long, default_value = DEFAULT_WEB_SESSION_FILE)]
    session_file: PathBuf,
    /// Saved setup Web UI URL.
    #[arg(long, default_value = DEFAULT_WEB_URL_FILE)]
    url_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct WebResumeArgs {
    /// Saved setup Web UI session metadata.
    #[arg(long, default_value = DEFAULT_WEB_SESSION_FILE)]
    session_file: PathBuf,
    /// Saved setup Web UI URL.
    #[arg(long, default_value = DEFAULT_WEB_URL_FILE)]
    url_file: PathBuf,
    /// tmux session used to keep the setup Web UI running.
    #[arg(long, default_value = DEFAULT_WEB_TMUX_SESSION)]
    tmux_session: String,
    /// Bind to 127.0.0.1 instead of opening the VPS public interface.
    #[arg(long)]
    local: bool,
    /// HTTP port for the temporary setup page.
    #[arg(long, default_value_t = 8787)]
    port: u16,
    /// Public IPv4 address shown in the setup URL and passed to node setup.
    #[arg(long)]
    public_ip: Option<String>,
    /// Stop the setup page after this many minutes without a valid request.
    #[arg(long, default_value_t = 60)]
    ttl_minutes: u64,
    /// Stop the setup page after this many minutes even if valid requests keep it alive.
    #[arg(long, default_value_t = 720)]
    max_ttl_minutes: u64,
    /// Do not restrict public Web UI access to the SSH client / saved client IPs.
    #[arg(long)]
    no_restrict_to_ssh_client: bool,
    /// When public, allow this IPv4 to access the Web UI port via UFW. Can be repeated.
    #[arg(long = "allow-client-ip")]
    allow_client_ips: Vec<String>,
    /// Force overwrite of an existing differing node unit when pressing Install.
    #[arg(long)]
    force: bool,
    /// Storage tuning for kaspad RocksDB. auto enables HDD tuning when the data mount is rotational.
    #[arg(long, default_value = "auto", value_parser = ["auto", "default", "hdd"])]
    storage_profile: String,
    /// Skip UFW rule creation when pressing Install.
    #[arg(long)]
    no_ufw: bool,
    /// Service user to create/use.
    #[arg(long, default_value = "misaka_user")]
    service_user: String,
    /// Node data directory.
    #[arg(long, default_value = "/var/lib/misaka")]
    appdir: PathBuf,
    /// systemd service name.
    #[arg(long, default_value = "misaka-kaspad")]
    service: String,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
    /// Local MISAKA source directory used by the browser prepare step.
    #[arg(long, default_value = "/opt/misakas")]
    repo_dir: PathBuf,
    /// Git repository URL used if the source directory does not exist.
    #[arg(long, default_value = "https://github.com/MISAKA-BTC/misakas.git")]
    repo_url: String,
}

#[derive(Args, Debug, Clone)]
pub struct WebStopArgs {
    /// Saved setup Web UI session metadata.
    #[arg(long, default_value = DEFAULT_WEB_SESSION_FILE)]
    session_file: PathBuf,
    /// Saved setup Web UI URL.
    #[arg(long, default_value = DEFAULT_WEB_URL_FILE)]
    url_file: PathBuf,
    /// tmux session used by the helper script.
    #[arg(long, default_value = DEFAULT_WEB_TMUX_SESSION)]
    tmux_session: String,
}

/// `/etc/misaka/setup.toml`. Not `deny_unknown_fields`: a state file written while this module
/// still onboarded validators carries a `[validator]` table, and it must keep loading. The next
/// write drops that table; the key file it named is left where it is.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SetupState {
    network_id: Option<String>,
    public_ip: Option<String>,
    node: StateNode,
    discord: StateDiscord,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StateNode {
    service: Option<String>,
    service_user: Option<String>,
    appdir: Option<String>,
    profile: Option<String>,
    p2p_port: Option<u16>,
    wrpc_borsh: Option<String>,
    utxoindex: Option<bool>,
    storage_profile: Option<String>,
    rocksdb_preset: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StateDiscord {
    registered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebSession {
    url: String,
    bind_host: String,
    display_host: String,
    port: u16,
    public: bool,
    network: String,
    pid: u32,
    started_unix: u64,
    expires_unix: u64,
    #[serde(default)]
    max_expires_unix: Option<u64>,
    #[serde(default)]
    allowed_client_ips: Vec<String>,
    #[serde(default)]
    firewall_rule_added: bool,
}

#[derive(Debug, Clone, Serialize)]
struct Check {
    check: String,
    value: String,
    status: &'static str,
    detail: Option<String>,
}

#[derive(Debug)]
struct NodePlan {
    network: String,
    service_user: String,
    appdir: PathBuf,
    state_file: PathBuf,
    unit_path: PathBuf,
    env_path: PathBuf,
    service_name: String,
    public_ip: Option<String>,
    p2p_port: u16,
    borsh_endpoint: String,
    unit: String,
    env: String,
    state: SetupState,
    commands: Vec<String>,
}

#[derive(Debug)]
struct NodeSnapshot {
    reachable: bool,
    synced: bool,
    version: Option<String>,
    network: Option<String>,
    virtual_daa_score: Option<u64>,
    utxoindex: Option<bool>,
    error: Option<String>,
}

pub async fn run(ctx: &Ctx, cmd: SetupCmd) -> CliResult {
    match cmd {
        SetupCmd::Preflight(args) => preflight(ctx, &args),
        SetupCmd::Node(args) => node_setup(ctx, &args).await,
        SetupCmd::Status(args) => status(ctx, &args).await,
        SetupCmd::Web(args) => web(ctx, &args).await,
        SetupCmd::WebStatus(args) => web_status(ctx, &args),
        SetupCmd::WebResume(args) => web_resume(ctx, &args),
        SetupCmd::WebStop(args) => web_stop(ctx, &args),
        SetupCmd::Discord(args) => discord(ctx, &args),
    }
}

fn parse_network(network: &str) -> Result<NetworkId, CliError> {
    NetworkId::from_str(network).map_err(|e| CliError::new(exit::GENERIC, format!("invalid network-id '{network}': {e}")))
}

fn net_flags(network: &str) -> Result<Vec<String>, CliError> {
    let nid = parse_network(network)?;
    Ok(match nid.network_type {
        NetworkType::Mainnet => vec![],
        NetworkType::Testnet => {
            let mut flags = vec!["--testnet".to_string()];
            if let Some(suffix) = nid.suffix {
                flags.push(format!("--netsuffix={suffix}"));
            }
            flags
        }
        NetworkType::Devnet => vec!["--devnet".to_string()],
        NetworkType::Simnet => vec!["--simnet".to_string()],
    })
}

fn wrpc_borsh_endpoint(network: &str, explicit: &Option<String>) -> Result<String, CliError> {
    let nid = parse_network(network)?;
    let registry = misaka_endpoints::EndpointRegistry::load(network);
    Ok(misaka_endpoints::resolve(&nid, EndpointKind::NodeWrpcBorsh, explicit.as_deref(), registry.as_ref()))
}

fn command_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    std::env::var_os("PATH")
        .and_then(|paths| std::env::split_paths(&paths).map(|p| p.join(name)).find(|candidate| candidate.is_file()))
}

fn standard_binary_path(name: &str) -> Option<PathBuf> {
    ["/usr/local/bin", "/usr/bin", "/bin", "/root/.cargo/bin"]
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

fn binary_available(name: &str) -> Option<PathBuf> {
    command_path(name).or_else(|| standard_binary_path(name))
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_join(args: &[String]) -> String {
    args.iter().map(|arg| sh_quote(arg)).collect::<Vec<_>>().join(" ")
}

fn read_tail(path: &Path, max_bytes: usize) -> String {
    let Ok(data) = fs::read(path) else {
        return String::new();
    };
    let start = data.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(&data[start..]).into_owned()
}

fn run_output<I, S>(program: &str, args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn run_status<I, S>(program: &str, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program).args(args).status().map(|s| s.success()).unwrap_or(false)
}

fn run_status_quiet<I, S>(program: &str, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

fn run_checked<I, S>(program: &str, args: I) -> CliResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args_vec: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string_lossy().into_owned()).collect();
    let status = Command::new(program)
        .args(&args_vec)
        .status()
        .map_err(|e| CliError::new(exit::GENERIC, format!("run {program} {}: {e}", args_vec.join(" "))))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::new(exit::GENERIC, format!("{program} {} exited with {status}", args_vec.join(" "))))
    }
}

fn root_uid() -> Option<u32> {
    run_output("id", ["-u"]).and_then(|s| s.parse::<u32>().ok())
}

fn user_exists(user: &str) -> bool {
    run_status_quiet("id", ["-u", user])
}

fn service_unit_path(service: &str) -> PathBuf {
    PathBuf::from(format!("/etc/systemd/system/{service}.service"))
}

fn systemctl_available() -> bool {
    command_path("systemctl").is_some()
}

fn systemd_unit_exists(service: &str) -> bool {
    if service_unit_path(service).exists() {
        return true;
    }
    run_output("systemctl", ["show", "-p", "LoadState", "--value", service])
        .map(|state| !state.is_empty() && state != "not-found")
        .unwrap_or(false)
}

fn service_state(service: &str) -> String {
    if !systemctl_available() {
        return "systemctl unavailable".to_string();
    }
    if run_status("systemctl", ["is-active", "--quiet", service]) {
        "active".to_string()
    } else if systemd_unit_exists(service) {
        run_output("systemctl", ["is-active", service]).unwrap_or_else(|| "inactive".to_string())
    } else {
        "not configured".to_string()
    }
}

fn mem_total_gib() -> Option<f64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kb = line.split_whitespace().nth(1)?.parse::<f64>().ok()?;
    Some(kb / 1024.0 / 1024.0)
}

fn existing_path_for_df(path: &Path) -> PathBuf {
    let mut p = path;
    loop {
        if p.exists() {
            return p.to_path_buf();
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return PathBuf::from("/"),
        }
    }
}

fn disk_available_gib(path: &Path) -> Option<f64> {
    let probe = existing_path_for_df(path);
    let probe = probe.display().to_string();
    let out = run_output("df", ["-Pk", probe.as_str()])?;
    let line = out.lines().nth(1)?;
    let avail_kb = line.split_whitespace().nth(3)?.parse::<f64>().ok()?;
    Some(avail_kb / 1024.0 / 1024.0)
}

fn disk_source(path: &Path) -> Option<String> {
    let probe = existing_path_for_df(path);
    let probe = probe.display().to_string();
    let out = run_output("df", ["-P", probe.as_str()])?;
    out.lines().nth(1)?.split_whitespace().next().map(str::to_string)
}

fn storage_is_rotational(path: &Path) -> Option<bool> {
    let source = disk_source(path)?;
    if !source.starts_with("/dev/") {
        return None;
    }
    let value = run_output("lsblk", ["-ndo", "ROTA", source.as_str()])?;
    match value.lines().next()?.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn storage_kind(path: &Path) -> &'static str {
    match storage_is_rotational(path) {
        Some(true) => "hdd",
        Some(false) => "ssd-or-nvme",
        None => "unknown",
    }
}

fn rocksdb_preset_for_storage(storage_profile: &str, appdir: &Path) -> Option<&'static str> {
    match storage_profile {
        "hdd" => Some("hdd"),
        "auto" if storage_is_rotational(appdir) == Some(true) => Some("hdd"),
        _ => None,
    }
}

fn os_pretty_name() -> Option<String> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

fn detect_public_ip() -> Option<String> {
    if let Ok(value) = std::env::var("MISAKA_PUBLIC_IP")
        && !value.trim().is_empty()
    {
        return Some(value.trim().to_string());
    }
    command_path("curl")?;
    run_output("curl", ["-4fsSL", "--max-time", "5", "https://api.ipify.org"])
}

fn parse_ipv4(value: &str) -> Option<Ipv4Addr> {
    value.trim().parse::<Ipv4Addr>().ok()
}

fn ssh_client_ip() -> Option<Ipv4Addr> {
    std::env::var("SSH_CLIENT")
        .ok()
        .and_then(|value| value.split_whitespace().next().and_then(parse_ipv4))
        .or_else(|| std::env::var("SSH_CONNECTION").ok().and_then(|value| value.split_whitespace().next().and_then(parse_ipv4)))
}

fn ssh_server_port() -> u16 {
    std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|value| value.split_whitespace().nth(3).and_then(|port| port.parse::<u16>().ok()))
        .filter(|port| *port > 0)
        .unwrap_or(22)
}

fn web_allowed_client_ips(args: &WebArgs) -> SetupResult<Vec<Ipv4Addr>> {
    let mut ips = Vec::new();
    for raw in &args.allow_client_ips {
        let ip = parse_ipv4(raw).ok_or_else(|| CliError::new(exit::GENERIC, format!("--allow-client-ip must be IPv4: {raw}")))?;
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    if args.restrict_to_ssh_client {
        let ip = ssh_client_ip().ok_or_else(|| {
            CliError::new(
                exit::GENERIC,
                "could not detect SSH client IP from SSH_CLIENT/SSH_CONNECTION; pass --allow-client-ip <IP> or use SSH tunnel",
            )
        })?;
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    Ok(ips)
}

fn ufw_is_active() -> bool {
    run_output("ufw", ["status"])
        .map(|status| status.lines().next().map(|line| line.contains("active")).unwrap_or(false))
        .unwrap_or(false)
}

fn ufw_allow_tcp_port(port: u16) -> bool {
    run_status("ufw", vec!["allow".to_string(), format!("{port}/tcp")])
}

fn ufw_allow_client_tcp_port(ip: Ipv4Addr, port: u16) -> bool {
    run_status(
        "ufw",
        vec![
            "allow".to_string(),
            "from".to_string(),
            ip.to_string(),
            "to".to_string(),
            "any".to_string(),
            "port".to_string(),
            port.to_string(),
            "proto".to_string(),
            "tcp".to_string(),
        ],
    )
}

fn ufw_delete_client_tcp_port(ip: &str, port: u16) -> bool {
    run_status(
        "ufw",
        vec![
            "delete".to_string(),
            "allow".to_string(),
            "from".to_string(),
            ip.to_string(),
            "to".to_string(),
            "any".to_string(),
            "port".to_string(),
            port.to_string(),
            "proto".to_string(),
            "tcp".to_string(),
        ],
    )
}

fn configure_web_firewall(args: &WebArgs, allowed_ips: &[Ipv4Addr]) -> SetupResult<bool> {
    if !args.public || allowed_ips.is_empty() || args.no_ufw {
        return Ok(false);
    }
    if root_uid() != Some(0) {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "--restrict-to-ssh-client/--allow-client-ip needs root to manage UFW"));
    }
    if command_path("ufw").is_none() {
        return Err(CliError::new(exit::GENERIC, "ufw is not installed; cannot restrict public Web UI by client IP"));
    }
    let ssh_port = ssh_server_port();
    let _ = ufw_allow_tcp_port(ssh_port);
    for ip in allowed_ips {
        if !ufw_allow_client_tcp_port(*ip, args.port) {
            return Err(CliError::new(exit::GENERIC, format!("failed to allow {ip} to access tcp/{}", args.port)));
        }
    }
    if !ufw_is_active() {
        let enabled = run_status("ufw", ["--force", "enable"]);
        if !enabled {
            return Err(CliError::new(exit::GENERIC, "failed to enable UFW after adding Web UI allow rules"));
        }
    }
    Ok(true)
}

fn cleanup_web_firewall(port: u16, allowed_ips: &[String]) {
    if command_path("ufw").is_none() {
        return;
    }
    for ip in allowed_ips {
        let _ = ufw_delete_client_tcp_port(ip, port);
    }
}

fn tcp_listening(host: &str, port: u16, timeout: Duration) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
        .unwrap_or(false)
}

fn check(check: impl Into<String>, value: impl Into<String>, status: &'static str, detail: Option<String>) -> Check {
    Check { check: check.into(), value: value.into(), status, detail }
}

fn render_checks(output: OutputFormat, title: &str, checks: &[Check]) {
    match output {
        OutputFormat::Json => {
            let ok = !checks.iter().any(|c| c.status == "FAIL");
            println!("{}", serde_json::json!({ "ok": ok, "title": title, "checks": checks }));
        }
        OutputFormat::Human => {
            println!("{title}");
            println!();
            let width = checks.iter().map(|c| c.check.len()).max().unwrap_or(20).max(20);
            for c in checks {
                match &c.detail {
                    Some(detail) => println!("{:<width$}  {:<36}  {:<5}  {}", c.check, c.value, c.status, detail, width = width),
                    None => println!("{:<width$}  {:<36}  {}", c.check, c.value, c.status, width = width),
                }
            }
        }
    }
}

fn preflight_checks(ctx: &Ctx, args: &PreflightArgs) -> SetupResult<Vec<Check>> {
    let nid = parse_network(&ctx.network)?;
    let p2p_port = nid.default_p2p_port();
    let borsh = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let mut checks = Vec::new();

    checks.push(check("Network", ctx.network.clone(), "OK", None));
    checks.push(check("OS", os_pretty_name().unwrap_or_else(|| "unknown".to_string()), "INFO", None));
    match mem_total_gib() {
        Some(gib) if gib >= 7.5 => checks.push(check("Memory", format!("{gib:.1} GiB"), "OK", None)),
        Some(gib) => checks.push(check("Memory", format!("{gib:.1} GiB"), "WARN", Some("8 GiB以上を推奨".to_string()))),
        None => checks.push(check("Memory", "unknown", "WARN", Some("/proc/meminfo unavailable".to_string()))),
    }
    match disk_available_gib(&args.appdir) {
        Some(gib) if gib >= 100.0 => checks.push(check("Disk available", format!("{gib:.1} GiB"), "OK", None)),
        Some(gib) => checks.push(check("Disk available", format!("{gib:.1} GiB"), "WARN", Some("100 GiB以上を推奨".to_string()))),
        None => checks.push(check("Disk available", "unknown", "WARN", Some("df unavailable".to_string()))),
    }
    let storage = storage_kind(&args.appdir);
    checks.push(check(
        "Storage type",
        storage,
        if storage == "hdd" { "WARN" } else { "INFO" },
        if storage == "hdd" {
            Some("HDD detected; setup node auto-applies --rocksdb-preset=hdd to reduce I/O stalls".to_string())
        } else if storage == "unknown" {
            Some("could not determine whether the data mount is HDD or SSD/NVMe".to_string())
        } else {
            None
        },
    ));
    match root_uid() {
        Some(0) => checks.push(check("Privilege", "root", "OK", None)),
        Some(uid) => {
            checks.push(check("Privilege", format!("uid {uid}"), "WARN", Some("node setup --yes needs root/sudo".to_string())))
        }
        None => checks.push(check("Privilege", "unknown", "WARN", Some("id command unavailable".to_string()))),
    }
    checks.push(check(
        "Public IP",
        detect_public_ip().unwrap_or_else(|| "unknown".to_string()),
        "INFO",
        Some("best-effort; use --public-ip for node setup if unknown".to_string()),
    ));
    checks.push(check(
        "Service user",
        args.service_user.clone(),
        if user_exists(&args.service_user) { "OK" } else { "INFO" },
        if user_exists(&args.service_user) { None } else { Some("will be created by setup node --yes".to_string()) },
    ));
    for bin in ["kaspad", "misaka"] {
        checks.push(match binary_available(bin) {
            Some(path) => check(format!("Binary {bin}"), path.display().to_string(), "OK", None),
            None => check(format!("Binary {bin}"), "not found", "WARN", Some("install release binaries first".to_string())),
        });
    }
    checks.push(check(
        format!("Local P2P {p2p_port}/tcp"),
        if tcp_listening("127.0.0.1", p2p_port, Duration::from_secs(1)) { "listening" } else { "not listening" },
        "INFO",
        None,
    ));
    checks.push(check("wRPC Borsh", borsh, "INFO", Some("node doctor uses this endpoint".to_string())));
    checks.push(check(
        "systemd",
        if systemctl_available() { "available" } else { "not found" },
        if systemctl_available() { "OK" } else { "WARN" },
        None,
    ));
    checks.push(check(
        "UFW",
        if binary_available("ufw").is_some() { "available" } else { "not found" },
        if binary_available("ufw").is_some() { "OK" } else { "INFO" },
        None,
    ));

    Ok(checks)
}

fn preflight(ctx: &Ctx, args: &PreflightArgs) -> CliResult {
    let checks = preflight_checks(ctx, args)?;
    render_checks(ctx.output, "MISAKA setup preflight", &checks);
    Ok(())
}

fn load_state(path: &Path) -> SetupState {
    fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn write_state(path: &Path, state: &SetupState) -> CliResult {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {}: {e}", parent.display())))?;
    }
    let data = toml::to_string_pretty(state).map_err(|e| CliError::new(exit::GENERIC, format!("serialize setup state: {e}")))?;
    fs::write(path, data).map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", path.display())))
}

fn build_kaspad_args(ctx: &Ctx, args: &NodeSetupArgs, p2p_port: u16, public_ip: Option<&str>) -> Result<Vec<String>, CliError> {
    let mut out = net_flags(&ctx.network)?;
    out.push("--yes".to_string());
    out.push(format!("--appdir={}", args.appdir.display()));
    out.push(format!("--listen=0.0.0.0:{p2p_port}"));
    if let Some(ip) = public_ip {
        out.push(format!("--externalip={ip}:{p2p_port}"));
    }
    out.push(format!("--profile={}", args.profile));
    out.push(format!("--outpeers={}", args.outpeers));
    out.push(format!("--maxinpeers={}", args.maxinpeers));
    out.push("--rpcmaxclients=8".to_string());
    out.push(format!("--min-disk-free-percent={}", args.min_disk_free_percent));
    if let Some(preset) = rocksdb_preset_for_storage(&args.storage_profile, &args.appdir) {
        out.push(format!("--rocksdb-preset={preset}"));
    }
    if !args.no_utxoindex {
        out.push("--utxoindex".to_string());
    }
    Ok(out)
}

fn render_unit(service_user: &str, kaspad_args: &[String]) -> String {
    format!(
        "[Unit]\n\
Description=MISAKA kaspad node\n\
After=network-online.target\n\
Wants=network-online.target\n\n\
[Service]\n\
User={service_user}\n\
Group={service_user}\n\
EnvironmentFile=-/etc/misaka/kaspad.env\n\
ExecStart=/usr/local/bin/kaspad {}\n\
Restart=always\n\
RestartSec=10\n\
LimitNOFILE=1048576\n\n\
[Install]\n\
WantedBy=multi-user.target\n",
        kaspad_args.join(" ")
    )
}

fn build_node_plan(ctx: &Ctx, args: &NodeSetupArgs) -> Result<NodePlan, CliError> {
    let nid = parse_network(&ctx.network)?;
    let p2p_port = nid.default_p2p_port();
    let existing_state = load_state(&args.state_file);
    let public_ip = args.public_ip.clone().or_else(|| existing_state.public_ip.clone()).or_else(detect_public_ip);
    let borsh_endpoint = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let kaspad_args = build_kaspad_args(ctx, args, p2p_port, public_ip.as_deref())?;
    let rocksdb_preset = rocksdb_preset_for_storage(&args.storage_profile, &args.appdir).map(str::to_string);
    let unit = render_unit(&args.service_user, &kaspad_args);
    let env = match &public_ip {
        Some(ip) => format!("PUBLIC_IP={ip}\nMISAKA_NETWORK={}\n", ctx.network),
        None => format!("MISAKA_NETWORK={}\n", ctx.network),
    };
    let state = SetupState {
        network_id: Some(ctx.network.clone()),
        public_ip: public_ip.clone(),
        node: StateNode {
            service: Some(args.service.clone()),
            service_user: Some(args.service_user.clone()),
            appdir: Some(args.appdir.display().to_string()),
            profile: Some(args.profile.clone()),
            p2p_port: Some(p2p_port),
            wrpc_borsh: Some(borsh_endpoint.clone()),
            utxoindex: Some(!args.no_utxoindex),
            storage_profile: Some(args.storage_profile.clone()),
            rocksdb_preset,
        },
        ..existing_state
    };
    let mut commands = vec![
        format!("useradd --system --home {} --shell /usr/sbin/nologin {}", args.appdir.display(), args.service_user),
        format!("mkdir -p {}", args.appdir.display()),
        format!("chown -R {}:{} {}", args.service_user, args.service_user, args.appdir.display()),
        format!("install unit {}", service_unit_path(&args.service).display()),
        "systemctl daemon-reload".to_string(),
        format!("systemctl enable --now {}", args.service),
    ];
    if !args.no_ufw {
        commands.push(format!("ufw allow {p2p_port}/tcp"));
    }
    Ok(NodePlan {
        network: ctx.network.clone(),
        service_user: args.service_user.clone(),
        appdir: args.appdir.clone(),
        state_file: args.state_file.clone(),
        unit_path: service_unit_path(&args.service),
        env_path: PathBuf::from("/etc/misaka/kaspad.env"),
        service_name: args.service.clone(),
        public_ip,
        p2p_port,
        borsh_endpoint,
        unit,
        env,
        state,
        commands,
    })
}

fn print_node_plan(output: OutputFormat, plan: &NodePlan) {
    match output {
        OutputFormat::Json => println!("{}", node_plan_json(plan)),
        OutputFormat::Human => {
            println!("MISAKA setup node plan");
            println!();
            println!("Network:      {}", plan.network);
            println!("Service:      {}", plan.service_name);
            println!("Service user: {}", plan.service_user);
            println!("Appdir:       {}", plan.appdir.display());
            println!("Public IP:    {}", plan.public_ip.as_deref().unwrap_or("(unknown; --externalip omitted)"));
            println!("P2P:          {}/tcp", plan.p2p_port);
            println!("wRPC Borsh:   {}", plan.borsh_endpoint);
            println!("State file:   {}", plan.state_file.display());
            println!("Unit file:    {}", plan.unit_path.display());
            println!();
            println!("Planned actions:");
            for cmd in &plan.commands {
                println!("  - {cmd}");
            }
            println!();
            println!("systemd unit preview:");
            println!("{}", plan.unit);
        }
    }
}

fn node_plan_json(plan: &NodePlan) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "network": &plan.network,
        "service": &plan.service_name,
        "serviceUser": &plan.service_user,
        "appdir": plan.appdir.display().to_string(),
        "stateFile": plan.state_file.display().to_string(),
        "unitPath": plan.unit_path.display().to_string(),
        "envPath": plan.env_path.display().to_string(),
        "publicIp": &plan.public_ip,
        "p2pPort": plan.p2p_port,
        "wrpcBorsh": &plan.borsh_endpoint,
        "commands": &plan.commands,
        "unit": &plan.unit,
    })
}

fn write_if_changed(path: &Path, content: &str, force: bool) -> CliResult {
    if path.exists() {
        let current = fs::read_to_string(path).unwrap_or_default();
        if current == content {
            return Ok(());
        }
        if !force {
            return Err(CliError::new(
                exit::UNSAFE_REFUSED,
                format!("{} already exists and differs (use --force to overwrite)", path.display()),
            ));
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {}: {e}", parent.display())))?;
    }
    fs::write(path, content).map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", path.display())))
}

async fn node_setup(ctx: &Ctx, args: &NodeSetupArgs) -> CliResult {
    if args.yes && args.dry_run {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "use either --dry-run or --yes, not both"));
    }
    if !args.yes && !args.dry_run {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "refusing to mutate host without --yes; use --dry-run to preview"));
    }
    let plan = build_node_plan(ctx, args)?;
    if args.dry_run {
        print_node_plan(ctx.output, &plan);
        return Ok(());
    }
    apply_node_plan(&plan, args)?;

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "service": &plan.service_name,
                "serviceUser": &plan.service_user,
                "appdir": plan.appdir.display().to_string(),
                "p2pPort": plan.p2p_port,
                "stateFile": plan.state_file.display().to_string(),
            })
        ),
        OutputFormat::Human => {
            println!("MISAKA node setup complete");
            println!("Service: {}", plan.service_name);
            println!("Appdir:  {}", plan.appdir.display());
            println!("P2P:     {}/tcp", plan.p2p_port);
            println!();
            println!("Next:");
            println!("  misaka setup status");
        }
    }
    Ok(())
}

fn apply_node_plan(plan: &NodePlan, args: &NodeSetupArgs) -> CliResult {
    if root_uid() != Some(0) {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "setup node --yes must be run as root or through sudo"));
    }
    if !Path::new("/usr/local/bin/kaspad").is_file() {
        return Err(CliError::new(exit::GENERIC, "kaspad not found at /usr/local/bin/kaspad; install release binaries first"));
    }
    if !user_exists(&plan.service_user) {
        let appdir = plan.appdir.display().to_string();
        run_checked(
            "useradd",
            vec![
                "--system".to_string(),
                "--home".to_string(),
                appdir,
                "--shell".to_string(),
                "/usr/sbin/nologin".to_string(),
                plan.service_user.clone(),
            ],
        )?;
    }
    fs::create_dir_all(&plan.appdir).map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {}: {e}", plan.appdir.display())))?;
    let user_group = format!("{}:{}", plan.service_user, plan.service_user);
    run_checked("chown", vec!["-R".to_string(), user_group, plan.appdir.display().to_string()])?;
    write_if_changed(&plan.env_path, &plan.env, args.force)?;
    write_state(&plan.state_file, &plan.state)?;
    write_if_changed(&plan.unit_path, &plan.unit, args.force)?;
    run_checked("systemctl", ["daemon-reload"])?;
    run_checked("systemctl", vec!["enable".to_string(), "--now".to_string(), plan.service_name.clone()])?;
    if !args.no_ufw && command_path("ufw").is_some() {
        let _ = run_checked("ufw", vec!["allow".to_string(), format!("{}/tcp", plan.p2p_port)]);
    }
    Ok(())
}

async fn connect_node(ctx: &Ctx) -> Result<KaspaRpcClient, CliError> {
    let hostport = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(ctx.timeout_secs.clamp(2, 15))),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e}")))?;
    Ok(client)
}

async fn node_snapshot(ctx: &Ctx) -> NodeSnapshot {
    match connect_node(ctx).await {
        Ok(client) => {
            let result = client.get_server_info().await;
            let _ = client.disconnect().await;
            match result {
                Ok(info) => NodeSnapshot {
                    reachable: true,
                    synced: info.is_synced,
                    version: Some(info.server_version),
                    network: Some(info.network_id.to_string()),
                    virtual_daa_score: Some(info.virtual_daa_score),
                    utxoindex: Some(info.has_utxo_index),
                    error: None,
                },
                Err(e) => NodeSnapshot {
                    reachable: true,
                    synced: false,
                    version: None,
                    network: None,
                    virtual_daa_score: None,
                    utxoindex: None,
                    error: Some(format!("getServerInfo: {e}")),
                },
            }
        }
        Err(e) => NodeSnapshot {
            reachable: false,
            synced: false,
            version: None,
            network: None,
            virtual_daa_score: None,
            utxoindex: None,
            error: Some(e.msg),
        },
    }
}

async fn status_json_value(ctx: &Ctx, args: &StatusArgs) -> SetupResult<serde_json::Value> {
    let state = load_state(&args.state_file);
    let nid = parse_network(&ctx.network)?;
    let p2p_port = state.node.p2p_port.unwrap_or_else(|| nid.default_p2p_port());
    let snapshot = node_snapshot(ctx).await;
    let p2p = tcp_listening("127.0.0.1", p2p_port, Duration::from_secs(1));
    let public_ip = state.public_ip.clone().or_else(detect_public_ip);
    let node_service = service_state(&args.node_service);
    let seeder_service = service_state(&args.seeder_service);

    Ok(serde_json::json!({
        "ok": snapshot.reachable && snapshot.synced,
        "network": &ctx.network,
        "publicIp": &public_ip,
        "node": {
            "service": &args.node_service,
            "serviceState": &node_service,
            "reachable": snapshot.reachable,
            "synced": snapshot.synced,
            "version": &snapshot.version,
            "network": &snapshot.network,
            "virtualDaaScore": snapshot.virtual_daa_score,
            "utxoIndex": snapshot.utxoindex,
            "error": &snapshot.error,
        },
        "p2p": { "port": p2p_port, "listening": p2p },
        "seeder": { "service": &args.seeder_service, "serviceState": &seeder_service },
    }))
}

async fn status(ctx: &Ctx, args: &StatusArgs) -> CliResult {
    if ctx.output == OutputFormat::Json {
        println!("{}", status_json_value(ctx, args).await?);
        return Ok(());
    }

    let state = load_state(&args.state_file);
    let nid = parse_network(&ctx.network)?;
    let p2p_port = state.node.p2p_port.unwrap_or_else(|| nid.default_p2p_port());
    let snapshot = node_snapshot(ctx).await;
    let p2p = tcp_listening("127.0.0.1", p2p_port, Duration::from_secs(1));
    let public_ip = state.public_ip.clone().or_else(detect_public_ip);
    let node_service = service_state(&args.node_service);
    let seeder_service = service_state(&args.seeder_service);

    println!("MISAKA setup status");
    println!();
    println!("Network:   {}", ctx.network);
    println!("Public IP: {}", public_ip.as_deref().unwrap_or("unknown"));
    println!("Node:      {}", status_label(&node_service));
    println!("Sync:      {}", if snapshot.reachable { if snapshot.synced { "SYNCED" } else { "SYNCING" } } else { "UNREACHABLE" });
    println!("P2P:       {}/tcp {}", p2p_port, if p2p { "LISTENING" } else { "NOT LISTENING" });
    println!("UTXO:      {}", snapshot.utxoindex.map(|v| if v { "ENABLED" } else { "DISABLED" }).unwrap_or("UNKNOWN"));
    println!("Seeder:    {}", status_label(&seeder_service));
    if let Some(daa) = snapshot.virtual_daa_score {
        println!("DAA:       {daa}");
    }
    if let Some(err) = &snapshot.error {
        println!("Node note: {err}");
    }
    println!();
    println!("Next:");
    if !snapshot.reachable {
        println!("  systemctl status {} --no-pager -l", args.node_service);
    } else if !snapshot.synced {
        println!("  wait and run: misaka setup status");
    } else {
        println!("  node is ready");
    }
    Ok(())
}

fn status_label(state: &str) -> &'static str {
    match state {
        "active" => "RUNNING",
        "not configured" => "NOT CONFIGURED",
        _ => "NOT RUNNING",
    }
}

fn discord_command(ip: &str, wallet: Option<&str>) -> String {
    let mut parts = vec![format!("/misaka register ip:{ip}")];
    if let Some(wallet) = wallet {
        parts.push(format!("wallet:{wallet}"));
    }
    parts.join(" ")
}

fn discord(ctx: &Ctx, args: &DiscordArgs) -> CliResult {
    let state = load_state(&args.state_file);
    let ip = args
        .public_ip
        .clone()
        .or(state.public_ip)
        .or_else(detect_public_ip)
        .ok_or_else(|| CliError::new(exit::GENERIC, "public IP unknown; pass --public-ip <IP>"))?;
    let command = discord_command(&ip, args.wallet.as_deref());

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "network": &ctx.network,
                "publicIp": &ip,
                "wallet": &args.wallet,
                "command": &command,
            })
        ),
        OutputFormat::Human => {
            println!("Discord registration");
            println!();
            println!("{command}");
            println!();
            println!("This command contains public identifiers only. Do not paste seed phrases or private keys into Discord.");
        }
    }
    Ok(())
}

fn web_preflight_args(args: &WebArgs) -> PreflightArgs {
    PreflightArgs { service_user: args.service_user.clone(), appdir: args.appdir.clone() }
}

fn web_node_args(args: &WebArgs, yes: bool, dry_run: bool) -> NodeSetupArgs {
    NodeSetupArgs {
        yes,
        dry_run,
        force: args.force,
        no_ufw: args.no_ufw,
        service_user: args.service_user.clone(),
        appdir: args.appdir.clone(),
        service: args.service.clone(),
        state_file: args.state_file.clone(),
        public_ip: args.public_ip.clone(),
        profile: "local-validator".to_string(),
        outpeers: 8,
        maxinpeers: 64,
        min_disk_free_percent: 15,
        storage_profile: args.storage_profile.clone(),
        no_utxoindex: false,
    }
}

fn web_status_args(args: &WebArgs) -> StatusArgs {
    StatusArgs {
        node_service: args.service.clone(),
        seeder_service: DEFAULT_SEEDER_SERVICE.to_string(),
        state_file: args.state_file.clone(),
    }
}

fn browser_host_candidate(value: Option<String>) -> Option<String> {
    let mut host = value?.trim().trim_matches(['[', ']']).to_string();
    if host.is_empty() {
        return None;
    }
    if let Some((without_port, port)) = host.rsplit_once(':')
        && !without_port.contains(':')
        && port.parse::<u16>().is_ok()
    {
        host = without_port.to_string();
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower == "127.0.0.1" || lower == "::1" || lower == "<vps_public_ip>" {
        return None;
    }
    host.parse::<Ipv4Addr>().is_ok().then_some(host)
}

fn public_ip_summary(args: &WebArgs, browser_host: Option<String>) -> serde_json::Value {
    let state = load_state(&args.state_file);
    let saved = state.public_ip.clone();
    let launch = args.public_ip.clone();
    let browser = browser_host_candidate(browser_host);
    let detected = detect_public_ip();
    let (selected, source) = if let Some(ip) = saved.clone() {
        (Some(ip), "saved")
    } else if let Some(ip) = launch.clone() {
        (Some(ip), "launch")
    } else if let Some(ip) = browser.clone() {
        (Some(ip), "browser")
    } else if let Some(ip) = detected.clone() {
        (Some(ip), "detected")
    } else {
        (None, "unknown")
    };

    serde_json::json!({
        "ok": selected.is_some(),
        "publicIpInfo": {
            "publicIp": selected,
            "source": source,
            "confirmed": saved.is_some(),
            "savedIp": saved,
            "launchIp": launch,
            "browserHost": browser,
            "detectedIp": detected,
            "stateFile": args.state_file.display().to_string(),
        }
    })
}

fn public_ip_confirm(ctx: &Ctx, args: &WebArgs, ip: &str) -> SetupResult<serde_json::Value> {
    let ip = ip.trim();
    if ip.parse::<Ipv4Addr>().is_err() {
        return Err(CliError::new(exit::GENERIC, "public IP must be an IPv4 address"));
    }
    let mut state = load_state(&args.state_file);
    state.network_id = Some(ctx.network.clone());
    state.public_ip = Some(ip.to_string());
    write_state(&args.state_file, &state)?;
    Ok(public_ip_summary(args, Some(ip.to_string())))
}

fn job_paths(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = PathBuf::from(SETUP_LOG_DIR);
    (dir.join(format!("{name}.sh")), dir.join(format!("{name}.log")), dir.join(format!("{name}.pid")))
}

fn pid_running(pid: u32) -> bool {
    run_status_quiet("kill", vec!["-0".to_string(), pid.to_string()])
}

fn job_pid(pid_file: &Path) -> Option<u32> {
    fs::read_to_string(pid_file).ok()?.trim().parse::<u32>().ok()
}

fn job_status_value(name: &str) -> serde_json::Value {
    let (_script_path, log_path, pid_path) = job_paths(name);
    let pid = job_pid(&pid_path);
    let log_tail = read_tail(&log_path, 24 * 1024);
    let complete = log_tail.contains("== MISAKA VPS prepare complete ==");
    let running = pid.map(pid_running).unwrap_or(false) && !complete;
    let failed = !running && log_path.exists() && !complete;
    serde_json::json!({
        "name": name,
        "pid": pid,
        "running": running,
        "complete": complete,
        "failed": failed,
        "logPath": log_path.display().to_string(),
        "logs": log_tail,
    })
}

fn prepare_script(args: &WebArgs) -> String {
    let repo_dir = sh_quote(&args.repo_dir.display().to_string());
    let repo_url = sh_quote(&args.repo_url);
    format!(
        r#"#!/bin/sh
set -eu

export DEBIAN_FRONTEND=noninteractive
REPO_DIR={repo_dir}
REPO_URL={repo_url}

echo "== MISAKA VPS prepare started: $(date -Is) =="
echo

echo "== 1/5 install OS packages =="
apt-get update
apt-get install -y git curl ca-certificates build-essential pkg-config libssl-dev protobuf-compiler clang lld tmux ufw dnsutils netcat-openbsd
echo

echo "== 2/5 install Rust if missing =="
if command -v cargo >/dev/null 2>&1; then
  cargo --version
elif [ -x "$HOME/.cargo/bin/cargo" ]; then
  "$HOME/.cargo/bin/cargo" --version
else
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
. "$HOME/.cargo/env"
rustc --version
cargo --version
echo

echo "== 3/5 prepare source =="
mkdir -p "$(dirname "$REPO_DIR")"
if [ ! -d "$REPO_DIR/.git" ]; then
  rm -rf "$REPO_DIR"
  git clone "$REPO_URL" "$REPO_DIR"
else
  echo "source exists: $REPO_DIR"
  echo "keeping current checkout"
fi
cd "$REPO_DIR"
echo "source revision: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo

echo "== 4/5 build release binaries =="
cargo build --release -p kaspad --features evm
cargo build --release -p misaka-cli
echo

echo "== 5/5 install release binaries =="
install -o root -g root -m 0755 target/release/kaspad /usr/local/bin/kaspad
install -o root -g root -m 0755 target/release/misaka /usr/local/bin/misaka

probe_binary() {{
  label="$1"
  shift
  echo "-- $label --"
  if command -v timeout >/dev/null 2>&1; then
    timeout 15 "$@" || echo "WARN: $label check did not finish cleanly; continuing"
  else
    "$@" || echo "WARN: $label check did not finish cleanly; continuing"
  fi
}}

probe_binary "kaspad" /usr/local/bin/kaspad --version
probe_binary "misaka" /usr/local/bin/misaka --version
echo

echo "== MISAKA VPS prepare complete =="
"#
    )
}

fn start_prepare_job(args: &WebArgs) -> SetupResult<serde_json::Value> {
    if root_uid() != Some(0) {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "Prepare VPS must be run as root or through sudo"));
    }
    let (script_path, log_path, pid_path) = job_paths(PREPARE_JOB);
    if let Some(pid) = job_pid(&pid_path)
        && pid_running(pid)
        && !read_tail(&log_path, 24 * 1024).contains("== MISAKA VPS prepare complete ==")
    {
        return Ok(serde_json::json!({
            "ok": true,
            "message": "Prepare VPS is already running.",
            "job": job_status_value(PREPARE_JOB),
        }));
    }
    fs::create_dir_all(SETUP_LOG_DIR).map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {SETUP_LOG_DIR}: {e}")))?;
    fs::write(&script_path, prepare_script(args))
        .map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", script_path.display())))?;
    let log = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| CliError::new(exit::GENERIC, format!("open {}: {e}", log_path.display())))?;
    let err_log = log.try_clone().map_err(|e| CliError::new(exit::GENERIC, format!("clone {}: {e}", log_path.display())))?;
    let child = Command::new("sh")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log))
        .spawn()
        .map_err(|e| CliError::new(exit::GENERIC, format!("start prepare job: {e}")))?;
    fs::write(&pid_path, child.id().to_string())
        .map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", pid_path.display())))?;
    Ok(serde_json::json!({
        "ok": true,
        "message": "Prepare VPS started. Build can take a while; press View Prepare Logs.",
        "job": job_status_value(PREPARE_JOB),
    }))
}

fn bootstrap_status_value(args: &WebArgs) -> serde_json::Value {
    let source_exists = args.repo_dir.join(".git").is_dir();
    let tools = ["git", "curl", "clang", "lld", "protoc", "tmux", "ufw"];
    let tool_values: Vec<serde_json::Value> = tools
        .iter()
        .map(|name| {
            let path = binary_available(name);
            serde_json::json!({
                "name": name,
                "ok": path.is_some(),
                "path": path.map(|p| p.display().to_string()),
            })
        })
        .collect();
    let release_bins = ["kaspad", "misaka"];
    let release_values: Vec<serde_json::Value> = release_bins
        .iter()
        .map(|name| {
            let path = binary_available(name);
            serde_json::json!({
                "name": name,
                "ok": path.is_some(),
                "path": path.map(|p| p.display().to_string()),
            })
        })
        .collect();
    let cargo = binary_available("cargo");
    let rustc = binary_available("rustc");
    let ready = source_exists && cargo.is_some() && release_values.iter().all(|v| v["ok"].as_bool().unwrap_or(false));
    serde_json::json!({
        "ok": ready,
        "bootstrap": {
            "repoDir": args.repo_dir.display().to_string(),
            "repoUrl": &args.repo_url,
            "sourceExists": source_exists,
            "cargo": cargo.map(|p| p.display().to_string()),
            "rustc": rustc.map(|p| p.display().to_string()),
            "tools": tool_values,
            "binaries": release_values,
            "ready": ready,
        },
        "job": job_status_value(PREPARE_JOB),
    })
}

fn random_token() -> String {
    let mut bytes = [0u8; 24];
    if let Ok(mut file) = fs::File::open("/dev/urandom")
        && file.read_exact(&mut bytes).is_ok()
    {
        return bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    format!("{now:x}{:x}", std::process::id())
}

fn target_path(target: &str) -> &str {
    target.split_once('?').map(|(path, _)| path).unwrap_or(target)
}

fn target_has_token(target: &str, token: &str) -> bool {
    target
        .split_once('?')
        .map(|(_, query)| {
            query.split('&').any(|part| {
                let (key, value) = part.split_once('=').unwrap_or((part, ""));
                key == "token" && percent_decode(value) == token
            })
        })
        .unwrap_or(false)
}

fn query_param(target: &str, name: &str) -> Option<String> {
    let query = target.split_once('?')?.1;
    for part in query.split('&') {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        if key == name {
            return Some(percent_decode(value));
        }
    }
    None
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &input[i + 1..i + 3];
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(char::from(byte)),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn js_string_content_escape(input: &str) -> String {
    match serde_json::to_string(input) {
        Ok(json) if json.len() >= 2 => json[1..json.len() - 1].replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026"),
        _ => input.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r"),
    }
}

fn setup_html(token: &str) -> String {
    include_str!("ui/setup.html").replace("__SETUP_TOKEN__", &js_string_content_escape(token))
}

fn dashboard_html(token: &str) -> String {
    include_str!("ui/dashboard.html").replace("__SETUP_TOKEN__", &js_string_content_escape(token))
}

fn learn_html(token: &str) -> String {
    include_str!("ui/learn.html").replace("__SETUP_TOKEN__", &js_string_content_escape(token))
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

fn setup_api_url_from_setup_url(url: &str) -> String {
    url.replace("/setup?", "/api/stop-setup?")
}

fn read_web_session(path: &Path) -> Option<WebSession> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_private_file(path: &Path, body: &str) -> SetupResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CliError::new(exit::GENERIC, format!("create {}: {e}", parent.display())))?;
    }
    fs::write(path, body).map_err(|e| CliError::new(exit::GENERIC, format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn write_web_session(session: &WebSession) -> SetupResult<()> {
    let json = serde_json::to_string_pretty(session)
        .map_err(|e| CliError::new(exit::GENERIC, format!("serialize setup web session: {e}")))?;
    write_private_file(Path::new(DEFAULT_WEB_SESSION_FILE), &format!("{json}\n"))?;
    write_private_file(Path::new(DEFAULT_WEB_URL_FILE), &format!("{}\n", session.url))?;
    Ok(())
}

fn saved_web_url(session_file: &Path, url_file: &Path) -> Option<String> {
    read_web_session(session_file)
        .map(|session| session.url)
        .or_else(|| fs::read_to_string(url_file).ok().map(|url| url.trim().to_string()).filter(|url| !url.is_empty()))
}

fn setup_web_bind_hint(port: u16) -> String {
    let session_path = Path::new(DEFAULT_WEB_SESSION_FILE);
    let url_path = Path::new(DEFAULT_WEB_URL_FILE);
    let saved = saved_web_url(session_path, url_path);
    let mut hint = format!(
        "Port {port} is already in use. A setup Web UI may already be running.\n\n\
         Check the current URL:\n  misaka setup web-status\n\n\
         Stop the saved setup Web UI:\n  misaka setup web-stop\n"
    );
    if let Some(url) = saved {
        hint.push_str("\nLast saved setup URL:\n  ");
        hint.push_str(&url);
        hint.push('\n');
    }
    hint
}

struct HttpRequest {
    method: String,
    target: String,
}

fn read_http_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    let mut buf = [0u8; 16 * 1024];
    let n = stream.read(&mut buf).ok()?;
    if n == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&buf[..n]);
    let mut parts = text.lines().next()?.split_whitespace();
    Some(HttpRequest { method: parts.next()?.to_string(), target: parts.next()?.to_string() })
}

fn http_status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn write_http(stream: &mut TcpStream, code: u16, content_type: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {code} {}\r\n\
Content-Type: {content_type}; charset=utf-8\r\n\
Content-Length: {}\r\n\
Cache-Control: no-store\r\n\
Referrer-Policy: no-referrer\r\n\
X-Frame-Options: DENY\r\n\
X-Content-Type-Options: nosniff\r\n\
Permissions-Policy: camera=(), microphone=(), geolocation=()\r\n\
Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\
Connection: close\r\n\r\n",
        http_status_text(code),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn json_response(value: serde_json::Value) -> (u16, &'static str, String, bool) {
    (200, "application/json", value.to_string(), false)
}

fn json_error(code: u16, msg: impl Into<String>) -> (u16, &'static str, String, bool) {
    (code, "application/json", serde_json::json!({ "ok": false, "error": msg.into() }).to_string(), false)
}

async fn web_route(ctx: &Ctx, args: &WebArgs, token: &str, req: &HttpRequest) -> (u16, &'static str, String, bool) {
    let path = target_path(&req.target);
    if !target_has_token(&req.target, token) {
        if path == "/" || path == "/setup" {
            return (403, "text/plain", "bad or missing setup token".to_string(), false);
        }
        return json_error(403, "bad or missing setup token");
    }

    match (req.method.as_str(), path) {
        ("GET", "/") | ("GET", "/setup") => (200, "text/html", setup_html(token), false),
        ("GET", "/dashboard") => (200, "text/html", dashboard_html(token), false),
        ("GET", "/learn") => (200, "text/html", learn_html(token), false),
        ("GET", "/api/session/ping") => json_response(serde_json::json!({
            "ok": true,
            "message": "setup session is alive",
        })),
        ("GET", "/api/bootstrap/status") => json_response(bootstrap_status_value(args)),
        ("POST", "/api/bootstrap/prepare") => match start_prepare_job(args) {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("GET", "/api/bootstrap/logs") => json_response(serde_json::json!({
            "ok": true,
            "job": job_status_value(PREPARE_JOB),
            "logs": job_status_value(PREPARE_JOB)["logs"].clone(),
        })),
        ("GET", "/api/public-ip") => json_response(public_ip_summary(args, query_param(&req.target, "browserHost"))),
        ("POST", "/api/public-ip/confirm") => {
            let ip = query_param(&req.target, "ip").unwrap_or_default();
            match public_ip_confirm(ctx, args, &ip) {
                Ok(value) => json_response(value),
                Err(e) => json_error(500, e.msg),
            }
        }
        ("GET", "/api/preflight") | ("POST", "/api/preflight") => match preflight_checks(ctx, &web_preflight_args(args)) {
            Ok(checks) => {
                let ok = !checks.iter().any(|c| c.status == "FAIL");
                json_response(serde_json::json!({ "ok": ok, "checks": checks }))
            }
            Err(e) => json_error(500, e.msg),
        },
        ("GET", "/api/status") => match status_json_value(ctx, &web_status_args(args)).await {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("POST", "/api/node/dry-run") => {
            let node_args = web_node_args(args, false, true);
            match build_node_plan(ctx, &node_args) {
                Ok(plan) => json_response(node_plan_json(&plan)),
                Err(e) => json_error(500, e.msg),
            }
        }
        ("POST", "/api/node/apply") => {
            let node_args = web_node_args(args, true, false);
            match build_node_plan(ctx, &node_args).and_then(|plan| apply_node_plan(&plan, &node_args).map(|()| plan)) {
                Ok(plan) => json_response(serde_json::json!({
                    "ok": true,
                    "message": "Node service was installed and started. Press Check Sync next.",
                    "service": plan.service_name,
                    "p2pPort": plan.p2p_port,
                })),
                Err(e) => json_error(500, e.msg),
            }
        }
        ("POST", "/api/node/restart") => match run_checked("systemctl", vec!["restart".to_string(), args.service.clone()]) {
            Ok(()) => json_response(serde_json::json!({ "ok": true, "message": "Node service restarted. Press Check Sync next." })),
            Err(e) => json_error(500, e.msg),
        },
        ("GET", "/api/logs") => {
            let logs = run_output("journalctl", ["-u", args.service.as_str(), "-n", "80", "--no-pager"])
                .unwrap_or_else(|| "No logs available, or journalctl is unavailable.".to_string());
            json_response(serde_json::json!({ "ok": true, "logs": logs }))
        }
        ("POST", "/api/stop-setup") => (
            200,
            "application/json",
            serde_json::json!({ "ok": true, "message": "Setup page is stopping. You can close this tab." }).to_string(),
            true,
        ),
        (_, _) if path.starts_with("/api/") => json_error(404, "unknown setup API"),
        _ => (404, "text/plain", "not found".to_string(), false),
    }
}

fn web_status(ctx: &Ctx, args: &WebStatusArgs) -> CliResult {
    let session = read_web_session(&args.session_file);
    let url = session.as_ref().map(|s| s.url.clone()).or_else(|| saved_web_url(&args.session_file, &args.url_file));
    let now = unix_now();
    let listening = session.as_ref().map(|s| tcp_listening("127.0.0.1", s.port, Duration::from_secs(1))).unwrap_or(false);
    let expired = session.as_ref().map(|s| now >= s.expires_unix).unwrap_or(false);
    let expires_in_secs = session.as_ref().map(|s| s.expires_unix.saturating_sub(now));

    match ctx.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "ok": url.is_some(),
                    "url": url,
                    "session": session,
                    "listening": listening,
                    "expired": expired,
                    "expiresInSecs": expires_in_secs,
                })
            );
        }
        OutputFormat::Human => {
            println!("MISAKA setup Web UI status");
            println!();
            if let Some(url) = url {
                println!("URL:       {url}");
                println!("Listening: {}", if listening { "yes" } else { "no" });
                println!("Expired:   {}", if expired { "yes" } else { "no" });
                if let Some(secs) = expires_in_secs {
                    println!("Expires:   in {} minute(s)", secs.div_ceil(60));
                }
                println!();
                println!("Stop:");
                println!("  misaka --network {} setup web-stop", ctx.network);
            } else {
                println!("No saved setup Web UI URL was found.");
                println!();
                println!("Start:");
                println!("  misaka --network {} setup web --public --public-ip <VPS_IP>", ctx.network);
            }
        }
    }
    Ok(())
}

fn display_host_from_session(session: Option<&WebSession>) -> Option<String> {
    session
        .map(|s| s.display_host.trim().to_string())
        .filter(|host| host.parse::<Ipv4Addr>().is_ok())
        .filter(|host| host != "127.0.0.1")
}

fn dashboard_url(display_host: &str, port: u16, token: &str) -> String {
    format!("http://{display_host}:{port}/dashboard?token={}", percent_encode_query(token))
}

fn setup_url(display_host: &str, port: u16, token: &str) -> String {
    format!("http://{display_host}:{port}/setup?token={}", percent_encode_query(token))
}

fn resume_allowed_client_ips(args: &WebResumeArgs, session: Option<&WebSession>, public: bool) -> SetupResult<Vec<String>> {
    let mut ips = Vec::new();
    let mut add_ip = |raw: &str| -> SetupResult<()> {
        let ip = parse_ipv4(raw).ok_or_else(|| CliError::new(exit::GENERIC, format!("--allow-client-ip must be IPv4: {raw}")))?;
        let value = ip.to_string();
        if !ips.contains(&value) {
            ips.push(value);
        }
        Ok(())
    };

    for raw in &args.allow_client_ips {
        add_ip(raw)?;
    }
    if let Some(session) = session {
        for raw in &session.allowed_client_ips {
            add_ip(raw)?;
        }
    }
    if public
        && !args.no_restrict_to_ssh_client
        && let Some(ip) = ssh_client_ip()
    {
        add_ip(&ip.to_string())?;
    }
    Ok(ips)
}

fn web_resume_command(
    ctx: &Ctx,
    args: &WebResumeArgs,
    token: &str,
    display_host: &str,
    allowed_ips: &[String],
) -> SetupResult<Vec<String>> {
    let exe = std::env::current_exe()
        .map_err(|e| CliError::new(exit::GENERIC, format!("locate current misaka executable: {e}")))?
        .display()
        .to_string();
    let mut cmd = vec![exe, "--network".to_string(), ctx.network.clone()];
    if let Some(rpc) = &ctx.rpc {
        cmd.extend(["--rpc".to_string(), rpc.clone()]);
    }
    if let Some(grpc) = &ctx.node_grpc {
        cmd.extend(["--node-grpc".to_string(), grpc.clone()]);
    }
    cmd.extend(["--evm-rpc".to_string(), ctx.evm_rpc.clone()]);
    cmd.extend(["--timeout".to_string(), ctx.timeout_secs.to_string()]);
    cmd.extend(["setup".to_string(), "web".to_string()]);
    if !args.local {
        cmd.push("--public".to_string());
        cmd.extend(["--public-ip".to_string(), display_host.to_string()]);
    }
    cmd.extend(["--port".to_string(), args.port.to_string()]);
    cmd.extend(["--token".to_string(), token.to_string()]);
    cmd.extend(["--ttl-minutes".to_string(), args.ttl_minutes.to_string()]);
    cmd.extend(["--max-ttl-minutes".to_string(), args.max_ttl_minutes.to_string()]);
    for ip in allowed_ips {
        cmd.extend(["--allow-client-ip".to_string(), ip.clone()]);
    }
    if args.force {
        cmd.push("--force".to_string());
    }
    cmd.extend(["--storage-profile".to_string(), args.storage_profile.clone()]);
    if args.no_ufw {
        cmd.push("--no-ufw".to_string());
    }
    cmd.extend(["--service-user".to_string(), args.service_user.clone()]);
    cmd.extend(["--appdir".to_string(), args.appdir.display().to_string()]);
    cmd.extend(["--service".to_string(), args.service.clone()]);
    cmd.extend(["--state-file".to_string(), args.state_file.display().to_string()]);
    cmd.extend(["--repo-dir".to_string(), args.repo_dir.display().to_string()]);
    cmd.extend(["--repo-url".to_string(), args.repo_url.clone()]);
    Ok(cmd)
}

fn web_resume(ctx: &Ctx, args: &WebResumeArgs) -> CliResult {
    let session = read_web_session(&args.session_file);
    let alive = session.as_ref().map(|s| tcp_listening("127.0.0.1", s.port, Duration::from_secs(1))).unwrap_or(false);

    if alive {
        let url = session.as_ref().map(|s| s.url.clone()).or_else(|| saved_web_url(&args.session_file, &args.url_file));
        let dashboard = url.as_ref().map(|url| url.replacen("/setup?", "/dashboard?", 1));
        match ctx.output {
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "reused": true,
                    "url": url,
                    "dashboardUrl": dashboard,
                    "session": session,
                })
            ),
            OutputFormat::Human => {
                println!("MISAKA setup Web UI is already running.");
                println!();
                if let Some(url) = url {
                    println!("Open:");
                    println!("  {url}");
                }
                if let Some(dashboard) = dashboard {
                    println!("Dashboard:");
                    println!("  {dashboard}");
                }
            }
        }
        return Ok(());
    }

    if command_path("tmux").is_none() {
        return Err(CliError::new(exit::GENERIC, "tmux is not installed; cannot start setup Web UI in the background"));
    }

    let public = !args.local;
    let display_host = if public {
        args.public_ip
            .clone()
            .or_else(|| display_host_from_session(session.as_ref()))
            .or_else(detect_public_ip)
            .ok_or_else(|| CliError::new(exit::GENERIC, "public IP unknown; pass --public-ip <VPS_IP>"))?
    } else {
        "127.0.0.1".to_string()
    };
    let allowed_ips = resume_allowed_client_ips(args, session.as_ref(), public)?;
    if public && !args.no_ufw && !args.no_restrict_to_ssh_client && allowed_ips.is_empty() {
        return Err(CliError::new(
            exit::GENERIC,
            "could not detect a client IP to restrict Web UI access; pass --allow-client-ip <YOUR_IP> or --no-restrict-to-ssh-client",
        ));
    }

    if let Some(session) = &session
        && session.firewall_rule_added
    {
        cleanup_web_firewall(session.port, &session.allowed_client_ips);
    }
    let _ = fs::remove_file(&args.session_file);
    let _ = fs::remove_file(&args.url_file);
    let _ = run_status("tmux", ["kill-session", "-t", args.tmux_session.as_str()]);

    let token = random_token();
    let url = setup_url(&display_host, args.port, &token);
    let dashboard = dashboard_url(&display_host, args.port, &token);
    let cmd = web_resume_command(ctx, args, &token, &display_host, &allowed_ips)?;
    let shell_cmd = format!("exec {}", shell_join(&cmd));
    run_checked("tmux", vec!["new-session".to_string(), "-d".to_string(), "-s".to_string(), args.tmux_session.clone(), shell_cmd])?;
    thread::sleep(Duration::from_secs(2));

    let started = read_web_session(&args.session_file)
        .as_ref()
        .map(|s| tcp_listening("127.0.0.1", s.port, Duration::from_secs(1)))
        .unwrap_or(false);
    if !started {
        return Err(CliError::new(
            exit::GENERIC,
            "setup Web UI did not start; check the tmux session or run `misaka setup web` directly",
        ));
    }

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "reused": false,
                "url": url,
                "dashboardUrl": dashboard,
                "tmuxSession": args.tmux_session,
                "allowedClientIps": allowed_ips,
            })
        ),
        OutputFormat::Human => {
            println!("MISAKA setup Web UI restarted.");
            println!();
            println!("Open:");
            println!("  {url}");
            println!("Dashboard:");
            println!("  {dashboard}");
            if public && !allowed_ips.is_empty() {
                println!();
                println!("Allowed client IP(s): {}", allowed_ips.join(", "));
            }
            println!();
            println!("Show URL again:");
            println!("  misaka --network {} setup web-status", ctx.network);
            println!("Stop:");
            println!("  misaka --network {} setup web-stop", ctx.network);
        }
    }

    Ok(())
}

fn web_stop(ctx: &Ctx, args: &WebStopArgs) -> CliResult {
    let session = read_web_session(&args.session_file);
    let url = session.as_ref().map(|s| s.url.clone()).or_else(|| saved_web_url(&args.session_file, &args.url_file));
    let stopped_by_api = url
        .as_ref()
        .map(|url| {
            let api_url = setup_api_url_from_setup_url(url);
            run_status("curl", vec!["-fsS".to_string(), "-X".to_string(), "POST".to_string(), api_url])
        })
        .unwrap_or(false);
    let stopped_by_tmux = run_status("tmux", ["kill-session", "-t", args.tmux_session.as_str()]);
    if let Some(session) = &session
        && session.firewall_rule_added
    {
        cleanup_web_firewall(session.port, &session.allowed_client_ips);
    }
    let _ = fs::remove_file(&args.session_file);
    let _ = fs::remove_file(&args.url_file);

    match ctx.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "ok": stopped_by_api || stopped_by_tmux,
                    "stoppedByApi": stopped_by_api,
                    "stoppedByTmux": stopped_by_tmux,
                    "url": url,
                })
            );
        }
        OutputFormat::Human => {
            if stopped_by_api || stopped_by_tmux {
                println!("MISAKA setup Web UI stop requested.");
            } else {
                println!("No running setup Web UI session was stopped.");
                println!("If it is running in another terminal, stop that process with Ctrl-C.");
            }
        }
    }
    Ok(())
}

async fn web(ctx: &Ctx, args: &WebArgs) -> CliResult {
    let bind_host = if args.public { "0.0.0.0" } else { "127.0.0.1" };
    let listener = TcpListener::bind((bind_host, args.port)).map_err(|e| {
        CliError::new(exit::GENERIC, format!("bind {bind_host}:{}: {e}\n\n{}", args.port, setup_web_bind_hint(args.port)))
    })?;
    listener.set_nonblocking(true).map_err(|e| CliError::new(exit::GENERIC, format!("set nonblocking listener: {e}")))?;

    let token = args.token.clone().unwrap_or_else(random_token);
    let allowed_ips = web_allowed_client_ips(args)?;
    let firewall_rule_added = configure_web_firewall(args, &allowed_ips)?;
    let display_host = if args.public {
        args.public_ip.clone().or_else(detect_public_ip).unwrap_or_else(|| "<VPS_PUBLIC_IP>".to_string())
    } else {
        "127.0.0.1".to_string()
    };
    let ttl = Duration::from_secs(args.ttl_minutes.max(1).saturating_mul(60));
    let max_ttl_minutes = args.max_ttl_minutes.max(args.ttl_minutes.max(1));
    let max_ttl = Duration::from_secs(max_ttl_minutes.saturating_mul(60));
    let now = unix_now();
    let url_token = percent_encode_query(&token);
    let url = format!("http://{display_host}:{}/setup?token={url_token}", args.port);
    let dashboard_url = format!("http://{display_host}:{}/dashboard?token={url_token}", args.port);
    let mut session = WebSession {
        url: url.clone(),
        bind_host: bind_host.to_string(),
        display_host: display_host.clone(),
        port: args.port,
        public: args.public,
        network: ctx.network.clone(),
        pid: std::process::id(),
        started_unix: now,
        expires_unix: now.saturating_add(ttl.as_secs()),
        max_expires_unix: Some(now.saturating_add(max_ttl.as_secs())),
        allowed_client_ips: allowed_ips.iter().map(ToString::to_string).collect(),
        firewall_rule_added,
    };
    if let Err(e) = write_web_session(&session) {
        eprintln!("Warning: could not save setup Web UI URL: {}", e.msg);
    }

    println!("MISAKA Setup is ready.");
    println!();
    println!("Open:");
    println!("  {url}");
    println!("Dashboard:");
    println!("  {dashboard_url}");
    println!();
    println!("Saved URL:");
    println!("  {DEFAULT_WEB_URL_FILE}");
    println!("Show URL again:");
    println!("  misaka --network {} setup web-status", ctx.network);
    println!();
    println!("This setup page stops after {} minute(s) without a valid browser request.", args.ttl_minutes.max(1));
    println!("It also stops after {max_ttl_minutes} minute(s) at maximum.");
    if args.public {
        if allowed_ips.is_empty() {
            println!("Warning: public HTTP mode is open to the network. Keep the token URL private.");
        } else {
            println!("Web UI allowed client IP(s): {}", allowed_ips.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "));
            if firewall_rule_added {
                println!("UFW restriction is active for tcp/{}.", args.port);
            }
        }
    }
    if root_uid() != Some(0) {
        println!("Note: Install / Start Node requires running this command with sudo/root.");
    }

    let mut stop = false;
    let mut expires_at = Instant::now() + ttl;
    let max_expires_at = Instant::now() + max_ttl;
    let mut last_session_write = Instant::now();
    while !stop && Instant::now() < expires_at && Instant::now() < max_expires_at {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let Some(req) = read_http_request(&mut stream) else {
                    let body = serde_json::json!({ "ok": false, "error": "bad request" }).to_string();
                    write_http(&mut stream, 400, "application/json", &body);
                    continue;
                };
                let path = target_path(&req.target);
                if target_has_token(&req.target, &token) && path != "/api/stop-setup" {
                    expires_at = Instant::now() + ttl;
                    let renewed = unix_now().saturating_add(ttl.as_secs());
                    session.expires_unix = session.max_expires_unix.map(|max| renewed.min(max)).unwrap_or(renewed);
                    if last_session_write.elapsed() >= Duration::from_secs(30) {
                        let _ = write_web_session(&session);
                        last_session_write = Instant::now();
                    }
                }
                let (code, content_type, body, should_stop) = web_route(ctx, args, &token, &req).await;
                write_http(&mut stream, code, content_type, &body);
                stop = should_stop;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(CliError::new(exit::GENERIC, format!("accept setup web connection: {e}"))),
        }
    }

    if firewall_rule_added {
        cleanup_web_firewall(args.port, &session.allowed_client_ips);
    }
    println!("MISAKA setup web stopped.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_ctx() -> Ctx {
        Ctx {
            output: OutputFormat::Human,
            network: "testnet-10".to_string(),
            rpc: None,
            node_grpc: None,
            evm_rpc: "http://127.0.0.1:8545".to_string(),
            timeout_secs: 3,
            quiet: false,
        }
    }

    fn test_state_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("misaka-setup-{name}-{}.toml", std::process::id()))
    }

    fn web_args() -> WebArgs {
        WebArgs {
            public: true,
            port: 8787,
            public_ip: Some("203.0.113.10".to_string()),
            token: None,
            ttl_minutes: 60,
            max_ttl_minutes: 720,
            restrict_to_ssh_client: false,
            allow_client_ips: vec![],
            force: false,
            storage_profile: "auto".to_string(),
            no_ufw: false,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            repo_dir: PathBuf::from(DEFAULT_REPO_DIR),
            repo_url: DEFAULT_REPO_URL.to_string(),
        }
    }

    #[test]
    fn network_flags_match_testnet_suffix() {
        assert_eq!(net_flags("testnet-10").unwrap(), vec!["--testnet".to_string(), "--netsuffix=10".to_string()]);
    }

    #[test]
    fn discord_command_omits_missing_optional_values() {
        assert_eq!(discord_command("203.0.113.10", None), "/misaka register ip:203.0.113.10");
        assert_eq!(
            discord_command("203.0.113.10", Some("misakatest:qexample")),
            "/misaka register ip:203.0.113.10 wallet:misakatest:qexample"
        );
    }

    #[test]
    fn setup_state_with_a_retired_validator_table_still_loads() {
        let old = r#"
network_id = "testnet-10"
public_ip = "203.0.113.30"

[node]
service = "misaka-kaspad"
service_user = "misaka_user"
appdir = "/var/lib/misaka"
profile = "local-validator"
p2p_port = 26211
utxoindex = true

[validator]
bond_outpoint = "abc:0"
validator_id = "validator123"
funding_address = "misakatest:qexample"
mining_address = "misakatest:qexample"
miner_threads = 2
miner_start_daa_score = 1000
key = "/var/lib/misaka/validator/validator.seed"
signed_epoch_db = "/var/lib/misaka/validator/validator.state"

[discord]
registered = true
"#;
        let state: SetupState = toml::from_str(old).expect("a state file with a [validator] table must still parse");
        assert_eq!(state.network_id.as_deref(), Some("testnet-10"));
        assert_eq!(state.public_ip.as_deref(), Some("203.0.113.30"));
        assert_eq!(state.node.service.as_deref(), Some("misaka-kaspad"));
        assert_eq!(state.node.service_user.as_deref(), Some("misaka_user"));
        assert_eq!(state.node.profile.as_deref(), Some("local-validator"));
        assert_eq!(state.node.p2p_port, Some(26211));
        assert_eq!(state.node.utxoindex, Some(true));
        assert!(state.discord.registered);

        // load_state swallows parse errors into the default, so check the file path too:
        // a default state would have no public IP.
        let state_file = test_state_file("retired-validator-table");
        fs::write(&state_file, old).unwrap();
        let loaded = load_state(&state_file);
        assert_eq!(loaded.public_ip.as_deref(), Some("203.0.113.30"));
        assert_eq!(loaded.node.appdir.as_deref(), Some("/var/lib/misaka"));

        // The limitation, pinned: the next write keeps the node values and drops the retired table.
        write_state(&state_file, &loaded).unwrap();
        let rewritten = fs::read_to_string(&state_file).unwrap();
        let _ = fs::remove_file(&state_file);
        assert!(rewritten.contains("203.0.113.30"));
        assert!(!rewritten.contains("[validator]"));
        assert!(!rewritten.contains("bond_outpoint"));
    }

    #[tokio::test]
    async fn retired_validator_and_miner_endpoints_are_unknown() {
        let web = web_args();
        for (method, path) in [
            ("GET", "/api/validator/status"),
            ("POST", "/api/validator/keygen"),
            ("POST", "/api/validator/balance"),
            ("POST", "/api/validator/bond"),
            ("GET", "/api/validator/chain-status"),
            ("POST", "/api/validator/service/apply"),
            ("GET", "/api/validator/logs"),
            ("GET", "/api/miner/status"),
            ("GET", "/api/miner/diagnostics"),
            ("POST", "/api/miner/service/apply"),
            ("POST", "/api/miner/service/stop"),
            ("GET", "/api/miner/logs"),
        ] {
            let req = HttpRequest { method: method.to_string(), target: format!("{path}?token=tok") };
            let (code, _content_type, body, stop) = web_route(&base_ctx(), &web, "tok", &req).await;
            assert_eq!(code, 404, "{method} {path} should be gone");
            assert!(body.contains("unknown setup API"), "{method} {path}: {body}");
            assert!(!stop);
        }
    }

    #[test]
    fn unit_uses_service_user_and_kaspad_args() {
        let unit = render_unit("misaka_user", &["--testnet".into(), "--netsuffix=10".into()]);
        assert!(unit.contains("User=misaka_user"));
        assert!(unit.contains("Group=misaka_user"));
        assert!(unit.contains("ExecStart=/usr/local/bin/kaspad --testnet --netsuffix=10"));
    }

    #[test]
    fn node_plan_defaults_to_wallet_ready_utxoindex() {
        let args = NodeSetupArgs {
            yes: false,
            dry_run: true,
            force: false,
            no_ufw: true,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            public_ip: Some("203.0.113.10".to_string()),
            profile: "local-validator".to_string(),
            outpeers: 8,
            maxinpeers: 64,
            min_disk_free_percent: 15,
            storage_profile: "auto".to_string(),
            no_utxoindex: false,
        };
        let plan = build_node_plan(&base_ctx(), &args).unwrap();
        assert!(plan.unit.contains("--externalip=203.0.113.10:26211"));
        assert!(plan.unit.contains("--utxoindex"));
        // kaspad's loopback wRPC-Borsh listener bundle keeps its name.
        assert!(plan.unit.contains("--profile=local-validator"));
        assert_eq!(plan.state.node.service_user.as_deref(), Some("misaka_user"));
    }

    #[test]
    fn node_plan_can_force_hdd_rocksdb_preset() {
        let args = NodeSetupArgs {
            yes: false,
            dry_run: true,
            force: false,
            no_ufw: true,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            public_ip: Some("203.0.113.10".to_string()),
            profile: "local-validator".to_string(),
            outpeers: 8,
            maxinpeers: 64,
            min_disk_free_percent: 15,
            storage_profile: "hdd".to_string(),
            no_utxoindex: false,
        };
        let plan = build_node_plan(&base_ctx(), &args).unwrap();
        assert!(plan.unit.contains("--rocksdb-preset=hdd"));
        assert_eq!(plan.state.node.rocksdb_preset.as_deref(), Some("hdd"));
    }

    #[test]
    fn node_plan_uses_confirmed_state_public_ip() {
        let state_file = test_state_file("confirmed-ip");
        let state = SetupState { public_ip: Some("203.0.113.22".to_string()), ..Default::default() };
        write_state(&state_file, &state).unwrap();
        let args = NodeSetupArgs {
            yes: false,
            dry_run: true,
            force: false,
            no_ufw: true,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: state_file.clone(),
            public_ip: None,
            profile: "local-validator".to_string(),
            outpeers: 8,
            maxinpeers: 64,
            min_disk_free_percent: 15,
            storage_profile: "auto".to_string(),
            no_utxoindex: false,
        };
        let plan = build_node_plan(&base_ctx(), &args).unwrap();
        assert!(plan.unit.contains("--externalip=203.0.113.22:26211"));
        let _ = fs::remove_file(state_file);
    }

    #[test]
    fn browser_host_candidate_rejects_localhost_and_strips_port() {
        assert_eq!(browser_host_candidate(Some("203.0.113.44:8787".to_string())).as_deref(), Some("203.0.113.44"));
        assert_eq!(browser_host_candidate(Some("localhost".to_string())), None);
        assert_eq!(browser_host_candidate(Some("127.0.0.1".to_string())), None);
        assert_eq!(browser_host_candidate(Some("setup.example.com".to_string())), None);
    }

    #[test]
    fn status_label_is_stable() {
        assert_eq!(status_label("active"), "RUNNING");
        assert_eq!(status_label("not configured"), "NOT CONFIGURED");
        assert_eq!(status_label("inactive"), "NOT RUNNING");
    }

    #[test]
    fn setup_token_is_required_in_query() {
        assert!(target_has_token("/api/status?token=abc123", "abc123"));
        assert!(target_has_token("/api/status?x=1&token=abc123", "abc123"));
        assert!(target_has_token("/api/status?token=abc%22%26", "abc\"&"));
        assert!(!target_has_token("/api/status?token=wrong", "abc123"));
        assert!(!target_has_token("/api/status", "abc123"));
    }

    #[test]
    fn setup_token_is_js_string_escaped() {
        let html = setup_html("abc\"</script>");
        assert!(html.contains("abc\\\"\\u003c/script\\u003e"));
        assert!(!html.contains("const token = \"abc\"</script>\";"));
    }

    #[test]
    fn web_node_args_are_wallet_ready_by_default() {
        let args = web_node_args(&web_args(), true, false);
        assert!(args.yes);
        assert!(!args.dry_run);
        assert!(!args.no_utxoindex);
        assert_eq!(args.profile, "local-validator");
        assert_eq!(args.public_ip.as_deref(), Some("203.0.113.10"));
    }

    #[test]
    fn web_resume_command_starts_setup_web_with_allowed_ip() {
        let args = WebResumeArgs {
            session_file: test_state_file("web-resume-session"),
            url_file: test_state_file("web-resume-url"),
            tmux_session: DEFAULT_WEB_TMUX_SESSION.to_string(),
            local: false,
            port: 8787,
            public_ip: Some("203.0.113.10".to_string()),
            ttl_minutes: 60,
            max_ttl_minutes: 720,
            no_restrict_to_ssh_client: false,
            allow_client_ips: vec!["198.51.100.20".to_string()],
            force: false,
            storage_profile: "auto".to_string(),
            no_ufw: false,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            repo_dir: PathBuf::from(DEFAULT_REPO_DIR),
            repo_url: DEFAULT_REPO_URL.to_string(),
        };
        let command = web_resume_command(&base_ctx(), &args, "tok", "203.0.113.10", &args.allow_client_ips).unwrap();
        assert!(command.windows(2).any(|w| w == ["setup", "web"]));
        assert!(command.windows(2).any(|w| w == ["--token", "tok"]));
        assert!(command.windows(2).any(|w| w == ["--allow-client-ip", "198.51.100.20"]));
        assert!(command.windows(2).any(|w| w == ["--public-ip", "203.0.113.10"]));
    }

    #[test]
    fn prepare_script_builds_kaspad_with_evm_feature() {
        let script = prepare_script(&web_args());
        assert!(script.contains("cargo build --release -p kaspad --features evm\n"));
        assert!(script.contains("cargo build --release -p misaka-cli\n"));
        assert!(script.contains("install -o root -g root -m 0755 target/release/kaspad /usr/local/bin/kaspad"));
        assert!(script.contains("install -o root -g root -m 0755 target/release/misaka /usr/local/bin/misaka"));
        assert!(script.contains("probe_binary \"kaspad\" /usr/local/bin/kaspad --version"));
        assert!(script.contains("probe_binary \"misaka\" /usr/local/bin/misaka --version"));
        // The retired validator sidecar and funding miner are neither built nor installed.
        assert!(!script.contains("kaspa-pq-validator"));
        assert!(!script.contains("misaminer"));
    }
}
