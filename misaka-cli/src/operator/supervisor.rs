//! **`misaka mining start | stop | run`** — ADR-0122 Decision 5: one command starts what
//! `mining.toml` describes, checks it the way the fleet's roll procedure does, keeps it running,
//! and stops it without abandoning a claim.
//!
//! * `start` runs the preflight (the doctor's identity, model and host checks), spawns `kaspad`
//!   with the flags the file implies, and waits through the readiness gates: the process, its boot
//!   fingerprint and fence heights against this build's, the wRPC port, a peer, the producer. With
//!   the prompt lane it then starts the gateway and the rail. In the foreground it hands over to a
//!   live status line; `--detach` runs the same supervisor in the background; `--service` prints a
//!   unit for a service manager instead; `--print-command` prints the exact command lines.
//! * `stop` refuses while this node's claims are still to be defended (exit 37), and offers
//!   `--drain` (stop drawing, keep serving, exit when the last claim ends) or `--force`. The order
//!   is the rail, the gateway, then `kaspad` with one SIGTERM and a grace (240 s by default,
//!   because a Qwen3.6 node takes three to four minutes to stop) before SIGKILL.
//! * `run` is the supervisor itself, for a service manager's `ExecStart`.
//!
//! `kaspad` is found beside this `misaka` or at `[advanced] kaspad`, never on `$PATH`: a
//! forwarder that resolved a bare name ran whatever a writable `PATH` entry held (ADR-0063 SA-4).

use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::profile::Profile;
use crate::operator::snapshot::Snapshot;
use crate::operator::work::{Lane, Outcome, WorkState};
use crate::operator::{doctor, procs, status};
use crate::{CliError, CliResult, OutputFormat, exit};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// One command a supervisor runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cmd {
    pub(crate) name: &'static str,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl Cmd {
    /// The command as one line a shell (and a unit file) takes.
    pub(crate) fn shell_line(&self) -> String {
        let mut parts: Vec<String> = self.env.iter().map(|(k, v)| format!("{k}={}", quote(v))).collect();
        parts.push(quote(&self.program.display().to_string()));
        parts.extend(self.args.iter().map(|a| quote(a)));
        parts.join(" ")
    }
}

fn quote(s: &str) -> String {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./:=,@%+".contains(&b)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Everything `start` runs, and where it keeps its state.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub(crate) network: String,
    pub(crate) kaspad: Cmd,
    /// The prompt lane's identity file, generated once when it is missing, and the command that
    /// generates it.
    pub(crate) identity: Option<(PathBuf, Cmd)>,
    pub(crate) gateway: Option<Cmd>,
    pub(crate) rail: Option<Cmd>,
    pub(crate) run_dir: PathBuf,
    pub(crate) grace: Duration,
}

/// `~/.misaka/<network>/run`: the supervisor's pid, its state and the children's output.
pub(crate) fn run_dir(network: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".misaka").join(network).join("run")
}

/// `testnet-11` → `--testnet --netsuffix=11`, as kaspad selects networks.
pub(crate) fn network_flags(network: &str) -> Result<Vec<String>, Finding> {
    match network {
        "mainnet" => Ok(Vec::new()),
        "devnet" => Ok(vec!["--devnet".to_string()]),
        "simnet" => Ok(vec!["--simnet".to_string()]),
        n => match n.strip_prefix("testnet-").filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())) {
            Some(suffix) => Ok(vec!["--testnet".to_string(), format!("--netsuffix={suffix}")]),
            None => Err(Finding::error("E-CONFIG-NETWORK", exit::CONFIG, format!("'{n}' is not a network kaspad can start"))
                .required("mainnet, devnet, simnet or testnet-<N>")
                .fix("set [mining] network in ~/.misaka/mining.toml")),
        },
    }
}

fn missing(what: &str, fix: &str) -> Finding {
    Finding::error("E-CONFIG-INCOMPLETE", exit::CONFIG, format!("mining.toml does not say {what}"))
        .reason("misaka mining start runs exactly what ~/.misaka/mining.toml describes")
        .fix(fix.to_string())
        .docs("docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md#7-d6--one-configuration-file-purpose-first")
}

/// **The `kaspad` flags a profile implies** — the subset of the fleet's flags a single operator's
/// node needs (ADR-0122 §7). Never `--enable-unsynced-mining` (a fresh chain's first block only),
/// `--unsaferpc`, `--yes` (kaspad's prompts delete databases; unanswered, they refuse and exit), or
/// any drill or devnet ruleset flag; `[advanced] extra_kaspad_args` is where an operator adds one.
pub(crate) fn kaspad_args(p: &Profile) -> Result<Vec<String>, Finding> {
    let file = p.config.clone().unwrap_or_default();
    let mut args = network_flags(&p.network)?;
    args.push(format!("--appdir={}", p.appdir.display()));
    if let Some(listen) = &file.advanced.listen {
        args.push(format!("--listen={listen}"));
    }
    args.push(format!("--rpclisten-borsh={}", file.advanced.rpc_borsh.clone().unwrap_or_else(|| "default".to_string())));
    // The wallet, the rewards screen and the prompt lane's rail all read the UTXO index.
    args.push("--utxoindex".to_string());
    for peer in &file.advanced.peers {
        args.push(format!("--addpeer={peer}"));
    }
    let key =
        p.key_path.as_ref().ok_or_else(|| missing("which key the node mines with", "set [mining] key = \"~/.misaka/miner.seed\""))?;
    let bond = p.bond.as_ref().ok_or_else(|| {
        missing("which bond the node mines under", "set [advanced] bond = \"<txid>:<index>\", or register one: misaka mining setup")
    })?;
    args.push("--palw-produce".to_string());
    args.push("--palw-panel".to_string());
    args.push(format!("--palw-producer-key={}", key.display()));
    args.push(format!("--palw-producer-bond={bond}"));
    if let Some(pay) = &p.pay_address {
        args.push(format!("--palw-producer-pay-address={pay}"));
    }
    if let Some(fee) = &p.fee_outpoint {
        args.push(format!("--palw-fee-outpoint={fee}"));
    } else if !p.persisted_fee_outpoint().exists() {
        // kaspad panics without one on a ConsensusV2 network: say it here, as a fix.
        return Err(missing(
            "which UTXO funds the panel's carriers",
            "set [advanced] fee_outpoint = \"<txid>:<index>\" (a mature, unbonded UTXO at the key's address, ≥ 0.1 MSK)",
        ));
    }
    if let Some(class) = &p.class {
        args.push(format!("--palw-producer-class={class}"));
    }
    for artifact in &p.artifacts {
        args.push(format!("--palw-class-artifact={}", artifact.display()));
    }
    if let Some(bytes) = file.advanced.resident_bytes.as_deref().filter(|b| *b != "auto") {
        args.push(format!("--palw-class-resident-bytes={bytes}"));
    }
    if file.advanced.challenge == Some(true) {
        args.push("--palw-challenge".to_string());
    }
    args.extend(file.advanced.extra_kaspad_args.iter().cloned());
    Ok(args)
}

