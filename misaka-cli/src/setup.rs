//! Guided VPS setup helpers.
//!
//! This module intentionally starts with a conservative MVP: read-only
//! preflight/status checks, a dry-runnable node service installer, and Discord
//! registration text generation. Mutating wallet/validator operations remain
//! outside this layer.

use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
const DEFAULT_VALIDATOR_SERVICE: &str = "misaka-validator";
#[cfg(test)]
const DEFAULT_REPO_DIR: &str = "/opt/misakas";
#[cfg(test)]
const DEFAULT_REPO_URL: &str = "https://github.com/MISAKA-BTC/misakas.git";
const SETUP_LOG_DIR: &str = "/var/log/misaka-setup";
const PREPARE_JOB: &str = "prepare-vps";
const DEFAULT_VALIDATOR_DIR: &str = "/var/lib/misaka/validator";
const DEFAULT_VALIDATOR_KEY: &str = "/var/lib/misaka/validator/validator.seed";
const DEFAULT_VALIDATOR_DB: &str = "/var/lib/misaka/validator/validator.state";
const DEFAULT_VALIDATOR_ENV: &str = "/etc/misaka/validator.env";

#[derive(Subcommand, Debug)]
pub enum SetupCmd {
    /// Check whether this VPS looks ready for MISAKA setup.
    Preflight(PreflightArgs),
    /// Create or preview the kaspad systemd service.
    Node(NodeSetupArgs),
    /// Show node/seeder/validator status in one place.
    Status(StatusArgs),
    /// Start a temporary browser setup wizard for button-first node joining.
    Web(WebArgs),
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
    /// Do not add --utxoindex. By default node setup is validator/wallet-ready.
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
    /// Validator service name.
    #[arg(long, default_value = "misaka-validator")]
    validator_service: String,
    /// Setup state file path.
    #[arg(long, default_value = "/etc/misaka/setup.toml")]
    state_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct DiscordArgs {
    /// Public node IP. Defaults to setup state, then a best-effort lookup.
    #[arg(long)]
    public_ip: Option<String>,
    /// Stake bond outpoint, if already created.
    #[arg(long)]
    validator_bond: Option<String>,
    /// Validator ID, if already known.
    #[arg(long)]
    validator_id: Option<String>,
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
    /// Stop the setup page after this many minutes.
    #[arg(long, default_value_t = 60)]
    ttl_minutes: u64,
    /// Force overwrite of an existing differing node unit when pressing Install.
    #[arg(long)]
    force: bool,
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SetupState {
    network_id: Option<String>,
    public_ip: Option<String>,
    node: StateNode,
    validator: StateValidator,
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
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StateValidator {
    bond_outpoint: Option<String>,
    validator_id: Option<String>,
    funding_address: Option<String>,
    key: Option<String>,
    signed_epoch_db: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StateDiscord {
    registered: bool,
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

fn run_capture<I, S>(program: &str, args: I) -> SetupResult<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args_vec: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string_lossy().into_owned()).collect();
    let output = Command::new(program)
        .args(&args_vec)
        .output()
        .map_err(|e| CliError::new(exit::GENERIC, format!("run {program} {}: {e}", args_vec.join(" "))))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(text)
    } else {
        Err(CliError::new(exit::GENERIC, format!("{program} {} exited with {}\n{text}", args_vec.join(" "), output.status)))
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
    for bin in ["kaspad", "kaspa-pq-validator"] {
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
    let public_ip = args.public_ip.clone().or_else(detect_public_ip);
    let borsh_endpoint = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let kaspad_args = build_kaspad_args(ctx, args, p2p_port, public_ip.as_deref())?;
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
        },
        ..load_state(&args.state_file)
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
    let validator_service = service_state(&args.validator_service);

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
        "validator": { "service": &args.validator_service, "serviceState": &validator_service },
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
    let validator_service = service_state(&args.validator_service);

    println!("MISAKA setup status");
    println!();
    println!("Network:   {}", ctx.network);
    println!("Public IP: {}", public_ip.as_deref().unwrap_or("unknown"));
    println!("Node:      {}", status_label(&node_service));
    println!("Sync:      {}", if snapshot.reachable { if snapshot.synced { "SYNCED" } else { "SYNCING" } } else { "UNREACHABLE" });
    println!("P2P:       {}/tcp {}", p2p_port, if p2p { "LISTENING" } else { "NOT LISTENING" });
    println!("UTXO:      {}", snapshot.utxoindex.map(|v| if v { "ENABLED" } else { "DISABLED" }).unwrap_or("UNKNOWN"));
    println!("Seeder:    {}", status_label(&seeder_service));
    println!("Validator: {}", status_label(&validator_service));
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

fn discord_command(ip: &str, bond: Option<&str>, validator_id: Option<&str>, wallet: Option<&str>) -> String {
    let mut parts = vec![format!("/misaka register ip:{ip}")];
    if let Some(bond) = bond {
        parts.push(format!("validator_bond:{bond}"));
    }
    if let Some(validator_id) = validator_id {
        parts.push(format!("validator_id:{validator_id}"));
    }
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
    let bond = args.validator_bond.clone().or(state.validator.bond_outpoint);
    let validator_id = args.validator_id.clone().or(state.validator.validator_id);
    let command = discord_command(&ip, bond.as_deref(), validator_id.as_deref(), args.wallet.as_deref());

    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "network": &ctx.network,
                "publicIp": &ip,
                "validatorBond": &bond,
                "validatorId": &validator_id,
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
        no_utxoindex: false,
    }
}

fn web_status_args(args: &WebArgs) -> StatusArgs {
    StatusArgs {
        node_service: args.service.clone(),
        seeder_service: DEFAULT_SEEDER_SERVICE.to_string(),
        validator_service: DEFAULT_VALIDATOR_SERVICE.to_string(),
        state_file: args.state_file.clone(),
    }
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
cargo build --release -p misaka-cli -p kaspa-pq-validator
echo

echo "== 5/5 install release binaries =="
install -o root -g root -m 0755 target/release/kaspad /usr/local/bin/kaspad
install -o root -g root -m 0755 target/release/misaka /usr/local/bin/misaka
install -o root -g root -m 0755 target/release/kaspa-pq-validator /usr/local/bin/kaspa-pq-validator

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
probe_binary "kaspa-pq-validator" /usr/local/bin/kaspa-pq-validator --help
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
    let release_bins = ["kaspad", "misaka", "kaspa-pq-validator"];
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

fn validator_dir() -> PathBuf {
    PathBuf::from(DEFAULT_VALIDATOR_DIR)
}

fn validator_key_path() -> PathBuf {
    PathBuf::from(DEFAULT_VALIDATOR_KEY)
}

fn validator_db_path() -> PathBuf {
    PathBuf::from(DEFAULT_VALIDATOR_DB)
}

fn validator_env_path() -> PathBuf {
    PathBuf::from(DEFAULT_VALIDATOR_ENV)
}

fn parse_prefixed_output(output: &str, key: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed.strip_prefix(key).map(|value| value.trim_start_matches(':').trim().to_string()).filter(|value| !value.is_empty())
    })
}

fn ensure_validator_dir(service_user: &str) -> SetupResult<()> {
    if root_uid() != Some(0) {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "validator setup must be run as root or through sudo"));
    }
    fs::create_dir_all(validator_dir())
        .map_err(|e| CliError::new(exit::GENERIC, format!("mkdir {}: {e}", validator_dir().display())))?;
    let user_group = format!("{service_user}:{service_user}");
    run_checked("chown", vec!["-R".to_string(), user_group, validator_dir().display().to_string()])?;
    run_checked("chmod", vec!["0700".to_string(), validator_dir().display().to_string()])?;
    Ok(())
}

