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
#[cfg(feature = "evm-send")]
mod evm_send;
mod keys;
mod node;
#[cfg(feature = "evm-send")]
mod prea;
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
    /// PQ wallet operations (UTXO list / consolidate / send).
    #[command(subcommand)]
    Wallet(WalletCmd),
    /// Key management (generate / show address). The secret is never a CLI arg.
    #[command(subcommand)]
    Key(KeyCmd),
    /// PREA PQ smart-account signing (executeRoot / executeSession). [needs --features evm-send]
    #[cfg(feature = "evm-send")]
    #[command(subcommand)]
    Prea(PreaCmd),
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
        #[command(flatten)]
        key: KeyArgs,
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
        Command::Wallet(WalletCmd::Utxo(UtxoCmd::List { address, key })) => {
            wallet::utxo_list(&ctx, address.as_deref(), &key.source()).await
        }
        Command::Wallet(WalletCmd::Utxo(UtxoCmd::Consolidate { max_inputs, yes, key })) => {
            wallet::consolidate(&ctx, &key.source(), max_inputs, !yes, yes).await
        }
        Command::Wallet(WalletCmd::Send { to, amount, yes, key }) => match parse_msk_to_sompi(&amount) {
            Ok(sompi) => wallet::send(&ctx, &key.source(), &to, sompi, !yes, yes).await,
            Err(e) => Err(e),
        },
        Command::Key(KeyCmd::Gen { out }) => key_gen(&ctx, &out),
        Command::Key(KeyCmd::Address { key }) => key_address(&ctx, &key.source()),
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