/// A helper binary: the configured path, else beside this `misaka`. Never a bare name on `$PATH`.
fn binary(name: &str, configured: Option<&Path>) -> Result<PathBuf, Finding> {
    if let Some(path) = configured {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(Finding::error("E-CONFIG-BINARY", exit::CONFIG, format!("{} does not exist", path.display()))
                .fix(format!("point the setting at the {name} you built (cargo build --release -p …)")))
        };
    }
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join(name)));
    match beside {
        Some(path) if path.is_file() => Ok(path),
        _ => Err(Finding::error("E-CONFIG-BINARY", exit::CONFIG, format!("no {name} beside this misaka"))
            .reason("the supervisor runs the binaries built with this CLI, and never a name found on $PATH")
            .current(format!(
                "looked in {}",
                std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.display().to_string())).unwrap_or_default()
            ))
            .fix("build it into the same target directory (cargo build --release -p <its crate>), or set its path in mining.toml")),
    }
}

/// **Everything `start` would run**, resolved and checked, without running anything.
pub(crate) fn plan(p: &Profile) -> Result<Plan, Finding> {
    let file = p.config.clone().unwrap_or_default();
    let kaspad = Cmd {
        name: "kaspad",
        program: binary("kaspad", file.advanced.kaspad.as_deref().map(|k| PathBuf::from(procs::expand_home(k))).as_deref())?,
        args: kaspad_args(p)?,
        env: Vec::new(),
    };
    let run_dir = run_dir(&p.network);
    let (identity, gateway, rail) = match &p.prompt {
        None => (None, None, None),
        Some(lane) => {
            let key = p.key_path.as_ref().expect("kaspad_args required the key");
            let net_home = dirs::home_dir().unwrap_or_default().join(".misaka").join(&p.network);
            let outbox = lane.outbox.clone().unwrap_or_else(|| net_home.join("fp-outbox"));
            // Kept apart from the seed and from the outbox: the gateway refuses to start if it can
            // reach a signing secret from its identity's directory or its outbox.
            let identity = lane.identity.clone().unwrap_or_else(|| net_home.join("fp-identity").join("identity.json"));
            let rpc = p.rpc.clone().unwrap_or_else(|| default_borsh(&p.network));
            let class = p.class.clone().ok_or_else(|| {
                missing(
                    "which class the prompt lane serves",
                    "set [mining] model to the class's 128-hex id (the prompt lane needs a certified class)",
                )
            })?;
            let gateway_bin = binary("misaka-palw-gateway", lane.gateway_bin.as_deref())?;
            let rail_bin = binary("misaka-palw-fp-rail", lane.rail_bin.as_deref())?;
            let worker = lane
                .worker
                .clone()
                .ok_or_else(|| missing("where the worker is", "set [advanced.prompt] worker = \"/abs/palw-a16-fp-worker\""))?;
            if !worker.is_absolute() {
                return Err(Finding::error("E-PROMPT-WORKER-PATH", exit::CONFIG, "The worker path is not absolute")
                    .reason("the gateway spawns the worker from its own working directory; a relative path finds nothing")
                    .current(worker.display().to_string())
                    .fix("give the worker's absolute path"));
            }
            let artifact = lane
                .artifact
                .clone()
                .ok_or_else(|| missing("which bound artifact the worker serves", "set [advanced.prompt] artifact"))?;
            let tokenizer = lane
                .tokenizer
                .clone()
                .ok_or_else(|| missing("which tokenizer the worker encodes with", "set [advanced.prompt] tokenizer"))?;
            let identity_cmd = Cmd {
                name: "identity",
                program: rail_bin.clone(),
                args: vec![
                    "--print-identity".into(),
                    "--bond-key-seed".into(),
                    key.display().to_string(),
                    "--rpc".into(),
                    rpc.clone(),
                    "--class-id".into(),
                    class,
                ],
                env: Vec::new(),
            };
            let gateway = Cmd {
                name: "gateway",
                program: gateway_bin,
                args: vec![
                    "--listen".into(),
                    lane.gateway_listen.clone(),
                    "--worker".into(),
                    worker.display().to_string(),
                    "--outbox".into(),
                    outbox.display().to_string(),
                    "--identity".into(),
                    identity.display().to_string(),
                    "--rpc".into(),
                    rpc.clone(),
                ],
                env: vec![
                    ("MISAKA_PALW_NETWORK_ID".into(), p.network.clone()),
                    ("MISAKA_PALW_ARTIFACT".into(), artifact.display().to_string()),
                    ("MISAKA_PALW_TOKENIZER".into(), tokenizer.display().to_string()),
                ],
            };
            let rail = Cmd {
                name: "rail",
                program: rail_bin,
                args: vec![
                    "--watch".into(),
                    outbox.display().to_string(),
                    "--bond-key-seed".into(),
                    key.display().to_string(),
                    "--rpc".into(),
                    rpc,
                ],
                env: Vec::new(),
            };
            (Some((identity, identity_cmd)), Some(gateway), Some(rail))
        }
    };
    Ok(Plan { network: p.network.clone(), kaspad, identity, gateway, rail, run_dir, grace: Duration::from_secs(p.stop_grace_secs) })
}

fn default_borsh(network: &str) -> String {
    match network.parse::<kaspa_consensus_core::network::NetworkId>() {
        Ok(net) => format!("127.0.0.1:{}", net.default_endpoint_port(kaspa_consensus_core::network::EndpointKind::NodeWrpcBorsh)),
        Err(_) => "127.0.0.1:27210".to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// the supervisor's files
// ---------------------------------------------------------------------------------------------

/// What the supervisor writes about itself, so `stop`, `status` and a second `start` can read it.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) supervisor_pid: u32,
    /// `starting`, `mining`, `draining`, `stopping`, `stopped`, `failed`.
    pub(crate) phase: String,
    pub(crate) kaspad_pid: Option<u32>,
    pub(crate) gateway_pid: Option<u32>,
    pub(crate) rail_pid: Option<u32>,
    pub(crate) restarts: u32,
    pub(crate) message: String,
    pub(crate) updated_unix: u64,
}

impl State {
    fn path(run_dir: &Path) -> PathBuf {
        run_dir.join("state.json")
    }

    pub(crate) fn read(run_dir: &Path) -> Option<State> {
        serde_json::from_slice(&std::fs::read(Self::path(run_dir)).ok()?).ok()
    }

    fn write(&mut self, run_dir: &Path) {
        self.updated_unix = procs::now_unix();
        let tmp = run_dir.join("state.json.partial");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, Self::path(run_dir));
        }
    }
}