async fn validator_status_value_async(ctx: &Ctx, args: &WebArgs) -> serde_json::Value {
    let state = load_state(&args.state_file);
    let key_path = state.validator.key.clone().unwrap_or_else(|| DEFAULT_VALIDATOR_KEY.to_string());
    let db_path = state.validator.signed_epoch_db.clone().unwrap_or_else(|| DEFAULT_VALIDATOR_DB.to_string());
    let binary = binary_available("kaspa-pq-validator");
    let service = service_state(DEFAULT_VALIDATOR_SERVICE);
    let node_snapshot = node_snapshot(ctx).await;
    serde_json::json!({
        "ok": binary.is_some() && Path::new(&key_path).is_file() && state.validator.bond_outpoint.is_some(),
        "validator": {
            "service": DEFAULT_VALIDATOR_SERVICE,
            "serviceState": service,
            "binary": binary.map(|p| p.display().to_string()),
            "keyPath": key_path,
            "keyExists": Path::new(&state.validator.key.clone().unwrap_or_else(|| DEFAULT_VALIDATOR_KEY.to_string())).is_file(),
            "signedEpochDb": db_path,
            "validatorId": state.validator.validator_id,
            "fundingAddress": state.validator.funding_address,
            "bondOutpoint": state.validator.bond_outpoint,
            "nodeSynced": node_snapshot.synced,
            "nodeReachable": node_snapshot.reachable,
        }
    })
}

