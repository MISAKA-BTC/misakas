//! `misaka` — the unified MISAKA operator CLI.
//!
//! One user-facing front-end over the functionality that today is scattered
//! across `kaspa-pq-cli`, the interactive wallet REPL, `kaspa-pq-validator`,
//! and the `evm_tx_gen` dev example. This is the **Tier A (observability)**
//! slice: read-only commands that wrap the EXISTING node wRPC + EVM JSON-RPC —
//! no new RPCs, no private keys, no transaction construction. They cover the
//! day-to-day "is my node healthy / where is my EVM tx" questions that
//! previously required hand-running raw RPC calls.
//!
//!   misaka node doctor                  # node health, ports, sync, versions
//!   misaka evm balance   --address 0x…
//!   misaka evm nonce     --address 0x…
//!   misaka evm estimate-gas --from 0x… --to 0x… [--value <sompi>] [--data 0x…]
//!   misaka evm tx status --hash 0x…     # one-shot misaka_getEvmTxStatus
//!   misaka evm tx wait   --hash 0x…     # poll until accepted / timeout
//!
//! Every command honors `--output human|json`. Exit codes are stable (see
//! `exit`) so systemd / shell / monitors can branch on them.

mod ask;
mod bond;
mod bootstrap;
mod config;
mod eth;
#[cfg(feature = "evm-send")]
mod evm_send;
mod forward;
mod key_roles;
mod keys;
mod node;
/// ADR-0122: `mining`, `doctor`, `work` — the operator surface over the components.
mod operator;
/// `palw claim` — what became of a free-prompt claim: its phase on the chain, and the next step.
mod palw_claim;
mod palw_court;
mod palw_da;
mod palw_derived;
/// ADR-0108: `palw extension inspect|verify|preflight|submit|receipt-verify`.
mod palw_extension;
mod palw_fp;
/// ADR-0088 Decision 12: `palw line-… / version-… / proposal-… / evaluate` — the model registry.
mod palw_line;
mod palw_model;
mod palw_shard_court;
mod palw_shard_licensing;
mod palw_verify_context;
#[cfg(feature = "evm-send")]
mod prea;
/// ADR-0079 Decision 13: `node security-report` — the host posture, printed from live state.
mod security;
mod setup;
mod validator_reader;
mod wallet;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Stable process exit codes (shared with the wider `misaka` CLI design).
pub mod exit {
    pub const SUCCESS: i32 = 0;
    pub const GENERIC: i32 = 1;
    // 2 is reserved for clap argument errors.
    pub const NETWORK_MISMATCH: i32 = 3;
    pub const CONNECTION: i32 = 4;
    pub const NODE_NOT_SYNCED: i32 = 5;
    pub const TX_REJECTED: i32 = 6;
    pub const TIMEOUT_PENDING: i32 = 7;
    pub const WALLET_LOCKED: i32 = 8;
    pub const UNSAFE_REFUSED: i32 = 10;
    /// `node liveness`: the node did not answer its RPC within the timeout — a wedged runtime.
    pub const LIVENESS_WEDGED: i32 = 11;
    /// `node liveness`: the node answers but its chain has not moved within the stall window.
    pub const LIVENESS_STALLED: i32 = 12;
    /// `node security-report` (ADR-0079 Decision 13): a PUBLIC entrance on a host whose
    /// confinement backend is `none`, or a process that parses public input while holding key
    /// material. Decision 10's refusals, observed after the fact rather than at startup.
    pub const SECURITY_EXPOSED: i32 = 13;
    /// `node security-report`: no platform confinement backend is in force, but nothing public is
    /// exposed behind it. The environment discipline is the whole of the posture.
    pub const SECURITY_DEGRADED: i32 = 14;
    /// `palw extension` (ADR-0108 §4): the manifest is wrong about itself — an id that does not
    /// recompute, a bound that is zero, a vector whose expected root the recomputation does not
    /// produce — or expressible but the chain would refuse it (already registered). The field is
    /// named. (The ADR's §4 draft said 2; 2 is clap's, and 3–5 were taken.)
    pub const EXTENSION_REFUSED: i32 = 20;
    /// `palw extension`: a node extension — unverifiable on this build, never "valid"; the
    /// missing thing (a transformer, a lineage, a file) is named.
    pub const EXTENSION_NODE_EXTENSION: i32 = 21;
    /// `palw extension`: a ruleset change — a fence; the report says what the arming build would
    /// print and whether arming it is a flag day. Activation is a release, never a manifest.
    pub const EXTENSION_RULESET_CHANGE: i32 = 22;
    /// `palw extension`: expressible, but the depth asked for was not reached because this
    /// machine lacks the bytes (an artifact, a vector file). Not a verdict on the manifest.
    pub const EXTENSION_DEPTH_NOT_REACHED: i32 = 23;
    // ADR-0122 Decision 4: the operator surface's range (`mining`, `doctor`, `work`). Each names
    // the area a failure is in, so a supervisor or a monitor can branch without reading prose.
    /// The miner, a seat or a lane is holding: running, and not doing its work, for a named reason.
    pub const NOT_READY: i32 = 30;
    /// The configuration: `mining.toml`, a path, a flag.
    pub const CONFIG: i32 = 31;
    /// The identity: the key, the bond, the pay address.
    pub const IDENTITY: i32 = 32;
    /// Funds: collateral, the fee outpoint, a funding output.
    pub const FUNDS: i32 = 33;
    /// The model: the class, the artifact, the tokenizer, the certification.
    pub const MODEL: i32 = 34;
    /// The host: disk, memory, ports, clock, background upgrades.
    pub const HOST: i32 = 35;
    /// A component is not running, or keeps crashing.
    pub const COMPONENT_DOWN: i32 = 36;
    /// `mining stop` refused: this node still has claims to defend.
    pub const STOP_INFLIGHT: i32 = 37;
}

/// A CLI error that carries the process exit code to surface.
#[derive(Debug)]
pub struct CliError {
    pub code: i32,
    pub msg: String,
}
impl CliError {
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        Self { code, msg: msg.into() }
    }
    pub fn generic(msg: impl Into<String>) -> Self {
        Self::new(exit::GENERIC, msg)
    }
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::new(exit::CONNECTION, msg)
    }
}
pub type CliResult = Result<(), CliError>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "misaka", version, about = "Unified MISAKA operator CLI (observability slice)")]
struct Cli {
    /// Output format.
    #[arg(long, global = true, value_enum, default_value = "human")]
    output: OutputFormat,

    /// Network id (e.g. testnet-10). Sets default RPC ports + the node-network match check.
    /// Resolution: CLI > env MISAKA_NETWORK > ~/.misaka/config.toml > testnet-10.
    #[arg(long, global = true, visible_alias = "network-id", env = "MISAKA_NETWORK")]
    network: Option<String>,

    /// Node wRPC Borsh endpoint host:port (validator/wallet/operator transport).
    /// Default derives from --network (testnet-10 => 127.0.0.1:27210). NOTE: this is
    /// the CODE default; some deployments bind borsh on a non-standard port (e.g.
    /// 27610) — pass it here. This is NOT node gRPC (26210) nor EVM JSON-RPC (8545).
    #[arg(long, global = true, visible_alias = "node-wrpc-borsh", env = "MISAKA_RPC")]
    rpc: Option<String>,

    /// Node gRPC endpoint host:port (low-level RPC transport: `setup`, `config show`).
    /// Resolution: CLI > env MISAKA_NODE_GRPC > ~/.misaka/config.toml [node].grpc >
    /// endpoint registry / network default.
    #[arg(long, global = true, env = "MISAKA_NODE_GRPC")]
    node_grpc: Option<String>,

    /// EVM JSON-RPC HTTP endpoint (the Ethereum lane). `--evm-rpc-url` / `--rpc-url`
    /// (Foundry/cast convention) are accepted aliases. Resolution: CLI > env
    /// MISAKA_EVM_RPC > ~/.misaka/config.toml [evm].rpc_url > http://127.0.0.1:8545.
    #[arg(long, global = true, visible_alias = "evm-rpc-url", visible_alias = "rpc-url", env = "MISAKA_EVM_RPC")]
    evm_rpc: Option<String>,

    /// Per-operation timeout, seconds (connect + request).
    #[arg(long, global = true, default_value_t = 30)]
    timeout: u64,

    /// Suppress non-essential human output (errors still print to stderr).
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Mining, as one purpose: is this host mining, and if not, why and what to do (ADR-0122).
    #[command(subcommand)]
    Mining(MiningCmd),
    /// Check everything a miner needs — node, identity, model, host, prompt lane — and name the fix.
    Doctor(DoctorArgs),
    /// One work from the request (or the won draw) to the reward, by one id (ADR-0122).
    #[command(subcommand)]
    Work(WorkCmd),
    /// Node operations.
    #[command(subcommand)]
    Node(NodeCmd),
    /// EVM-lane operations (read-only in this slice).
    #[command(subcommand)]
    Evm(EvmCmd),
    /// PQ wallet operations (UTXO list / consolidate / send).
    #[command(subcommand)]
    Wallet(WalletCmd),
    /// PALW free-prompt lane (ADR-0044): put a signed commitment on the chain.
    #[command(subcommand)]
    Palw(PalwCmd),
    /// Key management (generate / import / show address). The secret is never a CLI arg.
    #[command(subcommand)]
    Key(KeyCmd),
    /// PALW bond operations — see the collateral this key holds, and retire it (ADR-0063).
    #[command(subcommand)]
    Bond(BondCmd),
    /// Operator config (`~/.misaka/config.toml`): write a scaffold / show effective values.
    #[command(subcommand)]
    Config(ConfigCmd),
    /// P2P bootstrap visibility (debug): the DNS seeds and the peers they resolve to.
    #[command(subcommand)]
    Bootstrap(BootstrapCmd),
    /// Guided VPS setup: preflight, node service, status, and Discord registration helpers.
    #[command(subcommand)]
    Setup(setup::SetupCmd),
    /// Validator operations — forwarded to the `kaspa-pq-validator` binary with the
    /// global --network-id and --rpc (node wRPC Borsh) injected. Run
    /// `misaka validator --help` for its keygen/bond/run/status/... subcommands.
    Validator(PassThrough),
    /// Join the network for --network-id: start a local node that discovers peers via the DNS
    /// seeds (port-free). A newcomer-friendly front-end over `node start` that names the seeds.
    Join(NodeStartArgs),
    /// PREA PQ smart-account signing (executeRoot / executeSession). [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    #[command(subcommand)]
    Prea(PreaCmd),

    /// Ask this network's pinned model a question — and get a receipt anyone can re-run.
    ///
    /// Ordinary LLM use (any language, `--file`/stdin for long prompts), with the property the
    /// rest of this chain is built on: the request is pinned, so another host of the same class
    /// reproduces the answer byte for byte. `--verify <receipt>` is that check.
    Ask(ask::AskArgs),
}