pub(crate) fn alive(pid: u32) -> bool {
    // Signal 0 checks for existence and permission without delivering anything.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn signal(pid: u32, sig: libc::c_int) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, sig) == 0 }
}

/// The supervisor for this network, if one runs.
pub(crate) fn running_supervisor(network: &str) -> Option<State> {
    let state = State::read(&run_dir(network))?;
    (state.supervisor_pid != 0 && alive(state.supervisor_pid) && !matches!(state.phase.as_str(), "stopped" | "failed"))
        .then_some(state)
}

// ---------------------------------------------------------------------------------------------
// the claims a stop would abandon
// ---------------------------------------------------------------------------------------------

/// A claim this node still has to defend, and what stopping costs it.
#[derive(Clone, Debug)]
pub(crate) struct Owed {
    pub(crate) id: String,
    pub(crate) lane: Lane,
    pub(crate) state: WorkState,
    pub(crate) cost: String,
    pub(crate) ends_daa: Option<u64>,
}

/// **The claims a stop would abandon**: this node's works on the chain and not yet final or
/// voided. What stopping costs each one is said from the lane and the network's rules: a prompt
/// claim's seats need openings only the executor holds, and — where the data-availability court is
/// in force — any claim short of final can be accused, and an unanswered accusation slashes.
pub(crate) fn owed(snap: &Snapshot) -> Vec<Owed> {
    let da_court = snap.node.as_ref().ok().is_some_and(|n| n.nv.params.palw_da_court.is_some_and(|f| f.is_active(n.daa())));
    snap.works
        .iter()
        .filter(|w| matches!(w.reading.state.outcome(), Outcome::InFlight | Outcome::Paused))
        .filter(|w| {
            matches!(w.reading.state, WorkState::OnChain | WorkState::WaitingReceipts | WorkState::QuorumReached | WorkState::Disputed)
        })
        .map(|w| {
            let cost = match (w.lane, w.reading.state, da_court) {
                (_, WorkState::Disputed, _) => {
                    "an accusation is open: no disclosure in time voids it producer_withholding and slashes it".to_string()
                }
                (Lane::Prompt, WorkState::OnChain | WorkState::WaitingReceipts, _) => {
                    "its seats ask this node for openings no one else holds: down, it voids receipt_timeout".to_string()
                }
                (_, _, true) => {
                    "an accusation this node cannot answer voids it producer_withholding and slashes its collateral".to_string()
                }
                (Lane::Block, _, false) => {
                    "its seats replay the block's job; a court about it asks this node until it is final".to_string()
                }
                (Lane::Prompt, _, false) => "a court about it asks this node until it is final".to_string(),
            };
            Owed { id: w.display_id(), lane: w.lane, state: w.reading.state, cost, ends_daa: w.reading.deadline_daa }
        })
        .collect()
}

fn stop_refusal(owed: &[Owed]) -> Finding {
    let mut f = Finding::error(
        "E-STOP-INFLIGHT",
        exit::STOP_INFLIGHT,
        format!("Stopping now would abandon {} claim{} only this node can defend", owed.len(), if owed.len() == 1 { "" } else { "s" }),
    )
    .reason("a claim is defended by the node that made it until the chain is done with it — final, or voided");
    for o in owed {
        let ends = o.ends_daa.map(|d| format!(" (next date DAA {})", status::group(d))).unwrap_or_default();
        f = f.current(format!("{:<10} {:<6} {:<17} {}{ends}", o.id, o.lane.name(), o.state.name(), o.cost));
    }
    f.required("this node serves its claims until each is final or voided")
        .fix("misaka mining stop --drain   stop drawing now, keep serving, exit when the last one ends")
        .fix("misaka mining stop --force   stop now and accept what the list above costs")
        .docs("docs/testnet11-join-mining.md#6b-do-not-stop-your-node-with-claims-in-flight")
}

// ---------------------------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------------------------

/// What `start` was asked to do.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StartMode {
    pub(crate) detach: bool,
    pub(crate) print_command: bool,
    pub(crate) service: bool,
}

pub(crate) async fn start(
    ctx: &crate::node::Ctx,
    profile: Profile,
    mode: StartMode,
    reprofile: &dyn Fn() -> Result<Profile, CliError>,
    relaunch: &[String],
) -> CliResult {
    if profile.config.is_none() {
        let f = Finding::error("E-CONFIG-NONE", exit::CONFIG, "There is no mining configuration to start")
            .reason("misaka mining start runs exactly what ~/.misaka/mining.toml describes, and there is none")
            .fix("misaka mining setup")
            .fix("or write ~/.misaka/mining.toml by hand (ADR-0122 §7 has the schema)")
            .docs("docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md");
        return fail(ctx, f);
    }
    let plan = match plan(&profile) {
        Ok(plan) => plan,
        Err(f) => return fail(ctx, f),
    };
    if mode.print_command {
        println!("# kaspad");
        println!("{}", plan.kaspad.shell_line());
        if let Some((path, cmd)) = &plan.identity {
            println!("# the gateway's identity, once (to {})", path.display());
            println!("{} > {}", cmd.shell_line(), quote(&path.display().to_string()));
        }
        for cmd in [&plan.gateway, &plan.rail].into_iter().flatten() {
            println!("# {}", cmd.name);
            println!("{}", cmd.shell_line());
        }
        return Ok(());
    }
    if mode.service {
        print!("{}", service_text(&plan, relaunch));
        return Ok(());
    }
    if let Some(state) = running_supervisor(&plan.network) {
        println!("already running: supervisor pid {} ({}), kaspad pid {}", state.supervisor_pid, state.phase, opt(state.kaspad_pid));
        println!("  misaka mining status   ·   misaka mining stop");
        return Ok(());
    }
    if let Some((proc_, _)) = &profile.kaspad {
        println!("a kaspad for {} already runs here (pid {}), not under this supervisor", plan.network, proc_.pid);
        println!("  misaka mining status reads it as it is; stop it first to run it from mining.toml");
        return Ok(());
    }
    // The preflight: what would make kaspad refuse, or run a miner that cannot earn.
    let snap = Snapshot::gather(profile.clone(), Duration::from_secs(3), false).await;
    let rows = doctor::checks(&snap, &[doctor::Area::Identity, doctor::Area::Model, doctor::Area::Host]).await;
    let errors: Vec<&Finding> = rows
        .iter()
        .filter_map(|c| c.finding.as_ref())
        .filter(|f| f.severity == Severity::Error)
        // Needs the node, which is not running yet: said by the node's own gates below instead.
        .filter(|f| !matches!(f.code, "E-PROC-NODE-DOWN" | "E-NODE-RPC-UNREACHABLE"))
        .collect();
    let warnings = rows.iter().filter(|c| c.severity == Severity::Warning).count();
    step(
        Severity::Ok,
        "config",
        &format!(
            "{} · {} · prompt lane {}",
            profile.config_path.as_ref().map(|p| crate::operator::host::tilde(p)).unwrap_or_default(),
            plan.network,
            if plan.gateway.is_some() { "on" } else { "off" }
        ),
    );
    if let Some(first) = errors.first() {
        step(Severity::Error, "preflight", &format!("{} failure(s)", errors.len()));
        println!();
        return fail(ctx, (*first).clone());
    }
    step(
        Severity::Ok,
        "preflight",
        &format!(
            "{} ok{}",
            rows.iter().filter(|c| c.severity == Severity::Ok).count(),
            if warnings > 0 { format!(" · {warnings} warning(s): misaka doctor") } else { String::new() }
        ),
    );

    if mode.detach {
        return detach(&plan, relaunch).await;
    }
    run_supervisor(plan, true, reprofile).await
}

