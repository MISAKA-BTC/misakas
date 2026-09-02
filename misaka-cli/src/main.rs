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
mod keys;
mod node;
mod palw_fp;
#[cfg(feature = "evm-send")]
mod prea;
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

    /// Node gRPC endpoint host:port (miner / low-level RPC transport).
    /// Resolution: CLI > env MISAKA_NODE_GRPC > ~/.misaka/config.toml [node].grpc >
    /// endpoint registry / network default inside the child miner.
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
    /// Miner operations — forwarded to the `kaspa-pq-miner` binary with --network-id
    /// injected. Run `misaka miner --help` for its options.
    Miner(PassThrough),
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

#[derive(Subcommand, Debug)]
enum PalwCmd {
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
    /// Import an EXISTING 32-byte ML-DSA-87 seed, read as hex from stdin, into a 0600 file.
    ///
    /// The secret is never a CLI argument: pipe it in (`cat seed.hex | misaka key import --out
    /// /etc/misaka/bond.key`). A BIP39 mnemonic is NOT accepted — this tree and the web wallet
    /// have no agreed derivation to an ML-DSA-87 seed, and guessing one would silently produce a
    /// different address.
    Import {
        /// Output seed file path (refuses to overwrite).
        #[arg(long)]
        out: String,
        /// Read the 64-hex-character seed from stdin. Required, and the only accepted source.
        #[arg(long)]
        hex_stdin: bool,
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
    let (addr, _seed) = keys::generate(out, prefix)?;
    match ctx.output {
        OutputFormat::Human => {
            println!("Wrote a new ML-DSA-87 seed to {out} (mode 0600). BACK IT UP — it cannot be recovered.");
            println!("Address: {addr}");
        }
        OutputFormat::Json => println!("{}", serde_json::json!({ "ok": true, "file": out, "address": addr.to_string() })),
    }
    Ok(())
}

/// `misaka key import` (ADR-0063 D1). Stdin only — see `keys::import` for why an argument is
/// refused even though it would be more convenient exactly once.
fn key_import(ctx: &node::Ctx, out: &str, hex_stdin: bool) -> CliResult {
    if !hex_stdin {
        return Err(CliError::new(
            exit::GENERIC,
            "pass --hex-stdin and pipe the 64-hex-character ML-DSA-87 seed in. A mnemonic is not accepted: this tree and the web wallet have no agreed BIP39 derivation to an ML-DSA-87 seed, so an import that guessed one would return a different address without saying so."
                .to_string(),
        ));
    }
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
        .map_err(|e| CliError::new(exit::GENERIC, format!("read the seed from stdin: {e}")))?;
    let prefix = prefix_of(&ctx.network)?;
    let addr = keys::import(out, prefix, &buf)?;
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
    /// Show the effective local node RPC endpoints (the registry the node wrote, else the
    /// network defaults). Lets you see what `misaka miner`/`validator` will auto-connect to.
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

    let result = match cli.command {
        Command::Node(NodeCmd::Doctor) => node::doctor(&ctx).await,
        Command::Node(NodeCmd::Liveness { state, stall_secs }) => node::liveness(&ctx, &state, stall_secs).await,
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
        Command::Evm(EvmCmd::Balance { address }) => eth::balance(&ctx, &address),
        Command::Evm(EvmCmd::Nonce { address }) => eth::nonce(&ctx, &address),
        Command::Evm(EvmCmd::EstimateGas { from, to, value, data }) => {
            eth::estimate_gas(&ctx, &from, to.as_deref(), value, data.as_deref())
        }
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Status { hash })) => eth::tx_status(&ctx, &hash),
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Wait { hash, timeout, poll })) => eth::tx_wait(&ctx, &hash, timeout, poll),
        Command::Palw(PalwCmd::FpSubmit { tx, yes, material_out, capture }) => {
            palw_fp::submit(&ctx, &tx, yes, material_out.as_deref(), capture.as_deref()).await
        }
        Command::Palw(PalwCmd::SubmitObject { key, object, yes }) => palw_fp::submit_objects(&ctx, &key.source(), &object, yes).await,
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
        Command::Key(KeyCmd::Import { out, hex_stdin }) => key_import(&ctx, &out, hex_stdin),
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
        Command::Miner(p) => forward::miner(&ctx, &p.args),
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