fn validator_keygen(ctx: &Ctx, args: &WebArgs) -> SetupResult<serde_json::Value> {
    if binary_available("kaspa-pq-validator").is_none() {
        return Err(CliError::new(exit::GENERIC, "kaspa-pq-validator is not installed; press Prepare VPS first"));
    }
    if !user_exists(&args.service_user) {
        return Err(CliError::new(exit::GENERIC, "service user is missing; press Install / Start Node first"));
    }
    ensure_validator_dir(&args.service_user)?;
    let key_path = validator_key_path();
    if key_path.exists() {
        return Err(CliError::new(exit::UNSAFE_REFUSED, format!("validator key already exists at {}", key_path.display())));
    }
    let output = run_capture(
        "kaspa-pq-validator",
        vec!["keygen".to_string(), "--out".to_string(), key_path.display().to_string(), "--network".to_string(), ctx.network.clone()],
    )?;
    run_checked("chown", vec![format!("{}:{}", args.service_user, args.service_user), key_path.display().to_string()])?;
    run_checked("chmod", vec!["0600".to_string(), key_path.display().to_string()])?;
    let validator_id = parse_prefixed_output(&output, "validator_id");
    let funding_address = parse_prefixed_output(&output, "funding_address");
    let mut state = load_state(&args.state_file);
    state.validator.key = Some(key_path.display().to_string());
    state.validator.signed_epoch_db = Some(validator_db_path().display().to_string());
    state.validator.validator_id = validator_id.clone();
    state.validator.funding_address = funding_address.clone();
    write_state(&args.state_file, &state)?;
    Ok(serde_json::json!({
        "ok": true,
        "message": "Validator key created. Send testnet MSK to the funding address, then press Check Funding.",
        "validator": {
            "keyPath": key_path.display().to_string(),
            "keyExists": true,
            "validatorId": validator_id,
            "fundingAddress": funding_address,
        },
        "output": output,
    }))
}

fn validator_balance(ctx: &Ctx, args: &WebArgs) -> SetupResult<serde_json::Value> {
    let state = load_state(&args.state_file);
    let address = state
        .validator
        .funding_address
        .clone()
        .ok_or_else(|| CliError::new(exit::GENERIC, "funding address is unknown; generate validator key first"))?;
    let borsh = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let output = run_capture(
        "kaspa-pq-validator",
        vec![
            "balance".to_string(),
            "--node-wrpc-borsh".to_string(),
            borsh,
            "--network".to_string(),
            ctx.network.clone(),
            "--address".to_string(),
            address.clone(),
        ],
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "message": "Funding balance checked.",
        "validator": {
            "fundingAddress": address,
            "balanceOutput": output,
        },
        "logs": output,
    }))
}