fn opt(v: Option<u32>) -> String {
    v.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
}

fn fail(ctx: &crate::node::Ctx, f: Finding) -> CliResult {
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "ok": false, "finding": f })).expect("serializable"));
    } else {
        println!("{}", f.render());
    }
    Err(CliError::new(f.exit, String::new()))
}

fn step(sev: Severity, name: &str, value: &str) {
    println!(
        "  {} {:<11} {value}",
        sev.paint(match sev {
            Severity::Info => "◐",
            other => other.mark(),
        }),
        name
    );
}

/// The unit a service manager runs: `misaka mining run`, restarted on failure, with a stop timeout
/// longer than the supervisor's own grace so the manager never SIGKILLs a node mid-shutdown.
fn service_text(plan: &Plan, relaunch: &[String]) -> String {
    let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_else(|_| "misaka".into());
    let mut argv = vec![exe.clone()];
    argv.extend(relaunch.iter().cloned());
    let timeout = plan.grace.as_secs() + 60;
    if cfg!(target_os = "macos") {
        let label = format!("com.misaka.mining.{}", plan.network);
        let args: String = argv.iter().map(|a| format!("    <string>{}</string>\n", xml(a))).collect();
        let log = plan.run_dir.join("supervisor.log").display().to_string();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{label}</string>\n  \
             <key>ProgramArguments</key>\n  <array>\n{args}  </array>\n  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n  \
             <key>ExitTimeOut</key><integer>{timeout}</integer>\n  <key>StandardOutPath</key><string>{log}</string>\n  <key>StandardErrorPath</key><string>{log}</string>\n\
             </dict>\n</plist>\n\n<!-- Install (this prints it; nothing was installed):\n  misaka mining start --service > ~/Library/LaunchAgents/{label}.plist\n  \
             launchctl load ~/Library/LaunchAgents/{label}.plist\n-->\n"
        )
    } else {
        let line = argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
        format!(
            "[Unit]\nDescription=MISAKA mining on {net} (misaka mining run)\nAfter=network-online.target\nWants=network-online.target\n\n\
             [Service]\nExecStart={line}\nRestart=on-failure\nRestartSec=30\n# The supervisor stops its children in order and waits the stop grace ({grace} s) for kaspad;\n\
             # the manager must wait longer, or it SIGKILLs a node mid-shutdown.\nTimeoutStopSec={timeout}\nKillMode=mixed\nKillSignal=SIGTERM\n\
             # Bound it: an OOM kill takes the node's claims with it.\n#MemoryMax=\n\n[Install]\nWantedBy=default.target\n\n\
             # Install (this prints it; nothing was installed):\n#   misaka mining start --service > ~/.config/systemd/user/misaka-mining-{net}.service\n\
             #   systemctl --user daemon-reload && systemctl --user enable --now misaka-mining-{net}\n",
            net = plan.network,
            grace = plan.grace.as_secs(),
        )
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Run the same supervisor in the background (`relaunch` is `misaka`'s own arguments for
/// `mining run`), in a session of its own, and wait until it says it is up or failed.
async fn detach(plan: &Plan, relaunch: &[String]) -> CliResult {
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(&plan.run_dir).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", plan.run_dir.display())))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(plan.run_dir.join("supervisor.log"))
        .map_err(|e| CliError::new(exit::HOST, format!("supervisor.log: {e}")))?;
    let exe = std::env::current_exe().map_err(|e| CliError::new(exit::GENERIC, format!("current_exe: {e}")))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(relaunch);
    cmd.stdin(std::process::Stdio::null()).stdout(log.try_clone().map_err(|e| CliError::generic(e.to_string()))?).stderr(log);
    // A session of its own: closing this terminal must not hang the supervisor up.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| CliError::new(exit::COMPONENT_DOWN, format!("could not start the supervisor: {e}")))?;
    let pid = child.id();
    step(
        Severity::Ok,
        "supervisor",
        &format!("pid {pid} (in the background) · log {}", crate::operator::host::tilde(&plan.run_dir.join("supervisor.log"))),
    );
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut last = String::new();
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Some(state) = State::read(&plan.run_dir).filter(|s| s.supervisor_pid == pid) {
            if state.message != last {
                step(Severity::Info, &state.phase, &state.message);
                last = state.message.clone();
            }
            match state.phase.as_str() {
                "mining" => {
                    println!("{}", paint::green("● mining, in the background.  misaka mining status · misaka mining stop"));
                    return Ok(());
                }
                "failed" | "stopped" => {
                    return Err(CliError::new(exit::COMPONENT_DOWN, format!("the supervisor {}: {}", state.phase, state.message)));
                }
                _ => {}
            }
        }
        if !alive(pid) {
            return Err(CliError::new(
                exit::COMPONENT_DOWN,
                format!("the supervisor exited: see {}", plan.run_dir.join("supervisor.log").display()),
            ));
        }
    }
    println!("still starting after 10 minutes — the supervisor keeps going; misaka mining status shows where");
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// the supervisor
// ---------------------------------------------------------------------------------------------

struct Child {
    cmd: Cmd,
    child: std::process::Child,
    started: Instant,
}

fn spawn(cmd: &Cmd, run_dir: &Path) -> Result<Child, String> {
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join(format!("{}.out", cmd.name)))
        .map_err(|e| format!("{}.out: {e}", cmd.name))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let child = std::process::Command::new(&cmd.program)
        .args(&cmd.args)
        .envs(cmd.env.iter().cloned())
        // No stdin: a kaspad prompt (a database from another version) reads EOF, refuses and exits
        // — it never deletes anything on a supervisor's behalf.
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("{}: {e}", cmd.program.display()))?;
    Ok(Child { cmd: cmd.clone(), child, started: Instant::now() })
}