/// Port-free node launch args for `node start` / `join`: an optional RPC `--profile` plus
/// any extra kaspad args forwarded verbatim (after `--`). The network + ports come from the
/// global --network-id, so the operator never types a port.
#[derive(Args, Debug)]
struct NodeStartArgs {
    /// kaspad RPC listener profile (design §9): minimal | local-validator | local-full |
    /// public-evm-rpc | public-node-rpc. Omit to use kaspad's default listeners.
    #[arg(long)]
    profile: Option<String>,
    /// kaspad operational role profile: full | bootstrap-pruned | recovery-sync |
    /// validator | archive | public-rpc.
    #[arg(long)]
    node_profile: Option<String>,
    /// Apply kaspad's 8GB-VPS resource defaults for unspecified knobs.
    #[arg(long)]
    vps_8gb: bool,
    /// Refuse kaspad startup below this free-disk percentage on the data mount.
    #[arg(long)]
    min_disk_free_percent: Option<u8>,
    /// Extra args forwarded verbatim to kaspad, e.g. `-- --utxoindex --nodnsseed`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// ADR-0122 Decision 6: what a `mining` / `doctor` / `work` command reads the miner from. Every
/// flag is optional: each falls back to `~/.misaka/mining.toml`, then to the running node's own
/// command line, then to what `kaspad` itself would default to.
#[derive(Args, Debug, Clone, Default)]
struct ProfileArgs {
    /// The mining configuration (default ~/.misaka/mining.toml).
    #[arg(long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,
    /// The node's --appdir: which node, when several run here or it is not mining.toml's.
    #[arg(long, value_name = "DIR")]
    appdir: Option<String>,
    /// The producer key file (default: mining.toml, else the running node's --palw-producer-key).
    #[arg(long, value_name = "FILE")]
    key_file: Option<String>,
    /// The bond, <txid>:<index> (default: mining.toml, else the running node's --palw-producer-bond).
    #[arg(long, value_name = "OUTPOINT")]
    bond: Option<String>,
    /// The class id, 128 hex (default: mining.toml, else the node's, else the network's base class).
    #[arg(long, value_name = "CLASS_ID")]
    class: Option<String>,
    /// The prompt lane's outbox (the gateway's --outbox); naming it includes the prompt lane.
    #[arg(long, value_name = "DIR")]
    outbox: Option<String>,
}

impl ProfileArgs {
    fn overrides(&self) -> operator::profile::Overrides {
        operator::profile::Overrides {
            config: self.config.clone(),
            appdir: self.appdir.clone(),
            key_file: self.key_file.clone(),
            bond: self.bond.clone(),
            class: self.class.clone(),
            outbox: self.outbox.clone(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum MiningCmd {
    /// Is this host mining? The miner's state and the one line that says why, the bond, the
    /// works and what they paid. Needs no setup on a node that is already running.
    Status {
        #[command(flatten)]
        profile: ProfileArgs,
        /// Redraw every N seconds.
        #[arg(long, value_name = "SECS")]
        watch: Option<u64>,
    },
    /// Start what ~/.misaka/mining.toml describes: preflight, kaspad, the readiness gates, and the
    /// prompt lane's gateway and rail when it is on. Stays in the foreground unless --detach.
    Start {
        #[command(flatten)]
        profile: ProfileArgs,
        /// Run the supervisor in the background and return once it is mining.
        #[arg(long)]
        detach: bool,
        /// Print the exact command lines it would run, and run nothing.
        #[arg(long, conflicts_with_all = ["detach", "service"])]
        print_command: bool,
        /// Print a systemd unit (Linux) or launchd agent (macOS) that runs `misaka mining run`.
        #[arg(long, conflicts_with = "detach")]
        service: bool,
    },
    /// Stop mining — refused while this node still has claims to defend, unless --drain or --force.
    Stop {
        #[command(flatten)]
        profile: ProfileArgs,
        /// Stop drawing now, keep serving, and exit when the last claim is final or voided.
        #[arg(long, conflicts_with = "force")]
        drain: bool,
        /// Stop now, whatever the claims in flight cost.
        #[arg(long)]
        force: bool,
        /// Seconds kaspad gets to stop before it is killed (default: mining.toml's, else 240).
        #[arg(long, value_name = "SECS")]
        grace: Option<u64>,
    },
    /// The supervisor itself, in the foreground and without a terminal: what a service manager
    /// runs (`misaka mining start --service` prints the unit).
    Run {
        #[command(flatten)]
        profile: ProfileArgs,
    },
}

impl ProfileArgs {
    /// The flags that name this profile again, for a supervisor started in the background or by a
    /// service manager.
    fn relaunch_args(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |flag: &str, v: &Option<String>| {
            if let Some(v) = v {
                out.push(flag.to_string());
                out.push(v.clone());
            }
        };
        push("--config", &self.config.as_ref().map(|c| c.display().to_string()));
        push("--appdir", &self.appdir);
        push("--key-file", &self.key_file);
        push("--bond", &self.bond);
        push("--class", &self.class);
        push("--outbox", &self.outbox);
        out
    }
}

#[derive(Args, Debug)]
struct DoctorArgs {
    /// Only these areas (default: all).
    #[arg(value_enum)]
    areas: Vec<operator::doctor::Area>,
    /// Exit non-zero on a warning, too.
    #[arg(long)]
    strict: bool,
    #[command(flatten)]
    profile: ProfileArgs,
}

#[derive(Subcommand, Debug)]
enum WorkCmd {
    /// Every work this host made: block-lane claims from the node's log, prompt-lane jobs from the outbox.
    List {
        #[command(flatten)]
        profile: ProfileArgs,
    },
    /// One work's path from the request (or the won draw) to the reward. ID: a claim-id prefix
    /// (4+ hex), `job:<hex>`, or any full 128-hex claim id.
    Show {
        id: String,
        #[command(flatten)]
        profile: ProfileArgs,
    },
    /// Why a work stands where it stands, and the next step.
    Why {
        id: String,
        #[command(flatten)]
        profile: ProfileArgs,
    },
}

/// Captures all remaining args verbatim to forward to an underlying binary.
#[derive(Args, Debug)]
struct PassThrough {
    /// Arguments forwarded verbatim to the underlying binary (e.g. `keygen --out k`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Write a `~/.misaka/config.toml` scaffold for the selected `--network-id`,
    /// with the canonical per-network ports filled in.
    Init {
        /// Overwrite an existing config file instead of refusing.
        #[arg(long)]
        force: bool,
    },
    /// Print the effective config (CLI > env > file > default) + the config-file path.
    Show,
}

/// PREA signer subcommands. `sign-root` uses the ML-DSA-87 Operational Root key;
/// `sign-session` uses a restricted secp256k1 session key. The secret is never a CLI value.
#[cfg(feature = "evm-send")]
#[derive(Subcommand, Debug)]
enum PreaCmd {
    /// Sign an executeRoot op (ML-DSA-87, F003 v0x02) → F003 input + calldata.
    SignRoot {
        #[command(flatten)]
        key: KeyArgs,
        /// PQ account (smart-account) address (0x…20 bytes).
        #[arg(long)]
        account: String,
        /// Account version (immutable, bound into the op).
        #[arg(long, default_value_t = 1)]
        version: u64,
        /// Root nonce (must equal the account's current rootNonce).
        #[arg(long)]
        nonce: u64,
        /// validAfter block (inclusive).
        #[arg(long, default_value_t = 0)]
        valid_after: u64,
        /// validUntil block (inclusive).
        #[arg(long)]
        valid_until: u64,
        /// Max relayer fee in wei the op authorizes (0 = none / self-submit).
        #[arg(long, default_value = "0")]
        max_relayer_fee: String,
        /// Target address the account will CALL.
        #[arg(long)]
        to: String,
        /// Native value forwarded to the target, in wei.
        #[arg(long, default_value = "0")]
        value: String,
        /// 0x-hex calldata for the target call.
        #[arg(long, default_value = "0x")]
        calldata: String,
    },
    /// Sign an executeSession op (secp256k1) → r‖s‖v + calldata.
    SignSession {
        #[command(flatten)]
        key: EvmKeyArgs,
        /// PQ account (smart-account) address (0x…20 bytes).
        #[arg(long)]
        account: String,
        /// Account version (immutable, bound into the op).
        #[arg(long, default_value_t = 1)]
        version: u64,
        /// Session call index (must equal the account's current sessionNonce for the key).
        #[arg(long)]
        call_index: u64,
        /// Max relayer fee in wei the op authorizes (0 = none / self-submit).
        #[arg(long, default_value = "0")]
        max_relayer_fee: String,
        /// Target address the session will CALL.
        #[arg(long)]
        to: String,
        /// Native value forwarded to the target, in wei.
        #[arg(long, default_value = "0")]
        value: String,
        /// 0x-hex calldata for the target call.
        #[arg(long, default_value = "0x")]
        calldata: String,
    },
}

/// Key-source flags shared by keyed commands. The secret is loaded only from a
/// permission-checked file or stdin — NEVER as a command-line value.
#[derive(Args, Debug, Clone)]
struct KeyArgs {
    /// Path to a hex 32-byte ML-DSA-87 seed file (perms-checked).
    #[arg(long, env = "MISAKA_KEY_FILE")]
    key_file: Option<String>,
    /// Read the hex seed from stdin instead of a file.
    #[arg(long)]
    key_stdin: bool,
}
impl KeyArgs {
    fn source(&self) -> keys::KeySource {
        keys::KeySource { key_file: self.key_file.clone(), key_stdin: self.key_stdin }
    }
    /// `None` when no key was named — for commands that can read without one.
    fn source_opt(&self) -> Option<keys::KeySource> {
        (self.key_file.is_some() || self.key_stdin).then(|| self.source())
    }
}

#[derive(Subcommand, Debug)]
enum WalletCmd {
    /// UTXO-set operations (list / consolidate).
    #[command(subcommand)]
    Utxo(UtxoCmd),
    /// Send MSK to a recipient (dry-run unless --yes).
    Send {
        /// Recipient address (must match --network).
        #[arg(long)]
        to: String,
        /// Amount in MSK (decimal, e.g. 10.5).
        #[arg(long)]
        amount: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
        /// Select ONLY settled coinbase UTXOs — never a carrier change or fee float. For a
        /// producer/panel wallet whose reservations another node holds (the RPC this CLI asks
        /// only knows its OWN node's reserved outpoints), largest-first selection reaches for a
        /// foreign panel's float; this keeps the spend to mining rewards alone.
        #[arg(long)]
        coinbase_only: bool,
        #[command(flatten)]
        key: KeyArgs,
    },
}

/// `palw extension verify --depth` (ADR-0108 Decision 3).
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ExtensionDepthArg {
    /// Canonical form, bounds, the id, the kind's identity recomputed from what is inline.
    Structural,
    /// The declared vectors re-run with this build.
    Vectors,
    /// The chain's own computation over the bytes this machine holds.
    Full,
}

impl ExtensionDepthArg {
    fn depth(self) -> misaka_palw_extension::PalwExtensionDepthV1 {
        match self {
            Self::Structural => misaka_palw_extension::PalwExtensionDepthV1::Structural,
            Self::Vectors => misaka_palw_extension::PalwExtensionDepthV1::Vectors,
            Self::Full => misaka_palw_extension::PalwExtensionDepthV1::Full,
        }
    }
}

/// **ADR-0108: an extension is a manifest the verifier recomputes.** One manifest in; one of
/// three tiers out — expressible now, a node extension, or a ruleset change — or refused by the
/// field's name. A receipt is evidence of one reproduction and never a vote.
#[derive(Subcommand, Debug)]
enum ExtensionCmd {
    /// What it is, its id, its tier by structure alone (offline, at the genesis shape).
    Inspect {
        manifest: std::path::PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Recompute the manifest at a depth; write a receipt with --receipt-out (signed with --key-file).
    Verify {
        manifest: std::path::PathBuf,
        #[arg(long, value_enum, default_value_t = ExtensionDepthArg::Vectors)]
        depth: ExtensionDepthArg,
        /// Write a `misaka-palw/extension-receipt/v1` here — canonical JSON, signed if a key is given.
        #[arg(long)]
        receipt_out: Option<std::path::PathBuf>,
        #[command(flatten)]
        key: KeyArgs,
        #[arg(long)]
        json: bool,
    },
    /// Verify at Full, plus the chain's shape at the node's DAA: the object, its cost, and the
    /// refusal it would meet — before any fee. Registration terms are the genesis terms (§8).
    Preflight {
        manifest: std::path::PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Build the EXISTING object and file it (dry-run unless --yes). A class becomes a
    /// `ClassRegistered` signed by this key, which must be the bond's (`--bond txid:index`); a
    /// certification files its object; a transformer and a ruleset candidate have nothing to file.
    Submit {
        manifest: std::path::PathBuf,
        #[command(flatten)]
        key: KeyArgs,
        /// The registrant's producer bond (`txid:index`) — required for a class.
        #[arg(long)]
        bond: Option<String>,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// One receipt: its canonical form, its id, its signature, and the ruleset it was made on
    /// against this build's (SA-3). With --manifest, it must be about that manifest.
    ReceiptVerify {
        receipt: std::path::PathBuf,
        #[arg(long)]
        manifest: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PalwCmd {
    /// ADR-0108: verify, preflight, submit and receipt an extension manifest.
    #[command(subcommand)]
    Extension(ExtensionCmd),
    /// Submit a free-prompt commitment built by `misaka-palw-fp-rail` (dry-run unless --yes).
    FpSubmit {
        /// The rail's `*.commitment-tx.borsh`.
        #[arg(long)]
        tx: std::path::PathBuf,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
        /// Also write the claim's data-availability material (`<claim-id>.material`, the FPM1
        /// job+prompt payload) into this directory. Point it at a producer node's
        /// `palw-retention/` and that node's re-broadcast loop serves the panel; without the
        /// material no seat can replay the job, and an Unavailable quorum DEFAULTS the executor.
        #[arg(long)]
        material_out: Option<std::path::PathBuf>,
        /// The executor's CAPTURE for this claim — the family material the worker retained
        /// (`material.bin` under its `--trace-out`). With it, `--material-out` writes an `FPC1`
        /// payload (job + prompt + capture) instead of the question-only `FPM1`, and a seat can
        /// verify the claim from the bytes instead of re-running it, and a court can try it
        /// (ADR-0073 Decision 1a). Requires `--material-out`.
        #[arg(long, requires = "material_out")]
        capture: Option<std::path::PathBuf>,
        /// ADR-0078 Decision 6: put this claim's canonical DSL under the data-availability
        /// obligation — the gateway's `<job>.dsl-payload.fpd1`, staged as `<claim-id>.dsl` beside
        /// the material so the node serves it on request. Off unless given: the DSL is the
        /// answer to the person's prompt. Requires `--material-out`.
        #[arg(long, requires = "material_out")]
        dsl_payload: Option<std::path::PathBuf>,
    },
    /// Submit a PALW lifecycle object written by `palw-certify` — the ADR-0075 certification
    /// objects `FamilyCertified` (a family's drill evidence, graded on chain) and
    /// `ClassLaneCertified` (a class bound to a chain-certified family) — funded from this key.
    /// Nothing signs: the court grades the evidence and the fee is the rent. Dry-run unless --yes.
    SubmitObject {
        #[command(flatten)]
        key: KeyArgs,
        /// The borsh `PalwConsensusObjectV2` file(s) (`palw-certify drill|bind --out`). Pass the
        /// chunks of one object in index order (`f.chunk0 f.chunk1 …`); each rides its own
        /// carrier, funded from the previous carrier's change.
        #[arg(long = "object", required = true, num_args = 1..)]
        object: Vec<std::path::PathBuf>,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **File a court close, whole or in the carriage ADR-0080 gave it** (W13).
    ///
    /// A close that fits one carrier is a `CourtClosed` object on an ordinary lifecycle
    /// transaction. A close that does not — and on the RC ruleset a legal one needs twenty-three
    /// carriers — rides ADR-0080 design A's own table: a small signed `CourtCloseDeclared` that
    /// pins every byte behind it, then those bytes as `CourtCloseChunk`s, one per carrier, keyed
    /// `(session, side)` rather than by a digest of the bytes. This drives both: it checks the
    /// close against the court's own cost rule BEFORE spending anything, cuts it the way the court
    /// cuts it, refuses every limit it can check up front — the ruleset's carrier count, the
    /// group row's bitmap, the assembly window against `--deadline-at` — says how many blocks the
    /// carriage costs against the turn deadline it spends, funds and submits the parts in order,
    /// and names the part that failed. Resumable: a run interrupted mid-group picks up from the
    /// parts the chain still holds rather than re-paying for them.
    ///
    /// **The split path is planned and priced but not yet filable**: consensus refuses every
    /// `CourtCloseDeclared` until ADR-0080 W6 lands the declaration's signature check, so this
    /// command reports the whole carriage and then refuses rather than spending fees on carriers
    /// the chain would drop. A close that fits one carrier files normally.
    /// Build and sign a data-availability accusation (ADR-0062): make a claim's executor open one
    /// trace event on chain, at the price of your own charge if it answers. Writes a
    /// `PalwConsensusObjectV2::DefaultAccused` for `palw submit-object` to file.
    DaAccuse {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
        /// The logits row (decode position) to demand.
        #[arg(long)]
        row: u32,
        /// The tile inside that row (0 for a flat class; a tiled class's tile index).
        #[arg(long, default_value_t = 0)]
        tile: u8,
        /// Your own bond outpoint, `txid:index` — a seat of the claim's panel or a bonded challenger.
        #[arg(long)]
        bond: String,
        /// Where to write the signed object.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Sign a one-move accusation (ADR-0099 Decision 5 / ADR-0100): a named leaf of a claim's own
    /// execution that does not recompute, with its refutation. Reads an unsigned borsh
    /// `PalwShardCourtAccusationV1`, signs it under your bond key and writes a
    /// `PalwConsensusObjectV2::ShardCourtAccused` for `palw submit-object`.
    ShardAccuse {
        #[command(flatten)]
        key: KeyArgs,
        /// The unsigned accusation (borsh `PalwShardCourtAccusationV1`), as a seat or a tool wrote it.
        #[arg(long)]
        accusation: std::path::PathBuf,
        /// Your own bond outpoint, `txid:index` — any Active bond above the floor, never the claim's.
        #[arg(long)]
        bond: String,
        /// The ladder to check the named leaf against locally (default: the shipped court's 2^26).
        #[arg(long, default_value_t = 1u64 << 26)]
        ladder: u64,
        /// Where to write the signed object.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Declare a class's shard plan (ADR-0100 Decision 4), signed by the class's registrant bond:
    /// claims bound afterwards draw a panel per shard and license by parts. Declared once.
    /// Writes a `PalwConsensusObjectV2::ClassShardPlanDeclared` for `palw submit-object`.
    ShardPlan {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex class id (a class registered by a bond — a genesis class has no registrant).
        #[arg(long)]
        class: String,
        /// How many shards (2..=1024). A shard holds at least one layer; `palw-shard-plan` prices a count.
        #[arg(long)]
        count: u32,
        /// The class's registrant bond outpoint, `txid:index` — its key signs.
        #[arg(long)]
        bond: String,
        /// Where to write the signed object.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Declare which shards of a class your bond holds (ADR-0100 Decision 4) — a refinement of the
    /// class your bond declared in capable_classes. Writes a `BondShardsDeclared`; an empty
    /// `--shards ""` withdraws.
    BondShards {
        #[command(flatten)]
        key: KeyArgs,
        /// Your bond outpoint, `txid:index`.
        #[arg(long)]
        bond: String,
        /// 128-hex class id.
        #[arg(long)]
        class: String,
        /// The class's plan's shard count (it must match the declared plan).
        #[arg(long)]
        count: u32,
        /// Comma-separated shard indices, ascending, e.g. `0,1`; empty withdraws.
        #[arg(long, default_value = "")]
        shards: String,
        /// Where to write the signed object.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    CourtClose {
        #[command(flatten)]
        key: KeyArgs,
        /// The borsh `PalwConsensusObjectV2::CourtClosed` to file.
        #[arg(long)]
        close: std::path::PathBuf,
        /// 128-hex class id of the class under dispute, so the deadline is priced on ITS row.
        /// Without it the widest shipped row is assumed and the report says so.
        #[arg(long)]
        class: Option<String>,
        /// Which of the session's two bonds is declaring: `challenger` or `executor`. Required
        /// only for a close too large for one carrier — that one rides as a declaration attributed
        /// to one side, and nothing in the close file says which side is moving.
        #[arg(long)]
        side: Option<String>,
        /// The DAA score this move's turn expires at, when you know it. With it the report says
        /// whether the carriage fits the time that is LEFT, not the time a turn is worth.
        #[arg(long)]
        deadline_at: Option<u64>,
        /// Where the carriage index lives between runs. Defaults to `<close>.carriage.json`.
        #[arg(long)]
        state: Option<std::path::PathBuf>,
        /// Discard any existing carriage index and file every part again.
        #[arg(long)]
        restart: bool,
        /// Plan and price the move against `--network`'s own preset, without contacting a node.
        /// The cost rule, the cut and the deadline are pure functions of the close and the
        /// ruleset; deciding whether you will make your turn should not need a synced node.
        #[arg(long, conflicts_with = "yes")]
        offline: bool,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0075 / ADR-0082: the two free-prompt lane facts a builder must know before it
    /// builds.** Is this class SEATED on the free-prompt lane (`fp_certified` — the genesis
    /// certified set union the chain set, the same two `FreePromptCommitted` refuses on), and is
    /// ADR-0082's decode ruleset in force (`fp_decode_rules_armed` — past that fence a job carries
    /// its sampling seed and temperature inside its context hash and decode leaves are what earn)?
    ///
    /// A job on an uncertified class is still answered and its commitment is unsubmittable; a job
    /// built for the wrong decode ruleset is honest and unreproducible. Neither is guessable and
    /// both are one read. Nothing signs, nothing spends.
    Certified {
        /// 128-hex class id (`execution_class_id`, the shape profile id).
        class_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0110: run a context vector through the network's own pipeline** — produce, commit,
    /// seat, court, availability, fit — and write one canonical document whose `document_id`
    /// covers what every honest machine must agree on and nothing this host measured. No node is
    /// contacted. `--sign-with` writes a receipt: evidence that this key reproduced the vector,
    /// never a vote — nothing reads it, and no count of receipts arms anything.
    VerifyContext {
        /// A shipped vector by name (`--list` names them).
        #[arg(long, conflicts_with = "vector_file")]
        vector: Option<String>,
        /// A vector that is not shipped, as its JSON document — its document says `shipped: false`.
        #[arg(long)]
        vector_file: Option<std::path::PathBuf>,
        /// List the shipped vectors, their ids and where each is meant to run.
        #[arg(long)]
        list: bool,
        /// A comma-separated subset of produce,commit,seat,court,availability,fit (default: all).
        #[arg(long)]
        stages: Option<String>,
        /// Write the canonical document here.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Where the receipt goes (default: beside `--out`, as `<out>.receipt.json`).
        #[arg(long)]
        receipt_out: Option<std::path::PathBuf>,
        /// Sign a receipt with the ML-DSA-87 seed in this perms-checked file.
        #[arg(long)]
        sign_with: Option<String>,
    },
    /// **ADR-0110: check a context receipt** — its signature under the receipt context, and, with
    /// `--document`, that the document's recomputed id is the one the receipt signed.
    VerifyReceipt {
        /// The receipt JSON `verify-context --sign-with` wrote.
        receipt: std::path::PathBuf,
        /// The document it is a receipt for.
        #[arg(long)]
        document: Option<std::path::PathBuf>,
    },
    /// **ADR-0087: a line's model market** as the tip holds it — reserve, positions in the curve,
    /// price, sold, burned, paid, status — and a quote for a buy (`--quote-msk`). Nothing signs.
    /// Keyed by LINE (ADR-0088 Decision 9): a class id names the class's founding line.
    ModelShow {
        /// 128-hex line id (a class id names the class's founding line).
        line_id: String,
        /// Quote a buy of this many MSK (e.g. `12.5`, or `1250000000sompi`).
        #[arg(long)]
        quote_msk: Option<String>,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0087: every position a holder has**, by line. The holder is its payout payload —
    /// name it, or name the key it is derived from.
    ModelPositions {
        /// 128-hex holder (the BLAKE2b-512 of an ML-DSA-87 public key).
        #[arg(long)]
        holder: Option<String>,
        #[command(flatten)]
        key: KeyArgs,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// ADR-0089 Decision 5: buy a position FROM THE EVM — a signed call to the ModelWriter
    /// (0x…F013) carrying `sendAction(buy)` with the MSK as the call's value; the fold applies it
    /// after the block and the next chain block settles it. [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    ModelEvmBuy {
        #[command(flatten)]
        key: EvmKeyArgs,
        /// The line (128 hex; a class's own line has the class id).
        #[arg(long)]
        line: String,
        /// MSK to pay, whole sompi (e.g. "5" or "0.25").
        #[arg(long)]
        msk: String,
        #[arg(long, default_value_t = 0)]
        min_positions: u64,
        #[arg(long)]
        gas_limit: Option<u64>,
        #[arg(long)]
        max_fee: Option<u128>,
        #[arg(long)]
        nonce: Option<u64>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
    },
    /// ADR-0089 Decision 5: sell positions FROM THE EVM — `sendAction(sell)` with no value; the
    /// net MSK is credited to the signing account when the next chain block settles it.
    #[cfg(feature = "evm-send")]
    ModelEvmSell {
        #[command(flatten)]
        key: EvmKeyArgs,
        #[arg(long)]
        line: String,
        #[arg(long)]
        positions: u64,
        /// The least MSK (whole sompi) the sell may net, else the fold refuses it.
        #[arg(long, default_value = "0")]
        min_msk: String,
        #[arg(long)]
        gas_limit: Option<u64>,
        #[arg(long)]
        max_fee: Option<u128>,
        #[arg(long)]
        nonce: Option<u64>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
    },
    /// ADR-0089 Decision 7: the units an EVM account holds on a line, read through the position
    /// window (0x…F012) over the node's eth JSON-RPC.
    ModelEvmPosition {
        #[arg(long)]
        line: String,
        #[arg(long)]
        address: String,
    },
    /// **ADR-0090: seed a line's market** — lock at least the network's least seed (100,000 MSK)
    /// in the line's sink; the whole of it becomes the curve's reserve and nothing ever pays it
    /// back. Five hundred thousand whole positions open in the curve at `seed / 500,000` each.
    ModelSeed {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id (a class id names the class's founding line)
        #[arg(long)]
        line: String,
        /// MSK to lock (e.g. `100000`, or `10000000000000sompi`)
        #[arg(long)]
        msk: String,
        /// Actually broadcast (otherwise a dry-run preview)
        #[arg(long)]
        yes: bool,
    },
    /// ADR-0090 from the EVM: seed a line's market with the call's value (at least 100,000 MSK)
    /// through the ModelWriter; settled in the next chain block. [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    ModelEvmSeed {
        #[command(flatten)]
        key: EvmKeyArgs,
        #[arg(long)]
        line: String,
        /// MSK to lock, whole sompi (e.g. "100000")
        #[arg(long)]
        msk: String,
        #[arg(long)]
        gas_limit: Option<u64>,
        #[arg(long)]
        max_fee: Option<u128>,
        #[arg(long)]
        nonce: Option<u64>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
    },
    /// **ADR-0087: buy positions of a line from its curve.** The carrier pays `--msk` into the
    /// line's sink; the fold credits the net leg to the curve (5 % burned, and 1 % to the line's
    /// owner — 5 % past ADR-0114's fence; the preview prints the schedule the node serves) and
    /// the curve's positions to the key's payout payload. Refused on chain when fewer than
    /// `--min-positions` would be released.
    ModelBuy {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id (a class id names the class's founding line).
        #[arg(long)]
        line: String,
        /// MSK to pay (e.g. `12.5`, or `1250000000sompi`).
        #[arg(long)]
        msk: String,
        /// The fewest positions to accept; the move is refused, never partially filled, below it.
        #[arg(long, default_value_t = 0)]
        min_positions: u64,
        /// Actually broadcast (otherwise a dry-run preview with the quote).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0087: sell positions back to the curve.** Signed by the key whose payout payload
    /// holds them; the net leg (94 % of what the curve pays, 90 % past ADR-0114) reaches the same payload through the
    /// coinbase. Refused on chain when the net would be under `--min-msk`.
    ModelSell {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id (a class id names the class's founding line).
        #[arg(long)]
        line: String,
        /// Positions to sell.
        #[arg(long)]
        positions: u64,
        /// The least MSK to accept for them (e.g. `12.5`).
        #[arg(long)]
        min_msk: Option<String>,
        /// Actually broadcast (otherwise a dry-run preview with the quote).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: a line** as the tip holds it — owner, developer, maintainer (and their payout
    /// payloads), the current and preview versions, the roots in force for its class. A class id
    /// names the class's founding line. Nothing signs.
    LineShow {
        /// 128-hex line id.
        line_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0088: every version of a line the node holds**, oldest first, with status and usage.
    /// The last 64 stay in state; older ones are named as evicted (the explorer holds them).
    LineLog {
        /// 128-hex line id.
        line_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0088: every line of a class**, the founding line included.
    LineList {
        /// 128-hex class id.
        class_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0088: the proposals attached to a line** — root, note, proposer, adoption.
    Proposals {
        /// 128-hex line id.
        line_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// **ADR-0088: found a further line on a class.** `--bond` becomes its owner, developer and
    /// maintainer and signs; V1 is `--root`. Rent-priced (1 MSK of the fee is burned).
    LineFound {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex class id.
        #[arg(long)]
        class: String,
        /// The line's name (1..=64 bytes); shared, never squatted — the founder is in the line id.
        #[arg(long)]
        name: String,
        /// 128-hex artifact root of V1.
        #[arg(long)]
        root: String,
        /// The founding bond, `<txid>:<index>`; the key must be its registered key.
        #[arg(long)]
        bond: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: publish a version.** Signed by the line's developer bond; the version number is
    /// read off the chain (`versions_published + 1`). Without `--preview` it becomes current at
    /// once and the previous current keeps its root in force for the grace. Re-publishing an
    /// older root is a rollback; the current root again is refused.
    VersionPublish {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// 128-hex artifact root of the new version.
        #[arg(long)]
        root: String,
        /// The version this one continues from (omit for none).
        #[arg(long)]
        parent: Option<u32>,
        /// 128-hex proposal id this version adopts; the proposer is paid the contributor share.
        #[arg(long)]
        adopted_from: Option<String>,
        /// Declared, recorded, never checked: 128-hex each.
        #[arg(long)]
        runtime_hash: Option<String>,
        #[arg(long)]
        dataset_commitment: Option<String>,
        #[arg(long)]
        training_config_hash: Option<String>,
        #[arg(long)]
        notes_hash: Option<String>,
        /// Put the root in force without making it current (at most 2 previews at once).
        #[arg(long)]
        preview: bool,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: promote a preview to current** (developer).
    VersionPromote {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        #[arg(long)]
        version: u32,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: withdraw a preview, or a superseded version before its grace ends**
    /// (developer). A current version is succeeded, never withdrawn.
    VersionWithdraw {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        #[arg(long)]
        version: u32,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: set a line's developer, maintainer and contributor share** (owner). A flag
    /// omitted keeps the row's value; `owner` resets a role to the owner.
    /// ADR-0095 §4.1: declare what this line's positions grant their holders.
    LineBenefits {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// `units:GRANT[,GRANT…][:lead[:hold[:note]]]`, once per tier, e.g.
        /// `--tier 1:EARLY_VERSION,SUPPORT:600`. Pass none to WITHDRAW the declaration.
        #[arg(long = "tier")]
        tiers: Vec<String>,
        /// §4.6: publish a version this often, or the promise lapses on its own. 0 = no undertaking.
        #[arg(long, default_value_t = 0)]
        cadence_daa: u64,
        /// §4.6: the height the whole declaration stops granting. 0 = never.
        #[arg(long, default_value_t = 0)]
        expires_daa: u64,
        #[arg(long)]
        yes: bool,
    },
    LineRoles {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// `<txid>:<index>`, or `owner`.
        #[arg(long)]
        developer: Option<String>,
        /// `<txid>:<index>`, or `owner`.
        #[arg(long)]
        maintainer: Option<String>,
        /// The share of the owner's leg (1 %, 5 % past ADR-0114) an adopted contributor takes while its version is
        /// current, in permille (0..=1000).
        #[arg(long)]
        contributor_permille: Option<u16>,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: hand a line to a new owner bond** (owner). Developer and maintainer reset to
    /// the new owner; positions do not move.
    LineTransfer {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// `<txid>:<index>` of an Active bond.
        #[arg(long)]
        new_owner: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: retire a line** (owner): the market closes to buys, the roots leave force
    /// after the grace, the history stays.
    LineRetire {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: post a proposal on a line** — a root and a note — from any Active bond.
    /// Rent-priced (1 MSK of the fee is burned). Paid when a version adopts it.
    ProposalPost {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// 128-hex artifact root proposed.
        #[arg(long)]
        root: String,
        /// 128-hex hash of the note.
        #[arg(long)]
        note_hash: String,
        /// The proposing bond, `<txid>:<index>`; the key must be its registered key.
        #[arg(long)]
        bond: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: close a proposal** (developer), to make room.
    ProposalClose {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        /// 128-hex proposal id.
        #[arg(long)]
        proposal: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **ADR-0088: post an evaluation of a version** — a declaration from any Active bond, at
    /// most 16 per version, one per bond. Rent-priced (1 MSK of the fee is burned).
    Evaluate {
        #[command(flatten)]
        key: KeyArgs,
        /// 128-hex line id.
        #[arg(long)]
        line: String,
        #[arg(long)]
        version: u32,
        /// 128-hex id of the evaluator (a benchmark, a suite, a person's key hash).
        #[arg(long)]
        evaluator_id: String,
        /// The score, in permille (0..=1000).
        #[arg(long)]
        score_permille: u32,
        /// 128-hex hash of the report.
        #[arg(long)]
        report_hash: String,
        /// The evaluating bond, `<txid>:<index>`; the key must be its registered key.
        #[arg(long)]
        bond: String,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
    },
    /// **What became of a free-prompt claim** — its phase on the chain, what that means, and the
    /// next step.
    ///
    /// Reads `getPalwFreePromptClaim` and says what the phase means for the executor: a panel
    /// drawn and when, receipts due, final and when (and by whom) its quanta can be spent as
    /// receipt blocks, or voided and who paid — dated in the windows of the network's own
    /// ConsensusV2 bundle, against the node's current DAA. Name claim ids (the gateway's
    /// `fp_claim_id`), or pass `--outbox` to follow every job the gateway wrote there as far as it
    /// got: answered but not committed, committed but not submitted, signed but not submitted,
    /// expired, given up, or on the chain. A named claim the chain does not hold exits non-zero; an
    /// outbox job that is not on the chain is reported in its row. Nothing signs, nothing spends.
    #[command(group(clap::ArgGroup::new("claims").required(true).multiple(true).args(["claim_ids", "outbox"])))]
    Claim {
        /// 128-hex claim ids (the gateway's or the rail's `fp_claim_id`).
        #[arg(value_name = "CLAIM_ID")]
        claim_ids: Vec<String>,
        /// The gateway's outbox directory: every `fp-job-*.json` job in it, oldest first.
        #[arg(long, value_name = "DIR")]
        outbox: Option<std::path::PathBuf>,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// ADR-0078 Decision 5: read what the chain holds about a free-prompt claim's DERIVATIONS —
    /// the grammar, the transformer, the DSL and artifact hashes, and the claim's own output_root.
    /// The answer's token ids are on no chain; hold them from the gateway response.
    Derived {
        /// 128-hex free-prompt claim id (the gateway's `fp_claim_id`).
        claim_id: String,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
    /// ADR-0078 X6: re-run the derivation the chain names, over the answer you kept, and say
    /// whether the chain's copy agrees — `consistent`, or the first mismatching field by name.
    DerivedVerify {
        /// 128-hex free-prompt claim id.
        claim_id: String,
        /// The gateway's response JSON (its `misaka` block carries `output_token_ids`,
        /// `job_context_hash`, `family` and the canonical DSL), or a bare JSON array of the
        /// answer's output token ids.
        #[arg(long)]
        answer: std::path::PathBuf,
        /// The canonical DSL bytes, when the response file does not carry them inline.
        #[arg(long)]
        dsl: Option<std::path::PathBuf>,
        /// The whole `PalwJobContextV2` (borsh, or the same bytes as hex text), when the response
        /// file does not carry `misaka.job_context`. Its hash is DERIVED from it, so the tokenizer
        /// it pins and the `output_root` it implies cannot come from two different jobs.
        #[arg(long)]
        job_context: Option<std::path::PathBuf>,
        /// The tokenizer.json the class was converted from — the fourth binding input. With the
        /// ids, the context and the family, the DSL is RENDERED from the claim's own ids and the
        /// verdict is a statement about one inference (`binding_checked: true`).
        #[arg(long)]
        tokenizer: Option<std::path::PathBuf>,
        /// 128-hex job context hash, overriding the one in `--answer`. Recomputing `output_root`
        /// is all it can do: the binding needs `--job-context`, whose `tokenizer_id` a hash cannot
        /// carry.
        #[arg(long)]
        job_context_hash: Option<String>,
        /// Family of the class that answered (`base0`, `qwen36`, `qwen25-a16`), overriding
        /// `--answer`'s.
        #[arg(long)]
        family: Option<String>,
        /// JSON output (`--output json` does the same).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum UtxoCmd {
    /// Paged UTXO summary of an address (read-only; safe on huge addresses).
    List {
        /// Address to inspect; defaults to the key's funding address.
        #[arg(long)]
        address: Option<String>,
        #[command(flatten)]
        key: KeyArgs,
    },
    /// Merge many small self-UTXOs into fewer (chunked; dry-run unless --yes).
    Consolidate {
        /// Max inputs per consolidation tx (each ML-DSA input ≈7 KB; capped at 20).
        #[arg(long, default_value_t = 20)]
        max_inputs: usize,
        /// Max consolidation transactions to build/submit in one run (hard-capped at 200).
        #[arg(long, default_value_t = 100)]
        max_txs_per_run: usize,
        /// Milliseconds to sleep between live submits.
        #[arg(long, default_value_t = 200)]
        sleep_ms: u64,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        key: KeyArgs,
    },
}

#[derive(Subcommand, Debug)]
enum KeyCmd {
    /// Generate a fresh ML-DSA-87 seed to a 0600 file and print its address.
    Gen {
        /// Output seed file path (refuses to overwrite).
        #[arg(long)]
        out: String,
    },
    /// Print the funding (P2PKH-ML-DSA) address for a key.
    Address {
        #[command(flatten)]
        key: KeyArgs,
    },
    /// Import an EXISTING 32-byte ML-DSA-87 seed — from stdin, or from a 0600 file — into a
    /// 0600 file.
    ///
    /// The secret is never a CLI argument and never an environment variable (ADR-0063 SA-1):
    /// pipe it in (`cat seed.hex | misaka key import --out /etc/misaka/bond.key --hex-stdin`) or
    /// name a file (`--hex-file /media/backup/seed.hex`, which must be mode 0600). An argument
    /// lands in `ps` and the shell history; an env var is inherited by every child process.
    /// A BIP39 mnemonic is NOT accepted — this tree and the web wallet have no agreed derivation
    /// to an ML-DSA-87 seed, and guessing one would silently produce a different address.
    Import {
        /// Output seed file path (refuses to overwrite).
        #[arg(long)]
        out: String,
        /// Read the 64-hex-character seed from stdin.
        #[arg(long)]
        hex_stdin: bool,
        /// Read the 64-hex-character seed from this file, which must be mode 0600.
        /// The PATH is the argument; the secret never is.
        #[arg(long, conflicts_with = "hex_stdin")]
        hex_file: Option<String>,
    },
}

/// ADR-0063 D2/D3: the operator's half of the PALW bond lifecycle.
#[derive(Subcommand, Debug)]
enum BondCmd {
    /// Show the bond outpoint(s) this key holds, from the chain — the outpoint
    /// `--palw-producer-bond` needs, which the node prints once at registration and stores nowhere.
    Status {
        #[command(flatten)]
        key: KeyArgs,
        /// A class id (128-hex) the chain knows. With it, the chain is asked which bonds THIS KEY
        /// registered — the only reliable answer, because a bond's collateral often sits at another
        /// address (a genesis or sponsored registration) where no address scan will find it.
        #[arg(long)]
        class_id: Option<String>,
    },
    /// Request retirement of a bond: sign the release the consensus rule already accepts.
    ///
    /// The collateral is released after the withdrawal delay, not immediately. Refuses while the
    /// bond has unresolved claims, because retiring then would pull the collateral out from under
    /// a live dispute.
    /// Declare which classes this bond will judge — the panel draw seats it only for these.
    ///
    /// The set is REPLACED, not merged, so `--declare` with nothing stands the bond down. Declare
    /// only classes this node can actually run: a seat that cannot serve is a seat that gets
    /// convicted, which is what makes the declaration a stake rather than a permission list.
    Capability {
        #[command(flatten)]
        key: KeyArgs,
        /// Which bond, as `<txid>:<index>`. Optional when this key holds exactly one.
        #[arg(long)]
        bond: Option<String>,
        /// A class id (128-hex) the chain knows — required, because the only RPC that reports a
        /// bond's registered key will not answer without one.
        #[arg(long)]
        class_id: Option<String>,
        /// Class ids (128-hex) to declare. Repeatable, or comma-separated. Omit to declare none.
        #[arg(long)]
        declare: Vec<String>,
        /// Build and price the carrier, print it, submit nothing.
        #[arg(long)]
        dry_run: bool,
        /// Submit. Without it the command stops after printing what it would do.
        #[arg(long)]
        yes: bool,
    },
    Retire {
        #[command(flatten)]
        key: KeyArgs,
        /// Which bond, as `<txid>:<index>`. Optional when this key holds exactly one.
        #[arg(long)]
        bond: Option<String>,
        /// A class id (128-hex) the chain knows — required, because the only RPC that reports a
        /// bond's reserved exposure will not answer without one. The node prints the class id it
        /// produces under; any registered class works, since the exposure belongs to the bond.
        #[arg(long)]
        class_id: Option<String>,
        /// Build and price the carrier, print it, submit nothing.
        #[arg(long)]
        dry_run: bool,
        /// Submit. Without it the command stops after printing what it would do.
        #[arg(long)]
        yes: bool,
    },
}

/// Parse a decimal MSK string (e.g. "10.5") into sompi (1 MSK = 1e8 sompi).
fn parse_msk_to_sompi(s: &str) -> Result<u64, CliError> {
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return Err(CliError::new(exit::GENERIC, format!("invalid amount '{s}'")));
    }
    let whole: u64 =
        if whole.is_empty() { 0 } else { whole.parse().map_err(|_| CliError::new(exit::GENERIC, format!("invalid amount '{s}'")))? };
    if frac.len() > 8 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CliError::new(exit::GENERIC, format!("amount '{s}' has >8 fractional digits (1 MSK = 1e8 sompi)")));
    }
    let frac_sompi: u64 = format!("{frac:0<8}").parse().map_err(|_| CliError::new(exit::GENERIC, format!("invalid amount '{s}'")))?;
    whole
        .checked_mul(100_000_000)
        .and_then(|w| w.checked_add(frac_sompi))
        .ok_or_else(|| CliError::new(exit::GENERIC, "amount overflow".to_string()))
}

fn prefix_of(network: &str) -> Result<kaspa_addresses::Prefix, CliError> {
    use std::str::FromStr;
    let net = kaspa_consensus_core::network::NetworkId::from_str(network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{network}': {e}")))?;
    Ok(net.network_type().into())
}

fn key_gen(ctx: &node::Ctx, out: &str) -> CliResult {
    let prefix = prefix_of(&ctx.network)?;
    let addr = keys::generate(out, prefix)?;
    match ctx.output {
        OutputFormat::Human => {
            println!("Wrote a new ML-DSA-87 seed to {out} (mode 0600). BACK IT UP — it cannot be recovered.");
            println!("Address: {addr}");
        }
        OutputFormat::Json => println!("{}", serde_json::json!({ "ok": true, "file": out, "address": addr.to_string() })),
    }
    Ok(())
}

/// `misaka key import` (ADR-0063 D1, hardened by SA-1).
///
/// Two sources, and neither is argv or the environment: stdin, or a file whose PATH is the
/// argument and whose mode must be 0600. A seed passed as a value would be in `ps` output on
/// every host it ran on and in the shell history forever after; a seed in an environment variable
/// is inherited by every child this process spawns. There is deliberately no flag that takes one.
fn key_import(ctx: &node::Ctx, out: &str, hex_stdin: bool, hex_file: Option<&str>) -> CliResult {
    let source = match (hex_stdin, hex_file) {
        (true, None) => keys::SeedSource::Stdin,
        (false, Some(path)) => keys::SeedSource::File(path),
        (true, Some(_)) => {
            return Err(CliError::new(exit::GENERIC, "pass only one of --hex-stdin / --hex-file".to_string()));
        }
        (false, None) => {
            return Err(CliError::new(
                exit::GENERIC,
                "name a source for the seed: --hex-stdin (pipe the 64 hex characters in) or --hex-file <path> (mode 0600). \
                 The seed is never taken as a CLI value or from the environment. A mnemonic is not accepted either: this tree \
                 and the web wallet have no agreed BIP39 derivation to an ML-DSA-87 seed, so an import that guessed one would \
                 return a different address without saying so."
                    .to_string(),
            ));
        }
    };
    let prefix = prefix_of(&ctx.network)?;
    let addr = keys::import(out, prefix, source)?;
    match ctx.output {
        OutputFormat::Human => {
            println!("Imported an ML-DSA-87 seed to {out} (mode 0600).");
            println!("Address: {addr}");
        }
        OutputFormat::Json => println!("{}", serde_json::json!({ "ok": true, "file": out, "address": addr.to_string() })),
    }
    Ok(())
}

fn key_address(ctx: &node::Ctx, ks: &keys::KeySource) -> CliResult {
    let prefix = prefix_of(&ctx.network)?;
    let addr = ks.load_key()?.funding_address(prefix);
    match ctx.output {
        OutputFormat::Human => println!("{addr}"),
        OutputFormat::Json => println!("{}", serde_json::json!({ "ok": true, "address": addr.to_string() })),
    }
    Ok(())
}

#[derive(Subcommand, Debug)]
enum NodeCmd {
    /// One-shot health check: ports, sync, versions, RPC surface.
    Doctor,
    /// Liveness probe for a watchdog: is the node ANSWERING and is its chain MOVING? Exits 0 when
    /// the wRPC answers within --timeout and the virtual DAA (or block count) advanced since the
    /// last probe within --stall-secs; exits 11 (WEDGED) when the RPC does not answer — the shape
    /// of the 2026-09-02 public-node hang, which `Restart=` never sees — and 12 (STALLED) when it
    /// answers but nothing has moved for --stall-secs. State lives in --state between runs.
    Liveness {
        /// Where the previous probe's DAA/block count/timestamps are kept (JSON).
        #[arg(long, default_value = "/var/lib/misaka/liveness.json")]
        state: std::path::PathBuf,
        /// No progress for this long is STALLED. Size it to several block intervals.
        #[arg(long, default_value_t = 900)]
        stall_secs: u64,
    },
    /// Print the host's SECURITY POSTURE from live state (ADR-0079 Decision 13): the confinement
    /// backend actually in force, the worker environment as a child actually receives it, every
    /// listening socket with its bind address and whether the acknowledgement variable was
    /// required, which processes hold key material, the artifact paths and their digests, and the
    /// interpreter fence read off the running node's own argv. Never printed from config, and
    /// `none` is reported honestly where a backend is missing. Exits 13 (EXPOSED) when a public
    /// entrance runs on a `none`-backend host or a public parser holds a key, 14 (DEGRADED) when
    /// there is no backend and nothing public behind it. Signed by nobody; earns nothing.
    SecurityReport {
        /// Probe this worker's manifest for the roots it verified at load. Optional: without it
        /// the artifact section reports paths and says it did not probe.
        #[arg(long)]
        worker: Option<std::path::PathBuf>,
        /// Recompute the SHA-256 of every artifact path from its BYTES. This is a full read — the
        /// hybrid class's artifact is 33 GiB — so it is opt-in.
        #[arg(long, default_value_t = false)]
        verify_artifacts: bool,
    },
    /// Show the effective local node RPC endpoints (the registry the node wrote, else the
    /// network defaults). Lets you see what `misaka validator` will auto-connect to.
    Endpoints,
    /// Start a local node for --network-id (port-free; peers via the DNS seeds). Forwards to
    /// `kaspad` with the network selected and an optional --profile; extra kaspad args after `--`.
    Start(NodeStartArgs),
}

#[derive(Subcommand, Debug)]
enum BootstrapCmd {
    /// Show the DNS seed domains + default P2P port for the network.
    Seeds,
    /// Resolve the DNS seeds to live peer IPs (debug; the normal path does this internally).
    Resolve,
}

#[derive(Subcommand, Debug)]
enum EvmCmd {
    /// Native MSK balance of an EVM address (`eth_getBalance`).
    Balance {
        /// 0x-prefixed 20-byte EVM address.
        #[arg(long)]
        address: String,
    },
    /// Next nonce of an EVM address (`eth_getTransactionCount`, latest).
    Nonce {
        #[arg(long)]
        address: String,
    },
    /// Estimate gas for a call (`eth_estimateGas`).
    EstimateGas {
        #[arg(long)]
        from: String,
        /// Destination address; omit for a contract-CREATE estimate.
        #[arg(long)]
        to: Option<String>,
        /// Value in sompi (scaled to wei by EVM_NATIVE_SCALE).
        #[arg(long, default_value_t = 0)]
        value: u64,
        /// 0x calldata.
        #[arg(long)]
        data: Option<String>,
    },
    /// EVM transaction lifecycle (`misaka_getEvmTxStatus`).
    /// A read-only eth_call at latest (ADR-0089's doors answer any caller): --to <addr> --data <hex>.
    View {
        #[arg(long)]
        to: String,
        #[arg(long)]
        data: String,
    },
    #[command(subcommand)]
    Tx(EvmTxCmd),
    /// EVM HD wallet — create / import / address. [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    #[command(subcommand)]
    Wallet(EvmWalletCmd),
    /// Sign + broadcast an EIP-1559 transfer (dry-run unless --yes). [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    Send {
        /// Recipient 0x address.
        #[arg(long)]
        to: String,
        /// Amount in MSK (decimal; 1 MSK = 1e18 wei).
        #[arg(long)]
        amount: String,
        /// Gas limit (default: eth_estimateGas).
        #[arg(long)]
        gas_limit: Option<u64>,
        /// Max fee per gas, wei (default: eth_gasPrice).
        #[arg(long)]
        max_fee: Option<u128>,
        /// Nonce (default: eth_getTransactionCount pending).
        #[arg(long)]
        nonce: Option<u64>,
        /// Actually broadcast (otherwise a dry-run preview).
        #[arg(long)]
        yes: bool,
        /// After broadcast, poll until accepted.
        #[arg(long)]
        wait: bool,
        #[command(flatten)]
        key: EvmKeyArgs,
    },
    /// Deploy a contract (raw init code; dry-run unless --yes). [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    Deploy {
        /// Init code as inline 0x hex (creation bytecode + ABI-encoded ctor args).
        #[arg(long)]
        bytecode: Option<String>,
        /// Init code from a file (hex). Use this for large blobs.
        #[arg(long)]
        bytecode_file: Option<String>,
        /// Value to endow, MSK (decimal; usually 0).
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long)]
        gas_limit: Option<u64>,
        #[arg(long)]
        max_fee: Option<u128>,
        #[arg(long)]
        nonce: Option<u64>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
        #[command(flatten)]
        key: EvmKeyArgs,
    },
    /// Call a contract with raw calldata (dry-run unless --yes). [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    Call {
        /// Contract 0x address.
        #[arg(long)]
        to: String,
        /// Calldata as inline 0x hex (selector + ABI-encoded args).
        #[arg(long)]
        data: Option<String>,
        /// Calldata from a file (hex).
        #[arg(long)]
        data_file: Option<String>,
        /// Value to send, MSK (decimal; usually 0).
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long)]
        gas_limit: Option<u64>,
        #[arg(long)]
        max_fee: Option<u128>,
        #[arg(long)]
        nonce: Option<u64>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        wait: bool,
        #[command(flatten)]
        key: EvmKeyArgs,
    },
}

#[cfg(feature = "evm-send")]
#[derive(Subcommand, Debug)]
enum EvmWalletCmd {
    /// Generate a new 24-word BIP-39 mnemonic to a 0600 file + print the address.
    Create {
        #[arg(long)]
        out: String,
    },
    /// Import a mnemonic (read from stdin) to a 0600 file.
    Import {
        #[arg(long)]
        out: String,
    },
    /// Print the EVM address for a key.
    Address {
        #[command(flatten)]
        key: EvmKeyArgs,
    },
}

/// EVM key-source flags (BIP-39 mnemonic file / raw secp key file / stdin). The
/// secret is never a CLI value.
#[cfg(feature = "evm-send")]
#[derive(Args, Debug, Clone)]
struct EvmKeyArgs {
    /// Path to a BIP-39 mnemonic file (derives m/44'/60'/0'/0/0).
    #[arg(long, env = "MISAKA_EVM_MNEMONIC_FILE")]
    mnemonic_file: Option<String>,
    /// Path to a hex 32-byte secp256k1 private key file.
    #[arg(long, env = "MISAKA_EVM_KEY_FILE")]
    key_file: Option<String>,
    /// Read the mnemonic or hex key from stdin.
    #[arg(long)]
    key_stdin: bool,
}

#[cfg(feature = "evm-send")]
impl EvmKeyArgs {
    fn source(&self) -> evm_send::EvmKeySource {
        evm_send::EvmKeySource {
            mnemonic_file: self.mnemonic_file.clone(),
            key_file: self.key_file.clone(),
            key_stdin: self.key_stdin,
        }
    }
}

#[derive(Subcommand, Debug)]
enum EvmTxCmd {
    /// One-shot status by tx hash.
    Status {
        /// 0x-prefixed 32-byte EVM tx hash.
        #[arg(long)]
        hash: String,
    },
    /// Poll the status until the tx is accepted (mined) or the timeout elapses.
    Wait {
        #[arg(long)]
        hash: String,
        /// Overall wait timeout, seconds.
        #[arg(long, default_value_t = 1800)]
        timeout: u64,
        /// Poll interval, seconds.
        #[arg(long, default_value_t = 2)]
        poll: u64,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    // Config layer (~/.misaka/config.toml). A malformed file is a hard error so a
    // typo is never silently ignored; a missing file is the empty default. Precedence
    // for each value: CLI flag > env var (both filled by clap) > config file > default.
    let cfg = match config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e.msg);
            return std::process::ExitCode::from(e.code as u8);
        }
    };
    let ctx = node::Ctx {
        output: cli.output,
        network: cli.network.clone().or(cfg.network_id.clone()).unwrap_or_else(|| "testnet-10".to_string()),
        rpc: cli.rpc.clone().or_else(|| cfg.node.wrpc_borsh.clone()),
        node_grpc: cli.node_grpc.clone().or_else(|| cfg.node.grpc.clone()),
        evm_rpc: cli.evm_rpc.clone().or_else(|| cfg.evm.rpc_url.clone()).unwrap_or_else(|| "http://127.0.0.1:8545".to_string()),
        timeout_secs: cli.timeout,
        quiet: cli.quiet,
    };

    // ADR-0122: a mining command takes the network the operator NAMED (the flag, the env, the
    // config file) and otherwise reads it from mining.toml or the running node — never the CLI's
    // testnet-10 default, which is not a network anyone mines.
    let named_network = cli.network.clone().or_else(|| cfg.network_id.clone());
    let profile =
        |args: &ProfileArgs| operator::profile::Profile::resolve(&args.overrides(), named_network.as_deref(), ctx.rpc.as_deref());

    let result = match cli.command {
        Command::Mining(MiningCmd::Status { profile: args, watch }) => match profile(&args) {
            Ok(p) => operator::status::run(&ctx, p, watch).await,
            Err(e) => Err(e),
        },
        Command::Mining(MiningCmd::Start { profile: args, detach, print_command, service }) => match profile(&args) {
            Ok(p) => {
                // The supervisor a detach or a unit starts is this same profile, named the same way.
                let mut relaunch = vec!["--network".to_string(), p.network.clone()];
                if let Some(rpc) = &ctx.rpc {
                    relaunch.extend(["--rpc".to_string(), rpc.clone()]);
                }
                relaunch.extend(["mining".to_string(), "run".to_string()]);
                relaunch.extend(args.relaunch_args());
                let mode = operator::supervisor::StartMode { detach, print_command, service };
                operator::supervisor::start(&ctx, p, mode, &|| profile(&args), &relaunch).await
            }
            Err(e) => Err(e),
        },
        Command::Mining(MiningCmd::Stop { profile: args, drain, force, grace }) => match profile(&args) {
            Ok(p) => operator::supervisor::stop(&ctx, p, drain, force, grace).await,
            Err(e) => Err(e),
        },
        Command::Mining(MiningCmd::Run { profile: args }) => match profile(&args) {
            Ok(p) => match operator::supervisor::plan(&p) {
                Ok(plan) => operator::supervisor::run_supervisor(plan, false, &|| profile(&args)).await,
                Err(f) => Err(CliError::new(f.exit, f.render())),
            },
            Err(e) => Err(e),
        },
        Command::Doctor(args) => match profile(&args.profile) {
            Ok(p) => operator::doctor::run(&ctx, p, &args.areas, args.strict).await,
            Err(e) => Err(e),
        },
        Command::Work(WorkCmd::List { profile: args }) => match profile(&args) {
            Ok(p) => operator::work_cmd::list(&ctx, p).await,
            Err(e) => Err(e),
        },
        Command::Work(WorkCmd::Show { id, profile: args }) => match profile(&args) {
            Ok(p) => operator::work_cmd::show(&ctx, p, &id).await,
            Err(e) => Err(e),
        },
        Command::Work(WorkCmd::Why { id, profile: args }) => match profile(&args) {
            Ok(p) => operator::work_cmd::why(&ctx, p, &id).await,
            Err(e) => Err(e),
        },
        Command::Node(NodeCmd::Doctor) => node::doctor(&ctx).await,
        Command::Node(NodeCmd::Liveness { state, stall_secs }) => node::liveness(&ctx, &state, stall_secs).await,
        Command::Node(NodeCmd::SecurityReport { worker, verify_artifacts }) => {
            security::security_report(&ctx, worker.as_ref(), verify_artifacts)
        }
        Command::Node(NodeCmd::Endpoints) => bootstrap::endpoints(ctx.output, &ctx.network),
        Command::Node(NodeCmd::Start(a)) => {
            forward::node(&ctx, a.profile.as_deref(), a.node_profile.as_deref(), a.vps_8gb, a.min_disk_free_percent, &a.args, false)
        }
        Command::Join(a) => {
            forward::node(&ctx, a.profile.as_deref(), a.node_profile.as_deref(), a.vps_8gb, a.min_disk_free_percent, &a.args, true)
        }
        Command::Bootstrap(BootstrapCmd::Seeds) => bootstrap::seeds(ctx.output, &ctx.network),
        Command::Bootstrap(BootstrapCmd::Resolve) => bootstrap::resolve(ctx.output, &ctx.network),
        Command::Setup(cmd) => setup::run(&ctx, cmd).await,
        Command::Evm(EvmCmd::View { to, data }) => eth::view(&ctx, &to, &data),
        Command::Evm(EvmCmd::Balance { address }) => eth::balance(&ctx, &address),
        Command::Evm(EvmCmd::Nonce { address }) => eth::nonce(&ctx, &address),
        Command::Evm(EvmCmd::EstimateGas { from, to, value, data }) => {
            eth::estimate_gas(&ctx, &from, to.as_deref(), value, data.as_deref())
        }
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Status { hash })) => eth::tx_status(&ctx, &hash),
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Wait { hash, timeout, poll })) => eth::tx_wait(&ctx, &hash, timeout, poll),
        Command::Palw(PalwCmd::FpSubmit { tx, yes, material_out, capture, dsl_payload }) => {
            palw_fp::submit(&ctx, &tx, yes, material_out.as_deref(), capture.as_deref(), dsl_payload.as_deref()).await
        }
        Command::Palw(PalwCmd::SubmitObject { key, object, yes }) => palw_fp::submit_objects(&ctx, &key.source(), &object, yes).await,
        Command::Palw(PalwCmd::Extension(ExtensionCmd::Inspect { manifest, json })) => palw_extension::inspect(&ctx, &manifest, json),
        Command::Palw(PalwCmd::Extension(ExtensionCmd::Verify { manifest, depth, receipt_out, key, json })) => {
            palw_extension::verify(&ctx, &manifest, depth.depth(), receipt_out.as_deref(), key.source_opt(), json)
        }
        Command::Palw(PalwCmd::Extension(ExtensionCmd::Preflight { manifest, json })) => {
            palw_extension::preflight(&ctx, &manifest, json).await
        }
        Command::Palw(PalwCmd::Extension(ExtensionCmd::Submit { manifest, key, bond, yes, json })) => {
            palw_extension::submit(&ctx, &manifest, &key.source(), bond.as_deref(), yes, json).await
        }
        Command::Palw(PalwCmd::Extension(ExtensionCmd::ReceiptVerify { receipt, manifest, json })) => {
            palw_extension::receipt_verify(&ctx, &receipt, manifest.as_deref(), json)
        }
        Command::Palw(PalwCmd::DaAccuse { key, claim, row, tile, bond, out }) => {
            palw_da::accuse(&ctx, &key.source(), palw_da::DaAccuseArgs { claim: &claim, row, tile, bond: &bond, out: &out }).await
        }
        Command::Palw(PalwCmd::ShardPlan { key, class, count, bond, out }) => {
            palw_shard_licensing::shard_plan(&ctx, &key.source(), &class, count, &bond, &out).await
        }
        Command::Palw(PalwCmd::BondShards { key, bond, class, count, shards, out }) => {
            palw_shard_licensing::bond_shards(&ctx, &key.source(), &bond, &class, count, &shards, &out).await
        }
        Command::Palw(PalwCmd::ShardAccuse { key, accusation, bond, ladder, out }) => {
            palw_shard_court::accuse(
                &ctx,
                &key.source(),
                palw_shard_court::ShardAccuseArgs { accusation: &accusation, bond: &bond, ladder, out: &out },
            )
            .await
        }
        Command::Palw(PalwCmd::CourtClose { key, close, class, side, deadline_at, state, restart, offline, yes }) => {
            palw_court::court_close(
                &ctx,
                &key.source(),
                palw_court::CourtCloseArgs {
                    close: &close,
                    class: class.as_deref(),
                    side: side.as_deref(),
                    deadline_at,
                    state: state.as_deref(),
                    restart,
                    offline,
                    yes,
                },
            )
            .await
        }
        Command::Palw(PalwCmd::Certified { class_id, json }) => palw_fp::certified(&ctx, &class_id, json).await,
        Command::Palw(PalwCmd::Claim { claim_ids, outbox, json }) => palw_claim::show(&ctx, &claim_ids, outbox.as_deref(), json).await,
        Command::Palw(PalwCmd::VerifyContext { vector, vector_file, list, stages, out, receipt_out, sign_with }) => {
            palw_verify_context::verify_context(palw_verify_context::VerifyContextArgs {
                vector: vector.as_deref(),
                vector_file: vector_file.as_deref(),
                list,
                stages: stages.as_deref(),
                out: out.as_deref(),
                receipt_out: receipt_out.as_deref(),
                key: sign_with.map(|path| keys::KeySource { key_file: Some(path), key_stdin: false }),
            })
        }
        Command::Palw(PalwCmd::VerifyReceipt { receipt, document }) => {
            palw_verify_context::verify_receipt(&receipt, document.as_deref())
        }
        Command::Palw(PalwCmd::Derived { claim_id, json }) => palw_derived::show(&ctx, &claim_id, json).await,
        Command::Palw(PalwCmd::ModelShow { line_id, quote_msk, json }) => palw_model::show(&ctx, &line_id, quote_msk, json).await,
        Command::Palw(PalwCmd::ModelPositions { holder, key, json }) => {
            let source = key.source_opt();
            palw_model::positions(&ctx, holder, source.as_ref(), json).await
        }
        #[cfg(feature = "evm-send")]
        Command::Palw(PalwCmd::ModelEvmBuy { key, line, msk, min_positions, gas_limit, max_fee, nonce, yes, wait }) => {
            evm_send::model_evm_buy(&ctx, &key.source(), &line, &msk, min_positions, gas_limit, max_fee, nonce, yes, wait)
        }
        #[cfg(feature = "evm-send")]
        Command::Palw(PalwCmd::ModelEvmSell { key, line, positions, min_msk, gas_limit, max_fee, nonce, yes, wait }) => {
            evm_send::model_evm_sell(&ctx, &key.source(), &line, positions, &min_msk, gas_limit, max_fee, nonce, yes, wait)
        }
        Command::Palw(PalwCmd::ModelEvmPosition { line, address }) => eth::model_evm_position(&ctx, &line, &address),
        Command::Palw(PalwCmd::ModelSeed { key, line, msk, yes }) => palw_model::seed(&ctx, &key.source(), &line, &msk, yes).await,
        #[cfg(feature = "evm-send")]
        Command::Palw(PalwCmd::ModelEvmSeed { key, line, msk, gas_limit, max_fee, nonce, yes, wait }) => {
            evm_send::model_evm_seed(&ctx, &key.source(), &line, &msk, gas_limit, max_fee, nonce, yes, wait)
        }
        Command::Palw(PalwCmd::ModelBuy { key, line, msk, min_positions, yes }) => {
            palw_model::buy(&ctx, &key.source(), &line, &msk, min_positions, yes).await
        }
        Command::Palw(PalwCmd::ModelSell { key, line, positions, min_msk, yes }) => {
            palw_model::sell(&ctx, &key.source(), &line, positions, min_msk, yes).await
        }
        Command::Palw(PalwCmd::LineShow { line_id, json }) => palw_line::line_show(&ctx, &line_id, json).await,
        Command::Palw(PalwCmd::LineLog { line_id, json }) => palw_line::line_log(&ctx, &line_id, json).await,
        Command::Palw(PalwCmd::LineList { class_id, json }) => palw_line::line_list(&ctx, &class_id, json).await,
        Command::Palw(PalwCmd::Proposals { line_id, json }) => palw_line::proposals(&ctx, &line_id, json).await,
        Command::Palw(PalwCmd::LineFound { key, class, name, root, bond, yes }) => {
            palw_line::line_found(&ctx, &key.source(), &class, &name, &root, &bond, yes).await
        }
        Command::Palw(PalwCmd::VersionPublish {
            key,
            line,
            root,
            parent,
            adopted_from,
            runtime_hash,
            dataset_commitment,
            training_config_hash,
            notes_hash,
            preview,
            yes,
        }) => {
            palw_line::version_publish(
                &ctx,
                &key.source(),
                &line,
                &root,
                parent,
                adopted_from,
                runtime_hash,
                dataset_commitment,
                training_config_hash,
                notes_hash,
                preview,
                yes,
            )
            .await
        }
        Command::Palw(PalwCmd::VersionPromote { key, line, version, yes }) => {
            palw_line::version_move(&ctx, &key.source(), &line, version, true, yes).await
        }
        Command::Palw(PalwCmd::VersionWithdraw { key, line, version, yes }) => {
            palw_line::version_move(&ctx, &key.source(), &line, version, false, yes).await
        }
        Command::Palw(PalwCmd::LineBenefits { key, line, tiers, cadence_daa, expires_daa, yes }) => {
            palw_line::line_benefits(&ctx, &key.source(), &line, tiers, cadence_daa, expires_daa, yes).await
        }
        Command::Palw(PalwCmd::LineRoles { key, line, developer, maintainer, contributor_permille, yes }) => {
            palw_line::line_roles(&ctx, &key.source(), &line, developer, maintainer, contributor_permille, yes).await
        }
        Command::Palw(PalwCmd::LineTransfer { key, line, new_owner, yes }) => {
            palw_line::line_transfer(&ctx, &key.source(), &line, &new_owner, yes).await
        }
        Command::Palw(PalwCmd::LineRetire { key, line, yes }) => palw_line::line_retire(&ctx, &key.source(), &line, yes).await,
        Command::Palw(PalwCmd::ProposalPost { key, line, root, note_hash, bond, yes }) => {
            palw_line::proposal_post(&ctx, &key.source(), &line, &root, &note_hash, &bond, yes).await
        }
        Command::Palw(PalwCmd::ProposalClose { key, line, proposal, yes }) => {
            palw_line::proposal_close(&ctx, &key.source(), &line, &proposal, yes).await
        }
        Command::Palw(PalwCmd::Evaluate { key, line, version, evaluator_id, score_permille, report_hash, bond, yes }) => {
            palw_line::evaluate(&ctx, &key.source(), &line, version, &evaluator_id, score_permille, &report_hash, &bond, yes).await
        }
        Command::Palw(PalwCmd::DerivedVerify { claim_id, answer, dsl, job_context, tokenizer, job_context_hash, family, json }) => {
            palw_derived::verify(
                &ctx,
                palw_derived::DerivedVerifyArgs {
                    claim_id: &claim_id,
                    answer: &answer,
                    dsl: dsl.as_deref(),
                    job_context: job_context.as_deref(),
                    job_context_hash: job_context_hash.as_deref(),
                    tokenizer: tokenizer.as_deref(),
                    family: family.as_deref(),
                    json,
                },
            )
            .await
        }
        Command::Wallet(WalletCmd::Utxo(UtxoCmd::List { address, key })) => {
            wallet::utxo_list(&ctx, address.as_deref(), &key.source()).await
        }
        Command::Wallet(WalletCmd::Utxo(UtxoCmd::Consolidate { max_inputs, max_txs_per_run, sleep_ms, yes, key })) => {
            wallet::consolidate(&ctx, &key.source(), max_inputs, !yes, yes, max_txs_per_run, sleep_ms).await
        }
        Command::Wallet(WalletCmd::Send { to, amount, yes, coinbase_only, key }) => match parse_msk_to_sompi(&amount) {
            Ok(sompi) => wallet::send(&ctx, &key.source(), &to, sompi, !yes, yes, coinbase_only).await,
            Err(e) => Err(e),
        },
        Command::Key(KeyCmd::Gen { out }) => key_gen(&ctx, &out),
        Command::Key(KeyCmd::Address { key }) => key_address(&ctx, &key.source()),
        Command::Key(KeyCmd::Import { out, hex_stdin, hex_file }) => key_import(&ctx, &out, hex_stdin, hex_file.as_deref()),
        Command::Bond(BondCmd::Status { key, class_id }) => bond::status(&ctx, &key.source(), class_id.as_deref()).await,
        Command::Bond(BondCmd::Capability { key, bond, class_id, declare, dry_run, yes }) => {
            bond::capability(&ctx, &key.source(), bond.as_deref(), class_id.as_deref(), &declare, dry_run, yes).await
        }
        Command::Bond(BondCmd::Retire { key, bond, class_id, dry_run, yes }) => {
            bond::retire(&ctx, &key.source(), bond.as_deref(), class_id.as_deref(), dry_run, yes).await
        }
        Command::Config(ConfigCmd::Init { force }) => config::init(&ctx.network, force),
        Command::Config(ConfigCmd::Show) => {
            config::show(ctx.output, &ctx.network, &ctx.rpc, &cli.node_grpc, &cfg.node.grpc, &ctx.evm_rpc)
        }
        Command::Validator(p) => match validator_reader::maybe_handle(&ctx, &p.args).await {
            Some(result) => result,
            None => forward::validator(&ctx, &p.args),
        },
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Wallet(EvmWalletCmd::Create { out })) => evm_send::wallet_create(&ctx, &out),
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Wallet(EvmWalletCmd::Import { out })) => evm_send::wallet_import(&ctx, &out),
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Wallet(EvmWalletCmd::Address { key })) => evm_send::wallet_address(&ctx, &key.source()),
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Send { to, amount, gas_limit, max_fee, nonce, yes, wait, key }) => {
            match evm_send::parse_msk_to_wei(&amount) {
                Ok(wei) => evm_send::send(&ctx, &key.source(), &to, wei, gas_limit, max_fee, nonce, yes, wait),
                Err(e) => Err(e),
            }
        }
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Deploy { bytecode, bytecode_file, value, gas_limit, max_fee, nonce, yes, wait, key }) => {
            match (evm_send::read_hex_blob(&bytecode, &bytecode_file), evm_send::parse_msk_to_wei(&value)) {
                (Ok(code), Ok(wei)) => evm_send::deploy(&ctx, &key.source(), code, wei, gas_limit, max_fee, nonce, yes, wait),
                (Err(e), _) | (_, Err(e)) => Err(e),
            }
        }
        #[cfg(feature = "evm-send")]
        Command::Evm(EvmCmd::Call { to, data, data_file, value, gas_limit, max_fee, nonce, yes, wait, key }) => {
            match (evm_send::read_hex_blob(&data, &data_file), evm_send::parse_msk_to_wei(&value)) {
                (Ok(cd), Ok(wei)) => evm_send::call(&ctx, &key.source(), &to, cd, wei, gas_limit, max_fee, nonce, yes, wait),
                (Err(e), _) | (_, Err(e)) => Err(e),
            }
        }
        #[cfg(feature = "evm-send")]
        Command::Prea(PreaCmd::SignRoot {
            key,
            account,
            version,
            nonce,
            valid_after,
            valid_until,
            max_relayer_fee,
            to,
            value,
            calldata,
        }) => {
            // Audit H-3: refuse a PREA ML-DSA-87 root op unless the F003 precompile
            // is active on the connected node's network — an authorization produced
            // while F003 is fence-inert can never verify on-chain.
            match prea::gate_root_op(&ctx).await {
                Ok(()) => prea::run_sign_root(
                    ctx.output,
                    &key.source(),
                    &account,
                    version,
                    nonce,
                    valid_after,
                    valid_until,
                    &max_relayer_fee,
                    &to,
                    &value,
                    &calldata,
                ),
                Err(e) => Err(e),
            }
        }
        #[cfg(feature = "evm-send")]
        Command::Prea(PreaCmd::SignSession { key, account, version, call_index, max_relayer_fee, to, value, calldata }) => {
            prea::run_sign_session(ctx.output, &key.source(), &account, version, call_index, &max_relayer_fee, &to, &value, &calldata)
        }
        Command::Ask(args) => ask::run(args),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        // ADR-0122: an operator screen that already printed its verdict (the five fields, or a
        // JSON document carrying `exit`) returns an empty message, so it is not said twice.
        Err(e) if e.msg.is_empty() => std::process::ExitCode::from(e.code as u8),
        Err(e) => {
            // Errors always go to stderr (never swallowed by --quiet); in JSON
            // mode emit a machine-readable error object too.
            if ctx.output == OutputFormat::Json {
                let obj = serde_json::json!({ "ok": false, "error": e.msg, "exitCode": e.code });
                eprintln!("{obj}");
            } else {
                eprintln!("error: {}", e.msg);
            }
            std::process::ExitCode::from(e.code as u8)
        }
    }
}

/// **The command surface itself is an assertion** — ADR-0063 SA-1 and SA-4.
///
/// These tests read the parser rather than a function, because both rules are properties of the
/// SURFACE: a seed must have no way in through argv or the environment, and `misaka miner` must
/// have no way in at all. A unit test on a handler cannot see either — the handler is not reached
/// when the flag it would have to refuse does not exist, and that absence is what has to be
/// checked, and kept.
#[cfg(test)]
mod cli_surface_tests {
    use super::*;
    use clap::CommandFactory;

    const SEED_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn import_cmd() -> clap::Command {
        Cli::command().find_subcommand("key").expect("key").find_subcommand("import").expect("import").clone()
    }

    /// **SA-1: there is no flag that takes the seed as a value.**
    ///
    /// A seed on the command line is in `ps` output for the life of the process and in the shell
    /// history for the life of the host. Every spelling anyone would reach for is tried here, so
    /// "someone adds `--seed` back for convenience" is a failing test rather than a code review
    /// that may not happen.
    #[test]
    fn a_seed_on_argv_is_refused() {
        for flag in ["--seed", "--seed-hex", "--hex", "--mnemonic", "--key", "--secret"] {
            let parsed = Cli::try_parse_from(["misaka", "key", "import", "--out", "/tmp/never-written", flag, SEED_HEX]);
            assert!(parsed.is_err(), "`key import {flag} <seed>` parsed — a seed must have no argv door");
        }
        // A bare positional is the same door with the flag filed off.
        assert!(Cli::try_parse_from(["misaka", "key", "import", "--out", "/tmp/never-written", SEED_HEX]).is_err());
    }

    /// **SA-1: no argument of `key import` reads the environment.**
    ///
    /// An environment variable is inherited by every child this process spawns — the model worker
    /// included, until ADR-0079 R-01 lands — so an `env = …` on this command would hand the seed
    /// to processes that have no business holding it. The check walks the parser rather than
    /// grepping, so it also catches one added through a `#[command(flatten)]`.
    #[test]
    fn no_import_argument_reads_the_environment() {
        for arg in import_cmd().get_arguments() {
            assert!(arg.get_env().is_none(), "`key import --{}` reads an env var; the seed path must not", arg.get_id());
        }
    }

    /// The two legitimate sources are stdin and a file PATH, and they are mutually exclusive: a
    /// command given both would have to pick one, and an operator would not know which.
    #[test]
    fn the_two_seed_sources_are_stdin_and_a_path() {
        assert!(Cli::try_parse_from(["misaka", "key", "import", "--out", "/tmp/o", "--hex-stdin"]).is_ok());
        assert!(Cli::try_parse_from(["misaka", "key", "import", "--out", "/tmp/o", "--hex-file", "/tmp/s"]).is_ok());
        assert!(
            Cli::try_parse_from(["misaka", "key", "import", "--out", "/tmp/o", "--hex-stdin", "--hex-file", "/tmp/s"]).is_err(),
            "two sources at once must be refused at the parser, not silently ranked"
        );
    }

    /// **SA-4: `misaka miner` does not exist.**
    ///
    /// It forwarded to `kaspa-pq-miner`, which is on no fleet host — and the forwarder falls
    /// through to the bare name on `$PATH`, so the subcommand ran whatever a writable `PATH` entry
    /// held. On a network whose only hash lane is the fee-only heartbeat it could not have earned
    /// anything even if the binary were real, so Decision 4 resolves to deletion rather than to
    /// shipping a miner.
    #[test]
    fn the_miner_subcommand_is_gone() {
        assert!(Cli::command().find_subcommand("miner").is_none(), "`misaka miner` is back — SA-4 deleted it");
        assert!(Cli::try_parse_from(["misaka", "miner"]).is_err());
        assert!(Cli::try_parse_from(["misaka", "miner", "--blocks", "1"]).is_err());
        // The sibling forwarder is deliberately untouched: `validator` forwards to a binary this
        // tree actually builds, which is the difference SA-4 turns on.
        assert!(Cli::command().find_subcommand("validator").is_some());
    }
}