fn validator_bond(ctx: &Ctx, args: &WebArgs, amount: &str) -> SetupResult<serde_json::Value> {
    if amount.trim().is_empty() {
        return Err(CliError::new(exit::GENERIC, "amount is required, e.g. 10MSK"));
    }
    let key_path = validator_key_path();
    if !key_path.is_file() {
        return Err(CliError::new(exit::GENERIC, "validator key is missing; generate validator key first"));
    }
    let borsh = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let output = run_capture(
        "kaspa-pq-validator",
        vec![
            "bond".to_string(),
            "--node-wrpc-borsh".to_string(),
            borsh,
            "--validator-key".to_string(),
            key_path.display().to_string(),
            "--amount".to_string(),
            amount.trim().to_string(),
            "--network".to_string(),
            ctx.network.clone(),
        ],
    )?;
    let bond = parse_prefixed_output(&output, "bond_outpoint");
    let mut state = load_state(&args.state_file);
    state.validator.bond_outpoint = bond.clone();
    state.validator.key = Some(key_path.display().to_string());
    state.validator.signed_epoch_db = Some(validator_db_path().display().to_string());
    write_state(&args.state_file, &state)?;
    Ok(serde_json::json!({
        "ok": bond.is_some(),
        "message": if bond.is_some() { "Bond transaction submitted. Press Validator Status, then Start Validator when active." } else { "Bond command finished, but bond_outpoint was not found in output." },
        "validator": {
            "bondOutpoint": bond,
            "amount": amount,
        },
        "logs": output,
    }))
}

fn validator_chain_status(ctx: &Ctx, args: &WebArgs) -> SetupResult<serde_json::Value> {
    let state = load_state(&args.state_file);
    let bond = state
        .validator
        .bond_outpoint
        .clone()
        .ok_or_else(|| CliError::new(exit::GENERIC, "bond outpoint is unknown; create bond first"))?;
    let borsh = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let output = run_capture(
        "kaspa-pq-validator",
        vec![
            "status".to_string(),
            "--node-wrpc-borsh".to_string(),
            borsh,
            "--network".to_string(),
            ctx.network.clone(),
            "--stake-bond".to_string(),
            bond.clone(),
        ],
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "message": "Validator bond status checked.",
        "validator": {
            "bondOutpoint": bond,
            "statusOutput": output,
        },
        "logs": output,
    }))
}

fn render_validator_unit(service_user: &str, network: &str, borsh: &str) -> String {
    format!(
        "[Unit]\n\
Description=MISAKA validator sidecar\n\
After=misaka-kaspad.service\n\
Requires=misaka-kaspad.service\n\n\
[Service]\n\
User={service_user}\n\
Group={service_user}\n\
EnvironmentFile=/etc/misaka/validator.env\n\
ExecStart=/usr/local/bin/kaspa-pq-validator run \\\n  --node-wrpc-borsh {borsh} \\\n  --validator-key {DEFAULT_VALIDATOR_KEY} \\\n  --stake-bond ${{STAKE_BOND}} \\\n  --signed-epoch-db {DEFAULT_VALIDATOR_DB} \\\n  --network {network}\n\
Restart=always\n\
RestartSec=10\n\
LimitNOFILE=1048576\n\n\
[Install]\n\
WantedBy=multi-user.target\n"
    )
}