/// SIGTERM, then up to `grace` for the process to exit, then SIGKILL. Says which happened.
async fn stop_child(c: &mut Child, grace: Duration) -> String {
    let pid = c.child.id();
    if !matches!(c.child.try_wait(), Ok(None)) {
        return format!("{} had already exited", c.cmd.name);
    }
    signal(pid, libc::SIGTERM);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !matches!(c.child.try_wait(), Ok(None)) {
            return format!("{} stopped in {} s", c.cmd.name, (grace - (deadline - Instant::now())).as_secs());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    signal(pid, libc::SIGKILL);
    let _ = c.child.wait();
    format!("{} did not stop within {} s and was killed", c.cmd.name, grace.as_secs())
}

/// The last lines a child printed, for a failure message.
fn tail(run_dir: &Path, name: &str, lines: usize) -> String {
    let text = std::fs::read_to_string(run_dir.join(format!("{name}.out"))).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// How the supervisor is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Mining,
    Draining,
}

/// **The supervisor**: spawn, gate, keep alive, stop in order. `interactive` prints the gates and
/// the live line, and turns Ctrl-C into a safe stop.
pub(crate) async fn run_supervisor(plan: Plan, interactive: bool, reprofile: &dyn Fn() -> Result<Profile, CliError>) -> CliResult {
    use tokio::signal::unix::{SignalKind, signal as sig};
    std::fs::create_dir_all(&plan.run_dir).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", plan.run_dir.display())))?;
    let mut state = State { supervisor_pid: std::process::id(), phase: "starting".into(), ..Default::default() };
    let say = |state: &mut State, phase: &str, message: String| {
        state.phase = phase.to_string();
        state.message = message;
        state.write(&plan.run_dir);
    };
    let mut sigterm = sig(SignalKind::terminate()).map_err(|e| CliError::generic(e.to_string()))?;
    let mut sigint = sig(SignalKind::interrupt()).map_err(|e| CliError::generic(e.to_string()))?;
    let mut sigusr1 = sig(SignalKind::user_defined1()).map_err(|e| CliError::generic(e.to_string()))?;

    let spawned_at = procs::now_unix() as i64;
    let mut kaspad = spawn(&plan.kaspad, &plan.run_dir).map_err(|e| CliError::new(exit::COMPONENT_DOWN, e))?;
    state.kaspad_pid = Some(kaspad.child.id());
    say(&mut state, "starting", format!("kaspad pid {}", kaspad.child.id()));
    if interactive {
        step(
            Severity::Ok,
            "kaspad",
            &format!("pid {} · output {}", kaspad.child.id(), crate::operator::host::tilde(&plan.run_dir.join("kaspad.out"))),
        );
    }

    // The readiness gates (ADR-0122 §6.1), each bounded, each said as it passes.
    let gates = gates(&plan, &mut kaspad, spawned_at, interactive, reprofile).await;
    if let Err(message) = gates {
        let _ = stop_child(&mut kaspad, plan.grace).await;
        say(&mut state, "failed", message.clone());
        return Err(CliError::new(exit::COMPONENT_DOWN, message));
    }

    // The prompt lane.
    let mut helpers: Vec<Child> = Vec::new();
    if let Some((identity, cmd)) = &plan.identity
        && !identity.exists()
    {
        if let Some(dir) = identity.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::process::Command::new(&cmd.program).args(&cmd.args).output() {
            Ok(out) if out.status.success() => {
                let _ = std::fs::write(identity, &out.stdout);
                if interactive {
                    step(Severity::Ok, "identity", &crate::operator::host::tilde(identity));
                }
            }
            Ok(out) => {
                let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
                say(&mut state, "mining", format!("the prompt lane did not start: the identity could not be made: {why}"));
                if interactive {
                    step(Severity::Error, "identity", &why);
                }
            }
            Err(e) => {
                if interactive {
                    step(Severity::Error, "identity", &e.to_string());
                }
            }
        }
    }
    for cmd in [&plan.gateway, &plan.rail].into_iter().flatten() {
        if plan.identity.as_ref().is_some_and(|(path, _)| !path.exists()) {
            break;
        }
        match spawn(cmd, &plan.run_dir) {
            Ok(child) => {
                if interactive {
                    step(Severity::Ok, cmd.name, &format!("pid {}", child.child.id()));
                }
                helpers.push(child);
            }
            Err(e) => {
                if interactive {
                    step(Severity::Error, cmd.name, &e);
                }
            }
        }
    }
    state.gateway_pid = helpers.iter().find(|h| h.cmd.name == "gateway").map(|h| h.child.id());
    state.rail_pid = helpers.iter().find(|h| h.cmd.name == "rail").map(|h| h.child.id());
    say(&mut state, "mining", "the node is up; the producer is running".into());
    if interactive {
        println!("{}", paint::green("● mining.  misaka mining status · Ctrl-C stops safely (drains claims still to defend)"));
    }

    let mut mode = Mode::Mining;
    let mut crashes: Vec<Instant> = Vec::new();
    let mut last_interrupt: Option<Instant> = None;
    let mut last_drain_check = Instant::now() - Duration::from_secs(3600);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
            _ = sigterm.recv() => {
                // `misaka mining stop` (or the service manager) decided; the check was theirs.
                return shutdown(&plan, &mut state, kaspad, helpers, interactive, "stopped on SIGTERM").await;
            }
            _ = sigusr1.recv() => {
                if mode == Mode::Mining {
                    mode = Mode::Draining;
                    kaspad = drain(&plan, &mut state, kaspad, &mut helpers, interactive).await?;
                }
            }
            _ = sigint.recv() => {
                if !interactive || last_interrupt.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
                    return shutdown(&plan, &mut state, kaspad, helpers, interactive, "stopped on a second Ctrl-C").await;
                }
                last_interrupt = Some(Instant::now());
                println!();
                let owed = match reprofile() {
                    Ok(p) => owed(&Snapshot::gather(p, Duration::from_secs(5), true).await),
                    Err(_) => Vec::new(),
                };
                if owed.is_empty() {
                    return shutdown(&plan, &mut state, kaspad, helpers, interactive, "stopped on Ctrl-C; no claim was left to defend").await;
                }
                println!("{}", stop_refusal(&owed).render());
                println!("{}", paint::yellow("Draining instead: drawing stops now, the node keeps serving, and it exits when the last claim ends. Ctrl-C again within 10 s to stop at once."));
                if mode == Mode::Mining {
                    mode = Mode::Draining;
                    kaspad = drain(&plan, &mut state, kaspad, &mut helpers, interactive).await?;
                }
            }
        }
        // The node: a crash is restarted with backoff, and five in ten minutes is a crash loop.
        if !matches!(kaspad.child.try_wait(), Ok(None)) {
            let code = kaspad.child.try_wait().ok().flatten().and_then(|s| s.code());
            crashes.retain(|t| t.elapsed() < Duration::from_secs(600));
            crashes.push(Instant::now());
            state.restarts += 1;
            if crashes.len() >= 5 {
                let message = format!(
                    "kaspad exited {} times in ten minutes (last code {:?}); its last lines:\n{}",
                    crashes.len(),
                    code,
                    tail(&plan.run_dir, "kaspad", 12)
                );
                say(&mut state, "failed", message.clone());
                for mut h in helpers {
                    let _ = stop_child(&mut h, Duration::from_secs(30)).await;
                }
                return Err(CliError::new(exit::COMPONENT_DOWN, format!("E-PROC-CRASHLOOP: {message}")));
            }
            let backoff = [10u64, 30, 60, 120, 120][crashes.len() - 1];
            let phase = state.phase.clone();
            say(&mut state, &phase, format!("kaspad exited (code {code:?}); restarting in {backoff} s"));
            if interactive {
                println!(
                    "\n{}",
                    paint::yellow(&format!(
                        "kaspad exited (code {code:?}) after {} s; restarting in {backoff} s",
                        kaspad.started.elapsed().as_secs()
                    ))
                );
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            let cmd = if mode == Mode::Draining { without_produce(&plan.kaspad) } else { plan.kaspad.clone() };
            kaspad = spawn(&cmd, &plan.run_dir).map_err(|e| CliError::new(exit::COMPONENT_DOWN, e))?;
            state.kaspad_pid = Some(kaspad.child.id());
            state.write(&plan.run_dir);
        }
        // A helper that exits is restarted at the next tick, without backoff bookkeeping: the rail
        // and the gateway hold no state a restart loses.
        for h in helpers.iter_mut() {
            if !matches!(h.child.try_wait(), Ok(None))
                && h.started.elapsed() > Duration::from_secs(30)
                && let Ok(fresh) = spawn(&h.cmd, &plan.run_dir)
            {
                *h = fresh;
            }
        }
        if mode == Mode::Draining && last_drain_check.elapsed() > Duration::from_secs(60) {
            last_drain_check = Instant::now();
            if let Ok(p) = reprofile() {
                let owed = owed(&Snapshot::gather(p, Duration::from_secs(5), true).await);
                if owed.is_empty() {
                    return shutdown(&plan, &mut state, kaspad, helpers, interactive, "drained: no claim is left to defend").await;
                }
                say(&mut state, "draining", format!("{} claim(s) still to defend", owed.len()));
            }
        }
        if interactive && mode == Mode::Mining {
            live_line(reprofile).await;
        }
    }
}

