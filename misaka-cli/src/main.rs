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

mod eth;
mod node;

use clap::{Parser, Subcommand, ValueEnum};

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
}

/// A CLI error that carries the process exit code to surface.
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
    #[arg(long, global = true, env = "MISAKA_NETWORK", default_value = "testnet-10")]
    network: String,

    /// Node wRPC (borsh) endpoint host:port. Default derives from --network
    /// (testnet => 127.0.0.1:27210). NOTE: this is the CODE default; some
    /// deployments bind borsh on a non-standard port (e.g. 27610) — pass it here.
    #[arg(long, global = true, env = "MISAKA_RPC")]
    rpc: Option<String>,

    /// EVM JSON-RPC HTTP endpoint.
    #[arg(long, global = true, env = "MISAKA_EVM_RPC", default_value = "http://127.0.0.1:8545")]
    evm_rpc: String,

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
}

#[derive(Subcommand, Debug)]
enum NodeCmd {
    /// One-shot health check: ports, sync, versions, RPC surface.
    Doctor,
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
    let ctx = node::Ctx {
        output: cli.output,
        network: cli.network.clone(),
        rpc: cli.rpc.clone(),
        evm_rpc: cli.evm_rpc.clone(),
        timeout_secs: cli.timeout,
        quiet: cli.quiet,
    };

    let result = match cli.command {
        Command::Node(NodeCmd::Doctor) => node::doctor(&ctx).await,
        Command::Evm(EvmCmd::Balance { address }) => eth::balance(&ctx, &address),
        Command::Evm(EvmCmd::Nonce { address }) => eth::nonce(&ctx, &address),
        Command::Evm(EvmCmd::EstimateGas { from, to, value, data }) => {
            eth::estimate_gas(&ctx, &from, to.as_deref(), value, data.as_deref())
        }
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Status { hash })) => eth::tx_status(&ctx, &hash),
        Command::Evm(EvmCmd::Tx(EvmTxCmd::Wait { hash, timeout, poll })) => eth::tx_wait(&ctx, &hash, timeout, poll),
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