fn validator_service_install(ctx: &Ctx, args: &WebArgs) -> SetupResult<serde_json::Value> {
    if root_uid() != Some(0) {
        return Err(CliError::new(exit::UNSAFE_REFUSED, "validator service install must be run as root or through sudo"));
    }
    let state = load_state(&args.state_file);
    let bond = state
        .validator
        .bond_outpoint
        .clone()
        .ok_or_else(|| CliError::new(exit::GENERIC, "bond outpoint is unknown; create bond first"))?;
    if !validator_key_path().is_file() {
        return Err(CliError::new(exit::GENERIC, "validator key is missing; generate validator key first"));
    }
    ensure_validator_dir(&args.service_user)?;
    let env_path = validator_env_path();
    let env = format!("STAKE_BOND={bond}\n");
    write_if_changed(&env_path, &env, args.force)?;
    run_checked("chmod", vec!["0600".to_string(), env_path.display().to_string()])?;
    let borsh = wrpc_borsh_endpoint(&ctx.network, &ctx.rpc)?;
    let unit = render_validator_unit(&args.service_user, &ctx.network, &borsh);
    let unit_path = service_unit_path(DEFAULT_VALIDATOR_SERVICE);
    write_if_changed(&unit_path, &unit, args.force)?;
    run_checked("systemctl", ["daemon-reload"])?;
    run_checked("systemctl", ["enable", "--now", DEFAULT_VALIDATOR_SERVICE])?;
    Ok(serde_json::json!({
        "ok": true,
        "message": "Validator service installed and started.",
        "validator": {
            "service": DEFAULT_VALIDATOR_SERVICE,
            "bondOutpoint": bond,
            "unitPath": unit_path.display().to_string(),
        }
    }))
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
    let expected = format!("token={token}");
    target.split_once('?').map(|(_, query)| query.split('&').any(|part| part == expected.as_str())).unwrap_or(false)
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

fn html_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn setup_html(token: &str) -> String {
    include_str!("setup_ui.html").replace("__SETUP_TOKEN__", &html_escape(token))
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
        "HTTP/1.1 {code} {}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
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
        ("POST", "/api/preflight") => match preflight_checks(ctx, &web_preflight_args(args)) {
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
        ("GET", "/api/validator/status") => json_response(validator_status_value_async(ctx, args).await),
        ("POST", "/api/validator/keygen") => match validator_keygen(ctx, args) {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("POST", "/api/validator/balance") => match validator_balance(ctx, args) {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("POST", "/api/validator/bond") => {
            let amount = query_param(&req.target, "amount").unwrap_or_else(|| "10MSK".to_string());
            match validator_bond(ctx, args, &amount) {
                Ok(value) => json_response(value),
                Err(e) => json_error(500, e.msg),
            }
        }
        ("GET", "/api/validator/chain-status") => match validator_chain_status(ctx, args) {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("POST", "/api/validator/service/apply") => match validator_service_install(ctx, args) {
            Ok(value) => json_response(value),
            Err(e) => json_error(500, e.msg),
        },
        ("GET", "/api/validator/logs") => {
            let logs = run_output("journalctl", ["-u", DEFAULT_VALIDATOR_SERVICE, "-n", "100", "--no-pager"])
                .unwrap_or_else(|| "No validator logs available, or journalctl is unavailable.".to_string());
            json_response(serde_json::json!({ "ok": true, "logs": logs }))
        }
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

async fn web(ctx: &Ctx, args: &WebArgs) -> CliResult {
    let bind_host = if args.public { "0.0.0.0" } else { "127.0.0.1" };
    let listener = TcpListener::bind((bind_host, args.port))
        .map_err(|e| CliError::new(exit::GENERIC, format!("bind {bind_host}:{}: {e}", args.port)))?;
    listener.set_nonblocking(true).map_err(|e| CliError::new(exit::GENERIC, format!("set nonblocking listener: {e}")))?;

    let token = args.token.clone().unwrap_or_else(random_token);
    let display_host = if args.public {
        args.public_ip.clone().or_else(detect_public_ip).unwrap_or_else(|| "<VPS_PUBLIC_IP>".to_string())
    } else {
        "127.0.0.1".to_string()
    };
    let ttl = Duration::from_secs(args.ttl_minutes.max(1).saturating_mul(60));
    let start = Instant::now();

    println!("MISAKA Setup is ready.");
    println!();
    println!("Open:");
    println!("  http://{display_host}:{}/setup?token={token}", args.port);
    println!();
    println!("This setup page expires in {} minute(s).", args.ttl_minutes.max(1));
    if root_uid() != Some(0) {
        println!("Note: Install / Start Node requires running this command with sudo/root.");
    }

    let mut stop = false;
    while !stop && start.elapsed() < ttl {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let Some(req) = read_http_request(&mut stream) else {
                    let body = serde_json::json!({ "ok": false, "error": "bad request" }).to_string();
                    write_http(&mut stream, 400, "application/json", &body);
                    continue;
                };
                let (code, content_type, body, should_stop) = web_route(ctx, args, &token, &req).await;
                write_http(&mut stream, code, content_type, &body);
                stop = should_stop;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(CliError::new(exit::GENERIC, format!("accept setup web connection: {e}"))),
        }
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

    #[test]
    fn network_flags_match_testnet_suffix() {
        assert_eq!(net_flags("testnet-10").unwrap(), vec!["--testnet".to_string(), "--netsuffix=10".to_string()]);
    }

    #[test]
    fn discord_command_omits_missing_optional_values() {
        assert_eq!(discord_command("203.0.113.10", None, None, None), "/misaka register ip:203.0.113.10");
        assert_eq!(
            discord_command("203.0.113.10", Some("abc:0"), Some("validator123"), None),
            "/misaka register ip:203.0.113.10 validator_bond:abc:0 validator_id:validator123"
        );
    }

    #[test]
    fn unit_uses_service_user_and_kaspad_args() {
        let unit = render_unit("misaka_user", &["--testnet".into(), "--netsuffix=10".into()]);
        assert!(unit.contains("User=misaka_user"));
        assert!(unit.contains("Group=misaka_user"));
        assert!(unit.contains("ExecStart=/usr/local/bin/kaspad --testnet --netsuffix=10"));
    }

    #[test]
    fn node_plan_defaults_to_validator_ready_utxoindex() {
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
            no_utxoindex: false,
        };
        let plan = build_node_plan(&base_ctx(), &args).unwrap();
        assert!(plan.unit.contains("--externalip=203.0.113.10:26211"));
        assert!(plan.unit.contains("--utxoindex"));
        assert_eq!(plan.state.node.service_user.as_deref(), Some("misaka_user"));
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
        assert!(!target_has_token("/api/status?token=wrong", "abc123"));
        assert!(!target_has_token("/api/status", "abc123"));
    }

    #[test]
    fn web_node_args_are_validator_ready_by_default() {
        let web = WebArgs {
            public: true,
            port: 8787,
            public_ip: Some("203.0.113.10".to_string()),
            token: None,
            ttl_minutes: 60,
            force: false,
            no_ufw: false,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            repo_dir: PathBuf::from(DEFAULT_REPO_DIR),
            repo_url: DEFAULT_REPO_URL.to_string(),
        };
        let args = web_node_args(&web, true, false);
        assert!(args.yes);
        assert!(!args.dry_run);
        assert!(!args.no_utxoindex);
        assert_eq!(args.public_ip.as_deref(), Some("203.0.113.10"));
    }

    #[test]
    fn prepare_script_builds_kaspad_with_evm_feature() {
        let script = prepare_script(&WebArgs {
            public: true,
            port: 8787,
            public_ip: Some("203.0.113.10".to_string()),
            token: None,
            ttl_minutes: 60,
            force: false,
            no_ufw: false,
            service_user: DEFAULT_SERVICE_USER.to_string(),
            appdir: PathBuf::from(DEFAULT_APPDIR),
            service: DEFAULT_KASPAD_SERVICE.to_string(),
            state_file: PathBuf::from(DEFAULT_STATE_FILE),
            repo_dir: PathBuf::from(DEFAULT_REPO_DIR),
            repo_url: DEFAULT_REPO_URL.to_string(),
        });
        assert!(script.contains("cargo build --release -p kaspad --features evm"));
        assert!(script.contains("cargo build --release -p misaka-cli -p kaspa-pq-validator"));
    }
}