/// The gates, in order. `Err` is the reason the start failed; a gate that is slow is reported and
/// waited on, not failed, while the process lives.
async fn gates(
    plan: &Plan,
    kaspad: &mut Child,
    spawned_at: i64,
    interactive: bool,
    reprofile: &dyn Fn() -> Result<Profile, CliError>,
) -> Result<(), String> {
    let profile = reprofile().map_err(|e| e.msg)?;
    let log_file = profile.log_file.clone();
    let exited = |k: &mut Child| !matches!(k.child.try_wait(), Ok(None));
    // 1. The boot lines: this build's fingerprint and heights, from this run's log.
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut boot = None;
    while Instant::now() < deadline {
        if exited(kaspad) {
            return Err(format!("kaspad exited during startup; its last lines:\n{}", tail(&plan.run_dir, "kaspad", 12)));
        }
        if let Ok(log) = crate::operator::nodelog::read(&log_file, 4 << 20)
            && log.boot_ts.is_some_and(|b| b >= spawned_at - 2)
            && log.fingerprint.is_some()
        {
            boot = Some(log);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    match (&boot, status::expected(&plan.network)) {
        (Some(log), Some((fp, heights))) => {
            let node = log.fingerprint.clone().unwrap_or_default();
            let own = profile.kaspad.as_ref().and_then(|(_, a)| a.ruleset_flags.first().cloned());
            if let Some(flag) = own {
                if interactive {
                    step(
                        Severity::Ok,
                        "fork",
                        &format!("fingerprint {} · its own ruleset ({flag})", crate::operator::work::short_id(&node)),
                    );
                }
            } else if node == fp && log.schedule.as_ref() == Some(&heights) {
                if interactive {
                    let h = heights.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
                    step(Severity::Ok, "fork", &format!("fingerprint {} ✓ · schedule {h} ✓", crate::operator::work::short_id(&node)));
                }
            } else if interactive {
                step(
                    Severity::Warning,
                    "fork",
                    &format!(
                        "the node booted {} and this CLI is {} — one of the two is not the release (misaka doctor node)",
                        crate::operator::work::short_id(&node),
                        crate::operator::work::short_id(&fp)
                    ),
                );
            }
        }
        (None, _) if interactive => {
            step(Severity::Warning, "fork", &format!("no boot line in {} after 90 s", crate::operator::host::tilde(&log_file)))
        }
        _ => {}
    }
    // 2. The RPC.
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut answered = None;
    while Instant::now() < deadline {
        if exited(kaspad) {
            return Err(format!("kaspad exited before its RPC answered; its last lines:\n{}", tail(&plan.run_dir, "kaspad", 12)));
        }
        if let Ok(node) = crate::operator::snapshot::connect(&profile, Duration::from_secs(2)).await {
            answered = Some(node);
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let Some(node) = answered else { return Err("kaspad runs and its RPC did not answer within 5 minutes".into()) };
    if interactive {
        step(Severity::Ok, "rpc", &format!("{} · {}", node.url.trim_start_matches("ws://"), node.server.network_id));
    }
    // 3. A peer. Waited on, and reported — never a reason to stop a node that is up.
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut peers = 0;
    while Instant::now() < deadline {
        if exited(kaspad) {
            return Err(format!("kaspad exited; its last lines:\n{}", tail(&plan.run_dir, "kaspad", 12)));
        }
        if let Ok(fresh) = crate::operator::snapshot::connect(&profile, Duration::from_secs(2)).await {
            peers = fresh.peers.as_ref().map_or(0, |p| p.len());
            if peers > 0 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    if interactive {
        if peers > 0 {
            step(Severity::Ok, "peers", &format!("{peers}"));
        } else {
            step(Severity::Warning, "peers", "none after 3 minutes — the producer holds until one connects (misaka doctor node)");
        }
    }
    // 4. The producer: its start line, then whatever it says first.
    if interactive {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Ok(log) = crate::operator::nodelog::read(&log_file, 4 << 20)
                && log.producer_started.is_some_and(|t| t >= spawned_at - 2)
            {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let p = reprofile().map_err(|e| e.msg)?;
        let snap = Snapshot::gather(p, Duration::from_secs(5), false).await;
        let view = status::miner_state(&status::inputs(&snap, procs::now_unix() as i64));
        let sev = match view.state {
            status::MinerState::Drawing | status::MinerState::Ready => Severity::Ok,
            _ => Severity::Warning,
        };
        step(sev, "producer", &view.headline);
    }
    Ok(())
}

/// One overwritten line: the miner's state, now.
async fn live_line(reprofile: &dyn Fn() -> Result<Profile, CliError>) {
    let Ok(p) = reprofile() else { return };
    let snap = Snapshot::gather(p, Duration::from_secs(5), false).await;
    let view = status::miner_state(&status::inputs(&snap, procs::now_unix() as i64));
    let daa = snap.node.as_ref().map(|n| format!(" · DAA {}", status::group(n.daa()))).unwrap_or_default();
    let line = format!(
        "{} {}{daa}",
        match view.state {
            status::MinerState::Drawing => "●",
            _ => "◐",
        },
        view.headline
    );
    let width = 110usize;
    let mut shown: String = line.chars().take(width).collect();
    if line.chars().count() > width {
        shown.push('…');
    }
    use std::io::Write;
    print!("\r\x1b[K{shown}");
    let _ = std::io::stdout().flush();
}

fn without_produce(cmd: &Cmd) -> Cmd {
    let mut c = cmd.clone();
    c.args.retain(|a| a != "--palw-produce" && !a.starts_with("--palw-produce="));
    c
}

/// **Drain**: stop drawing (restart kaspad without `--palw-produce`, keeping the panel and the
/// artifacts that serve the claims), stop the prompt lane, and keep supervising until the claims end.
async fn drain(
    plan: &Plan,
    state: &mut State,
    mut kaspad: Child,
    helpers: &mut Vec<Child>,
    interactive: bool,
) -> Result<Child, CliError> {
    for mut h in helpers.drain(..) {
        let said = stop_child(&mut h, Duration::from_secs(60)).await;
        if interactive {
            step(Severity::Ok, h.cmd.name, &said);
        }
    }
    state.gateway_pid = None;
    state.rail_pid = None;
    let said = stop_child(&mut kaspad, plan.grace).await;
    let fresh = spawn(&without_produce(&plan.kaspad), &plan.run_dir).map_err(|e| CliError::new(exit::COMPONENT_DOWN, e))?;
    state.kaspad_pid = Some(fresh.child.id());
    state.phase = "draining".into();
    state.message = format!("{said}; restarted as a panel only (pid {}) until its claims end", fresh.child.id());
    state.write(&plan.run_dir);
    if interactive {
        step(Severity::Info, "draining", &state.message);
    }
    Ok(fresh)
}

/// **Stop in order**: the rail, the gateway, then kaspad with one SIGTERM and the grace.
async fn shutdown(plan: &Plan, state: &mut State, mut kaspad: Child, helpers: Vec<Child>, interactive: bool, why: &str) -> CliResult {
    state.phase = "stopping".into();
    state.message = why.to_string();
    state.write(&plan.run_dir);
    if interactive {
        println!();
    }
    // The rail first (it finishes the submission in flight), then the gateway (it finishes its job).
    let mut ordered = helpers;
    ordered.sort_by_key(|h| if h.cmd.name == "rail" { 0 } else { 1 });
    for mut h in ordered {
        let said = stop_child(&mut h, Duration::from_secs(60)).await;
        if interactive {
            step(Severity::Ok, h.cmd.name, &said);
        }
    }
    if interactive {
        step(Severity::Info, "kaspad", &format!("stopping — up to {} s (a large model takes minutes)", plan.grace.as_secs()));
    }
    let said = stop_child(&mut kaspad, plan.grace).await;
    let killed = said.contains("killed");
    if interactive {
        step(if killed { Severity::Warning } else { Severity::Ok }, "kaspad", &said);
    }
    state.phase = "stopped".into();
    state.message = format!("{why}; {said}");
    state.kaspad_pid = None;
    state.write(&plan.run_dir);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// stop
// ---------------------------------------------------------------------------------------------

/// `misaka mining stop [--drain | --force] [--grace SECS]`.
pub(crate) async fn stop(ctx: &crate::node::Ctx, profile: Profile, drain_: bool, force: bool, grace: Option<u64>) -> CliResult {
    let network = profile.network.clone();
    let supervised = running_supervisor(&network);
    let node = profile.kaspad.clone();
    if supervised.is_none() && node.is_none() {
        println!("nothing to stop: no supervisor and no kaspad for {network} run on this host");
        return Ok(());
    }
    if !force {
        let snap = Snapshot::gather(profile.clone(), Duration::from_secs(5), true).await;
        let owed = owed(&snap);
        if !owed.is_empty() && !drain_ {
            return fail(ctx, stop_refusal(&owed));
        }
        if owed.is_empty() && drain_ {
            println!("no claim is left to defend: stopping instead of draining");
        } else if drain_ {
            return match &supervised {
                Some(state) => {
                    signal(state.supervisor_pid, libc::SIGUSR1);
                    println!("draining: the supervisor (pid {}) stops drawing now and exits when the last of {} claim(s) ends", state.supervisor_pid, owed.len());
                    println!("  misaka mining status shows how many are left");
                    Ok(())
                }
                None => fail(
                    ctx,
                    Finding::error("E-STOP-NOT-SUPERVISED", exit::COMPONENT_DOWN, "This node was not started by misaka, so it cannot drain it")
                        .reason("draining restarts kaspad without --palw-produce, which only the process that started it can do")
                        .fix("restart it yourself without --palw-produce (keep --palw-panel and the artifacts) until misaka work list shows no claim in flight"),
                ),
            };
        }
    }
    match supervised {
        Some(state) => {
            let grace = Duration::from_secs(grace.unwrap_or(profile.stop_grace_secs) + 90);
            signal(state.supervisor_pid, libc::SIGTERM);
            println!(
                "stopping: the supervisor (pid {}) stops the rail, the gateway, then kaspad (up to {} s)",
                state.supervisor_pid, profile.stop_grace_secs
            );
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline && alive(state.supervisor_pid) {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            match State::read(&run_dir(&network)) {
                Some(s) if s.phase == "stopped" => {
                    println!("{} {}", paint::green("✓ stopped:"), s.message);
                    Ok(())
                }
                Some(s) => Err(CliError::new(
                    exit::COMPONENT_DOWN,
                    format!("the supervisor is {} after {} s: {}", s.phase, grace.as_secs(), s.message),
                )),
                None => Ok(()),
            }
        }
        None => {
            let (proc_, _) = node.expect("checked above");
            if let Some(unit) = systemd_unit(proc_.pid) {
                return fail(
                    ctx,
                    Finding::error("E-STOP-SERVICE", exit::COMPONENT_DOWN, format!("This node is run by the service {unit}"))
                        .reason("a service manager restarts a process killed under it; stop the service, not the process")
                        .fix(format!("sudo systemctl stop {unit}"))
                        .fix("(misaka checked the claims above: stopping now abandons none, or you passed --force)"),
                );
            }
            let grace = Duration::from_secs(grace.unwrap_or(profile.stop_grace_secs));
            signal(proc_.pid, libc::SIGTERM);
            println!("stopping kaspad (pid {}): one SIGTERM, then up to {} s", proc_.pid, grace.as_secs());
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline && alive(proc_.pid) {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            if alive(proc_.pid) {
                signal(proc_.pid, libc::SIGKILL);
                println!("{}", paint::yellow(&format!("! kaspad did not stop within {} s and was killed", grace.as_secs())));
            } else {
                println!("{}", paint::green("✓ kaspad stopped"));
            }
            Ok(())
        }
    }
}

/// The systemd unit a process runs under, from its cgroup (Linux).
fn systemd_unit(pid: u32) -> Option<String> {
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    cgroup.lines().filter_map(|l| l.rsplit('/').next()).find(|seg| seg.ends_with(".service")).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::profile::{MiningToml, Source};

    fn profile(file: MiningToml) -> Profile {
        Profile {
            network: "testnet-11".into(),
            network_source: Source::File,
            config_path: Some(PathBuf::from("/h/.misaka/mining.toml")),
            config: Some(file),
            appdir: PathBuf::from("/h/.misaka/testnet-11/node"),
            log_file: PathBuf::from("/h/log"),
            key_path: Some(PathBuf::from("/h/.misaka/miner.seed")),
            bond: Some("aa:0".into()),
            bond_source: Source::File,
            class: None,
            pay_address: None,
            fee_outpoint: Some("aa:1".into()),
            artifacts: vec![PathBuf::from("/m/a.bound.palwart")],
            produce: true,
            panel: true,
            rpc: None,
            prompt: None,
            stop_grace_secs: 240,
            kaspad: None,
            ambiguous_kaspads: Vec::new(),
        }
    }

    /// The flags a mining.toml implies: the network's, the node's, the miner's — and none of the
    /// ones a supervisor must never add by itself.
    #[test]
    fn a_mining_toml_starts_the_node_it_describes_and_nothing_else() {
        let mut file = MiningToml::default();
        file.advanced.peers = vec!["169.58.39.220:26311".into()];
        file.advanced.challenge = Some(true);
        let args = kaspad_args(&profile(file)).expect("complete");
        for want in [
            "--testnet",
            "--netsuffix=11",
            "--appdir=/h/.misaka/testnet-11/node",
            "--rpclisten-borsh=default",
            "--utxoindex",
            "--addpeer=169.58.39.220:26311",
            "--palw-produce",
            "--palw-panel",
            "--palw-producer-key=/h/.misaka/miner.seed",
            "--palw-producer-bond=aa:0",
            "--palw-fee-outpoint=aa:1",
            "--palw-class-artifact=/m/a.bound.palwart",
            "--palw-challenge",
        ] {
            assert!(args.contains(&want.to_string()), "missing {want}: {args:?}");
        }
        for never in ["--enable-unsynced-mining", "--unsaferpc", "--yes", "--palw-devnet-floor-only"] {
            assert!(!args.iter().any(|a| a.starts_with(never)), "{never} must never be generated: {args:?}");
        }
        assert!(!args.iter().any(|a| a.starts_with("--palw-producer-class")), "the base class needs no flag");
        assert!(!args.iter().any(|a| a.starts_with("--palw-producer-pay-address")), "kaspad derives the key's own address");
    }

    /// A file that does not say what the node needs is refused with the field it lacks.
    #[test]
    fn an_incomplete_file_names_what_it_lacks() {
        let mut p = profile(MiningToml::default());
        p.bond = None;
        assert!(kaspad_args(&p).unwrap_err().title.contains("which bond"));
        let mut p = profile(MiningToml::default());
        p.fee_outpoint = None;
        assert!(kaspad_args(&p).unwrap_err().title.contains("funds the panel"), "kaspad would panic: said here instead");
        let mut p = profile(MiningToml::default());
        p.network = "testnet-x".into();
        assert_eq!(kaspad_args(&p).unwrap_err().code, "E-CONFIG-NETWORK");
    }

    /// Draining keeps everything but the producer.
    #[test]
    fn a_drain_keeps_the_panel_and_drops_only_the_producer() {
        let cmd =
            Cmd { name: "kaspad", program: "/k".into(), args: kaspad_args(&profile(MiningToml::default())).unwrap(), env: Vec::new() };
        let drained = without_produce(&cmd);
        assert!(!drained.args.contains(&"--palw-produce".to_string()));
        assert!(drained.args.contains(&"--palw-panel".to_string()));
        assert!(drained.args.iter().any(|a| a.starts_with("--palw-class-artifact")), "the artifacts serve the claims' openings");
        assert_eq!(drained.args.len(), cmd.args.len() - 1);
    }

    #[test]
    fn a_command_line_quotes_only_what_a_shell_would_split() {
        let cmd = Cmd {
            name: "gateway",
            program: "/opt/misaka/gw".into(),
            args: vec!["--outbox".into(), "/Users/me/My Outbox".into(), "--listen".into(), "127.0.0.1:8790".into()],
            env: vec![("MISAKA_PALW_NETWORK_ID".into(), "testnet-11".into())],
        };
        assert_eq!(
            cmd.shell_line(),
            "MISAKA_PALW_NETWORK_ID=testnet-11 /opt/misaka/gw --outbox '/Users/me/My Outbox' --listen 127.0.0.1:8790"
        );
    }
}
